# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate outline_markings.xodr: markings inside outlines, as OpenDRIVE 1.9
writes them, and areas of paint.

Road 1 is a 60 m straight from the origin heading +X, banked 0.1 rad about
its reference line. It has one left lane and two right lanes, 3 m wide. Its
objects:

- 1, "Bay": a parking space outlined in road coordinates from s = 5 to 10
  and t = -1 to -5, with a solid marking along its s = 5 edge and a dashed
  one along its t = -5 edge, both inside the outline.
- 2, "Arrow": a closed paint area in road coordinates, s = 20 to 26 and
  t = 1 to 2, 0.2 m tall, with an inner outline from s = 22 to 24 and
  t = 1.25 to 1.75 with corners 4 to 7. Inside the outer outline, a marking
  along its first edge, and one that names corner 5, which only the inner
  outline has.
- 3, "Island": a grass outline in its own frame at s = 30, t = -3, 4 by 2 m
  and 0.15 m tall, with a marking along its first edge inside the outline.
- 4, "Patch": a paint area in road coordinates, s = 40 to 44 and t = -1 to
  -2, with a border round it, and a material whose `roadMarkColor` is
  yellow.
- 5, "Stripe": an open outline of paint in road coordinates along t = 2.5
  from s = 50 to 55, 0.1 m tall.
- 6, "Kerb": an outline in road coordinates from s = 50 to 55 and t = -3 to
  -4, with a marking inside the outline along its s = 50 edge and one under
  the object, as OpenDRIVE 1.5 writes it, along its s = 55 edge.

scenariogeneration writes no markings inside an outline, no borders and no
paint fill, so they are spliced into its XML. The header date is fixed, so the file is the
same each run.
"""

import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Line(60), id=1, left_lanes=1, right_lanes=2, lane_width=3)
road.planview.set_start_point(0, 0, 0)
road.add_superelevation(0, 0.1, 0, 0, 0)


def road_outline(corners, height, closed=True, fill=None, outer=None, id=0, first=0):
    outline = xodr.Outline(closed=closed, fill_type=fill, outer=outer, id=id)
    for corner, (s, t) in enumerate(corners, first):
        outline.add_corner(xodr.CornerRoad(s, t, 0, height, id=corner))
    return outline


def rectangle(s0, s1, t0, t1):
    return [(s0, t0), (s1, t0), (s1, t1), (s0, t1)]


bay = xodr.Object(s=7.5, t=-3, Type=xodr.ObjectType.parkingSpace, id="1", name="Bay")
bay.add_outline(road_outline(rectangle(5, 10, -1, -5), 0))
arrow = xodr.Object(s=23, t=1.5, Type=xodr.ObjectType.roadMark, id="2", name="Arrow")
arrow.add_outline(road_outline(rectangle(20, 26, 1, 2), 0.2))
arrow.add_outline(road_outline(rectangle(22, 24, 1.25, 1.75), 0.2, outer=False, id=1,
                               first=4))
island = xodr.Object(s=30, t=-3, Type=xodr.ObjectType.trafficIsland, id="3", name="Island")
grass = xodr.Outline(closed=True, fill_type=xodr.FillType.grass, id=0)
for corner, (u, v) in enumerate([(0, 0), (4, 0), (4, 2), (0, 2)]):
    grass.add_corner(xodr.CornerLocal(u, v, 0, 0.15, id=corner))
island.add_outline(grass)
patch = xodr.Object(s=42, t=-1.5, Type=xodr.ObjectType.roadMark, id="4", name="Patch")
patch.add_outline(road_outline(rectangle(40, 44, -1, -2), 0))
stripe = xodr.Object(s=52.5, t=2.5, Type=xodr.ObjectType.roadMark, id="5", name="Stripe")
stripe.add_outline(road_outline([(50, 2.5), (55, 2.5)], 0.1, closed=False))
kerb = xodr.Object(s=52.5, t=-3.5, Type=xodr.ObjectType.barrier, id="6", name="Kerb")
kerb.add_outline(road_outline(rectangle(50, 55, -3, -4), 0.1))
road.add_object([bay, arrow, island, patch, stripe, kerb])

odr = xodr.OpenDrive("outline_markings")
odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_name("outline_markings.xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
objects = {o.get("id"): o for o in root.iter("object")}


def marking(parent, corners, line=1.0, space=0.0):
    markings = parent.find("markings")
    if markings is None:
        markings = ET.SubElement(parent, "markings")
    m = ET.SubElement(markings, "marking", {
        "color": "white", "width": "0.1", "zOffset": "0.01", "startOffset": "0",
        "stopOffset": "0", "lineLength": str(line), "spaceLength": str(space),
    })
    for corner in corners:
        ET.SubElement(m, "cornerReference", {"id": str(corner)})


def outline(id, k=0):
    return objects[id].findall("outlines/outline")[k]


for id in ["2", "4", "5"]:
    outline(id).set("fillType", "paint")
marking(outline("1"), [3, 0])
marking(outline("1"), [1, 2], line=1.0, space=0.5)
marking(outline("2"), [0, 5])
marking(outline("2"), [0, 1])
marking(outline("3"), [0, 1])
ET.SubElement(objects["4"], "material", {"surface": "paint", "roadMarkColor": "yellow"})
ET.SubElement(objects["4"], "borders").append(ET.Element("border", {
    "width": "0.2", "type": "paint", "outlineId": "0", "useCompleteOutline": "true",
}))
marking(outline("6"), [3, 0])
marking(objects["6"], [1, 2])

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
