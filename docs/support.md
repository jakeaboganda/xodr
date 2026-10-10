# OpenDRIVE support

What the importer reads from an `.xodr` file, and what it skips. For the exact
element and attribute list, see [the crate docs](https://docs.rs/xodr).

Geometry is cross-checked against the reference C++
[libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE).
[comparison.md](comparison.md) sets each element and attribute beside what
esmini, libOpenDRIVE and CARLA do with it.

## Which OpenDRIVE version

The importer keeps a `<header>`'s attributes in `RoadNetwork::header`, and its
geo reference apart. It never inspects `revMajor` or `revMinor` to decide how
to read the file, and never rejects a file for its declared version.
Whether a file loads depends only on whether it uses the elements listed
below.

Every one of those elements but a lane's `<visibility>` and a road's
`<neighbor>` is in ASAM OpenDRIVE 1.9.0, the current revision. 1.9 defines
neither, and the importer reads them from the older files that write them:
`<visibility>` as CARLA does, and `<neighbor>` as libOpenDRIVE does.
1.9.0 deprecates `poly3` in favour of `paramPoly3`, and 1.8 deprecated a
signal's `<positionRoad>` and `<positionInertial>`. The importer reads them
like any other element and raises no warning. The test suite imports real
files declaring 1.4, 1.6 and 1.7.

## Roads and lanes

- Reference geometry: `line`, `arc`, `spiral` (clothoid), `paramPoly3`,
  `poly3`.
- `<elevationProfile>`, and `<lateralProfile>` superelevation, which tilts
  the road from side to side. The cross-section rolls about the reference
  line, so an outer lane
  rides higher and its surface normal leans.
- `<lateralProfile>` shapes, so a crowned road has its crown. Each lane
  border stands at the shape under it, and a lane goes straight across
  between its borders. So a curve inside a lane is lost: 5.5 mm at the
  middle of a 3.5 m lane on a crown falling 2.5 % at 7 m out. The first
  profile holds before it, where the spec gives 0. Objects, signals and
  road marks stand on the shape as the mesh has it.
- `<lateralProfile>` cross-section surfaces, baked as shapes are: strips
  of cubics across the road, each coefficient a cubic along it.
- Level lanes, `level="true"`, kept out of the superelevation and the
  shape. Each runs level from its inner neighbour's outer border.
- Per-lane widths, `laneOffset`, and multiple lane sections.
- The whole lane cross-section, the carriageway included. `LaneType` names
  every function the format defines, among them sidewalks, kerbs, ramps and
  tram track. An unrecognised name bakes as `LaneType::Unknown` rather than
  leaving a hole. `LaneType::is_drivable` separates the ones traffic uses.
- Lane widths that vary along a lane, so a gore area that opens out of a
  point is drawn as the wedge it is.
- Lane borders, which give a lane's outer edge as a `t` from the reference
  line rather than a width. A border lane stacks on the lane inside it, and
  width lanes stack on it. Where a lane has both, its widths win. A border
  ignores `laneOffset`, and one that crosses inside the lane within it
  gives its lane 0 width. The spec forbids all three, and each raises a
  warning.
- Lane heights, so a sidewalk stands above the road beside it. A lane's
  `<height>`s raise its centerline and tilt its surface, so the mesh and
  `Lane::sample_at` see the kerb. Poles, signs and road marks on a sidewalk
  stand on it. The step up to it is left open, as libOpenDRIVE and esmini
  leave it. `load_*_with_provenance` gives the heights at each centerline
  vertex.
- Road and lane `<link>`s and `<junction>`s, resolved into a lane graph in
  the direction traffic drives.
- A road's `rule`, so a left-hand-traffic road drives its left lanes along
  `+s`. A value other than `RHT` or `LHT` reads as `RHT` and raises a
  warning.
- A lane's `direction`: `reversed` runs it against its side, and `both`
  makes it a two-way lane, linked off both of its ends. A deprecated
  `type="bidirectional"` lane with no `direction` is two-way too. An
  unknown value reads as `standard` and raises a warning.
- A junction `<laneLink>`'s `overlapZone` is in
  `Provenance::lane_link_overlaps`. The crate uses none of it.
- Direct junctions, which join roads end to end through a `linkedRoad`.
  Each connection links both ways, as esmini reads it, unless the junction
  gives the way back itself.
- A road's `<neighbor>`s, from OpenDRIVE 1.4, as
  `RoadNetwork::road_neighbors`: the road beside it, and which way it runs.
- Junction `<boundary>`s and `<elevationGrid>`s, as
  `RoadNetwork::junction_areas`: the ring round a junction's ground, and
  the height of that ground, with a mesh of it. The grid does not replace
  the height of the junction's roads, as the spec says it does, and a
  warning says so.
- Railway `<switch>`es and `<station>`s, as `RoadNetwork::switches` and
  `RoadNetwork::stations`, each on the roads of its tracks. A switch keeps its
  partner's id and name.
- `<junctionGroup>`s, as `RoadNetwork::junction_groups`: the junctions the
  file groups as one, such as a roundabout's. The router does not use them.
- Virtual junctions, as `RoadNetwork::virtual_junctions`: roads meeting a
  main road part way along it, such as a driveway. The router does not
  follow those links.
- Junction `<crossPath>`s, as `RoadNetwork::cross_paths`: paths for
  pedestrians across the junction's roads, joining lanes part way along.
- Junction `<priority>`s, as `RoadNetwork::priorities`: which road of a
  junction gives way to which.
- An incoming road whose `<link>` leaves out its common junction, which the
  spec forbids, links into it at the end its connecting road names, with a
  warning.
- Road types and speed limits, as stretches along each lane:
  `RoadNetwork::road_types` and `RoadNetwork::speed_limits`, in m/s. A
  lane's `<speed>` overrides its road's to the end of its lane section. A
  `<speed>` with no unit is in m/s. One the crate can't read raises a
  warning. Speed limit signs don't set a limit. A road type's `country`
  is in `RoadNetwork::road_countries`. The crate applies none of its rules.
- Lane `<rule>`s, `<access>`es and `<material>`s, as stretches along each
  lane: `RoadNetwork::lane_rules`, `lane_access` and `lane_materials`.
  Each holds to the next of its kind or the end of its lane section.
  Access is `Access::Allow` or `Access::Deny` with the road users the file
  names, in `<restriction>`s or the older `restriction` attribute. A deny
  of `none` lifts the restrictions before it.
- Lane `<visibility>`s, as stretches along each lane:
  `RoadNetwork::lane_visibility`. OpenDRIVE 1.9 does not define the
  element. The crate reads it as CARLA does, from older files. One with a
  distance the crate can't read raises a warning.

## Road and lane positions

- `RoadNetwork::roads` keeps each road's reference line, profiles and lane
  sections. `road_point` turns a road `s` and `t` into the point on the
  surface there, and `road_position` turns a point back. Both include lane
  heights and the lateral shape. `road_lane` gives a lane's road, lane
  section and `<lane id>`.
- `road_position` takes the road whose surface is nearest in 3D, so a point
  on a bridge finds the bridge. Where roads overlap in a junction,
  `road_position` may return any of them. `road_position_on` takes the road
  to use.
- `lane_point` and `lane_position` do the same by lane: a road `s` and an
  offset from the lane's center, as esmini's `SetLanePos` takes them.
  `centerline_s` turns that road `s` into the distance along the lane's
  baked centerline, which `nearest_lane` and the speed limits use.
- `advance` moves a lane position along the lanes by a distance, and gives
  one place per branch. On a map with loops the branches grow exponentially
  with the distance, so step a few metres at a time. `left_of` and
  `right_of` step to the lane beside.
- `may_change_left` and `may_change_right` say whether the road mark on
  that border lets a vehicle cross into the lane beside, from its
  `laneChange`.

## Objects

`RoadNetwork::objects` has every `<object>`, in world coordinates on the road
surface.

- A plain object is a box or a cylinder with a heading, pitch and roll. It
  leans with the grade and bank of the road under it, as in libOpenDRIVE.
- A `<repeat>` with a `distance` becomes one object per step, so a row of
  posts arrives as posts. A `<repeat>` with a `distance` of 0, such as a guard
  rail, becomes one shape swept along the road. With a radius it is a round
  pipe.
- An `<outline>`, in road or local coordinates, becomes a footprint polygon
  with a height at every corner. An `outer="false"` outline is a hole cut out
  of the outline round it. Under a `<repeat>` with a `distance`, the outline
  repeats at every step.
- An `<objectReference>` places the object it names again, at the
  reference's own station.
- A `<marking>`, such as a crosswalk's stripes, becomes painted quads along
  the outline edges it names, or along one side of the object's box if it
  names none. A `<border>`, such as a kerb, becomes a band along outline
  edges.
- `<validity>` picks the lanes an object applies to.
- `<parkingSpace>`, `<material>` and `<userData>` are kept on the object as
  data.

Each object has an id to look it up by, a subtype, and whether it moves,
is invalidated or is temporary. A marking keeps its `weight`, and a material
its `roadMarkColor`. `load_*_with_provenance` gives its road, OpenDRIVE id,
`(s, t)`, orientation and valid length, and the `fillType` and `laneType` of
the outline it baked from.

## Tunnels and bridges

`RoadNetwork::structures` has each `<tunnel>` and `<bridge>` as the stretch of
each lane it covers. Call `structures_over(lane)` to find out whether a lane
runs through a tunnel or over a bridge, and where.

## Signals

`RoadNetwork::signals` has each `<signal>`: its catalogue codes, `value`,
`unit` and `text` as the file gives them, the lanes it applies to, and the
pose and size of its board.

- A board stands upright even on a banked road, and faces the traffic it
  addresses.
- A `<positionRoad>` or `<positionInertial>` moves the board, such as onto a
  gantry. The signal still applies where its `(s, t)` says.
- A `<signalReference>` applies the signal on another road too, adding the
  lanes there.
- `RoadNetwork::controllers` groups the signals that always show the same
  state, such as the lights of one approach. Each signal lists its
  controllers.
- A signal keeps its `<dependency>` and `<reference>` links as the ids of the
  signals and objects they name.
- `Signal::semantics` says what a signal means, from its `<semantics>`, and
  `Signal::boards` holds its `<staticBoard>`s and `<vmsBoard>`s, each sign
  and display area placed on the board. The crate applies no semantic. A
  sign also keeps its own `id`, `countryRevision`, `orientation`,
  `dynamic`, `invalidated`, `temporary`, `hOffset`, `pitch`, `roll` and
  `length`. The crate stores them and places the sign by none of them. It
  wraps the three angles into one turn.

`load_*_with_provenance` gives its road, OpenDRIVE id, `(s, t)` and
orientation.

## Lane layers

A road may have a temporary layer, such as for road works. It is a
`<lanes layer="temporary">` next to the permanent `<lanes>`. Each layer has
its own lane sections and `<laneOffset>`, and `Lane::layer` says which a lane
is on. A further `<lanes>` of one layer is left out, with an `Ignored`
warning.

- Both layers share the permanent layer's surface. `surface_mesh` and the
  road's heights are the permanent layer's, and `temporary_mesh` has the
  temporary lanes to draw over it.
- A lane's `<link>` may name a lane on each layer, so traffic goes from the
  permanent layer onto the temporary one and back. A junction `<laneLink>`
  names its layers with `fromLayer` and `toLayer`.
- A `<validity layer>` picks the lanes of that layer, and one with no `layer`
  the permanent layer's.
- Temporary lanes take precedence for driving. `nearest_lane`,
  `lane_position`, `route` and `advance` find them where they hold the point.
  Their `_with` forms take a `LayerPolicy`, and `PermanentOnly` looks at the
  permanent layer alone. Only driving lanes take precedence in `nearest_lane`.

A link with no `layer` stays on the lane's own layer when that layer has a
section to go on to, and otherwise goes to the permanent layer. The crate docs
list where it departs from the spec.

## Road marks

`RoadNetwork::road_marks` has the `<roadMark>`s on every lane border and the
center line. Each mark names the lanes either side of it, and keeps its type,
weight, colour, width, height and which way traffic may cross it. Its lines
are quads lying on the road surface.

- A mark's `<type><line>`s and `<explicit><line>`s are its lines, each with
  its own width, colour, dashes, offset and rule. Its `<sway>`s move them
  sideways.
- A mark without them gets stand-ins. A `solid` or `broken` mark paints one
  line and a double type two, with widths from libOpenDRIVE and dashes from
  esmini.
- A kerb, a grass edge and the other types that aren't paint keep their
  meaning and paint nothing.

The crate docs list where it departs from the spec, such as reading a mark
without a colour. `load_*_with_provenance` gives its road, lane section, lane
and stretch, and its `material` and its type's `name`.

## OpenCRG surfaces

The importer keeps `<surface><CRG>` records on roads and junctions, in all
four modes, for elevation and friction. `RoadSurface` loads the OpenCRG files
they name and gives a vehicle model the height, normal and friction under a
point in `f64`. Off the CRG it answers from the surface mesh, which the CRG
does not change. Where several records of one purpose cover a point, the
last in the file wins. A junction's records win over its roads'.

The crate parses CRG files with [`opencrg`](https://crates.io/crates/opencrg)
and re-exports it, so `xodr::opencrg` also reads a `.crg` file on its
own.

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

Public maps that come with CRG files are hard to find, so
`examples/crg_to_xodr.rs` writes one for any CRG file: a road along the
file's reference line, with the file laid on it. `examples/crg_data.sh`
downloads five measured and test-course files from ASAM and Project Chrono,
and writes a map and a viewer scene for each. `examples/crg_profile.rs`
drives a wheel down a lane and writes what it rolls over as CSV.

```sh
sh examples/crg_data.sh
cargo run --release --example crg_profile -- target/crg/country_road.xodr > profile.csv
```

## Geo reference

- `RoadNetwork::geo_reference` keeps the `<geoReference>` PROJ string and
  the `<offset>` as the file gives them. The importer applies neither, so
  points stay in the file's frame.
- A missing or unreadable `<offset>` attribute reads as 0, as in esmini.
- The spec's offset takes the map's frame to the projected one. Some
  exporters, `netconvert` among them, write the opposite sign.

## Warnings

`Provenance::warnings` lists what a load dropped from a bad file, or read
from a file that breaks a rule of the spec. `Warning::road_id` gives the
road a warning happened on, or an empty string for one about a whole
junction or junction group.

| `Warning` | Raised for |
| --- | --- |
| `Ignored` | a feature the crate ignores, once for each road, junction, or the rest of the document, with a count. See [What it ignores](#what-it-ignores) |
| `RoadSkipped` | a road with no finite `length`, one over 100 km, no `<planView>`, no geometry the importer can bake, or lanes that land outside the range of an `f32` |
| `GeometryDropped` | a `<geometry>` missing its pose or `length`, with a `length` that isn't above 0 or is over 100 km, or of a shape the crate doesn't know |
| `RoadMarkLineDropped` | a road mark line with more dashes than the crate paints (100,000 on the line, or 1,000,000 in the whole file), or whose paint lands out of range |
| `TooManyCopies` | an `<object>` whose `<repeat>` or dashed `<marking>` would make over 100,000 copies or dashes |
| `LaneIdUnreadable` | a `<lane>` under `<left>` or `<right>` whose `id` isn't a whole number, or is 0 |
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
| `CrossPathDropped` | a `<crossPath>` naming a road, lane or `s` the load didn't bake |
| `BoundarySegmentDropped` | a junction `<boundary>` segment naming a road or lane the load didn't bake, or a station more than 1 cm off its road |
| `BoundaryNotClosed` | a junction `<boundary>` whose segments don't meet |
| `BoundaryClockwise` | a junction `<boundary>` that runs clockwise |
| `ElevationGridDropped` | a junction `<elevationGrid>` the crate can't place |
| `ElevationGridNotApplied` | every junction `<elevationGrid>`, since the crate doesn't move the junction's roads onto it |
| `VirtualJunctionWithoutMainRoad` | a virtual junction without a `mainRoad` and stretch on a baked road |
| `VirtualLinkDropped` | a virtual junction link the crate can't place |
| `VirtualLaneDropped` | a lane pair of a virtual link that names a lane its road doesn't have there |
| `NeighborDropped` | a road `<neighbor>` the crate can't read |
| `RailwayDropped` | a railway `<switch>` or platform `<segment>` the crate can't place |
| `SpeedLimitDropped` | a `<speed>` whose `max` or `unit` the crate can't read, or whose limit is too large for an `f32` |
| `AccessDropped` | an `<access>` whose `rule` is neither `allow` nor `deny` |
| `VisibilityDropped` | a `<visibility>` with a distance the crate can't read |

## What it ignores

The importer skips everything else in the file. Each feature below raises
an `Ignored` warning, once for each road, junction, or the rest of the
document, with how many times the file uses it.

Elements it skips: `<vmsGroup>`, a header's `<license>` and
`<defaultRegulations>`, `<dataQuality>` and `<include>`, a junction's
`<objects>` and `<roadSection>`s, and an object's `<surface>`,
`<skeleton>` and `<curveLocal>` outlines. It skips `<userData>` anywhere but
directly on an object, which it reads. It also skips the `<markings>` of an
outline, and the `<validity>`, `<dependency>` and `<reference>` of a board
sign.

Attributes it reads but ignores:

| Feature | What the crate does |
| --- | --- |
| a second `<lanes>` of one layer | leaves it out |
| `<laneSection singleSide="true">` | reads the section as two-sided |
| `<object perpToRoad="true">` | places the object by its `hdg`, as if `perpToRoad` were `false` |
| a `<repeat>`'s `bT`, `cT` or `dT`, or `detachFromReferenceLine="true"` | copies run straight along the road |
| a boundary `<segment>`'s `transitionLength` above 0 | no blending |

A `layer` of `permanent`, a flag set to `false` and a `transitionLength` of 0
raise nothing.

The importer reads a lane section's `<center>` for the road marks on the
center line, and for the lane's `id`, `type`, `level` and `<link>`, which are
in `Provenance::center_lanes`. Lane 0 never becomes a `Lane`, so the lane
graph does not follow its link.
