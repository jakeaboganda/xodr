# /// script
# requires-python = ">=3.12"
# dependencies = ["usd-core==26.8"]
# ///
"""Layer catalogues over a stage and flatten the result into one file.

    uv run usd/flatten.py OUT.usda STAGE.usda [CATALOGUE.usda ...]

The flat file has no inherits or sublayers. Readers that can't combine
layers, such as three.js's USDLoader, then draw it as usdview would.
"""

import os
import sys

try:
    from pxr import Sdf, Usd
except ImportError:
    sys.exit(f"needs OpenUSD's Python module: run this script with uv run {sys.argv[0]}")


def compose(stage, catalogues):
    """Open `stage` with each catalogue layered over it, strongest first."""
    root = Sdf.Layer.CreateAnonymous(".usda")
    root.subLayerPaths = [os.path.abspath(p) for p in [*catalogues, stage]]
    base = Sdf.Layer.FindOrOpen(stage)
    if base is None:
        sys.exit(f"cannot open {stage}")
    for key in ["defaultPrim", "upAxis", "metersPerUnit", "customLayerData"]:
        if base.pseudoRoot.HasInfo(key):
            root.pseudoRoot.SetInfo(key, base.pseudoRoot.GetInfo(key))
    return Usd.Stage.Open(root)


def main(out, stage, *catalogues):
    if not compose(stage, catalogues).Flatten().Export(out):
        sys.exit(f"could not write {out}")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main(*sys.argv[1:])
