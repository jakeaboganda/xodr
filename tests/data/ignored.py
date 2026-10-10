"""Generate ignored.xodr: a file with every feature the crate ignores, in
counts that tell the uses apart, and the near misses that look like them.

    uv run -q --with scenariogeneration==0.16.6 tests/data/ignored.py

Road 1 holds a <lanes layer="temporary">, which the crate reads, then a
second permanent <lanes>, which it ignores, and a lane section with
singleSide="true", which the crate reads. Road 2 holds the objects: three
with perpToRoad and repeats with a cubic term or detached from the
reference line, which the crate reads, and an object with a <surface> and a
<skeleton>, and with a <curveLocal> outline and two outline <markings>,
which the crate reads. Road 3 holds a static board whose two signs each
carry a <validity>, a <dependency> and a <reference>, one validity of a
temporary layer. Road 4 uses each of those attributes at the value that
means nothing, and holds an object <userData>, which the crate reads. Roads
1 to 3 also hold <userData> and <include> in places the crate skips. Road 5
holds 500 <userData> on one lane.

Junction 100 is a tee of roads 11, 12 and 13, with an <objects>, two
<roadSection>s, boundary segments with and without a transitionLength, and
lane links that cross layers. The header holds a <license>, a
<dataQuality>, <defaultRegulations> and <userData>, and the file a
<vmsGroup>.

scenariogeneration writes none of these, so they are spliced into its
roads as XML. The header date is fixed, so the file is the same each run.
"""

import math
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr


def straight(id, y):
    road = xodr.create_road(xodr.Line(100), id, 1, 1, lane_width=3)
    road.planview.set_start_point(0, y, 0)
    return road


odr = xodr.OpenDrive("ignored")
for id, y in [(1, 0), (2, 20), (3, 40), (4, 60), (5, 80)]:
    odr.add_road(straight(id, y))

creator = xodr.CommonJunctionCreator(100, "tee", startnum=1000)
arms = []
for k, angle in enumerate([0, math.pi / 2, math.pi]):
    arm = xodr.create_road(xodr.Line(30), 11 + k, 1, 1, lane_width=3)
    arm.planview.set_start_point(
        1000 + 42 * math.cos(angle), 42 * math.sin(angle), angle + math.pi
    )
    creator.add_incoming_road_circular_geometry(arm, 12, angle, "successor")
    arms.append(arm)
    odr.add_road(arm)
creator.add_connection(11, 12, [-1], [1])
creator.add_connection(12, 13, [-1], [1])
odr.add_junction_creator(creator)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_suffix(".xodr")
odr.write_xml(str(path))


def frag(text):
    return ET.fromstring(text)


def add(parent, *texts):
    for text in texts:
        parent.append(frag(text))


tree = ET.parse(path)
root = tree.getroot()
header = root.find("header")
header.set("date", "2026-01-01T00:00:00")
road = {r.get("id"): r for r in root.findall("road")}
junction = root.find("junction")

add(
    header,
    '<license name="CC0" resource="https://example.org/license">terms</license>',
    '<dataQuality><error xyAbsolute="0.1" zAbsolute="0.1"/></dataQuality>',
    '<userData code="tool" value="ignored"/>',
    '<defaultRegulations><roadRegulations type="town"/></defaultRegulations>',
)
add(
    root,
    '<include file="other.xodr"/>',
    '<vmsGroup id="g" name="group"/>',
)

# Road 1: lane layers.
lanes = road["1"].find("lanes")
first = lanes.find("laneSection")
first.set("singleSide", "false")
second = frag(ET.tostring(first, encoding="unicode"))
second.set("s", "50")
second.set("singleSide", "true")
lanes.append(second)
link = second.find("right/lane/link")
add(link, '<predecessor id="-1" layer="temporary"/>', '<successor id="-1" layer="other"/>')
left = second.find("left/lane/link")
add(left, '<predecessor id="1" layer="temporary"/>')
permanent = first.find("right/lane/link")
add(permanent, '<predecessor id="-1" layer="permanent"/>')
temporary = frag(
    '<lanes layer="temporary"><laneSection s="0"><right>'
    '<lane id="-1" type="driving" level="false"><width sOffset="0" a="3"/></lane>'
    "</right></laneSection></lanes>"
)
road["1"].insert(list(road["1"]).index(lanes) + 1, temporary)
repeated = frag(
    '<lanes><laneSection s="0"><right>'
    '<lane id="-1" type="driving" level="false"><width sOffset="0" a="3"/></lane>'
    "</right></laneSection></lanes>"
)
road["1"].insert(list(road["1"]).index(lanes) + 2, repeated)
first.find("right/lane").append(frag('<userData code="a" value="1"/>'))
first.find("right/lane/roadMark").append(frag('<userData code="b" value="2"/>'))
add(road["1"], '<userData code="road" value="x"/>', '<include file="a.xodr"/>')
add(
    road["1"],
    '<signals><signal id="s1" s="10" t="-5" dynamic="no" orientation="+" zOffset="2" '
    'country="DE" type="274" subtype="50" value="50" name="s">'
    '<validity fromLane="-1" toLane="-1" layer="temporary"/>'
    '<validity fromLane="-1" toLane="-1" layer="permanent"/>'
    '</signal></signals>',
)

# Road 2: objects.
objects = frag("<objects/>")
add(
    objects,
    '<object id="o1" s="10" t="-5" type="barrier" name="a" perpToRoad="true" length="2" width="1" height="1"/>',
    '<object id="o2" s="20" t="-5" type="barrier" name="b" perpToRoad="1" length="2" width="1" height="1"/>',
    '<object id="o3" s="30" t="-5" type="barrier" name="c" perpToRoad="false" length="2" width="1" height="1"/>',
    '<object id="r1" s="40" t="-5" type="barrier" name="r" length="2" width="1" height="1">'
    '<repeat s="40" length="20" distance="5" tStart="-5" tEnd="-5" widthStart="1" widthEnd="1" '
    'heightStart="1" heightEnd="1" zOffsetStart="0" zOffsetEnd="0" bT="0.01" detachFromReferenceLine="true"/>'
    '<repeat s="62" length="10" distance="5" tStart="-5" tEnd="-5" widthStart="1" widthEnd="1" '
    'heightStart="1" heightEnd="1" zOffsetStart="0" zOffsetEnd="0" cT="0.1" dT="-0.1"/>'
    '</object>',
    '<object id="r2" s="75" t="-5" type="barrier" name="r2" length="2" width="1" height="1">'
    '<repeat s="75" length="10" distance="5" tStart="-5" tEnd="-5" widthStart="1" widthEnd="1" '
    'heightStart="1" heightEnd="1" zOffsetStart="0" zOffsetEnd="0" bT="0" cT="0" dT="0" '
    'detachFromReferenceLine="false"/></object>',
    '<object id="x" s="50" t="-8" type="building" name="x" length="4" width="4" height="3">'
    '<surface><CRG file="o.crg" sStart="50" sEnd="54" orientation="same" mode="attached" purpose="elevation"/></surface>'
    '<skeleton><polyline id="0"><vertex s="50" t="-8" dz="0" radius="0.1"/>'
    '<vertex s="52" t="-8" dz="1" radius="0.1"/></polyline></skeleton>'
    '<outlines>'
    '<outline id="0" closed="true"><cornerRoad s="50" t="-6" dz="0" height="1"/>'
    '<cornerRoad s="54" t="-6" dz="0" height="1"/><cornerRoad s="54" t="-10" dz="0" height="1"/>'
    '<markings><marking side="left" weight="standard" startOffset="0" stopOffset="0" '
    'spaceLength="0" lineLength="1" width="0.1" zOffset="0"><cornerReference id="0"/>'
    '<cornerReference id="1"/></marking></markings></outline>'
    '<outline id="1" closed="false"><curveLocal><line/></curveLocal>'
    '<markings><marking side="left" weight="standard" startOffset="0" stopOffset="0" '
    'spaceLength="0" lineLength="1" width="0.1" zOffset="0"><cornerReference id="0"/>'
    '</marking></markings></outline>'
    '</outlines>'
    '<userData code="read" value="by the crate"/></object>',
)
add(objects.find("object[@id='o1']"), '<userData code="k" value="v"/>')
road["2"].append(objects)
add(road["2"], '<userData code="road" value="y"/>')

# Road 3: a static board with two signs, the second with a validity of a
# temporary layer, which counts as a sign's validity once and not as a layer.
sign = (
    '<sign id="{n}" s="0" t="0" zOffset="0" hOffset="0" pitch="0" roll="0" orientation="+" '
    'length="0.5" height="0.5" width="0.1" country="DE" type="274" subtype="50" value="50" name="n" '
    'countryRevision="2017" dynamic="no" temporary="false" invalidated="false">'
    '<validity fromLane="-1" toLane="-1"{layer}/>'
    '<dependency id="s3" type="x"/>'
    '<reference elementType="signal" elementId="s3" type="x"/></sign>'
)
add(
    road["3"],
    '<signals><signal id="s3" s="10" t="-5" dynamic="no" orientation="+" zOffset="2" '
    'country="DE" type="1000" subtype="-1" name="board">'
    '<staticBoard width="1" height="1" length="0.1" v="0" z="0" pitch="0" roll="0">'
    + sign.format(n="a", layer="")
    + sign.format(n="b", layer=' layer="temporary"')
    + "</staticBoard><userData code='s' value='t'/></signal></signals>",
)

# Road 4: every attribute at the value that means nothing.
lanes4 = road["4"].find("lanes")
lanes4.set("layer", "permanent")
lanes4.find("laneSection").set("singleSide", "false")
add(
    road["4"],
    '<objects>'
    '<object id="q1" s="10" t="-5" type="barrier" name="q" perpToRoad="false" length="2" width="1" height="1">'
    '<userData code="read" value="by the crate"/>'
    '<repeat s="10" length="10" distance="5" tStart="-5" tEnd="-5" widthStart="1" widthEnd="1" '
    'heightStart="1" heightEnd="1" zOffsetStart="0" zOffsetEnd="0" bT="0" cT="0.0" dT="0" '
    'detachFromReferenceLine="false"/></object></objects>',
    '<signals><signal id="q2" s="30" t="-5" dynamic="no" orientation="+" zOffset="2" '
    'country="DE" type="274" subtype="50" value="50" name="q">'
    '<validity fromLane="-1" toLane="-1" layer="permanent"/></signal></signals>',
)

# Road 5: 500 <userData> on one lane, which raise one warning. The last holds
# vendor elements, which are not the file's own and count for nothing.
crowd = road["5"].find("lanes/laneSection/right/lane")
for k in range(500):
    crowd.append(frag(f'<userData code="k{k}" value="{k}"/>'))
crowd[-1].extend([
    frag('<userData code="inner"/>'),
    frag('<validity fromLane="-1" toLane="-1" layer="temporary"/>'),
    frag('<include file="vendor.xodr"/>'),
])

# Junction 100.
add(
    junction,
    '<objects><object id="j1" s="0" t="0" type="barrier" length="1" width="1" height="1"/></objects>',
    '<roadSection roadId="11" sStart="0" sEnd="30"/>',
    '<roadSection roadId="12" sStart="0" sEnd="30"/>',
    '<boundary><segment type="joint" roadId="11" contactPoint="end" jointLaneStart="1" '
    'jointLaneEnd="-1" transitionLength="2.5"/>'
    '<segment type="joint" roadId="12" contactPoint="end" jointLaneStart="1" jointLaneEnd="-1" '
    'transitionLength="0"/>'
    '<segment type="joint" roadId="13" contactPoint="end" jointLaneStart="1" jointLaneEnd="-1" '
    'transitionLength="1"/>'
    '<segment type="joint" roadId="11" contactPoint="end" jointLaneStart="1" jointLaneEnd="-1"/>'
    '</boundary>',
)
links = junction.findall("connection/laneLink")
links[0].set("fromLayer", "temporary")
links[1].set("toLayer", "temporary")
links[1].set("fromLayer", "permanent")

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
