# xodr

[![CI](https://github.com/jakeaboganda/xodr/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jakeaboganda/xodr/actions/workflows/ci.yml?query=branch%3Amain)
[![MSRV 1.85](https://img.shields.io/badge/MSRV-1.85-blue)](https://github.com/jakeaboganda/xodr/blob/main/Cargo.toml)

A pure-Rust OpenDRIVE (`.xodr`) importer. It turns a map into lanes, a lane
graph, and a mesh of the road surface. It also reads the OpenCRG (`.crg`)
files a map uses.

No C++, no bindings, no `unsafe`. Up to 0.4.1 the crate was called
`libopendrive`.

```rust
use xodr::{load_file, Point};

let net = load_file("maps/town07.xodr")?;

// Where is the road under this point, and which way does it lean?
let sample = net.sample_near(Point::new(12.0, -30.0, 0.0)).expect("on the map");
println!("{:?} banked {} rad", sample.point, sample.bank);

// Where is that on its road, in OpenDRIVE's own s and t?
let at = net.road_position(sample.point).expect("on a road");
println!("road {} s {} t {}", net.road(at.road).unwrap().od_id(), at.s, at.t);

// Drive somewhere.
let waypoints = net.route(sample.point, Point::new(280.0, 95.0, 0.0));

// Hand the surface to a collider or a renderer.
let mesh = net.surface_mesh();
mesh.validate()?;
```

## What it builds

Test maps in the [viewer](viewer/README.md). Click a picture for details.

| | |
| --- | --- |
| [![Temporary lanes, tinted amber, over the permanent lanes of a road](docs/images/lane-layers.png)](docs/support.md#lane-layers) | [![Two posts on a banked road: one plumb, one leaning with the road](docs/images/object-poses.png)](docs/support.md#objects) |
| **Lane layers.** Temporary lanes, amber, over the permanent ones. | **Object poses.** Plumb by default. The left post has `perpToRoad="true"`. |
| [![A traffic light pole with an arm over the road, drawn as tapered tubes](docs/images/skeleton.png)](docs/support.md#objects) | [![Smooth traffic islands, a circle and a wave drawn from curves](docs/images/curve-local.png)](docs/support.md#objects) |
| **Skeletons.** A pole and arm from `<skeleton>` polylines. | **Curved outlines.** Islands from `<curveLocal>` curves. |
| [![A grass island with cobble and concrete areas inside it and one hole](docs/images/inner-outlines.png)](docs/support.md#objects) | [![A parking bay with a solid and a dashed line, and a painted arrow](docs/images/outline-markings.png)](docs/support.md#objects) |
| **Inner outlines.** Cobble and concrete areas, and a hole. | **Markings.** A parking bay's lines and a painted area. |
| [![Rows of posts and walls along a curved road, two of them straight across it](docs/images/repeats.png)](docs/support.md#objects) | [![The height an object's CRG adds, as a heat map over a turned square](docs/images/object-surface.png)](docs/support.md#opencrg-surfaces) |
| **Repeats.** Posts and walls. Two are detached from the road. | **Object surfaces.** The height an object's CRG adds. |

## What it reads

Every element and attribute of the OpenDRIVE 1.9 schema. Of its 671 rows, 420
shape the network and 251 are kept as data. It also reads the crossfall of
OpenDRIVE 1.4. [docs/support.md](docs/support.md) says what it does with each
element.

From that it builds:

- roads you can query in `(road, s, t)` or `(lane, s, offset)`, both ways;
- lanes with widths, heights and layers, and a lane graph through default,
  direct and virtual junctions;
- speed limits from speed records, signs and default regulations;
- objects, signals and road marks, placed on the road;
- the road surface under a wheel, from elevation, shape and OpenCRG.

It loads any OpenDRIVE version. `Provenance::warnings` lists what it dropped,
what it ignores, and features newer than the header's version.
`Warning::is_ignored` tells ignored features from faults.

## Viewer

The crate draws nothing. [`viewer/`](viewer/README.md) is a three.js page that
runs the crate as WebAssembly. Open a `.xodr` and hover anything to see what
the crate knows about it.

```sh
sh viewer/run.sh   # serves http://localhost:8000
```

![A traffic island's details in the viewer](viewer/objects.png)

## OpenUSD

[`usd/`](usd/README.md) exports a map as an OpenUSD stage. It's a separate
tool, not part of the crate.

```sh
cargo run --release -p xodr-usd -- tests/data/town07.xodr
```

## How it fits together

![A .xodr and its CRG files go into load_file_with_provenance, which builds a
RoadNetwork and its Provenance. Callers query the network, mesh it, and
sample the road surface. The viewer and xodr-usd use the meshes.](docs/data-flow.svg)

## More

- [docs/design.md](docs/design.md): coordinates, mesh buffers, `serde`, bad
  input, timings.
- [docs/support.md](docs/support.md): what the crate does with each element.
- [docs/comparison.md](docs/comparison.md): each 1.9 element in this crate,
  esmini, libOpenDRIVE and CARLA.
- [CHANGELOG.md](CHANGELOG.md)

The published crate has no tests or maps. Run them from a git checkout.

MSRV is 1.85.

## License

MIT or Apache-2.0, at your option.
