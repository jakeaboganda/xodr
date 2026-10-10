"""Generate repeats.xodr: repeats whose t is a cubic, and repeats detached
from the reference line.

    uv run -q --with scenariogeneration==0.16.6 tests/data/repeats.py

Road 1 is an 80 m left arc of radius 100 m from the origin heading +X, at
elevation `0.002 s^2`. It has one left lane and two right lanes, 3 m wide.
Its objects:

- 1, "Drift": a 0.5 m box every 5 m over 20 m from s = 10, with t a cubic
  from -4: bT 0.05, cT 0.002, dT 0.0001, and the tEnd it reaches, -1.4.
- 2, "Kink": a continuous wall 0.4 m wide and 1 m tall over 20 m from
  s = 40, with t a cubic from 2: bT 0.1, dT 0.0001, and tEnd 4.8.
- 3, "Chord": a post every 10 m over 40 m from s = 0, t from -5 to -7,
  detached from the reference line.
- 4, "Wall": a continuous wall 0.3 m wide and 1.5 m tall over 40 m from
  s = 40 at t = -6, detached from the reference line.
- 5, "Stray": a box every 5 m over 10 m from s = 60 at t = 1, with a bT of
  0.1, so its cubic reaches 2 and misses its tEnd of 1.
- 6, "Flat": a box every 5 m over 10 m from s = 0 at t = 2 to 3, with
  bT, cT and dT of 0, so it stays at t = 2 and misses its tEnd.
- 7, "Right": a box every 5 m over 10 m from s = 20 at t = 4, with a bT of
  -0.1, which the schema forbids. Its tEnd of 4 tells the cubic from a
  straight line, and the cubic misses it.

scenariogeneration writes no bT, cT, dT or detachFromReferenceLine, so they
are set on its XML. The header date is fixed, so the file is the same each
run.
"""

import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Arc(0.01, length=80), id=1, left_lanes=1, right_lanes=2,
                        lane_width=3)
road.planview.set_start_point(0, 0, 0)
road.add_elevation(0, 0, 0, 0.002, 0)


def box(id, name, **size):
    return xodr.Object(s=0, t=0, Type=xodr.ObjectType.barrier, id=id, name=name, **size)


drift = box("1", "Drift", length=0.5, width=0.5, height=0.5)
drift.repeat(20, 5, sStart=10, tStart=-4, tEnd=-1.4, zOffsetStart=0, zOffsetEnd=0)
kink = box("2", "Kink")
kink.repeat(20, 0, sStart=40, tStart=2, tEnd=4.8, widthStart=0.4, widthEnd=0.4,
            heightStart=1, heightEnd=1, zOffsetStart=0, zOffsetEnd=0)
chord = box("3", "Chord", radius=0.2, height=1)
chord.repeat(40, 10, sStart=0, tStart=-5, tEnd=-7, zOffsetStart=0, zOffsetEnd=0)
wall = box("4", "Wall")
wall.repeat(40, 0, sStart=40, tStart=-6, tEnd=-6, widthStart=0.3, widthEnd=0.3,
            heightStart=1.5, heightEnd=1.5, zOffsetStart=0, zOffsetEnd=0)
stray = box("5", "Stray", length=0.5, width=0.5, height=0.5)
stray.repeat(10, 5, sStart=60, tStart=1, tEnd=1, zOffsetStart=0, zOffsetEnd=0)
flat = box("6", "Flat", length=0.5, width=0.5, height=0.5)
flat.repeat(10, 5, sStart=0, tStart=2, tEnd=3, zOffsetStart=0, zOffsetEnd=0)
right = box("7", "Right", length=0.5, width=0.5, height=0.5)
right.repeat(10, 5, sStart=20, tStart=4, tEnd=4, zOffsetStart=0, zOffsetEnd=0)
road.add_object([drift, kink, chord, wall, stray, flat, right])

odr = xodr.OpenDrive("repeats")
odr.add_road(road)
odr.adjust_roads_and_lanes()

path = Path(__file__).with_name("repeats.xodr")
odr.write_xml(str(path))
tree = ET.parse(path)
root = tree.getroot()
root.find("header").set("date", "2026-01-01T00:00:00")
repeats = {o.get("id"): o.find("repeat") for o in root.iter("object")}
for id, terms in [
    ("1", {"bT": "0.05", "cT": "0.002", "dT": "0.0001"}),
    ("2", {"bT": "0.1", "dT": "0.0001"}),
    ("3", {"detachFromReferenceLine": "true"}),
    ("4", {"detachFromReferenceLine": "true"}),
    ("5", {"bT": "0.1"}),
    ("6", {"bT": "0", "cT": "0", "dT": "0", "detachFromReferenceLine": "false"}),
    ("7", {"bT": "-0.1"}),
]:
    for name, value in terms.items():
        repeats[id].set(name, value)

ET.indent(tree, space="    ")
tree.write(path, encoding="utf-8", xml_declaration=True)
