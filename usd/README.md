# OpenUSD export

`xodr_usd` writes an OpenDRIVE map as an OpenUSD stage, in the `.usda`
text format, for Omniverse, Blender, Houdini, `usdview` or any other tool
that reads USD. It is pure Rust and needs no OpenUSD install.

```sh
cargo run --release -p xodr-usd -- tests/data/town07.xodr
cargo run --release -p xodr-usd -- tests/data/town07.xodr /tmp/town07.usda
```

Without an output path the stage goes beside the map, as `town07.usda`.

## What the stage holds

The stage is Z-up, in metres, in the map's own frame. Its default prim is
`/Map`:

| Prim | What it is |
| --- | --- |
| `/Map/Roads/road_<n>` | One `Scope` per road. |
| `/Map/Roads/road_<n>/lane_<n>` | One `Mesh` per lane: its slice of `RoadNetwork::surface_mesh`, with its normals. |
| `/Map/RoadMarks/mark_<n>` | One `Mesh` per road mark that paints anything: a quad per piece of each line, 5 mm above the lane so the lane doesn't hide it. |
| `/Map/Objects/object_<n>` | One double-sided `Mesh` per object with a volume: its slice of `RoadNetwork::object_mesh`. |

The `<n>` are the crate's ids, so every name is a valid USD name. What
OpenDRIVE calls each prim is in its `xodr:` attributes:

- A road: `xodr:roadId`, and `xodr:junction` on a road inside a junction.
- A lane: `xodr:section`, `xodr:laneId` and `xodr:laneType`.
- A road mark: `xodr:type`, `xodr:weight`, `xodr:color` and `xodr:laneChange`.
- An object: `xodr:type`, `xodr:subtype`, `xodr:name`, `xodr:objectId` and
  `xodr:roadId`.

Each mesh has a `displayColor`. Lanes are dark grey if traffic drives on
them, light grey for a sidewalk or curb, and mid grey otherwise. Paint takes
the colour its line names, and white for `standard` or a name OpenDRIVE
doesn't define. There are no materials.

Signals, object markings, junction areas and OpenCRG surfaces are not
exported.
