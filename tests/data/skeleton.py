"""Generate skeleton.xodr: objects described by <skeleton> polylines.

    uv run -q --with scenariogeneration==0.16.6 tests/data/skeleton.py

Road 1 is a 60 m straight from the origin heading +X, at elevation
`1 + 0.02 s`, with one left lane and two right lanes, 3 m wide. Its objects:

- 1, "Light": the spec's traffic light pole at s = 25, t = -4, a pole from
  the ground up 4 m, its radius 0.15 to 0.1, and an arm from 3 m up out
  over the road to t = 0, both in road coordinates.
- 2, "Tree": the spec's tree at s = 10, t = 5, turned 0.5 rad, a trunk in
  its own frame from (-0.2, 1, 1.12) up to 4.5 m.
- 3, "Stub": a polyline of one vertex, which is dropped, and a good one.
- 4, "Off": a cylinder whose one polyline has a vertex at s = 80, past the
  end of the road, so it keeps its cylinder.
- 6, "Cone": at s = 15, t = -6, a polyline in its own frame that starts
  with the same vertex twice, of radius 0, rises 2 m to radius 0.3, then
  1 m more to a radius of -0.2, which the schema does not allow.
- 7, "Lamp": at s = 5, t = -6, one polyline in its own frame that rises
  4 m and turns out 1.5 m at 30 degrees from u, rising 0.3 m more.
- 8, "Hook": at s = 20, t = 6, a polyline in its own frame that rises 2 m
  and doubles back down to 0.5 m, 10 cm along u, too sharp a bend to mitre.
- 5, "Post": a post in its own frame, repeated every 10 m over 20 m from
  s = 30 at t = 6.

An <objectReference> places the light again at s = 45, t = -5. Three signals
stand by the light: "ArmLight" hangs under the middle of its arm's last
stretch, at s = 25, t = -1.2 and 2.6 m up, over a metre from any vertex, and
"PoleSign" is on its pole, 2 m up and 0.6 m from its axis, which only its
0.15 m radius brings within reach. "Under" stands on the ground under the
arm, 0.5 m up, and "Lone" at s = 40, t = -9, by no pole.

scenariogeneration writes no <skeleton>, so the skeletons are spliced into
its XML. The header date is fixed, so the file is the same each run.
"""

import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Line(60), id=1, left_lanes=1, right_lanes=2, lane_width=3)
road.planview.set_start_point(0, 0, 0)
road.add_elevation(0, 1.0, 0.02, 0, 0)
road.add_object([
    xodr.Object(s=25, t=-4, Type=xodr.ObjectType.pole, subtype="trafficLight", id="1",
                name="Light", length=0.3, width=3, height=4),
    xodr.Object(s=10, t=5, Type=xodr.ObjectType.tree, id="2", name="Tree", hdg=0.5,
                length=6, width=6, height=12),
    xodr.Object(s=40, t=-5, Type=xodr.ObjectType.pole, id="3", name="Stub", radius=0.2,
                height=3),
    xodr.Object(s=55, t=-5, Type=xodr.ObjectType.pole, id="4", name="Off", radius=0.2,
                height=3),
])
post = xodr.Object(s=30, t=6, Type=xodr.ObjectType.pole, id="5", name="Post", radius=0.1,
                   height=1)
post.repeat(20, 10, sStart=30, tStart=6, tEnd=6, zOffsetStart=0, zOffsetEnd=0)
road.add_object(post)
road.add_object(xodr.Object(s=5, t=-6, Type=xodr.ObjectType.streetLamp, id="7", name="Lamp",
                            length=2, width=0.4, height=4.5))
road.add_object(xodr.Object(s=20, t=6, Type=xodr.ObjectType.obstacle, id="8", name="Hook",
                            length=0.5, width=0.5, height=2.2))
road.add_object(xodr.Object(s=15, t=-6, Type=xodr.ObjectType.obstacle, id="6", name="Cone",
                            radius=0.3, height=2))

odr = xodr.OpenDrive("skeleton")
odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_name("skeleton.xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
objects = {o.get("id"): o for o in root.iter("object")}


def skeleton(id, *polylines):
    node = ET.SubElement(objects[id], "skeleton")
    for k, vertices in enumerate(polylines, 1):
        polyline = ET.SubElement(node, "polyline", {"id": str(k)})
        for n, (tag, attributes) in enumerate(vertices, 1):
            ET.SubElement(polyline, tag, {"id": str(n), **{a: str(v) for a, v in attributes.items()}})


def road_vertex(s, t, dz, radius, ground=False):
    attributes = {"s": s, "t": t, "dz": dz, "radius": radius}
    if ground:
        attributes["intersectionPoint"] = "true"
    return ("vertexRoad", attributes)


def local_vertex(u, v, z, radius, ground=False):
    attributes = {"u": u, "v": v, "z": z, "radius": radius}
    if ground:
        attributes["intersectionPoint"] = "true"
    return ("vertexLocal", attributes)


skeleton(
    "1",
    [road_vertex(25, -4, 0, 0.15, ground=True), road_vertex(25, -4, 4, 0.1)],
    [road_vertex(25, -4, 3, 0.15), road_vertex(25, -2.5, 3.25, 0.15),
     road_vertex(25, 0, 3.25, 0.15)],
)
skeleton("2", [local_vertex(-0.2, 1, 1.12, 0.15, ground=True), local_vertex(-0.2, 1, 4.5, 0.12)])
skeleton(
    "3",
    [road_vertex(40, -5, 0, 0.2)],
    [road_vertex(40, -5, 0, 0.2, ground=True), road_vertex(40, -5, 3, 0.2)],
)
skeleton("4", [road_vertex(55, -5, 0, 0.2), road_vertex(80, -5, 3, 0.2)])
skeleton("5", [local_vertex(0, 0, 0, 0.1, ground=True), local_vertex(0, 0, 1, 0.05)])
skeleton("7", [local_vertex(0, 0, 0, 0.1, ground=True), local_vertex(0, 0, 4, 0.1),
               local_vertex(1.299, 0.75, 4.3, 0.1)])
skeleton("8", [local_vertex(0, 0, 0, 0.1), local_vertex(0, 0, 2, 0.1),
               local_vertex(0.1, 0, 0.5, 0.1)])
skeleton("6", [local_vertex(0, 0, 0, 0), local_vertex(0, 0, 0, 0), local_vertex(0, 0, 2, 0.3),
               local_vertex(0, 0, 3, -0.2)])

root.find("road/objects").append(ET.Element("objectReference", {
    "id": "1", "s": "45", "t": "-5", "zOffset": "0", "orientation": "none",
}))

signals = ET.SubElement(root.find("road"), "signals")
for id, name, s, t, z in [("s1", "ArmLight", 25, -1.2, 2.6), ("s2", "PoleSign", 25.6, -4, 2),
                          ("s4", "Under", 25, -1.2, 0.5),
                          ("s3", "Lone", 40, -9, 2)]:
    ET.SubElement(signals, "signal", {
        "id": id, "name": name, "s": str(s), "t": str(t), "zOffset": str(z),
        "orientation": "+", "dynamic": "no", "country": "DE", "type": "274",
        "subtype": "50", "value": "50", "unit": "km/h", "width": "0.4", "height": "0.4",
    })

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
