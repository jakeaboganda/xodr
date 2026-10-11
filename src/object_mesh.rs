//! Tessellating objects into triangles.

use std::ops::Range;

use crate::coords::{Point, Vector};
use crate::mesh::{Mesh, MIN_AREA};
use crate::network::RoadNetwork;
use crate::object::{orient, Corner, Extent, ObjectId, Section, Shape, SkeletonVertex};

/// Sides on a tessellated cylinder.
const CYLINDER_SEGMENTS: usize = 16;

/// The slice of a [`Mesh`] belonging to one object: a half-open range into
/// `vertices` (and, in step, `normals`) and one into `indices`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectSpan {
    /// The object this slice was tessellated from.
    pub object: ObjectId,
    /// Its range in `Mesh::vertices` and `Mesh::normals`.
    pub vertices: Range<u32>,
    /// Its range in `Mesh::indices`.
    pub indices: Range<u32>,
}

impl RoadNetwork {
    /// Tessellate every object into one mesh. [`Mesh::objects`] names the
    /// object behind each slice.
    ///
    /// Every face has its own vertices, carrying that face's normal, so edges
    /// stay sharp. Faces are wound to face outward: away from a solid's
    /// middle, a lid away from the floor, and the floor away from the lid. An
    /// open outline has no inside, so its walls keep the order of its
    /// corners.
    ///
    /// - A box is six faces and a cylinder is a prism of 16 sides.
    /// - An outline is a wall along each edge, from the corners' bases to their
    ///   tops. A closed one also has a lid over its tops and, if it has any
    ///   height, a floor under its bases. Each hole is cut out of both and
    ///   walled facing into it.
    /// - A sweep is a wall up each side, a top, a floor and a cap at each end.
    ///   A round one is a 16-sided tube with a cap at each end.
    /// - A skeleton is a 16-sided frustum along each segment of each
    ///   polyline, with a cap at each end of the polyline.
    ///
    /// A face with no area is left out, so an object with no volume has no
    /// span: a solid with no extent, or a post given a height and no
    /// footprint. Where a shape is flat, its two faces coincide and only one
    /// is kept: the lid of anything with no height, the +axis face of a box
    /// with one zero dimension, and the left wall of a sweep with no width.
    /// Draw those double-sided.
    ///
    /// A lid is triangulated in the plane of its corners, so a closed outline
    /// must not cross itself, and its lid is flat only if its tops lie in one
    /// plane. One standing on its edge gets a lid too.
    pub fn object_mesh(&self) -> Mesh {
        let mut mesh = Mesh::default();
        for object in self.objects() {
            let vertices = mesh.vertices.len() as u32;
            let indices = mesh.indices.len() as u32;
            match &object.shape {
                Shape::Solid {
                    position,
                    heading,
                    pitch,
                    roll,
                    extent: Some(extent),
                } => {
                    let frame = |local: [f32; 3]| {
                        let [u, v, z] = orient(
                            f64::from(*heading),
                            f64::from(*pitch),
                            f64::from(*roll),
                            local.map(f64::from),
                        );
                        Vector::new(u as f32, v as f32, z as f32)
                    };
                    match *extent {
                        Extent::Box {
                            length,
                            width,
                            height,
                        } => cuboid(&mut mesh, *position, frame, [length, width, height]),
                        Extent::Cylinder { radius, height } => {
                            cylinder(&mut mesh, *position, frame, radius, height)
                        }
                    }
                }
                Shape::Solid { extent: None, .. } => {}
                Shape::Outline {
                    corners,
                    closed,
                    holes,
                } => outline(&mut mesh, corners, *closed, holes),
                Shape::Sweep {
                    sections,
                    round: false,
                } => sweep(&mut mesh, sections),
                Shape::Sweep {
                    sections,
                    round: true,
                } => round_sweep(&mut mesh, sections),
                Shape::Skeleton { lines } => {
                    for line in lines {
                        skeleton(&mut mesh, line);
                    }
                }
            }
            if mesh.indices.len() as u32 > indices {
                mesh.objects.push(ObjectSpan {
                    object: object.id,
                    vertices: vertices..mesh.vertices.len() as u32,
                    indices: indices..mesh.indices.len() as u32,
                });
            }
        }
        mesh
    }
}

/// A box centred on `origin` in plan and rising from it. `frame` turns an
/// offset in the box's own frame into the network's.
fn cuboid(mesh: &mut Mesh, origin: Point, frame: impl Fn([f32; 3]) -> Vector, size: [f32; 3]) {
    // `unit` runs 0..1 along each axis, centred across and rising up.
    let at = |unit: [f32; 3]| {
        origin
            + frame([
                (unit[0] - 0.5) * size[0],
                (unit[1] - 0.5) * size[1],
                unit[2] * size[2],
            ])
    };
    for axis in 0..3 {
        let (b, c) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut direction = [0.0; 3];
        direction[axis] = 1.0;
        let out = frame(direction);
        // The + face, then the - face unless the box has no depth here and
        // it lies on the + one.
        for (side, out) in [(1.0, out), (0.0, -out)] {
            if side == 0.0 && size[axis] == 0.0 {
                continue;
            }
            let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|[p, q]| {
                let mut unit = [0.0; 3];
                (unit[axis], unit[b], unit[c]) = (side, p, q);
                at(unit)
            });
            face(mesh, &corners, &fan(4), out);
        }
    }
}

/// An upright cylinder centred on `origin` and rising from it.
fn cylinder(
    mesh: &mut Mesh,
    origin: Point,
    frame: impl Fn([f32; 3]) -> Vector,
    radius: f32,
    height: f32,
) {
    let angle = |k: usize| k as f32 * std::f32::consts::TAU / CYLINDER_SEGMENTS as f32;
    let ring = |z: f32| -> Vec<Point> {
        (0..CYLINDER_SEGMENTS)
            .map(|k| {
                let (sin, cos) = angle(k).sin_cos();
                origin + frame([radius * cos, radius * sin, z])
            })
            .collect()
    };
    let (bottom, top) = (ring(0.0), ring(height));
    for k in 0..CYLINDER_SEGMENTS {
        let j = (k + 1) % CYLINDER_SEGMENTS;
        let (sin, cos) = (angle(k) + angle(1) / 2.0).sin_cos();
        let out = frame([cos, sin, 0.0]);
        face(mesh, &[bottom[k], bottom[j], top[j], top[k]], &fan(4), out);
    }
    let up = frame([0.0, 0.0, 1.0]);
    face(mesh, &top, &fan(CYLINDER_SEGMENTS), up);
    if height > 0.0 {
        face(mesh, &bottom, &fan(CYLINDER_SEGMENTS), -up);
    }
}

fn outline(mesh: &mut Mesh, corners: &[Corner], closed: bool, holes: &[Vec<Corner>]) {
    let lid = lid_normal(corners);
    walls(mesh, corners, closed.then(|| about(corners, lid)));
    for hole in holes {
        walls(mesh, hole, Some(-about(hole, lid)));
    }
    if closed {
        let mut starts = Vec::new();
        let mut ring = corners.to_vec();
        for hole in holes {
            starts.push(ring.len());
            ring.extend_from_slice(hole);
        }
        let bases: Vec<Point> = ring.iter().map(|c| c.base).collect();
        let tops: Vec<Point> = ring.iter().map(|c| c.top).collect();
        // Flat in the lid's own plane, measured from its first corner.
        let e1 = if lid.z.abs() > 0.5 {
            Vector::X
        } else {
            Vector::Z
        };
        let e1 = (e1 - lid * e1.dot(lid)).normalize_or_zero();
        let e2 = lid.cross(e1);
        let flat: Vec<[f32; 2]> = tops
            .iter()
            .map(|&p| [(p - tops[0]).dot(e1), (p - tops[0]).dot(e2)])
            .collect();
        let triangles = triangulate(&flat, &starts);
        face(mesh, &tops, &triangles, lid);
        if bases != tops {
            face(mesh, &bases, &triangles, -lid);
        }
    }
}

/// Which way a closed outline's lid faces: square to its tops, away from its
/// bases, or up as far as it can where they coincide. Up for an outline lying
/// flat.
fn lid_normal(corners: &[Corner]) -> Vector {
    let tops: Vec<Point> = corners.iter().map(|c| c.top).collect();
    let normal = area_normal(&tops).normalize_or(Vector::Z);
    let rise = corners
        .iter()
        .fold(Vector::ZERO, |sum, c| sum + (c.top - c.base));
    let away = normal.dot(rise);
    if away.abs() > MIN_AREA {
        normal * away.signum()
    } else if normal.z < 0.0 {
        -normal
    } else {
        normal
    }
}

/// `lid`, or its reverse, whichever `ring` runs anticlockwise about.
fn about(ring: &[Corner], lid: Vector) -> Vector {
    let bases: Vec<Point> = ring.iter().map(|c| c.base).collect();
    lid * area_normal(&bases).dot(lid).signum()
}

/// A wall along each edge of `ring`, facing out of it seen from `normal`:
/// out of a ring running anticlockwise about it, into one running
/// clockwise. An open ring, with no `normal`, has no inside, so its walls
/// keep the order of its corners.
fn walls(mesh: &mut Mesh, ring: &[Corner], normal: Option<Vector>) {
    let n = ring.len();
    let bases: Vec<Point> = ring.iter().map(|c| c.base).collect();
    let (edges, normal) = match normal {
        Some(normal) => (n, normal),
        None => (n.saturating_sub(1), Vector::ZERO),
    };
    for i in 0..edges {
        let j = (i + 1) % n;
        let out = (bases[j] - bases[i]).cross(normal);
        face(
            mesh,
            &[bases[i], bases[j], ring[j].top, ring[i].top],
            &fan(4),
            out,
        );
    }
}

fn sweep(mesh: &mut Mesh, sections: &[Section]) {
    let thin = |s: &Section| s.left == s.right;
    let flat = |s: &Section| s.left.base == s.left.top && s.right.base == s.right.top;
    for pair in sections.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let along = b.left.base - a.left.base;
        let left = Vector::new(-along.y, along.x, 0.0);
        let (al, ar, bl, br) = (a.left, a.right, b.left, b.right);
        face(mesh, &[al.base, bl.base, bl.top, al.top], &fan(4), left);
        if thin(a) && thin(b) {
            continue;
        }
        face(mesh, &[ar.base, br.base, br.top, ar.top], &fan(4), -left);
        face(mesh, &[al.top, bl.top, br.top, ar.top], &fan(4), Vector::Z);
        if !(flat(a) && flat(b)) {
            face(
                mesh,
                &[al.base, bl.base, br.base, ar.base],
                &fan(4),
                -Vector::Z,
            );
        }
    }
    let base = |s: &Section| s.left.base;
    let Some((start_out, end_out)) = end_directions(sections, base) else {
        return;
    };
    let (first, last) = (&sections[0], &sections[sections.len() - 1]);
    for (s, out) in [(first, start_out), (last, end_out)] {
        face(
            mesh,
            &[s.left.base, s.right.base, s.right.top, s.left.top],
            &fan(4),
            out,
        );
    }
}

/// The directions a sweep's two end caps face: back along its first step at
/// the start, and on along its last step at the end, each from `at`, a point
/// on a section. Each cap faces its own end even where the sweep curves back
/// on itself. `None` for no sections. One section gives zero, which keeps a
/// cap's triangles as given.
fn end_directions<T>(sections: &[T], at: impl Fn(&T) -> Point) -> Option<(Vector, Vector)> {
    let n = sections.len();
    let (first, last) = (sections.first()?, sections.last()?);
    let second = &sections[1.min(n - 1)];
    let before_last = &sections[n.saturating_sub(2)];
    Some((at(first) - at(second), at(last) - at(before_last)))
}

/// A tube through the ellipse inscribed in each section, a prism of
/// [`CYLINDER_SEGMENTS`] sides between consecutive ones, capped at each end.
fn round_sweep(mesh: &mut Mesh, sections: &[Section]) {
    let rings: Vec<(Point, Vec<Point>)> = sections
        .iter()
        .map(|s| {
            let (l, r) = (s.left, s.right);
            let centre = l.base.lerp(r.top, 0.5);
            let across = l.base.lerp(l.top, 0.5) - centre;
            let up = l.top.lerp(r.top, 0.5) - centre;
            let ring = (0..CYLINDER_SEGMENTS)
                .map(|k| {
                    let angle = k as f32 * std::f32::consts::TAU / CYLINDER_SEGMENTS as f32;
                    centre + across * angle.cos() + up * angle.sin()
                })
                .collect();
            (centre, ring)
        })
        .collect();
    for pair in rings.windows(2) {
        let ((ca, a), (cb, b)) = (&pair[0], &pair[1]);
        for k in 0..CYLINDER_SEGMENTS {
            let j = (k + 1) % CYLINDER_SEGMENTS;
            let out = (a[k] - *ca) + (a[j] - *ca) + (b[k] - *cb) + (b[j] - *cb);
            face(mesh, &[a[k], a[j], b[j], b[k]], &fan(4), out);
        }
    }
    let Some((start_out, end_out)) = end_directions(&rings, |(centre, _)| *centre) else {
        return;
    };
    let (ring_first, ring_last) = (&rings[0].1, &rings[rings.len() - 1].1);
    face(mesh, ring_first, &fan(CYLINDER_SEGMENTS), start_out);
    face(mesh, ring_last, &fan(CYLINDER_SEGMENTS), end_out);
}

/// A skeleton polyline: a frustum of [`CYLINDER_SEGMENTS`] sides along each
/// segment, from one vertex's radius to the next's, with its ends square to
/// the segment. The polyline is capped at its two ends. A segment with no
/// length is left out.
fn skeleton(mesh: &mut Mesh, line: &[SkeletonVertex]) {
    let ring = |centre: Point, axis: Vector, radius: f32| -> Vec<Point> {
        let side = if axis.cross(Vector::Z).length() > 1e-3 {
            Vector::Z
        } else {
            Vector::X
        };
        let a = axis.cross(side).normalize_or_zero();
        let b = axis.cross(a);
        (0..CYLINDER_SEGMENTS)
            .map(|k| {
                let angle = k as f32 * std::f32::consts::TAU / CYLINDER_SEGMENTS as f32;
                centre + (a * angle.cos() + b * angle.sin()) * radius
            })
            .collect()
    };
    let segments: Vec<(&SkeletonVertex, &SkeletonVertex, Vector)> = line
        .windows(2)
        .filter_map(|w| {
            let axis = (w[1].position - w[0].position).normalize_or_zero();
            (axis != Vector::ZERO).then_some((&w[0], &w[1], axis))
        })
        .collect();
    for &(p, q, axis) in &segments {
        let (a, b) = (
            ring(p.position, axis, p.radius),
            ring(q.position, axis, q.radius),
        );
        for k in 0..CYLINDER_SEGMENTS {
            let j = (k + 1) % CYLINDER_SEGMENTS;
            let out = (a[k] - p.position) + (a[j] - p.position);
            face(mesh, &[a[k], a[j], b[j], b[k]], &fan(4), out);
        }
    }
    if let (Some(&(first, _, start)), Some(&(_, last, end))) = (segments.first(), segments.last()) {
        let caps = [(first, start, start * -1.0), (last, end, end)];
        for (vertex, axis, out) in caps {
            let cap = ring(vertex.position, axis, vertex.radius);
            face(mesh, &cap, &fan(CYLINDER_SEGMENTS), out);
        }
    }
}

/// The triangles of a convex polygon of `n` corners, fanned from the first.
fn fan(n: usize) -> Vec<[usize; 3]> {
    (1..n.saturating_sub(1)).map(|i| [0, i, i + 1]).collect()
}

/// One flat face: `points`, joined by `triangles`, each turned to face
/// `out`. An `out` of zero keeps the triangles as given. Triangles with no
/// area are dropped, and a face left with none adds nothing.
fn face(mesh: &mut Mesh, points: &[Point], triangles: &[[usize; 3]], out: Vector) {
    let mut kept = Vec::with_capacity(triangles.len());
    let mut sum = Vector::ZERO;
    for &[a, b, c] in triangles {
        let n = (points[b] - points[a]).cross(points[c] - points[a]);
        if n.length() < MIN_AREA {
            continue;
        }
        if n.dot(out) < 0.0 {
            kept.push([a, c, b]);
            sum = sum - n;
        } else {
            kept.push([a, b, c]);
            sum = sum + n;
        }
    }
    if kept.is_empty() {
        return;
    }
    let normal = sum.normalize_or(Vector::Z);
    let first = mesh.vertices.len() as u32;
    mesh.vertices.extend_from_slice(points);
    mesh.normals
        .extend(std::iter::repeat_n(normal, points.len()));
    mesh.indices
        .extend(kept.iter().flatten().map(|&i| first + i as u32));
}

/// Twice the vector area of the polygon through `points` (Newell's method):
/// square to it, facing the side it runs anticlockwise seen from.
fn area_normal(points: &[Point]) -> Vector {
    let n = points.len();
    (0..n).fold(Vector::ZERO, |sum, i| {
        let (a, b) = (points[i] - points[0], points[(i + 1) % n] - points[0]);
        sum + a.cross(b)
    })
}

/// Triangles covering a simple polygon with holes, by ear clipping. `polygon`
/// is the outer ring followed by each hole's, and `holes` is where each hole
/// starts. Each hole is bridged into the outer ring first, so the result is
/// one ring to clip. Wound counter-clockwise whichever way the rings run. A
/// ring with no area is skipped, as is the whole polygon if it is the outer
/// one, and a polygon that crosses itself gets as many as clip cleanly.
fn triangulate(polygon: &[[f32; 2]], holes: &[usize]) -> Vec<[usize; 3]> {
    let turn = |a: usize, b: usize, c: usize| {
        let (a, b, c) = (polygon[a], polygon[b], polygon[c]);
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let n = polygon.len();
    let starts: Vec<usize> = std::iter::once(0).chain(holes.iter().copied()).collect();
    let rings = starts.iter().enumerate().map(|(k, &from)| {
        let to = starts.get(k + 1).copied().unwrap_or(n);
        let ring: Vec<usize> = (from..to).collect();
        let area: f32 = (0..ring.len())
            .map(|i| turn(ring[0], ring[i], ring[(i + 1) % ring.len()]))
            .sum();
        (ring, area)
    });
    let mut ring = Vec::new();
    let mut inner = Vec::new();
    for (k, (mut r, area)) in rings.enumerate() {
        if r.len() < 3 || area.abs() < MIN_AREA {
            if k == 0 {
                return Vec::new();
            }
            continue;
        }
        // The outer ring counter-clockwise, and the holes clockwise.
        if (area < 0.0) == (k == 0) {
            r.reverse();
        }
        if k == 0 {
            ring = r;
        } else {
            inner.push(r);
        }
    }
    // Rightmost hole first, so each bridge runs to a ring with no hole
    // further right left to join it.
    let right = |r: &Vec<usize>| r.iter().map(|&i| polygon[i][0]).fold(f32::MIN, f32::max);
    inner.sort_by(|a, b| right(b).total_cmp(&right(a)));
    for hole in inner {
        bridge(polygon, &mut ring, &hole);
    }
    let mut triangles = Vec::with_capacity(ring.len().saturating_sub(2));
    while ring.len() > 3 {
        let m = ring.len();
        let corner = |i: usize| (ring[(i + m - 1) % m], ring[i], ring[(i + 1) % m]);
        let ear = (0..m).find(|&i| {
            let (a, b, c) = corner(i);
            turn(a, b, c) > 0.0
                && ring.iter().all(|&p| {
                    p == a
                        || p == b
                        || p == c
                        || turn(a, b, p) < 0.0
                        || turn(b, c, p) < 0.0
                        || turn(c, a, p) < 0.0
                })
        });
        if let Some(i) = ear {
            let (a, b, c) = corner(i);
            triangles.push([a, b, c]);
            ring.remove(i);
            continue;
        }
        // No ear: a corner on a straight run can hold the rest up, so drop
        // one. Without one, the polygon crosses itself.
        let straight = (0..m).find(|&i| {
            let (a, b, c) = corner(i);
            turn(a, b, c).abs() < MIN_AREA
        });
        match straight {
            Some(i) => {
                ring.remove(i);
            }
            None => return triangles,
        }
    }
    if turn(ring[0], ring[1], ring[2]) > 0.0 {
        triangles.push([ring[0], ring[1], ring[2]]);
    }
    triangles
}

/// Splice `hole`, a clockwise ring inside the counter-clockwise `ring`, into
/// it along a bridge from the hole's rightmost corner to a corner of `ring`
/// it can see (Eberly, "Triangulation by Ear Clipping"). Both ends of the
/// bridge appear twice in the result.
fn bridge(polygon: &[[f32; 2]], ring: &mut Vec<usize>, hole: &[usize]) {
    let at = |i: usize| polygon[i];
    let turn = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let (j, m) = hole
        .iter()
        .map(|&i| at(i))
        .enumerate()
        .max_by(|a, b| a.1[0].total_cmp(&b.1[0]))
        .expect("a hole has corners");

    // The nearest edge a ray from `m` toward +x meets, where it meets it,
    // and that edge's rightmost end.
    let len = ring.len();
    let mut hit: Option<(f32, usize)> = None;
    for k in 0..len {
        let (a, b) = (at(ring[k]), at(ring[(k + 1) % len]));
        if a[1] == b[1] || (a[1] > m[1]) == (b[1] > m[1]) && a[1] != m[1] && b[1] != m[1] {
            continue;
        }
        let x = a[0] + (m[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
        if x < m[0] || hit.is_some_and(|(best, _)| x >= best) {
            continue;
        }
        let end = if a[0] > b[0] { k } else { (k + 1) % len };
        hit = Some((x, end));
    }
    let Some((x, mut target)) = hit else {
        return;
    };
    // A corner of `ring` inside the triangle from `m` to the ray's hit to
    // `target` would block the bridge. The one at the shallowest angle to
    // the ray is visible.
    let (i, end) = ([x, m[1]], ring[target]);
    let p = at(end);
    let s = turn(m, i, p).signum();
    let inside = |r: [f32; 2]| {
        turn(m, i, r) * s >= 0.0 && turn(i, p, r) * s >= 0.0 && turn(p, m, r) * s >= 0.0
    };
    let mut best = f32::INFINITY;
    for (k, &v) in ring.iter().enumerate() {
        let r = at(v);
        if s == 0.0 || v == end || r[0] < m[0] || !inside(r) {
            continue;
        }
        let slope = (r[1] - m[1]).abs() / (r[0] - m[0]).max(f32::MIN_POSITIVE);
        if slope < best {
            (best, target) = (slope, k);
        }
    }
    // A corner a previous bridge doubled appears twice. Take the copy whose
    // corner `m` sits in.
    let v = ring[target];
    if let Some(k) = (0..len).filter(|&k| ring[k] == v).find(|&k| {
        let (a, b) = (at(ring[(k + len - 1) % len]), at(ring[(k + 1) % len]));
        let (l1, l2) = (turn(a, at(v), m) > 0.0, turn(at(v), b, m) > 0.0);
        if turn(a, at(v), b) >= 0.0 {
            l1 && l2
        } else {
            l1 || l2
        }
    }) {
        target = k;
    }

    let from_m = hole[j..].iter().chain(&hole[..=j]).copied();
    let spliced: Vec<usize> = from_m.chain([ring[target]]).collect();
    ring.splice(target + 1..target + 1, spliced);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(polygon: &[[f32; 2]], triangles: &[[usize; 3]]) -> f32 {
        triangles
            .iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (polygon[a], polygon[b], polygon[c]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2.0
            })
            .sum()
    }

    #[test]
    fn ear_clipping_covers_a_concave_polygon_either_way_round() {
        // An L of area 3, which a fan from its first corner would get wrong.
        let l = [[0., 0.], [2., 0.], [2., 1.], [1., 1.], [1., 2.], [0., 2.]];
        let mut reversed = l;
        reversed.reverse();
        for polygon in [l, reversed] {
            let triangles = triangulate(&polygon, &[]);
            assert_eq!(triangles.len(), 4);
            assert!((area(&polygon, &triangles) - 3.0).abs() < 1e-6);
        }
    }

    #[test]
    fn ear_clipping_steps_over_a_corner_on_a_straight_edge() {
        let polygon = [[0., 0.], [1., 0.], [2., 0.], [2., 1.], [0., 1.]];
        let triangles = triangulate(&polygon, &[]);
        assert!((area(&polygon, &triangles) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_polygon_with_no_area_has_no_triangles() {
        assert!(triangulate(&[[0., 0.], [1., 0.], [2., 0.]], &[]).is_empty());
    }

    /// Rings, each a list of corners, as one polygon and the hole starts.
    fn rings(rings: &[&[[f32; 2]]]) -> (Vec<[f32; 2]>, Vec<usize>) {
        let mut polygon = Vec::new();
        let mut holes = Vec::new();
        for (k, ring) in rings.iter().enumerate() {
            if k > 0 {
                holes.push(polygon.len());
            }
            polygon.extend_from_slice(ring);
        }
        (polygon, holes)
    }

    /// The triangles cover `want` square metres, all wound counter-clockwise,
    /// and none of them covers any of `empty`.
    fn assert_covers(rs: &[&[[f32; 2]]], want: f32, empty: &[[f32; 2]]) {
        let (polygon, holes) = rings(rs);
        let triangles = triangulate(&polygon, &holes);
        assert!((area(&polygon, &triangles) - want).abs() < 1e-4);
        for &[a, b, c] in &triangles {
            assert!(area(&polygon, &[[a, b, c]]) > 0.0);
            for &e in empty {
                let turn = |p: [f32; 2], q: [f32; 2]| {
                    (q[0] - p[0]) * (e[1] - p[1]) - (q[1] - p[1]) * (e[0] - p[0])
                };
                let (a, b, c) = (polygon[a], polygon[b], polygon[c]);
                let within = turn(a, b) > 0.0 && turn(b, c) > 0.0 && turn(c, a) > 0.0;
                assert!(!within, "{e:?} is covered by {:?}", [a, b, c]);
            }
        }
    }

    const SQUARE: [[f32; 2]; 4] = [[0., 0.], [4., 0.], [4., 4.], [0., 4.]];

    #[test]
    fn ear_clipping_leaves_a_hole_empty_whichever_way_it_runs() {
        let hole = [[1., 1.], [3., 1.], [3., 3.], [1., 3.]];
        let mut reversed = hole;
        reversed.reverse();
        for hole in [hole, reversed] {
            assert_covers(&[&SQUARE, &hole], 12.0, &[[2., 2.]]);
        }
    }

    #[test]
    fn ear_clipping_bridges_several_holes() {
        let outer = [[0., 0.], [10., 0.], [10., 10.], [0., 10.]];
        let (a, b) = ([1., 1.], [6., 6.]);
        let square = |[x, y]: [f32; 2]| [[x, y], [x + 2., y], [x + 2., y + 2.], [x, y + 2.]];
        // Side by side, one above the other with their right edges in line,
        // and diagonally apart.
        for other in [[6., 1.], [1., 6.], b] {
            assert_covers(
                &[&outer, &square(a), &square(other)],
                92.0,
                &[[2., 2.], [other[0] + 1., other[1] + 1.]],
            );
        }
    }

    #[test]
    fn a_bridge_goes_round_a_corner_in_the_way() {
        // A notch hangs down from the top edge to (4, 3), inside the triangle
        // the first bridge would cut across.
        let notched = [
            [0., 0.],
            [10., 0.],
            [10., 10.],
            [5., 10.],
            [4., 3.],
            [3., 10.],
            [0., 10.],
        ];
        let hole = [[1., 1.5], [2., 1.5], [2., 2.5], [1., 2.5]];
        assert_covers(&[&notched, &hole], 100.0 - 7.0 - 1.0, &[[1.5, 2.]]);
    }

    #[test]
    fn a_hole_with_no_area_is_skipped() {
        assert_covers(&[&SQUARE, &[[1., 1.], [2., 2.], [3., 3.]]], 16.0, &[]);
    }
}
