# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate lane_visibility.xodr: lane <visibility>s.

Road 0 is a 100 m straight heading +X, with lane sections at s = 0 and
s = 60, and 3.5 m lanes 1 and -1 in each. The header says OpenDRIVE 1.4,
since 1.9 does not define <visibility>.

In the first section:

- Lane -1 sees 200 m ahead, 50 m back, 3.5 m left and 10 m right from 0,
  then 80 m ahead from 30. Its <visibility>s come out of order.
- Lane 1, against +s, sees 120 m ahead from 0. Its <visibility> at 20 has
  no `left`, and the one at 40 a `back` of -5.

In the second section, lane -1 sees 300 m ahead, in a <visibility>
without `sOffset`, which the crate reads at the start of the section.
Lane 1 has none.
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


def seen(s, forward, back, left, right):
    offset = "" if s is None else f' sOffset="{s}"'
    parts = [("forward", forward), ("back", back), ("left", left), ("right", right)]
    return "<visibility" + offset + "".join(
        f' {k}="{v}"' for k, v in parts if v is not None
    ) + "/>"


LANE_MINUS_1 = seen(30, 80, 50, 3.5, 10) + seen(0, 200, 50, 3.5, 10)
LANE_1 = seen(0, 120, 40, 10, 3.5) + seen(20, 120, 40, None, 3.5) + seen(40, 120, -5, 10, 3.5)
SECOND = seen(None, 300, 50, 3.5, 10)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="4" name="lane_visibility"/>\n'
    '  <road name="" length="100" id="0" junction="-1">\n'
    '    <planView><geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>\n'
    "    <lanes>\n"
    f"      {section(0, lane(1, LANE_1), lane(-1, LANE_MINUS_1))}\n"
    f"      {section(60, lane(1), lane(-1, SECOND))}\n"
    "    </lanes>\n"
    "  </road>\n"
    "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
