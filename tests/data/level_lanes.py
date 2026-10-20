# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate level_lanes.xodr: lanes with level="true" beside tilted roads.

    uv run tests/data/level_lanes.py

Every road is a 50 m straight heading +X, on flat ground, with one lane
section.

Road 0, from the origin, has a superelevation of 0.05 rad. On the right,
lane -1 is a 3.5 m driving lane, lane -2 a 2 m level shoulder, and lane
-3 a 2 m level sidewalk with a <height> of 0.12 m. On the left, lane 1 is
a 3.5 m driving lane and lane 2 a 2 m level shoulder. A pole stands on
lane -2 at s = 25.

Road 1, from (0, 30), has a <shape> falling 2.5 % to the right from 0 at
the reference line, and no superelevation. Lane -1 is a 3.5 m driving
lane and lane -2 a 2 m level shoulder.

Road 2, from (0, 60), has a superelevation of 0.05 rad. Lane -1 is a
3.5 m level driving lane and lane -2 a 2 m shoulder that is not level,
which the spec does not allow. A pole stands on lane -1 at s = 25.
"""

from pathlib import Path


def lane(id, width, kind="driving", level=False, height=None):
    h = f'<height sOffset="0" inner="{height}" outer="{height}"/>' if height else ""
    return (
        f'<lane id="{id}" type="{kind}" level="{"true" if level else "false"}">'
        f'<width sOffset="0" a="{width}" b="0" c="0" d="0"/>{h}</lane>'
    )


def road(id, y, lateral, left, right, objects=""):
    return (
        f'  <road name="" length="50" id="{id}" junction="-1">\n'
        f'    <planView><geometry s="0" x="0" y="{y}" hdg="0" length="50"><line/></geometry></planView>\n'
        f"    <lateralProfile>{lateral}</lateralProfile>\n"
        '    <lanes><laneSection s="0">'
        f'<left>{"".join(left)}</left>'
        '<center><lane id="0" type="none"/></center>'
        f'<right>{"".join(right)}</right>'
        "</laneSection></lanes>\n"
        f"{objects}"
        "  </road>\n"
    )


BANK = '<superelevation s="0" a="0.05" b="0" c="0" d="0"/>'
CROWN = '<shape s="0" t="-10" a="-0.25" b="0.025" c="0" d="0"/>'

roads = [
    road(0, 0, BANK,
         [lane(1, 3.5), lane(2, 2, "shoulder", level=True)],
         [lane(-1, 3.5), lane(-2, 2, "shoulder", level=True),
          lane(-3, 2, "sidewalk", level=True, height=0.12)],
         '    <objects><object id="0" type="pole" s="25" t="-4.5" zOffset="0" '
         'radius="0.05" height="2"/></objects>\n'),
    road(1, 30, CROWN, [], [lane(-1, 3.5), lane(-2, 2, "shoulder", level=True)]),
    road(2, 60, BANK, [], [lane(-1, 3.5, level=True), lane(-2, 2, "shoulder")],
         '    <objects><object id="1" type="pole" s="25" t="-1.75" zOffset="0" '
         'radius="0.05" height="2"/></objects>\n'),
]

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="level_lanes"/>\n'
    + "".join(roads)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
