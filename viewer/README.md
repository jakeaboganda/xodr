# OpenDRIVE viewer

A three.js page that draws what `xodr` builds from a map. Hover a lane,
object, signal or junction to see what the crate knows about it.

The `xodr-viewer` crate here turns a `.xodr` into JSON, a scene, and
`web/index.html` draws it. The page runs that crate as WebAssembly, so it
opens a `.xodr` straight from disk.

![Hovering a lane in Town07, with its successor in green and its predecessor in orange](town07.png)

## Run it

```sh
sh viewer/run.sh        # or: sh viewer/run.sh 8080
```

This builds the page, serves it on <http://localhost:8000> and opens it. The
first run installs the `wasm32-unknown-unknown` target and `wasm-bindgen`.

Click `open .xodr` and pick a map. Pick its `.crg` files, the files they
include, and its included files in the same dialog. The sidebar names any it
is missing.

To host the page elsewhere, run `sh viewer/build.sh` and serve `viewer/web`
over HTTP. Opening `index.html` as a file won't work.

The page keeps opened files in IndexedDB, so a reload shows the same map.

## Baked maps

`viewer_export` bakes scenes ahead of time into `viewer/web/`, and the `map`
list offers them:

```sh
cargo run --release -p xodr-viewer -- tests/data/*.xodr
cargo run --release -p xodr-viewer -- tests/data/objects.xodr /tmp/objects.json
```

- It reads CRG and included files from beside the `.xodr` or below it, and
  nothing outside that folder.
- If a map fails, it reports it, bakes the rest, and exits with an error.
- `sh viewer/build.sh` runs `viewer_export --refresh`, which bakes again any
  scene older than its `.xodr` or than `viewer_export`. Run it from the
  repository root. Elsewhere it finds no `viewer/web` and fails.
- A scene is written whole or not at all, so a failed write leaves the old
  scene, or none.

A link can name a scene and a camera, as
`?scene=town07.json&view=x,y,z,tx,ty,tz`. Press `v` to put the current view
in the address.

`tests/data/objects.xodr` has one of everything the viewer draws.

## Controls

| Input | Action |
| --- | --- |
| drag / right-drag / scroll | orbit / pan / zoom |
| `w` | cycle solid, wireframe, both |
| `n` `o` `s` `m` `c` | toggle normals, objects, signals, road marks, CRG heat map |
| click a lane | drop a marker |
| `f` / `b` | step the marker 5 m along the lanes, or back |
| `Esc` | clear the marker |
| `v` | put the view in the address |
| click a sidebar entry | select it and frame it |
| filter box | filter by id, type, name or warning text |

The checkboxes also toggle centerlines, lane boundaries, tunnels, bridges
and junctions. `flag bad tris` paints inverted or degenerate triangles red.

## Sidebar

It lists each road's lanes, objects and signals, and its tunnels and
bridges. Warnings are at the top in amber. Click one to frame the road it
names. Hover the map list for the header: revision, vendor, licence and
default regulations.

## What it draws

- **Lanes.** Coloured by type. An arrow shows the way traffic drives.
  Successors are green, predecessors orange. Temporary lanes are amber.
  Hover for road and lane ids, `s` and `t`, width, slope, speed limit,
  rules, links and junction details.
- **Objects.** Drawn from `object_mesh()`, coloured by type or fill.
  Markings, painted areas and borders are drawn on the outline. Parking
  spaces are coloured by access.
- **Signals.** A board of the signal's size, white for static signs, dark
  for dynamic ones. Hover to light its lanes and draw a dashed line to where
  it applies. The readout shows its meaning, boards, controllers and links.
- **Road marks.** Painted lines with their dashes and sway. Kerbs and
  `none` draw nothing.
- **Tunnels and bridges.** Tint the lanes they cover, purple and cyan.
- **Junctions.** The ground of a junction with a boundary or elevation grid,
  in grey, and its boundary in amber. Virtual junctions are cyan and pink
  tubes.
- **Railways.** Platforms as grey strips, switches as red posts.
- **OpenCRG.** A heat map of the CRG height over the road, white at the
  median, blue below, red above. Grey means friction only. Hover for the
  height and friction.

![Hovering a traffic island](objects.png)

![Hovering a traffic light that also applies on another road](signals.png)

`examples/crg_data.sh` downloads five measured CRG files and bakes a scene
for each. Try `belgian_block`:

![Scanned cobbles from ASAM's belgian_block.crg](crg-cobbles.png)

## Coordinates

OpenDRIVE's frame: right-handed, Z up, metres. A coordinate on screen is the
coordinate in the file. `s` and `t` are road coordinates, `t` positive to
the left. A lane's offset is from its center toward `+t`.

## How picking works

`surface_mesh()` records a `LaneSpan` per lane, the triangles it owns. A
raycast hit is binary-searched against the spans to find the lane. Lane
boundaries come from the same mesh: even span vertices are the left edge,
odd ones the right.
