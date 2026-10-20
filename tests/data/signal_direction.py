# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate signal_direction.xodr: signals on roads whose lanes run against
their side of the road.

    uv run tests/data/signal_direction.py

All three roads are 100 m straights heading +X, 30 m apart.

Road 1, right-hand traffic, has left lanes 1 (standard) and 2 (reversed),
and right lanes -1 (standard), -2 (reversed) and -3 (both). So lanes 2 and
-1 run with +s, lanes 1 and -2 against it, and -3 both ways.

Road 2, left-hand traffic, has the same lanes, so lanes 1 and -2 run with
+s, and lanes 2 and -1 against it.

Road 3, right-hand traffic, has left lane 1 and right lanes -1 and -2,
where -2 is `type="bidirectional"` with no direction, so it runs both ways.

Each road carries a signal of each orientation, at s = 10, 20 and 30, and a
`+` signal at s = 40 with a <validity> of lane -2. Road 1's signal "ref" at
s = 50 is `+`, and road 2 holds a <signalReference> to it at s = 60 with
orientation `-`.

scenariogeneration writes neither lane directions, left-hand traffic nor
signals, so they are set on its XML after it writes the file. The header
date is fixed, so the file is the same each run.
"""

import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

odr = xodr.OpenDrive("signal_direction")
for road_id, y, left, right in [(1, 0, 2, 3), (2, 30, 2, 3), (3, 60, 1, 2)]:
    road = xodr.create_road(xodr.Line(100), road_id, left, right, lane_width=3)
    road.planview.set_start_point(0, y, 0)
    odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_suffix(".xodr")
odr.write_xml(str(path))

tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
roads = {r.get("id"): r for r in root.findall("road")}
roads["2"].set("rule", "LHT")


def lane(road, id):
    return roads[road].find(f"lanes/laneSection/*/lane[@id='{id}']")


for road in ("1", "2"):
    lane(road, 2).set("direction", "reversed")
    lane(road, -2).set("direction", "reversed")
    lane(road, -3).set("direction", "both")
lane("3", -2).set("type", "bidirectional")


def signal(id, s, orientation, *children):
    element = ET.Element("signal", {
        "id": id, "s": str(s), "t": "-8", "orientation": orientation,
        "dynamic": "no", "zOffset": "2", "country": "DE", "type": "274",
        "subtype": "50", "value": "50", "name": id,
    })
    element.extend(children)
    return element


for road in ("1", "2", "3"):
    signals = ET.SubElement(roads[road], "signals")
    signals.append(signal(f"{road}+", 10, "+"))
    signals.append(signal(f"{road}-", 20, "-"))
    signals.append(signal(f"{road}0", 30, "none"))
    signals.append(signal(
        f"{road}v", 40, "+",
        ET.Element("validity", {"fromLane": "-2", "toLane": "-2"}),
    ))
roads["1"].find("signals").append(signal("ref", 50, "+"))
roads["2"].find("signals").append(ET.Element("signalReference", {
    "id": "ref", "s": "60", "t": "-8", "orientation": "-",
}))

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
