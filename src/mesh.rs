use std::ops::Range;

use crate::coords::{Point, Vector};
use crate::geometry::left_normal;
use crate::grid::{Aabb, Grid};
use crate::network::{LaneId, Layer, RoadNetwork};
use crate::object_mesh::ObjectSpan;

/// A triangle mesh (Z-up, meters): per-vertex positions and normals, plus
/// triangle indices. The road surface from [`RoadNetwork::surface_mesh`], or
/// the objects from [`RoadNetwork::object_mesh`], shared by the physics
/// collider (which needs only positions) and a renderer (which needs normals
/// for lighting).
///
/// Deliberately a plain data type with public fields and no engine types in
/// sight: uploading it is a matter of copying three slices.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mesh {
    /// Vertex positions.
    pub vertices: Vec<Point>,
    /// Per-vertex normals, parallel to `vertices`. Up, on the road surface.
    pub normals: Vec<Vector>,
    /// Triangle vertex indices, three per triangle.
    pub indices: Vec<u32>,
    /// Which lane each part of the mesh came from, in emission order.
    ///
    /// [`RoadNetwork::surface_mesh`] merges every lane into one buffer, which
    /// is what a collider and a single draw call want. This is the way back:
    /// it is what lets a renderer pick the lane under the cursor, give one
    /// lane its own material, or name the lane a degenerate triangle came
    /// from. Empty on an object mesh or a mesh built by hand.
    pub lanes: Vec<LaneSpan>,
    /// Which object each part of the mesh came from, in emission order: the
    /// way back from [`RoadNetwork::object_mesh`], as `lanes` is from
    /// [`RoadNetwork::surface_mesh`]. Empty on a surface mesh or a mesh built
    /// by hand.
    pub objects: Vec<ObjectSpan>,
}

/// The slice of a [`Mesh`] belonging to one lane: a half-open range into
/// `vertices` (and, in step, `normals`) and one into `indices`.
///
/// Within `vertices` the layout is fixed: two vertices per rib, left edge
/// first, ribs in centerline order. So the even-numbered vertices of the span
/// are the lane's left boundary and the odd-numbered ones its right boundary,
/// each a polyline in its own right. That is a promise, not an accident of the
/// tessellator, so a renderer can draw lane edges straight off the buffer it
/// already uploaded instead of being handed them a second time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LaneSpan {
    /// The lane this slice was tessellated from.
    pub lane: LaneId,
    /// Its range in `Mesh::vertices` and `Mesh::normals`.
    pub vertices: Range<u32>,
    /// Its range in `Mesh::indices`.
    pub indices: Range<u32>,
}

/// Twice the area, in square metres, below which a triangle has none worth
/// drawing or colliding with. A post given a height and no footprint is all
/// such triangles, and so is a lane where its width is 0.
pub(crate) const MIN_AREA: f32 = 1e-6;

/// How far outside a triangle, as a share of its corners' weights, a point
/// still counts as on it, so a point on an edge shared by two triangles
/// finds one of them through rounding.
const EDGE_SLACK: f32 = 1e-4;

/// Whether the triangle `a b c` has less area than [`MIN_AREA`].
fn no_area(a: Point, b: Point, c: Point) -> bool {
    (b - a).cross(c - a).length() < MIN_AREA
}

/// Why a mesh cannot be turned into a physics trimesh.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum MeshError {
    /// No triangles at all.
    #[error("mesh has no triangles")]
    Empty,
    /// A vertex carries a `NaN` or an infinity.
    #[error("vertex {0} is not finite")]
    NonFiniteVertex(usize),
    /// A triangle names a vertex that does not exist.
    #[error("triangle {0} indexes vertex {1}, past the {2} vertices present")]
    IndexOutOfRange(usize, u32, usize),
    /// A triangle with no area: it repeats a vertex, its corners lie in a
    /// line, or it is the last and has fewer than three corners.
    #[error("triangle {0} has no area")]
    DegenerateTriangle(usize),
}

impl Mesh {
    /// Whether this mesh can back a physics collider.
    ///
    /// Trimesh builders answer the same question, but typically by failing
    /// deep inside scene setup, where there is nothing to do but panic.
    /// Checking here lets an untrusted map be rejected at *load*, with the
    /// file named, which is what a bad `.xodr` deserves.
    pub fn validate(&self) -> Result<(), MeshError> {
        if self.indices.is_empty() {
            return Err(MeshError::Empty);
        }
        if let Some(i) = self.vertices.iter().position(|v| !v.is_finite()) {
            return Err(MeshError::NonFiniteVertex(i));
        }
        for (t, triangle) in self.indices.chunks(3).enumerate() {
            for &index in triangle {
                if index as usize >= self.vertices.len() {
                    return Err(MeshError::IndexOutOfRange(t, index, self.vertices.len()));
                }
            }
            let &[a, b, c] = triangle else {
                return Err(MeshError::DegenerateTriangle(t));
            };
            let at = |i: u32| self.vertices[i as usize];
            if a == b || b == c || a == c || no_area(at(a), at(b), at(c)) {
                return Err(MeshError::DegenerateTriangle(t));
            }
        }
        Ok(())
    }

    /// The surface height (world Z) and up-normal directly under `(x, y)`, by a
    /// vertical ray against the triangles. `None` if no triangle covers the point
    /// (off the road). Where triangles overlap (a bridge over a road) it returns
    /// the highest, the surface you would be standing on.
    ///
    /// Height is the *actual* baked surface, faceted between vertices. That is
    /// what the collider and viewer use, so a body draped against it sits on
    /// the road that is drawn. The normal, though, is barycentric-interpolated from the smooth
    /// per-vertex normals, not the flat triangle normal: a body oriented to the
    /// flat normal snaps as each wheel crosses a facet edge (visible attitude
    /// vibration), while the interpolated normal varies continuously.
    ///
    /// Scans every triangle. For more than a query or two, build a
    /// [`Mesh::sampler`] instead. It answers the same thing off an index.
    pub fn height_at(&self, x: f32, y: f32) -> Option<(f32, Vector)> {
        self.highest(0..self.indices.len() / 3, x, y)
            .map(|(height, normal, _)| (height, normal))
    }

    /// An index over this mesh's triangles, for repeated [`Mesh::height_at`]
    /// queries.
    ///
    /// Building it is linear in the triangle count; each query afterwards
    /// touches only the triangles over the point rather than all of them. Two
    /// or three queries and it has paid for itself, and a per-wheel road
    /// conform on a city map does thousands per tick.
    pub fn sampler(&self) -> MeshSampler<'_> {
        MeshSampler::new(self)
    }

    /// The highest of `triangles` covering `(x, y)`, with its normal and
    /// index. A triangle naming a vertex the mesh lacks covers nothing.
    fn highest(
        &self,
        triangles: impl IntoIterator<Item = usize>,
        x: f32,
        y: f32,
    ) -> Option<(f32, Vector, usize)> {
        let mut best: Option<(f32, Vector, usize)> = None;
        for t in triangles {
            let Some([a, b, c]) = self.corners(t) else {
                continue;
            };
            let tri = &self.indices[t * 3..t * 3 + 3];
            let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            // Barycentric coords of (x,y) in the triangle's XY projection.
            let det = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
            if det.abs() < 1e-9 {
                continue; // edge-on triangle: no XY footprint
            }
            let l1 = ((b.y - c.y) * (x - c.x) + (c.x - b.x) * (y - c.y)) / det;
            let l2 = ((c.y - a.y) * (x - c.x) + (a.x - c.x) * (y - c.y)) / det;
            let l3 = 1.0 - l1 - l2;
            // Reject outside-the-triangle, and non-finite with it. A query far
            // enough out overflows the barycentric arithmetic to infinity, and
            // every comparison against the resulting NaN is false, so the
            // bounds test alone would wave it through and report a NaN height.
            if !(l1 >= -EDGE_SLACK && l2 >= -EDGE_SLACK && l3 >= -EDGE_SLACK) {
                continue;
            }
            let height = l1 * a.z + l2 * b.z + l3 * c.z;
            if best.is_some_and(|(best_height, _, _)| height <= best_height) {
                continue;
            }
            // Smooth (interpolated vertex) normal, so a draped body doesn't snap
            // at facet edges. Fall back to the flat normal if this mesh carries no
            // per-vertex normals.
            let n = if self.normals.len() == self.vertices.len() {
                (l1 * self.normals[ia] + l2 * self.normals[ib] + l3 * self.normals[ic])
                    .normalize_or(Vector::Z)
            } else {
                (b - a).cross(c - a).normalize_or(Vector::Z)
            };
            let n = if n.z < 0.0 { -n } else { n }; // face up regardless of winding
            best = Some((height, n, t));
        }
        best
    }

    /// The corners of triangle `t`, or `None` if it names a vertex the mesh
    /// lacks.
    fn corners(&self, t: usize) -> Option<[Point; 3]> {
        let tri = self.indices.get(t * 3..t * 3 + 3)?;
        let at = |i: u32| self.vertices.get(i as usize).copied();
        Some([at(tri[0])?, at(tri[1])?, at(tri[2])?])
    }
}

/// A mesh plus a ground-plane index over its triangles. Borrows the mesh, so
/// it cannot outlive or diverge from the geometry it describes.
///
/// Answers exactly what [`Mesh::height_at`] answers, including which surface
/// wins where triangles stack.
#[derive(Debug, Clone)]
pub struct MeshSampler<'a> {
    mesh: &'a Mesh,
    grid: Grid,
    /// The triangle behind each item of `grid`. A triangle naming a vertex
    /// the mesh lacks has no item.
    triangles: Vec<usize>,
}

impl<'a> MeshSampler<'a> {
    fn new(mesh: &'a Mesh) -> Self {
        let (bounds, triangles): (Vec<Aabb>, Vec<usize>) = (0..mesh.indices.len() / 3)
            .filter_map(|t| {
                let corners = mesh.corners(t)?;
                let b = Aabb::around(corners.map(|v| (v.x, v.y)))?;
                let slack = 2.0 * EDGE_SLACK * ((b.max_x - b.min_x) + (b.max_y - b.min_y));
                let grown = Aabb {
                    min_x: b.min_x - slack,
                    max_x: b.max_x + slack,
                    min_y: b.min_y - slack,
                    max_y: b.max_y + slack,
                };
                Some((grown, t))
            })
            .unzip();
        Self {
            mesh,
            grid: Grid::build(&bounds),
            triangles,
        }
    }

    /// The mesh this samples.
    pub fn mesh(&self) -> &'a Mesh {
        self.mesh
    }

    /// The surface height (world Z) and up-normal directly under `(x, y)`.
    /// See [`Mesh::height_at`], which this answers identically.
    pub fn height_at(&self, x: f32, y: f32) -> Option<(f32, Vector)> {
        self.top(x, y).map(|(height, normal, _)| (height, normal))
    }

    /// Like [`Self::height_at`], with the lane of the triangle, if the mesh
    /// says.
    pub(crate) fn top(&self, x: f32, y: f32) -> Option<(f32, Vector, Option<LaneId>)> {
        // A triangle covering the point overlaps the cell holding it, so the
        // one cell is the whole candidate set.
        let candidates = self.grid.at(x, y);
        let (height, normal, t) =
            self.mesh
                .highest(candidates.iter().map(|&k| self.triangles[k as usize]), x, y)?;
        let index = (t * 3) as u32;
        let lanes = &self.mesh.lanes;
        let span = lanes.partition_point(|span| span.indices.end <= index);
        let lane = lanes
            .get(span)
            .filter(|span| span.indices.contains(&index))
            .map(|span| span.lane);
        Some((height, normal, lane))
    }
}

impl RoadNetwork {
    /// Tessellate every lane of the permanent layer into one surface mesh,
    /// a quad strip per lane. The temporary layer's lanes lie on the same
    /// surface and are not in it. [`Self::temporary_mesh`] has them.
    /// Sidewalks, medians and shoulders are surfaces a body can stand on and a
    /// renderer must draw, so the mesh covers the whole cross-section, not only
    /// the part traffic uses. [`Mesh::lanes`] names the lane behind each slice,
    /// and its [`LaneType`](crate::LaneType) says what that slice is. Each rib sits +/-width/2 from the centerline along the per-vertex
    /// cross axis and carries the centerline's elevation. On a superelevated
    /// lane the cross axis tilts about the tangent by the local `bank`, so the
    /// outer edge of a banked curve rides above the inner one. Winding is
    /// consistent, so triangles face up.
    ///
    /// Ribs follow the centerline's own per-vertex tangent, which holds the
    /// width steady across vertices and, at a lane's two ends, is the true
    /// curve tangent whenever the importer knew it. That last part is what
    /// makes contiguous lane sections meet flush: both sides of a joint derive
    /// their rib from the same tangent, so neither opens a V-shaped seam.
    /// Two shapes would flip a triangle's winding, and the loop guards both.
    ///
    /// A stub segment, a joint sample landing centimetres past the one before
    /// it, tessellates to a sliver whose normal flips, or leaves a pinched rib
    /// next to a full-width one with no room between them. So a point that sits
    /// far closer to its predecessor than that predecessor sat to its own is
    /// welded away first, and the last survivor snapped onto the true endpoint,
    /// which keeps the lane's length and its join to the next section.
    ///
    /// On a corner tighter than the half-width the inner rib would cross the
    /// next inner vertex and fold the strip into a bowtie. So the inner offset
    /// is clamped, pinching the lane at the apex, while the outer rib keeps its
    /// full half-width and radius.
    pub fn surface_mesh(&self) -> Mesh {
        self.layer_mesh(Layer::Permanent)
    }

    /// The lanes of the temporary layer, tessellated as
    /// [`Self::surface_mesh`] does the permanent ones. They stand at the same
    /// height as the lanes under them, so draw them over the surface with a
    /// depth offset. Empty on a map with no temporary layer.
    pub fn temporary_mesh(&self) -> Mesh {
        self.layer_mesh(Layer::Temporary)
    }

    fn layer_mesh(&self, layer: Layer) -> Mesh {
        // Floor on the stub threshold, in metres: nothing this close together
        // is a real sample, whatever its neighbours look like.
        const WELD_FLOOR: f32 = 0.1;
        // A segment this fraction of the one before it is a stub rather than a
        // sample. Relative, not absolute, because a stub is an odd one out
        // among the samples around it: a section end landing centimetres past
        // the last one. A fixed threshold cannot tell that from a tight curve
        // sampled finely on purpose, and either lets a fold through or flattens
        // the curve.
        const STUB_FRACTION: f32 = 0.25;
        let horizontal = |v: Vector| Vector::new(v.x, v.y, 0.0);
        let mut mesh = Mesh::default();
        for lane in self.lanes().iter().filter(|l| l.layer == layer) {
            let raw = lane.center.points();
            let raw_tangents = lane.center.tangents();
            // Welding selects centerline vertices rather than rewriting them,
            // so every per-vertex attribute comes from one index and none can
            // fall out of step with the others. Mixing two indices does not
            // crash. It puts a rib a fraction of a millimetre out and opens a
            // seam at a section joint.
            let half_at = |i: usize| lane.widths.get(i).copied().unwrap_or(lane.width) * 0.5;
            let mut keep: Vec<usize> = Vec::with_capacity(raw.len());
            // Length of the last segment kept, which sets what counts as a stub
            // next to it.
            let mut last_seg = 0.0_f32;
            for i in 0..raw.len() {
                let step = keep.last().map(|&k| (raw[i] - raw[k]).length());
                let keep_it = match step {
                    None => true,
                    Some(d) => d > (STUB_FRACTION * last_seg).max(WELD_FLOOR),
                };
                if keep_it {
                    last_seg = step.unwrap_or(0.0);
                    keep.push(i);
                }
            }
            // The last survivor stands in for the lane's true endpoint, so it
            // has to be that endpoint. A joint is flush only when both sides
            // agree on position, tangent, bank and width, so a stub's value
            // for any one of them reopens the seam.
            match keep.len() {
                0 => continue,
                1 => {
                    // A lane welded down to a point still keeps a span, its two
                    // endpoints as one thin quad, which cannot fold.
                    if (raw[raw.len() - 1] - raw[0]).length() < 1e-6 {
                        continue; // a zero-length lane has no surface
                    }
                    keep = vec![0, raw.len() - 1];
                }
                _ => *keep.last_mut().expect("non-empty") = raw.len() - 1,
            }
            let points: Vec<Point> = keep.iter().map(|&i| raw[i]).collect();
            let bank: Vec<f32> = keep
                .iter()
                .map(|&i| lane.bank.get(i).copied().unwrap_or(0.0))
                .collect();
            let half: Vec<f32> = keep.iter().map(|&i| half_at(i)).collect();
            let along: Vec<Vector> = keep
                .iter()
                .map(|&i| raw_tangents.get(i).copied().unwrap_or(Vector::ZERO))
                .collect();
            let n = points.len();
            let base = mesh.vertices.len() as u32;
            let first_index = mesh.indices.len() as u32;
            for i in 0..n {
                // Chords either side of this vertex. They still decide the
                // inner-rib pinch below, which is about this welded polyline's
                // corner, not about the curve it was sampled from.
                let inc = if i > 0 {
                    horizontal(points[i] - points[i - 1]).normalize_or(Vector::ZERO)
                } else {
                    Vector::ZERO
                };
                let out = if i + 1 < n {
                    horizontal(points[i + 1] - points[i]).normalize_or(Vector::ZERO)
                } else {
                    Vector::ZERO
                };
                // The centerline's tangent at this vertex: the same bisector at
                // an interior vertex, but the analytical curve tangent at the
                // ends. A zero tangent falls back to the chords.
                let along =
                    horizontal(along[i]).normalize_or((inc + out).normalize_or(Vector::ZERO));
                // The grade, as rise over run between this vertex's neighbours.
                let (a, b) = (points[i.saturating_sub(1)], points[(i + 1).min(n - 1)]);
                let run = horizontal(b - a).length();
                let grade = if run > 1e-6 { (b.z - a.z) / run } else { 0.0 };
                let slope = (along + Vector::Z * grade).normalize_or(along);
                // Cross axis, rolled about the tangent by the local bank.
                // Positive bank raises the left rib. A flat lane leaves it the
                // horizontal left normal, so the mesh is unchanged.
                let lateral = left_normal(along).rotate_about(along, bank[i]);
                // Surface up-normal, from the tangent with its grade. It is +Z
                // on a flat road and tilts with grade and bank.
                let up = slope.cross(lateral).normalize_or(Vector::Z);

                // Offset `m` slides the inner vertex back along each segment by
                // `m * sin(turn/2)`. Cap that at ~half the shorter segment so the
                // two ends stay within it and the ribs never reverse. Endpoints
                // have no corner, so both keep the full half-width.
                let (mut left_mag, mut right_mag) = (half[i], half[i]);
                if i > 0 && i + 1 < n {
                    let seg = (points[i] - points[i - 1])
                        .length()
                        .min((points[i + 1] - points[i]).length());
                    // sin(turn/2), the along-segment eat-in per unit offset.
                    let cos_turn = inc.dot(out).clamp(-1.0, 1.0);
                    let eat = ((1.0 - cos_turn) * 0.5).max(0.0).sqrt();
                    if eat > 1e-4 && seg > 1e-6 {
                        let inner = half[i].min(0.49 * seg / eat);
                        // Pinch the rib the curve turns toward (the inner one).
                        if (out - inc).dot(left_normal(along)) > 0.0 {
                            left_mag = inner;
                        } else {
                            right_mag = inner;
                        }
                    }
                }

                mesh.vertices.push(points[i] + lateral * left_mag);
                mesh.normals.push(up);
                mesh.vertices.push(points[i] - lateral * right_mag);
                mesh.normals.push(up);
            }
            // Two triangles per segment, over the [left, right] rib pairs,
            // less any with no area, where the lane's width is 0.
            for i in 0..n as u32 - 1 {
                let l0 = base + i * 2;
                let (r0, l1, r1) = (l0 + 1, l0 + 2, l0 + 3);
                let at = |i: u32| mesh.vertices[i as usize];
                for [a, b, c] in
                    quad(at(l0), at(r0), at(r1), at(l1)).map(|t| t.map(|k| [l0, r0, r1, l1][k]))
                {
                    if !no_area(at(a), at(b), at(c)) {
                        mesh.indices.extend_from_slice(&[a, b, c]);
                    }
                }
            }
            mesh.lanes.push(LaneSpan {
                lane: lane.id,
                vertices: base..mesh.vertices.len() as u32,
                indices: first_index..mesh.indices.len() as u32,
            });
        }
        mesh
    }
}

/// The two triangles of the quad `[l0, r0, r1, l1]` between two ribs, as
/// corners 0 to 3 of it. Split along `l0`-`r1`, unless a rib pinched at a
/// corner leaves the quad concave there, so that a triangle would face down
/// in plan. Then along `r0`-`l1`, which a quad with one reflex corner always
/// splits cleanly.
fn quad(l0: Point, r0: Point, r1: Point, l1: Point) -> [[usize; 3]; 2] {
    let up =
        |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x) >= 0.0;
    if up(l0, r0, r1) && up(l0, r1, l1) {
        [[0, 1, 2], [0, 2, 3]]
    } else {
        [[0, 1, 3], [1, 2, 3]]
    }
}

#[cfg(test)]
mod tests {
    use crate::coords::{Point, Vector};
    use crate::fixtures::demo_road;
    use crate::{Direction, Lane, LaneId, LaneType, Mesh, Polyline, RoadNetwork};

    #[test]
    fn one_straight_lane_tessellates_to_two_quads() {
        let net = RoadNetwork::new(vec![Lane {
            id: LaneId(0),
            kind: LaneType::Driving,
            layer: crate::Layer::Permanent,
            direction: Direction::Forward,
            center: Polyline::new(vec![
                Point::new(0.0, 0.0, 0.0),
                Point::new(5.0, 0.0, 0.0),
                Point::new(10.0, 0.0, 0.0),
            ]),
            width: 4.0,
            widths: Vec::new(),
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }]);
        let mesh = net.surface_mesh();
        // 3 ribs × 2 vertices; 2 segments × 2 triangles × 3 indices.
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.normals.len(), 6);
        assert_eq!(mesh.indices.len(), 12);
        // The first rib straddles the centerline by +/-half-width in Y.
        assert!((mesh.vertices[0].y - mesh.vertices[1].y).abs() > 3.9);
        // A flat road faces straight up.
        assert!(mesh.normals[0].abs_diff_eq(Vector::Z, 1e-5));
    }

    #[test]
    fn every_shipped_map_tessellates_into_a_valid_trimesh() {
        // The hand-authored road here; imported maps are covered by the
        // integration tests. A consumer builds one static trimesh collider
        // from exactly this mesh, so a degenerate tessellation is a panic at
        // scene setup.
        demo_road()
            .surface_mesh()
            .validate()
            .expect("the built-in demo road tessellates");
    }

    #[test]
    fn validate_rejects_what_a_collider_cannot_build() {
        use crate::MeshError;
        assert_eq!(Mesh::default().validate(), Err(MeshError::Empty));

        let sound = Mesh {
            vertices: vec![
                Point::ORIGIN,
                Point::new(1.0, 0.0, 0.0),
                Point::new(0.0, 1.0, 0.0),
            ],
            normals: vec![Vector::Z; 3],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        assert_eq!(sound.validate(), Ok(()));

        let mut nan = sound.clone();
        nan.vertices[1].x = f32::NAN;
        assert_eq!(nan.validate(), Err(MeshError::NonFiniteVertex(1)));

        let mut past_end = sound.clone();
        past_end.indices = vec![0, 1, 7];
        assert_eq!(
            past_end.validate(),
            Err(MeshError::IndexOutOfRange(0, 7, 3))
        );

        let mut degenerate = sound.clone();
        degenerate.indices = vec![0, 1, 1];
        assert_eq!(degenerate.validate(), Err(MeshError::DegenerateTriangle(0)));
    }

    #[test]
    fn a_banked_lane_tilts_its_ribs() {
        // A straight lane heading +X, canted a constant 0.2 rad: +bank raises
        // the left rib, so the left (first) vertex of each pair rides above the
        // right, and the up-normal leans off vertical.
        let net = RoadNetwork::new(vec![Lane {
            bank: vec![0.2; 3],
            ..Lane {
                id: LaneId(0),
                kind: LaneType::Driving,
                layer: crate::Layer::Permanent,
                direction: Direction::Forward,
                center: Polyline::new(vec![
                    Point::new(0.0, 0.0, 0.0),
                    Point::new(5.0, 0.0, 0.0),
                    Point::new(10.0, 0.0, 0.0),
                ]),
                width: 4.0,
                widths: Vec::new(),
                bank: Vec::new(),
                successors: Vec::new(),
                predecessors: Vec::new(),
                neighbors: Vec::new(),
            }
        }]);
        let mesh = net.surface_mesh();
        mesh.validate().expect("a banked lane is a valid trimesh");
        // Left (index 0) above right (index 1) of the first rib pair.
        assert!(
            mesh.vertices[0].z - mesh.vertices[1].z > 0.5,
            "outer/left {} should ride above inner/right {}",
            mesh.vertices[0].z,
            mesh.vertices[1].z
        );
        // The up-normal is tilted but still points up.
        assert!(mesh.normals[0].z < 0.99 && mesh.normals[0].z > 0.9);
        assert!(
            (mesh.normals[0] - Vector::Z).length() > 0.05,
            "normal should tilt"
        );
    }

    // Build a one-lane network from centerline points and a per-vertex bank.
    fn banked_net(points: Vec<Point>, bank: Vec<f32>, width: f32) -> RoadNetwork {
        RoadNetwork::new(vec![Lane {
            id: LaneId(0),
            kind: LaneType::Driving,
            layer: crate::Layer::Permanent,
            direction: Direction::Forward,
            center: Polyline::new(points),
            width,
            widths: Vec::new(),
            bank,
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }])
    }

    // A CURVED banked lane: a right-hand arc (curving toward -Y), so the left
    // edge is the OUTER edge. With a constant positive bank the outer edge must
    // ride above the inner one all the way round, every normal still points up,
    // and the mesh is a valid trimesh.
    #[test]
    fn a_banked_curve_lifts_the_outer_edge_and_stays_valid() {
        let r = 20.0_f32;
        // Circle centered at (0,-r,0): point = (r sinθ, r cosθ - r, 0). θ=0 -> +X
        // heading, y falls -> a right turn, so left_normal (+Y at start) is outer.
        let points: Vec<Point> = (0..=8)
            .map(|k| {
                let th = (k as f32) * (std::f32::consts::FRAC_PI_4 / 8.0);
                Point::new(r * th.sin(), r * th.cos() - r, 0.0)
            })
            .collect();
        let net = banked_net(points.clone(), vec![0.15; points.len()], 5.0);
        let mesh = net.surface_mesh();
        mesh.validate().expect("a banked curve tessellates");

        // Every normal still points generally up.
        assert!(
            mesh.normals.iter().all(|n| n.z > 0.9),
            "some normal fell below 0.9: {:?}",
            mesh.normals.iter().map(|n| n.z).fold(1.0_f32, f32::min)
        );
        // Outer (left, even index) rib rides above the inner (right, odd) rib at
        // every rib pair, the physically-correct banked-curve profile.
        for i in 0..points.len() {
            let outer = mesh.vertices[2 * i].z;
            let inner = mesh.vertices[2 * i + 1].z;
            assert!(
                outer - inner > 0.5,
                "rib {i}: outer {outer} should ride above inner {inner}"
            );
        }
        // The normals are genuinely tilted, not vertical.
        assert!(mesh.normals.iter().any(|n| (n.z - 1.0).abs() > 0.005));
    }

    // Negative bank rolls the surface the other way: the RIGHT rib rides above
    // the left, and the normal leans toward +Y, the mirror of positive bank.
    #[test]
    fn negative_bank_raises_the_opposite_rib() {
        let pts = vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(5.0, 0.0, 0.0),
            Point::new(10.0, 0.0, 0.0),
        ];
        let neg = banked_net(pts.clone(), vec![-0.2; 3], 4.0).surface_mesh();
        let pos = banked_net(pts, vec![0.2; 3], 4.0).surface_mesh();
        neg.validate().expect("valid trimesh");
        // Right (index 1) above left (index 0) for negative bank.
        assert!(
            neg.vertices[1].z - neg.vertices[0].z > 0.5,
            "negative bank should raise the right rib: left {} right {}",
            neg.vertices[0].z,
            neg.vertices[1].z
        );
        // Exact mirror of the positive-bank mesh.
        assert!((neg.vertices[0].z + pos.vertices[0].z).abs() < 1e-5);
        assert!((neg.vertices[1].z + pos.vertices[1].z).abs() < 1e-5);
        // Normal leans toward +Y (positive bank leans -Y).
        assert!(neg.normals[0].y > 0.05, "neg normal {:?}", neg.normals[0]);
        assert!(pos.normals[0].y < -0.05, "pos normal {:?}", pos.normals[0]);
        assert!(neg.normals.iter().all(|n| n.z > 0.9));
    }

    // The two independent consumers of `bank` must agree: at each centerline
    // vertex the mesh's per-vertex up-normal equals the up-normal
    // `Lane::sample_at` derives at that vertex's arc length. Both roll +Z about
    // the same stored tangent by the same bank, so they cannot diverge.
    #[test]
    fn mesh_normal_matches_sample_at_up_at_each_vertex() {
        let pts = vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(8.0, 0.0, 0.0),
            Point::new(16.0, 6.0, 0.0),
            Point::new(20.0, 16.0, 0.0),
        ];
        let bank = vec![0.05, 0.12, 0.18, 0.1];
        let net = banked_net(pts.clone(), bank, 5.0);
        let mesh = net.surface_mesh();
        let lane = &net.lanes()[0];
        // Cumulative arc length at each vertex.
        let mut s = 0.0_f32;
        for i in 0..pts.len() {
            if i > 0 {
                s += (pts[i] - pts[i - 1]).length();
            }
            let mesh_up = mesh.normals[2 * i]; // left and right share the vertex normal
            let sampled_up = lane.sample_at(s).up;
            assert!(
                mesh_up.abs_diff_eq(sampled_up, 1e-5),
                "vertex {i} (s={s}): mesh normal {mesh_up:?} != sample_at up {sampled_up:?}"
            );
        }
    }

    #[test]
    fn height_at_reads_the_surface_under_a_point() {
        // One flat 4 m-wide lane heading +X, from x=0 to x=10 at z=0.
        let net = RoadNetwork::new(vec![Lane {
            id: LaneId(0),
            kind: LaneType::Driving,
            layer: crate::Layer::Permanent,
            direction: Direction::Forward,
            center: Polyline::new(vec![Point::ORIGIN, Point::new(10.0, 0.0, 0.0)]),
            width: 4.0,
            widths: Vec::new(),
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }]);
        let mesh = net.surface_mesh();
        // On the lane: z = 0, normal up.
        let (z, n) = mesh.height_at(5.0, 1.0).expect("on the lane");
        assert!(z.abs() < 1e-5, "z {z}");
        assert!(n.abs_diff_eq(Vector::Z, 1e-5), "n {n:?}");
        // Off the lane (beyond the half-width): nothing under the point.
        assert!(mesh.height_at(5.0, 10.0).is_none());
    }

    #[test]
    fn height_at_normal_is_continuous_across_facets() {
        // The banked oval's curves are faceted (a polyline of segments). A body
        // oriented to the flat per-triangle normal snaps as it crosses each facet
        // edge, which is visible attitude vibration. height_at interpolates the smooth
        // vertex normals, so the normal must vary continuously as the sample point
        // walks across facet boundaries, not jump.
        let mesh = crate::fixtures::banked_oval().surface_mesh();
        // Walk a fine line along a banked curve (the right curve: centre x=+35).
        let (mut prev, mut worst) = (None, 0.0f32);
        let mut s = 0.0;
        while s < std::f32::consts::PI {
            let x = 35.0 + 26.0 * s.cos();
            let y = -26.0 * s.sin();
            if let Some((_, n)) = mesh.height_at(x, y) {
                if let Some(p) = prev {
                    let cos = n.dot(p).clamp(-1.0, 1.0);
                    worst = worst.max(cos.acos().to_degrees());
                }
                prev = Some(n);
            }
            s += 0.02; // ~0.5 m steps, several per 2.5 m facet
        }
        // Interpolated normals drift by a fraction of a degree per step; the flat
        // per-facet normal would jump several degrees at each edge.
        assert!(
            worst < 1.5,
            "normal jumps {worst:.2} deg across a facet; not interpolated?"
        );
    }

    #[test]
    fn height_at_follows_a_banked_cross_section() {
        // A banked lane: the surface height varies across the width, and the
        // normal tilts. height_at reports the faceted surface, not a plane.
        let net = RoadNetwork::new(vec![Lane {
            id: LaneId(0),
            kind: LaneType::Driving,
            layer: crate::Layer::Permanent,
            direction: Direction::Forward,
            center: Polyline::new(vec![Point::ORIGIN, Point::new(10.0, 0.0, 0.0)]),
            width: 6.0,
            widths: Vec::new(),
            bank: vec![0.2, 0.2],
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }]);
        let mesh = net.surface_mesh();
        // -y is to the right of +X travel (negative offset); the left (+bank)
        // edge is toward +y and rides higher.
        let (left, _) = mesh.height_at(5.0, 2.0).expect("left of centre");
        let (right, _) = mesh.height_at(5.0, -2.0).expect("right of centre");
        assert!(
            left > right + 0.3,
            "left {left} should ride above right {right}"
        );
        let (_, n) = mesh.height_at(5.0, 0.0).expect("centre");
        assert!(n.z < 0.999 && n.z > 0.9, "normal should tilt: {n:?}");
    }

    #[test]
    fn demo_road_mesh_is_non_degenerate_and_faces_up() {
        let mesh = demo_road().surface_mesh();
        assert!(!mesh.vertices.is_empty());
        assert_eq!(mesh.normals.len(), mesh.vertices.len());
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
        // The graded road tilts slightly but every normal still points upward.
        assert!(mesh.normals.iter().all(|n| n.z > 0.9));
    }
}
