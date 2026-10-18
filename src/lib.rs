#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
//! A pure-Rust OpenDRIVE (`.xodr`) importer.
//!
//! It works out OpenDRIVE's curves and profiles once, at load, and returns a
//! [`RoadNetwork`] of lanes made of points. From there you sample points,
//! walk the lane graph, and mesh the road surface.
//!
//! No C++, no bindings, no `unsafe`, and no math crate in the public API.
//!
//! # Coordinate types
//!
//! A position is a [`Point`] and a direction is a [`Vector`]. `point - point`
//! is a `Vector`, `point + vector` is a `Point`, and adding two points doesn't
//! compile. Both are `#[repr(C)]` with three `f32` fields.
//! [`Point::to_array`] hands one to another math library.
//!
//! ```no_run
//! let net = xodr::load_file("maps/town07.xodr")?;
//! let lane = net.driving_lanes().next().expect("a driving lane");
//! let pose = lane.center.pose_at(25.0);
//! let route = net.route(pose.position, lane.center.point_at(400.0));
//! # Ok::<(), xodr::ImportError>(())
//! ```
//!
//! # Which OpenDRIVE version
//!
//! Any version loads. `revMajor` and `revMinor` are kept in
//! [`RoadNetwork::header`] and change nothing. A file that uses an element or
//! attribute from 1.8 or 1.9 but declares an older version raises
//! [`Warning::NewerThanDeclared`]. The crate reads the element anyway, since
//! the header is what is wrong.
//!
//! Two elements read are not in 1.9:
//!
//! - a lane's `<visibility>`, read as CARLA does. See
//!   [Lane visibility](#lane-visibility).
//! - a road's `<neighbor>`, from 1.4, as a [`RoadNeighbor`] in
//!   [`RoadNetwork::road_neighbors`]. The crate derives no lane changes from
//!   it. One naming no baked road, or with a `side` or `direction` 1.4
//!   doesn't allow, is dropped with a [`Warning::NeighborDropped`].
//!
//! Deprecated elements, `poly3` and a signal's `<positionRoad>` and
//! `<positionInertial>`, read like any other. The tests load real files
//! declaring 1.4, 1.6 and 1.7.
//!
//! # Which elements
//!
//! | Element | Attributes read |
//! | --- | --- |
//! | `<header>` | `revMajor`, `revMinor`, `name`, `version`, `date`, `vendor`, `north`, `south`, `east`, `west` |
//! | `<header><geoReference>` | its text |
//! | `<header><offset>` | `x`, `y`, `z`, `hdg` |
//! | `<include>`, anywhere | `file` |
//! | `<userData>`, anywhere | `code`, `value` |
//! | `<dataQuality>`, anywhere | none |
//! | `<dataQuality><error>` | `xyAbsolute`, `xyRelative`, `zAbsolute`, `zRelative` |
//! | `<dataQuality><rawData>` | `date`, `source`, `sourceComment`, `postProcessing`, `postProcessingComment` |
//! | `<header><license>` | `name`, `resource`, `spdxid`, `text` |
//! | `<road>` | `id`, `name`, `length`, `junction`, `rule` |
//! | `<road><link>` | `elementType`, `elementId`, `contactPoint`, `elementS`, `elementDir` |
//! | `<link><neighbor>` | `side`, `elementId`, `direction` |
//! | `<road><type>` | `s`, `type`, `country` |
//! | `<type><speed>` | `max`, `unit` |
//! | `<planView><geometry>` | `s`, `x`, `y`, `hdg`, `length` |
//! | `<line>` | none |
//! | `<arc>` | `curvature` |
//! | `<spiral>` | `curvStart`, `curvEnd` |
//! | `<poly3>` | `a`, `b`, `c`, `d` |
//! | `<paramPoly3>` | `aU`, `bU`, `cU`, `dU`, `aV`, `bV`, `cV`, `dV`, `pRange` |
//! | `<elevationProfile><elevation>` | `s`, `a`, `b`, `c`, `d` |
//! | `<lateralProfile><superelevation>` | `s`, `a`, `b`, `c`, `d` |
//! | `<lateralProfile><shape>` | `s`, `t`, `a`, `b`, `c`, `d` |
//! | `<lateralProfile><crossfall>`, from 1.4 | `s`, `side`, `a`, `b`, `c`, `d` |
//! | `<lateralProfile><crossSectionSurface>` | none |
//! | `<crossSectionSurface><tOffset>` | none |
//! | `<crossSectionSurface><surfaceStrips>` | none |
//! | `<surfaceStrips><strip>` | `id`, `mode` |
//! | `<strip><width>`, `<strip><constant>`, `<strip><linear>`, `<strip><quadratic>`, `<strip><cubic>` | none |
//! | `<coefficients>`, under each of those and `<tOffset>` | `s`, `a`, `b`, `c`, `d` |
//! | `<lanes><laneOffset>` | `s`, `a`, `b`, `c`, `d` |
//! | `<lanes>` | `layer` |
//! | `<laneSection>` | `s`, `length`, `singleSide` |
//! | `<left>`, `<center>`, `<right>` | none |
//! | `<lane>` | `id`, `type`, `level`, `direction`, `advisory`, `dynamicLaneType`, `dynamicLaneDirection`, `roadWorks` |
//! | `<center><lane>` | `id`, `type`, `level` |
//! | `<lane><width>` | `sOffset`, `a`, `b`, `c`, `d` |
//! | `<lane><border>` | `sOffset`, `a`, `b`, `c`, `d` |
//! | `<lane><height>` | `sOffset`, `inner`, `outer` |
//! | `<lane><speed>` | `sOffset`, `max`, `unit` |
//! | `<lane><rule>` | `sOffset`, `value` |
//! | `<lane><access>` | `sOffset`, `rule`, `restriction` |
//! | `<access><restriction>` | `type` |
//! | `<lane><material>` | `sOffset`, `surface`, `friction`, `roughness` |
//! | `<lane><visibility>` | `sOffset`, `forward`, `back`, `left`, `right` |
//! | `<lane><roadMark>` | `sOffset`, `type`, `weight`, `color`, `material`, `width`, `height`, `laneChange` |
//! | `<roadMark><type>` | `name`, `width` |
//! | `<type><line>` | `length`, `space`, `tOffset`, `sOffset`, `rule`, `width`, `color` |
//! | `<roadMark><explicit>` | none |
//! | `<explicit><line>` | `length`, `tOffset`, `sOffset`, `rule`, `width` |
//! | `<roadMark><sway>` | `ds`, `a`, `b`, `c`, `d` |
//! | `<lane><link>` | `id`, `layer` |
//! | `<junction>` | `id`, `name`, `type`, `mainRoad`, `sStart`, `sEnd`, `orientation` |
//! | `<connection>` | `id`, `type`, `incomingRoad`, `connectingRoad`, `linkedRoad`, `contactPoint` |
//! | `<connection><predecessor>`, `<connection><successor>` | `elementType`, `elementId`, `elementS`, `elementDir`, `contactPoint` |
//! | `<laneLink>` | `from`, `to`, `overlapZone`, `fromLayer`, `toLayer` |
//! | `<junction><priority>` | `high`, `low` |
//! | `<road><railroad>` | none |
//! | `<railroad><switch>` | `id`, `name`, `position` |
//! | `<switch><mainTrack>`, `<switch><sideTrack>` | `id`, `s`, `dir` |
//! | `<switch><partner>` | `id`, `name` |
//! | `<station>` | `id`, `name`, `type` |
//! | `<station><platform>` | `id`, `name` |
//! | `<platform><segment>` | `roadId`, `sStart`, `sEnd`, `side` |
//! | `<junctionGroup>` | `id`, `name`, `type` |
//! | `<junctionGroup><junctionReference>` | `junction` |
//! | `<junction><crossPath>` | `id`, `crossingRoad`, `roadAtStart`, `roadAtEnd` |
//! | `<junction><roadSection>` | `id`, `roadId`, `sStart`, `sEnd` |
//! | `<crossPath><startLaneLink>` | `s`, `from`, `to` |
//! | `<crossPath><endLaneLink>` | `s`, `from`, `to` |
//! | `<junction><planView>` | its `<geometry>`s, as a road's |
//! | `<junction><objects>` | its `<object>`s and `<objectReference>`s, as a road's |
//! | `<junction><boundary>` | none |
//! | `<boundary><segment>` | `type`, `roadId`, `boundaryLane`, `sStart`, `sEnd`, `contactPoint`, `jointLaneStart`, `jointLaneEnd`, `transitionLength` |
//! | `<junction><elevationGrid>` | `sStart`, `gridSpacing` |
//! | `<elevationGrid><elevation>` | `left`, `center`, `right` |
//! | `<objects><object>` | `id`, `type`, `subtype`, `name`, `dynamic`, `invalidated`, `temporary`, `orientation`, `validLength`, `s`, `t`, `zOffset`, `hdg`, `pitch`, `roll`, `perpToRoad`, `length`, `width`, `height`, `radius` |
//! | `<objects><objectReference>` | `id`, `s`, `t`, `zOffset`, `orientation`, `validLength` |
//! | `<objects><tunnel>` | `id`, `name`, `type`, `s`, `length`, `lighting`, `daylight` |
//! | `<objects><bridge>` | `id`, `name`, `type`, `s`, `length` |
//! | `<signals><signal>` | `id`, `name`, `dynamic`, `orientation`, `s`, `t`, `zOffset`, `hOffset`, `pitch`, `roll`, `country`, `countryRevision`, `type`, `subtype`, `value`, `unit`, `text`, `length`, `width`, `height`, `invalidated`, `temporary` |
//! | `<signal><positionRoad>` | `roadId`, `s`, `t`, `zOffset`, `hOffset`, `pitch`, `roll` |
//! | `<signal><positionInertial>` | `x`, `y`, `z`, `hdg`, `pitch`, `roll` |
//! | `<signal><dependency>`, `<staticBoard><dependency>`, `<vmsBoard><dependency>`, `<sign><dependency>`, `<displayArea><dependency>` | `id`, `type` |
//! | `<signal><reference>`, `<staticBoard><reference>`, `<vmsBoard><reference>`, `<sign><reference>` | `elementType`, `elementId`, `type` |
//! | `<sign><positionRoad>`, `<sign><positionInertial>` | as a signal's |
//! | `<signals><signalReference>` | `id`, `s`, `t`, `orientation` |
//! | `<signal><semantics>`, `<sign><semantics>`, `<roadRegulations><semantics>`, `<signalRegulations><semantics>` | none |
//! | `<semantics><speed>`, `<semantics><supplementaryDistance>` | `type`, `value`, `unit` |
//! | `<semantics><supplementaryTime>` | `type`, `value` |
//! | `<semantics><lane>`, `<semantics><priority>`, `<semantics><supplementaryEnvironment>` | `type` |
//! | `<semantics><prohibited>`, `<semantics><supplementaryAllows>`, `<semantics><supplementaryProhibits>` | none |
//! | `<animal>`, `<person>`, `<vehicle>` | none |
//! | `<person><type>`, `<vehicle><type>` | its text |
//! | `<semantics><warning>`, `<semantics><routing>`, `<semantics><streetname>`, `<semantics><parking>`, `<semantics><tourist>`, `<semantics><supplementaryExplanatory>` | none |
//! | `<signal><staticBoard>` | none |
//! | `<staticBoard><sign>` | `id`, `name`, `country`, `countryRevision`, `type`, `subtype`, `value`, `unit`, `text`, `orientation`, `dynamic`, `invalidated`, `temporary`, `hOffset`, `pitch`, `roll`, `length`, `width`, `height`, `v`, `z` |
//! | `<signal><vmsBoard>` | `displayType`, `displayWidth`, `displayHeight`, `v`, `z` |
//! | `<vmsBoard><displayArea>` | `index`, `width`, `height`, `v`, `z` |
//! | `<controller>`, at the top level | `id`, `name`, `sequence` |
//! | `<controller><control>` | `signalId`, `type` |
//! | `<junction><controller>` | `id`, `type`, `sequence` |
//! | `<vmsGroup>` | `id` |
//! | `<header><defaultRegulations>` | none |
//! | `<defaultRegulations><roadRegulations>` | `type`, and its `<semantics>` as a signal's |
//! | `<defaultRegulations><signalRegulations>` | `type`, `subtype`, and its `<semantics>` as a signal's |
//! | `<vmsGroup><vmsBoardReference>` | `signalId`, `vmsIndex`, `groupIndex` |
//! | `<validity>`, under `<object>`, `<objectReference>`, `<signal>`, `<signalReference>`, `<staticBoard>`, `<vmsBoard>`, `<sign>`, `<displayArea>`, `<tunnel>` and `<bridge>` | `fromLane`, `toLane`, `layer` |
//! | `<object><parkingSpace>` | `access`, `restrictions` |
//! | `<object><material>` | `surface`, `friction`, `roughness`, `roadMarkColor` |
//! | `<object><userData>` | `code`, `value` |
//! | `<object><repeat>` | `s`, `length`, `distance`, `bT`, `cT`, `dT`, `detachFromReferenceLine`, and the `Start`/`End` pair of `t`, `zOffset`, `length`, `width`, `height`, `radius` |
//! | `<outlines><outline>` | `id`, `closed`, `outer`, `fillType`, `laneType` |
//! | `<cornerRoad>` | `id`, `s`, `t`, `dz`, `height` |
//! | `<cornerLocal>` | `id`, `u`, `v`, `z`, `height` |
//! | `<skeleton><polyline>` | `id` |
//! | `<vertexRoad>` | `id`, `s`, `t`, `dz`, `radius`, `intersectionPoint` |
//! | `<vertexLocal>` | `id`, `u`, `v`, `z`, `radius`, `intersectionPoint` |
//! | `<curveLocal>` | `id`, `u`, `v`, `z`, `height`, `hdg`, `length`, and a `<line>`, `<arc>` or `<paramPoly3>` as in the plan view |
//! | `<markings><marking>`, `<outline><markings><marking>` | `side`, `color`, `weight`, `width`, `zOffset`, `lineLength`, `spaceLength`, `startOffset`, `stopOffset` |
//! | `<borders><border>` | `type`, `width`, `outlineId`, `useCompleteOutline` |
//! | `<cornerReference>` | `id` |
//! | `<road><surface><CRG>` | `file`, `mode`, `purpose`, `orientation`, `sStart`, `sEnd`, `sOffset`, `tOffset`, `hOffset`, `xOffset`, `yOffset`, `zOffset`, `zScale` |
//! | `<junction><surface><CRG>` | `file`, `mode`, `purpose`, `xOffset`, `yOffset`, `hOffset`, `zOffset`, `zScale` |
//! | `<object><surface><CRG>` | `file`, `hideRoadSurfaceCRG`, `zScale` |
//!
//! Eight attribute values steer the import. Any other value falls back as
//! shown:
//!
//! - `<road rule>`: `LHT`, else right-hand traffic. See
//!   [Coordinate frame](#coordinate-frame).
//! - `<junction type>`: `direct`, `virtual`, `crossing`, else common. A direct
//!   junction leads into `linkedRoad`, a common one into `connectingRoad`. See
//!   [Virtual junctions](#virtual-junctions).
//! - `<lane type>`: the [`LaneType`], else [`LaneType::Unknown`]. No lane is
//!   dropped for its type.
//! - `<link elementType>`: `junction`, else road.
//! - `contactPoint`: `end`, else start.
//! - `<paramPoly3 pRange>`: `arcLength`, any case, else normalized.
//! - `<laneSection singleSide>`: `true` or `1`, else every side.
//! - `<lanes layer>`: `temporary`, else permanent. See
//!   [Lane layers](#lane-layers).
//!
//! Links and junctions resolve into a lane graph in the direction traffic
//! drives. A successor is a lane you can drive into from this lane's exit end.
//! A lane section under 1 mm long, which many exporters write at a road's end,
//! bakes no lanes, and links step over it. A `<laneSection>` without `s` starts
//! at 0.
//!
//! A `<laneSection singleSide="true">` gives only the sides it has elements
//! for. The spec doesn't say what the other sides do. The crate carries them on
//! from the section before, so every section has both sides. A carried lane
//! keeps its records and their `sOffset`s, and each copy links to the next.
//!
//! A direct junction joins roads end to end. The spec doesn't say whether
//! traffic also flows back from the linked road. The crate links each
//! connection both ways unless the junction gives the way back itself. A
//! connection missing its `incomingRoad` or target road is dropped with a
//! [`Warning::ConnectionDropped`].
//!
//! A `<junctionGroup>`, such as a roundabout's junctions, is a
//! [`JunctionGroup`] in [`RoadNetwork::junction_groups`].
//! [`RoadNetwork::junction_groups_of`] gives every group of a road's
//! [`Road::junction`], since a junction may be in several. The router ignores
//! groups.
//!
//! - A reference to a missing junction is dropped with a
//!   [`Warning::JunctionReferenceDropped`]. A repeated one is dropped silently.
//! - A missing or unknown `type` is [`JunctionGroupKind::Unknown`], with a
//!   [`Warning::UnknownJunctionGroupType`].
//!
//! A `<crossPath>` is a pedestrian path across a junction's roads. Each is a
//! [`CrossPath`] in [`RoadNetwork::cross_paths`]: the crossing road, and at
//! each end the lane it joins, its `s`, and the crossing road's lane. It joins
//! lanes part way along, which [`Lane::successors`] can't, so it sits beside
//! the lane graph. One naming a missing road or lane, or an `s` off the road,
//! is dropped with a [`Warning::CrossPathDropped`].
//!
//! A `<junction type="crossing">` is where roads cross without joining, such
//! as a road and a railway. Each is a [`Crossing`] in
//! [`RoadNetwork::crossings`], with a [`CrossingSection`] per `<roadSection>`:
//! the road, its stretch from `sStart` to `sEnd`, and each lane's part as a
//! [`Coverage`]. An `sEnd` before `sStart` reads as the stretch between them.
//! A section naming a missing road, or an end off it, is dropped with a
//! [`Warning::RoadSectionDropped`].
//!
//! A junction's `<priority>`s say which road gives way to which. Each is a
//! [`Priority`] in [`RoadNetwork::priorities`], and
//! [`RoadNetwork::yields_to`] reads them for one road. One naming a road the
//! load didn't bake is dropped with a [`Warning::PriorityDropped`].
//!
//! # Virtual junctions
//!
//! A `<junction type="virtual">` joins roads to a main road part way along it,
//! such as a driveway, without cutting it. Each is a [`VirtualJunction`] in
//! [`RoadNetwork::virtual_junctions`], with its [`MainRoad`] and stretch, its
//! [`Orientation`], and a [`VirtualLink`] where a road meets another part way
//! along. A link comes from a road link with an `elementS`, or a deprecated
//! `<connection type="virtual">`. Each side is a [`LinkPoint`]: a road end, or
//! an `s` and a direction.
//!
//! - The router doesn't follow these links. A connection whose incoming road is
//!   the main road adds no lane-graph link. Other connections read as in a
//!   common junction.
//! - Outside a virtual junction, a road link's `elementS` is ignored and its
//!   `contactPoint` used, as ASAM's `UC_ParamPoly3` needs.
//! - With no main road, a main road not in the load, or `sStart` or `sEnd` off
//!   it, the junction keeps its links but has no [`MainRoad`], with a
//!   [`Warning::VirtualJunctionWithoutMainRoad`]. The spec's own example does
//!   the last.
//! - A link naming a missing road, an `s` off it, or an `elementDir` other than
//!   `+` or `-` is dropped with a [`Warning::VirtualLinkDropped`].
//!
//! The spec requires every road into a common junction to name it in its
//! `<link>`. Some files don't, such as ASAM's `UC_X_Junction`. The crate then
//! finds the end from the connecting road's link back, at the end the
//! connection's `contactPoint` names, and raises a
//! [`Warning::JunctionLinkMissing`]. A link back without a `contactPoint`, or
//! to an end already linked elsewhere, is not linked.
//!
//! Each `<object>` bakes to one or more [`Object`]s in world coordinates, on
//! the road surface. An object stands upright: `hdg` turns it from the road's
//! heading, `pitch` and `roll` from the horizontal, and `zOffset` raises it
//! straight up. With `perpToRoad="true"` it takes the road's axes at its
//! origin instead, turned by `hdg` and raised along the normal, and ignores
//! `pitch` and `roll`.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The schema gives `pitch` and `roll` against the horizontal. Figure 20
//!   gives them in the road's tilted frame. The crate follows the schema.
//! - The spec says a `perpToRoad` object is perpendicular to the road at all
//!   points. The crate turns the whole object by the road's axes at its origin,
//!   so a large outline doesn't bend.
//!
//! `<repeat>` sweeps and `<cornerRoad>` corners are in road coordinates and
//! rise straight up. `<object type>` gives the [`ObjectType`], else
//! [`ObjectType::Unknown`]. The [`Shape`]:
//!
//! - A plain object is a [`Shape::Solid`]. A `radius` makes its [`Extent`] a
//!   cylinder. `length`, `width` or `height` make a box, 0 where not given.
//! - A `<repeat>` with a `distance` makes a [`Shape::Solid`] every `distance`
//!   metres. With a `distance` of 0 it's a [`Shape::Sweep`] along the road,
//!   such as a guard rail. A `radius` makes the sweep a round pipe.
//! - A repeat's `t` runs straight from `tStart` to `tEnd`. With `bT`, `cT` or
//!   `dT` it's the cubic `tStart + bT ds + cT ds² + dT ds³`, and `tEnd` is
//!   ignored. Where the cubic ends over 1 mm from `tEnd`, the crate raises a
//!   [`Warning::RepeatEndMismatch`]. The schema forbids negative coefficients.
//!   The crate departs from it and reads them as written.
//! - With `detachFromReferenceLine="true"`, a repeat runs on the straight line
//!   between the ground at its two ends. Copies stand on the line. `hdg` and
//!   `perpToRoad` still take the road's heading and axes at each `s`, since the
//!   spec detaches only positions. `<cornerRoad>`s move as if not detached.
//! - Each `<outline>` is a [`Shape::Outline`], read under `<outlines>` or, as
//!   in 1.4, straight under `<object>`. An object with outlines has no solid.
//! - A `<skeleton>` is a [`Shape::Skeleton`] of `<polyline>`s, and replaces the
//!   solid too. `<vertexRoad>` and `<vertexLocal>` place as the matching
//!   corners do. [`RoadNetwork::object_mesh`] draws each segment as a frustum,
//!   mitred at bends up to about 140 degrees and squared beyond. A polyline
//!   with an unplaceable vertex, or under two, is dropped with a
//!   [`Warning::SkeletonDropped`]. An object left with nothing keeps its box. A
//!   missing or negative radius is 0. The spec's text gives a vertex a width
//!   and height, but its schema only a `radius`. The crate reads the schema.
//! - A `<curveLocal>` is a line, arc or paramPoly3 in the object's frame. Arcs
//!   and paramPoly3s are cut into edges of about 25 cm, at most 1024. Without a
//!   `length`, a curve runs to the next corner. A line without `hdg` heads for
//!   the next corner, and other curves carry on the previous curve's heading.
//!   The spec says each curve ends where the next starts:
//!   - where a curve's `hdg` leaves a gap that the previous heading would
//!     close, the crate uses that heading and raises a
//!     [`Warning::CurveHeadingIgnored`]. ASAM's
//!     `Ex_SmoothObjectOutline_traffic_island` needs this;
//!   - where a gap over 1 cm remains, the crate bridges it with a straight edge
//!     and raises a [`Warning::OutlineGap`];
//!   - a paramPoly3 with `pRange="arcLength"` and no `length` drops its
//!     outline.
//! - Under a `<repeat>` with a `distance`, outlines bake at every step.
//!   `<cornerLocal>`s follow the object's frame, `<cornerRoad>`s shift with the
//!   step. The repeat's dimensions don't resize them.
//! - An `outer="false"` outline is a hole in the first closed outer outline of
//!   the object that encloses it. With a `fillType` or `laneType` it is also a
//!   [`Shape::Outline`] of its own. A plain one no outline encloses is
//!   dropped.
//!
//! A junction's `<objects>` stand along its `<planView>`, at `z` 0 plus
//! `zOffset`, since that line has no elevation. [`Object::lanes`] is empty, a
//! `<surface>` CRG covers the junction's lanes, and
//! [`ObjectProvenance::junction_id`] names the junction. The spec allows that
//! line one `<line>`. The crate departs from it and bakes any road geometry.
//! Without one, the objects are dropped with a
//! [`Warning::JunctionObjectsDropped`].
//!
//! An `<objectReference>` bakes the object it names as if it stood at the
//! reference's `s`, `t` and `zOffset`. Repeats and `<cornerRoad>`s move with
//! it. A reference to a missing id is skipped.
//!
//! [`Object::lanes`] is the lanes along the object's stretch, narrowed to its
//! `<validity>` ranges. A reference uses its own validities.
//!
//! A `<marking>` becomes a [`Marking`]: a strip `width` wide along the edges
//! through the corners it names, dashed by `lineLength` and `spaceLength`. One
//! under an `<outline>`, as 1.9 writes it, belongs to that outline. One under
//! the object, as older versions write it, goes on the outline that has every
//! corner it names. A reference to a missing corner is skipped. With no corner
//! references it paints its `side` of a solid's box.
//!
//! A `<border>` becomes a [`Border`], a band `width` wide, on the outline its
//! `outlineId` names or every outline it fits. It follows the whole outline
//! with `useCompleteOutline`, else the edges through its corner references.
//!
//! Both lie on the surface under the edges: the road's for `<cornerRoad>`, the
//! object's plane otherwise, raised along the normal.
//!
//! A closed outline with `fillType="paint"` is a painted area, such as an
//! arrow. The crate lays it flat, 5 mm above its corners. The spec says a
//! border cuts back a filled area. The crate departs from it and keeps the
//! full area. An open outline's `fillType` does nothing.
//!
//! `<parkingSpace>` becomes a [`ParkingSpace`], each `<material>` a
//! [`Material`], and each `<userData>` a [`UserData`].
//!
//! An [`Object`] keeps its `subtype` and whether it's `dynamic`. Its road id,
//! id, `(s, t)`, `orientation` and `validLength` are in its
//! [`ObjectProvenance`], from [`load_str_with_provenance`] or
//! [`load_file_with_provenance`], as [`LaneProvenance`] does for lanes.
//! [`ObjectProvenance::referenced_from`] names a reference's original.
//! [`RoadNetwork::object_mesh`] meshes them. Markings and borders aren't in
//! it. They're already quads in [`Marking::pieces`] and [`Border::pieces`].
//!
//! Each `<tunnel>` and `<bridge>` is a [`Structure`] with no geometry. A
//! [`Coverage`] gives where it starts and ends on each lane, narrowed by its
//! `<validity>`s. [`RoadNetwork::structures_over`] finds them for a lane.
//! [`StructureProvenance`] has its road id, id, `s` and `length`.
//!
//! Each `<signal>` is a [`Signal`] in [`RoadNetwork::signals`]. It keeps
//! `country`, `type`, `subtype`, `value`, `unit` and `text` as written, and
//! looks nothing up. Its board stands `zOffset` above the road at its `(s, t)`,
//! which is [`Signal::applies_at`]. A `+` signal faces traffic along `+s`,
//! `-` or `none` the other way. `hOffset` turns it counter-clockwise. `pitch`
//! and `roll` are against the horizontal, so a board stays upright on a bank.
//! Angles wrap into `(-π, π]`.
//!
//! A `<positionRoad>` or `<positionInertial>` moves only the board. The signal
//! still applies at its own `(s, t)`. One naming no road, or missing a
//! coordinate, leaves the board in place.
//!
//! [`Signal::lanes`] is the lanes at `s` whose traffic runs the way
//! `orientation` says: along `+s` for `+`, against for `-`, all for `none`.
//! Two-way lanes are in both. Reversed and left-hand-traffic lanes get the
//! signals of their traffic, not their side. A `<validity>` replaces these
//! lanes. The spec says it must lie inside them, but esmini's maps don't, so
//! the crate departs from it. [`SignalProvenance`] has the road id, id,
//! `(s, t)` and `orientation`. A signal without `s` or `t`, or off its road, is
//! skipped.
//!
//! A `<signalReference>` applies the signal on its own road too. Its lanes and
//! point join [`Signal::lanes`] and [`Signal::applies_at`], and
//! [`SignalProvenance::references`] records it. With duplicate ids the first
//! signal wins. A reference to a missing id is skipped.
//!
//! `<dependency>`s become [`Signal::dependencies`], kept as written, since 1.7
//! and 1.9 disagree on which way they point. `<reference>`s become
//! [`Signal::references`] to a signal or the [`Object`]s an object baked to.
//! One to a missing id, or another `elementType`, is skipped.
//!
//! `<semantics>` become [`Signal::semantics`], a [`Semantic`] each, such as a
//! maximum speed or the [`RoadUser`]s barred. The crate applies only the speed
//! semantic, to [`RoadNetwork::speed_limits`]. See
//! [Speed limits and road types](#speed-limits-and-road-types). Unknown
//! children are skipped.
//!
//! A `<staticBoard>` or `<vmsBoard>` makes the signal a board in
//! [`Signal::boards`]. A static board has a [`BoardSign`] per `<sign>`, and a
//! message board its display and [`DisplayArea`]s. Each stands where its `v`
//! and `z` put it on the board.
//!
//! A sign's own `<validity>`, `<dependency>`s and `<reference>`s override its
//! board's, and a board's override the signal's. [`BoardSign::lanes`],
//! [`BoardSign::dependencies`] and [`BoardSign::references`] take the nearest.
//! A sign with its own `<positionInertial>` or `<positionRoad>` stands there. A
//! sign's dependency may name another sign on its board, a
//! [`SignTarget::Sign`]. A link to nothing baked is dropped with a
//! [`Warning::SignalLinkDropped`]. The crate departs from the spec here:
//!
//! - A sign's own validity applies where its signal stands, not where a
//!   `<signalReference>` applies it. The spec doesn't say.
//! - The spec's text gives a `<displayArea>` a `<validity>` and dependencies,
//!   though its schema doesn't. The crate reads them as [`DisplayArea::lanes`]
//!   and [`DisplayArea::dependencies`], spelled `<dependency>` as for a signal,
//!   not `<signalDependency>` as in the spec's example.
//!
//! Each top-level `<controller>` is a [`Controller`] in
//! [`RoadNetwork::controllers`], listed in each of its signals'
//! [`Signal::controllers`]. A `<control>` naming no signal is skipped. A
//! junction's `<controller>` list is in each controller's
//! [`ControllerProvenance`].
//!
//! Each `<vmsGroup>` is a [`VmsGroup`] in [`RoadNetwork::vms_groups`]: the
//! message boards on one gantry, with their `vmsIndex` and `groupIndex`. A
//! reference to no placed signal is dropped with a
//! [`Warning::SignalLinkDropped`].
//!
//! # Junction areas
//!
//! A junction's `<boundary>` and `<elevationGrid>` give the ground between and
//! around its roads. Each junction with either is a [`JunctionArea`] in
//! [`RoadNetwork::junction_areas`].
//!
//! [`JunctionArea::boundary`] is a closed ring on the road surface. A `lane`
//! segment runs along its `boundaryLane`'s outer border from `sStart` to
//! `sEnd`, or along the center line for lane 0. A `joint` segment runs across
//! its road at `contactPoint`, from `jointLaneStart` to `jointLaneEnd`.
//!
//! [`JunctionArea::grid`] is a grid of heights along and across the
//! junction's `<planView>`. [`JunctionArea::height_at`] reads it with the
//! spec's bicubic interpolation. Slopes come from the cubic through four grid
//! points, or a straight line where there are fewer, so the ground can rise
//! between equal heights. [`JunctionArea::mesh`] triangulates the ground
//! inside the boundary at the grid's spacing.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The grid overrides the elevation of the junction's roads. The crate
//!   stands every road of the junction on it: elevation, superelevation and
//!   shape give way to the grid, and lane heights add to it. Where the grid
//!   doesn't reach, which the spec forbids, the road's own elevation holds.
//!   This applies to any junction type, though the spec gives grids to common
//!   junctions only.
//! - A `joint` with a `transitionLength` blends the incoming road into the
//!   grid with the spec's cubic. The spec divides the slopes by the zone's
//!   depth. The crate multiplies, which makes the cubic meet both slopes, and
//!   reads the spec as a typo. The road's height at the joint excludes lane
//!   heights. A joint without a `lane` segment each side gets no blend, with a
//!   [`Warning::TransitionDropped`].
//! - Segments must close the boundary. The crate joins them straight, with a
//!   [`Warning::BoundaryNotClosed`] where two ends are over 10 cm apart. A
//!   segment naming a missing road or lane is dropped with a
//!   [`Warning::BoundarySegmentDropped`].
//! - Segments run counter-clockwise. A clockwise boundary is reversed, with a
//!   [`Warning::BoundaryClockwise`].
//! - A joint runs from `jointLaneStart` to `jointLaneEnd`. Where that leaves a
//!   gap, the crate reverses it, as it does a lane segment.
//! - A missing `sStart` is 0. A row without `center` is skipped.
//! - The spec allows any `<planView>` geometry. The crate reads a grid only
//!   along one straight `<line>` with a `gridSpacing` above 0, else drops it
//!   with a [`Warning::ElevationGridDropped`].
//! - The spec's height formula swaps rows and columns between its matrix and
//!   its product of powers. The crate follows the product, with `s` along the
//!   rows.
//!
//! # Railways
//!
//! Each railway `<switch>` is a [`Switch`] in [`RoadNetwork::switches`]: where
//! it sits on the main track, where the side track leaves, its setting, and
//! its [`Partner`]. Each point is a [`TrackPoint`]. Like a cross path, a switch
//! sits beside the lane graph.
//!
//! Each `<station>` is a [`Station`] in [`RoadNetwork::stations`], with its
//! platforms and the track each runs beside. A switch or platform segment
//! naming a missing road, an `s` off it, an `sEnd` before `sStart`, or a
//! `position`, `dir` or `side` the spec doesn't allow, is dropped with a
//! [`Warning::RailwayDropped`].
//!
//! # Lane layers
//!
//! A road has a permanent lane layer and may have a temporary one, as for road
//! works. Each is a `<lanes>` with its own sections and `<laneOffset>`.
//! [`Lane::layer`] gives a lane's layer. Section indexes count permanent
//! sections first, as in [`LaneProvenance::section`]. A temporary section's
//! `length` ends it early. A second `<lanes>` of one [`Layer`] raises a
//! [`Warning::Ignored`]. Temporary lanes have their own speed limits, rules,
//! access, materials and visibility.
//!
//! - Only the permanent layer defines the surface.
//!   [`RoadNetwork::surface_mesh`] and [`RoadNetwork::road_point`] follow it.
//!   [`RoadNetwork::temporary_mesh`] has the temporary lanes, to draw on top.
//!   A temporary lane past the permanent surface isn't clipped.
//! - A lane `<link>` may name lanes on either layer, so traffic moves onto the
//!   temporary layer and back. A junction `<laneLink>` uses `fromLayer` and
//!   `toLayer`.
//! - A `<validity layer>` picks that layer's lanes, one without picks the
//!   permanent layer. An object or signal with no `<validity>` applies to both.
//! - Temporary lanes take precedence for driving, as the spec says.
//!   [`RoadNetwork::nearest_lane`], [`RoadNetwork::lane_position`],
//!   [`RoadNetwork::route`] and [`RoadNetwork::advance`] find a temporary
//!   driving lane that holds the point. A temporary lane traffic may not use
//!   hides the lane under it from [`RoadNetwork::nearest_lane`]. Each has a
//!   `_with` form taking a [`LayerPolicy`]: [`LayerPolicy::Both`], the
//!   default, or [`LayerPolicy::PermanentOnly`].
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - A link with no `layer` means the permanent layer. The crate keeps it on
//!   the lane's own layer if that layer has a next section, so a temporary
//!   layer's sections join.
//! - A road with only a temporary `<lanes>` bakes it as the permanent layer.
//! - A permanent `<laneSection>`'s `length`, which the spec forbids, is
//!   ignored.
//!
//! # Lane borders
//!
//! A lane's `<border>`s give the `t` of its outer border, measured from the
//! reference line. Its width is that less the inner neighbour's outer border,
//! so a lane can open out of nothing. [`Lane::width`] and [`Lane::widths`]
//! follow. A width lane outside a border lane stacks on it.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - Widths and borders are exclusive, and widths win in a section with both.
//!   The crate reads each lane on its own: widths if it has any, else borders.
//!   A lane with borders in a section with widths raises a
//!   [`Warning::WidthAndBorder`].
//! - Borders are forbidden with a `<laneOffset>`. The crate ignores the offset
//!   for borders, with a [`Warning::BorderWithLaneOffset`].
//! - A border inside the lanes within it gives the lane 0 width, with a
//!   [`Warning::BorderCrossesInnerLane`].
//! - A record without `a` is skipped. A missing `sOffset`, `b`, `c` or `d` is
//!   0. Records out of order are sorted.
//!
//! # Lateral shapes
//!
//! A road's `<shape>`s give its cross-section, such as a crown. The shapes at
//! one `s` form a profile of cubics in `t`. Between profiles, height varies
//! linearly in `s`, along the road's normal.
//!
//! Each lane border stands on the shape plus the lane's `<height>`, and the
//! lane is straight between its borders. [`Lane::bank`] takes the slope. The
//! mesh stays closed. A crown that breaks on a border is exact. A curve inside
//! a lane is lost: `c w² / 4` for a parabola `c t²`, 5.5 mm on a 3.5 m lane of
//! a 2.5 % crown. Objects, signals and marks stand on the shape as meshed.
//! [`LaneProvenance::heights`] gives the `<height>`s only.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The default shape is 0, so the road would step up to the first profile.
//!   The crate holds the first profile before it and the last after it.
//! - Each profile covers the road. Where one starts inside the right edge, the
//!   first shape's value holds out to the edge, with a
//!   [`Warning::ShapeShortOfRoad`].
//! - The surface curves between borders. The crate's is straight, as above.
//! - A lane height includes the shape, but an `attached` CRG adds to the road
//!   without it.
//! - A shape without `s` or `t` is skipped. A missing `a`, `b`, `c` or `d` is
//!   0. Shapes out of order are sorted.
//!
//! A `<crossfall>`, from OpenDRIVE 1.4, folds in as a shape does. The road
//! falls from its reference line at an angle, a cubic in `s`, on the `side` it
//! names: height `-tan(angle) * |t|`. Any `side` other than `left` or `right`
//! means both. A crossfall and shapes add.
//!
//! # Cross-section surfaces
//!
//! A `<crossSectionSurface>` is another way to give the cross-section: up to
//! two strips each side, shifted by a `<tOffset>`. A strip's height is a cubic
//! in `dt`, whose coefficients `<constant>`, `<linear>`, `<quadratic>` and
//! `<cubic>` are cubics in `s`. An inner strip, `id` 1 or -1, reaches its
//! `<width>`. An outer strip, 2 or -2, runs on to the road's edge, on the inner
//! strip's edge for `mode="relative"` or the reference plane for
//! `independent`.
//!
//! It bakes as [Lateral shapes](#lateral-shapes) do. Level lanes keep out of
//! it. Where the two sides differ at `dt` 0, each lane uses its own side.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - A surface with `<shape>`s or `<superelevation>` is forbidden. The crate
//!   adds them, with a [`Warning::CrossSectionWithShape`].
//! - A missing or unknown outer `mode` is `independent`, the unknown one with
//!   a [`Warning::UnknownStripMode`].
//! - A strip with another `id`, a repeated `id`, or an outer strip without an
//!   inner one of known width is dropped with a [`Warning::StripDropped`].
//! - Past an inner strip with no outer strip, the inner one runs on. A side
//!   with no strip is flat.
//! - The spec's left-side rule reads `t_effective < w_left` for the outer
//!   strip. The crate reads `>`, mirroring the right side.
//!
//! # Level lanes
//!
//! A `level="true"` lane is kept out of superelevation and shape. It runs
//! level from its inner neighbour's outer border, so its [`Lane::bank`] is 0.
//! Its `<height>`s add on top. It's built from the tilted cross-section, so in
//! plan it is `w / cos φ` wide: 2.5 mm more on a 2 m lane at 5 %.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - Every lane outside a level lane must be level. The crate holds it level
//!   anyway, with a [`Warning::LaneNotLevel`].
//! - A level lane starts at its neighbour's border height, including the
//!   neighbour's `<height>`.
//!
//! # Lane heights
//!
//! A lane's `<height>`s raise it off the road, as a sidewalk stands above the
//! lanes beside it. Each gives the height at the inner and outer border from
//! its `sOffset`. The centerline rises by the mean, and [`Lane::bank`] takes
//! the slope, so the mesh and [`Lane::sample_at`] see the raised surface.
//! [`LaneProvenance::heights`] gives them per centerline vertex. A change of
//! pace gets a station, so a 1 m ramp bakes 1 m long.
//!
//! The mesh tilts a raised lane's cross axis rather than moving each edge, so
//! on a grade each edge shifts along the road by half the rise times the
//! grade: 1.5 mm for 0.1 m at 3 %. Where the two borders ramp at different
//! paces, a quad twists and misses the surface by up to a quarter of the
//! rise: 25 mm halfway up a 0.1 m ramp. The step up to a raised lane is left
//! open.
//!
//! Objects, signals and marks stand on the lane under them. A kerb mark
//! belongs to the lane below, so it stays on the road. A `perpToRoad` object
//! leans with the road, not the lane.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - Heights hold until the next. The crate ramps linearly between them, as
//!   esmini's maps expect, and holds the last after it.
//! - The first height also holds before it.
//! - A missing `sOffset`, `inner` or `outer` is 0, and a negative `sOffset` is
//!   0.
//! - Heights out of order are sorted.
//! - Heights on the center lane, which the spec forbids, are ignored.
//! - The spec doesn't say what stands on a raised lane. The crate stands
//!   objects, signals and marks on it.
//! - A point on the border between two lanes is on the inner one.
//! - Past the outermost lane, a point takes that lane's outer height.
//! - A height includes the surface, but an `attached` CRG adds to the road
//!   without it.
//!
//! # Road marks
//!
//! Each `<roadMark>` is a [`RoadMark`] in [`RoadNetwork::road_marks`], along
//! its lane's outer border from its `sOffset` to the next mark or the
//! section's end. The center lane's run along the center line.
//! [`RoadMark::left`] and [`RoadMark::right`] name the lanes either side,
//! looking along `+s`. `type` is a [`RoadMarkType`], `laneChange` a
//! [`LaneChange`], and `color` stays text. [`RoadMarkProvenance`] has its
//! road, section, lane, `s` and length.
//!
//! [`RoadMark::lines`] are quads on the lane mesh. Each `<type><line>` or
//! `<explicit><line>` is a [`RoadMarkLine`] with a width, colour,
//! [`LinePattern`] and [`LineRule`], offset by `tOffset` from the border. A
//! `<type><line>` repeats, an `<explicit><line>` paints once. A mark with no
//! lines gets stand-ins: one line for `solid` and `broken`, two for double
//! types. `<sway>`s shift the lines sideways. A mark at the same `sOffset` as
//! the next is replaced by it, with a [`Warning::RoadMarkDropped`].
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - A missing `color`, `type` or `sOffset` is `standard`, `none` or 0. An
//!   unknown `type` is [`RoadMarkType::Unknown`].
//! - A negative `sOffset` is 0.
//! - A missing `weight` is standard.
//! - A width of 0 counts as none, so the next applies: line, type, mark, then
//!   weight. esmini writes 0 on all three.
//! - A line with a `length` and a `space` of 0 is continuous, not empty.
//!   Otherwise every mark in esmini's `multi_intersections.xodr` vanishes.
//! - A `none` mark paints nothing, even with lines.
//! - `tOffset` and `<sway>` move along +t on either side. Nothing sways before
//!   the first `ds`.
//! - A missing `rule` is [`LineRule::None`].
//! - A line is 0.12 m wide, 0.25 m bold. `broken` is 4 m dashes with 8 m gaps.
//!   Double lines sit one width either side of the border. `none`, `edge`,
//!   `grass`, `curb`, `botts dots` and `custom` paint nothing.
//! - Dash lengths are measured along the reference line.
//! - Marks out of order are sorted.
//! - A lane with no `<width>` or `<border>` is dropped with its marks, with a
//!   [`Warning::LaneDropped`].
//!
//! # Speed limits and road types
//!
//! [`RoadNetwork::speed_limits`] and [`RoadNetwork::road_types`] are [`Along`]
//! stretches along each lane's centerline. [`RoadNetwork::speed_limit_at`] and
//! [`RoadNetwork::road_type_at`] read them at a point. A limit is in m/s, or
//! [`SpeedLimit::Unlimited`]. Where the map says nothing, there's no stretch.
//!
//! - A road's `<type>`, and its `<speed>`, hold from its `s` to the next or the
//!   road's end. Its `country` is in [`RoadNetwork::road_countries`].
//!   `max="undefined"` sets nothing.
//! - A lane's `<speed>` holds from its `sOffset` to the next or the section's
//!   end, and overrides the road's.
//! - A `<speed>` without `unit` is in m/s.
//! - [`Header::default_regulations`] holds `<defaultRegulations>`. The first
//!   `maximum` speed of a road type's `<roadRegulations>` applies where the
//!   road's `<type>` and lanes give none. A road with no `<type>` gets none.
//!   `<signalRegulations>` are kept only. The crate also reads `subType`, as
//!   the spec's example spells it.
//! - A speed limit sign overrides the records, since the spec says limits from
//!   signals take preference. A `<speed>` semantic of type `maximum` or `zone`
//!   starts a limit, and `maximumEnd` or `zoneEnd` ends one. It runs from the
//!   sign, and each `<signalReference>`, along each lane it applies to in the
//!   direction of travel, to the next speed sign. Where it differs from the
//!   record it overrides, the crate raises a [`Warning::SpeedLimitDisagrees`].
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec doesn't say how far a sign reaches. The crate stops it at the
//!   next speed sign or the road's end, and not through a junction.
//! - Files before 1.8 have no `<semantics>`, so their signs set no limit. The
//!   crate doesn't read limits from country sign codes.
//! - A `zone` speed is a maximum until `zoneEnd`. `minimum` and `recommended`
//!   set nothing.
//! - A two-way lane takes the direction its sign faces.
//! - A `<speed>` outside the spec's values or units, or too large for an
//!   `f32`, is dropped with a [`Warning::SpeedLimitDropped`]. The road's or the
//!   previous limit holds there.
//! - A `<type>` without `s` is skipped. A missing or negative `sOffset` is 0.
//!   Records out of order are sorted.
//! - An unknown `type` is [`RoadType::Unknown`].
//!
//! # Lane rules, access and materials
//!
//! [`RoadNetwork::lane_rules`], [`RoadNetwork::lane_access`] and
//! [`RoadNetwork::lane_materials`] are [`Along`] stretches of each lane's
//! `<rule>`s, `<access>`es and `<material>`s, with `*_at` methods. Each holds
//! from its `sOffset` to the next of its kind or the section's end.
//!
//! - A rule is free text. One without `value` is skipped. Rules at the same
//!   `sOffset` join with `"; "`.
//! - [`Access::Allow`] lists the only road users allowed, [`Access::Deny`]
//!   those barred. A lane with no access stretch is open to all. A `deny` of
//!   `none` lifts earlier restrictions.
//! - Files before 1.8 name one user per `<access>` in `restriction`. Entries
//!   at one `sOffset` merge. An `allow` and a `deny` merge into the `allow`,
//!   less the denied users.
//! - Of two materials, speeds or visibilities at one `sOffset`, the later
//!   wins.
//! - A material is a [`Material`]. A negative or overflowing friction or
//!   roughness reads as none. Its friction is unused. A CRG gives
//!   friction through [`RoadSurface`].
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - An `<access>` without `rule`, or with one other than `allow` or `deny`,
//!   is dropped with a [`Warning::AccessDropped`]. The lane is open up to the
//!   next.
//! - A missing or negative `sOffset` is 0. Entries out of order are sorted.
//!
//! # Lane visibility
//!
//! [`RoadNetwork::lane_visibility`] gives how far a driver can see from each
//! lane, as [`Along`] stretches of a [`Visibility`]: ahead, behind, left and
//! right, in metres. [`RoadNetwork::lane_visibility_at`] reads it at a point.
//! Each `<visibility>` holds from its `sOffset` to the next or the section's
//! end.
//!
//! OpenDRIVE 1.9 doesn't define `<visibility>`. The crate reads `sOffset`,
//! `forward`, `back`, `left` and `right`, as CARLA does:
//!
//! - The distances stay as named. Whether `forward` means traffic's way or
//!   `+s` is undefined.
//! - One with a distance missing, not a number, negative or too large for an
//!   `f32` is dropped with a [`Warning::VisibilityDropped`].
//! - A missing or negative `sOffset` is 0. Entries out of order are sorted.
//!
//! # Road surfaces
//!
//! Each road or junction `<surface><CRG>` is a [`CrgSurface`] in
//! [`RoadNetwork::crg_surfaces`]. The importer reads the record, not the file.
//! [`RoadSurface::new`] loads the files through your closure, and
//! [`RoadSurface::sample`] gives height, normal and friction under a point in
//! `f64`. Off the CRG, the surface mesh answers. The CRG doesn't change the
//! mesh.
//!
//! Where several records of one purpose cover a point, the last wins, per rule
//! `road.crg.use_last_entry`. Junction records come after road records, so
//! they win.
//!
//! Modes:
//!
//! - `attached` lays the grid along the road and adds its height. The spec's
//!   formula includes the file's reference line height, slope and bank, but
//!   its text says to disregard them. The crate follows the text.
//! - `attached0` lays it the same way, replacing the road's height.
//! - `genuine` starts the file's reference line at `(sOffset, tOffset)`,
//!   turned by `hOffset`.
//! - `global` keeps the file's coordinates, moved by `xOffset`, `yOffset` and
//!   `hOffset`.
//!
//! Along the road, `u = s - sOffset` and `v = t - tOffset`. The reference line
//! is sampled every 0.25 m and joined with arcs. That is exact on lines and
//! arcs, and within a micrometre on a spiral reaching a 50 m radius in 50 m.
//!
//! `orientation="opposite"` negates `u` and `v`. The spec's matrix also swaps
//! them, contradicting its text. The crate follows the text.
//!
//! A road CRG covers `sStart` to `sEnd`. A junction CRG covers the lanes of
//! the junction's roads and must be `global`. Other modes are skipped there.
//!
//! `purpose` is `elevation` unless it says `friction`. Any other is dropped
//! with a [`Warning::CrgDropped`]. A friction CRG ignores `zOffset` and
//! `zScale`, and skips OpenCRG's height shift, giving the file's values as is.
//!
//! An object's `<surface>` CRG is a [`CrgMode::Object`] record per box. Its
//! `(u, v)` are the object's own. Inside the box it adds `zScale` times the
//! file's height to the surface below and tilts the normal. With
//! `hideRoadSurfaceCRG`, true by default, it hides an `attached` road CRG
//! there. `zOffset` and `height` play no part. It covers its own road's lanes,
//! or a junction object's junction roads.
//!
//! The crate departs from the spec here:
//!
//! - The spec turns the file with the object. The crate uses the heading
//!   alone, so a pitched or rolled object's file lies flat.
//! - `hideRoadSurfaceCRG` over another mode is forbidden. The crate keeps the
//!   road CRG, adds the object's, and raises a [`Warning::CrgHideIgnored`].
//! - A surface on anything but a box raises a
//!   [`Warning::ObjectSurfaceDropped`].
//!
//! # Included files
//!
//! An `<include>` is replaced by the children of the included file's root,
//! which must match the `<include>`'s parent. Nested includes are read too.
//!
//! - [`load_file`] reads from the map's folder or below, and refuses paths out
//!   of it, links followed.
//! - [`load_str_with`] reads through your function.
//! - [`load_str`] reads none.
//!
//! Includes nest at most 8 deep and total at most [`MAX_INCLUDED_BYTES`]. One
//! that can't be read is left out with a [`Warning::IncludeDropped`], whose
//! [`IncludeProblem`] says why.
//!
//! # What the importer ignores
//!
//! The crate raises one [`Warning::Ignored`] per feature below in each road,
//! junction, and the rest of the document, with a count.
//! [`Warning::is_ignored`] tells these from faults.
//!
//! - a second `lanes` of one [`Layer`] on a road. The first is kept;
//! - a `roadSection` of a junction that isn't a crossing.
//!
//! The center lane, lane 0, never becomes a [`Lane`]. Its road marks are read,
//! and its `id`, `type`, `level` and `link` are kept as a
//! [`CenterLaneProvenance`] in [`Provenance::center_lanes`]. The lane graph
//! doesn't follow its link.
//!
//! # Kept for the caller
//!
//! These are kept but build nothing:
//!
//! - every `<userData>` and `<dataQuality>`, on any element, as an
//!   [`AdditionalData`] in [`Provenance::additional`] with its element's path
//!   and road. Nothing inside a `<userData>` is read. An object's is also in
//!   [`Object::user_data`];
//! - the `<header>` in [`RoadNetwork::header`], with its `<license>` in
//!   [`Header::license`];
//! - a road's `name` in [`Road::name`], a junction's in
//!   [`Provenance::junctions`] as a [`JunctionProvenance`];
//! - a road type's `country`, in [`RoadNetwork::road_countries`];
//! - a road mark's `material` and type `name`, in [`RoadMarkProvenance`];
//! - a lane's `advisory` as a [`LaneAdvisory`], `dynamicLaneType`,
//!   `dynamicLaneDirection` and `roadWorks`, in [`LaneProvenance`];
//! - the center lane's attributes, as above;
//! - an object's `invalidated`, `temporary`, marking `weight`, material
//!   `roadMarkColor`, outline `fillType` and `laneType`, and [`SkeletonIds`],
//!   in [`ObjectProvenance`]. `fillType="paint"` also lays the outline flat;
//! - a sign's own attributes, as a [`SignProvenance`] in
//!   [`SignalProvenance::signs`];
//! - a switch's [`Partner`];
//! - a junction `<laneLink>`'s `overlapZone`, as a [`LaneLinkOverlap`] in
//!   [`Provenance::lane_link_overlaps`].
//!
//! # Coordinate frame
//!
//! OpenDRIVE's own frame: right-handed, **Z up, metres**, the reference line
//! in the X-Y plane and elevation along +Z. A coordinate in the `.xodr` is the
//! coordinate you get back. A left turn curves toward +Y, and `t` is positive
//! to the left. A Y-up engine rotates on the way in.
//!
//! Traffic follows each road's `rule`. With right-hand traffic, the default,
//! negative (right) lanes run along `+s`. With `rule="LHT"` the left lanes do.
//! Links, lane changes and a signal's `orientation` follow. Any value other
//! than `RHT` or `LHT` reads as `RHT`, with a [`Warning::UnknownTrafficRule`].
//!
//! A lane's `direction` overrides its side. `reversed` runs it against its
//! side, and `both` makes it a [`Direction::Both`] lane. A deprecated
//! `type="bidirectional"` lane without `direction` is two-way too. Any other
//! value reads as `standard`, with a [`Warning::UnknownLaneDirection`].
//!
//! Two joining two-way lanes lead into each other both ways, even where only
//! one names the other. A two-way lane's successors are the lanes off either
//! end that run away from it. The lane graph links lane changes only between
//! lanes running the same way. [`RoadNetwork::left_of`] and
//! [`RoadNetwork::may_change_left`] don't check this.
//! [`RoadNetwork::route`] drives a two-way lane toward the next lane on the
//! route, and [`RoadNetwork::advance`] goes both ways.
//!
//! # Road coordinates
//!
//! [`RoadNetwork::roads`] keeps each baked road. A [`RoadPosition`] is a road,
//! `s` along its reference line and `t` across it.
//! [`RoadNetwork::road_point`] turns one into a point on the surface, and
//! [`RoadNetwork::road_position`] turns a point back.
//! [`RoadNetwork::road_lane`] gives a lane's road, section and `<lane id>`.
//! [`RoadNetwork::road_by_od_id`] finds a [`Road`] by its `<road id>`.
//!
//! ```no_run
//! use xodr::{load_file, RoadPosition};
//!
//! let net = load_file("maps/town07.xodr")?;
//! let road = net.road_by_od_id("5").expect("road 5").id();
//! let point = net.road_point(RoadPosition { road, s: 10.0, t: -1.75 });
//! let back = point.and_then(|p| net.road_position(p));
//! # Ok::<(), xodr::ImportError>(())
//! ```
//!
//! The surface includes elevation, superelevation, shape and lane height. `t`
//! runs along the tilted cross-section, so on a bank `φ` it is `t cos φ` in
//! plan. Signals, objects and marks are placed through the same call. The
//! forward call is exact to `f32`.
//!
//! - The inverse takes the road whose surface is nearest in 3D, so a point on
//!   a bridge finds the bridge. Where roads overlap in a junction, any may come
//!   back. [`RoadNetwork::road_position_on`] picks the road.
//! - A point off every road gets the nearest road's `(s, t)`, with `s` clamped.
//! - Round trips agree to 0.2 mm on the test maps, or the `f32` step where
//!   that's larger: 0.5 m in UTM coordinates.
//! - At a section seam where `<height>` steps, the seam belongs to the section
//!   starting there.
//! - A network from [`RoadNetwork::new`] has no roads, so road queries answer
//!   `None`.
//! - Where a road bends tighter than its lanes' half-width, a point can have
//!   two road positions. The crate returns one.
//!
//! # Lane positions
//!
//! A [`LanePosition`] is a lane, a road `s`, and an offset from the lane's
//! center toward `+t`. [`RoadNetwork::lane_point`] and
//! [`RoadNetwork::lane_position`] convert both ways.
//!
//! Two kinds of `s` exist. [`LanePosition`], [`RoadPosition`], signals and
//! objects use the reference line's. [`Projection`], [`Along`] stretches and
//! [`Lane::sample_at`] use distance along the baked centerline. On bends they
//! drift apart, up to 21 % on the test maps. [`RoadNetwork::centerline_s`]
//! converts.
//!
//! [`RoadNetwork::nearest_lane`] projects onto the baked centerline, sampled
//! at most 2 m and 0.05 rad apart. It is within 6 mm of the exact center at
//! the 95th percentile. [`RoadNetwork::lane_position`] is exact and about ten
//! times slower.
//!
//! # Moving along the lanes
//!
//! [`RoadNetwork::advance`] moves a [`LanePosition`] a distance along the
//! lanes, the way traffic runs, or back for a negative distance. It returns
//! one [`Advance`] per branch. A branch that runs out of lanes is an
//! [`Advance::DeadEnd`] with the distance left.
//!
//! ```no_run
//! # let net = xodr::load_file("maps/town07.xodr")?;
//! # let here = net.lane_position(xodr::Point::ORIGIN).expect("on a lane");
//! for branch in net.advance(here, 25.0) {
//!     let ahead = net.lane_point(branch.position());
//! }
//! # Ok::<(), xodr::ImportError>(())
//! ```
//!
//! The distance is along lane centerlines, as a vehicle travels. The offset
//! keeps its side of traffic, so it flips sign onto a road running the other
//! way.
//!
//! Branches multiply at every fork and don't merge. From one lane of Town07,
//! 2 km gives 72,912 places in about 150 ms, and each further 400 m about
//! seven times more. Nothing caps it. Step a few metres at a time and pick a
//! branch at each fork. An infinite or NaN distance gives none.
//!
//! [`RoadNetwork::left_of`] and [`RoadNetwork::right_of`] step to the lane
//! beside, relative to the lane's traffic.
//!
//! [`RoadNetwork::may_change_left`] and [`RoadNetwork::may_change_right`] read
//! the border mark's [`LaneChange`]. `increase` allows crossing toward the
//! higher `<lane id>`, `decrease` toward the lower. They answer `None` with no
//! lane beside, no mark, or an unknown `laneChange`. Where one mark ends and
//! the next begins, the next answers. `laneChange` applies whatever the mark's
//! type, so a `curb` with the default `both` allows crossing.
//!
//! # Geo reference
//!
//! [`RoadNetwork::geo_reference`] gives the `<geoReference>` PROJ string and
//! `<offset>` as written, and applies neither. To place the map on the earth,
//! apply the offset, then the projection.
//!
//! The spec rotates by the offset's `hdg`, then adds `x`, `y` and `z`.
//! `netconvert` writes the opposite direction, so check the sign against a
//! known point. A missing or unreadable offset value is 0. A blank
//! `<geoReference>` is none, meaning a local frame.
//!
//! # Robustness
//!
//! A road the importer can't read is skipped, not fatal. [`load_str`] errors
//! only if no lane comes out. Non-finite numbers such as `NaN` and `1e400` are
//! rejected as read, as is a number too large for the `f32` it's stored in.
//! A road over [`MAX_LENGTH`], or past [`MAX_BAKED_LENGTH`] of lanes in the
//! file, is skipped with a [`Warning::RoadSkipped`].
//!
//! [`Provenance::warnings`] says what the load did. Each [`Warning`] names
//! where it happened and prints as a sentence. The [`Warning`] enum documents
//! every kind. The crate prints nothing.
//!
//! Two drops raise no warning, since exporters write them and nothing is lost:
//! a lane section under 1 mm long inside its road, and a road mark at or past
//! its section's end.
//!
//! # Importer contract
//!
//! Code baking other map formats into a [`RoadNetwork`] must:
//!
//! - build lane geometry with [`Polyline::try_new`] and return an error on bad
//!   input, not call the panicking [`Polyline::new`];
//! - build contiguous lanes split from one curve with
//!   [`Polyline::try_new_with_tangents`], passing the exact end tangents, or
//!   the joint shows a seam;
//! - treat [`LaneId`]s as opaque, not as indexes.
//!
//! [`RoadNetwork::surface_mesh`] pulls in the inner edge on curves tighter
//! than the half-width, so the surface doesn't fold. The outer edge keeps its
//! width.

mod advance;
mod along;
mod coords;
mod crg;
mod geo;
mod geometry;
mod grid;
mod header;
mod junction;
mod mesh;
mod network;
mod object;
mod object_mesh;
mod parse;
mod railway;
mod road;
mod road_mark;
mod route;
mod signal;
mod structure;

#[cfg(test)]
mod fixtures;

pub use advance::Advance;
pub use along::{Access, Along, RoadType, SpeedLimit, Visibility};
pub use coords::{Point, Vector};
pub use crg::{
    CrgAlong, CrgMode, CrgObject, CrgPose, CrgPurpose, CrgSurface, RoadSurface, SurfaceHint,
    SurfaceSample,
};
pub use geo::{GeoOffset, GeoReference};
pub use geometry::TooFewPoints;
pub use geometry::{Polyline, Pose, Projection, RoadSample};
pub use header::{DefaultRegulations, Header, License, RoadRegulation, SignalRegulation};
pub use junction::{
    CrossPath, CrossPathEnd, Crossing, CrossingSection, ElevationGrid, GridRow, JunctionArea,
    JunctionGroup, JunctionGroupKind, LinkPoint, MainRoad, VirtualJunction, VirtualLink,
};
pub use mesh::{LaneSpan, Mesh, MeshError, MeshSampler};
pub use network::{
    Direction, Lane, LaneAdvisory, LaneId, LaneType, Layer, LayerPolicy, RoadNetwork,
};
pub use object::{
    Border, Corner, Extent, Marking, Material, Object, ObjectId, ObjectType, ParkingSpace, Section,
    Shape, SkeletonVertex, UserData,
};
pub use object_mesh::ObjectSpan;
/// The OpenCRG reader, for loading the files a [`RoadSurface`] evaluates.
pub use opencrg;
pub use parse::{
    load_file, load_file_with_provenance, load_str, load_str_with, load_str_with_provenance,
    Additional, AdditionalData, CenterLaneProvenance, ControllerProvenance, CrossPathProvenance,
    DataError, DataQuality, ImportError, IncludeProblem, JunctionControllerProvenance,
    JunctionProvenance, LaneHeight, LaneLinkOverlap, LaneProvenance, ObjectProvenance, Orientation,
    PriorityProvenance, Provenance, RawData, RoadEnd, RoadMarkProvenance, RoadSkipReason,
    SignProvenance, SignalProvenance, SignalReferenceProvenance, SkeletonIds, StructureProvenance,
    Warning, MAX_BAKED_LENGTH, MAX_INCLUDED_BYTES, MAX_LENGTH,
};
pub use railway::{
    Partner, Platform, PlatformSegment, Station, Switch, SwitchPosition, TrackPoint,
};
pub use road::{LanePosition, Priority, Road, RoadId, RoadLane, RoadNeighbor, RoadPosition, Side};
pub use road_mark::{
    LaneChange, LinePattern, LineRule, RoadMark, RoadMarkId, RoadMarkLine, RoadMarkType,
    RoadMarkWeight,
};
pub use signal::{
    BoardSign, Control, Controller, ControllerId, Dependency, DisplayArea, MessageBoard, Reference,
    Referenced, RoadUser, Semantic, SignDependency, SignTarget, Signal, SignalBoard, SignalId,
    Unit, VmsBoardReference, VmsGroup,
};
pub use structure::{Coverage, Structure, StructureId, StructureKind};
