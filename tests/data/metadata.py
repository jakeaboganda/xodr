"""Generate metadata.xodr and metadata_bare.xodr: the descriptive attributes
a map carries that the crate keeps for the caller and builds nothing from.

    uv run -q --with scenariogeneration==0.16.6 tests/data/metadata.py

metadata.xodr is a 100 m road 1, named "Main Street", and a tee of roads
11, 12 and 13 round junction 100, named "Market Square". Its <header> gives
every attribute the schema allows: revMajor 1, revMinor 9, name, version,
date, vendor, and the bounds north, south, east and west.

metadata_bare.xodr is the same map with none of it, except a <header> whose
numbers are unreadable: revMajor "one", revMinor "1.5", and a north of
"NaN" beside a south, an east and a west that read.

scenariogeneration writes some of these, but not all, and with a date that
changes on every run, so they are set on its XML. The files are the same
each run.
"""

import math
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr


def build(name):
    odr = xodr.OpenDrive(name)
    road = xodr.create_road(xodr.Line(100), 1, 1, 1, lane_width=3.5)
    road.planview.set_start_point(0, 0, 0)
    odr.add_road(road)
    creator = xodr.CommonJunctionCreator(100, "tee", startnum=1000)
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
    path = Path(__file__).parent / f"{name}.xodr"
    odr.write_xml(str(path))
    return path, ET.parse(path)


def finish(path, tree):
    ET.indent(tree, space="    ")
    tree.write(path, encoding="utf-8", xml_declaration=True)


path, tree = build("metadata")
root = tree.getroot()
header = root.find("header")
header.attrib.clear()
header.attrib.update({
    "revMajor": "1", "revMinor": "9", "name": "Metadata Map", "version": "3.2",
    "date": "2026-01-01T00:00:00", "vendor": "Example Maps",
    "north": "120.5", "south": "-30.25", "east": "1050", "west": "-5",
})
root.find("road[@id='1']").set("name", "Main Street")
root.find("junction[@id='100']").set("name", "Market Square")
finish(path, tree)

path, tree = build("metadata_bare")
root = tree.getroot()
header = root.find("header")
header.attrib.clear()
header.attrib.update({
    "revMajor": "one", "revMinor": "1.5", "north": "NaN",
    "south": "-1", "east": "2", "west": "-3",
})
for road in root.findall("road"):
    road.attrib.pop("name", None)
root.find("junction[@id='100']").attrib.pop("name", None)
finish(path, tree)
