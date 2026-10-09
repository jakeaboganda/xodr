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
    ShapeProfile, Strip, TrafficRule,
};
use crate::{
    Border, ControllerId, Corner, Coverage, CrgAlong, CrgMode, CrgPose, CrgPurpose, CrgSurface,
    Direction, Extent, GeoOffset, GeoReference, Lane, LaneId, LaneType, Marking, Material, Object,
    ObjectId, ObjectType, ParkingSpace, Polyline, Priority, RoadMarkId, RoadNeighbor, RoadNetwork,
    Section, Shape, Side, SignalId, Structure, StructureId, StructureKind, UserData,
};

mod gaps;
mod junction_areas;
mod links;
mod properties;
mod railway;
mod road_marks;
mod signals;
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
    /// The zero-based lane-section index within that road (sections ordered by
    /// `s`).
    pub section: usize,
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
    /// The `<road id>` the object is on.
    pub road_id: String,
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
    /// `Some` if an `<objectReference>` on `road_id` placed the object: the
    /// road its `<object>` is on. `s`, `t`, `orientation` and `valid_length`
    /// are then the reference's, and the shape is the `<object>`'s, moved to
    /// the reference's station.
    pub referenced_from: Option<String>,
}

/// Which direction of its road an object or a signal applies to, from its
/// `orientation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Orientation {
    /// Traffic along +s, `orientation="+"`.
    Positive,
    /// Traffic along -s, `orientation="-"`.
    Negative,
    /// Both, `orientation="none"`. Also what a missing or unrecognised
    /// value reads as.
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
}

/// The OpenDRIVE identity of everything a load baked, from
/// [`load_str_with_provenance`] or [`load_file_with_provenance`].
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Provenance {
    /// One per baked lane, in baked-lane order.
    pub lanes: Vec<LaneProvenance>,
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
    /// What the load dropped, or read against the spec, in file order.
    /// Empty for a clean file.
    pub warnings: Vec<Warning>,
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
    for road in root.children().filter(|n| n.has_tag_name("road")) {
        match parse_road(
            road,
            &index,
            &mut lanes,
            &mut objects,
            &mut structures,
            &mut surfaces,
            &mut topo,
        ) {
            Ok(mut baked) => {
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
    surfaces.extend(junction_crgs(root, &topo));
    let signals = signals::place(root, &roads, &objects.provenance);
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
    let provenance = Provenance {
        lanes: topo
            .metas
            .iter()
            .map(|m| LaneProvenance {
                lane: m.id,
                road_id: m.road.clone(),
                section: m.section,
                od_id: m.od_id,
                heights: m.heights.clone(),
            })
            .collect(),
        objects: objects.provenance,
        structures: structures.provenance,
        signals: signals.provenance,
        controllers: signals.controller_provenance,
        road_marks: road_marks.provenance,
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
            .with_lane_rules(properties.lane_rules)
            .with_lane_access(properties.lane_access)
            .with_lane_materials(properties.lane_materials)
            .with_lane_visibility(properties.lane_visibility)
            .with_geo_reference(geo_reference(root))
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
        let id = junction.attribute("id").unwrap_or_default();
        let roads: Vec<&str> = root
            .children()
            .filter(|n| n.has_tag_name("road") && n.attribute("junction") == Some(id))
            .filter_map(|n| n.attribute("id"))
            .collect();
        let lanes: Vec<LaneId> = topo
            .metas
            .iter()
            .filter(|m| roads.contains(&m.road.as_str()))
            .map(|m| m.id)
            .collect();
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
struct LaneDef {
    id: i32,
    kind: LaneType,
    pred_link: Option<i32>,
    succ_link: Option<i32>,
    /// The road marks on its outer border.
    marks: Vec<MarkDef>,
    speeds: Vec<properties::LaneSpeedDef>,
    rules: Vec<(f64, String)>,
    access: Vec<(f64, properties::AccessDef)>,
    materials: Vec<(f64, Material)>,
    visibility: Vec<(f64, properties::VisibilityDef)>,
    /// Its `direction`, or its value if the crate can't read it.
    direction: Result<LaneDirection, String>,
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
        let id = strip.attribute("id").and_then(|v| v.parse::<i32>().ok());
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
                let id = lane.attribute("id").and_then(|s| s.parse::<i32>().ok());
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
                let (pred_link, succ_link) = links::lane_link(lane);
                let def = LaneDef {
                    id,
                    kind: lane_type(lane.attribute("type")),
                    pred_link,
                    succ_link,
                    marks: road_marks::parse(lane),
                    speeds: properties::lane_speeds(lane),
                    rules: properties::lane_rules(lane),
                    access: properties::lane_access(lane),
                    materials: properties::lane_materials(lane),
                    visibility: properties::lane_visibility(lane),
                    direction: LaneDirection::parse(lane),
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
        let center = child(section, "center")
            .into_iter()
            .flat_map(|c| c.children())
            .find(|n| n.has_tag_name("lane"))
            .map(road_marks::parse)
            .unwrap_or_default();
        Self {
            left,
            right,
            geoms: [left_geoms, right_geoms],
            center,
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
/// road's physics trimesh impossible to build.
fn attr_f64(node: roxmltree::Node, name: &str) -> Option<f64> {
    node.attribute(name)
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
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
    surfaces: &mut Vec<CrgSurface>,
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
    let lane_offsets = child(road, "lanes")
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
            length,
            rule,
            geoms,
            elevations,
            superelevations,
            lateral,
            lane_offsets,
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
        place_objects(objects_node, index, &baked.road, objects);
        place_structures(objects_node, &baked.road, out, structures);
    }
    for node in child(road, "surface")
        .into_iter()
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name("CRG"))
    {
        surfaces.extend(road_crg(node, &baked.road));
    }
    Ok(baked)
}

/// One `<planView><geometry>` record, or `None` for one the crate can't
/// bake: one missing its `s`, `x`, `y`, `hdg` or `length`, one whose
/// `length` isn't above 0 or is over [`MAX_LENGTH`], or one of a shape it
/// doesn't know.
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
    Some(GeomRec::new(s, x, y, hdg, length, shape))
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
        if offset_within(&road.lane_offsets, section.start, section.end) {
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

/// A road `<CRG>`, or `None` if it lacks a file, a known mode, or a stretch
/// of road to lie on.
fn road_crg(node: roxmltree::Node, road: &Road) -> Option<CrgSurface> {
    let from = attr_f64(node, "sStart")?.max(0.0);
    let to = attr_f64(node, "sEnd")?.min(road.length);
    if to <= from {
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
    let Some(lanes_node) = child(road, "lanes") else {
        return;
    };

    let start = |section: &roxmltree::Node| attr_f64(*section, "s").unwrap_or(0.0).max(0.0);
    let mut sections: Vec<roxmltree::Node> = lanes_node
        .children()
        .filter(|n| n.has_tag_name("laneSection"))
        .collect();
    if sections.is_empty() {
        return;
    }
    sections.sort_by(|a, b| start(a).total_cmp(&start(b)));

    let length = baked.road.length;
    for (i, section) in sections.iter().enumerate() {
        let s_start = start(section);
        let s_end = sections.get(i + 1).map_or(length, start).min(length);
        if s_end - s_start < 1e-3 {
            continue; // zero-length section
        }
        let first = topo.metas.len();
        let mut def = SectionDef::parse(*section);
        let [left, right] = std::mem::take(&mut def.geoms);
        let mut geom = RoadSection {
            index: i,
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

    let (predecessor, successor) = links::road_link(road);
    topo.roads.insert(
        baked.road.od_id.clone(),
        RoadInfo {
            sections: baked.road.sections.iter().map(|s| s.index).collect(),
            junction: baked.road.junction().map(String::from),
            predecessor,
            successor,
        },
    );
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
        &road.lane_offsets,
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
        let across = side_across(road, side, sign, s_start, &sample_s);
        let heights = side_height_rows(road, side, sign, s_start, &sample_s, &road_bank, &across);
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
                succ_link: def.succ_link,
                pred_link: def.pred_link,
                heights: lane_heights(lane, s_start, &sample_s),
            });
            emitted.push((id, out.len(), kind, direction));
            let tilted = !lane.heights.is_empty() || !road.lateral.is_empty() || held;
            let (width, widths) = width_profile(borders, tilted.then_some(&heights[..]));
            out.push(Lane {
                id,
                kind,
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
    side: &[LaneGeom],
    sign: f64,
    s_start: f64,
    sample_s: &[f64],
) -> Vec<Vec<LaneBorders>> {
    let mut rows: Vec<Vec<LaneBorders>> = (0..side.len())
        .map(|_| Vec::with_capacity(sample_s.len()))
        .collect();
    for &s in sample_s {
        let base = road.base(s);
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
    side: &[LaneGeom],
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
            road.base(s),
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
}

/// Where an object sits along its road, and how big it is there. An
/// `<object>` gives one of these, and a `<repeat>` gives one per station.
struct Station {
    s: f64,
    t: f64,
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
    /// The frame of an object turned `hdg`, `pitch` and `roll` against the
    /// road's `axes`, as [`orient`] turns them, with its origin at `origin`.
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
                .filter(|(od_id, _)| is_valid(*od_id, &validity))
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

/// Every `<object>` in the document by its id, with the road it is on, for
/// an `<objectReference>` to find. Ids are meant to be unique across the
/// file. Where two objects share one, the first wins.
type ObjectIndex<'a> = HashMap<&'a str, (&'a str, roxmltree::Node<'a, 'a>)>;

fn object_index<'a>(root: roxmltree::Node<'a, 'a>) -> ObjectIndex<'a> {
    let mut index = ObjectIndex::new();
    for road in root.children().filter(|n| n.has_tag_name("road")) {
        let road_id = road.attribute("id").unwrap_or_default();
        let Some(objects) = child(road, "objects") else {
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
    /// The `<validity>` lane ranges, `fromLane` and `toLane`, of the
    /// `<object>` or the `<objectReference>`. A reference does not take the
    /// `<object>`'s, which name lanes on the `<object>`'s road.
    validity: Vec<(i32, i32)>,
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
            validity: validity(reference),
        })
    }
}

/// The `fromLane`-`toLane` range of each `<validity>` under `node`. One
/// missing either is skipped.
fn validity(node: roxmltree::Node) -> Vec<(i32, i32)> {
    let lane = |v: roxmltree::Node, name| v.attribute(name)?.parse::<i32>().ok();
    node.children()
        .filter(|n| n.has_tag_name("validity"))
        .filter_map(|v| Some((lane(v, "fromLane")?, lane(v, "toLane")?)))
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
/// `<objects>` into `out`. `index` finds the `<object>` a reference names.
///
/// A reference to an id no `<object>` has is skipped, as is one missing `s`
/// or `t`.
fn place_objects(
    objects_node: roxmltree::Node,
    index: &ObjectIndex,
    road: &Road,
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
        if let Some((object, at)) = placed {
            place_object(object, &at, road, out);
        }
    }
}

/// Bake one `<object>` into `out`, placed on `road` as `at` says.
///
/// Each object sits on the road surface, so it rides the elevation and
/// superelevation profiles like a lane does. Its own frame is the road's
/// axes there, from [`road_axes`], turned by its `hdg`, `pitch` and `roll`
/// and raised `zOffset` along the surface normal. One `<object>` can bake to
/// several [`Object`]s, following libOpenDRIVE:
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
    let too_many = Cell::new(false);
    let base = Station {
        s: at.s,
        t: at.t,
        z_offset: at.z_offset,
        length: attr_f64(node, "length"),
        width: attr_f64(node, "width"),
        height: attr_f64(node, "height"),
        radius: attr_f64(node, "radius"),
    };
    let hdg = attr_f64(node, "hdg").unwrap_or(0.0);
    let pitch = attr_f64(node, "pitch").unwrap_or(0.0);
    let roll = attr_f64(node, "roll").unwrap_or(0.0);
    let frame = |st: &Station| {
        let (on_surface, _) = road.surface(st.s, st.t);
        let axes = road.axes(st.s);
        let raise = axes[2].map(|c| (c * st.z_offset) as f32);
        let origin = on_surface + Vector::from_array(raise);
        Frame::on_road(origin, axes, (hdg, pitch, roll))
    };
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
        part.markings = side_markings(node, &f, extent(st), &too_many);
        part
    };

    let mut parts = Vec::new();
    let repeats: Vec<Repeat> = node
        .children()
        .filter(|n| n.has_tag_name("repeat"))
        .filter_map(|n| Repeat::parse(n, at.shift))
        .collect();
    let outlines = outline_nodes(node);
    let outlined = |st: &Station| {
        let origin = road.on_road(st.s).then(|| frame(st));
        let shift = (at.shift.0 + st.s - base.s, at.shift.1 + st.t - base.t);
        outline_parts(node, &outlines, origin.as_ref(), st, shift, road, &too_many)
    };
    for repeat in &repeats {
        if repeat.distance > 0.0 {
            let Some(stations) = repeat.stations(&base) else {
                too_many.set(true);
                continue;
            };
            for st in stations {
                if !outlines.is_empty() {
                    parts.extend(outlined(&st));
                } else if road.on_road(st.s) {
                    parts.push(point(&st));
                }
            }
        } else if let Some((shape, start, end)) = sweep(repeat, &base, road) {
            parts.push(Part::new(shape, &start, (start.s, end)));
        }
    }
    if !repeats.iter().any(|r| r.distance > 0.0) {
        if !outlines.is_empty() {
            parts.extend(outlined(&base));
        } else if repeats.is_empty() && road.on_road(base.s) {
            parts.push(point(&base));
        }
    }

    let kind = object_type(node.attribute("type"));
    let text = |name| node.attribute(name).unwrap_or_default().to_string();
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
    for part in parts {
        let id = ObjectId(out.baked.len());
        out.baked.push(Object {
            id,
            kind,
            subtype: text("subtype"),
            name: text("name"),
            dynamic: node.attribute("dynamic") == Some("yes"),
            lanes: road.lanes(part.stretch, &at.validity),
            markings: part.markings,
            borders: part.borders,
            parking_space: parking_space.clone(),
            materials: materials.clone(),
            user_data: user_data.clone(),
            shape: part.shape,
        });
        out.provenance.push(ObjectProvenance {
            object: id,
            road_id: road.od_id.clone(),
            od_id: text("id"),
            s: part.s,
            t: part.t,
            orientation: at.orientation,
            valid_length: at.valid_length,
            referenced_from: at.referenced_from.map(str::to_string),
        });
    }
}

/// An object's `outlines` placed at `st`, one [`Part`] per outer outline,
/// with its holes, markings and borders. `origin` is the object's frame at
/// `st`, if that is on the road, and `shift` moves `<cornerRoad>` corners.
/// An `outer="false"` outline is a hole in the first closed outer outline
/// that encloses it in plan, and is dropped if none does.
fn outline_parts(
    node: roxmltree::Node,
    outlines: &[roxmltree::Node],
    origin: Option<&Frame>,
    st: &Station,
    shift: (f64, f64),
    road: &Road,
    too_many: &Cell<bool>,
) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut holes = Vec::new();
    for &outline_node in outlines {
        let Some(ring) = outline(outline_node, origin, st.s, shift, road) else {
            continue;
        };
        if outline_node.attribute("outer") == Some("false") {
            holes.push((outline_node, ring));
            continue;
        }
        let mut part = Part::new(
            Shape::Outline {
                corners: ring.corners.clone(),
                closed: ring.closed,
                holes: Vec::new(),
            },
            st,
            ring.stretch,
        );
        part.markings = markings(node, outline_node, &ring, too_many);
        part.borders = borders(node, outline_node, &ring, ring.closed);
        parts.push(part);
    }
    for (outline_node, ring) in holes {
        let corners = &ring.corners;
        let owner = parts.iter_mut().find(|p| {
            matches!(&p.shape, Shape::Outline { corners: outer, closed: true, .. }
                if corners.len() >= 3 && corners.iter().all(|c| encloses(outer, c.base)))
        });
        let Some(owner) = owner else {
            continue;
        };
        owner
            .markings
            .extend(markings(node, outline_node, &ring, too_many));
        owner
            .borders
            .extend(borders(node, outline_node, &ring, true));
        if let Shape::Outline { holes, .. } = &mut owner.shape {
            holes.push(ring.corners);
        }
    }
    parts
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
    markings: Vec<Marking>,
    borders: Vec<Border>,
}

impl Part {
    fn new(shape: Shape, at: &Station, stretch: (f64, f64)) -> Self {
        Self {
            shape,
            s: at.s,
            t: at.t,
            stretch,
            markings: Vec::new(),
            borders: Vec::new(),
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
}

impl<'a> Repeat<'a> {
    /// The repeat moved by a [`Placement`]'s `shift`, or `None` for one
    /// missing its `s`, `length` or `distance`, or carrying a negative one of
    /// the last two.
    fn parse(node: roxmltree::Node<'a, 'a>, (shift_s, shift_t): (f64, f64)) -> Option<Self> {
        let repeat = Self {
            node,
            start: attr_f64(node, "s")? + shift_s,
            length: attr_f64(node, "length")?,
            distance: attr_f64(node, "distance")?,
            shift_t,
        };
        (repeat.length >= 0.0 && repeat.distance >= 0.0).then_some(repeat)
    }

    /// The object's station a fraction `f` of the way along the repeat, with
    /// `t`, `zOffset` and each dimension interpolated from its start value to
    /// its end value. A value the repeat does not give is the object's own.
    fn at(&self, base: &Station, f: f64) -> Station {
        let lerp = |name: &str, fallback: Option<f64>, shift: f64| {
            let given = |end| attr_f64(self.node, &format!("{name}{end}")).map(|v| v + shift);
            let start = given("Start").or(fallback)?;
            let end = given("End").unwrap_or(start);
            Some(start + (end - start) * f)
        };
        Station {
            s: self.start + self.length * f,
            t: lerp("t", Some(base.t), self.shift_t).unwrap_or(base.t),
            z_offset: lerp("zOffset", Some(base.z_offset), 0.0).unwrap_or(base.z_offset),
            length: lerp("length", base.length, 0.0),
            width: lerp("width", base.width, 0.0),
            height: lerp("height", base.height, 0.0),
            radius: lerp("radius", base.radius, 0.0),
        }
    }

    /// One station every `distance` metres, both ends included, or `None` if
    /// that is more than [`MAX_REPEAT_INSTANCES`].
    fn stations(&self, base: &Station) -> Option<Vec<Station>> {
        let count = (self.length / self.distance).floor() + 1.0;
        if count > MAX_REPEAT_INSTANCES {
            return None;
        }
        let stations = (0..count as usize)
            .map(|k| {
                let along = k as f64 * self.distance;
                let f = if self.length > 0.0 {
                    along / self.length
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
/// resting on `zOffset`, and `width` and `height` play no part. Also returns
/// the station the sections start at, and the `s` they end at. `None` if less
/// than a millimetre of it is on the road.
fn sweep(repeat: &Repeat, base: &Station, road: &Road) -> Option<(Shape, Station, f64)> {
    let start = repeat.start.max(0.0);
    let end = (repeat.start + repeat.length).min(road.length);
    if end - start < 1e-3 {
        return None;
    }
    let first = repeat.at(base, (start - repeat.start) / repeat.length);
    let round = first.radius.is_some();
    let section = |s: f64| {
        let st = repeat.at(base, (s - repeat.start) / repeat.length);
        let (half, height) = match st.radius {
            Some(r) => (r, 2.0 * r as f32),
            None => (
                st.width.unwrap_or(0.0) / 2.0,
                st.height.unwrap_or(0.0) as f32,
            ),
        };
        let corner = |t| {
            let (mut base, _) = road.surface(s, t);
            base.z += st.z_offset as f32;
            Corner {
                base,
                top: base + Vector::Z * height,
            }
        };
        Section {
            left: corner(st.t + half),
            right: corner(st.t - half),
        }
    };
    let sections = sweep_stations(start, end, |a, b, f| {
        let (a, b, at) = (section(a), section(b), section(a + (b - a) * f));
        let strays = |a: Corner, b: Corner, at: Corner| {
            a.base.lerp(b.base, f as f32).distance_to(at.base) > SWEEP_TOLERANCE
        };
        strays(a.left, b.left, at.left) || strays(a.right, b.right, at.right)
    })
    .into_iter()
    .map(section)
    .collect();
    Some((Shape::Sweep { sections, round }, first, end))
}

/// The stations of a sweep's sections over `[start, end]`: at most
/// [`SWEEP_MAX_STEP`] apart, and closer where the sweep bends. `bends(a, b,
/// f)` says whether the sweep a fraction `f` of the way from `a` to `b`
/// strays from the straight line between its sections there. An interval
/// halves while it does at a quarter, half or three quarters of the way,
/// down to [`SWEEP_MIN_STEP`].
fn sweep_stations(start: f64, end: f64, bends: impl Fn(f64, f64, f64) -> bool) -> Vec<f64> {
    let seeds = ((end - start) / SWEEP_MAX_STEP).ceil().max(1.0) as usize;
    let mut done = vec![start];
    let mut todo: Vec<f64> = (1..=seeds)
        .rev()
        .map(|k| start + (end - start) * k as f64 / seeds as f64)
        .collect();
    while let Some(&b) = todo.last() {
        let a = *done.last().expect("starts with start");
        if b - a > 2.0 * SWEEP_MIN_STEP && [0.25, 0.5, 0.75].iter().any(|&f| bends(a, b, f)) {
            todo.push((a + b) / 2.0);
        } else {
            done.push(b);
            todo.pop();
        }
    }
    done
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
struct Ring {
    corners: Vec<Corner>,
    /// The normal of the surface each corner is given on, in step with
    /// `corners`: the road's for a `<cornerRoad>`, the object frame's z axis
    /// for a `<cornerLocal>`. Bands along the edges lie in it.
    ups: Vec<Vector>,
    closed: bool,
    /// The stretch of road the outline spans.
    stretch: (f64, f64),
}

impl Ring {
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
/// axis. libOpenDRIVE raises a `<cornerRoad>` along the road's normal
/// instead, which differs on a banked road. An outline with a corner
/// that cannot be placed is dropped whole, because the polygon without it is
/// a different shape. So is one with fewer than two corners.
///
/// The stretch of road it spans runs from its first `<cornerRoad>` station to
/// its last, taking in `s`, the object's own station, if it has a
/// `<cornerLocal>`.
fn outline(
    node: roxmltree::Node,
    frame: Option<&Frame>,
    s: f64,
    (shift_s, shift_t): (f64, f64),
    road: &Road,
) -> Option<Ring> {
    let mut stretch = (f64::INFINITY, f64::NEG_INFINITY);
    let mut spans = |s: f64| stretch = (stretch.0.min(s), stretch.1.max(s));
    let corner = |c: roxmltree::Node| -> Option<(Corner, Vector)> {
        let height = attr_f64(c, "height").unwrap_or(0.0);
        if c.has_tag_name("cornerRoad") {
            let (s, t) = (attr_f64(c, "s")? + shift_s, attr_f64(c, "t")? + shift_t);
            if !road.on_road(s) {
                return None;
            }
            spans(s);
            let (mut base, _) = road.surface(s, t);
            base.z += attr_f64(c, "dz").unwrap_or(0.0) as f32;
            let corner = Corner {
                base,
                top: base + Vector::Z * height as f32,
            };
            Some((corner, normal(road.axes(s))))
        } else {
            let frame = frame?;
            spans(s);
            let (u, v) = (attr_f64(c, "u")?, attr_f64(c, "v")?);
            let z = attr_f64(c, "z").unwrap_or(0.0);
            let corner = Corner {
                base: frame.point([u, v, z]),
                top: frame.point([u, v, z + height]),
            };
            Some((corner, normal(frame.axes)))
        }
    };
    let (corners, ups): (Vec<Corner>, Vec<Vector>) = corner_nodes(node)
        .map(corner)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .unzip();
    if corners.len() < 2 {
        return None;
    }
    Some(Ring {
        corners,
        ups,
        // Closed unless the file says otherwise, as libOpenDRIVE reads it.
        closed: node.attribute("closed") != Some("false"),
        stretch,
    })
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
    outline
        .children()
        .filter(|n| n.has_tag_name("cornerRoad") || n.has_tag_name("cornerLocal"))
}

/// The `<marking>` elements under `object`'s `<markings>`.
fn marking_nodes<'a>(
    object: roxmltree::Node<'a, 'a>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    child(object, "markings")
        .into_iter()
        .flat_map(|m| m.children())
        .filter(|n| n.has_tag_name("marking"))
}

/// The `<marking>`s under `object`'s `<markings>` that run along edges of
/// one of its outlines, `outline`, placed as `ring`.
///
/// A marking follows the corners its `<cornerReference>`s name, in order, by
/// the corners' `id`s. It belongs to the outline that has every one of them,
/// so one naming a corner the outline lacks is left to another outline. So
/// is one with fewer than two references.
fn markings(
    object: roxmltree::Node,
    outline: roxmltree::Node,
    ring: &Ring,
    too_many: &Cell<bool>,
) -> Vec<Marking> {
    let ids: Vec<Option<&str>> = corner_nodes(outline).map(|c| c.attribute("id")).collect();
    marking_nodes(object)
        .filter_map(|m| paint(m, &corner_path(m, &ids, ring)?, too_many))
        .collect()
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
) -> Vec<Marking> {
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
fn paint(m: roxmltree::Node, path: &[(Point, Vector)], too_many: &Cell<bool>) -> Option<Marking> {
    let width = attr_f64(m, "width").filter(|w| *w > 0.0)?;
    let raise = attr_f64(m, "zOffset").unwrap_or(0.005) as f32;
    let path: Vec<(Point, Vector)> = path.iter().map(|&(p, up)| (p + up * raise, up)).collect();
    let line = attr_f64(m, "lineLength").unwrap_or(0.0).max(0.0);
    let space = attr_f64(m, "spaceLength").unwrap_or(0.0).max(0.0);
    let text = |name| m.attribute(name).unwrap_or_default().to_string();
    Some(Marking {
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
    })
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
    let ids: Vec<Option<&str>> = corner_nodes(outline).map(|c| c.attribute("id")).collect();
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
            let path = if b.attribute("useCompleteOutline") == Some("true") {
                let mut path = ring.path();
                if closed {
                    path.push(path[0]);
                }
                path
            } else {
                corner_path(b, &ids, ring)?
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
/// with their normals. `ids` are the corners' `id`s, in step with `ring`'s.
/// `None` if a reference names no corner in `ids`, or there are fewer than
/// two.
fn corner_path(
    node: roxmltree::Node,
    ids: &[Option<&str>],
    ring: &Ring,
) -> Option<Vec<(Point, Vector)>> {
    let path = node
        .children()
        .filter(|n| n.has_tag_name("cornerReference"))
        .map(|r| {
            let id = r.attribute("id")?;
            let at = ids.iter().position(|c| *c == Some(id))?;
            Some((ring.corners[at].base, ring.ups[at]))
        })
        .collect::<Option<Vec<_>>>()?;
    (path.len() >= 2).then_some(path)
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
        _ => ObjectType::Unknown,
    }
}

fn child<'a>(node: roxmltree::Node<'a, 'a>, tag: &str) -> Option<roxmltree::Node<'a, 'a>> {
    node.children().find(|n| n.has_tag_name(tag))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(kind(2), Some(LaneType::Special1));
        assert_eq!(kind(4), Some(LaneType::Unknown), "banana is not a type");
        // Every lane in the section, with nothing dropped for its type.
        assert_eq!(net.lanes().len(), 11);
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
        // A border lies in it, and paint 5 mm off it along its normal.
        let xml = banked_road_with(
            r#"<object id="a" s="5" t="-1">
                 <outlines><outline id="0">
                   <cornerRoad id="1" s="2" t="-1" dz="0"/><cornerRoad id="2" s="8" t="-1" dz="0"/>
                   <cornerRoad id="3" s="8" t="-3" dz="0"/></outline></outlines>
                 <markings><marking width="0.4"><cornerReference id="1"/><cornerReference id="2"/></marking></markings>
                 <borders><border width="1" useCompleteOutline="true"/></borders>
               </object>
               <object id="b" s="12" t="-2" hdg="0.5">
                 <outlines><outline><cornerLocal u="0" v="0"/><cornerLocal u="3" v="0"/>
                   <cornerLocal u="3" v="2"/></outline></outlines>
                 <borders><border width="1" useCompleteOutline="true"/></borders>
               </object>
               <object id="c" s="16" t="-2" length="2" width="1" height="1">
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
    fn an_object_rides_the_banked_surface_like_a_lane() {
        // Same pivot as a lane at t = 3: up by 3 sin 0.1, in to 3 cos 0.1.
        // Then zOffset raises it square to the banked surface, and it leans
        // with the bank, as libOpenDRIVE has it.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="10" t="3" zOffset="0.5" roll="0.05"/>"#,
        ));
        let (p, pitch, roll, _) = solid(&objects[0]);
        let (sin, cos) = 0.1_f32.sin_cos();
        assert!((p.x - 10.0).abs() < 1e-5, "x {}", p.x);
        assert!((p.y - (3.0 * cos - 0.5 * sin)).abs() < 1e-5, "y {}", p.y);
        assert!((p.z - (3.0 * sin + 0.5 * cos)).abs() < 1e-5, "z {}", p.z);
        assert!(pitch.abs() < 1e-6, "pitch {pitch}");
        assert!((roll - 0.15).abs() < 1e-6, "roll {roll}");
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
        // file has a square.
        let objects = objects_of(&banked_road_with(
            r#"<object id="1" s="5" t="0"><outlines><outline>
                 <cornerRoad s="18" t="0"/><cornerRoad s="22" t="0"/>
                 <cornerRoad s="22" t="2"/><cornerRoad s="18" t="2"/>
               </outline></outlines></object>"#,
        ));
        assert!(objects.is_empty());
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
            junction: None,
            length: 10.0,
            rule: TrafficRule::RightHand,
            geoms: vec![GeomRec::new(0.0, 0.0, 0.0, 0.4, 10.0, GeomShape::Line)],
            elevations: vec![cubic(0.0, 0.08)],
            superelevations: vec![cubic(-0.2, 0.0)],
            lateral: Lateral::default(),
            lane_offsets: Vec::new(),
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
