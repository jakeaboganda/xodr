# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate junction_priority.xodr: a junction whose roads give way to each other.

Every road is a flat straight with a 3.5 m driving lane -1.

Road 1 runs 50 m along +X from the origin into junction 5. Road 2 runs 50 m
along +X from (60, 0), and road 3 50 m along +Y from (55, 5), both out of
junction 5. Connecting road 10 runs straight on from road 1 into road 2,
and connecting road 11 turns off it into road 3. Junction 5 gives road 10
priority over road 11, and names road 99, which is not in the file, over
road 11.
"""

from pathlib import Path


def road(id, x, y, hdg, length, link, junction=-1, geometry="<line/>"):
    return (
        f'  <road name="" length="{length}" id="{id}" junction="{junction}">\n'
        f"    <link>{link}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="{hdg}" length="{length}">{geometry}</geometry></planView>\n'
        '    <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>'
        '<lane id="-1" type="driving" level="false"><link><predecessor id="-1"/><successor id="-1"/></link>'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>'
        "</right></laneSection></lanes>\n"
        "  </road>\n"
    )


def to(tag, kind, id, contact=None):
    point = f' contactPoint="{contact}"' if contact else ""
    return f'<{tag} elementType="{kind}" elementId="{id}"{point}/>'


QUARTER = 1.5707963267948966
roads = [
    road(1, 0, 0, 0, 50, to("successor", "junction", 5)),
    road(2, 60, 0, 0, 50, to("predecessor", "junction", 5)),
    road(3, 55, 5, QUARTER, 50, to("predecessor", "junction", 5)),
    road(10, 50, 0, 0, 10, to("predecessor", "road", 1, "end") + to("successor", "road", 2, "start"), 5),
    road(11, 50, 0, 0, 5 * QUARTER, to("predecessor", "road", 1, "end") + to("successor", "road", 3, "start"),
         5, '<arc curvature="0.2"/>'),
]
junction = (
    '  <junction id="5">\n'
    '    <connection id="0" incomingRoad="1" connectingRoad="10" contactPoint="start"><laneLink from="-1" to="-1"/></connection>\n'
    '    <connection id="1" incomingRoad="1" connectingRoad="11" contactPoint="start"><laneLink from="-1" to="-1"/></connection>\n'
    '    <priority high="10" low="11"/>\n'
    '    <priority high="99" low="11"/>\n'
    "  </junction>\n"
)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="junction_priority"/>\n'
    + "".join(roads)
    + junction
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
