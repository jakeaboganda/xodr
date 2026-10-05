//! What holds each signal up: a pole the map has, or one the exporter adds.

use std::f32::consts::TAU;
use std::io::{self, Write};

use xodr::{
    LaneType, Mesh, MeshSampler, ObjectType, Point, Provenance, Referenced, RoadNetwork, Shape,
    Signal, Vector,
};

use crate::{close, open, write_mesh, MeshPrim, Paths, Tag};

/// A board less than this many metres above the road is paint, which no
/// pole holds up.
const PAINT: f32 = 0.1;

/// Metres between a signal and a pole that holds it up, or two signals that
/// share one, measured across the ground.
const NEAR: f32 = 0.5;

/// The radius of a pole the exporter adds, in metres.
const RADIUS: f32 = 0.04;

/// Metres between a pole's axis and the back of a board it holds.
pub(crate) const GAP: f32 = RADIUS + 0.01;

/// Sides on a pole the exporter adds.
const SIDES: usize = 12;

/// Metres between a pole and the nearest lane that carries traffic.
const CLEAR: f32 = 0.5;

/// How far from its board, in metres across the ground, a pole may stand.
const REACH: f32 = 15.0;

/// Metres between the rings of spots tried for a pole.
const STEP: f32 = 0.25;

/// Spots tried on each ring.
const DIRECTIONS: usize = 32;

/// Metres between the top of the highest board a pole holds and the middle
/// of its arm.
const ABOVE: f32 = 0.25;

/// The radius of the bend where a pole turns from rising to its arm, in
/// metres.
const BEND: f32 = 1.0;

/// The radius of the bend where an arm turns down behind the boards, in
/// metres. Small, so the arm stays over the boards until it is behind them.
const ELBOW: f32 = 0.1;

/// Straight pieces in each bend.
const BEND_STEPS: usize = 8;

/// What holds a signal up.
pub(crate) enum Support {
    /// A pole object the map has, by its prim path.
    Object(String),
    /// A pole the exporter adds, by its index in the list of poles.
    Pole(usize),
    /// Nothing: road paint, or a board with no room beside the road.
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

/// How a pole reaches `column`, behind the boards it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Arm {
    /// None: the pole stands at `column` and goes straight up.
    Straight,
    /// The pole stands beside the boards, in line with them. Its arm runs
    /// behind them at the height of the highest board's middle, as on a
    /// roadside cantilever.
    Behind,
    /// The pole stands anywhere else. Its arm runs [`ABOVE`] over the
    /// highest board's top, so it can cross in front of the boards.
    Over,
}

/// A pole the exporter adds, standing at `base`, off every lane that
/// carries traffic. It goes straight up, or it rises, bends by [`BEND`] and
/// runs across to `column`, as [`Arm`] says. Then it drops behind the
/// boards to `bottom`, turning down by [`ELBOW`].
pub(crate) struct Pole {
    base: Point,
    column: Point,
    facing: Vector,
    arm: Arm,
    /// Whether it holds a two-faced signal, between its boards. No other
    /// signal shares it.
    two_faced: bool,
    /// The top of the highest board it holds.
    top: f32,
    /// The middle of the highest board it holds.
    high: f32,
    /// The middle of the lowest board it holds.
    bottom: f32,
}

impl Pole {
    /// The pole's centerline, with sharp corners.
    fn path(&self) -> Vec<Point> {
        let at = |p: Point, z| Point::new(p.x, p.y, z);
        let arm = match self.arm {
            Arm::Straight => return vec![self.base, at(self.base, self.top)],
            Arm::Behind => self.high,
            Arm::Over => self.top + ABOVE,
        };
        let mut path = vec![self.base, at(self.base, arm), at(self.column, arm)];
        if self.bottom < arm - 1e-3 {
            path.push(at(self.column, self.bottom));
        }
        path
    }

    /// Whether the pole can also hold `signal`, which faces `facing`: it
    /// stands within [`NEAR`] of it, behind its box, and faces the same way.
    fn holds(&self, signal: &Signal, facing: Vector) -> bool {
        let position = signal.position;
        let to = Vector::new(self.column.x - position.x, self.column.y - position.y, 0.0);
        let back = crate::signals::back(signal);
        !self.two_faced
            && across(self.column, position) <= NEAR
            && to.dot(facing) <= -(back + RADIUS)
            && self.facing.dot(facing) > 0.99
    }
}

/// What holds up each signal, in the order of `net.signals()`, and the
/// poles the exporter adds. `traffic` is the surface of the lanes that carry
/// traffic, from [`traffic`]. A signal's pole is the first of:
///
/// 1. nothing, for paint on the road;
/// 2. a pole object its `<reference>`s name;
/// 3. a pole object within [`NEAR`] of it;
/// 4. an added pole, shared with signals within [`NEAR`] that face the same
///    way and stand in front of it. A two-faced signal shares with none;
/// 5. nothing, if no spot off the traffic is within [`REACH`].
pub(crate) fn supports(
    net: &RoadNetwork,
    provenance: &Provenance,
    paths: &Paths,
    traffic: &MeshSampler,
) -> (Vec<Support>, Vec<Pole>) {
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
            let height = signal.height.unwrap_or(crate::signals::FALLBACK_SIZE);
            let (top, middle) = (signal.position.z + height, signal.position.z + height / 2.0);
            let (sin, cos) = signal.heading.sin_cos();
            let facing = Vector::new(cos, sin, 0.0);
            let two_faced = crate::signals::two_faced(provenance, signal);
            let found = (!two_faced)
                .then(|| poles.iter().position(|p| p.holds(signal, facing)))
                .flatten();
            if let Some(k) = found {
                let pole = &mut poles[k];
                pole.top = pole.top.max(top);
                pole.high = pole.high.max(middle);
                pole.bottom = pole.bottom.min(middle);
                return Support::Pole(k);
            }
            let behind = match two_faced {
                true => 0.0,
                false => crate::signals::back(signal) + GAP,
            };
            let column = Point::new(signal.position.x, signal.position.y, ground) - facing * behind;
            let found = match traffic.height_at(column.x, column.y) {
                None => Some((column, Arm::Straight)),
                Some(_) => beside_traffic(net, traffic, column, facing),
            };
            let Some((base, arm)) = found else {
                return Support::None;
            };
            poles.push(Pole {
                base,
                column,
                facing,
                arm,
                two_faced,
                top,
                high: middle,
                bottom: middle,
            });
            Support::Pole(poles.len() - 1)
        })
        .collect();
    (supports, poles)
}

/// The part of `surface`, a [`RoadNetwork::surface_mesh`], whose lanes
/// carry traffic: every lane but a sidewalk, border, curb, median or one of
/// type `none`. A pole may stand on those.
pub(crate) fn traffic(net: &RoadNetwork, surface: &Mesh) -> Mesh {
    let indices = surface
        .lanes
        .iter()
        .filter(|span| {
            net.lane(span.lane).is_some_and(|lane| {
                !matches!(
                    lane.kind,
                    LaneType::Sidewalk
                        | LaneType::Border
                        | LaneType::Curb
                        | LaneType::Median
                        | LaneType::None
                )
            })
        })
        .flat_map(|span| &surface.indices[span.indices.start as usize..span.indices.end as usize])
        .copied()
        .collect();
    Mesh {
        vertices: surface.vertices.clone(),
        normals: surface.normals.clone(),
        indices,
        ..Mesh::default()
    }
}

/// Where a pole for a board over traffic stands, and how its arm reaches
/// `column`. The spot is within [`REACH`] across the ground, has no traffic
/// within [`CLEAR`] of it, and is at the height of the road nearest it.
///
/// It is the nearest spot in line with the board, along its width, so the
/// arm runs behind the board. Failing that, it is the nearest spot in any
/// direction, and the arm runs over the board.
fn beside_traffic(
    net: &RoadNetwork,
    traffic: &MeshSampler,
    column: Point,
    facing: Vector,
) -> Option<(Point, Arm)> {
    let around = |x: f32, y: f32, radius: f32| {
        (0..DIRECTIONS).map(move |k| {
            let (sin, cos) = (TAU * k as f32 / DIRECTIONS as f32).sin_cos();
            (x + radius * cos, y + radius * sin)
        })
    };
    let clear = |&(x, y): &(f32, f32)| {
        std::iter::once((x, y))
            .chain(around(x, y, CLEAR))
            .all(|(x, y)| traffic.height_at(x, y).is_none())
    };
    let rings = (REACH / STEP) as usize;
    let side = Vector::Z.cross(facing);
    let mut in_line = (1..=rings).flat_map(|i| {
        let d = i as f32 * STEP;
        [d, -d].map(|d| (column.x + side.x * d, column.y + side.y * d))
    });
    let mut anywhere = (1..=rings).flat_map(|i| around(column.x, column.y, i as f32 * STEP));
    let ((x, y), arm) = in_line
        .find(clear)
        .map(|spot| (spot, Arm::Behind))
        .or_else(|| anywhere.find(clear).map(|spot| (spot, Arm::Over)))?;
    let near = Point::new(x, y, column.z);
    let ground = net.road_position(near).and_then(|at| net.road_point(at));
    Some((Point::new(x, y, ground.map_or(column.z, |p| p.z)), arm))
}

/// One `Mesh` per added pole under `/Map/Supports`: a tube along its path,
/// with round bends and a cap at the end.
pub(crate) fn write_poles(poles: &[Pole], out: &mut impl Write) -> io::Result<()> {
    open(out, 1, "def Scope", "Supports", &[], &[])?;
    for (k, pole) in poles.iter().enumerate() {
        let (points, normals, indices) = tube(&rounded(&pole.path(), &[BEND, ELBOW]));
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

/// `path` with each corner replaced by an arc. The arcs take their radii
/// from `radii` in order, or less where a straight piece is too short.
fn rounded(path: &[Point], radii: &[f32]) -> Vec<Point> {
    let mut line = vec![path[0]];
    for (w, &radius) in path.windows(3).zip(radii) {
        let (corner, a, b) = (w[1], w[1] - w[0], w[2] - w[1]);
        let (into, out) = (a.normalize_or(Vector::Z), b.normalize_or(Vector::Z));
        let turn = into.dot(out).clamp(-1.0, 1.0).acos();
        if turn < 1e-3 {
            line.push(corner);
            continue;
        }
        let cut = (radius * (turn / 2.0).tan())
            .min(a.length() / 2.0)
            .min(b.length() / 2.0);
        let arc = cut / (turn / 2.0).tan();
        let inward = (out - into * into.dot(out)).normalize_or(Vector::Z);
        let center = corner - into * cut + inward * arc;
        line.extend((0..=BEND_STEPS).map(|i| {
            let angle = turn * i as f32 / BEND_STEPS as f32;
            center + (inward * -angle.cos() + into * angle.sin()) * arc
        }));
    }
    line.extend(path.last());
    line.dedup_by(|a, b| (*a - *b).length() < 1e-4);
    line
}

/// A tube of [`RADIUS`] along `line`, which lies in one upright plane, and
/// a cap over its last end: points, normals and triangle indices.
fn tube(line: &[Point]) -> (Vec<Point>, Vec<Vector>, Vec<u32>) {
    let (first, last) = (line[0], line[line.len() - 1]);
    let across = Vector::new(last.x - first.x, last.y - first.y, 0.0);
    let side = Vector::Z.cross(across).normalize_or(Vector::X);
    let tangent = |i: usize| {
        let into = (i > 0).then(|| (line[i] - line[i - 1]).normalize_or_zero());
        let out = line
            .get(i + 1)
            .map(|&next| (next - line[i]).normalize_or_zero());
        (into.unwrap_or(Vector::ZERO) + out.unwrap_or(Vector::ZERO)).normalize_or(Vector::Z)
    };
    let ring = |center: Point, along: Vector| {
        let normal = side.cross(along);
        (0..SIDES).map(move |k| {
            let (sin, cos) = (TAU * k as f32 / SIDES as f32).sin_cos();
            let out = normal * cos + side * sin;
            (center + out * RADIUS, out)
        })
    };
    let (mut points, mut normals): (Vec<Point>, Vec<Vector>) = line
        .iter()
        .enumerate()
        .flat_map(|(i, &center)| ring(center, tangent(i)))
        .unzip();
    let n = SIDES as u32;
    let mut indices = Vec::new();
    for r in 0..line.len() as u32 - 1 {
        let (a, b) = (r * n, (r + 1) * n);
        for i in 0..n {
            let j = (i + 1) % n;
            indices.extend([a + i, a + j, b + j, a + i, b + j, b + i]);
        }
    }
    let end = tangent(line.len() - 1);
    let cap = points.len() as u32;
    points.extend(ring(last, end).map(|(p, _)| p));
    normals.extend(std::iter::repeat_n(end, SIDES));
    for i in 1..n - 1 {
        indices.extend([cap, cap + i, cap + i + 1]);
    }
    (points, normals, indices)
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

/// The distance from `a` to `b` across the ground.
fn across(a: Point, b: Point) -> f32 {
    Vector::new(a.x - b.x, a.y - b.y, 0.0).length()
}

fn pole_path(k: usize) -> String {
    format!("/Map/Supports/support_{k}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The poles the exporter adds to `map`, and the surface of its traffic.
    fn poles(map: &str) -> (Vec<Pole>, Mesh) {
        let (net, provenance) =
            xodr::load_file_with_provenance(format!("../tests/data/{map}.xodr")).expect("loads");
        let surface = net.surface_mesh();
        let sink = &mut io::sink();
        let paths = Paths {
            lanes: crate::roads(&net, &surface, sink).expect("writes"),
            objects: crate::objects(&net, &provenance, &net.object_mesh(), sink).expect("writes"),
        };
        let traffic = traffic(&net, &surface);
        let (_, poles) = supports(&net, &provenance, &paths, &traffic.sampler());
        (poles, traffic)
    }

    #[test]
    fn no_pole_stands_in_traffic() {
        for map in [
            "signals",
            "lane_heights",
            "signal_semantics",
            "traffic_rule",
        ] {
            let (poles, traffic) = poles(map);
            for pole in &poles {
                let base = pole.base;
                assert!(
                    traffic.height_at(base.x, base.y).is_none(),
                    "{map}: {base:?}"
                );
            }
        }
    }

    #[test]
    fn a_pole_bends_over_traffic_to_a_board_above_it() {
        let (poles, traffic) = poles("signals");
        let bent: Vec<&Pole> = poles
            .iter()
            .filter(|p| traffic.height_at(p.column.x, p.column.y).is_some())
            .collect();
        assert!(bent.len() >= 2, "the gantry and the side light");
        for pole in bent {
            let path = rounded(&pole.path(), &[BEND, ELBOW]);
            let end = path.last().expect("a path");
            assert!(
                across(*end, pole.column) < 1e-3,
                "{end:?} reaches {:?}",
                pole.column
            );
            assert!(across(path[0], pole.column) > CLEAR);
        }
    }

    #[test]
    fn a_bend_is_round() {
        let corner = [
            Point::ORIGIN,
            Point::new(0.0, 0.0, 4.0),
            Point::new(3.0, 0.0, 4.0),
        ];
        let line = rounded(&corner, &[BEND]);
        let center = Point::new(BEND, 0.0, 4.0 - BEND);
        let arc = &line[1..line.len() - 1];
        assert_eq!(arc.len(), BEND_STEPS + 1);
        for p in arc {
            assert!(((*p - center).length() - BEND).abs() < 1e-4, "{p:?}");
        }
    }

    #[test]
    fn a_pole_never_goes_through_its_boards() {
        let map = std::fs::read_to_string("../tests/data/signals.xodr").expect("reads");
        clears_its_boards(&map);
        let deep = r#"name="SideLight" length="0.6""#;
        clears_its_boards(&map.replace(r#"name="SideLight""#, deep));
    }

    /// Check that no pole the exporter adds to `map` goes through a board it
    /// holds. `map` is `signals.xodr`, maybe changed.
    fn clears_its_boards(map: &str) {
        let (net, provenance) = xodr::load_str_with_provenance(map).expect("loads");
        let surface = net.surface_mesh();
        let sink = &mut io::sink();
        let paths = Paths {
            lanes: crate::roads(&net, &surface, sink).expect("writes"),
            objects: crate::objects(&net, &provenance, &net.object_mesh(), sink).expect("writes"),
        };
        let traffic = traffic(&net, &surface);
        let (supports, poles) = supports(&net, &provenance, &paths, &traffic.sampler());
        let (mut arms, mut two_faced) = (0, 0);
        for (signal, support) in net.signals().iter().zip(&supports) {
            let Support::Pole(k) = support else { continue };
            let path = poles[*k].path();
            arms += usize::from(path.len() > 2);
            let both = crate::signals::two_faced(&provenance, signal);
            two_faced += usize::from(both);
            let depth = signal.length.unwrap_or(0.0) / 2.0;
            let (width, height) = (signal.width.unwrap_or(0.0), signal.height.unwrap_or(0.0));
            let gap = crate::signals::back(signal) + GAP;
            for p in rounded(&path, &[BEND, ELBOW]) {
                let [x, y, z] = crate::signals::unrotate(signal, p - signal.position);
                let beside = y.abs() > width / 2.0 + RADIUS;
                let off = z > height + RADIUS || z < -RADIUS;
                let clear = match both {
                    true => x.abs() + RADIUS <= gap - depth + 1e-4,
                    false => x + RADIUS <= -depth + 1e-4,
                };
                assert!(beside || off || clear, "{} at {p:?}", signal.name);
            }
        }
        assert!(arms >= 2, "the gantry and the side light");
        assert_eq!(two_faced, 1, "the road works sign");
    }

    #[test]
    fn a_pole_over_traffic_is_a_cantilever_beside_its_board() {
        let map = std::fs::read_to_string("../tests/data/signals.xodr").expect("reads");
        let (net, provenance) = xodr::load_str_with_provenance(&map).expect("loads");
        let surface = net.surface_mesh();
        let sink = &mut io::sink();
        let paths = Paths {
            lanes: crate::roads(&net, &surface, sink).expect("writes"),
            objects: crate::objects(&net, &provenance, &net.object_mesh(), sink).expect("writes"),
        };
        let traffic = traffic(&net, &surface);
        let (supports, poles) = supports(&net, &provenance, &paths, &traffic.sampler());
        for name in ["Gantry", "SideLight"] {
            let (signal, support) = net
                .signals()
                .iter()
                .zip(&supports)
                .find(|(s, _)| s.name == name)
                .expect("the signal");
            let Support::Pole(k) = support else {
                panic!("{name} has an added pole")
            };
            let pole = &poles[*k];
            assert_eq!(pole.arm, Arm::Behind, "{name}");
            let middle = signal.position.z + signal.height.expect("a height") / 2.0;
            assert!(
                (pole.path()[2].z - middle).abs() < 1e-4,
                "{name}'s arm is level"
            );
            let along = pole.facing.dot(pole.base - pole.column);
            assert!(along.abs() < 1e-3, "{name}'s pole is {along} m out of line");
        }
    }
}
