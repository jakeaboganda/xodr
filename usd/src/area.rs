//! A junction's ground as its `<boundary>` and `<elevationGrid>` give it.
//!
//! In ASAM OpenDRIVE 1.8, a junction's boundary takes in the whole of its
//! ground, gaps between the lanes included, and its elevation grid gives the
//! height of every point inside, with each lane's `<height>` on top.
//!
//! [`lay`] adds the ground inside the boundary to a junction's lane facets,
//! as fill. Where the grid covers the junction, it also sets each lane at
//! its `<height>` alone, since the grid lies under every lane alike and so
//! decides nothing between them. Once those are wrapped, [`raise`] sets the
//! wrap on the grid.

use std::collections::HashMap;

use spade::handles::FixedVertexHandle;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use xodr::{ElevationGrid, JunctionArea, LaneType, Point, Vector};

use crate::wrap::{cell, cells_between, cross, plan, Cell, Face, Facet, Wrap, CELL, SEAM};

/// Triangles to a side of a grid square. The grid's surface is bicubic, so
/// flat triangles stray from it by about a sixteenth of what they would
/// across a whole square: under a centimetre on a hump of 0.3 m.
const PIECES_PER_SQUARE: f64 = 4.0;

/// How many steps [`held`] takes toward the reference line, across one grid
/// square, looking for a point the grid reaches.
const STEPS_IN: usize = 8;

/// The most points [`raise`] adds inside a wrap. A finer grid on a larger
/// junction gets wider pieces.
const MOST_POINTS: f64 = 100_000.0;

type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

/// The facets of a junction with `area`, and whether they are set for
/// [`raise`]: its `lanes`, and the ground inside the boundary as fill. If
/// the junction has a grid that gives a height (see [`held`]) at every
/// lane corner and on the boundary, each lane corner stands at its
/// `<height>` from `lifts` and the fill at 0, to be raised onto the grid.
/// Otherwise the lanes keep their own heights and the fill takes the
/// boundary's.
pub(crate) fn lay(
    area: &JunctionArea,
    lanes: Vec<Facet>,
    lifts: &[[f32; 3]],
) -> (Vec<Facet>, bool) {
    let raised = area.grid.as_ref().is_some_and(|grid| {
        let on_grid = |p: &Point| held(grid, f64::from(p.x), f64::from(p.y)).is_some();
        area.boundary.iter().all(on_grid) && lanes.iter().flat_map(|f| &f.corners).all(on_grid)
    });
    let mut out = if raised {
        (lanes.into_iter().zip(lifts))
            .map(|(facet, lift)| Facet {
                corners: [0, 1, 2]
                    .map(|k| Point::new(facet.corners[k].x, facet.corners[k].y, lift[k])),
                normals: [Vector::Z; 3],
                ..facet
            })
            .collect()
    } else {
        lanes
    };
    out.extend(fill(area, raised));
    (out, raised)
}

/// The ground inside `area`'s boundary as fill, typed as a driving lane,
/// since the boundary takes in the ground meant for traffic: a
/// triangulation of the boundary, at height 0 if `flat`, or at the
/// boundary's height.
fn fill(area: &JunctionArea, flat: bool) -> Vec<Facet> {
    let ring = &area.boundary;
    let mut cdt = Cdt::new();
    let mut at = HashMap::new();
    let handles: Vec<FixedVertexHandle> = (ring.iter())
        .map(|&p| {
            let h = cdt.insert(plan(p)).expect("a finite corner");
            at.entry(h)
                .or_insert(if flat { Point::new(p.x, p.y, 0.0) } else { p });
            h
        })
        .collect();
    for k in 0..ring.len() {
        let (a, b) = (handles[k], handles[(k + 1) % ring.len()]);
        if a != b && cdt.can_add_constraint(a, b) {
            cdt.add_constraint(a, b);
        }
    }
    let faces = cdt.inner_faces().filter_map(|face| {
        let corners = face.vertices().map(|v| at[&v.fix()]);
        let middle = corners
            .iter()
            .fold((0.0, 0.0), |m, p| (m.0 + p.x / 3.0, m.1 + p.y / 3.0));
        within(ring, middle.0, middle.1).then_some(corners)
    });
    faces
        .map(|corners| Facet {
            corners,
            normals: [Vector::Z; 3],
            kind: LaneType::Driving,
            fill: true,
        })
        .collect()
}

/// `wrap`, laid with each lane at its `<height>` alone, cut into pieces
/// [`PIECES_PER_SQUARE`] to a grid square and set on `area`'s grid: each
/// point at the grid's height plus the wrap's. Every edge of `wrap` stays an
/// edge, so its steps stay steps.
pub(crate) fn raise(area: &JunctionArea, wrap: &Wrap) -> Wrap {
    let grid = area
        .grid
        .as_ref()
        .expect("a junction set for raising has a grid");
    let (low, high) = bounds(&wrap.vertices);
    let reach = (high.x - low.x) * (high.y - low.y);
    let step = (grid.spacing / PIECES_PER_SQUARE).max((reach / MOST_POINTS).sqrt());
    let corners = |face: &Face| face.corners.map(|k| wrap.vertices[k as usize]);
    let mut faces: HashMap<Cell, Vec<usize>> = HashMap::new();
    for (k, face) in wrap.faces.iter().enumerate() {
        let t = corners(face).map(plan);
        let low = Point2::new(
            t.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
            t.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        );
        let high = Point2::new(
            t.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
            t.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
        );
        for c in cells_between(low, high, CELL) {
            faces.entry(c).or_default().push(k);
        }
    }
    let under = |p: Point2<f64>| {
        let near = faces.get(&cell(p, CELL)).into_iter().flatten();
        near.copied().find(|&k| {
            weights(corners(&wrap.faces[k]).map(plan), p)
                .is_some_and(|w| w.iter().all(|&w| w >= -1e-9))
        })
    };

    let mut cdt = Cdt::new();
    let handles: Vec<FixedVertexHandle> = (wrap.vertices.iter())
        .map(|&v| cdt.insert(plan(v)).expect("a finite corner"))
        .collect();
    let columns = ((high.x - low.x) / step) as i64;
    let rows = ((high.y - low.y) / step) as i64;
    for i in 1..=columns {
        for j in 1..=rows {
            let p = Point2::new(low.x + i as f64 * step, low.y + j as f64 * step);
            let clear = under(p).is_some_and(|k| {
                let t = corners(&wrap.faces[k]).map(plan);
                (0..3).all(|e| distance(p, t[e], t[(e + 1) % 3]) > step / 8.0)
            });
            if clear {
                cdt.insert(p).expect("a finite point");
            }
        }
    }
    let mut edges = std::collections::HashSet::new();
    for face in &wrap.faces {
        for k in 0..3 {
            let (a, b) = (
                handles[face.corners[k] as usize],
                handles[face.corners[(k + 1) % 3] as usize],
            );
            if a == b || !edges.insert((a.min(b), a.max(b))) {
                continue;
            }
            let (p, q) = (cdt.vertex(a).position(), cdt.vertex(b).position());
            let n = (p.distance_2(q).sqrt() / step).ceil().max(1.0) as usize;
            let mut chain = vec![a];
            for i in 1..n {
                let t = i as f64 / n as f64;
                let at = Point2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
                chain.push(cdt.insert(at).expect("a finite point"));
            }
            chain.push(b);
            for pair in chain.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if a != b && !cdt.exists_constraint(a, b) && cdt.can_add_constraint(a, b) {
                    cdt.add_constraint(a, b);
                }
            }
        }
    }

    let mut out = Wrap::default();
    let mut made: HashMap<FixedVertexHandle, Vec<u32>> = HashMap::new();
    for face in cdt.inner_faces() {
        let positions = face.vertices().map(|v| v.position());
        let middle = Point2::new(
            (positions[0].x + positions[1].x + positions[2].x) / 3.0,
            (positions[0].y + positions[1].y + positions[2].y) / 3.0,
        );
        let Some(old) = under(middle) else {
            continue;
        };
        let t = corners(&wrap.faces[old]);
        let new = face.vertices().map(|v| {
            let p = v.position();
            let w = weights(t.map(plan), p).expect("a face with area");
            let lift =
                w[0] * f64::from(t[0].z) + w[1] * f64::from(t[1].z) + w[2] * f64::from(t[2].z);
            let ground = held(grid, p.x, p.y).unwrap_or_else(|| {
                let at = |q: Point| held(grid, f64::from(q.x), f64::from(q.y)).unwrap_or(0.0);
                w[0] * at(t[0]) + w[1] * at(t[1]) + w[2] * at(t[2])
            });
            let z = (ground + lift) as f32;
            let made = made.entry(v.fix()).or_default();
            let level = |&k: &u32| f64::from((out.vertices[k as usize].z - z).abs()) <= SEAM;
            if let Some(k) = made.iter().copied().find(level) {
                return k;
            }
            out.vertices.push(Point::new(p.x as f32, p.y as f32, z));
            out.normals.push(Vector::ZERO);
            made.push(out.vertices.len() as u32 - 1);
            out.vertices.len() as u32 - 1
        });
        let [a, b, c] = new.map(|k| out.vertices[k as usize]);
        let normal = (b - a).cross(c - a);
        if normal.z <= 0.0 {
            continue;
        }
        for k in new {
            out.normals[k as usize] = out.normals[k as usize] + normal;
        }
        out.faces.push(Face {
            corners: new,
            facet: wrap.faces[old].facet,
        });
    }
    for n in &mut out.normals {
        *n = n.normalize_or(Vector::Z);
    }
    out.part(&out.faces)
}

/// `grid`'s height at `(x, y)`. Where the grid falls short by no more
/// than a square, as where a map leaves a square short at its edge, its
/// height where it first reaches, along its reference line to its rows and
/// then toward the line, as the crate holds a lane's first and last
/// `<height>` past their ends. `None` further out.
fn held(grid: &ElevationGrid, x: f64, y: f64) -> Option<f64> {
    let (sin, cos) = grid.heading.sin_cos();
    let (dx, dy) = (x - grid.origin[0], y - grid.origin[1]);
    let last = grid.s_start + grid.rows.len().saturating_sub(1) as f64 * grid.spacing;
    let along = dx * cos + dy * sin;
    let s = along.clamp(grid.s_start, last.max(grid.s_start));
    if (s - along).abs() > grid.spacing {
        return None;
    }
    let t = dy * cos - dx * sin;
    let step = t.signum() * grid.spacing.min(t.abs()) / STEPS_IN as f64;
    (0..=STEPS_IN).find_map(|k| grid.height(s, t - k as f64 * step))
}

/// The corners of the box around `points` in plan.
fn bounds(points: &[Point]) -> (Point2<f64>, Point2<f64>) {
    points.iter().fold(
        (
            Point2::new(f64::INFINITY, f64::INFINITY),
            Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(l, h), v| {
            (
                Point2::new(l.x.min(f64::from(v.x)), l.y.min(f64::from(v.y))),
                Point2::new(h.x.max(f64::from(v.x)), h.y.max(f64::from(v.y))),
            )
        },
    )
}

/// The distance from `p` to the segment `a b`.
fn distance(p: Point2<f64>, a: Point2<f64>, b: Point2<f64>) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a.x + t * dx - p.x).hypot(a.y + t * dy - p.y)
}

/// The weights of `p` against the corners of `t` in plan, or `None` for a
/// triangle with no area.
fn weights(t: [Point2<f64>; 3], p: Point2<f64>) -> Option<[f64; 3]> {
    let area = cross(t[0], t[1], t[2]);
    if area.abs() < 1e-12 {
        return None;
    }
    let u = cross(p, t[1], t[2]) / area;
    let v = cross(t[0], p, t[2]) / area;
    Some([u, v, 1.0 - u - v])
}

/// Whether `(x, y)` is inside `ring` in plan, by the crossings of a ray.
fn within(ring: &[Point], x: f32, y: f32) -> bool {
    let mut inside = false;
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        if (a.y > y) != (b.y > y) && x < a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use xodr::GridRow;

    /// A grid along +X from the origin, `rows` rows `spacing` apart, two
    /// points either side of the line, all at `height`.
    fn grid(spacing: f64, rows: usize, height: f64) -> ElevationGrid {
        let row = GridRow {
            center: height,
            left: vec![height; 2],
            right: vec![height; 2],
        };
        ElevationGrid {
            origin: [0.0, 0.0],
            heading: 0.0,
            s_start: 0.0,
            spacing,
            rows: vec![row; rows],
        }
    }

    #[test]
    fn the_grid_holds_its_edge_a_square_out_and_no_further() {
        let grid = grid(5.0, 3, 1.0);
        assert_eq!(held(&grid, 5.0, 3.0), Some(1.0));
        assert_eq!(
            held(&grid, 5.0, 12.0),
            Some(1.0),
            "a square out across the line"
        );
        assert_eq!(held(&grid, 13.0, 0.0), Some(1.0), "a square out along it");
        assert_eq!(held(&grid, 5.0, 25.0), None);
        assert_eq!(held(&grid, 25.0, 0.0), None);
    }

    #[test]
    fn a_fine_grid_under_a_wide_junction_raises_in_wider_pieces() {
        let area = JunctionArea {
            od_id: "1".into(),
            boundary: Vec::new(),
            grid: Some(grid(0.001, 2, 0.0)),
        };
        let corners = [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)];
        let wrap = Wrap {
            vertices: corners
                .iter()
                .map(|&(x, y)| Point::new(x, y, 0.0))
                .collect(),
            normals: vec![Vector::Z; 4],
            faces: vec![
                Face {
                    corners: [0, 1, 2],
                    facet: 0,
                },
                Face {
                    corners: [0, 2, 3],
                    facet: 0,
                },
            ],
        };
        let raised = raise(&area, &wrap);
        assert!(
            raised.vertices.len() as f64 <= 2.0 * MOST_POINTS,
            "{} vertices",
            raised.vertices.len()
        );
        let covered: f64 = (raised.faces.iter())
            .map(|f| {
                let [a, b, c] = f.corners.map(|k| raised.vertices[k as usize]);
                f64::from((b - a).cross(c - a).z) / 2.0
            })
            .sum();
        assert!((covered - 400.0).abs() < 1e-2, "{covered} m²");
    }
}
