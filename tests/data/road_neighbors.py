# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate road_neighbors.xodr: roads that name the road beside them.

Roads 1, 2 and 3 are flat 50 m straights, each with a 3.5 m driving lane
-1. Road 1 heads +X from the origin. Road 2 heads +X from (0, 5), on road
1's left, and road 3 heads -X from (50, -5), on road 1's right, running
the other way. Road 1's <link> names road 2 on its left in the same
direction, and road 3 on its right in the opposite one. Road 2's names road
1 on its right, and a road 9 the file doesn't have. Road 3's names road 1
with a side of "up", which the element doesn't allow.

<neighbor> is from OpenDRIVE 1.4. 1.9 no longer has it.
"""

from pathlib import Path


def neighbor(side, id, direction):
    return f'<neighbor side="{side}" elementId="{id}" direction="{direction}"/>'


def road(id, x, y, hdg, neighbors):
    return (
        f'  <road name="" length="50" id="{id}" junction="-1">\n'
        f"    <link>{''.join(neighbors)}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="{hdg}" length="50"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>'
        '<lane id="-1" type="driving" level="false"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>'
        "</right></laneSection></lanes>\n"
        "  </road>\n"
    )


roads = [
    road(1, 0, 0, 0, [neighbor("left", 2, "same"), neighbor("right", 3, "opposite")]),
    road(2, 0, 5, 0, [neighbor("right", 1, "same"), neighbor("left", 9, "same")]),
    road(3, 50, -5, 3.141592653589793, [neighbor("up", 1, "opposite")]),
]

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="4" name="road_neighbors"/>\n'
    + "".join(roads)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
