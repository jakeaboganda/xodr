# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate signal_semantics.xodr: what signals mean, and the boards they are.

Road 0 is a flat 100 m straight heading +X, with 3.5 m driving lanes 1 and
-1. Its signals all face traffic along +s, so their boards face -X:

- Signal 1, at s = 10, t = -5, is a 50 km/h limit: its <semantics> say a
  maximum speed of 50 km/h, allowing buses and people on foot, and hold a
  child the spec does not name.
- Signal 2, at s = 30, t = -5, 5 m up, is a gantry. Its <staticBoard> holds
  two signs: a 60 km/h limit 1 m to the left of the board's middle and 0.5 m
  up it, and a no-lorries sign 1 m to the right.
- Signal 3, at s = 60, t = -5, 5 m up, is a variable message board, 4 m by
  2 m, LED, with two display areas.
- Signal 4, at s = 80, t = 0, is a stop line, its priority a stop line.
"""

from pathlib import Path


def signal(id, s, t, z, extra, dynamic="no"):
    return (
        f'<signal id="{id}" s="{s}" t="{t}" zOffset="{z}" orientation="+" dynamic="{dynamic}" '
        f'country="DE" type="274" subtype="-1" width="0.6" height="0.6">{extra}</signal>'
    )


SPEED = (
    '<semantics><speed type="maximum" value="50" unit="km/h"/>'
    '<supplementaryAllows><vehicle><type>bus</type></vehicle><person><type>pedestrian</type></person></supplementaryAllows>'
    "<sideways/></semantics>"
)
GANTRY = (
    "<staticBoard>"
    '<sign id="2a" type="274" subtype="60" value="60" unit="km/h" v="1" z="0.5" width="0.8" height="0.8">'
    '<semantics><speed type="maximum" value="60" unit="km/h"/></semantics></sign>'
    '<sign id="2b" type="253" v="-1" z="0.5">'
    "<semantics><prohibited><vehicle><type>truck</type></vehicle></prohibited></semantics></sign>"
    "</staticBoard>"
)
MESSAGE = (
    '<vmsBoard displayType="LED" displayWidth="4" displayHeight="2" v="0" z="0">'
    '<displayArea index="0" v="-1" z="0.5" width="1.5" height="1.5"/>'
    '<displayArea index="1" v="1" z="0.5" width="1.5" height="1.5"/>'
    "</vmsBoard>"
)
STOP = '<semantics><priority type="stopLine"/></semantics>'

signals = signal(1, 10, -5, 2, SPEED) + signal(2, 30, -5, 5, GANTRY) + signal(3, 60, -5, 5, MESSAGE, "yes") + signal(4, 80, 0, 0, STOP)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="signal_semantics"/>\n'
    '  <road name="" length="100" id="0" junction="-1">\n'
    '    <planView><geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>\n'
    '    <lanes><laneSection s="0">'
    '<left><lane id="1" type="driving" level="false"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></left>'
    '<center><lane id="0" type="none"/></center>'
    '<right><lane id="-1" type="driving" level="false"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></right>'
    "</laneSection></lanes>\n"
    f"    <signals>{signals}</signals>\n"
    "  </road>\n"
    "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
