# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate road_marks.xodr: every road mark type on one straight road.

Road 0 is a 100 m straight from the origin heading +X, banked at 0.05 rad,
with a laneOffset of 0.5 m and three 3 m lanes each side. It has two lane
sections, at s = 0 and s = 50. So every lane border is at a constant t, and
every mark has a closed form to check against.

Section 0 gives each border a different keyword type, and the center lane's
mark changes from solid to solid solid at s = 25. Section 1 gives the rest
of the types, and varies width, weight, colour, height and laneChange.

Road 1 is a flat 60 m straight from (0, -30) heading +X, with lanes 1 and -1
each 3 m wide. Its marks describe their lines with `<type><line>`, which
override their type: a center line of a yellow solid and a dashed line
starting 2 m in, a right edge dashed 6 m on and 12 m off, and a left edge
of blue dashes 0.3 m outside the border.

Road 2 is a flat 40 m straight from (0, 30) heading +X, with lanes 1 and -1
each 3 m wide. Lane -1's mark is two `<explicit>` lines that paint once,
and the center lane's solid mark sways: not at all for its first 10 m, then
out to +0.5 m at 20 m, and 0.5 m from there on.

scenariogeneration drops a `<line>`'s `color`, and cannot write `<sway>`, so
those are added to its output afterwards, as text.
"""

from pathlib import Path

from scenariogeneration import xodr

T = xodr.RoadMarkType
LC = xodr.LaneChange
COLOR = xodr.RoadMarkColor


def mark(kind, soffset=0, **kwargs):
    return xodr.RoadMark(kind, soffset=soffset, **kwargs)


def section(s, marks):
    """A lane section at `s` with lanes 3 to -3, each 3 m wide. `marks`
    maps a lane id to the road marks on it."""
    center = xodr.Lane(lane_type=xodr.LaneType.none)
    for m in marks.get(0, []):
        center.add_roadmark(m)
    ls = xodr.LaneSection(s, center)
    for side, ids in ((ls.add_left_lane, (1, 2, 3)), (ls.add_right_lane, (-1, -2, -3))):
        for i in ids:
            lane = xodr.Lane(a=3)
            for m in marks.get(i, []):
                lane.add_roadmark(m)
            side(lane)
    return ls


first = section(0, {
    0: [mark(T.solid, laneChange=LC.none), mark(T.solid_solid, soffset=25, laneChange=LC.none)],
    1: [mark(T.broken)],
    2: [mark(T.solid_broken, color=COLOR.yellow, marking_weight=xodr.RoadMarkWeight.bold)],
    3: [mark(T.curb)],
    -1: [mark(T.broken_broken)],
    -2: [mark(T.broken_solid)],
    -3: [mark(T.edge)],
})
second = section(50, {
    0: [mark(T.botts_dots)],
    1: [mark(T.none)],
    2: [mark(T.grass)],
    3: [mark(T.custom)],
    -1: [mark(T.solid, width=0.2, laneChange=LC.increase)],
    -2: [mark(T.solid, color=COLOR.yellow, laneChange=LC.decrease)],
    -3: [mark(T.broken, height=0.02)],
})

lanes = xodr.Lanes()
lanes.add_laneoffset(xodr.LaneOffset(0, 0.5, 0, 0, 0))
lanes.add_lanesection(first)
lanes.add_lanesection(second)

planview = xodr.PlanView(0, 0, 0)
planview.add_geometry(xodr.Line(100))
road = xodr.Road(0, planview, lanes)
road.add_superelevation(0, 0.05, 0, 0, 0)


def lined(kind, *lines, **kwargs):
    m = xodr.RoadMark(kind, **kwargs)
    for line in lines:
        m.add_specific_road_line(line)
    return m


RULE = xodr.MarkRule
center = xodr.Lane(lane_type=xodr.LaneType.none)
center.add_roadmark(lined(
    T.solid_broken,
    xodr.RoadLine(0.12, 0, 0, toffset=0.12, rule=RULE.no_passing),
    xodr.RoadLine(0.1, 3, 9, toffset=-0.12, soffset=2, rule=RULE.caution),
    width=0.35,
))
ls = xodr.LaneSection(0, center)
left = xodr.Lane(a=3)
left.add_roadmark(lined(T.custom, xodr.RoadLine(0.2, 1, 1, toffset=0.3)))
ls.add_left_lane(left)
right = xodr.Lane(a=3)
right.add_roadmark(lined(T.solid, xodr.RoadLine(0.15, 6, 12, rule=RULE.none)))
ls.add_right_lane(right)
lined_lanes = xodr.Lanes()
lined_lanes.add_lanesection(ls)
planview = xodr.PlanView(0, -30, 0)
planview.add_geometry(xodr.Line(60))
lined_road = xodr.Road(1, planview, lined_lanes)

center = xodr.Lane(lane_type=xodr.LaneType.none)
center.add_roadmark(mark(T.solid))
ls = xodr.LaneSection(0, center)
ls.add_left_lane(xodr.Lane(a=3))
right = xodr.Lane(a=3)
explicit = mark(T.custom)
explicit.add_explicit_road_line(xodr.ExplicitRoadLine(0.15, 2, 0, 5, RULE.caution))
explicit.add_explicit_road_line(xodr.ExplicitRoadLine(0.15, 3, 0.2, 20))
right.add_roadmark(explicit)
ls.add_right_lane(right)
swayed_lanes = xodr.Lanes()
swayed_lanes.add_lanesection(ls)
planview = xodr.PlanView(0, 30, 0)
planview.add_geometry(xodr.Line(40))
swayed_road = xodr.Road(2, planview, swayed_lanes)

odr = xodr.OpenDrive("road_marks")
odr.add_road(road)
odr.add_road(lined_road)
odr.add_road(swayed_road)
odr.adjust_roads_and_lanes()
out = Path(__file__).with_suffix(".xodr")
odr.write_xml(str(out))

xml = out.read_text()
for line, color in (('tOffset="0.12"', "yellow"), ('tOffset="0.3"', "blue")):
    at = xml.index(line) + len(line)
    xml = xml[:at] + f' color="{color}"' + xml[at:]
road2 = xml.index('<road rule="RHT" id="2"')
solid = xml.index('type="solid"', road2)
at = xml.index("/>", solid)
sways = """>
                            <sway ds="10" a="0" b="0.05" c="0" d="0"/>
                            <sway ds="20" a="0.5" b="0" c="0" d="0"/>
                        </roadMark>"""
xml = xml[:at] + sways + xml[at + 2:]
out.write_text(xml)
