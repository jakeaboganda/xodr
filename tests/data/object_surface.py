"""Generate object_surface.xodr: objects whose <surface> CRG adds to the road
height, over roads with and without road CRGs.

    uv run -q --with scenariogeneration==0.16.6 tests/data/object_surface.py

Roads 1, 2 and 3 are flat 100 m straights heading +X from (0, 0), (0, 40)
and (0, 80), with one 3.5 m lane each side. `plate.crg` and `small.crg` are
planes centred on the object, and `road.crg` a plane along the road. The
tests make their own copies. This script writes them beside the map too, so
the viewer can show them.

Road 1 has no road CRG. Its objects:

- "Plate": a 2 by 2 m box at s = 20, t = -1.75, turned 0.5 rad, raised
  0.3 m, with `plate.crg` at zScale 2.
- "Cover": a 2 by 2 m box at s = 30, t = 1.75, with `small.crg`, which has
  no values beyond u = 0.5.
- "Hole": a 1 by 1 m box every 10 m over 20 m from s = 50, with
  `plate.crg`, so three of them.
- "Disc": a cylinder at s = 80 with `plate.crg`, which cannot take it.
- "Tilted": a 2 by 2 m box at s = 90, t = -1.75, turned 0.4 rad, pitched
  0.3 and rolled 0.2, with `plate.crg`.

Road 2 has an `attached` road CRG, `road.crg`, over its whole length. Its
objects are 2 by 2 m boxes with `plate.crg` at t = -1.75: "Hider" at s = 20,
which hides the road CRG, "Default" at s = 40, which says nothing and so
hides it, "Stacker" at s = 60, which does not, and "Zero" at s = 80, which
says "0" and does not.

Road 3 has an `attached0` road CRG, `road.crg`, over its whole length. Its
objects: "Clash" at s = 20, which hides it, as the spec forbids, and "Sum"
at s = 60, which does not.

Junction 9 is a crossroads at (1000, 0) of roads 11 to 14, with connecting
roads from 1000. Its reference line runs 40 m along +X from (980, 0), and
carries a "Grate" with `plate.crg` at s = 20, t = 1, on the connecting road
straight across. That road, 1000, has an `attached0` CRG over its first 5 m,
which is away from the grate but on a lane the grate's CRG covers.

scenariogeneration writes no <surface>, so the records are spliced into its
XML. The header date is fixed, so the file is the same each run.
"""

import math
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

odr = xodr.OpenDrive("object_surface")
for road_id, y in [(1, 0), (2, 40), (3, 80)]:
    road = xodr.create_road(xodr.Line(100), road_id, 1, 1, lane_width=3.5)
    road.planview.set_start_point(0, y, 0)
    odr.add_road(road)

creator = xodr.CommonJunctionCreator(9, "cross", startnum=1000)
for k, angle in enumerate([0, math.pi / 2, math.pi, 3 * math.pi / 2]):
    arm = xodr.create_road(xodr.Line(30), 11 + k, 1, 1, lane_width=3.5)
    arm.planview.set_start_point(
        1000 + 42 * math.cos(angle), 42 * math.sin(angle), angle + math.pi
    )
    creator.add_incoming_road_circular_geometry(arm, 12, angle, "successor")
    odr.add_road(arm)
creator.add_connection(11, 13)
creator.add_connection(12, 14)
odr.add_junction_creator(creator)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_name("object_surface.xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
roads = {r.get("id"): r for r in root.findall("road")}


def frag(text):
    return ET.fromstring(text)


def crg(file, hide="true", scale="1"):
    hidden = "" if hide is None else f' hideRoadSurfaceCRG="{hide}"'
    return f'<surface><CRG file="{file}"{hidden} zScale="{scale}"/></surface>'



def box(name, s, t, inner, size=2, extra=""):
    return (f'<object id="{name.lower()}" name="{name}" type="roadSurface" s="{s}" t="{t}" '
            f'zOffset="0" length="{size}" width="{size}" height="0.05" {extra}>{inner}</object>')


def objects(*items):
    return frag("<objects>" + "".join(items) + "</objects>")


roads["1"].append(objects(
    box("Plate", 20, -1.75, crg("plate.crg", scale="2"), extra='hdg="0.5"'),
    box("Cover", 30, 1.75, crg("small.crg")),
    box("Hole", 50, -1.75, '<repeat s="50" length="20" distance="10" tStart="-1.75" '
        'tEnd="-1.75" zOffsetStart="0" zOffsetEnd="0"/>' + crg("plate.crg"), size=1),
    '<object id="disc" name="Disc" type="roadSurface" s="80" t="-1.75" zOffset="0" '
    f'radius="1" height="0.05">{crg("plate.crg")}</object>',
    box("Tilted", 90, -1.75, crg("plate.crg"), extra='hdg="0.4" pitch="0.3" roll="0.2"'),
))
roads["1"].find("objects/object[@id='plate']").set("zOffset", "0.3")


def road_crg(mode):
    return frag(f'<surface><CRG file="road.crg" sStart="0" sEnd="100" orientation="same" '
                f'mode="{mode}" sOffset="0" tOffset="0"/></surface>')


roads["2"].append(objects(
    box("Hider", 20, -1.75, crg("plate.crg", hide="true")),
    box("Default", 40, -1.75, crg("plate.crg", hide=None)),
    box("Stacker", 60, -1.75, crg("plate.crg", hide="false")),
    box("Zero", 80, -1.75, crg("plate.crg", hide="0")),
))
roads["2"].append(road_crg("attached"))
roads["3"].append(objects(
    box("Clash", 20, -1.75, crg("plate.crg", hide="true")),
    box("Sum", 60, -1.75, crg("plate.crg", hide="false")),
))
roads["3"].append(road_crg("attached0"))

roads["1000"].append(frag(
    '<surface><CRG file="road.crg" sStart="0" sEnd="5" orientation="same" mode="attached0" '
    'sOffset="0" tOffset="0"/></surface>'
))
junction = root.find("junction")
junction.insert(0, frag(
    '<planView><geometry s="0" x="980" y="0" hdg="0" length="40"><line/></geometry></planView>'
))
junction.append(objects(box("Grate", 20, 1, crg("plate.crg"))))

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)


def crg_file(name, start, length, half, step, value):
    """An LRFI CRG file of `value(u, v)`, `u` from `start` over `length`."""
    columns = round(2 * half / step) + 1
    lines = [
        f"* {name}, generated by tests/data/object_surface.py.",
        "$ROAD_CRG",
        f"REFERENCE_LINE_START_U = {start}",
        f"REFERENCE_LINE_INCREMENT = {step}",
        f"LONG_SECTION_V_RIGHT = {-half}",
        f"LONG_SECTION_V_INCREMENT = {step}",
        "$",
        "$KD_Definition",
        "#:LRFI",
    ]
    lines += [f"D:long section {c + 1},m" for c in range(columns)]
    lines += ["$", "$$$$"]
    for r in range(round(length / step) + 1):
        values = [value(start + r * step, -half + c * step) for c in range(columns)]
        cells = ["       NaN" if math.isnan(z) else f"{z:10.6f}" for z in values]
        for at in range(0, columns, 8):
            lines.append("".join(cells[at : at + 8]))
    path.with_name(name).write_text("\n".join(lines) + "\n")


crg_file("plate.crg", -2.0, 4.0, 2.0, 0.25, lambda u, v: 0.05 * u - 0.03 * v + 0.01)
crg_file("small.crg", -1.0, 2.0, 1.0, 0.25,
         lambda u, v: math.nan if u > 0.5 or abs(v) > 0.5 else 0.02 * u + 0.04 * v + 0.02)
crg_file("road.crg", 0.0, 100.0, 5.0, 0.5, lambda u, v: 0.01 * u + 0.02 * v + 0.005)
