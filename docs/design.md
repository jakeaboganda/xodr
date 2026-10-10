# Design notes

How the baked network is laid out, and what the crate promises about it.

## Coordinate frame

Baked geometry is OpenDRIVE's own frame: right-handed, Z-up, metres, with the
reference line in the X-Y plane and elevation along +Z. A point imports
unchanged, so a coordinate you read out of the `.xodr` is the coordinate you
get back. An OpenDRIVE left turn curves toward +Y, and positive lane offset
`t` is to the left of the heading.

The `<geoReference>` and `<offset>` are kept on the network, not applied.
Reprojecting would pull in PROJ, and exporters disagree on the offset's
sign, so a consumer that needs world coordinates applies them itself.

Travel direction follows each road's `rule`. Under right-hand traffic, the
default, negative-id lanes run with `+s` and positive-id lanes against it.
Under `rule="LHT"` it is the other way round.

## Coordinate types

A position is a `Point` and a direction is a `Vector`. They are separate types
with the arithmetic that relates them, so `point - point` is a `Vector`,
`point + vector` is a `Point`, and adding two positions does not compile.
Neither does `nearest_lane(sample.up)`, which passes a direction where a
position belongs. With one three-float type for both, it would compile.

Both are `#[repr(C)]` structs of three public `f32` fields, so handing one to
another math library is one call:

```rust
let v = glam::Vec3::from(point.to_array());
```

The required dependencies are `roxmltree`, `thiserror` and `opencrg`, which
has none of its own. Nothing here constrains which math or engine crate you
use, or its version.

## Meshes

`surface_mesh()` returns plain position, normal and index buffers with no
engine types in them, and a `LaneSpan` per lane saying which slice of those
buffers it owns. Buffers plus spans are enough to pick the lane under a
cursor, or give one lane its own material without rebuilding the mesh. The viewer
uses the same spans to resolve a raycast hit to a lane.

A road's temporary lane layer shares the permanent layer's surface, so
`surface_mesh()` has the permanent lanes only. `temporary_mesh()` has the
temporary ones, to draw over it.

`object_mesh()` does the same for objects: every box, cylinder, outline and
sweep turned into triangles in one mesh of outward-facing faces, with an `ObjectSpan`
per object.

## Serialization

The optional `serde` feature serializes the network and its mesh.
The `viewer/` crate uses it to bake maps to the JSON the viewer reads.
You can also use it to cache an import. A `RoadNetwork` serializes everything
it read, and rebuilds its lookup indexes when deserialized, so the result
behaves like a freshly imported map. A road serializes as the records the
file gives, and rebakes its spirals and cubic curves on the way in.

```toml
xodr = { version = "0.5", features = ["serde"] }
```

## Untrusted input

A road the importer cannot interpret is skipped, not fatal. Losing a city map
to one junk road is the worse failure. `load_str` still errors if the document
yields no lanes at all.

A road or geometry longer than `MAX_LENGTH`, 100 km, is skipped too. The
crate samples a road along its whole length, so a length like 1e13 from a
broken file would need more memory than any machine has.

`Provenance::warnings` says what the load did with a bad file. A `Warning` is
data, not a log line: one enum variant per kind, naming the road, lane section
or lane it happened at. A test can assert on it, and a caller can filter the
kinds it doesn't care about. The crate prints nothing and depends on no
logging crate. A file that uses nothing the crate ignores and breaks no rule
allocates no warnings.

The crate warns where it drops something, or reads a file that breaks a rule
of the spec, such as a road with no geometry, a lane with no `<width>` or
`<border>`, or a junction boundary that isn't closed. The `Warning` enum
lists every kind. It also says what it ignores. A `Warning::Ignored` names
each feature of the file the crate doesn't read, such as `<userData>`, once
for each road, junction, or the rest of the document, with a count, so a map
full of them raises a few. `Warning::is_ignored` tells these from the
warnings for faults. [support.md](support.md) lists what is ignored.

The parser rejects non-finite attribute values as it reads them. Rust's float
parser accepts `NaN` and turns `1e400` into infinity, and one such value
poisons every point derived from it.

## Performance

Lane and surface lookups go through a ground-plane index, so their cost tracks
local road density rather than map size. On CARLA's Town07 (234 roads, 947
lanes, 673 of them driving):

| | per call |
| --- | --- |
| `nearest_lane` / `sample_near` | ~0.4 us |
| `road_position` | ~6 us |
| `route` (across the map) | ~29 us |
| `MeshSampler::height_at` | ~0.2 us |

Import is ~20 ms for that map. `RoadSurface::sample` takes ~0.5 us on
`tests/data/crg.xodr`.

`cargo bench` measures the import and every lookup in the table except
`road_position`. `tests/budgets.rs` guards `nearest_lane` and
`MeshSampler::height_at` in CI by racing each against the scan it replaced.
Racing needs no fixed per-machine threshold, unlike timing a call directly.
The same test fails if any route across Town07 takes longer than one 64 Hz
tick.
