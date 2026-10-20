# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate crg_order.xodr: roads and a junction whose <CRG> records overlap,
to show the last record in the file wins.

Roads 1 and 2 are 100 m straights, 40 m apart. Both list three elevation
records over one file, `elev.crg`, each lifting the road by its own
`zOffset`, and two friction records over `f1.crg` and `f2.crg`:

- elevation, 0 to 60 m, zOffset 0.1;
- elevation, 20 to 80 m, zOffset 0.2;
- elevation, 40 to 100 m, zOffset 0.3;
- friction, 0 to 50 m, `f1.crg`;
- friction, 30 to 100 m, `f2.crg`.

Road 2 lists the elevation records in the opposite order, so which record
is last differs from which one starts last.

Junction 9 is a tee of roads 11, 12 and 13, with connecting roads from 1000.
It lists two global elevation records over `elev.crg`, zOffset 0.6 and 0.7,
covering all of it. They place the file along +X from (960, 0), so the junction lies inside a 60 m by 40 m grid. Connecting road 1000 also has a road record of zOffset
0.5. A junction's records come before its roads' ones.

scenariogeneration writes no <surface>, so the records are spliced into its
XML. The header date is fixed, so the file is the same each run.
"""

import math
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

odr = xodr.OpenDrive("crg_order")
for road_id, y in [(1, 0), (2, 40)]:
    road = xodr.create_road(xodr.Line(100), road_id, 1, 1, lane_width=3.5)
    road.planview.set_start_point(0, y, 0)
    odr.add_road(road)

creator = xodr.CommonJunctionCreator(9, "tee", startnum=1000)
for k, angle in enumerate([0, math.pi / 2, math.pi]):
    arm = xodr.create_road(xodr.Line(30), 11 + k, 1, 1, lane_width=3.5)
    arm.planview.set_start_point(
        1000 + 42 * math.cos(angle), 42 * math.sin(angle), angle + math.pi
    )
    creator.add_incoming_road_circular_geometry(arm, 12, angle, "successor")
    odr.add_road(arm)
creator.add_connection(11, 12, [-1], [1])
creator.add_connection(12, 13, [-1], [1])
odr.add_junction_creator(creator)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_suffix(".xodr")
odr.write_xml(str(path))

tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
roads = {r.get("id"): r for r in root.findall("road")}


def crg(**attributes):
    return ET.Element("CRG", {k: str(v) for k, v in attributes.items()})


def surface(*records):
    element = ET.Element("surface")
    element.extend(records)
    return element


def elevation(start, end, z):
    return crg(file="elev.crg", sStart=start, sEnd=end, orientation="same",
               mode="attached", zOffset=z)


def friction(file, start, end):
    return crg(file=file, sStart=start, sEnd=end, orientation="same",
               mode="attached0", purpose="friction")


rising = [elevation(0, 60, 0.1), elevation(20, 80, 0.2), elevation(40, 100, 0.3)]
grip = [friction("f1.crg", 0, 50), friction("f2.crg", 30, 100)]
roads["1"].append(surface(*rising, *grip))
roads["2"].append(surface(*reversed(rising), *grip))

connecting = roads["1000"]
connecting.append(surface(elevation(0, 20, 0.5)))
junction = root.find("junction")
junction.append(surface(
    crg(file="elev.crg", mode="global", xOffset=960, yOffset=0, hOffset=0, zOffset=0.6),
    crg(file="elev.crg", mode="global", xOffset=960, yOffset=0, hOffset=0, zOffset=0.7),
))

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
