# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate junction_groups.xodr: junctions grouped as one, as round a roundabout.

Roads 1, 2 and 3 are flat 20 m straights heading +X, from x = 0, 30 and 60,
each with a 3.5 m driving lane -1. Connecting road 10, in junction 5, joins
road 1 to road 2, and connecting road 11, in junction 6, road 2 to road 3.

Junction group 1, "Ring", a roundabout, names junctions 5, 6 and 9, which
the file does not have. Junction group 2 has type "spiral", which the spec
does not allow, and names junction 5.
"""

from pathlib import Path


def road(id, x, length, link, junction=-1):
    lane_link = '<link><predecessor id="-1"/><successor id="-1"/></link>' if junction != -1 else ""
    return (
        f'  <road name="" length="{length}" id="{id}" junction="{junction}">\n'
        f"    <link>{link}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="0" hdg="0" length="{length}"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>'
        f'<lane id="-1" type="driving" level="false">{lane_link}'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>'
        "</right></laneSection></lanes>\n"
        "  </road>\n"
    )


def to(tag, kind, id, contact=None):
    point = f' contactPoint="{contact}"' if contact else ""
    return f'<{tag} elementType="{kind}" elementId="{id}"{point}/>'


def junction(id, incoming, connecting):
    return (
        f'  <junction id="{id}">'
        f'<connection id="0" incomingRoad="{incoming}" connectingRoad="{connecting}" contactPoint="start">'
        '<laneLink from="-1" to="-1"/></connection></junction>\n'
    )


roads = [
    road(1, 0, 20, to("successor", "junction", 5)),
    road(2, 30, 20, to("predecessor", "junction", 5) + to("successor", "junction", 6)),
    road(3, 60, 20, to("predecessor", "junction", 6)),
    road(10, 20, 10, to("predecessor", "road", 1, "end") + to("successor", "road", 2, "start"), 5),
    road(11, 50, 10, to("predecessor", "road", 2, "end") + to("successor", "road", 3, "start"), 6),
]
groups = (
    '  <junctionGroup name="Ring" id="1" type="roundabout">'
    '<junctionReference junction="5"/><junctionReference junction="6"/>'
    '<junctionReference junction="9"/></junctionGroup>\n'
    '  <junctionGroup id="2" type="spiral"><junctionReference junction="5"/></junctionGroup>\n'
)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="junction_groups"/>\n'
    + "".join(roads)
    + junction(5, 1, 10)
    + junction(6, 2, 11)
    + groups
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
