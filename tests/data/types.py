# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate types.xodr: every lane type and object type the 1.9 schema
names, and the neighbours a shared lane has.

    uv run tests/data/types.py

Road 1 is a 100 m straight with 31 lanes of 1 m, one of each lane type:
lanes 1 to 15 on the left and -1 to -16 on the right, in the order of
LANE_TYPES. It holds one object of each object type, 3 m apart along its
edge. Both lists are the schema's, plus `banana`, which is in neither.

Road 2 is a 100 m straight with five lanes on the right: driving (-1),
shared (-2), driving (-3), sidewalk (-4) and shared (-5). So the shared
lane -2 stands between two driving lanes, and -5 stands beside a sidewalk
only. Roads 3 and 4 continue road 2 with the same lanes, so a shared
lane links to a shared lane.

scenariogeneration has no `shared` lane type and no `roadSurface` object
type, so the types are set on its XML after it writes the file. The header
date is fixed, so the file is the same each run.
"""

import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

LANE_TYPES = [
    "shoulder", "border", "driving", "stop", "none", "restricted", "parking",
    "median", "biking", "shared", "sidewalk", "curb", "exit", "entry",
    "onRamp", "offRamp", "connectingRamp", "bidirectional", "special1",
    "special2", "special3", "roadWorks", "tram", "rail", "bus", "taxi", "HOV",
    "mwyEntry", "mwyExit", "walking", "slipLane",
]
OBJECT_TYPES = [
    "none", "obstacle", "car", "pole", "tree", "vegetation", "barrier",
    "building", "parkingSpace", "patch", "railing", "trafficIsland",
    "crosswalk", "streetLamp", "gantry", "soundBarrier", "van", "bus",
    "trailer", "bike", "motorbike", "tram", "train", "pedestrian", "wind",
    "roadMark", "roadSurface", "banana",
]

odr = xodr.OpenDrive("types")
all_types = xodr.create_road(xodr.Line(100), 1, 15, 16, lane_width=1)
all_types.planview.set_start_point(0, 0, 0)
odr.add_road(all_types)

mixed = xodr.create_road(xodr.Line(100), 2, 0, 5, lane_width=3)
mixed.planview.set_start_point(0, 50, 0)
second = xodr.create_road(xodr.Line(100), 3, 0, 5, lane_width=3)
third = xodr.create_road(xodr.Line(100), 4, 0, 5, lane_width=3)
mixed.add_successor(xodr.ElementType.road, 3, xodr.ContactPoint.start)
second.add_predecessor(xodr.ElementType.road, 2, xodr.ContactPoint.end)
second.add_successor(xodr.ElementType.road, 4, xodr.ContactPoint.start)
third.add_predecessor(xodr.ElementType.road, 3, xodr.ContactPoint.end)
for road in (mixed, second, third):
    odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_suffix(".xodr")
odr.write_xml(str(path))

tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
roads = {r.get("id"): r for r in root.findall("road")}

lanes = roads["1"].findall("lanes/laneSection/*/lane")
by_id = {int(lane.get("id")): lane for lane in lanes if lane.get("id") != "0"}
ids = list(range(1, 16)) + list(range(-1, -17, -1))
for lane_id, lane_type in zip(ids, LANE_TYPES):
    by_id[lane_id].set("type", lane_type)

for road_id, types in [("2", ["driving", "shared", "driving", "sidewalk", "shared"]),
                       ("3", ["driving", "shared", "driving", "sidewalk", "shared"]),
                       ("4", ["driving", "shared", "driving", "sidewalk", "shared"])]:
    right = roads[road_id].findall("lanes/laneSection/right/lane")
    for lane, lane_type in zip(sorted(right, key=lambda l: -int(l.get("id"))), types):
        lane.set("type", lane_type)

objects = ET.SubElement(roads["1"], "objects")
for k, object_type in enumerate(OBJECT_TYPES):
    ET.SubElement(objects, "object", {
        "id": f"o{k}", "type": object_type, "s": str(2 + 3 * k), "t": "-20",
        "zOffset": "0", "hdg": "0", "length": "1", "width": "1", "height": "1",
        "name": object_type,
    })

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
