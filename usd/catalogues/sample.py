"""Generate sample.usda, the sample signal catalogue.

    python3 usd/catalogues/sample.py > usd/catalogues/sample.usda

Each entry in `types` overrides one type class: its labels and its meshes,
in board units. See usd/SCHEMA.md, section Catalogues.
"""

import math
N = 24
def f(x):
    s = f"{x:.4f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s
def pt(p): return "(" + ", ".join(f(c) for c in p) + ")"
def mesh(name, pts, counts, color, x_front=None, double=True, pad="        ", over=False):
    idx = list(range(len(pts))) if counts is None else None
    lines = []
    head = f'over "{name}"' if over else f'def Mesh "{name}"'
    lines.append(f"{pad}{head}")
    lines.append(f"{pad}{{")
    lo = [min(p[i] for p in pts) for i in range(3)]; hi = [max(p[i] for p in pts) for i in range(3)]
    lines.append(f"{pad}    float3[] extent = [{pt(lo)}, {pt(hi)}]")
    lines.append(f"{pad}    int[] faceVertexCounts = [{', '.join(str(c) for c in counts)}]")
    lines.append(f"{pad}    int[] faceVertexIndices = [{', '.join(str(i) for i in range(sum(counts)))}]")
    lines.append(f"{pad}    point3f[] points = [{', '.join(pt(p) for p in pts)}]")
    lines.append(f"{pad}    color3f[] primvars:displayColor = [{pt(color)}]")
    if not over:
        lines.append(f"{pad}    uniform bool doubleSided = {1 if double else 0}")
        lines.append(f'{pad}    uniform token subdivisionScheme = "none"')
    lines.append(f"{pad}}}")
    return "\n".join(lines)
def ellipse(x, cy, cz, ry, rz):
    return [(x, cy + ry*math.cos(2*math.pi*k/N), cz + rz*math.sin(2*math.pi*k/N)) for k in range(N)]
def poly(x, corners):  # corners in (y, z)
    return [(x, y, z) for y, z in corners]
RED, WHITE, YELLOW, BLUE, DARK = (0.8,0.1,0.1), (0.95,0.95,0.95), (0.95,0.8,0.1), (0.1,0.3,0.7), (0.1,0.1,0.1)
def round_sign(): # red ring, white middle
    return [mesh("Board", ellipse(0, 0, 0.5, 0.5, 0.5), [N], RED, over=True),
            mesh("Face", ellipse(0.005, 0, 0.5, 0.4, 0.4), [N], WHITE, double=False)]
def triangle(up):
    if up:
        outer, inner = [(-0.5,0.067),(0.5,0.067),(0,0.933)], [(-0.36,0.15),(0.36,0.15),(0,0.77)]
    else:
        outer, inner = [(0,0.067),(0.5,0.933),(-0.5,0.933)], [(0,0.23),(0.36,0.85),(-0.36,0.85)]
    return [mesh("Board", poly(0, outer), [3], RED, over=True),
            mesh("Face", poly(0.005, inner), [3], WHITE, double=False)]
def plate(color):
    return [mesh("Board", poly(0, [(-0.5,0),(0.5,0),(0.5,1),(-0.5,1)]), [4], color, over=True)]
def light():
    # housing: a box 0.3 m deep, centred on the signal's position
    y0, y1, z0, z1, x0, x1 = -0.5, 0.5, 0, 1, -0.15, 0.15
    c = lambda x, y, z: (x, y, z)
    faces = [
        [c(x1,y0,z0), c(x1,y1,z0), c(x1,y1,z1), c(x1,y0,z1)],
        [c(x0,y1,z0), c(x0,y0,z0), c(x0,y0,z1), c(x0,y1,z1)],
        [c(x0,y0,z0), c(x1,y0,z0), c(x1,y0,z1), c(x0,y0,z1)],
        [c(x1,y1,z0), c(x0,y1,z0), c(x0,y1,z1), c(x1,y1,z1)],
        [c(x0,y0,z1), c(x1,y0,z1), c(x1,y1,z1), c(x0,y1,z1)],
        [c(x0,y1,z0), c(x1,y1,z0), c(x1,y0,z0), c(x0,y0,z0)],
    ]
    out = [mesh("Board", [p for f in faces for p in f], [4]*6, DARK, over=True)]
    # three lamps, round on a 0.3 m by 0.9 m board
    for name, z, col in [("Red", 5/6, (0.9,0.1,0.1)), ("Amber", 0.5, (0.95,0.6,0.05)), ("Green", 1/6, (0.1,0.8,0.2))]:
        out.append(mesh(name, ellipse(0.155, 0, z, 0.35, 0.35*0.3/0.9), [N], col, double=False))
    return out
def stop_line():
    # flat on the road, 0.3 m deep. Z is in board units, so 0.2 of a 3 cm board.
    return [mesh("Board", poly(0, []) + [(-0.15,-0.5,0.2),(0.15,-0.5,0.2),(0.15,0.5,0.2),(-0.15,0.5,0.2)], [4], WHITE, over=True)]

types = [
    ("DE_274__1", "speed limit", ["traffic_sign", "speed_limit"], round_sign),
    ("DE_274_53", "speed limit 30", ["traffic_sign", "speed_limit"], round_sign),
    ("DE_274_55", "speed limit 50", ["traffic_sign", "speed_limit"], round_sign),
    ("DE_274_60", "speed limit 60", ["traffic_sign", "speed_limit"], round_sign),
    ("DE_274_58", "speed limit 80", ["traffic_sign", "speed_limit"], round_sign),
    ("DE_274_62", "speed limit 120", ["traffic_sign", "speed_limit"], round_sign),
    ("DE_262__1", "weight limit", ["traffic_sign", "prohibitory"], round_sign),
    ("DE_253_", "no lorries", ["traffic_sign", "prohibitory"], round_sign),
    ("DE_101__1", "danger", ["traffic_sign", "warning"], lambda: triangle(True)),
    ("DE_205__1", "give way", ["traffic_sign", "priority"], lambda: triangle(False)),
    ("DE_1048_12", "plate: lorries only", ["traffic_sign", "supplementary"], lambda: plate(WHITE)),
    ("DE_310__1", "town entrance", ["traffic_sign", "information"], lambda: plate(YELLOW)),
    ("DE_332__1", "motorway exit", ["traffic_sign", "direction"], lambda: plate(BLUE)),
    ("OPENDRIVE_1000001__1", "traffic light, red, amber and green", ["traffic_light"], light),
    ("OPENDRIVE_294__1", "stop line", ["road_marking", "stop_line"], stop_line),
    ("OPENDRIVE_1000015__1", "variable message board", ["traffic_sign", "variable_message"], lambda: plate(DARK)),
]
print("""#usda 1.0
(
    doc = \"\"\"A sample catalogue for the signal types in the xodr test maps.
Each prim overrides one type class from a stage xodr_usd writes.
See usd/SCHEMA.md, section Catalogues.\"\"\"
)

over "_SignalTypes"
{""")
for i, (name, what, labels, make) in enumerate(types):
    if i: print()
    print(f"    # {what}")
    print(f'    over "{name}" (')
    print('        prepend apiSchemas = ["SemanticsLabelsAPI:class"]')
    print("    )")
    print("    {")
    print(f"        token[] semantics:labels:class = [{', '.join(chr(34)+l+chr(34) for l in labels)}]")
    print()
    print("\n\n".join(make()))
    print("    }")
print("}")
