"""Layer catalogues over a stage and flatten the result into one file.

    python usd/flatten.py OUT.usda STAGE.usda [CATALOGUE.usda ...]

The flat file has no inherits or sublayers, so readers that can't compose
USD, such as three.js's USDLoader, draw it as a full USD reader would. Needs
OpenUSD's Python module: pip install usd-core.
"""

import os
import sys

try:
    from pxr import Sdf, Usd
except ImportError:
    sys.exit("needs OpenUSD's Python module: pip install usd-core")


def compose(stage, catalogues):
    """Open `stage` with each catalogue layered over it, strongest first."""
    root = Sdf.Layer.CreateAnonymous(".usda")
    root.subLayerPaths = [os.path.abspath(p) for p in [*catalogues, stage]]
    base = Sdf.Layer.FindOrOpen(stage)
    for key in ["defaultPrim", "upAxis", "metersPerUnit", "customLayerData"]:
        if base.pseudoRoot.HasInfo(key):
            root.pseudoRoot.SetInfo(key, base.pseudoRoot.GetInfo(key))
    return Usd.Stage.Open(root)


def main(out, stage, *catalogues):
    compose(stage, catalogues).Flatten().Export(out)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main(*sys.argv[1:])
