"""Check stages from xodr_usd with OpenUSD, alone and under a catalogue.

    python usd/check.py CATALOGUE.usda STAGE.usda [STAGE.usda ...]

For each stage, alone and with the catalogue layered over it, this checks
that OpenUSD's validators pass, that every relationship target exists, that
every mesh is well formed, and that every signal board has geometry. Then
it flattens the stage and checks the boards still have geometry. It also
checks that every type in the catalogue matches a board in some stage, to
catch a misspelled type. Exits 1 if any check fails. Needs OpenUSD's Python
module: pip install usd-core.
"""

import sys

try:
    from pxr import Gf, Sdf, Usd, UsdGeom, UsdValidation

    # Register the validators of each schema module.
    from pxr import UsdPhysics, UsdShade, UsdSkel  # noqa: F401
except ImportError:
    sys.exit("needs OpenUSD's Python module: pip install usd-core")

from flatten import compose

SCHEMA_VERSION = 1


def problems(stage):
    """Every problem with `stage`, as text."""
    found = [str(e) for e in stage.GetCompositionErrors()]
    validators = UsdValidation.ValidationRegistry().GetOrLoadAllValidators()
    for error in UsdValidation.ValidationContext(validators).Validate(stage):
        found.append(error.GetMessage())
    version = stage.GetRootLayer().customLayerData.get("xodr", {})
    if version.get("schemaVersion") != SCHEMA_VERSION:
        found.append(f"schema version is {version}, not {SCHEMA_VERSION}")
    for prim in stage.Traverse():
        for rel in prim.GetRelationships():
            for target in rel.GetTargets():
                if not stage.GetPrimAtPath(target):
                    found.append(f"{rel.GetPath()} links to missing {target}")
        if prim.IsA(UsdGeom.Mesh):
            found += mesh_problems(prim)
        if prim.GetPath().GetParentPath().name.startswith("signal_"):
            if not any(p.IsA(UsdGeom.Mesh) for p in Usd.PrimRange(prim)):
                found.append(f"{prim.GetPath()} has no geometry")
    return found


def mesh_problems(prim):
    """Every problem with one mesh: indices, extent and, on lanes and
    marks, faces that point down."""
    mesh = UsdGeom.Mesh(prim)
    points = mesh.GetPointsAttr().Get()
    indices = mesh.GetFaceVertexIndicesAttr().Get()
    counts = mesh.GetFaceVertexCountsAttr().Get()
    if sum(counts) != len(indices) or max(indices) >= len(points):
        return [f"{prim.GetPath()} has bad indices"]
    found = []
    extent = mesh.GetExtentAttr().Get()
    if not Gf.IsClose(mesh.ComputeExtent(points)[0], extent[0], 1e-3):
        found.append(f"{prim.GetPath()} has the wrong extent")
    if prim.GetName().startswith(("lane_", "mark_")):
        start = 0
        for n in counts:
            a, b, c = (Gf.Vec3d(points[indices[start + k]]) for k in range(3))
            if Gf.Cross(b - a, c - a)[2] < 0:
                found.append(f"{prim.GetPath()} has a face pointing down")
                break
            start += n
    return found


def types(stage):
    """The type classes the stage's boards inherit."""
    return {
        str(path)
        for prim in stage.Traverse()
        for path in prim.GetInherits().GetAllDirectInherits()
    }


def main(catalogue, *stages):
    failed = False
    layer = Sdf.Layer.FindOrOpen(catalogue)
    unused = {
        f"/{root.name}/{name}"
        for root in layer.rootPrims
        for name in root.nameChildren.keys()
    }
    for path in stages:
        unused -= types(Usd.Stage.Open(path))
        flat = Usd.Stage.Open(Sdf.Layer.CreateAnonymous(".usda"))
        flat.GetRootLayer().ImportFromString(
            compose(path, [catalogue]).Flatten().ExportToString()
        )
        found = [
            *(f"alone: {p}" for p in problems(Usd.Stage.Open(path))),
            *(f"with catalogue: {p}" for p in problems(compose(path, [catalogue]))),
            *(f"flattened: {p}" for p in problems(flat)),
        ]
        failed |= bool(found)
        print(f"{'FAIL' if found else 'ok  '} {path}")
        for p in found:
            print(f"     {p}")
    for path in sorted(unused):
        print(f"FAIL {catalogue}: no board has type {path}")
    return 1 if failed or unused else 0


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    sys.exit(main(*sys.argv[1:]))
