# Comparison with esmini, libOpenDRIVE and CARLA

This table compares what this crate does with each element and attribute of
ASAM OpenDRIVE 1.9.0 against three other open-source readers. It was drawn
up on 2026-09-29, from these releases:

| Library | Release | Commit | Files read |
| --- | --- | --- | --- |
| this crate | the commit that adds this file | | `src/parse/` |
| [esmini](https://github.com/esmini/esmini/tree/v3.8.2) | v3.8.2 | `61b44a7` | `EnvironmentSimulator/Modules/RoadManager/RoadManager.cpp` |
| [libOpenDRIVE](https://github.com/pageldev/libOpenDRIVE/tree/0.5.0) | 0.5.0 | `d88e6ab` | `src/`, mainly `OpenDriveMap.cpp` |
| [CARLA](https://github.com/carla-simulator/carla/tree/0.10.0) | 0.10.0 | `ada75f9` | `LibCarla/source/carla/opendrive/parser/`, and `road/` for what it builds |

The rows are every element and attribute reachable from `<OpenDRIVE>` in the
1.9.0 XSD schema files, 671 of them. The right and left lanes share a row,
as do a link's predecessor and successor. `<dataQuality>`, `<include>` and
`<userData>`, which the schema allows under almost every element, have a row
each at the end. An element whose schema type repeats elsewhere is listed
once, under the first element that has it: a signal's `<semantics>` kinds
are under the header's `<defaultRegulations>`, and a board sign's attributes
under `<staticBoard>`. This crate ignores `<defaultRegulations>`, but reads
every one of those `<semantics>` kinds under a `<signal>` and a board's
`<sign>`. The `<semantics>` rows under `<signal>` give where.

Each cell says what the library does with the row:

- **builds**: it computes geometry or behaviour from it: road and lane
  positions, heights, meshes, the lane graph, travel direction, junction
  links, where an object or a signal stands, or a limit or rule resolved
  onto each lane.
- **stores**: it reads the value and keeps it for the caller, but derives
  nothing from it.
- **-**: it never reads it.

An element builds if anything under it builds. A cell cites the source line
that reads the value. This crate's column comes from the element table in
the crate docs, which `tests/documented_support.rs` checks against the
parser. The other three columns were read from each library's parser source,
and a sample of rows checked by hand against it. A parser read that only
logs that the element isn't supported counts as ignored.

This crate aims to read more of OpenDRIVE than any other open-source
library. That holds when no row has another library doing more. 21 rows do,
and the next section lists them.

## Summary

Rows each library reads, of the 671 in the table. An element and each of
its attributes is its own row.

| | builds | stores | ignores |
| --- | --- | --- | --- |
| this crate | 360 | 173 | 138 |
| esmini | 176 | 54 | 441 |
| libOpenDRIVE | 146 | 57 | 468 |
| CARLA | 124 | 76 | 471 |

## Where another library does more

These are the rows where at least one other library builds or stores more
than this crate, grouped by what the other library does with them:

- **Header offset.** esmini applies `<offset>` to every point. The crate
  keeps it for the caller and applies nothing, since reprojecting is out of
  its scope. See [Geo reference](https://docs.rs/xodr/latest/xodr/#geo-reference).
- **Signal meaning.** esmini looks a signal's `country`, `type`, `subtype`
  and `value` up in a catalogue, and esmini and CARLA pick traffic lights by
  them. The crate keeps the codes and looks nothing up.
- **The center lane.** Its `id`, `level` and `<link>`. Others build lane 0
  from them. The crate keeps them, and leaves lane 0 out of the lane graph.
- **Object `type="crosswalk"`.** CARLA builds a crosswalk area from a
  crosswalk object's outline corners (`ObjectParser.cpp:36`). The crate
  keeps the type and the outline, and meshes the outline, but marks no
  crosswalk area.
- **Object and signal `name`s.** CARLA turns RoadRunner objects named
  `Speed_*` or `Stencil_STOP` into signals, and treats a signal named
  `Stencil_STOP` or `STATIC` as a stop (`MapBuilder.cpp:1054`). That is one
  exporter's convention, not OpenDRIVE. The crate keeps the names.
- **`<userData>`.** esmini textures, colours and models objects and
  tunnels from it. The crate keeps an object's pairs and draws nothing.

| Element | This crate | Another library |
| --- | --- | --- |
| &lt;header&gt; | stores | esmini builds `RoadManager.cpp:3791` (offset applied) |
| &lt;header&gt; &lt;offset&gt; | stores | esmini builds `RoadManager.cpp:3808` (offset shifts geometry) |
| &lt;header&gt; &lt;offset&gt; `@x` | stores | esmini builds `RoadManager.cpp:3808` |
| &lt;header&gt; &lt;offset&gt; `@y` | stores | esmini builds `RoadManager.cpp:3809` |
| &lt;header&gt; &lt;offset&gt; `@z` | stores | esmini builds `RoadManager.cpp:3810` |
| &lt;header&gt; &lt;offset&gt; `@hdg` | stores | esmini builds `RoadManager.cpp:3811` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@id` | stores | esmini builds `RoadManager.cpp:4288`; libOpenDRIVE builds `OpenDriveMap.cpp:383`; CARLA builds `RoadParser.cpp:206` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@level` | stores | libOpenDRIVE builds `OpenDriveMap.cpp:388` (lane 0 level only matters exactly on center line) |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; &lt;link&gt; | stores | esmini builds `RoadManager.cpp:4305`; libOpenDRIVE builds `OpenDriveMap.cpp:391` (same lane-link code as side lanes) |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@type` | stores | CARLA builds `ObjectParser.cpp:34` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@name` | stores | CARLA builds `ObjectParser.cpp:35` (name prefixes create signals) |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@name` | stores | CARLA builds `SignalParser.cpp:50` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@dynamic` | stores | esmini builds `RoadManager.cpp:4856` (dynamic selects TrafficLight) |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@country` | stores | esmini builds `RoadManager.cpp:4897` (country for type lookup) |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@countryRevision` | stores | esmini builds `RoadManager.cpp:4901` (revision affects TrafficLight choice) |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@type` | stores | esmini builds `RoadManager.cpp:4917`; CARLA builds `SignalParser.cpp:55` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@subtype` | stores | esmini builds `RoadManager.cpp:4918` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@value` | stores | esmini builds `RoadManager.cpp:4919` |
| &lt;userData&gt; | stores | esmini builds `RoadManager.cpp:5397` (object userData: texture, color; tunnel generate3DModel) |
| &lt;userData&gt; `@code` | stores | esmini builds `RoadManager.cpp:5399` |
| &lt;userData&gt; `@value` | stores | esmini builds `RoadManager.cpp:5402` |

## Queries

What each library answers about the map it built, beyond reading it.

| Query | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| Road `(s, t)` to a point | `road_point`, `network.rs:930`, on the lane surface | `Position::SetTrackPos`, `RoadManager.cpp:10099` | `Road::get_surface_pt`, `Road.cpp:139` | - (a waypoint has no `t`) |
| A point to road `(s, t)` | `road_position`, `network.rs:947` | `Position::XYZ2TrackPos`, `RoadManager.cpp:9048` | reference line `s` only: `RefLine::match`, `RefLine.cpp:86` | - |
| Lane, `s` and offset to a point | `lane_point`, `network.rs:1002` | `Position::SetLanePos`, `RoadManager.cpp:10757` | - | lane center only: `Map::ComputeTransform`, `Map.cpp:273` |
| A point to lane, `s` and offset | `lane_position`, `network.rs:1028` | `Position::XYZ2TrackPos`, `RoadManager.cpp:9048` | lane from `(s, t)`: `LaneSection::get_lane`, `LaneSection.cpp:35` | snapped to a lane center: `Map::GetWaypoint`, `Map.cpp:212` |
| Move a distance along the lanes | every branch: `advance`, `advance.rs:74` | one branch: `Position::MoveAlongS`, `RoadManager.cpp:10547` | - | every branch: `Map::GetNext`, `Map.cpp:554` |
| Step to the lane beside | `left_of`, `right_of`, `advance.rs:208` | - (`SetLanePos` with another lane) | - | `Map::GetLeft`, `Map.cpp:636` |
| May a vehicle cross into the lane beside | `may_change_left`, `may_change_right`, `advance.rs:228`, from the road mark's `laneChange` | - (stores `laneChange`) | - (stores `laneChange`) | the lane marking's `lane_change`, from `Map::GetMarkRecord`, `Map.cpp:307` |
| Route between two points | `route`, `route.rs:34` | `RoadPath::Calculate`, `RoadManager.cpp:6199` | lane keys only: `RoutingGraph::shortest_path`, `RoutingGraph.cpp:40` | - (not in `LibCarla/road`) |
| Road surface mesh | `surface_mesh`, `mesh.rs:313` | - (not in `RoadManager`) | `OpenDriveMap::get_road_network_mesh`, `OpenDriveMap.cpp:682` | `Map::GenerateMesh`, `Map.cpp:1005` |
| Object mesh | `object_mesh`, `object_mesh.rs:54` | - (not in `RoadManager`) | `Road::get_road_object_mesh`, in the network mesh | - |

## Elements from before 1.9

Two elements some readers take are not in the 1.9 schema, so they have no
row above:

- A road's `<link><neighbor>`, removed before 1.9. libOpenDRIVE reads it
  (`OpenDriveMap.cpp:202`), and so does the crate.
- A lane's `<visibility>`. CARLA reads it (`LaneParser.cpp:139`), and so
  does the crate.

## Every element and attribute

`-` means the library ignores it. A cell cites the line that reads it, in
the files the first table lists for that library.


### &lt;OpenDRIVE&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;OpenDRIVE&gt; | builds | builds `RoadManager.cpp:3762` | builds `OpenDriveMap.cpp:71` | builds `OpenDriveParser.cpp:38` |

### &lt;header&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;header&gt; | stores `mod.rs:1294` | builds `RoadManager.cpp:3791` | stores `OpenDriveMap.cpp:73` | stores `GeoReferenceParser.cpp:66` |
| &lt;header&gt; `@revMajor` | stores `mod.rs:1300` | stores `RoadManager.cpp:3794` | - | - |
| &lt;header&gt; `@revMinor` | stores `mod.rs:1301` | stores `RoadManager.cpp:3795` | - | - |
| &lt;header&gt; `@name` | stores `mod.rs:1302` | - | - | - |
| &lt;header&gt; `@version` | stores `mod.rs:1303` | - | - | - |
| &lt;header&gt; `@date` | stores `mod.rs:1304` | - | - | - |
| &lt;header&gt; `@north` | stores `mod.rs:1306` | - | - | - |
| &lt;header&gt; `@south` | stores `mod.rs:1307` | - | - | - |
| &lt;header&gt; `@east` | stores `mod.rs:1308` | - | - | - |
| &lt;header&gt; `@west` | stores `mod.rs:1309` | - | - | - |
| &lt;header&gt; `@vendor` | stores `mod.rs:1305` | - | - | - |
| &lt;header&gt; &lt;geoReference&gt; | stores `mod.rs:1321` | stores `RoadManager.cpp:3804` | stores `OpenDriveMap.cpp:74` | stores `GeoReferenceParser.cpp:66` |
| &lt;header&gt; &lt;offset&gt; | stores `mod.rs:1326` | builds `RoadManager.cpp:3808` | - | - |
| &lt;header&gt; &lt;offset&gt; `@x` | stores `mod.rs:1329` | builds `RoadManager.cpp:3808` | - | - |
| &lt;header&gt; &lt;offset&gt; `@y` | stores `mod.rs:1330` | builds `RoadManager.cpp:3809` | - | - |
| &lt;header&gt; &lt;offset&gt; `@z` | stores `mod.rs:1331` | builds `RoadManager.cpp:3810` | - | - |
| &lt;header&gt; &lt;offset&gt; `@hdg` | stores `mod.rs:1332` | builds `RoadManager.cpp:3811` | - | - |
| &lt;header&gt; &lt;license&gt; | - | - | - | - |
| &lt;header&gt; &lt;license&gt; `@name` | - | - | - | - |
| &lt;header&gt; &lt;license&gt; `@spdxid` | - | - | - | - |
| &lt;header&gt; &lt;license&gt; `@text` | - | - | - | - |
| &lt;header&gt; &lt;license&gt; `@resource` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@value` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@unit` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;lane&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;lane&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;priority&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;priority&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;animal&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;person&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;person&gt; &lt;type&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;vehicle&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;vehicle&gt; &lt;type&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;warning&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;routing&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;streetname&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;parking&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;tourist&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; `@value` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;animal&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;person&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;vehicle&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;animal&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;person&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;vehicle&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@value` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@unit` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryEnvironment&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryEnvironment&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryExplanatory&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; `@type` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; `@subtype` | - | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; &lt;semantics&gt; | - | - | - | - |

### &lt;road&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;road&gt; | builds `mod.rs:521` | builds `RoadManager.cpp:3830` | builds `OpenDriveMap.cpp:147` | builds `RoadParser.cpp:119` |
| &lt;road&gt; `@name` | stores `mod.rs:1423` | stores `RoadManager.cpp:3829` | stores `OpenDriveMap.cpp:164` | stores `RoadParser.cpp:124` |
| &lt;road&gt; `@length` | builds `mod.rs:1360` | builds `RoadManager.cpp:3832` | builds `OpenDriveMap.cpp:162` | builds `RoadParser.cpp:125` |
| &lt;road&gt; `@id` | builds `mod.rs:551` | builds `RoadManager.cpp:3830` | builds `OpenDriveMap.cpp:150` | builds `RoadParser.cpp:123` |
| &lt;road&gt; `@junction` | builds `mod.rs:1419` | builds `RoadManager.cpp:3836` | stores `OpenDriveMap.cpp:163` | builds `RoadParser.cpp:126` |
| &lt;road&gt; `@rule` | builds `mod.rs:1239` | builds `RoadManager.cpp:3846` | stores `OpenDriveMap.cpp:155` | - |
| &lt;road&gt; &lt;link&gt; | builds `links.rs:416` | builds `RoadManager.cpp:3958` | builds `OpenDriveMap.cpp:175` | builds `RoadParser.cpp:129` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; | builds `links.rs:434` | builds `RoadManager.cpp:3958` | builds `OpenDriveMap.cpp:175` | builds `RoadParser.cpp:129` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementId` | builds `links.rs:427` | builds `RoadManager.cpp:2579` | builds `OpenDriveMap.cpp:179` | builds `RoadParser.cpp:132` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementType` | builds `links.rs:421` | builds `RoadManager.cpp:2576` | builds `OpenDriveMap.cpp:181` | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@contactPoint` | builds `links.rs:429` | builds `RoadManager.cpp:2583` | builds `OpenDriveMap.cpp:190` | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementS` | builds `links.rs:430` | - | - | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementDir` | builds `virtual_junctions.rs:186` | - | - | - |
| &lt;road&gt; &lt;type&gt; | builds `properties.rs:82` | builds `RoadManager.cpp:3857` | stores `OpenDriveMap.cpp:213` | stores `RoadParser.cpp:140` |
| &lt;road&gt; &lt;type&gt; `@s` | builds `properties.rs:85` | builds `RoadManager.cpp:3921` | stores `OpenDriveMap.cpp:215` | stores `RoadParser.cpp:143` |
| &lt;road&gt; &lt;type&gt; `@type` | stores `properties.rs:82` | stores `RoadManager.cpp:3861` | stores `OpenDriveMap.cpp:216` | stores `RoadParser.cpp:144` |
| &lt;road&gt; &lt;type&gt; `@country` | stores `properties.rs:87` | - | - | - |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; | builds `properties.rs:90` | builds `RoadManager.cpp:3928` | stores `OpenDriveMap.cpp:221` | stores `RoadParser.cpp:147` |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; `@max` | builds `properties.rs:38` | builds `RoadManager.cpp:3928` | stores `OpenDriveMap.cpp:223` | stores `RoadParser.cpp:149` |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; `@unit` | builds `properties.rs:47` | builds `RoadManager.cpp:3929` | stores `OpenDriveMap.cpp:224` | stores `RoadParser.cpp:150` |
| &lt;road&gt; &lt;planView&gt; | builds `mod.rs:1365` | builds `RoadManager.cpp:3987` | builds `OpenDriveMap.cpp:232` | builds `GeometryParser.cpp:70` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; | builds `mod.rs:1368` | builds `RoadManager.cpp:3990` | builds `OpenDriveMap.cpp:232` | builds `GeometryParser.cpp:73` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@s` | builds `mod.rs:1373` | builds `RoadManager.cpp:3992` | builds `OpenDriveMap.cpp:234` | builds `GeometryParser.cpp:80` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@x` | builds `mod.rs:1329` | builds `RoadManager.cpp:3993` | builds `OpenDriveMap.cpp:235` | builds `GeometryParser.cpp:81` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@y` | builds `mod.rs:1330` | builds `RoadManager.cpp:3994` | builds `OpenDriveMap.cpp:236` | builds `GeometryParser.cpp:82` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@hdg` | builds `mod.rs:1332` | builds `RoadManager.cpp:3995` | builds `OpenDriveMap.cpp:237` | builds `GeometryParser.cpp:83` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@length` | builds `mod.rs:1472` | builds `RoadManager.cpp:3996` | builds `OpenDriveMap.cpp:238` | builds `GeometryParser.cpp:84` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;line&gt; | builds `mod.rs:1511` | builds `RoadManager.cpp:4010` | builds `OpenDriveMap.cpp:245` | builds `GeometryParser.cpp:88` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; | builds `mod.rs:1482` | builds `RoadManager.cpp:4021` | builds `OpenDriveMap.cpp:249` | builds `GeometryParser.cpp:91` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; `@curvStart` | builds `mod.rs:1483` | builds `RoadManager.cpp:4021` | builds `OpenDriveMap.cpp:251` | builds `GeometryParser.cpp:92` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; `@curvEnd` | builds `mod.rs:1483` | builds `RoadManager.cpp:4022` | builds `OpenDriveMap.cpp:252` | builds `GeometryParser.cpp:93` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;arc&gt; | builds `mod.rs:1479` | builds `RoadManager.cpp:4016` | builds `OpenDriveMap.cpp:276` | builds `GeometryParser.cpp:89` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;arc&gt; `@curvature` | builds `mod.rs:1480` | builds `RoadManager.cpp:4016` | builds `OpenDriveMap.cpp:278` | builds `GeometryParser.cpp:90` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; | builds `mod.rs:1502` | builds `RoadManager.cpp:4027` | - | builds `GeometryParser.cpp:94` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@a` | builds `mod.rs:1508` | builds `RoadManager.cpp:4027` | - | builds `GeometryParser.cpp:95` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@b` | builds `mod.rs:1508` | builds `RoadManager.cpp:4028` | - | builds `GeometryParser.cpp:96` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@c` | builds `mod.rs:1508` | builds `RoadManager.cpp:4029` | - | builds `GeometryParser.cpp:97` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@d` | builds `mod.rs:1508` | builds `RoadManager.cpp:4030` | - | builds `GeometryParser.cpp:98` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; | builds `mod.rs:1488` | builds `RoadManager.cpp:4035` | builds `OpenDriveMap.cpp:281` | builds `GeometryParser.cpp:99` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@aU` | builds `mod.rs:1498` | builds `RoadManager.cpp:4035` | builds `OpenDriveMap.cpp:283` | builds `GeometryParser.cpp:100` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@bU` | builds `mod.rs:1498` | builds `RoadManager.cpp:4036` | builds `OpenDriveMap.cpp:284` | builds `GeometryParser.cpp:101` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@cU` | builds `mod.rs:1498` | builds `RoadManager.cpp:4037` | builds `OpenDriveMap.cpp:285` | builds `GeometryParser.cpp:102` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@dU` | builds `mod.rs:1498` | builds `RoadManager.cpp:4038` | builds `OpenDriveMap.cpp:286` | builds `GeometryParser.cpp:103` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@aV` | builds `mod.rs:1499` | builds `RoadManager.cpp:4039` | builds `OpenDriveMap.cpp:287` | builds `GeometryParser.cpp:104` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@bV` | builds `mod.rs:1499` | builds `RoadManager.cpp:4040` | builds `OpenDriveMap.cpp:288` | builds `GeometryParser.cpp:105` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@cV` | builds `mod.rs:1499` | builds `RoadManager.cpp:4041` | builds `OpenDriveMap.cpp:289` | builds `GeometryParser.cpp:106` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@dV` | builds `mod.rs:1499` | builds `RoadManager.cpp:4042` | builds `OpenDriveMap.cpp:290` | builds `GeometryParser.cpp:107` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@pRange` | builds `mod.rs:1493` | builds `RoadManager.cpp:4045` | builds `OpenDriveMap.cpp:293` | builds `GeometryParser.cpp:108` |
| &lt;road&gt; &lt;elevationProfile&gt; | builds `mod.rs:1382` | builds `RoadManager.cpp:4088` | builds `OpenDriveMap.cpp:313` | builds `ProfilesParser.cpp:56` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; | builds `mod.rs:1383` | builds `RoadManager.cpp:4088` | builds `OpenDriveMap.cpp:325` | builds `ProfilesParser.cpp:60` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@s` | builds `mod.rs:1383` | builds `RoadManager.cpp:4090` | builds `OpenDriveMap.cpp:328` | builds `ProfilesParser.cpp:68` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@a` | builds `mod.rs:2404` | builds `RoadManager.cpp:4091` | builds `OpenDriveMap.cpp:329` | builds `ProfilesParser.cpp:69` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@b` | builds `mod.rs:2405` | builds `RoadManager.cpp:4092` | builds `OpenDriveMap.cpp:330` | builds `ProfilesParser.cpp:70` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@c` | builds `mod.rs:2406` | builds `RoadManager.cpp:4093` | builds `OpenDriveMap.cpp:331` | builds `ProfilesParser.cpp:71` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@d` | builds `mod.rs:2407` | builds `RoadManager.cpp:4094` | builds `OpenDriveMap.cpp:332` | builds `ProfilesParser.cpp:72` |
| &lt;road&gt; &lt;lateralProfile&gt; | builds `mod.rs:1388` | builds `RoadManager.cpp:4108` | builds `OpenDriveMap.cpp:317` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; | builds `mod.rs:1389` | builds `RoadManager.cpp:4111` | builds `OpenDriveMap.cpp:317` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@s` | builds `mod.rs:1389` | builds `RoadManager.cpp:4114` | builds `OpenDriveMap.cpp:328` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@a` | builds `mod.rs:2404` | builds `RoadManager.cpp:4115` | builds `OpenDriveMap.cpp:329` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@b` | builds `mod.rs:2405` | builds `RoadManager.cpp:4116` | builds `OpenDriveMap.cpp:330` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@c` | builds `mod.rs:2406` | builds `RoadManager.cpp:4117` | builds `OpenDriveMap.cpp:331` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@d` | builds `mod.rs:2407` | builds `RoadManager.cpp:4118` | builds `OpenDriveMap.cpp:332` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; | builds `mod.rs:895` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@s` | builds `mod.rs:899` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@t` | builds `mod.rs:901` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@a` | builds `mod.rs:902` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@b` | builds `mod.rs:903` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@c` | builds `mod.rs:904` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@d` | builds `mod.rs:905` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; | builds `mod.rs:1397` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; | builds `mod.rs:938` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; | builds `mod.rs:935` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@a` | builds `mod.rs:902` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@b` | builds `mod.rs:903` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@c` | builds `mod.rs:904` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@d` | builds `mod.rs:905` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@s` | builds `mod.rs:935` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; | builds `mod.rs:941` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; | builds `mod.rs:944` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; `@id` | builds `mod.rs:952` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; `@mode` | builds `mod.rs:971` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;width&gt; | builds `mod.rs:985` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;width&gt; &lt;coefficients&gt; | builds `mod.rs:935` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;constant&gt; | builds `mod.rs:987` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;constant&gt; &lt;coefficients&gt; | builds `mod.rs:935` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;linear&gt; | builds `mod.rs:988` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;linear&gt; &lt;coefficients&gt; | builds `mod.rs:935` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;quadratic&gt; | builds `mod.rs:989` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;quadratic&gt; &lt;coefficients&gt; | builds `mod.rs:935` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;cubic&gt; | builds `mod.rs:990` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;cubic&gt; &lt;coefficients&gt; | builds `mod.rs:935` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; | builds `mod.rs:1901` | builds `RoadManager.cpp:4132` | builds `OpenDriveMap.cpp:374` | builds `LaneParser.cpp:197` |
| &lt;road&gt; &lt;lanes&gt; `@layer` | builds `mod.rs:1904` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; | builds `mod.rs:1413` | builds `RoadManager.cpp:4137` | builds `OpenDriveMap.cpp:314` | builds `RoadParser.cpp:158` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@s` | builds `mod.rs:1410` | builds `RoadManager.cpp:4139` | builds `OpenDriveMap.cpp:328` | builds `RoadParser.cpp:160` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@a` | builds `mod.rs:2404` | builds `RoadManager.cpp:4140` | builds `OpenDriveMap.cpp:329` | builds `RoadParser.cpp:161` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@b` | builds `mod.rs:2405` | builds `RoadManager.cpp:4141` | builds `OpenDriveMap.cpp:330` | builds `RoadParser.cpp:162` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@c` | builds `mod.rs:2406` | builds `RoadManager.cpp:4142` | builds `OpenDriveMap.cpp:331` | builds `RoadParser.cpp:163` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@d` | builds `mod.rs:2407` | builds `RoadManager.cpp:4143` | builds `OpenDriveMap.cpp:332` | builds `RoadParser.cpp:164` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; | builds `mod.rs:1816` | builds `RoadManager.cpp:4148` | builds `OpenDriveMap.cpp:374` | builds `LaneParser.cpp:199` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@length` | builds `mod.rs:1825` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@s` | builds `mod.rs:1806` | builds `RoadManager.cpp:4148` | builds `OpenDriveMap.cpp:376` | builds `LaneParser.cpp:200` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@singleSide` | - | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; | builds `mod.rs:1073` | builds `RoadManager.cpp:4163` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:201` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; | builds `mod.rs:1077` | builds `RoadManager.cpp:4179` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:22` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@type` | builds `mod.rs:1100` | builds `RoadManager.cpp:4190` | stores `OpenDriveMap.cpp:388` | builds `RoadParser.cpp:184` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@advisory` | stores `mod.rs:837` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@direction` | builds `mod.rs:864` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@dynamicLaneType` | stores `mod.rs:843` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@dynamicLaneDirection` | stores `mod.rs:844` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@level` | builds `mod.rs:1116` | - | builds `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:185` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@roadWorks` | stores `mod.rs:1202` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@id` | builds `mod.rs:1078` | builds `RoadManager.cpp:4288` | builds `OpenDriveMap.cpp:383` | builds `RoadParser.cpp:183` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; | builds `links.rs:438` | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | builds `RoadParser.cpp:188` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; | builds `links.rs:438` | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | builds `RoadParser.cpp:188` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@id` | builds `links.rs:458` | builds `RoadManager.cpp:4311` | builds `OpenDriveMap.cpp:392` | builds `RoadParser.cpp:191` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@layer` | builds `links.rs:459` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; | builds `mod.rs:1084` | - | - | stores `LaneParser.cpp:48` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@sOffset` | builds `mod.rs:2422` | - | - | stores `LaneParser.cpp:49` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@a` | builds `mod.rs:2423` | - | - | stores `LaneParser.cpp:50` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@b` | builds `mod.rs:2424` | - | - | stores `LaneParser.cpp:51` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@c` | builds `mod.rs:2425` | - | - | stores `LaneParser.cpp:52` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@d` | builds `mod.rs:2426` | - | - | stores `LaneParser.cpp:53` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; | builds `mod.rs:1083` | builds `RoadManager.cpp:4321` | builds `OpenDriveMap.cpp:397` | builds `LaneParser.cpp:30` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@sOffset` | builds `mod.rs:2422` | builds `RoadManager.cpp:4323` | builds `OpenDriveMap.cpp:399` | builds `LaneParser.cpp:31` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@a` | builds `mod.rs:2423` | builds `RoadManager.cpp:4324` | builds `OpenDriveMap.cpp:400` | builds `LaneParser.cpp:32` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@b` | builds `mod.rs:2424` | builds `RoadManager.cpp:4325` | builds `OpenDriveMap.cpp:401` | builds `LaneParser.cpp:33` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@c` | builds `mod.rs:2425` | builds `RoadManager.cpp:4326` | builds `OpenDriveMap.cpp:402` | builds `LaneParser.cpp:34` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@d` | builds `mod.rs:2426` | builds `RoadManager.cpp:4327` | builds `OpenDriveMap.cpp:403` | builds `LaneParser.cpp:35` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; | builds `road_marks.rs:149` | builds `RoadManager.cpp:4341` | builds `OpenDriveMap.cpp:422` | builds `LaneParser.cpp:61` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@sOffset` | builds `road_marks.rs:163` | builds `RoadManager.cpp:4344` | builds `OpenDriveMap.cpp:429` | stores `LaneParser.cpp:64` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@type` | builds `road_marks.rs:164` | builds `RoadManager.cpp:4360` | stores `OpenDriveMap.cpp:430` | stores `LaneParser.cpp:65` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@weight` | builds `road_marks.rs:165` | stores `RoadManager.cpp:4415` | builds `OpenDriveMap.cpp:431` | stores `LaneParser.cpp:66` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@color` | stores `road_marks.rs:169` | stores `RoadManager.cpp:1543` | stores `OpenDriveMap.cpp:432` | stores `LaneParser.cpp:67` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@material` | stores `road_marks.rs:170` | - | stores `OpenDriveMap.cpp:433` | stores `LaneParser.cpp:68` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@width` | builds `road_marks.rs:153` | builds `RoadManager.cpp:4483` | builds `OpenDriveMap.cpp:427` | stores `LaneParser.cpp:69` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@laneChange` | builds `road_marks.rs:177` | stores `RoadManager.cpp:4449` | stores `OpenDriveMap.cpp:434` | builds `LaneParser.cpp:70` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@height` | builds `road_marks.rs:176` | stores `RoadManager.cpp:4499` | stores `OpenDriveMap.cpp:428` | stores `LaneParser.cpp:71` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; | builds `road_marks.rs:183` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@ds` | builds `road_marks.rs:183` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@a` | builds `mod.rs:2404` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@b` | builds `mod.rs:2405` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@c` | builds `mod.rs:2406` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@d` | builds `mod.rs:2407` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; | builds `road_marks.rs:151` | builds `RoadManager.cpp:4511` | builds `OpenDriveMap.cpp:440` | stores `LaneParser.cpp:77` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; `@name` | stores `road_marks.rs:172` | stores `RoadManager.cpp:4515` | stores `OpenDriveMap.cpp:442` | stores `LaneParser.cpp:79` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; `@width` | builds `road_marks.rs:153` | stores `RoadManager.cpp:4516` | builds `OpenDriveMap.cpp:443` | stores `LaneParser.cpp:80` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; | builds `road_marks.rs:160` | builds `RoadManager.cpp:4519` | builds `OpenDriveMap.cpp:445` | stores `LaneParser.cpp:99` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@length` | builds `road_marks.rs:79` | builds `RoadManager.cpp:4521` | builds `OpenDriveMap.cpp:455` | stores `LaneParser.cpp:101` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@space` | builds `road_marks.rs:63` | builds `RoadManager.cpp:4522` | builds `OpenDriveMap.cpp:456` | stores `LaneParser.cpp:102` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@tOffset` | builds `road_marks.rs:89` | builds `RoadManager.cpp:4523` | builds `OpenDriveMap.cpp:457` | stores `LaneParser.cpp:103` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@sOffset` | builds `road_marks.rs:88` | builds `RoadManager.cpp:4524` | builds `OpenDriveMap.cpp:458` | stores `LaneParser.cpp:104` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@rule` | stores `road_marks.rs:90` | stores `RoadManager.cpp:4553` | stores `OpenDriveMap.cpp:460` | stores `LaneParser.cpp:105` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@width` | builds `road_marks.rs:96` | builds `RoadManager.cpp:4575` | builds `OpenDriveMap.cpp:447` | stores `LaneParser.cpp:106` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@color` | stores `road_marks.rs:97` | stores `RoadManager.cpp:4542` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; | builds `road_marks.rs:181` | builds `RoadManager.cpp:4601` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; | builds `road_marks.rs:160` | builds `RoadManager.cpp:4601` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@length` | builds `road_marks.rs:79` | builds `RoadManager.cpp:4603` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@tOffset` | builds `road_marks.rs:89` | builds `RoadManager.cpp:4604` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@sOffset` | builds `road_marks.rs:88` | builds `RoadManager.cpp:4605` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@rule` | stores `road_marks.rs:90` | stores `RoadManager.cpp:4610` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@width` | builds `road_marks.rs:96` | builds `RoadManager.cpp:4606` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; | stores `properties.rs:193` | stores `RoadManager.cpp:4744` | - | stores `LaneParser.cpp:123` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@sOffset` | stores `properties.rs:169` | stores `RoadManager.cpp:4747` | - | stores `LaneParser.cpp:125` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@surface` | stores `mod.rs:1249` | - | - | stores `LaneParser.cpp:126` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@friction` | stores `mod.rs:1250` | stores `RoadManager.cpp:4750` | - | stores `LaneParser.cpp:127` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@roughness` | stores `mod.rs:1251` | - | - | stores `LaneParser.cpp:128` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; | builds `properties.rs:107` | - | - | stores `LaneParser.cpp:147` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@sOffset` | builds `properties.rs:109` | - | - | stores `LaneParser.cpp:148` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@max` | builds `properties.rs:38` | - | - | stores `LaneParser.cpp:149` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@unit` | builds `properties.rs:47` | - | - | stores `LaneParser.cpp:150` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; | stores `properties.rs:202` | - | - | stores `LaneParser.cpp:157` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@sOffset` | stores `properties.rs:169` | - | - | stores `LaneParser.cpp:158` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@rule` | stores `properties.rs:140` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@restriction` | stores `properties.rs:130` | - | - | stores `LaneParser.cpp:159` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; &lt;restriction&gt; | stores `properties.rs:134` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; &lt;restriction&gt; `@type` | stores `properties.rs:135` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; | builds `mod.rs:1016` | stores `RoadManager.cpp:4332` | builds `OpenDriveMap.cpp:411` | stores `LaneParser.cpp:166` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@sOffset` | builds `mod.rs:1018` | stores `RoadManager.cpp:4334` | builds `OpenDriveMap.cpp:413` | stores `LaneParser.cpp:167` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@inner` | builds `mod.rs:1019` | stores `RoadManager.cpp:4335` | builds `OpenDriveMap.cpp:414` | stores `LaneParser.cpp:168` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@outer` | builds `mod.rs:1020` | stores `RoadManager.cpp:4336` | builds `OpenDriveMap.cpp:415` | stores `LaneParser.cpp:169` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; | stores `properties.rs:179` | - | - | stores `LaneParser.cpp:176` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; `@sOffset` | stores `properties.rs:169` | - | - | stores `LaneParser.cpp:177` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; `@value` | stores `properties.rs:179` | - | - | stores `LaneParser.cpp:178` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; | builds `mod.rs:1135` | builds `RoadManager.cpp:4163` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:206` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; | builds `mod.rs:1138` | builds `RoadManager.cpp:4179` | builds `OpenDriveMap.cpp:380` | builds `RoadParser.cpp:203` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@id` | stores `mod.rs:1143` | builds `RoadManager.cpp:4288` | builds `OpenDriveMap.cpp:383` | builds `RoadParser.cpp:206` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@type` | stores `mod.rs:1144` | stores `RoadManager.cpp:4190` | stores `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:207` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@level` | stores `mod.rs:1145` | - | builds `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:208` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; &lt;link&gt; | stores `links.rs:416` | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | stores `RoadParser.cpp:211` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; &lt;roadMark&gt; | builds `road_marks.rs:149` | builds `RoadManager.cpp:4341` | builds `OpenDriveMap.cpp:422` | stores `LaneParser.cpp:61` |
| &lt;road&gt; &lt;objects&gt; | builds `mod.rs:1448` | builds `RoadManager.cpp:5054` | builds `OpenDriveMap.cpp:523` | builds `ObjectParser.cpp:28` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; | builds `mod.rs:2586` | builds `RoadManager.cpp:5057` | builds `OpenDriveMap.cpp:523` | builds `ObjectParser.cpp:31` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@t` | builds `mod.rs:2625` | builds `RoadManager.cpp:5063` | builds `OpenDriveMap.cpp:536` | builds `ObjectParser.cpp:56` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@zOffset` | builds `mod.rs:2626` | builds `RoadManager.cpp:5111` | builds `OpenDriveMap.cpp:537` | builds `ObjectParser.cpp:57` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@type` | stores `mod.rs:2808` | stores `RoadManager.cpp:5088` | stores `OpenDriveMap.cpp:546` | builds `ObjectParser.cpp:34` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@validLength` | stores `mod.rs:2628` | - | stores `OpenDriveMap.cpp:539` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@orientation` | builds `mod.rs:2676` | builds `RoadManager.cpp:5069` | stores `OpenDriveMap.cpp:548` | builds `ObjectParser.cpp:61` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@subtype` | stores `mod.rs:2850` | stores `RoadManager.cpp:5090` | stores `OpenDriveMap.cpp:549` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@dynamic` | stores `mod.rs:2852` | - | stores `OpenDriveMap.cpp:530` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@hdg` | builds `mod.rs:2746` | builds `RoadManager.cpp:5113` | builds `OpenDriveMap.cpp:543` | builds `ObjectParser.cpp:58` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@name` | stores `mod.rs:2561` | stores `RoadManager.cpp:5065` | stores `OpenDriveMap.cpp:547` | builds `ObjectParser.cpp:35` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@pitch` | builds `mod.rs:2747` | builds `RoadManager.cpp:5114` | builds `OpenDriveMap.cpp:544` | builds `ObjectParser.cpp:59` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@id` | builds `mod.rs:2587` | stores `RoadManager.cpp:5064` | stores `OpenDriveMap.cpp:525` | builds `ObjectParser.cpp:78` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@roll` | builds `mod.rs:2748` | builds `RoadManager.cpp:5115` | builds `OpenDriveMap.cpp:545` | builds `ObjectParser.cpp:60` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@height` | builds `mod.rs:2743` | builds `RoadManager.cpp:5112` | builds `OpenDriveMap.cpp:542` | builds `ObjectParser.cpp:90` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@s` | builds `mod.rs:2624` | builds `RoadManager.cpp:5062` | builds `OpenDriveMap.cpp:535` | builds `ObjectParser.cpp:55` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@length` | builds `mod.rs:2741` | builds `RoadManager.cpp:5091` | builds `OpenDriveMap.cpp:538` | builds `ObjectParser.cpp:63` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@width` | builds `mod.rs:2742` | builds `RoadManager.cpp:5092` | builds `OpenDriveMap.cpp:540` | builds `ObjectParser.cpp:62` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@radius` | builds `mod.rs:2744` | builds `RoadManager.cpp:5093` | builds `OpenDriveMap.cpp:541` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@perpToRoad` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@invalidated` | stores `mod.rs:2853` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@temporary` | stores `mod.rs:2854` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; | builds `mod.rs:2774` | builds `RoadManager.cpp:5120` | builds `OpenDriveMap.cpp:560` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@s` | builds `mod.rs:2994` | builds `RoadManager.cpp:5123` | builds `OpenDriveMap.cpp:562` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@length` | builds `mod.rs:2995` | builds `RoadManager.cpp:5129` | builds `OpenDriveMap.cpp:563` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@distance` | builds `mod.rs:2996` | builds `RoadManager.cpp:5130` | builds `OpenDriveMap.cpp:564` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@tStart` | builds `mod.rs:3007` | builds `RoadManager.cpp:5131` | builds `OpenDriveMap.cpp:565` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@tEnd` | builds `mod.rs:3007` | builds `RoadManager.cpp:5132` | builds `OpenDriveMap.cpp:566` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@heightStart` | builds `mod.rs:3007` | builds `RoadManager.cpp:5133` | builds `OpenDriveMap.cpp:569` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@heightEnd` | builds `mod.rs:3007` | builds `RoadManager.cpp:5134` | builds `OpenDriveMap.cpp:570` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@zOffsetStart` | builds `mod.rs:3007` | builds `RoadManager.cpp:5135` | builds `OpenDriveMap.cpp:571` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@zOffsetEnd` | builds `mod.rs:3007` | builds `RoadManager.cpp:5136` | builds `OpenDriveMap.cpp:572` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@widthStart` | builds `mod.rs:3007` | builds `RoadManager.cpp:5138` | builds `OpenDriveMap.cpp:567` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@widthEnd` | builds `mod.rs:3007` | builds `RoadManager.cpp:5139` | builds `OpenDriveMap.cpp:568` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@lengthStart` | builds `mod.rs:3007` | builds `RoadManager.cpp:5140` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@lengthEnd` | builds `mod.rs:3007` | builds `RoadManager.cpp:5141` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@radiusStart` | builds `mod.rs:3007` | builds `RoadManager.cpp:5142` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@radiusEnd` | builds `mod.rs:3007` | builds `RoadManager.cpp:5143` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@detachFromReferenceLine` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@bT` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@cT` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@dT` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; | builds `mod.rs:3126` | - | builds `OpenDriveMap.cpp:591` | builds `ObjectParser.cpp:39` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@id` | builds `mod.rs:3369` | - | stores `OpenDriveMap.cpp:593` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@fillType` | stores `mod.rs:2915` | - | stores `OpenDriveMap.cpp:594` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@outer` | builds `mod.rs:2899` | - | stores `OpenDriveMap.cpp:596` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@closed` | builds `mod.rs:3215` | - | stores `OpenDriveMap.cpp:597` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@laneType` | stores `mod.rs:2919` | - | stores `OpenDriveMap.cpp:595` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; | builds `mod.rs:3178` | - | builds `OpenDriveMap.cpp:614` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@id` | builds `mod.rs:3268` | - | stores `OpenDriveMap.cpp:620` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@s` | builds `mod.rs:3179` | - | builds `OpenDriveMap.cpp:616` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@t` | builds `mod.rs:3179` | - | builds `OpenDriveMap.cpp:617` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@dz` | builds `mod.rs:3185` | - | builds `OpenDriveMap.cpp:618` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@height` | builds `mod.rs:3177` | - | builds `OpenDriveMap.cpp:622` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; | builds `mod.rs:3242` | - | builds `OpenDriveMap.cpp:600` | builds `ObjectParser.cpp:42` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@id` | builds `mod.rs:3268` | - | stores `OpenDriveMap.cpp:606` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@u` | builds `mod.rs:3194` | - | builds `OpenDriveMap.cpp:602` | builds `ObjectParser.cpp:43` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@v` | builds `mod.rs:3194` | - | builds `OpenDriveMap.cpp:603` | builds `ObjectParser.cpp:44` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@z` | builds `mod.rs:3195` | - | builds `OpenDriveMap.cpp:604` | builds `ObjectParser.cpp:45` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@height` | builds `mod.rs:3177` | - | builds `OpenDriveMap.cpp:608` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@u` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@v` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@z` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@height` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@length` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@hdg` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;arc&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;arc&gt; `@curvature` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;line&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@aU` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@bU` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@cU` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@dU` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@aV` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@bV` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@cV` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@dV` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@pRange` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; | builds `mod.rs:3249` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; | builds `mod.rs:3252` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@side` | builds `mod.rs:3296` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@weight` | stores `mod.rs:3325` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@width` | builds `mod.rs:3316` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@color` | builds `mod.rs:3324` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@zOffset` | builds `mod.rs:3317` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@spaceLength` | builds `mod.rs:3320` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@lineLength` | builds `mod.rs:3319` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@startOffset` | builds `mod.rs:3334` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@stopOffset` | builds `mod.rs:3335` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; &lt;cornerReference&gt; | builds `mod.rs:3294` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; &lt;cornerReference&gt; `@id` | builds `mod.rs:3404` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outlines&gt; | builds `mod.rs:3123` | builds `RoadManager.cpp:5248` | builds `OpenDriveMap.cpp:590` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outlines&gt; &lt;outline&gt; | builds `mod.rs:3126` | builds `RoadManager.cpp:5251` | builds `OpenDriveMap.cpp:591` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; | stores `mod.rs:2820` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@surface` | stores `mod.rs:1249` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@friction` | stores `mod.rs:1250` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@roughness` | stores `mod.rs:1251` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@roadMarkColor` | stores `mod.rs:2826` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; | builds `mod.rs:2662` | stores `RoadManager.cpp:5389` | stores `OpenDriveMap.cpp:38` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@fromLane` | builds `mod.rs:2665` | stores `RoadManager.cpp:5392` | stores `OpenDriveMap.cpp:40` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@toLane` | builds `mod.rs:2666` | stores `RoadManager.cpp:5393` | stores `OpenDriveMap.cpp:40` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@layer` | builds `mod.rs:2667` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; | stores `mod.rs:2811` | stores `RoadManager.cpp:5290` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; `@access` | stores `mod.rs:2814` | stores `RoadManager.cpp:5295` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; `@restrictions` | stores `mod.rs:2815` | stores `RoadManager.cpp:5334` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;markings&gt; | builds `mod.rs:3249` | builds `RoadManager.cpp:5339` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; | builds `mod.rs:3361` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; | builds `mod.rs:3366` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@width` | builds `mod.rs:3372` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@type` | builds `mod.rs:3383` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@outlineId` | builds `mod.rs:3368` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@useCompleteOutline` | builds `mod.rs:3373` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; &lt;cornerReference&gt; | builds `mod.rs:3402` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@file` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@hideRoadSurfaceCRG` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@zScale` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@dz` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@radius` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@intersectionPoint` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@s` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@t` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@radius` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@intersectionPoint` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@id` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@u` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@v` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@z` | - | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; | builds `mod.rs:2697` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@s` | builds `mod.rs:2650` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@t` | builds `mod.rs:2650` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@id` | builds `mod.rs:2698` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@zOffset` | builds `mod.rs:2647` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@validLength` | stores `mod.rs:2649` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@orientation` | builds `mod.rs:2676` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; &lt;validity&gt; | builds `mod.rs:2662` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; | builds `mod.rs:2520` | builds `RoadManager.cpp:5439` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@s` | builds `mod.rs:2531` | builds `RoadManager.cpp:5448` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@length` | builds `mod.rs:2531` | builds `RoadManager.cpp:5445` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@name` | stores `mod.rs:2561` | stores `RoadManager.cpp:5447` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@id` | stores `mod.rs:2567` | stores `RoadManager.cpp:5444` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@type` | stores `mod.rs:2522` | stores `RoadManager.cpp:5449` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@lighting` | stores `mod.rs:2523` | stores `RoadManager.cpp:5446` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@daylight` | stores `mod.rs:2524` | stores `RoadManager.cpp:5443` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; &lt;validity&gt; | builds `mod.rs:2662` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; | builds `mod.rs:2526` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@s` | builds `mod.rs:2531` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@length` | builds `mod.rs:2531` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@name` | stores `mod.rs:2561` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@id` | stores `mod.rs:2567` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@type` | stores `mod.rs:2527` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; &lt;validity&gt; | builds `mod.rs:2662` | - | - | - |
| &lt;road&gt; &lt;signals&gt; | builds `signals.rs:53` | builds `RoadManager.cpp:4839` | builds `OpenDriveMap.cpp:637` | builds `SignalParser.cpp:44` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; | builds `signals.rs:61` | builds `RoadManager.cpp:4845` | builds `OpenDriveMap.cpp:637` | builds `SignalParser.cpp:46` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@id` | builds `signals.rs:248` | builds `RoadManager.cpp:4851` | stores `OpenDriveMap.cpp:639` | builds `SignalParser.cpp:49` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@name` | stores `signals.rs:121` | stores `RoadManager.cpp:4852` | stores `OpenDriveMap.cpp:648` | builds `SignalParser.cpp:50` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@dynamic` | stores `signals.rs:220` | builds `RoadManager.cpp:4856` | stores `OpenDriveMap.cpp:651` | stores `SignalParser.cpp:51` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@orientation` | builds `signals.rs:201` | builds `RoadManager.cpp:4875` | stores `OpenDriveMap.cpp:659` | builds `SignalParser.cpp:52` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@country` | stores `signals.rs:221` | builds `RoadManager.cpp:4897` | stores `OpenDriveMap.cpp:660` | stores `SignalParser.cpp:54` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@countryRevision` | stores `signals.rs:222` | builds `RoadManager.cpp:4901` | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@type` | stores `signals.rs:223` | builds `RoadManager.cpp:4917` | stores `OpenDriveMap.cpp:661` | builds `SignalParser.cpp:55` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@subtype` | stores `signals.rs:224` | builds `RoadManager.cpp:4918` | stores `OpenDriveMap.cpp:662` | stores `SignalParser.cpp:56` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@value` | stores `signals.rs:225` | builds `RoadManager.cpp:4919` | stores `OpenDriveMap.cpp:653` | stores `SignalParser.cpp:57` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@text` | stores `signals.rs:227` | stores `RoadManager.cpp:4960` | stores `OpenDriveMap.cpp:664` | stores `SignalParser.cpp:61` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@unit` | stores `signals.rs:226` | stores `RoadManager.cpp:4956` | stores `OpenDriveMap.cpp:663` | stores `SignalParser.cpp:58` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@height` | builds `signals.rs:241` | builds `RoadManager.cpp:4957` | builds `OpenDriveMap.cpp:654` | builds `SignalParser.cpp:59` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@length` | builds `signals.rs:239` | builds `RoadManager.cpp:4959` | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@hOffset` | builds `signals.rs:317` | builds `RoadManager.cpp:4961` | builds `OpenDriveMap.cpp:656` | builds `SignalParser.cpp:62` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@width` | builds `signals.rs:240` | builds `RoadManager.cpp:4958` | builds `OpenDriveMap.cpp:655` | builds `SignalParser.cpp:60` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@roll` | builds `signals.rs:319` | builds `RoadManager.cpp:4963` | builds `OpenDriveMap.cpp:658` | builds `SignalParser.cpp:64` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@pitch` | builds `signals.rs:318` | builds `RoadManager.cpp:4962` | builds `OpenDriveMap.cpp:657` | builds `SignalParser.cpp:63` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@invalidated` | stores `signals.rs:228` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@temporary` | stores `signals.rs:229` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@s` | builds `signals.rs:198` | builds `RoadManager.cpp:4849` | builds `OpenDriveMap.cpp:649` | builds `SignalParser.cpp:47` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@t` | builds `signals.rs:198` | builds `RoadManager.cpp:4850` | builds `OpenDriveMap.cpp:650` | builds `SignalParser.cpp:48` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@zOffset` | builds `signals.rs:316` | builds `RoadManager.cpp:4896` | builds `OpenDriveMap.cpp:652` | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;validity&gt; | builds `signals.rs:233` | stores `RoadManager.cpp:5028` | stores `OpenDriveMap.cpp:38` | builds `SignalParser.cpp:25` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; | builds `signals.rs:268` | - | - | stores `SignalParser.cpp:109` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; `@id` | builds `signals.rs:269` | - | - | stores `SignalParser.cpp:110` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; `@type` | stores `signals.rs:272` | - | - | stores `SignalParser.cpp:111` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; | builds `signals.rs:276` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@elementType` | stores `signals.rs:278` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@elementId` | builds `signals.rs:277` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@type` | stores `signals.rs:288` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; | builds `signals.rs:209` | - | - | stores `SignalParser.cpp:115` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@x` | builds `signals.rs:477` | - | - | stores `SignalParser.cpp:116` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@y` | builds `signals.rs:478` | - | - | stores `SignalParser.cpp:117` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@z` | builds `signals.rs:398` | - | - | stores `SignalParser.cpp:118` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@hdg` | builds `signals.rs:483` | - | - | stores `SignalParser.cpp:119` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@pitch` | builds `signals.rs:318` | - | - | stores `SignalParser.cpp:120` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@roll` | builds `signals.rs:319` | - | - | stores `SignalParser.cpp:121` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; | builds `signals.rs:206` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@roadId` | builds `signals.rs:207` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@s` | builds `signals.rs:305` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@t` | builds `signals.rs:305` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@zOffset` | builds `signals.rs:316` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@hOffset` | builds `signals.rs:317` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@pitch` | builds `signals.rs:318` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@roll` | builds `signals.rs:319` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;semantics&gt; | stores `signals.rs:347` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; | stores `signals.rs:419` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;validity&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;dependency&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;reference&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; | stores `signals.rs:422` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@id` | stores `signals.rs:424` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@name` | stores `signals.rs:425` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@dynamic` | stores `signals.rs:438` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@orientation` | stores `signals.rs:422` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@country` | stores `signals.rs:426` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@countryRevision` | stores `signals.rs:427` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@type` | stores `signals.rs:428` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@subtype` | stores `signals.rs:429` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@value` | stores `signals.rs:430` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@text` | stores `signals.rs:432` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@unit` | stores `signals.rs:431` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@height` | stores `signals.rs:436` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@length` | stores `signals.rs:437` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@hOffset` | stores `signals.rs:442` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@width` | stores `signals.rs:435` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@roll` | stores `signals.rs:444` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@pitch` | stores `signals.rs:443` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@invalidated` | stores `signals.rs:439` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@temporary` | stores `signals.rs:440` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@v` | stores `signals.rs:397` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@z` | stores `signals.rs:398` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;validity&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;dependency&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;reference&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;positionInertial&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;positionRoad&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;semantics&gt; | stores `signals.rs:347` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; | stores `signals.rs:448` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayType` | stores `signals.rs:450` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayHeight` | stores `signals.rs:453` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayWidth` | stores `signals.rs:452` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@v` | stores `signals.rs:397` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@z` | stores `signals.rs:479` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;validity&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;dependency&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;reference&gt; | - | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; | stores `signals.rs:456` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@index` | stores `signals.rs:458` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@width` | stores `signals.rs:460` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@height` | stores `signals.rs:461` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@v` | stores `signals.rs:397` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@z` | stores `signals.rs:479` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; | builds `signals.rs:80` | - | - | builds `SignalParser.cpp:128` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@s` | builds `signals.rs:159` | - | - | builds `SignalParser.cpp:129` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@t` | builds `signals.rs:159` | - | - | builds `SignalParser.cpp:130` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@id` | builds `signals.rs:81` | - | - | builds `SignalParser.cpp:131` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@orientation` | builds `signals.rs:165` | - | - | builds `SignalParser.cpp:133` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; &lt;validity&gt; | builds `signals.rs:166` | - | - | builds `SignalParser.cpp:147` |
| &lt;road&gt; &lt;surface&gt; | builds `mod.rs:1452` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; | builds `mod.rs:779` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@file` | builds `mod.rs:1765` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sStart` | builds `mod.rs:1684` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sEnd` | builds `mod.rs:1685` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@orientation` | builds `mod.rs:1693` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@mode` | builds `mod.rs:971` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@purpose` | builds `mod.rs:1753` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sOffset` | builds `mod.rs:1691` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@tOffset` | builds `mod.rs:938` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@xOffset` | builds `mod.rs:1738` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@yOffset` | builds `mod.rs:1739` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@zOffset` | builds `mod.rs:1759` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@zScale` | builds `mod.rs:1760` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@hOffset` | builds `mod.rs:1706` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; | stores `railway.rs:17` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; | stores `railway.rs:20` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; `@name` | stores `railway.rs:52` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; `@id` | stores `railway.rs:25` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; `@position` | stores `railway.rs:38` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;mainTrack&gt; | stores `railway.rs:46` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;mainTrack&gt; `@id` | stores `railway.rs:51` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;mainTrack&gt; `@s` | stores `railway.rs:31` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;mainTrack&gt; `@dir` | stores `railway.rs:26` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;sideTrack&gt; | stores `railway.rs:47` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;sideTrack&gt; `@id` | stores `railway.rs:51` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;sideTrack&gt; `@s` | stores `railway.rs:31` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;sideTrack&gt; `@dir` | stores `railway.rs:26` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;partner&gt; | stores `railway.rs:56` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;partner&gt; `@name` | stores `railway.rs:59` | - | - | - |
| &lt;road&gt; &lt;railroad&gt; &lt;switch&gt; &lt;partner&gt; `@id` | stores `railway.rs:58` | - | - | - |

### &lt;controller&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;controller&gt; | builds `signals.rs:103` | stores `RoadManager.cpp:5472` | - | builds `ControllerParser.cpp:23` |
| &lt;controller&gt; `@id` | builds `signals.rs:125` | stores `RoadManager.cpp:5474` | - | builds `ControllerParser.cpp:27` |
| &lt;controller&gt; `@name` | stores `signals.rs:121` | stores `RoadManager.cpp:5475` | - | stores `ControllerParser.cpp:28` |
| &lt;controller&gt; `@sequence` | stores `signals.rs:100` | stores `RoadManager.cpp:5476` | - | stores `ControllerParser.cpp:29` |
| &lt;controller&gt; &lt;control&gt; | builds `signals.rs:106` | stores `RoadManager.cpp:5479` | - | builds `ControllerParser.cpp:38` |
| &lt;controller&gt; &lt;control&gt; `@signalId` | builds `signals.rs:107` | stores `RoadManager.cpp:5483` | - | builds `ControllerParser.cpp:39` |
| &lt;controller&gt; &lt;control&gt; `@type` | stores `signals.rs:112` | stores `RoadManager.cpp:5484` | - | - |

### &lt;junction&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;junction&gt; | builds `mod.rs:632` | builds `RoadManager.cpp:5491` | builds `OpenDriveMap.cpp:92` | builds `JunctionParser.cpp:44` |
| &lt;junction&gt; `@name` | stores `virtual_junctions.rs:171` | stores `RoadManager.cpp:5493` | stores `OpenDriveMap.cpp:98` | stores `JunctionParser.cpp:47` |
| &lt;junction&gt; `@id` | builds `mod.rs:635` | builds `RoadManager.cpp:5495` | builds `OpenDriveMap.cpp:95` | builds `JunctionParser.cpp:46` |
| &lt;junction&gt; `@type` | builds `links.rs:518` | builds `RoadManager.cpp:5494` | - | - |
| &lt;junction&gt; `@mainRoad` | builds `virtual_junctions.rs:34` | - | - | - |
| &lt;junction&gt; `@sStart` | builds `virtual_junctions.rs:37` | - | - | - |
| &lt;junction&gt; `@sEnd` | builds `virtual_junctions.rs:37` | - | - | - |
| &lt;junction&gt; `@orientation` | builds `virtual_junctions.rs:173` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; | builds `links.rs:506` | builds `RoadManager.cpp:5510` | builds `OpenDriveMap.cpp:101` | builds `JunctionParser.cpp:50` |
| &lt;junction&gt; &lt;connection&gt; `@id` | builds `links.rs:503` | stores `RoadManager.cpp:5515` | stores `OpenDriveMap.cpp:110` | stores `JunctionParser.cpp:53` |
| &lt;junction&gt; &lt;connection&gt; `@incomingRoad` | builds `links.rs:509` | builds `RoadManager.cpp:5517` | builds `OpenDriveMap.cpp:114` | builds `JunctionParser.cpp:54` |
| &lt;junction&gt; &lt;connection&gt; `@contactPoint` | builds `links.rs:429` | builds `RoadManager.cpp:5548` | builds `OpenDriveMap.cpp:103` | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; | builds `links.rs:543` | builds `RoadManager.cpp:5564` | builds `OpenDriveMap.cpp:119` | builds `JunctionParser.cpp:58` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@from` | builds `links.rs:546` | builds `RoadManager.cpp:5567` | builds `OpenDriveMap.cpp:121` | builds `JunctionParser.cpp:61` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@to` | builds `links.rs:546` | builds `RoadManager.cpp:5568` | builds `OpenDriveMap.cpp:121` | builds `JunctionParser.cpp:62` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@overlapZone` | stores `links.rs:543` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@fromLayer` | builds `links.rs:550` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@toLayer` | builds `links.rs:551` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@type` | builds `links.rs:518` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@connectingRoad` | builds `links.rs:523` | builds `RoadManager.cpp:5527` | builds `OpenDriveMap.cpp:115` | builds `JunctionParser.cpp:55` |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; | builds `virtual_junctions.rs:137` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementType` | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementId` | builds `virtual_junctions.rs:80` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementS` | builds `virtual_junctions.rs:71` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementDir` | builds `virtual_junctions.rs:186` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;successor&gt; | builds `virtual_junctions.rs:138` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; | builds `junction_areas.rs:127` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@crossingRoad` | builds `junction_areas.rs:128` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@id` | builds `junction_areas.rs:126` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@roadAtEnd` | builds `junction_areas.rs:170` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@roadAtStart` | builds `junction_areas.rs:169` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; | builds `junction_areas.rs:169` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@s` | builds `junction_areas.rs:135` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@from` | builds `junction_areas.rs:158` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@to` | builds `junction_areas.rs:162` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;endLaneLink&gt; | builds `junction_areas.rs:170` | - | - | - |
| &lt;junction&gt; &lt;priority&gt; | stores `mod.rs:690` | - | stores `OpenDriveMap.cpp:131` | - |
| &lt;junction&gt; &lt;priority&gt; `@high` | stores `mod.rs:691` | - | stores `OpenDriveMap.cpp:133` | - |
| &lt;junction&gt; &lt;priority&gt; `@low` | stores `mod.rs:691` | - | stores `OpenDriveMap.cpp:133` | - |
| &lt;junction&gt; &lt;controller&gt; | builds `signals.rs:134` | stores `RoadManager.cpp:5575` | stores `OpenDriveMap.cpp:137` | builds `JunctionParser.cpp:71` |
| &lt;junction&gt; &lt;controller&gt; `@id` | builds `signals.rs:135` | stores `RoadManager.cpp:5579` | stores `OpenDriveMap.cpp:139` | builds `JunctionParser.cpp:72` |
| &lt;junction&gt; &lt;controller&gt; `@type` | stores `signals.rs:142` | stores `RoadManager.cpp:5580` | stores `OpenDriveMap.cpp:142` | - |
| &lt;junction&gt; &lt;controller&gt; `@sequence` | stores `signals.rs:100` | stores `RoadManager.cpp:5581` | stores `OpenDriveMap.cpp:143` | - |
| &lt;junction&gt; &lt;surface&gt; | builds `mod.rs:776` | - | - | - |
| &lt;junction&gt; &lt;planView&gt; | builds `junction_areas.rs:202` | - | - | - |
| &lt;junction&gt; &lt;objects&gt; | - | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@linkedRoad` | builds `links.rs:521` | builds `RoadManager.cpp:5523` | - | - |
| &lt;junction&gt; &lt;roadSection&gt; | - | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@id` | - | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@roadId` | - | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@sStart` | - | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@sEnd` | - | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; | builds `junction_areas.rs:26` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; | builds `junction_areas.rs:258` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@roadId` | builds `junction_areas.rs:259` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@type` | builds `junction_areas.rs:260` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@boundaryLane` | builds `junction_areas.rs:361` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@sStart` | builds `junction_areas.rs:362` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@sEnd` | builds `junction_areas.rs:363` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@contactPoint` | builds `junction_areas.rs:389` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@jointLaneStart` | builds `junction_areas.rs:409` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@jointLaneEnd` | builds `junction_areas.rs:409` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@transitionLength` | - | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; | builds `junction_areas.rs:27` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; `@sStart` | builds `junction_areas.rs:225` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; `@gridSpacing` | builds `junction_areas.rs:210` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; | builds `junction_areas.rs:229` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@left` | builds `junction_areas.rs:233` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@center` | builds `junction_areas.rs:232` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@right` | builds `junction_areas.rs:234` | - | - | - |

### &lt;junctionGroup&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;junctionGroup&gt; | stores `junction_areas.rs:64` | - | - | - |
| &lt;junctionGroup&gt; `@name` | stores `junction_areas.rs:100` | - | - | - |
| &lt;junctionGroup&gt; `@id` | stores `junction_areas.rs:66` | - | - | - |
| &lt;junctionGroup&gt; `@type` | stores `junction_areas.rs:67` | - | - | - |
| &lt;junctionGroup&gt; &lt;junctionReference&gt; | stores `junction_areas.rs:83` | - | - | - |
| &lt;junctionGroup&gt; &lt;junctionReference&gt; `@junction` | stores `junction_areas.rs:85` | - | - | - |

### &lt;station&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;station&gt; | stores `railway.rs:83` | - | - | - |
| &lt;station&gt; `@name` | stores `railway.rs:90` | - | - | - |
| &lt;station&gt; `@id` | stores `railway.rs:89` | - | - | - |
| &lt;station&gt; `@type` | stores `railway.rs:128` | - | - | - |
| &lt;station&gt; &lt;platform&gt; | stores `railway.rs:87` | - | - | - |
| &lt;station&gt; &lt;platform&gt; `@name` | stores `railway.rs:90` | - | - | - |
| &lt;station&gt; &lt;platform&gt; `@id` | stores `railway.rs:89` | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; | stores `railway.rs:93` | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; `@roadId` | stores `railway.rs:96` | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; `@sStart` | stores `railway.rs:98` | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; `@sEnd` | stores `railway.rs:99` | - | - | - |
| &lt;station&gt; &lt;platform&gt; &lt;segment&gt; `@side` | stores `railway.rs:101` | - | - | - |

### &lt;vmsGroup&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;vmsGroup&gt; | - | - | - | - |
| &lt;vmsGroup&gt; `@id` | - | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; | - | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@signalId` | - | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@vmsIndex` | - | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@groupIndex` | - | - | - | - |

### &lt;dataQuality&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;dataQuality&gt; | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@xyAbsolute` | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@zAbsolute` | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@xyRelative` | - | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@zRelative` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@date` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@source` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@sourceComment` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@postProcessing` | - | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@postProcessingComment` | - | - | - | - |

### &lt;include&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;include&gt; | - | - | - | - |
| &lt;include&gt; `@file` | - | - | - | - |

### &lt;userData&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;userData&gt; | stores `mod.rs:2830` | builds `RoadManager.cpp:5397` | - | - |
| &lt;userData&gt; `@code` | stores `mod.rs:2834` | builds `RoadManager.cpp:5399` | - | - |
| &lt;userData&gt; `@value` | stores `mod.rs:2835` | builds `RoadManager.cpp:5402` | - | - |
