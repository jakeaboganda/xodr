#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
//! A pure-Rust OpenDRIVE (`.xodr`) importer.
//!
//! OpenDRIVE describes roads with formulas: clothoids, arcs, cubic elevation
//! and width profiles, lane links. This crate works all of it out once, at
//! load, and hands back a [`RoadNetwork`] of lanes made of points. Code that
//! uses it never touches OpenDRIVE again. It samples points, walks the lane
//! graph, and builds a triangle mesh of the road surface.
//!
//! No C++ dependency, no bindings, no `unsafe`, and no math crate in the
//! public API.
//!
//! # Coordinate types
//!
//! A position is a [`Point`] and a direction or displacement is a [`Vector`].
//! They are separate types carrying the arithmetic that relates them:
//! `point - point` is a `Vector`, `point + vector` is a `Point`, and adding
//! two positions does not compile. One three-float type used for all three
//! roles would make `nearest_lane(sample.up)` legal, which it is not.
//!
//! Both are `#[repr(C)]` structs of three public `f32` fields, so handing one
//! to another math library is `Point::to_array`.
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
//! The importer keeps a `<header>`'s attributes in [`RoadNetwork::header`]
//! and its geo reference in [`RoadNetwork::geo_reference`]. It never inspects
//! `revMajor` or `revMinor` to
//! decide how to read the file, and it never rejects a file for its
//! version. Whether a file loads depends only on whether it uses the
//! elements below.
//!
//! Every one of those elements but two is in ASAM OpenDRIVE 1.9.0, the
//! current revision. `poly3`, and a signal's `<positionRoad>` and
//! `<positionInertial>`, are deprecated there, still specified, and still
//! read here. A lane's `<visibility>` is not in 1.9 at all. The crate reads
//! it from the older files that write it, as CARLA does. See
//! [Lane visibility](#lane-visibility). A road's `<neighbor>`, from 1.4, is
//! not in 1.9 either. libOpenDRIVE reads it too, as raw strings. The crate
//! resolves and checks it: each is a [`RoadNeighbor`] in
//! [`RoadNetwork::road_neighbors`], the road beside a road, on its left or
//! right, running the same way or the other. The crate
//! derives no lane changes from it. One naming no baked road, or with a
//! `side` or `direction` 1.4 doesn't allow, is dropped, with a
//! [`Warning::NeighborDropped`]. The test suite imports real files
//! declaring 1.4, 1.6 and 1.7.
//!
//! # Which elements
//!
//! | Element | Attributes read |
//! | --- | --- |
//! | `<header>` | `revMajor`, `revMinor`, `name`, `version`, `date`, `vendor`, `north`, `south`, `east`, `west` |
//! | `<header><geoReference>` | its text |
//! | `<header><offset>` | `x`, `y`, `z`, `hdg` |
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
//! | `<lateralProfile><crossSectionSurface>` | none |
//! | `<crossSectionSurface><tOffset>` | none |
//! | `<crossSectionSurface><surfaceStrips>` | none |
//! | `<surfaceStrips><strip>` | `id`, `mode` |
//! | `<strip><width>`, `<strip><constant>`, `<strip><linear>`, `<strip><quadratic>`, `<strip><cubic>` | none |
//! | `<coefficients>`, under each of those and `<tOffset>` | `s`, `a`, `b`, `c`, `d` |
//! | `<lanes><laneOffset>` | `s`, `a`, `b`, `c`, `d` |
//! | `<laneSection>` | `s` |
//! | `<left>`, `<center>`, `<right>` | none |
//! | `<lane>` | `id`, `type`, `level`, `direction`, `advisory`, `dynamicLaneType`, `dynamicLaneDirection`, `roadWorks` |
//! | `<center><lane>` | `type`, `level` |
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
//! | `<lane><link>` | `id` |
//! | `<junction>` | `id`, `name`, `type`, `mainRoad`, `sStart`, `sEnd`, `orientation` |
//! | `<connection>` | `id`, `type`, `incomingRoad`, `connectingRoad`, `linkedRoad`, `contactPoint` |
//! | `<connection><predecessor>`, `<connection><successor>` | `elementId`, `elementS`, `elementDir`, `contactPoint` |
//! | `<laneLink>` | `from`, `to`, `overlapZone` |
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
//! | `<crossPath><startLaneLink>` | `s`, `from`, `to` |
//! | `<crossPath><endLaneLink>` | `s`, `from`, `to` |
//! | `<junction><planView>` | its `<geometry>`'s `s`, `x`, `y`, `hdg` |
//! | `<junction><boundary>` | none |
//! | `<boundary><segment>` | `type`, `roadId`, `boundaryLane`, `sStart`, `sEnd`, `contactPoint`, `jointLaneStart`, `jointLaneEnd` |
//! | `<junction><elevationGrid>` | `sStart`, `gridSpacing` |
//! | `<elevationGrid><elevation>` | `left`, `center`, `right` |
//! | `<objects><object>` | `id`, `type`, `subtype`, `name`, `dynamic`, `invalidated`, `temporary`, `orientation`, `validLength`, `s`, `t`, `zOffset`, `hdg`, `pitch`, `roll`, `length`, `width`, `height`, `radius` |
//! | `<objects><objectReference>` | `id`, `s`, `t`, `zOffset`, `orientation`, `validLength` |
//! | `<objects><tunnel>` | `id`, `name`, `type`, `s`, `length`, `lighting`, `daylight` |
//! | `<objects><bridge>` | `id`, `name`, `type`, `s`, `length` |
//! | `<signals><signal>` | `id`, `name`, `dynamic`, `orientation`, `s`, `t`, `zOffset`, `hOffset`, `pitch`, `roll`, `country`, `countryRevision`, `type`, `subtype`, `value`, `unit`, `text`, `length`, `width`, `height`, `invalidated`, `temporary` |
//! | `<signal><positionRoad>` | `roadId`, `s`, `t`, `zOffset`, `hOffset`, `pitch`, `roll` |
//! | `<signal><positionInertial>` | `x`, `y`, `z`, `hdg`, `pitch`, `roll` |
//! | `<signal><dependency>` | `id`, `type` |
//! | `<signal><reference>` | `elementType`, `elementId`, `type` |
//! | `<signals><signalReference>` | `id`, `s`, `t`, `orientation` |
//! | `<signal><semantics>`, `<sign><semantics>` | none |
//! | `<semantics><speed>`, `<semantics><supplementaryDistance>` | `type`, `value`, `unit` |
//! | `<semantics><supplementaryTime>` | `type`, `value` |
//! | `<semantics><lane>`, `<semantics><priority>`, `<semantics><supplementaryEnvironment>` | `type` |
//! | `<semantics><prohibited>`, `<semantics><supplementaryAllows>`, `<semantics><supplementaryProhibits>` | none |
//! | `<animal>`, `<person>`, `<vehicle>` | none, and a `<type>`'s text |
//! | `<semantics><warning>`, `<semantics><routing>`, `<semantics><streetname>`, `<semantics><parking>`, `<semantics><tourist>`, `<semantics><supplementaryExplanatory>` | none |
//! | `<signal><staticBoard>` | none |
//! | `<staticBoard><sign>` | `id`, `name`, `country`, `countryRevision`, `type`, `subtype`, `value`, `unit`, `text`, `orientation`, `dynamic`, `invalidated`, `temporary`, `hOffset`, `pitch`, `roll`, `length`, `width`, `height`, `v`, `z` |
//! | `<signal><vmsBoard>` | `displayType`, `displayWidth`, `displayHeight`, `v`, `z` |
//! | `<vmsBoard><displayArea>` | `index`, `width`, `height`, `v`, `z` |
//! | `<controller>`, at the top level | `id`, `name`, `sequence` |
//! | `<controller><control>` | `signalId`, `type` |
//! | `<junction><controller>` | `id`, `type`, `sequence` |
//! | `<validity>`, under `<object>`, `<objectReference>`, `<signal>`, `<signalReference>`, `<tunnel>` and `<bridge>` | `fromLane`, `toLane` |
//! | `<object><parkingSpace>` | `access`, `restrictions` |
//! | `<object><material>` | `surface`, `friction`, `roughness`, `roadMarkColor` |
//! | `<object><userData>` | `code`, `value` |
//! | `<object><repeat>` | `s`, `length`, `distance`, and the `Start`/`End` pair of `t`, `zOffset`, `length`, `width`, `height`, `radius` |
//! | `<outlines><outline>` | `id`, `closed`, `outer`, `fillType`, `laneType` |
//! | `<cornerRoad>` | `id`, `s`, `t`, `dz`, `height` |
//! | `<cornerLocal>` | `id`, `u`, `v`, `z`, `height` |
//! | `<markings><marking>` | `side`, `color`, `weight`, `width`, `zOffset`, `lineLength`, `spaceLength`, `startOffset`, `stopOffset` |
//! | `<borders><border>` | `type`, `width`, `outlineId`, `useCompleteOutline` |
//! | `<cornerReference>` | `id` |
//! | `<road><surface><CRG>` | `file`, `mode`, `purpose`, `orientation`, `sStart`, `sEnd`, `sOffset`, `tOffset`, `hOffset`, `xOffset`, `yOffset`, `zOffset`, `zScale` |
//! | `<junction><surface><CRG>` | `file`, `mode`, `purpose`, `xOffset`, `yOffset`, `hOffset`, `zOffset`, `zScale` |
//!
//! Six attribute values steer the import:
//!
//! - `<road rule>` is `LHT` for left-hand traffic, or right-hand traffic
//!   for `RHT` or none. See [Coordinate frame](#coordinate-frame).
//! - `<junction type>` is `direct`, `virtual`, or a common junction for any
//!   other value. A direct junction's connections lead into their
//!   `linkedRoad`, and a common junction's into their `connectingRoad`. See
//!   [Virtual junctions](#virtual-junctions).
//! - `<lane type>` chooses the [`LaneType`] a lane bakes as. A name this
//!   crate does not recognise bakes as [`LaneType::Unknown`], so no lane is
//!   ever dropped for its type.
//! - `<link elementType>` is `junction`, or a road for any other value.
//! - `contactPoint` is `end`, or the start for any other value.
//! - `<paramPoly3 pRange>` is `arcLength`, matched without case, or
//!   normalized for any other value.
//!
//! `<link>`s and `<junction>`s resolve into a drive-direction lane graph.
//! "Successor" means "a lane you can drive into off this lane's exit end",
//! not a raw mirror of the file's `+s` links. A lane section shorter than a
//! millimetre, such as the one many exporters write at a road's end, bakes
//! no lanes, and links step over it to the section beyond. A
//! `<laneSection>` without the `s` the spec requires starts at 0, as
//! libOpenDRIVE reads it, so a section that also starts at 0 has no length.
//!
//! A direct junction joins roads end to end, with no connecting road
//! between them. Each `<laneLink>` there says which lane of the linked road
//! carries on from a lane of the incoming road. The spec does not say
//! whether traffic also crosses from the linked road back into the
//! incoming one. The crate reads each connection both ways, as esmini
//! does, unless the junction gives the way back itself. So a two-way road
//! through a direct junction links in both directions. A connection
//! missing its `incomingRoad`, or the road it leads into, is dropped with a
//! [`Warning::ConnectionDropped`].
//!
//! A `<junctionGroup>` groups junctions that routing should see as one, such
//! as the junctions round a roundabout. Each is a [`JunctionGroup`] in
//! [`RoadNetwork::junction_groups`], with the `<junction id>`s it names, and
//! [`RoadNetwork::junction_groups_of`] finds a road's through its
//! [`Road::junction`]: every group, since the spec lets a junction be in
//! more than one. The router does not use them. A reference to a junction
//! the file lacks is left out, with a [`Warning::JunctionReferenceDropped`],
//! and one repeating a junction the group already names is left out too. A
//! `type` the spec does not allow, or a missing one, reads as
//! [`JunctionGroupKind::Unknown`], with a
//! [`Warning::UnknownJunctionGroupType`]. No other reader to compare
//! against reads them.
//!
//! A junction's `<crossPath>`s are paths for pedestrians across its roads,
//! such as a crosswalk. Each is a [`CrossPath`] in
//! [`RoadNetwork::cross_paths`]: the crossing road, and at each end the lane
//! of the road it starts from or ends at, the `s` along that road, and the
//! crossing road's lane there. The crossing road's lane at its start is in
//! its first lane section, and at its end in its last. A cross path joins
//! lanes part way along them, which [`Lane::successors`] can't, so it is
//! kept beside the lane graph. One naming a road the load lacks, a lane
//! that road does not have at its `s`, or an `s` off the road, is dropped
//! with a [`Warning::CrossPathDropped`]. No other reader to compare against
//! reads them.
//!
//! A junction's `<priority>`s say which of its roads gives way to which.
//! Each is a [`Priority`] in [`RoadNetwork::priorities`], and
//! [`RoadNetwork::yields_to`] reads them for one road. The crate reads
//! nothing else into them, such as the signs or the lanes' order. One
//! naming a road the load did not bake is dropped, with a
//! [`Warning::PriorityDropped`]. libOpenDRIVE stores them too.
//!
//! # Virtual junctions
//!
//! A `<junction type="virtual">` joins roads to a main road part way along
//! it, such as a driveway, without cutting it. Each is a
//! [`VirtualJunction`] in [`RoadNetwork::virtual_junctions`], with its
//! [`MainRoad`] and the stretch of it the junction spans, its
//! [`Orientation`], and a
//! [`VirtualLink`] for each place a road meets another part way along. A
//! link comes from a road of the junction whose `<predecessor>` or
//! `<successor>` gives an `elementS`, with the lane pairs its lanes'
//! `<link>`s and the junction's `<laneLink>`s give, or from a deprecated
//! `<connection type="virtual">`. Each side is a [`LinkPoint`]: an end of a
//! road, or an `s` along one and which way the link runs there.
//!
//! The lane graph joins lanes end to end, so these links are kept beside
//! it, and the router does not follow them. A connection whose incoming
//! road is the main road adds no lane-graph link either. The junction's
//! other connections, such as from a lot road into a connecting road that
//! ends on the main road, are read as in a common junction. A road link
//! with an `elementS` outside a virtual junction is read by its
//! `contactPoint`, since files such as ASAM's `UC_ParamPoly3` give one
//! there. A junction without its main road, one naming a road the load
//! lacks, or an `sStart` or `sEnd` off it, as the spec's own example of
//! virtual connections is, keeps its links with no [`MainRoad`], and raises
//! a [`Warning::VirtualJunctionWithoutMainRoad`]. A link naming a road the
//! load lacks, an `s` off it, or an `elementDir` other than `+` or `-` is
//! dropped, with a [`Warning::VirtualLinkDropped`]. esmini reads a virtual
//! junction as a
//! common one. No other reader to compare against reads `elementS`.
//!
//! A road's lanes drive into a common junction from the end whose road
//! `<link>` names it. The spec requires every incoming road to name its
//! junction so, but some files leave it out, as `UC_X_Junction` among
//! ASAM's examples does. Then the crate finds that end from the connecting
//! road's own link back to the incoming road, at the end of the connecting
//! road the connection's `contactPoint` names, and raises
//! [`Warning::JunctionLinkMissing`]. A link back with no `contactPoint`
//! names no end, and an end that already links elsewhere is left as it is,
//! so neither is linked. libOpenDRIVE skips such a connection
//! (`OpenDriveMap.cpp:830`).
//!
//! Each `<object>` bakes to one or more [`Object`]s in world coordinates,
//! sitting on the road surface. As in libOpenDRIVE, an object's own frame
//! follows the road under it: `hdg`, `pitch` and `roll` turn it against the
//! road's grade and bank, and `zOffset` raises it square to the surface. So
//! an object on a banked road leans with the bank. `<repeat>` sweeps and
//! `<cornerRoad>` corners are given in road coordinates, not the object's
//! frame, and still rise straight up. libOpenDRIVE raises a `<cornerRoad>`
//! along the road's normal instead, which differs on a banked road.
//! `<object type>`
//! chooses their [`ObjectType`], and an unrecognised name bakes as
//! [`ObjectType::Unknown`]. The [`Shape`] follows libOpenDRIVE:
//!
//! - A plain object is a [`Shape::Solid`] at its `(s, t)`. A `radius` makes
//!   its [`Extent`] a cylinder. Any of `length`, `width` and `height` makes it
//!   a box, 0 in the dimensions not given.
//! - A `<repeat>` with a `distance` is one [`Shape::Solid`] every `distance`
//!   metres. One with a `distance` of 0 is a [`Shape::Sweep`], its
//!   cross-section swept continuously along the road, such as a guard rail.
//!   A `radius` makes the sweep round, a pipe resting on its `zOffset`, and
//!   its `width` and `height` play no part.
//! - Each `<outline>` is a [`Shape::Outline`], and an object with outlines
//!   has no solid of its own. `<outline>` is read under `<outlines>`, and
//!   straight under `<object>` as OpenDRIVE 1.4 writes it.
//! - Under a `<repeat>` with a `distance`, the outlines are baked at every
//!   step in place of the solid. libOpenDRIVE bakes them once. At each step
//!   `<cornerLocal>` corners follow the object's frame, and `<cornerRoad>`
//!   corners move by the step's offset from the object's `(s, t)`. The
//!   repeat's dimensions do not resize them.
//! - An outline with `outer="false"` is a hole. It goes into the holes of
//!   the first closed outer outline of the same object that encloses it, and
//!   the importer drops it if there is none.
//!
//! An `<objectReference>` bakes the `<object>` it names, on whichever road
//! that is, as if it stood at the reference's `s` and `t` with the
//! reference's `zOffset`. What the object gives in road coordinates, its
//! repeats and `<cornerRoad>`s, moves by the same distance along and across
//! the road. A reference to an id no `<object>` has is skipped.
//!
//! [`Object::lanes`] is the lanes alongside the stretch of road an object
//! spans, the lane sections it stands in or sweeps across, narrowed to the
//! `fromLane`-`toLane` ranges of its `<validity>`s if it has any. A
//! reference's own `<validity>`s apply, not its `<object>`'s, whose lane ids
//! are on another road.
//!
//! A `<marking>` becomes a [`Marking`] on the outline that has every corner
//! its `<cornerReference>`s name. The marking is a strip `width` across,
//! centred on the edges through those corners, and cut into dashes by
//! `lineLength` and `spaceLength`. A marking with no corner references
//! paints the edge of its `side` of a solid's box, on its base: `front`,
//! `rear`, `left` or `right`. A cylinder's box is the square round it.
//!
//! A `<border>` becomes a [`Border`] on the outline its `outlineId` names, or
//! on every outline it fits if it names none. The border is a band `width`
//! across, centred on every edge with `useCompleteOutline`, and on the edges
//! through its `<cornerReference>`s otherwise.
//!
//! Both lie in the surface under the edges: the road's for `<cornerRoad>`
//! corners, and the plane of the object's frame for `<cornerLocal>` corners
//! and a solid's box. So on a banked road a band tilts with the bank rather
//! than lying level, and paint is raised off the road along its normal.
//!
//! An object's `<parkingSpace>` becomes its [`ParkingSpace`], and each
//! `<material>` one of its [`Material`]s. Each `<userData>` is a
//! [`UserData`] of its `code` and `value`. Any XML nested inside one is not
//! kept.
//!
//! An [`Object`] keeps what describes the thing itself: its `subtype`, and
//! whether it is `dynamic`. What ties it to an OpenDRIVE road, its road id,
//! `<object id>`, `(s, t)`, `orientation` and `validLength`, is in its
//! [`ObjectProvenance`], from [`load_str_with_provenance`] or
//! [`load_file_with_provenance`], the same way [`LaneProvenance`] names a
//! lane's road. For a referenced object, those are the reference's, and
//! [`ObjectProvenance::referenced_from`] names the road the original is on.
//! [`RoadNetwork::object_mesh`] turns them all into triangles. Markings and borders
//! are not in it: they are already quads, in [`Marking::pieces`] and
//! [`Border::pieces`].
//!
//! Each `<tunnel>` and `<bridge>` bakes to a [`Structure`] with no geometry,
//! since OpenDRIVE describes neither a tube nor a deck. It covers the lanes
//! alongside its `length` metres of road from `s`, narrowed by its
//! `<validity>`s. A [`Coverage`] says where it starts and ends along each
//! lane. [`RoadNetwork::structures_over`] tells you whether a lane runs
//! through a tunnel or over a bridge. Its road id, id, `s` and `length` are
//! in its [`StructureProvenance`].
//!
//! Each `<signal>` bakes to a [`Signal`] in [`RoadNetwork::signals`]. It
//! keeps the catalogue codes the file gives, `country`, `type`, `subtype`,
//! `value`, `unit` and `text`, and looks nothing up in a catalogue. Its board
//! stands `zOffset` straight above the road surface at its `(s, t)`, the
//! point in [`Signal::applies_at`]. A `+` signal faces back along the road,
//! toward traffic running along `+s`. A `-` or `none` signal faces along it.
//! `hOffset` turns it counter-clockwise from there. `pitch` and `roll` are
//! against the horizontal, as the spec has them, so unlike an object, a board
//! on a banked road stays upright. Angles wrap into `(-π, π]`, since real
//! files give `hOffset`s of several turns.
//!
//! A `<positionRoad>` stands the board on the road it names instead, the same
//! way, with its own `s`, `t`, `zOffset`, `hOffset`, `pitch` and `roll`, and
//! still facing the traffic the signal's `orientation` names. A
//! `<positionInertial>` stands it at `(x, y, z)`, facing `hdg`. Either moves
//! only the board. The signal still takes effect at its own `(s, t)`, and
//! applies to the lanes there. One that names no road, or is missing a
//! coordinate, leaves the board at the signal's station.
//!
//! [`Signal::lanes`] is the lanes at `s` whose traffic runs the way its
//! `orientation` names: with `+s` for `+`, against it for `-`, and every lane
//! for `none`. A lane with `direction="both"`, and a lane of the deprecated
//! `type="bidirectional"`, carries traffic both ways, so it is in either. A
//! reversed lane, and a lane on a road with `rule="LHT"`, get the signals of
//! the traffic they carry, not of their side of the road. A signal's
//! `<validity>` ranges replace these lanes rather than narrow them. The spec
//! says they must lie inside them, but esmini's maps give `+` signals
//! validities on both sides. The road id, `<signal id>`, `(s, t)` and
//! `orientation` are in its [`SignalProvenance`]. A signal missing `s` or
//! `t`, or off the ends of its road, is skipped.
//!
//! A `<signalReference>` applies the signal it names on its own road too,
//! and draws nothing new. It adds the lanes at its `s`, picked by its own
//! `orientation` and `<validity>`s the same way, to [`Signal::lanes`], and
//! its point to [`Signal::applies_at`].
//! [`SignalProvenance::references`] records its road, `(s, t)` and
//! `orientation`. Where two signals share an id, a reference applies the
//! first. A reference to an id no signal has is skipped.
//!
//! A signal's `<dependency>`s become its [`Signal::dependencies`], each the
//! signal it names and the `type`. OpenDRIVE 1.7 says a dependency names the
//! signal controlled, and 1.9 the one controlling, so the importer keeps the
//! link as the file gives it. Its `<reference>`s become its
//! [`Signal::references`]: a signal, or each [`Object`] the named `<object>`
//! baked to, with the `type`. One to an id nothing has, or to an
//! `elementType` other than `signal` or `object`, is skipped.
//!
//! A signal's `<semantics>` say what it means, whatever its catalogue codes,
//! as [`Signal::semantics`]: each is a [`Semantic`], such as a maximum speed
//! of 50 km/h, a stop line, or the [`RoadUser`]s it bars. The crate keeps
//! each kind's `type` as the file spells it, and applies none of them: a
//! speed semantic sets no [`RoadNetwork::speed_limits`]. A child the spec
//! does not name is skipped.
//!
//! A `<staticBoard>` or a `<vmsBoard>` makes the signal a board, as on a
//! gantry, in [`Signal::boards`]. A static board keeps each of its `<sign>`s
//! as a [`BoardSign`], with its codes and semantics, and a variable message
//! board its display and its [`DisplayArea`]s. Each stands where its `v` and
//! `z` put it: across the signal's board, to the left of its heading, and
//! up it, turned with it. A sign's own validities, links and position off
//! the board are not read. No other reader to compare against reads any of
//! these.
//!
//! Each top-level `<controller>` bakes to a [`Controller`] in
//! [`RoadNetwork::controllers`], with the signals its `<control>`s name, and
//! each of those signals lists it in [`Signal::controllers`]. A control that
//! names no signal is skipped, and its controller kept. A `<junction>`'s own
//! `<controller>` list names controllers that switch in step there. The
//! baked network has no list of junctions, so each entry is in the
//! [`ControllerProvenance`] of the controller it names.
//!
//! # Junction areas
//!
//! A junction's `<boundary>` and `<elevationGrid>` give the ground it
//! covers, between and around its connecting roads. Each junction with
//! either is a [`JunctionArea`] in [`RoadNetwork::junction_areas`].
//!
//! Its [`JunctionArea::boundary`] is a closed ring on the road surface. A
//! `lane` segment runs along the outer border of its `boundaryLane`, from
//! `sStart` to `sEnd`, at each of its road's lane stations, and the center
//! lane's is the line between the sides. A `joint` segment runs across its
//! road at its `contactPoint`, over the whole of `jointLaneStart`,
//! `jointLaneEnd` and the lanes between, through each lane border. Two
//! lanes on one side end at the inner one's inner border.
//!
//! Its [`JunctionArea::grid`] is the grid of heights along the junction's
//! reference line, its `<planView>`, and square to it.
//! [`JunctionArea::height_at`] reads a smooth surface through it, the
//! spec's bicubic interpolation: from a square's corners and their slopes and twist, each slope from the
//! cubic through the four grid points in line with the square's edge, or the
//! straight line along the edge where the grid has too few. So between two
//! equal heights the ground can rise past them, as the cubic through its
//! neighbours does. [`JunctionArea::mesh`] triangulates the ground inside
//! the boundary at the grid's spacing, at the grid's height where it
//! reaches, so a renderer can draw the whole junction.
//!
//! No other reader to compare against reads either.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec's grid overrides the height of the junction's roads, and a
//!   joint's `transitionLength` blends it into the roads coming in. The crate
//!   keeps the roads' own heights, lanes, marks and objects on them alike,
//!   and raises [`Warning::ElevationGridNotApplied`]. So where a file's roads
//!   and grid disagree, the lanes and the junction mesh do too.
//! - The spec requires the segments to close the boundary. The crate joins
//!   them straight, and raises [`Warning::BoundaryNotClosed`] where two are
//!   more than 10 cm apart in plan, as a boundary of one segment is. A
//!   segment naming no baked road, or a lane its road lacks, is dropped
//!   with a [`Warning::BoundarySegmentDropped`].
//! - The spec orders the segments counter-clockwise. The crate turns a
//!   clockwise boundary round, and raises [`Warning::BoundaryClockwise`].
//! - The spec orders a joint from `jointLaneStart` to `jointLaneEnd`. Where
//!   the segment before it ends nearer its end, the crate turns it round, as
//!   it does a lane segment, so each runs on from the one before.
//! - The spec requires `sStart` and every row's `center`. A missing `sStart`
//!   is 0, and a row without a `center` is skipped.
//! - The spec allows a junction's `<planView>` any number of geometries of
//!   any shape. The crate reads a grid only along one straight `<line>`, and
//!   drops one without it, or without a `gridSpacing` above 0, with a
//!   [`Warning::ElevationGridDropped`].
//! - The spec's formula for the height puts the grid's rows and columns the
//!   other way round in its matrix of corners from its product of powers.
//!   The crate follows the product, with `s` along the rows.
//!
//! # Railways
//!
//! A road's `<railroad>` holds its switches, each a [`Switch`] in
//! [`RoadNetwork::switches`]: where on the main track it is, where the side
//! track leaves it, each a [`TrackPoint`] with a road, an `s` and which way
//! along it the switch leads, which way it is set, and its partner's id and
//! name. A switch joins tracks part way along them, so, as a cross path is, it is
//! kept beside the lane graph, and the router does not follow it. Each
//! `<station>` is a [`Station`] in [`RoadNetwork::stations`], with its
//! platforms and the stretches of track each runs beside. A switch or a
//! platform segment naming a road the load lacks, an `s` off it, an `sEnd`
//! before its `sStart`, or a `position`, `dir` or `side` the spec does not
//! allow is dropped, with a
//! [`Warning::RailwayDropped`]. No other reader to compare against reads
//! either.
//!
//! # Lane borders
//!
//! A lane's `<border>`s give the `t` of its outer border, a cubic from each
//! `sOffset` in the lane section, held until the next. A border is measured
//! from the reference line, not from the lanes inside it. The lane's width
//! is its border less its inner neighbour's outer border, so a lane can open
//! out of nothing, as a gore area does. [`Lane::width`] and [`Lane::widths`]
//! come from that. A lane with widths outside a border lane stacks on its
//! border.
//!
//! No other reader to compare against builds lanes from borders.
//! libOpenDRIVE logs that it does not support them, esmini reads only
//! widths, and CARLA parses them but does not use them.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec makes widths and borders exclusive, and uses the widths when
//!   a lane section has both. Read literally, a lane with only borders in
//!   such a section has no extent. The crate reads each lane on its own: its
//!   widths if it has any, and its borders if not. It raises
//!   [`Warning::WidthAndBorder`] for each lane with borders in a section
//!   with widths.
//! - The spec forbids borders with a `<laneOffset>`. The crate measures a
//!   border from the reference line and ignores the offset for it. Width
//!   lanes still move with the offset. It raises
//!   [`Warning::BorderWithLaneOffset`] for each border lane where the offset
//!   is not 0 in its section.
//! - The spec forbids a border crossing inside the lanes within it. The
//!   crate gives the lane 0 width there, as it does a negative width, and
//!   raises [`Warning::BorderCrossesInnerLane`] at the first of the lane's
//!   stations where it crosses.
//! - The spec requires `sOffset`, `a`, `b`, `c` and `d`. The crate reads a
//!   border as it reads a width: a record without `a` is skipped, and a
//!   missing `sOffset`, `b`, `c` or `d` is 0.
//! - The spec says the records come in ascending `sOffset`. Ones out of
//!   order are sorted rather than dropped.
//!
//! # Lateral shapes
//!
//! A road's `<shape>`s give its cross-section, such as a crown or a
//! crossfall. The shapes at one `s` form a profile: each is a cubic in `dt`
//! from its `t`, held until the next. Between two profiles the height at a
//! `t` goes linearly along `s`. The height stands off the road along its
//! normal, which superelevation tilts, as a lane height does.
//!
//! The shape folds into the lane heights. Each lane border stands at the
//! shape under it plus the lane's `<height>` there, and the lane goes
//! straight across from one border to the other. Its centerline stands at
//! the mean of the two, and [`Lane::bank`] adds the slope between them.
//! Neighbouring lanes share a border height, so the mesh stays closed. A
//! crown that breaks on a lane border is exact. A curve inside a lane is
//! lost: the straight line across a parabola `c t²` is `c w² / 4` off at its
//! middle, 5.5 mm on a 3.5 m lane of a crown falling 2.5 % at 7 m out.
//! [`LaneProvenance::heights`] still gives the `<height>`s only. Each
//! profile's `s` is a station of every lane section it falls in.
//!
//! Objects, signals and road marks stand on the lane under them, so on the
//! shape as the mesh has it. A mark on the center line lies flat at the
//! shape there, on a crown's ridge.
//!
//! No other reader to compare against builds shapes. libOpenDRIVE logs
//! that it does not support them, esmini reads only superelevation, and
//! CARLA parses them but does not store them.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec's default shape is 0, so read literally the road steps from
//!   flat up to the first profile at its `s`. The crate holds the first
//!   profile before it, and the last after it, as a superelevation holds.
//! - The spec says each profile covers the whole road. Where one starts
//!   inside the road's right edge, the crate holds the first shape's value
//!   from its `t` out to the edge, and raises [`Warning::ShapeShortOfRoad`].
//!   The last shape runs on to the left edge, as each runs to the next.
//! - The spec's surface curves between a lane's borders. The crate's goes
//!   straight across, as above, and objects and signals stand on it, flush
//!   with the mesh.
//! - The spec measures a lane height from the road including its shape. An
//!   `attached` CRG adds its grid to the road without the shape, so a CRG
//!   laid over a shaped road answers without it.
//! - The spec requires `s`, `t`, `a`, `b`, `c` and `d`. A shape without `s`
//!   or `t` is skipped, and a missing `a`, `b`, `c` or `d` is 0, as for
//!   superelevation. Shapes out of order are sorted rather than dropped.
//!
//! # Cross-section surfaces
//!
//! A road's `<crossSectionSurface>` gives its surface across it another
//! way than `<shape>`s do: up to two strips each side of the reference line,
//! shifted across it by a `<tOffset>`. Each strip's height is a cubic in
//! `dt`, the distance across the strip, whose four coefficients, its
//! `<constant>`, `<linear>`, `<quadratic>` and `<cubic>`, are each cubics in
//! `s`. An inner strip, `id` 1 or -1, reaches as far as its `<width>`, and an
//! outer strip, 2 or -2, from there to the road's edge, measured from the
//! inner strip's edge. A `mode="relative"` outer strip stands on the inner
//! strip's edge, and an `independent` one on the reference plane.
//!
//! The crate bakes it as it bakes shapes: each lane border stands at the
//! surface's height under it plus the lane's `<height>`, and the lane goes
//! straight across between them. Level lanes keep out of it. Where the
//! two sides meet at `dt` 0 and stand at different heights, a lane's inner
//! border stands on its own side's strip. So what
//! [Lateral shapes](#lateral-shapes) says of the lanes, objects, signals and
//! marks on a shaped road holds on one with a cross-section surface too.
//!
//! No other reader to compare against reads it.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec forbids a cross-section surface with `<shape>`s or a
//!   `<superelevation>`. The crate adds their heights, and raises
//!   [`Warning::CrossSectionWithShape`].
//! - The spec gives no default `mode` for an outer strip. The crate reads a
//!   missing one as `independent`, and one it doesn't know too, with a
//!   [`Warning::UnknownStripMode`]. An inner strip's `mode` means nothing,
//!   and is not read.
//! - The spec allows strips 1, 2, -1 and -2, one of each, and needs an
//!   inner strip's width to place the outer strip beside it. The crate
//!   drops a strip with another `id`, one that repeats an `id`, and an outer
//!   strip with no inner strip of a width beside it, each with a
//!   [`Warning::StripDropped`].
//! - The spec says what lies past an inner strip only where an outer strip
//!   is beside it. Past an inner strip with a width and no outer strip, the
//!   crate runs the inner strip on, as the last shape runs on to the road's
//!   edge. A side with no strip is flat.
//! - The spec's rule for `dt` on the left side reads "if `t_effective <
//!   w_left`" for the outer strip. The crate reads it as `>`, the mirror of
//!   the right side.
//!
//! # Level lanes
//!
//! A lane with `level="true"`, such as a verge beside a banked motorway,
//! is kept out of the superelevation and the lateral shape. It starts at
//! its inner neighbour's outer border and runs level from there, so its
//! [`Lane::bank`] is 0 and the mesh stays closed. Its own `<height>`s
//! stand on top, and the lane outside it stacks on its outer border. Its
//! objects, signals and road marks stand on it.
//!
//! The crate builds a level lane from the road's tilted cross-section, as
//! libOpenDRIVE does. So in plan it is `w / cos φ` wide on a road
//! superelevated by `φ`: 2.5 mm more on a 2 m lane at 5 %.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec says every lane outside a level lane is level too. The crate
//!   holds such a lane level even where the file says it is not, and raises
//!   [`Warning::LaneNotLevel`].
//! - The spec says a level lane stays at the height of its inner
//!   neighbour's outer border. The crate reads that height with the
//!   neighbour's `<height>`, so a level lane outside a raised sidewalk
//!   starts at the sidewalk's height. libOpenDRIVE starts it on the road.
//!
//! # Lane heights
//!
//! A lane's `<height>`s raise its surface off the road, as a sidewalk or a
//! kerb stands above the lanes beside it. Each gives the height at the
//! lane's inner and outer border, along the road's normal, from its
//! `sOffset` in the lane section. The baked [`Lane`] stands at that height.
//! Its centerline rises by the height halfway across it, and
//! [`Lane::bank`] adds the slope from its inner border to its outer one. So
//! the surface mesh and [`Lane::sample_at`] see the raised surface.
//! [`LaneProvenance::heights`] gives the heights at each centerline vertex.
//! Where a lane's heights change pace, its lane section has a station, so a
//! ramp 1 m long bakes 1 m long.
//!
//! The mesh builder tilts a raised lane's cross axis by its slope, rather
//! than moving each edge to its own height. The lane's width is the straight
//! line across its surface, so each edge lands on its border and at its height.
//! On a grade the edges also sit half the rise times the grade along the
//! road from where the road's normal puts them, 1.5 mm for a 0.1 m rise at
//! 3 %.
//! Where a lane's kerb and outer edge change height at different paces, as
//! on a ramp, each quad of its mesh is twisted, and its two triangles cut
//! the corner by up to a quarter of the rise across that quad. Halfway up
//! a 0.1 m kerb ramp the mesh is 25 mm off. libOpenDRIVE also meshes a lane
//! as quads between its borders.
//!
//! The step between a raised lane and the lane beside it stays open. The
//! spec defines no kerb face, and libOpenDRIVE and esmini draw none.
//!
//! Objects and signals stand on the lane at their point, and a road mark on
//! its own lane. So a pole on a sidewalk stands on the sidewalk, a
//! sidewalk's outer mark rises with it, and the mark along the kerb, which
//! belongs to the lane below it, stays on the road. An object still leans
//! with the road, not with the slope of the lane under it, as in
//! libOpenDRIVE.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec's rule for lane geometry holds a height until the next one.
//!   The crate goes straight from each height to the next, as libOpenDRIVE
//!   and esmini do, and holds the last one after it. esmini's maps are
//!   written for this reading. Read as steps, a sidewalk giving 0.02 m at
//!   0 m and 2 m and 0.12 m at 3 m would jump at 3 m rather than ramp up
//!   from 2 m.
//! - The spec gives only the heights at a lane's two borders. The crate goes
//!   straight across from one to the other, as both readers do.
//! - The spec gives no height before a lane's first `<height>`. The first
//!   one holds there. libOpenDRIVE carries the first ramp on backwards.
//! - The spec requires `sOffset`, `inner` and `outer`. A missing one is 0,
//!   as libOpenDRIVE reads it. A negative `sOffset` is 0, as the crate reads
//!   a road mark's.
//! - The spec says a lane's heights come in ascending `sOffset`. Ones out of
//!   order are sorted rather than dropped.
//! - The spec forbids heights on the center lane. The crate never bakes the
//!   center lane, so it ignores them.
//! - The spec does not say whether objects, signals and road marks stand
//!   on a raised lane or on the road below it. The crate stands them on the
//!   lane.
//! - The spec does not say which lane a point on the border between two is
//!   on. The crate puts it on the inner one, as libOpenDRIVE does.
//! - The spec gives no height past the outermost lane. A point there takes
//!   that lane's outer height. libOpenDRIVE carries the lane's slope on.
//! - The spec measures a height from the road including its surface. An
//!   `attached` CRG adds its grid to the road without the lane's height, so
//!   a CRG laid over a raised lane answers at road level.
//!
//! # Road marks
//!
//! Each `<roadMark>` bakes to a [`RoadMark`] in [`RoadNetwork::road_marks`].
//! It runs along its lane's outer border, from its `sOffset` in the lane
//! section to the lane's next `<roadMark>` or the end of the section. The
//! center lane's marks run along the line between the two sides. A mark
//! names the lanes either side of it, [`RoadMark::left`] and
//! [`RoadMark::right`], looking along `+s`. Its `type` is a
//! [`RoadMarkType`], its `laneChange` a [`LaneChange`], and its `color` stays
//! the file's text. Its road, lane section, `<lane id>`, `s` and length are in
//! its [`RoadMarkProvenance`].
//!
//! Each of its [`RoadMark::lines`] is quads lying in the road surface, placed
//! at the lane's own stations, so the paint lies on the lane mesh. A mark's
//! `<type><line>`s and `<explicit><line>`s are its lines, whatever its
//! `type` other than `none`. Each is a [`RoadMarkLine`] with its own width,
//! colour, [`LinePattern`] and [`LineRule`], `tOffset` from the border. A
//! `<type><line>` repeats from its `sOffset` to the end of the mark, and an
//! `<explicit><line>` paints once. A mark with no lines gets stand-ins for
//! its type: one line for `solid` and `broken`, two for a double type, and
//! none for the rest. Each `<sway>` moves every line of the mark sideways by
//! its cubic, from its `ds` along the mark to the next. The marks of a lane
//! section too short to bake do not bake either.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec requires `color`, `type` and `sOffset`. A mark missing one is
//!   `standard`, `none` or 0, as libOpenDRIVE reads it. A `type` this crate
//!   does not know is [`RoadMarkType::Unknown`].
//! - The spec says a mark's and a line's `sOffset` are at least 0. A
//!   negative one is 0, as libOpenDRIVE and esmini read it.
//! - The spec gives no default `weight`. A mark missing one is standard.
//! - The spec says a mark's, a `<type>`'s and a `<line>`'s width are above
//!   0. A width of 0 counts as none, so the next in line applies: the
//!   line's, then the type's, then the mark's, then the weight's. esmini
//!   writes 0 on all three.
//! - The spec makes a line's `length` the part painted, so a line with a
//!   `length` and a `space` of 0 paints nothing. The crate reads it as one
//!   continuous line, as esmini means it. Without this, every mark in
//!   esmini's `multi_intersections.xodr` vanishes.
//! - The spec lets a `none` mark have lines. It paints none, since esmini
//!   writes a line of no width under each of its `none` marks and draws
//!   nothing there.
//! - The spec says only that `tOffset` is a lateral offset from the border.
//!   The crate adds it along +t on either side, as libOpenDRIVE and esmini
//!   both do. So a positive `tOffset` moves a right lane's line inward and a
//!   left lane's outward.
//! - The spec does not say which way a `<sway>` moves the lines either. The
//!   crate reads it along +t, as it does `tOffset`, and moves nothing before
//!   the first `ds`.
//! - The spec gives no default `rule`. A line without one has
//!   [`LineRule::None`].
//! - The spec does not say how wide a line of each weight is, or how a type
//!   looks. A line is 0.12 m wide, or 0.25 m bold, as libOpenDRIVE has it.
//!   `broken` is dashes 4 m long with 8 m between, and a double type's lines
//!   are one width either side of the border, as esmini draws them. `none`,
//!   `edge`, `grass`, `curb`, `botts dots` and `custom` paint nothing.
//! - The spec does not say what a dash's length is measured along. The
//!   crate measures it along the reference line, as libOpenDRIVE and esmini
//!   do, so a dash on the outside of a bend is longer.
//! - The spec says a lane's marks come in ascending `sOffset`. Marks out of
//!   order are sorted, as lane sections are, rather than dropped.
//! - The spec draws the marks of a lane with neither a `<width>` nor a
//!   `<border>` on its border, which is then its inner border. The crate
//!   does not bake the lane, so its marks go with it, and raises
//!   [`Warning::LaneDropped`].
//!
//! # Speed limits and road types
//!
//! [`RoadNetwork::speed_limits`] and [`RoadNetwork::road_types`] say what
//! holds along each lane, as [`Along`] stretches in metres along its
//! centerline. [`RoadNetwork::speed_limit_at`] and
//! [`RoadNetwork::road_type_at`] read them at a point, such as the `s` of
//! [`RoadNetwork::nearest_lane`]. A limit is in metres a second, or
//! [`SpeedLimit::Unlimited`]. Where the map says nothing, there is no
//! stretch.
//!
//! A road's `<type>` holds from its `s` to the next, or to the end of the
//! road, across every lane. So does the limit its `<speed>` sets. Its
//! `country` is in [`RoadNetwork::road_countries`] the same way, where the
//! map gives one. The crate applies none of a country's rules. A
//! `max="undefined"` sets none. A lane's own `<speed>` holds from its
//! `sOffset` to the next, or to the end of its lane section, and overrides
//! its road's. A `<speed>` without a `unit` is in m/s, as the spec says.
//!
//! The limits come from `<speed>` records only. The spec says a speed limit
//! sign takes precedence, but the crate does not read signs as limits. A
//! caller that wants them can find the signs in [`RoadNetwork::signals`].
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec requires a `max` of at least 0 in `m/s`, `km/h` or `mph`, or
//!   on a road `no limit` or `undefined`. The crate drops any other
//!   `<speed>`, and one whose limit is too large for an `f32`, and raises
//!   [`Warning::SpeedLimitDropped`]. The lane's limit
//!   there is then its road's, or the lane's `<speed>` before it.
//! - The spec requires a `<type>`'s `s` and a lane `<speed>`'s `sOffset`.
//!   A type without `s` is skipped. A missing or negative `sOffset` is 0, as
//!   for a `<height>`.
//! - The spec says types and a lane's speeds come in ascending order. Ones
//!   out of order are sorted rather than dropped.
//! - A `type` the spec does not name bakes as [`RoadType::Unknown`], as an
//!   unrecognised lane type bakes as [`LaneType::Unknown`].
//!
//! # Lane rules, access and materials
//!
//! [`RoadNetwork::lane_rules`], [`RoadNetwork::lane_access`] and
//! [`RoadNetwork::lane_materials`] hold a lane's own `<rule>`s, `<access>`es
//! and `<material>`s, as [`Along`] stretches like the speed limits. The
//! `*_at` methods read them at a point. Each holds from its `sOffset` to the
//! next of its kind, or to the end of its lane section.
//!
//! - A rule is the file's free text, such as `no stopping at any time`. A
//!   `<rule>` without a `value` is skipped. Rules at the same `sOffset` all
//!   apply, so their texts join with `"; "`.
//! - [`Access::Allow`] lists the only road users that may use the lane, and
//!   [`Access::Deny`] those that may not. Each is a restriction `type` as
//!   the file names it, such as `bus`. Where a lane has no access stretch,
//!   everyone may use it, as the spec says. A `rule="deny"` naming `none`
//!   lifts the restrictions before it.
//! - Files before 1.8 name one road user per `<access>`, in its
//!   `restriction` attribute. `<access>`es at the same `sOffset` apply
//!   together. Two with the same `rule` merge their road users. An `allow`
//!   and a `deny` merge into the `allow`, less the road users the `deny`
//!   names, since only those may then use the lane.
//! - A lane has one material, speed and visibility at a time. Of two at the
//!   same `sOffset`, the later in the file holds.
//! - A material is a [`Material`], as on an object. The crate does not use
//!   its friction, which a CRG gives through [`RoadSurface`].
//!
//! The crate departs from the OpenDRIVE 1.9 spec here:
//!
//! - The spec allows an `<access>` without a `rule`, but it then says
//!   nothing about who may use the lane. The crate drops it, and any `rule`
//!   other than `allow` or `deny`, and raises [`Warning::AccessDropped`].
//!   The lane is then open to everyone up to the next `<access>`.
//! - A missing or negative `sOffset` is 0, and entries out of order are
//!   sorted, as for a `<speed>`.
//!
//! # Lane visibility
//!
//! [`RoadNetwork::lane_visibility`] says how far a driver can see from each
//! lane, as [`Along`] stretches of a [`Visibility`]: the distance ahead,
//! behind, to the left and to the right, in metres.
//! [`RoadNetwork::lane_visibility_at`] reads it at a point. Each
//! `<visibility>` holds from its `sOffset` to the next, or to the end of its
//! lane section, as a lane's `<rule>` does.
//!
//! OpenDRIVE 1.9 does not define `<visibility>`, and no map in the test
//! corpus or among ASAM's 1.9 examples uses it. CARLA reads its `sOffset`,
//! `forward`, `back`, `left` and `right`, and the crate reads the same.
//! esmini and libOpenDRIVE ignore it.
//!
//! The crate reads the element this way:
//!
//! - Neither CARLA nor the attribute names say whether `forward` is the way
//!   the lane's traffic runs or the road's `+s`. The crate keeps the four
//!   distances as the file names them and turns none of them.
//! - A `<visibility>` with a distance missing, not a number, below 0 or too
//!   large for an `f32` is dropped, with a [`Warning::VisibilityDropped`].
//!   The lane then has no visibility up to the next `<visibility>`. CARLA
//!   reads a missing distance as 0.
//! - A missing or negative `sOffset` is 0, and entries out of order are
//!   sorted, as for a `<speed>`.
//!
//! # Road surfaces
//!
//! Each `<CRG>` under a road's or a junction's `<surface>` becomes a
//! [`CrgSurface`] in [`RoadNetwork::crg_surfaces`]. The importer reads the
//! record, not the OpenCRG file it names. [`RoadSurface::new`] loads the
//! files through a closure you pass. Then [`RoadSurface::sample`] gives the
//! height, normal and friction under a point, in `f64`. A CRG answers where
//! it covers the point, and the surface mesh answers everywhere else. The CRG
//! does not change the mesh.
//!
//! Where several records of one purpose cover a point, the last in the file
//! wins, as rule `road.crg.use_last_entry` has it, and the rest are ignored.
//! Elevation and friction count apart. The crate lists a junction's
//! records after every road's, so they win over its roads', which the spec
//! doesn't rank.
//!
//! The modes follow the road surface section of ASAM OpenDRIVE 1.9:
//!
//! - `attached` lays the CRG grid along the road's reference line and adds
//!   its height to the road's. It ignores the file's reference line, with its
//!   height, slope and bank. The spec's formula evaluates the file with
//!   `crgEvaluv2z`, which includes them, but its text says they are
//!   disregarded. The importer follows the text.
//! - `attached0` lays the file along the road the same way, and its
//!   elevation replaces the road's height.
//! - `genuine` starts the file's own reference line at the road's
//!   `(sOffset, tOffset)`, turned by `hOffset`.
//! - `global` leaves the file in its own coordinates, moved by `xOffset`,
//!   `yOffset` and `hOffset`.
//!
//! Along the road, `u = s - sOffset` and `v = t - tOffset`. The importer
//! samples the reference line under a road CRG every 0.25 m and joins the
//! samples with arcs, so `(u, v)` is exact on lines and arcs. On a spiral
//! that tightens from straight to a 50 m radius over 50 m, it is within a
//! micrometre.
//!
//! `orientation="opposite"` negates `u` and `v`. The spec's matrix for it also
//! swaps them, which contradicts its own text that the file turns 180
//! degrees. The importer follows the text.
//!
//! A road's CRG applies between `sStart` and `sEnd`, on the lanes of the
//! sections there. A junction's CRG applies on every lane of the roads in the
//! junction, before any CRG of those roads, since the spec says it supersedes
//! their elevation. A junction's CRG must be `global`. The importer reads no
//! junction reference line, so it skips the other modes there.
//!
//! A friction CRG ignores `zOffset` and `zScale`, as the spec says, and gives
//! the grid values exactly as the file has them. OpenCRG, like its C-API,
//! shifts the heights of a file without a `$ROAD_CRG_MODS` block so its
//! reference line starts at 0. Friction skips that shift.
//!
//! # What the importer ignores
//!
//! Everything else in the file. Each feature below raises a
//! [`Warning::Ignored`], once per road, junction, or the rest of the
//! document, with how many times the file uses it.
//!
//! The crate skips these elements: `<vmsGroup>`, a header's `<license>` and
//! `<defaultRegulations>`, `<dataQuality>` and `<include>` anywhere, a
//! junction's `<objects>` and `<roadSection>`s, and an object's `<surface>`,
//! `<skeleton>` and `<curveLocal>` outlines. It skips `<userData>` anywhere
//! but directly on an object, which it reads. It skips the `<markings>` of
//! an outline, and the `validity`, `dependency` and `reference` of a
//! board sign.
//!
//! It also ignores these uses of elements it reads:
//!
//! - a second `lanes` on a road, and a `lanes` with `layer="temporary"`. The
//!   crate bakes the first `lanes` of a road as the road, as if it were
//!   permanent;
//! - `singleSide="true"` on a lane section;
//! - a `layer` other than `permanent` on a lane link or a `validity`, and a
//!   `fromLayer` or `toLayer` on a junction lane link;
//! - `perpToRoad="true"` on an object;
//! - a repeat's `bT`, `cT` or `dT` other than 0, and
//!   `detachFromReferenceLine="true"`;
//! - a boundary segment's `transitionLength` above 0.
//!
//! One omission changes the road you get back, rather than only dropping
//! detail around it: the center lane, lane 0, never becomes a [`Lane`].
//! Only its road marks are read, and its `type` and `level`, which are in
//! [`Provenance::center_lanes`].
//!
//! # Coordinate frame
//!
//! Baked geometry is OpenDRIVE's own frame: right-handed, **Z-up, metres**,
//! with the reference line in the X-Y plane, `hdg` the heading within it, and
//! elevation along +Z. A point imports unchanged, so a coordinate you read out
//! of the `.xodr` is the coordinate you get back.
//!
//! An OpenDRIVE left turn (increasing `hdg`) curves toward +Y. Positive lane
//! offset `t` is to the left of the heading, which for heading +X is +Y.
//!
//! A renderer or physics engine that wants Y-up has to rotate on the way in.
//! Doing that here instead would mean every coordinate in this API disagreed
//! with the file it came from, which is the harder bug to find.
//!
//! Travel direction follows each road's `rule`. Under right-hand traffic,
//! the default, negative-id (right) lanes run with `+s` and positive-id
//! (left) lanes against it. Under `rule="LHT"` the left lanes run with `+s`.
//! Links, lane changes and a signal's `orientation` follow the direction.
//!
//! The spec allows only `RHT` and `LHT`. The crate reads any other value as
//! `RHT`, the spec's default, and raises [`Warning::UnknownTrafficRule`].
//! esmini also reads `lht` as left-hand traffic. The crate does not.
//!
//! A lane's `direction` overrides its side's. `reversed` runs it against
//! its side, and `both` makes it a [`Direction::Both`] lane, which runs
//! either way. A lane of the deprecated `type="bidirectional"` with no
//! `direction` runs both ways too, since the spec says `both` replaces it.
//! Two two-way lanes that join lead into each other both ways, even where
//! only one names the other. A two-way lane's successors are the lanes off
//! both of its ends, and its predecessors those that drive into it at either
//! end. Of the lanes off a two-way lane's end, only those whose traffic runs
//! away from the joint count. A lane
//! change is only ever to a lane running the same way. [`RoadNetwork::route`]
//! drives a two-way lane toward the next lane on the route, and
//! [`RoadNetwork::advance`] each way from one. esmini, libOpenDRIVE and
//! CARLA ignore `direction`.
//!
//! The crate departs from the OpenDRIVE 1.9 spec here: the spec allows only
//! `standard`, `reversed` and `both`. The crate reads any other value as
//! `standard`, the spec's default, and raises
//! [`Warning::UnknownLaneDirection`].
//!
//! # Road coordinates
//!
//! The network keeps each road it baked, in [`RoadNetwork::roads`]: its
//! reference line, the profiles along it, and where its lanes lie across it.
//! A [`RoadPosition`] names a place on one in OpenDRIVE's road coordinates:
//! `s` along the reference line and `t` across it, positive to the left.
//! [`RoadNetwork::road_point`] turns one into the point on the road surface
//! there, and [`RoadNetwork::road_position`] turns a point back.
//! [`RoadNetwork::road_lane`] says which road, lane section and `<lane id>`
//! a lane is. A [`Road`] is found by its [`RoadId`], or by its `<road id>`
//! with [`RoadNetwork::road_by_od_id`].
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
//! The surface is the one the lanes are baked on: the elevation, the
//! superelevation, the lateral shape and the `<height>` of the lane at `t`.
//! `t` runs along the tilted cross-section, so a point at `t` on a road
//! banked by `φ` is `t cos φ` from the reference line in plan, as the spec
//! has it. Signals, objects, road marks and `<positionRoad>` stand on the
//! road through the same call. The forward call is exact, up to the `f32`
//! of a [`Point`].
//!
//! The inverse finds the roads whose lanes come near the point, and on each
//! solves for the `(s, t)` whose surface lies straight under or over it.
//! It takes the road whose surface is nearest in 3D, so a point on a bridge
//! finds the bridge. Where roads overlap in a junction, their surfaces meet
//! to within rounding, and which road comes back is not defined.
//! [`RoadNetwork::road_position_on`] takes the road to use. A point off
//! every road gets the nearest road's `(s, t)`, with `s` held within the
//! road. A road
//! without lanes has no surface to find. On every map in the test corpus,
//! a point on a lane comes back to within 0.2 mm, or within the `f32` step
//! of its coordinates where that is larger: 0.5 m on a map in UTM
//! coordinates. Where the surface steps at a lane section seam, as a lane's
//! `<height>` can, a station on the seam is on the section that starts
//! there, so the end of the lane before it comes back at the new height.
//!
//! A network built with [`RoadNetwork::new`], or serialized before roads were
//! kept, has none, so the road queries answer `None`.
//!
//! Where a road bends tighter than its lanes are wide, as some exports do at
//! a sharp turn, its outer lanes fold over its inner ones. A point there has
//! more than one road position, and the crate returns one of them.
//!
//! # Lane positions
//!
//! A [`LanePosition`] is a lane, a road `s`, and an offset from the lane's
//! center toward `+t`, as esmini's `SetLanePos` takes them.
//! [`RoadNetwork::lane_point`] turns one into the point on the lane's
//! surface, and [`RoadNetwork::lane_position`] turns a point back, on the
//! lane of any type whose borders hold the road `t`. At offset 0 a lane
//! position is on the lane's center, where the baked centerline stands.
//!
//! A lane has two kinds of `s`. A [`LanePosition`], like a [`RoadPosition`],
//! a signal's or an object's, measures it along the road's reference line.
//! [`Projection`], the [`Along`] stretches and [`Lane::sample_at`] measure it
//! along the lane's baked centerline. On a bend the two drift apart by the
//! lane's `t` over the radius, and under a changing lane offset by how far
//! the lane swings. Across the test corpus the drift reaches 21 % of the
//! distance, 85 m along a 400 m spiral. [`RoadNetwork::centerline_s`] turns a
//! lane position's `s` into the centerline's, to read a speed limit or a
//! [`Lane::sample_at`] there.
//!
//! [`RoadNetwork::nearest_lane`] projects onto the baked centerline, which
//! runs straight between points at most 2 m and 0.05 rad apart. Across the
//! corpus its point is within 0.06 mm of the lane's exact center at the
//! median, 6 mm at the 95th percentile and 1.1 cm at the 99th.
//! [`RoadNetwork::lane_position`] is exact, and costs about ten times as much.
//!
//! # Moving along the lanes
//!
//! [`RoadNetwork::advance`] moves a [`LanePosition`] a distance along the
//! lanes, the way their traffic runs, or back against it for a negative
//! distance. It gives every place the distance reaches, one [`Advance`] per
//! branch, as CARLA's `Waypoint::GetNext` does, and leaves the choice to the
//! caller. A branch that reaches a lane with nothing after it stops at the
//! lane's end, as an [`Advance::DeadEnd`] with the distance left.
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
//! The distance runs along the lanes' centerlines, so it is how far a
//! vehicle on them travels. esmini's `MoveAlongS` and CARLA step by road `s`,
//! which the outside of a bend covers in fewer metres of road than of lane.
//! The offset keeps its side of the traffic, so onto a road that runs the
//! other way it changes sign. esmini picks one branch by a heading or a
//! route. The crate gives them all: a caller with a heading or a route can
//! pick, and one without can see every way on.
//!
//! Every way on grows fast. Each fork splits the branches, and on a map with
//! loops two paths that reach the same lane have rarely come the same
//! length, so they end at different places and don't merge. From one lane
//! of Town07, 400 m gives 26 places, 1.2 km 1,343, 2 km 72,912 in about
//! 150 ms, and 2.4 km 464,066 in 1.3 s: about seven times as many for each
//! further 400 m. Nothing caps it, so a long enough distance runs out of
//! memory. Step a few metres at a time, as CARLA's callers do, and pick a
//! branch at each fork. An infinite or NaN distance gives no places.
//!
//! [`RoadNetwork::left_of`] and [`RoadNetwork::right_of`] step to the lane
//! beside, at the same `s`, left and right of the lane's traffic. That is
//! the lane of any type, running either way.
//!
//! [`RoadNetwork::may_change_left`] and [`RoadNetwork::may_change_right`]
//! say whether the road mark on that border lets a vehicle cross into the
//! lane, from the mark's [`LaneChange`]. `increase` allows a crossing toward
//! the higher `<lane id>` only, and `decrease` toward the lower, whichever
//! side traffic drives on. Left and right follow the lane's traffic, so
//! under `rule="LHT"` the center line is on the right of lane 1. The answer
//! is `None` where there is no lane beside, no mark on the border at the
//! `s`, or a `laneChange` the crate does not recognise. Whether the lane
//! runs the other way, or is one traffic may use, is the caller's to decide.
//! CARLA reads `laneChange` the same way. esmini and libOpenDRIVE store it
//! and answer nothing from it.
//!
//! The crate reads the mark this way:
//!
//! - The spec does not say which mark holds where one ends and the next
//!   begins. The one starting there answers, as a lane section does.
//! - The spec does not tie `laneChange` to the mark's type. The crate
//!   reads it whatever the type, so a `curb` with the default `both`
//!   allows a crossing.
//!
//! # Geo reference
//!
//! [`RoadNetwork::geo_reference`] gives the `<geoReference>` PROJ string and
//! the `<offset>` as the file has them, and applies neither. Points stay in
//! the file's frame, as [Coordinate frame](#coordinate-frame) says. To place
//! the map on the earth, apply the offset and then the projection, with a
//! library such as PROJ.
//!
//! The spec rotates a point by the offset's `hdg` and then adds its `x`,
//! `y` and `z`. Not every exporter agrees: `netconvert` writes the offset
//! that takes the projected frame back to the map's. Check a map's sign
//! against a known point before relying on it.
//!
//! A missing or unreadable `<offset>` attribute reads as 0, as in esmini. A
//! blank `<geoReference>` reads as none, which the spec takes to mean a
//! local Cartesian frame.
//!
//! # Robustness
//!
//! `.xodr` files come from outside your program. A road the importer cannot
//! interpret is skipped rather than fatal, because losing a whole city map to
//! one junk road is the worse failure. [`load_str`] still errors if the
//! document yielded no lanes at all. Non-finite attribute values are rejected
//! at parse. Rust's float parser accepts `NaN` and turns `1e400` into
//! infinity, and one such value poisons every point derived from it. A
//! number the crate keeps as an `f32`, such as a signal's or a road mark's size, counts as
//! unreadable too when it is too large for one.
//!
//! [`Provenance::warnings`] says what the load did with a bad file. Each
//! [`Warning`] names where in the file it happened, and prints as a
//! sentence. The crate raises one where it drops or ignores something, or
//! reads a file that breaks a rule of the spec:
//!
//! - [`Warning::Ignored`] for a feature of the file the crate ignores. See
//!   [What the importer ignores](#what-the-importer-ignores).
//! - [`Warning::RoadSkipped`] for a road with no finite `length`, one over
//!   [`MAX_LENGTH`], no `<planView>`, no `<geometry>` it can bake, or lanes
//!   that land outside the range of an `f32`. The [`RoadSkipReason`]
//!   says which.
//! - [`Warning::GeometryDropped`] for a `<geometry>` it can't bake, such as
//!   one whose `length` isn't above 0. The road keeps its other geometry.
//! - [`Warning::RoadMarkLineDropped`] for a road mark line with too many
//!   dashes to paint, such as dashes a fraction of a millimetre long, or one
//!   so wide or so far off its border that its paint lands out of range.
//! - [`Warning::TooManyCopies`] for an object whose `<repeat>` or dashed
//!   `<marking>` would make over 100,000 copies or dashes.
//! - [`Warning::LaneIdUnreadable`] for a lane whose `id` isn't a whole
//!   number, or is 0 outside `<center>`.
//! - [`Warning::LaneDropped`] for a lane with no `<width>` or `<border>` it
//!   can read.
//! - [`Warning::WidthAndBorder`], [`Warning::BorderWithLaneOffset`] and
//!   [`Warning::BorderCrossesInnerLane`] for the lane borders the spec
//!   forbids. See [Lane borders](#lane-borders).
//! - [`Warning::ShapeShortOfRoad`] for a lateral profile that does not
//!   cover the road. See [Lateral shapes](#lateral-shapes).
//! - [`Warning::CrossSectionWithShape`], [`Warning::UnknownStripMode`] and
//!   [`Warning::StripDropped`] for a cross-section surface the spec
//!   forbids. See [Cross-section surfaces](#cross-section-surfaces).
//! - [`Warning::UnknownTrafficRule`] for a road `rule` other than `RHT` or
//!   `LHT`, and [`Warning::UnknownLaneDirection`] for a lane `direction`
//!   the spec does not allow. See [Coordinate frame](#coordinate-frame).
//! - [`Warning::ConnectionDropped`] for a junction connection without the
//!   roads it joins, or in a junction without an `id`, and
//!   [`Warning::LaneLinkDropped`] for a `<laneLink>` whose `from` or `to`
//!   isn't a whole number.
//! - [`Warning::JunctionLinkMissing`] for an incoming road whose `<link>`
//!   leaves out its junction.
//! - [`Warning::VirtualJunctionWithoutMainRoad`] and
//!   [`Warning::VirtualLinkDropped`] for a virtual junction or link the
//!   crate can't place, and [`Warning::VirtualLaneDropped`] for a lane pair
//!   of a link that names a lane its road doesn't have. See
//!   [Virtual junctions](#virtual-junctions).
//! - [`Warning::RailwayDropped`] for a railway switch or platform segment
//!   the crate can't place. See [Railways](#railways).
//! - [`Warning::JunctionReferenceDropped`] and
//!   [`Warning::UnknownJunctionGroupType`] for a `<junctionGroup>` the
//!   crate can't read whole.
//! - [`Warning::CrossPathDropped`] for a `<crossPath>` whose roads or lanes
//!   the load lacks.
//! - [`Warning::ElevationGridNotApplied`], [`Warning::ElevationGridDropped`],
//!   [`Warning::BoundaryNotClosed`],
//!   [`Warning::BoundarySegmentDropped`] and [`Warning::BoundaryClockwise`]
//!   for a junction's area. See [Junction areas](#junction-areas).
//! - [`Warning::PriorityDropped`] for a junction `<priority>` naming a road
//!   the load did not bake, and [`Warning::NeighborDropped`] for a road
//!   `<neighbor>` the crate can't read.
//! - [`Warning::RoadLengthMismatch`] for a road whose `length` is more than
//!   1 cm from where its `<planView>` ends. The road bakes to its `length`.
//! - [`Warning::LinkGap`] for a lane link whose lanes don't meet: more than
//!   10 cm apart, measured across both lanes from border to border where one
//!   leaves off and the next begins. So a lane that splits in two, or hands
//!   over to one opening out of nothing beside it, meets it. The link is
//!   kept. The check measures the lanes as baked, so where the file changes
//!   `level` across a lane section seam, and the crate's reading stacks the
//!   level lanes' heights, the lanes step there and warn too, as in ASAM's
//!   `Ex_CrossFall_LeftTurn`.
//! - [`Warning::AccessDropped`] for an `<access>` it can't read. See
//!   [Lane rules, access and materials](#lane-rules-access-and-materials).
//! - [`Warning::SpeedLimitDropped`] for a `<speed>` it can't read. See
//!   [Speed limits and road types](#speed-limits-and-road-types).
//! - [`Warning::LaneNotLevel`] for a lane outside a level lane that is not
//!   level. See [Level lanes](#level-lanes).
//! - [`Warning::VisibilityDropped`] for a `<visibility>` with a distance it
//!   can't read. See [Lane visibility](#lane-visibility).
//!
//! An element the crate does not read raises one [`Warning::Ignored`] for
//! each road, junction, or the rest of the document that uses it, with a
//! count, so a map with thousands of `<userData>` raises a few. Other things
//! it drops, such as a lane section under 1 mm long or a signal off the end
//! of its road, raise none. The crate prints nothing. To log the warnings,
//! print the list.
//!
//! # Importer contract
//!
//! Code baking other map formats into a [`RoadNetwork`] must follow these
//! rules:
//!
//! - Build lane geometry with [`Polyline::try_new`], and return an error on
//!   bad input, such as fewer than two points, rather than call the
//!   panicking [`Polyline::new`].
//! - Where it splits one curve into contiguous lanes, build them with
//!   [`Polyline::try_new_with_tangents`] and pass the curve's exact tangent at
//!   each end. A polyline that has to guess its end tangent guesses from its
//!   last segment. The two lanes meeting at a joint then
//!   guess differently and leave a visible seam.
//! - Keep [`LaneId`]s opaque, and never assume one indexes the lane list.
//!
//! On curves tighter than the half-width, [`RoadNetwork::surface_mesh`]
//! pulls in the inner edge so the surface doesn't fold over itself. The outer
//! edge keeps its full width and radius.

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
    CrgAlong, CrgMode, CrgPose, CrgPurpose, CrgSurface, RoadSurface, SurfaceHint, SurfaceSample,
};
pub use geo::{GeoOffset, GeoReference};
pub use geometry::TooFewPoints;
pub use geometry::{Polyline, Pose, Projection, RoadSample};
pub use header::Header;
pub use junction::{
    CrossPath, CrossPathEnd, ElevationGrid, GridRow, JunctionArea, JunctionGroup,
    JunctionGroupKind, LinkPoint, MainRoad, VirtualJunction, VirtualLink,
};
pub use mesh::{LaneSpan, Mesh, MeshError, MeshSampler};
pub use network::{Direction, Lane, LaneAdvisory, LaneId, LaneType, RoadNetwork};
pub use object::{
    Border, Corner, Extent, Marking, Material, Object, ObjectId, ObjectType, ParkingSpace, Section,
    Shape, UserData,
};
pub use object_mesh::ObjectSpan;
/// The OpenCRG reader, for loading the files a [`RoadSurface`] evaluates.
pub use opencrg;
pub use parse::{
    load_file, load_file_with_provenance, load_str, load_str_with_provenance, CenterLaneProvenance,
    ControllerProvenance, CrossPathProvenance, ImportError, JunctionControllerProvenance,
    JunctionProvenance, LaneHeight, LaneLinkOverlap, LaneProvenance, ObjectProvenance, Orientation,
    PriorityProvenance, Provenance, RoadEnd, RoadMarkProvenance, RoadSkipReason, SignalProvenance,
    SignalReferenceProvenance, StructureProvenance, Warning, MAX_LENGTH,
};
pub use railway::{Platform, PlatformSegment, Station, Switch, SwitchPosition, TrackPoint};
pub use road::{LanePosition, Priority, Road, RoadId, RoadLane, RoadNeighbor, RoadPosition, Side};
pub use road_mark::{
    LaneChange, LinePattern, LineRule, RoadMark, RoadMarkId, RoadMarkLine, RoadMarkType,
    RoadMarkWeight,
};
pub use signal::{
    BoardSign, Control, Controller, ControllerId, Dependency, DisplayArea, MessageBoard, Reference,
    Referenced, RoadUser, Semantic, Signal, SignalBoard, SignalId, Unit,
};
pub use structure::{Coverage, Structure, StructureId, StructureKind};
