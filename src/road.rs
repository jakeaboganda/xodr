//! Roads in road coordinates: each road's reference line, the profiles along
//! it, and the geometry of its lane sections, and what places a road station
//! `(s, t)` in the world.

use crate::coords::Point;
use crate::crg::RefPoint;
use crate::junction::ElevationGrid;
use crate::{Direction, LaneId, Layer, RoadMarkId};

/// A road's identity: its position in
/// [`RoadNetwork::roads`](crate::RoadNetwork::roads). Look a road up with
/// [`RoadNetwork::road`](crate::RoadNetwork::road).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadId(pub usize);

/// A place on a road in its own coordinates, as OpenDRIVE gives them.
///
/// `s` runs along the road's reference line from its start, and `t` across
/// it, positive to the left. Both are in metres. `t` is measured along the
/// road's cross-section, which superelevation tilts, so a point at `t` stands
/// `t cos φ` from the reference line in plan on a road banked by `φ`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadPosition {
    /// The road.
    pub road: RoadId,
    /// How far along its reference line.
    pub s: f64,
    /// How far across it, positive to the left.
    pub t: f64,
}

/// One road giving way to another where both are in the same junction, from
/// an OpenDRIVE junction `<priority>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Priority {
    /// The road with priority.
    pub high: RoadId,
    /// The road that gives way to it.
    pub low: RoadId,
}

/// A road running beside another, from an OpenDRIVE road `<neighbor>`: the
/// road that names it, which side of that road it is on, and whether it runs
/// the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadNeighbor {
    /// The road whose `<link>` names it.
    pub road: RoadId,
    /// The road beside it.
    pub neighbor: RoadId,
    /// Which side of `road` it is on, looking along `road`'s `+s`.
    pub side: Side,
    /// Whether its `+s` runs the same way as `road`'s.
    pub same_direction: bool,
}

/// The left or the right of a road, looking along its `+s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Side {
    /// Toward `+t`.
    Left,
    /// Toward `-t`.
    Right,
}

/// A place on a lane: `s` along its road's reference line, and `offset`
/// across from the lane's center, as esmini's `SetLanePos` takes them.
///
/// `s` is the road's, not the distance along the lane's own centerline, so a
/// lane position and a road position at one place share it. `offset` is
/// along the road's `+t`, positive to the left of the reference line
/// whichever way the lane's traffic runs. Both are in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LanePosition {
    /// The lane.
    pub lane: LaneId,
    /// How far along its road's reference line.
    pub s: f64,
    /// How far across from its center, positive toward `+t`.
    pub offset: f64,
}

/// Where a lane lies on its road: the road, the lane section, and its
/// OpenDRIVE `<lane id>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadLane {
    /// The road.
    pub road: RoadId,
    /// The zero-based lane-section index within the road, ordered by `s`, as
    /// in [`LaneProvenance::section`](crate::LaneProvenance::section).
    pub section: usize,
    /// The lane's `<lane id>`: negative right of the reference line, positive
    /// left.
    pub od_id: i32,
}

/// One road: its reference line, the profiles along it, and where its lanes
/// lie across it. What turns a [`RoadPosition`] into a point, and back.
///
/// Serializes as the records the file gives, and rebakes its spirals and
/// cubic curves on the way back in.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Road {
    pub(crate) id: RoadId,
    pub(crate) od_id: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub(crate) name: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub(crate) junction: Option<String>,
    pub(crate) length: f64,
    pub(crate) rule: TrafficRule,
    pub(crate) geoms: Vec<GeomRec>,
    pub(crate) elevations: Vec<Cubic>,
    pub(crate) superelevations: Vec<Cubic>,
    pub(crate) lateral: Lateral,
    pub(crate) lane_offsets: Vec<Cubic>,
    /// The `<laneOffset>`s of the temporary layer, which are its own.
    #[cfg_attr(feature = "serde", serde(default))]
    pub(crate) temporary_lane_offsets: Vec<Cubic>,
    pub(crate) sections: Vec<RoadSection>,
}

impl Road {
    /// Its identity on the network.
    pub fn id(&self) -> RoadId {
        self.id
    }

    /// The `<road id>` it came from.
    pub fn od_id(&self) -> &str {
        &self.od_id
    }

    /// The road's `name`. Empty if it has none.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The `<junction id>` of the junction it is part of, from its
    /// `junction`, or `None` for a road outside any junction (`-1`).
    pub fn junction(&self) -> Option<&str> {
        self.junction.as_deref()
    }

    /// How long its reference line is, in metres, from its `length`.
    pub fn length(&self) -> f64 {
        self.length
    }
}

/// One lane section of a road: the stretch of road it covers, where its
/// lanes lie across it, and the lanes it baked to.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct RoadSection {
    /// Its position among the road's `<laneSection>`s: the permanent
    /// layer's in order of `s`, then the temporary layer's.
    pub index: usize,
    /// The layer it is on.
    #[cfg_attr(feature = "serde", serde(default))]
    pub layer: Layer,
    pub start: f64,
    pub end: f64,
    /// The stations its lanes are sampled at. Each baked lane has one
    /// centerline point at each.
    pub stations: Vec<f64>,
    /// The left lanes, ordered from the center outward: 1, 2, 3, ...
    pub left: Vec<LaneGeom>,
    /// The right lanes, ordered from the center outward: -1, -2, -3, ...
    pub right: Vec<LaneGeom>,
    /// Each baked lane's `<lane id>` and [`LaneId`].
    pub lanes: Vec<(i32, LaneId)>,
    /// The stretch of road each of its road marks runs along.
    #[cfg_attr(feature = "serde", serde(default))]
    pub marks: Vec<MarkSpan>,
}

/// The stretch of road one road mark runs along, in metres along the
/// reference line, on the outer border of the lane `od_lane_id`, or on the
/// center line for 0.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct MarkSpan {
    pub od_lane_id: i32,
    pub start: f64,
    pub end: f64,
    pub mark: RoadMarkId,
}

/// Where one lane lies across its road: its `<width>`s or `<border>`s, its
/// `<height>`s, and whether it is level.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct LaneGeom {
    pub id: i32,
    pub extent: LaneExtent,
    /// Its `<height>`s, in order along it.
    pub heights: Vec<HeightDef>,
    pub level: bool,
}

/// How far across the road a lane reaches: its `<width>`s, or where it has
/// none, its `<border>`s. Each list is sorted by `sOffset`.
///
/// A border is the `t` of the lane's outer border, measured from the
/// reference line, so `<laneOffset>` does not move it. The spec forbids the
/// two together. The lane's width is its border less its inner neighbour's
/// outer border, signed by side and 0 where the border crosses inside it,
/// which the spec also forbids.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum LaneExtent {
    Width(Vec<Cubic>),
    Border(Vec<Cubic>),
}

/// A lane's `<height>`: how far its surface stands off the road, along the
/// road's normal, at its inner and outer border, from `s_offset` metres into
/// its lane section.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct HeightDef {
    pub s_offset: f64,
    pub inner: f64,
    pub outer: f64,
}

/// A road's height off its reference plane across it, from its
/// `<lateralProfile>`: its `<shape>`s, and its `<crossSectionSurface>`. The
/// spec forbids the two together. Where a file gives both, their heights add.
/// A road of a junction with an `<elevationGrid>` stands on the grid
/// instead, its [`Ground`].
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Lateral {
    pub profiles: Vec<ShapeProfile>,
    pub surface: Option<CrossSection>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub ground: Option<Box<Ground>>,
    /// Its `<crossfall>`s from OpenDRIVE 1.4, in order of `s`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub crossfall: Vec<Crossfall>,
}

/// A `<crossfall>` from OpenDRIVE 1.4: from `s` on, the road falls away
/// from its reference line at an angle, a cubic in `s` in radians, on the
/// side it names. Its height at `t` is `-tan(angle) * |t|`, as libOpenDRIVE
/// reads it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Crossfall {
    pub angle: Cubic,
    /// Whether it applies left of the reference line, and right of it.
    pub left: bool,
    pub right: bool,
}

/// The ground a road of a junction with an `<elevationGrid>` stands on: the
/// grid's height under each point of the road, which overrides the road's
/// own elevation, superelevation and lateral shape, as the spec says. The
/// road's reference plane is then level at 0, so the ground is its whole
/// height. Where the grid does not reach, the road's own elevation and
/// superelevation hold.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Ground {
    pub grid: ElevationGrid,
    /// The road's reference line, to find where `(s, t)` is in plan.
    pub plan: Vec<GeomRec>,
    /// The road's own elevation and superelevation, where the grid does not
    /// reach.
    pub elevations: Vec<Cubic>,
    pub superelevations: Vec<Cubic>,
    /// Where roads come into the junction, the zones that blend each into
    /// the grid.
    #[cfg_attr(feature = "serde", serde(default))]
    pub transitions: Vec<Transition>,
}

/// The zone where a road comes into a junction, from a boundary `joint`
/// segment with a `transitionLength`, where the height blends from the road
/// into the grid, as the spec's section 12.11.2 gives it.
///
/// The zone is the four-sided polygon from `corners[0]` to `corners[1]`
/// along the joint, then `corners[2]` and `corners[3]`, `transitionLength`
/// along the boundary's `lane` segments either side. A point in it is
/// `k` along the joint and `l` from the joint to the far side. Its height is
/// the cubic in `l` from the road's height and slope at the joint to the
/// grid's on the far side.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Transition {
    pub corners: [[f64; 2]; 4],
    /// The road's height at the joint, and its slope into the junction, in
    /// metres per metre, at even steps of `k` from 0 to 1.
    pub road: Vec<(f64, f64)>,
}

impl Transition {
    /// The height at `(x, y)` on `grid`, or `None` outside the zone or where
    /// the grid does not reach its far side.
    fn height(&self, grid: &ElevationGrid, x: f64, y: f64) -> Option<f64> {
        let (k, l) = self.locate(x, y)?;
        let [p0, p1, p2, p3] = self.corners;
        let lerp =
            |a: [f64; 2], b: [f64; 2], f: f64| [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
        let (a, b) = (lerp(p0, p1, k), lerp(p3, p2, k));
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if length < 1e-9 {
            return None;
        }
        let (ux, uy) = ((b[0] - a[0]) / length, (b[1] - a[1]) / length);
        let (z_a, slope_a) = self.road_at(k);
        let z_b = grid.height_at(b[0], b[1])?;
        const STEP: f64 = 0.05;
        let back = grid.height_at(b[0] - ux * STEP, b[1] - uy * STEP)?;
        let (m_a, m_b) = (slope_a * length, (z_b - back) / STEP * length);
        let c = -3.0 * z_a + 3.0 * z_b - 2.0 * m_a - m_b;
        let d = 2.0 * z_a - 2.0 * z_b + m_a + m_b;
        Some(z_a + m_a * l + c * l * l + d * l * l * l)
    }

    /// The road's height and slope at `k` along the joint.
    fn road_at(&self, k: f64) -> (f64, f64) {
        let last = self.road.len().saturating_sub(1);
        let at = (k.clamp(0.0, 1.0) * last as f64).min(last as f64);
        let (i, f) = (at.floor() as usize, at.fract());
        let (a, b) = (self.road[i], self.road[(i + 1).min(last)]);
        (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
    }

    /// Where `(x, y)` is in the zone, as `(k, l)` each from 0 to 1, or `None`
    /// outside it. Solves the spec's bilinear `P = P0 + k (P1 - P0) +
    /// l (P3 - P0) + k l (P2 - P3 - P1 + P0)`.
    fn locate(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        const EPS: f64 = 1e-9;
        let [p0, p1, p2, p3] = self.corners;
        let e = [p1[0] - p0[0], p1[1] - p0[1]];
        let f = [p3[0] - p0[0], p3[1] - p0[1]];
        let g = [p0[0] - p1[0] + p2[0] - p3[0], p0[1] - p1[1] + p2[1] - p3[1]];
        let h = [x - p0[0], y - p0[1]];
        let cross = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
        let (k2, k1, k0) = (cross(g, f), cross(e, f) + cross(h, g), cross(h, e));
        let ls: Vec<f64> = if k2.abs() < EPS {
            if k1.abs() < EPS {
                return None;
            }
            vec![-k0 / k1]
        } else {
            let disc = k1 * k1 - 4.0 * k0 * k2;
            if disc < 0.0 {
                return None;
            }
            let root = disc.sqrt();
            vec![(-k1 - root) / (2.0 * k2), (-k1 + root) / (2.0 * k2)]
        };
        ls.into_iter().find_map(|l| {
            let den = [e[0] + g[0] * l, e[1] + g[1] * l];
            let k = match den[0].abs() > den[1].abs() {
                true => (h[0] - f[0] * l) / den[0],
                false => (h[1] - f[1] * l) / den[1],
            };
            let inside = |v: f64| (-EPS..=1.0 + EPS).contains(&v);
            (inside(k) && inside(l)).then_some((k.clamp(0.0, 1.0), l.clamp(0.0, 1.0)))
        })
    }
}

impl Ground {
    /// The height at `(s, t)` on the road.
    pub fn height(&self, s: f64, t: f64) -> f64 {
        let (x, y, hdg) = geom_at(&self.plan, s).pose(s);
        let (x, y) = (x - t * hdg.sin(), y + t * hdg.cos());
        let blended = self
            .transitions
            .iter()
            .find_map(|zone| zone.height(&self.grid, x, y));
        blended
            .or_else(|| self.grid.height_at(x, y))
            .unwrap_or_else(|| {
                let elevation = active(&self.elevations, s).map_or(0.0, |e| e.eval(s));
                let bank = active(&self.superelevations, s).map_or(0.0, |e| e.eval(s));
                elevation + t * bank.sin()
            })
    }

    /// Stations a quarter of the grid's spacing apart along the road, so the
    /// lanes follow the grid's curve.
    fn knots(&self) -> impl Iterator<Item = f64> + '_ {
        let end = self.plan.iter().map(|g| g.s + g.length).fold(0.0, f64::max);
        let step = (self.grid.spacing / 4.0).max(0.25);
        let count = (end / step).min(10_000.0) as usize;
        (1..=count).map(move |k| k as f64 * step)
    }
}

impl Lateral {
    /// The height at `(s, t)`. 0 on a road with neither.
    pub fn height(&self, s: f64, t: f64) -> f64 {
        self.height_toward(s, t, -1.0)
    }

    /// [`Self::height`], but on a cross-section's ridge, where its two sides
    /// meet, the height of the side `toward` points to: 1.0 for the left.
    /// What a lane on that side stands on at its inner border.
    pub fn height_toward(&self, s: f64, t: f64, toward: f64) -> f64 {
        shape_at(&self.profiles, s, t)
            + self
                .surface
                .as_ref()
                .map_or(0.0, |c| c.height(s, t, toward))
            + self.ground.as_ref().map_or(0.0, |g| g.height(s, t))
            + self.crossfall_height(s, t, toward)
    }

    /// The crossfall's height at `(s, t)`: from the last `<crossfall>` to
    /// start at or before `s`, 0 on a side it doesn't name. On the
    /// reference line, the side `toward` points to.
    fn crossfall_height(&self, s: f64, t: f64, toward: f64) -> f64 {
        let Some(c) = self
            .crossfall
            .iter()
            .rev()
            .find(|c| c.angle.start <= s + 1e-9)
        else {
            return 0.0;
        };
        let left = if t == 0.0 { toward > 0.0 } else { t > 0.0 };
        match (left && c.left) || (!left && c.right) {
            true => -c.angle.eval(s).tan() * t.abs(),
            false => 0.0,
        }
    }

    /// Whether the road is flat across.
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
            && self.surface.is_none()
            && self.ground.is_none()
            && self.crossfall.is_empty()
    }

    /// Where along the road the height changes pace: each profile's `s`, each
    /// cross-section record's start, and steps along a grid.
    pub fn knots(&self) -> impl Iterator<Item = f64> + '_ {
        self.profiles
            .iter()
            .map(|p| p.s)
            .chain(self.surface.iter().flat_map(CrossSection::knots))
            .chain(self.ground.iter().flat_map(|g| g.knots()))
            .chain(self.crossfall.iter().map(|c| c.angle.start))
    }
}

/// A road's `<crossSectionSurface>`: up to two strips each side of the
/// reference line, shifted across it by `t_offset`, each a cubic in `dt`
/// whose four coefficients are cubics in `s`.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct CrossSection {
    pub t_offset: Vec<Cubic>,
    /// Strips 1 and 2, then -1 and -2.
    pub left: [Option<Strip>; 2],
    pub right: [Option<Strip>; 2],
}

/// One `<strip>` of a [`CrossSection`].
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Strip {
    /// Its `<width>`s: how far across the road an inner strip reaches.
    /// Empty for one that reaches to the road's edge.
    pub width: Vec<Cubic>,
    /// Its `<constant>`, `<linear>`, `<quadratic>` and `<cubic>` records.
    pub terms: [Vec<Cubic>; 4],
    /// An outer strip's `mode="relative"`: its height stands on the inner
    /// strip's outer edge.
    pub relative: bool,
}

impl Strip {
    /// The strip's height `dt` across it at `s`.
    fn height(&self, s: f64, dt: f64) -> f64 {
        self.terms.iter().rev().fold(0.0, |h, term| {
            h * dt + active(term, s).map_or(0.0, |c| c.eval(s))
        })
    }

    /// How far across the road it reaches at `s`, or `None` to the edge.
    fn width(&self, s: f64) -> Option<f64> {
        active(&self.width, s).map(|w| w.eval(s).max(0.0))
    }
}

impl CrossSection {
    /// The height at `(s, t)`, as the spec gives it: `t` less the offset,
    /// then the inner strip on that side up to its width, and the outer
    /// strip past it, `dt` from the inner strip's edge. Past an inner strip
    /// with no outer strip beside it, the inner strip runs on. 0 on a side
    /// with no strip. On the ridge, within a nanometre of `dt` 0, the side
    /// `toward` points to: 1.0 for the left.
    pub fn height(&self, s: f64, t: f64, toward: f64) -> f64 {
        let t = t - active(&self.t_offset, s).map_or(0.0, |c| c.eval(s));
        let left = if t.abs() < 1e-9 {
            toward > 0.0
        } else {
            t > 0.0
        };
        let (side, sign) = if left {
            (&self.left, 1.0)
        } else {
            (&self.right, -1.0)
        };
        let [Some(inner), outer] = side else {
            return 0.0;
        };
        let reach = inner.width(s);
        match (reach, outer) {
            (Some(w), Some(outer)) if sign * t > w => {
                let base = if outer.relative {
                    inner.height(s, sign * w)
                } else {
                    0.0
                };
                base + outer.height(s, t - sign * w)
            }
            _ => inner.height(s, t),
        }
    }

    /// Where along the road any of its records starts.
    fn knots(&self) -> impl Iterator<Item = f64> + '_ {
        self.left
            .iter()
            .chain(&self.right)
            .flatten()
            .flat_map(|strip| strip.terms.iter().chain([&strip.width]))
            .chain([&self.t_offset])
            .flatten()
            .map(|c| c.start)
    }
}

/// A `<lateralProfile>`'s `<shape>`s at one `s`: the height off the
/// reference plane across the road, a cubic in `dt` from each shape's `t`,
/// held until the next. Each [`Cubic`] here starts at its `t`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct ShapeProfile {
    pub s: f64,
    pub shapes: Vec<Cubic>,
}

/// Which side of a road drives along `+s`, from its `<road rule>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum TrafficRule {
    RightHand,
    LeftHand,
}

impl TrafficRule {
    /// How the lanes on the left side of the road travel, or on the right.
    pub fn direction(self, left: bool) -> Direction {
        if left == (self == Self::LeftHand) {
            Direction::Forward
        } else {
            Direction::Backward
        }
    }
}

// --- Reference-line geometry --------------------------------------------------

/// One `<geometry>` record: its start pose on the reference line plus its
/// shape. A spiral or a paramPoly3 has no elementary arc-length form, so it
/// is baked to `(ds, x, y, hdg)` samples when built, which [`GeomRec::pose`]
/// interpolates. It serializes without them, and rebakes them on the way
/// back in.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(from = "GeomDef", into = "GeomDef")
)]
pub(crate) struct GeomRec {
    pub s: f64,
    pub x: f64,
    pub y: f64,
    pub hdg: f64,
    pub length: f64,
    pub shape: GeomShape,
    samples: Vec<(f64, f64, f64, f64)>,
}

/// The shape of a `<geometry>`. A `<poly3>` is the paramPoly3 with
/// `u(p) = p` over `p` in `[0, length]`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum GeomShape {
    Line,
    Arc {
        curvature: f64,
    },
    Spiral {
        curv_start: f64,
        curv_end: f64,
    },
    ParamPoly3 {
        u: [f64; 4],
        v: [f64; 4],
        p_max: f64,
    },
}

/// What a [`GeomRec`] serializes as: its definition, without the samples.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
struct GeomDef {
    s: f64,
    x: f64,
    y: f64,
    hdg: f64,
    length: f64,
    shape: GeomShape,
}

#[cfg(feature = "serde")]
impl From<GeomDef> for GeomRec {
    fn from(d: GeomDef) -> Self {
        Self::new(d.s, d.x, d.y, d.hdg, d.length, d.shape)
    }
}

#[cfg(feature = "serde")]
impl From<GeomRec> for GeomDef {
    fn from(g: GeomRec) -> Self {
        Self {
            s: g.s,
            x: g.x,
            y: g.y,
            hdg: g.hdg,
            length: g.length,
            shape: g.shape,
        }
    }
}

impl GeomRec {
    /// The record starting at `s` from `(x, y)` heading `hdg`, `length`
    /// long.
    pub fn new(s: f64, x: f64, y: f64, hdg: f64, length: f64, shape: GeomShape) -> Self {
        let samples = match shape {
            GeomShape::Spiral {
                curv_start,
                curv_end,
            } => bake_spiral(x, y, hdg, curv_start, curv_end, length),
            GeomShape::ParamPoly3 { u, v, p_max } => {
                bake_param_poly3(x, y, hdg, u, v, p_max, length)
            }
            GeomShape::Line | GeomShape::Arc { .. } => Vec::new(),
        };
        Self {
            s,
            x,
            y,
            hdg,
            length,
            shape,
            samples,
        }
    }

    /// Whether every pose along the record is finite. A curvature or
    /// coefficient near the limit of an `f64` can overflow to infinity.
    pub fn is_finite(&self) -> bool {
        let (x, y, h) = self.pose(self.s + self.length);
        let samples = self.samples.iter().flat_map(|&(s, x, y, h)| [s, x, y, h]);
        samples.chain([x, y, h]).all(f64::is_finite)
    }

    /// Position `(x, y)` and heading at road arc-length `s` within this record.
    pub fn pose(&self, s: f64) -> (f64, f64, f64) {
        let ds = s - self.s;
        match &self.shape {
            GeomShape::Arc { curvature } if curvature.abs() > 1e-9 => {
                let h = self.hdg + curvature * ds;
                let x = self.x + (h.sin() - self.hdg.sin()) / curvature;
                let y = self.y - (h.cos() - self.hdg.cos()) / curvature;
                (x, y, h)
            }
            GeomShape::Spiral { .. } | GeomShape::ParamPoly3 { .. } => {
                let samples = &self.samples;
                let max = samples.last().map(|p| p.0).unwrap_or(0.0);
                let ds = ds.clamp(0.0, max);
                let i = samples
                    .partition_point(|p| p.0 <= ds)
                    .saturating_sub(1)
                    .min(samples.len().saturating_sub(2));
                let (a_s, ax, ay, ah) = samples[i];
                let (b_s, bx, by, bh) = samples[i + 1];
                let t = if b_s > a_s {
                    (ds - a_s) / (b_s - a_s)
                } else {
                    0.0
                };
                (ax + (bx - ax) * t, ay + (by - ay) * t, ah + (bh - ah) * t)
            }
            // A line, or a degenerate (straight) arc.
            _ => (
                self.x + ds * self.hdg.cos(),
                self.y + ds * self.hdg.sin(),
                self.hdg,
            ),
        }
    }
}

/// Bake a clothoid to fine `(ds, x, y, hdg)` samples. Curvature varies linearly
/// `curv_start -> curv_end` over `length`, so heading is the closed form
/// `hdg0 + curv_start*u + (c_dot/2)*u^2`; position is its running integral,
/// which has no elementary form, so integrate cos/sin(heading) by the midpoint
/// rule at a fine step (mm-accurate over hundreds of meters). Always at
/// least two samples, so a spiral of no length still has a pose.
fn bake_spiral(
    x0: f64,
    y0: f64,
    hdg0: f64,
    curv_start: f64,
    curv_end: f64,
    length: f64,
) -> Vec<(f64, f64, f64, f64)> {
    const STEP: f64 = 0.25;
    let c_dot = if length > 1e-9 {
        (curv_end - curv_start) / length
    } else {
        0.0
    };
    let heading = |u: f64| hdg0 + curv_start * u + 0.5 * c_dot * u * u;
    let (mut x, mut y, mut u) = (x0, y0, 0.0);
    let mut out = vec![(0.0, x0, y0, hdg0)];
    while u < length - 1e-9 {
        let step = STEP.min(length - u);
        let theta_mid = heading(u + step / 2.0);
        x += theta_mid.cos() * step;
        y += theta_mid.sin() * step;
        u += step;
        out.push((u, x, y, heading(u)));
    }
    if out.len() == 1 {
        out.push((0.0, x0, y0, hdg0));
    }
    out
}

/// Evaluate `a + b*p + c*p^2 + d*p^3`.
pub(crate) fn eval_poly3([a, b, c, d]: [f64; 4], p: f64) -> f64 {
    a + b * p + c * p * p + d * p * p * p
}

/// Bake a paramPoly3 to `(ds, x, y, hdg)` samples. The curve is given in a local
/// `(u, v)` frame (u forward, v left of `hdg0`) by two cubics in a parameter
/// `p`; `p_max` is `1` for `pRange="normalized"`, else the geometry length.
/// Because `p` is not arc length, we step `p` finely, transform each point into
/// world space, and accumulate the true arc length `ds` so `pose` can sample by
/// distance like every other geometry.
fn bake_param_poly3(
    x0: f64,
    y0: f64,
    hdg0: f64,
    u: [f64; 4],
    v: [f64; 4],
    p_max: f64,
    length: f64,
) -> Vec<(f64, f64, f64, f64)> {
    let (sin, cos) = hdg0.sin_cos();
    let world = |p: f64| {
        let (uu, vv) = (eval_poly3(u, p), eval_poly3(v, p));
        (x0 + uu * cos - vv * sin, y0 + uu * sin + vv * cos)
    };
    let heading = |p: f64| {
        let du = u[1] + 2.0 * u[2] * p + 3.0 * u[3] * p * p;
        let dv = v[1] + 2.0 * v[2] * p + 3.0 * v[3] * p * p;
        hdg0 + dv.atan2(du)
    };
    // ~0.25 m resolution, from the geometry length.
    let n = ((length / 0.25).ceil() as usize).max(8);
    let (mut px, mut py) = world(0.0);
    let (mut ds, mut h) = (0.0, heading(0.0));
    let mut out = vec![(0.0, px, py, h)];
    for k in 1..=n {
        let p = p_max * (k as f64) / (n as f64);
        let (x, y) = world(p);
        ds += ((x - px).powi(2) + (y - py).powi(2)).sqrt();
        h += turn(heading(p) - h);
        out.push((ds, x, y, h));
        (px, py) = (x, y);
    }
    out
}

/// `angle` wrapped into `[-π, π)`, so consecutive headings interpolate the
/// short way round.
fn turn(angle: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    (angle + PI).rem_euclid(TAU) - PI
}

/// The geometry, of `geoms` sorted by start, that `s` falls in: the last
/// to start at or before it, or else the first. A binary search, as a
/// converter's plan view can hold a record every few metres.
pub(crate) fn geom_at(geoms: &[GeomRec], s: f64) -> &GeomRec {
    let i = geoms.partition_point(|g| g.s <= s + 1e-9);
    &geoms[i.saturating_sub(1)]
}

// --- Profiles -----------------------------------------------------------------

/// One cubic record (elevation, or a lane width), evaluated relative to its
/// own start offset: `a + b*ds + c*ds^2 + d*ds^3`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Cubic {
    pub start: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
}

impl Cubic {
    pub fn eval(&self, s: f64) -> f64 {
        let ds = s - self.start;
        self.a + self.b * ds + self.c * ds * ds + self.d * ds * ds * ds
    }

    /// The record as it reads `by` metres further along its lane section,
    /// for a section that starts there: the same curve, from an `sOffset`
    /// `by` less. One that starts before the new section starts at 0, as the
    /// curve stands there.
    pub fn moved_on(&self, by: f64) -> Cubic {
        let start = self.start - by;
        if start >= 0.0 {
            return Cubic {
                start,
                ..self.clone()
            };
        }
        let at = -start;
        Cubic {
            start: 0.0,
            a: self.eval(self.start + at),
            b: self.slope(self.start + at),
            c: self.c + 3.0 * self.d * at,
            d: self.d,
        }
    }

    /// The rate of change at `s`.
    pub fn slope(&self, s: f64) -> f64 {
        let ds = s - self.start;
        self.b + 2.0 * self.c * ds + 3.0 * self.d * ds * ds
    }
}

/// The record whose start is the greatest not exceeding `s` (records sorted by
/// start). `None` if `s` precedes them all / the list is empty.
///
/// A scan, not a binary search, even though this runs once per profile per
/// sampled station. Real files keep these lists short. Across Town07's 234
/// roads the longest elevation, width and laneOffset lists are 13, 17 and a
/// handful of records, and at that size `partition_point` measured slower
/// than walking back from the end.
pub(crate) fn active(records: &[Cubic], s: f64) -> Option<&Cubic> {
    records.iter().rev().find(|r| r.start <= s + 1e-9)
}

/// The road's lateral shape at `(s, t)`: its height off the reference
/// plane there. 0 on a road with no `<shape>`.
///
/// Between two profiles the height at a `t` goes linearly along `s`, as the
/// spec has it. Before the first profile the first holds, and after the
/// last the last. The spec gives 0 before the first. Across a profile, the
/// last shape runs on to the road's edge, and before the first shape's `t`
/// its value there holds, flat.
pub(crate) fn shape_at(profiles: &[ShapeProfile], s: f64, t: f64) -> f64 {
    let across = |p: &ShapeProfile| active(&p.shapes, t).map_or(p.shapes[0].a, |c| c.eval(t));
    let next = profiles.partition_point(|p| p.s <= s + 1e-9);
    match (
        next.checked_sub(1).map(|i| &profiles[i]),
        profiles.get(next),
    ) {
        (None, None) => 0.0,
        (Some(p), None) | (None, Some(p)) => across(p),
        (Some(a), Some(b)) => {
            let f = (s - a.s) / (b.s - a.s);
            across(a) + (across(b) - across(a)) * f
        }
    }
}

/// A lane's height off the road at its inner and outer border, `s_lane`
/// metres into its section. 0 for a lane with no `<height>`.
///
/// The spec's rule for lane geometry holds a height until the next one. The
/// crate goes straight from one to the next instead, as libOpenDRIVE and
/// esmini read them, and holds the last after it. Before the first, the
/// first holds. The spec gives no height there, and libOpenDRIVE carries the
/// first ramp on backwards.
pub(crate) fn height_at(lane: &LaneGeom, s_lane: f64) -> (f64, f64) {
    let heights = &lane.heights;
    let next = heights.partition_point(|h| h.s_offset <= s_lane + 1e-9);
    match (next.checked_sub(1).map(|i| &heights[i]), heights.get(next)) {
        (None, None) => (0.0, 0.0),
        (Some(h), None) | (None, Some(h)) => (h.inner, h.outer),
        (Some(a), Some(b)) => {
            let f = (s_lane - a.s_offset) / (b.s_offset - a.s_offset);
            (
                a.inner + (b.inner - a.inner) * f,
                a.outer + (b.outer - a.outer) * f,
            )
        }
    }
}

// --- Lanes across the road ----------------------------------------------------

/// Where one lane lies across the road at one station: the `t` of its inner
/// and outer border, and its width between them.
#[derive(Clone, Copy)]
pub(crate) struct LaneBorders {
    pub inner: f64,
    pub outer: f64,
    pub width: f64,
}

/// Each lane of `side` across the road `s_lane` metres into its section,
/// from the center outward. `sign` is 1.0 for the left side and -1.0 for the
/// right, and `base` the `t` of the center lane, from `<laneOffset>`. Every
/// place that needs a lane's borders reads them here.
///
/// A lane is 0 wide before its first `<width>` or `<border>`, and where
/// either would make it narrower than 0. See [`LaneExtent`].
pub(crate) fn side_borders(
    side: &[LaneGeom],
    sign: f64,
    base: f64,
    s_lane: f64,
) -> impl Iterator<Item = LaneBorders> + '_ {
    side.iter().scan(base, move |t, lane| {
        let inner = *t;
        let width = match &lane.extent {
            LaneExtent::Width(widths) => active(widths, s_lane).map_or(0.0, |w| w.eval(s_lane)),
            LaneExtent::Border(borders) => {
                active(borders, s_lane).map_or(0.0, |b| sign * (b.eval(s_lane) - inner))
            }
        }
        .max(0.0);
        *t += sign * width;
        Some(LaneBorders {
            inner,
            outer: *t,
            width,
        })
    })
}

/// Each lane of `side`'s height off the road at its inner and outer border
/// at station `s`, `s_lane` into its section, from the center outward.
/// `lateral` is the road's lateral shape and cross-section surface.
/// `borders` is where each lies, from [`side_borders`], `sign` which way it
/// stacks, `base` the `t` of the center lane, and `phi` the superelevation.
///
/// A lane's heights are its `<height>`s plus the road's lateral shape under
/// each border. A lane with `level="true"`, and every lane outside it, is
/// kept out of the shape and the superelevation instead. It starts at its
/// inner neighbour's outer height, `<height>` and all, and runs level from
/// there, with its own `<height>`s on top. Measured along the road's tilted
/// normal, level is `w tan phi` lower at its outer border, which gives it a
/// bank of 0.
pub(crate) fn side_heights<'a>(
    side: &'a [LaneGeom],
    sign: f64,
    lateral: &'a Lateral,
    (s, s_lane): (f64, f64),
    base: f64,
    phi: f64,
    borders: impl Iterator<Item = LaneBorders> + 'a,
) -> impl Iterator<Item = (f64, f64)> + 'a {
    let start = (lateral.height_toward(s, base, sign), false);
    side.iter()
        .zip(borders)
        .scan(start, move |(outer, level), (lane, b)| {
            let (own_inner, own_outer) = height_at(lane, s_lane);
            *level |= lane.level;
            let heights = if *level {
                (
                    *outer + own_inner,
                    *outer - sign * b.width * phi.tan() + own_outer,
                )
            } else {
                (
                    own_inner + lateral.height_toward(s, b.inner, sign),
                    own_outer + lateral.height_toward(s, b.outer, sign),
                )
            };
            *outer = heights.1;
            Some(heights)
        })
}

/// One lane across the road at a station: the `t` of its inner and outer
/// border, and its height at each, lateral shape included.
pub(crate) struct Across {
    pub od_id: i32,
    pub inner_t: f64,
    pub outer_t: f64,
    pub inner_height: f64,
    pub outer_height: f64,
}

impl Across {
    /// The lane's height at `t`, straight from its inner border to its outer
    /// one, and on past them.
    pub fn height(&self, t: f64) -> f64 {
        let width = self.outer_t - self.inner_t;
        let f = if width == 0.0 {
            0.0
        } else {
            (t - self.inner_t) / width
        };
        self.inner_height + (self.outer_height - self.inner_height) * f
    }
}

impl RoadSection {
    /// Whether any of its lanes stands off the road's cross-section: has a
    /// `<height>`, or is level.
    pub fn raised(&self) -> bool {
        self.left
            .iter()
            .chain(&self.right)
            .any(|l| !l.heights.is_empty() || l.level)
    }
}

// --- The road surface ---------------------------------------------------------

/// How far `locate` steps, in metres, to measure how the surface moves.
const LOCATE_STEP: f64 = 1e-4;

/// When `locate` stops: after this many steps, or once a step moves `s` and
/// `t` less than this, in metres.
const LOCATE_STEPS: usize = 24;
const LOCATE_TOLERANCE: f64 = 1e-9;

impl Road {
    /// The reference line at `s`.
    pub(crate) fn station(&self, s: f64) -> RefPoint {
        let (x, y, heading) = geom_at(&self.geoms, s).pose(s);
        let elevation = active(&self.elevations, s);
        let bank = active(&self.superelevations, s);
        let (z, grade) = match &self.lateral.ground {
            Some(ground) => {
                const DS: f64 = 0.01;
                let at = |s: f64| ground.height(s, 0.0);
                (at(s), (at(s + DS) - at(s - DS)) / (2.0 * DS))
            }
            None => (
                elevation.map_or(0.0, |e| e.eval(s)),
                elevation.map_or(0.0, |e| e.slope(s)),
            ),
        };
        RefPoint {
            s,
            x,
            y,
            heading,
            z,
            grade,
            bank: bank.map_or(0.0, |e| e.eval(s)),
            bank_rate: bank.map_or(0.0, |e| e.slope(s)),
        }
    }

    /// The `t` of the center lane of the permanent layer at `s`, from
    /// `<laneOffset>`.
    pub(crate) fn base(&self, s: f64) -> f64 {
        self.base_of(Layer::Permanent, s)
    }

    /// The `<laneOffset>`s of `layer`.
    pub(crate) fn offsets_of(&self, layer: Layer) -> &[Cubic] {
        match layer {
            Layer::Permanent => &self.lane_offsets,
            Layer::Temporary => &self.temporary_lane_offsets,
        }
    }

    /// The `t` of the center lane of `layer` at `s`. The layers have their
    /// own `<laneOffset>`s.
    pub(crate) fn base_of(&self, layer: Layer, s: f64) -> f64 {
        active(self.offsets_of(layer), s).map_or(0.0, |o| o.eval(s))
    }

    /// The surface at a station `(s, t)`, on the lane there, and the
    /// reference line's heading. A point on the border between two lanes is
    /// on the inner one, as libOpenDRIVE finds it and as a road mark there
    /// belongs to it. A point past the outermost lane takes that lane's outer
    /// height. libOpenDRIVE carries the lane's slope on instead. A point on
    /// no lane takes the lateral shape under it.
    pub(crate) fn surface(&self, s: f64, t: f64) -> (Point, f64) {
        let (p, hdg) = self.surface_xyz(s, t);
        (to_point(p), hdg)
    }

    /// [`Self::surface`] in `f64`.
    fn surface_xyz(&self, s: f64, t: f64) -> ([f64; 3], f64) {
        let height = self
            .section_at(s)
            .filter(|section| section.raised() || !self.lateral.is_empty())
            .and_then(|section| self.lane_at(section, s, t))
            .map_or_else(
                || self.lateral.height(s, t),
                |a| a.height(t.clamp(a.inner_t.min(a.outer_t), a.inner_t.max(a.outer_t))),
            );
        self.raised_xyz(s, t, height)
    }

    /// The lane of `section` at `(s, t)`: the outermost on the side of `t`
    /// whose inner border `t` lies beyond. So a point on the border between
    /// two lanes is on the inner one, and a point past the outermost lane on
    /// that lane. `None` on the center line, or on a side with no lanes.
    pub(crate) fn lane_at(&self, section: &RoadSection, s: f64, t: f64) -> Option<Across> {
        self.across(section, s)
            .filter(|a| f64::from(a.od_id.signum()) * (t - a.inner_t) > 0.0)
            .last()
    }

    /// The surface of the lane `od_id` of `section` at `(s, t)`, straight
    /// across the lane and on past its borders, so paint on its border lies
    /// in it. For the center lane, the lateral shape on the center line, held
    /// flat across, so its paint lies on a crown's ridge rather than under it.
    pub(crate) fn lane_surface(
        &self,
        section: &RoadSection,
        od_id: i32,
        s: f64,
        t: f64,
    ) -> (Point, f64) {
        if !section.raised() && self.lateral.is_empty() {
            return self.raised(s, t, 0.0);
        }
        let height = self
            .across(section, s)
            .find(|a| a.od_id == od_id)
            .map_or_else(
                || self.lateral.height(s, self.base_of(section.layer, s)),
                |a| a.height(t),
            );
        self.raised(s, t, height)
    }

    /// The road surface at `(s, t)`, stood off along the road's normal there
    /// by `height`, and the reference line's heading.
    pub(crate) fn raised(&self, s: f64, t: f64, height: f64) -> (Point, f64) {
        let (p, hdg) = self.raised_xyz(s, t, height);
        (to_point(p), hdg)
    }

    /// [`Self::raised`] in `f64`.
    ///
    /// Superelevation rolls the cross-section about the reference line by
    /// `phi`, so a point at `t` rides up by `t sin phi` and reaches `t cos
    /// phi` across in plan.
    fn raised_xyz(&self, s: f64, t: f64, height: f64) -> ([f64; 3], f64) {
        let (x, y, hdg) = geom_at(&self.geoms, s).pose(s);
        let elev = active(&self.elevations, s).map_or(0.0, |e| e.eval(s));
        let phi = active(&self.superelevations, s).map_or(0.0, |e| e.eval(s));
        let (sin_phi, cos_phi) = phi.sin_cos();
        let t_h = t * cos_phi;
        let point = [x - t_h * hdg.sin(), y + t_h * hdg.cos(), elev + t * sin_phi];
        if height == 0.0 {
            return (point, hdg);
        }
        let up = self.axes(s)[2];
        ([0, 1, 2].map(|i| point[i] + up[i] * height), hdg)
    }

    /// The station `(s, t)` whose surface lies over `(x, y)`, searched from
    /// `s`, and held within the road's ends.
    ///
    /// It solves on the surface itself by Newton's method, so a lane height
    /// or a lateral shape that stands the surface off along a tilted normal
    /// moves the answer as it moves the point.
    pub(crate) fn locate(&self, x: f64, y: f64, s: f64) -> (f64, f64) {
        let miss = |s: f64, t: f64| {
            let (p, _) = self.surface_xyz(s, t);
            [p[0] - x, p[1] - y]
        };
        let mut s = s.clamp(0.0, self.length);
        let (rx, ry, hdg) = geom_at(&self.geoms, s).pose(s);
        let phi = active(&self.superelevations, s).map_or(0.0, |e| e.eval(s));
        let mut t = ((y - ry) * hdg.cos() - (x - rx) * hdg.sin()) / phi.cos();
        for _ in 0..LOCATE_STEPS {
            let r = miss(s, t);
            if r[0].hypot(r[1]) < LOCATE_TOLERANCE {
                break;
            }
            let ahead = if s + LOCATE_STEP <= self.length {
                LOCATE_STEP
            } else {
                -LOCATE_STEP
            };
            let a = miss(s + ahead, t);
            let along = [0, 1].map(|i| (a[i] - r[i]) / ahead);
            let a = miss(s, t + LOCATE_STEP);
            let across = [0, 1].map(|i| (a[i] - r[i]) / LOCATE_STEP);
            let det = along[0] * across[1] - along[1] * across[0];
            if det.abs() < 1e-12 {
                break;
            }
            let ds = (r[1] * across[0] - r[0] * across[1]) / det;
            let dt = (r[0] * along[1] - r[1] * along[0]) / det;
            let next = (s + ds).clamp(0.0, self.length);
            let moved = (next - s).abs() + dt.abs();
            (s, t) = (next, t + dt);
            if moved < LOCATE_TOLERANCE {
                break;
            }
        }
        (s, t)
    }

    /// The lane section with this index among the road's `<laneSection>`s.
    pub(crate) fn section(&self, index: usize) -> Option<&RoadSection> {
        self.sections.iter().find(|section| section.index == index)
    }

    /// The permanent layer's lane section at `s`. On a boundary, the one
    /// starting there. The permanent layer defines the surface.
    pub(crate) fn section_at(&self, s: f64) -> Option<&RoadSection> {
        let mut permanent = self
            .sections
            .iter()
            .filter(|section| section.layer == Layer::Permanent);
        permanent
            .clone()
            .rev()
            .find(|section| section.start <= s + 1e-9)
            .or(permanent.next())
    }

    /// The lane section of `layer` that covers `s`, if there is one. On a
    /// boundary, the one starting there.
    pub(crate) fn section_of(&self, layer: Layer, s: f64) -> Option<&RoadSection> {
        self.sections
            .iter()
            .rev()
            .filter(|section| section.layer == layer)
            .find(|section| section.start <= s + 1e-9 && s <= section.end + 1e-9)
    }

    /// Each lane of `section` across the road at `s`, from the center
    /// outward, the left side first.
    pub(crate) fn across<'a>(
        &'a self,
        section: &'a RoadSection,
        s: f64,
    ) -> impl Iterator<Item = Across> + 'a {
        let base = self.base_of(section.layer, s);
        let s_lane = s - section.start;
        let phi = active(&self.superelevations, s).map_or(0.0, |e| e.eval(s));
        [(&section.left, 1.0), (&section.right, -1.0)]
            .into_iter()
            .flat_map(move |(side, sign)| {
                let borders: Vec<LaneBorders> = side_borders(side, sign, base, s_lane).collect();
                let heights: Vec<(f64, f64)> = side_heights(
                    side,
                    sign,
                    &self.lateral,
                    (s, s_lane),
                    base,
                    phi,
                    borders.iter().copied(),
                )
                .collect();
                side.iter().zip(borders).zip(heights).map(
                    |((lane, borders), (inner_height, outer_height))| Across {
                        od_id: lane.id,
                        inner_t: borders.inner,
                        outer_t: borders.outer,
                        inner_height,
                        outer_height,
                    },
                )
            })
    }

    /// The road's own axes at station `s`, as libOpenDRIVE has them: along
    /// the reference line up its grade, across it tilted by the
    /// superelevation, and square to both. Unit length and square to each
    /// other, right-handed.
    pub(crate) fn axes(&self, s: f64) -> [[f64; 3]; 3] {
        let (_, _, hdg) = geom_at(&self.geoms, s).pose(s);
        let grade = active(&self.elevations, s).map_or(0.0, |e| e.slope(s));
        let phi = active(&self.superelevations, s).map_or(0.0, |e| e.eval(s));
        let (sin_h, cos_h) = hdg.sin_cos();
        let (sin_p, cos_p) = phi.sin_cos();
        let along = unit([cos_h, sin_h, grade]);
        let up = unit(cross(along, [-sin_h * cos_p, cos_h * cos_p, sin_p]));
        [along, cross(up, along), up]
    }

    pub(crate) fn on_road(&self, s: f64) -> bool {
        (0.0..=self.length).contains(&s)
    }

    /// The lanes alongside the stretch `[from, to]`, on either layer, whose
    /// `<lane id>` and layer are in one of the `validity` ranges, or all of
    /// them if there are no ranges. A section starting exactly at `to`
    /// counts only if the stretch is a single station, so a station on a
    /// section boundary is in the section that starts there.
    pub(crate) fn lanes(&self, (from, to): (f64, f64), validity: &[Validity]) -> Vec<LaneId> {
        self.sections
            .iter()
            .filter(|sec| {
                let ends_the_road = sec.end >= self.length - 1e-9;
                let starts = sec.start < to || (from == to && sec.start <= to);
                starts && (from < sec.end || (ends_the_road && from <= sec.end))
            })
            .flat_map(|sec| sec.lanes.iter().map(move |lane| (sec.layer, lane)))
            .filter(|(layer, (od_id, _))| is_valid(*od_id, *layer, validity))
            .map(|(_, &(_, id))| id)
            .collect()
    }
}

/// How far along a lane road station `s` is, in metres from its first
/// point. `points` are the lane's centerline, sampled at the road
/// `stations`, one each.
pub(crate) fn along(points: &[Point], stations: &[f64], s: f64) -> f32 {
    debug_assert_eq!(points.len(), stations.len());
    let i = stations
        .partition_point(|&x| x <= s)
        .saturating_sub(1)
        .min(stations.len().saturating_sub(2));
    let before: f64 = points[..=i]
        .windows(2)
        .map(|w| f64::from(w[0].distance_to(w[1])))
        .sum();
    let f = ((s - stations[i]) / (stations[i + 1] - stations[i])).clamp(0.0, 1.0);
    before as f32 + points[i].distance_to(points[i + 1]) * f as f32
}

/// The road station `arc` metres along a lane from its first point, the
/// inverse of [`along`]. `points` are the lane's centerline, sampled at the
/// road `stations`, one each.
pub(crate) fn station_at(points: &[Point], stations: &[f64], arc: f32) -> f64 {
    debug_assert_eq!(points.len(), stations.len());
    let last = points.len() - 2;
    let arc = f64::from(arc);
    let mut before = 0.0;
    for (i, w) in points.windows(2).enumerate() {
        let step = f64::from(w[0].distance_to(w[1]));
        if before + step >= arc || i == last {
            let f = if step > 0.0 {
                ((arc - before) / step).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return stations[i] + (stations[i + 1] - stations[i]) * f;
        }
        before += step;
    }
    stations[0]
}

/// A `<validity>`: the lanes `fromLane` to `toLane` on a layer.
pub(crate) type Validity = (i32, i32, Layer);

/// Whether the lane `od_id` on `layer` is in one of the `validity` ranges,
/// or there are none.
pub(crate) fn is_valid(od_id: i32, layer: Layer, validity: &[Validity]) -> bool {
    validity.is_empty()
        || validity
            .iter()
            .any(|&(a, b, l)| l == layer && (a.min(b)..=a.max(b)).contains(&od_id))
}

fn to_point([x, y, z]: [f64; 3]) -> Point {
    Point::new(x as f32, y as f32, z as f32)
}

fn cross([a, b, c]: [f64; 3], [x, y, z]: [f64; 3]) -> [f64; 3] {
    [b * z - c * y, c * x - a * z, a * y - b * x]
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|c| c * c).sum::<f64>().sqrt();
    v.map(|c| c / length)
}
