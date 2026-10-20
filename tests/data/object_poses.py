# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate object_poses.xodr: objects standing upright on a graded, banked
road, and the same objects following its surface with `perpToRoad`.

Road 1 is a 60 m straight from the origin heading 0.5 rad, at elevation
`2 + 0.05 s`, banked 0.1 rad about its reference line. It has one left lane
and two right lanes, 3 m wide. Its objects:

- 1, "Upright": a 2 x 1 x 3 m box at s = 20, t = -4.5, raised 0.5 m, turned
  0.3 rad, pitched 0.2 and rolled -0.1, with `perpToRoad="false"`.
- 2, "Follower": the same box at s = 40, with `perpToRoad="true"`.
- 3, "Shelter": a 3 x 2 m footprint in its own frame, 2.5 m tall, at s = 30,
  t = 4, turned 0.2 rad, with `perpToRoad="true"`, and a marking along its
  first edge.
- 4, "Booth": the shelter at s = 50, standing upright.
- 5, "Plumb" and 6, "Leaning": 3 m posts at s = 10, at t = -2 and t = 2,
  the second with `perpToRoad="true"`, to show the two side by side.

scenariogeneration writes no `perpToRoad` and no `<markings>`, so they are
spliced into its XML. The header date is fixed, so the file is the same each
run.
"""

import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Line(60), id=1, left_lanes=1, right_lanes=2, lane_width=3)
road.planview.set_start_point(0, 0, 0.5)
road.add_elevation(0, 2.0, 0.05, 0, 0)
road.add_superelevation(0, 0.1, 0, 0, 0)

for id, name, s in [("1", "Upright", 20), ("2", "Follower", 40)]:
    road.add_object(xodr.Object(s=s, t=-4.5, Type=xodr.ObjectType.pole, id=id, name=name,
                                zOffset=0.5, hdg=0.3, pitch=0.2, roll=-0.1,
                                length=2, width=1, height=3))

for id, name, t in [("5", "Plumb", -2), ("6", "Leaning", 2)]:
    road.add_object(xodr.Object(s=10, t=t, Type=xodr.ObjectType.pole, id=id, name=name,
                                length=0.3, width=0.3, height=3))

for id, name, s in [("3", "Shelter", 30), ("4", "Booth", 50)]:
    shelter = xodr.Object(s=s, t=4, Type=xodr.ObjectType.building, id=id, name=name,
                          hdg=0.2)
    footprint = xodr.Outline(closed=True, id=0)
    for corner, (u, v) in enumerate([(0, 0), (3, 0), (3, 2), (0, 2)]):
        footprint.add_corner(xodr.CornerLocal(u, v, 0, 2.5, id=corner))
    shelter.add_outline(footprint)
    road.add_object(shelter)

odr = xodr.OpenDrive("object_poses")
odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_name("object_poses.xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
objects = {o.get("id"): o for o in root.iter("object")}
objects["1"].set("perpToRoad", "false")
for id in ["2", "3", "6"]:
    objects[id].set("perpToRoad", "true")
for id in ["3", "4"]:
    markings = ET.SubElement(objects[id], "markings")
    marking = ET.SubElement(markings, "marking", {
        "color": "white", "width": "0.2", "zOffset": "0.01",
        "spaceLength": "0", "lineLength": "0",
    })
    for corner in ["0", "1"]:
        ET.SubElement(marking, "cornerReference", {"id": corner})

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
