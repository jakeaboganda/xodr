# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate link_gaps.xodr: lane links across a gap, and ones that meet.

Every road is a flat straight heading +X with right lanes only.

Road 0 runs 50 m from the origin. Its lane section at 0 has lane -1, 7 m
wide, linked on to lane -1 of its section at 25, which has two 3.5 m
lanes, -1 and -2, as if it split in two. A lane links to one successor, so
only the -1 half of the split is in the lane graph. Lane -1 of that section links to lane -1 of road
1, and lane -2 to lane -3. Road 1 carries on from (50, 0) with lanes -1
and -2, and lane -3 opening out of nothing, 0 m wide at its start. Road 1 is 30 m long, but its
<planView> gives 40 m.

Road 2 runs 50 m from (0, 30). Its successor is road 3, whose reference
line starts 2 m further on, at (52, 30). Lane -1 links to lane -1.
"""

from pathlib import Path


def lane(id, width="3.5", b="0", link=""):
    return (
        f'<lane id="{id}" type="driving" level="false">{link}'
        f'<width sOffset="0" a="{width}" b="{b}" c="0" d="0"/></lane>'
    )


def links(pred=None, succ=None):
    ends = "".join(f'<{k} id="{v}"/>' for k, v in (("predecessor", pred), ("successor", succ)) if v)
    return f"<link>{ends}</link>" if ends else ""


def section(s, *lanes):
    return (
        f'<laneSection s="{s}"><center><lane id="0" type="none"/></center>'
        f'<right>{"".join(lanes)}</right></laneSection>'
    )


def road(id, x, y, length, plan, sections, link=""):
    return (
        f'  <road name="" length="{length}" id="{id}" junction="-1">\n'
        f"    <link>{link}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="0" length="{plan}"><line/></geometry></planView>\n'
        f'    <lanes>{"".join(sections)}</lanes>\n'
        "  </road>\n"
    )


def to(tag, road, contact):
    return f'<{tag} elementType="road" elementId="{road}" contactPoint="{contact}"/>'


roads = [
    road(0, 0, 0, 50, 50, [
        section(0, lane(-1, "7", link=links(succ=-1))),
        section(25, lane(-1, link=links(pred=-1, succ=-1)), lane(-2, link=links(pred=-1, succ=-3))),
    ], to("successor", 1, "start")),
    road(1, 50, 0, 30, 40, [
        section(0, lane(-1, link=links(pred=-1)), lane(-2), lane(-3, "0", "0.1", links(pred=-2))),
    ], to("predecessor", 0, "end")),
    road(2, 0, 30, 50, 50, [section(0, lane(-1, link=links(succ=-1)))], to("successor", 3, "start")),
    road(3, 52, 30, 50, 50, [section(0, lane(-1, link=links(pred=-1)))], to("predecessor", 2, "end")),
]

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="link_gaps"/>\n'
    + "".join(roads)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
