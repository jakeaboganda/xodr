# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate junction_links.xodr: incoming roads whose <link> leaves out their junction.

    uv run tests/data/junction_links.py

Every road is a flat straight with 3.5 m driving lanes 1 and -1, and lane
links on the connecting roads only.

Road 1 runs 50 m along +X from the origin and has no <link>. Road 2 runs
50 m on from (60, 0), with junction 5 as its predecessor. Connecting road
10, in junction 5, runs the 10 m between them: its predecessor is road 1's
end, and its successor road 2's start. Junction 5 connects road 1 into road
10 at its start, lane -1 to -1, and road 2 into road 10 at its end, lane 1
to 1.

Road 3 runs 50 m along +X from (0, 30), with an empty <link>. Connecting
road 11, in junction 6, runs 10 m on from (50, 30) and names no road at
either end. Junction 6 connects road 3 into road 11 at its start, lane -1
to -1.
"""

from pathlib import Path


def lane(id, link=""):
    return (
        f'<lane id="{id}" type="driving" level="false">{link}'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/>'
        "</lane>"
    )


def road(id, x, y, length, link="", junction=-1, lane_links=False):
    ends = lambda n: f"<link><predecessor id=\"{n}\"/><successor id=\"{n}\"/></link>" if lane_links else ""
    return (
        f'  <road name="" length="{length}" id="{id}" junction="{junction}">\n'
        + (f"    {link}\n" if link else "")
        + f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="0" length="{length}"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0">'
        f"<left>{lane(1, ends(1))}</left>"
        '<center><lane id="0" type="none"/></center>'
        f"<right>{lane(-1, ends(-1))}</right>"
        "</laneSection></lanes>\n"
        "  </road>\n"
    )


def link(predecessor=None, successor=None):
    parts = []
    for tag, target in (("predecessor", predecessor), ("successor", successor)):
        if target:
            kind, id, *contact = target
            point = f' contactPoint="{contact[0]}"' if contact else ""
            parts.append(f'<{tag} elementType="{kind}" elementId="{id}"{point}/>')
    return "<link>" + "".join(parts) + "</link>" if parts else ""


def connection(id, incoming, connecting, contact, lanes):
    links = "".join(f'<laneLink from="{a}" to="{b}"/>' for a, b in lanes)
    return (
        f'    <connection id="{id}" incomingRoad="{incoming}" connectingRoad="{connecting}" '
        f'contactPoint="{contact}">{links}</connection>\n'
    )


roads = [
    road(1, 0, 0, 50),
    road(2, 60, 0, 50, link(predecessor=("junction", 5))),
    road(10, 50, 0, 10, link(("road", 1, "end"), ("road", 2, "start")), junction=5, lane_links=True),
    road(3, 0, 30, 50, "<link></link>"),
    road(11, 50, 30, 10, junction=6, lane_links=True),
]
junctions = (
    '  <junction id="5">\n'
    + connection(0, 1, 10, "start", [(-1, -1)])
    + connection(1, 2, 10, "end", [(1, 1)])
    + "  </junction>\n"
    + '  <junction id="6">\n'
    + connection(0, 3, 11, "start", [(-1, -1)])
    + "  </junction>\n"
)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="junction_links"/>\n'
    + "".join(roads)
    + junctions
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
