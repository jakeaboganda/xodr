"""Generate city.xodr: a seaside town, loosely after Barcelona's Eixample,
for the viewer and the OpenUSD export.

    uv run -q --with scenariogeneration==0.16.6 tests/data/city.py

The town is a grid of five streets inland by four along the coast, 130 m
apart, on land rising 2 % from the sea. Each road climbs with the land
along it, and its superelevation lays it on the land across it.

- The seafront road along y = 0 has two lanes each way and an 8 m
  promenade on the sea side, with palms, and a beach beyond it. Streets end
  at it in T-junctions.
- Along y = 130 and y = 390 run one-way streets of three lanes, east and
  west, with a parking lane. Along y = 260 runs an avenue of three lanes
  each way with a planted median.
- Inland, x = 0 and x = 390 run one-way north and x = 130 one-way south,
  with parking. x = 260 is a boulevard of two lanes each way, and so is
  x = 520, which climbs to the roundabout.
- A diagonal avenue of two lanes each way, with a tram line each way down
  its middle, runs from the seafront at the origin through the grid at
  45 degrees. Where it crosses the grid it makes a six-arm plaza, and it
  ends in a roundabout of two lanes, at (520, 520), whose
  island is a park with a monument and a fountain.
- The x = 390 street runs on north of the grid to a T-junction with the
  roundabout's west entry road.
- Every street has a 0.3 m kerb rising to 0.15 m and a 5 m sidewalk each
  side, carried round each junction corner.

Each crossroads has a traffic light on each arm traffic enters by, a stop
line, and a zebra crossing on every arm, and a controller for each
direction of its arms. The roundabout's entries have give-way and
roundabout signs instead. Speed limit signs give 50 km/h on the seafront,
avenues, boulevards and diagonal, and 30 km/h elsewhere. Every sign and
light has its <semantics>.

Each block is an octagon, its corners cut where the junction's corners
run. Its ground is a patch at sidewalk height, paved, or grass for the
park block and some courtyards. Buildings stand round its edge, of seeded
random size, with two towers on the seafront. Trees and lamps line the
sidewalks, benches and bins stand on them, and cars are parked in about
two thirds of the parking spaces.

Each junction has a <boundary> across each arm and round each corner, and
an <elevationGrid> on the slope, so its ground is filled and follows the
land. Road marks end at the stop line, and each zebra stripe is a closed
outline of fillType paint, so no two surfaces lie at one height.

scenariogeneration writes none of the controllers, semantics, boundaries,
grids or paint fill types, writes road marks on connecting roads, and
writes subtype="None" on an object without a subtype. The script adds the
first five to its output, and drops the marks and the "None"s. It makes
one connecting road per lane it joins, so a corner's kerb and sidewalk are
two roads.
"""

import math
import random
import xml.etree.ElementTree as ET
from pathlib import Path

from scenariogeneration import xodr

GRADE = 0.02
NORTH_EDGE = 450.0
LANE = 3.25
KERB = 0.3
KERB_HEIGHT = 0.15
WALK = 5.0
PROMENADE = 8.0
PARKING = 2.2
MEDIAN = 1.5
SPACING = 130.0
COLUMNS = 5
ROWS = 4
STUB = 60.0
GRID_RADIUS = 26.0
PLAZA_RADIUS = 48.0
RING = 70.0
ROUNDABOUT = (520.0, 520.0)
ENTRY_GAP = 6.0
STRIPE = 0.5
FOUNTAIN = 6.0
CROWN = 2.3
STOP = 5.0
TALL_LIMIT = 50
SLOW_LIMIT = 30

rng = random.Random(7)
odr = xodr.OpenDrive("city", revMinor="8")
object_ids = iter(range(1, 1000000))


def terrain(y):
    return GRADE * y


def cross_slope(heading):
    """The roll that lays a road heading `heading` on the land across it."""
    return math.atan(GRADE * math.cos(heading))


def side(drive=0, parking=False, tram=False, median=False, walk=WALK):
    """One side of a road, from the reference line out: (kind, width)."""
    lanes = []
    if median:
        lanes.append(("median", MEDIAN))
    if tram:
        lanes.append(("tram", LANE))
    lanes += [("driving", LANE)] * drive
    if parking:
        lanes.append(("parking", PARKING))
    lanes.append(("curb", KERB))
    if walk:
        lanes.append(("sidewalk", walk))
    return lanes


def profile(left, right, limit=SLOW_LIMIT):
    return {"left": left, "right": right, "limit": limit}


SEAFRONT = profile(side(2), side(2, walk=PROMENADE), TALL_LIMIT)
EASTBOUND = profile(side(0), side(3, parking=True))
WESTBOUND = profile(side(3, parking=True), side(0))
AVENUE = profile(side(3, median=True), side(3, median=True), TALL_LIMIT)
NORTHBOUND = profile(side(0), side(3, parking=True))
SOUTHBOUND = profile(side(3, parking=True), side(0))
BOULEVARD = profile(side(2), side(2), TALL_LIMIT)
DIAGONAL = profile(side(2, tram=True), side(2, tram=True), TALL_LIMIT)
RING_ROAD = profile([("curb", KERB)], side(2))
LOCAL = profile(side(1), side(1))

ROW_PROFILES = [SEAFRONT, EASTBOUND, AVENUE, WESTBOUND]
COLUMN_PROFILES = [NORTHBOUND, SOUTHBOUND, BOULEVARD, NORTHBOUND, BOULEVARD]

LANE_TYPES = {
    "driving": xodr.LaneType.driving,
    "parking": xodr.LaneType.parking,
    "tram": xodr.LaneType.tram,
    "median": xodr.LaneType.median,
    "curb": xodr.LaneType.curb,
    "sidewalk": xodr.LaneType.sidewalk,
}
HEIGHTS = {"curb": (0, KERB_HEIGHT), "sidewalk": (KERB_HEIGHT, KERB_HEIGHT),
           "median": (KERB_HEIGHT, KERB_HEIGHT)}
CONNECTED = ("driving", "tram")


def width(lanes, upto=None):
    """How far out `lanes` reach, or reach before the first of kind `upto`."""
    total = 0.0
    for kind, w in lanes:
        if kind == upto:
            break
        total += w
    return total


def carriageway(lanes):
    return sum(w for kind, w in lanes if kind not in ("curb", "sidewalk"))


def outer_mark(kind, beyond):
    if kind == "driving":
        return xodr.RoadMarkType.broken if beyond == "driving" else xodr.RoadMarkType.solid
    if kind in ("tram", "median") and beyond in ("driving", "tram"):
        return xodr.RoadMarkType.solid
    return None


def centre_mark(p):
    inner = {p["left"][0][0], p["right"][0][0]}
    if inner & {"median", "tram"}:
        return None
    return xodr.RoadMarkType.solid


class Geometry:
    """A road's reference line: a line, or an arc of `curvature`."""

    def __init__(self, x, y, heading, length, curvature=0.0):
        self.x, self.y, self.heading = x, y, heading
        self.length, self.curvature = length, curvature

    def pose(self, s):
        if self.curvature == 0.0:
            return (self.x + s * math.cos(self.heading),
                    self.y + s * math.sin(self.heading), self.heading)
        r = 1.0 / self.curvature
        cx = self.x - r * math.sin(self.heading)
        cy = self.y + r * math.cos(self.heading)
        h = self.heading + s * self.curvature
        return (cx + r * math.sin(h), cy - r * math.cos(h), h)

    def point(self, s, t):
        x, y, h = self.pose(s)
        return (x - t * math.sin(h), y + t * math.cos(h))

    def local(self, s, t, px, py):
        """`(px, py)` in the frame of the point `(s, t)`: along, across."""
        x, y = self.point(s, t)
        h = self.pose(s)[2]
        dx, dy = px - x, py - y
        return (dx * math.cos(h) + dy * math.sin(h), -dx * math.sin(h) + dy * math.cos(h))


class Street:
    def __init__(self, road_id, geometry, p):
        self.id, self.geometry, self.profile = road_id, geometry, p
        self.road = build(road_id, geometry, p)


def build(road_id, g, p):
    plan = xodr.PlanView()
    if g.curvature == 0.0:
        plan.add_geometry(xodr.Line(g.length))
    else:
        plan.add_geometry(xodr.Arc(g.curvature, length=g.length))
    centre = xodr.Lane()
    mark = centre_mark(p)
    if mark:
        centre.add_roadmark(xodr.RoadMark(mark))
    section = xodr.LaneSection(0, centre)
    for name, add in (("left", section.add_left_lane), ("right", section.add_right_lane)):
        lanes = p[name]
        for k, (kind, w) in enumerate(lanes):
            lane = xodr.Lane(LANE_TYPES[kind], a=w)
            beyond = lanes[k + 1][0] if k + 1 < len(lanes) else None
            mark = outer_mark(kind, beyond)
            if mark:
                lane.add_roadmark(xodr.RoadMark(mark))
            if kind in HEIGHTS:
                lane.add_height(*HEIGHTS[kind])
            add(lane)
    lanes = xodr.Lanes()
    lanes.add_lanesection(section)
    road = xodr.Road(road_id, plan, lanes)
    road.planview.set_start_point(g.x, g.y, g.heading)
    elevate(road, g)
    return road


def elevate(road, g):
    """Lay the road on the land, along it and across it: one line on a
    straight road, and a line every 5 m round an arc."""
    steps = 1 if g.curvature == 0.0 else max(1, math.ceil(g.length / 5.0))
    stations = [g.length * k / steps for k in range(steps + 1)]
    for a, b in zip(stations, stations[1:]):
        (_, ya, ha), (_, yb, hb) = g.pose(a), g.pose(b)
        za, zb = terrain(ya), terrain(yb)
        road.add_elevation(a, za, (zb - za) / (b - a), 0, 0)
        ra, rb = cross_slope(ha), cross_slope(hb)
        road.add_superelevation(a, ra, (rb - ra) / (b - a), 0, 0)


streets = []
junctions = []
next_road = iter(range(1, 1000))


def street(g, p):
    s = Street(next(next_road), g, p)
    streets.append(s)
    odr.add_road(s.road)
    return s


def node(centre, kind, radius):
    j = {"centre": centre, "kind": kind, "radius": radius, "arms": []}
    junctions.append(j)
    return j


def attach(j, s, end):
    j["arms"].append({"street": s, "end": end})


def grid_centre(i, j):
    return (SPACING * i, SPACING * j)


on_diagonal = {(k, k) for k in range(ROWS)}
grid = {}
for j in range(ROWS):
    for i in range(COLUMNS):
        radius = PLAZA_RADIUS if (i, j) in on_diagonal else GRID_RADIUS
        grid[(i, j)] = node(grid_centre(i, j), "lights", radius)

column_roads = {}
row_roads = {}
for j in range(ROWS):
    p = ROW_PROFILES[j]
    y = SPACING * j
    for i in range(-1, COLUMNS):
        start = grid[(i, j)]["radius"] if i >= 0 else 0.0
        x0 = SPACING * i + start if i >= 0 else -grid[(0, j)]["radius"] - STUB
        if 0 <= i < COLUMNS - 1:
            length = SPACING - start - grid[(i + 1, j)]["radius"]
        elif i < 0:
            length = STUB
        else:
            length = STUB
        s = street(Geometry(x0, y, 0.0, length), p)
        row_roads[(i, j)] = s
        if i >= 0:
            attach(grid[(i, j)], s, "start")
        if i + 1 < COLUMNS:
            attach(grid[(i + 1, j)], s, "end")

ring_centre = ROUNDABOUT
ring_entries = {}
for i in range(COLUMNS):
    p = COLUMN_PROFILES[i]
    x = SPACING * i
    for j in range(ROWS):
        start = grid[(i, j)]["radius"]
        y0 = SPACING * j + start
        if j < ROWS - 1:
            length = SPACING - start - grid[(i, j + 1)]["radius"]
        elif i == COLUMNS - 1:
            ring_entries["south"] = (i, j, p)
            continue
        elif i == COLUMNS - 2:
            ring_entries["tee"] = (i, j, p)
            continue
        else:
            length = STUB
        s = street(Geometry(x, y0, math.pi / 2, length), p)
        column_roads[(i, j)] = s
        attach(grid[(i, j)], s, "start")
        if j < ROWS - 1:
            attach(grid[(i, j + 1)], s, "end")

diagonal_roads = []
for k in range(ROWS - 1):
    a, b = grid[(k, k)], grid[(k + 1, k + 1)]
    offset = a["radius"] / math.sqrt(2)
    length = SPACING * math.sqrt(2) - a["radius"] - b["radius"]
    s = street(Geometry(SPACING * k + offset, SPACING * k + offset, math.pi / 4, length), DIAGONAL)
    diagonal_roads.append(s)
    attach(a, s, "start")
    attach(b, s, "end")


def hw(p):
    return max(width(p["left"]), width(p["right"]))


cx, cy = ring_centre
entry_specs = [
    ("east", 0.0, LOCAL, "out"),
    ("north", math.pi / 2, LOCAL, "out"),
    ("west", math.pi, LOCAL, "out"),
    ("diagonal", 5 * math.pi / 4, DIAGONAL, "in"),
    ("south", 3 * math.pi / 2, COLUMN_PROFILES[COLUMNS - 1], "in"),
]
ring_outer = width(RING_ROAD["right"])
entry_reach = RING + ring_outer + ENTRY_GAP
ring_nodes = []
for name, angle, p, way in entry_specs:
    centre = (cx + (RING + LANE) * math.cos(angle), cy + (RING + LANE) * math.sin(angle))
    jn = node(centre, "ring", None)
    jn["angle"] = angle
    jn["half"] = (hw(p) + 10.0) / RING
    ring_nodes.append(jn)
    ex, ey = cx + entry_reach * math.cos(angle), cy + entry_reach * math.sin(angle)
    if name == "west":
        i, j, column = ring_entries["tee"]
        tee = node((SPACING * i, ey), "lights", GRID_RADIUS)
        s = street(Geometry(ex, ey, angle, ex - SPACING * i - GRID_RADIUS), p)
        attach(jn, s, "start")
        attach(tee, s, "end")
        s = street(Geometry(SPACING * i - GRID_RADIUS, ey, angle, STUB), p)
        attach(tee, s, "start")
        bottom = grid[(i, j)]
        y0 = SPACING * j + bottom["radius"]
        s = street(Geometry(SPACING * i, y0, math.pi / 2, ey - GRID_RADIUS - y0), column)
        column_roads[(i, j)] = s
        attach(bottom, s, "start")
        attach(tee, s, "end")
    elif way == "out":
        s = street(Geometry(ex, ey, angle, STUB), p)
        attach(jn, s, "start")
    elif name == "diagonal":
        top = grid[(ROWS - 1, ROWS - 1)]
        offset = SPACING * (ROWS - 1) + top["radius"] / math.sqrt(2)
        length = (ex - offset) * math.sqrt(2)
        s = street(Geometry(offset, offset, math.pi / 4, length), DIAGONAL)
        diagonal_roads.append(s)
        attach(top, s, "start")
        attach(jn, s, "end")
    else:
        i, j, _ = ring_entries["south"]
        bottom = grid[(i, j)]
        y0 = SPACING * j + bottom["radius"]
        s = street(Geometry(SPACING * i, y0, math.pi / 2, ey - y0), p)
        column_roads[(i, j)] = s
        attach(bottom, s, "start")
        attach(jn, s, "end")

ring_nodes.sort(key=lambda n: n["angle"])
ring_roads = []
for k, a in enumerate(ring_nodes):
    b = ring_nodes[(k + 1) % len(ring_nodes)]
    start = a["angle"] + a["half"]
    stop = b["angle"] - b["half"]
    if stop <= start:
        stop += 2 * math.pi
    g = Geometry(cx + RING * math.cos(start), cy + RING * math.sin(start), start + math.pi / 2,
                 RING * (stop - start), 1.0 / RING)
    s = street(g, RING_ROAD)
    ring_roads.append(s)
    attach(a, s, "start")
    attach(b, s, "end")


def contact(arm):
    g = arm["street"].geometry
    if arm["end"] == "end":
        x, y, h = g.pose(g.length)
        return x, y, h
    x, y, h = g.pose(0.0)
    return x, y, h + math.pi


def wrap(a):
    return (a + math.pi) % (2 * math.pi) - math.pi


def side_of(arm, way):
    """The side whose lanes run `way`, "in" to the junction or "out"."""
    into = "right" if arm["end"] == "end" else "left"
    return into if way == "in" else ("left" if into == "right" else "right")


def lane_ids(arm, name, kinds):
    sign = -1 if name == "right" else 1
    lanes = arm["street"].profile[name]
    return [(sign * (k + 1), kind) for k, (kind, _) in enumerate(lanes) if kind in kinds]


corner_roads = {}
creators = []
for number, jn in enumerate(junctions, start=1):
    jn["id"] = number
    jx, jy = jn["centre"]
    for arm in jn["arms"]:
        x, y, h = contact(arm)
        arm["angle"] = math.atan2(y - jy, x - jx)
        arm["into"] = h
    jn["arms"].sort(key=lambda arm: arm["angle"])
    creator = xodr.CommonJunctionCreator(number, f"junction {number}", startnum=1000 * number)
    for arm in jn["arms"]:
        x, y, h = contact(arm)
        connection = "successor" if arm["end"] == "end" else "predecessor"
        creator.add_incoming_road_cartesian_geometry(arm["street"].road, x - jx, y - jy, h, connection)
    creator.add_constant_elevation(terrain(jy))
    arms = jn["arms"]
    for a in arms:
        ins = {kind: [i for i, k in lane_ids(a, side_of(a, "in"), (kind,))] for kind in CONNECTED}
        for b in arms:
            if b is a:
                continue
            outs = {kind: [i for i, k in lane_ids(b, side_of(b, "out"), (kind,))] for kind in CONNECTED}
            turn = wrap(b["into"] + math.pi - a["into"])
            if abs(turn) > math.radians(150):
                continue
            pairs = []
            if abs(turn) < math.radians(35):
                for kind in CONNECTED:
                    pairs += list(zip(ins[kind], outs[kind]))
            elif ins["driving"] and outs["driving"]:
                pick = -1 if turn < 0 else 0
                pairs = [(ins["driving"][pick], outs["driving"][pick])]
            if pairs:
                creator.add_connection(a["street"].id, b["street"].id,
                                       [p[0] for p in pairs], [p[1] for p in pairs])
    corners = []
    for k, a in enumerate(arms):
        b = arms[(k + 1) % len(arms)]
        facing_a = "right" if a["end"] == "end" else "left"
        facing_b = "left" if b["end"] == "end" else "right"
        la = dict((kind, i) for i, kind in lane_ids(a, facing_a, ("curb", "sidewalk")))
        lb = dict((kind, i) for i, kind in lane_ids(b, facing_b, ("curb", "sidewalk")))
        kinds = [kind for kind in ("curb", "sidewalk") if kind in la and kind in lb]
        creator.add_connection(a["street"].id, b["street"].id,
                               [la[kind] for kind in kinds], [lb[kind] for kind in kinds])
        made = creator.junction_roads[-len(kinds):]
        for road, kind in zip(made, kinds):
            section = road.lanes.lanesections[0]
            for lane in section.leftlanes + section.rightlanes:
                lane.lane_type = LANE_TYPES[kind]
                lane.add_height(*HEIGHTS[kind])
        corners.append((made[-1].id, kinds[-1]))
    corner_roads[number] = corners
    creators.append(creator)
    odr.add_junction_creator(creator)

odr.adjust_roads_and_lanes()
out = Path(__file__).with_suffix(".xodr")
odr.write_xml(str(out))
laid = {r.get("id"): r for r in ET.parse(out).getroot().findall("road")}


def edge_of(road, lane_id):
    """The outer border of lane `lane_id` of `road`, as written, every 0.5 m."""
    plan = road.find("planView")
    offsets = [(float(o.get("s")), [float(o.get(k)) for k in "abcd"]) for o in road.iter("laneOffset")]
    lane = next(l for l in road.iter("lane") if l.get("id") == str(lane_id))
    w = lane.find("width")
    widths = [float(w.get(k)) for k in "abcd"]
    flip = 1 if lane_id > 0 else -1
    points = []
    for geom in plan.findall("geometry"):
        s0, x, y, h = (float(geom.get(k)) for k in ("s", "x", "y", "hdg"))
        length = float(geom.get("length"))
        shape = geom[0]
        if shape.tag == "spiral":
            k0, k1 = float(shape.get("curvStart")), float(shape.get("curvEnd"))
        elif shape.tag == "arc":
            k0 = k1 = float(shape.get("curvature"))
        else:
            k0 = k1 = 0.0
        steps = max(2, math.ceil(length / 0.05))
        ds = length / steps
        for n in range(steps + 1):
            if n % 10 == 0 or n == steps:
                s = s0 + n * ds
                o = max((r for r in offsets if r[0] <= s + 1e-9), default=(0.0, [0.0] * 4), key=lambda r: r[0])
                d = s - o[0]
                base = o[1][0] + o[1][1] * d + o[1][2] * d * d + o[1][3] * d ** 3
                t = base + flip * (widths[0] + widths[1] * s + widths[2] * s * s + widths[3] * s ** 3)
                points.append((x - t * math.sin(h), y + t * math.cos(h)))
            if n < steps:
                k = k0 + (k1 - k0) * (n + 0.5) / steps
                h_mid = h + k * ds / 2
                x += ds * math.cos(h_mid)
                y += ds * math.sin(h_mid)
                h += k * ds
    return points


corner_edges = {}
for number, corners in corner_roads.items():
    for k, (road_id, kind) in enumerate(corners):
        road = laid[str(road_id)]
        lane = next(l for l in road.iter("lane") if l.get("type") == kind)
        corner_edges[(number, k)] = edge_of(road, int(lane.get("id")))


def new_id():
    return str(next(object_ids))


def obj(street_, s, t, kind, **kw):
    o = xodr.Object(s, t, kind, id=new_id(), **kw)
    street_.road.add_object(o)
    return o


def sign(street_, s, t, orientation, country, kind, subtype, name, semantics, **kw):
    sid = f"sign{next(object_ids)}"
    street_.road.add_signal(xodr.Signal(s=s, t=t, country=country, Type=kind, subtype=subtype,
                                        id=sid, name=name, orientation=orientation, **kw))
    semantic_of[sid] = semantics
    return sid


semantic_of = {}
lights_of = {}
crossing_arms = []

for jn in junctions:
    groups = {}
    for arm in jn["arms"]:
        s_ = arm["street"]
        p = s_.profile
        g = s_.geometry
        if s_ in ring_roads:
            continue
        crossing_arms.append(arm)
        at_end = arm["end"] == "end"
        into = side_of(arm, "in")
        sign_t = -1 if into == "right" else 1
        orientation = xodr.Orientation.positive if at_end else xodr.Orientation.negative
        drive = [k for k, _ in p[into] if k == "driving"]
        middle = g.length - 2.5 if at_end else 2.5
        crossing = xodr.Object(middle, 0.0, xodr.ObjectType.crosswalk, id=f"crossing{s_.id}{arm['end'][0]}")
        t = -carriageway(p["right"])
        stripe = 0
        while t + STRIPE <= carriageway(p["left"]) + 1e-9:
            outline = xodr.Outline(closed=True, id=stripe)
            for ds, dt in ((-1.5, 0), (1.5, 0), (1.5, STRIPE), (-1.5, STRIPE)):
                outline.add_corner(xodr.CornerRoad(middle + ds, t + dt, 0, 0))
            crossing.add_outline(outline)
            t += 2 * STRIPE
            stripe += 1
        s_.road.add_object(crossing)
        if not drive:
            continue
        edge = width(p[into], "sidewalk")
        inner = width(p[into], "driving")
        s_stop = g.length - STOP if at_end else STOP
        if jn["kind"] == "lights":
            light = sign(s_, g.length - STOP - 0.5 if at_end else STOP + 0.5, sign_t * (edge + 0.7),
                         orientation, "OpenDRIVE", "1000001", "-1", f"light {s_.id}",
                         ("priority", "trafficLight"), dynamic=xodr.Dynamic.yes,
                         zOffset=2.5, height=1.15, width=0.38)
            sign(s_, s_stop, sign_t * (inner + len(drive) * LANE / 2), orientation, "OpenDRIVE", "294",
                 "-1", "stop line", ("priority", "stopLine"), zOffset=0.0, height=0.03,
                 width=len(drive) * LANE)
            axis = round(math.degrees(arm["angle"]) / 45.0) % 4
            groups.setdefault(axis, []).append(light)
        else:
            s_sign = g.length - STOP if at_end else STOP
            sign(s_, s_sign, sign_t * (edge + 0.7), orientation, "DE", "205", "-1", "give way",
                 ("priority", "yield"), zOffset=2.2, height=0.9, width=0.9)
            sign(s_, s_sign, sign_t * (edge + 0.7), orientation, "DE", "215", "-1", "roundabout",
                 None, zOffset=1.4, height=0.6, width=0.6)
    lights_of[jn["id"]] = groups


def speed_signs(s_):
    p, g = s_.profile, s_.geometry
    limit = p["limit"]
    for name, s, orientation, flip in (("right", 8.0, xodr.Orientation.positive, -1),
                                       ("left", g.length - 8.0, xodr.Orientation.negative, 1)):
        if not any(k == "driving" for k, _ in p[name]) or g.length < 20.0:
            continue
        sign(s_, s, flip * (width(p[name], "sidewalk") + 0.6), orientation, "DE", "274",
             "55" if limit == TALL_LIMIT else "53", f"{limit} km/h", ("speed", limit),
             value=limit, unit="km/h", zOffset=2.0, height=0.6, width=0.6)


def furnish(s_):
    p, g = s_.profile, s_.geometry
    if g.length < 24.0:
        return
    for name, flip in (("right", -1), ("left", 1)):
        lanes = p[name]
        if not any(k == "sidewalk" for k, _ in lanes):
            continue
        kerb = width(lanes, "sidewalk")
        walk = dict(lanes)["sidewalk"]
        s, k = 12.0, 0
        while s < g.length - 12.0:
            if walk > WALK:
                obj(s_, s, flip * (kerb + walk - 2.0), xodr.ObjectType.tree, subtype="palm",
                    radius=0.25, height=8.0)
                obj(s_, s, flip * (kerb + walk - 2.0), xodr.ObjectType.vegetation, subtype="palm crown",
                    radius=2.4, height=1.4, zOffset=7.6)
            if k % 2 == 0:
                obj(s_, s, flip * (kerb + 0.6), xodr.ObjectType.streetLamp, radius=0.1, height=8.0)
            else:
                obj(s_, s, flip * (kerb + 1.2), xodr.ObjectType.tree, radius=0.18, height=2.6)
                obj(s_, s, flip * (kerb + 1.2), xodr.ObjectType.vegetation, subtype="crown",
                    radius=1.7, height=3.6, zOffset=2.3)
            if k % 3 == 1:
                obj(s_, s + 4.0, flip * (kerb + 3.6), xodr.ObjectType.obstacle, subtype="bench",
                    length=1.8, width=0.6, height=0.45)
                obj(s_, s + 6.5, flip * (kerb + 3.8), xodr.ObjectType.obstacle, subtype="bin",
                    radius=0.3, height=1.0)
            s += 15.0
            k += 1
        if p["limit"] == TALL_LIMIT and g.length > 60.0:
            obj(s_, g.length / 2 + 7.5, flip * (kerb + 3.4), xodr.ObjectType.obstacle,
                subtype="busShelter", length=5.0, width=1.6, height=2.6)
        if any(k == "parking" for k, _ in lanes):
            t = flip * (width(lanes, "parking") + PARKING / 2)
            s = 12.0
            while s < g.length - 12.0:
                if rng.random() < 0.65:
                    obj(s_, s, t, xodr.ObjectType.obstacle, subtype="car",
                        length=4.4, width=1.8, height=1.45)
                s += 5.8
    if p["left"][0][0] == "median":
        s = 12.0
        while s < g.length - 12.0:
            obj(s_, s, 0.0, xodr.ObjectType.tree, radius=0.18, height=2.6)
            obj(s_, s, 0.0, xodr.ObjectType.vegetation, subtype="crown", radius=1.4, height=3.4, zOffset=2.3)
            s += 12.0


for s_ in streets:
    if s_ in ring_roads:
        continue
    speed_signs(s_)
    furnish(s_)


def clip(poly, a, n):
    """The part of `poly` on the side of the line through `a` that `n` points to."""
    out = []
    side_ = lambda p: (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1]
    for k, p in enumerate(poly):
        q = poly[(k + 1) % len(poly)]
        sp, sq = side_(p), side_(q)
        if sp >= 0:
            out.append(p)
        if (sp >= 0) != (sq >= 0):
            f = sp / (sp - sq)
            out.append((p[0] + f * (q[0] - p[0]), p[1] + f * (q[1] - p[1])))
    return out


def round_corner(poly, a, b, curve):
    """`poly` with its edge from near `a` to near `b`, the chord a corner was
    cut along, replaced by `curve`, the corner's sidewalk edge, which runs
    from `a` to `b`."""
    for k, p in enumerate(poly):
        q = poly[(k + 1) % len(poly)]
        for first, last, points in ((a, b, curve), (b, a, curve[::-1])):
            if math.dist(p, first) < 0.5 and math.dist(q, last) < 0.5:
                return poly[:k + 1] + points[1:-1] + poly[k + 1:]
    return poly


def area(poly):
    return 0.5 * sum(p[0] * q[1] - q[0] * p[1] for p, q in zip(poly, poly[1:] + poly[:1]))


def centroid(poly):
    """The centre of `poly`'s area, which its vertices' mean is not where a
    rounded corner crowds them."""
    a = area(poly)
    if abs(a) < 1e-9:
        return (sum(p[0] for p in poly) / len(poly), sum(p[1] for p in poly) / len(poly))
    cx_ = sum((p[0] + q[0]) * (p[0] * q[1] - q[0] * p[1]) for p, q in zip(poly, poly[1:] + poly[:1]))
    cy_ = sum((p[1] + q[1]) * (p[0] * q[1] - q[0] * p[1]) for p, q in zip(poly, poly[1:] + poly[:1]))
    return (cx_ / (6 * a), cy_ / (6 * a))


def column_x(i):
    return SPACING * i


def row_y(j):
    return SPACING * j


def extent(p, name):
    return width(p[name])


blocks = []
for j in range(ROWS):
    for i in range(-1, COLUMNS):
        x_lo = column_x(i) + extent(COLUMN_PROFILES[i], "right") if i >= 0 else column_x(0) - 100.0
        x_hi = column_x(i + 1) - extent(COLUMN_PROFILES[i + 1], "left") if i + 1 < COLUMNS else column_x(COLUMNS - 1) + 100.0
        y_lo = row_y(j) + extent(ROW_PROFILES[j], "left")
        if j + 1 < ROWS:
            y_hi = row_y(j + 1) - extent(ROW_PROFILES[j + 1], "right")
        else:
            y_hi = 428.0 if i >= ROWS - 1 else NORTH_EDGE
        poly = [(x_lo, y_lo), (x_hi, y_lo), (x_hi, y_hi), (x_lo, y_hi)]
        if i == j:
            d = width(DIAGONAL["left"]) * math.sqrt(2)
            blocks.append({"poly": clip(poly, (d, 0.0), (1.0, -1.0)), "cell": (i, j)})
            blocks.append({"poly": clip(poly, (0.0, d), (-1.0, 1.0)), "cell": (i, j)})
        else:
            blocks.append({"poly": poly, "cell": (i, j)})

for jn in junctions:
    if jn["kind"] != "lights":
        continue
    jx, jy = jn["centre"]
    arms = jn["arms"]
    for k, a in enumerate(arms):
        b = arms[(k + 1) % len(arms)]
        gap = (b["angle"] - a["angle"]) % (2 * math.pi)
        if gap > math.pi - 1e-6:
            continue
        ends = []
        for arm, name in ((a, "right" if a["end"] == "end" else "left"),
                          (b, "left" if b["end"] == "end" else "right")):
            g = arm["street"].geometry
            s = g.length if arm["end"] == "end" else 0.0
            t = width(arm["street"].profile[name]) * (-1 if name == "right" else 1)
            ends.append(g.point(s, t))
        (ax, ay), (bx, by) = ends
        n = (by - ay, -(bx - ax))
        if (jx - ax) * n[0] + (jy - ay) * n[1] > 0:
            n = (-n[0], -n[1])
        curve = corner_edges[(jn["id"], k)]
        if math.dist(curve[0], (bx, by)) < math.dist(curve[0], (ax, ay)):
            curve = curve[::-1]
        curve = [(px + 0.05 * (px - jx) / math.dist((px, py), (jx, jy)),
                  py + 0.05 * (py - jy) / math.dist((px, py), (jx, jy))) for px, py in curve]
        for block in blocks:
            if len(block["poly"]) < 3:
                continue
            bxc, byc = centroid(block["poly"])
            angle = math.atan2(byc - jy, bxc - jx)
            within = (angle - a["angle"]) % (2 * math.pi) < gap
            if within and math.hypot(bxc - jx, byc - jy) < 150.0:
                block["poly"] = round_corner(clip(block["poly"], (ax, ay), n), (ax, ay), (bx, by), curve)

blocks = [b for b in blocks if len(b["poly"]) >= 3 and area(b["poly"]) > 50.0]
PARK = (0, 2)


def edge_distance(a, b, p):
    dx, dy = b[0] - a[0], b[1] - a[1]
    f = max(0.0, min(1.0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy or 1e-12)))
    return math.hypot(p[0] - a[0] - f * dx, p[1] - a[1] - f * dy)


def inside(poly, p, margin=0.0):
    """Whether `p` lies in `poly`, at least `margin` from its edges."""
    crossings = False
    for k, a in enumerate(poly):
        b = poly[(k + 1) % len(poly)]
        if (a[1] > p[1]) != (b[1] > p[1]) and p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]:
            crossings = not crossings
        if edge_distance(a, b, p) < margin:
            return False
    return crossings


def within(poly, corners, margin):
    """Whether the convex `corners` lie in `poly`, `margin` from its edges."""
    return all(inside(poly, p, margin) for p in corners) and \
        not any(inside(corners, v) for v in poly)


def separated(p, q):
    for poly in (p, q):
        for k, a in enumerate(poly):
            b = poly[(k + 1) % len(poly)]
            n = (b[1] - a[1], a[0] - b[0])
            pa = [x * n[0] + y * n[1] for x, y in p]
            qa = [x * n[0] + y * n[1] for x, y in q]
            if max(pa) <= min(qa) or max(qa) <= min(pa):
                return True
    return False


for block in blocks:
    if area(block["poly"]) < 0:
        block["poly"].reverse()

footprints = []


def place(s_, s, t, along, depth, height):
    g = s_.geometry
    x, y = g.point(s, t)
    h = g.pose(s)[2]
    c, sn = math.cos(h), math.sin(h)
    corners = [(x + u * c - v * sn, y + u * sn + v * c)
               for u, v in ((-along / 2, -depth / 2), (along / 2, -depth / 2),
                            (along / 2, depth / 2), (-along / 2, depth / 2))]
    host = next((b for b in blocks if within(b["poly"], corners, 0.3)), None)
    if host is None or host["cell"] == PARK:
        return False
    if any(not separated(corners, f) for f in footprints):
        return False
    footprints.append(corners)
    obj(s_, s, t, xodr.ObjectType.building, length=along, width=depth,
        height=round(height + 1.0, 1), zOffset=-1.0)
    return True


towers = row_roads[(3, 0)]
for frac, height in ((0.3, 154.0), (0.72, 142.0)):
    place(towers, towers.geometry.length * frac, extent(SEAFRONT, "left") + 2.0 + 15.0, 30.0, 30.0, height)

for s_ in streets:
    if s_ in ring_roads:
        continue
    p, g = s_.profile, s_.geometry
    tall = 26.0 if p["limit"] == TALL_LIMIT else 21.0
    for name, flip in (("right", -1), ("left", 1)):
        reach = width(p[name])
        s = 2.0
        while s < g.length - 2.0:
            along = rng.uniform(8.0, 18.0)
            if s + along > g.length:
                break
            depth = rng.uniform(14.0, 20.0)
            height = rng.uniform(tall * 0.8, tall * 1.2)
            if rng.random() < 0.06:
                height *= 2.2
            if place(s_, s + along / 2, flip * (reach + 0.6 + depth / 2), along, depth, height):
                s += along + rng.uniform(0.0, 0.8)
            else:
                s += 2.0


def host_column(block):
    x, y = centroid(block["poly"])
    best = None
    for (i, j), s_ in column_roads.items():
        g = s_.geometry
        if not g.y - 1.0 <= y <= g.y + g.length + 1.0:
            continue
        d = abs(x - g.x)
        if best is None or d < best[0]:
            best = (d, s_)
    return best[1] if best else None


patches = []
for block in blocks:
    s_ = host_column(block)
    if s_ is None:
        continue
    g = s_.geometry
    x, y = centroid(block["poly"])
    name = "right" if x > g.x else "left"
    t0 = width(s_.profile[name]) * (-1 if name == "right" else 1)
    s0 = min(max(y - g.y, 2.0), g.length - 2.0)
    ox, oy = g.point(s0, t0)
    park = block["cell"] == PARK
    garden = park or rng.random() < 0.3
    fill = xodr.FillType.grass if garden else xodr.FillType.pavement
    patch = xodr.Object(s0, t0, xodr.ObjectType.patch, id=new_id(), subtype="park" if park else "courtyard")
    outline = xodr.Outline(closed=True, fill_type=fill, id=0)
    for px, py in block["poly"]:
        u, v = g.local(s0, t0, px, py)
        outline.add_corner(xodr.CornerLocal(u, v, terrain(py) - terrain(oy), 0))
    patch.add_outline(outline)
    s_.road.add_object(patch)
    if garden:
        bx0 = min(p[0] for p in block["poly"])
        bx1 = max(p[0] for p in block["poly"])
        by0 = min(p[1] for p in block["poly"])
        by1 = max(p[1] for p in block["poly"])
        step = 11.0 if park else 15.0
        middle = centroid(block["poly"])
        clear = FOUNTAIN + CROWN + 1.0 if park else 0.0
        yy = by0 + step / 2
        while yy < by1:
            xx = bx0 + step / 2
            while xx < bx1:
                spot = [(xx - 2.5, yy - 2.5), (xx + 2.5, yy - 2.5), (xx + 2.5, yy + 2.5), (xx - 2.5, yy + 2.5)]
                if inside(block["poly"], (xx, yy), 3.0) and all(separated(spot, f) for f in footprints) \
                        and math.dist((xx, yy), middle) > clear and (park or rng.random() < 0.4):
                    s = yy - g.y
                    t = -(xx - g.x)
                    obj(s_, s, t, xodr.ObjectType.tree, radius=0.22, height=3.0)
                    obj(s_, s, t, xodr.ObjectType.vegetation, subtype="crown", radius=CROWN, height=4.0, zOffset=2.6)
                xx += step
            yy += step
    if park:
        mx, my = middle
        obj(s_, my - g.y, -(mx - g.x), xodr.ObjectType.obstacle, subtype="fountain",
            radius=FOUNTAIN, height=0.7)

seafront = row_roads[(0, 0)]
g = seafront.geometry
edge = width(SEAFRONT["right"])
s0 = g.length / 2
ox, oy = g.point(s0, -edge)
beach = xodr.Object(s0, -edge, xodr.ObjectType.patch, id=new_id(), subtype="beach")
outline = xodr.Outline(closed=True, fill_type=xodr.FillType.gravel, id=0)
for px, py in ((-100.0, -edge - 40.0), (640.0, -edge - 40.0), (640.0, -edge), (-100.0, -edge)):
    u, v = g.local(s0, -edge, px, py)
    outline.add_corner(xodr.CornerLocal(u, v, 0, 0))
beach.add_outline(outline)
seafront.road.add_object(beach)
for i in range(-1, COLUMNS):
    front = row_roads[(i, 0)]
    obj(front, front.geometry.length / 2, -edge - 22.0, xodr.ObjectType.obstacle,
        subtype="lifeguardTower", length=2.0, width=2.0, height=4.0)

island = ring_roads[0]
g = island.geometry
s0 = g.length / 2
inner = RING - KERB - 0.3
ox, oy = g.point(s0, KERB)
patch = xodr.Object(s0, KERB, xodr.ObjectType.patch, id=new_id(), subtype="island")
outline = xodr.Outline(closed=True, fill_type=xodr.FillType.grass, id=0)
for k in range(72):
    a = 2 * math.pi * k / 72
    px, py = cx + inner * math.cos(a), cy + inner * math.sin(a)
    u, v = g.local(s0, KERB, px, py)
    outline.add_corner(xodr.CornerLocal(u, v, terrain(py) - terrain(oy), 0))
patch.add_outline(outline)
island.road.add_object(patch)
obj(island, s0, RING, xodr.ObjectType.obstacle, subtype="fountain", radius=16.0, height=0.6)
obj(island, s0, RING, xodr.ObjectType.obstacle, subtype="monument", radius=2.5, height=46.0)
for s_ in ring_roads:
    gg = s_.geometry
    s = 4.0
    while s < gg.length - 4.0:
        for t in (12.0, 26.0, 40.0):
            obj(s_, s, t, xodr.ObjectType.tree, radius=0.22, height=3.0)
            obj(s_, s, t, xodr.ObjectType.vegetation, subtype="crown", radius=2.3, height=4.0, zOffset=2.6)
        s += 14.0

odr.write_xml(str(out))

tree = ET.parse(out)
root = tree.getroot()
by_id = {r.get("id"): r for r in root.findall("road")}

for thing in root.iter():
    if thing.get("subtype") == "None":
        del thing.attrib["subtype"]

for road in root.findall("road"):
    if road.get("junction") != "-1":
        for lane in road.iter("lane"):
            for mark in lane.findall("roadMark"):
                lane.remove(mark)

ends = {}
for arm in crossing_arms:
    ends.setdefault(str(arm["street"].id), set()).add(arm["end"])
for road in root.findall("road"):
    if road.get("id") not in ends:
        continue
    length = float(road.get("length"))
    for lane in road.iter("lane"):
        marks = lane.findall("roadMark")
        if not marks:
            continue
        mark = marks[0]
        at = list(lane).index(mark)
        if "start" in ends[road.get("id")]:
            lane.insert(at, ET.Element("roadMark", sOffset="0", type="none", color="standard"))
            mark.set("sOffset", f"{STOP}")
            at += 1
        if "end" in ends[road.get("id")]:
            lane.insert(at + 1, ET.Element("roadMark", sOffset=f"{length - STOP}", type="none",
                                           color="standard"))

for crossing in root.iter("object"):
    if crossing.get("type") == "crosswalk":
        for outline in crossing.iter("outline"):
            outline.set("fillType", "paint")

for sid, semantics in semantic_of.items():
    if semantics is None:
        continue
    signal = root.find(f".//signal[@id='{sid}']")
    holder = ET.SubElement(signal, "semantics")
    kind, value = semantics
    if kind == "speed":
        ET.SubElement(holder, "speed", type="maximum", value=str(value), unit="km/h")
    else:
        ET.SubElement(holder, "priority", type=value)

controllers = []
for jn in junctions:
    for axis, lights in sorted(lights_of.get(jn["id"], {}).items()):
        controllers.append((f"{jn['id']}{axis}", jn["id"], lights))
first_junction = list(root).index(root.find("junction"))
for offset, (cid, _, lights) in enumerate(controllers):
    controller = ET.Element("controller", id=cid, name=f"phase {cid}")
    for light in lights:
        ET.SubElement(controller, "control", signalId=light, type="0")
    root.insert(first_junction + offset, controller)

for jn in junctions:
    junction = root.find(f"junction[@id='{jn['id']}']")
    for sequence, (cid, owner, _) in enumerate(c for c in controllers if c[1] == jn["id"]):
        ET.SubElement(junction, "controller", id=cid, type="0", sequence=str(sequence + 1))
    jx, jy = jn["centre"]
    reach = 5.0 * math.ceil((max(math.hypot(*(lambda c: (c[0] - jx, c[1] - jy))(contact(a)[:2]))
                                 for a in jn["arms"]) + 24.0) / 5.0)
    plan = ET.SubElement(junction, "planView")
    ET.SubElement(ET.SubElement(plan, "geometry", s="0", x=f"{jx - reach}", y=f"{jy}",
                                hdg="0", length=f"{2 * reach}"), "line")
    boundary = ET.SubElement(junction, "boundary")
    for k, arm in enumerate(jn["arms"]):
        p = arm["street"].profile
        ET.SubElement(boundary, "segment", type="joint", roadId=str(arm["street"].id),
                      contactPoint=arm["end"], jointLaneStart=str(len(p["left"])),
                      jointLaneEnd=str(-len(p["right"])))
        corner_id, kind = corner_roads[jn["id"]][k]
        corner = by_id[str(corner_id)]
        lane = next(l for l in corner.iter("lane") if l.get("type") == kind)
        ET.SubElement(boundary, "segment", type="lane", roadId=corner.get("id"),
                      boundaryLane=lane.get("id"), sStart="0", sEnd=corner.get("length"))
    spacing = 5.0
    count = int(reach / spacing)
    grid_node = ET.SubElement(junction, "elevationGrid", gridSpacing=f"{spacing}", sStart="0")
    for row in range(2 * count + 1):
        heights = lambda sign_: " ".join(f"{terrain(jy + sign_ * k * spacing):.3f}" for k in range(1, count + 1))
        ET.SubElement(grid_node, "elevation", center=f"{terrain(jy):.3f}", left=heights(1), right=heights(-1))

ET.indent(tree, "    ")
tree.write(out, encoding="utf-8", xml_declaration=True)
