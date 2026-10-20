# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate virtual_junction.xodr: a driveway off a main road.

    uv run tests/data/virtual_junction.py

Road 1 is a 100 m main road heading +X, with lane 1 on its left and lanes
-1 and -2 on its right, each 3.5 m. Road 99 is a 30 m lot road heading -Y
from (50, -20), with lanes -1 and 1, each 3 m.

Virtual junction 555, orientation "+", spans road 1 from s = 45 to 75.
Connecting road 2 runs from road 1's outer edge at s = 50 down to road
99's start, its predecessor road 1 at elementS = 50. Connecting road 4
runs back up, its successor road 1 at elementS = 50. Each lane gives its
lane links. Connection 0 enters road 2 from road 1, connection 1 enters
road 4 from road 99. Connection 2 is a deprecated virtual connection from
road 1 at s = 90 to road 99's end, and connection 3 one from road 7, which
the file does not have.
"""

from pathlib import Path
import math


def lane(id, width, link=""):
    return (
        f'<lane id="{id}" type="driving" level="false">{link}'
        f'<width sOffset="0" a="{width}" b="0" c="0" d="0"/></lane>'
    )


def road(id, x, y, hdg, length, junction, link, left, right):
    return (
        f'  <road name="" length="{length}" id="{id}" junction="{junction}">\n'
        f"    <link>{link}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="{hdg}" length="{length}"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0">'
        + (f"<left>{left}</left>" if left else "")
        + '<center><lane id="0" type="none"/></center>'
        + f"<right>{right}</right></laneSection></lanes>\n"
        "  </road>\n"
    )


down, up = -math.pi / 2, math.pi / 2
roads = (
    road(1, 0, 0, 0, 100, -1, "", lane(1, 3.5), lane(-1, 3.5) + lane(-2, 3.5))
    + road(
        2, 50, -7, down, 13, 555,
        '<predecessor elementType="road" elementId="1" elementS="50" elementDir="+"/>'
        '<successor elementType="road" elementId="99" contactPoint="start"/>',
        "",
        lane(-1, 3, '<link><predecessor id="-2"/><successor id="-1"/></link>'),
    )
    + road(
        4, 50, -20, up, 13, 555,
        '<predecessor elementType="road" elementId="99" contactPoint="start"/>'
        '<successor elementType="road" elementId="1" elementS="50" elementDir="+"/>',
        "",
        lane(-1, 3, '<link><predecessor id="1"/><successor id="-2"/></link>'),
    )
    + road(
        99, 50, -20, down, 30, -1,
        '<predecessor elementType="junction" elementId="555"/>',
        lane(1, 3),
        lane(-1, 3),
    )
)
junction = (
    '  <junction name="driveway" type="virtual" id="555" mainRoad="1" sStart="45" sEnd="75" orientation="+">\n'
    '    <connection id="0" incomingRoad="1" connectingRoad="2" contactPoint="start"><laneLink from="-2" to="-1"/></connection>\n'
    '    <connection id="1" incomingRoad="99" connectingRoad="4" contactPoint="start"><laneLink from="1" to="-1"/></connection>\n'
    '    <connection id="2" type="virtual">'
    '<predecessor elementType="road" elementId="1" elementS="90" elementDir="-"/>'
    '<successor elementType="road" elementId="99" contactPoint="end"/>'
    '<laneLink from="1" to="1"/></connection>\n'
    '    <connection id="3" type="virtual">'
    '<predecessor elementType="road" elementId="7" elementS="10" elementDir="+"/>'
    '<successor elementType="road" elementId="99" contactPoint="end"/>'
    '<laneLink from="-1" to="1"/></connection>\n'
    "  </junction>\n"
)
xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="virtual_junction"/>\n'
    + roads
    + junction
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
