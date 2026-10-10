"""Generate curve_local.xodr: outlines drawn with <curveLocal> lines, arcs
and paramPoly3 curves.

    uv run -q --with scenariogeneration==0.16.6 tests/data/curve_local.py

Road 1 is a flat 60 m straight from the origin heading +X, with one left and
two right lanes, 3 m wide. So `(s, t)` is `(x, y)`. Its objects:

- 1, "Island" at s = 10, t = -5: a stadium 14 m by 4 m and 0.3 m tall, as
  ASAM's `Ex_SmoothObjectOutline_traffic_island` means it: a 10 m line,
  a half circle of radius 2, a line back and a half circle home. A marking
  runs along the first half circle, from corner 1 to corner 2, and a border
  round it all.
- 2, "Lens" at s = 30, t = 5: one arc of radius 2 with no length, which
  closes on itself, a full circle.
- 3, "Wave" at s = 45, t = -6, turned 0.5 rad: an open outline of a
  normalized paramPoly3 with no length from (0, 0) to (8, 0) bulging 1 m to
  the left, 1 m tall, then a line with no hdg or length to (8, 2), 2 m tall,
  and a last line from there 1 m on to (8, 3).
- 4, "Broken" at s = 50, t = 4: lines that do not join. The first ends 1 m
  short of the second, and the second 4 m short of the third.
- 5, "Unsized" at s = 20, t = 5: a paramPoly3 of pRange arcLength with no
  length, which cannot be drawn.
- 7, "Smooth" at s = 10, t = 6: the island's stadium, 4 m long, with no
  hdg after the first curve, so each curve goes on from the last.
- 6, "Swarm" at s = 0, t = 8: a 300 m arc, a thousand corners, repeated
  every 2.5 cm over 50 m, which would bake two million corners.

scenariogeneration writes no <curveLocal>, so the outlines are spliced into
its XML. The header date is fixed, so the file is the same each run.
"""

import math
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Line(60), id=1, left_lanes=1, right_lanes=2, lane_width=3)
road.planview.set_start_point(0, 0, 0)
road.add_object([
    xodr.Object(s=10, t=-5, Type=xodr.ObjectType.trafficIsland, id="1", name="Island"),
    xodr.Object(s=30, t=5, Type=xodr.ObjectType.obstacle, id="2", name="Lens"),
    xodr.Object(s=45, t=-6, Type=xodr.ObjectType.barrier, id="3", name="Wave", hdg=0.5),
    xodr.Object(s=50, t=4, Type=xodr.ObjectType.obstacle, id="4", name="Broken"),
    xodr.Object(s=20, t=5, Type=xodr.ObjectType.obstacle, id="5", name="Unsized"),
    xodr.Object(s=0, t=8, Type=xodr.ObjectType.obstacle, id="6", name="Swarm"),
    xodr.Object(s=10, t=6, Type=xodr.ObjectType.trafficIsland, id="7", name="Smooth"),
])

odr = xodr.OpenDrive("curve_local")
odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_name("curve_local.xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
objects = {o.get("id"): o for o in root.iter("object")}


def curve(u, v, height, shape, **given):
    node = ET.Element("curveLocal", {"u": str(u), "v": str(v), "z": "0", "height": str(height)})
    for name, value in given.items():
        node.set(name, str(value))
    node.append(shape)
    return node


def outline(id, closed, *curves):
    outlines = ET.SubElement(objects[id], "outlines")
    node = ET.SubElement(outlines, "outline", {"id": "0", "closed": closed})
    node.extend(curves)
    return node


def line():
    return ET.Element("line")


def arc(curvature):
    return ET.Element("arc", {"curvature": str(curvature)})


def poly3(prange, u, v):
    names = {f"{c}{axis}": str(value)
             for axis, values in [("U", u), ("V", v)]
             for c, value in zip("abcd", values)}
    return ET.Element("paramPoly3", {"pRange": prange, **names})


half = math.pi / 0.5
outline(
    "1", "true",
    curve(0, 0, 0.3, line(), hdg=0, length=10, id=0),
    curve(10, 0, 0.3, arc(0.5), hdg=0, length=half, id=1),
    curve(10, 4, 0.3, line(), hdg=math.pi, length=10, id=2),
    curve(0, 4, 0.3, arc(0.5), hdg=math.pi, length=half, id=3),
)
markings = ET.SubElement(objects["1"].find("outlines/outline"), "markings")
marking = ET.SubElement(markings, "marking", {
    "color": "white", "width": "0.1", "zOffset": "0.01", "startOffset": "0",
    "stopOffset": "0", "lineLength": "1", "spaceLength": "0",
})
for corner in ["1", "2"]:
    ET.SubElement(marking, "cornerReference", {"id": corner})
ET.SubElement(objects["1"], "borders").append(ET.Element("border", {
    "width": "0.2", "type": "curb", "outlineId": "0", "useCompleteOutline": "true",
}))
outline("2", "true", curve(0, 0, 1, arc(0.5), hdg=0))
outline(
    "3", "false",
    curve(0, 0, 1, poly3("normalized", [0, 8, 0, 0], [0, 4, -4, 0]), hdg=0),
    curve(8, 0, 2, line()),
    curve(8, 2, 2, line(), hdg=math.pi / 2, length=1),
)
outline(
    "4", "true",
    curve(0, 0, 1, line(), hdg=0, length=4),
    curve(4, 1, 1, line(), hdg=math.pi / 2, length=2),
    curve(0, 3, 1, line(), hdg=-math.pi / 2, length=3),
)
outline("5", "true", curve(0, 0, 1, poly3("arcLength", [0, 1, 0, 0], [0, 0, 0.1, 0]), hdg=0))
outline("6", "false", curve(0, 0, 1, arc(0.01), hdg=0, length=300))
outline(
    "7", "true",
    curve(0, 0, 0.3, line(), hdg=0, length=4),
    curve(4, 0, 0.3, arc(0.5), length=half),
    curve(4, 4, 0.3, line()),
    curve(0, 4, 0.3, arc(0.5), length=half),
)
ET.SubElement(objects["6"], "repeat", {
    "s": "0", "length": "50", "distance": "0.025", "tStart": "8", "tEnd": "8",
    "zOffsetStart": "0", "zOffsetEnd": "0",
})

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
