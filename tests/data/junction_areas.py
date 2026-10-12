"""Generate junction_areas.xodr: junction boundaries and an elevation grid.

    python3 tests/data/junction_areas.py

Roads 1 and 2 run 20 m along +X from (0, 0) and (30, 0), with 3.5 m driving
lanes 1 and -1, flat at 0 m. Connecting road 10, in junction 7, joins them
across the 10 m between.

Junction 7's boundary runs counter-clockwise round the 10 m by 7 m between
them: along road 10's lane -1, across road 2's start, back along road 10's
lane 1, and across road 1's end, where a 2 m transitionLength blends road
1 into the grid. Its reference line runs 14 m along +X from
(18, 0), and its elevation grid, 4 m apart, has four rows of three points
across, 0 m at the ends and a 0.2 m hump on the line in the middle two.

Junction 8 has a boundary round the same ground, listed clockwise, with a
segment on road 99, which the file does not have.
"""

from pathlib import Path


def lane(id, link=""):
    return (
        f'<lane id="{id}" type="driving" level="false">{link}'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>'
    )


def road(id, x, link, junction=-1, linked=False):
    length = 10 if junction != -1 else 20
    ends = lambda n: f'<link><predecessor id="{n}"/><successor id="{n}"/></link>' if linked else ""
    return (
        f'  <road name="" length="{length}" id="{id}" junction="{junction}">\n'
        f"    <link>{link}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="0" hdg="0" length="{length}"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0">'
        f"<left>{lane(1, ends(1))}</left>"
        '<center><lane id="0" type="none"/></center>'
        f"<right>{lane(-1, ends(-1))}</right>"
        "</laneSection></lanes>\n"
        "  </road>\n"
    )


def to(tag, kind, id, contact=None):
    point = f' contactPoint="{contact}"' if contact else ""
    return f'<{tag} elementType="{kind}" elementId="{id}"{point}/>'


def lane_segment(road, lane, start, end):
    return f'<segment type="lane" roadId="{road}" boundaryLane="{lane}" sStart="{start}" sEnd="{end}"/>'


def joint(road, contact, start, end, transition=None):
    blend = f' transitionLength="{transition}"' if transition else ""
    return f'<segment type="joint" roadId="{road}" contactPoint="{contact}" jointLaneStart="{start}" jointLaneEnd="{end}"{blend}/>'


BOUNDARY = [
    lane_segment(10, -1, "start", "end"),
    joint(2, "start", -1, 1),
    lane_segment(10, 1, "end", "start"),
    joint(1, "end", 1, -1, 2),
]

roads = [
    road(1, 0, to("successor", "junction", 7)),
    road(2, 30, to("predecessor", "junction", 7)),
    road(10, 20, to("predecessor", "road", 1, "end") + to("successor", "road", 2, "start"), 7, True),
]

grid_rows = [
    ("0.0", "0.0", "0.0"),
    ("0.0", "0.2", "0.0"),
    ("0.0", "0.2", "0.0"),
    ("0.0", "0.0", "0.0"),
]
junctions = (
    '  <junction id="7">\n'
    '    <connection id="0" incomingRoad="1" connectingRoad="10" contactPoint="start">'
    '<laneLink from="-1" to="-1"/></connection>\n'
    '    <connection id="1" incomingRoad="2" connectingRoad="10" contactPoint="end">'
    '<laneLink from="1" to="1"/></connection>\n'
    '    <planView><geometry s="0" x="18" y="0" hdg="0" length="14"><line/></geometry></planView>\n'
    f'    <boundary>{"".join(BOUNDARY)}</boundary>\n'
    '    <elevationGrid sStart="0" gridSpacing="4">'
    + "".join(f'<elevation left="{l}" center="{c}" right="{r}"/>' for l, c, r in grid_rows)
    + "</elevationGrid>\n"
    "  </junction>\n"
    '  <junction id="8">\n'
    f'    <boundary>{"".join(reversed(BOUNDARY[1:]))}{lane_segment(99, -1, "start", "end")}</boundary>\n'
    "  </junction>\n"
)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="junction_areas"/>\n'
    + "".join(roads)
    + junctions
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
