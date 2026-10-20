# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate speed_limits.xodr: road types and speed limits on roads and lanes.

    uv run tests/data/speed_limits.py

Every road is a flat straight heading +X, with 3.5 m driving lanes.

Road 0, 100 m from the origin, has one lane section and lanes 1, -1 and
-2. It is a town road at 50 km/h from s = 0, a motorway with no limit from
s = 30, and a rural road at 60 mph from s = 60. Lane -2 has its own
`<speed>`s: 20 with no unit from sOffset 10, and 30 km/h from sOffset 40.

Road 1, 40 m from (0, 20), has lane sections at s = 0 and s = 20, and
lane -1 in both. Its only type is a town road from s = 5, with
`max="undefined"`. Lane -1 has a `<speed>` of 10 m/s in the first
section only.

Road 2, 20 m from (0, 40), has lane -1. Its type is `highway`, which the
spec does not name, with a `<speed>` in `kmh`. Lane -1 has a `<speed>`
with `max="fast"`.

Road 3, 20 m from (0, 60), has lane -1, with a `<speed>` of 1e39 m/s, a
number too large for an `f32`.
"""

from pathlib import Path


def lane(id, speeds=()):
    records = "".join(
        f'<speed sOffset="{s}" max="{m}"' + (f' unit="{u}"' if u else "") + "/>"
        for s, m, u in speeds
    )
    return (
        f'<lane id="{id}" type="driving" level="false">'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/>'
        f"{records}</lane>"
    )


def section(s, left=(), right=()):
    return (
        f'<laneSection s="{s}">'
        f'<left>{"".join(left)}</left>'
        '<center><lane id="0" type="none"/></center>'
        f'<right>{"".join(right)}</right>'
        "</laneSection>"
    )


def road_type(s, kind, max=None, unit=None):
    speed = ""
    if max is not None:
        speed = f'<speed max="{max}"' + (f' unit="{unit}"' if unit else "") + "/>"
    return f'<type s="{s}" type="{kind}">{speed}</type>'


def road(id, y, length, types, sections):
    return (
        f'  <road name="" length="{length}" id="{id}" junction="-1">\n'
        f"    {''.join(types)}\n"
        f'    <planView><geometry s="0" x="0" y="{y}" hdg="0" length="{length}"><line/></geometry></planView>\n'
        f"    <lanes>{''.join(sections)}</lanes>\n"
        "  </road>\n"
    )


roads = [
    road(0, 0, 100,
         [road_type(0, "town", 50, "km/h"),
          road_type(30, "motorway", "no limit"),
          road_type(60, "rural", 60, "mph")],
         [section(0, [lane(1)], [lane(-1), lane(-2, [(10, 20, None), (40, 30, "km/h")])])]),
    road(1, 20, 40,
         [road_type(5, "town", "undefined")],
         [section(0, right=[lane(-1, [(0, 10, "m/s")])]),
          section(20, right=[lane(-1)])]),
    road(2, 40, 20,
         [road_type(0, "highway", 80, "kmh")],
         [section(0, right=[lane(-1, [(0, "fast", None)])])]),
    road(3, 60, 20,
         [],
         [section(0, right=[lane(-1, [(0, "1e39", None)])])]),
]

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="speed_limits"/>\n'
    + "".join(roads)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
