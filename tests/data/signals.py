"""Generate signals.xodr: signs and traffic lights on three straight roads.

    uv run -q --with scenariogeneration==0.16.6 tests/data/signals.py

Road 0 is a 120 m straight from the origin heading +X, climbing at 2 %, with
two 3 m lanes each side. Road 1 is a 60 m straight from (0, -40) heading +X,
banked at 0.15 rad. Road 2 is a flat 60 m straight from (0, 40) heading +X.
So every placement has a closed form to check against.

scenariogeneration writes `<signal>` with one `<validity>`, and
`<signalReference>`. It drops `length`. It has no `text`, `invalidated`,
`temporary`, `<dependency>`, `<reference>`, `<positionRoad>`,
`<positionInertial>` or `<controller>`, so those are added to its output
afterwards, as text.
"""

from pathlib import Path

from scenariogeneration import xodr

main = xodr.create_road(xodr.Line(120), id=0, left_lanes=2, right_lanes=2)
main.planview.set_start_point(0, 0, 0)
main.add_elevation(0, 0, 0.02, 0, 0)

positive = xodr.Orientation.positive
negative = xodr.Orientation.negative
both = xodr.Orientation.none


def sign(road, **kwargs):
    signal = xodr.Signal(**kwargs)
    road.add_signal(signal)
    return signal


# A German 50 km/h limit for traffic along +s, on the right of the road, and
# the "lorries only" plate under it that it depends on.
sign(main, s=20, t=-7, id="1", name="SpeedLimit50", country="DE", countryRevision="2017",
     Type="274", subtype="55", value=50, unit="km/h", zOffset=2, orientation=positive,
     height=0.6, width=0.6)
sign(main, s=20, t=-7, id="2", name="LorriesOnly", country="DE", countryRevision="2017",
     Type="1048", subtype="12", zOffset=1.6, orientation=positive, height=0.33, width=0.6)

# A town sign for traffic along -s, on the left, turned 0.3 rad.
sign(main, s=40, t=7, id="3", name="TownSign", country="DE", Type="310", subtype="-1",
     zOffset=1.5, orientation=negative, hOffset=0.3, height=0.8, width=1.2)

# A traffic light for lane -1 only, on a mast beside the road. Its hOffset
# is 14 pi, as esmini writes some. It refers to its mast and to the stop line after it,
# which depends on it.
light = sign(main, s=60, t=-7, id="4", name="Light", country="OpenDRIVE", Type="1000001",
             subtype="-1", dynamic=xodr.Dynamic.yes, zOffset=4, orientation=positive,
             hOffset=43.982297150257104, height=0.9, width=0.3, length=0.3)
light.add_validity(-1, -1)
main.add_object(xodr.Object(s=60, t=-7, Type=xodr.ObjectType.pole, id="30", name="Mast",
                            radius=0.1, height=4))
stop = sign(main, s=64, t=-1.5, id="5", name="StopLine", country="OpenDRIVE", Type="294",
            subtype="-1", zOffset=0, orientation=positive, height=0.03, width=3)
stop.add_validity(-1, -1)

# A struck-out road-works limit for both directions, pitched and rolled.
sign(main, s=80, t=-7, id="6", name="RoadWorks30", country="DE", Type="274", subtype="53",
     value=30, unit="km/h", zOffset=2, orientation=both, pitch=0.1, roll=-0.05, height=0.6,
     width=0.6)

# Two signs that apply on road 0 and stand elsewhere: one beside road 2, and
# one on a gantry above lane -1 of road 0 itself.
sign(main, s=100, t=-7, id="7", name="Elsewhere", country="DE", Type="205", subtype="-1",
     zOffset=2, orientation=positive, height=0.9, width=0.9)
sign(main, s=110, t=-7, id="8", name="Gantry", country="DE", Type="332", subtype="-1",
     zOffset=2, orientation=positive, height=1.5, width=3)

# A 3.5 t weight limit.
sign(main, s=115, t=-7, id="9", name="Weight", country="DE", Type="262", subtype="-1",
     value=3.5, unit="t", zOffset=2, orientation=positive, height=0.6, width=0.6)

banked = xodr.create_road(xodr.Line(60), id=1, left_lanes=1, right_lanes=1)
banked.planview.set_start_point(0, -40, 0)
banked.add_superelevation(0, 0.15, 0, 0, 0)

# A sign on the bank, which stays upright.
sign(banked, s=30, t=-5, id="20", name="Upright", country="DE", Type="101", subtype="-1",
     zOffset=2, orientation=positive, height=0.6, width=0.6)

side = xodr.create_road(xodr.Line(60), id=2, left_lanes=1, right_lanes=1)
side.planview.set_start_point(0, 40, 0)

# A second light, on the same controller as road 0's.
sign(side, s=55, t=-3, id="21", name="SideLight", country="OpenDRIVE", Type="1000001",
     subtype="-1", dynamic=xodr.Dynamic.yes, zOffset=4, orientation=positive, height=0.9,
     width=0.3)

# Road 2 applies the 50 km/h limit to its lane -1, and the light to its
# right side. The third names no signal.
limit = xodr.SignalReference(s=10, t=-4, id="1", orientation=positive)
limit.add_validity(-1, -1)
side.add_signal(limit)
side.add_signal(xodr.SignalReference(s=50, t=-4, id="4", orientation=positive))
side.add_signal(xodr.SignalReference(s=30, t=0, id="99", orientation=both))

odr = xodr.OpenDrive("signals")
odr.add_road(main)
odr.add_road(banked)
odr.add_road(side)
odr.adjust_roads_and_lanes()
out = Path(__file__).with_name("signals.xodr")
odr.write_xml(str(out))


def insert(xml, anchor, after, text):
    """`xml` with `text` spliced in straight after the first `after` that
    follows `anchor`."""
    at = xml.index(after, xml.index(anchor)) + len(after)
    return xml[:at] + text + xml[at:]


def children(xml, name, text):
    """`xml` with `text` added as the last children of the signal `name`."""
    start = xml.index(f'name="{name}"')
    end = xml.index(">", start)
    if xml[end - 1] == "/":
        return xml[:end - 1] + ">" + text + "\n            </signal>" + xml[end + 1:]
    close = xml.index("\n            </signal>", end)
    return xml[:close] + text + xml[close:]


xml = out.read_text()
xml = insert(xml, 'name="TownSign"', 'name="TownSign"', ' text="Gate Town"')
xml = insert(xml, 'name="Light"', 'name="Light"', ' length="0.3"')
xml = insert(xml, 'name="RoadWorks30"', 'name="RoadWorks30"',
             ' invalidated="true" temporary="true"')
xml = children(xml, "SpeedLimit50", """
                <dependency id="2" type="lorries"/>""")
xml = children(xml, "Light", """
                <reference elementType="signal" elementId="5" type="stopline"/>
                <reference elementType="object" elementId="30" type="mast"/>""")
xml = children(xml, "StopLine", """
                <dependency id="4" type="light"/>""")
xml = children(xml, "Elsewhere", """
                <positionRoad roadId="2" s="30" t="-4" zOffset="2.5" hOffset="0.2"/>""")
xml = children(xml, "Gantry", """
                <positionInertial x="110" y="-1.5" z="7.2" hdg="3.14159265358979" pitch="0.05"/>""")

# Signals 4 and 21 always show the same, and junction 500 syncs their
# controller with another that names a signal no road has.
controllers = """    <controller id="100" name="Approach" sequence="1">
        <control signalId="4" type="0"/>
        <control signalId="21" type="0"/>
    </controller>
    <controller id="101" name="Ghost">
        <control signalId="98"/>
    </controller>
    <junction name="Sync" id="500">
        <controller id="100" type="sync" sequence="2"/>
        <controller id="101"/>
    </junction>"""
at = xml.rindex("</OpenDRIVE>")
xml = xml[:at] + controllers + "\n" + xml[at:]
out.write_text(xml)
