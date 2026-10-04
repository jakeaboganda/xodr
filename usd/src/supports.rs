//! What holds each signal up: a pole the map has, or one the exporter adds.

use std::f32::consts::TAU;
use std::io::{self, Write};

use xodr::{ObjectType, Point, Referenced, RoadNetwork, Shape, Signal, Vector};

use crate::{close, open, write_mesh, MeshPrim, Paths, Tag};

/// A board less than this many metres above the road is paint, which no
/// pole holds up.
const PAINT: f32 = 0.1;

/// Metres between a signal and a pole that holds it up, or two signals that
/// share one, measured across the ground.
const NEAR: f32 = 0.5;

/// The radius of a pole the exporter adds, in metres.
const RADIUS: f32 = 0.04;

/// Sides on a pole the exporter adds.
const SIDES: usize = 12;

/// What holds a signal up.
pub(crate) enum Support {
    /// A pole object the map has, by its prim path.
    Object(String),
    /// A pole the exporter adds, by its index in the list of poles.
    Pole(usize),
    /// Nothing: road paint, or a board over a driving lane.
    None,
}

impl Support {
    /// The signal's `xodr:support` and `xodr:supportPrim`.
    pub(crate) fn tags(&self) -> [(&'static str, Tag); 2] {
        let (kind, targets) = match self {
            Self::Object(path) => ("object", vec![path.clone()]),
            Self::Pole(k) => ("synthesized", vec![pole_path(*k)]),
            Self::None => ("none", vec![]),
        };
        [
            ("support", Tag::Token(kind)),
            ("supportPrim", Tag::Targets(targets)),
        ]
    }
}

/// A pole the exporter adds: upright, from `base` up to `top` metres.
pub(crate) struct Pole {
    base: Point,
    top: f32,
    /// The signal that placed the pole. Signals near it share it.
    anchor: Point,
}

/// What holds up each signal, in the order of `net.signals()`, and the
/// poles the exporter adds. A signal's pole is the first of:
///
/// 1. nothing, for paint on the road;
/// 2. a pole object its `<reference>`s name;
/// 3. a pole object within [`NEAR`] of it;
/// 4. nothing, for a board over a driving lane, such as on a gantry;
/// 5. an added pole, shared with signals within [`NEAR`].
pub(crate) fn supports(net: &RoadNetwork, paths: &Paths) -> (Vec<Support>, Vec<Pole>) {
    let mut poles: Vec<Pole> = Vec::new();
    let supports = net
        .signals()
        .iter()
        .map(|signal| {
            let ground = ground(net, signal);
            if signal.position.z - ground < PAINT {
                return Support::None;
            }
            if let Some(path) = pole_object(net, paths, signal) {
                return Support::Object(path);
            }
            let below = Point::new(signal.position.x, signal.position.y, ground);
            if over_driving_lane(net, below) {
                return Support::None;
            }
            let height = signal.height.unwrap_or(crate::signals::FALLBACK_SIZE);
            let top = signal.position.z + height / 2.0;
            if let Some(k) = poles
                .iter()
                .position(|p| across(p.anchor, signal.position) <= NEAR)
            {
                let pole = &mut poles[k];
                pole.top = pole.top.max(top);
                pole.base.z = pole.base.z.min(ground);
                return Support::Pole(k);
            }
            let (sin, cos) = signal.heading.sin_cos();
            let behind = Vector::new(-cos, -sin, 0.0) * (RADIUS + 0.01);
            poles.push(Pole {
                base: below + behind,
                top,
                anchor: signal.position,
            });
            Support::Pole(poles.len() - 1)
        })
        .collect();
    (supports, poles)
}

/// One `Mesh` per added pole under `/Map/Supports`.
pub(crate) fn write_poles(poles: &[Pole], out: &mut impl Write) -> io::Result<()> {
    open(out, 1, "def Scope", "Supports", &[], &[])?;
    for (k, pole) in poles.iter().enumerate() {
        let ring = |z: f32| {
            (0..SIDES).map(move |i| {
                let (sin, cos) = (TAU * i as f32 / SIDES as f32).sin_cos();
                (
                    Point::new(pole.base.x + RADIUS * cos, pole.base.y + RADIUS * sin, z),
                    Vector::new(cos, sin, 0.0),
                )
            })
        };
        let (mut points, mut normals): (Vec<Point>, Vec<Vector>) =
            ring(pole.base.z).chain(ring(pole.top)).unzip();
        let (cap, up): (Vec<Point>, Vec<Vector>) =
            ring(pole.top).map(|(p, _)| (p, Vector::Z)).unzip();
        points.extend(cap);
        normals.extend(up);
        let n = SIDES as u32;
        let mut indices = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            indices.extend([i, j, n + j, i, n + j, n + i]);
        }
        for i in 1..n - 1 {
            indices.extend([2 * n, 2 * n + i, 2 * n + i + 1]);
        }
        let mesh = MeshPrim {
            name: format!("support_{k}"),
            tags: vec![("synthesized", Tag::Bool(true))],
            points: &points,
            normals: &normals,
            face_size: 3,
            indices,
            colors: vec![[0.5; 3]],
            double_sided: false,
        };
        write_mesh(out, 2, &mesh)?;
    }
    close(out, 1)
}

/// The height of the road straight under `signal`'s board, or where it
/// applies if no road is under it.
fn ground(net: &RoadNetwork, signal: &Signal) -> f32 {
    net.road_position(signal.position)
        .and_then(|at| net.road_point(at))
        .or(signal.applies_at.first().copied())
        .map_or(signal.position.z, |p| p.z)
}

/// The prim of a pole object that holds `signal` up: one its `<reference>`s
/// name, or else the nearest within [`NEAR`].
fn pole_object(net: &RoadNetwork, paths: &Paths, signal: &Signal) -> Option<String> {
    let is_pole = |id| net.object(id).is_some_and(|o| o.kind == ObjectType::Pole);
    let referenced = signal.references.iter().find_map(|r| match r.to {
        Referenced::Object(id) if is_pole(id) => paths.objects.get(&id),
        _ => None,
    });
    let near = || {
        net.objects()
            .iter()
            .filter(|o| o.kind == ObjectType::Pole)
            .filter_map(|o| match o.shape {
                Shape::Solid { position, .. } => Some((across(position, signal.position), o.id)),
                _ => None,
            })
            .filter(|(d, _)| *d <= NEAR)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .and_then(|(_, id)| paths.objects.get(&id))
    };
    referenced.or_else(near).cloned()
}

/// Whether `point` is on a driving lane, seen from above.
fn over_driving_lane(net: &RoadNetwork, point: Point) -> bool {
    net.nearest_lane(point).is_some_and(|(id, at)| {
        net.lane(id)
            .is_some_and(|lane| at.offset.abs() <= lane.width_at(at.s) / 2.0)
    })
}

/// The distance from `a` to `b` across the ground.
fn across(a: Point, b: Point) -> f32 {
    Vector::new(a.x - b.x, a.y - b.y, 0.0).length()
}

fn pole_path(k: usize) -> String {
    format!("/Map/Supports/support_{k}")
}
