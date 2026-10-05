"""Generate structures.xodr: signs over traffic that need each kind of
structure the OpenUSD exporter adds.

    python3 tests/data/structures.py

Four straight roads, each 100 m long heading +X, with 3.5 m driving lanes
on the right only, 60 m apart. Every sign is at s = 50, 5 m up.

- Road 1, 2 lanes: back-to-back 2.5 m by 1.5 m signs over the middle. A
  cantilever.
- Road 2, 4 lanes: two 3 m by 1.5 m signs across the road, and one facing
  the other way, lower. Too many lanes for a cantilever: a span gantry.
- Road 3, 6 lanes: back-to-back 6 m by 3 m signs. Too many lanes for a
  gantry: a space frame.
- Road 4, 2 lanes: one 5 m by 5 m sign. Too much sign for a cantilever: a
  span gantry.
"""

from pathlib import Path


def sign(id, t, width, height, orientation, z=5):
    return (
        f'<signal id="{id}" s="50" t="{t}" zOffset="{z}" orientation="{orientation}" '
        f'dynamic="no" country="DE" type="332" subtype="-1" '
        f'width="{width}" height="{height}"/>'
    )


def road(id, y, lanes, signals):
    right = "".join(
        f'<lane id="-{k}" type="driving">'
        '<width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>'
        for k in range(1, lanes + 1)
    )
    return (
        f'<road id="{id}" length="100" junction="-1">'
        f'<planView><geometry s="0" x="0" y="{y}" hdg="0" length="100"><line/></geometry></planView>'
        '<lanes><laneSection s="0">'
        '<center><lane id="0" type="none"/></center>'
        f"<right>{right}</right></laneSection></lanes>"
        f"<signals>{''.join(signals)}</signals></road>"
    )


roads = [
    road(1, 0, 2, [sign(1, -3.5, 2.5, 1.5, "+"), sign(2, -3.5, 2.5, 1.5, "-")]),
    road(2, 60, 4, [
        sign(3, -4, 3, 1.5, "+"),
        sign(4, -10, 3, 1.5, "+"),
        sign(5, -7, 3, 1.5, "-", z=2.5),
    ]),
    road(3, 120, 6, [sign(6, -10.5, 6, 3, "+"), sign(7, -10.5, 6, 3, "-")]),
    road(4, 180, 2, [sign(8, -3.5, 5, 5, "+")]),
]

xml = (
    '<?xml version="1.0" encoding="utf-8"?>\n'
    '<OpenDRIVE><header revMajor="1" revMinor="6" name="structures"/>\n'
    + "\n".join(roads)
    + "\n</OpenDRIVE>\n"
)
Path(__file__).with_name("structures.xodr").write_text(xml)
