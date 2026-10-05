//! What holds each signal up: a pole the map has, or a structure the
//! exporter adds.
//!
//! Signals at one spot share a structure, whichever way they face. Over
//! traffic, the structure is a cantilever, a span gantry or a space frame,
//! chosen by its span, the sign area it carries and the lanes it crosses.
//! The limits are AASHTO's, as state DOTs such as WSDOT and DelDOT apply
//! them.

use std::collections::HashSet;
use std::f32::consts::TAU;

use xodr::{
    LaneType, Mesh, MeshSampler, ObjectType, Point, Provenance, Referenced, RoadId, RoadNetwork,
    Shape, Signal, Vector,
};

use crate::signals::{back, two_faced, FALLBACK_SIZE};
use crate::structures::{Frame, Held, Kind, Structure, ABOVE, GAP};
use crate::{Paths, Tag};

/// A board less than this many metres above the road is paint, which no
/// structure holds up.
const PAINT: f32 = 0.1;

/// Metres across the ground within which a pole object holds a signal up,
/// and within which signals beside the road share a pole.
const NEAR: f32 = 0.5;

/// Metres along the way they face within which signals share a structure.
const ALONG: f32 = 0.5;

/// Metres between a structure's foot and the nearest lane that carries
/// traffic.
const CLEAR: f32 = 0.5;

/// Metres between the spots tried for a foot.
const STEP: f32 = 0.25;

/// How far out from its boards, in metres, a structure's leg may stand.
const SEARCH: f32 = 40.0;

/// How far from its boards, in metres, a fallback arm may stand.
const REACH: f32 = 15.0;

/// Spots tried on each ring around the boards, for a fallback arm.
const DIRECTIONS: usize = 32;

/// The most a structure over traffic may span, carry and cross.
#[derive(Clone, Copy)]
struct Limits {
    /// Metres: a cantilever's arm, or a gantry's span from leg to leg.
    span: f32,
    /// Square metres of sign.
    area: f32,
    /// Lanes under the arm or span: driving lanes for a cantilever, every
    /// lane that carries traffic for a gantry.
    lanes: usize,
}

/// AASHTO's limits on a cantilever.
const CANTILEVER: Limits = Limits {
    span: 13.0,
    area: 20.0,
    lanes: 2,
};

/// AASHTO's limits on a span gantry. A structure over them is a space
/// frame.
const GANTRY: Limits = Limits {
    span: 27.5,
    area: 55.0,
    lanes: 5,
};

/// What holds a signal up.
pub(crate) enum Support {
    /// A pole object the map has, by its prim path.
    Object(String),
    /// A structure the exporter adds, by its index.
    Added(usize),
    /// Nothing: road paint, or a board with no room for a structure.
    None,
}

impl Support {
    /// The signal's `xodr:support` and `xodr:supportPrim`.
    pub(crate) fn tags(&self) -> [(&'static str, Tag); 2] {
        let (kind, targets) = match self {
            Self::Object(path) => ("object", vec![path.clone()]),
            Self::Added(k) => ("synthesized", vec![format!("/Map/Supports/support_{k}")]),
            Self::None => ("none", vec![]),
        };
        [
            ("support", Tag::Token(kind)),
            ("supportPrim", Tag::Targets(targets)),
        ]
    }
}

/// What holds a signal up, and where its boards stand in its own frame, so
/// they clear the structure. `front` is the X of `board`, and `back` how far
/// behind the position `board_back` stands, on a two-faced signal.
pub(crate) struct Placement {
    pub(crate) support: Support,
    pub(crate) front: f32,
    pub(crate) back: f32,
}

/// A signal a structure holds, in the structure's frame.
struct Member {
    signal: usize,
    /// `+1` if it faces along the frame's axis, `-1` if against.
    faces: f32,
    /// Metres along the axis from the frame's origin.
    along: f32,
    /// Metres across, along the frame's side.
    u: f32,
    two_faced: bool,
    /// How far behind its position the back of its box reaches.
    back: f32,
    width: f32,
    area: f32,
    middle: f32,
    top: f32,
    ground: f32,
}

/// Signals that share a structure: over traffic, or beside it.
struct Group {
    origin: Point,
    axis: Vector,
    over: bool,
    members: Vec<Member>,
}

impl Group {
    fn side(&self) -> Vector {
        Vector::Z.cross(self.axis)
    }

    /// The center line of a structure `clearance` metres from the back of
    /// every board, between the two ways the boards face if they face both.
    fn line(&self, clearance: f32) -> f32 {
        let faces = self.members.iter().flat_map(|m| {
            let both = m.two_faced.then_some(-m.faces);
            std::iter::once(m.faces).chain(both).map(move |f| (f, m))
        });
        let (mut front, mut behind) = (f32::INFINITY, f32::NEG_INFINITY);
        for (faces, m) in faces {
            if faces > 0.0 {
                front = front.min(m.along - m.back - clearance);
            } else {
                behind = behind.max(m.along + m.back + clearance);
            }
        }
        match (front.is_finite(), behind.is_finite()) {
            (true, true) => (front + behind) / 2.0,
            (true, false) => front,
            _ => behind,
        }
    }

    /// How far each board must move toward its traffic, in its signal's
    /// frame, to stand `clearance` from the line: `board`'s, then
    /// `board_back`'s.
    fn shifts(&self, line: f32, clearance: f32) -> Vec<(f32, f32)> {
        self.members
            .iter()
            .map(|m| {
                let ahead = m.faces * (m.along - line);
                let front = (clearance + m.back - ahead).max(0.0);
                let back = (clearance + m.back + ahead).max(0.0);
                (front, back)
            })
            .collect()
    }

    /// The boards, for hangers.
    fn held(&self) -> Vec<Held> {
        let held = |m: &Member, faces| Held {
            u: m.u,
            faces,
            middle: m.middle,
        };
        self.members
            .iter()
            .flat_map(|m| {
                let both = m.two_faced.then(|| held(m, -m.faces));
                std::iter::once(held(m, m.faces)).chain(both)
            })
            .collect()
    }
}

/// How far a structure over traffic reaches, and the lanes it crosses.
#[derive(Clone, Copy, Debug)]
struct Reach {
    length: f32,
    lanes: usize,
}

/// Which structure to build over traffic.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Choice {
    /// A cantilever whose leg stands on the left (`-1`) or right (`+1`).
    Cantilever(i8),
    Gantry,
    SpaceFrame,
    /// Neither leg has room, so no structure from AASHTO's list fits.
    Arm,
}

/// The structure for `area` square metres of sign, given how far a
/// cantilever from each side would reach and how far a gantry would span.
/// The shorter cantilever that fits [`CANTILEVER`] wins, then a gantry that
/// fits [`GANTRY`], then a space frame.
fn choose(area: f32, left: Option<Reach>, right: Option<Reach>, span: Option<Reach>) -> Choice {
    let fits = |r: &Reach, l: Limits| r.length <= l.span && area <= l.area && r.lanes <= l.lanes;
    let cantilever = [(-1, left), (1, right)]
        .into_iter()
        .filter_map(|(side, r)| r.filter(|r| fits(r, CANTILEVER)).map(|r| (side, r.length)))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    match (cantilever, span) {
        (Some((side, _)), _) => Choice::Cantilever(side),
        (None, Some(r)) if fits(&r, GANTRY) => Choice::Gantry,
        (None, Some(_)) => Choice::SpaceFrame,
        (None, None) => Choice::Arm,
    }
}

/// What holds up each signal, in the order of `net.signals()`, and the
/// structures the exporter adds. A signal's support is the first of:
///
/// 1. nothing, for paint on the road;
/// 2. a pole object its `<reference>`s name, or one within [`NEAR`];
/// 3. a structure it shares with the signals at its spot. Beside the road
///    that is a pole. Over traffic it is what [`choose`] picks;
/// 4. nothing, if no structure has room.
pub(crate) fn supports(
    net: &RoadNetwork,
    provenance: &Provenance,
    paths: &Paths,
    surface: &Mesh,
) -> (Vec<Placement>, Vec<Structure>) {
    let traffic_mesh = traffic(net, surface);
    let traffic = traffic_mesh.sampler();
    let lanes = Lanes::new(net, surface);
    let signals = net.signals();
    let mut placements: Vec<Placement> = signals
        .iter()
        .map(|s| {
            let apart = match two_faced(provenance, s) {
                true => back(s) + GAP,
                false => 0.0,
            };
            Placement {
                support: Support::None,
                front: apart,
                back: apart,
            }
        })
        .collect();
    let mut groups: Vec<Group> = vec![];
    for (i, signal) in signals.iter().enumerate() {
        let ground = ground(net, signal);
        if signal.position.z - ground < PAINT {
            continue;
        }
        if let Some(path) = pole_object(net, paths, signal) {
            placements[i].support = Support::Object(path);
            continue;
        }
        let (sin, cos) = signal.heading.sin_cos();
        let facing = Vector::new(cos, sin, 0.0);
        let p = signal.position;
        let over = traffic.height_at(p.x, p.y).is_some();
        let height = signal.height.unwrap_or(FALLBACK_SIZE);
        let width = signal.width.unwrap_or(FALLBACK_SIZE);
        let member = |g: &Group| {
            let to = Vector::new(p.x - g.origin.x, p.y - g.origin.y, 0.0);
            Member {
                signal: i,
                faces: facing.dot(g.axis).signum(),
                along: to.dot(g.axis),
                u: to.dot(g.side()),
                two_faced: two_faced(provenance, signal),
                back: back(signal),
                width,
                area: width * height,
                middle: p.z + height / 2.0,
                top: p.z + height,
                ground,
            }
        };
        let joins = |g: &Group| {
            let m = member(g);
            let reach = if over { SEARCH } else { NEAR };
            g.over == over
                && facing.dot(g.axis).abs() > 0.99
                && m.along.abs() <= ALONG
                && m.u.abs() <= reach
        };
        match groups.iter().position(joins) {
            Some(k) => {
                let m = member(&groups[k]);
                groups[k].members.push(m);
            }
            None => {
                let mut group = Group {
                    origin: Point::new(p.x, p.y, ground),
                    axis: facing,
                    over,
                    members: vec![],
                };
                group.members.push(member(&group));
                groups.push(group);
            }
        }
    }
    let mut structures = vec![];
    for group in &groups {
        let Some((structure, frame_line, kind)) = build(net, &traffic, &lanes, group) else {
            continue;
        };
        let shifts = group.shifts(frame_line, kind.clearance());
        for (m, (front, back)) in group.members.iter().zip(shifts) {
            placements[m.signal] = Placement {
                support: Support::Added(structures.len()),
                front,
                back,
            };
        }
        structures.push(structure);
    }
    (placements, structures)
}

/// The structure for `group`, its center line and its kind, or `None` if
/// none has room.
fn build(
    net: &RoadNetwork,
    traffic: &MeshSampler,
    lanes: &Lanes,
    group: &Group,
) -> Option<(Structure, f32, Kind)> {
    let frame = |line| Frame {
        origin: group.origin,
        axis: group.axis,
        side: group.side(),
        line,
    };
    let members = &group.members;
    let top = members.iter().map(|m| m.top).fold(f32::MIN, f32::max);
    let high = members.iter().map(|m| m.middle).fold(f32::MIN, f32::max);
    let ground = members.iter().map(|m| m.ground).fold(f32::MAX, f32::min);
    if !group.over {
        let line = group.line(Kind::Pole.clearance());
        let foot = frame(line).at(0.0, 0.0, ground);
        let structure = Structure::pole(foot, top);
        return Some((structure, line, Kind::Pole));
    }
    let lo = members
        .iter()
        .map(|m| m.u - m.width / 2.0)
        .fold(f32::MAX, f32::min);
    let hi = members
        .iter()
        .map(|m| m.u + m.width / 2.0)
        .fold(f32::MIN, f32::max);
    let face = |d: f32| {
        let facing = members.iter().filter(|m| m.faces == d || m.two_faced);
        facing.map(|m| m.area).sum::<f32>()
    };
    let area = face(1.0).max(face(-1.0));
    let measure = |kind: Kind| {
        let f = frame(group.line(kind.clearance()));
        let clear = |u| {
            let depth = kind.footprint();
            [-depth / 2.0, depth / 2.0]
                .iter()
                .all(|&a| clear(traffic, f.at(a, u, 0.0)))
        };
        let (left, right) = (leg(lo, -1.0, clear), leg(hi, 1.0, clear));
        (f, left, right)
    };
    let reach = |f: &Frame, from: f32, to: f32, drivable| Reach {
        length: (to - from).abs(),
        lanes: lanes.count(f.at(0.0, from, 0.0), f.at(0.0, to, 0.0), drivable),
    };
    let (cf, cl, cr) = measure(Kind::Cantilever);
    let (gf, gl, gr) = measure(Kind::Gantry);
    let choice = choose(
        area,
        cl.map(|u| reach(&cf, u, hi, true)),
        cr.map(|u| reach(&cf, lo, u, true)),
        gl.zip(gr).map(|(l, r)| reach(&gf, l, r, false)),
    );
    let held = group.held();
    let at_ground = |f: &Frame, u: f32| {
        let p = f.at(0.0, u, ground);
        let road = net.road_position(p).and_then(|at| net.road_point(at));
        (u, road.map_or(ground, |r| r.z))
    };
    let span = |kind, f: Frame, left, right| {
        let structure = Structure::span(
            kind,
            &f,
            at_ground(&f, left),
            at_ground(&f, right),
            high,
            &held,
        );
        Some((structure, f.line, kind))
    };
    let arm = || {
        let f = frame(group.line(Kind::Arm.clearance()));
        let reach = (lo + hi) / 2.0;
        let foot = anywhere(net, traffic, f.at(0.0, reach, ground))?;
        let structure = Structure::arm(&f, foot, reach, top + ABOVE, &held);
        Some((structure, f.line, Kind::Arm))
    };
    match choice {
        Choice::Cantilever(side) => {
            let (leg, end) = match side {
                -1 => (cl?, members.iter().map(|m| m.u).fold(f32::MIN, f32::max)),
                _ => (cr?, members.iter().map(|m| m.u).fold(f32::MAX, f32::min)),
            };
            let (_, z) = at_ground(&cf, leg);
            let foot = cf.at(0.0, leg, z);
            let structure = Structure::cantilever(&cf, foot, leg, end, high, &held);
            Some((structure, cf.line, Kind::Cantilever))
        }
        Choice::Gantry => span(Kind::Gantry, gf, gl?, gr?),
        Choice::SpaceFrame => match measure(Kind::SpaceFrame) {
            (f, Some(left), Some(right)) => span(Kind::SpaceFrame, f, left, right),
            _ => arm(),
        },
        Choice::Arm => arm(),
    }
}

/// The first `u` past `from`, going `dir`, where `clear(u)` holds, within
/// [`SEARCH`].
fn leg(from: f32, dir: f32, clear: impl Fn(f32) -> bool) -> Option<f32> {
    (1..=(SEARCH / STEP) as usize)
        .map(|i| from + dir * i as f32 * STEP)
        .find(|&u| clear(u))
}

/// Whether no traffic is within [`CLEAR`] of `p`, across the ground.
fn clear(traffic: &MeshSampler, p: Point) -> bool {
    std::iter::once((p.x, p.y))
        .chain(around(p.x, p.y, CLEAR))
        .all(|(x, y)| traffic.height_at(x, y).is_none())
}

fn around(x: f32, y: f32, radius: f32) -> impl Iterator<Item = (f32, f32)> {
    (0..DIRECTIONS).map(move |k| {
        let (sin, cos) = (TAU * k as f32 / DIRECTIONS as f32).sin_cos();
        (x + radius * cos, y + radius * sin)
    })
}

/// The nearest spot to `center` with no traffic within [`CLEAR`], within
/// [`REACH`] across the ground, at the height of the road nearest it.
fn anywhere(net: &RoadNetwork, traffic: &MeshSampler, center: Point) -> Option<Point> {
    let (x, y) = (1..=(REACH / STEP) as usize)
        .flat_map(|i| around(center.x, center.y, i as f32 * STEP))
        .find(|&(x, y)| clear(traffic, Point::new(x, y, center.z)))?;
    let near = Point::new(x, y, center.z);
    let road = net.road_position(near).and_then(|at| net.road_point(at));
    Some(Point::new(x, y, road.map_or(center.z, |p| p.z)))
}

/// Whether a lane of `kind` carries traffic: every type but a sidewalk,
/// border, curb, median or `none`. A structure may stand on those.
fn carries_traffic(kind: LaneType) -> bool {
    !matches!(
        kind,
        LaneType::Sidewalk | LaneType::Border | LaneType::Curb | LaneType::Median | LaneType::None
    )
}

/// The part of `surface`, a [`RoadNetwork::surface_mesh`], whose lanes
/// carry traffic.
pub(crate) fn traffic(net: &RoadNetwork, surface: &Mesh) -> Mesh {
    let indices = surface
        .lanes
        .iter()
        .filter(|span| net.lane(span.lane).is_some_and(|l| carries_traffic(l.kind)))
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

/// The lanes that carry traffic, as triangles seen from above, to count the
/// lanes a structure crosses. Lanes inside a junction don't count: its
/// connecting lanes overlap on the same pavement, so a line across a
/// junction would count each of them.
struct Lanes {
    lanes: Vec<LaneArea>,
}

struct LaneArea {
    /// The road and `<lane id>`, so a lane split into sections counts once.
    key: (RoadId, i32),
    drivable: bool,
    low: [f32; 2],
    high: [f32; 2],
    triangles: Vec<[[f32; 2]; 3]>,
}

impl Lanes {
    fn new(net: &RoadNetwork, surface: &Mesh) -> Self {
        let lanes = surface
            .lanes
            .iter()
            .filter_map(|span| {
                let lane = net.lane(span.lane).filter(|l| carries_traffic(l.kind))?;
                let at = net.road_lane(span.lane)?;
                if net.road(at.road)?.junction().is_some() {
                    return None;
                }
                let indices =
                    &surface.indices[span.indices.start as usize..span.indices.end as usize];
                let corner = |i: u32| {
                    let v = surface.vertices[i as usize];
                    [v.x, v.y]
                };
                let triangles: Vec<[[f32; 2]; 3]> = indices
                    .chunks_exact(3)
                    .map(|t| [corner(t[0]), corner(t[1]), corner(t[2])])
                    .collect();
                let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
                for c in triangles.iter().flatten() {
                    for k in 0..2 {
                        low[k] = low[k].min(c[k]);
                        high[k] = high[k].max(c[k]);
                    }
                }
                Some(LaneArea {
                    key: (at.road, at.od_id),
                    drivable: lane.kind.is_drivable(),
                    low,
                    high,
                    triangles,
                })
            })
            .collect();
        Self { lanes }
    }

    /// The lanes under the line from `a` to `b`, seen from above: driving
    /// lanes only if `drivable`, else every lane that carries traffic. It
    /// samples the middle of each [`STEP`], so a lane the line only touches
    /// at an end doesn't count.
    fn count(&self, a: Point, b: Point, drivable: bool) -> usize {
        let steps = ((b - a).length() / STEP).ceil().max(1.0) as usize;
        let mut hit = HashSet::new();
        for k in 0..steps {
            let p = a + (b - a) * ((k as f32 + 0.5) / steps as f32);
            for lane in self.lanes.iter().filter(|l| l.drivable || !drivable) {
                let inside = (0..2).all(|i| {
                    let v = [p.x, p.y][i];
                    lane.low[i] <= v && v <= lane.high[i]
                });
                if inside && lane.triangles.iter().any(|t| covers(t, [p.x, p.y])) {
                    hit.insert(lane.key);
                }
            }
        }
        hit.len()
    }
}

/// Whether triangle `t` covers `p`, seen from above.
fn covers(t: &[[f32; 2]; 3], p: [f32; 2]) -> bool {
    let edge =
        |a: [f32; 2], b: [f32; 2]| (p[0] - b[0]) * (a[1] - b[1]) - (a[0] - b[0]) * (p[1] - b[1]);
    let s = [edge(t[0], t[1]), edge(t[1], t[2]), edge(t[2], t[0])];
    let (low, high) = s
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    !(low < 0.0 && high > 0.0)
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

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::signals::unrotate;

    /// The placements and structures the exporter makes for `map`, an
    /// OpenDRIVE document, with the map and the surface of its traffic.
    fn export(
        map: &str,
    ) -> (
        RoadNetwork,
        Provenance,
        Vec<Placement>,
        Vec<Structure>,
        Mesh,
    ) {
        let (net, provenance) = xodr::load_str_with_provenance(map).expect("loads");
        let surface = net.surface_mesh();
        let sink = &mut io::sink();
        let paths = Paths {
            lanes: crate::roads(&net, &surface, sink).expect("writes"),
            objects: crate::objects(&net, &provenance, &net.object_mesh(), sink).expect("writes"),
        };
        let (placements, structures) = supports(&net, &provenance, &paths, &surface);
        let traffic = traffic(&net, &surface);
        (net, provenance, placements, structures, traffic)
    }

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("../tests/data/{name}.xodr")).expect("reads")
    }

    /// A straight road 100 m long with `lanes` driving lanes 3.5 m wide on
    /// its right, and `signals`, `<signal>` elements.
    fn road(lanes: usize, signals: &str) -> String {
        let right: String = (1..=lanes)
            .map(|k| {
                format!(
                    r#"<lane id="-{k}" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>"#
                )
            })
            .collect();
        format!(
            r#"<OpenDRIVE><header revMajor="1" revMinor="6"/>
            <road id="1" length="100" junction="-1">
              <planView><geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>
              <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>{right}</right></laneSection></lanes>
              <signals>{signals}</signals>
            </road></OpenDRIVE>"#
        )
    }

    /// A sign at `t` across the road, `width` by `height`, facing `orientation`.
    fn sign(id: u32, t: f32, width: f32, height: f32, orientation: &str) -> String {
        format!(
            r#"<signal id="{id}" s="50" t="{t}" zOffset="5" orientation="{orientation}" dynamic="no" country="DE" type="332" subtype="-1" width="{width}" height="{height}"/>"#
        )
    }

    /// `signal` 2.5 m lower.
    fn low(signal: String) -> String {
        signal.replace(r#"zOffset="5""#, r#"zOffset="2.5""#)
    }

    fn kinds(map: &str) -> Vec<Kind> {
        export(map).3.iter().map(|s| s.kind).collect()
    }

    #[test]
    fn the_structure_follows_the_aashto_limits() {
        let at = |length, lanes| Some(Reach { length, lanes });
        assert_eq!(
            choose(20.0, at(13.0, 2), None, None),
            Choice::Cantilever(-1)
        );
        assert_eq!(
            choose(20.0, at(13.1, 2), at(9.0, 2), None),
            Choice::Cantilever(1)
        );
        assert_eq!(
            choose(20.0, at(5.0, 2), at(4.0, 2), None),
            Choice::Cantilever(1)
        );
        assert_eq!(
            choose(20.1, at(5.0, 2), at(5.0, 2), at(10.0, 3)),
            Choice::Gantry
        );
        assert_eq!(choose(20.0, at(5.0, 3), None, at(27.5, 5)), Choice::Gantry);
        assert_eq!(choose(55.0, None, None, at(27.5, 5)), Choice::Gantry);
        assert_eq!(choose(55.1, None, None, at(10.0, 2)), Choice::SpaceFrame);
        assert_eq!(choose(10.0, None, None, at(27.6, 5)), Choice::SpaceFrame);
        assert_eq!(choose(10.0, None, None, at(20.0, 6)), Choice::SpaceFrame);
        assert_eq!(choose(10.0, at(14.0, 1), None, None), Choice::Arm);
        assert_eq!(choose(10.0, None, None, None), Choice::Arm);
    }

    #[test]
    fn wider_roads_and_bigger_signs_get_bigger_structures() {
        let cases = [
            (2, sign(1, -3.5, 2.0, 1.0, "+"), Kind::Cantilever),
            (4, sign(1, -7.0, 3.0, 1.5, "+"), Kind::Gantry),
            (6, sign(1, -10.5, 3.0, 1.5, "+"), Kind::SpaceFrame),
            (2, sign(1, -3.5, 5.0, 5.0, "+"), Kind::Gantry),
            (2, sign(1, -3.5, 8.0, 7.0, "+"), Kind::SpaceFrame),
            (2, sign(1, -9.0, 0.6, 0.6, "+"), Kind::Pole),
        ];
        for (lanes, signal, want) in cases {
            assert_eq!(
                kinds(&road(lanes, &signal)),
                [want],
                "{lanes} lanes, {signal}"
            );
        }
    }

    #[test]
    fn signs_back_to_back_share_a_structure() {
        for (t, kind) in [(-9.0, Kind::Pole), (-3.5, Kind::Cantilever)] {
            let pair = sign(1, t, 2.0, 1.0, "+") + &sign(2, t, 2.0, 1.0, "-");
            let map = road(2, &pair);
            let (_, _, placements, structures, _) = export(&map);
            assert_eq!(structures.len(), 1, "t {t}");
            assert_eq!(structures[0].kind, kind);
            for p in &placements {
                assert!(matches!(p.support, Support::Added(0)));
                assert!(
                    (p.front - GAP).abs() < 1e-4,
                    "each board steps {} m",
                    p.front
                );
            }
            clears_its_boards(&map);
        }
    }

    #[test]
    fn signs_across_the_road_share_a_gantry() {
        let row = sign(1, -2.0, 2.5, 1.5, "+") + &sign(2, -8.0, 2.5, 1.5, "+");
        let (_, _, placements, structures, _) = export(&road(3, &row));
        assert_eq!(structures.len(), 1);
        assert_eq!(structures[0].kind, Kind::Gantry);
        assert!(placements
            .iter()
            .all(|p| matches!(p.support, Support::Added(0))));
    }

    #[test]
    fn a_cantilever_runs_behind_its_board_at_its_middle() {
        let (net, _, placements, structures, _) = export(&fixture("signals"));
        for name in ["Gantry", "SideLight"] {
            let (signal, placement) = net
                .signals()
                .iter()
                .zip(&placements)
                .find(|(s, _)| s.name == name)
                .expect("the signal");
            let Support::Added(k) = placement.support else {
                panic!("{name} has an added structure")
            };
            let structure = &structures[k];
            assert_eq!(structure.kind, Kind::Cantilever, "{name}");
            let arm = &structure.members[0].0;
            let middle = signal.position.z + signal.height.expect("a height") / 2.0;
            assert!(
                (arm[arm.len() - 1].z - middle).abs() < 1e-4,
                "{name}'s arm is level"
            );
        }
    }

    #[test]
    fn no_structure_stands_in_traffic_or_goes_through_a_board() {
        let maps = [
            fixture("signals"),
            fixture("lane_heights"),
            fixture("signal_semantics"),
            fixture("traffic_rule"),
            fixture("signals").replace(r#"name="SideLight""#, r#"name="SideLight" length="0.6""#),
            road(
                4,
                &(sign(1, -7.0, 3.0, 1.5, "+") + &sign(2, -7.0, 3.0, 1.5, "-")),
            ),
            road(
                6,
                &(sign(1, -10.5, 3.0, 1.5, "+") + &sign(2, -10.5, 3.0, 1.5, "-")),
            ),
            road(
                4,
                &(sign(1, -7.0, 3.0, 1.5, "+") + &low(sign(2, -7.0, 3.0, 1.5, "-"))),
            ),
        ];
        for map in &maps {
            let (_, _, _, structures, traffic) = export(map);
            for foot in structures.iter().flat_map(|s| &s.feet) {
                assert!(traffic.height_at(foot.x, foot.y).is_none(), "{foot:?}");
            }
            clears_its_boards(map);
        }
    }

    /// Check that no member of a structure the exporter adds to `map` goes
    /// through a board it holds, measured in each signal's own frame.
    fn clears_its_boards(map: &str) {
        let (net, provenance, placements, structures, _) = export(map);
        for (signal, placement) in net.signals().iter().zip(&placements) {
            let Support::Added(k) = placement.support else {
                continue;
            };
            let both = two_faced(&provenance, signal);
            let depth = signal.length.unwrap_or(0.0) / 2.0;
            let (width, height) = (
                signal.width.unwrap_or(FALLBACK_SIZE),
                signal.height.unwrap_or(FALLBACK_SIZE),
            );
            for (line, radius) in &structures[k].members {
                for w in line.windows(2) {
                    let steps = ((w[1] - w[0]).length() / 0.05).ceil().max(1.0) as usize;
                    for i in 0..=steps {
                        let p = w[0] + (w[1] - w[0]) * (i as f32 / steps as f32);
                        let [x, y, z] = unrotate(signal, p - signal.position);
                        let beside = y.abs() > width / 2.0 + radius;
                        let off = z > height + radius || z < -radius;
                        let front = x + radius <= placement.front - depth + 1e-3;
                        let back = !both || x - radius >= -placement.back + depth - 1e-3;
                        assert!(
                            beside || off || (front && back),
                            "{} at {p:?}, {x} m along",
                            signal.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn two_way_boards_never_overlap_whatever_holds_them() {
        let two_way = sign(1, -9.0, 0.6, 0.6, "none").replace("/>", r#" length="0.3"/>"#);
        let pole = r#"<objects><object id="9" type="pole" s="50" t="-9" zOffset="0" radius="0.05" height="5"/></objects>"#;
        let maps = [
            road(2, &two_way),
            road(2, &two_way).replace("<signals>", &format!("{pole}<signals>")),
            road(2, &two_way.replace(r#"zOffset="5""#, r#"zOffset="0""#)),
        ];
        for map in &maps {
            let (net, _, placements, _, _) = export(map);
            let length = net.signals()[0].length.expect("a length");
            let p = &placements[0];
            assert!(
                p.front + p.back >= length - 1e-4,
                "{} and {} overlap",
                p.front,
                p.back
            );
        }
    }

    #[test]
    fn signs_back_to_back_count_their_area_once() {
        let pair = sign(1, -3.5, 4.0, 3.0, "+") + &sign(2, -3.5, 4.0, 3.0, "-");
        let two_way = sign(1, -3.5, 4.0, 3.0, "none");
        assert_eq!(kinds(&road(2, &pair)), [Kind::Cantilever]);
        assert_eq!(kinds(&road(2, &two_way)), [Kind::Cantilever]);
    }
}
