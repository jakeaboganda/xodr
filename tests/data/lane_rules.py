# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate lane_rules.xodr: lane <rule>s, <access>es and <material>s.

    uv run tests/data/lane_rules.py

Road 0 is a 100 m straight heading +X, with lane sections at s = 0 and
s = 60, and 3.5 m lanes 1, -1 and -2 in each.

In the first section:

- Lane -1 has the rule "no stopping at any time" from 0 and "car pool"
  from 30, with a <rule> without a value at 40 between. It is open to
  buses and taxis only from 0, in 1.8 <restriction>s, until a deny of
  "none" at 40 lifts that. Its surface is asphalt with friction 0.9 and
  roughness 0.002 from 0, then gravel with friction 0.6 from 20.
- Lane -2 denies trucks and bicycles from 10, in two <access>es in the
  form before 1.8, then has an <access> at 30 without a rule. Its
  <material>s at 25 and 5 are out of order.
- Lane 1, against +s, has the rule "disabled parking" from 0 and
  "no stopping at any time" from 20.

The second section has none of them.
"""

from pathlib import Path


def lane(id, extra=""):
    return (
        f'<lane id="{id}" type="driving">'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/>'
        f"{extra}</lane>"
    )


def section(s, left, right):
    return (
        f'<laneSection s="{s}"><left>{left}</left>'
        '<center><lane id="0" type="none"/></center>'
        f"<right>{right}</right></laneSection>"
    )


LANE_MINUS_1 = (
    '<material sOffset="0" surface="asphalt" friction="0.9" roughness="0.002"/>'
    '<material sOffset="20" surface="gravel" friction="0.6"/>'
    '<access sOffset="0" rule="allow"><restriction type="bus"/><restriction type="taxi"/></access>'
    '<access sOffset="40" rule="deny"><restriction type="none"/></access>'
    '<rule sOffset="0" value="no stopping at any time"/>'
    '<rule sOffset="30" value="car pool"/>'
    '<rule sOffset="40"/>'
)
LANE_MINUS_2 = (
    '<material sOffset="25" surface="concrete" friction="0.8"/>'
    '<material sOffset="5" surface="asphalt" friction="0.9"/>'
    '<access sOffset="10" rule="deny" restriction="truck"/>'
    '<access sOffset="10" rule="deny" restriction="bicycle"/>'
    '<access sOffset="30" restriction="bus"/>'
)
LANE_1 = (
    '<rule sOffset="0" value="disabled parking"/>'
    '<rule sOffset="20" value="no stopping at any time"/>'
)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="lane_rules"/>\n'
    '  <road name="" length="100" id="0" junction="-1">\n'
    '    <planView><geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>\n'
    "    <lanes>\n"
    f"      {section(0, lane(1, LANE_1), lane(-1, LANE_MINUS_1) + lane(-2, LANE_MINUS_2))}\n"
    f"      {section(60, lane(1), lane(-1) + lane(-2))}\n"
    "    </lanes>\n"
    "  </road>\n"
    "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
