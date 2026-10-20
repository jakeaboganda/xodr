# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate direct_junctions.xodr: roads joined end to end by direct junctions.

    uv run tests/data/direct_junctions.py

Every road is a flat straight with 3.5 m driving lanes and no lane links.

Road 0 runs 50 m along +X from the origin, with lanes 1, -1 and -2. Its
successor is direct junction 8. Road 1 carries on from (50, 0) with lanes
1 and -1, and road 2 bends off to the right from (50, -3.5), heading
-0.3 rad, with lane -1. Both have junction 8 as predecessor. Junction 8
links road 0 into road 1, lanes 1 to 1 and -1 to -1, and into road 2,
lane -2 to -1. It gives no connection back out of roads 1 and 2. Its third
connection has no `linkedRoad`.

Roads 3 and 4 do the same from (0, 20), lanes 1 and -1, through direct
junction 9, which gives the connection both ways.

Junction 10 is a common junction whose only connection has no
`connectingRoad`.
"""

from pathlib import Path


def lane(id):
    return (
        f'<lane id="{id}" type="driving" level="false">'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/>'
        "</lane>"
    )


def road(id, x, y, hdg, left, right, predecessor=None, successor=None):
    ends = "".join(
        f'<{k} elementType="junction" elementId="{v}"/>'
        for k, v in (("predecessor", predecessor), ("successor", successor))
        if v is not None
    )
    return (
        f'  <road name="" length="50" id="{id}" junction="-1">\n'
        f"    <link>{ends}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="{hdg}" length="50"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0">'
        f'<left>{"".join(lane(i) for i in left)}</left>'
        '<center><lane id="0" type="none"/></center>'
        f'<right>{"".join(lane(i) for i in right)}</right>'
        "</laneSection></lanes>\n"
        "  </road>\n"
    )


def connection(id, incoming, links, linked=None, connecting=None, contact="start"):
    target = ""
    if linked is not None:
        target += f' linkedRoad="{linked}"'
    if connecting is not None:
        target += f' connectingRoad="{connecting}"'
    lane_links = "".join(f'<laneLink from="{a}" to="{b}"/>' for a, b in links)
    return (
        f'    <connection id="{id}" incomingRoad="{incoming}"{target} contactPoint="{contact}">'
        f"{lane_links}</connection>\n"
    )


def junction(id, kind, connections):
    return f'  <junction name="" id="{id}" type="{kind}">\n{"".join(connections)}  </junction>\n'


roads = [
    road(0, 0, 0, 0, [1], [-1, -2], successor=8),
    road(1, 50, 0, 0, [1], [-1], predecessor=8),
    road(2, 50, -3.5, -0.3, [], [-1], predecessor=8),
    road(3, 0, 20, 0, [1], [-1], successor=9),
    road(4, 50, 20, 0, [1], [-1], predecessor=9),
]
junctions = [
    junction(8, "direct", [
        connection(0, 0, [(1, 1), (-1, -1)], linked=1),
        connection(1, 0, [(-2, -1)], linked=2),
        connection(2, 0, [(-1, -1)]),
    ]),
    junction(9, "direct", [
        connection(0, 3, [(1, 1), (-1, -1)], linked=4),
        connection(1, 4, [(1, 1), (-1, -1)], linked=3, contact="end"),
    ]),
    junction(10, "default", [connection(0, 3, [(-1, -1)])]),
]

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="direct_junctions"/>\n'
    + "".join(roads)
    + "".join(junctions)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
