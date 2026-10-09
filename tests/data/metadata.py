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

Road 1 has three <type> records, with country "DE" from 0 m, "AT" from 50 m
and none from 80 m, and its lane -1 two road marks: a "thermoplastic" one
named "Edge Line" by its <type>, and from 50 m a "cold-plastic" one named
"Dashed". Its lane 1 mark has neither, and its center mark has a material
and a <type> with no name.

Road 2, an 80 m straight 40 m north with three lanes a side, has two lane
sections, from 0 and from 40 m. In the first the lanes carry `advisory`
(1 inner, 2 outer, 3 both, -1 "banana"), `dynamicLaneType` (3, and -2 as
"1"), `dynamicLaneDirection` (3) and `roadWorks` (-1), and its center lane
has `level="true"`. The second has the same lanes without -1's `roadWorks`,
and a center lane of `type="driving"`. Lane -3 says each as "false".

Road 1 also has objects beside it: boxes "a1" to "a3" that are invalidated
and temporary, one of each as "false" or "1", and neither; "m1", with a
material of roadMarkColor "yellow" and a second without; "w", a box with
four markings of weight bold, standard, "banana" and none; and "o1", with
four outlines: closed ones with a fillType "grass" and laneType "sidewalk",
and with "paint" and "banana", one with neither, and a hole in the first
with fillType "soil". "o2" has a deprecated single <outline> of fillType
"gravel".

Road 1 holds signal "sg", a static board of three signs: "p1" with every
attribute the schema allows on a sign, "p2" with a `hOffset` of 7 rad and
orientation "-", and "p3" with a type only. The tee's first lane link has an
`overlapZone` of 35.5, a second one on it, from 1 to -1, has 7.5, and the
second connection's has the unreadable "oops".

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
    lanes = xodr.create_road(xodr.Line(80), 2, 3, 3, lane_width=3)
    lanes.planview.set_start_point(0, 40, 0)
    odr.add_road(lanes)
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
    tree = ET.parse(path)
    sections = tree.getroot().find("road[@id='2']/lanes")
    first = sections.find("laneSection")
    second = ET.fromstring(ET.tostring(first, encoding="unicode"))
    second.set("s", "40")
    sections.append(second)
    return path, tree


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

road = root.find("road[@id='1']")
after_link = list(road).index(road.find("link")) + 1
for k, (s, kind, country) in enumerate([(0, "town", "DE"), (50, "rural", "AT"), (80, "motorway", None)]):
    element = ET.Element("type", {"s": str(s), "type": kind})
    if country:
        element.set("country", country)
    road.insert(after_link + k, element)
section = road.find("lanes/laneSection")
outer = section.find("right/lane[@id='-1']")
first = outer.find("roadMark")
first.set("material", "thermoplastic")
first.append(ET.Element("type", {"name": "Edge Line", "width": "0.2"}))
second = ET.fromstring(ET.tostring(first, encoding="unicode"))
second.set("sOffset", "50")
second.set("material", "cold-plastic")
second.find("type").set("name", "Dashed")
outer.insert(list(outer).index(first) + 1, second)
lanes = root.find("road[@id='2']/lanes")
one, two = lanes.findall("laneSection")
def lane(section, id):
    return section.find(f"*/lane[@id='{id}']")
lane(one, 1).set("advisory", "inner")
lane(one, 2).set("advisory", "outer")
lane(one, 3).attrib.update({"advisory": "both", "dynamicLaneType": "true", "dynamicLaneDirection": "true"})
lane(one, -1).attrib.update({"advisory": "banana", "roadWorks": "true"})
lane(one, -2).set("dynamicLaneType", "1")
lane(one, -3).attrib.update({"dynamicLaneType": "false", "dynamicLaneDirection": "false", "roadWorks": "false"})
one.find("center/lane").set("level", "true")
for id in (1, 2, 3, -1, -2, -3):
    for name in ("advisory", "dynamicLaneType", "dynamicLaneDirection", "roadWorks"):
        if name in lane(one, id).attrib:
            lane(two, id).set(name, lane(one, id).get(name))
lane(two, -1).attrib.pop("roadWorks")
two.find("center/lane").set("type", "driving")
BOX_AT = {"a1": 5, "a2": 10, "a3": 15, "m1": 20, "w": 25}


def box(id, **attributes):
    return ET.Element("object", {
        "id": id, "type": "barrier", "s": str(BOX_AT[id]), "t": "-10", "zOffset": "0",
        "hdg": "0", "length": "2", "width": "1", "height": "1", **attributes,
    })


def corners(s, t, size=4):
    return [
        ET.Element("cornerRoad", {"id": str(i), "s": str(s + ds), "t": str(t + dt),
                                  "dz": "0", "height": "1"})
        for i, (ds, dt) in enumerate([(0, 0), (size, 0), (size, -size), (0, -size)])
    ]


def outline(id, s, t, size=4, **attributes):
    element = ET.Element("outline", {"id": str(id), "closed": "true", **attributes})
    element.extend(corners(s, t, size))
    return element


objects = ET.Element("objects")
objects.append(box("a1", invalidated="true", temporary="true"))
objects.append(box("a2", invalidated="false", temporary="1"))
objects.append(box("a3"))
material = box("m1")
material.append(ET.Element("material", {"surface": "asphalt", "roadMarkColor": "yellow"}))
material.append(ET.Element("material", {"surface": "gravel"}))
objects.append(material)
paint = box("w")
markings = ET.SubElement(paint, "markings")
for side, weight in [("front", "bold"), ("left", "standard"), ("rear", "banana"), ("right", None)]:
    marking = ET.SubElement(markings, "marking", {
        "side": side, "color": "white", "width": "0.1", "zOffset": "0.01",
    })
    if weight:
        marking.set("weight", weight)
objects.append(paint)
outlined = ET.Element("object", {
    "id": "o1", "type": "vegetation", "s": "35", "t": "-10", "zOffset": "0", "hdg": "0",
})
outlines = ET.SubElement(outlined, "outlines")
outlines.append(outline(0, 35, -10, fillType="grass", laneType="sidewalk"))
hole = outline(1, 36, -11, 2, fillType="soil", outer="false")
outlines.append(hole)
outlines.append(outline(2, 45, -10, fillType="paint", laneType="banana"))
outlines.append(outline(3, 55, -10))
objects.append(outlined)
old = ET.Element("object", {
    "id": "o2", "type": "vegetation", "s": "65", "t": "-10", "zOffset": "0", "hdg": "0",
})
old.append(outline(0, 65, -10, fillType="gravel"))
objects.append(old)
lanes_at = list(road).index(road.find("lanes")) + 1
road.insert(lanes_at, objects)

signal = ET.Element("signal", {
    "id": "sg", "name": "board", "s": "30", "t": "-6", "zOffset": "2", "orientation": "+",
    "dynamic": "no", "country": "DE", "type": "1000", "subtype": "-1",
})
board = ET.SubElement(signal, "staticBoard", {"width": "1", "height": "1", "length": "0.1"})
board.append(ET.Element("sign", {
    "id": "p1", "name": "limit", "country": "DE", "countryRevision": "2017",
    "type": "274", "subtype": "50", "value": "50", "unit": "km/h",
    "orientation": "+", "dynamic": "yes", "invalidated": "true", "temporary": "1",
    "hOffset": "0.5", "pitch": "0.25", "roll": "-0.125",
    "length": "0.1", "width": "0.6", "height": "0.6", "v": "0", "z": "0",
}))
board.append(ET.Element("sign", {
    "id": "p2", "type": "274", "orientation": "-", "hOffset": "7", "v": "0", "z": "0",
}))
board.append(ET.Element("sign", {"type": "101", "v": "0", "z": "0"}))
signals = ET.Element("signals")
signals.append(signal)
road.insert(list(road).index(objects) + 1, signals)
links = root.findall("junction[@id='100']/connection/laneLink")
links[0].set("overlapZone", "35.5")
crossing = ET.Element("laneLink", {"from": "1", "to": "-1", "overlapZone": "7.5"})
root.find("junction[@id='100']/connection[@id='0']").append(crossing)
links[1].set("overlapZone", "oops")

centre = section.find("center/lane/roadMark")
centre.set("material", "paint")
centre.append(ET.Element("type", {"width": "0.2"}))
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
