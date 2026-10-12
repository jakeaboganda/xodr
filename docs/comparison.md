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
under `<staticBoard>`. This crate reads every one of those `<semantics>`
kinds under the header's `<defaultRegulations>`, a `<signal>` and a board's
`<sign>` alike. The `<semantics>` rows under `<signal>` give where.

This crate reads every row. A `<crossfall>` from OpenDRIVE 1.4, which 1.9
dropped and so has no row, it reads too, as libOpenDRIVE does.

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
library. That holds when no row has another library doing more. 20 rows do,
and the next section lists them.

## Summary

Rows each library reads, of the 671 in the table. An element and each of
its attributes is its own row.

| | builds | stores | ignores |
| --- | --- | --- | --- |
| this crate | 420 | 251 | 0 |
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
  tunnels from it. The crate keeps every pair, with the element it is on,
  and draws nothing.

| Element | This crate | Another library |
| --- | --- | --- |
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
| Road `(s, t)` to a point | `road_point`, `network.rs:970`, on the lane surface | `Position::SetTrackPos`, `RoadManager.cpp:10099` | `Road::get_surface_pt`, `Road.cpp:139` | - (a waypoint has no `t`) |
| A point to road `(s, t)` | `road_position`, `network.rs:987` | `Position::XYZ2TrackPos`, `RoadManager.cpp:9048` | reference line `s` only: `RefLine::match`, `RefLine.cpp:86` | - |
| Lane, `s` and offset to a point | `lane_point`, `network.rs:1042` | `Position::SetLanePos`, `RoadManager.cpp:10757` | - | lane center only: `Map::ComputeTransform`, `Map.cpp:273` |
| A point to lane, `s` and offset | `lane_position`, `network.rs:1068` | `Position::XYZ2TrackPos`, `RoadManager.cpp:9048` | lane from `(s, t)`: `LaneSection::get_lane`, `LaneSection.cpp:35` | snapped to a lane center: `Map::GetWaypoint`, `Map.cpp:212` |
| Move a distance along the lanes | every branch: `advance`, `advance.rs:74` | one branch: `Position::MoveAlongS`, `RoadManager.cpp:10547` | - | every branch: `Map::GetNext`, `Map.cpp:554` |
| Step to the lane beside | `left_of`, `right_of`, `advance.rs:208` | - (`SetLanePos` with another lane) | - | `Map::GetLeft`, `Map.cpp:636` |
| May a vehicle cross into the lane beside | `may_change_left`, `may_change_right`, `advance.rs:228`, from the road mark's `laneChange` | - (stores `laneChange`) | - (stores `laneChange`) | the lane marking's `lane_change`, from `Map::GetMarkRecord`, `Map.cpp:307` |
| Route between two points | `route`, `route.rs:34` | `RoadPath::Calculate`, `RoadManager.cpp:6199` | lane keys only: `RoutingGraph::shortest_path`, `RoutingGraph.cpp:40` | - (not in `LibCarla/road`) |
| Road surface mesh | `surface_mesh`, `mesh.rs:313` | - (not in `RoadManager`) | `OpenDriveMap::get_road_network_mesh`, `OpenDriveMap.cpp:682` | `Map::GenerateMesh`, `Map.cpp:1005` |
| Object mesh | `object_mesh`, `object_mesh.rs:61` | - (not in `RoadManager`) | `Road::get_road_object_mesh`, in the network mesh | - |

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
| &lt;header&gt; | builds `mod.rs:1708` | builds `RoadManager.cpp:3791` | stores `OpenDriveMap.cpp:73` | stores `GeoReferenceParser.cpp:66` |
| &lt;header&gt; `@revMajor` | stores `mod.rs:1714` | stores `RoadManager.cpp:3794` | - | - |
| &lt;header&gt; `@revMinor` | stores `mod.rs:1715` | stores `RoadManager.cpp:3795` | - | - |
| &lt;header&gt; `@name` | stores `mod.rs:1716` | - | - | - |
| &lt;header&gt; `@version` | stores `mod.rs:1717` | - | - | - |
| &lt;header&gt; `@date` | stores `mod.rs:1718` | - | - | - |
| &lt;header&gt; `@north` | stores `mod.rs:1720` | - | - | - |
| &lt;header&gt; `@south` | stores `mod.rs:1721` | - | - | - |
| &lt;header&gt; `@east` | stores `mod.rs:1722` | - | - | - |
| &lt;header&gt; `@west` | stores `mod.rs:1723` | - | - | - |
| &lt;header&gt; `@vendor` | stores `mod.rs:1719` | - | - | - |
| &lt;header&gt; &lt;geoReference&gt; | stores `mod.rs:1766` | stores `RoadManager.cpp:3804` | stores `OpenDriveMap.cpp:74` | stores `GeoReferenceParser.cpp:66` |
| &lt;header&gt; &lt;offset&gt; | stores `mod.rs:1771` | builds `RoadManager.cpp:3808` | - | - |
| &lt;header&gt; &lt;offset&gt; `@x` | stores `mod.rs:1774` | builds `RoadManager.cpp:3808` | - | - |
| &lt;header&gt; &lt;offset&gt; `@y` | stores `mod.rs:1775` | builds `RoadManager.cpp:3809` | - | - |
| &lt;header&gt; &lt;offset&gt; `@z` | stores `mod.rs:1776` | builds `RoadManager.cpp:3810` | - | - |
| &lt;header&gt; &lt;offset&gt; `@hdg` | stores `mod.rs:1777` | builds `RoadManager.cpp:3811` | - | - |
| &lt;header&gt; &lt;license&gt; | stores `mod.rs:1708` | - | - | - |
| &lt;header&gt; &lt;license&gt; `@name` | stores `mod.rs:1716` | - | - | - |
| &lt;header&gt; &lt;license&gt; `@spdxid` | stores `mod.rs:1708` | - | - | - |
| &lt;header&gt; &lt;license&gt; `@text` | stores `mod.rs:1708` | - | - | - |
| &lt;header&gt; &lt;license&gt; `@resource` | stores `mod.rs:1708` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; | builds `mod.rs:1724` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; | builds `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; `@type` | builds `mod.rs:1743` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; | builds `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; | builds `properties.rs:94` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@type` | builds `properties.rs:90` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@value` | builds `properties.rs:183` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;speed&gt; `@unit` | builds `properties.rs:50` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;lane&gt; | stores `mod.rs:1545` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;lane&gt; `@type` | stores `mod.rs:1551` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;priority&gt; | stores `mod.rs:921` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;priority&gt; `@type` | stores `mod.rs:1240` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;animal&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;person&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;person&gt; &lt;type&gt; | stores `mod.rs:1743` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;vehicle&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;prohibited&gt; &lt;vehicle&gt; &lt;type&gt; | stores `mod.rs:1743` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;warning&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;routing&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;streetname&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;parking&gt; | stores `mod.rs:1599` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;tourist&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; `@type` | stores `mod.rs:1743` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryTime&gt; `@value` | stores `mod.rs:3755` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;animal&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;person&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryAllows&gt; &lt;vehicle&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;animal&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;person&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryProhibits&gt; &lt;vehicle&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@type` | stores `mod.rs:1743` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@value` | stores `mod.rs:3755` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryDistance&gt; `@unit` | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryEnvironment&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryEnvironment&gt; `@type` | stores `mod.rs:1743` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;roadRegulations&gt; &lt;semantics&gt; &lt;supplementaryExplanatory&gt; | stores `mod.rs:1741` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; | stores `mod.rs:1748` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; `@type` | stores `mod.rs:1750` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; `@subtype` | stores `mod.rs:1751` | - | - | - |
| &lt;header&gt; &lt;defaultRegulations&gt; &lt;signalRegulations&gt; &lt;semantics&gt; | stores `mod.rs:1748` | - | - | - |

### &lt;road&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;road&gt; | builds `mod.rs:695` | builds `RoadManager.cpp:3830` | builds `OpenDriveMap.cpp:147` | builds `RoadParser.cpp:119` |
| &lt;road&gt; `@name` | stores `mod.rs:1884` | stores `RoadManager.cpp:3829` | stores `OpenDriveMap.cpp:164` | stores `RoadParser.cpp:124` |
| &lt;road&gt; `@length` | builds `mod.rs:696` | builds `RoadManager.cpp:3832` | builds `OpenDriveMap.cpp:162` | builds `RoadParser.cpp:125` |
| &lt;road&gt; `@id` | builds `mod.rs:737` | builds `RoadManager.cpp:3830` | builds `OpenDriveMap.cpp:150` | builds `RoadParser.cpp:123` |
| &lt;road&gt; `@junction` | builds `mod.rs:1863` | builds `RoadManager.cpp:3836` | stores `OpenDriveMap.cpp:163` | builds `RoadParser.cpp:126` |
| &lt;road&gt; `@rule` | builds `mod.rs:1653` | builds `RoadManager.cpp:3846` | stores `OpenDriveMap.cpp:155` | - |
| &lt;road&gt; &lt;link&gt; | builds `links.rs:414` | builds `RoadManager.cpp:3958` | builds `OpenDriveMap.cpp:175` | builds `RoadParser.cpp:129` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; | builds `links.rs:432` | builds `RoadManager.cpp:3958` | builds `OpenDriveMap.cpp:175` | builds `RoadParser.cpp:129` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementId` | builds `links.rs:425` | builds `RoadManager.cpp:2579` | builds `OpenDriveMap.cpp:179` | builds `RoadParser.cpp:132` |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementType` | builds `links.rs:419` | builds `RoadManager.cpp:2576` | builds `OpenDriveMap.cpp:181` | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@contactPoint` | builds `links.rs:427` | builds `RoadManager.cpp:2583` | builds `OpenDriveMap.cpp:190` | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementS` | builds `links.rs:428` | - | - | - |
| &lt;road&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@elementDir` | builds `virtual_junctions.rs:189` | - | - | - |
| &lt;road&gt; &lt;type&gt; | builds `properties.rs:86` | builds `RoadManager.cpp:3857` | stores `OpenDriveMap.cpp:213` | stores `RoadParser.cpp:140` |
| &lt;road&gt; &lt;type&gt; `@s` | builds `properties.rs:89` | builds `RoadManager.cpp:3921` | stores `OpenDriveMap.cpp:215` | stores `RoadParser.cpp:143` |
| &lt;road&gt; &lt;type&gt; `@type` | stores `properties.rs:86` | stores `RoadManager.cpp:3861` | stores `OpenDriveMap.cpp:216` | stores `RoadParser.cpp:144` |
| &lt;road&gt; &lt;type&gt; `@country` | stores `properties.rs:91` | - | - | - |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; | builds `properties.rs:94` | builds `RoadManager.cpp:3928` | stores `OpenDriveMap.cpp:221` | stores `RoadParser.cpp:147` |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; `@max` | builds `properties.rs:41` | builds `RoadManager.cpp:3928` | stores `OpenDriveMap.cpp:223` | stores `RoadParser.cpp:149` |
| &lt;road&gt; &lt;type&gt; &lt;speed&gt; `@unit` | builds `properties.rs:50` | builds `RoadManager.cpp:3929` | stores `OpenDriveMap.cpp:224` | stores `RoadParser.cpp:150` |
| &lt;road&gt; &lt;planView&gt; | builds `mod.rs:1808` | builds `RoadManager.cpp:3987` | builds `OpenDriveMap.cpp:232` | builds `GeometryParser.cpp:70` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; | builds `mod.rs:1811` | builds `RoadManager.cpp:3990` | builds `OpenDriveMap.cpp:232` | builds `GeometryParser.cpp:73` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@s` | builds `mod.rs:1816` | builds `RoadManager.cpp:3992` | builds `OpenDriveMap.cpp:234` | builds `GeometryParser.cpp:80` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@x` | builds `mod.rs:1774` | builds `RoadManager.cpp:3993` | builds `OpenDriveMap.cpp:235` | builds `GeometryParser.cpp:81` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@y` | builds `mod.rs:1775` | builds `RoadManager.cpp:3994` | builds `OpenDriveMap.cpp:236` | builds `GeometryParser.cpp:82` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@hdg` | builds `mod.rs:1777` | builds `RoadManager.cpp:3995` | builds `OpenDriveMap.cpp:237` | builds `GeometryParser.cpp:83` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; `@length` | builds `mod.rs:1973` | builds `RoadManager.cpp:3996` | builds `OpenDriveMap.cpp:238` | builds `GeometryParser.cpp:84` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;line&gt; | builds `mod.rs:2012` | builds `RoadManager.cpp:4010` | builds `OpenDriveMap.cpp:245` | builds `GeometryParser.cpp:88` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; | builds `mod.rs:1983` | builds `RoadManager.cpp:4021` | builds `OpenDriveMap.cpp:249` | builds `GeometryParser.cpp:91` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; `@curvStart` | builds `mod.rs:1984` | builds `RoadManager.cpp:4021` | builds `OpenDriveMap.cpp:251` | builds `GeometryParser.cpp:92` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;spiral&gt; `@curvEnd` | builds `mod.rs:1984` | builds `RoadManager.cpp:4022` | builds `OpenDriveMap.cpp:252` | builds `GeometryParser.cpp:93` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;arc&gt; | builds `mod.rs:1980` | builds `RoadManager.cpp:4016` | builds `OpenDriveMap.cpp:276` | builds `GeometryParser.cpp:89` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;arc&gt; `@curvature` | builds `mod.rs:1981` | builds `RoadManager.cpp:4016` | builds `OpenDriveMap.cpp:278` | builds `GeometryParser.cpp:90` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; | builds `mod.rs:2003` | builds `RoadManager.cpp:4027` | - | builds `GeometryParser.cpp:94` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@a` | builds `mod.rs:2009` | builds `RoadManager.cpp:4027` | - | builds `GeometryParser.cpp:95` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@b` | builds `mod.rs:2009` | builds `RoadManager.cpp:4028` | - | builds `GeometryParser.cpp:96` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@c` | builds `mod.rs:2009` | builds `RoadManager.cpp:4029` | - | builds `GeometryParser.cpp:97` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;poly3&gt; `@d` | builds `mod.rs:2009` | builds `RoadManager.cpp:4030` | - | builds `GeometryParser.cpp:98` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; | builds `mod.rs:1989` | builds `RoadManager.cpp:4035` | builds `OpenDriveMap.cpp:281` | builds `GeometryParser.cpp:99` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@aU` | builds `mod.rs:1999` | builds `RoadManager.cpp:4035` | builds `OpenDriveMap.cpp:283` | builds `GeometryParser.cpp:100` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@bU` | builds `mod.rs:1999` | builds `RoadManager.cpp:4036` | builds `OpenDriveMap.cpp:284` | builds `GeometryParser.cpp:101` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@cU` | builds `mod.rs:1999` | builds `RoadManager.cpp:4037` | builds `OpenDriveMap.cpp:285` | builds `GeometryParser.cpp:102` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@dU` | builds `mod.rs:1999` | builds `RoadManager.cpp:4038` | builds `OpenDriveMap.cpp:286` | builds `GeometryParser.cpp:103` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@aV` | builds `mod.rs:2000` | builds `RoadManager.cpp:4039` | builds `OpenDriveMap.cpp:287` | builds `GeometryParser.cpp:104` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@bV` | builds `mod.rs:2000` | builds `RoadManager.cpp:4040` | builds `OpenDriveMap.cpp:288` | builds `GeometryParser.cpp:105` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@cV` | builds `mod.rs:2000` | builds `RoadManager.cpp:4041` | builds `OpenDriveMap.cpp:289` | builds `GeometryParser.cpp:106` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@dV` | builds `mod.rs:2000` | builds `RoadManager.cpp:4042` | builds `OpenDriveMap.cpp:290` | builds `GeometryParser.cpp:107` |
| &lt;road&gt; &lt;planView&gt; &lt;geometry&gt; &lt;paramPoly3&gt; `@pRange` | builds `mod.rs:1994` | builds `RoadManager.cpp:4045` | builds `OpenDriveMap.cpp:293` | builds `GeometryParser.cpp:108` |
| &lt;road&gt; &lt;elevationProfile&gt; | builds `mod.rs:1825` | builds `RoadManager.cpp:4088` | builds `OpenDriveMap.cpp:313` | builds `ProfilesParser.cpp:56` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; | builds `mod.rs:1826` | builds `RoadManager.cpp:4088` | builds `OpenDriveMap.cpp:325` | builds `ProfilesParser.cpp:60` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@s` | builds `mod.rs:1826` | builds `RoadManager.cpp:4090` | builds `OpenDriveMap.cpp:328` | builds `ProfilesParser.cpp:68` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@a` | builds `mod.rs:3009` | builds `RoadManager.cpp:4091` | builds `OpenDriveMap.cpp:329` | builds `ProfilesParser.cpp:69` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@b` | builds `mod.rs:3010` | builds `RoadManager.cpp:4092` | builds `OpenDriveMap.cpp:330` | builds `ProfilesParser.cpp:70` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@c` | builds `mod.rs:3011` | builds `RoadManager.cpp:4093` | builds `OpenDriveMap.cpp:331` | builds `ProfilesParser.cpp:71` |
| &lt;road&gt; &lt;elevationProfile&gt; &lt;elevation&gt; `@d` | builds `mod.rs:3012` | builds `RoadManager.cpp:4094` | builds `OpenDriveMap.cpp:332` | builds `ProfilesParser.cpp:72` |
| &lt;road&gt; &lt;lateralProfile&gt; | builds `mod.rs:1831` | builds `RoadManager.cpp:4108` | builds `OpenDriveMap.cpp:317` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; | builds `mod.rs:1832` | builds `RoadManager.cpp:4111` | builds `OpenDriveMap.cpp:317` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@s` | builds `mod.rs:1832` | builds `RoadManager.cpp:4114` | builds `OpenDriveMap.cpp:328` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@a` | builds `mod.rs:3009` | builds `RoadManager.cpp:4115` | builds `OpenDriveMap.cpp:329` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@b` | builds `mod.rs:3010` | builds `RoadManager.cpp:4116` | builds `OpenDriveMap.cpp:330` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@c` | builds `mod.rs:3011` | builds `RoadManager.cpp:4117` | builds `OpenDriveMap.cpp:331` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;superelevation&gt; `@d` | builds `mod.rs:3012` | builds `RoadManager.cpp:4118` | builds `OpenDriveMap.cpp:332` | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; | builds `mod.rs:1302` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@s` | builds `mod.rs:1306` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@t` | builds `mod.rs:1308` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@a` | builds `mod.rs:1309` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@b` | builds `mod.rs:1310` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@c` | builds `mod.rs:1311` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;shape&gt; `@d` | builds `mod.rs:1312` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; | builds `mod.rs:1840` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; | builds `mod.rs:1345` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; | builds `mod.rs:1342` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@a` | builds `mod.rs:1309` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@b` | builds `mod.rs:1310` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@c` | builds `mod.rs:1311` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@d` | builds `mod.rs:1312` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;tOffset&gt; &lt;coefficients&gt; `@s` | builds `mod.rs:1342` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; | builds `mod.rs:1348` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; | builds `mod.rs:1351` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; `@id` | builds `mod.rs:1359` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; `@mode` | builds `mod.rs:1378` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;width&gt; | builds `mod.rs:1392` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;width&gt; &lt;coefficients&gt; | builds `mod.rs:1342` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;constant&gt; | builds `mod.rs:1394` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;constant&gt; &lt;coefficients&gt; | builds `mod.rs:1342` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;linear&gt; | builds `mod.rs:1395` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;linear&gt; &lt;coefficients&gt; | builds `mod.rs:1342` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;quadratic&gt; | builds `mod.rs:1396` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;quadratic&gt; &lt;coefficients&gt; | builds `mod.rs:1342` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;cubic&gt; | builds `mod.rs:1397` | - | - | - |
| &lt;road&gt; &lt;lateralProfile&gt; &lt;crossSectionSurface&gt; &lt;surfaceStrips&gt; &lt;strip&gt; &lt;cubic&gt; &lt;coefficients&gt; | builds `mod.rs:1342` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; | builds `mod.rs:2043` | builds `RoadManager.cpp:4132` | builds `OpenDriveMap.cpp:374` | builds `LaneParser.cpp:197` |
| &lt;road&gt; &lt;lanes&gt; `@layer` | builds `mod.rs:2479` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; | builds `mod.rs:1860` | builds `RoadManager.cpp:4137` | builds `OpenDriveMap.cpp:314` | builds `RoadParser.cpp:158` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@s` | builds `mod.rs:1857` | builds `RoadManager.cpp:4139` | builds `OpenDriveMap.cpp:328` | builds `RoadParser.cpp:160` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@a` | builds `mod.rs:3009` | builds `RoadManager.cpp:4140` | builds `OpenDriveMap.cpp:329` | builds `RoadParser.cpp:161` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@b` | builds `mod.rs:3010` | builds `RoadManager.cpp:4141` | builds `OpenDriveMap.cpp:330` | builds `RoadParser.cpp:162` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@c` | builds `mod.rs:3011` | builds `RoadManager.cpp:4142` | builds `OpenDriveMap.cpp:331` | builds `RoadParser.cpp:163` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneOffset&gt; `@d` | builds `mod.rs:3012` | builds `RoadManager.cpp:4143` | builds `OpenDriveMap.cpp:332` | builds `RoadParser.cpp:164` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; | builds `mod.rs:2048` | builds `RoadManager.cpp:4148` | builds `OpenDriveMap.cpp:374` | builds `LaneParser.cpp:199` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@length` | builds `mod.rs:1973` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@s` | builds `mod.rs:2057` | builds `RoadManager.cpp:4148` | builds `OpenDriveMap.cpp:376` | builds `LaneParser.cpp:200` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; `@singleSide` | builds `mod.rs:2048` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; | builds `mod.rs:2050` | builds `RoadManager.cpp:4163` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:201` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; | builds `mod.rs:2054` | builds `RoadManager.cpp:4179` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:22` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@type` | builds `mod.rs:1750` | builds `RoadManager.cpp:4190` | stores `OpenDriveMap.cpp:388` | builds `RoadParser.cpp:184` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@advisory` | stores `mod.rs:1212` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@direction` | builds `mod.rs:1239` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@dynamicLaneType` | stores `mod.rs:1218` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@dynamicLaneDirection` | stores `mod.rs:1219` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@level` | builds `mod.rs:1552` | - | builds `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:185` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@roadWorks` | stores `mod.rs:1609` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; `@id` | builds `mod.rs:1958` | builds `RoadManager.cpp:4288` | builds `OpenDriveMap.cpp:383` | builds `RoadParser.cpp:183` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; | builds `links.rs:436` | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | builds `RoadParser.cpp:188` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; | builds `links.rs:436` | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | builds `RoadParser.cpp:188` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@id` | builds `links.rs:456` | builds `RoadManager.cpp:4311` | builds `OpenDriveMap.cpp:392` | builds `RoadParser.cpp:191` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;link&gt; &lt;predecessor\|successor&gt; `@layer` | builds `links.rs:457` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; | builds `mod.rs:1491` | - | - | stores `LaneParser.cpp:48` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@sOffset` | builds `mod.rs:3027` | - | - | stores `LaneParser.cpp:49` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@a` | builds `mod.rs:3028` | - | - | stores `LaneParser.cpp:50` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@b` | builds `mod.rs:3029` | - | - | stores `LaneParser.cpp:51` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@c` | builds `mod.rs:3030` | - | - | stores `LaneParser.cpp:52` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;border&gt; `@d` | builds `mod.rs:3031` | - | - | stores `LaneParser.cpp:53` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; | builds `mod.rs:1490` | builds `RoadManager.cpp:4321` | builds `OpenDriveMap.cpp:397` | builds `LaneParser.cpp:30` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@sOffset` | builds `mod.rs:3027` | builds `RoadManager.cpp:4323` | builds `OpenDriveMap.cpp:399` | builds `LaneParser.cpp:31` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@a` | builds `mod.rs:3028` | builds `RoadManager.cpp:4324` | builds `OpenDriveMap.cpp:400` | builds `LaneParser.cpp:32` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@b` | builds `mod.rs:3029` | builds `RoadManager.cpp:4325` | builds `OpenDriveMap.cpp:401` | builds `LaneParser.cpp:33` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@c` | builds `mod.rs:3030` | builds `RoadManager.cpp:4326` | builds `OpenDriveMap.cpp:402` | builds `LaneParser.cpp:34` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;width&gt; `@d` | builds `mod.rs:3031` | builds `RoadManager.cpp:4327` | builds `OpenDriveMap.cpp:403` | builds `LaneParser.cpp:35` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; | builds `road_marks.rs:153` | builds `RoadManager.cpp:4341` | builds `OpenDriveMap.cpp:422` | builds `LaneParser.cpp:61` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@sOffset` | builds `road_marks.rs:168` | builds `RoadManager.cpp:4344` | builds `OpenDriveMap.cpp:429` | stores `LaneParser.cpp:64` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@type` | builds `road_marks.rs:169` | builds `RoadManager.cpp:4360` | stores `OpenDriveMap.cpp:430` | stores `LaneParser.cpp:65` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@weight` | builds `road_marks.rs:170` | stores `RoadManager.cpp:4415` | builds `OpenDriveMap.cpp:431` | stores `LaneParser.cpp:66` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@color` | stores `road_marks.rs:174` | stores `RoadManager.cpp:1543` | stores `OpenDriveMap.cpp:432` | stores `LaneParser.cpp:67` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@material` | stores `road_marks.rs:175` | - | stores `OpenDriveMap.cpp:433` | stores `LaneParser.cpp:68` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@width` | builds `road_marks.rs:157` | builds `RoadManager.cpp:4483` | builds `OpenDriveMap.cpp:427` | stores `LaneParser.cpp:69` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@laneChange` | builds `road_marks.rs:182` | stores `RoadManager.cpp:4449` | stores `OpenDriveMap.cpp:434` | builds `LaneParser.cpp:70` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; `@height` | builds `road_marks.rs:181` | stores `RoadManager.cpp:4499` | stores `OpenDriveMap.cpp:428` | stores `LaneParser.cpp:71` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; | builds `road_marks.rs:188` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@ds` | builds `road_marks.rs:188` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@a` | builds `mod.rs:3009` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@b` | builds `mod.rs:3010` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@c` | builds `mod.rs:3011` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;sway&gt; `@d` | builds `mod.rs:3012` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; | builds `road_marks.rs:155` | builds `RoadManager.cpp:4511` | builds `OpenDriveMap.cpp:440` | stores `LaneParser.cpp:77` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; `@name` | stores `road_marks.rs:177` | stores `RoadManager.cpp:4515` | stores `OpenDriveMap.cpp:442` | stores `LaneParser.cpp:79` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; `@width` | builds `road_marks.rs:157` | stores `RoadManager.cpp:4516` | builds `OpenDriveMap.cpp:443` | stores `LaneParser.cpp:80` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; | builds `road_marks.rs:164` | builds `RoadManager.cpp:4519` | builds `OpenDriveMap.cpp:445` | stores `LaneParser.cpp:99` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@length` | builds `road_marks.rs:83` | builds `RoadManager.cpp:4521` | builds `OpenDriveMap.cpp:455` | stores `LaneParser.cpp:101` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@space` | builds `road_marks.rs:67` | builds `RoadManager.cpp:4522` | builds `OpenDriveMap.cpp:456` | stores `LaneParser.cpp:102` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@tOffset` | builds `road_marks.rs:93` | builds `RoadManager.cpp:4523` | builds `OpenDriveMap.cpp:457` | stores `LaneParser.cpp:103` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@sOffset` | builds `road_marks.rs:92` | builds `RoadManager.cpp:4524` | builds `OpenDriveMap.cpp:458` | stores `LaneParser.cpp:104` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@rule` | stores `road_marks.rs:94` | stores `RoadManager.cpp:4553` | stores `OpenDriveMap.cpp:460` | stores `LaneParser.cpp:105` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@width` | builds `road_marks.rs:100` | builds `RoadManager.cpp:4575` | builds `OpenDriveMap.cpp:447` | stores `LaneParser.cpp:106` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;type&gt; &lt;line&gt; `@color` | stores `road_marks.rs:101` | stores `RoadManager.cpp:4542` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; | builds `road_marks.rs:186` | builds `RoadManager.cpp:4601` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; | builds `road_marks.rs:164` | builds `RoadManager.cpp:4601` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@length` | builds `road_marks.rs:83` | builds `RoadManager.cpp:4603` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@tOffset` | builds `road_marks.rs:93` | builds `RoadManager.cpp:4604` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@sOffset` | builds `road_marks.rs:92` | builds `RoadManager.cpp:4605` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@rule` | stores `road_marks.rs:94` | stores `RoadManager.cpp:4610` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;roadMark&gt; &lt;explicit&gt; &lt;line&gt; `@width` | builds `road_marks.rs:100` | builds `RoadManager.cpp:4606` | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; | stores `properties.rs:197` | stores `RoadManager.cpp:4744` | - | stores `LaneParser.cpp:123` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@sOffset` | stores `properties.rs:173` | stores `RoadManager.cpp:4747` | - | stores `LaneParser.cpp:125` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@surface` | stores `mod.rs:1663` | - | - | stores `LaneParser.cpp:126` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@friction` | stores `mod.rs:1664` | stores `RoadManager.cpp:4750` | - | stores `LaneParser.cpp:127` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;material&gt; `@roughness` | stores `mod.rs:1665` | - | - | stores `LaneParser.cpp:128` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; | builds `properties.rs:111` | - | - | stores `LaneParser.cpp:147` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@sOffset` | builds `properties.rs:113` | - | - | stores `LaneParser.cpp:148` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@max` | builds `properties.rs:41` | - | - | stores `LaneParser.cpp:149` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;speed&gt; `@unit` | builds `properties.rs:50` | - | - | stores `LaneParser.cpp:150` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; | stores `properties.rs:206` | - | - | stores `LaneParser.cpp:157` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@sOffset` | stores `properties.rs:173` | - | - | stores `LaneParser.cpp:158` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@rule` | stores `properties.rs:144` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; `@restriction` | stores `properties.rs:134` | - | - | stores `LaneParser.cpp:159` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; &lt;restriction&gt; | stores `properties.rs:138` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;access&gt; &lt;restriction&gt; `@type` | stores `properties.rs:139` | - | - | - |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; | builds `mod.rs:1423` | stores `RoadManager.cpp:4332` | builds `OpenDriveMap.cpp:411` | stores `LaneParser.cpp:166` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@sOffset` | builds `mod.rs:1425` | stores `RoadManager.cpp:4334` | builds `OpenDriveMap.cpp:413` | stores `LaneParser.cpp:167` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@inner` | builds `mod.rs:1426` | stores `RoadManager.cpp:4335` | builds `OpenDriveMap.cpp:414` | stores `LaneParser.cpp:168` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;height&gt; `@outer` | builds `mod.rs:1427` | stores `RoadManager.cpp:4336` | builds `OpenDriveMap.cpp:415` | stores `LaneParser.cpp:169` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; | stores `properties.rs:183` | - | - | stores `LaneParser.cpp:176` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; `@sOffset` | stores `properties.rs:173` | - | - | stores `LaneParser.cpp:177` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;left\|right&gt; &lt;lane&gt; &lt;rule&gt; `@value` | stores `properties.rs:183` | - | - | stores `LaneParser.cpp:178` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; | builds `mod.rs:1542` | builds `RoadManager.cpp:4163` | builds `OpenDriveMap.cpp:380` | builds `LaneParser.cpp:206` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; | builds `mod.rs:1545` | builds `RoadManager.cpp:4179` | builds `OpenDriveMap.cpp:380` | builds `RoadParser.cpp:203` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@id` | stores `mod.rs:1550` | builds `RoadManager.cpp:4288` | builds `OpenDriveMap.cpp:383` | builds `RoadParser.cpp:206` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@type` | stores `mod.rs:1551` | stores `RoadManager.cpp:4190` | stores `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:207` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; `@level` | stores `mod.rs:1552` | - | builds `OpenDriveMap.cpp:388` | stores `RoadParser.cpp:208` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; &lt;link&gt; | stores `links.rs:414` | builds `RoadManager.cpp:4305` | builds `OpenDriveMap.cpp:391` | stores `RoadParser.cpp:211` |
| &lt;road&gt; &lt;lanes&gt; &lt;laneSection&gt; &lt;center&gt; &lt;lane&gt; &lt;roadMark&gt; | builds `road_marks.rs:153` | builds `RoadManager.cpp:4341` | builds `OpenDriveMap.cpp:422` | stores `LaneParser.cpp:61` |
| &lt;road&gt; &lt;objects&gt; | builds `mod.rs:755` | builds `RoadManager.cpp:5054` | builds `OpenDriveMap.cpp:523` | builds `ObjectParser.cpp:28` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; | builds `mod.rs:3362` | builds `RoadManager.cpp:5057` | builds `OpenDriveMap.cpp:523` | builds `ObjectParser.cpp:31` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@t` | builds `mod.rs:3404` | builds `RoadManager.cpp:5063` | builds `OpenDriveMap.cpp:536` | builds `ObjectParser.cpp:56` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@zOffset` | builds `mod.rs:3405` | builds `RoadManager.cpp:5111` | builds `OpenDriveMap.cpp:537` | builds `ObjectParser.cpp:57` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@type` | stores `mod.rs:3704` | stores `RoadManager.cpp:5088` | stores `OpenDriveMap.cpp:546` | builds `ObjectParser.cpp:34` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@validLength` | stores `mod.rs:3407` | - | stores `OpenDriveMap.cpp:539` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@orientation` | builds `mod.rs:3457` | builds `RoadManager.cpp:5069` | stores `OpenDriveMap.cpp:548` | builds `ObjectParser.cpp:61` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@subtype` | stores `mod.rs:3813` | stores `RoadManager.cpp:5090` | stores `OpenDriveMap.cpp:549` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@dynamic` | stores `mod.rs:3815` | - | stores `OpenDriveMap.cpp:530` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@hdg` | builds `mod.rs:3544` | builds `RoadManager.cpp:5113` | builds `OpenDriveMap.cpp:543` | builds `ObjectParser.cpp:58` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@name` | stores `mod.rs:3295` | stores `RoadManager.cpp:5065` | stores `OpenDriveMap.cpp:547` | builds `ObjectParser.cpp:35` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@pitch` | builds `mod.rs:3545` | builds `RoadManager.cpp:5114` | builds `OpenDriveMap.cpp:544` | builds `ObjectParser.cpp:59` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@id` | builds `mod.rs:3363` | stores `RoadManager.cpp:5064` | stores `OpenDriveMap.cpp:525` | builds `ObjectParser.cpp:78` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@roll` | builds `mod.rs:3546` | builds `RoadManager.cpp:5115` | builds `OpenDriveMap.cpp:545` | builds `ObjectParser.cpp:60` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@height` | builds `mod.rs:3541` | builds `RoadManager.cpp:5112` | builds `OpenDriveMap.cpp:542` | builds `ObjectParser.cpp:90` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@s` | builds `mod.rs:3403` | builds `RoadManager.cpp:5062` | builds `OpenDriveMap.cpp:535` | builds `ObjectParser.cpp:55` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@length` | builds `mod.rs:3539` | builds `RoadManager.cpp:5091` | builds `OpenDriveMap.cpp:538` | builds `ObjectParser.cpp:63` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@width` | builds `mod.rs:3540` | builds `RoadManager.cpp:5092` | builds `OpenDriveMap.cpp:540` | builds `ObjectParser.cpp:62` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@radius` | builds `mod.rs:3542` | builds `RoadManager.cpp:5093` | builds `OpenDriveMap.cpp:541` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@perpToRoad` | builds `mod.rs:3547` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@invalidated` | stores `mod.rs:3834` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; `@temporary` | stores `mod.rs:3835` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; | builds `mod.rs:3567` | builds `RoadManager.cpp:5120` | builds `OpenDriveMap.cpp:560` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@s` | builds `mod.rs:4085` | builds `RoadManager.cpp:5123` | builds `OpenDriveMap.cpp:562` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@length` | builds `mod.rs:4086` | builds `RoadManager.cpp:5129` | builds `OpenDriveMap.cpp:563` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@distance` | builds `mod.rs:4087` | builds `RoadManager.cpp:5130` | builds `OpenDriveMap.cpp:564` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@tStart` | builds `mod.rs:4142` | builds `RoadManager.cpp:5131` | builds `OpenDriveMap.cpp:565` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@tEnd` | builds `mod.rs:4142` | builds `RoadManager.cpp:5132` | builds `OpenDriveMap.cpp:566` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@heightStart` | builds `mod.rs:4142` | builds `RoadManager.cpp:5133` | builds `OpenDriveMap.cpp:569` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@heightEnd` | builds `mod.rs:4142` | builds `RoadManager.cpp:5134` | builds `OpenDriveMap.cpp:570` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@zOffsetStart` | builds `mod.rs:4142` | builds `RoadManager.cpp:5135` | builds `OpenDriveMap.cpp:571` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@zOffsetEnd` | builds `mod.rs:4142` | builds `RoadManager.cpp:5136` | builds `OpenDriveMap.cpp:572` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@widthStart` | builds `mod.rs:4142` | builds `RoadManager.cpp:5138` | builds `OpenDriveMap.cpp:567` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@widthEnd` | builds `mod.rs:4142` | builds `RoadManager.cpp:5139` | builds `OpenDriveMap.cpp:568` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@lengthStart` | builds `mod.rs:4142` | builds `RoadManager.cpp:5140` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@lengthEnd` | builds `mod.rs:4142` | builds `RoadManager.cpp:5141` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@radiusStart` | builds `mod.rs:4142` | builds `RoadManager.cpp:5142` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@radiusEnd` | builds `mod.rs:4142` | builds `RoadManager.cpp:5143` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@detachFromReferenceLine` | builds `mod.rs:4099` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@bT` | builds `mod.rs:4082` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@cT` | builds `mod.rs:4082` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;repeat&gt; `@dT` | builds `mod.rs:4082` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; | builds `mod.rs:4327` | - | builds `OpenDriveMap.cpp:591` | builds `ObjectParser.cpp:39` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@id` | builds `mod.rs:4768` | - | stores `OpenDriveMap.cpp:593` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@fillType` | stores `mod.rs:3984` | - | stores `OpenDriveMap.cpp:594` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@outer` | builds `mod.rs:3914` | - | stores `OpenDriveMap.cpp:596` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@closed` | builds `mod.rs:4427` | - | stores `OpenDriveMap.cpp:597` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; `@laneType` | stores `mod.rs:3988` | - | stores `OpenDriveMap.cpp:595` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; | builds `mod.rs:4439` | - | builds `OpenDriveMap.cpp:614` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@id` | builds `mod.rs:4452` | - | stores `OpenDriveMap.cpp:620` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@s` | builds `mod.rs:4440` | - | builds `OpenDriveMap.cpp:616` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@t` | builds `mod.rs:4440` | - | builds `OpenDriveMap.cpp:617` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@dz` | builds `mod.rs:4446` | - | builds `OpenDriveMap.cpp:618` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerRoad&gt; `@height` | builds `mod.rs:4437` | - | builds `OpenDriveMap.cpp:622` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; | builds `mod.rs:4597` | - | builds `OpenDriveMap.cpp:600` | builds `ObjectParser.cpp:42` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@id` | builds `mod.rs:4648` | - | stores `OpenDriveMap.cpp:606` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@u` | builds `mod.rs:4530` | - | builds `OpenDriveMap.cpp:602` | builds `ObjectParser.cpp:43` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@v` | builds `mod.rs:4530` | - | builds `OpenDriveMap.cpp:603` | builds `ObjectParser.cpp:44` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@z` | builds `mod.rs:4529` | - | builds `OpenDriveMap.cpp:604` | builds `ObjectParser.cpp:45` |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;cornerLocal&gt; `@height` | builds `mod.rs:4437` | - | builds `OpenDriveMap.cpp:608` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; | builds `mod.rs:4458` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@id` | builds `mod.rs:4476` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@u` | builds `curve_local.rs:85` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@v` | builds `curve_local.rs:85` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@z` | builds `curve_local.rs:83` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@height` | builds `curve_local.rs:84` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@length` | builds `curve_local.rs:90` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; `@hdg` | builds `curve_local.rs:61` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;arc&gt; | builds `curve_local.rs:101` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;arc&gt; `@curvature` | builds `curve_local.rs:102` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;line&gt; | builds `curve_local.rs:91` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; | builds `curve_local.rs:111` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@aU` | builds `curve_local.rs:120` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@bU` | builds `curve_local.rs:120` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@cU` | builds `curve_local.rs:120` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@dU` | builds `curve_local.rs:120` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@aV` | builds `curve_local.rs:121` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@bV` | builds `curve_local.rs:121` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@cV` | builds `curve_local.rs:121` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@dV` | builds `curve_local.rs:121` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;curveLocal&gt; &lt;paramPoly3&gt; `@pRange` | builds `curve_local.rs:113` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; | builds `mod.rs:4607` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; | builds `mod.rs:4610` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@side` | builds `mod.rs:4678` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@weight` | stores `mod.rs:4714` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@width` | builds `mod.rs:4705` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@color` | builds `mod.rs:4720` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@zOffset` | builds `mod.rs:4709` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@spaceLength` | builds `mod.rs:4712` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@lineLength` | builds `mod.rs:4711` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@startOffset` | builds `mod.rs:4726` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; `@stopOffset` | builds `mod.rs:4727` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; &lt;cornerReference&gt; | builds `mod.rs:4652` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outline&gt; &lt;markings&gt; &lt;marking&gt; &lt;cornerReference&gt; `@id` | builds `mod.rs:4800` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outlines&gt; | builds `mod.rs:4324` | builds `RoadManager.cpp:5248` | builds `OpenDriveMap.cpp:590` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;outlines&gt; &lt;outline&gt; | builds `mod.rs:4327` | builds `RoadManager.cpp:5251` | builds `OpenDriveMap.cpp:591` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; | stores `mod.rs:3740` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@surface` | stores `mod.rs:1663` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@friction` | stores `mod.rs:1664` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@roughness` | stores `mod.rs:1665` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;material&gt; `@roadMarkColor` | stores `mod.rs:3746` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; | builds `mod.rs:3443` | stores `RoadManager.cpp:5389` | stores `OpenDriveMap.cpp:38` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@fromLane` | builds `mod.rs:3446` | stores `RoadManager.cpp:5392` | stores `OpenDriveMap.cpp:40` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@toLane` | builds `mod.rs:3447` | stores `RoadManager.cpp:5393` | stores `OpenDriveMap.cpp:40` | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;validity&gt; `@layer` | builds `mod.rs:3448` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; | stores `mod.rs:3731` | stores `RoadManager.cpp:5290` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; `@access` | stores `mod.rs:3734` | stores `RoadManager.cpp:5295` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;parkingSpace&gt; `@restrictions` | stores `mod.rs:3735` | stores `RoadManager.cpp:5334` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;markings&gt; | builds `mod.rs:4607` | builds `RoadManager.cpp:5339` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; | builds `mod.rs:4760` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; | builds `mod.rs:4765` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@width` | builds `mod.rs:4771` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@type` | builds `mod.rs:4782` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@outlineId` | builds `mod.rs:4767` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; `@useCompleteOutline` | builds `mod.rs:4772` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;borders&gt; &lt;border&gt; &lt;cornerReference&gt; | builds `mod.rs:4798` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; | builds `mod.rs:3707` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; | builds `mod.rs:3707` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@file` | builds `mod.rs:2320` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@hideRoadSurfaceCRG` | builds `mod.rs:3877` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;surface&gt; &lt;CRG&gt; `@zScale` | builds `mod.rs:3882` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; | builds `mod.rs:3589` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; | builds `mod.rs:4544` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; `@id` | stores `mod.rs:4540` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; | builds `mod.rs:4547` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@dz` | builds `mod.rs:4525` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@radius` | builds `mod.rs:4535` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@intersectionPoint` | builds `mod.rs:4536` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@id` | stores `mod.rs:4540` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@s` | builds `mod.rs:4519` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexRoad&gt; `@t` | builds `mod.rs:4519` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; | builds `mod.rs:4547` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@radius` | builds `mod.rs:4535` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@intersectionPoint` | builds `mod.rs:4536` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@id` | stores `mod.rs:4540` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@u` | builds `mod.rs:4530` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@v` | builds `mod.rs:4530` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;object&gt; &lt;skeleton&gt; &lt;polyline&gt; &lt;vertexLocal&gt; `@z` | builds `mod.rs:4529` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; | builds `mod.rs:3480` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@s` | builds `mod.rs:3430` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@t` | builds `mod.rs:3430` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@id` | builds `mod.rs:3481` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@zOffset` | builds `mod.rs:3427` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@validLength` | stores `mod.rs:3429` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; `@orientation` | builds `mod.rs:3457` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;objectReference&gt; &lt;validity&gt; | builds `mod.rs:3443` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; | builds `mod.rs:3265` | builds `RoadManager.cpp:5439` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@s` | builds `mod.rs:3276` | builds `RoadManager.cpp:5448` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@length` | builds `mod.rs:3276` | builds `RoadManager.cpp:5445` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@name` | stores `mod.rs:3295` | stores `RoadManager.cpp:5447` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@id` | stores `mod.rs:3279` | stores `RoadManager.cpp:5444` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@type` | stores `mod.rs:3267` | stores `RoadManager.cpp:5449` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@lighting` | stores `mod.rs:3268` | stores `RoadManager.cpp:5446` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; `@daylight` | stores `mod.rs:3269` | stores `RoadManager.cpp:5443` | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;tunnel&gt; &lt;validity&gt; | builds `mod.rs:3443` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; | builds `mod.rs:3271` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@s` | builds `mod.rs:3276` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@length` | builds `mod.rs:3276` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@name` | stores `mod.rs:3295` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@id` | stores `mod.rs:3279` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; `@type` | stores `mod.rs:3272` | - | - | - |
| &lt;road&gt; &lt;objects&gt; &lt;bridge&gt; &lt;validity&gt; | builds `mod.rs:3443` | - | - | - |
| &lt;road&gt; &lt;signals&gt; | builds `signals.rs:56` | builds `RoadManager.cpp:4839` | builds `OpenDriveMap.cpp:637` | builds `SignalParser.cpp:44` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; | builds `signals.rs:64` | builds `RoadManager.cpp:4845` | builds `OpenDriveMap.cpp:637` | builds `SignalParser.cpp:46` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@id` | builds `signals.rs:330` | builds `RoadManager.cpp:4851` | stores `OpenDriveMap.cpp:639` | builds `SignalParser.cpp:49` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@name` | stores `signals.rs:175` | stores `RoadManager.cpp:4852` | stores `OpenDriveMap.cpp:648` | builds `SignalParser.cpp:50` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@dynamic` | stores `signals.rs:286` | builds `RoadManager.cpp:4856` | stores `OpenDriveMap.cpp:651` | stores `SignalParser.cpp:51` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@orientation` | builds `signals.rs:266` | builds `RoadManager.cpp:4875` | stores `OpenDriveMap.cpp:659` | builds `SignalParser.cpp:52` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@country` | stores `signals.rs:287` | builds `RoadManager.cpp:4897` | stores `OpenDriveMap.cpp:660` | stores `SignalParser.cpp:54` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@countryRevision` | stores `signals.rs:288` | builds `RoadManager.cpp:4901` | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@type` | stores `signals.rs:289` | builds `RoadManager.cpp:4917` | stores `OpenDriveMap.cpp:661` | builds `SignalParser.cpp:55` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@subtype` | stores `signals.rs:290` | builds `RoadManager.cpp:4918` | stores `OpenDriveMap.cpp:662` | stores `SignalParser.cpp:56` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@value` | stores `signals.rs:291` | builds `RoadManager.cpp:4919` | stores `OpenDriveMap.cpp:653` | stores `SignalParser.cpp:57` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@text` | stores `signals.rs:293` | stores `RoadManager.cpp:4960` | stores `OpenDriveMap.cpp:664` | stores `SignalParser.cpp:61` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@unit` | stores `signals.rs:292` | stores `RoadManager.cpp:4956` | stores `OpenDriveMap.cpp:663` | stores `SignalParser.cpp:58` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@height` | builds `signals.rs:307` | builds `RoadManager.cpp:4957` | builds `OpenDriveMap.cpp:654` | builds `SignalParser.cpp:59` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@length` | builds `signals.rs:305` | builds `RoadManager.cpp:4959` | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@hOffset` | builds `signals.rs:365` | builds `RoadManager.cpp:4961` | builds `OpenDriveMap.cpp:656` | builds `SignalParser.cpp:62` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@width` | builds `signals.rs:306` | builds `RoadManager.cpp:4958` | builds `OpenDriveMap.cpp:655` | builds `SignalParser.cpp:60` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@roll` | builds `signals.rs:367` | builds `RoadManager.cpp:4963` | builds `OpenDriveMap.cpp:658` | builds `SignalParser.cpp:64` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@pitch` | builds `signals.rs:366` | builds `RoadManager.cpp:4962` | builds `OpenDriveMap.cpp:657` | builds `SignalParser.cpp:63` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@invalidated` | stores `signals.rs:294` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@temporary` | stores `signals.rs:295` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@s` | builds `signals.rs:263` | builds `RoadManager.cpp:4849` | builds `OpenDriveMap.cpp:649` | builds `SignalParser.cpp:47` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@t` | builds `signals.rs:263` | builds `RoadManager.cpp:4850` | builds `OpenDriveMap.cpp:650` | builds `SignalParser.cpp:48` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; `@zOffset` | builds `signals.rs:541` | builds `RoadManager.cpp:4896` | builds `OpenDriveMap.cpp:652` | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;validity&gt; | builds `signals.rs:281` | stores `RoadManager.cpp:5028` | stores `OpenDriveMap.cpp:38` | builds `SignalParser.cpp:25` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; | builds `signals.rs:422` | - | - | stores `SignalParser.cpp:109` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; `@id` | builds `signals.rs:423` | - | - | stores `SignalParser.cpp:110` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;dependency&gt; `@type` | stores `signals.rs:426` | - | - | stores `SignalParser.cpp:111` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; | builds `signals.rs:399` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@elementType` | stores `signals.rs:401` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@elementId` | builds `signals.rs:400` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;reference&gt; `@type` | stores `signals.rs:414` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; | builds `signals.rs:274` | - | - | stores `SignalParser.cpp:115` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@x` | builds `signals.rs:711` | - | - | stores `SignalParser.cpp:116` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@y` | builds `signals.rs:712` | - | - | stores `SignalParser.cpp:117` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@z` | builds `signals.rs:639` | - | - | stores `SignalParser.cpp:118` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@hdg` | builds `signals.rs:717` | - | - | stores `SignalParser.cpp:119` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@pitch` | builds `signals.rs:366` | - | - | stores `SignalParser.cpp:120` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionInertial&gt; `@roll` | builds `signals.rs:367` | - | - | stores `SignalParser.cpp:121` |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; | builds `signals.rs:271` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@roadId` | builds `signals.rs:272` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@s` | builds `signals.rs:530` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@t` | builds `signals.rs:530` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@zOffset` | builds `signals.rs:541` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@hOffset` | builds `signals.rs:365` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@pitch` | builds `signals.rs:366` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;positionRoad&gt; `@roll` | builds `signals.rs:367` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;semantics&gt; | stores `signals.rs:572` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; | stores `signals.rs:348` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;validity&gt; | stores `signals.rs:348` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;dependency&gt; | stores `signals.rs:422` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;reference&gt; | stores `signals.rs:399` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; | stores `signals.rs:351` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@id` | stores `signals.rs:356` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@name` | stores `signals.rs:285` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@dynamic` | stores `signals.rs:361` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@orientation` | stores `signals.rs:351` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@country` | stores `signals.rs:287` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@countryRevision` | stores `signals.rs:358` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@type` | stores `signals.rs:289` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@subtype` | stores `signals.rs:290` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@value` | stores `signals.rs:291` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@text` | stores `signals.rs:293` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@unit` | stores `signals.rs:292` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@height` | stores `signals.rs:307` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@length` | stores `signals.rs:368` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@hOffset` | stores `signals.rs:365` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@width` | stores `signals.rs:306` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@roll` | stores `signals.rs:367` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@pitch` | stores `signals.rs:366` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@invalidated` | stores `signals.rs:362` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@temporary` | stores `signals.rs:363` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@v` | stores `signals.rs:638` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; `@z` | stores `signals.rs:639` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;validity&gt; | stores `signals.rs:351` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;dependency&gt; | stores `signals.rs:422` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;reference&gt; | stores `signals.rs:399` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;positionInertial&gt; | stores `signals.rs:274` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;positionRoad&gt; | stores `signals.rs:322` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;staticBoard&gt; &lt;sign&gt; &lt;semantics&gt; | stores `signals.rs:572` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; | stores `signals.rs:348` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayType` | stores `signals.rs:682` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayHeight` | stores `signals.rs:685` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@displayWidth` | stores `signals.rs:684` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@v` | stores `signals.rs:638` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; `@z` | stores `signals.rs:639` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;validity&gt; | stores `signals.rs:348` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;dependency&gt; | stores `signals.rs:422` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;reference&gt; | stores `signals.rs:399` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; | stores `signals.rs:495` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@index` | stores `signals.rs:690` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@width` | stores `signals.rs:672` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@height` | stores `signals.rs:673` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@v` | stores `signals.rs:638` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signal&gt; &lt;vmsBoard&gt; &lt;displayArea&gt; `@z` | stores `signals.rs:639` | - | - | - |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; | builds `signals.rs:92` | - | - | builds `SignalParser.cpp:128` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@s` | builds `signals.rs:213` | - | - | builds `SignalParser.cpp:129` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@t` | builds `signals.rs:213` | - | - | builds `SignalParser.cpp:130` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@id` | builds `signals.rs:93` | - | - | builds `SignalParser.cpp:131` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; `@orientation` | builds `signals.rs:219` | - | - | builds `SignalParser.cpp:133` |
| &lt;road&gt; &lt;signals&gt; &lt;signalReference&gt; &lt;validity&gt; | builds `signals.rs:220` | - | - | builds `SignalParser.cpp:147` |
| &lt;road&gt; &lt;surface&gt; | builds `mod.rs:707` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; | builds `mod.rs:710` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@file` | builds `mod.rs:1020` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sStart` | builds `mod.rs:2237` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sEnd` | builds `mod.rs:2238` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@orientation` | builds `mod.rs:2247` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@mode` | builds `mod.rs:1378` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@purpose` | builds `mod.rs:1028` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@sOffset` | builds `mod.rs:2245` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@tOffset` | builds `mod.rs:1345` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@xOffset` | builds `mod.rs:2293` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@yOffset` | builds `mod.rs:2294` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@zOffset` | builds `mod.rs:2314` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@zScale` | builds `mod.rs:2315` | - | - | - |
| &lt;road&gt; &lt;surface&gt; &lt;CRG&gt; `@hOffset` | builds `mod.rs:2260` | - | - | - |
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
| &lt;controller&gt; | builds `signals.rs:154` | stores `RoadManager.cpp:5472` | - | builds `ControllerParser.cpp:23` |
| &lt;controller&gt; `@id` | builds `signals.rs:179` | stores `RoadManager.cpp:5474` | - | builds `ControllerParser.cpp:27` |
| &lt;controller&gt; `@name` | stores `signals.rs:175` | stores `RoadManager.cpp:5475` | - | stores `ControllerParser.cpp:28` |
| &lt;controller&gt; `@sequence` | stores `signals.rs:151` | stores `RoadManager.cpp:5476` | - | stores `ControllerParser.cpp:29` |
| &lt;controller&gt; &lt;control&gt; | builds `signals.rs:157` | stores `RoadManager.cpp:5479` | - | builds `ControllerParser.cpp:38` |
| &lt;controller&gt; &lt;control&gt; `@signalId` | builds `signals.rs:158` | stores `RoadManager.cpp:5483` | - | builds `ControllerParser.cpp:39` |
| &lt;controller&gt; &lt;control&gt; `@type` | stores `signals.rs:166` | stores `RoadManager.cpp:5484` | - | - |

### &lt;junction&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;junction&gt; | builds `mod.rs:759` | builds `RoadManager.cpp:5491` | builds `OpenDriveMap.cpp:92` | builds `JunctionParser.cpp:44` |
| &lt;junction&gt; `@name` | stores `virtual_junctions.rs:171` | stores `RoadManager.cpp:5493` | stores `OpenDriveMap.cpp:98` | stores `JunctionParser.cpp:47` |
| &lt;junction&gt; `@id` | builds `mod.rs:765` | builds `RoadManager.cpp:5495` | builds `OpenDriveMap.cpp:95` | builds `JunctionParser.cpp:46` |
| &lt;junction&gt; `@type` | builds `links.rs:505` | builds `RoadManager.cpp:5494` | - | - |
| &lt;junction&gt; `@mainRoad` | builds `virtual_junctions.rs:34` | - | - | - |
| &lt;junction&gt; `@sStart` | builds `virtual_junctions.rs:37` | - | - | - |
| &lt;junction&gt; `@sEnd` | builds `virtual_junctions.rs:37` | - | - | - |
| &lt;junction&gt; `@orientation` | builds `virtual_junctions.rs:173` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; | builds `links.rs:493` | builds `RoadManager.cpp:5510` | builds `OpenDriveMap.cpp:101` | builds `JunctionParser.cpp:50` |
| &lt;junction&gt; &lt;connection&gt; `@id` | builds `links.rs:490` | stores `RoadManager.cpp:5515` | stores `OpenDriveMap.cpp:110` | stores `JunctionParser.cpp:53` |
| &lt;junction&gt; &lt;connection&gt; `@incomingRoad` | builds `links.rs:496` | builds `RoadManager.cpp:5517` | builds `OpenDriveMap.cpp:114` | builds `JunctionParser.cpp:54` |
| &lt;junction&gt; &lt;connection&gt; `@contactPoint` | builds `links.rs:427` | builds `RoadManager.cpp:5548` | builds `OpenDriveMap.cpp:103` | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; | builds `links.rs:530` | builds `RoadManager.cpp:5564` | builds `OpenDriveMap.cpp:119` | builds `JunctionParser.cpp:58` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@from` | builds `links.rs:533` | builds `RoadManager.cpp:5567` | builds `OpenDriveMap.cpp:121` | builds `JunctionParser.cpp:61` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@to` | builds `links.rs:533` | builds `RoadManager.cpp:5568` | builds `OpenDriveMap.cpp:121` | builds `JunctionParser.cpp:62` |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@overlapZone` | stores `links.rs:530` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@fromLayer` | builds `links.rs:537` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;laneLink&gt; `@toLayer` | builds `links.rs:538` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@type` | builds `links.rs:505` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@connectingRoad` | builds `links.rs:510` | builds `RoadManager.cpp:5527` | builds `OpenDriveMap.cpp:115` | builds `JunctionParser.cpp:55` |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; | builds `virtual_junctions.rs:137` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementType` | builds `virtual_junctions.rs:183` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementId` | builds `virtual_junctions.rs:80` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementS` | builds `virtual_junctions.rs:71` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;predecessor&gt; `@elementDir` | builds `virtual_junctions.rs:189` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; &lt;successor&gt; | builds `virtual_junctions.rs:138` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; | builds `junction_areas.rs:125` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@crossingRoad` | builds `junction_areas.rs:126` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@id` | builds `junction_areas.rs:124` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@roadAtEnd` | builds `junction_areas.rs:168` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; `@roadAtStart` | builds `junction_areas.rs:167` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; | builds `junction_areas.rs:167` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@s` | builds `junction_areas.rs:133` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@from` | builds `junction_areas.rs:156` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;startLaneLink&gt; `@to` | builds `junction_areas.rs:160` | - | - | - |
| &lt;junction&gt; &lt;crossPath&gt; &lt;endLaneLink&gt; | builds `junction_areas.rs:168` | - | - | - |
| &lt;junction&gt; &lt;priority&gt; | stores `mod.rs:921` | - | stores `OpenDriveMap.cpp:131` | - |
| &lt;junction&gt; &lt;priority&gt; `@high` | stores `mod.rs:922` | - | stores `OpenDriveMap.cpp:133` | - |
| &lt;junction&gt; &lt;priority&gt; `@low` | stores `mod.rs:922` | - | stores `OpenDriveMap.cpp:133` | - |
| &lt;junction&gt; &lt;controller&gt; | builds `signals.rs:188` | stores `RoadManager.cpp:5575` | stores `OpenDriveMap.cpp:137` | builds `JunctionParser.cpp:71` |
| &lt;junction&gt; &lt;controller&gt; `@id` | builds `signals.rs:189` | stores `RoadManager.cpp:5579` | stores `OpenDriveMap.cpp:139` | builds `JunctionParser.cpp:72` |
| &lt;junction&gt; &lt;controller&gt; `@type` | stores `signals.rs:196` | stores `RoadManager.cpp:5580` | stores `OpenDriveMap.cpp:142` | - |
| &lt;junction&gt; &lt;controller&gt; `@sequence` | stores `signals.rs:151` | stores `RoadManager.cpp:5581` | stores `OpenDriveMap.cpp:143` | - |
| &lt;junction&gt; &lt;surface&gt; | builds `mod.rs:707` | - | - | - |
| &lt;junction&gt; &lt;planView&gt; | builds `junction_areas.rs:200` | - | - | - |
| &lt;junction&gt; &lt;objects&gt; | builds `mod.rs:760` | - | - | - |
| &lt;junction&gt; &lt;connection&gt; `@linkedRoad` | builds `links.rs:508` | builds `RoadManager.cpp:5523` | - | - |
| &lt;junction&gt; &lt;roadSection&gt; | stores `mod.rs:759` | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@id` | stores `mod.rs:765` | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@roadId` | stores `mod.rs:759` | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@sStart` | stores `mod.rs:2237` | - | - | - |
| &lt;junction&gt; &lt;roadSection&gt; `@sEnd` | stores `mod.rs:2238` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; | builds `junction_areas.rs:29` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; | builds `junction_areas.rs:256` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@roadId` | builds `junction_areas.rs:257` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@type` | builds `junction_areas.rs:258` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@boundaryLane` | builds `junction_areas.rs:360` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@sStart` | builds `junction_areas.rs:361` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@sEnd` | builds `junction_areas.rs:362` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@contactPoint` | builds `junction_areas.rs:388` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@jointLaneStart` | builds `junction_areas.rs:408` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@jointLaneEnd` | builds `junction_areas.rs:408` | - | - | - |
| &lt;junction&gt; &lt;boundary&gt; &lt;segment&gt; `@transitionLength` | builds `junction_areas.rs:455` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; | builds `junction_areas.rs:30` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; `@sStart` | builds `junction_areas.rs:223` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; `@gridSpacing` | builds `junction_areas.rs:208` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; | builds `junction_areas.rs:227` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@left` | builds `junction_areas.rs:231` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@center` | builds `junction_areas.rs:230` | - | - | - |
| &lt;junction&gt; &lt;elevationGrid&gt; &lt;elevation&gt; `@right` | builds `junction_areas.rs:232` | - | - | - |

### &lt;junctionGroup&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;junctionGroup&gt; | stores `junction_areas.rs:62` | - | - | - |
| &lt;junctionGroup&gt; `@name` | stores `junction_areas.rs:98` | - | - | - |
| &lt;junctionGroup&gt; `@id` | stores `junction_areas.rs:64` | - | - | - |
| &lt;junctionGroup&gt; `@type` | stores `junction_areas.rs:65` | - | - | - |
| &lt;junctionGroup&gt; &lt;junctionReference&gt; | stores `junction_areas.rs:81` | - | - | - |
| &lt;junctionGroup&gt; &lt;junctionReference&gt; `@junction` | stores `junction_areas.rs:83` | - | - | - |

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
| &lt;vmsGroup&gt; | stores | - | - | - |
| &lt;vmsGroup&gt; `@id` | stores `mod.rs:737` | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; | stores | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@signalId` | stores | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@vmsIndex` | stores | - | - | - |
| &lt;vmsGroup&gt; &lt;vmsBoardReference&gt; `@groupIndex` | stores | - | - | - |

### &lt;dataQuality&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;dataQuality&gt; | stores | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; | stores | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@xyAbsolute` | stores | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@zAbsolute` | stores | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@xyRelative` | stores | - | - | - |
| &lt;dataQuality&gt; &lt;error&gt; `@zRelative` | stores | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; | stores | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@date` | stores `mod.rs:1718` | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@source` | stores | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@sourceComment` | stores | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@postProcessing` | stores | - | - | - |
| &lt;dataQuality&gt; &lt;rawData&gt; `@postProcessingComment` | stores | - | - | - |

### &lt;include&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;include&gt; | builds | - | - | - |
| &lt;include&gt; `@file` | builds `mod.rs:1020` | - | - | - |

### &lt;userData&gt;

| Element | This crate | esmini | libOpenDRIVE | CARLA |
| --- | --- | --- | --- | --- |
| &lt;userData&gt; | stores `mod.rs:3750` | builds `RoadManager.cpp:5397` | - | - |
| &lt;userData&gt; `@code` | stores `mod.rs:3754` | builds `RoadManager.cpp:5399` | - | - |
| &lt;userData&gt; `@value` | stores `mod.rs:3755` | builds `RoadManager.cpp:5402` | - | - |
