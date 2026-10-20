# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate lane_direction.xodr: lanes whose direction overrides their side's.

    uv run tests/data/lane_direction.py

Roads 0 and 1 are flat 50 m straights heading +X, from (0, 0) and (50, 0),
joined end to start, in right-hand traffic. Each has 3.5 m driving lanes,
each linked to the same lane of the other road:

- lane 1, with no direction, so standard;
- lane -1, standard;
- lane -2, reversed;
- lane -3, both;
- lane -4, direction "sideways", which the spec does not allow.
"""

from pathlib import Path

LANES = [(1, None), (-1, "standard"), (-2, "reversed"), (-3, "both"), (-4, "sideways")]


def lane(id, direction, link):
    d = f' direction="{direction}"' if direction else ""
    return (
        f'<lane id="{id}" type="driving" level="false"{d}>'
        f"<link>{link(id)}</link>"
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>'
    )


def road(id, x, road_link, lane_link):
    lanes = [lane(i, d, lane_link) for i, d in LANES]
    return (
        f'  <road name="" length="50" id="{id}" junction="-1">\n'
        f"    <link>{road_link}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="0" hdg="0" length="50"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0">'
        f"<left>{lanes[0]}</left>"
        '<center><lane id="0" type="none"/></center>'
        f'<right>{"".join(lanes[1:])}</right>'
        "</laneSection></lanes>\n"
        "  </road>\n"
    )


roads = [
    road(0, 0, '<successor elementType="road" elementId="1" contactPoint="start"/>',
         lambda i: f'<successor id="{i}"/>'),
    road(1, 50, '<predecessor elementType="road" elementId="0" contactPoint="end"/>',
         lambda i: f'<predecessor id="{i}"/>'),
]

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="lane_direction"/>\n'
    + "".join(roads)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
