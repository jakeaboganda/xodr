"""Generate gantries.xodr: demo scenes of signs on gantries.

    python3 tests/data/gantries.py

Road 1 is a motorway, 300 m long heading +X. Each carriageway has three
3.75 m driving lanes and a 2.5 m shoulder, with a border beyond it. A 4 m
median separates the carriageways. It has three gantries:

- s = 60: lane control. Each lane has its own speed limit, 120, 100 and
  80 km/h. A span gantry.
- s = 150: direction signs over both carriageways, back to back. The span
  is too long for a gantry, so it's a space frame.
- s = 240: a variable message board over the right carriageway. A span
  gantry.

Road 2 is an urban road, 100 m long, 200 m north of road 1. It has three
3.25 m driving lanes each way, a 3 m median and sidewalks. At s = 50 a
traffic light hangs over each right-hand lane, on one span gantry.
"""

from pathlib import Path


def lane(id, kind, width):
    return (
        f'<lane id="{id}" type="{kind}">'
        f'<width sOffset="0" a="{width}" b="0" c="0" d="0"/></lane>'
    )


def side(sign, lanes):
    """Lanes from the centre out, numbered away from it on side `sign`."""
    return "".join(lane(sign * k, kind, width) for k, (kind, width) in enumerate(lanes, 1))


def road(id, y, length, left, right, signals):
    return (
        f'<road id="{id}" length="{length}" junction="-1" rule="RHT">'
        f'<planView><geometry s="0" x="0" y="{y}" hdg="0" length="{length}">'
        "<line/></geometry></planView>"
        '<lanes><laneSection s="0">'
        f"<left>{side(1, left)}</left>"
        '<center><lane id="0" type="none"/></center>'
        f"<right>{side(-1, right)}</right></laneSection></lanes>"
        f"<signals>{''.join(signals)}</signals></road>"
    )


def signal(id, s, t, z, width, height, orientation, country, kind, subtype,
           name, dynamic="no", value=None, unit=None, inner=""):
    shows = f' value="{value}" unit="{unit}"' if value is not None else ""
    return (
        f'<signal id="{id}" name="{name}" s="{s}" t="{t}" zOffset="{z}" '
        f'orientation="{orientation}" dynamic="{dynamic}" country="{country}" '
        f'type="{kind}" subtype="{subtype}"{shows} '
        f'width="{width}" height="{height}">{inner}</signal>'
    )


def speed(id, s, t, lane_id, kph, subtype):
    inner = (
        f'<validity fromLane="{lane_id}" toLane="{lane_id}"/>'
        f'<semantics><speed type="maximum" value="{kph}" unit="km/h"/></semantics>'
    )
    return signal(id, s, t, 6, 1.0, 1.0, "+", "DE", "274", subtype,
                  f"Limit{kph}", value=kph, unit="km/h", inner=inner)


carriageway = [
    ("driving", 3.75),
    ("driving", 3.75),
    ("driving", 3.75),
    ("shoulder", 2.5),
    ("border", 2.0),
]
motorway_left = [("median", 4.0)] + carriageway
middle = [-1.875, -5.625, -9.375]
oncoming = [5.875, 9.625, 13.375]

motorway = [
    speed(1, 60, middle[0], -1, 120, "62"),
    speed(2, 60, middle[1], -2, 100, "60"),
    speed(3, 60, middle[2], -3, 80, "58"),
    signal(4, 150, -3.75, 5.5, 4.5, 3.0, "+", "DE", "332", "-1", "ExitAhead"),
    signal(5, 150, -9.5, 5.5, 4.5, 3.0, "+", "DE", "332", "-1", "ExitLeft"),
    signal(6, 150, oncoming[0] + 1.875, 5.5, 4.5, 3.0, "-", "DE", "332", "-1",
           "OncomingExitAhead"),
    signal(7, 150, oncoming[2] - 1.875, 5.5, 4.5, 3.0, "-", "DE", "332", "-1",
           "OncomingExitRight"),
    signal(8, 240, middle[1], 5.5, 6.0, 2.0, "+", "OPENDRIVE", "1000015", "-1",
           "MessageBoard", dynamic="yes",
           inner='<vmsBoard displayType="LED" displayWidth="6" displayHeight="2" v="0" z="0">'
                 '<displayArea index="0" v="-1.5" z="0.2" width="2.5" height="1.6"/>'
                 '<displayArea index="1" v="1.5" z="0.2" width="2.5" height="1.6"/>'
                 "</vmsBoard>"),
]

urban_side = [("driving", 3.25)] * 3 + [("sidewalk", 3.0)]
urban = [
    signal(9 + k, 50, -1.625 - 3.25 * k, 5.0, 0.3, 0.9, "+", "OPENDRIVE", "1000001", "-1",
           f"Light{k + 1}", dynamic="yes",
           inner=f'<validity fromLane="-{k + 1}" toLane="-{k + 1}"/>')
    for k in range(3)
]

roads = [
    road(1, 0, 300, motorway_left, carriageway, motorway),
    road(2, 200, 100, [("median", 3.0)] + urban_side, urban_side, urban),
]

xml = (
    '<?xml version="1.0" encoding="utf-8"?>\n'
    '<OpenDRIVE><header revMajor="1" revMinor="6" name="gantries"/>\n'
    + "\n".join(roads)
    + "\n</OpenDRIVE>\n"
)
Path(__file__).with_name("gantries.xodr").write_text(xml)
