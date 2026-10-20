# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate junction_objects.xodr: objects placed along a junction's
reference line.

Junction 100 is a flat tee of roads 1, 2 and 3, with connecting roads from
1000. Its reference line is a 40 m <line> from (-20, -10) heading 0.3 rad.
Its <objects> hold:

- "Kerbstone": a 2 x 1 x 0.2 m box at s = 10, t = 2, raised 0.1 m and
  turned 0.2 rad.
- "Pad": an outline in road coordinates, s = 15 to 18 and t = -1 to 1,
  0.15 m tall.
- "Bollard": a post repeated every 5 m over 10 m from s = 20 at t = -3.
- an <objectReference> at s = 30, t = 0 to road 1's "Post".

Connecting road 1000 carries a "Cone" of its own, and a road with an empty
id in junction 100 a "Lost" pole. Junction 300's reference
line is an arc of radius 100 m from (100, 0) heading +X, which the spec does
not allow, with a "Marker" 20 m along it. Junction 200 has
<objects> but no <planView>, so they have nothing to stand
along.

scenariogeneration writes no junction <planView> or <objects>, so they are
spliced into its XML. The header date is fixed, so the file is the same each
run.
"""

import math
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

odr = xodr.OpenDrive("junction_objects")
creator = xodr.CommonJunctionCreator(100, "tee", startnum=1000)
for k, angle in enumerate([0, math.pi / 2, math.pi]):
    arm = xodr.create_road(xodr.Line(30), 1 + k, 1, 1, lane_width=3)
    creator.add_incoming_road_circular_geometry(arm, 12, angle, "successor")
    odr.add_road(arm)
creator.add_connection(1, 2, [-1], [1])
creator.add_connection(2, 3, [-1], [1])
odr.add_junction_creator(creator)
odr.roads["1"].add_object(xodr.Object(s=10, t=-4, Type=xodr.ObjectType.pole, id="post",
                                      name="Post", radius=0.1, height=1))
odr.adjust_roads_and_lanes()

path = Path(__file__).with_name("junction_objects.xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")


def frag(text):
    return ET.fromstring(text)


junction = root.find("junction[@id='100']")
junction.insert(0, frag(
    '<planView><geometry s="0" x="-20" y="-10" hdg="0.3" length="40"><line/></geometry>'
    '</planView>'
))
junction.append(frag(
    '<objects>'
    '<object id="j1" name="Kerbstone" type="obstacle" s="10" t="2" zOffset="0.1" hdg="0.2" '
    'length="2" width="1" height="0.2"/>'
    '<object id="j2" name="Pad" type="patch" s="16.5" t="0" zOffset="0">'
    '<outlines><outline id="0" closed="true">'
    '<cornerRoad s="15" t="-1" dz="0" height="0.15"/><cornerRoad s="18" t="-1" dz="0" height="0.15"/>'
    '<cornerRoad s="18" t="1" dz="0" height="0.15"/><cornerRoad s="15" t="1" dz="0" height="0.15"/>'
    '</outline></outlines></object>'
    '<object id="j3" name="Bollard" type="pole" s="20" t="-3" zOffset="0" radius="0.1" height="1">'
    '<repeat s="20" length="10" distance="5" tStart="-3" tEnd="-3" zOffsetStart="0" zOffsetEnd="0"/>'
    '</object>'
    '<objectReference id="post" s="30" t="0" zOffset="0" orientation="none"/>'
    '</objects>'
))
connecting = root.find("road[@id='1000']")
connecting.append(frag(
    '<objects><object id="cone" name="Cone" type="obstacle" s="2" t="0" zOffset="0" '
    'radius="0.2" height="0.5"/></objects>'
))
root.append(frag(
    '<road id="" junction="100" length="10"><planView>'
    '<geometry s="0" x="0" y="100" hdg="0" length="10"><line/></geometry></planView>'
    '<lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>'
    '<lane id="-1" type="driving"><width sOffset="0" a="3" b="0" c="0" d="0"/></lane>'
    '</right></laneSection></lanes><objects>'
    '<object id="lost" name="Lost" type="pole" s="5" t="-1.5" zOffset="0" radius="0.1" height="1"/>'
    '</objects></road>'
))
root.append(frag(
    '<junction id="300" name="curved"><planView>'
    '<geometry s="0" x="100" y="0" hdg="0" length="50"><arc curvature="0.01"/></geometry>'
    '</planView><objects>'
    '<object id="j5" name="Marker" type="pole" s="20" t="0" zOffset="0" radius="0.1" height="1"/>'
    '</objects></junction>'
))
root.append(frag(
    '<junction id="200" name="lineless"><objects>'
    '<object id="j4" name="Stray" type="pole" s="1" t="0" zOffset="0" radius="0.1" height="1"/>'
    '</objects></junction>'
))

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
