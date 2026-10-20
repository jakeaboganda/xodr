# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate lateral_shapes.xodr: roads with a lateral <shape> across them.

scenariogeneration writes no lateral shapes, so this writes the XML itself.
Every road is a flat 60 m straight heading +X, with a solid mark on every
lane and on the center lane. Lanes 1 and -1 are 3.5 m driving lanes.

Road 0, from the origin, adds driving lanes 2 and -2, 3.5 m each, and a
crown that breaks on the center line: it falls 2.5 % either way, so the
road's edges at t = +-7 m stand 0.175 m below its middle.

Road 1, from (0, 30), has the same lanes and a parabolic crown,
h = -k t^2 with k = 0.175 / 49, at s = 0. At s = 50 the profile is flat, so
the crown flattens linearly between them and stays flat after. A pole
stands in the middle of lane -1 at s = 10, and a sign over the middle of
lane 1.

Road 2, from (0, 60), is road 0 banked at 0.05 rad.

Road 3, from (0, 90), has a 2 m sidewalk -2, raised 0.12 m, outside lane
-1, and the crown of road 0 out to its edge at t = -5.5 m.

Road 4, from (0, 120), has one shape at t = -2 m, 0.1 m high and flat, so
its profile starts inside the road's right edge at t = -3.5 m.

Road 5, from (0, 150), has a crossfall of 30 %, rising from 0 at its right
edge at t = -3.5 m to 2.1 m at its left edge.
"""

from pathlib import Path

MARK = '<roadMark sOffset="0" type="solid" weight="standard" color="standard" width="0.12"/>'
K = 0.175 / 49


def lane(id, width, extra="", kind="driving"):
    return (
        f'<lane id="{id}" type="{kind}" level="false">'
        f'<width sOffset="0" a="{width}" b="0" c="0" d="0"/>{extra}{MARK}</lane>'
    )


def shape(s, t, a, b=0.0, c=0.0, d=0.0):
    return f'<shape s="{s}" t="{t}" a="{a}" b="{b}" c="{c}" d="{d}"/>'


def crown(edge):
    """Falls 2.5 % from the center line out to t = -edge and on to the left."""
    return [shape(0, -edge, -0.025 * edge, 0.025), shape(0, 0, 0, -0.025)]


def road(id, y, left, right, lateral, extra=""):
    return (
        f'  <road name="" length="60" id="{id}" junction="-1">\n'
        f'    <planView><geometry s="0" x="0" y="{y}" hdg="0" length="60"><line/></geometry></planView>\n'
        f"    <lateralProfile>{''.join(lateral)}</lateralProfile>\n"
        '    <lanes><laneSection s="0">'
        f'<left>{"".join(left)}</left>'
        f'<center><lane id="0" type="none">{MARK}</lane></center>'
        f'<right>{"".join(right)}</right>'
        "</laneSection></lanes>\n"
        f"{extra}"
        "  </road>\n"
    )


FOUR = ([lane(1, 3.5), lane(2, 3.5)], [lane(-1, 3.5), lane(-2, 3.5)])
PARABOLA = [shape(0, -7, -K * 49, 14 * K, -K), shape(50, -7, 0)]
POLE = (
    '    <objects><object id="1" name="Pole" type="pole" s="10" t="-1.75" zOffset="0"'
    ' hdg="0" radius="0.05" height="2"/></objects>\n'
)
SIGN = (
    '    <signals><signal id="2" name="Sign" s="10" t="1.75" zOffset="2" orientation="-"'
    ' dynamic="no" country="DE" type="274" subtype="55" value="50" unit="km/h"'
    ' height="0.6" width="0.6"/></signals>\n'
)
SIDEWALK = lane(-2, 2, '<height sOffset="0" inner="0.12" outer="0.12"/>', kind="sidewalk")

roads = [
    road(0, 0, *FOUR, crown(7)),
    road(1, 30, *FOUR, PARABOLA, POLE + SIGN),
    road(2, 60, *FOUR, ['<superelevation s="0" a="0.05" b="0" c="0" d="0"/>'] + crown(7)),
    road(3, 90, [lane(1, 3.5)], [lane(-1, 3.5), SIDEWALK], crown(5.5)),
    road(4, 120, [lane(1, 3.5)], [lane(-1, 3.5)], [shape(0, -2, 0.1)]),
    road(5, 150, [lane(1, 3.5)], [lane(-1, 3.5)], [shape(0, -3.5, 0, 0.3)]),
]

xml = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<!-- Written by lateral_shapes.py. -->\n"
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="9" name="lateral_shapes" version="1.00"/>\n'
    + "".join(roads)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xml)
