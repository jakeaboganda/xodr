//! The geometry of the structures the exporter adds to hold signals up.
//!
//! Every structure is a set of tubes. A cantilever is one bent tube. A span
//! gantry is two legs and a box truss. A space frame is a deeper truss on a
//! tower of two columns at each end.

use std::f32::consts::TAU;
use std::io::{self, Write};

use xodr::{Point, Vector};

use crate::{close, open, write_mesh, MeshPrim, Tag};

/// The radius of a pole or cantilever arm, and of a hanger, in metres.
pub(crate) const RADIUS: f32 = 0.04;

/// Metres between the axis of a pole or cantilever arm and the back of a
/// board it holds.
pub(crate) const GAP: f32 = RADIUS + 0.01;

/// The radius of a gantry's legs, in metres.
const LEG: f32 = 0.15;

/// The radius of a space frame's tower columns, in metres.
const COLUMN: f32 = 0.12;

/// The radius of a truss chord, in metres.
const CHORD: f32 = 0.06;

/// The radius of a truss's lacing, in metres.
const LACE: f32 = 0.03;

/// Metres between a truss's panel points, and between a tower's braces.
const PANEL: f32 = 1.5;

/// Metres between the top of the highest board a fallback arm holds and
/// the middle of the arm.
pub(crate) const ABOVE: f32 = 0.25;

/// The radius of the bend where a pole turns into its arm, in metres.
pub(crate) const BEND: f32 = 1.0;

/// Straight pieces in each bend.
const BEND_STEPS: usize = 8;

/// Sides on each tube.
const SIDES: usize = 12;

/// The kinds of structure the exporter adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A straight pole beside the road.
    Pole,
    /// A pole beside the road with an arm over traffic, behind the boards.
    Cantilever,
    /// Two legs and a box truss across the road, behind the boards.
    Gantry,
    /// A tower at each end and a deep box truss across the road.
    SpaceFrame,
    /// A pole whose arm runs over the boards, where no other structure
    /// has room.
    Arm,
}

impl Kind {
    /// Its `xodr:structure` token.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pole => "pole",
            Self::Cantilever => "cantilever",
            Self::Gantry => "gantry",
            Self::SpaceFrame => "spaceFrame",
            Self::Arm => "arm",
        }
    }

    /// Metres front to back between the columns at each end: a space
    /// frame's tower depth, and 0 for one leg.
    pub(crate) fn footprint(self) -> f32 {
        match self {
            Self::SpaceFrame => Truss::of(self).depth,
            _ => 0.0,
        }
    }

    /// Metres between the structure's center line and the back of a board
    /// it holds.
    pub(crate) fn clearance(self) -> f32 {
        match self {
            Self::Pole | Self::Cantilever | Self::Arm => GAP,
            Self::Gantry | Self::SpaceFrame => Truss::of(self).depth / 2.0 + CHORD + 0.01,
        }
    }
}

/// A box truss's section, in metres.
struct Truss {
    /// Front to back.
    depth: f32,
    /// Bottom to top.
    height: f32,
}

impl Truss {
    fn of(kind: Kind) -> Self {
        match kind {
            Kind::SpaceFrame => Self {
                depth: 1.2,
                height: 1.5,
            },
            _ => Self {
                depth: 0.6,
                height: 0.8,
            },
        }
    }
}

/// A structure's frame: `axis` is the way the boards face, `side` runs
/// along them, and the structure's center line is `line` metres along
/// `axis` from `origin`.
pub(crate) struct Frame {
    pub(crate) origin: Point,
    pub(crate) axis: Vector,
    pub(crate) side: Vector,
    pub(crate) line: f32,
}

impl Frame {
    /// The point `along` metres from the center line, `u` along it, at
    /// height `z`.
    pub(crate) fn at(&self, along: f32, u: f32, z: f32) -> Point {
        let p = self.origin + self.axis * (self.line + along) + self.side * u;
        Point::new(p.x, p.y, z)
    }
}

/// A board a structure holds, in its frame: `u` along the line, which way
/// it faces (`+1` along `axis`, `-1` against), and the height of its
/// middle.
pub(crate) struct Held {
    pub(crate) u: f32,
    pub(crate) faces: f32,
    pub(crate) middle: f32,
}

/// One structure: tubes, and the points where it meets the ground.
pub(crate) struct Structure {
    pub(crate) kind: Kind,
    /// Each tube's center line and radius.
    pub(crate) members: Vec<(Vec<Point>, f32)>,
    pub(crate) feet: Vec<Point>,
}

impl Structure {
    /// A straight pole from `foot` up to `top`.
    pub(crate) fn pole(foot: Point, top: f32) -> Self {
        Self {
            kind: Kind::Pole,
            members: vec![(vec![foot, Point::new(foot.x, foot.y, top)], RADIUS)],
            feet: vec![foot],
        }
    }

    /// A cantilever: a pole at `foot`, which is at `leg` along the line,
    /// rising to `height`, bending, and running along the line to `end`.
    /// Each board below the arm hangs from it.
    pub(crate) fn cantilever(
        frame: &Frame,
        foot: Point,
        leg: f32,
        end: f32,
        height: f32,
        held: &[Held],
    ) -> Self {
        let path = [foot, frame.at(0.0, leg, height), frame.at(0.0, end, height)];
        let mut members = vec![(rounded(&path, &[BEND]), RADIUS)];
        members.extend(hangers(frame, held, 0.0, height));
        Self {
            kind: Kind::Cantilever,
            members,
            feet: vec![foot],
        }
    }

    /// A span gantry or a space frame, as `kind` says, between legs at
    /// `left` and `right`, each `(u, ground)`. Its truss is centered at
    /// `height`.
    pub(crate) fn span(
        kind: Kind,
        frame: &Frame,
        left: (f32, f32),
        right: (f32, f32),
        height: f32,
        held: &[Held],
    ) -> Self {
        let truss = Truss::of(kind);
        let (low, high) = (height - truss.height / 2.0, height + truss.height / 2.0);
        let mut members = lattice(frame, left.0, right.0, low, high, truss.depth);
        let mut feet = vec![];
        for (u, ground) in [left, right] {
            if kind == Kind::SpaceFrame {
                members.extend(tower(frame, u, ground, high, truss.depth));
                feet.extend([-1.0, 1.0].map(|s| frame.at(s * truss.depth / 2.0, u, ground)));
            } else {
                let foot = frame.at(0.0, u, ground);
                members.push((vec![foot, frame.at(0.0, u, high)], LEG));
                feet.push(foot);
            }
        }
        members.extend(hangers(frame, held, truss.depth / 2.0, low));
        Self {
            kind,
            members,
            feet,
        }
    }

    /// The fallback arm: a pole at `foot` rising to `height`, over the
    /// boards, bending and running to the line at `reach`. A bar along the
    /// line covers every board, and each board hangs from it.
    pub(crate) fn arm(frame: &Frame, foot: Point, reach: f32, height: f32, held: &[Held]) -> Self {
        let path = [
            foot,
            Point::new(foot.x, foot.y, height),
            frame.at(0.0, reach, height),
        ];
        let mut members = vec![(rounded(&path, &[BEND]), RADIUS)];
        let (low, high) = held
            .iter()
            .fold((reach, reach), |(lo, hi), h| (lo.min(h.u), hi.max(h.u)));
        if high - low > 1e-3 {
            let bar = vec![frame.at(0.0, low, height), frame.at(0.0, high, height)];
            members.push((bar, RADIUS));
        }
        members.extend(hangers(frame, held, 0.0, height));
        Self {
            kind: Kind::Arm,
            members,
            feet: vec![foot],
        }
    }
}

/// A hanger from `from` down to the middle of each board below it, `face`
/// metres from the center line on the board's side.
fn hangers(frame: &Frame, held: &[Held], face: f32, from: f32) -> Vec<(Vec<Point>, f32)> {
    held.iter()
        .filter(|h| h.middle < from - 1e-3)
        .map(|h| {
            let along = h.faces * face;
            let line = vec![frame.at(along, h.u, from), frame.at(along, h.u, h.middle)];
            (line, if face > 0.0 { LACE } else { RADIUS })
        })
        .collect()
}

/// A box truss from `u0` to `u1` along the line, `depth` deep, between
/// heights `low` and `high`: four chords, and lacing at each panel.
fn lattice(
    frame: &Frame,
    u0: f32,
    u1: f32,
    low: f32,
    high: f32,
    depth: f32,
) -> Vec<(Vec<Point>, f32)> {
    let (back, front) = (-depth / 2.0, depth / 2.0);
    let at = |along, u, z| frame.at(along, u, z);
    let mut members: Vec<(Vec<Point>, f32)> = [back, front]
        .into_iter()
        .flat_map(|a| [low, high].map(|z| (vec![at(a, u0, z), at(a, u1, z)], CHORD)))
        .collect();
    let panels = ((u1 - u0) / PANEL).ceil().max(1.0) as usize;
    let u = |k: usize| u0 + (u1 - u0) * k as f32 / panels as f32;
    for k in 0..=panels {
        for a in [back, front] {
            members.push((vec![at(a, u(k), low), at(a, u(k), high)], LACE));
        }
        for z in [low, high] {
            members.push((vec![at(back, u(k), z), at(front, u(k), z)], LACE));
        }
    }
    for k in 0..panels {
        let (from, to) = if k % 2 == 0 { (low, high) } else { (high, low) };
        for a in [back, front] {
            members.push((vec![at(a, u(k), from), at(a, u(k + 1), to)], LACE));
        }
        members.push((vec![at(back, u(k), high), at(front, u(k + 1), high)], LACE));
    }
    members
}

/// A space frame's tower at `u`: a column at the front and back of the
/// truss from `ground` up to `high`, braced across at each panel.
fn tower(frame: &Frame, u: f32, ground: f32, high: f32, depth: f32) -> Vec<(Vec<Point>, f32)> {
    let (back, front) = (-depth / 2.0, depth / 2.0);
    let at = |along, z| frame.at(along, u, z);
    let mut members = vec![
        (vec![at(back, ground), at(back, high)], COLUMN),
        (vec![at(front, ground), at(front, high)], COLUMN),
    ];
    let levels = ((high - ground) / PANEL).floor().max(1.0) as usize;
    let z = |k: usize| ground + (high - ground) * k as f32 / levels as f32;
    for k in 1..=levels {
        members.push((vec![at(back, z(k)), at(front, z(k))], LACE));
        let (from, to) = if k % 2 == 0 {
            (back, front)
        } else {
            (front, back)
        };
        members.push((vec![at(from, z(k - 1)), at(to, z(k))], LACE));
    }
    members
}

/// One `Mesh` per structure under `/Map/Supports`, named `support_<n>`.
pub(crate) fn write(structures: &[Structure], out: &mut impl Write) -> io::Result<()> {
    open(out, 1, "def Scope", "Supports", &[], &[])?;
    for (k, structure) in structures.iter().enumerate() {
        let (mut points, mut normals, mut indices) = (vec![], vec![], vec![]);
        for (line, radius) in &structure.members {
            let (p, n, i) = tube(line, *radius);
            let first = points.len() as u32;
            indices.extend(i.into_iter().map(|i| i + first));
            points.extend(p);
            normals.extend(n);
        }
        let mesh = MeshPrim {
            name: format!("support_{k}"),
            tags: vec![
                ("synthesized", Tag::Bool(true)),
                ("structure", Tag::Token(structure.kind.as_str())),
                ("feet", Tag::Points(structure.feet.clone())),
            ],
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
pub(crate) fn rounded(path: &[Point], radii: &[f32]) -> Vec<Point> {
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

/// A tube of `radius` along `line`, which lies in one upright plane, and a
/// cap over its last end: points, normals and triangle indices.
fn tube(line: &[Point], radius: f32) -> (Vec<Point>, Vec<Vector>, Vec<u32>) {
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
        let normal = side.cross(along).normalize_or(Vector::Z);
        let side = along.cross(normal);
        (0..SIDES).map(move |k| {
            let (sin, cos) = (TAU * k as f32 / SIDES as f32).sin_cos();
            let out = normal * cos + side * sin;
            (center + out * radius, out)
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_tube_faces_out_along_any_member() {
        let ends = [
            [Point::ORIGIN, Point::new(0.0, 0.0, 3.0)],
            [Point::ORIGIN, Point::new(3.0, 0.0, 0.0)],
            [Point::ORIGIN, Point::new(2.0, 2.0, 1.0)],
        ];
        for line in ends {
            let (points, normals, _) = tube(&line, 0.1);
            for (p, n) in points.iter().zip(&normals).take(2 * SIDES) {
                let axis = (line[1] - line[0]).normalize_or_zero();
                let off = *p - line[0];
                let radial = off - axis * off.dot(axis);
                assert!((radial.length() - 0.1).abs() < 1e-4, "{line:?}");
                assert!(radial.dot(*n) > 0.0, "{line:?} faces in");
            }
        }
    }
}
