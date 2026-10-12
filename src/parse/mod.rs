//! The OpenDRIVE parser: `.xodr` XML in, a baked [`RoadNetwork`] out.
//!
//! Reference geometry (`line`, `arc`, `spiral`, `paramPoly3`, `poly3`) is
//! evaluated to points at a fixed arc-length step, elevation and
//! superelevation profiles are applied per sample, and lane connectivity is
//! resolved in [`links`] once every lane has an id.
//!
//! Geometry is cross-checked against the reference C++
//! [libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE).

use std::cell::Cell;
use std::collections::HashMap;

use crate::coords::{Point, Vector};
use crate::crg::Stretch;
use crate::object::orient;
use crate::road::{
    active, along, height_at, is_valid, side_borders, side_heights, CrossSection, Cubic, GeomRec,
    GeomShape, HeightDef, LaneBorders, LaneExtent, LaneGeom, Lateral, Road, RoadId, RoadSection,
    ShapeProfile, Strip, TrafficRule, Validity,
};
use crate::{
    Border, ControllerId, Corner, Coverage, CrgAlong, CrgMode, CrgObject, CrgPose, CrgPurpose,
    CrgSurface, Direction, Extent, GeoOffset, GeoReference, Header, Lane, LaneAdvisory, LaneId,
    LaneType, Layer, Marking, Material, Object, ObjectId, ObjectType, ParkingSpace, Polyline,
    Priority, RoadMarkId, RoadMarkWeight, RoadNeighbor, RoadNetwork, Section, Shape, Side,
    SignalId, SkeletonVertex, Structure, StructureId, StructureKind, UserData,
};

mod curve_local;
mod gaps;
mod ignored;
mod junction_areas;
mod links;
mod properties;
mod railway;
mod road_marks;
mod signals;
mod single_side;
mod virtual_junctions;
mod warning;
use links::{LaneMeta, RoadInfo, Topology};
use road_marks::MarkDef;
pub use warning::{RoadEnd, RoadSkipReason, Warning};

/// The furthest apart, in metres, a lane's centerline samples get, on
/// straight or gently curving road.
const SAMPLE_STEP: f64 = 2.0;

/// The most a lane may turn, in radians, from one sample to the next, and the
/// closest together, in metres, its samples get on a tight curve to keep to
/// it.
const SAMPLE_TURN: f64 = 0.05;
const SAMPLE_MIN_STEP: f64 = 0.25;

/// The most a lane's sample step may grow or shrink, in metres, per metre of
/// road. Kept low so each step is well over the fraction of the one before
/// that the mesh welds away as a stub.
const SAMPLE_GRADE: f64 = 0.25;

/// How far apart, in metres, a section is probed for how sharply its lanes
/// turn. Road curvature changes over metres, and probing finer cost more at
/// import than sampling the lanes did.
const SAMPLE_PROBE: f64 = 1.0;

/// The furthest apart, in metres, a sweep's sections may be, and the closest
/// they get where it bends.
const SWEEP_MAX_STEP: f64 = 10.0;
const SWEEP_MIN_STEP: f64 = 0.05;

/// How far, in metres, a sweep may stray from the straight walls between its
/// sections.
const SWEEP_TOLERANCE: f32 = 0.01;

/// The most objects one `<repeat>` may expand to, and the most dashes one
/// `<marking>` may paint. A tiny `distance` over a long `length` would
/// otherwise ask for billions of them. Real rows are far
/// shorter: esmini's e6mini, a 1.5 km highway lined with posts every 4 m,
/// repeats 367 at most.
const MAX_REPEAT_INSTANCES: f64 = 100_000.0;

/// The most corners the outlines of one `<repeat>` may bake to, over all its
/// copies. A `<curveLocal>` can sample to a thousand corners, so a repeat
/// well under [`MAX_REPEAT_INSTANCES`] could still bake a billion.
const MAX_OUTLINE_CORNERS: usize = 1_000_000;

/// The most objects one load bakes, an `<objectReference>` counting again.
/// Three rows of posts every 2 m along a 60 km highway are 90,000. Many
/// repeats or references to one object could otherwise ask for gigabytes
/// from a few kilobytes of file.
const MAX_OBJECTS: usize = 100_000;

/// The most corners of outlines, vertices of skeletons and sections of
/// continuous repeats one load bakes for its objects, an
/// `<objectReference>` counting again. The 60 km A9 test field, lined with
/// guardrails, bakes about 750,000.
const MAX_OBJECT_PIECES: usize = 2_000_000;

/// What baking objects costs: how many objects, and how many corners,
/// vertices and sections they have. See [`MAX_OBJECTS`] and
/// [`MAX_OBJECT_PIECES`].
#[derive(Clone, Copy, Default)]
struct Cost {
    objects: usize,
    pieces: usize,
}

impl std::ops::Add for Cost {
    type Output = Cost;

    fn add(self, o: Cost) -> Cost {
        Cost {
            objects: self.objects.saturating_add(o.objects),
            pieces: self.pieces.saturating_add(o.pieces),
        }
    }
}

impl std::iter::Sum for Cost {
    fn sum<I: Iterator<Item = Cost>>(iter: I) -> Cost {
        iter.fold(Cost::default(), |a, b| a + b)
    }
}

/// How far paint stands off the surface under it, in metres, where the map
/// does not say: an object's `<marking>` with no `zOffset`, and a
/// `fillType="paint"` area.
const PAINT_RAISE: f64 = 0.005;

/// Why an OpenDRIVE document did not import.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The document is not well-formed XML.
    #[error("invalid OpenDRIVE XML: {0}")]
    Xml(#[from] roxmltree::Error),
    /// The XML parsed, but it is not a usable map. Either it was unreadable
    /// from disk, or it carries no lanes at all.
    #[error("malformed OpenDRIVE: {0}")]
    Malformed(String),
}

/// The OpenDRIVE identity of one baked lane: which road, lane section, and
/// original `<lane id>` it was tessellated from.
///
/// A baked [`Lane`] is format-agnostic and carries only an opaque [`LaneId`];
/// this is the OpenDRIVE-specific provenance, kept in a separate table so
/// [`Lane`] stays format-neutral. Obtain it from [`load_str_with_provenance`]
/// or [`load_file_with_provenance`], and look a lane up by its [`LaneId`]
/// rather than by position.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LaneProvenance {
    /// The baked lane this record describes.
    pub lane: LaneId,
    /// The `<road id>` attribute the lane came from.
    pub road_id: String,
    /// The zero-based lane-section index within that road: the permanent
    /// layer's sections ordered by `s`, then the temporary layer's.
    pub section: usize,
    /// The lane layer the lane is on.
    #[cfg_attr(feature = "serde", serde(default))]
    pub layer: Layer,
    /// The original OpenDRIVE `<lane id>` (signed; negative right of the
    /// reference line, positive left).
    pub od_id: i32,
    /// The lane's height off the road at each vertex of its centerline,
    /// parallel to its [`Lane::center`] points, from its `<height>`s. Empty
    /// for a lane level with the road all along. The baked lane already
    /// stands at these heights.
    ///
    /// The spec holds each `<height>` until the next. These go straight from
    /// one to the next, as libOpenDRIVE and esmini read them, and hold the
    /// first before it and the last after it. A `<height>` missing `sOffset`,
    /// `inner` or `outer` reads it as 0, and a negative `sOffset` as 0.
    pub heights: Vec<LaneHeight>,
    /// Whether traffic from the lanes beside it may also use it, from its
    /// `advisory`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub advisory: LaneAdvisory,
    /// Whether a scenario may change its type while it runs, from
    /// `dynamicLaneType`. A stop lane that a message board turns into a
    /// driving lane is one. The crate never changes a lane.
    #[cfg_attr(feature = "serde", serde(default))]
    pub dynamic_lane_type: bool,
    /// Whether a scenario may change its direction, from
    /// `dynamicLaneDirection`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub dynamic_lane_direction: bool,
    /// Whether the lane is under construction, with access and driving
    /// restricted, from `roadWorks`. Its [`LaneType`] is unchanged.
    #[cfg_attr(feature = "serde", serde(default))]
    pub road_works: bool,
}

/// How far a lane's surface stands off the road at one station, from its
/// `<height>`s: along the road's normal, at the lane's inner and outer
/// border, in metres. The spec gives only these two. The lane goes straight
/// across from one to the other, as libOpenDRIVE and esmini read it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LaneHeight {
    /// At the border nearer the reference line.
    pub inner: f32,
    /// At the border further from it.
    pub outer: f32,
}

/// The OpenDRIVE identity of one baked object: which road and `<object>` it
/// came from, and where on that road it is anchored.
///
/// Like [`LaneProvenance`], this is kept apart from [`Object`] so the baked
/// object stays format-neutral. Everything here is relative to an OpenDRIVE
/// road.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectProvenance {
    /// The baked object this record describes.
    pub object: ObjectId,
    /// The `<road id>` the object is on. Empty for an object of a junction.
    pub road_id: String,
    /// The `<junction id>` whose `<objects>` hold the object, which stands
    /// along the junction's reference line. Empty for an object on a road.
    #[cfg_attr(feature = "serde", serde(default))]
    pub junction_id: String,
    /// The `<object id>` it came from. Every object a `<repeat>` or several
    /// outlines bake from one `<object>` shares it. Empty if the file gives
    /// none. For an object an `<objectReference>` placed, this is the id the
    /// reference names.
    pub od_id: String,
    /// Where on the road the object is anchored, in metres along the
    /// reference line: a solid's own station, the start of the part of a
    /// sweep on the road, or the `<object>`'s station for an outline.
    pub s: f64,
    /// The lateral offset from the reference line at that station, in metres,
    /// positive to the left.
    pub t: f64,
    /// Which direction of the road the object applies to.
    pub orientation: Orientation,
    /// How far along the road the object is valid from `s`, in metres, if
    /// the file says. Meant for objects such as a speed bump that act over a
    /// stretch of road.
    pub valid_length: Option<f64>,
    /// `Some` if an `<objectReference>` on `road_id` or in `junction_id`
    /// placed the object: the road its `<object>` is on, empty if that is in
    /// a junction. `s`, `t`, `orientation` and `valid_length`
    /// are then the reference's, and the shape is the `<object>`'s, moved to
    /// the reference's station.
    pub referenced_from: Option<String>,
    /// The `roadMarkColor` of each of the object's [`Object::materials`], in
    /// step with them, such as `white`. Free text, and empty where the map
    /// gives none.
    #[cfg_attr(feature = "serde", serde(default))]
    pub material_road_mark_colors: Vec<String>,
    /// The `weight` of each of the object's [`Object::markings`], in step
    /// with them. `Standard` where the map gives none, or gives a weight the
    /// spec does not allow.
    #[cfg_attr(feature = "serde", serde(default))]
    pub marking_weights: Vec<RoadMarkWeight>,
    /// Whether the map marks the object as no longer valid, from
    /// `invalidated`. A removed road sign is one. The crate still bakes it.
    #[cfg_attr(feature = "serde", serde(default))]
    pub invalidated: bool,
    /// Whether the object is there for a while only, such as a cone at road
    /// works, from `temporary`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub temporary: bool,
    /// The `fillType` of the `<outline>` it baked from, such as `grass`. Free
    /// text. Empty for a solid, and where the outline has none. A closed
    /// outline of `paint` lies flat on the road, and keeps its full area
    /// under a border, which the spec says cuts it back. An inner outline
    /// (`outer="false"`) with a `fillType` or a `laneType` bakes to an
    /// object of its own, which has them here.
    #[cfg_attr(feature = "serde", serde(default))]
    pub fill_type: String,
    /// The `laneType` of the `<outline>` it baked from, which the spec says
    /// is the lane type the outline's area is, if the outline gives one.
    #[cfg_attr(feature = "serde", serde(default))]
    pub outline_lane_type: Option<LaneType>,
    /// The `id`s of a [`Shape::Skeleton`]'s polylines and their vertices, in
    /// step with its lines. Empty for any other shape.
    #[cfg_attr(feature = "serde", serde(default))]
    pub skeleton_ids: Vec<SkeletonIds>,
}

/// The `id` of one `<skeleton>` `<polyline>` and of each of its vertices,
/// in order. Each is empty where the map gives none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SkeletonIds {
    /// The `<polyline id>`.
    pub polyline: String,
    /// The `id` of each `<vertexRoad>` or `<vertexLocal>`.
    pub vertices: Vec<String>,
}

/// Which direction of its road an object or a signal applies to, from its
/// `orientation`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Orientation {
    /// Traffic along +s, `orientation="+"`.
    Positive,
    /// Traffic along -s, `orientation="-"`.
    Negative,
    /// Both, `orientation="none"`. Also what a missing or unrecognised
    /// value reads as.
    #[default]
    Both,
}

/// The OpenDRIVE identity of one baked tunnel or bridge: which road and
/// `<tunnel>` or `<bridge>` it came from, and the stretch of road it spans.
///
/// Kept apart from [`Structure`] as [`ObjectProvenance`] is from [`Object`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StructureProvenance {
    /// The baked structure this record describes.
    pub structure: StructureId,
    /// The `<road id>` it is on.
    pub road_id: String,
    /// The `<tunnel id>` or `<bridge id>` it came from. Empty if the file
    /// gives none.
    pub od_id: String,
    /// Where it starts, in metres along the road's reference line.
    pub s: f64,
    /// How far along the road it runs from `s`, in metres.
    pub length: f64,
}

/// The OpenDRIVE identity of one baked signal: which road and `<signal>` it
/// came from, and where on that road it takes effect.
///
/// Kept apart from [`Signal`](crate::Signal) as [`ObjectProvenance`] is from
/// [`Object`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignalProvenance {
    /// The baked signal this record describes.
    pub signal: SignalId,
    /// The `<road id>` the signal is on.
    pub road_id: String,
    /// The `<signal id>` it came from. Empty if the file gives none. Meant to
    /// be unique, but real files repeat them.
    pub od_id: String,
    /// Where on the road it takes effect, in metres along the reference
    /// line.
    pub s: f64,
    /// The lateral offset from the reference line there, in metres, positive
    /// to the left.
    pub t: f64,
    /// Which direction of the road it applies to.
    pub orientation: Orientation,
    /// Each `<signalReference>` that applies the signal again, in file
    /// order. Its points follow the signal's own in
    /// [`Signal::applies_at`](crate::Signal::applies_at).
    pub references: Vec<SignalReferenceProvenance>,
    /// What each `<sign>` of its static boards says about itself, in file
    /// order.
    #[cfg_attr(feature = "serde", serde(default))]
    pub signs: Vec<SignProvenance>,
}

/// What a `<sign>` on a static board says about itself that the crate builds
/// nothing from. It turns a sign by none of its angles.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignProvenance {
    /// Which of the signal's [`Signal::boards`](crate::Signal::boards) it is
    /// on.
    pub board: usize,
    /// Its position among that board's signs.
    pub sign: usize,
    /// Its `id`. Empty if the map gives none.
    pub id: String,
    /// Its `countryRevision`. Empty if the map gives none.
    pub country_revision: String,
    /// Whether the map says it is `dynamic`.
    pub dynamic: bool,
    /// Whether the map marks it `invalidated`.
    pub invalidated: bool,
    /// Whether the map marks it `temporary`.
    pub temporary: bool,
    /// Which traffic it is for, from its `orientation`.
    pub orientation: Orientation,
    /// How far it is turned about the vertical from the board, from its
    /// `hOffset`, in radians in `(-π, π]`. 0 if the map gives none.
    pub h_offset: f32,
    /// Its `pitch`, about its lateral axis, as `h_offset`.
    pub pitch: f32,
    /// Its `roll`, about its own forward axis, as `h_offset`.
    pub roll: f32,
    /// Metres deep, if the map gives it.
    pub length: Option<f32>,
}

/// A `<signalReference>`: a road, and a station on it, where a signal also
/// applies. The signal itself stands where its `<signal>` puts it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignalReferenceProvenance {
    /// The `<road id>` the reference is on.
    pub road_id: String,
    /// Where on that road the signal takes effect, in metres along the
    /// reference line.
    pub s: f64,
    /// The lateral offset from the reference line there, in metres, positive
    /// to the left.
    pub t: f64,
    /// Which direction of that road it applies to.
    pub orientation: Orientation,
}

/// The OpenDRIVE identity of one baked signal controller: the top-level
/// `<controller>` it came from, and the junctions that list it.
///
/// Kept apart from [`Controller`](crate::Controller) as
/// [`SignalProvenance`] is from [`Signal`](crate::Signal).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ControllerProvenance {
    /// The baked controller this record describes.
    pub controller: ControllerId,
    /// The `<controller id>` it came from. Empty if the file gives none.
    pub od_id: String,
    /// Each `<junction>` whose own `<controller>` list names it, in file
    /// order. A junction's controllers are meant to switch in step.
    pub junctions: Vec<JunctionControllerProvenance>,
}

/// One entry of a `<junction>`'s `<controller>` list.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct JunctionControllerProvenance {
    /// The `<junction id>`.
    pub junction_id: String,
    /// How the junction uses the controller. Free text, and empty if the
    /// file gives none.
    pub kind: String,
    /// Its priority among the junction's controllers, if the file gives one.
    pub sequence: Option<u32>,
}

/// The OpenDRIVE identity of one baked road mark: which road, lane section
/// and `<lane>` its `<roadMark>` is on, and the stretch of road it covers.
///
/// Kept apart from [`RoadMark`](crate::RoadMark) as [`LaneProvenance`] is
/// from [`Lane`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadMarkProvenance {
    /// The baked road mark this record describes.
    pub road_mark: RoadMarkId,
    /// The `<road id>` it is on.
    pub road_id: String,
    /// The zero-based lane-section index within that road, as in
    /// [`LaneProvenance::section`].
    pub section: usize,
    /// The `<lane id>` the `<roadMark>` is under: 0 for the center lane's,
    /// the line between the two sides, and otherwise the lane whose outer
    /// border it runs along.
    pub od_lane_id: i32,
    /// Where it starts, in metres along the road's reference line: its lane
    /// section's start plus its `sOffset`. The spec requires `sOffset`, and
    /// says it is at least 0. A mark without one, or with a negative one,
    /// starts at its section.
    pub s: f64,
    /// How far along the road it runs from `s`, in metres: to the lane's
    /// next `<roadMark>`, or the end of the lane section. The spec says a
    /// lane's marks come in ascending `sOffset`, and ones out of order are
    /// sorted rather than dropped.
    pub length: f64,
    /// What it is made of, from its `material`. Free text, and empty if the
    /// map gives none.
    #[cfg_attr(feature = "serde", serde(default))]
    pub material: String,
    /// What the map calls its type, from its `<type>`'s `name`. Free text,
    /// and empty if the map gives none.
    #[cfg_attr(feature = "serde", serde(default))]
    pub type_name: String,
}

/// The OpenDRIVE identity of everything a load baked, from
/// [`load_str_with_provenance`] or [`load_file_with_provenance`].
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Provenance {
    /// One per baked lane, in baked-lane order.
    pub lanes: Vec<LaneProvenance>,
    /// One per lane section with a `<center>` lane, in file order.
    #[cfg_attr(feature = "serde", serde(default))]
    pub center_lanes: Vec<CenterLaneProvenance>,
    /// One per baked object, in baked-object order.
    pub objects: Vec<ObjectProvenance>,
    /// One per baked tunnel or bridge, in baked-structure order.
    pub structures: Vec<StructureProvenance>,
    /// One per baked signal, in baked-signal order.
    pub signals: Vec<SignalProvenance>,
    /// One per baked controller, in baked-controller order.
    pub controllers: Vec<ControllerProvenance>,
    /// One per baked road mark, in baked-road-mark order.
    pub road_marks: Vec<RoadMarkProvenance>,
    /// One per junction priority, in step with
    /// [`RoadNetwork::priorities`](crate::RoadNetwork::priorities).
    #[cfg_attr(feature = "serde", serde(default))]
    pub priorities: Vec<PriorityProvenance>,
    /// One per cross path, in step with
    /// [`RoadNetwork::cross_paths`](crate::RoadNetwork::cross_paths).
    #[cfg_attr(feature = "serde", serde(default))]
    pub cross_paths: Vec<CrossPathProvenance>,
    /// One per `<junction>` with an `id`, in file order.
    #[cfg_attr(feature = "serde", serde(default))]
    pub junctions: Vec<JunctionProvenance>,
    /// One per junction `<laneLink>` that gives an `overlapZone`, in file
    /// order.
    #[cfg_attr(feature = "serde", serde(default))]
    pub lane_link_overlaps: Vec<LaneLinkOverlap>,
    /// What the load dropped, or read against the spec, in file order.
    /// Empty for a clean file.
    pub warnings: Vec<Warning>,
}

/// What a lane section's center lane, lane 0, says about itself. It never
/// bakes to a [`Lane`], so its attributes are kept here.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CenterLaneProvenance {
    /// The `<road id>` it is on.
    pub road_id: String,
    /// The zero-based lane-section index within that road, as in
    /// [`LaneProvenance::section`].
    pub section: usize,
    /// Its `id`, which the spec says is 0. `None` if the file gives none or
    /// one that is not a whole number.
    #[cfg_attr(feature = "serde", serde(default))]
    pub od_id: Option<i32>,
    /// Its `type`, which the spec says is `none`. A missing or unrecognised
    /// one is [`LaneType::Unknown`].
    pub kind: LaneType,
    /// Its `level`, which the crate reads for no lane but the center one.
    pub level: bool,
    /// The `id` of the lane its `<link><predecessor>` names, in the road's
    /// previous lane section or the road before. The lane graph does not
    /// follow it, since the center lane is no [`Lane`].
    #[cfg_attr(feature = "serde", serde(default))]
    pub predecessor: Option<i32>,
    /// The `id` its `<link><successor>` names, as `predecessor`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub successor: Option<i32>,
}

/// A junction `<laneLink>`'s `overlapZone`: how far the traffic of two
/// lanes that overlap shares the space, which the spec intends for direct
/// junctions. The crate keeps it and uses none of it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LaneLinkOverlap {
    /// The `<junction id>`.
    pub junction_id: String,
    /// The `<connection id>`. Empty if the file gives none.
    pub connection_id: String,
    /// The connection's `incomingRoad`. Empty if the file gives none.
    pub incoming_road: String,
    /// The connection's `connectingRoad`, or its `linkedRoad` in a direct
    /// junction. Empty if the file gives neither.
    pub connecting_road: String,
    /// The link's `from` lane, as the file writes the `<lane id>` on the
    /// incoming road.
    pub from: i32,
    /// The link's `to` lane, on the connecting road.
    pub to: i32,
    /// Its `overlapZone`, in metres along `s` from the junction.
    pub overlap_zone: f64,
}

/// What a `<junction>` says about itself: its `id`, and the `name` the
/// network keeps nowhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct JunctionProvenance {
    /// The `<junction id>`.
    pub od_id: String,
    /// Its `name`. Empty if it has none.
    pub name: String,
}

/// The OpenDRIVE identity of one cross path: the `<junction>` and the
/// `<crossPath id>` it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrossPathProvenance {
    /// The `<junction id>`.
    pub junction_id: String,
    /// The `<crossPath id>`, empty if it has none.
    pub od_id: String,
}

/// The OpenDRIVE identity of one junction priority: the `<junction>` whose
/// `<priority>` gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PriorityProvenance {
    /// The `<junction id>`.
    pub junction_id: String,
}

/// Load an OpenDRIVE file from disk and bake it into a `RoadNetwork`.
pub fn load_file(path: impl AsRef<std::path::Path>) -> Result<RoadNetwork, ImportError> {
    load_file_with_provenance(path).map(|(net, _)| net)
}

/// Bake an OpenDRIVE document (as a string) into a `RoadNetwork`.
pub fn load_str(xml: &str) -> Result<RoadNetwork, ImportError> {
    load_str_with_provenance(xml).map(|(net, _)| net)
}

/// Like [`load_file`], but also returns the OpenDRIVE [`Provenance`] of
/// every baked lane, object, structure, signal, controller and road mark, for a viewer
/// or editor that must name the road and original id each one came from.
pub fn load_file_with_provenance(
    path: impl AsRef<std::path::Path>,
) -> Result<(RoadNetwork, Provenance), ImportError> {
    let xml = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ImportError::Malformed(format!("reading {:?}: {e}", path.as_ref())))?;
    load_str_with_provenance(&xml)
}

/// Like [`load_str`], but also returns the OpenDRIVE [`Provenance`] of
/// every baked lane, object, structure, signal, controller and road mark. Each record
/// carries the id of what it describes.
pub fn load_str_with_provenance(xml: &str) -> Result<(RoadNetwork, Provenance), ImportError> {
    let cleaned = sanitize(xml);
    if deeper_than(&cleaned, MAX_DEPTH) {
        return Err(ImportError::Malformed(format!(
            "elements nested more than {MAX_DEPTH} deep"
        )));
    }
    let doc = roxmltree::Document::parse(cleaned.as_ref())?;
    let root = doc.root_element();
    let mut lanes = Vec::new();
    let mut objects = Objects::default();
    let mut structures = Structures::default();
    let mut surfaces = Vec::new();
    let mut topo = Topology::default();
    let index = object_index(root);
    let mut roads = Vec::new();
    let mut warnings = Vec::new();
    let mut length_left = MAX_BAKED_LENGTH;
    for road in root.children().filter(|n| n.has_tag_name("road")) {
        let length = attr_f64(road, "length")
            .unwrap_or(0.0)
            .clamp(0.0, MAX_LENGTH);
        let cost = lane_length(road, length);
        let parsed = match cost > length_left {
            true => Err(RoadSkipReason::MapTooLarge),
            false => parse_road(
                road,
                &index,
                &mut lanes,
                &mut objects,
                &mut structures,
                &mut topo,
            ),
        };
        match parsed {
            Ok(mut baked) => {
                length_left -= cost;
                for node in child(road, "surface")
                    .into_iter()
                    .flat_map(|n| n.children())
                    .filter(|n| n.has_tag_name("CRG"))
                {
                    surfaces.extend(road_crg(node, &baked.road, &mut length_left));
                }
                baked.road.id = RoadId(roads.len());
                if let Err(rule) = traffic_rule(road) {
                    warnings.push(Warning::UnknownTrafficRule {
                        road_id: baked.road.od_id.clone(),
                        rule: rule.to_string(),
                    });
                }
                for (section, def) in baked.sections() {
                    warnings.extend(baked.lane_warnings(section, def));
                }
                warnings.extend(baked.warnings.iter().cloned());
                warnings.extend(baked.shape_warnings());
                warnings.extend(gaps::road_length(&baked.road));
                warnings.extend(baked.speed_warnings());
                warnings.extend(baked.access_warnings());
                warnings.extend(baked.visibility_warnings());
                roads.push((road, baked));
            }
            Err(reason) => warnings.push(Warning::RoadSkipped {
                road_id: road.attribute("id").unwrap_or_default().to_string(),
                reason,
            }),
        }
    }
    if lanes.is_empty() {
        return Err(ImportError::Malformed("no lanes found".into()));
    }
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        let Some(objects_node) = child(junction, "objects") else {
            continue;
        };
        match junction_line(junction) {
            Some(line) => {
                let id = junction.attribute("id");
                place_objects(objects_node, &index, &line, id, &mut objects);
            }
            None => warnings.push(Warning::JunctionObjectsDropped {
                junction_id: junction.attribute("id").unwrap_or_default().to_string(),
            }),
        }
    }
    surfaces.extend(junction_crgs(root, &topo));
    let placed = std::mem::take(&mut objects.surfaces);
    let object_surfaces = object_crgs(root, &topo, &surfaces, placed, &mut warnings);
    surfaces.extend(object_surfaces);
    let signals = signals::place(root, &roads, &lanes, &objects.provenance);
    let mut road_marks = road_marks::place(&mut roads);
    warnings.append(&mut road_marks.warnings);
    let properties = properties::place(roads.iter().map(|(_, road)| road), &lanes);
    let (neighbors, dropped_neighbors) = road_neighbors(&roads);
    warnings.extend(dropped_neighbors);
    let (areas, area_warnings) = junction_areas::place(root, &roads);
    warnings.extend(area_warnings);
    let (cross_paths, cross_path_provenance, cross_path_warnings) =
        junction_areas::cross_paths(root, &roads);
    warnings.extend(cross_path_warnings);
    let (groups, group_warnings) = junction_areas::groups(root);
    warnings.extend(group_warnings);
    let (switches, switch_warnings) = railway::switches(&roads);
    warnings.extend(switch_warnings);
    let (stations, station_warnings) = railway::stations(root, &roads);
    warnings.extend(station_warnings);
    // Resolve connectivity once all lanes exist and are registered.
    let (junctions, dropped) = links::junctions(root, &mut topo.roads);
    topo.junctions = junctions;
    warnings.extend(dropped);
    let (priorities, priority_provenance, dropped_priorities) = priorities(root, &roads);
    warnings.extend(dropped_priorities);
    links::resolve(&mut lanes, &topo);
    let (virtual_junctions, virtual_warnings) = virtual_junctions::place(root, &roads, &topo);
    warnings.extend(virtual_warnings);
    warnings.extend(gaps::link_gaps(&lanes, &topo.metas));
    // The parser already recorded each lane's OpenDRIVE origin while baking;
    // surface it rather than reconstructing lane-id order downstream.
    warnings.append(&mut objects.warnings);
    warnings.extend(ignored::place(root));
    let provenance = Provenance {
        center_lanes: roads
            .iter()
            .flat_map(|(_, road)| {
                road.sections().filter_map(|(section, def)| {
                    let center = def.center_lane?;
                    Some(CenterLaneProvenance {
                        road_id: road.road.od_id.clone(),
                        section: section.index,
                        od_id: center.od_id,
                        kind: center.kind,
                        level: center.level,
                        predecessor: center.predecessor,
                        successor: center.successor,
                    })
                })
            })
            .collect(),
        lanes: topo
            .metas
            .iter()
            .map(|m| LaneProvenance {
                lane: m.id,
                road_id: m.road.clone(),
                section: m.section,
                layer: m.layer,
                od_id: m.od_id,
                heights: m.heights.clone(),
                advisory: m.flags.advisory,
                dynamic_lane_type: m.flags.dynamic_type,
                dynamic_lane_direction: m.flags.dynamic_direction,
                road_works: m.flags.road_works,
            })
            .collect(),
        objects: objects.provenance,
        structures: structures.provenance,
        signals: signals.provenance,
        controllers: signals.controller_provenance,
        road_marks: road_marks.provenance,
        junctions: root
            .children()
            .filter(|n| n.has_tag_name("junction"))
            .filter_map(|n| {
                Some(JunctionProvenance {
                    od_id: n.attribute("id")?.to_string(),
                    name: n.attribute("name").unwrap_or_default().to_string(),
                })
            })
            .collect(),
        lane_link_overlaps: lane_link_overlaps(root),
        priorities: priority_provenance,
        cross_paths: cross_path_provenance,
        warnings,
    };
    Ok((
        RoadNetwork::new(lanes)
            .with_objects(objects.baked)
            .with_structures(structures.baked)
            .with_signals(signals.baked)
            .with_controllers(signals.controllers)
            .with_road_marks(road_marks.baked)
            .with_crg_surfaces(surfaces)
            .with_speed_limits(properties.speed_limits)
            .with_road_types(properties.road_types)
            .with_road_countries(properties.road_countries)
            .with_lane_rules(properties.lane_rules)
            .with_lane_access(properties.lane_access)
            .with_lane_materials(properties.lane_materials)
            .with_lane_visibility(properties.lane_visibility)
            .with_geo_reference(geo_reference(root))
            .with_header(header(root))
            .with_roads(roads.into_iter().map(|(_, baked)| baked.road).collect())
            .with_priorities(priorities)
            .with_road_neighbors(neighbors)
            .with_junction_areas(areas)
            .with_cross_paths(cross_paths)
            .with_junction_groups(groups)
            .with_virtual_junctions(virtual_junctions)
            .with_railways(switches, stations),
        provenance,
    ))
}

/// Every junction `<priority>` between two baked roads, in file order, with
/// the junction each is in, and a warning for each that names a road the
/// load has none of.
fn priorities(
    root: roxmltree::Node,
    roads: &[(roxmltree::Node, BakedRoad)],
) -> (Vec<Priority>, Vec<PriorityProvenance>, Vec<Warning>) {
    let road = |id: Option<&str>| {
        roads
            .iter()
            .find(|(_, r)| Some(r.road.od_id.as_str()) == id)
            .map(|(_, r)| r.road.id)
    };
    let (mut out, mut provenance, mut warnings) = (Vec::new(), Vec::new(), Vec::new());
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        let junction_id = junction.attribute("id").unwrap_or_default().to_string();
        for p in junction.children().filter(|n| n.has_tag_name("priority")) {
            let (high, low) = (p.attribute("high"), p.attribute("low"));
            match (road(high), road(low)) {
                (Some(high), Some(low)) => {
                    out.push(Priority { high, low });
                    provenance.push(PriorityProvenance {
                        junction_id: junction_id.clone(),
                    });
                }
                (found_high, _) => warnings.push(Warning::PriorityDropped {
                    junction_id: junction_id.clone(),
                    high: high.unwrap_or_default().to_string(),
                    low: low.unwrap_or_default().to_string(),
                    road_id: if found_high.is_some() { high } else { low }
                        .unwrap_or_default()
                        .to_string(),
                }),
            }
        }
    }
    (out, provenance, warnings)
}

/// Every road `<neighbor>` naming a baked road, road by road in file order,
/// and a warning for each the crate can't read. The spec that defined the
/// element required `side`, `elementId` and `direction`.
fn road_neighbors(roads: &[(roxmltree::Node, BakedRoad)]) -> (Vec<RoadNeighbor>, Vec<Warning>) {
    let id_of = |od_id: &str| {
        roads
            .iter()
            .find(|(_, r)| r.road.od_id == od_id)
            .map(|(_, r)| r.road.id)
    };
    let (mut out, mut warnings) = (Vec::new(), Vec::new());
    for (node, baked) in roads {
        let neighbors = child(*node, "link")
            .into_iter()
            .flat_map(|l| l.children())
            .filter(|n| n.has_tag_name("neighbor"));
        for n in neighbors {
            let text = |name| n.attribute(name).unwrap_or_default();
            let side = match text("side") {
                "left" => Some(Side::Left),
                "right" => Some(Side::Right),
                _ => None,
            };
            let same_direction = match text("direction") {
                "same" => Some(true),
                "opposite" => Some(false),
                _ => None,
            };
            match (id_of(text("elementId")), side, same_direction) {
                (Some(neighbor), Some(side), Some(same_direction)) => out.push(RoadNeighbor {
                    road: baked.road.id,
                    neighbor,
                    side,
                    same_direction,
                }),
                _ => warnings.push(Warning::NeighborDropped {
                    road_id: baked.road.od_id.clone(),
                    neighbor_id: text("elementId").to_string(),
                    side: text("side").to_string(),
                    direction: text("direction").to_string(),
                }),
            }
        }
    }
    (out, warnings)
}

/// Every `<junction>` `<CRG>`, over the lanes of the roads in its junction.
fn junction_crgs(root: roxmltree::Node, topo: &Topology) -> Vec<CrgSurface> {
    let mut surfaces = Vec::new();
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        let lanes = junction_lanes(root, topo, junction.attribute("id").unwrap_or_default());
        for node in child(junction, "surface")
            .into_iter()
            .flat_map(|n| n.children())
            .filter(|n| n.has_tag_name("CRG"))
        {
            surfaces.extend(junction_crg(node, lanes.clone()));
        }
    }
    surfaces
}

/// The lanes of every road in the junction `id`.
fn junction_lanes(root: roxmltree::Node, topo: &Topology, id: &str) -> Vec<LaneId> {
    let roads: Vec<&str> = root
        .children()
        .filter(|n| n.has_tag_name("road") && n.attribute("junction") == Some(id))
        .filter_map(|n| n.attribute("id"))
        .collect();
    topo.metas
        .iter()
        .filter(|m| roads.contains(&m.road.as_str()))
        .map(|m| m.id)
        .collect()
}

/// The object `<surface>` CRGs, after the road and junction ones in
/// `surfaces`. A junction's object's covers the lanes of the junction's
/// roads. One that hides the road's CRGs over one that replaces the road's
/// height, on a lane they share, raises [`Warning::CrgHideIgnored`]. On a
/// road they must overlap along it too. A junction's object stands along the
/// junction's line, which measures `s` apart from its roads, so there a
/// shared lane is enough.
fn object_crgs(
    root: roxmltree::Node,
    topo: &Topology,
    surfaces: &[CrgSurface],
    objects: Vec<ObjectCrg>,
    warnings: &mut Vec<Warning>,
) -> Vec<CrgSurface> {
    let replaces = |road: &CrgSurface, object: &ObjectCrg| {
        let (from, to) = object.stretch;
        let on_junction = !object.junction_id.is_empty();
        let along = on_junction
            || road.road.as_ref().is_none_or(|r| {
                let (first, last) = (r.stations.first(), r.stations.last());
                first
                    .zip(last)
                    .is_some_and(|(a, b)| a.s <= to && from <= b.s)
            });
        road.purpose == CrgPurpose::Elevation
            && matches!(
                road.mode,
                CrgMode::Attached0(_) | CrgMode::Genuine { .. } | CrgMode::Global { .. }
            )
            && along
            && road.lanes.iter().any(|l| object.surface.lanes.contains(l))
    };
    objects
        .into_iter()
        .map(|mut object| {
            if !object.junction_id.is_empty() {
                object.surface.lanes = junction_lanes(root, topo, &object.junction_id);
            }
            let hides = matches!(object.surface.mode, CrgMode::Object(o) if o.hide_road);
            if hides && surfaces.iter().any(|road| replaces(road, &object)) {
                warnings.push(Warning::CrgHideIgnored {
                    road_id: object.road_id,
                    junction_id: object.junction_id,
                    object_id: object.object_id,
                });
            }
            object.surface
        })
        .collect()
}

/// The deepest the crate lets elements nest. The XML parser recurses once
/// for each level, so a file nested tens of thousands deep would overflow
/// the stack. OpenDRIVE nests about ten deep, vendor `<userData>` a little
/// more.
const MAX_DEPTH: usize = 256;

/// Whether elements in `xml` nest more than `limit` deep, from a scan of its
/// tags that skips comments, CDATA, processing instructions, the doctype and
/// quoted attribute values.
fn deeper_than(xml: &str, limit: usize) -> bool {
    let b = xml.as_bytes();
    let find = |from: usize, pat: &[u8]| {
        b[from.min(b.len())..]
            .windows(pat.len())
            .position(|w| w == pat)
            .map_or(b.len(), |k| from + k + pat.len())
    };
    let (mut i, mut depth) = (0, 0usize);
    while let Some(k) = b[i..].iter().position(|&c| c == b'<') {
        let at = i + k + 1;
        let rest = &b[at..];
        i = if rest.starts_with(b"/") {
            depth = depth.saturating_sub(1);
            find(at, b">")
        } else if rest.starts_with(b"!--") {
            find(at, b"-->")
        } else if rest.starts_with(b"![CDATA[") {
            find(at, b"]]>")
        } else if rest.starts_with(b"?") {
            find(at, b"?>")
        } else if rest.starts_with(b"!") {
            let close = find(at, b">");
            match b[at..close].contains(&b'[') {
                true => find(at, b"]>"),
                false => close,
            }
        } else {
            let mut j = at;
            let mut quote = None;
            while j < b.len() {
                match (quote, b[j]) {
                    (None, b'"' | b'\'') => quote = Some(b[j]),
                    (Some(q), c) if c == q => quote = None,
                    (None, b'>') => break,
                    _ => {}
                }
                j += 1;
            }
            if b.get(j.wrapping_sub(1)) != Some(&b'/') {
                depth += 1;
                if depth > limit {
                    return true;
                }
            }
            j + 1
        };
        if i >= b.len() {
            break;
        }
    }
    false
}

/// Make a real-world document parseable: strip a UTF-8 BOM and remove the
/// `<?xml ... ?>` declaration. Tools (e.g. CARLA) emit a license comment
/// *before* the declaration, which is malformed XML that strict parsers reject;
/// the declaration only names version/encoding, which we don't need for UTF-8.
fn sanitize(xml: &str) -> std::borrow::Cow<'_, str> {
    let xml = xml.trim_start_matches('\u{feff}');
    if let Some(start) = xml.find("<?xml") {
        if let Some(rel_end) = xml[start..].find("?>") {
            let mut out = String::with_capacity(xml.len());
            out.push_str(&xml[..start]);
            out.push_str(&xml[start + rel_end + 2..]);
            return std::borrow::Cow::Owned(out);
        }
    }
    std::borrow::Cow::Borrowed(xml)
}

/// One lane parsed from a `<laneSection>`, before it is sampled into a [`Lane`].
/// Where it lies across the road is its [`LaneGeom`], which goes to the
/// [`Road`].
#[derive(Clone)]
struct LaneDef {
    id: i32,
    kind: LaneType,
    pred_links: Vec<links::LaneRef>,
    succ_links: Vec<links::LaneRef>,
    /// The road marks on its outer border.
    marks: Vec<MarkDef>,
    speeds: Vec<properties::LaneSpeedDef>,
    rules: Vec<(f64, String)>,
    access: Vec<(f64, properties::AccessDef)>,
    materials: Vec<(f64, Material)>,
    visibility: Vec<(f64, properties::VisibilityDef)>,
    /// Its `direction`, or its value if the crate can't read it.
    direction: Result<LaneDirection, String>,
    flags: LaneFlags,
}

/// What a `<lane>` says about itself that the crate builds nothing from.
#[derive(Clone, Copy, Default)]
pub(crate) struct LaneFlags {
    advisory: LaneAdvisory,
    dynamic_type: bool,
    dynamic_direction: bool,
    road_works: bool,
}

impl LaneFlags {
    fn parse(lane: roxmltree::Node) -> Self {
        let flag = |name| matches!(lane.attribute(name), Some("true" | "1"));
        Self {
            advisory: match lane.attribute("advisory") {
                Some("inner") => LaneAdvisory::Inner,
                Some("outer") => LaneAdvisory::Outer,
                Some("both") => LaneAdvisory::Both,
                _ => LaneAdvisory::None,
            },
            dynamic_type: flag("dynamicLaneType"),
            dynamic_direction: flag("dynamicLaneDirection"),
            road_works: flag("roadWorks"),
        }
    }
}

/// A lane's `direction`: which way it runs against the way its side of the
/// road does.
#[derive(Clone, Copy)]
enum LaneDirection {
    Standard,
    Reversed,
    Both,
}

impl LaneDirection {
    /// A lane's `direction`. A missing one is `standard`, as the spec says,
    /// but `both` on a lane of the deprecated `type="bidirectional"`, which
    /// the spec says `direction="both"` replaces.
    fn parse(lane: roxmltree::Node) -> Result<Self, String> {
        match lane.attribute("direction") {
            None if lane.attribute("type") == Some("bidirectional") => Ok(Self::Both),
            None | Some("standard") => Ok(Self::Standard),
            Some("reversed") => Ok(Self::Reversed),
            Some("both") => Ok(Self::Both),
            Some(other) => Err(other.to_string()),
        }
    }

    /// Which way a lane with this `direction` runs, on a side whose traffic
    /// runs `side`.
    fn apply(self, side: Direction) -> Direction {
        match (self, side) {
            (Self::Both, _) => Direction::Both,
            (Self::Standard, side) => side,
            (Self::Reversed, Direction::Forward) => Direction::Backward,
            (Self::Reversed, _) => Direction::Forward,
        }
    }
}

/// A `<lateralProfile>`'s `<shape>`s, grouped into profiles by `s`, in
/// ascending `s`, each sorted by `t`.
///
/// The spec requires every attribute, and says the shapes come in ascending
/// `s`, then `t`. A shape without `s` or `t` is skipped, a missing `a`, `b`,
/// `c` or `d` is 0, as for superelevation, and shapes out of order are
/// sorted rather than dropped.
fn shape_profiles(lateral: roxmltree::Node) -> Vec<ShapeProfile> {
    let mut shapes: Vec<(f64, Cubic)> = lateral
        .children()
        .filter(|n| n.has_tag_name("shape"))
        .filter_map(|n| {
            let coeff = |name| attr_f64(n, name).unwrap_or(0.0);
            Some((
                attr_f64(n, "s")?,
                Cubic {
                    start: attr_f64(n, "t")?,
                    a: coeff("a"),
                    b: coeff("b"),
                    c: coeff("c"),
                    d: coeff("d"),
                },
            ))
        })
        .collect();
    shapes.sort_by(|(s1, c1), (s2, c2)| s1.total_cmp(s2).then(c1.start.total_cmp(&c2.start)));
    let mut profiles: Vec<ShapeProfile> = Vec::new();
    for (s, shape) in shapes {
        match profiles.last_mut() {
            Some(p) if p.s == s => p.shapes.push(shape),
            _ => profiles.push(ShapeProfile {
                s,
                shapes: vec![shape],
            }),
        }
    }
    profiles
}

/// A `<crossSectionSurface>`: its `<tOffset>`, and its `<strip>`s by `id`.
/// A strip with an `id` other than 1, 2, -1 or -2 is skipped. The spec
/// gives no default `mode` for an outer strip. The crate reads a missing one
/// as `independent`, and one it doesn't know too, with a
/// [`Warning::UnknownStripMode`] for `road_id`.
fn cross_section(
    node: roxmltree::Node,
    road_id: &str,
    warnings: &mut Vec<Warning>,
) -> CrossSection {
    let records = |parent: Option<roxmltree::Node>| {
        parent.map_or_else(Vec::new, |p| cubics_in(p, "coefficients", "s"))
    };
    let mut out = CrossSection {
        t_offset: records(child(node, "tOffset")),
        ..CrossSection::default()
    };
    let strips = child(node, "surfaceStrips")
        .into_iter()
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name("strip"));
    let dropped = |warnings: &mut Vec<Warning>, strip: String| {
        warnings.push(Warning::StripDropped {
            road_id: road_id.to_string(),
            strip,
        })
    };
    for strip in strips {
        let id = attr_i32(strip, "id");
        let slot = match id {
            Some(1) => &mut out.left[0],
            Some(2) => &mut out.left[1],
            Some(-1) => &mut out.right[0],
            Some(-2) => &mut out.right[1],
            _ => {
                dropped(
                    warnings,
                    strip.attribute("id").unwrap_or_default().to_string(),
                );
                continue;
            }
        };
        if slot.is_some() {
            dropped(warnings, id.unwrap_or_default().to_string());
            continue;
        }
        let outer = id.is_some_and(|id| id.abs() == 2);
        let relative = match strip.attribute("mode") {
            Some("relative") if outer => true,
            _ if !outer => false,
            None | Some("independent") => false,
            Some(other) => {
                warnings.push(Warning::UnknownStripMode {
                    road_id: road_id.to_string(),
                    strip: id.unwrap_or_default(),
                    mode: other.to_string(),
                });
                false
            }
        };
        *slot = Some(Strip {
            width: records(child(strip, "width")),
            terms: [
                records(child(strip, "constant")),
                records(child(strip, "linear")),
                records(child(strip, "quadratic")),
                records(child(strip, "cubic")),
            ],
            relative,
        });
    }
    for (side, id) in [(&mut out.left, 2), (&mut out.right, -2)] {
        let beside = side[0]
            .as_ref()
            .is_some_and(|inner| !inner.width.is_empty());
        if side[1].is_some() && !beside {
            side[1] = None;
            dropped(warnings, id.to_string());
        }
    }
    out
}

/// A lane's `<height>`s, in order along it.
///
/// The spec requires `sOffset`, `inner` and `outer`, says `sOffset` is at
/// least 0, and says the entries come in ascending `sOffset`. A missing one
/// is 0, as libOpenDRIVE reads it. A negative `sOffset` is 0, and entries out
/// of order are sorted rather than dropped.
fn parse_heights(lane: roxmltree::Node) -> Vec<HeightDef> {
    let mut heights: Vec<HeightDef> = lane
        .children()
        .filter(|n| n.has_tag_name("height"))
        .map(|h| HeightDef {
            s_offset: attr_f64(h, "sOffset").unwrap_or(0.0).max(0.0),
            inner: attr_f64(h, "inner").unwrap_or(0.0),
            outer: attr_f64(h, "outer").unwrap_or(0.0),
        })
        .collect();
    heights.sort_by(|a, b| a.s_offset.total_cmp(&b.s_offset));
    heights
}

/// A `<center>` lane as the file gives it.
#[derive(Clone, Copy)]
struct CenterDef {
    od_id: Option<i32>,
    kind: LaneType,
    level: bool,
    predecessor: Option<i32>,
    successor: Option<i32>,
}

/// A `<laneSection>`'s lanes as the file gives them. Only lanes with a
/// width or a border are here, since a lane with neither has nothing to
/// sample. The ids of those are in `dropped`.
struct SectionDef {
    /// The left lanes, ordered from the center outward: 1, 2, 3, ...
    left: Vec<LaneDef>,
    /// The right lanes, ordered from the center outward: -1, -2, -3, ...
    right: Vec<LaneDef>,
    /// Where the left and right lanes lie across the road, in step with
    /// `left` and `right`, until the [`RoadSection`] takes them.
    geoms: [Vec<LaneGeom>; 2],
    /// The road marks of the center lane, on the line between the sides.
    center: Vec<MarkDef>,
    /// The center lane's `type` and `level`, if the section has one.
    center_lane: Option<CenterDef>,
    /// The `<lane id>`s it skipped for having neither a width nor a border,
    /// in file order.
    dropped: Vec<i32>,
    /// The `id`s, as the file writes them, of the lanes under `<left>` or
    /// `<right>` it skipped for an `id` that isn't a whole number other than
    /// 0, in file order.
    bad_ids: Vec<String>,
    /// The `<lane id>`s with borders, if any lane has widths, in file order.
    mixed: Vec<i32>,
}

impl SectionDef {
    /// Its lane with `<lane id>` `od_id`, if that has a width or a border.
    fn lane(&self, od_id: i32) -> Option<&LaneDef> {
        self.left.iter().chain(&self.right).find(|l| l.id == od_id)
    }

    fn parse(section: roxmltree::Node) -> Self {
        let (mut left, mut right, mut dropped) = (Vec::new(), Vec::new(), Vec::new());
        let mut bad_ids = Vec::new();
        let (mut with_widths, mut with_borders) = (false, Vec::new());
        for side in ["left", "right"] {
            let Some(side_node) = child(section, side) else {
                continue;
            };
            for lane in side_node.children().filter(|n| n.has_tag_name("lane")) {
                let id = attr_i32(lane, "id");
                let Some(id) = id.filter(|&id| id != 0) else {
                    bad_ids.push(lane.attribute("id").unwrap_or_default().to_string());
                    continue;
                };
                let widths = lane_cubics(lane.children().filter(|n| n.has_tag_name("width")));
                let borders = lane_cubics(lane.children().filter(|n| n.has_tag_name("border")));
                with_widths |= !widths.is_empty();
                if !borders.is_empty() {
                    with_borders.push(id);
                }
                let extent = match (widths.is_empty(), borders.is_empty()) {
                    (false, _) => LaneExtent::Width(widths),
                    (true, false) => LaneExtent::Border(borders),
                    (true, true) => {
                        dropped.push(id);
                        continue;
                    }
                };
                let (pred_links, succ_links) = links::lane_refs(lane);
                let def = LaneDef {
                    id,
                    kind: lane_type(lane.attribute("type")),
                    pred_links,
                    succ_links,
                    marks: road_marks::parse(lane),
                    speeds: properties::lane_speeds(lane),
                    rules: properties::lane_rules(lane),
                    access: properties::lane_access(lane),
                    materials: properties::lane_materials(lane),
                    visibility: properties::lane_visibility(lane),
                    direction: LaneDirection::parse(lane),
                    flags: LaneFlags::parse(lane),
                };
                let geom = LaneGeom {
                    id,
                    extent,
                    heights: parse_heights(lane),
                    level: matches!(lane.attribute("level"), Some("true" | "1")),
                };
                let def = (def, geom);
                if id > 0 {
                    left.push(def);
                } else {
                    right.push(def);
                }
            }
        }
        let mixed = if with_widths {
            with_borders
        } else {
            Vec::new()
        };
        left.sort_by_key(|(l, _)| l.id);
        right.sort_by_key(|(l, _)| -l.id);
        let (left, left_geoms) = left.into_iter().unzip();
        let (right, right_geoms) = right.into_iter().unzip();
        let center_node = child(section, "center")
            .into_iter()
            .flat_map(|c| c.children())
            .find(|n| n.has_tag_name("lane"));
        let center = center_node.map(road_marks::parse).unwrap_or_default();
        let center_lane = center_node.map(|n| {
            let (predecessor, successor) = links::lane_link(n);
            CenterDef {
                od_id: n.attribute("id").and_then(|v| v.trim().parse().ok()),
                kind: lane_type(n.attribute("type")),
                level: matches!(n.attribute("level"), Some("true" | "1")),
                predecessor,
                successor,
            }
        });
        Self {
            left,
            right,
            geoms: [left_geoms, right_geoms],
            center,
            center_lane,
            dropped,
            bad_ids,
            mixed,
        }
    }
}

/// The [`LaneType`] an OpenDRIVE `<lane>` `type` maps to. A new lane type is
/// one arm here.
///
/// Every name maps to something. An absent or unrecognised type becomes
/// [`LaneType::Unknown`], so every `<lane>` carrying a width becomes a [`Lane`].
/// The importer used to drop the types it had no variant for, which left holes
/// in the surface where `none` and vendor-specific lanes belonged. Town07 alone
/// lost 27 lanes that way, averaging 3.5 m wide.
///
/// `mwyEntry` and `mwyExit` are the older spellings of `entry` and `exit`, and
/// `sidewalk` the older spelling of `walking`, which OpenDRIVE 1.8 brought in.
/// Each pair lands on one variant.
fn lane_type(od_type: Option<&str>) -> LaneType {
    let Some(od_type) = od_type else {
        return LaneType::Unknown;
    };
    match od_type {
        "none" => LaneType::None,
        "driving" => LaneType::Driving,
        "bidirectional" => LaneType::Bidirectional,
        "bus" => LaneType::Bus,
        "taxi" => LaneType::Taxi,
        "HOV" => LaneType::Hov,
        "entry" | "mwyEntry" => LaneType::Entry,
        "exit" | "mwyExit" => LaneType::Exit,
        "onRamp" => LaneType::OnRamp,
        "offRamp" => LaneType::OffRamp,
        "connectingRamp" => LaneType::ConnectingRamp,
        "slipLane" => LaneType::SlipLane,
        "parking" => LaneType::Parking,
        "stop" => LaneType::Stop,
        "restricted" => LaneType::Restricted,
        "biking" => LaneType::Biking,
        "sidewalk" | "walking" => LaneType::Sidewalk,
        "shared" => LaneType::Shared,
        "shoulder" => LaneType::Shoulder,
        "border" => LaneType::Border,
        "curb" => LaneType::Curb,
        "median" => LaneType::Median,
        "roadWorks" => LaneType::RoadWorks,
        "tram" => LaneType::Tram,
        "rail" => LaneType::Rail,
        "special1" => LaneType::Special1,
        "special2" => LaneType::Special2,
        "special3" => LaneType::Special3,
        _ => LaneType::Unknown,
    }
}

// --- Parsing ------------------------------------------------------------------

/// A numeric attribute, or `None` if it is absent, unparseable, or non-finite.
///
/// The finiteness check is not paranoia: Rust's float parser accepts the
/// literal `NaN`, and turns an out-of-range exponent (`1e400`) into infinity,
/// so an XML attribute carries either straight into the baked geometry. One
/// such value poisons every point derived from it, and a NaN vertex makes the
/// road's physics trimesh impossible to build. Surrounding whitespace is
/// allowed, as the schema's numeric types collapse it.
fn attr_f64(node: roxmltree::Node, name: &str) -> Option<f64> {
    node.attribute(name)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

/// An integer attribute, or `None` if it is absent or unparseable.
/// Surrounding whitespace is allowed, as for [`attr_f64`].
fn attr_i32(node: roxmltree::Node, name: &str) -> Option<i32> {
    node.attribute(name)?.trim().parse().ok()
}

/// `node`'s attribute `name` as an `f32`, or `None` like [`attr_f64`]. A
/// value too large for an `f32` counts as unreadable too, rather than
/// becoming infinity.
fn attr_f32(node: roxmltree::Node, name: &str) -> Option<f32> {
    attr_f64(node, name)
        .map(|v| v as f32)
        .filter(|v| v.is_finite())
}

/// `road`'s traffic rule, or the value of its `rule` if that is neither
/// `RHT` nor `LHT`. A road without one is right-hand traffic.
fn traffic_rule<'a>(road: roxmltree::Node<'a, '_>) -> Result<TrafficRule, &'a str> {
    match road.attribute("rule") {
        None | Some("RHT") => Ok(TrafficRule::RightHand),
        Some("LHT") => Ok(TrafficRule::LeftHand),
        Some(other) => Err(other),
    }
}

/// An object's or a lane's `<material>`.
fn material(node: roxmltree::Node) -> Material {
    Material {
        surface: node.attribute("surface").unwrap_or_default().to_string(),
        friction: attr_f64(node, "friction").map(|v| v as f32),
        roughness: attr_f64(node, "roughness").map(|v| v as f32),
    }
}

/// The `overlapZone` of each junction `<laneLink>` that has one, with a
/// `from`, a `to` and a zone the crate can read.
fn lane_link_overlaps(root: roxmltree::Node) -> Vec<LaneLinkOverlap> {
    let mut out = Vec::new();
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        let Some(junction_id) = junction.attribute("id") else {
            continue;
        };
        for connection in junction.children().filter(|n| n.has_tag_name("connection")) {
            for link in connection.children().filter(|n| n.has_tag_name("laneLink")) {
                let lane = |name| link.attribute(name).and_then(|v| v.trim().parse().ok());
                let (Some(from), Some(to), Some(overlap_zone)) =
                    (lane("from"), lane("to"), attr_f64(link, "overlapZone"))
                else {
                    continue;
                };
                let text = |name| connection.attribute(name).unwrap_or_default().to_string();
                out.push(LaneLinkOverlap {
                    junction_id: junction_id.to_string(),
                    connection_id: text("id"),
                    incoming_road: text("incomingRoad"),
                    connecting_road: connection
                        .attribute("connectingRoad")
                        .or(connection.attribute("linkedRoad"))
                        .unwrap_or_default()
                        .to_string(),
                    from,
                    to,
                    overlap_zone,
                });
            }
        }
    }
    out
}

/// The `<header>`'s own attributes. A missing one is empty or `None`, and a
/// `revMajor` or `revMinor` that isn't a whole number is `None`.
fn header(root: roxmltree::Node) -> Header {
    let Some(node) = root.children().find(|n| n.has_tag_name("header")) else {
        return Header::default();
    };
    let text = |name| node.attribute(name).unwrap_or_default().to_string();
    let revision = |name| node.attribute(name).and_then(|v| v.trim().parse().ok());
    Header {
        rev_major: revision("revMajor"),
        rev_minor: revision("revMinor"),
        name: text("name"),
        version: text("version"),
        date: text("date"),
        vendor: text("vendor"),
        north: attr_f64(node, "north"),
        south: attr_f64(node, "south"),
        east: attr_f64(node, "east"),
        west: attr_f64(node, "west"),
    }
}

/// The `<header>`'s `<geoReference>` and `<offset>`. A missing or unreadable
/// offset attribute reads as 0, as in esmini.
fn geo_reference(root: roxmltree::Node) -> GeoReference {
    let Some(header) = root.children().find(|n| n.has_tag_name("header")) else {
        return GeoReference::default();
    };
    let child = |tag| header.children().find(|n| n.has_tag_name(tag));
    GeoReference {
        proj: child("geoReference")
            .and_then(|g| g.text())
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(String::from),
        offset: child("offset").map(|o| {
            let at = |name| attr_f64(o, name).unwrap_or(0.0);
            GeoOffset {
                x: at("x"),
                y: at("y"),
                z: at("z"),
                hdg: at("hdg"),
            }
        }),
    }
}

/// Bakes one `<road>`: its lanes into `out`, its objects into `objects`,
/// looking up what its `<objectReference>`s name in `index`, and its tunnels
/// and bridges over its lanes into `structures`. Returns the road, for
/// placing what the file puts on it later. Its id is 0 until the caller
/// sets it.
///
/// A road the importer cannot interpret is *skipped*, not fatal, and the
/// reason returned. That covers no length, no `<planView>` and no supported
/// geometry. A road with no `<lanes>` bakes no lanes. Real exports carry the
/// occasional junk road, and losing a whole city map to one of them is the
/// worse failure. `load_str` still errors if the document as a whole yielded
/// no lanes at all, so a thoroughly broken file is never silently accepted.
fn parse_road(
    road: roxmltree::Node,
    index: &ObjectIndex,
    out: &mut Vec<Lane>,
    objects: &mut Objects,
    structures: &mut Structures,
    topo: &mut Topology,
) -> Result<BakedRoad, RoadSkipReason> {
    let road_id = road.attribute("id").unwrap_or_default().to_string();
    let length = attr_f64(road, "length").ok_or(RoadSkipReason::NoLength)?;
    if length > MAX_LENGTH {
        return Err(RoadSkipReason::TooLong);
    }
    let rule = traffic_rule(road).unwrap_or(TrafficRule::RightHand);
    let plan_view = child(road, "planView").ok_or(RoadSkipReason::NoPlanView)?;
    let mut geoms: Vec<GeomRec> = Vec::new();
    let mut dropped = Vec::new();
    for g in plan_view.children().filter(|n| n.has_tag_name("geometry")) {
        match geometry(g) {
            Some(rec) => geoms.push(rec),
            None => dropped.push(Warning::GeometryDropped {
                road_id: road_id.clone(),
                s: attr_f64(g, "s"),
            }),
        }
    }
    if geoms.is_empty() {
        return Err(RoadSkipReason::NoGeometry);
    }
    geoms.sort_by(|a, b| a.s.total_cmp(&b.s));

    let elevations = child(road, "elevationProfile")
        .map(|n| cubics_in(n, "elevation", "s"))
        .unwrap_or_default();
    // Superelevation: a roll of the whole cross-section about the reference
    // line, a cubic in road-s. Same `Cubic`/`active` machinery as elevation;
    // applied per lane by the reference-line pivot in `sample_lane`.
    let superelevations = child(road, "lateralProfile")
        .map(|n| cubics_in(n, "superelevation", "s"))
        .unwrap_or_default();
    let mut warnings = dropped;
    let lateral = Lateral {
        profiles: child(road, "lateralProfile")
            .map(shape_profiles)
            .unwrap_or_default(),
        surface: child(road, "lateralProfile")
            .and_then(|n| child(n, "crossSectionSurface"))
            .map(|n| cross_section(n, &road_id, &mut warnings)),
    };
    if lateral.surface.is_some() && (!lateral.profiles.is_empty() || !superelevations.is_empty()) {
        warnings.push(Warning::CrossSectionWithShape {
            road_id: road_id.clone(),
        });
    }
    // laneOffset shifts the whole lane cross-section laterally off lane 0 (lane
    // widening, merges, a centerline that isn't the road reference). It adds to
    // every lane's offset, so it must be applied or all lanes are mis-placed.
    let (permanent, temporary) = layer_nodes(road);
    let lane_offsets = permanent
        .map(|n| cubics_in(n, "laneOffset", "s"))
        .unwrap_or_default();
    let temporary_lane_offsets = temporary
        .map(|n| cubics_in(n, "laneOffset", "s"))
        .unwrap_or_default();
    let mut baked = BakedRoad {
        road: Road {
            id: RoadId(0),
            junction: road
                .attribute("junction")
                .filter(|j| !j.is_empty() && *j != "-1")
                .map(String::from),
            od_id: road_id,
            name: road.attribute("name").unwrap_or_default().to_string(),
            length,
            rule,
            geoms,
            elevations,
            superelevations,
            lateral,
            lane_offsets,
            temporary_lane_offsets,
            sections: Vec::new(),
        },
        types: properties::road_types(road),
        defs: Vec::new(),
        warnings,
    };
    let (lanes_before, metas_before, next_before) = (out.len(), topo.metas.len(), topo.next_id);
    bake_lanes(road, &mut baked, out, topo);
    if !out[lanes_before..].iter().all(finite) {
        out.truncate(lanes_before);
        topo.metas.truncate(metas_before);
        topo.next_id = next_before;
        topo.registry.retain(|_, id| id.0 < next_before);
        topo.roads.remove(&baked.road.od_id);
        return Err(RoadSkipReason::OutOfRange);
    }
    if let Some(objects_node) = child(road, "objects") {
        place_objects(objects_node, index, &baked.road, None, objects);
        place_structures(objects_node, &baked.road, out, structures);
    }
    Ok(baked)
}

/// One `<planView><geometry>` record, or `None` for one the crate can't
/// bake: one missing its `s`, `x`, `y`, `hdg` or `length`, one whose
/// `length` isn't above 0 or is over [`MAX_LENGTH`], one of a shape it
/// doesn't know, or one whose poses overflow.
fn geometry(g: roxmltree::Node) -> Option<GeomRec> {
    let (Some(s), Some(x), Some(y), Some(hdg), Some(length)) = (
        attr_f64(g, "s"),
        attr_f64(g, "x"),
        attr_f64(g, "y"),
        attr_f64(g, "hdg"),
        attr_f64(g, "length"),
    ) else {
        return None;
    };
    if length <= 0.0 || length > MAX_LENGTH {
        return None;
    }
    let shape = if let Some(arc) = child(g, "arc") {
        let curvature = attr_f64(arc, "curvature")?;
        GeomShape::Arc { curvature }
    } else if let Some(sp) = child(g, "spiral") {
        let (curv_start, curv_end) = (attr_f64(sp, "curvStart")?, attr_f64(sp, "curvEnd")?);
        GeomShape::Spiral {
            curv_start,
            curv_end,
        }
    } else if let Some(pp) = child(g, "paramPoly3") {
        let coeff = |n: &str| attr_f64(pp, n).unwrap_or(0.0);
        // "arcLength" -> p in [0,len]; anything else, including an absent
        // attribute, is "normalized" p in [0,1], matching libOpenDRIVE's
        // default and case-insensitive compare (files set it explicitly).
        let p_max = match pp.attribute("pRange").map(str::to_ascii_lowercase) {
            Some(ref r) if r == "arclength" => length,
            _ => 1.0,
        };
        GeomShape::ParamPoly3 {
            u: [coeff("aU"), coeff("bU"), coeff("cU"), coeff("dU")],
            v: [coeff("aV"), coeff("bV"), coeff("cV"), coeff("dV")],
            p_max,
        }
    } else if let Some(p3) = child(g, "poly3") {
        // poly3 is the special case of paramPoly3 with u(p)=p and v(p) the
        // cubic in u; reuse the same baker over p in [0, length].
        let coeff = |n: &str| attr_f64(p3, n).unwrap_or(0.0);
        GeomShape::ParamPoly3 {
            u: [0.0, 1.0, 0.0, 0.0],
            v: [coeff("a"), coeff("b"), coeff("c"), coeff("d")],
            p_max: length,
        }
    } else if child(g, "line").is_some() {
        GeomShape::Line
    } else {
        return None;
    };
    Some(GeomRec::new(s, x, y, hdg, length, shape)).filter(GeomRec::is_finite)
}

/// Whether every number in `lane` is finite. A coefficient too large for an
/// `f32`, such as an elevation of `1e39`, bakes to infinity.
fn finite(lane: &Lane) -> bool {
    let center = &lane.center;
    let points = center.points().iter().map(|p| p.to_array());
    let tangents = center.tangents().iter().map(|t| t.to_array());
    points.chain(tangents).flatten().all(f32::is_finite)
        && lane.width.is_finite()
        && lane.widths.iter().chain(&lane.bank).all(|v| v.is_finite())
}

/// The most lane and road CRG one load bakes, in metres: each lane's length
/// and each road `<CRG>`'s stretch, at [`CRG_COST`], summed over the file. It
/// is 2,000 km, about six times the lanes of the largest real map the crate
/// is tried on. The crate samples each every few centimetres to metres, so a small
/// broken file that repeats a long road's lanes or CRG records would
/// otherwise need more memory than any machine has. A road past it is
/// skipped, and a road CRG past it dropped.
pub const MAX_BAKED_LENGTH: f64 = 2_000_000.0;

/// The length of `road`'s lanes, `length` long, summed over its lane
/// sections of both layers, from the `<lane>`s the file gives.
fn lane_length(road: roxmltree::Node, length: f64) -> f64 {
    let layers = road.children().filter(|n| n.has_tag_name("lanes"));
    layers
        .map(|lanes| {
            let mut sections: Vec<(f64, usize)> = lanes
                .children()
                .filter(|n| n.has_tag_name("laneSection"))
                .map(|section| {
                    let sides = ["left", "right"]
                        .into_iter()
                        .filter_map(|side| child(section, side));
                    let count = sides
                        .map(|side| side.children().filter(|n| n.has_tag_name("lane")).count())
                        .sum();
                    (
                        attr_f64(section, "s").unwrap_or(0.0).clamp(0.0, length),
                        count,
                    )
                })
                .collect();
            sections.sort_by(|a, b| a.0.total_cmp(&b.0));
            let ends = sections.iter().skip(1).map(|&(s, _)| s).chain([length]);
            sections
                .iter()
                .zip(ends)
                .map(|(&(s, count), end)| (end - s) * count as f64)
                .sum::<f64>()
        })
        .sum()
}

/// The longest road or geometry the crate bakes, in metres. The crate
/// samples a road every few centimetres to metres along its length, so a
/// length from a broken file, such as 1e13, would need more memory than any
/// machine has. Real roads are far shorter.
pub const MAX_LENGTH: f64 = 100_000.0;

/// How far, in metres, a `<border>` may lie inside its inner neighbour's
/// outer border before it counts as crossing it, rather than as rounding.
const BORDER_CROSSING: f64 = 1e-6;

/// How far, in metres, a lateral profile's first `<shape>` may start inside
/// the road's right edge before it counts as short of the road.
const SHAPE_SHORT: f64 = 1e-3;

impl BakedRoad {
    /// What the load dropped from `section`'s lanes, or read against the
    /// spec: each dropped lane, then each lane with borders in a section with
    /// widths,
    /// each border lane under a `<laneOffset>`, each border that crosses
    /// inside its inner neighbour at one of the section's stations, and each
    /// lane outside a level lane that is not level itself.
    fn lane_warnings(&self, section: &RoadSection, def: &SectionDef) -> Vec<Warning> {
        let road = &self.road;
        let (road_id, index) = (&road.od_id, section.index);
        let mut out: Vec<Warning> = def
            .dropped
            .iter()
            .map(|&lane| Warning::LaneDropped {
                road_id: road_id.clone(),
                section: index,
                lane,
            })
            .chain(def.mixed.iter().map(|&lane| Warning::WidthAndBorder {
                road_id: road_id.clone(),
                section: index,
                lane,
            }))
            .collect();
        out.splice(
            0..0,
            def.bad_ids.iter().map(|id| Warning::LaneIdUnreadable {
                road_id: road_id.clone(),
                section: index,
                id: id.clone(),
            }),
        );
        let border_lanes = || {
            section
                .left
                .iter()
                .chain(&section.right)
                .filter(|l| matches!(l.extent, LaneExtent::Border(_)))
        };
        if offset_within(road.offsets_of(section.layer), section.start, section.end) {
            out.extend(border_lanes().map(|l| Warning::BorderWithLaneOffset {
                road_id: road_id.clone(),
                section: index,
                lane: l.id,
            }));
        }
        let mut crossed: Vec<(i32, f64)> = Vec::new();
        if border_lanes().next().is_some() {
            for &s in &section.stations {
                let base = road.base(s);
                let s_lane = s - section.start;
                for (side, sign) in [(&section.left, 1.0), (&section.right, -1.0)] {
                    for (lane, b) in side.iter().zip(side_borders(side, sign, base, s_lane)) {
                        let LaneExtent::Border(borders) = &lane.extent else {
                            continue;
                        };
                        let inside = active(borders, s_lane)
                            .is_some_and(|r| sign * (r.eval(s_lane) - b.inner) < -BORDER_CROSSING);
                        if inside && !crossed.iter().any(|&(id, _)| id == lane.id) {
                            crossed.push((lane.id, s));
                        }
                    }
                }
            }
        }
        out.extend(
            crossed
                .into_iter()
                .map(|(lane, s)| Warning::BorderCrossesInnerLane {
                    road_id: road_id.clone(),
                    section: index,
                    lane,
                    s,
                }),
        );
        for lane in def.left.iter().chain(&def.right) {
            if let Err(direction) = &lane.direction {
                out.push(Warning::UnknownLaneDirection {
                    road_id: road_id.clone(),
                    section: index,
                    lane: lane.id,
                    direction: direction.clone(),
                });
            }
        }
        for side in [&section.left, &section.right] {
            let first_level = side.iter().position(|l| l.level).unwrap_or(side.len());
            out.extend(side[first_level..].iter().filter(|l| !l.level).map(|l| {
                Warning::LaneNotLevel {
                    road_id: road_id.clone(),
                    section: index,
                    lane: l.id,
                }
            }));
        }
        out
    }

    /// Each lateral profile whose first `<shape>` starts more than
    /// [`SHAPE_SHORT`] inside the road's right edge, at the profile's `s`.
    /// The last shape runs on past the left edge, so only the right can fall
    /// short.
    fn shape_warnings(&self) -> Vec<Warning> {
        let road = &self.road;
        road.lateral
            .profiles
            .iter()
            .filter(|p| {
                let Some(section) = road.section_at(p.s) else {
                    return false;
                };
                let edge = road
                    .across(section, p.s)
                    .fold(road.base(p.s), |edge, a| edge.min(a.inner_t).min(a.outer_t));
                p.shapes[0].start > edge + SHAPE_SHORT
            })
            .map(|p| Warning::ShapeShortOfRoad {
                road_id: road.od_id.clone(),
                s: p.s,
            })
            .collect()
    }
}

/// Whether a `<laneOffset>` is anything but 0 anywhere over `[start, end)`.
fn offset_within(lane_offsets: &[Cubic], start: f64, end: f64) -> bool {
    lane_offsets.iter().enumerate().any(|(i, r)| {
        let until = lane_offsets.get(i + 1).map_or(f64::INFINITY, |n| n.start);
        r.start < end && until > start && [r.a, r.b, r.c, r.d].iter().any(|&c| c != 0.0)
    })
}

/// How far apart a CRG stretch's stations are, in metres. The stretch joins
/// them with arcs, so lines and arcs are exact. A spiral from straight to a
/// 50 m radius over 50 m is off by under a micrometre.
const CRG_STEP: f64 = 0.25;

/// How many metres of [`MAX_BAKED_LENGTH`] a metre of road CRG takes. Its
/// stations, every [`CRG_STEP`], hold about eight times what a lane holds
/// in a metre.
const CRG_COST: f64 = 8.0;

/// A road `<CRG>`, or `None` if it lacks a file, a known mode, or a stretch
/// of road to lie on, or if its stretch costs more than `length_left`,
/// which it takes its [`CRG_COST`] from.
fn road_crg(node: roxmltree::Node, road: &Road, length_left: &mut f64) -> Option<CrgSurface> {
    node.attribute("file")?;
    let from = attr_f64(node, "sStart")?.max(0.0);
    let to = attr_f64(node, "sEnd")?.min(road.length);
    let cost = (to - from) * CRG_COST;
    if to <= from || cost > *length_left {
        return None;
    }
    let offset = |name: &str| attr_f64(node, name).unwrap_or(0.0);
    let along = CrgAlong {
        s_offset: offset("sOffset"),
        t_offset: offset("tOffset"),
        opposite: node.attribute("orientation") == Some("opposite"),
    };
    let mode = match node.attribute("mode")? {
        "attached" => CrgMode::Attached(along),
        "attached0" => CrgMode::Attached0(along),
        "genuine" => {
            let at = road.station(along.s_offset);
            let t = along.t_offset * at.bank.cos();
            let (sin, cos) = at.heading.sin_cos();
            CrgMode::Genuine {
                start: CrgPose {
                    x: at.x - t * sin,
                    y: at.y + t * cos,
                    heading: at.heading + offset("hOffset"),
                },
            }
        }
        "global" => global_crg(node),
        _ => return None,
    };
    *length_left -= cost;
    let steps = ((to - from) / CRG_STEP).ceil().max(1.0) as usize;
    let stations = (0..=steps)
        .map(|k| road.station(from + (to - from) * k as f64 / steps as f64))
        .collect();
    Some(crg_surface(
        node,
        mode,
        road.lanes((from, to), &[]),
        Some(Stretch { stations }),
    ))
}

/// A `<junction>` `<CRG>` over the lanes of the junction's roads. `None`
/// unless it is `global`, the one mode with no reference line.
fn junction_crg(node: roxmltree::Node, lanes: Vec<LaneId>) -> Option<CrgSurface> {
    if node.attribute("mode") != Some("global") {
        return None;
    }
    Some(crg_surface(node, global_crg(node), lanes, None))
}

fn global_crg(node: roxmltree::Node) -> CrgMode {
    let offset = |name: &str| attr_f64(node, name).unwrap_or(0.0);
    CrgMode::Global {
        origin: CrgPose {
            x: offset("xOffset"),
            y: offset("yOffset"),
            heading: offset("hOffset"),
        },
    }
}

/// The attributes every `<CRG>` shares. `zOffset` and `zScale` do not apply
/// to friction.
fn crg_surface(
    node: roxmltree::Node,
    mode: CrgMode,
    lanes: Vec<LaneId>,
    road: Option<Stretch>,
) -> CrgSurface {
    let purpose = match node.attribute("purpose") {
        Some("friction") => CrgPurpose::Friction,
        _ => CrgPurpose::Elevation,
    };
    let (z_offset, z_scale) = match purpose {
        CrgPurpose::Elevation => (
            attr_f64(node, "zOffset").unwrap_or(0.0),
            attr_f64(node, "zScale").unwrap_or(1.0),
        ),
        CrgPurpose::Friction => (0.0, 1.0),
    };
    CrgSurface {
        file: node.attribute("file").unwrap_or_default().to_string(),
        purpose,
        mode,
        z_offset,
        z_scale,
        lanes,
        road,
    }
}

/// A baked road: the [`Road`] the network keeps, and what the file puts
/// along it that the network holds elsewhere. `defs` is in step with the
/// road's sections.
struct BakedRoad {
    road: Road,
    types: Vec<properties::TypeDef>,
    defs: Vec<SectionDef>,
    /// What reading the road itself found wrong.
    warnings: Vec<Warning>,
}

impl BakedRoad {
    /// Each lane section, with what the file says about its lanes.
    fn sections(&self) -> impl Iterator<Item = (&RoadSection, &SectionDef)> {
        self.road.sections.iter().zip(&self.defs)
    }
}

/// Bakes one `<road>`'s lanes into `out`, and adds its lane sections to
/// `baked`. None for a road with no `<lanes>`.
///
/// Each section runs from its `s` to the next section's, or the road's end.
/// The spec requires `s`. A section without one, or with a negative one,
/// starts at 0, as libOpenDRIVE reads it.
fn bake_lanes(
    road: roxmltree::Node,
    baked: &mut BakedRoad,
    out: &mut Vec<Lane>,
    topo: &mut Topology,
) {
    let (permanent, temporary) = layer_nodes(road);
    let start = |section: &roxmltree::Node| attr_f64(*section, "s").unwrap_or(0.0).max(0.0);
    let length = baked.road.length;
    let mut next_index = 0;
    let mut any = false;
    for (layer, lanes_node) in [(Layer::Permanent, permanent), (Layer::Temporary, temporary)] {
        let Some(lanes_node) = lanes_node else {
            continue;
        };
        let mut sections: Vec<roxmltree::Node> = lanes_node
            .children()
            .filter(|n| n.has_tag_name("laneSection"))
            .collect();
        sections.sort_by(|a, b| start(a).total_cmp(&start(b)));
        any |= !sections.is_empty();
        let starts: Vec<f64> = sections.iter().map(start).collect();
        let defs = single_side::full_sections(&sections, &starts);
        for (i, (section, mut def)) in sections.iter().zip(defs).enumerate() {
            let index = next_index + i;
            let s_start = start(section);
            let next = sections.get(i + 1).map_or(length, start);
            let explicit = match layer {
                Layer::Temporary => attr_f64(*section, "length").filter(|l| *l > 0.0),
                Layer::Permanent => None,
            };
            let s_end = explicit
                .map_or(next, |l| (s_start + l).min(next))
                .min(length);
            if s_end - s_start < 1e-3 {
                continue; // zero-length section
            }
            let first = topo.metas.len();
            let [left, right] = std::mem::take(&mut def.geoms);
            let mut geom = RoadSection {
                index,
                layer,
                start: s_start,
                end: s_end,
                stations: Vec::new(),
                left,
                right,
                lanes: Vec::new(),
                marks: Vec::new(),
            };
            geom.stations = emit_section(&baked.road, &geom, &def, out, topo);
            // emit_section records a lane's meta as it pushes the lane, so the
            // new metas and the new lanes are in step.
            geom.lanes = topo.metas[first..]
                .iter()
                .map(|m| (m.od_id, m.id))
                .collect();
            baked.road.sections.push(geom);
            baked.defs.push(def);
        }
        next_index += sections.len();
    }
    if !any {
        return;
    }

    let (predecessor, successor) = links::road_link(road);
    topo.roads.insert(
        baked.road.od_id.clone(),
        RoadInfo {
            sections: baked
                .road
                .sections
                .iter()
                .map(|s| links::SectionInfo {
                    index: s.index,
                    layer: s.layer,
                    start: s.start,
                    end: s.end,
                })
                .collect(),
            length: baked.road.length,
            predecessor,
            successor,
        },
    );
}

/// The `<lanes>` of a road's permanent layer and of its temporary one.
///
/// The permanent layer is the first `<lanes>` that isn't `layer="temporary"`,
/// or the first of all if every one is, as before layers were read. The
/// temporary layer is the first other `<lanes>` that is. A further `<lanes>`
/// of either layer is left out.
fn layer_nodes<'a>(
    road: roxmltree::Node<'a, 'a>,
) -> (
    Option<roxmltree::Node<'a, 'a>>,
    Option<roxmltree::Node<'a, 'a>>,
) {
    let all: Vec<_> = road
        .children()
        .filter(|n| n.has_tag_name("lanes"))
        .collect();
    let temporary_of =
        |n: &roxmltree::Node| Layer::from_attr(n.attribute("layer")) == Layer::Temporary;
    let permanent = all
        .iter()
        .find(|n| !temporary_of(n))
        .or(all.first())
        .copied();
    let temporary = all
        .iter()
        .find(|n| temporary_of(n) && Some(**n) != permanent)
        .copied();
    (permanent, temporary)
}

/// Append each lane of one section as a `Lane` spanning `[s_start, s_end]`,
/// and return the stations they are sampled at, from [`sample_positions`].
/// Malformed individual lanes are skipped, not fatal (real files).
///
/// Every lane with a width or a border becomes a `Lane`, whatever its type.
/// `section` is where its lanes lie on `road`, and `def` the rest of what
/// the file says about them.
fn emit_section(
    road: &Road,
    section: &RoadSection,
    def: &SectionDef,
    out: &mut Vec<Lane>,
    topo: &mut Topology,
) -> Vec<f64> {
    let (s_start, s_end) = (section.start, section.end);
    let (road_id, section_idx) = (&road.od_id, section.index);
    let bends = section_bends(
        &road.geoms,
        road.offsets_of(section.layer),
        [&section.left, &section.right],
        s_start,
        s_end,
    );
    let knots: Vec<f64> = section
        .left
        .iter()
        .chain(&section.right)
        .flat_map(|l| &l.heights)
        .map(|h| s_start + h.s_offset)
        .chain(road.lateral.knots())
        .collect();
    let sample_s = sample_positions(s_start, s_end, &bends, &knots);
    let road_bank: Vec<f64> = sample_s
        .iter()
        .map(|&s| active(&road.superelevations, s).map_or(0.0, |e| e.eval(s)))
        .collect();
    let level_bank = unless_flat(road_bank.iter().map(|&b| b as f32).collect());
    for (defs, side) in [(&def.left, &section.left), (&def.right, &section.right)] {
        let is_left = side.first().map(|l| l.id > 0).unwrap_or(false);
        let sign = if is_left { 1.0 } else { -1.0 };
        let side_direction = road.rule.direction(is_left);
        // Lanes emitted on this side, center-outward, as (id, index in `out`,
        // kind, direction). Consecutive ones are lateral neighbors (lane-change
        // edges), subject to the tests below.
        let mut emitted: Vec<(LaneId, usize, LaneType, Direction)> = Vec::new();
        let across = side_across(road, section.layer, side, sign, s_start, &sample_s);
        let heights = side_height_rows(
            road,
            (section.layer, side),
            sign,
            s_start,
            &sample_s,
            &road_bank,
            &across,
        );
        let lanes = defs.iter().zip(side).zip(&across).zip(&heights);
        for (k, (((def, lane), borders), heights)) in lanes.enumerate() {
            let kind = def.kind;
            let direction = def
                .direction
                .as_ref()
                .map_or(LaneDirection::Standard, |d| *d)
                .apply(side_direction);
            let held = side[..=k].iter().any(|l| l.level);
            let (points, headings) = sample_lane(road, borders, heights, sign, &sample_s);
            // Anchor the ends on the true curve tangents so this section's ribs
            // line up with its neighbours'.
            let (Some(&start_tangent), Some(&end_tangent)) = (headings.first(), headings.last())
            else {
                continue;
            };
            let Some(center) = Polyline::try_new_with_tangents(points, start_tangent, end_tangent)
            else {
                continue;
            };
            let id = LaneId(topo.next_id);
            topo.next_id += 1;
            topo.registry
                .insert((road_id.to_string(), section_idx, lane.id), id);
            topo.metas.push(LaneMeta {
                id,
                road: road_id.to_string(),
                section: section_idx,
                od_id: lane.id,
                direction,
                layer: section.layer,
                succ_links: def.succ_links.clone(),
                pred_links: def.pred_links.clone(),
                heights: lane_heights(lane, s_start, &sample_s),
                flags: def.flags,
            });
            emitted.push((id, out.len(), kind, direction));
            let tilted = !lane.heights.is_empty() || !road.lateral.is_empty() || held;
            let (width, widths) = width_profile(borders, tilted.then_some(&heights[..]));
            out.push(Lane {
                id,
                kind,
                layer: section.layer,
                direction,
                center,
                width,
                widths,
                bank: if tilted {
                    lane_bank(borders, heights, sign, &road_bank)
                } else {
                    level_bank.clone()
                },
                // Filled by links::resolve once all lanes are registered.
                successors: Vec::new(),
                predecessors: Vec::new(),
                neighbors: Vec::new(),
            });
        }
        // Consecutive same-side lanes are each other's lane-change neighbors,
        // but only where both ends carry through traffic the same way. Without
        // that test a driving lane would list the sidewalk beside it, or a
        // reversed lane, as a lane change, and the router would happily take
        // it. A lane between two driving lanes that fails it separates them
        // for the same reason: it stands in the sequence, so they are not
        // consecutive and no edge spans it.
        for k in 0..emitted.len() {
            let (_, at, kind, direction) = emitted[k];
            if !kind.is_drivable() {
                continue;
            }
            let nbrs = [k.checked_sub(1), Some(k + 1)]
                .into_iter()
                .flatten()
                .filter_map(|n| emitted.get(n))
                .filter(|(_, _, kind, d)| kind.is_drivable() && *d == direction)
                .map(|(id, _, _, _)| *id)
                .collect();
            out[at].neighbors = nbrs;
        }
    }
    sample_s
}

/// A lane's bank at each station, parallel to its `borders` and the
/// `heights` of its borders there, from [`border_heights`]: the road's
/// superelevation `road_bank`, plus the slope from one border height to the
/// other, signed to raise its outer border when that is the higher. `sign`
/// is 1.0 for a left lane and -1.0 for a right one. Empty for a lane flat
/// across at every station, so a level road bakes as it did before lane
/// heights.
///
/// The tessellator turns the lane's cross axis by this angle. The lane's
/// width is the chord `sqrt(w² + d²)`, so each edge lands on its border at
/// its height. It turns about the level tangent, so on a grade `g` the
/// edges also sit `d / 2 * g` along the road from where the road's normal
/// puts them.
fn lane_bank(
    borders: &[LaneBorders],
    heights: &[(f64, f64)],
    sign: f64,
    road_bank: &[f64],
) -> Vec<f32> {
    unless_flat(
        borders
            .iter()
            .zip(heights)
            .zip(road_bank)
            .map(|((b, &(inner, outer)), &phi)| {
                let slope = (outer - inner).atan2(b.width);
                (phi + sign * slope) as f32
            })
            .collect(),
    )
}

/// `bank`, or empty, the flat lane's profile, if it is 0 all along.
fn unless_flat(bank: Vec<f32>) -> Vec<f32> {
    if bank.iter().all(|b| b.abs() < 1e-9) {
        Vec::new()
    } else {
        bank
    }
}

/// A lane's heights at each of `sample_s`, parallel to them. Empty for a
/// lane level with the road at every station.
fn lane_heights(lane: &LaneGeom, s_start: f64, sample_s: &[f64]) -> Vec<LaneHeight> {
    if lane.heights.is_empty() {
        return Vec::new();
    }
    let heights: Vec<LaneHeight> = sample_s
        .iter()
        .map(|&s| {
            let (inner, outer) = height_at(lane, s - s_start);
            LaneHeight {
                inner: inner as f32,
                outer: outer as f32,
            }
        })
        .collect();
    if heights.iter().all(|h| h.inner == 0.0 && h.outer == 0.0) {
        Vec::new()
    } else {
        heights
    }
}

/// Sample one lane's centerline to points in our coordinate frame, with the
/// reference line's analytical heading at each sample. A raised lane's
/// centerline stands off the road along its normal by the lane's height at
/// its middle, halfway between its inner and outer height.
///
/// The headings are the exact `hdg` of the underlying geometry record, not the
/// chords between the points, so two sections sampled either side of the same
/// station report the same heading there. That shared value is what lets their
/// meshed ribs meet flush; see [`Polyline::try_new_with_tangents`].
///
/// `borders` is where the lane lies across the road at each of `sample_s`,
/// from [`side_across`], `heights` its border heights there, from
/// [`border_heights`], and `sign` which way it stacks.
fn sample_lane(
    road: &Road,
    borders: &[LaneBorders],
    heights: &[(f64, f64)],
    sign: f64,
    sample_s: &[f64],
) -> (Vec<Point>, Vec<Vector>) {
    sample_s
        .iter()
        .zip(borders)
        .zip(heights)
        .map(|((&s, b), &(inner, outer))| {
            let t = b.inner + sign * b.width / 2.0;
            let (point, hdg) = road.raised(s, t, (inner + outer) / 2.0);
            // A lane offset laterally by a constant `t` is parallel to the
            // reference line, so it shares its heading. Where `t` varies with
            // `s` the lane's own tangent swings off it slightly; the reference
            // heading is used anyway, because being the *same* on both sides of
            // a section joint is what closes the seam, and a lane's `dt/ds`
            // generally steps across that joint.
            let heading = Vector::new(hdg.cos() as f32, hdg.sin() as f32, 0.0);
            (point, heading)
        })
        .unzip()
}

/// Each lane of `side` across the road at each of `sample_s`, from
/// [`side_borders`]: one row per lane, parallel to `sample_s`.
fn side_across(
    road: &Road,
    layer: Layer,
    side: &[LaneGeom],
    sign: f64,
    s_start: f64,
    sample_s: &[f64],
) -> Vec<Vec<LaneBorders>> {
    let mut rows: Vec<Vec<LaneBorders>> = (0..side.len())
        .map(|_| Vec::with_capacity(sample_s.len()))
        .collect();
    for &s in sample_s {
        let base = road.base_of(layer, s);
        for (row, b) in rows
            .iter_mut()
            .zip(side_borders(side, sign, base, s - s_start))
        {
            row.push(b);
        }
    }
    rows
}

/// Each lane of `side`'s border heights at each of `sample_s`, from
/// [`side_heights`]: one row per lane, parallel to `sample_s` and to the
/// rows of `across`, from [`side_across`]. `road_bank` is the
/// superelevation at each station.
fn side_height_rows(
    road: &Road,
    (layer, side): (Layer, &[LaneGeom]),
    sign: f64,
    s_start: f64,
    sample_s: &[f64],
    road_bank: &[f64],
    across: &[Vec<LaneBorders>],
) -> Vec<Vec<(f64, f64)>> {
    let mut rows: Vec<Vec<(f64, f64)>> = (0..side.len())
        .map(|_| Vec::with_capacity(sample_s.len()))
        .collect();
    for (i, (&s, &phi)) in sample_s.iter().zip(road_bank).enumerate() {
        let borders = across.iter().map(|row| row[i]);
        let heights = side_heights(
            side,
            sign,
            &road.lateral,
            (s, s - s_start),
            road.base_of(layer, s),
            phi,
            borders,
        );
        for (row, h) in rows.iter_mut().zip(heights) {
            row.push(h);
        }
    }
    rows
}

/// A lane's nominal width and its per-sample profile, from its `borders` at
/// each station.
///
/// For a lane whose border `heights` differ, the width is the chord across
/// the tilted lane, `sqrt(w² + Δh²)`. The tessellator turns the lane's cross
/// axis by its bank, so the chord puts each edge on its border. The plan
/// width alone would leave each edge `w / 2 * (1 - cos α)` inside it, 7.4 cm
/// on a 3.5 m lane at 30 %. `heights` is `None` for a lane nothing tilts.
///
/// The profile collapses to empty when the lane holds one width all the way
/// along, which covers most lanes, so an ordinary lane bakes to exactly what
/// it baked to before per-station widths existed.
///
/// The nominal width is the widest the lane gets, not the width where it
/// starts. A lane that opens out of a point starts at 0 m, and reporting that
/// as its width gave a gore area no surface at all.
fn width_profile(borders: &[LaneBorders], heights: Option<&[(f64, f64)]>) -> (f32, Vec<f32>) {
    let widths: Vec<f32> = match heights {
        None => borders.iter().map(|b| b.width as f32).collect(),
        Some(heights) => borders
            .iter()
            .zip(heights)
            .map(|(b, (inner, outer))| b.width.hypot(outer - inner) as f32)
            .collect(),
    };
    let nominal = widths.iter().copied().fold(0.0_f32, f32::max);
    let constant = widths.iter().all(|w| (w - nominal).abs() < 1e-6);
    (nominal, if constant { Vec::new() } else { widths })
}

/// Arc-length sample positions over `[start, end]`, always including the
/// exact endpoints. They are [`SAMPLE_STEP`] apart, or closer where lanes
/// turning `bends` radians a metre would turn more than [`SAMPLE_TURN`] in
/// that, down to [`SAMPLE_MIN_STEP`]. `bends` is from [`section_bends`],
/// evenly spaced over the stretch.
///
/// The step eases in and out of a tight spot by at most [`SAMPLE_GRADE`] a
/// metre, rather than jumping, because the mesh welds a sample much closer
/// than the one before it away as a stub. So only the road near a tight
/// curve is sampled finely, not the whole section around it.
///
/// Every one of `knots` inside the stretch is a station too, such as where a
/// lane's heights change pace. A step that would leave less than a step to
/// the next knot goes halfway there instead, so no knot lands as a stub.
fn sample_positions(start: f64, end: f64, bends: &[f64], knots: &[f64]) -> Vec<f64> {
    let n = bends.len().saturating_sub(1).max(1);
    let h = (end - start) / n as f64;
    // The step wanted at each probe, graded so it changes no faster than
    // SAMPLE_GRADE a metre.
    let mut want: Vec<f64> = bends
        .iter()
        .map(|&b| (SAMPLE_TURN / b).clamp(SAMPLE_MIN_STEP, SAMPLE_STEP))
        .collect();
    for k in 1..want.len() {
        want[k] = want[k].min(want[k - 1] + SAMPLE_GRADE * h);
    }
    for k in (1..want.len()).rev() {
        want[k - 1] = want[k - 1].min(want[k] + SAMPLE_GRADE * h);
    }
    // The step wanted at `s`, straight between the probes either side, so it
    // too changes no faster than SAMPLE_GRADE a metre.
    let last = want.len() - 1;
    let probe = |s: f64| ((s - start) / h).clamp(0.0, last as f64);
    let want_at = |s: f64| {
        let p = probe(s);
        let k = (p.floor() as usize).min(last.saturating_sub(1));
        match want.get(k + 1) {
            Some(&next) => want[k] + (next - want[k]) * (p - k as f64),
            None => want[k],
        }
    };
    let mut ss = Vec::new();
    let mut s = start;
    while s < end - 1e-6 {
        ss.push(s);
        // The tightest step wanted anywhere the step would cross, so no
        // tight spot is stepped over.
        let reach = want_at(s);
        let inside = want[probe(s).ceil() as usize..=probe(s + reach).floor() as usize]
            .iter()
            .copied();
        let step = inside.fold(reach.min(want_at(s + reach)), f64::min);
        let knot = knots
            .iter()
            .map(|k| k - s)
            .filter(|&d| d > 1e-6 && s + d < end - 1e-6)
            .fold(f64::INFINITY, f64::min);
        let next = s + if knot <= step {
            knot
        } else if knot < 2.0 * step {
            knot / 2.0
        } else {
            step
        };
        if next <= s {
            break;
        }
        s = next;
    }
    ss.push(end);
    ss
}

/// How sharply the lane edges of a section over `[start, end]` turn in plan,
/// in radians per metre of road, at probes every [`SAMPLE_PROBE`] or so:
/// the sharpest edge at each. The edges bend with the reference line, and
/// also as widths and `laneOffset` move them across it. `sides` are the
/// section's left and right lanes, each ordered from the center outward.
fn section_bends(
    geoms: &[GeomRec],
    lane_offsets: &[Cubic],
    sides: [&[LaneGeom]; 2],
    start: f64,
    end: f64,
) -> Vec<f64> {
    let n = ((end - start) / SAMPLE_PROBE).ceil().max(1.0) as usize;
    let h = (end - start) / n as f64;
    // The probes run forward, so the geometry record is found by walking on
    // from the last one rather than scanning the road's list at every probe.
    let mut geom = 0;
    // Each edge's plan position at `s`, into `out`.
    let mut edges = |s: f64, out: &mut Vec<(f64, f64)>| {
        while geom + 1 < geoms.len() && geoms[geom + 1].s <= s + 1e-9 {
            geom += 1;
        }
        let (x, y, hdg) = geoms[geom].pose(s);
        let (sin, cos) = hdg.sin_cos();
        let base = active(lane_offsets, s).map_or(0.0, |o| o.eval(s));
        out.clear();
        out.push((x - base * sin, y + base * cos));
        for (side, sign) in sides.into_iter().zip([1.0, -1.0]) {
            for b in side_borders(side, sign, base, s - start) {
                out.push((x - b.outer * sin, y + b.outer * cos));
            }
        }
    };
    let (mut prev, mut next) = (Vec::new(), Vec::new());
    // Each edge's chord over the last probe interval.
    let mut chords: Vec<(f64, f64)> = Vec::new();
    let mut bends = vec![0.0; n + 1];
    edges(start, &mut prev);
    for k in 1..=n {
        edges(start + k as f64 * h, &mut next);
        let mut sharpest = 0.0_f64;
        for (e, (a, b)) in prev.iter().zip(&next).enumerate() {
            let chord = (b.0 - a.0, b.1 - a.1);
            if k > 1 {
                // The angle between this chord and the last, one atan2.
                let (u, v) = (chords[e], chord);
                let turn = (u.0 * v.1 - u.1 * v.0).atan2(u.0 * v.0 + u.1 * v.1);
                sharpest = sharpest.max(turn.abs() / h);
                chords[e] = chord;
            } else {
                chords.push(chord);
            }
        }
        if k > 1 {
            bends[k - 1] = sharpest;
        }
        std::mem::swap(&mut prev, &mut next);
    }
    // The ends have a heading on one side only, so take their neighbour's.
    if n > 1 {
        bends[0] = bends[1];
        bends[n] = bends[n - 1];
    }
    // A probe only half sees a curve starting or ending between it and the
    // next, so each takes the sharpest of itself and its neighbours.
    (0..=n)
        .map(|k| {
            bends[k.saturating_sub(1)..=(k + 1).min(n)]
                .iter()
                .copied()
                .fold(0.0, f64::max)
        })
        .collect()
}

/// Parse every `<item>` child of `parent` as a cubic (elevation, laneOffset),
/// keyed on `start_attr`, sorted by start.
fn cubics_in(parent: roxmltree::Node, item: &str, start_attr: &str) -> Vec<Cubic> {
    let mut out: Vec<Cubic> = parent
        .children()
        .filter(|n| n.has_tag_name(item))
        .filter_map(|n| {
            Some(Cubic {
                start: attr_f64(n, start_attr)?,
                a: attr_f64(n, "a").unwrap_or(0.0),
                b: attr_f64(n, "b").unwrap_or(0.0),
                c: attr_f64(n, "c").unwrap_or(0.0),
                d: attr_f64(n, "d").unwrap_or(0.0),
            })
        })
        .collect();
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// A lane's `<width>` or `<border>` records as cubics, sorted by `sOffset`.
/// The spec requires every attribute. A record without `a` is skipped, and
/// a missing `sOffset`, `b`, `c` or `d` is 0.
fn lane_cubics<'a>(records: impl Iterator<Item = roxmltree::Node<'a, 'a>>) -> Vec<Cubic> {
    let mut out: Vec<Cubic> = records
        .filter_map(|n| {
            Some(Cubic {
                start: attr_f64(n, "sOffset").unwrap_or(0.0),
                a: attr_f64(n, "a")?,
                b: attr_f64(n, "b").unwrap_or(0.0),
                c: attr_f64(n, "c").unwrap_or(0.0),
                d: attr_f64(n, "d").unwrap_or(0.0),
            })
        })
        .collect();
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

// --- Objects ------------------------------------------------------------------

/// The objects baked so far, the provenance of each, in step, and what
/// placing them left out.
#[derive(Default)]
struct Objects {
    baked: Vec<Object>,
    provenance: Vec<ObjectProvenance>,
    warnings: Vec<Warning>,
    /// What placing them has cost so far.
    cost: Cost,
    /// The `<surface>` CRG of each object that has one, for each solid it
    /// bakes to.
    surfaces: Vec<ObjectCrg>,
}

impl Objects {
    /// Whether baking `cost` more stays within [`MAX_OBJECTS`] and
    /// [`MAX_OBJECT_PIECES`].
    fn affords(&self, cost: Cost) -> bool {
        let total = self.cost + cost;
        total.objects <= MAX_OBJECTS && total.pieces <= MAX_OBJECT_PIECES
    }

    /// Spend `cost`, or `false`, spending nothing, if it is more than the
    /// load has left.
    fn spend(&mut self, cost: Cost) -> bool {
        let fits = self.affords(cost);
        if fits {
            self.cost = self.cost + cost;
        }
        fits
    }
}

/// One object's `<surface>` CRG, with what its warnings name.
struct ObjectCrg {
    surface: CrgSurface,
    road_id: String,
    /// For an object of a junction, the junction's id. Empty otherwise.
    junction_id: String,
    object_id: String,
    /// Where it applies, along its road.
    stretch: (f64, f64),
}

/// Where an object sits along its road, and how big it is there. An
/// `<object>` gives one of these, and a `<repeat>` gives one per station.
struct Station {
    s: f64,
    t: f64,
    /// Where the object stands, if not on the road at `(s, t)`: on the line
    /// of a repeat detached from the reference line.
    ground: Option<Point>,
    z_offset: f64,
    length: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    radius: Option<f64>,
}

/// An object's own frame at one station: its origin, and its u, v and z
/// axes in the network's frame.
struct Frame {
    origin: Point,
    axes: [[f64; 3]; 3],
}

impl Frame {
    /// The frame of an object at `st` on `road`, turned `(hdg, pitch, roll)`.
    ///
    /// Upright, `hdg` turns it from the road's heading in plan, and `pitch`
    /// and `roll` from the horizontal, and `zOffset` raises it straight up.
    /// `perp_to_road` makes it follow the surface instead: the road's axes
    /// there, from [`Road::axes`], turned by `hdg`, raised along the normal,
    /// with `pitch` and `roll` ignored.
    fn at(
        road: &Road,
        st: &Station,
        perp_to_road: bool,
        (hdg, pitch, roll): (f64, f64, f64),
    ) -> Self {
        let (on_road, heading) = road.surface(st.s, st.t);
        let ground = st.ground.unwrap_or(on_road);
        if perp_to_road {
            let axes = road.axes(st.s);
            let raise = axes[2].map(|c| (c * st.z_offset) as f32);
            return Self::on_road(ground + Vector::from_array(raise), axes, (hdg, 0.0, 0.0));
        }
        let level = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let origin = ground + Vector::Z * st.z_offset as f32;
        Self::on_road(origin, level, (heading + hdg, pitch, roll))
    }

    /// The frame of an object turned `hdg`, `pitch` and `roll` against the
    /// `road` axes, as [`orient`] turns them, with its origin at `origin`.
    fn on_road(origin: Point, road: [[f64; 3]; 3], (hdg, pitch, roll): (f64, f64, f64)) -> Self {
        let turned = |local: [f64; 3]| {
            let [u, v, z] = orient(hdg, pitch, roll, local);
            [0, 1, 2].map(|i| road[0][i] * u + road[1][i] * v + road[2][i] * z)
        };
        Self {
            origin,
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(turned),
        }
    }

    /// A point given in this frame, in the network's.
    fn point(&self, local: [f64; 3]) -> Point {
        let [u, v, w] = self.axes;
        let at = |i: usize| local[0] * u[i] + local[1] * v[i] + local[2] * w[i];
        self.origin + Vector::new(at(0) as f32, at(1) as f32, at(2) as f32)
    }

    /// The frame's orientation as the yaw, pitch and roll [`orient`] takes.
    /// Pitched straight up or down, yaw and roll turn about the same axis,
    /// so the roll is 0.
    fn angles(&self) -> (f64, f64, f64) {
        let [u, v, w] = self.axes;
        let pitch = (-u[2]).clamp(-1.0, 1.0).asin();
        if u[2].abs() > 1.0 - 1e-9 {
            return (f64::atan2(-v[0], v[1]), pitch, 0.0);
        }
        (f64::atan2(u[1], u[0]), pitch, f64::atan2(v[2], w[2]))
    }
}

/// What placing one object found that its warnings report.
#[derive(Default)]
struct Faults {
    /// Whether a repeat or a dashed marking would make too many copies.
    too_many: Cell<bool>,
    /// The widest gap between one `<curveLocal>` and the next, in metres.
    gap: Cell<f64>,
    /// Whether a `<skeleton>` `<polyline>` could not be placed.
    skeleton_dropped: Cell<bool>,
    /// Whether an `<outline>` could not be placed.
    outline_dropped: Cell<bool>,
    /// Whether a `<curveLocal>` set its `hdg` aside to join up.
    turned: Cell<bool>,
}

/// The structures baked so far, and the provenance of each, in step.
#[derive(Default)]
struct Structures {
    baked: Vec<Structure>,
    provenance: Vec<StructureProvenance>,
}

/// Bake every `<tunnel>` and `<bridge>` under one road's `<objects>` into
/// `out`, as the part of each of the road's `lanes` it covers.
///
/// A structure covers `length` metres of road from `s`, across every lane
/// there, or across those in its `<validity>` ranges if it has any. It has no
/// geometry: OpenDRIVE describes neither a tunnel's tube nor a bridge's deck.
/// One missing `s` or `length`, or with a negative length, is skipped.
fn place_structures(
    objects_node: roxmltree::Node,
    road: &Road,
    lanes: &[Lane],
    out: &mut Structures,
) {
    for node in objects_node.children() {
        let text = |name| node.attribute(name).unwrap_or_default().to_string();
        let kind = if node.has_tag_name("tunnel") {
            StructureKind::Tunnel {
                kind: text("type"),
                lighting: attr_f64(node, "lighting").map(|v| v as f32),
                daylight: attr_f64(node, "daylight").map(|v| v as f32),
            }
        } else if node.has_tag_name("bridge") {
            StructureKind::Bridge { kind: text("type") }
        } else {
            continue;
        };
        let (Some(s), Some(length)) = (attr_f64(node, "s"), attr_f64(node, "length")) else {
            continue;
        };
        if length < 0.0 {
            continue;
        }
        let validity = validity(node);
        let mut covered = Vec::new();
        for sec in &road.sections {
            let (from, to) = (s.max(sec.start), (s + length).min(sec.end));
            if to - from < 1e-6 {
                continue;
            }
            for &(_, id) in sec
                .lanes
                .iter()
                .filter(|(od_id, _)| is_valid(*od_id, sec.layer, &validity))
            {
                let center = &lanes[id.0].center;
                covered.push(Coverage {
                    lane: id,
                    from: along(center.points(), &sec.stations, from),
                    to: along(center.points(), &sec.stations, to),
                });
            }
        }
        let id = StructureId(out.baked.len());
        out.baked.push(Structure {
            id,
            kind,
            name: text("name"),
            lanes: covered,
        });
        out.provenance.push(StructureProvenance {
            structure: id,
            road_id: road.od_id.clone(),
            od_id: text("id"),
            s,
            length,
        });
    }
}

/// A junction's reference line, from its `<planView>`, as a road for its
/// `<objects>` to stand along: no `od_id`, and no lanes, elevation or
/// superelevation, as the spec says. So an object there stands at `z` 0,
/// raised by its `zOffset`. `None` for a junction with no `<geometry>` the
/// crate can bake.
///
/// The spec allows one `<line>`. The crate bakes any geometry a road may
/// have.
fn junction_line(junction: roxmltree::Node) -> Option<Road> {
    let mut geoms: Vec<GeomRec> = child(junction, "planView")?
        .children()
        .filter(|n| n.has_tag_name("geometry"))
        .filter_map(geometry)
        .collect();
    if geoms.is_empty() {
        return None;
    }
    geoms.sort_by(|a, b| a.s.total_cmp(&b.s));
    Some(Road {
        id: RoadId(usize::MAX),
        od_id: String::new(),
        name: String::new(),
        junction: None,
        length: geoms.iter().map(|g| g.s + g.length).fold(0.0, f64::max),
        rule: TrafficRule::RightHand,
        geoms,
        elevations: Vec::new(),
        superelevations: Vec::new(),
        lateral: Lateral::default(),
        lane_offsets: Vec::new(),
        temporary_lane_offsets: Vec::new(),
        sections: Vec::new(),
    })
}

/// Every `<object>` in the document by its id, with the road it is on, for
/// an `<objectReference>` to find. The road is empty for an object of a
/// junction. Ids are meant to be unique across the file. Where two objects
/// share one, the first wins.
type ObjectIndex<'a> = HashMap<&'a str, (&'a str, roxmltree::Node<'a, 'a>)>;

fn object_index<'a>(root: roxmltree::Node<'a, 'a>) -> ObjectIndex<'a> {
    let mut index = ObjectIndex::new();
    for parent in root
        .children()
        .filter(|n| n.has_tag_name("road") || n.has_tag_name("junction"))
    {
        let road_id = match parent.has_tag_name("road") {
            true => parent.attribute("id").unwrap_or_default(),
            false => "",
        };
        let Some(objects) = child(parent, "objects") else {
            continue;
        };
        for object in objects.children().filter(|n| n.has_tag_name("object")) {
            if let Some(id) = object.attribute("id") {
                index.entry(id).or_insert((road_id, object));
            }
        }
    }
    index
}

/// Where one `<object>` is baked, and what its provenance says about it. An
/// `<object>` is placed at its own station. An `<objectReference>` places the
/// `<object>` it names at the reference's station instead, with the
/// reference's `zOffset`, `orientation` and `validLength`.
struct Placement<'a> {
    s: f64,
    t: f64,
    z_offset: f64,
    orientation: Orientation,
    valid_length: Option<f64>,
    /// How far the placement moves the object along and across the road,
    /// from the station its `<object>` gives to this one. What the
    /// `<object>` gives in road coordinates, its `<repeat>`s and its
    /// `<cornerRoad>`s, moves with it. `(0, 0)` for an `<object>` placed
    /// where it says.
    shift: (f64, f64),
    /// For a reference, the road the `<object>` is on.
    referenced_from: Option<&'a str>,
    /// The junction whose `<objects>` hold it, which it stands along the
    /// reference line of. `None` on a road.
    junction: Option<&'a str>,
    /// The `<validity>` lane ranges, `fromLane` and `toLane`, of the
    /// `<object>` or the `<objectReference>`. A reference does not take the
    /// `<object>`'s, which name lanes on the `<object>`'s road.
    validity: Vec<Validity>,
}

impl<'a> Placement<'a> {
    /// An `<object>` where it says it is, or `None` if it is missing `s` or
    /// `t`.
    fn own(object: roxmltree::Node) -> Option<Self> {
        Some(Self {
            s: attr_f64(object, "s")?,
            t: attr_f64(object, "t")?,
            z_offset: attr_f64(object, "zOffset").unwrap_or(0.0),
            orientation: orientation(object),
            valid_length: attr_f64(object, "validLength"),
            shift: (0.0, 0.0),
            referenced_from: None,
            junction: None,
            validity: validity(object),
        })
    }

    /// `object`, on `road`, where `reference` puts it, or `None` if the
    /// reference is missing `s` or `t`.
    fn reference(
        reference: roxmltree::Node,
        object: roxmltree::Node,
        road: &'a str,
    ) -> Option<Self> {
        let (s, t) = (attr_f64(reference, "s")?, attr_f64(reference, "t")?);
        let from = |name| attr_f64(object, name).unwrap_or(0.0);
        Some(Self {
            s,
            t,
            z_offset: attr_f64(reference, "zOffset").unwrap_or(0.0),
            orientation: orientation(reference),
            valid_length: attr_f64(reference, "validLength"),
            shift: (s - from("s"), t - from("t")),
            referenced_from: Some(road),
            junction: None,
            validity: validity(reference),
        })
    }
}

/// The `fromLane`-`toLane` range of each `<validity>` under `node`. One
/// missing either is skipped.
fn validity(node: roxmltree::Node) -> Vec<Validity> {
    let lane = attr_i32;
    node.children()
        .filter(|n| n.has_tag_name("validity"))
        .filter_map(|v| {
            Some((
                lane(v, "fromLane")?,
                lane(v, "toLane")?,
                Layer::from_attr(v.attribute("layer")),
            ))
        })
        .collect()
}

/// Which direction of the road an `<object>` or `<objectReference>` applies
/// to.
fn orientation(node: roxmltree::Node) -> Orientation {
    match node.attribute("orientation") {
        Some("+") => Orientation::Positive,
        Some("-") => Orientation::Negative,
        _ => Orientation::Both,
    }
}

/// Bake every `<object>` and `<objectReference>` under one road's
/// `<objects>` into `out`, or under the `junction`'s with `road` its
/// reference line. `index` finds the `<object>` a reference names.
///
/// A reference to an id no `<object>` has is skipped, as is one missing `s`
/// or `t`.
fn place_objects<'a>(
    objects_node: roxmltree::Node<'a, 'a>,
    index: &ObjectIndex<'a>,
    road: &Road,
    junction: Option<&'a str>,
    out: &mut Objects,
) {
    for node in objects_node.children() {
        let placed = if node.has_tag_name("object") {
            Placement::own(node).map(|at| (node, at))
        } else if node.has_tag_name("objectReference") {
            node.attribute("id")
                .and_then(|id| index.get(id))
                .and_then(|&(road, object)| {
                    Placement::reference(node, object, road).map(|at| (object, at))
                })
        } else {
            None
        };
        if let Some((object, mut at)) = placed {
            at.junction = junction;
            place_object(object, &at, road, out);
        }
    }
}

/// Bake one `<object>` into `out`, placed on `road` as `at` says.
///
/// Each object sits on the road surface, so it rides the elevation and
/// superelevation profiles like a lane does. Its own frame is a [`Frame::at`]
/// there, upright or following the road. One `<object>` can bake to several
/// [`Object`]s, following libOpenDRIVE:
///
/// - with neither `<repeat>`s nor outlines, one [`Shape::Solid`];
/// - one [`Shape::Sweep`] per `<repeat>` with a `distance` of 0;
/// - at each step of each `<repeat>` with a `distance`, its outlines, or a
///   [`Shape::Solid`] if it has none. Unlike libOpenDRIVE, which bakes the
///   outlines once, the outlines move to each step: `<cornerLocal>` corners
///   with the object's frame, `<cornerRoad>` corners by the step's offset
///   from the object's `(s, t)`;
/// - with no such repeat, one [`Shape::Outline`] per outer `<outline>`, see
///   [`outline_parts`]. An object with outlines gets no solid of its own.
///
/// Any part of it that falls off the ends of the road is skipped: a solid, a
/// whole outline with a corner there, or the length of a sweep past the end.
///
/// Each baked object applies to the lanes alongside the stretch of road it
/// spans, narrowed by `at`'s validity.
fn place_object(node: roxmltree::Node, at: &Placement, road: &Road, out: &mut Objects) {
    let faults = Faults::default();
    let too_many = &faults.too_many;
    let base = Station {
        s: at.s,
        t: at.t,
        ground: None,
        z_offset: at.z_offset,
        length: attr_f64(node, "length"),
        width: attr_f64(node, "width"),
        height: attr_f64(node, "height"),
        radius: attr_f64(node, "radius"),
    };
    let hdg = attr_f64(node, "hdg").unwrap_or(0.0);
    let pitch = attr_f64(node, "pitch").unwrap_or(0.0);
    let roll = attr_f64(node, "roll").unwrap_or(0.0);
    let perp_to_road = matches!(node.attribute("perpToRoad"), Some("true" | "1"));
    let frame = |st: &Station| Frame::at(road, st, perp_to_road, (hdg, pitch, roll));
    let point = |st: &Station| {
        let f = frame(st);
        let (yaw, pitch, roll) = f.angles();
        let solid = Shape::Solid {
            position: f.origin,
            heading: yaw as f32,
            pitch: pitch as f32,
            roll: roll as f32,
            extent: extent(st),
        };
        let mut part = Part::new(solid, st, (st.s, st.s));
        part.markings = side_markings(node, &f, extent(st), too_many);
        part
    };

    let mut parts = Vec::new();
    let repeats: Vec<Repeat> = node
        .children()
        .filter(|n| n.has_tag_name("repeat"))
        .filter_map(|n| Repeat::parse(n, at.shift, &base, road))
        .collect();
    for (t_end, reached) in repeats.iter().filter_map(|r| r.missed_end(&base)) {
        out.warnings.push(Warning::RepeatEndMismatch {
            road_id: road.od_id.clone(),
            object_id: node.attribute("id").unwrap_or_default().to_string(),
            t_end,
            reached,
        });
    }
    let outlines = outline_nodes(node);
    let skeleton = child(node, "skeleton");
    let shaped = !outlines.is_empty() || skeleton.is_some();
    let outlined = |st: &Station| {
        let origin = road.on_road(st.s).then(|| frame(st));
        let shift = (at.shift.0 + st.s - base.s, at.shift.1 + st.t - base.t);
        let mut parts = outline_parts(node, &outlines, origin.as_ref(), st, shift, road, &faults);
        let bones =
            skeleton.and_then(|n| skeleton_part(n, origin.as_ref(), st, shift, road, &faults));
        match bones {
            Some(part) => parts.push(part),
            None if parts.is_empty() && road.on_road(st.s) => parts.push(point(st)),
            None => {}
        }
        parts
    };
    for repeat in &repeats {
        if !out.affords(repeat.least_cost()) {
            too_many.set(true);
        } else if repeat.distance > 0.0 {
            let Some(stations) = repeat.stations(&base) else {
                too_many.set(true);
                continue;
            };
            let on_road = stations.iter().find(|st| road.on_road(st.s));
            let corners = match (shaped, on_road) {
                (true, Some(st)) => outlined(st).iter().map(Part::corner_count).sum(),
                _ => 0,
            };
            let per_station = outlines.len() + usize::from(skeleton.is_some());
            if corners.saturating_mul(stations.len()) > MAX_OUTLINE_CORNERS
                || !out.spend(Cost {
                    objects: stations.len() * per_station.max(1),
                    pieces: stations.len() * corners,
                })
            {
                too_many.set(true);
                continue;
            }
            for st in stations {
                if shaped {
                    parts.extend(outlined(&st));
                } else if road.on_road(st.s) {
                    parts.push(point(&st));
                }
            }
        } else {
            let most = MAX_OBJECT_PIECES.saturating_sub(out.cost.pieces);
            match sweep(repeat, &base, road, most) {
                Ok(Some((shape, start, end))) => {
                    let part = Part::new(shape, &start, (start.s, end));
                    out.spend(part.cost());
                    parts.push(part);
                }
                Ok(None) => {}
                Err(TooMany) => too_many.set(true),
            }
        }
    }
    if !repeats.iter().any(|r| r.distance > 0.0) {
        let mut own = Vec::new();
        if shaped {
            own.extend(outlined(&base));
        } else if repeats.is_empty() && road.on_road(base.s) {
            own.push(point(&base));
        }
        match out.spend(own.iter().map(Part::cost).sum()) {
            true => parts.extend(own),
            false => too_many.set(true),
        }
    }

    let kind = object_type(node.attribute("type"));
    let text = |name| node.attribute(name).unwrap_or_default().to_string();
    let junction_id = at.junction.unwrap_or_default().to_string();
    if let Some(crg) = child(node, "surface").and_then(|s| child(s, "CRG")) {
        let surfaces: Vec<ObjectCrg> = parts
            .iter()
            .filter_map(|part| {
                let (surface, stretch) = object_crg(crg, part, road)?;
                Some(ObjectCrg {
                    surface,
                    road_id: road.od_id.clone(),
                    junction_id: junction_id.clone(),
                    object_id: text("id"),
                    stretch,
                })
            })
            .collect();
        if surfaces.is_empty() {
            out.warnings.push(Warning::ObjectSurfaceDropped {
                road_id: road.od_id.clone(),
                junction_id: junction_id.clone(),
                object_id: text("id"),
            });
        }
        out.surfaces.extend(surfaces);
    }
    let flag = |name| matches!(node.attribute(name), Some("true" | "1"));
    let parking_space = child(node, "parkingSpace").map(|p| {
        let text = |name| p.attribute(name).unwrap_or_default().to_string();
        ParkingSpace {
            access: text("access"),
            restrictions: text("restrictions"),
        }
    });
    let materials: Vec<Material> = node
        .children()
        .filter(|n| n.has_tag_name("material"))
        .map(material)
        .collect();
    let mark_colors: Vec<String> = node
        .children()
        .filter(|n| n.has_tag_name("material"))
        .map(|m| m.attribute("roadMarkColor").unwrap_or_default().to_string())
        .collect();
    let user_data: Vec<UserData> = node
        .children()
        .filter(|n| n.has_tag_name("userData"))
        .map(|u| {
            let text = |name| u.attribute(name).unwrap_or_default().to_string();
            UserData {
                code: text("code"),
                value: text("value"),
            }
        })
        .collect();
    if too_many.get() {
        out.warnings.push(Warning::TooManyCopies {
            road_id: road.od_id.clone(),
            object_id: text("id"),
        });
    }
    if faults.turned.get() {
        out.warnings.push(Warning::CurveHeadingIgnored {
            road_id: road.od_id.clone(),
            junction_id: junction_id.clone(),
            object_id: text("id"),
        });
    }
    if faults.outline_dropped.get() {
        out.warnings.push(Warning::OutlineDropped {
            road_id: road.od_id.clone(),
            junction_id: junction_id.clone(),
            object_id: text("id"),
        });
    }
    if faults.skeleton_dropped.get() {
        out.warnings.push(Warning::SkeletonDropped {
            road_id: road.od_id.clone(),
            junction_id: junction_id.clone(),
            object_id: text("id"),
        });
    }
    if faults.gap.get() > 0.0 {
        out.warnings.push(Warning::OutlineGap {
            road_id: road.od_id.clone(),
            object_id: text("id"),
            gap: faults.gap.get(),
        });
    }
    for part in parts {
        let id = ObjectId(out.baked.len());
        let (markings, marking_weights): (Vec<Marking>, Vec<RoadMarkWeight>) =
            part.markings.into_iter().unzip();
        out.baked.push(Object {
            id,
            kind,
            subtype: text("subtype"),
            name: text("name"),
            dynamic: node.attribute("dynamic") == Some("yes"),
            lanes: road.lanes(part.stretch, &at.validity),
            markings,
            borders: part.borders,
            parking_space: parking_space.clone(),
            materials: materials.clone(),
            user_data: user_data.clone(),
            shape: part.shape,
        });
        out.provenance.push(ObjectProvenance {
            object: id,
            road_id: road.od_id.clone(),
            junction_id: junction_id.clone(),
            od_id: text("id"),
            s: part.s,
            t: part.t,
            orientation: at.orientation,
            valid_length: at.valid_length,
            referenced_from: at.referenced_from.map(str::to_string),
            invalidated: flag("invalidated"),
            temporary: flag("temporary"),
            marking_weights,
            material_road_mark_colors: mark_colors.clone(),
            fill_type: part.fill_type,
            outline_lane_type: part.outline_lane_type,
            skeleton_ids: part.skeleton_ids,
        });
    }
}

/// An object's `<surface>` `<CRG>`, for one solid `part` of it on `road`,
/// and the stretch of road it covers. `None` unless the part is a box with a
/// length and a width, the one shape the spec allows a surface.
///
/// The CRG covers the lanes of `road` within the box's diagonal of the
/// part's station, which takes in the box on a curve too. It covers no
/// other road's lanes. An object's `zOffset` and `height` play no part, as
/// the spec says.
fn object_crg(crg: roxmltree::Node, part: &Part, road: &Road) -> Option<(CrgSurface, (f64, f64))> {
    let Shape::Solid {
        position,
        heading,
        extent: Some(Extent::Box { length, width, .. }),
        ..
    } = part.shape
    else {
        return None;
    };
    if length <= 0.0 || width <= 0.0 {
        return None;
    }
    let (half_length, half_width) = (f64::from(length) / 2.0, f64::from(width) / 2.0);
    let reach = 2.0 * half_length.hypot(half_width);
    let stretch = (part.s - reach, part.s + reach);
    let mode = CrgMode::Object(CrgObject {
        origin: CrgPose {
            x: f64::from(position.x),
            y: f64::from(position.y),
            heading: f64::from(heading),
        },
        half_length,
        half_width,
        hide_road: !matches!(crg.attribute("hideRoadSurfaceCRG"), Some("false" | "0")),
    });
    let mut surface = crg_surface(crg, mode, road.lanes(stretch, &[]), None);
    surface.purpose = CrgPurpose::Elevation;
    surface.z_offset = 0.0;
    surface.z_scale = attr_f64(crg, "zScale").unwrap_or(1.0);
    Some((surface, stretch))
}

/// An object's `outlines` placed at `st`, one [`Part`] per outer outline,
/// with its holes, markings and borders. `origin` is the object's frame at
/// `st`, if that is on the road, and `shift` moves `<cornerRoad>` corners.
///
/// An `outer="false"` outline is a hole in the first closed outer outline
/// that encloses it in plan. One with a `fillType` or a `laneType` is an
/// area of its own as well, as the spec has inner outlines, and gets a
/// [`Part`] with its own markings and borders, whether or not an outer one
/// encloses it. Otherwise its markings and borders go to the outline round
/// it, and it is dropped if none does.
fn outline_parts(
    node: roxmltree::Node,
    outlines: &[roxmltree::Node],
    origin: Option<&Frame>,
    st: &Station,
    shift: (f64, f64),
    road: &Road,
    faults: &Faults,
) -> Vec<Part> {
    let too_many = &faults.too_many;
    let mut parts = Vec::new();
    let mut holes = Vec::new();
    for &outline_node in outlines {
        let Some(ring) = outline(outline_node, origin, st.s, shift, road) else {
            faults.outline_dropped.set(true);
            continue;
        };
        faults.gap.set(faults.gap.get().max(ring.gap));
        faults.turned.set(faults.turned.get() || ring.turned);
        if matches!(outline_node.attribute("outer"), Some("false" | "0")) {
            holes.push((outline_node, ring));
        } else {
            parts.push(area(node, outline_node, &ring, st, too_many));
        }
    }
    let outer = parts.len();
    for (outline_node, ring) in holes {
        let filled = outline_node.attribute("fillType").is_some()
            || outline_node.attribute("laneType").is_some();
        let corners = &ring.corners;
        let owner = parts[..outer].iter_mut().find(|p| {
            matches!(&p.shape, Shape::Outline { corners: outer, closed: true, .. }
                if corners.len() >= 3 && corners.iter().all(|c| encloses(outer, c.base)))
        });
        if owner.is_none() && !filled {
            faults.outline_dropped.set(true);
        }
        if let Some(owner) = owner {
            if !filled {
                owner
                    .markings
                    .extend(markings(node, outline_node, &ring, too_many));
                owner
                    .borders
                    .extend(borders(node, outline_node, &ring, true));
            }
            let painted = owner.fill_type == "paint";
            if let Shape::Outline { holes, .. } = &mut owner.shape {
                holes.push(if painted {
                    ring.paint()
                } else {
                    ring.corners.clone()
                });
            }
        }
        if filled {
            parts.push(area(node, outline_node, &ring, st, too_many));
        }
    }
    parts
}

/// One outline of `node`, `outline_node`, placed as `ring`, as an area of
/// its own: its shape, laid flat if it is closed and of paint, its markings
/// and borders, and its `fillType` and `laneType`.
fn area(
    node: roxmltree::Node,
    outline_node: roxmltree::Node,
    ring: &Ring,
    st: &Station,
    too_many: &Cell<bool>,
) -> Part {
    let painted = ring.closed && outline_node.attribute("fillType") == Some("paint");
    let mut part = Part::new(
        Shape::Outline {
            corners: if painted {
                ring.paint()
            } else {
                ring.corners.clone()
            },
            closed: ring.closed,
            holes: Vec::new(),
        },
        st,
        ring.stretch,
    );
    part.markings = markings(node, outline_node, ring, too_many);
    part.borders = borders(node, outline_node, ring, ring.closed);
    part.fill_type = outline_node
        .attribute("fillType")
        .unwrap_or_default()
        .to_string();
    part.outline_lane_type = outline_node
        .attribute("laneType")
        .map(|t| lane_type(Some(t)));
    part
}

/// One shape an `<object>` bakes to, and what goes with it, before it becomes
/// an [`Object`].
struct Part {
    shape: Shape,
    /// The road station it is anchored at.
    s: f64,
    t: f64,
    /// The stretch of road it spans, which picks its lanes.
    stretch: (f64, f64),
    markings: Vec<(Marking, RoadMarkWeight)>,
    borders: Vec<Border>,
    /// The `fillType` and `laneType` of the outline it came from.
    fill_type: String,
    outline_lane_type: Option<LaneType>,
    /// The ids of the skeleton it came from.
    skeleton_ids: Vec<SkeletonIds>,
}

impl Part {
    /// What baking it costs: one object, and its corners, vertices or
    /// sections.
    fn cost(&self) -> Cost {
        let sections = match &self.shape {
            Shape::Sweep { sections, .. } => sections.len(),
            _ => 0,
        };
        Cost {
            objects: 1,
            pieces: self.corner_count() + sections,
        }
    }

    /// How many corners its outline has, holes included, or vertices its
    /// skeleton has. 0 for any other shape.
    fn corner_count(&self) -> usize {
        match &self.shape {
            Shape::Outline { corners, holes, .. } => {
                corners.len() + holes.iter().map(Vec::len).sum::<usize>()
            }
            Shape::Skeleton { lines } => lines.iter().map(Vec::len).sum(),
            _ => 0,
        }
    }

    fn new(shape: Shape, at: &Station, stretch: (f64, f64)) -> Self {
        Self {
            shape,
            s: at.s,
            t: at.t,
            stretch,
            markings: Vec::new(),
            borders: Vec::new(),
            fill_type: String::new(),
            outline_lane_type: None,
            skeleton_ids: Vec::new(),
        }
    }
}

/// One `<repeat>`: the object again along `length` metres of road from `s`,
/// either every `distance` metres or, for a `distance` of 0, continuously.
struct Repeat<'a> {
    node: roxmltree::Node<'a, 'a>,
    start: f64,
    length: f64,
    distance: f64,
    /// Added to the `t` values the repeat gives, from its [`Placement`].
    shift_t: f64,
    /// `bT`, `cT` and `dT`, if it gives any of them. Then `t` is a cubic in
    /// the distance from `start`, and `tEnd` plays no part. A negative one,
    /// which the schema forbids, is read as written.
    cubic: Option<[f64; 3]>,
    /// For `detachFromReferenceLine="true"`, the ground at its start and its
    /// end. Its copies stand on the straight line between them.
    line: Option<[Point; 2]>,
}

impl<'a> Repeat<'a> {
    /// The repeat moved by a [`Placement`]'s `shift`, for an object at
    /// `base` on `road`, or `None` for one missing its `s`, `length` or
    /// `distance`, or carrying a negative one of the last two.
    fn parse(
        node: roxmltree::Node<'a, 'a>,
        (shift_s, shift_t): (f64, f64),
        base: &Station,
        road: &Road,
    ) -> Option<Self> {
        let terms = ["bT", "cT", "dT"].map(|name| attr_f64(node, name));
        let mut repeat = Self {
            node,
            start: attr_f64(node, "s")? + shift_s,
            length: attr_f64(node, "length")?,
            distance: attr_f64(node, "distance")?,
            shift_t,
            cubic: terms
                .iter()
                .any(Option::is_some)
                .then(|| terms.map(|c| c.unwrap_or(0.0))),
            line: None,
        };
        if repeat.length < 0.0 || repeat.distance < 0.0 {
            return None;
        }
        if matches!(
            node.attribute("detachFromReferenceLine"),
            Some("true" | "1")
        ) {
            let end = |f: f64| {
                road.surface(repeat.start + repeat.length * f, repeat.t(base, f))
                    .0
            };
            repeat.line = Some([end(0.0), end(1.0)]);
        }
        Some(repeat)
    }

    /// The `t` a fraction `f` of the way along: the cubic from `tStart`, if
    /// the repeat has one, or else straight from `tStart` to `tEnd`. A `t`
    /// the repeat does not give is the object's own.
    fn t(&self, base: &Station, f: f64) -> f64 {
        let given = |name| attr_f64(self.node, name).map(|v| v + self.shift_t);
        let start = given("tStart").unwrap_or(base.t);
        match self.cubic {
            Some([b, c, d]) => {
                let ds = self.length * f;
                start + ds * (b + ds * (c + ds * d))
            }
            None => start + (given("tEnd").unwrap_or(start) - start) * f,
        }
    }

    /// Where a cubic `t` ends, if the repeat has one and its `tEnd` is more
    /// than a millimetre away. The spec says the two agree.
    fn missed_end(&self, base: &Station) -> Option<(f64, f64)> {
        self.cubic?;
        let t_end = attr_f64(self.node, "tEnd")? + self.shift_t;
        let reached = self.t(base, 1.0);
        ((reached - t_end).abs() > 1e-3).then_some((t_end, reached))
    }

    /// The object's station a fraction `f` of the way along the repeat, with
    /// `t` from [`Self::t`], and `zOffset` and each dimension interpolated
    /// from its start value to its end value. A value the repeat does not
    /// give is the object's own. On a detached repeat, the station's ground
    /// is on its line.
    fn at(&self, base: &Station, f: f64) -> Station {
        let lerp = |name: &str, fallback: Option<f64>| {
            let given = |end| attr_f64(self.node, &format!("{name}{end}"));
            let start = given("Start").or(fallback)?;
            let end = given("End").unwrap_or(start);
            Some(start + (end - start) * f)
        };
        Station {
            s: self.start + self.length * f,
            t: self.t(base, f),
            ground: self.line.map(|[a, b]| a.lerp(b, f as f32)),
            z_offset: lerp("zOffset", Some(base.z_offset)).unwrap_or(base.z_offset),
            length: lerp("length", base.length),
            width: lerp("width", base.width),
            height: lerp("height", base.height),
            radius: lerp("radius", base.radius),
        }
    }

    /// The least baking it can cost, known before it is baked: an object
    /// per station, or for a continuous repeat, one object and a section
    /// every [`SWEEP_MAX_STEP`].
    fn least_cost(&self) -> Cost {
        let count = |n: f64| n.min(usize::MAX as f64) as usize;
        match self.distance > 0.0 {
            true => Cost {
                objects: count(self.length / self.distance),
                pieces: 0,
            },
            false => Cost {
                objects: 1,
                pieces: count(self.length / SWEEP_MAX_STEP),
            },
        }
    }

    /// One station every `distance` metres, both ends included, or `None` if
    /// that is more than [`MAX_REPEAT_INSTANCES`].
    fn stations(&self, base: &Station) -> Option<Vec<Station>> {
        let count = (self.length / self.distance + 1e-9).floor() + 1.0;
        if count > MAX_REPEAT_INSTANCES {
            return None;
        }
        let stations = (0..count as usize)
            .map(|k| {
                let along = k as f64 * self.distance;
                let f = if self.length > 0.0 {
                    (along / self.length).min(1.0)
                } else {
                    0.0
                };
                self.at(base, f)
            })
            .collect();
        Some(stations)
    }
}

/// A continuous repeat as a [`Shape::Sweep`], with a cross-section at each of
/// its [`sweep_stations`] on the road: `width` wide about its `t`, `height`
/// tall from its `zOffset`. A missing width is 0, a wall with no thickness, as
/// libOpenDRIVE has it. With a `radius`, the sweep is round instead, a pipe
/// resting on `zOffset`, and `width` and `height` play no part. A detached
/// repeat's sections lie level and square to its line, or across the road
/// where its ends meet in plan. Also returns the station the sections start
/// at, and the `s` they end at. `None` if less than a millimetre of it is on
/// the road, and [`TooMany`] if it would take more than `most` sections.
fn sweep(
    repeat: &Repeat,
    base: &Station,
    road: &Road,
    most: usize,
) -> Result<Option<(Shape, Station, f64)>, TooMany> {
    let start = repeat.start.max(0.0);
    let end = (repeat.start + repeat.length).min(road.length);
    if end - start < 1e-3 {
        return Ok(None);
    }
    let first = repeat.at(base, (start - repeat.start) / repeat.length);
    let round = first.radius.is_some();
    let left = repeat
        .line
        .map(|[a, b]| Vector::new(a.y - b.y, b.x - a.x, 0.0).normalize_or_zero())
        .filter(|left| *left != Vector::ZERO);
    let section = |s: f64| {
        let st = repeat.at(base, (s - repeat.start) / repeat.length);
        let (half, height) = match st.radius {
            Some(r) => (r, 2.0 * r as f32),
            None => (
                st.width.unwrap_or(0.0) / 2.0,
                st.height.unwrap_or(0.0) as f32,
            ),
        };
        let corner = |side: f64| {
            let mut base = match (st.ground, left) {
                (Some(ground), Some(left)) => ground + left * (side * half) as f32,
                _ => road.surface(s, st.t + side * half).0,
            };
            base.z += st.z_offset as f32;
            Corner {
                base,
                top: base + Vector::Z * height,
            }
        };
        Section {
            left: corner(1.0),
            right: corner(-1.0),
        }
    };
    let sections = sweep_stations(start, end, most, |a, b, f| {
        let (a, b, at) = (section(a), section(b), section(a + (b - a) * f));
        let strays = |a: Corner, b: Corner, at: Corner| {
            a.base.lerp(b.base, f as f32).distance_to(at.base) > SWEEP_TOLERANCE
        };
        strays(a.left, b.left, at.left) || strays(a.right, b.right, at.right)
    })
    .ok_or(TooMany)?
    .into_iter()
    .map(section)
    .collect();
    Ok(Some((Shape::Sweep { sections, round }, first, end)))
}

/// A sweep with more sections than it may have.
struct TooMany;

/// The stations of a sweep's sections over `[start, end]`: at most
/// [`SWEEP_MAX_STEP`] apart, and closer where the sweep bends. `bends(a, b,
/// f)` says whether the sweep a fraction `f` of the way from `a` to `b`
/// strays from the straight line between its sections there. An interval
/// halves while it does at a quarter, half or three quarters of the way,
/// down to [`SWEEP_MIN_STEP`]. `None` once there would be more than `most`.
fn sweep_stations(
    start: f64,
    end: f64,
    most: usize,
    bends: impl Fn(f64, f64, f64) -> bool,
) -> Option<Vec<f64>> {
    let seeds = ((end - start) / SWEEP_MAX_STEP).ceil().max(1.0);
    if seeds >= most as f64 {
        return None;
    }
    let seeds = seeds as usize;
    let mut done = vec![start];
    let mut todo: Vec<f64> = (1..=seeds)
        .rev()
        .map(|k| start + (end - start) * k as f64 / seeds as f64)
        .collect();
    while let Some(&b) = todo.last() {
        let a = *done.last().expect("starts with start");
        if done.len() + todo.len() > most {
            return None;
        }
        if b - a > 2.0 * SWEEP_MIN_STEP && [0.25, 0.5, 0.75].iter().any(|&f| bends(a, b, f)) {
            todo.push((a + b) / 2.0);
        } else {
            done.push(b);
            todo.pop();
        }
    }
    Some(done)
}

/// An object's `<outline>`s: under `<outlines>` since OpenDRIVE 1.5, and
/// straight under the `<object>` in 1.4.
fn outline_nodes<'a>(object: roxmltree::Node<'a, 'a>) -> Vec<roxmltree::Node<'a, 'a>> {
    child(object, "outlines")
        .unwrap_or(object)
        .children()
        .filter(|n| n.has_tag_name("outline"))
        .collect()
}

/// One `<outline>` placed in the network's frame.
struct Ring<'a> {
    corners: Vec<Corner>,
    /// The `id` of each corner, in step with `corners`, for a
    /// `<cornerReference>` to name. A `<curveLocal>`'s is on its first.
    ids: Vec<Option<&'a str>>,
    /// Whether each corner is a sample along a `<curveLocal>` after its
    /// first, in step with `corners`.
    sampled: Vec<bool>,
    /// The normal of the surface each corner is given on, in step with
    /// `corners`: the road's for a `<cornerRoad>`, the object frame's z axis
    /// for a `<cornerLocal>`. Bands along the edges lie in it.
    ups: Vec<Vector>,
    closed: bool,
    /// The stretch of road the outline spans.
    stretch: (f64, f64),
    /// The widest gap between one `<curveLocal>` and the next, in metres.
    gap: f64,
    /// Whether a `<curveLocal>` set its `hdg` aside to join up.
    turned: bool,
}

impl Ring<'_> {
    /// The corners as an area of paint: each base raised [`PAINT_RAISE`]
    /// along its normal, with no height.
    fn paint(&self) -> Vec<Corner> {
        self.corners
            .iter()
            .zip(&self.ups)
            .map(|(c, &up)| {
                let base = c.base + up * PAINT_RAISE as f32;
                Corner { base, top: base }
            })
            .collect()
    }

    /// The samples of the curve from corner `from` to corner `to`, either
    /// way round the ring, or none if no curve joins them.
    fn along_curve(&self, from: usize, to: usize) -> Vec<usize> {
        let n = self.corners.len();
        let walk = |step: usize| {
            let mut found = Vec::new();
            let mut k = from;
            loop {
                let wraps = if step == 1 { k + 1 == n } else { k == 0 };
                if wraps && !self.closed {
                    return None;
                }
                k = (k + step) % n;
                if k == to {
                    return Some(found);
                }
                if !self.sampled[k] {
                    return None;
                }
                found.push(k);
            }
        };
        walk(1).or_else(|| walk(n - 1)).unwrap_or_default()
    }

    /// Each corner's base, with its normal.
    fn path(&self) -> Vec<(Point, Vector)> {
        self.corners
            .iter()
            .map(|c| c.base)
            .zip(self.ups.iter().copied())
            .collect()
    }
}

/// Place one `<outline>` in the network's frame.
///
/// A `<cornerRoad>` is a road station `(s, t)`, moved by `shift`, raised
/// `dz` straight up, with its top `height` straight above its base. A
/// `<cornerLocal>` is `(u, v, z)` in the object's own frame, so it needs the
/// object's origin to be on the road, with its top `height` up that frame's
/// axis. A `<curveLocal>` is a curve in the object's frame too, sampled to
/// corners by [`curve_local::sample`]. libOpenDRIVE raises a `<cornerRoad>`
/// along the road's normal instead, which differs on a banked road. An
/// outline with a corner that cannot be placed is dropped whole, because
/// the polygon without it is a different shape. So is one with fewer than
/// two corners.
///
/// The stretch of road it spans runs from its first `<cornerRoad>` station to
/// its last, taking in `s`, the object's own station, if it has a
/// `<cornerLocal>` or a `<curveLocal>`.
fn outline<'a>(
    node: roxmltree::Node<'a, 'a>,
    frame: Option<&Frame>,
    s: f64,
    (shift_s, shift_t): (f64, f64),
    road: &Road,
) -> Option<Ring<'a>> {
    let mut stretch = (f64::INFINITY, f64::NEG_INFINITY);
    let mut spans = |s: f64| stretch = (stretch.0.min(s), stretch.1.max(s));
    let closed = !matches!(node.attribute("closed"), Some("false" | "0"));
    let nodes: Vec<roxmltree::Node> = corner_nodes(node).collect();
    let mut corners = Vec::new();
    let mut ups = Vec::new();
    let mut ids = Vec::new();
    let mut sampled = Vec::new();
    let mut gap: f64 = 0.0;
    let mut turned = false;
    let mut heading = None;
    for (i, &c) in nodes.iter().enumerate() {
        let height = attr_f64(c, "height").unwrap_or(0.0);
        let previous = heading.take();
        if c.has_tag_name("cornerRoad") {
            let (s, t) = (attr_f64(c, "s")? + shift_s, attr_f64(c, "t")? + shift_t);
            if !road.on_road(s) {
                return None;
            }
            spans(s);
            let (mut base, _) = road.surface(s, t);
            base.z += attr_f64(c, "dz").unwrap_or(0.0) as f32;
            corners.push(Corner {
                base,
                top: base + Vector::Z * height as f32,
            });
            ups.push(normal(road.axes(s)));
            ids.push(c.attribute("id"));
            sampled.push(false);
            continue;
        }
        let frame = frame?;
        spans(s);
        let local: Vec<[f64; 4]> = if c.has_tag_name("curveLocal") {
            let next = nodes.get(i + 1).or(nodes.first().filter(|_| closed));
            let next = next.filter(|n| !n.has_tag_name("cornerRoad")).copied();
            let curve = curve_local::sample(c, next, previous)?;
            heading = Some(curve.end_heading);
            gap = gap.max(curve.gap);
            turned |= curve.turned;
            curve.points
        } else {
            let z = attr_f64(c, "z").unwrap_or(0.0);
            vec![[attr_f64(c, "u")?, attr_f64(c, "v")?, z, height]]
        };
        for (k, [u, v, z, height]) in local.into_iter().enumerate() {
            corners.push(Corner {
                base: frame.point([u, v, z]),
                top: frame.point([u, v, z + height]),
            });
            ups.push(normal(frame.axes));
            ids.push(c.attribute("id").filter(|_| k == 0));
            sampled.push(k > 0);
        }
    }
    if corners.len() < 2 {
        return None;
    }
    Some(Ring {
        corners,
        ids,
        sampled,
        ups,
        // Closed unless the file says otherwise, as libOpenDRIVE reads it.
        closed,
        stretch,
        gap,
        turned,
    })
}

/// An object's `<skeleton>` placed at `st`, as one [`Part`] of every
/// `<polyline>` it can place, or `None` if it can place none.
///
/// A `<vertexRoad>` is a road station `(s, t)`, moved by `shift`, raised
/// `dz` straight up, as a `<cornerRoad>` is. A `<vertexLocal>` is
/// `(u, v, z)` in the object's own frame, `frame`, which it needs. A
/// polyline with a vertex that cannot be placed is dropped whole, as is one
/// with fewer than two vertices, and either sets `faults`. A missing or
/// negative `radius` is 0.
///
/// The stretch of road it spans runs from the first `<vertexRoad>` station
/// of the polylines it places to the last, taking in the object's own if
/// one has a `<vertexLocal>`.
fn skeleton_part(
    node: roxmltree::Node,
    frame: Option<&Frame>,
    st: &Station,
    (shift_s, shift_t): (f64, f64),
    road: &Road,
    faults: &Faults,
) -> Option<Part> {
    let vertex = |v: roxmltree::Node| -> Option<(SkeletonVertex, f64)> {
        let (position, s) = if v.has_tag_name("vertexRoad") {
            let (s, t) = (attr_f64(v, "s")? + shift_s, attr_f64(v, "t")? + shift_t);
            if !road.on_road(s) {
                return None;
            }
            let (base, _) = road.surface(s, t);
            (
                base + Vector::Z * attr_f64(v, "dz").unwrap_or(0.0) as f32,
                s,
            )
        } else {
            let z = attr_f64(v, "z").unwrap_or(0.0);
            let point = frame?.point([attr_f64(v, "u")?, attr_f64(v, "v")?, z]);
            (point, st.s)
        };
        let vertex = SkeletonVertex {
            position,
            radius: attr_f64(v, "radius").unwrap_or(0.0).max(0.0) as f32,
            ground: matches!(v.attribute("intersectionPoint"), Some("true" | "1")),
        };
        Some((vertex, s))
    };
    let text = |n: roxmltree::Node| n.attribute("id").unwrap_or_default().to_string();
    let mut stretch = (f64::INFINITY, f64::NEG_INFINITY);
    let mut lines = Vec::new();
    let mut ids = Vec::new();
    for polyline in node.children().filter(|n| n.has_tag_name("polyline")) {
        let vertices: Vec<roxmltree::Node> = polyline
            .children()
            .filter(|n| n.has_tag_name("vertexRoad") || n.has_tag_name("vertexLocal"))
            .collect();
        let placed = vertices
            .iter()
            .map(|&v| vertex(v))
            .collect::<Option<Vec<_>>>()
            .filter(|line| line.len() >= 2);
        let Some(line) = placed else {
            faults.skeleton_dropped.set(true);
            continue;
        };
        for &(_, s) in &line {
            stretch = (stretch.0.min(s), stretch.1.max(s));
        }
        lines.push(line.into_iter().map(|(v, _)| v).collect());
        ids.push(SkeletonIds {
            polyline: text(polyline),
            vertices: vertices.into_iter().map(text).collect(),
        });
    }
    if lines.is_empty() {
        return None;
    }
    let mut part = Part::new(Shape::Skeleton { lines }, st, stretch);
    part.skeleton_ids = ids;
    Some(part)
}

/// The z axis of `axes`, which [`road_axes`] and [`Frame`] give.
fn normal(axes: [[f64; 3]; 3]) -> Vector {
    Vector::from_array(axes[2].map(|c| c as f32))
}

/// Whether `point` is inside `ring` in plan, by the even-odd rule.
fn encloses(ring: &[Corner], point: Point) -> bool {
    let n = ring.len();
    (0..n).fold(false, |inside, i| {
        let (a, b) = (ring[i].base, ring[(i + 1) % n].base);
        let crosses = (a.y > point.y) != (b.y > point.y)
            && point.x < a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y);
        inside != crosses
    })
}

/// An `<outline>`'s corners, in order.
fn corner_nodes<'a>(
    outline: roxmltree::Node<'a, 'a>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    outline.children().filter(|n| {
        n.has_tag_name("cornerRoad")
            || n.has_tag_name("cornerLocal")
            || n.has_tag_name("curveLocal")
    })
}

/// The `<marking>` elements under the `<markings>` of `node`, an object or
/// an outline.
fn marking_nodes<'a>(
    node: roxmltree::Node<'a, 'a>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    child(node, "markings")
        .into_iter()
        .flat_map(|m| m.children())
        .filter(|n| n.has_tag_name("marking"))
}

/// The `<marking>`s that run along edges of one of `object`'s outlines,
/// `outline`, placed as `ring`: those under the outline's own `<markings>`,
/// as OpenDRIVE 1.9 writes them, then those under the object's.
///
/// A marking follows the corners its `<cornerReference>`s name, in order, by
/// the corners' `id`s. One under the object belongs to the first outline
/// that has every one of them, as corner ids are unique only within an
/// outline. One under the outline naming a corner it lacks is skipped. So is
/// one with fewer than two references.
fn markings(
    object: roxmltree::Node,
    outline: roxmltree::Node,
    ring: &Ring,
    too_many: &Cell<bool>,
) -> Vec<(Marking, RoadMarkWeight)> {
    let outlines = outline_nodes(object);
    let owned =
        |m: &roxmltree::Node| outlines.iter().find(|o| names_corners_of(m, o)) == Some(&outline);
    marking_nodes(outline)
        .chain(marking_nodes(object).filter(owned))
        .filter_map(|m| paint(m, &corner_path(m, ring)?, too_many))
        .collect()
}

/// Whether every `<cornerReference>` of `marking` names a corner of
/// `outline`.
fn names_corners_of(marking: &roxmltree::Node, outline: &roxmltree::Node) -> bool {
    let ids: Vec<&str> = outline
        .children()
        .filter_map(|c| c.attribute("id"))
        .collect();
    marking
        .children()
        .filter(|n| n.has_tag_name("cornerReference"))
        .all(|r| r.attribute("id").is_some_and(|id| ids.contains(&id)))
}

/// The `<marking>`s under `object`'s `<markings>` with no
/// `<cornerReference>`s, for a solid at `frame` filling `extent`. Each runs
/// along its `side` of the box round the extent, on its base: `front` and
/// `rear` across the +u and -u ends, `left` and `right` along the +v and -v
/// sides. The edge runs anticlockwise round the box seen from above. One
/// with any other side, or on a solid with no extent, is skipped.
fn side_markings(
    object: roxmltree::Node,
    frame: &Frame,
    extent: Option<Extent>,
    too_many: &Cell<bool>,
) -> Vec<(Marking, RoadMarkWeight)> {
    let (u, v) = match extent {
        Some(Extent::Box { length, width, .. }) => (length / 2.0, width / 2.0),
        Some(Extent::Cylinder { radius, .. }) => (radius, radius),
        None => return Vec::new(),
    };
    let (u, v) = (f64::from(u), f64::from(v));
    let corner = |u, v| (frame.point([u, v, 0.0]), normal(frame.axes));
    marking_nodes(object)
        .filter(|m| !m.children().any(|n| n.has_tag_name("cornerReference")))
        .filter_map(|m| {
            let edge = match m.attribute("side")? {
                "front" => [corner(u, -v), corner(u, v)],
                "left" => [corner(u, v), corner(-u, v)],
                "rear" => [corner(-u, v), corner(-u, -v)],
                "right" => [corner(-u, -v), corner(u, -v)],
                _ => return None,
            };
            paint(m, &edge, too_many)
        })
        .collect()
}

/// One `<marking>` painted along `path`, or `None` if it has no width. A
/// marking with more dashes than [`MAX_REPEAT_INSTANCES`] paints nothing,
/// and sets `too_many`.
///
/// The paint is raised `zOffset` off the path along its normals, 5 mm if the
/// map does not say, and starts `startOffset` along the first edge and stops `stopOffset` short
/// of the end of the last.
fn paint(
    m: roxmltree::Node,
    path: &[(Point, Vector)],
    too_many: &Cell<bool>,
) -> Option<(Marking, RoadMarkWeight)> {
    let width = attr_f64(m, "width").filter(|w| *w > 0.0)?;
    let raise = attr_f64(m, "zOffset").unwrap_or(PAINT_RAISE) as f32;
    let path: Vec<(Point, Vector)> = path.iter().map(|&(p, up)| (p + up * raise, up)).collect();
    let line = attr_f64(m, "lineLength").unwrap_or(0.0).max(0.0);
    let space = attr_f64(m, "spaceLength").unwrap_or(0.0).max(0.0);
    let text = |name| m.attribute(name).unwrap_or_default().to_string();
    let weight = match m.attribute("weight") {
        Some("bold") => RoadMarkWeight::Bold,
        _ => RoadMarkWeight::Standard,
    };
    let marking = Marking {
        side: text("side"),
        color: text("color"),
        width: width as f32,
        line_length: line as f32,
        space_length: space as f32,
        pieces: strip(
            &path,
            attr_f64(m, "startOffset").unwrap_or(0.0),
            attr_f64(m, "stopOffset").unwrap_or(0.0),
            (line > 0.0 && space > 0.0).then_some((line, space)),
            width / 2.0,
        )
        .unwrap_or_else(|| {
            too_many.set(true);
            Vec::new()
        }),
    };
    Some((marking, weight))
}

/// The `<border>`s under `object`'s `<borders>` that run along edges of one
/// of its outlines, `outline`, placed as `ring`.
///
/// A border belongs to the outline whose `id` is its `outlineId`, or with no
/// `outlineId`, to any outline that fits it. With `useCompleteOutline` it
/// runs along every edge, the closing one too if the outline is `closed`.
/// Otherwise it follows the corners its `<cornerReference>`s name, as a
/// [`Marking`] does. One with no width is skipped.
fn borders(
    object: roxmltree::Node,
    outline: roxmltree::Node,
    ring: &Ring,
    closed: bool,
) -> Vec<Border> {
    let Some(borders) = child(object, "borders") else {
        return Vec::new();
    };
    borders
        .children()
        .filter(|n| n.has_tag_name("border"))
        .filter(|b| {
            b.attribute("outlineId")
                .is_none_or(|id| outline.attribute("id") == Some(id))
        })
        .filter_map(|b| {
            let width = attr_f64(b, "width").filter(|w| *w > 0.0)?;
            let path = if matches!(b.attribute("useCompleteOutline"), Some("true" | "1")) {
                let mut path = ring.path();
                if closed {
                    path.push(path[0]);
                }
                path
            } else {
                corner_path(b, ring)?
            };
            Some(Border {
                kind: b.attribute("type").unwrap_or_default().to_string(),
                width: width as f32,
                pieces: strip(&path, 0.0, 0.0, None, width / 2.0).unwrap_or_default(),
            })
        })
        .collect()
}

/// The bases of the corners `node`'s `<cornerReference>`s name, in order,
/// with their normals, by the `id`s of `ring`'s corners. Between two named
/// corners joined by a `<curveLocal>`, either way round, the path follows
/// the curve's samples. `None` if a reference names no corner, or there are
/// fewer than two.
fn corner_path(node: roxmltree::Node, ring: &Ring) -> Option<Vec<(Point, Vector)>> {
    let named = node
        .children()
        .filter(|n| n.has_tag_name("cornerReference"))
        .map(|r| {
            let id = r.attribute("id")?;
            ring.ids.iter().position(|c| *c == Some(id))
        })
        .collect::<Option<Vec<usize>>>()?;
    if named.len() < 2 {
        return None;
    }
    let mut path = vec![named[0]];
    for w in named.windows(2) {
        path.extend(ring.along_curve(w[0], w[1]));
        path.push(w[1]);
    }
    Some(
        path.into_iter()
            .map(|k| (ring.corners[k].base, ring.ups[k]))
            .collect(),
    )
}

/// A strip `half_width` either side of the line through `path`, from `start`
/// metres along it to `stop` metres short of its end: one quad per edge, or
/// with `dashes` of `(line, space)` metres, one per dash per edge. Each point
/// of `path` comes with the normal of the surface it is on, and each quad
/// lies in the surface its edge's ends share, anticlockwise seen from that
/// normal. An edge along that normal has no across to measure, and gets no
/// quad. `None` for a pattern of more than [`MAX_REPEAT_INSTANCES`] dashes.
fn strip(
    path: &[(Point, Vector)],
    start: f64,
    stop: f64,
    dashes: Option<(f64, f64)>,
    half_width: f64,
) -> Option<Vec<[Point; 4]>> {
    let mut along = vec![0.0];
    for w in path.windows(2) {
        along.push(along.last().unwrap() + f64::from(w[0].0.distance_to(w[1].0)));
    }
    let end = along.last().unwrap() - stop;
    let painted: Vec<(f64, f64)> = match dashes {
        None => vec![(start, end)],
        Some((line, space)) if (end - start) / (line + space) > MAX_REPEAT_INSTANCES => {
            return None
        }
        Some((line, space)) => (0..)
            .map(|k| start + k as f64 * (line + space))
            .take_while(|&a| a < end)
            .map(|a| (a, (a + line).min(end)))
            .collect(),
    };
    let mut pieces = Vec::new();
    for (i, w) in path.windows(2).enumerate() {
        let ((a, up_a), (b, up_b)) = (w[0], w[1]);
        let across = (up_a + up_b).cross(b - a).normalize_or_zero() * half_width as f32;
        if across == Vector::ZERO {
            continue;
        }
        let (from, to) = (along[i], along[i + 1]);
        for &(p, q) in &painted {
            let (p, q) = (p.max(from), q.min(to));
            if q - p < 1e-6 {
                continue;
            }
            let at = |x: f64| a.lerp(b, ((x - from) / (to - from)) as f32);
            let (p, q) = (at(p), at(q));
            pieces.push([p - across, q - across, q + across, p + across]);
        }
    }
    Some(pieces)
}

/// The volume a solid occupies: a cylinder if it has a radius, a box if it
/// has any of a length, a width or a height, and nothing otherwise. A
/// dimension the box does not have is 0.
fn extent(st: &Station) -> Option<Extent> {
    let height = st.height.unwrap_or(0.0) as f32;
    if let Some(radius) = st.radius {
        return Some(Extent::Cylinder {
            radius: radius as f32,
            height,
        });
    }
    if st.length.is_none() && st.width.is_none() && st.height.is_none() {
        return None;
    }
    Some(Extent::Box {
        length: st.length.unwrap_or(0.0) as f32,
        width: st.width.unwrap_or(0.0) as f32,
        height,
    })
}

/// The [`ObjectType`] an OpenDRIVE `<object>` `type` maps to. The deprecated
/// moving participants (`car`, `pedestrian` and the rest) are
/// [`ObjectType::Unknown`] along with every other unrecognised name.
fn object_type(od_type: Option<&str>) -> ObjectType {
    match od_type.unwrap_or_default() {
        "none" => ObjectType::None,
        "obstacle" => ObjectType::Obstacle,
        "pole" => ObjectType::Pole,
        "tree" => ObjectType::Tree,
        "vegetation" => ObjectType::Vegetation,
        "barrier" => ObjectType::Barrier,
        "building" => ObjectType::Building,
        "parkingSpace" => ObjectType::ParkingSpace,
        "patch" => ObjectType::Patch,
        "railing" => ObjectType::Railing,
        "trafficIsland" => ObjectType::TrafficIsland,
        "crosswalk" => ObjectType::Crosswalk,
        "streetLamp" => ObjectType::StreetLamp,
        "gantry" => ObjectType::Gantry,
        "soundBarrier" => ObjectType::SoundBarrier,
        "roadMark" => ObjectType::RoadMark,
        "roadSurface" => ObjectType::RoadSurface,
        _ => ObjectType::Unknown,
    }
}

fn child<'a>(node: roxmltree::Node<'a, 'a>, tag: &str) -> Option<roxmltree::Node<'a, 'a>> {
    node.children().find(|n| n.has_tag_name(tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_is_measured_by_tags_alone() {
        let deep = |n| format!("{}{}", "<a>".repeat(n), "</a>".repeat(n));
        assert!(!deeper_than(&deep(3), 3));
        assert!(deeper_than(&deep(4), 3));
        let flat =
            r#"<r><a/><a x="1>2"/><!-- <a><a><a><a> --><![CDATA[<a><a><a><a>]]><?p <a><a>?></r>"#;
        assert!(!deeper_than(flat, 1));
        let doctype = r#"<!DOCTYPE r [<!ENTITY e "x">]><r><a><b/></a></r>"#;
        assert!(!deeper_than(doctype, 2));
        assert!(deeper_than(doctype, 1));
    }
    use crate::coords::Vector;

    // A straight 40 m road climbing at 4%, one right (forward) driving lane.
    const STRAIGHT: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="s" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <elevationProfile>
      <elevation s="0.0" a="0.0" b="0.04" c="0.0" d="0.0"/>
    </elevationProfile>
    <lanes>
      <laneSection s="0.0">
        <right>
          <lane id="-1" type="driving">
            <width sOffset="0.0" a="3.5" b="0.0" c="0.0" d="0.0"/>
          </lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn straight_one_forward_lane() {
        let net = load_str(STRAIGHT).expect("import");
        assert_eq!(net.driving_lanes().count(), 1);
        let lane = net.lanes().first().unwrap();
        assert_eq!(lane.direction, Direction::Forward);
        let start = lane.center.pose_at(0.0);
        let end = lane.center.pose_at(lane.center.length());
        // Heads +X, the right lane sits on the -Y side, and it climbs.
        assert!(start.heading.x > 0.9, "start heading {:?}", start.heading);
        assert!(
            (start.position.y + 1.75).abs() < 0.1,
            "y {}",
            start.position.y
        );
        assert!(end.position.z > start.position.z + 1.0, "no climb");
    }

    // A straight then a 90-degree left arc (radius 30), two opposing lanes,
    // the same shape as the hand-authored demo_road.
    const STRAIGHT_ARC: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sa" length="87.12" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
      <geometry s="40.0" x="40.0" y="0.0" hdg="0.0" length="47.12"><arc curvature="0.03333"/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <left>
          <lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
        </left>
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn straight_then_left_arc_two_lanes() {
        let net = load_str(STRAIGHT_ARC).expect("import");
        assert_eq!(net.driving_lanes().count(), 2);
        // Forward lane = the right (negative-id) lane.
        let fwd = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .unwrap();
        let start = fwd.center.pose_at(0.0);
        let end = fwd.center.pose_at(fwd.center.length());
        assert!(start.heading.x > 0.9, "start {:?}", start.heading);
        // After a 90-degree left turn, heading points toward +Y.
        assert!(end.heading.y > 0.9, "end {:?}", end.heading);
    }

    #[test]
    fn lanes_sit_a_width_apart() {
        let net = load_str(STRAIGHT_ARC).expect("import");
        let fwd = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .unwrap();
        let bwd = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .unwrap();
        // On the straight, the two lane centers are ~one lane width apart.
        let a = fwd.center.point_at(10.0);
        let gap = (a - bwd.center.project(a).point).length();
        assert!((gap - 3.5).abs() < 0.2, "gap {gap}");
    }

    // A straight road placed away from the origin and pointed along +Y, so
    // every axis carries a distinct number and a transposed or negated axis
    // cannot hide. One right lane, 4 m wide, at a constant 3 m elevation.
    const PLACED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="placed" length="30.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="100.0" y="50.0" hdg="1.5707963267948966" length="30.0"><line/></geometry>
    </planView>
    <elevationProfile>
      <elevation s="0.0" a="3.0" b="0.0" c="0.0" d="0.0"/>
    </elevationProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    // The baked frame is OpenDRIVE's own, so a point can be read straight off
    // the file. Heading is +Y, so left is -X and the right lane sits at +X.
    #[test]
    fn a_placed_road_bakes_to_its_opendrive_coordinates() {
        let net = load_str(PLACED).expect("import");
        let lane = &net.lanes()[0];

        // Reference line runs (100, 50) -> (100, 80); lane centre is t = -2,
        // which for heading +Y is 2 m toward +X. Elevation is a flat 3.
        for (s, want) in [
            (0.0, Point::new(102.0, 50.0, 3.0)),
            (15.0, Point::new(102.0, 65.0, 3.0)),
            (30.0, Point::new(102.0, 80.0, 3.0)),
        ] {
            let got = lane.center.point_at(s);
            assert!(
                (got - want).length() < 1e-3,
                "s={s}: baked {got:?}, want {want:?}"
            );
        }
        // Travel is along +Y and the surface is level.
        assert!(lane
            .center
            .pose_at(15.0)
            .heading
            .abs_diff_eq(Vector::Y, 1e-4));
        assert!(lane.sample_at(15.0).up.abs_diff_eq(Vector::Z, 1e-5));
    }

    // A straight road with a constant laneOffset of +2.0 (shifts the whole
    // cross-section left, toward +Y in the baked frame).
    const STRAIGHT_OFFSET: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="o" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>
      <laneOffset s="0.0" a="2.0" b="0.0" c="0.0" d="0.0"/>
      <laneSection s="0.0">
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn lane_offset_shifts_the_cross_section() {
        // Right lane without offset sits at y = -1.75; laneOffset +2.0 shifts
        // it left by 2.0 -> y = +0.25.
        let net = load_str(STRAIGHT_OFFSET).expect("import");
        let y = net.lanes()[0].center.pose_at(0.0).position.y;
        assert!((y - 0.25).abs() < 0.05, "y {y}");
    }

    // A driving lane outboard of a shoulder. The shoulder is a lane of its own,
    // and its width also pushes the driving lane out.
    const SHOULDER_INBOARD: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="s" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right>
          <lane id="-1" type="shoulder"><width sOffset="0.0" a="2.0"/></lane>
          <lane id="-2" type="driving"><width sOffset="0.0" a="3.0"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn a_non_driving_inner_lane_still_offsets_the_driving_lane() {
        // Lane -2 sits outboard of the 2.0 m shoulder: its center is at
        // t = -(2.0 + 3.0/2) = -3.5, so y = -3.5. Skipping the shoulder's width
        // would leave it at -1.5.
        let net = load_str(SHOULDER_INBOARD).expect("import");
        assert_eq!(net.lanes().len(), 2, "the shoulder is a lane too");
        let driving = net.driving_lanes().next().expect("the driving lane");
        let y = driving.center.pose_at(0.0).position.y;
        assert!((y + 3.5).abs() < 0.05, "y {y}, expected -3.5");
        assert!(
            driving.neighbors.is_empty(),
            "a shoulder is not a lane you change into: {:?}",
            driving.neighbors
        );
    }

    // A straight road split into two lane sections at s=25. Each section's
    // lanes should become their own polyline spanning only that section.
    const TWO_SECTIONS: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="ms" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
      <laneSection s="25.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn each_lane_section_becomes_its_own_lane() {
        let net = load_str(TWO_SECTIONS).expect("import");
        assert_eq!(net.driving_lanes().count(), 2, "one lane per section");
        let mut lens: Vec<f32> = net.lanes().iter().map(|l| l.center.length()).collect();
        lens.sort_by(|a, b| a.total_cmp(b));
        // Sections span [0,25] and [25,40] -> ~25 and ~15 m.
        assert!((lens[0] - 15.0).abs() < 1.0, "short section {}", lens[0]);
        assert!((lens[1] - 25.0).abs() < 1.0, "long section {}", lens[1]);
    }

    // A cross-section using most of the lane vocabulary: a sidewalk, a kerb, a
    // parking lane and an unnamed strip outboard of two running lanes, a median
    // between the two directions, a vendor-specific type, and a type that is
    // not in the format at all.
    const MANY_TYPES: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="m" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <left>
          <lane id="1" type="median"><width sOffset="0.0" a="1.0"/></lane>
          <lane id="2" type="special1"><width sOffset="0.0" a="1.0"/></lane>
          <lane id="3" type="driving"><width sOffset="0.0" a="3.5"/></lane>
          <lane id="4" type="banana"><width sOffset="0.0" a="1.5"/></lane>
        </left>
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
          <lane id="-2" type="onRamp"><width sOffset="0.0" a="3.0"/></lane>
          <lane id="-3" type="none"><width sOffset="0.0" a="0.5"/></lane>
          <lane id="-4" type="parking"><width sOffset="0.0" a="2.5"/></lane>
          <lane id="-5" type="curb"><width sOffset="0.0" a="0.3"/></lane>
          <lane id="-6" type="sidewalk"><width sOffset="0.0" a="2.0"/></lane>
          <lane id="-7" type="walking"><width sOffset="0.0" a="2.0"/></lane>
          <lane id="-8" type="shared"><width sOffset="0.0" a="3.0"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn every_named_lane_type_bakes_as_itself() {
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(MANY_TYPES).expect("import");
        let kind = |od_id: i32| {
            prov.iter()
                .find(|p| p.od_id == od_id)
                .and_then(|p| net.lane(p.lane))
                .map(|l| l.kind)
        };
        assert_eq!(kind(1), Some(LaneType::Median));
        assert_eq!(kind(3), Some(LaneType::Driving));
        assert_eq!(kind(-1), Some(LaneType::Driving));
        assert_eq!(kind(-2), Some(LaneType::OnRamp));
        assert_eq!(kind(-3), Some(LaneType::None), "`none` is paved surface");
        assert_eq!(kind(-4), Some(LaneType::Parking));
        assert_eq!(kind(-5), Some(LaneType::Curb));
        assert_eq!(kind(-6), Some(LaneType::Sidewalk));
        assert_eq!(
            kind(-7),
            Some(LaneType::Sidewalk),
            "1.8 calls a sidewalk `walking`"
        );
        assert_eq!(kind(-8), Some(LaneType::Shared));
        assert_eq!(kind(2), Some(LaneType::Special1));
        assert_eq!(kind(4), Some(LaneType::Unknown), "banana is not a type");
        // Every lane in the section, with nothing dropped for its type.
        assert_eq!(net.lanes().len(), 12);
        assert_eq!(net.driving_lanes().count(), 2);
    }

    // A gore area, as CARLA writes one: a lane that starts as a point and opens
    // out cubically, then holds a constant width. Town07's road 64 lane -5 and
    // road 17 lane -2 are both this shape.
    const TAPERED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="t" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
          <lane id="-2" type="none">
            <width sOffset="0.0" a="0.0" b="0.0" c="0.04" d="0.0"/>
            <width sOffset="10.0" a="4.0" b="0.0" c="0.0" d="0.0"/>
          </lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn a_lane_that_opens_out_of_nothing_keeps_its_width_along_its_length() {
        // The width runs 0 -> 4 m. Reporting the width at s=0 as the lane's
        // width made the whole strip 0 m wide.
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(TAPERED).expect("import");
        let lane = prov
            .iter()
            .find(|p| p.od_id == -2)
            .and_then(|p| net.lane(p.lane))
            .expect("the tapered lane");

        assert!((lane.width - 4.0).abs() < 0.01, "nominal {}", lane.width);
        assert!(
            lane.width_at(0.0) < 0.01,
            "it starts as a point: {}",
            lane.width_at(0.0)
        );
        assert!(
            (lane.width_at(lane.center.length()) - 4.0).abs() < 0.01,
            "and ends at full width: {}",
            lane.width_at(lane.center.length())
        );
        // The profile grows the whole way, following the cubic rather than
        // stepping between the two width records.
        let mut last = -1.0;
        for k in 0..=20 {
            let w = lane.width_at(lane.center.length() * k as f32 / 20.0);
            assert!(w >= last - 1e-4, "width dipped at step {k}: {last} -> {w}");
            last = w;
        }
    }

    #[test]
    fn a_tapered_lane_tessellates_to_a_wedge_not_a_sliver() {
        // What the viewer showed. The lane was in the lane list, it had a span
        // in the mesh, and it covered no area.
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(TAPERED).expect("import");
        let id = prov
            .iter()
            .find(|p| p.od_id == -2)
            .expect("provenance")
            .lane;
        let mesh = net.surface_mesh();
        let span = mesh
            .lanes
            .iter()
            .find(|s| s.lane == id)
            .expect("the lane owns a mesh slice");

        let ribs: Vec<f32> = mesh.vertices
            [span.vertices.start as usize..span.vertices.end as usize]
            .chunks_exact(2)
            .map(|p| (p[0] - p[1]).length())
            .collect();
        assert!(ribs.first().copied().unwrap_or(1.0) < 0.01, "starts sharp");
        assert!(
            ribs.last().copied().unwrap_or(0.0) > 3.9,
            "opens to full width: {:?}",
            ribs.last()
        );
    }

    #[test]
    fn an_unrecognised_lane_type_is_a_lane_rather_than_a_hole() {
        // An unrecognised type still describes a real strip of surface. It
        // bakes with its geometry intact, under a name that says the type was
        // not understood.
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(MANY_TYPES).expect("import");
        let lane = prov
            .iter()
            .find(|p| p.od_id == 4)
            .and_then(|p| net.lane(p.lane))
            .expect("the unrecognised lane is baked");
        assert_eq!(lane.kind, LaneType::Unknown);
        assert!(
            !lane.kind.is_drivable(),
            "and nothing may be routed onto it"
        );
        assert!((lane.width - 1.5).abs() < 1e-6, "width {}", lane.width);
        assert!(lane.center.length() > 19.0, "it has the road's length");
    }

    #[test]
    fn inner_lane_widths_accumulate_across_the_cross_section() {
        // Left side, center outward: median 1.0, special1 1.0, driving 3.5. The
        // driving lane's center is at t = +(1.0 + 1.0 + 3.5/2) = +3.75. Missing
        // either inner width would leave it at +2.75 or nearer.
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(MANY_TYPES).expect("import");
        let driving = prov
            .iter()
            .find(|p| p.od_id == 3)
            .and_then(|p| net.lane(p.lane))
            .expect("the left driving lane");
        let y = driving.center.pose_at(0.0).position.y;
        assert!((y - 3.75).abs() < 0.05, "y {y}, expected 3.75");
    }

    #[test]
    fn lane_change_edges_stop_at_the_first_lane_traffic_cannot_use() {
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(MANY_TYPES).expect("import");
        let lane = |od_id: i32| {
            prov.iter()
                .find(|p| p.od_id == od_id)
                .and_then(|p| net.lane(p.lane))
                .expect("a baked lane")
        };
        // Right side: driving -1 and the onRamp -2 beside it are a lane change
        // apart; the parking lane past them is not, and neither is the sidewalk.
        let (driving, ramp) = (lane(-1), lane(-2));
        assert_eq!(driving.neighbors, vec![ramp.id]);
        assert_eq!(ramp.neighbors, vec![driving.id]);
        assert!(
            lane(-4).neighbors.is_empty(),
            "parking is not a lane change"
        );
        assert!(lane(-6).neighbors.is_empty(), "sidewalk is not either");
        assert!(lane(-3).neighbors.is_empty(), "nor is unnamed surface");
        // Left side: nothing drivable sits beside the only driving lane, so it
        // has nothing to change into at all.
        assert!(lane(3).neighbors.is_empty(), "a median is not crossable");
    }

    #[test]
    fn provenance_names_each_baked_lane_by_road_section_and_od_id() {
        let (net, Provenance { lanes: prov, .. }) =
            load_str_with_provenance(TWO_SECTIONS).expect("import with provenance");
        assert_eq!(prov.len(), net.lanes().len(), "one record per baked lane");
        // Every record points at a real lane, and names the source road/lane.
        for p in &prov {
            assert!(net.lane(p.lane).is_some(), "{:?} names no lane", p.lane);
            assert_eq!(p.road_id, "1");
            assert_eq!(p.od_id, -1);
        }
        // The two sections are distinguished, 0 and 1.
        let mut sections: Vec<usize> = prov.iter().map(|p| p.section).collect();
        sections.sort_unstable();
        assert_eq!(sections, vec![0, 1], "one lane per section, indexed 0,1");
    }

    // A pure clothoid: curvStart 0, curvEnd 0.1 over 10 m. End heading is the
    // closed form 0.5*c_dot*L^2 = 0.5*(0.01)*100 = 0.5 rad (a left turn -> +Y).
    const SPIRAL_ONLY: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sp" length="10.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="10.0">
        <spiral curvStart="0.0" curvEnd="0.1"/>
      </geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn spiral_heading_matches_closed_form() {
        // Sampled at s=5 (interior, where the polyline's interpolated tangent is
        // accurate, since the very endpoint tangent is a last-segment artifact).
        // Reference heading there = 0.5*c_dot*s^2 = 0.5*0.01*25 = 0.125 rad.
        let net = load_str(SPIRAL_ONLY).expect("import");
        let h = net.lanes()[0].center.pose_at(5.0).heading;
        let theta = h.y.atan2(h.x);
        assert!((theta - 0.125).abs() < 0.03, "mid heading angle {theta}");
    }

    // A normalized paramPoly3: u(p)=10p, v(p)=5p^2 for p in [0,1], so the curve
    // runs from OD (0,0) to OD (10,5), which is the baked frame unchanged, so
    // the end lands near y=+5. The road is longer than the curve (arc length
    // ~11.5), so the end clamps to that point. A straight line (v ignored)
    // would end at y=0.
    const PARAM_POLY3: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="pp" length="15.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="15.0">
        <paramPoly3 pRange="normalized" aU="0" bU="10" cU="0" dU="0" aV="0" bV="0" cV="5" dV="0"/>
      </geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn param_poly3_follows_the_v_deviation() {
        let net = load_str(PARAM_POLY3).expect("import");
        let end = net.lanes()[0]
            .center
            .pose_at(net.lanes()[0].center.length())
            .position;
        assert!(end.x > 8.0, "end x {}", end.x);
        assert!(end.y > 3.0, "end y {} (should follow v to ~+5)", end.y);
    }

    // poly3 with v(u)=0.05*u^2 over length 10, curving laterally toward +y
    // (like the arcLength paramPoly3 case; exact endpoint depends on the
    // arc-length reparametrization, so just assert a clear deviation).
    const POLY3: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="p3" length="10.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="10.0">
        <poly3 a="0" b="0" c="0.05" d="0"/>
      </geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn poly3_curves_laterally() {
        let net = load_str(POLY3).expect("import");
        let end = net.lanes()[0]
            .center
            .pose_at(net.lanes()[0].center.length())
            .position;
        assert!(end.y > 2.0, "end y {} (poly3 should curve)", end.y);
    }

    // Two lane sections in one road, linked lane -1 -> lane -1.
    const TWO_SECTION_LINKED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="r" length="40.0" id="1" junction="-1">
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry></planView>
    <lanes>
      <laneSection s="0.0"><right><lane id="-1" type="driving">
        <link><successor id="-1"/></link><width sOffset="0.0" a="3.5"/>
      </lane></right></laneSection>
      <laneSection s="15.0"><right><lane id="-1" type="driving">
        <link><predecessor id="-1"/></link><width sOffset="0.0" a="3.5"/>
      </lane></right></laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn cross_section_link() {
        let net = load_str(TWO_SECTION_LINKED).expect("import");
        assert_eq!(net.lanes().len(), 2);
        assert_eq!(net.lanes()[0].successors, vec![net.lanes()[1].id]);
        assert_eq!(net.lanes()[1].predecessors, vec![net.lanes()[0].id]);
    }

    // Road 1 -> road 2 (its successor), each a single forward lane.
    const TWO_ROADS_LINKED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="a" length="20.0" id="1" junction="-1">
    <link><successor elementType="road" elementId="2" contactPoint="start"/></link>
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <link><successor id="-1"/></link><width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>
  <road name="b" length="20.0" id="2" junction="-1">
    <link><predecessor elementType="road" elementId="1" contactPoint="end"/></link>
    <planView><geometry s="0.0" x="20.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <link><predecessor id="-1"/></link><width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn road_to_road_link() {
        let net = load_str(TWO_ROADS_LINKED).expect("import");
        assert_eq!(net.lanes().len(), 2);
        assert_eq!(net.lanes()[0].successors, vec![net.lanes()[1].id], "A -> B");
        assert_eq!(net.lanes()[1].predecessors, vec![net.lanes()[0].id]);
    }

    // Road 1 -> junction 100 -> connecting road 2, via a laneLink.
    const JUNCTION: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="in" length="20.0" id="1" junction="-1">
    <link><successor elementType="junction" elementId="100"/></link>
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <link><successor id="-1"/></link><width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>
  <road name="conn" length="20.0" id="2" junction="100">
    <link><predecessor elementType="road" elementId="1" contactPoint="end"/></link>
    <planView><geometry s="0.0" x="20.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>
  <junction id="100">
    <connection id="0" incomingRoad="1" connectingRoad="2" contactPoint="start">
      <laneLink from="-1" to="-1"/>
    </connection>
  </junction>
</OpenDRIVE>"#;

    #[test]
    fn junction_link() {
        let net = load_str(JUNCTION).expect("import");
        assert_eq!(net.lanes().len(), 2);
        assert_eq!(
            net.lanes()[0].successors,
            vec![net.lanes()[1].id],
            "road -> junction -> connecting road"
        );
    }

    #[test]
    fn empty_or_junk_is_an_error() {
        assert!(load_str("<OpenDRIVE></OpenDRIVE>").is_err());
        assert!(load_str("not xml at all <<<").is_err());
    }

    // A flat straight (no lateralProfile) must leave the bank profile empty,
    // the "flat lane" sentinel, so imported flat roads are unchanged.
    #[test]
    fn no_lateral_profile_leaves_bank_empty() {
        let net = load_str(STRAIGHT).expect("import");
        assert!(
            net.lanes()[0].bank.is_empty(),
            "a flat road carries no bank"
        );
        assert_eq!(net.lanes()[0].bank_at(10.0), 0.0);
    }

    // A straight, level road banked at a constant 0.1 rad, one lane each side.
    // Reference-line pivot: the left (+t) lane rides up by ~t·sin φ, the right
    // (−t) lane drops by the same, and both read the same bank angle φ.
    const SUPERELEV_CONST: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="se" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn constant_superelevation_pivots_about_the_reference_line() {
        let net = load_str(SUPERELEV_CONST).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");
        let right = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .expect("a right lane");

        // Both lanes read the surface roll angle, ~0.1 rad, everywhere.
        assert!(
            (left.bank_at(10.0) - 0.1).abs() < 1e-4,
            "{}",
            left.bank_at(10.0)
        );
        assert!(
            (right.bank_at(10.0) - 0.1).abs() < 1e-4,
            "{}",
            right.bank_at(10.0)
        );
        assert!(!left.bank.is_empty(), "a banked lane carries a profile");

        // Lane centers sit half a lane-width off the reference (t = ±1.75), so
        // the pivot raises the left by 1.75·sin0.1 and drops the right likewise.
        let expect = 1.75 * 0.1_f32.sin();
        let ly = left.center.point_at(10.0).z;
        let ry = right.center.point_at(10.0).z;
        assert!((ly - expect).abs() < 0.02, "left z {ly}, want {expect}");
        assert!((ry + expect).abs() < 0.02, "right z {ry}, want {}", -expect);
        // Positive φ raises the left edge: left above right.
        assert!(ly > ry, "left {ly} should ride above right {ry}");

        // Horizontal offset shrinks by cos φ (the lane leans in, not straight
        // out): |y| a touch under 1.75.
        let ly = left.center.point_at(10.0).y.abs();
        assert!(ly < 1.75 && ly > 1.75 * 0.1_f32.cos() - 0.02, "y {ly}");
    }

    // Superelevation ramping in along s: φ(s) = 0.01·s, so the bank grows.
    const SUPERELEV_RAMP: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="ser" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.0" b="0.01" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn ramped_superelevation_grows_along_s() {
        let net = load_str(SUPERELEV_RAMP).expect("import");
        let lane = &net.lanes()[0];
        // φ(5)=0.05, φ(15)=0.15, a monotonic increase matching the cubic.
        assert!(
            (lane.bank_at(5.0) - 0.05).abs() < 5e-3,
            "{}",
            lane.bank_at(5.0)
        );
        assert!(
            (lane.bank_at(15.0) - 0.15).abs() < 5e-3,
            "{}",
            lane.bank_at(15.0)
        );
        assert!(lane.bank_at(15.0) > lane.bank_at(5.0));
    }

    // Two left lanes (id 1 inner, id 2 outer) on a road banked +0.1 rad. The
    // headline reference-line-pivot case: the outer lane, further from the
    // reference line, rides measurably higher than the inner one.
    const SUPERELEV_TWO_LEFT: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="se2" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left>
          <lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane>
          <lane id="2" type="driving"><width sOffset="0.0" a="3.5"/></lane>
        </left>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn outer_lane_of_a_banked_road_rides_higher() {
        let net = load_str(SUPERELEV_TWO_LEFT).expect("import");
        // Inner lane center t = 1.75, outer t = 5.25 (one full width further out).
        // Both climb by t·sin0.1; the outer sits ~3.5·sin0.1 ≈ 0.35 m above.
        let mut zs: Vec<f32> = net
            .lanes()
            .iter()
            .map(|l| l.center.point_at(10.0).z)
            .collect();
        zs.sort_by(|a, b| a.total_cmp(b));
        let inner_z = 1.75 * 0.1_f32.sin();
        let outer_z = 5.25 * 0.1_f32.sin();
        assert!((zs[0] - inner_z).abs() < 0.02, "inner z {}", zs[0]);
        assert!((zs[1] - outer_z).abs() < 0.02, "outer z {}", zs[1]);
        assert!(
            zs[1] - zs[0] > 0.3,
            "outer lane {} should ride well above inner {}",
            zs[1],
            zs[0]
        );
    }

    // Negative superelevation rolls the other way: the right (−t) edge lifts and
    // the left (+t) edge drops, the mirror of the positive case, pinning sign.
    const SUPERELEV_NEG: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sen" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="-0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn negative_superelevation_raises_the_right_edge() {
        let net = load_str(SUPERELEV_NEG).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");
        let right = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .expect("a right lane");
        // φ = −0.1: the right lane now rides above the left (mirror of +φ).
        assert!(
            right.center.point_at(10.0).z > left.center.point_at(10.0).z,
            "right {} should ride above left {} for negative bank",
            right.center.point_at(10.0).z,
            left.center.point_at(10.0).z
        );
        // The stored angle carries the sign.
        assert!(
            (left.bank_at(10.0) + 0.1).abs() < 1e-4,
            "{}",
            left.bank_at(10.0)
        );
    }

    // ===================================================================
    // Stress the reference-line pivot beyond the straight, level,
    // single-offset cases above.
    // ===================================================================

    // A straight-then-90-degree-left-arc road (the STRAIGHT_ARC shape) with a
    // constant 0.15 rad superelevation. Bank must bake finite, sane geometry all
    // the way around the curve and read the surface roll at any station.
    const SUPERELEV_ARC: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sa" length="87.12" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
      <geometry s="40.0" x="40.0" y="0.0" hdg="0.0" length="47.12"><arc curvature="0.03333"/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.15" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn superelevation_on_a_banked_arc() {
        let net = load_str(SUPERELEV_ARC).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");
        let right = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .expect("a right lane");

        // Every baked centerline point is finite (no NaN/inf from the arc math).
        for lane in [left, right] {
            assert!(!lane.bank.is_empty(), "banked lane carries a profile");
            for &b in &lane.bank {
                assert!(b.is_finite(), "bank entry not finite: {b}");
            }
            for p in lane.center.points() {
                assert!(p.is_finite(), "centerline point not finite: {p:?}");
            }
        }

        // Read the roll at several stations, including well into the arc
        // (s=20 straight, s=60 arc, near the very end). Constant profile => 0.15
        // everywhere, on both lanes.
        for &s in &[0.0_f32, 20.0, 60.0, 85.0] {
            assert!(
                (left.bank_at(s) - 0.15).abs() < 1e-3,
                "left bank_at({s}) = {}",
                left.bank_at(s)
            );
            assert!(
                (right.bank_at(s) - 0.15).abs() < 1e-3,
                "right bank_at({s}) = {}",
                right.bank_at(s)
            );
        }

        // Reference-line pivot: with constant t (=+/-1.75) and constant phi, the
        // banked height is constant along the whole road, arc included. The left
        // (+t) lane rides up by 1.75*sin0.15, the right (-t) down by the same.
        let expect = 1.75 * 0.15_f32.sin();
        for &s in &[20.0_f32, 60.0, 85.0] {
            let ly = left.center.point_at(s).z;
            let ry = right.center.point_at(s).z;
            assert!(
                (ly - expect).abs() < 0.05,
                "left z@{s} = {ly}, want {expect}"
            );
            assert!((ry + expect).abs() < 0.05, "right z@{s} = {ry}");
            assert!(ly > ry, "left {ly} should ride above right {ry} @ s={s}");
        }
    }

    // End-to-end: importing a banked `.xodr` and calling `sample_near` on the
    // imported network yields a sane RoadSample on both the straight and the
    // arc portion of the banked road: correct bank, and a unit up-normal that
    // points up and carries lateral cant only (up.heading == 0).
    #[test]
    fn sample_near_on_an_imported_banked_road_is_sane() {
        let net = load_str(SUPERELEV_ARC).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");

        // Query at points sitting on the left lane's own centerline: s=20 (the
        // straight) and s=60 (well into the arc). sample_near must land back on
        // that lane and report its ~0.15 rad bank.
        for &s in &[20.0_f32, 60.0] {
            let on_lane = left.center.point_at(s);
            let rs = net.sample_near(on_lane).expect("a sample near the road");
            assert!(rs.point.is_finite(), "sample point not finite @ s={s}");
            assert!(
                (rs.bank.abs() - 0.15).abs() < 1e-2,
                "bank @ s={s} = {} (want |0.15|)",
                rs.bank
            );
            // Unit up-normal, pointing up, lateral cant only.
            assert!(
                (rs.up.length() - 1.0).abs() < 1e-4,
                "up not unit @ s={s}: {:?}",
                rs.up
            );
            assert!(rs.up.z > 0.9, "up.z too low @ s={s}: {}", rs.up.z);
            assert!(
                rs.up.dot(rs.heading).abs() < 1e-5,
                "up.heading @ s={s} = {}",
                rs.up.dot(rs.heading)
            );
            // The sample landed close to where we queried (same lane).
            assert!(
                (rs.point - on_lane).length() < 0.5,
                "sample drifted from the query @ s={s}: {:?} vs {on_lane:?}",
                rs.point
            );
        }
    }

    // Superelevation composed with an elevation grade: the road climbs at 4% AND
    // banks at 0.1 rad. A lane's height must be grade(s) + t*sin(phi). The two
    // add, and neither clobbers the other.
    const SUPERELEV_PLUS_GRADE: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sg" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <elevationProfile>
      <elevation s="0.0" a="0.0" b="0.04" c="0.0" d="0.0"/>
    </elevationProfile>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3.5"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn superelevation_composes_with_elevation_grade() {
        let net = load_str(SUPERELEV_PLUS_GRADE).expect("import");
        let left = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Backward)
            .expect("a left lane");
        let right = net
            .lanes()
            .iter()
            .find(|l| l.direction == Direction::Forward)
            .expect("a right lane");
        let cant = 1.75 * 0.1_f32.sin();
        for &s in &[10.0_f32, 20.0, 30.0] {
            let grade = 0.04 * s; // elevation cubic: a=0, b=0.04
                                  // Left (+t) rides above the grade line, right (-t) below it, by the
                                  // same cant, so the grade is the midline of the two.
            let ly = left.center.point_at(s).z;
            let ry = right.center.point_at(s).z;
            assert!(
                (ly - (grade + cant)).abs() < 0.02,
                "left z@{s} = {ly}, want {}",
                grade + cant
            );
            assert!(
                (ry - (grade - cant)).abs() < 0.02,
                "right z@{s} = {ry}, want {}",
                grade - cant
            );
            // The mean of the two lanes recovers the grade (bank cancels).
            assert!(((ly + ry) / 2.0 - grade).abs() < 0.02, "grade midline @{s}");
        }
    }

    // Superelevation + laneOffset: the +2.0 laneOffset shifts the whole
    // cross-section, and the pivot must apply to the *shifted* t. Right lane -1
    // (own offset -1.75) with laneOffset +2.0 lands at t = +0.25, so its height
    // is the small POSITIVE 0.25*sin(phi), not -1.75*sin(phi) (own offset only)
    // nor +2.0*sin(phi) (laneOffset only).
    const SUPERELEV_PLUS_OFFSET: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="so" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneOffset s="0.0" a="2.0" b="0.0" c="0.0" d="0.0"/>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn superelevation_pivots_about_the_offset_shifted_cross_section() {
        let net = load_str(SUPERELEV_PLUS_OFFSET).expect("import");
        let lane = &net.lanes()[0];
        let t = 2.0 - 1.75; // laneOffset + own (right) offset = +0.25
        let want_z = t * 0.1_f32.sin();
        let z = lane.center.point_at(10.0).z;
        assert!(
            (z - want_z).abs() < 5e-3,
            "z = {z}, want {want_z} (pivot on shifted t=+0.25, not own -1.75 nor offset +2.0)"
        );
        // Height is clearly positive: had the pivot used the lane's own -1.75,
        // it would be negative (~ -0.175).
        assert!(
            z > 0.0,
            "shifted t is +0.25 -> height must be positive, got {z}"
        );
        // Horizontal reach shrinks by cos(phi): |y| = t*cos0.1 ~= 0.2487.
        let y = lane.center.point_at(10.0).y;
        assert!(
            (y - t * 0.1_f32.cos()).abs() < 5e-3,
            "y = {y}, want {}",
            t * 0.1_f32.cos()
        );
    }

    // Importing the same banked map twice must yield byte-identical bank vectors
    // and centerline points (no ordering / float nondeterminism).
    #[test]
    fn banked_import_is_deterministic() {
        let a = load_str(SUPERELEV_ARC).expect("import a");
        let b = load_str(SUPERELEV_ARC).expect("import b");
        assert_eq!(a.lanes().len(), b.lanes().len());
        for (la, lb) in a.lanes().iter().zip(b.lanes().iter()) {
            assert_eq!(la.bank, lb.bank, "bank vectors differ between imports");
            assert_eq!(
                la.center.points(),
                lb.center.points(),
                "centerline points differ between imports"
            );
        }
    }

    // A very large roll near pi/2: sin ~= 1 (height ~= t), cos ~= 0 (horizontal
    // reach collapses). Must stay finite and read back the angle, with no blow-up.
    const SUPERELEV_STEEP: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="st" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="1.5" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn steep_superelevation_stays_finite() {
        let net = load_str(SUPERELEV_STEEP).expect("import");
        let lane = &net.lanes()[0];
        assert!(
            (lane.bank_at(10.0) - 1.5).abs() < 1e-3,
            "{}",
            lane.bank_at(10.0)
        );
        let p = lane.center.point_at(10.0);
        assert!(p.is_finite(), "point not finite at steep bank: {p:?}");
        // t = -1.75; height ~= -1.75*sin1.5 ~= -1.746, |y| ~= 1.75*cos1.5 ~= 0.124.
        assert!((p.z - (-1.75 * 1.5_f32.sin())).abs() < 0.02, "z {}", p.z);
        assert!(
            p.y.abs() < 0.2,
            "horizontal reach should collapse, y {}",
            p.y
        );
    }

    // An explicit zero superelevation (a="0.0") must still collapse to the empty
    // "flat lane" sentinel, exactly like no lateralProfile at all.
    const SUPERELEV_EXPLICIT_ZERO: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sz" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.0" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn explicit_zero_superelevation_collapses_to_empty() {
        let net = load_str(SUPERELEV_EXPLICIT_ZERO).expect("import");
        assert!(
            net.lanes()[0].bank.is_empty(),
            "an all-zero profile must collapse to the flat sentinel"
        );
        assert_eq!(net.lanes()[0].bank_at(10.0), 0.0);
        // And its centerline height is pure horizontal (y = -1.75, z = 0).
        let p = net.lanes()[0].center.point_at(10.0);
        assert!(p.z.abs() < 1e-4, "flat road, z {}", p.z);
    }

    // A superelevation record that starts at s=10 on a 20 m road: stations before
    // s=10 fall back to 0 (no active record), stations after read the profile.
    const SUPERELEV_LATE_START: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <road name="sl" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="10.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

    #[test]
    fn superelevation_starting_late_leaves_early_stations_flat() {
        let net = load_str(SUPERELEV_LATE_START).expect("import");
        let lane = &net.lanes()[0];
        // Profile is kept (later stations are banked), not collapsed.
        assert!(!lane.bank.is_empty());
        // Before the record: flat.
        assert!(
            lane.bank_at(2.0).abs() < 1e-4,
            "early bank {}",
            lane.bank_at(2.0)
        );
        assert!(
            lane.center.point_at(2.0).z.abs() < 1e-3,
            "early height should be flat, z {}",
            lane.center.point_at(2.0).z
        );
        // After the record starts: banked ~0.1.
        assert!(
            (lane.bank_at(18.0) - 0.1).abs() < 1e-3,
            "late bank {}",
            lane.bank_at(18.0)
        );
        assert!(
            lane.center.point_at(18.0).z < -0.1,
            "late height should be banked (t=-1.75), z {}",
            lane.center.point_at(18.0).z
        );
    }

    /// A straight 20 m road along +X, banked a constant 0.1 rad, carrying
    /// `objects` as its `<objects>` children.
    fn banked_road_with(objects: &str) -> String {
        format!(
            r#"<OpenDRIVE>
  <road name="o" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="3.5"/></lane></right>
      </laneSection>
    </lanes>
    <objects>{objects}</objects>
  </road>
</OpenDRIVE>"#
        )
    }

    /// A road of two lane sections, split at s = 10, each with lanes 1, -1
    /// and -2.
    fn two_section_road_with(objects: &str) -> String {
        let section = |s| {
            format!(
                r#"<laneSection s="{s}">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="3"/></lane></left>
        <right>
          <lane id="-1" type="driving"><width sOffset="0.0" a="3"/></lane>
          <lane id="-2" type="sidewalk"><width sOffset="0.0" a="2"/></lane>
        </right>
      </laneSection>"#
            )
        };
        format!(
            r#"<OpenDRIVE>
  <road name="o" length="20.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry>
    </planView>
    <lanes>{}{}</lanes>
    <objects>{objects}</objects>
  </road>
</OpenDRIVE>"#,
            section("0.0"),
            section("10.0")
        )
    }

    /// The `(section, <lane id>)` of each lane each object applies to.
    fn object_lanes(xml: &str) -> Vec<Vec<(usize, i32)>> {
        let (net, prov) = load_str_with_provenance(xml).expect("import");
        net.objects()
            .iter()
            .map(|o| {
                o.lanes
                    .iter()
                    .map(|id| {
                        let p = prov.lanes.iter().find(|p| p.lane == *id).unwrap();
                        (p.section, p.od_id)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn an_object_applies_to_the_lanes_of_the_sections_it_spans() {
        let xml = two_section_road_with(
            r#"<object id="a" s="5" t="0"/>
               <object id="b" s="10" t="0">
                 <validity fromLane="-2" toLane="-1"/>
               </object>
               <object id="c" s="0" t="0">
                 <repeat s="2" length="16" distance="0" height="1"/>
                 <validity fromLane="1" toLane="1"/>
               </object>
               <object id="d" s="20" t="0">
                 <validity fromLane="1" toLane="1"/>
                 <validity fromLane="-2" toLane="-2"/>
               </object>"#,
        );
        assert_eq!(
            object_lanes(&xml),
            [
                // No validity: every lane of the section it stands in.
                vec![(0, 1), (0, -1), (0, -2)],
                // On the boundary, it is in the section that starts there.
                // The range runs either way round.
                vec![(1, -1), (1, -2)],
                // A sweep across the boundary is in both.
                vec![(0, 1), (1, 1)],
                // Each <validity> adds its range. At the very end of the road
                // it is in the last section.
                vec![(1, 1), (1, -2)],
            ]
        );
    }

    #[test]
    fn an_object_on_a_road_without_lanes_applies_to_none() {
        let xml = r#"<OpenDRIVE>
  <road length="20.0" id="1">
    <planView><geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>
    <objects><object id="a" s="5" t="0"/></objects>
  </road>
  <road length="20.0" id="2">
    <planView><geometry s="0" x="0" y="9" hdg="0" length="20"><line/></geometry></planView>
    <lanes><laneSection s="0"><right><lane id="-1" type="driving"><width sOffset="0" a="3"/></lane></right></laneSection></lanes>
  </road>
</OpenDRIVE>"#;
        let net = load_str(xml).expect("import");
        assert_eq!(net.objects().len(), 1);
        assert!(net.objects()[0].lanes.is_empty());
    }

    #[test]
    fn a_marking_goes_to_the_outline_holding_its_corners() {
        let square = |id: &str| {
            let corners: String = [(0, 0), (1, 0), (1, 1), (0, 1)]
                .iter()
                .enumerate()
                .map(|(k, (u, v))| {
                    format!(r#"<cornerLocal u="{u}" v="{v}" height="0" id="{id}{k}"/>"#)
                })
                .collect();
            format!("<outline>{corners}</outline>")
        };
        let marking = |refs: &[&str], width: &str| {
            let refs: String = refs
                .iter()
                .map(|r| format!(r#"<cornerReference id="{r}"/>"#))
                .collect();
            format!(r#"<marking width="{width}">{refs}</marking>"#)
        };
        let xml = banked_road_with(&format!(
            r#"<object id="o" s="5" t="0"><outlines>{}{}</outlines><markings>{}{}{}{}{}{}</markings></object>"#,
            square("a"),
            square("b"),
            marking(&["a0", "a1"], "0.1"),
            marking(&["b1", "b2", "b3"], "0.1"),
            // Corners of two outlines, a corner of none, a single corner,
            // and no width.
            marking(&["a0", "b1"], "0.1"),
            marking(&["a0", "z9"], "0.1"),
            marking(&["a0"], "0.1"),
            marking(&["a0", "a1"], "0"),
        ));
        let objects = objects_of(&xml);
        let pieces = |o: &Object| {
            o.markings
                .iter()
                .map(|m| m.pieces.len())
                .collect::<Vec<_>>()
        };
        assert_eq!(pieces(&objects[0]), [1]);
        assert_eq!(pieces(&objects[1]), [2]);
    }

    #[test]
    fn a_band_lies_in_a_banked_road_rather_than_level() {
        // The road banks 0.1 rad about +X, so its surface is z = y tan 0.1.
        // A border lies in it, and paint 5 mm off it along its normal. Objects
        // b and c follow the road, so their own frames lie in it too.
        let xml = banked_road_with(
            r#"<object id="a" s="5" t="-1">
                 <outlines><outline id="0">
                   <cornerRoad id="1" s="2" t="-1" dz="0"/><cornerRoad id="2" s="8" t="-1" dz="0"/>
                   <cornerRoad id="3" s="8" t="-3" dz="0"/></outline></outlines>
                 <markings><marking width="0.4"><cornerReference id="1"/><cornerReference id="2"/></marking></markings>
                 <borders><border width="1" useCompleteOutline="true"/></borders>
               </object>
               <object id="b" s="12" t="-2" hdg="0.5" perpToRoad="true">
                 <outlines><outline><cornerLocal u="0" v="0"/><cornerLocal u="3" v="0"/>
                   <cornerLocal u="3" v="2"/></outline></outlines>
                 <borders><border width="1" useCompleteOutline="true"/></borders>
               </object>
               <object id="c" s="16" t="-2" length="2" width="1" height="1" perpToRoad="true">
                 <markings><marking side="left" width="0.4"/></markings>
               </object>"#,
        );
        let tan = 0.1_f32.tan();
        let off = |p: Point| p.z - p.y * tan;
        let objects = objects_of(&xml);
        let mut bands = 0;
        for object in &objects {
            for border in &object.borders {
                for p in border.pieces.iter().flatten() {
                    assert!(off(*p).abs() < 1e-5, "{} border {p:?}", object.id.0);
                    bands += 1;
                }
            }
            for marking in &object.markings {
                for p in marking.pieces.iter().flatten() {
                    let want = 0.005 / 0.1_f32.cos();
                    assert!((off(*p) - want).abs() < 1e-5, "{} paint {p:?}", object.id.0);
                    bands += 1;
                }
            }
        }
        assert_eq!(bands, 4 * (3 + 1 + 3 + 1));
    }

    #[test]
    fn a_border_goes_to_the_outline_it_names() {
        let square = |id: &str| {
            format!(
                r#"<outline id="{id}"><cornerLocal u="0" v="0"/><cornerLocal u="1" v="0"/><cornerLocal u="1" v="1"/></outline>"#
            )
        };
        let border =
            |outline: &str| format!(r#"<border width="0.2" {outline} useCompleteOutline="true"/>"#);
        let xml = banked_road_with(&format!(
            r#"<object id="o" s="5" t="0"><outlines>{}{}</outlines><borders>{}{}{}</borders></object>"#,
            square("a"),
            square("b"),
            border(r#"outlineId="b""#),
            border(""),
            border(r#"outlineId="z""#),
        ));
        let objects = objects_of(&xml);
        let pieces = |o: &Object| o.borders.iter().map(|b| b.pieces.len()).collect::<Vec<_>>();
        // A closed triangle has three edges. The border naming no outline is
        // on both, and the one naming an outline that is not there on none.
        assert_eq!(pieces(&objects[0]), [3]);
        assert_eq!(pieces(&objects[1]), [3, 3]);
    }

    fn ends(pieces: &[[Point; 4]]) -> Vec<(f32, f32)> {
        pieces
            .iter()
            .map(|[a, b, c, d]| (a.lerp(*d, 0.5).x, b.lerp(*c, 0.5).x))
            .collect()
    }

    /// `points` on level ground.
    fn level(points: &[Point]) -> Vec<(Point, Vector)> {
        points.iter().map(|&p| (p, Vector::Z)).collect()
    }

    #[test]
    fn a_strip_steps_over_an_edge_with_no_length() {
        let (a, b, c) = (
            Point::ORIGIN,
            Point::new(2.0, 0.0, 0.0),
            Point::new(4.0, 0.0, 0.0),
        );
        // A repeated corner, and a corner straight above the one before:
        // no length in plan, so no across to measure.
        let up = Point::new(2.0, 0.0, 1.0);
        assert_eq!(
            ends(&strip(&level(&[a, a, b]), 0.0, 0.0, None, 0.1).unwrap()),
            [(0.0, 2.0)]
        );
        assert_eq!(
            ends(&strip(&level(&[a, b, up]), 0.0, 0.0, None, 0.1).unwrap()),
            [(0.0, 2.0)]
        );
        // A dash across the repeated corner carries on past it.
        let dashes = strip(&level(&[a, b, b, c]), 1.0, 0.0, Some((2.0, 1.0)), 0.1).unwrap();
        assert_eq!(ends(&dashes), [(1.0, 2.0), (2.0, 3.0)]);
    }

    #[test]
    fn a_strip_of_too_many_dashes_is_refused() {
        let path = level(&[Point::ORIGIN, Point::new(10.0, 0.0, 0.0)]);
        assert!(strip(&path, 0.0, 0.0, Some((1e-5, 1e-5)), 0.1).is_none());
        assert_eq!(
            strip(&path, 0.0, 0.0, Some((0.5, 0.5)), 0.1).unwrap().len(),
            10
        );
    }

    #[test]
    fn offsets_past_the_end_of_a_strip_leave_nothing() {
        let path = level(&[Point::ORIGIN, Point::new(10.0, 0.0, 0.0)]);
        for (start, stop) in [(20.0, 0.0), (0.0, 20.0), (6.0, 6.0)] {
            for dashes in [None, Some((1.0, 1.0))] {
                let pieces = strip(&path, start, stop, dashes, 0.1).unwrap();
                assert!(pieces.is_empty(), "{start}, {stop}, {dashes:?}: {pieces:?}");
            }
        }
        assert_eq!(
            ends(&strip(&path, 4.0, 4.0, None, 0.1).unwrap()),
            [(4.0, 6.0)]
        );
    }

    #[test]
    fn a_side_marking_paints_every_solid_with_a_box() {
        let markings = r#"<markings>
            <marking side="front" width="0.1"/>
            <marking side="top" width="0.1"/>
            <marking side="left" width="0.1"><cornerReference id="0"/><cornerReference id="1"/></marking>
        </markings>"#;
        let xml = banked_road_with(&format!(
            r#"<object id="a" s="5" t="0" length="2" width="1">
                 <repeat s="5" length="4" distance="2"/>{markings}</object>
               <object id="b" s="5" t="0" radius="1">{markings}</object>
               <object id="c" s="5" t="0">{markings}</object>"#
        ));
        // Three repeats and a cylinder, each with its front only. A side
        // the spec does not name, a marking with corners, and an object
        // with no extent get none.
        let counts: Vec<usize> = objects_of(&xml)
            .iter()
            .map(|o| o.markings.iter().map(|m| m.pieces.len()).sum())
            .collect();
        assert_eq!(counts, [1, 1, 1, 1, 0]);
    }

    #[test]
    fn a_hole_goes_to_the_closed_outline_round_it() {
        let square = |attrs: &str, (u, v): (f64, f64), size: f64| {
            let corners: String = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
                .map(|(du, dv)| {
                    let (u, v) = (u + du * size, v + dv * size);
                    format!(r#"<cornerLocal u="{u}" v="{v}"/>"#)
                })
                .concat();
            format!("<outline {attrs}>{corners}</outline>")
        };
        let hole = |at| square(r#"outer="false""#, at, 1.0);
        let xml = banked_road_with(&format!(
            r#"<object id="o" s="5" t="0"><outlines>{}{}{}{}{}{}</outlines></object>"#,
            // A hole inside the second outline, one in the open third, and
            // one inside none.
            hole((11.0, 1.0)),
            square("", (0.0, 0.0), 3.0),
            square("", (10.0, 0.0), 3.0),
            square(r#"closed="false""#, (20.0, 0.0), 3.0),
            hole((21.0, 1.0)),
            hole((30.0, 0.0)),
        ));
        let holes: Vec<usize> = objects_of(&xml)
            .iter()
            .map(|o| match &o.shape {
                Shape::Outline { holes, .. } => holes.len(),
                _ => panic!("an outline: {:?}", o.shape),
            })
            .collect();
        assert_eq!(holes, [0, 1, 0]);
    }

    #[test]
    fn a_structure_covers_each_section_it_spans_as_far_along_each_lane() {
        let xml = two_section_road_with(
            r#"<tunnel s="5" length="10" name="t"/>
               <bridge s="12" length="3" name="b"><validity fromLane="-2" toLane="-2"/></bridge>
               <tunnel s="5" name="no length"/>
               <bridge s="5" length="-1" name="negative"/>"#,
        );
        let (net, prov) = load_str_with_provenance(&xml).expect("import");
        let section_of = |id: LaneId| {
            let p = prov.lanes.iter().find(|p| p.lane == id).unwrap();
            (p.section, p.od_id)
        };
        let covered = |i: usize| {
            net.structures()[i]
                .lanes
                .iter()
                .map(|c| (section_of(c.lane), c.from, c.to))
                .collect::<Vec<_>>()
        };
        assert_eq!(net.structures().len(), 2);
        // The road is straight and flat, so a lane is as long as its section.
        // Section 1 starts at s = 10, and its lanes at 0.
        assert_eq!(
            covered(0),
            [
                ((0, 1), 5.0, 10.0),
                ((0, -1), 5.0, 10.0),
                ((0, -2), 5.0, 10.0),
                ((1, 1), 0.0, 5.0),
                ((1, -1), 0.0, 5.0),
                ((1, -2), 0.0, 5.0),
            ]
        );
        assert_eq!(covered(1), [((1, -2), 2.0, 5.0)]);
    }

    fn objects_of(xml: &str) -> Vec<Object> {
        load_str(xml).expect("import").objects().to_vec()
    }

    /// A solid's position, pitch, roll and extent.
    fn solid(object: &Object) -> (Point, f32, f32, Option<Extent>) {
        match object.shape {
            Shape::Solid {
                position,
                pitch,
                roll,
                extent,
                ..
            } => (position, pitch, roll, extent),
            ref other => panic!("not a solid: {other:?}"),
        }
    }

    #[test]
    fn an_object_stands_upright_on_a_banked_road() {
        // Same pivot as a lane at t = 3: up by 3 sin 0.1, in to 3 cos 0.1.
        // Then zOffset raises it straight up, and its roll is against the
        // horizontal, not the bank.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="10" t="3" zOffset="0.5" roll="0.05"/>"#,
        ));
        let (p, pitch, roll, _) = solid(&objects[0]);
        let (sin, cos) = 0.1_f32.sin_cos();
        assert!((p.x - 10.0).abs() < 1e-5, "x {}", p.x);
        assert!((p.y - 3.0 * cos).abs() < 1e-5, "y {}", p.y);
        assert!((p.z - (3.0 * sin + 0.5)).abs() < 1e-5, "z {}", p.z);
        assert!(pitch.abs() < 1e-6, "pitch {pitch}");
        assert!((roll - 0.05).abs() < 1e-6, "roll {roll}");
    }

    #[test]
    fn an_object_perpendicular_to_the_road_leans_with_the_bank_and_ignores_pitch_and_roll() {
        // zOffset raises it square to the banked surface, and it rolls with
        // the bank alone.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="10" t="3" zOffset="0.5" pitch="0.2" roll="0.05" perpToRoad="true"/>"#,
        ));
        let (p, pitch, roll, _) = solid(&objects[0]);
        let (sin, cos) = 0.1_f32.sin_cos();
        assert!((p.x - 10.0).abs() < 1e-5, "x {}", p.x);
        assert!((p.y - (3.0 * cos - 0.5 * sin)).abs() < 1e-5, "y {}", p.y);
        assert!((p.z - (3.0 * sin + 0.5 * cos)).abs() < 1e-5, "z {}", p.z);
        assert!(pitch.abs() < 1e-6, "pitch {pitch}");
        assert!((roll - 0.1).abs() < 1e-6, "roll {roll}");
    }

    #[test]
    fn an_object_without_a_station_is_skipped() {
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" t="3"/><object id="2" s="NaN" t="3"/><object id="3" s="4" t="0"/>"#,
        ));
        assert_eq!(objects.len(), 1);
        assert_eq!(solid(&objects[0]).0.x, 4.0);
    }

    #[test]
    fn instances_past_the_ends_of_the_road_are_dropped() {
        // s = 15, 20, 25, 30: only the two on the 20 m road survive, and a lone
        // object before its start goes too.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="15" length="15" distance="5" tStart="0" tEnd="0"/>
               </object>
               <object id="2" s="-1" t="0"/>"#,
        ));
        let xs: Vec<f32> = objects.iter().map(|o| solid(o).0.x).collect();
        assert_eq!(xs, vec![15.0, 20.0]);
    }

    #[test]
    fn a_runaway_repeat_is_refused_rather_than_expanded() {
        // A billion posts is a malformed file, not a map; the road still loads.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="0" length="20" distance="0.00000002"/>
               </object>"#,
        ));
        assert!(objects.is_empty());
    }

    #[test]
    fn each_repeat_of_an_object_contributes_its_own_row() {
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" type="pole" s="0" t="0" radius="0.1">
                 <repeat s="0" length="10" distance="10" tStart="2" tEnd="2"/>
                 <repeat s="0" length="10" distance="10" tStart="-2" tEnd="-2"/>
               </object>"#,
        ));
        assert_eq!(objects.len(), 4);
        assert!(objects.iter().all(|o| o.kind == ObjectType::Pole));
        assert!(objects.iter().all(|o| solid(o).3
            == Some(Extent::Cylinder {
                radius: 0.1,
                height: 0.0
            })));
    }

    #[test]
    fn a_repeat_moves_the_outlines_to_each_step() {
        // A local square with a hole, and a road triangle. `ds` and `dt` move
        // the triangle's corners.
        let outlines = |ds: f64, dt: f64| {
            let road = |s: f64, t: f64| format!(r#"<cornerRoad s="{}" t="{}"/>"#, s + ds, t + dt);
            format!(
                r#"<outlines>
                     <outline><cornerLocal u="0" v="0"/><cornerLocal u="2" v="0"/>
                       <cornerLocal u="2" v="2"/><cornerLocal u="0" v="2"/></outline>
                     <outline outer="false"><cornerLocal u="0.5" v="0.5"/>
                       <cornerLocal u="1.5" v="0.5"/><cornerLocal u="1" v="1.5"/></outline>
                     <outline>{}{}{}</outline>
                   </outlines>"#,
                road(2.0, 3.0),
                road(4.0, 3.0),
                road(3.0, 5.0),
            )
        };
        let repeated = objects_of(&banked_road_with(&format!(
            r#"<object id="1" s="2" t="3" hdg="0.3">
                 <repeat s="1" length="10" distance="5" tStart="1" tEnd="2"/>{}
               </object>"#,
            outlines(0.0, 0.0),
        )));
        // Each step bakes as the same object placed there would.
        let mut want = Vec::new();
        for (s, t) in [(1.0, 1.0), (6.0, 1.5), (11.0, 2.0)] {
            want.extend(objects_of(&banked_road_with(&format!(
                r#"<object id="1" s="{s}" t="{t}" hdg="0.3">{}</object>"#,
                outlines(s - 2.0, t - 3.0),
            ))));
        }
        assert_eq!(want.len(), 6);
        let shapes =
            |objects: &[Object]| objects.iter().map(|o| o.shape.clone()).collect::<Vec<_>>();
        assert_eq!(shapes(&repeated), shapes(&want));
        assert!(matches!(&want[0].shape, Shape::Outline { holes, .. } if holes.len() == 1));
    }

    #[test]
    fn a_section_bends_as_sharply_as_its_sharpest_lane_edge() {
        let width = |a, c| Cubic {
            start: 0.0,
            a,
            b: 0.0,
            c,
            d: 0.0,
        };
        let lane = |widths| LaneGeom {
            id: -1,
            extent: LaneExtent::Width(vec![widths]),
            heights: Vec::new(),
            level: false,
        };
        let road = |shape| [GeomRec::new(0.0, 0.0, 0.0, 0.0, 10.0, shape)];
        let bend = |shape, right: &[LaneGeom]| {
            section_bends(&road(shape), &[], [&[], right], 0.0, 10.0)
                .into_iter()
                .fold(0.0, f64::max)
        };
        let even = [lane(width(3.0, 0.0))];
        // Straight and even, nothing bends. On a 10 m radius every edge turns
        // 0.1 rad a metre, however far out.
        assert!(bend(GeomShape::Line, &even) < 1e-9);
        let arc = bend(GeomShape::Arc { curvature: 0.1 }, &even);
        assert!((arc - 0.1).abs() < 1e-6, "{arc}");
        // A lane widening as 0.1 s^2 swings its outer edge off a straight
        // line, turning it 0.2 rad a metre where it starts.
        let flare = bend(GeomShape::Line, &[lane(width(3.0, 0.1))]);
        assert!((flare - 0.2).abs() < 0.01, "{flare}");

        // Samples every 2 m with no bend, closer to keep to SAMPLE_TURN, and
        // no closer than SAMPLE_MIN_STEP.
        assert_eq!(
            sample_positions(0.0, 6.0, &[0.0; 25], &[]),
            [0.0, 2.0, 4.0, 6.0]
        );
        assert_eq!(sample_positions(0.0, 1.0, &[0.1; 5], &[]), [0.0, 0.5, 1.0]);
        assert_eq!(sample_positions(0.0, 1.0, &[10.0; 5], &[]).len(), 5);
    }

    #[test]
    fn only_the_road_near_a_tight_spot_is_sampled_finely() {
        // 100 m of straight with one sharp probe in the middle.
        let mut bends = vec![0.0; 101];
        bends[50] = 10.0;
        let ss = sample_positions(0.0, 100.0, &bends, &[]);
        let steps: Vec<f64> = ss.windows(2).map(|w| w[1] - w[0]).collect();
        // Fine at the spot, coarse well away from it.
        let at = |s: f64| steps[ss.iter().rposition(|&x| x <= s).unwrap()];
        assert!((at(50.0) - SAMPLE_MIN_STEP).abs() < 1e-9, "{}", at(50.0));
        assert!((at(5.0) - SAMPLE_STEP).abs() < 1e-9, "{}", at(5.0));
        assert!((at(95.0) - SAMPLE_STEP).abs() < 1e-9, "{}", at(95.0));
        assert!(ss.len() < 100, "{} samples", ss.len());
        // Each step is well over what the mesh welds away as a stub, a
        // quarter of the step before it, bar the remainder at the end.
        for w in steps[..steps.len() - 1].windows(2) {
            assert!(w[1] > 0.5 * w[0], "{} after {}", w[1], w[0]);
        }
    }

    #[test]
    fn a_road_without_objects_has_none() {
        assert!(load_str(STRAIGHT).expect("import").objects().is_empty());
    }

    #[test]
    fn a_sweep_rises_straight_up_off_a_banked_road() {
        // The base follows the bank like a lane does. The top is `height`
        // above it in +Z, the same way zOffset raises a solid.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="0" length="20" distance="0" tStart="3" tEnd="3" widthStart="1"
                         heightStart="2" zOffsetStart="0.5"/>
               </object>"#,
        ));
        let Shape::Sweep { sections, .. } = &objects[0].shape else {
            panic!("not a sweep: {:?}", objects[0].shape);
        };
        assert_eq!(sections.len(), 3, "every 10 m, as it runs straight");
        let (sin, cos) = 0.1_f32.sin_cos();
        for section in sections {
            for (corner, t) in [(section.left, 3.5_f32), (section.right, 2.5)] {
                assert!(
                    (corner.base.y - t * cos).abs() < 1e-5,
                    "y {}",
                    corner.base.y
                );
                assert!((corner.base.z - (t * sin + 0.5)).abs() < 1e-5);
                assert_eq!(corner.top - corner.base, Vector::new(0.0, 0.0, 2.0));
            }
        }
    }

    #[test]
    fn a_sweep_is_sampled_closer_where_it_bends() {
        // A straight 10 m, then 10 m round a circle of radius 10 centred on
        // (10, 10), with a wall along the reference line.
        let xml = r#"<OpenDRIVE><road length="20" id="1" junction="-1">
            <planView>
              <geometry s="0" x="0" y="0" hdg="0" length="10"><line/></geometry>
              <geometry s="10" x="10" y="0" hdg="0" length="10"><arc curvature="0.1"/></geometry>
            </planView>
            <lanes><laneSection s="0"><right><lane id="-1" type="driving">
              <width sOffset="0" a="3.5"/></lane></right></laneSection></lanes>
            <objects><object id="1" s="0" t="0">
              <repeat s="0" length="20" distance="0" heightStart="1"/>
            </object></objects></road></OpenDRIVE>"#;
        let objects = objects_of(xml);
        let Shape::Sweep { sections, .. } = &objects[0].shape else {
            panic!("not a sweep: {:?}", objects[0].shape);
        };
        let bases: Vec<Point> = sections.iter().map(|s| s.left.base).collect();
        let on_arc = bases.iter().filter(|p| p.x > 10.0 + 1e-3).count();
        assert_eq!(bases.len() - on_arc, 2, "the straight is one piece");
        // Each wall between two sections on the arc strays under 1 cm inside
        // it, which a wall every 2 m does not: that strays 5 cm.
        let centre = Point::new(10.0, 10.0, 0.0);
        for pair in bases.windows(2).skip(1) {
            let middle = pair[0].lerp(pair[1], 0.5);
            let strays = 10.0 - middle.distance_to(centre);
            assert!(strays < SWEEP_TOLERANCE, "{pair:?} strays {strays} m");
        }
    }

    #[test]
    fn a_sweep_starting_before_the_road_is_anchored_where_the_road_starts() {
        // s = -10..10 with t going 0..4: at s = 0 it is halfway, t = 2.
        let (_, prov) = load_str_with_provenance(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="-10" length="20" distance="0" tStart="0" tEnd="4"/>
               </object>"#,
        ))
        .expect("import");
        let p = &prov.objects[0];
        assert_eq!((p.s, p.t), (0.0, 2.0));
    }

    #[test]
    fn a_sweep_entirely_off_the_road_bakes_nothing() {
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="0" t="0">
                 <repeat s="25" length="10" distance="0" tStart="3"/>
               </object>"#,
        ));
        assert!(objects.is_empty());
    }

    #[test]
    fn an_outline_straight_under_the_object_is_read_as_opendrive_1_4_writes_it() {
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="5" t="0">
                 <outline><cornerRoad s="5" t="0"/><cornerRoad s="6" t="0"/></outline>
               </object>"#,
        ));
        assert!(matches!(
            &objects[0].shape,
            Shape::Outline { corners, closed: true, .. } if corners.len() == 2
        ));
    }

    #[test]
    fn an_outline_with_a_corner_off_the_road_is_dropped_whole() {
        // Keeping the three corners that fit would bake a triangle where the
        // file has a square. The object keeps its box.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="5" t="0"><outlines><outline>
                 <cornerRoad s="18" t="0"/><cornerRoad s="22" t="0"/>
                 <cornerRoad s="22" t="2"/><cornerRoad s="18" t="2"/>
               </outline></outlines></object>"#,
        ));
        assert_eq!(objects.len(), 1);
        assert!(matches!(objects[0].shape, Shape::Solid { .. }));
    }

    /// A road running along +X on level ground.
    const FLAT: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    #[test]
    fn a_frames_angles_turn_it_as_it_is() {
        // Turned on a road that climbs, banks and bends, and pitched straight
        // up, where yaw and roll turn about the same axis.
        let cubic = |a, b| Cubic {
            start: 0.0,
            a,
            b,
            c: 0.0,
            d: 0.0,
        };
        let road = Road {
            id: RoadId(0),
            od_id: String::new(),
            name: String::new(),
            junction: None,
            length: 10.0,
            rule: TrafficRule::RightHand,
            geoms: vec![GeomRec::new(0.0, 0.0, 0.0, 0.4, 10.0, GeomShape::Line)],
            elevations: vec![cubic(0.0, 0.08)],
            superelevations: vec![cubic(-0.2, 0.0)],
            lateral: Lateral::default(),
            lane_offsets: Vec::new(),
            temporary_lane_offsets: Vec::new(),
            sections: Vec::new(),
        }
        .axes(5.0);
        let half = std::f64::consts::FRAC_PI_2;
        for (axes, turn) in [
            (FLAT, (0.3, -0.2, 0.5)),
            (road, (0.3, -0.2, 0.5)),
            (FLAT, (0.3, half, 0.5)),
            (road, (1.0, 0.0, 0.0)),
        ] {
            let frame = Frame::on_road(Point::ORIGIN, axes, turn);
            let again = Frame::on_road(Point::ORIGIN, FLAT, frame.angles());
            for (a, b) in frame.axes.iter().zip(again.axes) {
                for (x, y) in a.iter().zip(b) {
                    assert!((x - y).abs() < 1e-9, "{:?} != {:?}", frame.axes, again.axes);
                }
            }
        }
    }

    #[test]
    fn a_frame_turns_yaw_then_pitch_then_roll() {
        let frame = |yaw: f64, pitch: f64, roll: f64| {
            Frame::on_road(Point::new(1.0, 2.0, 3.0), FLAT, (yaw, pitch, roll))
        };
        let near = |got: Point, want: [f32; 3]| {
            assert!(
                (got - Point::from_array(want)).length() < 1e-6,
                "{got:?} != {want:?}"
            );
        };
        let half = std::f64::consts::FRAC_PI_2;
        // Each on its own: yaw takes u to +Y, pitch takes u down, roll takes
        // v up.
        near(
            frame(half, 0.0, 0.0).point([1.0, 0.0, 0.0]),
            [1.0, 3.0, 3.0],
        );
        near(
            frame(0.0, half, 0.0).point([1.0, 0.0, 0.0]),
            [1.0, 2.0, 2.0],
        );
        near(
            frame(0.0, 0.0, half).point([0.0, 1.0, 0.0]),
            [1.0, 2.0, 4.0],
        );
        // Together, roll acts first: it turns v up, pitch then tips up to
        // forward, and yaw turns forward to +Y.
        near(
            frame(half, half, half).point([0.0, 1.0, 0.0]),
            [1.0, 3.0, 3.0],
        );
    }

    /// A straight 20 m road along +X, with a 3 m driving lane -1 and a 2 m
    /// sidewalk -2 carrying `heights`, and `center` inside the center lane.
    fn raised_road(heights: &str, center: &str) -> String {
        format!(
            r#"<OpenDRIVE><road id="1" length="20" junction="-1">
  <planView><geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>
  <lanes><laneSection s="0">
    <center><lane id="0" type="none">{center}</lane></center>
    <right>
      <lane id="-1" type="driving"><width sOffset="0" a="3" b="0" c="0" d="0"/></lane>
      <lane id="-2" type="sidewalk"><width sOffset="0" a="2" b="0" c="0" d="0"/>{heights}</lane>
    </right>
  </laneSection></lanes>
</road></OpenDRIVE>"#
        )
    }

    /// The height of the sidewalk's centerline at the station at `x`.
    fn sidewalk_z(xml: &str, x: f32) -> f32 {
        let net = load_str(xml).unwrap();
        let sidewalk = net
            .lanes()
            .iter()
            .find(|l| l.kind == LaneType::Sidewalk)
            .unwrap();
        sidewalk
            .center
            .points()
            .iter()
            .find(|p| (p.x - x).abs() < 1e-4)
            .unwrap_or_else(|| panic!("no station at {x}"))
            .z
    }

    fn assert_z(got: f32, want: f32) {
        assert!((got - want).abs() < 1e-6, "z {got}, want {want}");
    }

    /// At 4 m the kerb is 0.08 m, and the middle halfway to 0.2 m. Read as
    /// steps, it would be 0.1 m until 10 m.
    #[test]
    fn heights_go_straight_from_one_to_the_next_rather_than_step() {
        let xml = raised_road(
            r#"<height sOffset="0" inner="0" outer="0.2"/>
               <height sOffset="10" inner="0.2" outer="0.2"/>"#,
            "",
        );
        assert_z(sidewalk_z(&xml, 4.0), 0.14);
        assert_z(sidewalk_z(&xml, 10.0), 0.2);
    }

    #[test]
    fn the_last_height_holds_after_it() {
        let xml = raised_road(
            r#"<height sOffset="0" inner="0" outer="0"/>
               <height sOffset="10" inner="0.2" outer="0.2"/>"#,
            "",
        );
        assert_z(sidewalk_z(&xml, 16.0), 0.2);
    }

    /// libOpenDRIVE would carry the ramp on back to 0.05 m at 4 m.
    #[test]
    fn the_first_height_holds_before_it() {
        let xml = raised_road(
            r#"<height sOffset="6" inner="0.1" outer="0.1"/>
               <height sOffset="10" inner="0.2" outer="0.2"/>"#,
            "",
        );
        assert_z(sidewalk_z(&xml, 4.0), 0.1);
    }

    #[test]
    fn a_height_missing_an_attribute_reads_it_as_0() {
        let no_inner = raised_road(r#"<height sOffset="0" outer="0.2"/>"#, "");
        assert_z(sidewalk_z(&no_inner, 4.0), 0.1);
        let no_outer = raised_road(r#"<height sOffset="0" inner="0.2"/>"#, "");
        assert_z(sidewalk_z(&no_outer, 4.0), 0.1);
        let no_s_offset = raised_road(
            r#"<height inner="0.1" outer="0.1"/>
               <height sOffset="10" inner="0.3" outer="0.3"/>"#,
            "",
        );
        assert_z(sidewalk_z(&no_s_offset, 0.0), 0.1);
        assert_z(sidewalk_z(&no_s_offset, 4.0), 0.18);
    }

    /// From -5 m the ramp would already be at 0.1667 m by 0 m.
    #[test]
    fn a_negative_height_s_offset_reads_as_0() {
        let xml = raised_road(
            r#"<height sOffset="-5" inner="0.1" outer="0.1"/>
               <height sOffset="10" inner="0.3" outer="0.3"/>"#,
            "",
        );
        assert_z(sidewalk_z(&xml, 0.0), 0.1);
        assert_z(sidewalk_z(&xml, 4.0), 0.18);
    }

    #[test]
    fn heights_out_of_order_are_sorted() {
        let xml = raised_road(
            r#"<height sOffset="10" inner="0.2" outer="0.2"/>
               <height sOffset="0" inner="0" outer="0"/>"#,
            "",
        );
        assert_z(sidewalk_z(&xml, 4.0), 0.08);
        assert_z(sidewalk_z(&xml, 16.0), 0.2);
    }

    #[test]
    fn a_second_height_at_the_same_s_offset_takes_over_there() {
        let xml = raised_road(
            r#"<height sOffset="0" inner="0.1" outer="0.1"/>
               <height sOffset="10" inner="0.1" outer="0.1"/>
               <height sOffset="10" inner="0.3" outer="0.3"/>"#,
            "",
        );
        assert_z(sidewalk_z(&xml, 8.0), 0.1);
        assert_z(sidewalk_z(&xml, 10.0), 0.3);
        assert_z(sidewalk_z(&xml, 16.0), 0.3);
    }

    #[test]
    fn heights_on_the_center_lane_are_ignored() {
        let xml = raised_road("", r#"<height sOffset="0" inner="0.5" outer="0.5"/>"#);
        let net = load_str(&xml).unwrap();
        for lane in net.lanes() {
            assert!(lane.bank.is_empty());
            assert!(lane.center.points().iter().all(|p| p.z == 0.0));
        }
    }

    #[test]
    fn a_lane_without_heights_bakes_level_with_the_road() {
        let (net, prov) = load_str_with_provenance(&raised_road("", "")).unwrap();
        assert!(net.lanes().iter().all(|l| l.bank.is_empty()));
        assert!(prov.lanes.iter().all(|p| p.heights.is_empty()));
    }

    #[test]
    fn a_knot_becomes_a_station_and_never_a_stub() {
        let ss = sample_positions(0.0, 10.0, &[0.0; 11], &[2.1, 5.0, 10.0, 12.0]);
        assert_eq!(ss, [0.0, 1.05, 2.1, 3.55, 5.0, 7.0, 9.0, 10.0]);
        let ss = sample_positions(0.0, 10.0, &[0.0; 11], &[4.0]);
        assert_eq!(ss, [0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
    }

    /// `xml` with `objects` on its road.
    fn with_objects(xml: &str, objects: &str) -> String {
        xml.replace("</road>", &format!("<objects>{objects}</objects></road>"))
    }

    /// A sidewalk 0.2 m up from its kerb at t = -3 m. A point exactly on
    /// the kerb is on lane -1, the lane inside it, as libOpenDRIVE finds it.
    #[test]
    fn a_point_on_a_border_stands_on_the_inner_lane() {
        let xml = with_objects(
            &raised_road(r#"<height sOffset="0" inner="0.2" outer="0.2"/>"#, ""),
            r#"<object id="1" s="10" t="-3"/><object id="2" s="10" t="-3.001"/>"#,
        );
        let objects = objects_of(&xml);
        assert_z(solid(&objects[0]).0.z, 0.0);
        assert_z(solid(&objects[1]).0.z, 0.2);
    }

    /// A sidewalk from 0.1 m at its kerb to 0.2 m at its outer edge, at
    /// t = -5 m. 2 m past it, a point is at 0.2 m, where libOpenDRIVE would
    /// carry the slope on to 0.3 m.
    #[test]
    fn a_point_past_the_outermost_lane_takes_its_outer_height() {
        let xml = with_objects(
            &raised_road(r#"<height sOffset="0" inner="0.1" outer="0.2"/>"#, ""),
            r#"<object id="1" s="10" t="-7"/>"#,
        );
        assert_z(solid(&objects_of(&xml)[0]).0.z, 0.2);
    }

    /// An outline from the road onto the sidewalk has its corners on each.
    #[test]
    fn an_outline_across_a_kerb_has_its_corners_on_either_side() {
        let xml = with_objects(
            &raised_road(r#"<height sOffset="0" inner="0.2" outer="0.2"/>"#, ""),
            r#"<object id="1" s="10" t="-3"><outlines><outline id="0" closed="true">
                 <cornerRoad s="9" t="-2" dz="0" height="1"/>
                 <cornerRoad s="9" t="-4" dz="0" height="1"/>
                 <cornerRoad s="11" t="-4" dz="0" height="1"/>
               </outline></outlines></object>"#,
        );
        let Shape::Outline { corners, .. } = &objects_of(&xml)[0].shape else {
            panic!("not an outline");
        };
        let z: Vec<f32> = corners.iter().map(|c| c.base.z).collect();
        assert_z(z[0], 0.0);
        assert_z(z[1], 0.2);
        assert_z(z[2], 0.2);
    }

    /// A guard rail swept along the sidewalk stands on it all the way.
    #[test]
    fn a_sweep_along_a_sidewalk_stands_on_it() {
        let xml = with_objects(
            &raised_road(r#"<height sOffset="0" inner="0.2" outer="0.2"/>"#, ""),
            r#"<object id="1" s="2" t="-4" height="0.8">
                 <repeat s="2" length="10" distance="0" tStart="-4" tEnd="-4"
                         heightStart="0.8" heightEnd="0.8" zOffsetStart="0" zOffsetEnd="0"/>
               </object>"#,
        );
        let Shape::Sweep { sections, .. } = &objects_of(&xml)[0].shape else {
            panic!("not a sweep");
        };
        assert!(!sections.is_empty());
        for section in sections {
            assert_z(section.left.base.z, 0.2);
            assert_z(section.right.base.z, 0.2);
        }
    }
}
