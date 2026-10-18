//! The baked road-network model: lanes, their connectivity graph, and the
//! queries over both. Format-agnostic: nothing here knows OpenDRIVE.

use std::collections::HashMap;
use std::ops::Range;

use crate::along::{self, Access, Along, RoadType, SpeedLimit, Visibility};
use crate::coords::Point;
use crate::crg::CrgSurface;
use crate::geo::GeoReference;
use crate::geometry::{Polyline, Projection, RoadSample};
use crate::grid::{Aabb, Grid};
use crate::header::Header;
use crate::junction::{CrossPath, Crossing, JunctionArea, JunctionGroup, VirtualJunction};
use crate::object::{Material, Object, ObjectId};
use crate::railway::{Station, Switch};
use crate::road::{LanePosition, Priority, Road, RoadId, RoadLane, RoadNeighbor, RoadPosition};
use crate::road_mark::{RoadMark, RoadMarkId};
use crate::signal::{Controller, ControllerId, Signal, SignalId, VmsGroup};
use crate::structure::{Coverage, Structure, StructureId};

/// An opaque lane identifier. **Not** a vector index into `RoadNetwork.lanes`.
/// An importer may assign arbitrary ids, such as OpenDRIVE lane keys, so look
/// lanes up with [`RoadNetwork::lane`], never by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LaneId(pub usize);

/// What a lane is for.
///
/// The set mirrors the lane functions a road cross-section distinguishes,
/// because that is the vocabulary the maps are written in, and collapsing it
/// would throw away the only thing that tells a sidewalk from a bus lane.
///
/// Two questions get asked of a lane type often enough to answer here rather
/// than at every call site: whether through traffic belongs on it
/// ([`LaneType::is_drivable`]) and what to call it ([`LaneType::as_str`]).
/// Everything else is the consumer's policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LaneType {
    /// Surface inside the road with no function assigned to it: the paved area
    /// a cross-section has to account for but does not name. Not a gap. It has
    /// a width and a surface like any other lane, and a map that leaves it out
    /// has holes in it.
    None,
    /// An ordinary traffic lane.
    Driving,
    /// One lane carrying traffic both ways, such as a centre turn lane.
    Bidirectional,
    /// Reserved for buses.
    Bus,
    /// Reserved for taxis.
    Taxi,
    /// Reserved for high-occupancy vehicles.
    Hov,
    /// An acceleration lane joining a road.
    Entry,
    /// A deceleration lane leaving a road.
    Exit,
    /// A ramp onto a motorway.
    OnRamp,
    /// A ramp off a motorway.
    OffRamp,
    /// A ramp linking two motorways.
    ConnectingRamp,
    /// A short bypass lane at an intersection, usually for turning traffic.
    SlipLane,
    /// A lane vehicles park in.
    Parking,
    /// A hard shoulder for emergency stops.
    Stop,
    /// A lane traffic may not use, such as a painted gore area.
    Restricted,
    /// A cycle lane.
    Biking,
    /// A footway.
    Sidewalk,
    /// A lane shared by all traffic participants, vehicles and pedestrians
    /// alike. The spec says to use an `<access>` for a lane shared by
    /// walkers and bikes only.
    Shared,
    /// Hard shoulder, outboard of the running lanes.
    Shoulder,
    /// The paved strip between a running lane and whatever is beside it.
    Border,
    /// The kerb between the carriageway and the footway.
    Curb,
    /// The strip separating opposing carriageways.
    Median,
    /// A lane closed for works.
    RoadWorks,
    /// Tram track sharing the road surface.
    Tram,
    /// Railway track.
    Rail,
    /// Vendor-defined surface. The format reserves three of these without
    /// saying what they are for.
    Special1,
    /// Vendor-defined surface. See [`LaneType::Special1`].
    Special2,
    /// Vendor-defined surface. See [`LaneType::Special1`].
    Special3,
    /// A surface whose declared type this crate does not recognise.
    ///
    /// A lane has geometry whether or not its type means anything here, so it
    /// bakes as one of these rather than being dropped. Dropping it left holes
    /// in the road surface instead, which is harder to notice than a lane of
    /// the wrong colour.
    Unknown,
}

impl LaneType {
    /// Whether ordinary through traffic belongs on this lane.
    ///
    /// This is the routing predicate. It decides which lanes a vehicle may
    /// change into, so it is deliberately narrower than "has a paved surface":
    /// a parking lane and a hard shoulder are drivable in the everyday sense
    /// and are excluded, because a route that ran through them would be wrong.
    pub fn is_drivable(&self) -> bool {
        matches!(
            self,
            Self::Driving
                | Self::Bidirectional
                | Self::Bus
                | Self::Taxi
                | Self::Hov
                | Self::Entry
                | Self::Exit
                | Self::OnRamp
                | Self::OffRamp
                | Self::ConnectingRamp
                | Self::SlipLane
                | Self::Shared
        )
    }

    /// A stable lowercase name, for a legend, a log line, or a viewer readout.
    /// Format-neutral, so it is not necessarily the token any particular map
    /// format spells the type with.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Driving => "driving",
            Self::Bidirectional => "bidirectional",
            Self::Bus => "bus",
            Self::Taxi => "taxi",
            Self::Hov => "hov",
            Self::Entry => "entry",
            Self::Exit => "exit",
            Self::OnRamp => "on-ramp",
            Self::OffRamp => "off-ramp",
            Self::ConnectingRamp => "connecting-ramp",
            Self::SlipLane => "slip-lane",
            Self::Parking => "parking",
            Self::Stop => "stop",
            Self::Restricted => "restricted",
            Self::Biking => "biking",
            Self::Sidewalk => "sidewalk",
            Self::Shared => "shared",
            Self::Shoulder => "shoulder",
            Self::Border => "border",
            Self::Curb => "curb",
            Self::Median => "median",
            Self::RoadWorks => "road-works",
            Self::Tram => "tram",
            Self::Rail => "rail",
            Self::Special1 => "special1",
            Self::Special2 => "special2",
            Self::Special3 => "special3",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for LaneType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The lane layer a lane is on, from its `<lanes layer>`, or the permanent
/// layer for a road whose only `<lanes>` is a temporary one.
///
/// A road has a permanent layer along its whole length, and may have a
/// temporary one over parts of it, such as for road works. The temporary
/// layer's lanes take precedence for driving. Both layers share one surface,
/// which only the permanent layer defines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Layer {
    /// The road's lasting lanes. What a missing or unrecognised `layer`
    /// reads as.
    #[default]
    Permanent,
    /// Lanes laid over the permanent ones for a while.
    Temporary,
}

impl Layer {
    /// The layer a `layer` attribute names. A missing or unrecognised one is
    /// the permanent layer, as the spec says.
    pub(crate) fn from_attr(name: Option<&str>) -> Self {
        match name {
            Some("temporary") => Self::Temporary,
            _ => Self::Permanent,
        }
    }

    /// `permanent` or `temporary`, as the spec spells them.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Permanent => "permanent",
            Self::Temporary => "temporary",
        }
    }
}

/// Which lane layers a query looks at, where a road has two.
///
/// The temporary layer's lanes take precedence for driving, so by default a
/// query that finds a place on the road, or a way along it, may use them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LayerPolicy {
    /// Both layers. Where a temporary driving lane holds a place it is the
    /// lane found there, and the lane graph holds both layers' lanes.
    #[default]
    Both,
    /// The permanent layer only, as if the map had no temporary lanes.
    PermanentOnly,
}

impl LayerPolicy {
    /// Whether a query under this policy may use `lane`.
    pub fn admits(&self, lane: &Lane) -> bool {
        match self {
            Self::Both => true,
            Self::PermanentOnly => lane.layer == Layer::Permanent,
        }
    }
}

/// Whether traffic from the lanes beside a lane may also use it, from a
/// `<lane>`'s `advisory`. A bike lane that cars may also use is one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LaneAdvisory {
    /// Only its own traffic uses it. What a missing or unrecognised value
    /// reads as.
    #[default]
    None,
    /// Traffic from the lane on its inner side may use it.
    Inner,
    /// Traffic from the lane on its outer side may use it.
    Outer,
    /// Traffic from both neighbouring lanes may use it.
    Both,
}

impl LaneAdvisory {
    /// A stable lowercase name, as the spec spells it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Inner => "inner",
            Self::Outer => "outer",
            Self::Both => "both",
        }
    }
}

/// Travel direction of a lane relative to its geometry's start→end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    /// Travel runs along the centerline, start to end.
    Forward,
    /// Travel runs against it, end to start.
    Backward,
    /// Travel runs either way, as on a single-track road. Its
    /// [`Lane::successors`] are the lanes off both ends, and its
    /// [`Lane::predecessors`] those that drive into it at either end.
    Both,
}

/// One lane: a strip of road surface described by its centerline and width.
/// Not necessarily drivable; a sidewalk and a median are lanes too, and
/// [`Lane::kind`] is what separates them.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lane {
    /// This lane's identity. Not a position in any list.
    pub id: LaneId,
    /// What the lane is for.
    pub kind: LaneType,
    /// Which lane layer it is on. A lane on the temporary layer is not part
    /// of the road surface. [`RoadNetwork::surface_mesh`] leaves it out.
    #[cfg_attr(feature = "serde", serde(default))]
    pub layer: Layer,
    /// Which way traffic runs along `center`.
    pub direction: Direction,
    /// Lane centerline.
    pub center: Polyline,
    /// Nominal lane width (metres), meaning the widest this lane gets.
    ///
    /// The whole width of a lane that holds one, and the full-section width of
    /// a lane that tapers. This names the lane, a 3.5 m lane. It is not the
    /// width at a given station; [`Lane::width_at`] answers that, and the
    /// tessellator uses it.
    ///
    /// A lane whose borders stand at different heights, from its `<height>`s
    /// or the road's `<shape>`s, is measured across its surface rather than
    /// in plan: `sqrt(w² + Δh²)`. That is 1.1 mm more on a 3.5 m lane at
    /// 2.5 %, and 15 cm more at 30 %.
    pub width: f32,
    /// Per-centerline-vertex width (metres), parallel to `center.points()`.
    ///
    /// Empty means a lane of constant `width`, which covers most of them. A
    /// non-empty profile has `center.points().len()` entries. A shorter one
    /// holds its last value.
    /// A gore area at an off-ramp is what this exists for. It is 0 m wide
    /// where it begins and 5 m wide further along, and a single width put it
    /// at neither. Each is measured across the surface, as `width` is.
    pub widths: Vec<f32>,
    /// Per-centerline-vertex cross-slope angle (radians, signed), parallel to
    /// `center.points()`: the road's superelevation, plus the slope from the
    /// lane's inner border to its outer one that its `<height>`s and the
    /// road's lateral `<shape>`s give it.
    /// Positive raises the **+offset** edge, the left-hand
    /// normal of the centerline's *stored* tangent (its geometry direction), which
    /// for a `Backward` lane is opposite its travel direction. Consumers deriving
    /// a surface normal must roll about `center.tangents()`, not travel, or a
    /// backward lane's normal disagrees with its own (correct) baked heights.
    /// Empty means a flat lane (bank ≡ 0); a non-empty profile has
    /// `center.points().len()` entries, and a shorter one holds its last value. The centerline points already carry the
    /// banked *height* (reference-line pivot) and any lane height; this angle
    /// is the surface tilt that the mesh cant and [`Lane::sample_at`] read.
    pub bank: Vec<f32>,
    /// Lanes reachable by driving off this lane's exit (travel-direction) end.
    /// May fan out (a junction) or be empty (a dead end / unlinked lane). Built
    /// by an importer from road/lane links and junctions; empty otherwise.
    pub successors: Vec<LaneId>,
    /// Lanes that drive into this lane, the reverse of `successors`.
    pub predecessors: Vec<LaneId>,
    /// Adjacent same-section, same-direction lanes you can change into (lateral
    /// lane-change edges). Empty if there's no neighbor to change to. A lane
    /// running the other way is never one, and a [`Direction::Both`] lane is
    /// only one of another.
    pub neighbors: Vec<LaneId>,
}

/// The "compiled map": everything a consumer needs, baked and format-agnostic.
///
/// Immutable once built. The lane list is private so the network can derive
/// lookup structures from it in [`RoadNetwork::new`] without any way for them
/// to go stale. A map that changed under its own index would answer
/// `nearest_lane` with a lane that is no longer there.
///
/// Serializes as its lanes, objects, structures, signals, controllers, road
/// marks, CRG surfaces, what holds along each lane, geo reference and roads;
/// the indexes are rebuilt on the way back in, so a network that crossed a
/// process boundary is indistinguishable from one that was just imported.
#[derive(Debug, Clone, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(into = "NetworkData", from = "NetworkData")
)]
pub struct RoadNetwork {
    lanes: Vec<Lane>,
    objects: Vec<Object>,
    structures: Vec<Structure>,
    signals: Vec<Signal>,
    controllers: Vec<Controller>,
    vms_groups: Vec<VmsGroup>,
    road_marks: Vec<RoadMark>,
    crg: Vec<CrgSurface>,
    speed_limits: Vec<Along<SpeedLimit>>,
    road_types: Vec<Along<RoadType>>,
    road_countries: Vec<Along<String>>,
    lane_rules: Vec<Along<String>>,
    lane_access: Vec<Along<Access>>,
    lane_materials: Vec<Along<Material>>,
    lane_visibility: Vec<Along<Visibility>>,
    geo: GeoReference,
    header: Header,
    roads: Vec<Road>,
    priorities: Vec<Priority>,
    road_neighbors: Vec<RoadNeighbor>,
    junction_areas: Vec<JunctionArea>,
    cross_paths: Vec<CrossPath>,
    junction_groups: Vec<JunctionGroup>,
    crossings: Vec<Crossing>,
    virtual_junctions: Vec<VirtualJunction>,
    switches: Vec<Switch>,
    stations: Vec<Station>,
    /// Lanes of kind [`LaneType::Driving`] bucketed by their XY footprint, for
    /// [`Self::nearest_lane`]. Derived from `lanes`, so it takes no part in
    /// equality.
    index: LaneIndex,
    /// As `index`, for the temporary layer's lanes.
    temporary_index: LaneIndex,
    /// The temporary layer's lanes that close the lanes under them.
    closing_index: LaneIndex,
    /// Each lane's place on its road, from `roads`.
    road_lanes: HashMap<LaneId, RoadLane>,
    /// Every lane's surface bucketed by its XY footprint, for
    /// [`Self::road_position`]. Empty without roads.
    footprints: LaneIndex,
}

/// Two networks are equal when their lanes, objects, structures, signals,
/// controllers, road marks, CRG surfaces, what holds along each lane, geo
/// references and roads are; the indexes are functions of those.
impl PartialEq for RoadNetwork {
    fn eq(&self, other: &Self) -> bool {
        self.lanes == other.lanes
            && self.objects == other.objects
            && self.structures == other.structures
            && self.signals == other.signals
            && self.controllers == other.controllers
            && self.vms_groups == other.vms_groups
            && self.road_marks == other.road_marks
            && self.crg == other.crg
            && self.speed_limits == other.speed_limits
            && self.road_types == other.road_types
            && self.road_countries == other.road_countries
            && self.lane_rules == other.lane_rules
            && self.lane_access == other.lane_access
            && self.lane_materials == other.lane_materials
            && self.lane_visibility == other.lane_visibility
            && self.geo == other.geo
            && self.header == other.header
            && self.roads == other.roads
            && self.priorities == other.priorities
            && self.road_neighbors == other.road_neighbors
            && self.junction_areas == other.junction_areas
            && self.cross_paths == other.cross_paths
            && self.junction_groups == other.junction_groups
            && self.crossings == other.crossings
            && self.virtual_junctions == other.virtual_junctions
            && self.switches == other.switches
            && self.stations == other.stations
    }
}

/// What a [`RoadNetwork`] serializes as: its content without the index.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
struct NetworkData {
    lanes: Vec<Lane>,
    objects: Vec<Object>,
    structures: Vec<Structure>,
    signals: Vec<Signal>,
    controllers: Vec<Controller>,
    #[serde(default)]
    vms_groups: Vec<VmsGroup>,
    road_marks: Vec<RoadMark>,
    crg: Vec<CrgSurface>,
    speed_limits: Vec<Along<SpeedLimit>>,
    road_types: Vec<Along<RoadType>>,
    #[serde(default)]
    road_countries: Vec<Along<String>>,
    #[serde(default)]
    lane_rules: Vec<Along<String>>,
    #[serde(default)]
    lane_access: Vec<Along<Access>>,
    #[serde(default)]
    lane_materials: Vec<Along<Material>>,
    #[serde(default)]
    lane_visibility: Vec<Along<Visibility>>,
    #[serde(default)]
    geo: GeoReference,
    #[serde(default)]
    header: Header,
    #[serde(default)]
    roads: Vec<Road>,
    #[serde(default)]
    priorities: Vec<Priority>,
    #[serde(default)]
    road_neighbors: Vec<RoadNeighbor>,
    #[serde(default)]
    junction_areas: Vec<JunctionArea>,
    #[serde(default)]
    cross_paths: Vec<CrossPath>,
    #[serde(default)]
    junction_groups: Vec<JunctionGroup>,
    #[serde(default)]
    crossings: Vec<Crossing>,
    #[serde(default)]
    virtual_junctions: Vec<VirtualJunction>,
    #[serde(default)]
    switches: Vec<Switch>,
    #[serde(default)]
    stations: Vec<Station>,
}

#[cfg(feature = "serde")]
impl From<NetworkData> for RoadNetwork {
    fn from(data: NetworkData) -> Self {
        Self::new(data.lanes)
            .with_objects(data.objects)
            .with_structures(data.structures)
            .with_signals(data.signals)
            .with_controllers(data.controllers)
            .with_vms_groups(data.vms_groups)
            .with_road_marks(data.road_marks)
            .with_crg_surfaces(data.crg)
            .with_speed_limits(data.speed_limits)
            .with_road_types(data.road_types)
            .with_road_countries(data.road_countries)
            .with_lane_rules(data.lane_rules)
            .with_lane_access(data.lane_access)
            .with_lane_materials(data.lane_materials)
            .with_lane_visibility(data.lane_visibility)
            .with_geo_reference(data.geo)
            .with_header(data.header)
            .with_roads(data.roads)
            .with_priorities(data.priorities)
            .with_road_neighbors(data.road_neighbors)
            .with_junction_areas(data.junction_areas)
            .with_cross_paths(data.cross_paths)
            .with_junction_groups(data.junction_groups)
            .with_crossings(data.crossings)
            .with_virtual_junctions(data.virtual_junctions)
            .with_railways(data.switches, data.stations)
    }
}

#[cfg(feature = "serde")]
impl From<RoadNetwork> for NetworkData {
    fn from(net: RoadNetwork) -> Self {
        Self {
            lanes: net.lanes,
            objects: net.objects,
            structures: net.structures,
            signals: net.signals,
            controllers: net.controllers,
            vms_groups: net.vms_groups,
            road_marks: net.road_marks,
            crg: net.crg,
            speed_limits: net.speed_limits,
            road_types: net.road_types,
            road_countries: net.road_countries,
            lane_rules: net.lane_rules,
            lane_access: net.lane_access,
            lane_materials: net.lane_materials,
            lane_visibility: net.lane_visibility,
            geo: net.geo,
            header: net.header,
            roads: net.roads,
            priorities: net.priorities,
            road_neighbors: net.road_neighbors,
            junction_areas: net.junction_areas,
            cross_paths: net.cross_paths,
            junction_groups: net.junction_groups,
            crossings: net.crossings,
            virtual_junctions: net.virtual_junctions,
            switches: net.switches,
            stations: net.stations,
        }
    }
}

impl From<Vec<Lane>> for RoadNetwork {
    fn from(lanes: Vec<Lane>) -> Self {
        Self::new(lanes)
    }
}

/// Segments of a lane's centerline per indexed span. A query projects only
/// onto the spans the grid offers it, so its cost tracks how finely the road
/// near it is sampled, not how long the lanes there are.
const SPAN_SEGMENTS: usize = 4;

/// How far past a lane's half width its footprint reaches, in metres. A
/// lane's edge bows out between centerline points on a curve, and a raised
/// lane on a tilted road leans off its centerline.
const FOOTPRINT_MARGIN: f32 = 0.5;

/// Lane centerlines, cut into spans of [`SPAN_SEGMENTS`] segments, with each
/// span's XY footprint and a grid over them. Spans are in lane order, then
/// along the lane, so the grid's lowest-index tie break picks what projecting
/// onto each lane whole in turn would.
#[derive(Debug, Clone, Default)]
struct LaneIndex {
    bounds: Vec<Aabb>,
    /// Each span's lane, as a position in `lanes`, and its segments.
    spans: Vec<(usize, Range<usize>)>,
    grid: Grid,
}

/// How much further than half a lane's width, in metres, a point may lie
/// from its centerline and still be on the lane.
const HOLD_MARGIN: f32 = 0.05;

/// How far, in metres, a point may stand over or under a lane's centerline
/// and still be on the lane, rather than on a road above or below it.
const HOLD_HEIGHT: f32 = 1.5;

/// Whether `point` is on `lane`, which it projects onto as `projection`:
/// across it, within half its width where it is, and not beyond either
/// end of it or far above or below it.
fn holds(lane: &Lane, point: Point, projection: &Projection) -> bool {
    let along = (point.x - projection.point.x).hypot(point.y - projection.point.y);
    let across = projection.offset.abs();
    across <= lane.width_at(projection.s) * 0.5 + HOLD_MARGIN
        && along <= across + HOLD_MARGIN
        && (point.z - projection.point.z).abs() <= HOLD_HEIGHT
}

impl LaneIndex {
    /// Whether it holds no lane.
    fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// The lanes of the temporary layer traffic may not use, which close
    /// the permanent lanes under them to [`RoadNetwork::nearest_lane`].
    fn closing(lanes: &[Lane]) -> Self {
        Self::build(
            lanes,
            |lane| lane.layer == Layer::Temporary && !lane.kind.is_drivable(),
            |_, _| 0.0,
        )
    }

    /// The [`LaneType::Driving`] lanes' centerlines, for
    /// [`RoadNetwork::nearest_lane`].
    ///
    /// Only [`LaneType::Driving`] is indexed, not everything
    /// [`LaneType::is_drivable`] admits. Snapping a body to the road must land
    /// it on an ordinary traffic lane, so a bus lane or a slip lane beside it
    /// never wins on distance alone.
    fn driving(lanes: &[Lane], layer: Layer) -> Self {
        Self::build(
            lanes,
            |lane| lane.kind == LaneType::Driving && lane.layer == layer,
            |_, _| 0.0,
        )
    }

    /// Every lane's surface: its centerline, widened by half its width and
    /// [`FOOTPRINT_MARGIN`].
    fn footprints(lanes: &[Lane]) -> Self {
        Self::build(
            lanes,
            |_| true,
            |lane, span| {
                let widest = match lane.widths.get(span.start..=span.end) {
                    Some(widths) => widths.iter().copied().fold(0.0, f32::max),
                    None => lane.width,
                };
                widest / 2.0 + FOOTPRINT_MARGIN
            },
        )
    }

    /// The spans of the lanes `keep` takes, each box grown by `pad`.
    fn build(
        lanes: &[Lane],
        keep: impl Fn(&Lane) -> bool,
        pad: impl Fn(&Lane, &Range<usize>) -> f32,
    ) -> Self {
        let (mut bounds, mut spans) = (Vec::new(), Vec::new());
        for (i, lane) in lanes.iter().enumerate() {
            if !keep(lane) {
                continue;
            }
            let points = lane.center.points();
            let segments = points.len() - 1;
            for first in (0..segments).step_by(SPAN_SEGMENTS) {
                let span = first..(first + SPAN_SEGMENTS).min(segments);
                // A span has at least one segment, so two points: Some.
                if let Some(b) =
                    Aabb::around(points[span.start..=span.end].iter().map(|p| (p.x, p.y)))
                {
                    bounds.push(b.padded(pad(lane, &span)));
                    spans.push((i, span));
                }
            }
        }
        let grid = Grid::build(&bounds);
        Self {
            bounds,
            spans,
            grid,
        }
    }
}

/// How far apart two points are in plan.
fn horizontal_distance(a: Point, b: Point) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// A non-empty `profile`, one value per centerline vertex, at `t` of the
/// way along segment `i`. A profile shorter than the centerline holds its
/// last value.
fn along_profile(profile: &[f32], (i, t): (usize, f32)) -> f32 {
    let at = |k: usize| profile[k.min(profile.len() - 1)];
    at(i) + (at(i + 1) - at(i)) * t
}

impl Lane {
    /// Superelevation angle (radians, signed) at arc length `s`, interpolated
    /// between vertices; zero everywhere on a flat lane. Positive raises the
    /// left edge. See [`Lane::bank`].
    pub fn bank_at(&self, s: f32) -> f32 {
        if self.bank.is_empty() {
            return 0.0;
        }
        along_profile(&self.bank, self.center.locate(s))
    }

    /// Lane width (metres) at arc length `s`, interpolated between vertices;
    /// the constant [`Lane::width`] on a lane with no profile. See
    /// [`Lane::widths`].
    pub fn width_at(&self, s: f32) -> f32 {
        if self.widths.is_empty() {
            return self.width;
        }
        along_profile(&self.widths, self.center.locate(s))
    }

    /// The road surface at arc length `s` along this lane: the banked centerline
    /// point, its stored-tangent heading, the bank angle, and the surface
    /// up-normal. What draping a body onto the (possibly canted) lane needs.
    pub fn sample_at(&self, s: f32) -> RoadSample {
        let pose = self.center.pose_at(s);
        RoadSample::new(pose.position, pose.heading, self.bank_at(s))
    }
}

impl RoadNetwork {
    /// Bake a lane list into a network, indexing the [`LaneType::Driving`]
    /// lanes by their ground footprint. Linear in the total number of centerline points.
    pub fn new(lanes: Vec<Lane>) -> Self {
        let index = LaneIndex::driving(&lanes, Layer::Permanent);
        let temporary_index = LaneIndex::driving(&lanes, Layer::Temporary);
        let closing_index = LaneIndex::closing(&lanes);
        Self {
            lanes,
            objects: Vec::new(),
            structures: Vec::new(),
            signals: Vec::new(),
            controllers: Vec::new(),
            vms_groups: Vec::new(),
            road_marks: Vec::new(),
            crg: Vec::new(),
            speed_limits: Vec::new(),
            road_types: Vec::new(),
            road_countries: Vec::new(),
            lane_rules: Vec::new(),
            lane_access: Vec::new(),
            lane_materials: Vec::new(),
            lane_visibility: Vec::new(),
            geo: GeoReference::default(),
            header: Header::default(),
            roads: Vec::new(),
            priorities: Vec::new(),
            road_neighbors: Vec::new(),
            junction_areas: Vec::new(),
            cross_paths: Vec::new(),
            junction_groups: Vec::new(),
            crossings: Vec::new(),
            virtual_junctions: Vec::new(),
            switches: Vec::new(),
            stations: Vec::new(),
            index,
            temporary_index,
            closing_index,
            road_lanes: HashMap::new(),
            footprints: LaneIndex::default(),
        }
    }

    /// This network with `roads` under its lanes, replacing any it had. Each
    /// road's id must be its position in `roads`, and the lanes its sections
    /// name must be on the network.
    pub fn with_roads(mut self, roads: Vec<Road>) -> Self {
        self.road_lanes = roads
            .iter()
            .flat_map(|road| {
                road.sections.iter().flat_map(move |section| {
                    section.lanes.iter().map(move |&(od_id, lane)| {
                        let at = RoadLane {
                            road: road.id,
                            section: section.index,
                            od_id,
                        };
                        (lane, at)
                    })
                })
            })
            .collect();
        self.footprints = if roads.is_empty() {
            LaneIndex::default()
        } else {
            LaneIndex::footprints(&self.lanes)
        };
        self.roads = roads;
        self
    }

    /// This network with `priorities` between its roads, replacing any it had.
    pub fn with_priorities(mut self, priorities: Vec<Priority>) -> Self {
        self.priorities = priorities;
        self
    }

    /// Every priority between two roads of a junction, in the order the
    /// importer emitted them: file order for an OpenDRIVE import.
    pub fn priorities(&self) -> &[Priority] {
        &self.priorities
    }

    /// The roads `road` gives way to, by the junction priorities.
    pub fn yields_to(&self, road: RoadId) -> impl Iterator<Item = RoadId> + '_ {
        self.priorities
            .iter()
            .filter(move |p| p.low == road)
            .map(|p| p.high)
    }

    /// This network with `road_neighbors` beside its roads, replacing any it
    /// had.
    pub fn with_road_neighbors(mut self, road_neighbors: Vec<RoadNeighbor>) -> Self {
        self.road_neighbors = road_neighbors;
        self
    }

    /// Every road running beside another, as the roads name them, in the
    /// order the importer emitted them: road by road, in file order.
    pub fn road_neighbors(&self) -> &[RoadNeighbor] {
        &self.road_neighbors
    }

    /// This network with `junction_areas` over its junctions, replacing any
    /// it had.
    pub fn with_junction_areas(mut self, junction_areas: Vec<JunctionArea>) -> Self {
        self.junction_areas = junction_areas;
        self
    }

    /// The area of each junction that gives a boundary or an elevation grid,
    /// in file order.
    pub fn junction_areas(&self) -> &[JunctionArea] {
        &self.junction_areas
    }

    /// This network with `cross_paths` over its junctions, replacing any it
    /// had.
    pub fn with_cross_paths(mut self, cross_paths: Vec<CrossPath>) -> Self {
        self.cross_paths = cross_paths;
        self
    }

    /// Every path across a junction's roads for pedestrians, in file order.
    /// Each joins lanes part way along them, which [`Lane::successors`]
    /// can't, so they are here rather than in the lane graph.
    pub fn cross_paths(&self) -> &[CrossPath] {
        &self.cross_paths
    }

    /// This network with `crossings`, replacing any it had.
    pub fn with_crossings(mut self, crossings: Vec<Crossing>) -> Self {
        self.crossings = crossings;
        self
    }

    /// Every crossing, where roads cross at one level without joining, in
    /// file order.
    pub fn crossings(&self) -> &[Crossing] {
        &self.crossings
    }

    /// This network with `junction_groups`, replacing any it had.
    pub fn with_junction_groups(mut self, junction_groups: Vec<JunctionGroup>) -> Self {
        self.junction_groups = junction_groups;
        self
    }

    /// Every group of junctions routing should see as one, such as a
    /// roundabout's, in file order.
    pub fn junction_groups(&self) -> &[JunctionGroup] {
        &self.junction_groups
    }

    /// Every group `road`'s junction is in, in file order. None if the road
    /// is in no junction, or its junction in no group. The spec does not
    /// stop a junction from being in more than one.
    pub fn junction_groups_of(&self, road: RoadId) -> impl Iterator<Item = &JunctionGroup> {
        let junction = self.road(road).and_then(|r| r.junction());
        self.junction_groups
            .iter()
            .filter(move |g| junction.is_some_and(|j| g.junctions.iter().any(|m| m == j)))
    }

    /// This network with `virtual_junctions`, replacing any it had.
    pub fn with_virtual_junctions(mut self, virtual_junctions: Vec<VirtualJunction>) -> Self {
        self.virtual_junctions = virtual_junctions;
        self
    }

    /// Every virtual junction, in file order. Its roads meet the main road
    /// part way along it, which [`Lane::successors`] can't, so its links are
    /// kept beside the lane graph, and the router does not follow them.
    pub fn virtual_junctions(&self) -> &[VirtualJunction] {
        &self.virtual_junctions
    }

    /// This network with `switches` and `stations` on its tracks, replacing
    /// any it had.
    pub fn with_railways(mut self, switches: Vec<Switch>, stations: Vec<Station>) -> Self {
        self.switches = switches;
        self.stations = stations;
        self
    }

    /// Every railway switch, road by road in file order. A switch joins
    /// tracks part way along them, which [`Lane::successors`] can't, so they
    /// are kept beside the lane graph.
    pub fn switches(&self) -> &[Switch] {
        &self.switches
    }

    /// Every railway station, in file order.
    pub fn stations(&self) -> &[Station] {
        &self.stations
    }

    /// Every road, in the order the importer emitted them, which is file
    /// order less the roads it skipped. A network built with [`Self::new`],
    /// or serialized before roads were kept, has none, so the road queries
    /// answer `None`.
    pub fn roads(&self) -> &[Road] {
        &self.roads
    }

    /// The road with this id.
    pub fn road(&self, id: RoadId) -> Option<&Road> {
        match self.roads.get(id.0) {
            Some(road) if road.id == id => Some(road),
            _ => self.roads.iter().find(|r| r.id == id),
        }
    }

    /// The first road whose `<road id>` is `od_id`. A scan of the roads.
    pub fn road_by_od_id(&self, od_id: &str) -> Option<&Road> {
        self.roads.iter().find(|road| road.od_id == od_id)
    }

    /// Where `lane` lies on its road: the road, its lane section and its
    /// `<lane id>`.
    pub fn road_lane(&self, lane: LaneId) -> Option<RoadLane> {
        self.road_lanes.get(&lane).copied()
    }

    /// The point on the road surface at `at`, or `None` if there is no such
    /// road, `s` is off its ends, or `t` is not finite.
    ///
    /// The surface is the one the lanes are baked on: the reference line with
    /// its elevation, superelevation and lateral shape, and the `<height>` of
    /// the lane at `t`. A point on the border between two lanes stands on the
    /// inner one, and one past the outermost lane at that lane's outer height.
    /// Signals, objects and road marks stand on the road through this same
    /// call.
    pub fn road_point(&self, at: RoadPosition) -> Option<Point> {
        let road = self.road(at.road)?;
        (road.on_road(at.s) && at.t.is_finite()).then(|| road.surface(at.s, at.t).0)
    }

    /// The road position nearest `point`, or `None` on a network without
    /// roads.
    ///
    /// Of the roads whose lanes come near `point` in plan, it takes the one
    /// whose surface is nearest in 3D, so a point on a bridge finds the
    /// bridge, not the road under it. On that road `s` and `t` put the
    /// surface straight under or over `point`: [`Self::road_point`] gives
    /// `point` back, less its height off the road. Where roads overlap, as in
    /// a junction, their surfaces meet to within rounding, and which of them
    /// comes back is not defined: [`Self::road_position_on`] picks one. A
    /// point off every road gets the nearest road's `s` and `t`, with
    /// `s` held within the road. A road with no lanes is never found.
    pub fn road_position(&self, point: Point) -> Option<RoadPosition> {
        self.nearest_road_position(point, None)
    }

    /// The position on `road` nearest `point`, as [`Self::road_position`]
    /// finds it, but on that road alone. `None` if there is no such road, or
    /// it has no lanes. Where roads overlap, as in a junction, this keeps a
    /// caller that knows its road on it.
    pub fn road_position_on(&self, road: RoadId, point: Point) -> Option<RoadPosition> {
        self.road(road)?;
        self.nearest_road_position(point, Some(road))
    }

    /// The road position nearest `point`, on `only` if it is given. A first
    /// pass picks the span whose lane comes nearest in plan, and solves on it,
    /// so the second pass can drop every span whose lane cannot beat it.
    fn nearest_road_position(&self, point: Point, only: Option<RoadId>) -> Option<RoadPosition> {
        let index = &self.footprints;
        let on_road = |item: u32| {
            let lane = self.lanes[index.spans[item as usize].0].id;
            only.is_none_or(|road| self.road_lanes.get(&lane).is_some_and(|at| at.road == road))
        };
        let seed = index.grid.nearest(point.x, point.y, |item, best| {
            if !on_road(item) {
                return None;
            }
            let (lane, segments) = &index.spans[item as usize];
            let off = self.lane_reach(&self.lanes[*lane], segments, point).1;
            Some((off.max(0.0).powi(2), item)).filter(|&(d2, _)| d2 <= best)
        })?;
        let seeded = self.footprint_position(seed, point);
        let ceiling = seeded.map_or(f32::INFINITY, |(d2, _)| d2);
        index.grid.nearest(point.x, point.y, |item, best| {
            if item == seed {
                return seeded;
            }
            let best = best.min(ceiling);
            if index.bounds[item as usize].dist2(point.x, point.y) > best || !on_road(item) {
                return None;
            }
            let (lane, segments) = &index.spans[item as usize];
            let off = self.lane_reach(&self.lanes[*lane], segments, point).1;
            if off > 0.0 && off * off > best {
                return None;
            }
            self.footprint_position(item, point)
        })
    }

    /// The point on `at.lane`'s surface at its position, or `None` if there
    /// is no such lane on a road, `s` is off the lane's section, or `offset`
    /// is not finite.
    ///
    /// The surface is the lane's own, carried on flat past its borders, so an
    /// `offset` past the lane's edge stays level with the lane rather than
    /// stepping onto a kerb beside it. At `offset` 0 it is the lane's center,
    /// where the baked centerline stands at each of its points.
    pub fn lane_point(&self, at: LanePosition) -> Option<Point> {
        let on = self.road_lane(at.lane)?;
        let road = self.road(on.road)?;
        let section = road.section(on.section)?;
        if !(section.start..=section.end).contains(&at.s) || !at.offset.is_finite() {
            return None;
        }
        let lane = road.across(section, at.s).find(|a| a.od_id == on.od_id)?;
        let t = (lane.inner_t + lane.outer_t) / 2.0 + at.offset;
        let (low, high) = (
            lane.inner_t.min(lane.outer_t),
            lane.inner_t.max(lane.outer_t),
        );
        Some(road.raised(at.s, t, lane.height(t.clamp(low, high))).0)
    }

    /// The lane position of `point`: the lane under or over it on the road
    /// [`Self::road_position`] finds, of any type, with the offset from its
    /// center. `None` on a network without roads.
    ///
    /// The lane is the one whose borders hold the road `t`. A point on the
    /// border between two lanes is on the inner one, and one past the
    /// outermost lane on that lane. A point on the center line is on the
    /// first right lane, or the first left one where there is none. `None`
    /// too at a road `s` no lane section covers, such as before a road's
    /// first section, where there is no lane.
    pub fn lane_position(&self, point: Point) -> Option<LanePosition> {
        self.lane_position_with(point, LayerPolicy::default())
    }

    /// [`Self::lane_position`] under `policy`. With [`LayerPolicy::Both`],
    /// a lane of the temporary layer that holds the point, between its
    /// borders, is the one found. Otherwise it is the permanent layer's.
    pub fn lane_position_with(&self, point: Point, policy: LayerPolicy) -> Option<LanePosition> {
        let at = self.road_position(point)?;
        let road = self.road(at.road)?;
        if policy == LayerPolicy::Both {
            let held = road
                .section_of(Layer::Temporary, at.s)
                .and_then(|section| Some((section, road.lane_at(section, at.s, at.t)?)))
                .filter(|(_, lane)| {
                    let (lo, hi) = (
                        lane.inner_t.min(lane.outer_t),
                        lane.inner_t.max(lane.outer_t),
                    );
                    (lo..=hi).contains(&at.t)
                })
                .and_then(|(section, lane)| {
                    let (_, id) = *section.lanes.iter().find(|(od, _)| *od == lane.od_id)?;
                    Some(LanePosition {
                        lane: id,
                        s: at.s,
                        offset: at.t - (lane.inner_t + lane.outer_t) / 2.0,
                    })
                });
            if held.is_some() {
                return held;
            }
        }
        let section = road
            .section_at(at.s)
            .filter(|section| (section.start - 1e-9..=section.end).contains(&at.s))?;
        let innermost = |od_id: i32| road.across(section, at.s).find(|a| a.od_id == od_id);
        let lane = road
            .lane_at(section, at.s, at.t)
            .or_else(|| innermost(-1))
            .or_else(|| innermost(1))?;
        let (_, id) = *section
            .lanes
            .iter()
            .find(|(od_id, _)| *od_id == lane.od_id)?;
        Some(LanePosition {
            lane: id,
            s: at.s.max(section.start).min(section.end),
            offset: at.t - (lane.inner_t + lane.outer_t) / 2.0,
        })
    }

    /// How far along `at.lane`'s centerline, from its first point, the lane
    /// position is: the `s` of [`Projection`], of the [`Along`] stretches and
    /// of [`Lane::sample_at`]. `None` if there is no such lane on a road, or
    /// `s` is off the lane's section.
    ///
    /// A lane position's `s` is its road's, measured along the reference
    /// line. The two drift apart on a bend, and under a lane offset. See
    /// [Lane positions](crate#lane-positions).
    pub fn centerline_s(&self, at: LanePosition) -> Option<f32> {
        let on = self.road_lane(at.lane)?;
        let section = self.road(on.road)?.section(on.section)?;
        let lane = self.lane(at.lane)?;
        (section.start..=section.end)
            .contains(&at.s)
            .then(|| crate::road::along(lane.center.points(), &section.stations, at.s))
    }

    /// Where `point` projects onto `lane`'s centerline over `segments`, and
    /// how far outside the lane's reach it is in plan: past half its width
    /// and [`FOOTPRINT_MARGIN`]. A lower bound on its distance from the lane.
    fn lane_reach(&self, lane: &Lane, segments: &Range<usize>, point: Point) -> (Projection, f32) {
        let projection = lane.center.project_segments(point, segments.clone());
        let reach = lane.width_at(projection.s) / 2.0 + FOOTPRINT_MARGIN;
        let off = horizontal_distance(point, projection.point) - reach;
        (projection, off)
    }

    /// The road position of `point` solved from the footprint span `item`,
    /// and its squared distance from the span's lane.
    fn footprint_position(&self, item: u32, point: Point) -> Option<(f32, RoadPosition)> {
        let (lane, segments) = &self.footprints.spans[item as usize];
        let lane = &self.lanes[*lane];
        let at = self.road_lanes.get(&lane.id)?;
        let road = self.road(at.road)?;
        let section = road.sections.iter().find(|sec| sec.index == at.section)?;
        let (projection, _) = self.lane_reach(lane, segments, point);
        let (k, f) = lane.center.locate(projection.s);
        let (from, to) = (*section.stations.get(k)?, *section.stations.get(k + 1)?);
        let guess = from + (to - from) * f64::from(f);
        let (s, t) = road.locate(f64::from(point.x), f64::from(point.y), guess);
        let on = s.max(section.start).min(section.end);
        let across = road.across(section, on).find(|a| a.od_id == at.od_id)?;
        let (low, high) = (
            across.inner_t.min(across.outer_t),
            across.inner_t.max(across.outer_t),
        );
        let nearest = road.surface(on, t.clamp(low, high)).0;
        let position = RoadPosition {
            road: road.id,
            s,
            t,
        };
        Some(((point - nearest).length_squared(), position))
    }

    /// This network with `objects` placed on it, replacing any it had.
    pub fn with_objects(mut self, objects: Vec<Object>) -> Self {
        self.objects = objects;
        self
    }

    /// This network with `structures` over its lanes, replacing any it had.
    pub fn with_structures(mut self, structures: Vec<Structure>) -> Self {
        self.structures = structures;
        self
    }

    /// This network with `signals` on its roads, replacing any it had.
    pub fn with_signals(mut self, signals: Vec<Signal>) -> Self {
        self.signals = signals;
        self
    }

    /// This network with `vms_groups` over its signals, replacing any it
    /// had.
    pub fn with_vms_groups(mut self, vms_groups: Vec<VmsGroup>) -> Self {
        self.vms_groups = vms_groups;
        self
    }

    /// Every group of variable message boards on one gantry, in file order.
    pub fn vms_groups(&self) -> &[VmsGroup] {
        &self.vms_groups
    }

    /// This network with `controllers` over its signals, replacing any it
    /// had.
    pub fn with_controllers(mut self, controllers: Vec<Controller>) -> Self {
        self.controllers = controllers;
        self
    }

    /// This network with `road_marks` along its lanes, replacing any it had.
    pub fn with_road_marks(mut self, road_marks: Vec<RoadMark>) -> Self {
        self.road_marks = road_marks;
        self
    }

    /// This network with the OpenCRG surfaces `crg` laid on its lanes,
    /// replacing any it had.
    pub fn with_crg_surfaces(mut self, crg: Vec<CrgSurface>) -> Self {
        self.crg = crg;
        self
    }

    /// This network with `speed_limits` on its lanes, replacing any it had.
    pub fn with_speed_limits(mut self, speed_limits: Vec<Along<SpeedLimit>>) -> Self {
        self.speed_limits = along::sorted(speed_limits);
        self
    }

    /// This network with `road_types` along its lanes, replacing any it had.
    pub fn with_road_types(mut self, road_types: Vec<Along<RoadType>>) -> Self {
        self.road_types = along::sorted(road_types);
        self
    }

    /// This network with `road_countries` along its lanes, replacing any it
    /// had.
    pub fn with_road_countries(mut self, road_countries: Vec<Along<String>>) -> Self {
        self.road_countries = along::sorted(road_countries);
        self
    }

    /// This network with `lane_rules` along its lanes, replacing any it had.
    pub fn with_lane_rules(mut self, lane_rules: Vec<Along<String>>) -> Self {
        self.lane_rules = along::sorted(lane_rules);
        self
    }

    /// This network with `lane_access` along its lanes, replacing any it
    /// had.
    pub fn with_lane_access(mut self, lane_access: Vec<Along<Access>>) -> Self {
        self.lane_access = along::sorted(lane_access);
        self
    }

    /// This network with `lane_materials` along its lanes, replacing any it
    /// had.
    pub fn with_lane_materials(mut self, lane_materials: Vec<Along<Material>>) -> Self {
        self.lane_materials = along::sorted(lane_materials);
        self
    }

    /// Every speed limit, sorted by lane, then along it. A lane has at most
    /// one limit at any point, and none where the map doesn't say. An
    /// OpenDRIVE import reads them from `<speed>`s and speed limit signs,
    /// and from `<defaultRegulations>` where neither gives one. See
    /// [Speed limits and road types](crate#speed-limits-and-road-types).
    pub fn speed_limits(&self) -> &[Along<SpeedLimit>] {
        &self.speed_limits
    }

    /// The speed limit `s` metres along `lane`, or `None` where the map
    /// doesn't say. Where two limits meet, the later one holds.
    pub fn speed_limit_at(&self, lane: LaneId, s: f32) -> Option<SpeedLimit> {
        along::at(&self.speed_limits, lane, s).copied()
    }

    /// The type of road along every lane, sorted by lane, then along it.
    pub fn road_types(&self) -> &[Along<RoadType>] {
        &self.road_types
    }

    /// The type of road `s` metres along `lane`, or `None` where the map
    /// doesn't say. Where two types meet, the later one holds.
    pub fn road_type_at(&self, lane: LaneId, s: f32) -> Option<RoadType> {
        along::at(&self.road_types, lane, s).copied()
    }

    /// The country whose rules each road type follows, along every lane,
    /// sorted by lane, then along it: the `country` of an OpenDRIVE road
    /// `<type>`, a code such as `DE`. None where the map gives none. The
    /// crate applies none of a country's rules.
    pub fn road_countries(&self) -> &[Along<String>] {
        &self.road_countries
    }

    /// The country of the road type `s` metres along `lane`, or `None` where
    /// the map gives none. Where two meet, the later one holds.
    pub fn road_country_at(&self, lane: LaneId, s: f32) -> Option<&str> {
        along::at(&self.road_countries, lane, s).map(String::as_str)
    }

    /// Every lane rule, sorted by lane, then along it: free text such as
    /// `no stopping at any time`, from an OpenDRIVE `<rule>`.
    pub fn lane_rules(&self) -> &[Along<String>] {
        &self.lane_rules
    }

    /// The rule `s` metres along `lane`, or `None` where the map gives none.
    /// Where two rules meet, the later one holds.
    pub fn lane_rule_at(&self, lane: LaneId, s: f32) -> Option<&str> {
        along::at(&self.lane_rules, lane, s).map(String::as_str)
    }

    /// Who may use each lane, sorted by lane, then along it. A lane is open
    /// to everyone where it has none.
    pub fn lane_access(&self) -> &[Along<Access>] {
        &self.lane_access
    }

    /// Who may use `lane` `s` metres along it, or `None` where everyone
    /// may. Where two meet, the later one holds.
    pub fn lane_access_at(&self, lane: LaneId, s: f32) -> Option<&Access> {
        along::at(&self.lane_access, lane, s)
    }

    /// What each lane's surface is made of, sorted by lane, then along it.
    pub fn lane_materials(&self) -> &[Along<Material>] {
        &self.lane_materials
    }

    /// What `lane`'s surface is made of `s` metres along it, or `None` where
    /// the map doesn't say. Where two meet, the later one holds.
    pub fn lane_material_at(&self, lane: LaneId, s: f32) -> Option<&Material> {
        along::at(&self.lane_materials, lane, s)
    }

    /// This network with `lane_visibility` along its lanes, replacing any it
    /// had.
    pub fn with_lane_visibility(mut self, lane_visibility: Vec<Along<Visibility>>) -> Self {
        self.lane_visibility = along::sorted(lane_visibility);
        self
    }

    /// How far a driver can see from each lane, sorted by lane, then along
    /// it, from OpenDRIVE `<visibility>`s. None where the map doesn't say.
    pub fn lane_visibility(&self) -> &[Along<Visibility>] {
        &self.lane_visibility
    }

    /// How far a driver can see from `lane` `s` metres along it, or `None`
    /// where the map doesn't say. Where two meet, the later one holds.
    pub fn lane_visibility_at(&self, lane: LaneId, s: f32) -> Option<&Visibility> {
        along::at(&self.lane_visibility, lane, s)
    }

    /// This network placed on the earth by `geo`, replacing any geo
    /// reference it had.
    pub fn with_geo_reference(mut self, geo: GeoReference) -> Self {
        self.geo = geo;
        self
    }

    /// Where the map sits on the earth: its `<geoReference>` and `<offset>`,
    /// unapplied. See [Geo reference](crate#geo-reference).
    pub fn geo_reference(&self) -> &GeoReference {
        &self.geo
    }

    /// This network with `header` as its header, replacing any it had.
    pub fn with_header(mut self, header: Header) -> Self {
        self.header = header;
        self
    }

    /// What the map says about itself in its `<header>`: its version, name,
    /// date, vendor and bounds. The crate reads none of it to decide how to
    /// read the map.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The OpenCRG files the map lays on its roads, in map order. Load them
    /// into a [`RoadSurface`](crate::RoadSurface).
    pub fn crg_surfaces(&self) -> &[CrgSurface] {
        &self.crg
    }

    /// Every tunnel and bridge, in the order the importer emitted them.
    pub fn structures(&self) -> &[Structure] {
        &self.structures
    }

    /// The structure with this id, by identity (not position), the same way
    /// as [`Self::lane`].
    pub fn structure(&self, id: StructureId) -> Option<&Structure> {
        match self.structures.get(id.0) {
            Some(structure) if structure.id == id => Some(structure),
            _ => self.structures.iter().find(|s| s.id == id),
        }
    }

    /// Every structure over `lane`, with the part of the lane it covers. Use
    /// it to find whether a lane runs through a tunnel or over a bridge, and
    /// where.
    pub fn structures_over(&self, lane: LaneId) -> impl Iterator<Item = (&Structure, &Coverage)> {
        self.structures.iter().flat_map(move |s| {
            s.lanes
                .iter()
                .filter(move |c| c.lane == lane)
                .map(move |c| (s, c))
        })
    }

    /// Every lane, in the order the importer emitted them. Positions are not
    /// ids. Look a specific lane up with [`RoadNetwork::lane`].
    pub fn lanes(&self) -> &[Lane] {
        &self.lanes
    }

    /// Every object placed along the roads, in the order the importer emitted
    /// them.
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    /// The object with this id, by identity (not position), the same way as
    /// [`Self::lane`].
    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        match self.objects.get(id.0) {
            Some(object) if object.id == id => Some(object),
            _ => self.objects.iter().find(|o| o.id == id),
        }
    }

    /// Every signal, in the order the importer emitted them.
    pub fn signals(&self) -> &[Signal] {
        &self.signals
    }

    /// The signal with this id, by identity (not position), the same way as
    /// [`Self::lane`].
    pub fn signal(&self, id: SignalId) -> Option<&Signal> {
        match self.signals.get(id.0) {
            Some(signal) if signal.id == id => Some(signal),
            _ => self.signals.iter().find(|s| s.id == id),
        }
    }

    /// Every signal controller, in the order the importer emitted them.
    pub fn controllers(&self) -> &[Controller] {
        &self.controllers
    }

    /// The controller with this id, by identity (not position), the same way
    /// as [`Self::lane`].
    pub fn controller(&self, id: ControllerId) -> Option<&Controller> {
        match self.controllers.get(id.0) {
            Some(controller) if controller.id == id => Some(controller),
            _ => self.controllers.iter().find(|c| c.id == id),
        }
    }

    /// Every road mark, in the order the importer emitted them.
    pub fn road_marks(&self) -> &[RoadMark] {
        &self.road_marks
    }

    /// The road mark with this id, by identity (not position), the same way
    /// as [`Self::lane`].
    pub fn road_mark(&self, id: RoadMarkId) -> Option<&RoadMark> {
        match self.road_marks.get(id.0) {
            Some(mark) if mark.id == id => Some(mark),
            _ => self.road_marks.iter().find(|m| m.id == id),
        }
    }

    /// The lane with this id, by identity (not position), so ids stay valid
    /// however an importer assigns them.
    ///
    /// Importers hand out ids sequentially, so the id is almost always its own
    /// index, so try that first and verify, falling back to a scan when it is
    /// not. The fallback keeps the by-identity contract. The fast path keeps
    /// the router off an O(lanes) probe per Dijkstra pop.
    pub fn lane(&self, id: LaneId) -> Option<&Lane> {
        match self.lanes.get(id.0) {
            Some(lane) if lane.id == id => Some(lane),
            _ => self.lanes.iter().find(|l| l.id == id),
        }
    }

    /// Every lane of kind [`LaneType::Driving`]. Narrower than
    /// [`LaneType::is_drivable`], and narrower still than [`Self::lanes`],
    /// which also yields sidewalks, medians, and the rest of the
    /// cross-section.
    pub fn driving_lanes(&self) -> impl Iterator<Item = &Lane> {
        self.lanes.iter().filter(|l| l.kind == LaneType::Driving)
    }

    /// The lowest point of any lane centerline (Z-up, metres), so how far down
    /// the road legitimately reaches. `None` if the network has no lanes. Used
    /// to set an off-map fall floor relative to the terrain, so a map that dips
    /// well below zero (a valley, an underpass) isn't mistaken for freefall.
    /// A height that is NaN is skipped.
    pub fn min_elevation(&self) -> Option<f32> {
        self.lanes
            .iter()
            .flat_map(|l| l.center.points())
            .map(|p| p.z)
            .filter(|z| !z.is_nan())
            .reduce(f32::min)
    }

    /// The lanes reachable by driving off `id`'s exit end (its `successors`).
    pub fn successors(&self, id: LaneId) -> impl Iterator<Item = &Lane> {
        self.lane(id)
            .into_iter()
            .flat_map(|l| l.successors.iter())
            .filter_map(|s| self.lane(*s))
    }

    /// The driving lane whose centerline is nearest `point`, with the
    /// projection onto it. That is the lane a body is in, and its lane-keeping
    /// error. The projection's `s` is along the lane's baked centerline, not
    /// its road. [`Self::lane_position`] gives the road's, on the exact lane
    /// surface.
    ///
    /// Answered through the ground-plane index built in [`RoadNetwork::new`],
    /// so the cost tracks the local lane density rather than the size of the
    /// map.
    ///
    /// Nearest is by full 3D distance, but the index prunes in XY only. That
    /// is sound, because a horizontal distance is never more than the 3D one,
    /// so pruning on it can only keep candidates, never drop a winner. It is
    /// also what keeps both of two stacked roads, a bridge over a road,
    /// candidates for a point between them.
    ///
    /// `None` for a point that isn't finite.
    ///
    /// Where a temporary lane lies over the point, that lane is the one
    /// found. See [`Self::nearest_lane_with`] to look at the permanent
    /// layer only.
    pub fn nearest_lane(&self, point: Point) -> Option<(LaneId, Projection)> {
        self.nearest_lane_with(point, LayerPolicy::default())
    }

    /// [`Self::nearest_lane`] under `policy`. With [`LayerPolicy::Both`], a
    /// temporary driving lane whose width holds the point wins over a nearer
    /// lane of the permanent layer. A temporary lane traffic may not use, such
    /// as a restricted one or one closed for works, hides the permanent lane
    /// under it, so that lane is passed over. With
    /// [`LayerPolicy::PermanentOnly`], only the permanent layer's lanes are
    /// looked at, as if there were no temporary ones.
    pub fn nearest_lane_with(
        &self,
        point: Point,
        policy: LayerPolicy,
    ) -> Option<(LaneId, Projection)> {
        if !point.to_array().iter().all(|c| c.is_finite()) {
            return None;
        }
        let both = policy == LayerPolicy::Both;
        if both && !self.temporary_index.is_empty() {
            let held = self.nearest_in(&self.temporary_index, point, true, false);
            if held.is_some() {
                return held;
            }
        }
        let hidden = both && !self.closing_index.is_empty();
        self.nearest_in(&self.index, point, false, hidden)
    }

    /// The nearest lane of `index` to `point`. With `must_hold`, the nearest
    /// whose width holds the point. With `skip_closed`, one whose place under
    /// the point a lane of the temporary layer traffic may not use closes is
    /// passed over.
    fn nearest_in(
        &self,
        index: &LaneIndex,
        point: Point,
        must_hold: bool,
        skip_closed: bool,
    ) -> Option<(LaneId, Projection)> {
        index.grid.nearest(point.x, point.y, |item, best| {
            let i = item as usize;
            // The footprint is a lower bound on the distance to the
            // centerline, so a span whose box already loses needs no
            // projection. That is most of them, and all the repeats of a
            // span that crosses several cells.
            if index.bounds[i].dist2(point.x, point.y) > best {
                return None;
            }
            let (lane, segments) = &index.spans[i];
            let lane = &self.lanes[*lane];
            let projection = lane.center.project_segments(point, segments.clone());
            if must_hold && !holds(lane, point, &projection) {
                return None;
            }
            if skip_closed {
                // Where the lane lies under the point, not just along this
                // span of it.
                let under = lane.center.project(point).point;
                if self
                    .nearest_in(&self.closing_index, under, true, false)
                    .is_some()
                {
                    return None;
                }
            }
            Some((
                (point - projection.point).length_squared(),
                (lane.id, projection),
            ))
        })
    }

    /// The road surface nearest `point`: project onto the nearest driving lane,
    /// then sample it. The entry point for draping a body onto the road.
    /// `None` if there are no driving lanes. NB: on a banked
    /// multi-lane road, adjacent lanes differ in height, so this can step
    /// vertically as the nearest lane flips at a lane boundary.
    pub fn sample_near(&self, point: Point) -> Option<RoadSample> {
        let (id, proj) = self.nearest_lane(point)?;
        self.lane(id).map(|lane| lane.sample_at(proj.s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::Vector;

    fn lane(id: usize, points: &[[f32; 3]]) -> Lane {
        Lane {
            id: LaneId(id),
            kind: LaneType::Driving,
            layer: crate::Layer::Permanent,
            direction: Direction::Forward,
            center: Polyline::new(points.iter().map(|p| Point::from_array(*p)).collect()),
            width: 3.5,
            widths: Vec::new(),
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        }
    }

    #[test]
    fn nearest_lane_picks_the_closer_centerline() {
        let net = RoadNetwork::new(vec![
            lane(0, &[[0.0, 2.0, 0.0], [10.0, 2.0, 0.0]]),
            lane(1, &[[0.0, -2.0, 0.0], [10.0, -2.0, 0.0]]),
        ]);
        let (id, proj) = net.nearest_lane(Point::new(5.0, 1.5, 0.0)).expect("a lane");
        assert_eq!(id, LaneId(0));
        assert!((proj.point - Point::new(5.0, 2.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn nearest_lane_is_none_when_empty() {
        assert!(RoadNetwork::default().nearest_lane(Point::ORIGIN).is_none());
    }

    #[test]
    fn sample_at_reads_bank_and_tilts_the_up_normal() {
        let mut banked = lane(0, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        banked.bank = vec![0.2, 0.2];
        let s = banked.sample_at(5.0);
        assert!((s.bank - 0.2).abs() < 1e-5, "bank {}", s.bank);
        assert!(
            s.heading.abs_diff_eq(Vector::X, 1e-4),
            "heading {:?}",
            s.heading
        );
        // The up-normal leans off vertical but still points up.
        assert!(s.up.z < 1.0 && s.up.z > 0.9, "up {:?}", s.up);
        assert!((s.up - Vector::Z).length() > 0.05, "up should tilt");

        // A flat lane samples bank 0 and a vertical up-normal.
        let flat = lane(1, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        let fs = flat.sample_at(5.0);
        assert_eq!(fs.bank, 0.0);
        assert!(fs.up.abs_diff_eq(Vector::Z, 1e-5), "flat up {:?}", fs.up);
    }

    #[test]
    fn sample_near_samples_the_nearest_lane() {
        let mut banked = lane(0, &[[0.0, 2.0, 0.0], [10.0, 2.0, 0.0]]);
        banked.bank = vec![0.1, 0.1];
        let flat = lane(1, &[[0.0, -2.0, 0.0], [10.0, -2.0, 0.0]]);
        let net = RoadNetwork::new(vec![banked, flat]);
        // Nearer the banked lane -> its bank.
        let a = net
            .sample_near(Point::new(5.0, 1.8, 0.0))
            .expect("a sample");
        assert!((a.bank - 0.1).abs() < 1e-5, "bank {}", a.bank);
        // Nearer the flat lane -> bank 0.
        let b = net
            .sample_near(Point::new(5.0, -1.8, 0.0))
            .expect("a sample");
        assert_eq!(b.bank, 0.0);
    }

    #[test]
    fn sample_near_is_none_when_empty() {
        assert!(RoadNetwork::default().sample_near(Point::ORIGIN).is_none());
    }

    #[test]
    fn sample_at_up_follows_stored_tangent_not_travel() {
        // A Backward lane and a Forward lane with the SAME centerline and bank
        // must sample identically: sample_at rolls about the stored tangent, so
        // travel direction never enters and `up` agrees with the baked heights.
        let pts = &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]];
        let mut fwd = lane(0, pts);
        fwd.bank = vec![0.2, 0.2];
        let mut bwd = lane(1, pts);
        bwd.direction = Direction::Backward;
        bwd.bank = vec![0.2, 0.2];

        let f = fwd.sample_at(5.0);
        let b = bwd.sample_at(5.0);
        assert_eq!(f.up, b.up, "up must not depend on travel direction");
        assert_eq!(f.bank, b.bank);
        assert!(f.heading.abs_diff_eq(b.heading, 1e-6));
        // Raised edge is the +offset (left of the stored tangent): for heading
        // +X, left is +Y, so the up-normal leans toward -Y.
        assert!(
            b.up.y < -0.05,
            "up should lean away from the raised +Y edge: {:?}",
            b.up
        );
    }

    // --- curved + banked sample_at -------------------------------------------

    // On a lane that TURNS and is banked, `heading` tracks the curve, `bank`
    // interpolates between vertices, and `up` stays unit, tilted, and orthogonal
    // to the heading at every station.
    #[test]
    fn sample_at_on_a_curved_banked_lane() {
        // Two segments: +X for 10 m, then turning toward +Y. Bank ramps 0 -> 0.2.
        let mut l = lane(0, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 10.0, 0.0]]);
        l.bank = vec![0.0, 0.1, 0.2];
        let seg1 = 10.0_f32;
        let seg2 = (100.0_f32 + 100.0).sqrt(); // sqrt(200)

        // Heading at the very start is the first segment direction (+X).
        assert!(
            l.sample_at(0.0).heading.abs_diff_eq(Vector::X, 1e-5),
            "start heading {:?}",
            l.sample_at(0.0).heading
        );
        // Heading at the very end is the last segment direction (+X+Y / sqrt2).
        let end_dir = Vector::new(1.0, 1.0, 0.0).normalize_or_zero();
        assert!(
            l.sample_at(seg1 + seg2).heading.abs_diff_eq(end_dir, 1e-4),
            "end heading {:?} want {end_dir:?}",
            l.sample_at(seg1 + seg2).heading
        );

        // Bank reads the profile: 0.1 at the interior vertex, 0.2 at the end,
        // and the midpoint of the ramped second segment is halfway (0.15).
        assert!((l.sample_at(seg1).bank - 0.1).abs() < 1e-5);
        assert!((l.sample_at(seg1 + seg2).bank - 0.2).abs() < 1e-5);
        assert!(
            (l.sample_at(seg1 + seg2 * 0.5).bank - 0.15).abs() < 1e-5,
            "mid-seg2 bank {}",
            l.sample_at(seg1 + seg2 * 0.5).bank
        );

        // At every station up is unit, orthogonal to heading, and (where banked)
        // tilted off vertical.
        for s in [0.0, 3.0, seg1, seg1 + 4.0, seg1 + seg2] {
            let rs = l.sample_at(s);
            assert!((rs.up.length() - 1.0).abs() < 1e-5, "up not unit @ {s}");
            assert!(rs.up.dot(rs.heading).abs() < 1e-6, "up.heading != 0 @ {s}");
            assert!(rs.up.z > 0.9, "up.z too low @ {s}: {}", rs.up.z);
            if rs.bank.abs() > 1e-3 {
                assert!(
                    (rs.up - Vector::Z).length() > 0.02,
                    "up should tilt where banked @ {s}: {:?}",
                    rs.up
                );
            }
        }
    }

    // Negative bank in sample_at leans the up-normal the opposite way from
    // positive bank (raised edge flips sides).
    #[test]
    fn sample_at_negative_bank_flips_the_up_normal() {
        let mut pos = lane(0, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        pos.bank = vec![0.25, 0.25];
        let mut neg = lane(1, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        neg.bank = vec![-0.25, -0.25];
        let p = pos.sample_at(5.0);
        let n = neg.sample_at(5.0);
        // Heading +X: +bank leans up toward -Y, -bank toward +Y.
        assert!(p.up.y < -0.05, "+bank up {:?}", p.up);
        assert!(n.up.y > 0.05, "-bank up {:?}", n.up);
        assert!((p.up.y + n.up.y).abs() < 1e-6, "should be mirrored in y");
        assert!((p.up.z - n.up.z).abs() < 1e-6, "same height component");
    }

    // The documented lane-boundary vertical step: two adjacent banked lanes sit
    // at different heights, and sample_near returns the *nearest* lane's height
    // and bank, stepping as the nearest lane flips across the boundary.
    #[test]
    fn sample_near_steps_at_a_banked_lane_boundary() {
        // Lane A raised (+y side), lane B lowered (-y side); each carries its own
        // bank. The reference-line pivot makes their centerlines differ in z.
        let mut a = lane(0, &[[0.0, 2.0, 0.3], [10.0, 2.0, 0.3]]);
        a.bank = vec![0.1, 0.1];
        let mut b = lane(1, &[[0.0, -2.0, -0.3], [10.0, -2.0, -0.3]]);
        b.bank = vec![-0.15, -0.15];
        let net = RoadNetwork::new(vec![a, b]);

        // Just on A's side of the midline -> A's height and bank.
        let sa = net
            .sample_near(Point::new(5.0, 0.1, 0.0))
            .expect("sample A");
        assert!((sa.bank - 0.1).abs() < 1e-5, "A bank {}", sa.bank);
        assert!((sa.point.z - 0.3).abs() < 1e-5, "A height {}", sa.point.z);
        // Just on B's side -> B's height and bank.
        let sb = net
            .sample_near(Point::new(5.0, -0.1, 0.0))
            .expect("sample B");
        assert!((sb.bank + 0.15).abs() < 1e-5, "B bank {}", sb.bank);
        assert!((sb.point.z + 0.3).abs() < 1e-5, "B height {}", sb.point.z);
        // The seam is a real vertical step, not a blend.
        assert!(
            (sa.point.z - sb.point.z).abs() > 0.5,
            "expected a vertical step across the boundary: {} vs {}",
            sa.point.z,
            sb.point.z
        );
    }

    // sample_at at a vertex reads exactly the bank stored there.
    #[test]
    fn sample_at_reads_vertex_bank_exactly() {
        let mut l = lane(0, &[[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 0.0, 6.0]]);
        l.bank = vec![0.05, 0.12, -0.08];
        // Arc length at each vertex: 0, 4, 10.
        for (s, want) in [(0.0, 0.05), (4.0, 0.12), (10.0, -0.08)] {
            assert!(
                (l.sample_at(s).bank - want).abs() < 1e-6,
                "vertex bank @ {s} = {}, want {want}",
                l.sample_at(s).bank
            );
        }
    }

    #[test]
    fn min_elevation_is_the_lowest_centerline_point() {
        // A network that dips to z=-40 (a deep valley) reports -40, not 0.
        let net = RoadNetwork::new(vec![
            lane(0, &[[0.0, 0.0, 5.0], [10.0, 0.0, 2.0]]),
            lane(1, &[[0.0, 0.0, -40.0], [10.0, 0.0, -12.0]]),
        ]);
        assert_eq!(net.min_elevation(), Some(-40.0));
        assert_eq!(RoadNetwork::default().min_elevation(), None);
    }

    #[test]
    fn lane_lookup_is_by_id_not_position() {
        // Ids need not equal vec positions. An importer may assign arbitrary
        // ones. Position 0 holds id 17, position 1 holds id 4.
        let net = RoadNetwork::new(vec![
            Lane {
                id: LaneId(17),
                ..lane(0, &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])
            },
            Lane {
                id: LaneId(4),
                ..lane(1, &[[0.0, 5.0, 0.0], [1.0, 5.0, 0.0]])
            },
        ]);
        assert_eq!(net.lane(LaneId(17)).map(|l| l.id), Some(LaneId(17)));
        assert_eq!(net.lane(LaneId(4)).map(|l| l.id), Some(LaneId(4)));
        assert!(net.lane(LaneId(0)).is_none()); // position 0, but not id 0
                                                // nearest_lane's returned id round-trips through lane().
        let (id, _) = net.nearest_lane(Point::new(0.5, 0.0, 0.0)).unwrap();
        assert!(net.lane(id).is_some());
    }

    // --- bank_at sampling ----------------------------------------------------

    // A flat lane (empty bank) reads 0 everywhere and never panics, including at
    // and past the ends and below zero.
    #[test]
    fn bank_at_of_a_flat_lane_is_zero_and_never_panics() {
        let l = lane(0, &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]]);
        assert!(l.bank.is_empty());
        for s in [-10.0, -0.0, 0.0, 1.0, 2.0, 4.0, 4.0001, 1000.0] {
            assert_eq!(l.bank_at(s), 0.0, "flat bank_at({s})");
        }
    }

    // A non-empty profile interpolates linearly between vertices and clamps past
    // both ends. Centerline at x = 0, 2, 4 (two 2 m segments); bank = 0, 0.1,
    // 0.2, so bank_at grows linearly with s and flattens outside [0, 4].
    #[test]
    fn bank_at_interpolates_between_vertices_and_clamps() {
        let l = Lane {
            bank: vec![0.0, 0.1, 0.2],
            ..lane(0, &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]])
        };
        // Exactly on vertices.
        assert!((l.bank_at(0.0) - 0.0).abs() < 1e-6, "{}", l.bank_at(0.0));
        assert!((l.bank_at(2.0) - 0.1).abs() < 1e-6, "{}", l.bank_at(2.0));
        assert!((l.bank_at(4.0) - 0.2).abs() < 1e-6, "{}", l.bank_at(4.0));
        // Midway through each segment -> the midpoint value.
        assert!((l.bank_at(1.0) - 0.05).abs() < 1e-6, "{}", l.bank_at(1.0));
        assert!((l.bank_at(3.0) - 0.15).abs() < 1e-6, "{}", l.bank_at(3.0));
        // Past the far end clamps to the last vertex; below zero to the first.
        assert!(
            (l.bank_at(100.0) - 0.2).abs() < 1e-6,
            "{}",
            l.bank_at(100.0)
        );
        assert!(
            (l.bank_at(-100.0) - 0.0).abs() < 1e-6,
            "{}",
            l.bank_at(-100.0)
        );
        // Exactly at length and just past it must not panic and stay clamped.
        let len = l.center.length();
        assert!((l.bank_at(len) - 0.2).abs() < 1e-6);
        assert!((l.bank_at(len + 5.0) - 0.2).abs() < 1e-6);
    }

    // The stored sign is preserved (a negative bank stays negative through
    // interpolation and clamping).
    #[test]
    fn bank_at_preserves_a_negative_profile() {
        let l = Lane {
            bank: vec![-0.2, -0.1, 0.0],
            ..lane(0, &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]])
        };
        assert!((l.bank_at(0.0) + 0.2).abs() < 1e-6, "{}", l.bank_at(0.0));
        assert!((l.bank_at(1.0) + 0.15).abs() < 1e-6, "{}", l.bank_at(1.0));
        assert!((l.bank_at(-5.0) + 0.2).abs() < 1e-6, "clamp low");
    }
}
