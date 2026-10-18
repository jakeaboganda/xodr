# Changelog

## Unreleased

### Breaking

- The enums of spec values and importer outcomes are `#[non_exhaustive]`,
  so a later release can add a variant without breaking a `match`:
  `LaneType`, `LaneAdvisory`, `RoadType`, `ObjectType`, `Extent`, `Shape`,
  `RoadMarkType`, `RoadMarkWeight`, `LaneChange`, `LineRule`, `Unit`,
  `Semantic`, `RoadUser`, `SignalBoard`, `StructureKind`, `SwitchPosition`,
  `CrgMode`, `CrgPurpose`, `JunctionGroupKind`, `Additional`, `MeshError`
  and `ImportError`. A `match` on one needs a `_` arm.
- `Provenance`, every `*Provenance` record in it, `LaneHeight`,
  `SkeletonIds`, `LaneLinkOverlap` and `SurfaceSample` are
  `#[non_exhaustive]`, so a later release can add a field. Only the crate
  builds them. The types the `RoadNetwork::with_*` builders take stay open
  to struct literals, so another importer can still build a network.
- `SwitchPosition::Unknown` holds a `position` the spec doesn't list.
  Before, such a switch was dropped with a `RailwayDropped`. One with no
  `position` still is.

- A road mark's `width` is its `<type>`'s `width` where it has one, as the
  1.9 schema says the type's width supersedes the `<roadMark>`'s. Before,
  the `<roadMark>`'s won. A line without a width still falls back to the
  `<roadMark>`'s in a type of several lines.

### Fixes

- A `<lane>` whose `id` sign disagrees with its `<left>` or `<right>`
  is dropped with a new `Warning::LaneOnWrongSide`. Before, its id alone
  set its side, so it moved across the road. A second lane with an `id`
  already in its section is dropped with a new `Warning::LaneIdRepeated`.
  Before, both baked and lookups by id found only the first.
- A second `<junction>` with an `id` already used is dropped with a new
  `Warning::JunctionIdRepeated`. Before, its connections replaced the
  first's, while other parts of the load read both.
- An `<include>` inside another `<include>` no longer panics. The outer one
  replaces it, as before.
- A map that includes itself is read once, and `./name` and `name` count as
  one file, so a self-include is a `Cycle` rather than copies of the map
  down to the depth limit. Includes are spliced in one pass, so a file of
  many includes no longer takes quadratic time.
- A road whose `length` is zero or negative is skipped with
  `RoadSkipReason::NoLength`. The spec requires a length above zero.
  Before, it baked with no lanes, or failed the load when it was the only
  road.
- A lane id of `-2147483648` no longer panics in a debug build.
- A `<material>` friction or roughness that is negative, or too large for
  an `f32`, reads as `None`. Before, it could be infinity.
- A road mark `<line>` with a `length` and a `space` of 0 is continuous,
  as its dashes touch. Before, it was dashed, and a short `length` could
  take it past the dash limit and drop it.
- `<access>`es at one `sOffset` merge the same way in any order. Before,
  an unreadable one could hide the readable ones beside it, and a third
  entry could let a denied user back in. A `deny` of `none` and other
  users denies those users. Before, it lifted every restriction.
- A `Polyline` with a point that is not finite no longer panics in
  `point_at` or `pose_at`.
- `Polyline::project` of a point that is not finite gives a NaN `s` and
  `offset`. Before, it said the point lay on the line's start. A repeated
  point no longer reports an offset of 0 for a point beside it.
- `RoadNetwork::lane_point` and `road_point` give `None` for an offset or
  `t` that is not finite, as `nearest_lane` does. Before, they gave a NaN
  point. `road_position_on` an unknown road returns at once rather than
  searching the whole map, and a deserialized section whose start is past
  its end no longer panics a position query.
- An object `length`, `width` or `radius` of 0 or less, or a negative
  `height`, reads as not given, on the object and along its `<repeat>`.
  The spec rules them out. Before, `radius="0"` beside a `length` and
  `width` made a cylinder of no size, and a negative one an inside-out
  solid.
- A `<border>` round a whole inner outline that is not closed stays open.
  Before, it got an edge back to its first corner.
- A `<curveLocal>` arc without a `length` whose way round to the next
  corner is over 100 km, the most a `length` may be, is bridged with a
  straight edge and an `OutlineGap`. Before, a slight curve could lay a
  corner thousands of kilometres out.
- The center lane's paint on a temporary lane layer takes its height from
  that layer's `<laneOffset>`. Before, it used the permanent layer's, so on
  a crowned road it floated over or sank under the road.
- A junction `<boundary>` far from the origin, as in a map in projected
  coordinates, keeps its winding. Before, `f32` rounding could flip it,
  raise a false `BoundaryClockwise` and reverse a correct ring.
- A lane `<link>`'s `id` may have spaces round it, as every other id may.
  Before, such a link was dropped without a warning.
- `MeshSampler::height_at` finds every triangle `Mesh::height_at` does.
  Its boxes are grown by the most a point can lie outside a triangle and
  still count, which is twice what they were grown by.
- A `<repeat>` sweep of one section draws one end cap, not two in one
  place.
- Only an XML declaration with nothing but comments before it is removed
  before parsing, and only once. Before, a second pass could cut from a
  `<?xml` inside a comment to a later processing instruction, losing the
  roads between.
- A road skipped as `OutOfRange` no longer drops the links of an earlier
  road with the same id.
- An object `<marking>` or `<border>` visits only the dashes each edge of
  its path overlaps. A dashed marking along a finely sampled curve took
  over a second per marking, and a file of many could hang the import.
- An object's `<border>`s stop at the object piece budget as they are
  built, with a `TooManyCopies`. Before, every border was built first, so
  a small file of many borders round a large outline could take gigabytes
  before the budget refused it.
- A dashed marking with a large negative `startOffset` paints the dashes
  that reach its path. Before, the dashes before the path counted toward
  the dash limit and dropped the marking.
- A junction's `<controller>` naming no top-level `<controller>` raises a
  `SignalLinkDropped`, as every other dangling signal link does. Before,
  it was dropped without one.
- A route through a two-way lane leaves it by the end it didn't come in
  by, and enters a two-way successor at the end it meets. Before, a route
  could enter a two-way lane and leave it at the same end, a U-turn that
  drove none of it.
- A boundary `joint` whose `contactPoint` is a station, as the other
  segments may give, blends its transition from that end of the road.
  Before, any value but `end` blended from the road's start.
- A lane's length sums in `f64`, so a 16 km lane is no longer about 2 m
  too long.
- A road's geometry at a station is found by binary search, so a long
  plan view of short records no longer costs a scan per sampled point.

### Docs

- A board sign's and a display area's dependencies, and a sign's
  references, say they fall back to the board's, then the signal's. They
  left out the board.
- The speed semantic's example type is `zone`, as the schema spells it,
  not `zoneBegin`. `RoadMark::weight` says an unknown weight is `Standard`.

### OpenUSD

- `xodr_usd` prints each warning the load raises, but those for parts the
  crate only ignores, and writes the stage whole or not at all. It refuses
  to write the stage over the map.
- `usd/SCHEMA.md` says a road with no lane triangles has no prim, and that
  type attributes hold `xodr`'s names, such as `on-ramp`, not the map's.
- `usd/check.py` compares both corners of a mesh's extent, and reports a
  mesh with no geometry or a signal with no support mesh rather than
  crashing. `usd/flatten.py` exits with an error when it can't open the
  stage or write the result.

### Viewer

- The page loads a CRG that includes other files through
  `$ROAD_CRG_FILE`, as `viewer_export` does. Before, the page refused it.
- `viewer_export` writes each scene whole or not at all. Before, a failed
  write left a truncated scene that `--refresh` took as up to date.
- The page escapes an error's message unless it wrote the message as HTML,
  shows why the WebAssembly baker failed to load, reports a failure to keep
  the opened files, refuses a `?scene=` URL on another site, and draws no
  grid for a map with no road surface.
- `viewer_export --refresh` fails, naming the folder, when there is no
  `viewer/web` where it runs. Before, it said every scene was up to date.

### Build

- The workspace uses Cargo's resolver 3, which prefers dependency versions
  that build on the declared MSRV of 1.85.

### Tests

- The `height_at` budget test and benchmark probe at `(x, y)`, the ground
  plane. They passed the elevation as `y`, so most probes missed the road
  and timed the miss path. `docs/design.md` gives the measured hit cost,
  about 0.3 us.

## 0.7.1 - 2026-10-18

The crate changes only in its docs. The rest of this release is the
OpenUSD exporter and the test maps, which are not published.

### Docs

- `RoadNetwork::speed_limits` says speed limit signs and default
  regulations set limits too. It said limits came from `<speed>` records,
  not signs, which 0.7.0 changed.
- The README drops its line on C++, bindings and the old crate name.

### Test maps

- `tests/data/city.xodr`, from `city.py`, is a seaside town for the
  viewer and OpenUSD, loosely after Barcelona: 25 junctions on a sloping
  grid of chamfered blocks, one-way streets, a diagonal avenue with a tram
  line, six-arm plazas, seafront T-junctions and a roundabout, with
  traffic lights and their controllers, crossings, signs that set the
  lanes' limits, and buildings, parks, trees, lamps and parked cars. Its
  junctions have a boundary and an elevation grid that follow the slope,
  so their ground has no holes.

### OpenUSD

- An object takes a colour from its outline's `fillType` or its type, so
  buildings are sand, trees brown, vegetation and grass green, and lamps
  and poles dark grey. `usd/SCHEMA.md` lists them. Before, every object
  but paint was grey.
- A signal type's class takes its country in capitals, so a catalogue's
  `OPENDRIVE_...` and `DE_...` classes style signals whose files write
  `OpenDRIVE`, as the schema spells it, or `de`. Before, ASAM's
  `UC_LHT_Complex-TrafficLights` drew its traffic lights and stop lines as
  plain boxes under the sample catalogue.

## 0.7.0 - 2026-10-17

The crate now reads every element and attribute of the OpenDRIVE 1.9
schema.

### Upgrading

- A `Provenance` literal needs the new `additional` field.
- `BoardSign` has `lanes`, `dependencies` and `references`, and
  `DisplayArea` has `lanes` and `dependencies`. Literals need them.
- `Warning::ElevationGridNotApplied` is gone. A junction's roads now stand
  on its grid.
- Speed limit signs now change `speed_limits` where a map gives them
  semantics.

### Meshes

- A lane whose `<laneOffset>` kinks sharply no longer folds a triangle of
  the surface mesh over. Where a rib pinched at a corner leaves a quad
  concave, the crate splits it along its other diagonal. ASAM's
  `UC_5Road_Junction` road 82 had three.

### Older files

- `<crossfall>` from OpenDRIVE 1.4, which libOpenDRIVE and CARLA read: the
  road falls away from its reference line at an angle, on one side or
  both. It folds into the lane heights as a `<shape>` does.

### Additional data

- **Breaking:** `Provenance::additional` keeps every `<userData>` and
  `<dataQuality>` in the file, as `AdditionalData` with the path of the
  element each is on and its road, and `Header::license` the header's
  `<license>`. A `Provenance` literal needs the new field. All three
  leave the `Ignored` list, which keeps two features: a repeated `<lanes>`
  of one layer, and a `<roadSection>` outside a crossing. An `<include>`
  inside a `<userData>` is a vendor's own and isn't read in.
- `<include>` reads in the file it names where it stands, as the spec
  says. `load_file` reads included files from the map's folder or one
  under it, `load_str_with` with a function you give, and `load_str`
  none. Includes nest at most 8 deep and read at most
  `MAX_INCLUDED_BYTES`. `Warning::IncludeDropped` and its
  `IncludeProblem` name one the crate can't read in. `<include>` leaves
  the `Ignored` list.

### Regulations

- **Breaking:** speed limit signs set the limit, over the `<speed>`
  records, as the spec says signals take preference. A `<speed>` semantic
  of type `maximum` or `zone` sets one, and `maximumEnd` or `zoneEnd` ends
  it. The limit runs along each lane the sign applies to, in its direction
  of travel, to the next speed sign or the end of the road. The spec
  leaves the reach open. The crate stops at the road's end, and reads no
  limit from catalogue codes, so a sign before OpenDRIVE 1.8 sets none.
  `Warning::SpeedLimitDisagrees` names a sign whose limit differs from the
  record it overrides.
- `Header::default_regulations` gives the header's `<defaultRegulations>`:
  each `RoadRegulation` and `SignalRegulation`, with its semantics. A
  road type's default `maximum` speed is the limit where neither the
  road's `<type>` nor its lanes give one. `<defaultRegulations>` leaves the
  `Ignored` list.

### Signal boards

- A `<staticBoard>`'s or `<vmsBoard>`'s own `<validity>`, `<dependency>`s
  and `<reference>`s, which the schema gives them, stand between the
  signal's and its signs' and display areas'. A sign with its own
  `<positionInertial>` or `<positionRoad>` stands there, not on its board.
- `RoadNetwork::vms_groups` gives each `<vmsGroup>` as a `VmsGroup`: the
  variable message boards on one gantry, each a `VmsBoardReference` to its
  signal with its `vmsIndex` and `groupIndex`. A reference naming no signal
  raises `Warning::SignalLinkDropped`. `<vmsGroup>` leaves the `Ignored`
  list.
- **Breaking:** a sign's own `<validity>`, `<dependency>`s and
  `<reference>`s override its signal's, as the spec says.
  `BoardSign::lanes`, `dependencies` and `references` hold the sign's own,
  or the signal's where it gives none. A sign's dependency may name another
  sign on its board, as `SignTarget::Sign`. A link naming nothing the load
  baked raises `Warning::SignalLinkDropped`. The three leave the `Ignored`
  list.
- **Breaking:** `DisplayArea::lanes` and `dependencies`, from a display
  area's own `<validity>` and `<dependency>`s, which the spec's text gives
  it though its schema does not.

### Crossings

- A deprecated `<connection type="virtual">`'s `<predecessor>` or
  `<successor>`, or a road link with an `elementS`, whose `elementType`
  isn't `road` is dropped, with `Warning::VirtualLinkDropped`.
- `RoadNetwork::crossings` gives each `<junction type="crossing">` as a
  `Crossing`: where roads cross at one level without joining, such as a
  road and a railway. Each `CrossingSection`, from a `<roadSection>`, has
  its road, the stretch from `sStart` to `sEnd`, and the part of each lane
  there. An `sEnd` before its `sStart` reads as the stretch between them.
  `Warning::RoadSectionDropped` names a section the crate can't place.
  `<roadSection>` leaves the `Ignored` list, but for one in a junction that
  is not a crossing, which the spec doesn't allow.

### Junction elevation grids

- **Breaking:** the roads of a junction with an `<elevationGrid>` stand on
  it, as the spec says the grid overrides the heights of the junction's
  roads. Their elevation, superelevation and lateral shape give way to the
  grid's height, and their lane heights add to it. Lanes, the mesh, road
  marks, objects and signals on them follow. Where the grid doesn't reach,
  the road's own elevation and superelevation hold. In ASAM's
  `UC_5Road_Junction`, 19 lane links that joined lanes 0.9 to 1 m apart in
  height now meet.
- **Breaking:** `Warning::ElevationGridNotApplied` is gone.
- A boundary `joint`'s `transitionLength` blends the road coming in into
  the grid, over the spec's zone and with its cubic. The spec divides the
  slopes by the zone's depth. The crate multiplies, which makes the cubic
  meet them. `Warning::TransitionDropped` names a joint without a `lane`
  segment each side to measure the zone along. `transitionLength` is no
  longer on the `Ignored` list. `UC_5Road_Junction`'s last 0.1 m step,
  where road 411 meets road 323, is gone.
- `xodr_usd` takes the junction lanes' heights from the crate, grid and
  blend included, and sets the ground between them on the grid as before.

### Viewer and OpenUSD

- `viewer_export` keeps meshes as typed buffers, written with `f32`
  digits, and finds each item's provenance by its id. The 60 km A9 test
  field exports in 2.8 GB rather than 6.6 GB, in 27 s rather than 39 s,
  and its scene is 689 MB rather than 842 MB. A map at the 100,000-object
  cap takes 1.4 GB rather than 6.3 GB. The scene's keys and values are
  unchanged.
- `xodr_usd` finds each object's, signal's and controller's provenance by
  its id rather than by a scan.

### Warnings

- `Warning::NewerThanDeclared` for an element or attribute that OpenDRIVE 1.8
  or 1.9 introduced, in a file whose header declares an older `revMinor`. The
  crate reads it anyway. It comes once per feature for each road, junction,
  or the rest of the document, with a count. A file with no version raises
  none.
- `Warning::ContactPointMissing` for a road-to-road link without the
  `contactPoint` the spec requires. The crate still joins the start.
- `Warning::RoadMarkDropped` for a `<roadMark>` at the same `sOffset` as
  the next on its lane, which replaces it.
- `Warning::SignalDropped` for a `<signal>` the crate can't place, and
  `Warning::SignalLinkDropped` for a `<dependency>`, `<reference>`,
  `<signalReference>` or `<control>` naming a signal or object the load
  didn't bake.
- `Warning::CrgDropped` for a road or junction `<CRG>` the crate drops. A
  `<CRG>` whose `purpose` is neither `elevation` nor `friction` is now
  dropped too. Before, it was read as an elevation.
- `Warning::ObjectDropped`, `Warning::RepeatDropped`,
  `Warning::MarkingDropped` and `Warning::StructureDropped` for an object,
  a `<repeat>`, an object `<marking>`, or a tunnel or bridge the crate
  drops for lacking what places it.
- `Warning::LaneSectionDropped` for a `<laneSection>` that starts past
  the end of its road, which the crate drops. One under 1 mm long inside
  the road, which exporters write where two sections meet, still goes
  without a warning: town07 has 82.
- `Warning::RecordDropped` for a profile record the crate drops: an
  `<elevation>`, `<superelevation>`, `<laneOffset>` or road `<type>`
  without a readable `s`, a `<shape>` without `s` or `t`, or a lane
  `<width>` or `<border>` without `a`.
- `Warning::RecordValueUnreadable` for a number of a profile record the
  file writes but the crate can't read, such as `b="NaN"`. The crate reads
  it as 0, as before.
- `Warning::RecordsOutOfOrder` for records of one kind that don't come in
  ascending order, which the crate sorts, as before. ASAM's own
  `Ex_Slip_Lane` raises one, for road 66's elevations.

### Fixes

- A `<repeat>` that runs past the end of its road keeps the copies on the
  road, and only they count toward what one load bakes. Before, a repeat
  far longer than its road was refused whole.
- A `<repeat>` whose copies grow or shrink is sized by the smaller of its
  end copies. Before, one that shrank from a large first copy could be
  refused though it fit.
- An object refused for its size no longer counts the paint of its
  markings against the load, so the objects after it keep theirs.

## 0.6.1 - 2026-10-12

### Fixes

- A number attribute with spaces round it, such as `length=" 100"`, is
  read. The schema allows them. Before, the road, lane or record was
  dropped.
- `outer`, `closed` and `useCompleteOutline` read `0` and `1` as well as
  `false` and `true`.
- A `<repeat>` whose `length` is a whole number of steps keeps its last
  copy when the division rounds down, as `0.7 / 0.1` does.
- An `<outline>` the crate cannot place raises `Warning::OutlineDropped`,
  and an object left with no outline keeps its box. Before, the object
  vanished without a warning.
- An `<objectReference>` can name an object of a junction. Its
  `referenced_from` is empty. Before, the reference was dropped.
- A `<geometry>` whose poses overflow, such as a spiral with
  `curvEnd="1e308"`, is dropped with `Warning::GeometryDropped`. Before, a
  road with no lanes kept it and baked its objects at NaN.
- A `<paramPoly3>` whose tangent turns past half a turn keeps its heading
  continuous. Before, the last stretch of such a U-turn put its lanes on
  the wrong side.
- The `<line>`s of a road mark `<type>` with several lines no longer each
  take the type's `width`, which spans all of them. A line with no width
  of its own takes the mark's. A type of one line still lends it its width.
- A `<marking>` under an object paints only the first outline that has
  every corner it names. Before, it painted every outline that reused
  those corner ids.
- A road link with an `elementS` joins nothing on any road, as on a road in
  a virtual junction. Before, outside one it joined the start of the road
  it names.
- A lane `<speed>` the crate cannot read ends the one before it and holds
  no limit. Before, the limit before it ran on.
- `lane_position` finds a point whose road `s` lands a rounding error
  short of a lane section. Before, it returned `None`.
- Under `LayerPolicy::Both`, `lane_position` takes a temporary lane only
  for a point between its borders, as documented. Before, a point past the
  outermost temporary lane was on it.
- `RoadNetwork::road` finds a road given to `with_roads` out of order, as
  `lane` and `object` already do. Before, every query through it returned
  `None`.
- `Lane::bank_at` and `Lane::width_at` hold the last value of a profile
  shorter than the centerline. Before, they panicked.
- `route` stops on the goal lane only where it enters it behind the goal,
  and a lane change that lands too far along, on the goal lane or on one
  before it, no longer hides a longer way that lands behind it. Before, it could return a route that ended past
  the goal, or one to a goal behind the start that no loop reaches.
- The OpenUSD stage escapes a carriage return or other control character in
  a string from the map. Before, a name such as `Sh&#13;ed` wrote a raw one
  and the stage did not parse.
- In the OpenUSD stage, a skeleton pole that leans holds the board on it,
  a nearer pole object with no prim no longer hides one with a prim, and a
  board that reaches more than 0.5 m over a lane counts as over traffic, so
  it gets an arm or a span rather than a roadside pole.
- `viewer_export` reads a CRG file, and each file it includes, only from
  the map's folder or one under it once links are followed, and only a
  regular file of at most 1 GiB. An include is found beside the file that
  names it, without `$VARIABLE`s. Before, a map could name any path, such
  as `/dev/zero`.
- `viewer_export --refresh` skips a `<CRG file=...>` it can't read instead
  of panicking on a non-ASCII character after `file=`.
- The viewer sets its clipping planes from the camera's distance to what it
  looks at. Before, they were sized once for the whole map, so on a large
  map a close look at a sign or lane cut it away.
- One load bakes at most `MAX_BAKED_LENGTH`, 2,000 km, of lanes and road
  CRG stretches, a metre of CRG counting as eight. A road past it is
  skipped with `RoadSkipReason::MapTooLarge`, and a road CRG past it is
  dropped. Before, an 84 KB file of one 100 km road with 1,000 lanes, or
  one with hundreds of long `<CRG>` records, used gigabytes of memory.
- One load bakes at most 100,000 objects and 2,000,000 outline corners,
  skeleton vertices, sweep sections and marking and border pieces, an
  `<objectReference>` counting again. That is room for three rows of posts
  every 2 m along a 60 km highway, and over twice the guardrails of the 60
  km A9 test field. An object past it is left out with
  `Warning::TooManyCopies`. Before, a few kilobytes of repeats or
  references to one object used gigabytes.
- A road `<CRG>` with no `file` is dropped, as its documentation said.
- A document whose elements nest more than 256 deep is an
  `ImportError::Malformed`. Before, one nested tens of thousands deep
  overflowed the XML parser's stack and aborted the process.

## 0.6.0 - 2026-10-11

### OpenUSD objects

- A junction's object names its junction in `xodr:junction`, the attribute
  lanes and roads in a junction already use, instead of an empty
  `xodr:roadId`.
- An object with a `<skeleton>` carries its polylines as flat arrays, as
  USD writes curves: `xodr:skeletonCounts`, `xodr:skeletonPoints`,
  `xodr:skeletonRadii`, `xodr:skeletonGround`, and the ids in
  `xodr:skeletonIds` and `xodr:skeletonVertexIds`.
- A signal on a skeleton pole, or hung under its arm, is held by that pole
  rather than by a pole the exporter adds. The pole's surface must come
  within 0.5 m of the board, up and down as well as across, so a sign on the
  ground under the arm keeps its own pole. Before, only a solid pole could
  hold a signal it was not named by.
- `tests/data/skeleton.xodr` has a light under the arm of its traffic light
  pole, a sign on the pole, a sign on the ground under the arm, and a sign by
  no pole.
- An area of paint takes the paint colour of its material's `roadMarkColor`,
  or white, as road marks do, instead of grey. Each object from an outline
  with a `fillType` carries it in `xodr:fillType`.
- In `tests/data/outline_markings.xodr`, the "Patch" paint area has a
  yellow `roadMarkColor`.
- `usd/SCHEMA.md` says the stage carries no OpenCRG data, an object's
  `<surface>` CRG included.

### Docs and viewer

- The README shows what the crate builds, in pictures of the test maps in
  the viewer. `docs/support.md` puts each picture beside its feature, and
  `docs/design.md` has a diagram of how a map flows through the crate.
- `docs/images/shots.py` takes the pictures again from the test maps.
- The viewer opens on the view a `view=` address parameter names, and `v`
  writes the current view there, so a link opens an exact view.
- The viewer colours an outline with a `fillType` by its fill, so the grass,
  cobble and concrete of an island read apart. The legend lists the fills.
- `tests/data/object_surface.py` writes the CRG files its map names, so the
  viewer can show them. `tests/data/object_poses.xodr` has a plumb post and
  a leaning one side by side.

### OpenUSD export

- New `xodr-usd` tool exports a map as an OpenUSD stage. Run
  `cargo run -p xodr-usd -- map.xodr`.
- `sh usd/run.sh map.xodr` shows the stage in the browser. See
  `usd/README.md`.
- `usd/SCHEMA.md` describes the stage. The stage records its schema
  version.
- Signals export, each with its board, its OpenDRIVE codes as labels, and
  links to its lanes and the signals and objects it names. Each board
  inherits a type class that a catalogue layer can override. Controllers
  export too.
- Each signal links to what holds it up: a pole the map has, or a
  structure the exporter adds. Beside the road, that's a pole. Over
  traffic, it's a cantilever, a span gantry or a space frame. AASHTO's
  limits on span, sign area and lanes pick which.
- Signs at one spot share a structure, whichever way they face. No
  structure stands in traffic. Road paint gets none.
- `usd/catalogues/sample.usda` sets the look and labels of the signal
  types in the test maps. `usd/flatten.py` stacks catalogues over a stage
  and flattens it, and `usd/check.py` checks stages with OpenUSD in CI.
- `usd/run.sh` takes catalogues and flattens each stage before the page
  draws it. The page shows signals and the structures that hold them up,
  with their attributes and labels. Double-click a prim to frame it.
- `tests/data/structures.xodr` has a cantilever, two span gantries and a
  space frame.
- Each junction's overlapping lanes export as one wrap: a surface that
  covers each point once, on the lane that owns the ground there, split
  into one mesh per lane type. A lane more than 2 m over another, such as a
  bridge, keeps its own level. Lanes inside a junction keep their prim and
  attributes but no geometry. Schema version 2.
- A junction with a `<boundary>` exports as the spec lays out its ground:
  the wrap covers everything inside the boundary, and stands on the
  `<elevationGrid>` with each lane's `<height>` on top.
- `tests/data/junction_wraps.xodr` has fourteen junctions, each laid out
  to stress the wrap.
- `tests/data/gantries.xodr` is a demo of gantry scenes: lane control,
  a sign bridge, a message board and traffic lights.

### Object poses

- Breaking, behaviour change: an object stands upright. Its `pitch` and
  `roll` are against the horizontal, as the schema says, and `zOffset`
  raises it straight up. Before, it leaned with the grade and bank of the
  road, as in libOpenDRIVE. This is a departure from figure 20 of the spec,
  which places objects in the road's tilted frame. The spec contradicts
  itself there, and the crate follows the schema.
- `perpToRoad="true"` is read. Such an object follows the road surface at
  its origin and ignores its `pitch` and `roll`. The spec says it is
  perpendicular to the road at all points. The crate turns the whole object
  by the road at its origin, so a large outline does not bend with the road.
- `tests/data/object_poses.xodr` has the same box and outline on a graded,
  banked road, upright and following the road.
- Behaviour change: a `<repeat>`'s `bT`, `cT` and `dT` are read. With any
  of them, `t` is a cubic from `tStart` and `tEnd` plays no part, so
  coefficients of 0 hold `t` at `tStart`. New
  `Warning::RepeatEndMismatch` where the cubic misses `tEnd`. A negative
  coefficient, which the schema forbids, is read as written.
- Behaviour change: `detachFromReferenceLine="true"` is read. The repeat
  runs on the straight line between its two ends.
- `tests/data/repeats.xodr` has cubic, detached and mismatched repeats on
  an arc.
- Behaviour change: a `<marking>` inside an `<outline>` is read, as
  OpenDRIVE 1.9 writes it. `Ex_Parkingspace_rhomboid` and `Ex_Objects` lost
  these markings before.
- Behaviour change: a closed outline with `fillType="paint"` lies flat, 5 mm
  above its corners, with no height. A border round it does not cut it
  back, as the spec says it does.
- `tests/data/outline_markings.xodr` has markings inside outlines and areas
  of paint on a banked road.
- Behaviour change: `<curveLocal>` outlines are read, as lines, arcs and
  paramPoly3 curves in the object's frame, cut into corners.
  `Ex_SmoothObjectOutline_traffic_island` gets its island. New
  `Warning::OutlineGap` where one curve ends away from the next.
- Two of that file's curves give a `hdg` a right angle off. Where a curve's
  `hdg` leaves a gap and going on from the curve before closes it, the
  crate departs from the `hdg` and goes on from the curve before, with new
  `Warning::CurveHeadingIgnored`.
- Behaviour change: an inner outline, `outer="false"`, with a `fillType` or
  a `laneType` is an object of its own as well as a hole in the outline
  round it, with its own markings and borders. Before, it was only a hole,
  so the concrete and cobble areas of `Ex_Objects` and
  `UC_2Lane-RoundAbout-3Arms` were lost.
- `tests/data/inner_outlines.xodr` has filled, laned and plain inner
  outlines, inside the outer one and outside it.
- `tests/data/curve_local.xodr` has a stadium, a circle, a paramPoly3, a gap
  and a curve that cannot be drawn.
- Behaviour change: a junction's `<objects>` are read. They stand along the
  junction's `<planView>` reference line, at height 0 plus their `zOffset`.
  New `Warning::JunctionObjectsDropped` for a junction with objects and no
  reference line.
- Breaking: `ObjectProvenance` has a new `junction_id` field, so a
  literal one needs it. An older serialized one loads with it empty. A
  junction's object has an empty `road_id`.
- `tests/data/junction_objects.xodr` has a tee whose reference line carries a
  box, an outline, a repeat and a reference to a road's object.
- An object's `<surface>` CRG is read. `RoadSurface::sample` adds it to the
  road height inside the object's box, and it hides an `attached` road CRG
  under it unless `hideRoadSurfaceCRG="false"`. Breaking: new
  `CrgMode::Object` and `CrgObject`. Match on `CrgMode` with an arm for it.
- The file turns with the object's heading only, not its pitch and roll,
  which the spec also turns it by. New `Warning::ObjectSurfaceDropped` for a
  surface on an object with no box, and `Warning::CrgHideIgnored` for one
  that hides a road CRG of a mode the spec does not let it hide.
- Breaking: an object's `<skeleton>` is read, as new `Shape::Skeleton` of
  `SkeletonVertex` polylines, each vertex with a radius. `object_mesh` makes
  each segment a frustum, mitred where the polyline bends. Match on `Shape` with an arm for it. The spec's
  text also gives vertices a width and a height, which its schema does not,
  and the crate reads the schema. A negative radius reads as 0. New
  `Warning::SkeletonDropped` for a polyline it cannot place. An object left
  with nothing keeps its box.
- Breaking: `ObjectProvenance` has a new `skeleton_ids` field, of the new
  `SkeletonIds`, with the `id`s of a skeleton's polylines and vertices. A
  literal `ObjectProvenance` needs it. An older serialized one loads with it
  empty.
- `tests/data/skeleton.xodr` has the spec's traffic light and tree, a
  repeat, a reference and polylines that cannot be placed.
- `tests/data/object_surface.xodr` has object CRGs on bare roads, over
  `attached` and `attached0` road CRGs, on repeats and in a junction.

### Single-side lane sections

- Behaviour change: a `<laneSection singleSide="true">` is read. The sides
  it leaves out carry on from the sections before, so each section the crate
  bakes has both sides. Before, such a section read as the whole
  cross-section, and the other side stopped at its start.
- `tests/data/single_side.xodr` has sections that give the left, the right
  and each, with carried widths, heights, marks and links.

### Lane layers

- A road's temporary lane layer is read. Both `<lanes>` bake, each with its
  own lane sections and `<laneOffset>`. A temporary section may give a
  `length`. Before, a file that listed the temporary layer first baked it as
  the road, and the other was dropped.
- New `Layer`, `Lane::layer`, `LaneProvenance::layer`, and
  `RoadNetwork::temporary_mesh`. `surface_mesh` is the permanent layer's
  lanes alone, since the layers share one surface.
- A lane's `<link>` may name several lanes, each with a `layer`, and a
  junction `<laneLink>` has `fromLayer` and `toLayer`. A `<validity>` has a
  `layer`.
- Temporary lanes take precedence for driving. `nearest_lane`,
  `lane_position`, `route` and `advance` find them where they hold the
  point. New `LayerPolicy`, and `nearest_lane_with`, `lane_position_with`,
  `route_with` and `advance_with` take it.
- A temporary lane traffic may not use, such as a restricted one or one
  closed for works, hides the permanent lane under it from `nearest_lane`.
- Breaking: `Lane` has a new `layer` field. An older serialized network
  without it loads as permanent. Behaviour change on a map with a temporary
  layer: its lanes are now in the network, so the lane count and
  `Lane::successors` change, and so does a lane section's index.
- The viewer tints temporary lanes amber, lifts them a centimetre off the
  shared surface so both draw and hover, and names the layer in the readout
  and the sidebar.
- `tests/data/lane_layers.xodr` has roads with temporary layers, links
  between the layers, a layer listed first, validities of each layer and a
  junction link between temporary lanes.

### Ignored features

- New `Warning::Ignored` names each feature of a file the crate ignores,
  such as `<userData>`, a further `<lanes>` of one layer and
  `perpToRoad="true"`. It comes once per feature for each road, junction,
  or the rest of the document, with a count. `docs/support.md` lists them.
- `Warning::is_ignored` tells these from the warnings for faults. A map
  that loads clean can still raise them.
- `tests/data/ignored.xodr` uses each one.
- The viewer lists them with the other warnings.

### Lane types

- Behaviour change: a lane of `type="walking"`, the name OpenDRIVE 1.8 gives
  a sidewalk, is a `LaneType::Sidewalk`. It was `LaneType::Unknown`.
- Breaking: new `LaneType::Shared` for `type="shared"`, a lane shared by all
  traffic participants. It was `LaneType::Unknown`. Match on `LaneType` with
  a wildcard arm.
- Behaviour change: a `shared` lane counts as drivable, so traffic changes
  lanes into it and out of it.

### Metadata

The crate now keeps descriptive attributes it builds nothing from.
`tests/data/metadata.xodr` has all of them, and `metadata_bare.xodr` none.

- New `RoadNetwork::header` and `Header` give a `<header>`'s `revMajor`,
  `revMinor`, `name`, `version`, `date`, `vendor` and bounds. The crate
  still never reads the version to decide how to read a map.
- New `Road::name`, and `Provenance::junctions` of `JunctionProvenance`,
  with each `<junction>`'s `id` and `name`.
- New `RoadNetwork::road_countries` and `road_country_at` give a road
  `<type>`'s `country` along each lane.
- `LaneProvenance` has a lane's `advisory`, `dynamic_lane_type`,
  `dynamic_lane_direction` and `road_works`. New `LaneAdvisory` names the
  `advisory`.
- New `Provenance::center_lanes` of `CenterLaneProvenance` has the center
  lane's `id`, `type`, `level` and `<link>` for each lane section. The lane
  graph does not follow the link, since lane 0 is no `Lane`.
- `ObjectProvenance` has the object's `invalidated` and `temporary`, the
  `marking_weights` of its markings and the `material_road_mark_colors` of
  its materials, and the `fill_type` and `outline_lane_type` of the outline
  it baked from.
- `RoadMarkProvenance` has the mark's `material` and its `<type>`'s `name`.
- `SignalProvenance::signs` has a `SignProvenance` for each sign on a static
  board, with its `id`, `country_revision`, `dynamic`, `invalidated`,
  `temporary`, `orientation`, `h_offset`, `pitch`, `roll` and `length`. The
  crate turns the sign by none of them. `Switch::partner` is a new `Partner`
  with the partner's `id` and `name`. Breaking: it was the id alone.
- New `Provenance::lane_link_overlaps` of `LaneLinkOverlap` has each
  junction lane link's `overlapZone`.
- The viewer shows all of it on hover. The map list shows the `<header>`.
  A lane shows its road's and junction's `name`, its road type's `country`,
  its `advisory` and flags, and each junction lane link's `overlapZone`. A
  mark on the center line shows the center lane. Objects, road marks and
  signs show their new attributes, and a switch its partner's `name`.
- Breaking: `Switch`, `LaneProvenance`, `ObjectProvenance`,
  `RoadMarkProvenance`, `SignalProvenance` and `Provenance` have new fields
  or types, so a struct literal needs them. `LaneProvenance` has `layer` too,
  from the lane layers below. `Object`, `Marking` and `BoardSign` keep their
  fields. Each new field has a serde default, so a network serialized before
  still loads.

### Signals

- Behaviour change: a signal applies to the lanes whose traffic runs the way
  its `orientation` names, as the spec says, not to a side of the road. A
  reversed lane, a lane under `rule="LHT"` and a two-way lane now get the
  right signals, so `Signal::lanes` changes for them. A `<validity>` still
  picks its lanes by id.
- `tests/data/signal_direction.xodr` has signals on roads with reversed,
  two-way and left-hand lanes.

### CRG surfaces

- Behaviour change: where several `<CRG>` records of one purpose cover a
  point, the last in the file wins, not the first, as rule
  `road.crg.use_last_entry` has it. A junction's records still win over its
  roads'. The spec doesn't rank them.
- `tests/data/crg_order.xodr` has roads and a junction with overlapping
  records.

### Object types

- Breaking: new `ObjectType::RoadSurface` for `type="roadSurface"`, which
  replaces the deprecated `patch`. It was `ObjectType::Unknown`. Match on
  `ObjectType` with a wildcard arm.
- `tests/data/types.xodr` holds every lane type and object type the schema
  names.

## 0.5.0 - 2026-10-03

### Renamed to `xodr`

- The crate is now `xodr`, renamed from `libopendrive`, which was easily
  confused with the C++ library libOpenDRIVE. To upgrade, replace
  `libopendrive` with `xodr` in `Cargo.toml` and in your `use` paths.
  Nothing else changes.
- The viewer crate is now `xodr-viewer`, so run the exporter with
  `cargo run -p xodr-viewer -- map.xodr`. Files opened in the viewer under
  the old name won't reopen after a reload. Open them again.

## 0.4.1 - 2026-10-03

### Lane changes

- `RoadNetwork::may_change_left` and `may_change_right` say whether the
  road mark between a lane and the lane beside it lets a vehicle cross, from
  the mark's `laneChange`. `None` where there is no lane beside, no mark, or
  a `laneChange` the crate does not recognise. Where two marks meet, the one
  starting there answers. The crate reads `laneChange` whatever the mark's
  type, since the spec does not tie the two.
- The viewer shows whether a hovered lane may change left and right, or
  `n/a` where no mark or no lane says.
- `viewer_export` records the `.xodr` each scene in `viewer/web/` came
  from, in `sources.json`. `viewer_export --refresh` bakes again every
  scene older than its `.xodr` or than `viewer_export`, and
  `viewer/build.sh`, and so `viewer/run.sh`, runs it. So the `map` list
  stays current with the viewer.

### Speed limits

- Fixed: a `<speed>` whose limit is too large for an `f32`, such as
  `max="1e39"`, became an infinite `SpeedLimit::Max`. It is now dropped
  with a `Warning::SpeedLimitDropped`, as an unreadable one is.

### Viewer

- The viewer opens a `.xodr` itself. Click `open .xodr` and pick a map and
  any `.crg` files it uses. The page bakes the map with `libopendrive-viewer`
  compiled to WebAssembly. A reload keeps the map.
- `sh viewer/run.sh` builds the page, serves it on localhost and opens it in
  your browser.
- `viewer_export` moved from `examples/viewer_export.rs` to the
  `libopendrive-viewer` crate in `viewer/`. The page's WebAssembly build
  uses the same crate. Run it with
  `cargo run -p libopendrive-viewer -- map.xodr`.
- A CRG file that does not load, or a mesh that is not a valid trimesh, is
  now listed with the map's warnings in the sidebar.

### Fixes

- Fixed: the viewer wrote a road's id into the lane tooltip as HTML, so a
  map could run script in the page. It now shows the id as text.
- Fixed: a `<geometry>` whose `length` is 0 or less made the whole load
  panic. The crate now drops it with the new `Warning::GeometryDropped`,
  which it also raises for a geometry missing its pose or of a shape it
  doesn't know. It used to drop those without a word.
- Fixed: a road, geometry or lane section with a huge length, such as
  `1e13`, made the load run out of memory or never finish. A road or
  geometry over the new `MAX_LENGTH`, 100 km, is now skipped with a
  warning (`RoadSkipReason::TooLong` or `Warning::GeometryDropped`). A
  negative `<laneSection s>` counts as 0.
- Fixed: a small file with many road mark lines of tiny dashes made
  millions of quads, 228 MB from 2 KB. The cap on dashes was per line. A
  load now paints at most 1,000,000 dashes. A line over that, or over
  100,000 on its own, paints nothing and raises the new
  `Warning::RoadMarkLineDropped`. Before, a line over 100,000 dropped
  without a word.
- Fixed: a lane section shorter than a millimetre, such as one at
  `s = length` that many exporters write, cut every link across it, with
  no warning. Links now step over it to the next section or road.
- Fixed: a `<laneSection>` without `s` was read as starting at 0 but
  ending the section before it at the road's end. The road got two sets of
  lanes on top of each other, and a false `LinkGap`. A missing `s` now
  means 0 everywhere, as libOpenDRIVE reads it.
- Fixed: a signal's `width`, `height`, `length` or `zOffset`, a
  `<positionInertial>` coordinate, or a board sign's size or offset, too
  large for an `f32` (such as `1e39`) became infinity. Each now reads as
  missing, as a value like `inf` already did.
- Fixed: a road mark or line `width`, `tOffset`, `length`, `space`,
  `height` or `sOffset` too large for an `f32` became infinity, and a
  huge width or offset made every corner of the paint NaN. Each now reads
  as missing. A line whose paint still lands out of range paints nothing,
  with a `RoadMarkLineDropped` warning.
- Fixed: one huge number in a road, such as an elevation, lane width,
  lane offset, superelevation or start point of `1e39`, gave its lanes and
  the mesh infinite points, with no warning. The road is now skipped with
  `RoadSkipReason::OutOfRange`.
- Fixed: `RoadNetwork::route` drove backwards along a one-way lane when
  the goal was behind the start on the same lane, or behind the point a
  lane change entered the last lane. It now goes round the network to reach
  such a goal, or returns `None` if it can't, and never runs against a
  lane's traffic.
- Fixed: the surface mesh's normals ignored the road's grade, so on a
  climb they pointed straight up. `Mesh::height_at` and `MeshSampler`
  pass them on, and `RoadSample::up` points callers to them for the grade.
  They now lean with grade and bank alike.
- Fixed: of several `<access>`es or `<rule>`s at one `sOffset`, only the
  last was kept. An `allow` and a `deny` there now merge into the
  `allow` less the denied users, and rules join with `"; "`.
- Fixed: an object, sweep or CRG whose stretch of road ended exactly where
  a lane section starts also took that section's lanes. It now stops at the
  boundary. A single station on a boundary still counts in the section that
  starts there.
- Fixed: the end caps of a swept object, such as a guard rail, were
  turned by the line from its first section to its last. On a sweep bending
  past a right angle a cap faced into the rail. Each cap now faces along
  its own end.
- Fixed: on a junction `<elevationGrid>` whose rows have different
  lengths, a point on the edge of a fully covered square could get no
  height, so the junction mesh dropped to the boundary there. Every square
  the point is on is now tried.
- Fixed: an object whose `<repeat>` or dashed `<marking>` would make
  over 100,000 copies or dashes vanished without a word. It now raises the
  new `Warning::TooManyCopies`.
- Fixed: a `<lane>` under `<left>` or `<right>` whose `id` isn't a
  whole number, or is 0, was dropped without a word. It now raises the new
  `Warning::LaneIdUnreadable`.
- Fixed: a junction `<laneLink>` whose `from` or `to` isn't a whole
  number, and a `<junction>` without an `id` with all its connections,
  were dropped without a word. The first now raises the new
  `Warning::LaneLinkDropped`, and each connection of the second a
  `ConnectionDropped`.
- Fixed: a virtual junction link's lane pair naming a lane its road
  doesn't have was dropped without a word, which could leave a link with
  no lanes. It now raises the new `Warning::VirtualLaneDropped`.
- Fixed: a junction boundary segment whose `sStart`, `sEnd` or
  `contactPoint` lies off its road was quietly moved to the road's end. One
  more than 1 cm off is now dropped with `BoundarySegmentDropped`.
- Fixed: `nearest_lane` and `sample_near` of a point that isn't finite
  returned a lane instead of `None`. In a hand-built or deserialized
  network, a lane with a NaN point also beat every other lane. Now it never
  wins.
- Fixed: `with_speed_limits` and the other builders for stretches along
  lanes panicked on a stretch whose `from` was NaN, and so did
  deserializing one. Such a stretch is now left out.
- Fixed: `min_elevation` returned NaN when the first point it looked at
  had a NaN height. It now skips NaN heights.
- Fixed: `surface_mesh` made triangles with no area wherever a lane's
  width is 0, as where a lane opens out of nothing. A collider builder can
  choke on those. They are now left out. `Mesh::validate` didn't catch
  them, nor a leftover index that makes no whole triangle. It now reports
  both as `MeshError::DegenerateTriangle`.
- Fixed: `Mesh::height_at` panicked on a triangle naming a vertex the
  mesh lacks, and `MeshSampler` then read the wrong triangles. Such a
  triangle now covers nothing. Only a hand-built mesh can have one.
- Fixed: `MeshSampler` missed a point a hair outside a triangle, or on
  the far edge of the mesh, that `Mesh::height_at` finds, though the docs
  say the two always agree.
- Fixed: `object_mesh` panicked on an open `Shape::Outline` with no
  corners. It now meshes to nothing. Only a hand-built object can have one.
- Fixed: `Signal::heading`, documented in `(-π, π]`, could be exactly
  `-π` after rounding to `f32`. It is now `π` there, and so are a
  signal's pitch and roll.
- Fixed: `examples/crg_data.sh` worked only from the repository root, and
  kept a half-downloaded `.crg` file forever, so later runs failed on it.
  It now runs from any folder, and downloads to a `.part` file it renames
  only once the download is complete.
- Fixed: `examples/crg_profile.rs --step 0` ran forever, and a negative
  or unreadable value was taken without a word. A value that isn't a
  number, or a step that isn't above 0, is now an error.
- Fixed: `examples/crg_to_xodr.rs` wrote the CRG file's name into the
  map unescaped, so a name with `&`, `<` or `"` made a map that doesn't
  parse. It is now escaped.
- Fixed: `examples/crg_to_xodr.rs` could end a road with a segment a
  fraction of a micrometre long, when the grid's length was a hair over a
  multiple of 0.25 m. Its curvature came out huge. The last segment is now
  between 0.025 m and 0.275 m long. A grid too short for a road is an error.
- Fixed: the published crate included `examples/crg_data.sh`, which
  builds the viewer and so can't run outside a git checkout. It is now left
  out, like the tests.
- Fixed: `viewer_export --refresh` gave up on a scene whose recorded
  `.xodr` had moved, as after moving the checkout, instead of trying
  `tests/data`. It now falls back, and records the new path.
- Fixed: `viewer_export --refresh` said every scene was up to date right
  after naming scenes it couldn't check. It now says "every other scene"
  then.
- Fixed: `viewer_export --refresh` didn't notice a changed OpenCRG file,
  so a scene's heat map could go stale. It now also compares each `.crg`
  file the map names.

### Docs

- The docs of a `<cornerRoad>` corner said its top rises along the road's
  normal. It rises straight up, as the crate docs already said elsewhere.
  Both now say so, and that libOpenDRIVE raises it along the normal.
- `Pose::heading` and `Polyline::tangents` said they are always unit
  vectors. They are zero where a hand-built polyline turns straight back or
  repeats a point, and now say so.

## 0.4.0 - 2026-09-30

### Virtual junctions

- Fixed: a road in a `type="virtual"` junction whose `<predecessor>` or
  `<successor>` gives an `elementS` meets that road part way along it. It
  was joined to the road's start instead. Its lanes now get no lane-graph
  link there, nor does the main road's, whether or not it names the
  junction, and the main road is no longer linked into the junction with a
  `JunctionLinkMissing`. Deprecated `type="virtual"` connections are not
  reported as dropped.
- `RoadNetwork::virtual_junctions` gives each `<junction type="virtual">`
  as a `VirtualJunction`: its `MainRoad` and the stretch of it it spans, its
  orientation, and each `VirtualLink` where a road meets another part way
  along, with the lanes it joins. Each side is a `LinkPoint`. The router
  does not follow them.
- `Warning::VirtualJunctionWithoutMainRoad` names a virtual junction
  without the main road and stretch the spec requires. It keeps its links.
  `Warning::VirtualLinkDropped` names a link the crate can't place.
- The viewer draws each virtual junction's stretch and links, and names them
  on hover.

### Railways

- `RoadNetwork::switches` gives each road's `<railroad><switch>` as a
  `Switch`: the point on its main and side tracks, each a `TrackPoint`,
  which way it is set, and its partner. `RoadNetwork::stations` gives each
  `<station>` as a `Station`, with its platforms and the stretches of track
  they run beside. The router does not follow switches.
- `Warning::RailwayDropped` names a switch or a platform segment the crate
  can't place.
- The viewer draws each platform and switch, and names them on hover.

### Signal semantics and boards

- `feat!`: `Signal::semantics` says what a signal means, from its
  `<semantics>`: each a `Semantic`, such as a maximum speed, a stop line or
  the `RoadUser`s it bars. `Signal::boards` holds its `<staticBoard>`s, with
  each `BoardSign`, and its `<vmsBoard>`s, with each `DisplayArea`, placed
  on the signal's board by their `v` and `z`. `Signal` gains both fields, so
  a `Signal` literal no longer compiles.
- The crate applies no semantic: a speed semantic sets no speed limit.
- In ASAM's `UC_5Road_Junction`, 39 of the 64 signals give their priority
  or a parking rule this way.
- The viewer's signal readout lists each semantic, and each board's signs
  and display areas.

### Junction groups

- `RoadNetwork::junction_groups` gives each `<junctionGroup>` as a
  `JunctionGroup`: its id, name, `JunctionGroupKind` and the junctions it
  names, each once. `RoadNetwork::junction_groups_of` finds every group a
  road's junction is in, through the new `Road::junction`. The router does
  not use them.
- `Warning::JunctionReferenceDropped` names a reference to a junction the
  file lacks, and `Warning::UnknownJunctionGroupType` a `type` the spec
  does not allow, or a missing one, read as unknown.
- The viewer's lane readout names the lane's junction and its groups.

### Cross-section surfaces

- A road's `<crossSectionSurface>` shapes its surface as the spec gives
  it: strips each side of the reference line, shifted by a `<tOffset>`, each
  a cubic across the road whose coefficients are cubics along it. It bakes
  as `<shape>`s bake: each lane border stands on it, lanes go straight
  across, and level lanes keep out of it. ASAM's `Ex_Camber_Straight_Profile`
  and `Ex_Roof_Profile_1Strip` now have their camber and roof.
- `Warning::CrossSectionWithShape` names a road with a cross-section surface
  and shapes or superelevation, which the spec forbids. The crate adds them.
  `Warning::UnknownStripMode` names an outer strip whose `mode` is neither
  `independent` nor `relative`, read as `independent`, the crate's reading
  of a missing one. `Warning::StripDropped` names a strip the crate can't
  place: an unknown or repeated `id`, or an outer strip with no inner strip
  of a width beside it.

### Cross paths

- `RoadNetwork::cross_paths` gives each `<crossPath>` as a `CrossPath`: the
  crossing road, and at each end the lane it joins, the `s` along that
  lane's road, and the crossing road's lane. It joins lanes part way along
  them, so it stays beside the lane graph. `Provenance::cross_paths` names
  each one's junction and id.
- Breaking: `Provenance` has a new `cross_paths` field, so a `Provenance`
  literal needs it. Provenance serialized before loads with none.
- `Warning::CrossPathDropped` names one whose roads or lanes the load
  lacks, and the road it can't be placed on. In ASAM's
  `UC_5Road_Junction`, three name an `s` past the end of their road or a
  lane it lacks.
- The viewer's readout for a crossing road's lane, and for a lane a cross
  path joins, names the other end.

### Junction areas

- `RoadNetwork::junction_areas` gives each junction with a `<boundary>` or
  an `<elevationGrid>` as a `JunctionArea`: the boundary as a closed ring on
  the road surface, and the grid, read bicubically as the spec gives it by
  `JunctionArea::height_at`. `JunctionArea::mesh` triangulates the ground
  inside the boundary at the grid's height.
- The grid doesn't replace the junction's roads' heights, as the spec says
  it does, nor blend into the roads coming in. `Warning::ElevationGridNotApplied`
  says so. A grid needs one straight `<planView>` `<line>` and a
  `gridSpacing` above 0, or it is dropped with `Warning::ElevationGridDropped`.
- `Warning::BoundaryNotClosed`, `BoundarySegmentDropped` and
  `BoundaryClockwise` name a boundary the spec doesn't allow. In ASAM's
  `UC_5Road_Junction`, road 384's lane offset leaves a 15 cm gap.
- `Warning::road_id` is empty for a warning about a junction as a whole.
- The viewer draws each junction's ground and outlines its boundary.

### Road neighbors

- `RoadNetwork::road_neighbors` lists each road `<neighbor>`, an element of
  OpenDRIVE 1.4 that 1.9 no longer has, as a `RoadNeighbor`: the road beside
  a road, its `Side`, and whether it runs the same way. The crate derives no
  lane changes from it. libOpenDRIVE reads it too, as raw strings, and
  checks none of them.
- `Warning::NeighborDropped` names one naming no baked road, or with a
  `side` or `direction` 1.4 doesn't allow.
- The viewer's lane readout lists the roads beside the lane's road.

### Junction priority

- `RoadNetwork::priorities` lists each junction `<priority>` as a
  `Priority`: the road with priority and the road that gives way to it.
  `RoadNetwork::yields_to` reads them for one road, and
  `Provenance::priorities` names each one's junction. They serialize with
  the network.
- Breaking: `Provenance` has a new `priorities` field, so a `Provenance`
  literal needs it. Provenance serialized before loads with none.
- `Warning::PriorityDropped` names a `<priority>` whose `high` or `low` is
  no road the load baked, and its `road_id` is the one the load has.
- The viewer's lane readout says which roads the lane's road gives way to,
  and which give way to it.

### Lane direction

- `feat!`: a lane's `direction` overrides the way its side of the road runs.
  `reversed` runs it the other way, and `both` makes it a new
  `Direction::Both` lane. `Direction` gains the variant, so a `match` on it
  without a wildcard no longer compiles.
- A two-way lane's successors are the lanes off both of its ends, and its
  predecessors those that drive into it at either end. From one, only lanes
  whose traffic runs away from the joint count. In ASAM's
  `Ex_Bidirectional_Junction`, road 2 now leads back into roads 5 and 6.
  Two two-way lanes that join lead into each other both ways, even where
  only one names the other.
- A lane of the deprecated `type="bidirectional"` with no `direction` runs
  both ways, as the spec says `direction="both"` replaces it.
- A lane change is only ever to a lane running the same way.
  `RoadNetwork::route` drives a two-way lane toward the next lane on the
  route, and `RoadNetwork::advance` goes each way from one.
- `Warning::UnknownLaneDirection` names a `direction` other than
  `standard`, `reversed` or `both`, which the crate reads as `standard`.
- A signal still picks its lanes by side, not by the way each runs.
- The viewer's lane readout says `both` for a two-way lane, and its arrow
  points both ways. The marker steps each way along one.

### Links across a gap

- `Warning::RoadLengthMismatch` names a road whose `length` is more than
  1 cm from where its `<planView>` ends. The crate still bakes the road to
  its `length`. ASAM's `UC_T_Junction` road 6 is 80 m long with 111.6 m of
  geometry.
- `Warning::LinkGap` names a lane link whose lanes are more than 10 cm
  apart where one leaves off and the next begins, measured across both
  lanes from border to border. A lane that splits in two, or hands over to
  one opening beside it, meets it and raises none. The crate keeps the
  link. It catches the four ASAM examples that link lanes that don't meet,
  and `UC_5Road_Junction`'s 1 m step.
- The check measures the lanes as baked. In `Ex_CrossFall_LeftTurn` the
  file switches lanes' `level` at a seam, and the crate stacks level lanes'
  heights, so those lanes step 0.12 to 0.36 m there and warn too.

### Junctions

- Fix: a road that a common junction's connection names as its incoming
  road, but whose own `<link>` leaves the junction out, drives into it. The
  crate finds the end that meets the junction from the connecting road's
  link back to it, and raises `Warning::JunctionLinkMissing`, with the
  `RoadEnd` it linked. In ASAM's `UC_X_Junction`, road 82's three driving
  lanes into junction 1 no longer dead-end.

### Comparison

- `docs/comparison.md` sets each of the 671 elements and attributes in the
  OpenDRIVE 1.9 schema beside what this crate, esmini 3.8.2, libOpenDRIVE
  0.5.0 and CARLA 0.10.0 do with it, with the source line for each. 31
  rows have another library doing more, mostly names the others keep and
  signal logic. It also compares the queries each answers.

### Lane visibility

- `RoadNetwork::lane_visibility` and `lane_visibility_at` read each lane's
  `<visibility>`s as `Along` stretches of a `Visibility`: the distance a
  driver can see ahead, behind, left and right, in metres. Each holds to
  the next or to the end of its lane section.
- OpenDRIVE 1.9 does not define `<visibility>`. The crate reads it as
  CARLA does, and keeps the four distances as the file names them.
- `Warning::VisibilityDropped` names a `<visibility>` with a distance
  missing, not a number, or below 0. The crate drops it. CARLA reads a
  missing one as 0.
- The viewer's lane readout shows the visibility at the hovered point.

### Moving along the lanes

- `RoadNetwork::advance` moves a `LanePosition` a distance along the lanes,
  the way their traffic runs, or back for a negative distance. It gives
  every place the distance reaches, one `Advance` per branch, as CARLA
  does. A branch that runs out of lanes is an `Advance::DeadEnd` at the
  lane's end, with the distance left.
- The distance runs along the lanes' centerlines, so it is how far a
  vehicle travels. esmini and CARLA step by road `s`. The offset keeps its
  side of the traffic onto a road that runs the other way.
- The branches grow exponentially with the distance on a map with loops:
  from one lane of Town07, 72,912 places at 2 km. Step a few metres at a
  time. An infinite or NaN distance gives none.
- The crate doesn't pick a branch by a heading or a route, as esmini can.
  The caller gets them all and picks.
- `RoadNetwork::left_of` and `right_of` step to the lane beside, left and
  right of the traffic, at the same `s`.
- In the viewer, click a lane to drop a marker, and press `f` or `b` to
  step it along the lanes. It splits at a fork and turns red at a dead end.

### Lane positions

- `RoadNetwork::lane_point` turns a `LanePosition`, a lane with a road `s`
  and an offset from its center toward `+t`, into the point on the lane's
  surface. `RoadNetwork::lane_position` turns a point back, on the lane of
  any type whose borders hold its road `t`. Both are exact, and follow
  esmini's `SetLanePos`.
- `RoadNetwork::centerline_s` turns a lane position's road `s` into the
  distance along the lane's baked centerline, which `Projection`, the
  `Along` stretches and `Lane::sample_at` use. The two drift apart on a
  bend: by up to 21 % of the distance across the test corpus.
- `Projection` and the `Along` stretches stay on the centerline's `s`.
  `nearest_lane`'s point is within 1.1 cm of the exact center for 99 % of
  the points tried, so moving them to road `s` would break every caller
  for no gain in where they land.
- The viewer's lane readout shows the lane offset and the centerline's `s`
  beside the road `s` and `t`.

### Road coordinates

- The network keeps its roads. `RoadNetwork::roads`, `road` and
  `road_by_od_id` return a `Road`, with its `RoadId`, `<road id>` and
  length. `RoadNetwork::road_lane` gives a lane's road, lane section and
  `<lane id>` as a `RoadLane`.
- `RoadNetwork::road_point` turns a `RoadPosition`, a road with `s` and
  `t`, into the point on the road surface there, lane heights and lateral
  shape included. `None` off the ends of the road. Signals, objects and
  road marks stand on the road through the same call.
- `RoadNetwork::road_position` turns a point back. Of the roads whose
  lanes come near it, it takes the one whose surface is nearest in 3D, and
  solves for the `(s, t)` straight under or over the point. A point on a
  lane round-trips to within 0.2 mm on every map in the test corpus, or the
  `f32` step of its coordinates where that is larger. A station on a lane
  section seam is on the section that starts there. It takes about 5 us on
  Town07. `RoadNetwork::road_position_on` does the same on one road, for a
  caller that knows its road where roads overlap.
- Roads serialize with the network, as the records the file gives. They
  add 22 % to Town07's JSON and 1 to 3 % to esmini's maps. A network
  serialized before deserializes with none, and the road queries answer
  `None`. Network equality compares roads too, so such a network, or one
  rebuilt from its lanes alone, no longer equals a fresh import.
- Roads that don't match the network's lanes, from `with_roads` or a
  hand-edited serialized network, give `None` from the road queries.
- Import takes about 4 % longer, to index every lane's footprint.
- The viewer's lane readout shows the road `s` and `t` under the mouse.

### Geo reference

- `RoadNetwork::geo_reference` returns a `GeoReference`: the
  `<geoReference>` PROJ string, trimmed, and the `<offset>` as a
  `GeoOffset`. The importer applies neither, so points stay in the file's
  frame. A missing or unreadable offset attribute reads as 0.
- `RoadNetwork::with_geo_reference` sets it. It serializes with the network,
  and a network serialized before it deserializes with none.
- The viewer shows the PROJ string and offset under the map's name.

### Level lanes

- A lane with `level="true"` is kept out of the superelevation and the
  lateral shape. It starts at its inner neighbour's outer border and runs
  level, with its own `<height>`s on top, so its `Lane::bank` is 0. The
  lanes outside it stack on it. Objects, signals and road marks stand on
  it, also where no lane in its section has a `<height>`. The A9 Testfeld
  map has 16, beside banked roads.
- A level lane is `w / cos φ` wide in plan on a road superelevated by `φ`,
  as libOpenDRIVE builds it: 2.5 mm more on a 2 m lane at 5 %.
- Where the crate departs from the spec:
  - A lane outside a level lane is held level even where the file says it
    is not, and raises `Warning::LaneNotLevel`. The spec says it is level.
  - A level lane starts at its inner neighbour's outer height, `<height>`
    included. libOpenDRIVE starts it on the road.

### Lane rules, access and materials

- `RoadNetwork::lane_rules`, `lane_access` and `lane_materials` hold each
  lane's `<rule>`s, `<access>`es and `<material>`s as `Along` stretches.
  `lane_rule_at`, `lane_access_at` and `lane_material_at` read them at a
  point. Each holds from its `sOffset` to the next of its kind, or to the
  end of its lane section.
- A rule is the file's free text. Access is `Access::Allow` or
  `Access::Deny`, with the road users the file names. A material is the
  `Material` objects already use.
- Access in the form before 1.8, one `restriction` attribute per
  `<access>`, merges where several share an `sOffset` and a `rule`. A
  deny of `none` lifts the restrictions before it.
- `Warning::AccessDropped` names an `<access>` whose `rule` is neither
  `allow` nor `deny`. The spec makes `rule` optional, but without it an
  `<access>` doesn't say who may use the lane.
- The viewer's lane readout gives the rule, access and material at the
  hovered point.

### Speed limits and road types

- `RoadNetwork::speed_limits` and `RoadNetwork::road_types` say what holds
  along each lane, as `Along` stretches in metres along its centerline.
  `speed_limit_at` and `road_type_at` read them at a point. A limit is a
  `SpeedLimit`, in m/s or `Unlimited`, and a type a `RoadType`.
- A road's `<type>` and its `<speed>` hold to the next `<type>`. A lane's
  `<speed>` overrides its road's to the end of its lane section. A
  `<speed>` without a `unit` is in m/s, as the spec says. Limits come from
  `<speed>`s, not signs.
- `Warning::SpeedLimitDropped` names a `<speed>` the crate can't read.
- The viewer's lane readout gives the speed limit and road type at the
  hovered point.
- Where the crate departs from the spec:
  - A `<speed>` whose `max` is below 0 or not a number, or whose `unit` is
    not `m/s`, `km/h` or `mph`, is dropped with the warning. A road's
    `no limit` and `undefined` are read. A lane's are not, as the spec
    allows them only on a road.
  - A `<type>` without `s` is skipped. A lane `<speed>` without `sOffset`,
    or with a negative one, starts at its lane section.
  - Types and speeds out of order are sorted.
  - A `type` the spec does not name is `RoadType::Unknown`.

### Direct junctions

- Fix: a `type="direct"` junction's connections link its roads. The crate
  required a `connectingRoad` on every `<connection>`, so it dropped every
  direct connection, which names a `linkedRoad` instead. On esmini's
  `soderleden.xodr`, the six drivable lanes that meet at junction 8 now
  link across it.
- `Warning::ConnectionDropped` names a connection without an
  `incomingRoad`, or without the road it leads into. The crate used to
  drop it silently.
- Where the crate departs from the spec:
  - A direct connection also links its linked road back into its incoming
    road, as esmini reads it, unless the junction gives that connection
    itself. The spec does not say.

### Left-hand traffic

- Fix: a road with `rule="LHT"` drives its left lanes along `+s` and its
  right lanes against it. The crate used to read every road as right-hand
  traffic, so on esmini's `e6mini-lht.xodr` every `Lane::direction`,
  successor and predecessor was reversed. A signal's `orientation` picks
  its lanes by the same rule.
- Where the crate departs from the spec:
  - A `rule` other than `RHT` or `LHT` reads as `RHT`, the spec's default,
    and raises `Warning::UnknownTrafficRule`. esmini also reads `lht` as
    left-hand traffic.
- The viewer's travel arrows and link colours follow the rule, and its
  sidebar lists the new warning.

### Warnings

- Breaking: `Provenance` has a new `warnings` field, so a `Provenance`
  literal no longer compiles. It lists what a load dropped from a bad file,
  in file order, as `Warning`s. A clean file has none.
- `Warning` is an enum with one variant per kind, marked `#[non_exhaustive]`.
  Each names where in the file it happened, and `Display` gives the message.
  It derives `serde` under the feature.
- `Warning::RoadSkipped` names a road with no finite `length`, no
  `<planView>`, or no `<geometry>` the crate can bake. `RoadSkipReason`
  says which.
- `Warning::LaneDropped` names a lane with no `<width>`, which the crate
  drops along with its road marks. The spec draws those marks.
- Elements the crate does not read raise no warning.
- The viewer lists the warnings at the top of its sidebar. Picking one
  lights the lanes of the road it names.
- `Warning::road_id` gives the road any warning happened on.

### Lane borders

- A lane's `<border>`s give the `t` of its outer border, measured from the
  reference line. The lane's width is its border less its inner
  neighbour's outer border, so `Lane::width` and `Lane::widths` follow it,
  and a lane can open out of nothing. Such lanes used to be dropped.
- A width lane outside a border lane stacks on its border. Road marks run
  along a border, and links and lane-change neighbours hold as for any lane.
- `Warning::LaneDropped` now means a lane with neither a width nor a border.
- Where the crate departs from the spec:
  - A lane with widths follows them, and a lane with only borders follows
    those, even in a lane section where other lanes have widths. The spec
    uses the widths when a section has both. Raises
    `Warning::WidthAndBorder`.
  - A border ignores `<laneOffset>`. The spec forbids the two together.
    Raises `Warning::BorderWithLaneOffset` where the offset is not 0.
  - A border that crosses inside the lanes within it gives its lane 0
    width. The spec forbids it. Raises `Warning::BorderCrossesInnerLane`.
  - A border without `a` is skipped, and a missing `sOffset`, `b`, `c` or
    `d` is 0, as for a width. The spec requires all five.
  - Borders out of order are sorted rather than dropped.
- The viewer's lane readout gives the lane's width at the hovered point.

### Lateral shapes

- A road's `<lateralProfile><shape>`s give its cross-section, such as a
  crown. Each lane border stands at the shape under it plus the lane's
  `<height>` there, along the road's normal, and the lane goes straight
  across between them. Heights go linearly along `s` between profiles.
  So a crowned road no longer imports flat.
- A curve inside a lane is lost to the straight chord: `c w² / 4` at its
  middle, 5.5 mm on a 3.5 m lane of a crown falling 2.5 % at 7 m out.
- Each profile's `s` is a station of its lane sections.
- Objects, signals and road marks stand on the shape as the mesh has it,
  straight across each lane rather than on the spec's curve. A center
  line mark lies flat on a crown's ridge.
- The viewer's lane readout gives the cross slope at the hovered point, in
  percent.
- A tilted lane's `Lane::width` and `Lane::widths` are the chord across
  its surface, so its edges land on its borders. The plan width left each
  edge short: 1.3 mm on a 2 m lane rising 0.1 m, 7.4 cm on a 3.5 m lane
  at 30 %.
- Where the crate departs from the spec:
  - The first profile holds before it. The spec's default there is 0.
  - A profile that starts inside the road holds its first shape's value
    out to the edge, and raises `Warning::ShapeShortOfRoad`. The spec
    says each profile covers the road.
  - An `attached` CRG over a shaped road answers without the shape.
  - A shape without `s` or `t` is skipped, and a missing `a`, `b`, `c` or
    `d` is 0. The spec requires all six. Shapes out of order are sorted.

## 0.3.1 - 2026-09-27

### Docs

- The README is shorter. The element-by-element OpenDRIVE support moved to
  `docs/support.md`, and the coordinate frame, point types, meshes, `serde`,
  bad input and timings moved to `docs/design.md`.
- The `serde` snippet asks for version 0.3, not 0.2.
- The list of ignored elements no longer says signals are ignored.

## 0.3.0 - 2026-09-27

### Lane heights

- A lane's `<height>`s raise it off the road, along the road's normal. Its
  centerline rises by the height halfway across it, and `Lane::bank` adds
  the slope from its inner border to its outer one. So the surface mesh and
  `Lane::sample_at` see a raised sidewalk. The step up to it stays open, as
  libOpenDRIVE and esmini leave it.
- Objects and signals stand on the lane at their point, and a road mark on
  its own lane. So a sidewalk's outer mark rises with it, and the kerb's
  mark stays on the road.
- `LaneProvenance::heights` gives a lane's `LaneHeight`, inner and outer,
  at each centerline vertex.
- A lane section has a station wherever a lane's heights change pace, so a
  1 m kerb ramp bakes 1 m long.
- The mesh tilts a raised lane rather than moving each edge to its height.
  For a 0.1 m rise over 2 m, an edge is under 0.1 mm off its height and
  1.3 mm inside its border. A quad on a kerb ramp is twisted, and its
  triangles cut the corner by up to a quarter of the rise.
- Where the crate departs from the spec:
  - Heights go straight from one entry to the next, as libOpenDRIVE and
    esmini read them, and the last holds after it. The spec's rule for
    lane geometry holds each until the next.
  - Heights go straight across the lane from `inner` to `outer`, as both
    readers do. The spec gives only the two borders.
  - The first entry holds before it. The spec gives no height there.
  - A missing `sOffset`, `inner` or `outer` is 0, as libOpenDRIVE reads
    it. The spec requires all three. A negative `sOffset` is 0.
  - Entries out of order are sorted rather than dropped.
  - Heights on the center lane are ignored. The spec forbids them.
  - Objects, signals and road marks stand on a raised lane. The spec does
    not say whether they stand there or on the road below.
  - A point on the border between two lanes is on the inner one, as in
    libOpenDRIVE, and a point past the outermost lane takes its outer
    height. The spec says neither.
  - An `attached` CRG over a raised lane answers at road level. The spec
    measures a height from the road including its surface.
- The viewer's lane readout gives a raised lane's inner and outer height
  at the hovered point.

### Road marks

- Each `<roadMark>` imports as a `RoadMark` on `RoadNetwork::road_marks`,
  with a `RoadMarkId`. It runs along its lane's outer border, or the line
  between the two sides for the center lane, to the lane's next mark or
  the end of its lane section. `RoadMark::left` and `RoadMark::right` name
  the lanes either side of it.
- A mark keeps its `type` as a `RoadMarkType`, its `laneChange` as a
  `LaneChange`, its `weight`, width and height, and its `color` as the
  file's text.
- Its `lines` are quads in the road surface. A mark described by its type
  alone paints what esmini draws for it: one line for `solid`, 4 m dashes
  8 m apart for `broken`, and two lines one width either side of the border
  for a double type. The other types paint nothing. A line is 0.12 m wide,
  or 0.25 m bold, as in libOpenDRIVE. The spec gives none of these.
- A mark's `<type><line>`s replace its stand-ins. Each `RoadMarkLine`
  keeps its width, colour, `LinePattern`, `sOffset`, `tOffset` and
  `LineRule`. A line without a width or colour takes its type's or mark's.
- An `<explicit><line>` paints once, as `LinePattern::Single`. Each
  `<sway>` moves the mark's lines sideways by its cubic, from its `ds`.
- `RoadMarkProvenance` gives each mark's road, lane section, `<lane id>`,
  `s` and length.
- Where the crate departs from the spec:
  - A mark without a `color` is `standard`, one without a `type` is
    `none`, and one without an `sOffset` starts at its section. The spec
    requires all three.
  - A negative `sOffset` on a mark or a line is 0. The spec says it is at
    least 0.
  - A mark without a `weight` is standard, and a line without a `rule` has
    none. The spec gives no default for either.
  - A width of 0 counts as none, so the line's, the type's, the mark's and
    the weight's apply in turn. The spec says a width is above 0.
  - A line with a `length` and `space` of 0 is continuous, as esmini means
    it. The spec would paint nothing.
  - A `none` mark paints nothing even with lines, as esmini draws it.
  - `tOffset` and a sway point along +t on both sides of the road, and a
    sway moves nothing before its `ds`. Dashes are measured along the
    reference line. The spec says none of this.
  - Marks out of order are sorted rather than dropped.
  - A lane with no `<width>` does not bake, so its marks go too. The spec
    would draw them on its inner border.
- The viewer paints each road mark's lines in their colour. Hover one to
  read the mark, each of its lines with its pattern and rule, and light up
  the lanes either side. `m` toggles road marks.

### Signals

- Each `<signal>` imports as a `Signal` on `RoadNetwork::signals`, with a
  `SignalId`. It keeps the file's `country`, `countryRevision`, `type`,
  `subtype`, `value`, `unit`, `text`, and the 1.9 flags `invalidated` and
  `temporary`. `type` and `subtype` stay the file's strings.
- A signal's board stands `zOffset` straight above the road at its
  `(s, t)`, faces the traffic its `orientation` names, and is turned by
  `hOffset`. Pitch and roll are against the horizontal, so a board on a
  banked road stays upright. Angles wrap into `(-π, π]`.
- `Signal::lanes` is the side of the road its `orientation` names, or the
  lanes its `<validity>` ranges name.
- A `<positionRoad>` or `<positionInertial>` moves the board, and the signal
  still applies at its own `(s, t)`. `Signal::applies_at` is where it takes
  effect.
- A `<signalReference>` adds its road's lanes and its point to the signal it
  names. `SignalProvenance::references` records each one.
- Each top-level `<controller>` imports as a `Controller` on
  `RoadNetwork::controllers`, with the signals it controls.
  `Signal::controllers` names a signal's controllers, and
  `ControllerProvenance` lists the junctions that sync each one.
- `Signal::dependencies` and `Signal::references` keep a signal's
  `<dependency>` and `<reference>` links, as the `SignalId`s and
  `ObjectId`s they name.
- `SignalProvenance` gives each signal's road, OpenDRIVE id, `(s, t)` and
  orientation.
- The viewer draws each signal as a board, white when static and dark when
  dynamic, with its front lit. Hover one to read it, see its lanes, and
  follow a dashed line to each place it applies, which may be on another
  road. The readout names its controllers and the other signals in each,
  and the signals and objects it depends on or refers to.
  `s` toggles signals.

### OpenCRG road surfaces

- Each `<CRG>` under a road's or a junction's `<surface>` imports as a
  `CrgSurface` in `RoadNetwork::crg_surfaces`. The importer reads all four
  modes, `attached`, `attached0`, `genuine` and `global`, and both purposes,
  elevation and friction.
- `RoadSurface` loads the files through a closure. `sample(x, y, hint)`
  returns the height, up-normal, CRG grid height and friction under a point,
  in `f64`. Where no CRG covers the point, the surface mesh answers. Give each
  moving point its own `SurfaceHint`, and its next search starts where the
  last one ended.
- Friction is the grid value exactly as the file has it. OpenCRG shifts
  heights to start at 0 and leaves friction alone.
- The crate re-exports `opencrg`.
- The viewer draws the CRG heights as a heat map over the road, centred on
  their median. Hover it to read the height and friction under the cursor.
- `examples/crg_to_xodr.rs` writes a map for an OpenCRG file.
  `examples/crg_profile.rs` drives a wheel down a lane and writes the
  surface under it as CSV. `examples/crg_data.sh` downloads five CRG files
  from ASAM and Project Chrono and prepares each for the viewer.

### Viewer

- A `map` picker in the toolbar switches between every map you have baked.
  The page opens on the map `?scene=` names, and asks for one without it.
- `viewer_export` takes any number of maps, such as `tests/data/*.xodr`,
  and names each output after its map instead of `scene.json`.
- Hovering a lane draws an arrow the way traffic drives, tints its
  successors green and its predecessors orange, and lists them in the
  readout. `viewer_export` writes each lane's `successors` and
  `predecessors`.
- The sidebar lists each road's objects and signals under its lanes, in
  groups that fold. Click one to select it, frame it and read it. The
  filter matches them too, and the header names the map.

### Breaking

- The minimum Rust version is 1.85, up from 1.82, for the `opencrg`
  dependency.
- `Provenance` has new `signals`, `controllers` and `road_marks` fields, so a
  `Provenance` literal needs them or `..Default::default()`. The `serde`
  form of `RoadNetwork` has new `signals`, `controllers` and `road_marks`
  keys, so JSON written before does not load.
- `LaneProvenance` has a new `heights` field, so a literal needs it. It no
  longer derives `Eq`, since the heights are `f32`s.

## 0.2.1 - 2026-09-26

### Performance

- `nearest_lane` and `sample_near` index each lane in spans of 4 segments
  rather than whole, and project only onto the spans near the point. Their
  cost no longer grows with how finely a lane is sampled, or how long it is.
  On Town07 they take about 0.4 us a call, where 0.2.0 took 1.4 us and 0.1.1
  took 0.7 us. The answers are the same, to the bit.

### Fixes

- 0.2.0 sampled a whole lane section every 25 cm if any part of it turned
  sharply, and probed every section every 25 cm to find out. Only the road
  near a tight curve is sampled finely now, easing back out to 2 m, and
  sections are probed every metre. On Town07 that is 14,209 lane samples
  where 0.2.0 had 19,518 and 0.1.1 had 8,818, and `nearest_lane` and
  `surface_mesh` are about a quarter faster than in 0.2.0. Import is
  23% faster on Town07 and 3.9 times faster on the test track. Tight
  curves are as round as in 0.2.0.

## 0.2.0 - 2026-09-26

### Objects

`<object>`s now import. Each one bakes to one or more `Object`s on
`RoadNetwork::objects`, in world coordinates, with an `ObjectId`, an
`ObjectType`, a subtype, a name, whether it is dynamic, and a `Shape`:

- `Shape::Solid`: a plain object, placed on the road surface at its `(s, t)`
  and raised by `zOffset`. It carries its heading, pitch and roll, and an
  `Extent`: a cylinder if it has a radius, or a box if it has any of a
  length, a width and a height. As in libOpenDRIVE, an object leans with
  the grade and bank of the road under it, and `zOffset` raises it square to
  the surface. So does an outline in `cornerLocal` corners. Sweeps and
  `cornerRoad` corners still rise straight up.
- A `<repeat>` with a `distance` is one solid every `distance` metres, with
  `t`, `zOffset` and the dimensions interpolated along it. On esmini's
  e6mini that turns four objects into 794 posts.
- `Shape::Sweep`: a `<repeat>` with a `distance` of 0, a cross-section swept
  along the road, such as a guard rail or a wall. Its sections are at most
  10 m apart, and closer where it bends, so its walls stay within 1 cm of the
  road. With a radius the sweep is `round`, a pipe, and the mesh is a
  16-sided tube.
- `Shape::Outline`: one per `<outline>`, a polygon of `cornerRoad` or
  `cornerLocal` corners, each with a base and a top. An object with outlines
  gets no solid of its own. The 1.4 layout, `<outline>` straight under
  `<object>`, is read too. A `<repeat>` with a `distance` bakes the outlines
  at every step, where libOpenDRIVE bakes them once.
- An `<outline>` with `outer="false"` is a hole in `Shape::Outline::holes`
  of the closed outline round it. The mesh cuts it out of the lid and the
  floor and walls it facing in.

`RoadNetwork::object_mesh` tessellates them into one `Mesh` of
outward-facing faces, with an `ObjectSpan` per object in `Mesh::objects`, as
`surface_mesh` does for lanes. A closed outline gets a lid and a floor in the
plane of its corners, so one standing on its edge, such as a sign, gets them
too. A face with no area is left out, so a post given only a height has no
span.

`RoadNetwork::object` looks one up by its id. Its road id, `<object id>`,
anchoring `(s, t)`, `orientation` and `validLength` are in an
`ObjectProvenance`, kept apart from the object as a lane's are.

An `<objectReference>` bakes the `<object>` it names, from any road, at the
reference's `s`, `t` and `zOffset`. The object's repeats and `cornerRoad`
outlines move with it. Its provenance has the reference's `orientation` and
`validLength`, and `referenced_from` names the road the original is on. The
importer skips a reference to an id no object has.

`Object::lanes` lists the lanes an object applies to. These are the lanes
alongside the stretch of road it spans, narrowed to the `fromLane`-`toLane`
range of each `<validity>` it has. A reference uses its own `<validity>`, not
its object's.

`Object::markings` has one `Marking` per `<marking>`. Each one
has the marking's side, colour, width, line length and space length. Its
pieces are world-space quads along the edges its `<cornerReference>`s name,
cut into dashes if the marking is dashed. A marking with no corner
references paints its `side` of a solid's box, such as a parking bay's
lines.

`Object::borders` has one `Border` per `<border>`, such as a traffic island's
kerb. Each one has the border's type and width, and one world-space quad per
edge. With `useCompleteOutline` the band runs along every edge of the
outline. Otherwise it follows the `<cornerReference>`s. Marking and border
quads lie in the surface under their edges, so on a banked road they tilt
with the bank.

`Object::parking_space` holds a `<parkingSpace>`'s `access`, such as
`handicapped`, and its free-text `restrictions`. `Object::materials` has one
`Material` per `<material>`, with its `surface`, `friction` and `roughness`.
`Object::user_data` keeps each `<userData>` `code` and `value` as text.

These placements follow libOpenDRIVE.

Breaking changes:

- The `serde` form of `RoadNetwork` is now
  `{ "lanes": [...], "objects": [...], "structures": [...] }` rather than a
  bare lane array. JSON written by 0.1.1 does not load.
- `From<RoadNetwork> for Vec<Lane>` is gone, since it would drop the objects.
  Read `lanes()` instead.
- `Mesh` has a new `objects` field, so a `Mesh` literal needs it or
  `..Default::default()`.
- `load_str_with_provenance` and `load_file_with_provenance` return a
  `Provenance`, with the lane records in `lanes`, the object records in
  `objects` and the structure records in `structures`, instead of a
  `Vec<LaneProvenance>`.

### Tunnels and bridges

Each `<tunnel>` and `<bridge>` bakes to a `Structure` on
`RoadNetwork::structures`. A `Structure` has a `StructureId`, a name and a
`StructureKind`. A tunnel's kind has its type, lighting and daylight, and a
bridge's has its type.

OpenDRIVE describes neither a tube nor a deck, so a structure has no
geometry. It lists the lanes it covers instead. Each `Coverage` says where
along one lane the structure starts and ends, in metres along that lane. On
a bend that differs from the road's `s`. `<validity>` narrows the lanes.

`RoadNetwork::structures_over(lane)` returns the structures over one lane.
`StructureProvenance` has each structure's road id, OpenDRIVE id, `s` and
`length`.

## 0.1.1 - 2026-09-23

### Lane types

`LaneType` named one function, `Driving`, so a map imported as its carriageway
and nothing else. It now names every function the format defines: sidewalks,
shoulders, kerbs, medians, parking, cycle lanes, bus and taxi lanes, the ramp
family, tram and rail, the unnamed `none` surface, and the three vendor-defined
`special` types. A name the crate does not recognise bakes as
`LaneType::Unknown`, so no lane is dropped for its type and the importer
cannot leave a hole in a map without naming it.

- `surface_mesh` tessellates every lane, not only the drivable ones, so a
  collider built from it now has the footway and the median in it. The mesh is
  bigger: Town07 goes from 673 lanes to 947.
- `driving_lanes`, `nearest_lane`, `sample_near` and `route` are unchanged, and
  still see `LaneType::Driving` alone. `LaneType::is_drivable` is the wider
  predicate, and it decides which lanes are lane-change neighbours, so a route
  can no longer be planned across a sidewalk.

### Lane widths that vary

`Lane` carried one width, sampled where the lane began. A lane that opens out
of a point, such as the gore area at an off-ramp, therefore reported a width of
0 m and tessellated to nothing. It was in the lane list and it owned a slice of
the mesh, but it covered no area. Town07 has four of those, and 64 lanes whose
width varies.

`Lane::widths` now holds a width per centerline vertex, parallel to
`Lane::bank` and empty on the constant-width lanes that are most of them.
`Lane::width_at` reads it, and the tessellator uses it per rib. `Lane::width`
stays, but now means the widest the lane gets rather than the width where it
starts.

### Fixed

- A lane section ending centimetres past its last sample left a pinched rib
  beside a full-width one with no room between them, and the quad folded. What
  counts as a stub is now relative to the samples around it rather than a fixed
  10 cm, which catches the case without flattening a finely sampled curve.
- Welding a stub vertex away moved the surviving vertex onto the lane's true
  endpoint but left it holding the stub's width, which opened a seam of half a
  millimetre at some section joints. Welding now selects vertices rather than
  rewriting them, so position, tangent, bank and width cannot drift apart.
- Lanes were sampled every 2 m however sharply they turned, so a tight
  junction curve came out as a polygon, turning 35° at a sample. A section's
  step now keeps every lane edge within 0.05 rad a sample, following the
  reference line and any widening or narrowing lane, down to 25 cm.

### Viewer

- Hovering reads out the surface point's `x`, `y`, `z`, the lane heading there,
  and the lane type. The surface normal joins them behind a toggle, which also
  draws a normal hair at every mesh vertex.
- Lane centerlines and lane boundaries each draw as their own overlay, the
  centerlines in a colour of their own. Boundaries come from the mesh itself:
  a lane's vertices alternate left rib and right rib, which `LaneSpan` now
  states as a promise.
- The surface is coloured by lane type, with a legend of the types the loaded
  map contains, and the lane filter matches on type. An `unknown` lane, or one
  of a type this page has no colour for, draws in magenta so a gap in either
  table is visible rather than silently shaded like a driving lane.

### API

- `Lane::widths` and `Lane::width_at`. `Lane::width` changes meaning from the
  width at the lane's start to the widest the lane gets; the two agree on any
  lane of constant width.
- `LaneType::is_drivable` and `LaneType::as_str`, plus `Display`.
- `Polyline::tangents` is public, so a consumer can reproduce `pose_at`'s
  heading at a vertex instead of re-deriving one from the chords.

## 0.1.0 - 2026-09-22

First release. Extracted from a simulator that had grown it in-tree, and
reworked for standalone use.

### Imports

- Reference geometry: `line`, `arc`, `spiral` (clothoid), `paramPoly3`,
  `poly3`.
- `<elevationProfile>`, and `<lateralProfile>` superelevation baked as a real
  cant about the reference line.
- Per-lane widths, `laneOffset`, and multiple lane sections.
- Road/lane `<link>`s and `<junction>`s, resolved into a drive-direction lane
  graph.

### Coordinates

Baked geometry is OpenDRIVE's own frame: right-handed, Z-up, metres, with the
reference line in the X-Y plane. A point imports verbatim.

Positions are `Point` and directions are `Vector`, both defined here. There is
no math crate in the public API, and no required dependency beyond `roxmltree`
and `thiserror`.

### Queries

- `nearest_lane`, `sample_near`, and `route` over the lane graph.
- `surface_mesh` tessellation, with `Mesh::validate` and `Mesh::height_at`.

### Changes since the in-tree version

- `glam::Vec3` no longer appears in the API. A position is a `Point` and a
  direction is a `Vector`, which makes `nearest_lane(sample.up)` and adding
  two positions compile errors rather than silent nonsense. glam is gone as a
  dependency, so it no longer constrains a consumer's version of it.
- The baked frame is Z-up, matching OpenDRIVE, where the in-tree version
  rotated to Y-up for its renderer. Every coordinate moves: `(x, y, z)`
  becomes `(x, -z, y)`. Note that `height_at`'s second argument changed
  meaning from `z` to `y` without changing type, so a call site that compiles
  is not evidence it is right.
- `RoadNetwork`'s lane list is private behind `new()` and `lanes()`, so the
  network can index itself with no way for the index to go stale.
- `nearest_lane` and the new `Mesh::sampler` answer off a ground-plane grid
  rather than scanning the map. On Town07: `nearest_lane` 67x faster,
  `sample_near` 64x, `route` 4.7x, `height_at` 168x.
- `Mesh::height_at` no longer reports a `NaN` height for a query far enough
  out to overflow its barycentric arithmetic.
- `Mesh` carries a `LaneSpan` per lane, naming the slice of the buffers that
  lane contributed.
- Optional `serde` feature for the network and its mesh.

### Not yet

`<lateralProfile>` `<shape>` (per-`t` crowning and camber), lane types other
than `driving`, and visualization.
