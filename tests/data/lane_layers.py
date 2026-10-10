"""Generate lane_layers.xodr: roads with a temporary lane layer over a
permanent one.

    uv run -q --with scenariogeneration==0.16.6 tests/data/lane_layers.py

Road 1 is a 200 m straight with a left lane 1 and right lanes -1 and -2,
3.5 m wide. Its permanent layer has sections from 0, 40 and 160 m. Its
temporary layer has lanes -1 and -2, 2.5 m wide, shifted 1.5 m right by
their own <laneOffset>, in two sections: from 40 m, and from 100 m with a
`length` of 60 m, so it ends at 160 m and not at the end of the road.

The links join the layers:

- permanent section 0 leads on to permanent section 1 and, for lanes -1 and
  -2, into temporary section 0, each `<successor>` with its `layer`;
- the temporary sections lead into each other, lane -1 saying
  `layer="temporary"` and lane -2 saying nothing, and into the last
  permanent section, which names both of its predecessors.

Road 2 is road 1, 60 m to the north, with its temporary `<lanes>` listed
first in the file. Road 1 carries four signals at 70 m, where both layers
have lanes: one with no <validity>, one for lanes -1 to -2 of no layer, one
for the same lanes of the temporary layer, and one for both.

Road 3 is a 100 m straight with one right lane. Its temporary layer is a lane
of 3 m on a <laneOffset> of -2.5, so it reaches 1.5 m past the permanent
surface, for its first 50 m by a `length`. A second temporary <lanes> follows
it, which the crate leaves out, and the permanent lane's link names the layer
"bogus", which reads as permanent.

Roads 6 and 7 are 100 m straights. Road 6 has two right lanes, and a
`restricted` temporary lane of 3.5 m over lane -1 from 20 to 60 m. Road 7 has
one right lane, and a temporary lane of type `roadWorks` over it likewise.

Roads 4 and 5 are 100 m straights end to end with one right lane. Road 4's
temporary layer is its last 40 m, and road 5's the 40 m from 40 m, so a
temporary lane leaving road 4 has no temporary lane to enter at the start of
road 5.

Junction 9 is a tee of roads 11, 12 and 13, with connecting roads from 1000.
Arm 11 and connecting road 1000 each have a temporary layer with a lane -1.
The connection from 11 to 1000 links the permanent lanes, and a second link
from -1 to -1 with `fromLayer` and `toLayer` "temporary" the temporary ones.

scenariogeneration writes no temporary layers, links between them or
validities, so they are spliced into its XML. The header date is fixed, so
the file is the same each run.
"""

import math
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr


def frag(text):
    return ET.fromstring(text)


odr = xodr.OpenDrive("lane_layers")
for road_id, y in [(1, 0), (2, 60)]:
    road = xodr.create_road(xodr.Line(200), road_id, 1, 2, lane_width=3.5)
    road.planview.set_start_point(0, y, 0)
    odr.add_road(road)

# Road 3: a temporary lane partly outside the permanent surface, from 0 to
# 50 m of 100, and a second temporary <lanes>, which is left out.
road3 = xodr.create_road(xodr.Line(100), 3, 0, 1, lane_width=3.5)
road3.planview.set_start_point(0, 120, 0)
odr.add_road(road3)

# Roads 6 and 7: a temporary lane traffic may not use over a driving lane.
# Road 6 has two driving lanes, and a restricted temporary lane over lane -1
# from 20 to 60 m. Road 7 has one, and a lane closed for works over it.
for road_id, y, lanes in [(6, 240, 2), (7, 300, 1)]:
    road = xodr.create_road(xodr.Line(100), road_id, 0, lanes, lane_width=3.5)
    road.planview.set_start_point(0, y, 0)
    odr.add_road(road)

# Roads 4 and 5 join end to end. Road 4's temporary layer reaches its end,
# and road 5's lies in the middle, away from where the link enters.
chain = {}
for road_id, x in [(4, 0), (5, 100)]:
    chain[road_id] = xodr.create_road(xodr.Line(100), road_id, 0, 1, lane_width=3.5)
    chain[road_id].planview.set_start_point(x, 180, 0)
    odr.add_road(chain[road_id])
chain[4].add_successor(xodr.ElementType.road, 5, xodr.ContactPoint.start)
chain[5].add_predecessor(xodr.ElementType.road, 4, xodr.ContactPoint.end)

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


def link(*entries):
    """A <link> of (tag, id, layer) entries, `layer` None to leave it out."""
    element = ET.Element("link")
    for tag, id, layer in entries:
        attributes = {"id": str(id)}
        if layer:
            attributes["layer"] = layer
        ET.SubElement(element, tag, attributes)
    return element


def set_link(section, lane_id, *entries):
    lane = section.find(f"*/lane[@id='{lane_id}']")
    old = lane.find("link")
    index = list(lane).index(old) if old is not None else 0
    if old is not None:
        lane.remove(old)
    lane.insert(index, link(*entries))


def permanent_sections(road, starts):
    lanes = road.find("lanes")
    first = lanes.find("laneSection")
    sections = [first]
    for s in starts[1:]:
        copy = frag(ET.tostring(first, encoding="unicode"))
        copy.set("s", str(s))
        lanes.append(copy)
        sections.append(copy)
    return lanes, sections


def temporary_lanes(sections, offset=-1.5, kind="driving"):
    """A <lanes layer="temporary"> of sections [(s, length or None)]."""
    element = frag(
        '<lanes layer="temporary">'
        f'<laneOffset s="0" a="{offset}" b="0" c="0" d="0"/></lanes>'
    )
    out = []
    for s, length in sections:
        section = ET.SubElement(element, "laneSection", {"s": str(s)})
        if length:
            section.set("length", str(length))
        section.append(frag(
            '<center><lane id="0" type="none" level="false">'
            '<roadMark sOffset="0" type="solid" weight="standard" color="yellow" width="0.15"/>'
            "</lane></center>"
        ))
        right = ET.SubElement(section, "right")
        for lane_id in (-1, -2):
            right.append(frag(
                f'<lane id="{lane_id}" type="{kind}" level="false"><link/>'
                '<width sOffset="0" a="2.5" b="0" c="0" d="0"/>'
                '<roadMark sOffset="0" type="solid" weight="standard" color="yellow" width="0.15"/>'
                "</lane>"
            ))
        out.append(section)
    return element, out


def layered_road(road, place_temporary_first):
    lanes, (p0, p1, p2) = permanent_sections(road, [0, 40, 160])
    temporary, (t0, t1) = temporary_lanes([(40, None), (100, 60)])
    index = list(road).index(lanes)
    road.insert(index if place_temporary_first else index + 1, temporary)
    for lane_id in (-1, -2):
        set_link(p0, lane_id, ("successor", lane_id, None), ("successor", lane_id, "temporary"))
        set_link(p1, lane_id, ("predecessor", lane_id, None), ("successor", lane_id, None))
        set_link(p2, lane_id, ("predecessor", lane_id, None), ("predecessor", lane_id, "temporary"))
        set_link(t0, lane_id, ("predecessor", lane_id, "permanent"),
                 ("successor", lane_id, "temporary" if lane_id == -1 else None))
        set_link(t1, lane_id, ("predecessor", lane_id, "temporary" if lane_id == -1 else None),
                 ("successor", lane_id, "permanent"))
    set_link(p0, 1, ("successor", 1, None))
    set_link(p1, 1, ("predecessor", 1, None), ("successor", 1, None))
    set_link(p2, 1, ("predecessor", 1, None))
    return lanes


layered_road(roads["1"], place_temporary_first=False)
layered_road(roads["2"], place_temporary_first=True)


def signal(id, *validities, s=70):
    element = ET.Element("signal", {
        "id": id, "s": str(s), "t": "-9", "orientation": "none", "dynamic": "no",
        "zOffset": "2", "country": "DE", "type": "274", "subtype": "50", "name": id,
    })
    for lane_from, lane_to, layer in validities:
        attributes = {"fromLane": str(lane_from), "toLane": str(lane_to)}
        if layer:
            attributes["layer"] = layer
        ET.SubElement(element, "validity", attributes)
    return element


signals = ET.SubElement(roads["1"], "signals")
signals.append(signal("any"))
signals.append(signal("permanent", (-2, -1, None)))
signals.append(signal("temporary", (-2, -1, "temporary")))
signals.append(signal("both", (-1, -1, None), (-1, -1, "temporary")))
signals.append(signal("where the temporary layer ends", s=160))

# Road 3.
lanes3 = roads["3"].find("lanes")
lanes3.find("laneSection/right/lane[@id='-1']/link").append(
    ET.Element("successor", {"id": "-1", "layer": "bogus"})
)
first, _ = temporary_lanes([(0, 50)], offset=-2.5)
first.find("laneSection/right").remove(first.find("laneSection/right/lane[@id='-2']"))
first.find("laneSection/right/lane/width").set("a", "3")
second, _ = temporary_lanes([(10, None)], offset=-6)
roads["3"].insert(list(roads["3"]).index(lanes3) + 1, first)
roads["3"].insert(list(roads["3"]).index(lanes3) + 2, second)

# Roads 4 and 5.
for road_id, sections in [("4", [(60, None)]), ("5", [(40, 40)])]:
    lanes = roads[road_id].find("lanes")
    temporary, (section, *_) = temporary_lanes(sections, offset=0)
    right = section.find("right")
    right.remove(right.find("lane[@id='-2']"))
    roads[road_id].insert(list(roads[road_id]).index(lanes) + 1, temporary)
set_link(roads["4"].find("lanes/laneSection"), -1, ("successor", -1, None))
set_link(roads["4"].find("lanes[@layer='temporary']/laneSection"), -1,
         ("predecessor", -1, "permanent"), ("successor", -1, "temporary"))
set_link(roads["5"].find("lanes/laneSection"), -1, ("predecessor", -1, None))

# Roads 6 and 7.
for road_id, kind in [("6", "restricted"), ("7", "roadWorks")]:
    lanes = roads[road_id].find("lanes")
    temporary, (section, *_) = temporary_lanes([(20, 40)], offset=0, kind=kind)
    right = section.find("right")
    right.remove(right.find("lane[@id='-2']"))
    right.find("lane/width").set("a", "3.5")
    roads[road_id].insert(list(roads[road_id]).index(lanes) + 1, temporary)

# Junction 9: arm 11 and connecting road 1000 get a temporary lane -1.
for road_id, length in [("11", 30), ("1000", None)]:
    lanes = roads[road_id].find("lanes")
    section = frag(
        '<laneSection s="0"><center><lane id="0" type="none" level="false"/></center>'
        '<right><lane id="-1" type="driving" level="false"><link/>'
        '<width sOffset="0" a="2.5" b="0" c="0" d="0"/></lane></right></laneSection>'
    )
    temporary = ET.Element("lanes", {"layer": "temporary"})
    temporary.append(section)
    roads[road_id].insert(list(roads[road_id]).index(lanes) + 1, temporary)
connection = root.find("junction[@id='9']/connection[@id='0']")
ET.SubElement(connection, "laneLink", {
    "from": "-1", "to": "-1", "fromLayer": "temporary", "toLayer": "temporary",
})

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
