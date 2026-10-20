# /// script
# requires-python = ">=3.12"
# dependencies = ["scenariogeneration==0.16.6"]
# ///
"""Generate objects.xodr: an arc road with objects placed along it, and a
straight road that places some of them again by reference.

Road 0 is a 100 m left arc of radius 200 m starting at the origin heading +X,
climbing at 2 %. Road 1 is a flat 60 m straight from (0, -40) heading +X, and
road 2 a 40 m straight from (0, -80) banked at 0.15 rad. So every placement
has a closed form to check against.

scenariogeneration has no `<objectReference>`, `<borders>` or `<bridge>`, so
those are added to its output afterwards, as text.
"""

from pathlib import Path

from scenariogeneration import xodr

road = xodr.create_road(xodr.Arc(0.005, length=100), id=0, left_lanes=1, right_lanes=1)
road.planview.set_start_point(0, 0, 0)
road.add_elevation(0, 1.0, 0.02, 0, 0)

# A box, turned against the road, with a subtype, valid for traffic along +s
# over 8 m, and only for lane -1.
shed = xodr.Object(s=40, t=-6, Type=xodr.ObjectType.building, subtype="garage", id="1",
                   name="Shed", zOffset=0.5, hdg=0.3, length=8, width=4, height=3,
                   orientation=xodr.Orientation.positive, validLength=8)
shed.add_validity(-1, -1)
road.add_object(
    [
        shed,
        # A cylinder, pitched and rolled.
        xodr.Object(s=60, t=5, Type=xodr.ObjectType.tree, id="2", radius=1.5, height=7,
                    pitch=0.1, roll=-0.2),
        # No size at all.
        xodr.Object(s=10, t=0, Type=xodr.ObjectType.none, id="3"),
        # A type OpenDRIVE does not define.
        xodr.Object(s=20, t=4, Type="guide-post", id="4", height=1.2),
        # Something that moves, for traffic along -s.
        xodr.Object(s=90, t=-3, Type=xodr.ObjectType.barrier, subtype="boom", id="10",
                    name="Gate", dynamic=xodr.Dynamic.yes, length=4, width=0.2, height=1,
                    orientation=xodr.Orientation.negative),
    ]
)

# A row of posts drifting outward: s = 5, 15, ..., 85, t from -4 to -6.
posts = xodr.Object(s=0, t=0, Type=xodr.ObjectType.pole, id="5", radius=0.1, height=1)
posts.repeat(repeatLength=80, repeatDistance=10, sStart=5, tStart=-4, tEnd=-6,
             heightStart=1, heightEnd=2, zOffsetStart=0, zOffsetEnd=0)
road.add_object(posts)

# A continuous railing with no width: a wall swept along the road.
rail = xodr.Object(s=0, t=7, Type=xodr.ObjectType.railing, id="6", height=0.8)
rail.repeat(repeatLength=100, repeatDistance=0, sStart=0, tStart=7, tEnd=7,
            heightStart=0.8, heightEnd=0.8, zOffsetStart=0, zOffsetEnd=0)
road.add_object(rail)

# A continuous barrier that widens, moves outward and rises over 40 m, and
# runs 10 m past the end of the road.
barrier = xodr.Object(s=0, t=0, Type=xodr.ObjectType.barrier, id="7")
barrier.repeat(repeatLength=40, repeatDistance=0, sStart=70, tStart=-8, tEnd=-9,
               widthStart=0.5, widthEnd=1.0, heightStart=1, heightEnd=1,
               zOffsetStart=0, zOffsetEnd=0.4)
road.add_object(barrier)

# A building outlined in its own frame: a 6 x 4 m footprint, 5 m tall,
# turned 0.2 rad against the road.
house = xodr.Object(s=30, t=10, Type=xodr.ObjectType.building, id="8", name="House",
                    zOffset=0.1, hdg=0.2)
footprint = xodr.Outline(closed=True)
for u, v in [(0, 0), (6, 0), (6, 4), (0, 4)]:
    footprint.add_corner(xodr.CornerLocal(u, v, 0, 5))
house.add_outline(footprint)
road.add_object(house)

# A fence outlined in road coordinates, open at the end.
fence = xodr.Object(s=50, t=-12, Type=xodr.ObjectType.barrier, id="9", name="Fence")
line = xodr.Outline(closed=False)
for s, t in [(50, -12), (60, -12), (60, -14)]:
    line.add_corner(xodr.CornerRoad(s, t, 0, 1.5))
fence.add_outline(line)
road.add_object(fence)

# A crosswalk across the road, outlined flat in road coordinates. One marking
# dashes across the road along its s = 13 edge. The other is solid and turns
# the corner from the s = 17 edge's far end back across.
crosswalk = xodr.Object(s=15, t=0, Type=xodr.ObjectType.crosswalk, id="11", name="Crossing")
area = xodr.Outline(closed=True)
for id, (s, t) in enumerate([(13, -3), (13, 3), (17, 3), (17, -3)]):
    area.add_corner(xodr.CornerRoad(s, t, 0, 0, id=id))
crosswalk.add_outline(area)
stripes = xodr.Marking(xodr.RoadMarkColor.white, lineLength=0.5, side="left",
                       spaceLength=0.5, startOffset=0.25, stopOffset=0.25, width=0.4,
                       zOffset=0.01)
stripes.add_cornerReference(0)
stripes.add_cornerReference(1)
crosswalk.add_marking(stripes)
edge = xodr.Marking(xodr.RoadMarkColor.yellow, lineLength=0, side="right", spaceLength=0,
                    startOffset=0, stopOffset=0, width=0.2)
for id in [1, 2, 3]:
    edge.add_cornerReference(id)
crosswalk.add_marking(edge)
crosswalk.add_material(surface="asphalt", friction=0.6)
road.add_object(crosswalk)

# A traffic island in the middle of the road, outlined in road coordinates,
# with borders added below.
island = xodr.Object(s=67, t=0, Type=xodr.ObjectType.trafficIsland, id="12", name="Island")
kerb = xodr.Outline(closed=True, id=0)
for id, (s, t) in enumerate([(64, -1), (70, -1), (70, 1), (64, 1)]):
    kerb.add_corner(xodr.CornerRoad(s, t, 0, 0.15, id=id))
island.add_outline(kerb)
island.add_material(surface="concrete", friction=0.7, roughness=0.02)
island.add_userdata(xodr.UserData("builder", "county roads"))
island.add_userdata(xodr.UserData("inspected", "2026-04"))
road.add_object(island)

# Two flat parking bays beside the road, one kept for disabled drivers and
# one for anyone for two hours. Markings with no corner references paint
# sides of each bay's box: the first's left and right in solid white, the
# second's rear dashed in yellow.
for id, s, access, restrictions, lines in [
        ("13", 26, xodr.Access.handicapped, None,
         [("left", xodr.RoadMarkColor.white, 0), ("right", xodr.RoadMarkColor.white, 0)]),
        ("14", 32, xodr.Access.all, "2 hours", [("rear", xodr.RoadMarkColor.yellow, 0.5)])]:
    bay = xodr.Object(s=s, t=-9, Type=xodr.ObjectType.parkingSpace, id=id, length=5.5,
                      width=2.5, height=0, hdg=1.5708)
    bay.add_parking_space(xodr.ParkingSpace(access, restrictions))
    for side, color, dash in lines:
        bay.add_marking(xodr.Marking(color, lineLength=dash, side=side, spaceLength=dash,
                                     startOffset=0, stopOffset=0, width=0.1))
    road.add_object(bay)

# A building round a courtyard, outlined in its own frame: a 10 x 8 m
# footprint, 4 m tall, with a 4 x 3 m well cut out of it. The well has a kerb
# round it, added below.
courtyard = xodr.Object(s=78, t=10, Type=xodr.ObjectType.building, id="15", name="Courtyard")
for id, outer, corners in [(0, True, [(0, 0), (10, 0), (10, 8), (0, 8)]),
                           (1, False, [(3, 2), (7, 2), (7, 5), (3, 5)])]:
    ring = xodr.Outline(closed=True, outer=outer, id=id)
    for u, v in corners:
        ring.add_corner(xodr.CornerLocal(u, v, 0, 4))
    courtyard.add_outline(ring)
road.add_object(courtyard)

# A pipe lying along the road, its radius growing from 0.3 m to 0.5 m. A
# round sweep has no use for the height.
pipe = xodr.Object(s=0, t=0, Type=xodr.ObjectType.obstacle, id="16", name="Pipe")
pipe.repeat(repeatLength=40, repeatDistance=0, sStart=5, tStart=-16, tEnd=-16,
            radiusStart=0.3, radiusEnd=0.5, heightStart=1, heightEnd=1,
            zOffsetStart=0, zOffsetEnd=0)
road.add_object(pipe)

# A board standing on its edge: pitched a right angle back, so its outline's
# u axis points up. 2 m tall, 1 m wide, and its height a 0.1 m thickness.
board = xodr.Object(s=20, t=-20, Type=xodr.ObjectType.obstacle, id="17", name="Board",
                    pitch=-1.5708)
face = xodr.Outline(closed=True)
for u, v in [(0, 0), (2, 0), (2, 1), (0, 1)]:
    face.add_corner(xodr.CornerLocal(u, v, 0, 0.1))
board.add_outline(face)
road.add_object(board)

# A tunnel over the last 25 m of the arc, both lanes.
road.add_tunnel(xodr.Tunnel(s=75, length=25, id="20", name="Hill",
                            tunnel_type=xodr.TunnelType.standard, daylight=0.1, lighting=0.8))

straight = xodr.create_road(xodr.Line(60), id=1, left_lanes=1, right_lanes=1)
straight.planview.set_start_point(0, -40, 0)

# A straight banked at 0.15 rad from (0, -80) heading +X, with a kiosk
# standing on it.
banked = xodr.create_road(xodr.Line(40), id=2, left_lanes=1, right_lanes=1)
banked.planview.set_start_point(0, -80, 0)
banked.add_superelevation(0, 0.15, 0, 0, 0)
banked.add_object(xodr.Object(s=20, t=-5, Type=xodr.ObjectType.building, id="30",
                              name="Kiosk", length=3, width=2, height=2.5, zOffset=0.2))

# A row of planters outlined in their own frame, 1.5 x 1 m and 0.6 m tall,
# repeated every 10 m across the bank at s = 5, 15, 25, 35.
planters = xodr.Object(s=5, t=5, Type=xodr.ObjectType.obstacle, id="31", name="Planter")
box = xodr.Outline(closed=True)
for u, v in [(0, 0), (1.5, 0), (1.5, 1), (0, 1)]:
    box.add_corner(xodr.CornerLocal(u, v, 0, 0.6))
planters.add_outline(box)
planters.repeat(repeatLength=30, repeatDistance=10, sStart=5, tStart=5, tEnd=5,
                heightStart=0.6, heightEnd=0.6)
banked.add_object(planters)

# A flat pad across the bank, outlined in road coordinates, with a kerb added
# below that has to tilt with the bank to stay on the road.
pad = xodr.Object(s=9, t=1.5, Type=xodr.ObjectType.patch, id="32", name="Pad")
slab = xodr.Outline(closed=True, id=0)
for id, (s, t) in enumerate([(6, 0.5), (12, 0.5), (12, 2.5), (6, 2.5)]):
    slab.add_corner(xodr.CornerRoad(s, t, 0, 0, id=id))
pad.add_outline(slab)
banked.add_object(pad)

odr = xodr.OpenDrive("objects")
odr.add_road(road)
odr.add_road(straight)
odr.add_road(banked)
odr.adjust_roads_and_lanes()
out = Path(__file__).with_name("objects.xodr")
odr.write_xml(str(out))


def insert(xml, anchor, after, text):
    """`xml` with `text` spliced in straight after the first `after` that
    follows `anchor`."""
    at = xml.index(after, xml.index(anchor)) + len(after)
    return xml[:at] + text + xml[at:]


# The island gets a curb all the way round, and paint along its s = 70 end.
xml = out.read_text()
xml = insert(xml, 'name="Island"', "</outlines>", """
                <borders>
                    <border width="0.3" type="curb" outlineId="0" useCompleteOutline="true"/>
                    <border width="0.5" type="paint" outlineId="0" useCompleteOutline="false">
                        <cornerReference id="1"/>
                        <cornerReference id="2"/>
                    </border>
                </borders>""")

xml = insert(xml, 'name="Courtyard"', "</outlines>", """
                <borders>
                    <border width="0.2" type="curb" outlineId="1" useCompleteOutline="true"/>
                </borders>""")

xml = insert(xml, 'name="Pad"', "</outlines>", """
                <borders>
                    <border width="0.6" type="curb" outlineId="0" useCompleteOutline="true"/>
                </borders>""")

# Road 1 places objects from road 0 again: the shed with its own zOffset,
# orientation, validLength and validity; the row of posts, which moves with the
# reference; the house, outlined in its own frame; the fence, outlined in road
# coordinates, which moves too; and an id that no object has.
references = [
    ('id="1" s="10" t="-5" zOffset="0.2" orientation="-" validLength="3"',
     '<validity fromLane="1" toLane="1"/>'),
    ('id="5" s="0" t="-1"', ""),
    ('id="8" s="30" t="8"', ""),
    ('id="9" s="20" t="12"', ""),
    ('id="99" s="40" t="0"', ""),
]


def reference(attrs, children):
    if not children:
        return f"\n            <objectReference {attrs}/>"
    return (f"\n            <objectReference {attrs}>"
            f"\n                {children}"
            "\n            </objectReference>")


# And a bridge carries road 1's lane -1 over s = 40..55.
bridge = """
            <bridge s="40" length="15" name="Creek" id="21" type="concrete">
                <validity fromLane="-1" toLane="-1"/>
            </bridge>"""
xml = insert(xml, '<road rule="RHT" id="1"', "</lanes>", "\n        <objects>" + "".join(
    reference(*r) for r in references) + bridge + "\n        </objects>")
out.write_text(xml)
