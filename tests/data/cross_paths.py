# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate cross_paths.xodr: a pedestrian crossing over a road, as a cross path.

    uv run tests/data/cross_paths.py

Road 1 runs 50 m along +X from the origin, with 3.5 m driving lanes 1 and
-1 and 2 m sidewalks 2 and -2. Road 20, in junction 5, crosses it at x =
25, from the middle of sidewalk 2 at (25, 4.5) to the middle of sidewalk -2
at (25, -4.5), heading -Y, with a 2 m walking lane -1.

Road 20's reference line runs along x = 26, so its lane is centred on
x = 25. Junction 5's cross path 0 runs along road 20 from sidewalk 2 at s = 25 to
sidewalk -2 at s = 25. Its cross path 1 names s = 80 on road 1, which is
only 50 m long.
"""

from pathlib import Path


def lane(id, kind, width):
    return (
        f'<lane id="{id}" type="{kind}" level="false">'
        f'<width sOffset="0" a="{width}" b="0" c="0" d="0"/></lane>'
    )


ROAD_1 = (
    '  <road name="" length="50" id="1" junction="-1">\n'
    '    <planView><geometry s="0" x="0" y="0" hdg="0" length="50"><line/></geometry></planView>\n'
    '    <lanes><laneSection s="0">'
    f'<left>{lane(1, "driving", 3.5)}{lane(2, "sidewalk", 2)}</left>'
    '<center><lane id="0" type="none"/></center>'
    f'<right>{lane(-1, "driving", 3.5)}{lane(-2, "sidewalk", 2)}</right>'
    "</laneSection></lanes>\n"
    "  </road>\n"
)
ROAD_20 = (
    '  <road name="" length="9" id="20" junction="5">\n'
    '    <planView><geometry s="0" x="26" y="4.5" hdg="-1.5707963267948966" length="9"><line/></geometry></planView>\n'
    '    <lanes><laneSection s="0">'
    '<center><lane id="0" type="none"/></center>'
    f'<right>{lane(-1, "walking", 2)}</right>'
    "</laneSection></lanes>\n"
    "  </road>\n"
)


def cross_path(id, s):
    return (
        f'    <crossPath id="{id}" crossingRoad="20" roadAtStart="1" roadAtEnd="1">'
        f'<startLaneLink s="{s}" from="2" to="-1"/><endLaneLink s="{s}" from="-2" to="-1"/>'
        "</crossPath>\n"
    )


xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="cross_paths"/>\n'
    + ROAD_1
    + ROAD_20
    + '  <junction id="5">\n'
    + cross_path(0, 25)
    + cross_path(1, 80)
    + "  </junction>\n"
    "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
