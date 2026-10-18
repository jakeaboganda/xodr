//! The area of a junction: the boundary around it, and the elevation grid
//! over it. Also junction groups, cross paths and virtual junctions.

use crate::coords::{Point, Vector};
use crate::mesh::Mesh;
use crate::network::LaneId;
use crate::road::RoadId;
use crate::structure::Coverage;
use crate::{Orientation, RoadEnd};

/// A virtual junction, from a `<junction type="virtual">`: roads that leave
/// or join a main road part way along it, such as a driveway, without
/// cutting it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualJunction {
    /// Its `id`.
    pub od_id: String,
    /// Its `name`, empty if it has none.
    pub name: String,
    /// The road the others branch off and the stretch of it the junction
    /// spans, or `None` if the file doesn't give them, as the spec's own
    /// example of virtual connections does not.
    pub main: Option<MainRoad>,
    /// Which way along the main road it applies to, from `orientation`.
    pub orientation: Orientation,
    /// Each place in it where a road meets another part way along, in file
    /// order.
    pub links: Vec<VirtualLink>,
}

/// The road a [`VirtualJunction`]'s roads branch off, and the stretch of it
/// the junction spans.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MainRoad {
    /// The road, from `mainRoad`.
    pub road: RoadId,
    /// Where along it the junction starts, from `sStart`.
    pub s_start: f64,
    /// Where it ends, from `sEnd`.
    pub s_end: f64,
}

/// Where a road meets another part way along it, in a [`VirtualJunction`].
/// It runs the way the file links them: from a road's `<predecessor>` into
/// the road, or from the road into its `<successor>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualLink {
    /// Where it comes from.
    pub from: LinkPoint,
    /// Where it leads.
    pub to: LinkPoint,
    /// The lanes it joins, each a `from` lane and its `to` lane. Which way
    /// traffic crosses is the lanes' own direction.
    pub lanes: Vec<(LaneId, LaneId)>,
}

/// One side of a [`VirtualLink`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LinkPoint {
    /// An end of a road.
    End {
        /// The road.
        road: RoadId,
        /// Which end.
        end: RoadEnd,
    },
    /// Part way along a road, from a link's `elementS` and `elementDir`.
    Along {
        /// The road.
        road: RoadId,
        /// How far along its reference line.
        s: f64,
        /// Whether the link runs along the road's `+s` there, from
        /// `elementDir="+"`.
        forward: bool,
    },
}

impl LinkPoint {
    /// The road it is on.
    pub fn road(&self) -> RoadId {
        match *self {
            Self::End { road, .. } | Self::Along { road, .. } => road,
        }
    }
}

/// A crossing, from an OpenDRIVE `<junction type="crossing">`: where two or
/// more roads cross at one level, such as a road and a railway, and traffic
/// cannot change from one to the other. It has no connecting roads, only the
/// stretch of each road where traffic crosses.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Crossing {
    /// Its `<junction id>`.
    pub od_id: String,
    /// Its `name`, empty if it has none.
    pub name: String,
    /// Where each road crosses, from its `<roadSection>`s, in file order.
    pub sections: Vec<CrossingSection>,
}

/// One road's stretch through a [`Crossing`], from a `<roadSection>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrossingSection {
    /// Its `id`, empty if it has none.
    pub od_id: String,
    /// The road.
    pub road: RoadId,
    /// Where the stretch starts along the road's reference line, `sStart`,
    /// in metres. Never past `s_end`.
    pub s_start: f64,
    /// Where it ends, `sEnd`, in metres.
    pub s_end: f64,
    /// The part of each of the road's lanes in the stretch.
    pub lanes: Vec<Coverage>,
}

/// Junctions that routing should see as one, such as the junctions round a
/// roundabout, from an OpenDRIVE `<junctionGroup>`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct JunctionGroup {
    /// Its `id`.
    pub od_id: String,
    /// Its `name`, empty if it has none.
    pub name: String,
    /// What the junctions together are.
    pub kind: JunctionGroupKind,
    /// The `<junction id>` of each junction in it, in file order.
    pub junctions: Vec<String>,
}

/// What a [`JunctionGroup`]'s junctions together are, from its `type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum JunctionGroupKind {
    /// A roundabout.
    Roundabout,
    /// A junction built of several, such as one with slip lanes.
    ComplexJunction,
    /// Where two motorways meet.
    HighwayInterchange,
    /// The file says `unknown`, or names a type the crate doesn't know.
    Unknown,
}

/// A path across a junction's roads for pedestrians, from an OpenDRIVE
/// `<crossPath>`: a crossing road, and the lanes it joins at each end, part
/// way along them.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrossPath {
    /// The road the path runs along.
    pub crossing: RoadId,
    /// Where it joins the lane its crossing road starts from.
    pub start: CrossPathEnd,
    /// Where it joins the lane its crossing road ends at.
    pub end: CrossPathEnd,
}

/// One end of a [`CrossPath`]: the lane of another road it joins, where
/// along that road, and the lane of the crossing road there.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrossPathEnd {
    /// The lane of the road at this end.
    pub lane: LaneId,
    /// How far along that road's reference line the path joins it.
    pub s: f64,
    /// The crossing road's lane at this end.
    pub crossing_lane: LaneId,
}

/// A junction's area, from an OpenDRIVE junction's `<boundary>` and
/// `<elevationGrid>`: the edge of the ground its traffic may use, sidewalks
/// included, and the height of that ground.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct JunctionArea {
    /// The `<junction id>` it came from.
    pub od_id: String,
    /// The boundary, a closed ring running counter-clockwise, on the road
    /// surface. The last point is not the first again. Empty for a junction
    /// with a grid and no boundary.
    pub boundary: Vec<Point>,
    /// The height of the ground over the junction, if the junction gives
    /// one.
    pub grid: Option<ElevationGrid>,
}

/// A junction's `<elevationGrid>`: heights at evenly spaced points along the
/// junction's reference line and square to it, and the surface through them.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ElevationGrid {
    /// Where the junction's reference line starts, and its heading. It is one
    /// straight line.
    pub origin: [f64; 2],
    /// The reference line's heading, in radians.
    pub heading: f64,
    /// The `s` along the reference line of the first row of points.
    pub s_start: f64,
    /// How far apart the points are, along and across the line, in metres.
    pub spacing: f64,
    /// The rows of points, one per `<elevation>`, in order along the line.
    pub rows: Vec<GridRow>,
}

/// One row of an [`ElevationGrid`]: the heights at its points, square to the
/// junction's reference line.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GridRow {
    /// On the reference line.
    pub center: f64,
    /// To its left, from the line outward.
    pub left: Vec<f64>,
    /// To its right, from the line outward.
    pub right: Vec<f64>,
}

impl GridRow {
    /// The height `j` points to the left of the line, or to the right for a
    /// negative `j`, if the row has one there.
    fn at(&self, j: i64) -> Option<f64> {
        match j {
            0 => Some(self.center),
            j if j > 0 => self.left.get(j as usize - 1).copied(),
            j => self.right.get((-j) as usize - 1).copied(),
        }
    }
}

impl ElevationGrid {
    /// The height at `(s, t)` on the junction's reference line, or `None`
    /// outside the squares the grid covers whole, or for a coordinate that
    /// is not finite.
    ///
    /// Inside a square the height is bicubic, as the spec gives it: the
    /// square's corners, and their slopes along and across the line and
    /// their twist, each from the cubic through the four points of the row
    /// or column the square's edge lies on. Where the grid has too few points
    /// for that cubic, the slope is the straight line along the edge.
    pub fn height(&self, s: f64, t: f64) -> Option<f64> {
        if self.spacing.is_nan() || self.spacing <= 0.0 {
            return None;
        }
        let snap = |at: f64| {
            if (at - at.round()).abs() < 1e-6 {
                at.round()
            } else {
                at
            }
        };
        let (u, v) = (
            snap((s - self.s_start) / self.spacing),
            snap(t / self.spacing),
        );
        let last_row = self.rows.len().checked_sub(1)? as f64;
        let widest = self
            .rows
            .iter()
            .map(|row| row.left.len().max(row.right.len()))
            .max()? as f64;
        if !(0.0..=last_row).contains(&u) || !(-widest..=widest).contains(&v) {
            return None;
        }
        // A point on a grid line is on the edge of the squares either side.
        let squares = |at: f64| {
            let i = at.floor() as i64;
            let frac = at - i as f64;
            if frac == 0.0 {
                vec![(i, 0.0), (i - 1, 1.0)]
            } else {
                vec![(i, frac)]
            }
        };
        let (rows, columns) = (squares(u), squares(v));
        rows.iter()
            .flat_map(|&row| columns.iter().map(move |&column| (row, column)))
            .find_map(|((i, x), (j, y))| self.in_square(i, x, j, y))
    }

    /// The bicubic height at `(x, y)` within the square from grid point
    /// `(i, j)`, or `None` if the grid lacks a point it needs.
    fn in_square(&self, i: i64, x: f64, j: i64, y: f64) -> Option<f64> {
        let z = |i: i64, j: i64| self.value(i, j);
        let dt = |i: i64| slopes(|k| z(i, j - 1 + k));
        let [z00, z01, z10, z11] = [z(i, j)?, z(i, j + 1)?, z(i + 1, j)?, z(i + 1, j + 1)?];
        let (t0, t1) = (dt(i)?, dt(i + 1)?);
        let (s0, s1) = (
            slopes(|k| z(i - 1 + k, j))?,
            slopes(|k| z(i - 1 + k, j + 1))?,
        );
        let (st0, st1) = (
            slopes(|k| dt(i - 1 + k).map(|d| d.0))?,
            slopes(|k| dt(i - 1 + k).map(|d| d.1))?,
        );
        let f = [
            [z00, z01, t0.0, t0.1],
            [z10, z11, t1.0, t1.1],
            [s0.0, s1.0, st0.0, st1.0],
            [s0.1, s1.1, st0.1, st1.1],
        ];
        let alpha = mul(&mul(&A, &f), &transpose(&A));
        let powers = |x: f64| [1.0, x, x * x, x * x * x];
        let (px, py) = (powers(x), powers(y));
        Some(
            (0..4)
                .flat_map(|r| (0..4).map(move |c| (r, c)))
                .map(|(r, c)| px[r] * alpha[r][c] * py[c])
                .sum(),
        )
    }

    /// The height at grid point `(i, j)`: row `i`, `j` points to the left
    /// of the line.
    fn value(&self, i: i64, j: i64) -> Option<f64> {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.rows.get(i))
            .and_then(|row| row.at(j))
    }

    /// The height under `(x, y)`, or `None` where the grid does not reach.
    pub(crate) fn height_at(&self, x: f64, y: f64) -> Option<f64> {
        let (s, t) = self.locate(x, y);
        self.height(s, t)
    }

    /// Where `(x, y)` is on the junction's reference line: `s` along it and
    /// `t` to its left.
    fn locate(&self, x: f64, y: f64) -> (f64, f64) {
        let (sin, cos) = self.heading.sin_cos();
        let (dx, dy) = (x - self.origin[0], y - self.origin[1]);
        (dx * cos + dy * sin, dy * cos - dx * sin)
    }
}

/// The matrix the spec's bicubic interpolation turns a square's corners and
/// slopes into its coefficients with.
const A: [[f64; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [-3.0, 3.0, -2.0, -1.0],
    [2.0, -2.0, 1.0, 1.0],
];

fn mul(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}

fn transpose(a: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for (r, row) in a.iter().enumerate() {
        for (c, &cell) in row.iter().enumerate() {
            out[c][r] = cell;
        }
    }
    out
}

/// The slopes at points 1 and 2 of four evenly spaced points 0 to 3, a grid
/// step apart, from the cubic through all four. From the line through 1 and
/// 2 where 0 or 3 is missing, and `None` where 1 or 2 is.
fn slopes(value: impl Fn(i64) -> Option<f64>) -> Option<(f64, f64)> {
    let (b, c) = (value(1)?, value(2)?);
    match (value(0), value(3)) {
        (Some(a), Some(d)) => Some((
            (-2.0 * a - 3.0 * b + 6.0 * c - d) / 6.0,
            (a - 6.0 * b + 3.0 * c + 2.0 * d) / 6.0,
        )),
        _ => Some((c - b, c - b)),
    }
}

impl JunctionArea {
    /// The ground's height under `(x, y)`, from the elevation grid, or
    /// `None` where the junction has no grid or the grid does not reach.
    pub fn height_at(&self, x: f64, y: f64) -> Option<f64> {
        self.grid.as_ref()?.height_at(x, y)
    }

    /// The ground inside the boundary as triangles, split to the grid's
    /// spacing and stood at the grid's height where it reaches, or at the
    /// boundary's height across each triangle where it doesn't. Empty for an
    /// area without a boundary.
    pub fn mesh(&self) -> Mesh {
        let ring = &self.boundary;
        let step = self.grid.as_ref().map_or(f64::INFINITY, |g| g.spacing);
        let mut mesh = Mesh::default();
        for [a, b, c] in ear_clip(ring) {
            let (a, b, c) = (ring[a], ring[b], ring[c]);
            let longest = [a.distance_to(b), b.distance_to(c), c.distance_to(a)]
                .into_iter()
                .fold(0.0_f32, f32::max);
            let n = (f64::from(longest) / step).ceil().clamp(1.0, 64.0) as usize;
            let at = |i: usize, j: usize| {
                let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                let p = a + (b - a) * u + (c - a) * v;
                let z = self.height_at(f64::from(p.x), f64::from(p.y));
                Point::new(p.x, p.y, z.map_or(p.z, |z| z as f32))
            };
            for i in 0..n {
                for j in 0..n - i {
                    push(&mut mesh, [at(i, j), at(i + 1, j), at(i, j + 1)]);
                    if i + j + 1 < n {
                        push(&mut mesh, [at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
                    }
                }
            }
        }
        mesh
    }
}

/// Add one triangle to `mesh`, with its own vertices and its face's normal,
/// turned up.
fn push(mesh: &mut Mesh, corners: [Point; 3]) {
    let normal = (corners[1] - corners[0])
        .cross(corners[2] - corners[0])
        .normalize_or(Vector::Z);
    let normal = if normal.z < 0.0 {
        normal * -1.0
    } else {
        normal
    };
    let first = mesh.vertices.len() as u32;
    mesh.vertices.extend(corners);
    mesh.normals.extend([normal; 3]);
    mesh.indices.extend([first, first + 1, first + 2]);
}

/// The triangles of a simple counter-clockwise polygon in plan, as indices
/// into `ring`, by clipping its ears.
fn ear_clip(ring: &[Point]) -> Vec<[usize; 3]> {
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let inside = |p: Point, [a, b, c]: [Point; 3]| {
        cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
    };
    let mut left: Vec<usize> = (0..ring.len()).collect();
    let mut out = Vec::new();
    while left.len() > 3 {
        let n = left.len();
        let ear = (0..n).find(|&k| {
            let (a, b, c) = (left[(k + n - 1) % n], left[k], left[(k + 1) % n]);
            let corners = [ring[a], ring[b], ring[c]];
            cross(ring[a], ring[b], ring[c]) > 0.0
                && !left
                    .iter()
                    .filter(|&&p| p != a && p != b && p != c)
                    .any(|&p| inside(ring[p], corners))
        });
        let Some(k) = ear else {
            break;
        };
        out.push([left[(k + n - 1) % n], left[k], left[(k + 1) % n]]);
        left.remove(k);
    }
    if left.len() == 3 && cross(ring[left[0]], ring[left[1]], ring[left[2]]) > 0.0 {
        out.push([left[0], left[1], left[2]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: Vec<GridRow>) -> ElevationGrid {
        ElevationGrid {
            origin: [0.0, 0.0],
            heading: 0.0,
            s_start: 0.0,
            spacing: 2.0,
            rows,
        }
    }

    fn row(center: f64, left: &[f64], right: &[f64]) -> GridRow {
        GridRow {
            center,
            left: left.to_vec(),
            right: right.to_vec(),
        }
    }

    #[test]
    fn a_flat_grid_is_flat_and_ends_at_its_last_whole_square() {
        let g = grid(vec![row(5.0, &[5.0], &[5.0]); 3]);
        for (s, t) in [(0.0, 0.0), (1.3, -1.7), (3.9, 1.9), (4.0, 2.0)] {
            assert!((g.height(s, t).unwrap() - 5.0).abs() < 1e-12, "({s}, {t})");
        }
        assert!(g.height(4.1, 0.0).is_none());
        assert!(g.height(1.0, 2.1).is_none());
        assert!(g.height(-0.1, 0.0).is_none());
    }

    #[test]
    fn a_coordinate_far_off_or_not_finite_has_no_height() {
        let g = grid(vec![row(5.0, &[5.0], &[5.0]); 3]);
        for (s, t) in [
            (f64::NAN, 0.0),
            (0.0, f64::NAN),
            (f64::INFINITY, 0.0),
            (0.0, f64::NEG_INFINITY),
            (1e30, 0.0),
            (2.0, -1e30),
        ] {
            assert!(g.height(s, t).is_none(), "({s}, {t})");
        }
        let fine = ElevationGrid {
            spacing: 1e-300,
            ..g.clone()
        };
        assert!(fine.height(1.0, 0.0).is_none());
        assert!(grid(Vec::new()).height(0.0, 0.0).is_none());
    }

    #[test]
    fn a_grid_passes_through_its_points_and_is_exact_on_a_plane() {
        let plane = |s: f64, t: f64| 1.0 + 0.1 * s - 0.05 * t;
        let rows = (0..5)
            .map(|i| {
                let s = 2.0 * i as f64;
                row(
                    plane(s, 0.0),
                    &[plane(s, 2.0), plane(s, 4.0)],
                    &[plane(s, -2.0), plane(s, -4.0)],
                )
            })
            .collect();
        let g = grid(rows);
        for (s, t) in [(2.0, 2.0), (3.1, -0.7), (5.5, 3.3), (7.9, -3.9)] {
            assert!(
                (g.height(s, t).unwrap() - plane(s, t)).abs() < 1e-9,
                "({s}, {t})"
            );
        }
    }

    #[test]
    fn a_grid_follows_a_cubic_along_the_line_between_its_points() {
        // Heights on a cubic along s: the four-point cubic slopes make the
        // interpolation exact inside the middle square.
        let f = |s: f64| 0.01 * s * s * s - 0.2 * s + 1.0;
        let rows = (0..4).map(|i| {
            let z = f(2.0 * i as f64);
            row(z, &[z], &[z])
        });
        let g = grid(rows.collect());
        for s in [2.0, 2.5, 3.0, 3.7] {
            assert!((g.height(s, 0.5).unwrap() - f(s)).abs() < 1e-9, "{s}");
        }
    }

    #[test]
    fn a_square_ring_clips_to_two_triangles_and_meshes_at_the_grid_spacing() {
        let ring = vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(4.0, 0.0, 0.0),
            Point::new(4.0, 4.0, 0.0),
            Point::new(0.0, 4.0, 0.0),
        ];
        assert_eq!(ear_clip(&ring).len(), 2);
        let mut area = JunctionArea {
            od_id: "1".into(),
            boundary: ring.clone(),
            grid: None,
        };
        assert_eq!(area.mesh().indices.len(), 6);
        area.grid = Some(ElevationGrid {
            origin: [0.0, 0.0],
            heading: 0.0,
            s_start: 0.0,
            spacing: 2.0,
            rows: vec![row(3.0, &[3.0, 3.0], &[]); 3],
        });
        let mesh = area.mesh();
        mesh.validate().unwrap();
        assert!(mesh.indices.len() > 6);
        assert!(mesh.vertices.iter().all(|v| (v.z - 3.0).abs() < 1e-6));
        assert!(mesh.normals.iter().all(|n| n.z > 0.99));
    }

    #[test]
    fn a_concave_ring_clips_without_covering_its_notch() {
        let ring = vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(4.0, 0.0, 0.0),
            Point::new(4.0, 4.0, 0.0),
            Point::new(2.0, 1.0, 0.0),
            Point::new(0.0, 4.0, 0.0),
        ];
        let triangles = ear_clip(&ring);
        assert_eq!(triangles.len(), 3);
        let area: f32 = triangles
            .iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (ring[a], ring[b], ring[c]);
                ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) / 2.0
            })
            .sum();
        assert!((area - 10.0).abs() < 1e-5, "{area}");
    }
}
