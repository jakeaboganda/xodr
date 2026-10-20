# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate traffic_rule.xodr: the same roads in right- and left-hand traffic.

    uv run tests/data/traffic_rule.py

Every road is a flat 50 m straight heading +X, with a 3.5 m driving lane
on each side. Lanes 1 and -1 link to the same lane on the next road.

Roads 0 and 1, from (0, 0) and (50, 0), have no `rule`, so right-hand
traffic. Road 0's successor is road 1. Roads 2 and 3, from (0, 20) and
(50, 20), are the same with `rule="LHT"`. Roads 0 and 2 each have a signal
at s = 10 with `orientation="+"` and no `<validity>`.

Road 4, from (0, 40), has `rule="lht"`, which the spec does not allow.
"""

from pathlib import Path


def lane(id, **ends):
    links = "".join(f'<{k} id="{v}"/>' for k, v in ends.items())
    return (
        f'<lane id="{id}" type="driving" level="false">'
        f"<link>{links}</link>"
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/>'
        "</lane>"
    )


def road(id, x, y, rule=None, predecessor=None, successor=None, signal=""):
    rule = f' rule="{rule}"' if rule else ""
    ends = {}
    if predecessor is not None:
        ends["predecessor"] = predecessor
    if successor is not None:
        ends["successor"] = successor
    road_links = "".join(
        f'<{k} elementType="road" elementId="{v}" contactPoint="{"end" if k == "predecessor" else "start"}"/>'
        for k, v in ends.items()
    )
    return (
        f'  <road name="" length="50" id="{id}" junction="-1"{rule}>\n'
        f"    <link>{road_links}</link>\n"
        f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="0" length="50"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0">'
        f'<left>{lane(1, **{k: 1 for k in ends})}</left>'
        '<center><lane id="0" type="none"/></center>'
        f'<right>{lane(-1, **{k: -1 for k in ends})}</right>'
        "</laneSection></lanes>\n"
        f"{signal}"
        "  </road>\n"
    )


def signal(id, t):
    return (
        f'    <signals><signal id="{id}" s="10" t="{t}" zOffset="2" orientation="+" '
        'dynamic="no" country="DE" type="274" subtype="-1" value="50" unit="km/h"/></signals>\n'
    )


roads = [
    road(0, 0, 0, successor=1, signal=signal("s0", -5)),
    road(1, 50, 0, predecessor=0),
    road(2, 0, 20, rule="LHT", successor=3, signal=signal("s2", 5)),
    road(3, 50, 20, rule="LHT", predecessor=2),
    road(4, 0, 40, rule="lht"),
]

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="traffic_rule"/>\n'
    + "".join(roads)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
