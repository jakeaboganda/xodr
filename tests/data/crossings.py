# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate crossings.xodr: a railway crossing a road at one level.

    uv run tests/data/crossings.py

Road 1 runs 200 m along +X from (0, 0), with 3.5 m driving lanes 1 and
-1. Road 2, a railway, runs 300 m along +Y from (55, -150), with one rail
lane -1, 1.5 m wide, so it crosses road 1 at its s = 55 and its own
s = 150.

Crossing junction 555, as the spec's example, gives road 2 priority, and
crosses road 1 from s = 50 to 60 and road 2 from s = 160 to 140, its ends
the wrong way round. A third section names road 99, which the file does
not have.
"""

from pathlib import Path


def lane(id, kind, width):
    return (
        f'<lane id="{id}" type="{kind}" level="false">'
        f'<width sOffset="0" a="{width}" b="0" c="0" d="0"/></lane>'
    )


def road(id, x, y, hdg, length, left, right):
    return (
        f'  <road name="" length="{length}" id="{id}" junction="-1">\n'
        f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="{hdg}" length="{length}"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0">'
        f"<left>{left}</left>"
        '<center><lane id="0" type="none"/></center>'
        f"<right>{right}</right>"
        "</laneSection></lanes>\n"
        "  </road>\n"
    )


roads = [
    road(1, 0, 0, 0, 200, lane(1, "driving", 3.5), lane(-1, "driving", 3.5)),
    road(2, 55, -150, 1.5707963267948966, 300, "", lane(-1, "rail", 1.5)),
]

junction = (
    '  <junction name="myRailCrossing" type="crossing" id="555">\n'
    '    <priority high="2" low="1"/>\n'
    '    <roadSection id="0" roadId="1" sStart="50" sEnd="60"/>\n'
    '    <roadSection id="1" roadId="2" sStart="160" sEnd="140"/>\n'
    '    <roadSection id="2" roadId="99" sStart="0" sEnd="10"/>\n'
    "  </junction>\n"
)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="crossings"/>\n'
    + "".join(roads)
    + junction
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
