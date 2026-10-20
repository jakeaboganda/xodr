# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate railways.xodr: a railway switch and a station.

    uv run tests/data/railways.py

Road 1 is a 100 m straight track heading +X, with a 1.5 m rail lane -1.
Road 2 is a 40 m side track leaving it at s = 30. Road 1's <railroad> has
switch 7, set straight, from road 1 at s = 30 onto road 2 at its start,
partnered with switch 8, "Crossover end"; and switch 9, whose side track names road 5, which
the file does not have.

Station 20, "Halt", small, has platform 20-1 beside road 1 on its right
from s = 50 to 80, and a segment with side "up", which the spec does not
allow.
"""

from pathlib import Path


def track(id, x, y, hdg, length, railroad=""):
    return (
        f'  <road name="" length="{length}" id="{id}" junction="-1">\n'
        f'    <planView><geometry s="0" x="{x}" y="{y}" hdg="{hdg}" length="{length}"><line/></geometry></planView>\n'
        '    <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>'
        '<lane id="-1" type="rail" level="false"><width sOffset="0" a="1.5" b="0" c="0" d="0"/></lane>'
        "</right></laneSection></lanes>\n"
        f"    {railroad}\n"
        "  </road>\n"
    )


RAILROAD = (
    "<railroad>"
    '<switch name="Points" id="7" position="straight">'
    '<mainTrack id="1" s="30" dir="+"/><sideTrack id="2" s="0" dir="+"/><partner id="8" name="Crossover end"/></switch>'
    '<switch id="9" position="turn">'
    '<mainTrack id="1" s="60" dir="+"/><sideTrack id="5" s="0" dir="+"/></switch>'
    "</railroad>"
)
STATION = (
    '  <station name="Halt" id="20" type="small"><platform name="Up" id="20-1">'
    '<segment roadId="1" sStart="50" sEnd="80" side="right"/>'
    '<segment roadId="1" sStart="50" sEnd="80" side="up"/>'
    "</platform></station>\n"
)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="railways"/>\n'
    + track(1, 0, 0, 0, 100, RAILROAD)
    + track(2, 30, 0, -0.1, 40)
    + STATION
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
