# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate lane_change.xodr: road marks that allow or forbid a lane change.

    uv run tests/data/lane_change.py

Road 0 is a flat 100 m straight heading +X from the origin, in right-hand
traffic, with 3.5 m driving lanes 1, -1, -2 and -3. Its road marks:

- the center line is `solid` with `laneChange="none"` to s = 50, then
  `broken` with no `laneChange`, which the spec reads as `both`;
- lane -1's outer border is `broken` with `laneChange="increase"`;
- lane -2's outer border has no mark to s = 20, then `broken` with
  `laneChange="decrease"`, then from s = 60 `broken` with
  `laneChange="sideways"`, which the spec does not allow;
- lanes 1 and -3 have no marks.
"""

from pathlib import Path


def mark(s, kind, change=None):
    c = f' laneChange="{change}"' if change else ""
    return f'<roadMark sOffset="{s}" type="{kind}" weight="standard" color="standard" width="0.12"{c}/>'


def lane(id, marks=()):
    return (
        f'<lane id="{id}" type="driving" level="false">'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/>'
        + "".join(marks)
        + "</lane>"
    )


center = '<lane id="0" type="none">' + mark(0, "solid", "none") + mark(50, "broken") + "</lane>"
right = [
    lane(-1, [mark(0, "broken", "increase")]),
    lane(-2, [mark(20, "broken", "decrease"), mark(60, "broken", "sideways")]),
    lane(-3),
]

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="lane_change"/>\n'
    '  <road name="" length="100" id="0" junction="-1">\n'
    '    <planView><geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>\n'
    '    <lanes><laneSection s="0">'
    f"<left>{lane(1)}</left>"
    f"<center>{center}</center>"
    f'<right>{"".join(right)}</right>'
    "</laneSection></lanes>\n"
    "  </road>\n"
    "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
