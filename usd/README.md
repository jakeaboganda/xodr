# OpenUSD export

`xodr_usd` turns an OpenDRIVE map into an OpenUSD stage (`.usda`). Omniverse,
Blender, Houdini and `usdview` can open it. It's pure Rust, so you don't need
OpenUSD installed.

```sh
cargo run --release -p xodr-usd -- tests/data/town07.xodr
cargo run --release -p xodr-usd -- tests/data/town07.xodr /tmp/town07.usda
```

With no output path, the stage goes next to the map.

## View a stage in the browser

```sh
sh usd/run.sh tests/data/town07.xodr tests/data/objects.xodr
```

The script exports each map to `usd/web/` and opens the first one at
<http://localhost:8001>. Set `PORT` to use another port.

- To open another stage, click `open .usda` or drop the file on the page.
- To see a mesh's path and `xodr:` attributes, hover over it.
- To hide roads, road marks or objects, clear their boxes in the toolbar.

The page reads stages with three.js's `USDLoader`, not with code from this
repo. `USDLoader` draws each mesh in one colour. A road mark with lines of
different colours gets its first line's colour.

## What's in the stage

The stage is Z-up, in metres, in the map's frame.

| Prim | Contents |
| --- | --- |
| `/Map/Roads/road_<n>/lane_<n>` | One mesh per lane, from `RoadNetwork::surface_mesh`. |
| `/Map/RoadMarks/mark_<n>` | One mesh per painted road mark, 5 mm above its lane. |
| `/Map/Objects/object_<n>` | One double-sided mesh per object, from `RoadNetwork::object_mesh`. |

`<n>` is the crate's id. Each prim keeps its OpenDRIVE names in `xodr:`
attributes:

- Road: `xodr:roadId`, and `xodr:junction` if the road is in a junction.
- Lane: `xodr:section`, `xodr:laneId`, `xodr:laneType`.
- Road mark: `xodr:type`, `xodr:weight`, `xodr:color`, `xodr:laneChange`.
- Object: `xodr:type`, `xodr:subtype`, `xodr:name`, `xodr:objectId`,
  `xodr:roadId`.

Each mesh has a `displayColor` and no material. Driving lanes are dark grey
and sidewalks and curbs light grey. Other lanes are mid grey. Paint uses the
colour its line names, or white.

The export leaves out signals, object markings, junction areas and OpenCRG
surfaces.
