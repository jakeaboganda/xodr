"""Generate inner_outlines.xodr: an object whose inner outlines are areas of
their own, beside plain holes.

    uv run -q --with scenariogeneration==0.16.6 tests/data/inner_outlines.py

Road 1 is a flat 60 m straight from the origin heading +X, with one left and
two right lanes, 3 m wide, so a road station `(s, t)` is the point `(s, t)`.
Object 1, "Island", at s = 20, t = -4, has outlines in road coordinates, 0.2
m tall:

- 0: the outer outline, grass, s = 10 to 30 and t = -2 to -6;
- 1: inner, cobble and `laneType="walking"`, s = 12 to 16 and t = -3 to -5,
  with a marking along its first edge and a kerb all round;
- 2: inner, with no fill or lane type, s = 18 to 20: a plain hole;
- 3: inner, concrete, s = 22 to 26;
- 4: inner, gravel, s = 40 to 42, which no outer outline encloses;
- 5: inner, with no fill, s = 45 to 46, which no outer outline encloses;
- 6: inner, `laneType="driving"` with no fill, s = 27 to 29.

scenariogeneration writes no inner outline with a lane type, markings or
borders, so the outlines are spliced into its XML. The header date is fixed,
so the file is the same each run.
"""

import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Line(60), id=1, left_lanes=1, right_lanes=2, lane_width=3)
road.planview.set_start_point(0, 0, 0)
road.add_object(xodr.Object(s=20, t=-4, Type=xodr.ObjectType.trafficIsland, id="1",
                            name="Island"))

odr = xodr.OpenDrive("inner_outlines")
odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_name("inner_outlines.xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
island = root.find("road/objects/object")
outlines = ET.SubElement(island, "outlines")


def rectangle(id, s0, s1, t0, t1, **attributes):
    outline = ET.SubElement(outlines, "outline", {"id": str(id), "closed": "true", **attributes})
    for corner, (s, t) in enumerate([(s0, t0), (s1, t0), (s1, t1), (s0, t1)]):
        ET.SubElement(outline, "cornerRoad", {
            "s": str(s), "t": str(t), "dz": "0", "height": "0.2", "id": str(10 * id + corner),
        })
    return outline


rectangle(0, 10, 30, -2, -6, fillType="grass")
walk = rectangle(1, 12, 16, -3, -5, outer="false", fillType="cobble", laneType="walking")
markings = ET.SubElement(walk, "markings")
marking = ET.SubElement(markings, "marking", {
    "color": "white", "width": "0.1", "zOffset": "0.01", "startOffset": "0",
    "stopOffset": "0", "lineLength": "1", "spaceLength": "0",
})
for corner in ["10", "11"]:
    ET.SubElement(marking, "cornerReference", {"id": corner})
rectangle(2, 18, 20, -3, -5, outer="false")
rectangle(3, 22, 26, -3, -5, outer="false", fillType="concrete")
rectangle(4, 40, 42, -3, -5, outer="false", fillType="gravel")
rectangle(5, 45, 46, -3, -5, outer="false")
rectangle(6, 27, 29, -3, -5, outer="false", laneType="driving")
ET.SubElement(island, "borders").append(ET.Element("border", {
    "width": "0.2", "type": "curb", "outlineId": "1", "useCompleteOutline": "true",
}))

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
