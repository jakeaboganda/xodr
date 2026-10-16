# Design notes

How the baked network is laid out, and what the crate promises about it.

![A .xodr and its CRG files go into load_file_with_provenance, which builds a
RoadNetwork and its Provenance. Callers query the network, mesh it, and
sample the road surface. The viewer and xodr-usd use the meshes.](data-flow.svg)

`docs/data-flow.lini` is the diagram's source. Compile it with
`lini docs/data-flow.lini -o docs/data-flow.svg`.

## Coordinate frame

OpenDRIVE's own: right-handed, Z up, metres. A coordinate in the `.xodr` is
the coordinate you get back. A left turn curves toward +Y, and `t` is
positive to the left.

The `<geoReference>` and `<offset>` are kept, not applied. Applying them
would need PROJ, and exporters disagree on the offset's sign.

Traffic follows each road's `rule`. With right-hand traffic, the default,
negative lanes run along `+s`. With `rule="LHT"` it's the other way round.

## Coordinate types

A position is a `Point` and a direction is a `Vector`. `point - point` is a
`Vector`, `point + vector` is a `Point`, and adding two points doesn't
compile. Neither does passing a `Vector` where a position belongs.

Both are `#[repr(C)]` with three `f32` fields, so converting is one call:

```rust
let v = glam::Vec3::from(point.to_array());
```

The only dependencies are `roxmltree`, `thiserror` and `opencrg`. No math or
engine crate is imposed.

## Meshes

`surface_mesh()` returns position, normal and index buffers, and a
`LaneSpan` per lane naming its slice. That is enough to pick a lane or give
it its own material. `temporary_mesh()` has the temporary lane layer, to
draw over the surface. `object_mesh()` does the same for objects, with an
`ObjectSpan` each.

## Serialization

The `serde` feature serializes the network and its meshes. Use it to cache
an import. A deserialized `RoadNetwork` rebuilds its indexes and behaves
like a fresh import.

```toml
xodr = { version = "0.7", features = ["serde"] }
```

## Untrusted input

- A road the importer can't read is skipped, not fatal. `load_str` errors
  only if nothing yields a lane.
- Non-finite numbers, such as `NaN` or `1e400`, are rejected as they're
  read.
- Caps bound memory on hostile files:

  | Cap | Limit |
  | --- | --- |
  | `MAX_LENGTH` | 100 km per road or geometry |
  | `MAX_BAKED_LENGTH` | 2,000 km per map |
  | objects | 100,000 per map, 2,000,000 pieces |
  | XML nesting | 256 deep |
  | `<include>` | 8 deep, `MAX_INCLUDED_BYTES` (256 MiB) in all |

`Provenance::warnings` says what the load did. A `Warning` is data: one
variant per kind, naming the road, section or lane. The crate prints
nothing and has no logging dependency. It warns where it drops something,
where a file breaks a rule, where it ignores a feature, and where a file uses
something newer than its header's version. `Warning::is_ignored` picks out
the ignored features. A clean file raises no warnings.

## Performance

Lookups go through a ground-plane index. Their cost depends on how dense
the roads are nearby, not on the map's size. On CARLA's Town07 (234 roads, 947 lanes):

| | per call |
| --- | --- |
| `nearest_lane` / `sample_near` | ~0.4 us |
| `road_position` | ~6 us |
| `route` across the map | ~29 us |
| `MeshSampler::height_at` | ~0.2 us |

Import takes ~20 ms. `RoadSurface::sample` takes ~0.5 us on
`tests/data/crg.xodr`.

`cargo bench` covers the import and the table except `road_position`.
`tests/budgets.rs` guards `nearest_lane` and `height_at` in CI by racing each
against a plain scan, and fails if a route across Town07 takes longer than a
64 Hz tick.
