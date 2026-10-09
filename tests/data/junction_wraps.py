"""Generate junction_wraps.xodr: junctions whose connecting lanes overlap in
every way the OpenUSD junction wrap has to handle.

    uv run -q --with scenariogeneration==0.16.6 tests/data/junction_wraps.py

Each junction stands on its own, 400 m from the next along +X, its arms on
a circle round its middle. Incoming roads are 30 m straights with 3 m
lanes. Junction j's arms are roads 100 j + k and its connecting roads start
at 10000 j.

1. Crossroads: four arms, two driving lanes each way and a 2 m sidewalk
   each side. Every pair of arms is joined lane by lane, and each corner
   has a sidewalk on the connecting road of its right turn. The right turn
   from arm 0 to arm 1 is banked at 0.08 rad, so it lies off the flat
   lanes it overlaps.
2. Tee: arms at 0, 90 and 180 degrees, the stem with one lane each way and
   the other two with two.
3. Sharp Y: arms at 0, 160 and 200 degrees, so two arms' connecting roads
   run nearly side by side.
4. Five arms, one lane each way, every pair joined.
5. Ring: four arms 25 m out, joined only by their right turns, so the
   junction's ground has a hole in the middle.
6. Slope: four arms, one lane each way, on ground climbing 6 % along +X.
   Every connecting road climbs straight from the height at its start to
   the height at its end, except the left turn from arm 2 to arm 3, which
   stays at its start height, as some of Town07's do.
7. Pass: two arms facing each other, one lane each way, so the junction's
   two connecting roads touch and never overlap.
8. Far: the crossroads of junction 1 without sidewalks, 20 km out on each
   axis, where an f32 coordinate steps 2 mm.
9. Direct: a direct junction from a two-lane road onto a three-lane one.
   Its roads belong to no junction, so it gets no wrap.
10. Flyover: four arms, one lane each way, joined only straight across.
    The east-west connecting roads rise 5 m over the middle, above the
    north-south ones.
11. Kerbs, built by hand: arm 1100 ends 10 m west of the middle and arm
    1101 starts 10 m east, each with a driving lane each way, and on the
    right a 1 m shoulder and a 2 m sidewalk raised 0.15 m. Connecting road
    110000 runs straight across in two sections: in the first, a border
    lane on the left narrows from 0.5 m to nothing; in the second it is
    gone. Road 110001 runs back the other way, contact point end, with a
    level 2 m sidewalk on its left over 110000's shoulder. Road 110002 is a
    0.5 m stub off arm 1100.
12. and 13. Two tees 26 m apart, sharing the 6 m road 1200 as an arm, so
    two wraps meet near each other.
14. The crossroads of junction 1, with every sidewalk, on the arms and
    round the corners, raised 0.1 m on its inner edge and 0.2 m on its
    outer edge, a <boundary> along the corners' outer edge and across each
    arm, and an
    <elevationGrid> on a reference line along +X through its middle: a
    0.3 m hump that falls to 0 by 10 m out. The boundary takes in the
    slivers between the corner sidewalks and the right turns, which no lane
    covers.

scenariogeneration doesn't write sidewalks in a junction, banked or
sloped connecting roads, or arms anywhere but round the origin, so those
are set on its road objects before it writes them. Junction 11 is added
after adjust_roads_and_lanes, whose lane linking can't follow its lanes.
It writes no junction <boundary> or <elevationGrid> either, so junction
14's are added to its output as text.
"""

import math
from pathlib import Path

from scenariogeneration import xodr

ARM = 30.0
LANE = 3.0
SIDEWALK = 2.0
odr = xodr.OpenDrive("junction_wraps")


def origin(j):
    if j == 8:
        return (20000.0, -20000.0)
    if j == 13:
        x, y = origin(12)
        return (x + 26.0, y)
    return (400.0 * (j - 1), 0.0)


def kerbed_sidewalk(kerb):
    """A sidewalk, raised (inner, outer) by `kerb` if it is given."""
    lane = xodr.Lane(xodr.LaneType.sidewalk, a=SIDEWALK)
    if kerb:
        lane.add_height(*kerb)
    return lane


def arm(j, k, angle, radius, lanes, sidewalk=False, kerb=None, grade=0.0):
    """Road 100 j + k, ending `radius` from junction j's middle at `angle`."""
    road = xodr.create_road(xodr.Line(ARM), 100 * j + k, lanes, lanes, lane_width=LANE)
    if sidewalk:
        section = road.lanes.lanesections[0]
        section.add_left_lane(kerbed_sidewalk(kerb))
        section.add_right_lane(kerbed_sidewalk(kerb))
    ox, oy = origin(j)
    x = ox + (radius + ARM) * math.cos(angle)
    y = oy + (radius + ARM) * math.sin(angle)
    heading = angle + math.pi
    road.planview.set_start_point(x, y, heading)
    if grade:
        road.add_elevation(0, grade * (x - ox), grade * math.cos(heading), 0, 0)
    return road


def junction(j, arms):
    creator = xodr.CommonJunctionCreator(j, f"junction {j}", startnum=10000 * j)
    for road, angle, radius in arms:
        creator.add_incoming_road_circular_geometry(road, radius, angle, "successor")
    return creator


corner_roads = {}

# How high junction 14's sidewalks stand on their inner and outer edges.
KERB = (0.1, 0.2)


def crossroads(j, sidewalks):
    radius = 14.0
    angles = [0, math.pi / 2, math.pi, 3 * math.pi / 2]
    kerb = KERB if j == 14 else None
    roads = [arm(j, k, a, radius, 2, sidewalk=sidewalks, kerb=kerb) for k, a in enumerate(angles)]
    creator = junction(j, [(r, a, radius) for r, a in zip(roads, angles)])
    for a in range(4):
        for b in range(4):
            if a != b:
                creator.add_connection(roads[a].id, roads[b].id, [-1, -2], [1, 2])
    corners = []
    if sidewalks:
        for a in range(4):
            b = (a + 1) % 4
            creator.add_connection(roads[a].id, roads[b].id, -3, 3)
            corners.append(creator.junction_roads[-1])
    corner_roads[j] = [road.id for road in corners]
    for road in corners:
        sidewalk = road.lanes.lanesections[0].rightlanes[0]
        sidewalk.lane_type = xodr.LaneType.sidewalk
        if kerb:
            sidewalk.add_height(*kerb)
    if sidewalks:
        turn = next(r for r in creator.junction_roads
                    if r.predecessor.element_id == roads[0].id
                    and r.successor.element_id == roads[1].id)
        turn.add_superelevation(0, 0.08, 0, 0, 0)
    return creator


def tee(j):
    radius = 12.0
    spec = [(0, 2), (math.pi / 2, 1), (math.pi, 2)]
    roads = [arm(j, k, a, radius, n) for k, (a, n) in enumerate(spec)]
    creator = junction(j, [(r, a, radius) for r, (a, _) in zip(roads, spec)])
    for a in range(3):
        for b in range(a + 1, 3):
            creator.add_connection(roads[a].id, roads[b].id)
    return creator


def sharp_y(j):
    radius = 12.0
    angles = [0, math.radians(160), math.radians(200)]
    roads = [arm(j, k, a, radius, 1) for k, a in enumerate(angles)]
    creator = junction(j, [(r, a, radius) for r, a in zip(roads, angles)])
    for a in range(3):
        for b in range(a + 1, 3):
            creator.add_connection(roads[a].id, roads[b].id)
    return creator


def five_arms(j):
    radius = 14.0
    angles = [2 * math.pi * k / 5 for k in range(5)]
    roads = [arm(j, k, a, radius, 1) for k, a in enumerate(angles)]
    creator = junction(j, [(r, a, radius) for r, a in zip(roads, angles)])
    for a in range(5):
        for b in range(a + 1, 5):
            creator.add_connection(roads[a].id, roads[b].id)
    return creator


def ring(j):
    radius = 25.0
    angles = [0, math.pi / 2, math.pi, 3 * math.pi / 2]
    roads = [arm(j, k, a, radius, 1) for k, a in enumerate(angles)]
    creator = junction(j, [(r, a, radius) for r, a in zip(roads, angles)])
    for a in range(4):
        creator.add_connection(roads[a].id, roads[(a + 1) % 4].id, -1, 1)
    return creator


GRADE = 0.06


def slope(j):
    radius = 14.0
    angles = [0, math.pi / 2, math.pi, 3 * math.pi / 2]
    roads = [arm(j, k, a, radius, 1, grade=GRADE) for k, a in enumerate(angles)]
    creator = junction(j, [(r, a, radius) for r, a in zip(roads, angles)])
    for a in range(4):
        for b in range(a + 1, 4):
            creator.add_connection(roads[a].id, roads[b].id)
    return creator, roads


def climb(j, creator, roads):
    """Give each of junction j's connecting roads a straight climb from the
    ground's height at its start to the height at its end, once
    adjust_roads_and_lanes has placed it."""
    ox, _ = origin(j)
    flat = (roads[2].id, roads[3].id)
    for road in creator.junction_roads:
        x0, _, _ = road.planview.get_start_point()
        x1, _, _ = road.planview.get_end_point()
        z0, z1 = GRADE * (x0 - ox), GRADE * (x1 - ox)
        if (road.predecessor.element_id, road.successor.element_id) == flat:
            z1 = z0
        road.add_elevation(0, z0, (z1 - z0) / road.planview.get_total_length(), 0, 0)


def passing(j):
    radius = 10.0
    angles = [0, math.pi]
    roads = [arm(j, k, a, radius, 1) for k, a in enumerate(angles)]
    creator = junction(j, [(r, a, radius) for r, a in zip(roads, angles)])
    creator.add_connection(roads[0].id, roads[1].id)
    return creator


def flyover(j):
    radius = 14.0
    angles = [0, math.pi / 2, math.pi, 3 * math.pi / 2]
    roads = [arm(j, k, a, radius, 1) for k, a in enumerate(angles)]
    creator = junction(j, [(r, a, radius) for r, a in zip(roads, angles)])
    creator.add_connection(roads[0].id, roads[2].id)
    creator.add_connection(roads[1].id, roads[3].id)
    return creator, (roads[0].id, roads[2].id)


def rise(creator, ends, height):
    """Lift the connecting roads between `ends` into an arch `height` high
    in the middle, once adjust_roads_and_lanes has placed them."""
    for road in creator.junction_roads:
        if {road.predecessor.element_id, road.successor.element_id} == set(ends):
            length = road.planview.get_total_length()
            road.add_elevation(0, 0, 4 * height / length, -4 * height / length**2, 0)


def section(s, left, right):
    lanes = xodr.LaneSection(s, xodr.Lane(xodr.LaneType.none))
    for lane in left:
        lanes.add_left_lane(lane)
    for lane in right:
        lanes.add_right_lane(lane)
    return lanes


def straight(id, x, y, heading, length, sections, junction=-1):
    planview = xodr.PlanView(x, y, heading)
    planview.add_geometry(xodr.Line(length))
    planview.adjust_geometries()
    lanes = xodr.Lanes()
    for lanesection in sections:
        lanes.add_lanesection(lanesection)
    return xodr.Road(id, planview, lanes, road_type=junction)


def kerbed():
    """A driving lane each way, and on the right a shoulder and a sidewalk
    raised 0.15 m."""
    sidewalk = xodr.Lane(xodr.LaneType.sidewalk, a=SIDEWALK)
    sidewalk.add_height(0.15)
    right = [xodr.Lane(a=LANE), xodr.Lane(xodr.LaneType.shoulder, a=1.0), sidewalk]
    return section(0, [xodr.Lane(a=LANE)], right)


def kerbs(j):
    ox, oy = origin(j)
    west = straight(100 * j, ox - 10 - ARM, oy, 0, ARM, [kerbed()])
    east = straight(100 * j + 1, ox + 10, oy, 0, ARM, [kerbed()])
    west.add_successor(xodr.ElementType.junction, j)
    east.add_predecessor(xodr.ElementType.junction, j)
    taper = xodr.Lane(xodr.LaneType.border, a=0.5, b=-0.05)
    first = kerbed()
    first.add_left_lane(taper)
    second = kerbed()
    second.s = 10
    across = straight(10000 * j, ox - 10, oy, 0, 20, [first, second], junction=j)
    across.add_predecessor(xodr.ElementType.road, west.id, xodr.ContactPoint.end)
    across.add_successor(xodr.ElementType.road, east.id, xodr.ContactPoint.start)
    back = straight(10000 * j + 1, ox + 10, oy, math.pi, 20, [
        section(0, [xodr.Lane(a=LANE), xodr.Lane(xodr.LaneType.sidewalk, a=SIDEWALK)],
                [xodr.Lane(a=LANE)]),
    ], junction=j)
    back.add_predecessor(xodr.ElementType.road, east.id, xodr.ContactPoint.start)
    back.add_successor(xodr.ElementType.road, west.id, xodr.ContactPoint.end)
    stub = straight(10000 * j + 2, ox - 10, oy, 0, 0.5,
                    [section(0, [xodr.Lane(a=LANE)], [xodr.Lane(a=LANE)])], junction=j)
    stub.add_predecessor(xodr.ElementType.road, west.id, xodr.ContactPoint.end)
    joint = xodr.Junction(f"junction {j}", j)
    for road, incoming, contact in [
        (across, west, xodr.ContactPoint.start),
        (back, west, xodr.ContactPoint.end),
        (stub, west, xodr.ContactPoint.start),
    ]:
        connection = xodr.Connection(incoming.id, road.id, contact)
        connection.add_lanelink(-1, -1 if contact == xodr.ContactPoint.start else 1)
        joint.add_connection(connection)
    for road in (west, east, across, back, stub):
        odr.add_road(road)
    odr.add_junction(joint)


def neighbours(j):
    """Junctions j and j + 1, tees 26 m apart, sharing road 100 j as an arm."""
    radius = 10.0
    ox, oy = origin(j)
    shared = xodr.create_road(xodr.Line(6), 100 * j, 1, 1, lane_width=LANE)
    shared.planview.set_start_point(ox + 26 - radius, oy, math.pi)
    creators = []
    for jj, angles, connection in [(j, [math.pi / 2, 3 * math.pi / 2], "successor"),
                                   (j + 1, [math.pi / 2, 3 * math.pi / 2], "predecessor")]:
        roads = [arm(jj, k + 1, a, radius, 1) for k, a in enumerate(angles)]
        creator = xodr.CommonJunctionCreator(jj, f"junction {jj}", startnum=10000 * jj)
        angle = 0 if jj == j else math.pi
        creator.add_incoming_road_circular_geometry(shared, radius, angle, connection)
        for road, a in zip(roads, angles):
            creator.add_incoming_road_circular_geometry(road, radius, a, "successor")
        for a, b in [(shared, roads[0]), (shared, roads[1]), (roads[0], roads[1])]:
            creator.add_connection(a.id, b.id)
        creators.append(creator)
    return creators, shared


def direct(j):
    ox, oy = origin(j)
    two = xodr.create_road(xodr.Line(ARM), 100 * j, 1, 1, lane_width=LANE)
    two.planview.set_start_point(ox - ARM, oy, 0)
    three = xodr.create_road(xodr.Line(ARM), 100 * j + 1, 1, 2, lane_width=LANE)
    two.add_successor(xodr.ElementType.junction, j)
    three.add_predecessor(xodr.ElementType.junction, j)
    creator = xodr.DirectJunctionCreator(j, f"junction {j}")
    creator.add_connection(two, three)
    for road in (two, three):
        odr.add_road(road)
    odr.add_junction_creator(creator)


creators = [
    crossroads(1, sidewalks=True),
    tee(2),
    sharp_y(3),
    five_arms(4),
    ring(5),
]
sloped, sloped_arms = slope(6)
raised, across = flyover(10)
pair, shared = neighbours(12)
creators += [sloped, passing(7), crossroads(8, sidewalks=False), raised, *pair,
             crossroads(14, sidewalks=True)]
added = set()
for creator in creators:
    for road in creator.incoming_roads:
        if road.id not in added:
            odr.add_road(road)
            added.add(road.id)
    odr.add_junction_creator(creator)
direct(9)

odr.adjust_roads_and_lanes()
climb(6, sloped, sloped_arms)
rise(raised, across, 5.0)
kerbs(11)
odr.write_xml(str(Path(__file__).with_suffix(".xodr")))


def area(j):
    """Junction j's <planView>, <boundary> and <elevationGrid>, as text."""
    ox, oy = origin(j)
    segments = []
    for k in range(4):
        segments.append(f'<segment type="joint" roadId="{100 * j + k}" contactPoint="end" '
                        'jointLaneStart="3" jointLaneEnd="-3"/>')
        segments.append(f'<segment type="lane" roadId="{corner_roads[j][k]}" boundaryLane="-1" '
                        'sStart="start" sEnd="end"/>')
    hump = lambda x, y: max(0.0, 0.3 * (1 - math.hypot(x, y) / 10))
    rows = []
    for i in range(9):
        x = -20 + 5 * i
        side = lambda sign: " ".join(f"{hump(x, sign * 5 * k):.2f}" for k in range(1, 5))
        rows.append(f'<elevation left="{side(1)}" center="{hump(x, 0):.2f}" right="{side(-1)}"/>')
    return (
        f'<planView><geometry s="0" x="{ox - 20}" y="{oy}" hdg="0" length="40"><line/></geometry></planView>'
        f'<boundary>{"".join(segments)}</boundary>'
        f'<elevationGrid sStart="0" gridSpacing="5">{"".join(rows)}</elevationGrid>'
    )


path = Path(__file__).with_suffix(".xodr")
xml = path.read_text()
close = xml.index("</junction>", xml.index('<junction name="junction 14"'))
path.write_text(xml[:close] + area(14) + xml[close:])
