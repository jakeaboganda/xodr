# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate lane_heights.xodr: raised sidewalks on four roads.

Every road has the same cross-section: lanes 1 and -1 are 3.5 m driving
lanes, and lanes 2 and -2 are 2 m sidewalks. Sidewalk 2 is flat at 0.12 m.
Sidewalk -2 slopes from 0.02 m at the kerb to 0.12 m at its outer edge, and
from s = 20 to s = 21 its kerb ramps up to 0.12 m, the way esmini's
multi_intersections.xodr writes a sidewalk.

Road 0 is a flat 60 m straight from the origin heading +X. It carries a
solid mark on every border, a pole on each sidewalk, a pole on lane -1, and
a sign on sidewalk 2, for checking what stands on a raised lane.

Road 1 is the same 60 m straight from (0, -40), banked at 0.05 rad and
climbing at 3 %, so a height stands off along a tilted normal.

Road 2 is a flat 60 m straight from (0, 40) with two lane sections, at 0
and 30. Sidewalk -2 is level in the first. In the second it gives 0 m and
0.1 m from sOffset 5, then 0.15 m across from sOffset 7, so its heights are
measured from its own section.

Road 3 is a flat 40 m arc from (0, 80) heading +X, curving left on a 25 m
radius, with both sidewalks flat at 0.12 m.
"""

from pathlib import Path

from scenariogeneration import xodr

SIDEWALK = xodr.LaneType.sidewalk


def section(s, left_heights, right_heights):
    """A lane section at `s`: lanes 1 and -1 driving, 2 and -2 sidewalks.
    Each list of heights is `(sOffset, inner, outer)` for its sidewalk."""
    center = xodr.Lane(lane_type=xodr.LaneType.none)
    center.add_roadmark(xodr.RoadMark(xodr.RoadMarkType.solid))
    ls = xodr.LaneSection(s, center)
    for add, heights in ((ls.add_left_lane, left_heights), (ls.add_right_lane, right_heights)):
        driving = xodr.Lane(a=3.5)
        driving.add_roadmark(xodr.RoadMark(xodr.RoadMarkType.solid))
        add(driving)
        sidewalk = xodr.Lane(lane_type=SIDEWALK, a=2)
        sidewalk.add_roadmark(xodr.RoadMark(xodr.RoadMarkType.solid))
        for s_offset, inner, outer in heights:
            sidewalk.add_height(inner, outer, soffset=s_offset)
        add(sidewalk)
    return ls


FLAT = [(0, 0.12, 0.12)]
RAMP = [(0, 0.02, 0.12), (20, 0.02, 0.12), (21, 0.12, 0.12)]


def road(id, start, geometry, *sections):
    lanes = xodr.Lanes()
    for ls in sections:
        lanes.add_lanesection(ls)
    planview = xodr.PlanView(*start)
    planview.add_geometry(geometry)
    return xodr.Road(id, planview, lanes)


flat = road(0, (0, 0, 0), xodr.Line(60), section(0, FLAT, RAMP))
flat.add_object(xodr.Object(s=10, t=-4.5, Type=xodr.ObjectType.pole, id="1", name="SlopedPole",
                            radius=0.05, height=2))
flat.add_object(xodr.Object(s=30, t=-4.5, Type=xodr.ObjectType.pole, id="2", name="RaisedPole",
                            radius=0.05, height=2))
flat.add_object(xodr.Object(s=30, t=4.5, Type=xodr.ObjectType.pole, id="3", name="LeftPole",
                            radius=0.05, height=2))
flat.add_object(xodr.Object(s=30, t=-1.75, Type=xodr.ObjectType.pole, id="4", name="RoadPole",
                            radius=0.05, height=2))
flat.add_signal(xodr.Signal(s=15, t=4.5, id="5", name="Sign", country="DE", Type="274",
                            subtype="55", value=50, unit="km/h", zOffset=2,
                            orientation=xodr.Orientation.negative, height=0.6, width=0.6))

banked = road(1, (0, -40, 0), xodr.Line(60), section(0, FLAT, RAMP))
banked.add_superelevation(0, 0.05, 0, 0, 0)
banked.add_elevation(0, 0, 0.03, 0, 0)

two_sections = road(2, (0, 40, 0), xodr.Line(60), section(0, [], []),
                    section(30, [], [(5, 0, 0.1), (7, 0.15, 0.15)]))

arc = road(3, (0, 80, 0), xodr.Arc(0.04, length=40), section(0, FLAT, FLAT))

odr = xodr.OpenDrive("lane_heights")
for r in (flat, banked, two_sections, arc):
    odr.add_road(r)
odr.adjust_roads_and_lanes()
odr.write_xml(str(Path(__file__).with_suffix(".xodr")))
