# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate lane_borders.xodr: lanes whose extent is a <border>.

scenariogeneration writes no lane borders, so this writes the XML itself.
Every road is a flat 60 m straight heading +X, with a solid mark on every
lane and on the center lane.

Road 0, from the origin, is ASAM's Ex_Lane-Border.xodr at 60 m. Its first
section, to s = 30, has border lanes 1 and -1 at 3.5 m and -3.5 m. In the
second, lane -2 opens outside lane -1, its border a smooth cubic from
-3.5 m to -7 m over 20 m, then held at -7 m. Lanes 1 and -1 link across.

Road 1, from (0, 20), has width lane -1 3.5 m wide, border lane -2 at
-6 m, and width lane -3 2 m wide outside that. Lane 1 is a 3.5 m width lane.

Road 2, from (0, 40), has lane -1 with a 3.5 m width and a border at -5 m,
and a 2 m width lane -2 outside it.

Road 3, from (0, 60), has a laneOffset of 1 m, border lane 1 at 4 m and
border lane -1 at -3 m.

Road 4, from (0, 80), has border lane -1 at -3.5 m, and border lane -2
going straight from -5 m to -2 m, so it crosses lane -1 at s = 30.
"""

from pathlib import Path

MARK = '<roadMark sOffset="0" type="solid" weight="standard" color="standard" width="0.12"/>'


def cubic(tag, a, b=0.0, c=0.0, d=0.0, s_offset=0.0):
    return f'<{tag} sOffset="{s_offset}" a="{a}" b="{b}" c="{c}" d="{d}"/>'


def lane(id, *records, links=""):
    return f'<lane id="{id}" type="driving" level="false">{links}{"".join(records)}{MARK}</lane>'


def section(s, left, right):
    return (
        f'<laneSection s="{s}">'
        f'<left>{"".join(left)}</left>'
        f'<center><lane id="0" type="none">{MARK}</lane></center>'
        f'<right>{"".join(right)}</right>'
        "</laneSection>"
    )


def road(id, y, sections, offset=""):
    return (
        f'  <road name="" length="60" id="{id}" junction="-1">\n'
        f'    <planView><geometry s="0" x="0" y="{y}" hdg="0" length="60"><line/></geometry></planView>\n'
        f"    <lanes>{offset}{''.join(sections)}</lanes>\n"
        "  </road>\n"
    )


def link(**ends):
    return "<link>" + "".join(f'<{k} id="{v}"/>' for k, v in ends.items()) + "</link>"


# Opens from -3.5 to -7 over 20 m with no slope at either end.
OPEN = 20.0
OPENING = [
    cubic("border", -3.5, 0.0, -3 * 3.5 / OPEN**2, 2 * 3.5 / OPEN**3),
    cubic("border", -7.0, s_offset=OPEN),
]

roads = [
    road(0, 0, [
        section(0,
                [lane(1, cubic("border", 3.5), links=link(successor=1))],
                [lane(-1, cubic("border", -3.5), links=link(successor=-1))]),
        section(30,
                [lane(1, cubic("border", 3.5), links=link(predecessor=1))],
                [lane(-1, cubic("border", -3.5), links=link(predecessor=-1)),
                 lane(-2, *OPENING)]),
    ]),
    road(1, 20, [section(0, [lane(1, cubic("width", 3.5))], [
        lane(-1, cubic("width", 3.5)),
        lane(-2, cubic("border", -6)),
        lane(-3, cubic("width", 2)),
    ])]),
    road(2, 40, [section(0, [], [
        lane(-1, cubic("width", 3.5), cubic("border", -5)),
        lane(-2, cubic("width", 2)),
    ])]),
    road(3, 60, [section(0, [lane(1, cubic("border", 4))], [lane(-1, cubic("border", -3))])],
         offset=cubic("laneOffset", 1).replace("sOffset", "s")),
    road(4, 80, [section(0, [], [
        lane(-1, cubic("border", -3.5)),
        lane(-2, cubic("border", -5, 0.05)),
    ])]),
]

xml = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<!-- Written by lane_borders.py. -->\n"
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="9" name="lane_borders" version="1.00"/>\n'
    + "".join(roads)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xml)
