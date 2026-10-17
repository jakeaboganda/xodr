"""Generate city.xodr: a small town grid with signalised crossroads, raised
sidewalks, buildings, trees and street furniture, for the viewer and the
OpenUSD export.

    uv run -q --with scenariogeneration==0.16.6 tests/data/city.py

Six crossroads stand on a 150 m grid, three along y = 0 and three along
y = 150. The avenue along y = 0 has two 3.25 m lanes each way; every other
street has one. Each street has, on each side, a 0.3 m kerb rising from
the road to 0.15 m and a 4 m sidewalk at 0.15 m. Streets past the edge of
the grid run on 60 m.

Each crossroads joins every lane to every arm: straight on lane by lane,
right turns from the outer lanes and left turns from the inner ones. Its
corners carry the kerb and sidewalk round, each its own connecting road,
as scenariogeneration makes one road per lane it joins. Its <boundary> runs across each
arm and along each corner's sidewalk, and a flat <elevationGrid> sets its
ground at 0, so the ground between the turns is filled and the sidewalks
stand 0.15 m above it.

Each arm has a zebra crossing across its end, a stop line across its
incoming lanes 5 m back, and a traffic light on the sidewalk beside the
stop line. Road marks end at the stop line. Each zebra stripe is a closed outline of fillType paint, so it
lies on the road as paint and the crossing has no solid of its own. Each crossroads has two controllers,
one for its east-west arms and one for its north-south ones. Each street
has a speed limit sign after each crossroads, 50 km/h on the avenue and
30 km/h elsewhere, with its <semantics>.

Along each street stand trees, street lamps, benches and bins on the
sidewalks, and buildings behind them, of seeded random size. The avenue
has a bus shelter each way.

scenariogeneration writes none of the controllers, sign semantics,
boundaries, grids or paint fill types, and writes road marks on
connecting roads, which would paint lines across the junction. It also
writes subtype="None" on an object without a subtype. The script adds the
first five to its output, and drops the marks and the "None"s.
"""

import math
import random
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

LANE = 3.25
KERB = 0.3
KERB_HEIGHT = 0.15
SIDEWALK = 4.0
STRIPE = 0.5
STOP = 5.0
RADIUS = 15.0
SPACING = 150.0
STUB = 60.0
COLUMNS = 3
ROWS = 2

rng = random.Random(7)
odr = xodr.OpenDrive("city", revMinor="8")
next_id = iter(range(1, 100000))


def lanes_on(y):
    return 2 if y == 0 else 1


def half_width(n):
    return n * LANE + KERB + SIDEWALK


def street(road_id, length, n, x, y, heading):
    road = xodr.create_road(xodr.Line(length), road_id, n, n, lane_width=LANE)
    section = road.lanes.lanesections[0]
    for add in (section.add_left_lane, section.add_right_lane):
        add(xodr.Lane(xodr.LaneType.curb, a=KERB).add_height(0, KERB_HEIGHT))
        add(xodr.Lane(xodr.LaneType.sidewalk, a=SIDEWALK).add_height(KERB_HEIGHT, KERB_HEIGHT))
    road.planview.set_start_point(x, y, heading)
    return road


def centre(i, j):
    return (SPACING * i, SPACING * j)


roads = []
arms = {}
signal_ids = {}
corner_roads = {}
controllers = []


def arm(junction, angle, road, end, n):
    arms.setdefault(junction, []).append((angle, road, end, n))


road_id = 1
for j in range(ROWS):
    y = SPACING * j
    n = lanes_on(y)
    for i in range(-1, COLUMNS):
        x0 = SPACING * i + RADIUS if i >= 0 else -RADIUS - STUB
        length = SPACING - 2 * RADIUS if 0 <= i < COLUMNS - 1 else STUB
        if i == COLUMNS - 1:
            x0 = SPACING * i + RADIUS
        road = street(road_id, length, n, x0, y, 0.0)
        roads.append((road, length, n, "x"))
        if i >= 0:
            arm((i, j), 0.0, road, "start", n)
        if i + 1 < COLUMNS:
            arm((i + 1, j), math.pi, road, "end", n)
        road_id += 1
for i in range(COLUMNS):
    x = SPACING * i
    for j in range(-1, ROWS):
        y0 = SPACING * j + RADIUS if j >= 0 else -RADIUS - STUB
        length = SPACING - 2 * RADIUS if 0 <= j < ROWS - 1 else STUB
        road = street(road_id, length, 1, x, y0, math.pi / 2)
        roads.append((road, length, 1, "y"))
        if j >= 0:
            arm((i, j), math.pi / 2, road, "start", 1)
        if j + 1 < ROWS:
            arm((i, j + 1), 3 * math.pi / 2, road, "end", 1)
        road_id += 1

for road, *_ in roads:
    odr.add_road(road)


def into(end):
    """The sign of the lane ids that carry traffic into a junction at `end`."""
    return -1 if end == "end" else 1


creators = []
for number, ((i, j), members) in enumerate(sorted(arms.items()), start=1):
    members.sort(key=lambda m: m[0])
    creator = xodr.CommonJunctionCreator(number, f"crossroads {i},{j}", startnum=1000 * number)
    for angle, road, end, _ in members:
        creator.add_incoming_road_circular_geometry(road, RADIUS, angle, "predecessor" if end == "start" else "successor")
    count = len(members)
    for a in range(count):
        _, ra, ea, na = members[a]
        for turn in (1, 2, 3):
            _, rb, eb, nb = members[(a + turn) % count]
            k = min(na, nb)
            if turn == 1:
                pick_a, pick_b = range(na - k + 1, na + 1), range(nb - k + 1, nb + 1)
            elif turn == 3:
                pick_a, pick_b = range(1, k + 1), range(1, k + 1)
            else:
                pick_a, pick_b = range(1, k + 1), range(1, k + 1)
            creator.add_connection(
                ra.id, rb.id,
                [into(ea) * l for l in pick_a],
                [-into(eb) * l for l in pick_b],
            )
        _, rb, eb, nb = members[(a + 1) % count]
        creator.add_connection(
            ra.id, rb.id,
            [into(ea) * (na + 1), into(ea) * (na + 2)],
            [-into(eb) * (nb + 1), -into(eb) * (nb + 2)],
        )
        kerb, walk = creator.junction_roads[-2:]
        for road, kind, heights in ((kerb, xodr.LaneType.curb, (0, KERB_HEIGHT)),
                                    (walk, xodr.LaneType.sidewalk, (KERB_HEIGHT, KERB_HEIGHT))):
            section = road.lanes.lanesections[0]
            for lane in section.leftlanes + section.rightlanes:
                lane.lane_type = kind
                lane.add_height(*heights)
        corner_roads.setdefault(number, []).append(walk.id)
    creators.append(((i, j), number, members))
    odr.add_junction_creator(creator)

odr.adjust_roads_and_lanes()


def furnish(road, length, n, axis):
    edge = n * LANE + KERB
    limit = 50 if axis == "x" and road.planview.y_start == 0 else 30
    for sign, orientation in ((-1, xodr.Orientation.positive), (1, xodr.Orientation.negative)):
        s_sign = 8.0 if sign < 0 else length - 8.0
        sid = f"speed{road.id}{'p' if sign < 0 else 'n'}"
        road.add_signal(xodr.Signal(
            s=s_sign, t=sign * (edge + 0.6), country="DE", Type="274",
            subtype="55" if limit == 50 else "53", value=limit, unit="km/h",
            id=sid, name=f"{limit} km/h", zOffset=2.0, orientation=orientation,
            height=0.6, width=0.6))
        signal_ids.setdefault("speed", []).append((sid, limit))
        s = 12.0
        k = 0
        while s < length - 10.0:
            if k % 2 == 0:
                road.add_object(xodr.Object(s, sign * (edge + 0.9), xodr.ObjectType.streetLamp,
                                            id=str(next(next_id)), radius=0.1, height=8.0))
            else:
                road.add_object(xodr.Object(s, sign * (edge + 1.2), xodr.ObjectType.tree,
                                            id=str(next(next_id)), radius=0.18, height=2.6))
                road.add_object(xodr.Object(s, sign * (edge + 1.2), xodr.ObjectType.vegetation,
                                            subtype="crown", id=str(next(next_id)),
                                            radius=1.7, height=3.6, zOffset=2.3))
            if k % 3 == 1:
                road.add_object(xodr.Object(s + 4.0, sign * (edge + 3.4), xodr.ObjectType.obstacle,
                                            subtype="bench", id=str(next(next_id)),
                                            length=1.8, width=0.6, height=0.45))
                road.add_object(xodr.Object(s + 6.0, sign * (edge + 3.6), xodr.ObjectType.obstacle,
                                            subtype="bin", id=str(next(next_id)),
                                            radius=0.3, height=1.0))
            s += 15.0
            k += 1
        if limit == 50:
            road.add_object(xodr.Object(length / 2, sign * (edge + 3.0), xodr.ObjectType.obstacle,
                                        subtype="busShelter", id=str(next(next_id)),
                                        length=5.0, width=1.6, height=2.6))
        s = 15.0
        tall = 18.0 if limit == 50 else 10.0
        while s < length - 15.0:
            along = rng.uniform(14.0, 26.0)
            if s + along > length - 15.0:
                break
            depth = rng.uniform(12.0, 20.0)
            height = rng.uniform(tall * 0.6, tall * 2.0)
            road.add_object(xodr.Object(
                s + along / 2, sign * (half_width(n) + 1.5 + depth / 2),
                xodr.ObjectType.building, id=str(next(next_id)),
                length=along, width=depth, height=round(height, 1)))
            s += along + rng.uniform(2.0, 6.0)


for road, length, n, axis in roads:
    furnish(road, length, n, axis)

for (i, j), number, members in creators:
    for p, axis in enumerate(("x", "y")):
        controllers.append((f"{number}{p}", f"crossroads {i},{j} {axis}", [], number))
    for angle, road, end, n in members:
        length = road.planview.get_total_length()
        edge = n * LANE + KERB
        incoming = into(end)
        s = length - 5.5 if end == "end" else 5.5
        orientation = xodr.Orientation.positive if end == "end" else xodr.Orientation.negative
        light = f"light{road.id}{end[0]}"
        road.add_signal(xodr.Signal(
            s=s, t=incoming * (edge + 0.7), country="OpenDRIVE", Type="1000001",
            id=light, name=f"light {road.id}", dynamic=xodr.Dynamic.yes,
            zOffset=2.5, orientation=orientation, height=1.15, width=0.38))
        stop = f"stop{road.id}{end[0]}"
        road.add_signal(xodr.Signal(
            s=length - STOP if end == "end" else STOP, t=incoming * n * LANE / 2,
            country="OpenDRIVE", Type="294", id=stop, name="stop line",
            zOffset=0.0, orientation=orientation, height=0.03, width=n * LANE))
        middle = length - 2.5 if end == "end" else 2.5
        crossing = xodr.Object(middle, 0.0, xodr.ObjectType.crosswalk, id=f"crossing{road.id}{end[0]}")
        t = -n * LANE
        stripe = 0
        while t + STRIPE <= n * LANE + 1e-9:
            outline = xodr.Outline(closed=True, id=stripe)
            for ds, dt in ((-1.5, 0), (1.5, 0), (1.5, STRIPE), (-1.5, STRIPE)):
                outline.add_corner(xodr.CornerRoad(middle + ds, t + dt, 0, 0))
            crossing.add_outline(outline)
            t += 2 * STRIPE
            stripe += 1
        road.add_object(crossing)
        axis = 0 if math.isclose(math.sin(angle), 0.0, abs_tol=1e-9) else 1
        controllers[-2 + axis][2].append(light)

out = Path(__file__).with_suffix(".xodr")
odr.write_xml(str(out))

tree = ET.parse(out)
root = tree.getroot()
by_id = {r.get("id"): r for r in root.findall("road")}

for road in root.findall("road"):
    if road.get("junction") != "-1":
        for lane in road.iter("lane"):
            for mark in lane.findall("roadMark"):
                lane.remove(mark)

ends = {}
for _, _, members in creators:
    for _, road, end, _ in members:
        ends.setdefault(str(road.id), set()).add(end)
for road in root.findall("road"):
    if road.get("junction") != "-1":
        continue
    length = float(road.get("length"))
    for lane in road.iter("lane"):
        marks = lane.findall("roadMark")
        if not marks:
            continue
        mark = marks[0]
        at = list(lane).index(mark)
        if "start" in ends.get(road.get("id"), ()):
            lane.insert(at, ET.Element("roadMark", sOffset="0", type="none", color="standard"))
            mark.set("sOffset", f"{STOP}")
            at += 1
        if "end" in ends.get(road.get("id"), ()):
            lane.insert(at + 1, ET.Element("roadMark", sOffset=f"{length - STOP}", type="none", color="standard"))

for thing in root.iter():
    if thing.get("subtype") == "None":
        del thing.attrib["subtype"]

for crossing in root.iter("object"):
    if crossing.get("type") == "crosswalk":
        for outline in crossing.iter("outline"):
            outline.set("fillType", "paint")

for sid, limit in signal_ids["speed"]:
    signal = root.find(f".//signal[@id='{sid}']")
    semantics = ET.SubElement(signal, "semantics")
    ET.SubElement(semantics, "speed", type="maximum", value=str(limit), unit="km/h")

first_junction = list(root).index(root.find("junction"))
for offset, (cid, name, lights, _) in enumerate(controllers):
    controller = ET.Element("controller", id=cid, name=name)
    for light in lights:
        ET.SubElement(controller, "control", signalId=light, type="0")
    root.insert(first_junction + offset, controller)

for (i, j), number, members in creators:
    junction = root.find(f"junction[@id='{number}']")
    for p in range(2):
        ET.SubElement(junction, "controller", id=f"{number}{p}", type="0", sequence=str(p + 1))
    cx, cy = centre(i, j)
    reach = 25.0
    plan = ET.SubElement(junction, "planView")
    ET.SubElement(ET.SubElement(plan, "geometry", s="0", x=f"{cx - reach}", y=f"{cy}",
                                hdg="0", length=f"{2 * reach}"), "line")
    boundary = ET.SubElement(junction, "boundary")
    for k, (angle, road, end, n) in enumerate(members):
        ET.SubElement(boundary, "segment", type="joint", roadId=str(road.id), contactPoint=end,
                      jointLaneStart=str(n + 2), jointLaneEnd=str(-(n + 2)))
        corner = by_id[str(corner_roads[number][k])]
        sidewalk = next(l for l in corner.iter("lane") if l.get("type") == "sidewalk")
        ET.SubElement(boundary, "segment", type="lane", roadId=corner.get("id"),
                      boundaryLane=sidewalk.get("id"), sStart="0", sEnd=corner.get("length"))
    spacing = 5.0
    side = int(reach / spacing)
    grid = ET.SubElement(junction, "elevationGrid", gridSpacing=f"{spacing}", sStart="0")
    zeros = " ".join(["0"] * side)
    for _ in range(2 * side + 1):
        ET.SubElement(grid, "elevation", center="0", left=zeros, right=zeros)

ET.indent(tree, "    ")
tree.write(out, encoding="utf-8", xml_declaration=True)
