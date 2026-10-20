# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Generate cross_sections.xodr: road surfaces given as cross-section strips.

    uv run tests/data/cross_sections.py

Road 0 is a flat 60 m straight heading +X, with 3 m driving lanes 1, 2, -1
and -2. Its <crossSectionSurface> is shifted 0.5 m left by its <tOffset>.
Strip 1 is 3 m wide and falls 2 % to the left. Strip 2, relative to it,
rises 5 %. Strip -1, the only strip on the right, has no width: it stands
at 0.1 m plus 1 mm for each metre along the road, and rises 2 % to the left.

Road 1 is a 30 m straight from (0, 20) with 3 m lanes 1 and -1, a
<superelevation> of 0.02 rad and a <crossSectionSurface>, which the spec
forbids together. Its strip 2 has mode "sideways".
"""

from pathlib import Path


def coefficients(tag, a, b=0):
    return f'<{tag}><coefficients s="0" a="{a}" b="{b}"/></{tag}>'


def lane(id):
    return f'<lane id="{id}" type="driving" level="false"><width sOffset="0" a="3" b="0" c="0" d="0"/></lane>'


def road(id, y, length, lanes, lateral):
    left = "".join(lane(i) for i in lanes if i > 0)
    right = "".join(lane(i) for i in lanes if i < 0)
    return (
        f'  <road name="" length="{length}" id="{id}" junction="-1">\n'
        f'    <planView><geometry s="0" x="0" y="{y}" hdg="0" length="{length}"><line/></geometry></planView>\n'
        f"    <lateralProfile>{lateral}</lateralProfile>\n"
        '    <lanes><laneSection s="0">'
        f"<left>{left}</left>"
        '<center><lane id="0" type="none"/></center>'
        f"<right>{right}</right>"
        "</laneSection></lanes>\n"
        "  </road>\n"
    )


SURFACE_0 = (
    "<crossSectionSurface>"
    + coefficients("tOffset", 0.5)
    + "<surfaceStrips>"
    + '<strip id="1">' + coefficients("width", 3) + coefficients("linear", -0.02) + "</strip>"
    + '<strip id="2" mode="relative">' + coefficients("linear", 0.05) + "</strip>"
    + '<strip id="-1">' + coefficients("constant", 0.1, 0.001) + coefficients("linear", 0.02) + "</strip>"
    + "</surfaceStrips></crossSectionSurface>"
)
SURFACE_1 = (
    '<superelevation s="0" a="0.02" b="0" c="0" d="0"/>'
    "<crossSectionSurface><surfaceStrips>"
    + '<strip id="1">' + coefficients("width", 1) + "</strip>"
    + '<strip id="2" mode="sideways">' + coefficients("linear", 0.05) + "</strip>"
    + "</surfaceStrips></crossSectionSurface>"
)

xodr = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    "<OpenDRIVE>\n"
    '  <header revMajor="1" revMinor="8" name="cross_sections"/>\n'
    + road(0, 0, 60, [1, 2, -1, -2], SURFACE_0)
    + road(1, 20, 30, [1, -1], SURFACE_1)
    + "</OpenDRIVE>\n"
)
Path(__file__).with_suffix(".xodr").write_text(xodr)
