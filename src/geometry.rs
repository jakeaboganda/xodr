use std::ops::Range;

use crate::coords::{Point, Vector};

/// A position plus a horizontal heading along a lane. Z is up (elevation).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pose {
    /// Position on the lane.
    pub position: Point,
    /// Unit tangent in the XY (ground) plane, the direction of travel. Zero
    /// where the polyline has no direction: at a vertex where it turns
    /// straight back, or where it has no horizontal length either side, as at
    /// a repeated point. Polylines the importer bakes have neither.
    pub heading: Vector,
}

/// The road surface at one station: where a body sits and how it is oriented on
/// a (possibly canted) road, plus the bank angle a vehicle model consumes. This
/// is what draping a body onto the road needs. See [`crate::Lane::sample_at`]
/// and [`crate::RoadNetwork::sample_near`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadSample {
    /// Centerline surface point, already at the banked height.
    pub point: Point,
    /// Unit tangent in the XY plane. The centerline's *stored* (geometry)
    /// direction, which for a `Backward` lane opposes travel; it is the frame
    /// `bank`/`up` are defined in.
    pub heading: Vector,
    /// Superelevation (rad, signed; positive raises the +offset / left edge).
    pub bank: f32,
    /// Surface up-normal: +Z rolled about `heading` by `bank`. Lateral cant
    /// only. Since `heading` is horizontal, `up.dot(heading) == 0`, so this
    /// carries no fore-aft **grade** pitch. Exact on a level road; on a graded
    /// lane it omits the small pitch component. Read the meshed vertices via
    /// [`Mesh::height_at`](crate::Mesh::height_at) if you need the grade too.
    pub up: Vector,
}

impl RoadSample {
    /// Build a sample, deriving the up-normal by rolling +Z about the horizontal
    /// `heading` by `bank`. `heading` must be the centerline's stored tangent so
    /// the roll frame agrees with how `bank` is defined (positive raises the
    /// left-hand-normal edge).
    pub fn new(point: Point, heading: Vector, bank: f32) -> Self {
        let up = Vector::Z
            .rotate_about(heading, bank)
            .normalize_or(Vector::Z);
        Self {
            point,
            heading,
            bank,
            up,
        }
    }
}

/// The result of projecting a world point onto a polyline.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Projection {
    /// Arc length of the nearest point along the polyline.
    pub s: f32,
    /// The nearest point on the polyline itself.
    pub point: Point,
    /// Signed lateral distance from the polyline, in the ground plane.
    /// Positive is to the **left** of travel; negative is to the right.
    pub offset: f32,
}

/// A polyline in 3D (Z-up, meters), queried by arc length. This is the baked
/// form every curve reduces to: an importer samples clothoids/arcs into points;
/// consumers only ever see the points. At least two points.
///
/// Serializes as its points plus its two boundary tangents, and nothing else.
/// The cumulative lengths and the interior tangents are derived, so sending
/// them would be both wasteful and a way to receive a polyline whose cached
/// state disagrees with its geometry. The boundary tangents are not derivable
/// (that is the whole point of [`Polyline::try_new_with_tangents`]), so they
/// have to travel, or a round trip would silently reopen the section seams they
/// exist to close.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(into = "PolylineData", try_from = "PolylineData")
)]
pub struct Polyline {
    points: Vec<Point>,
    /// Cumulative arc length at each point; `cumulative[0] == 0`.
    cumulative: Vec<f32>,
    /// Per-vertex unit horizontal tangent: the angle bisector at interior
    /// vertices; at the ends either the importer's analytical curve tangent or,
    /// failing that, the lone segment direction. Interpolating these gives a
    /// heading that's continuous across vertices (no per-segment step), and
    /// their normals give a consistent lateral offset for lanes/meshes.
    tangents: Vec<Vector>,
}

impl Polyline {
    /// Build a polyline, or `None` if given fewer than two points. Importers
    /// baking **external** map data (which may be malformed) must use this and
    /// surface the error, rather than crash. See [`Polyline::new`].
    pub fn try_new(points: Vec<Point>) -> Option<Self> {
        Self::build(points, None)
    }

    /// Build a polyline whose first and last tangents are given rather than
    /// approximated from the end segments.
    ///
    /// A chord only approximates the curve it was sampled from, and at an
    /// endpoint there is no second chord to average with, so the error does not
    /// cancel. Two contiguous polylines sampled from the same curve therefore
    /// disagree about the tangent at the vertex they share, and anything that
    /// offsets laterally (lane edges, mesh ribs) opens a V-shaped seam there.
    /// An importer that knows the curve analytically passes the true tangents
    /// here; both sides then derive the same normal and the seam closes.
    ///
    /// Tangents are used horizontally and normalized; a zero, degenerate, or
    /// non-finite one falls back to the end segment's direction.
    pub fn try_new_with_tangents(
        points: Vec<Point>,
        start_tangent: Vector,
        end_tangent: Vector,
    ) -> Option<Self> {
        Self::build(points, Some((start_tangent, end_tangent)))
    }

    fn build(points: Vec<Point>, ends: Option<(Vector, Vector)>) -> Option<Self> {
        if points.len() < 2 {
            return None;
        }
        let mut cumulative = Vec::with_capacity(points.len());
        let mut acc = 0.0;
        cumulative.push(0.0);
        for pair in points.windows(2) {
            acc += (pair[1] - pair[0]).length();
            cumulative.push(acc);
        }
        let tangents = vertex_tangents(&points, ends);
        Some(Self {
            points,
            cumulative,
            tangents,
        })
    }

    /// Build from trusted, in-code geometry. Panics on fewer than two points,
    /// which is a construction bug rather than a runtime condition. Importers
    /// handling external files use [`Polyline::try_new`] instead.
    pub fn new(points: Vec<Point>) -> Self {
        Self::try_new(points).expect("a polyline needs at least two points")
    }

    /// The baked vertices, in geometry order.
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Per-vertex horizontal unit tangents, parallel to `points`: the bisector
    /// at an interior vertex, and the curve's analytical tangent at either end
    /// where the importer supplied one. Zero at a vertex with no direction, as
    /// [`Pose::heading`] says. These are what [`Polyline::pose_at`]
    /// interpolates, so reading them is how a consumer reproduces its heading
    /// at a vertex rather than re-deriving one from the chords and disagreeing.
    pub fn tangents(&self) -> &[Vector] {
        &self.tangents
    }

    /// Total arc length (metres).
    pub fn length(&self) -> f32 {
        self.cumulative.last().copied().unwrap_or(0.0)
    }

    /// The segment index containing arc length `s` (clamped to a valid segment).
    fn segment(&self, s: f32) -> usize {
        let last = self.points.len() - 2;
        self.cumulative
            .partition_point(|&c| c <= s)
            .saturating_sub(1)
            .min(last)
    }

    /// Fractional position `t` within segment `i` for arc length `s`.
    fn local_t(&self, i: usize, s: f32) -> f32 {
        let seg_len = self.cumulative[i + 1] - self.cumulative[i];
        if seg_len > 1e-6 {
            (s - self.cumulative[i]) / seg_len
        } else {
            0.0
        }
    }

    /// The segment index containing arc length `s`, and the fractional position
    /// `t` in `[0, 1]` within it, both clamped to a valid segment. The one place
    /// arc length becomes a `(vertex i, vertex i+1, t)` lerp. Every
    /// by-arc-length sampler shares it (position, heading, and a lane's
    /// per-vertex bank), so they cannot disagree about where `s` lands.
    pub(crate) fn locate(&self, s: f32) -> (usize, f32) {
        let s = s.clamp(0.0, self.length());
        let i = self.segment(s);
        (i, self.local_t(i, s))
    }

    /// Position at arc length `s` (clamped to `[0, length]`).
    pub fn point_at(&self, s: f32) -> Point {
        let (i, t) = self.locate(s);
        self.points[i].lerp(self.points[i + 1], t)
    }

    /// Position and horizontal heading at arc length `s`. The heading
    /// interpolates the per-vertex tangents, so it's continuous across vertices
    /// (a path-tracking controller sees no per-segment step).
    pub fn pose_at(&self, s: f32) -> Pose {
        let (i, t) = self.locate(s);
        Pose {
            position: self.points[i].lerp(self.points[i + 1], t),
            heading: self.tangents[i]
                .lerp(self.tangents[i + 1], t)
                .normalize_or_zero(),
        }
    }

    /// Nearest point on the polyline to `point`, with its arc length and signed
    /// lateral offset. Nearest is by full 3D distance; the offset is measured in
    /// the ground plane against the containing segment's direction.
    pub fn project(&self, point: Point) -> Projection {
        self.project_segments(point, 0..self.points.len() - 1)
    }

    /// [`Self::project`] onto only the segments `segments`, segment `i`
    /// running from point `i` to point `i + 1`. Arc length is still measured
    /// from the start of the whole line. Exact ties go to the earliest
    /// segment, as in `project`.
    pub(crate) fn project_segments(&self, point: Point, segments: Range<usize>) -> Projection {
        let mut best = Projection {
            s: self.cumulative[segments.start],
            point: self.points[segments.start],
            offset: 0.0,
        };
        let mut best_dist = f32::INFINITY;
        for i in segments {
            let a = self.points[i];
            let ab = self.points[i + 1] - a;
            let len2 = ab.length_squared();
            let t = if len2 > 1e-9 {
                ((point - a).dot(ab) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let closest = a + ab * t;
            let dist = (point - closest).length_squared();
            if dist < best_dist {
                best_dist = dist;
                let heading = horizontal(ab).normalize_or_zero();
                best = Projection {
                    s: self.cumulative[i] + ab.length() * t,
                    point: closest,
                    offset: horizontal(point - closest).dot(left_normal(heading)),
                };
            }
        }
        best
    }
}

impl From<Polyline> for Vec<Point> {
    fn from(line: Polyline) -> Self {
        line.points
    }
}

/// Why a list of points is not a polyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a polyline needs at least two points")]
pub struct TooFewPoints;

impl TryFrom<Vec<Point>> for Polyline {
    type Error = TooFewPoints;

    fn try_from(points: Vec<Point>) -> Result<Self, Self::Error> {
        Self::try_new(points).ok_or(TooFewPoints)
    }
}

/// The wire form of a [`Polyline`]: everything about it that cannot be
/// recomputed from the rest. `tangents` is `(first, last)`.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
struct PolylineData {
    points: Vec<Point>,
    tangents: (Vector, Vector),
}

#[cfg(feature = "serde")]
impl From<Polyline> for PolylineData {
    fn from(line: Polyline) -> Self {
        // A polyline always has at least two points, so both tangents exist.
        let tangents = (line.tangents[0], line.tangents[line.tangents.len() - 1]);
        Self {
            points: line.points,
            tangents,
        }
    }
}

#[cfg(feature = "serde")]
impl TryFrom<PolylineData> for Polyline {
    type Error = TooFewPoints;

    fn try_from(data: PolylineData) -> Result<Self, Self::Error> {
        let (start, end) = data.tangents;
        // A malformed tangent from the wire degrades to the end chord rather
        // than rejecting the polyline: the geometry is still usable.
        Self::try_new_with_tangents(data.points, start, end).ok_or(TooFewPoints)
    }
}

/// Drop a vector onto the XY ground plane.
fn horizontal(v: Vector) -> Vector {
    Vector::new(v.x, v.y, 0.0)
}

/// Per-vertex unit horizontal tangents: the bisector of the adjacent segment
/// directions at interior vertices. At the two ends, `ends` supplies the true
/// curve tangents when the caller knows them; otherwise, and whenever a supplied
/// tangent is degenerate, the single adjacent segment direction stands in.
fn vertex_tangents(points: &[Point], ends: Option<(Vector, Vector)>) -> Vec<Vector> {
    let n = points.len();
    (0..n)
        .map(|i| {
            let incoming = if i > 0 {
                horizontal(points[i] - points[i - 1]).normalize_or_zero()
            } else {
                Vector::ZERO
            };
            let outgoing = if i + 1 < n {
                horizontal(points[i + 1] - points[i]).normalize_or_zero()
            } else {
                Vector::ZERO
            };
            let chord = (incoming + outgoing).normalize_or_zero();
            match ends {
                Some((start, _)) if i == 0 => horizontal(start).normalize_or(chord),
                Some((_, end)) if i == n - 1 => horizontal(end).normalize_or(chord),
                _ => chord,
            }
        })
        .collect()
}

/// Unit left-hand normal of a horizontal `heading` (about +Z up). For heading
/// +X this is +Y. Zero if the heading has no horizontal extent.
pub(crate) fn left_normal(heading: Vector) -> Vector {
    Vector::Z.cross(horizontal(heading)).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[[f32; 3]]) -> Polyline {
        Polyline::new(points.iter().map(|p| Point::from_array(*p)).collect())
    }

    #[test]
    fn supplied_tangents_replace_the_end_chords() {
        // An L, sampled as if from a curve that arrives along +X at 45 degrees
        // and leaves along +Y at 45 degrees.
        let pts = vec![
            Point::ORIGIN,
            Point::new(10.0, 0.0, 0.0),
            Point::new(10.0, 10.0, 0.0),
        ];
        let diag = Vector::new(1.0, 1.0, 0.0).normalize_or_zero();
        let l = Polyline::try_new_with_tangents(pts.clone(), diag, diag).unwrap();
        assert!(l.tangents()[0].abs_diff_eq(diag, 1e-6));
        assert!(l.tangents()[2].abs_diff_eq(diag, 1e-6));
        // The interior vertex keeps its chord bisector, which for this L is the
        // same diagonal - so only the ends moved.
        assert!(l.tangents()[1].abs_diff_eq(diag, 1e-6));
        // Without them the ends fall back to the lone adjacent chord.
        let plain = Polyline::new(pts);
        assert!(plain.tangents()[0].abs_diff_eq(Vector::new(1.0, 0.0, 0.0), 1e-6));
        assert!(plain.tangents()[2].abs_diff_eq(Vector::new(0.0, 1.0, 0.0), 1e-6));
    }

    #[test]
    fn a_degenerate_supplied_tangent_falls_back_to_the_chord() {
        let pts = vec![Point::ORIGIN, Point::new(10.0, 0.0, 0.0)];
        let l = Polyline::try_new_with_tangents(pts, Vector::ZERO, Vector::new(f32::NAN, 0.0, 0.0))
            .unwrap();
        assert!(l.tangents()[0].abs_diff_eq(Vector::new(1.0, 0.0, 0.0), 1e-6));
        assert!(l.tangents()[1].abs_diff_eq(Vector::new(1.0, 0.0, 0.0), 1e-6));
    }

    #[test]
    fn try_new_rejects_too_few_points() {
        assert!(Polyline::try_new(vec![Point::ORIGIN]).is_none());
        assert!(Polyline::try_new(vec![]).is_none());
        assert!(Polyline::try_new(vec![Point::ORIGIN, Point::new(1.0, 0.0, 0.0)]).is_some());
    }

    #[test]
    fn length_sums_the_segments() {
        let l = line(&[[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 4.0, 0.0]]);
        assert!((l.length() - 7.0).abs() < 1e-5);
    }

    #[test]
    fn point_at_interpolates_and_clamps() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        assert!((l.point_at(5.0) - Point::new(5.0, 0.0, 0.0)).length() < 1e-5);
        // Past the end clamps to the last point.
        assert!((l.point_at(100.0) - Point::new(10.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn pose_heading_exact_at_ends_and_continuous_across_a_vertex() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]]);
        // Endpoints resolve to the exact segment directions.
        assert!(l.pose_at(0.0).heading.abs_diff_eq(Vector::X, 1e-5));
        assert!(l.pose_at(l.length()).heading.abs_diff_eq(Vector::Y, 1e-5));
        // No per-segment jump: heading just before and after the vertex agree.
        let before = l.pose_at(10.0 - 0.01).heading;
        let after = l.pose_at(10.0 + 0.01).heading;
        assert!((before - after).length() < 0.02, "{before:?} vs {after:?}");
    }

    #[test]
    fn project_gives_arc_length_and_signed_offset() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        // A point to the left of +X travel (toward +Y) is a positive offset.
        let left = l.project(Point::new(5.0, 3.0, 0.0));
        assert!((left.s - 5.0).abs() < 1e-4);
        assert!((left.offset - 3.0).abs() < 1e-4, "offset {}", left.offset);
        // A point to the right (-Y) is negative.
        let right = l.project(Point::new(5.0, -3.0, 0.0));
        assert!((right.offset + 3.0).abs() < 1e-4, "offset {}", right.offset);
    }

    #[test]
    fn project_arc_length_spans_multiple_segments() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]]);
        // A point beside the second segment lands past the first segment's end.
        let p = l.project(Point::new(12.0, 6.0, 0.0));
        assert!((p.s - 16.0).abs() < 1e-4, "s {}", p.s);
    }

    #[test]
    fn left_normal_of_plus_x_is_plus_y() {
        assert!(left_normal(Vector::X).abs_diff_eq(Vector::Y, 1e-5));
    }

    // --- RoadSample::new invariants ------------------------------------------

    // For any bank angle the up-normal stays unit length and orthogonal to the
    // horizontal heading (the roll axis), and at bank 0 it is *exactly* +Z.
    #[test]
    fn road_sample_up_is_unit_orthogonal_and_plumb_at_zero() {
        let headings = [
            Vector::X,
            Vector::Y,
            Vector::new(1.0, 1.0, 0.0).normalize_or_zero(),
            Vector::new(-2.0, 1.0, 0.0).normalize_or_zero(),
        ];
        for h in headings {
            for bank in [0.0_f32, 0.2, -0.2, 1.5, -1.5] {
                let s = RoadSample::new(Point::new(3.0, 4.0, 1.0), h, bank);
                // Unit length.
                assert!(
                    (s.up.length() - 1.0).abs() < 1e-5,
                    "up not unit for heading {h:?} bank {bank}: len {}",
                    s.up.length()
                );
                // Orthogonal to heading (lateral cant only; no fore-aft grade).
                assert!(
                    s.up.dot(h).abs() < 1e-6,
                    "up.heading = {} for heading {h:?} bank {bank}",
                    s.up.dot(h)
                );
            }
            // At bank 0 the up-normal is exactly +Z (not just approximately).
            assert_eq!(
                RoadSample::new(Point::ORIGIN, h, 0.0).up,
                Vector::Z,
                "bank 0 up must be exactly +Z for heading {h:?}"
            );
        }
    }

    // Sign: for heading +X, +bank raises the left (+Y) edge, so the surface
    // tips down to the right and its up-normal leans toward -Y. -bank mirrors it.
    #[test]
    fn road_sample_up_sign_flips_with_bank_sign() {
        let pos = RoadSample::new(Point::ORIGIN, Vector::X, 0.3);
        let neg = RoadSample::new(Point::ORIGIN, Vector::X, -0.3);
        assert!(pos.up.y < -0.05, "+bank should lean -Y: {:?}", pos.up);
        assert!(neg.up.y > 0.05, "-bank should lean +Y: {:?}", neg.up);
        // Mirrored: same height component, opposite lateral one.
        assert!((pos.up.z - neg.up.z).abs() < 1e-6);
        assert!((pos.up.y + neg.up.y).abs() < 1e-6);
    }

    #[test]
    fn locate_finds_segment_and_fraction() {
        let l = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]]);
        // Mid first segment.
        assert_eq!(l.locate(5.0), (0, 0.5));
        // Mid second segment (arc length 15 of 20).
        let (i, t) = l.locate(15.0);
        assert_eq!(i, 1);
        assert!((t - 0.5).abs() < 1e-5, "t {t}");
        // Clamps below and above the polyline.
        assert_eq!(l.locate(-3.0), (0, 0.0));
        let (i, t) = l.locate(100.0);
        assert_eq!(i, 1);
        assert!((t - 1.0).abs() < 1e-5, "t {t}");
    }
}
