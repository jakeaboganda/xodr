# OpenDRIVE support

What the importer builds from each part of an `.xodr` file, and where to
find it. The [crate docs](https://docs.rs/xodr) list every element and
attribute, and every place the crate departs from the spec.
[comparison.md](comparison.md) compares this crate with esmini,
libOpenDRIVE and CARLA.

## Which OpenDRIVE version

- Any version loads. `revMajor` and `revMinor` are kept in
  `RoadNetwork::header` and change nothing.
- A file that uses an element or attribute from 1.8 or 1.9 but declares an
  older version raises `Warning::NewerThanDeclared`. A file with no version
  raises none. `tests/data/introduced.py` generates the table from the 1.9
  XSD and the spec's revision history.
- Two elements outside 1.9 are read: a lane's `<visibility>`, as CARLA reads
  it, and a road's `<neighbor>`, as libOpenDRIVE reads it.
- Deprecated elements, such as `poly3` and a signal's `<positionRoad>`, read
  like any other.
- The tests load real files declaring 1.4, 1.6 and 1.7.

## Roads and lanes

| Element | What you get |
| --- | --- |
| `<planView>`: line, arc, spiral, poly3, paramPoly3 | each road's reference line, in `RoadNetwork::roads` |
| `<elevationProfile>`, superelevation, `<shape>`, 1.4 `<crossfall>`, `<crossSectionSurface>` | the road surface, in the lanes and the mesh |
| `<lane>`, `<width>`, `<border>`, `<height>`, `<laneOffset>`, `level` | `Lane`s with widths, heights and bank. Every lane type, sidewalks included |
| `<laneSection singleSide>` | the other side carried on from the section before |
| `<link>`, `<junction>`, direct junctions, `rule`, `direction` | a lane graph in the direction traffic drives, two-way lanes included |
| `<speed>`, road `<type>`, speed signs, `<defaultRegulations>` | `RoadNetwork::speed_limits` and `road_types` along each lane |
| lane `<rule>`, `<access>`, `<material>`, `<visibility>` | `lane_rules`, `lane_access`, `lane_materials`, `lane_visibility` |
| `<neighbor>` (1.4) | `RoadNetwork::road_neighbors` |
| `<include>` | the included file read in place |
| `<userData>`, `<dataQuality>`, `<license>` | `Provenance::additional`, `Header::license` |

## Junctions

| Element | What you get |
| --- | --- |
| `<boundary>`, `<elevationGrid>` | `RoadNetwork::junction_areas`, with a ground mesh. The junction's roads stand on the grid |
| `<junctionGroup>` | `RoadNetwork::junction_groups` |
| `<priority>` | `RoadNetwork::priorities` and `yields_to` |
| `<crossPath>` | `RoadNetwork::cross_paths` |
| `type="crossing"`, `<roadSection>` | `RoadNetwork::crossings` |
| `type="virtual"` | `RoadNetwork::virtual_junctions`, beside the lane graph |

## Lane layers

A `<lanes layer="temporary">`, as for road works, is a second set of lanes
over the permanent ones. `nearest_lane`, `lane_position`, `route` and
`advance` prefer temporary lanes. Their `_with` forms take
`LayerPolicy::PermanentOnly` to ignore them. `temporary_mesh` draws them.

## Positions

- `road_point` and `road_position` convert between points and road
  `(s, t)`.
- `lane_point` and `lane_position` do the same for a lane and an offset.
- `advance` moves along the lanes and returns every branch. `left_of` and
  `right_of` step sideways. `may_change_left` and `may_change_right` read
  the road mark's `laneChange`.

## Objects

`RoadNetwork::objects` has every `<object>` and `<objectReference>` in
world coordinates, and `object_mesh` meshes them:

- boxes and cylinders, upright by default, leaning with
  `perpToRoad="true"`;
- `<repeat>`s as copies or swept shapes, with cubic `t` and
  `detachFromReferenceLine`;
- `<outline>`s from corners or `<curveLocal>` curves, inner outlines as holes
  or areas;
- `<marking>`s, `<border>`s and painted areas as quads;
- `<skeleton>`s as tapered tubes;
- `<parkingSpace>`, `<material>` and `<userData>` as data.

`RoadNetwork::structures` has `<tunnel>`s and `<bridge>`s as the stretch of
lane they cover.

## Signals

`RoadNetwork::signals` has every `<signal>`, with the lanes it applies to and
its board's pose. It reads `<signalReference>`, `<validity>`,
`<dependency>`, `<reference>`, `<semantics>`, `<staticBoard>` and
`<vmsBoard>` with their signs, `<controller>`s and `<vmsGroup>`s. Catalogue
codes are kept, not looked up. Speed signs set speed limits.

## Road marks

`RoadNetwork::road_marks` has every `<roadMark>` as quads on the lane
surface, with explicit lines, `<type>` lines, sway, and stand-ins for
marks without lines. Kerbs and other non-paint types paint nothing.

## Railways

`RoadNetwork::switches` and `RoadNetwork::stations`, beside the lane graph.

## OpenCRG surfaces

`<surface><CRG>` on roads, junctions and objects, in all four modes, for
elevation and friction. `RoadSurface` loads the files and gives the height,
normal and friction under a point.

```rust
use xodr::{load_file, opencrg::CrgGrid, RoadSurface, SurfaceHint};

let net = load_file("maps/track.xodr")?;
let mesh = net.surface_mesh();
let surface = RoadSurface::new(&net, &mesh, |file| {
    CrgGrid::from_path(format!("maps/{file}")).ok()
});
let mut wheel = SurfaceHint::default();
let ground = surface.sample(12.0, -30.0, &mut wheel);
```

`examples/crg_data.sh` downloads five measured CRG files and writes a map
for each. `examples/crg_profile.rs` drives a wheel down a lane and writes
CSV.

## Geo reference

`RoadNetwork::geo_reference` keeps the PROJ string and `<offset>` and
applies neither. `netconvert` writes the offset with the opposite sign.

## Warnings

`Provenance::warnings` lists what a load dropped, rule breaks it read
anyway, features it ignores, and features newer than the header's version.
`Warning::road_id` names the road, or is empty for a whole junction.
`Ignored` and `NewerThanDeclared` also have a `junction_id`.

An `Ignored` warning is not a fault, so a clean map can still have
warnings. `!w.is_ignored()` filters them out.

| `Warning` | Raised for |
| --- | --- |
| `Ignored` | a feature the crate ignores, once for each road, junction, or the rest of the document, with a count. See [What it ignores](#what-the-importer-ignores) |
| `NewerThanDeclared` | an element or attribute 1.8 or 1.9 introduced, in a file declaring an older `revMinor`, once for each road, junction, or the rest of the document, with a count. The crate reads it |
| `RoadSkipped` | a road with no finite `length` above zero, one over 100 km, no `<planView>`, no geometry the importer can bake, lanes that land outside the range of an `f32`, or lanes that take the file past 2,000 km of lanes |
| `GeometryDropped` | a `<geometry>` missing its pose or `length`, with a `length` that isn't above 0 or is over 100 km, or of a shape the crate doesn't know |
| `RoadMarkLineDropped` | a road mark line with more dashes than the crate paints (100,000 on the line, or 1,000,000 in the whole file), or whose paint lands out of range |
| `TooManyCopies` | an `<object>` whose `<repeat>` or dashed `<marking>` would make over 100,000 copies or dashes, or whose copies would take the file's objects past what one load bakes |
| `JunctionObjectsDropped` | a junction's `<objects>` with no `<planView>` geometry to stand along |
| `ObjectSurfaceDropped` | an object's `<surface>` on an object with no box, such as a cylinder |
| `CrgHideIgnored` | an object's `<surface>` with `hideRoadSurfaceCRG` over a road CRG that replaces the road's height, which the spec forbids. The road CRG stays |
| `OutlineDropped` | an `<outline>` with a corner off the road or fewer than two corners, or an unfilled hole no closed outline encloses. An object left with nothing keeps its box |
| `SkeletonDropped` | a `<skeleton>` `<polyline>` with a vertex off the road or fewer than two vertices. An object left with nothing keeps its box |
| `CurveHeadingIgnored` | a `<curveLocal>` whose `hdg` leaves a gap that going on from the curve before closes. The crate goes on from the curve before |
| `OutlineGap` | an `<outline>` whose `<curveLocal>`s leave a gap over 1 cm. The crate joins it with a straight edge |
| `RepeatEndMismatch` | a `<repeat>` whose cubic `t` ends more than a millimetre from its `tEnd`. The crate follows the cubic |
| `LaneIdUnreadable` | a `<lane>` under `<left>` or `<right>` whose `id` isn't a whole number, or is 0 |
| `LaneOnWrongSide` | a `<lane>` under `<left>` with a negative `id`, or under `<right>` with a positive one |
| `LaneIdRepeated` | a `<lane>` whose `id` an earlier lane of its section has |
| `LaneDropped` | a lane with no `<width>` or `<border>` the crate can read |
| `WidthAndBorder` | a lane with `<border>`s in a lane section that also has `<width>`s |
| `BorderWithLaneOffset` | a border lane on a road whose `<laneOffset>` is not 0 |
| `BorderCrossesInnerLane` | a `<border>` inside the lane within it |
| `ShapeShortOfRoad` | a `<lateralProfile>` whose shapes start inside the road's edge |
| `CrossSectionWithShape` | a `<crossSectionSurface>` on a road that also has `<shape>`s or a `<superelevation>` |
| `StripDropped` | a cross-section `<strip>` with an `id` the spec doesn't allow, a repeated `id`, or no inner strip or `<width>` to place it by |
| `UnknownStripMode` | an outer `<strip>` whose `mode` is neither `independent` nor `relative` |
| `LaneNotLevel` | a lane outside a level lane that isn't level |
| `UnknownTrafficRule` | a road `rule` other than `RHT` or `LHT` |
| `UnknownLaneDirection` | a lane `direction` other than `standard`, `reversed` or `both` |
| `RoadLengthMismatch` | a road whose `length` isn't where its `<planView>` ends |
| `LinkGap` | a lane link between lanes more than 10 cm apart |
| `ConnectionDropped` | a junction `<connection>` without the roads it joins, or in a junction without an `id` |
| `LaneLinkDropped` | a junction `<laneLink>` whose `from` or `to` isn't a whole number |
| `JunctionLinkMissing` | an incoming road whose `<link>` leaves out its junction |
| `PriorityDropped` | a junction `<priority>` naming a road the load didn't bake |
| `JunctionReferenceDropped` | a `<junctionReference>` naming a junction the file lacks |
| `UnknownJunctionGroupType` | a `<junctionGroup>` with a missing `type` or one the spec doesn't allow |
| `RoadSectionDropped` | a crossing's `<roadSection>` naming a road the load didn't bake, or an end off it |
| `CrossPathDropped` | a `<crossPath>` naming a road, lane or `s` the load didn't bake |
| `BoundarySegmentDropped` | a junction `<boundary>` segment naming a road or lane the load didn't bake, or a station more than 1 cm off its road |
| `BoundaryNotClosed` | a junction `<boundary>` whose segments don't meet |
| `BoundaryClockwise` | a junction `<boundary>` that runs clockwise |
| `ElevationGridDropped` | a junction `<elevationGrid>` the crate can't place |
| `TransitionDropped` | a boundary `joint` with a `transitionLength`, in a junction with a grid, without a `lane` segment each side |
| `VirtualJunctionWithoutMainRoad` | a virtual junction without a `mainRoad` and stretch on a baked road |
| `VirtualLinkDropped` | a virtual junction link the crate can't place |
| `VirtualLaneDropped` | a lane pair of a virtual link that names a lane its road doesn't have there |
| `NeighborDropped` | a road `<neighbor>` the crate can't read |
| `RailwayDropped` | a railway `<switch>` or platform `<segment>` the crate can't place |
| `SpeedLimitDisagrees` | a speed limit sign whose limit differs from the `<speed>` record it overrides |
| `SpeedLimitDropped` | a `<speed>` whose `max` or `unit` the crate can't read, or whose limit is too large for an `f32` |
| `AccessDropped` | an `<access>` whose `rule` is neither `allow` nor `deny` |
| `VisibilityDropped` | a `<visibility>` with a distance the crate can't read |
| `ContactPointMissing` | a road `<predecessor>` or `<successor>` naming a road without a `contactPoint`. The crate joins that road's start |
| `RoadMarkDropped` | a `<roadMark>` at the same `sOffset` as the next on its lane, which replaces it. One at or past its section's end goes without a warning |
| `IncludeDropped` | an `<include>` naming no file, one that can't be read, one that includes itself, nests over 8 deep, reads past 256 MiB, isn't XML or has the wrong root |
| `SignalDropped` | a `<signal>` without a readable `s` or `t`, or off the ends of its road |
| `SignalLinkDropped` | a signal's `<dependency>` or `<reference>`, a `<signalReference>` or a `<control>` naming a signal or object the load didn't bake, a junction's `<controller>` naming no controller, or a `<signalReference>` off its road |
| `CrgDropped` | a road or junction `<CRG>` without a `file`, a stretch on the road, a `mode` the crate knows (`global` for a junction's), or a `purpose` of `elevation` or `friction`, or a road one past the 2,000 km budget |
| `ObjectDropped` | an `<object>` without a readable `s` or `t`, or an `<objectReference>` without one or naming no object |
| `RepeatDropped` | a `<repeat>` without a readable `s`, `length` or `distance`, or with a negative one of the last two |
| `MarkingDropped` | an object `<marking>` without a width, naming fewer than two corners or a corner no outline has, or with a `side` the crate doesn't know |
| `StructureDropped` | a `<tunnel>` or `<bridge>` without a readable `s` or `length`, or with a negative `length` |
| `LaneSectionDropped` | a `<laneSection>` that starts past the end of its road. One under 1 mm long inside the road goes without a warning |
| `RecordDropped` | an `<elevation>`, `<superelevation>`, `<crossfall>`, `<laneOffset>` or road `<type>` without a readable `s`, a `<shape>` without a readable `s` or `t`, or a lane `<width>` or `<border>` without a readable `a` |
| `RecordValueUnreadable` | a number of a profile record that isn't one, such as `b="NaN"`. The crate reads it as 0 |
| `RecordsOutOfOrder` | records of one kind, such as a lane's `<width>`s, that don't come in ascending order of where they start. The crate sorts them |

## What the importer ignores

The importer skips what the sections above do not list as read. Each feature
below raises an `Ignored` warning, once for each road, junction, or the rest
of the document, with how many times the file uses it.

Uses of elements it reads that it ignores:

| Feature | What the crate does |
| --- | --- |
| a second `<lanes>` of one layer | leaves it out |
| a `<roadSection>` of a junction that is not a crossing | leaves it out |

A first `<lanes>` of either layer raises nothing.

The importer reads a lane section's `<center>` for the road marks on the
center line, and for the lane's `id`, `type`, `level` and `<link>`, which are
in `Provenance::center_lanes`. Lane 0 never becomes a `Lane`, so the lane
graph does not follow its link.
