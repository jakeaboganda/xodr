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

## Use a catalogue

A catalogue is a layer that sets how each signal type looks.
[`catalogues/sample.usda`](catalogues/sample.usda) covers the types in the
test maps. [SCHEMA.md](SCHEMA.md#catalogues) explains how to write one.

Full USD tools such as `usdview` can stack a catalogue over a stage. Other
readers need one flat file. To make one, install OpenUSD's Python module and
run `flatten.py`:

```sh
pip install usd-core
python usd/flatten.py town07.flat.usda town07.usda usd/catalogues/sample.usda
```

## Check stages with OpenUSD

`check.py` runs OpenUSD's validators and this repo's own checks on stages,
alone, under a catalogue, and flattened. CI runs it on every test map:

```sh
python usd/check.py usd/catalogues/sample.usda target/usd/*.usda
```

## What's in the stage

[SCHEMA.md](SCHEMA.md) lists every prim and attribute.
