"""Generate single_side.xodr: lane sections that give one side of the road.

    uv run -q --with scenariogeneration==0.16.6 tests/data/single_side.py

Road 1 is a 200 m straight of four lane sections, 3 m wide lanes unless
said:

- from 0 m, both sides: left lanes 1 and 2, and right lanes -1, -2 and -3.
  Lane -1 widens 2 cm a metre from 3 m, rises from 0 to 0.2 m over its
  `<height>`s from 0 to 100 m, and has a solid mark and, from 70 m, a broken
  one. Lane -3 has no width, so the crate drops it;
- from 50 m, `singleSide="true"`, left lanes only, 4 m wide;
- from 120 m, `singleSide="true"`, right lanes only, 3.5 m wide;
- from 160 m, both sides.

So the right side runs on under the second section, as it stood where it
began, and the left under the third. Lanes link to the next section that
gave their side, as a file written for `singleSide` links them.

Road 2 is 100 m. Its first section gives the right lane only and its second
the left lane only, both `singleSide="true"`. Road 3 is the same with
`singleSide="false"`, so nothing is carried on.

scenariogeneration writes no `singleSide`, so the lanes of its roads are
replaced with the sections above, as XML. The header date is fixed, so the
file is the same each run.
"""

import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr


def lane(id, a, b=0.0, links="", extra=""):
    width = f'<width sOffset="0" a="{a}" b="{b}" c="0" d="0"/>' if a is not None else ""
    return (
        f'<lane id="{id}" type="driving" level="false">{links}{width}{extra}</lane>'
    )


def link(*entries):
    inner = "".join(f'<{tag} id="{id}"/>' for tag, id in entries)
    return f"<link>{inner}</link>"


MARKS = (
    '<roadMark sOffset="0" type="solid" weight="standard" color="standard" width="0.12"/>'
    '<roadMark sOffset="70" type="broken" weight="standard" color="standard" width="0.12"/>'
)
HEIGHTS = (
    '<height sOffset="0" inner="0" outer="0"/><height sOffset="100" inner="0.2" outer="0.2"/>'
)


def section(s, left, right, single=None):
    attributes = f' singleSide="{single}"' if single else ""
    parts = f'<laneSection s="{s}"{attributes}>'
    if left is not None:
        parts += f"<left>{left}</left>"
    parts += '<center><lane id="0" type="none" level="false"/></center>'
    if right is not None:
        parts += f"<right>{right}</right>"
    return parts + "</laneSection>"


road_1 = "".join([
    section(
        0,
        lane(2, 2.5, 0.01, link(("successor", 2))) + lane(1, 3, 0, link(("successor", 1))),
        lane(-1, 3, 0.02, link(("successor", -1)), MARKS + HEIGHTS)
        + lane(-2, 3, 0, link(("successor", -2)))
        + lane(-3, None),
    ),
    section(
        50,
        lane(2, 4, 0, link(("predecessor", 2), ("successor", 2)))
        + lane(1, 4, 0, link(("predecessor", 1), ("successor", 1))),
        None,
        "true",
    ),
    section(
        120,
        None,
        lane(-1, 3.5, 0, link(("predecessor", -1), ("successor", -1)))
        + lane(-2, 3, 0, link(("predecessor", -2), ("successor", -2))),
        "true",
    ),
    section(
        160,
        lane(2, 3, 0, link(("predecessor", 2))) + lane(1, 3, 0, link(("predecessor", 1))),
        lane(-1, 3, 0, link(("predecessor", -1))) + lane(-2, 3, 0, link(("predecessor", -2))),
    ),
])


def two(single):
    return section(0, None, lane(-1, 3), single) + section(50, lane(1, 3), None, single)


odr = xodr.OpenDrive("single_side")
for road_id, length, y in [(1, 200, 0), (2, 100, 40), (3, 100, 80)]:
    road = xodr.create_road(xodr.Line(length), road_id, 1, 1, lane_width=3)
    road.planview.set_start_point(0, y, 0)
    odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_suffix(".xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
for road_id, sections in [("1", road_1), ("2", two("true")), ("3", two("false"))]:
    road = root.find(f"road[@id='{road_id}']")
    old = road.find("lanes")
    new = ET.fromstring(f"<lanes>{sections}</lanes>")
    road.insert(list(road).index(old), new)
    road.remove(old)
ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
