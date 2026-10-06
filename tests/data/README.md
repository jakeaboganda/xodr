# Test maps

Fixtures for the integration tests. `exclude` in `Cargo.toml` leaves them out
of the published crate, so run these tests from a git checkout.

## Third-party

| File | Origin | Licence |
| --- | --- | --- |
| `town07.xodr` | CARLA [opendrive-test-files], `Town07.xodr` | MIT, notice preserved in the file |
| `e6mini.xodr` | esmini, `resources/xodr/e6mini.xodr` | MPL-2.0 |
| `straight_500m_signs.xodr` | esmini, `resources/xodr/straight_500m_signs.xodr` at `61b44a7` | MPL-2.0 |

[opendrive-test-files]: https://github.com/carla-simulator/opendrive-test-files

`town07.xodr` is the exercise for real exports: 234 roads, 31 junctions, 673
driving lanes, `laneOffset`, many lane sections, no spirals. `e6mini.xodr` is a
highway built from `paramPoly3`. `straight_500m_signs.xodr` is a straight
road with 19 speed and warning signs from four catalogues. Two of its ids
repeat, and some of its validities contradict their orientation.

## Written here

`cross_paths.xodr` comes from `cross_paths.py`, which writes the XML
itself. It has a crosswalk from one sidewalk of a road to the other, as a
cross path, and one naming an `s` past the road's end. Regenerate with

    python3 tests/data/cross_paths.py

`cross_sections.xodr` comes from `cross_sections.py`, which writes the XML
itself. It has a cross-section surface with an offset, both inner strips
and a relative outer strip, and one with superelevation and a strip mode
the spec doesn't allow. Regenerate with

    python3 tests/data/cross_sections.py

`demo.xodr` is the OpenDRIVE twin of the `demo_road` fixture in `src`.
`testtrack.xodr` is a purpose-built vehicle-tuning track: straight, crest, dip,
and three banked curves of known radius. `spiral`, `banked_sweeper`,
`right_banked_sweeper`, `climbing_banked` and `mid_road_super` are minimal
files aimed at one behaviour each. So are the malformed ones: `no_geometry`,
`non_finite`, `zero_length` and `dangling_link`.

`crg.xodr` lays generated OpenCRG files on four roads, one per mode:
`crg_bumps.crg`, a 40 m elevation grid with a speed bump, a washboard, two
potholes and ruts, and `crg_grip.crg`, a friction grid with a wet patch.
`crg.py` writes all three. Regenerate with

    python3 tests/data/crg.py

`objects.xodr` is generated rather than written by hand: `objects.py` builds it
with [scenariogeneration], so the object fixture comes from a writer other than
this crate. Regenerate with

    uv run --with scenariogeneration==0.16.6 tests/data/objects.py

`signals.xodr` comes from `signals.py` the same way. It has signs and
traffic lights on three straight roads, with signal references and
controllers. Regenerate with

    uv run --with scenariogeneration==0.16.6 tests/data/signals.py

`road_marks.xodr` comes from `road_marks.py` the same way. It has every
road mark type on one straight, banked road with a laneOffset and two lane
sections, a road whose marks give `<type><line>`s, and one with
`<explicit>` lines and a `<sway>`. Regenerate with

    uv run --with scenariogeneration==0.16.6 tests/data/road_marks.py

`structures.xodr` comes from `structures.py`, which writes the XML
itself. It has four straight roads with signs over traffic. Road 1 gets a
cantilever. Road 2 has too many lanes for one, so it gets a span gantry.
Road 3 gets a space frame. Road 4 has too much sign for a cantilever, so it
gets a span gantry.
Regenerate with

    python3 tests/data/structures.py

`gantries.xodr` comes from `gantries.py`, which writes the XML itself. It
is a demo of signs on gantries: a motorway with a lane-control gantry, a
space frame over both carriageways and a message-board gantry, and an urban
road with a traffic-light gantry. Regenerate with

    python3 tests/data/gantries.py

`traffic_rule.xodr` comes from `traffic_rule.py`, which writes the XML
itself. It has the same pair of linked roads in right- and left-hand
traffic, each with a signal, and a road whose `rule` the spec does not
allow. Regenerate with

    python3 tests/data/traffic_rule.py

`direct_junctions.xodr` comes from `direct_junctions.py`, which writes the
XML itself. It has a road splitting in two through a direct junction that
gives its connections one way, two roads joined through one that gives
them both ways, and a connection of each junction type without the road
it leads into. Regenerate with

    python3 tests/data/direct_junctions.py

`lane_rules.xodr` comes from `lane_rules.py`, which writes the XML
itself. It has lane rules, one without a value, access in the 1.8 form
and the older one, one without a rule, and materials, some out of order,
across two lane sections. Regenerate with

    python3 tests/data/lane_rules.py

`junction_areas.xodr` comes from `junction_areas.py`, which writes the
XML itself. It has a junction with a boundary, a reference line and an
elevation grid with a hump in it, and a clockwise boundary with a segment
on a road the file lacks. Regenerate with

    python3 tests/data/junction_areas.py

`junction_groups.xodr` comes from `junction_groups.py`, which writes the
XML itself. It has a roundabout group of two junctions that also names a
junction the file lacks, and a group with a type the spec doesn't allow.
Regenerate with

    python3 tests/data/junction_groups.py

`junction_links.xodr` comes from `junction_links.py`, which writes the
XML itself. It has an incoming road whose `<link>` leaves out its junction,
and one whose connecting road doesn't name it either. Regenerate with

    python3 tests/data/junction_links.py

`link_gaps.xodr` comes from `link_gaps.py`, which writes the XML itself.
It has a lane that narrows onto one half of a split across a lane section
seam, one that hands over to a lane opening out of nothing beside it, a
link across a 2 m gap, and a road whose `length` is short of its
`<planView>`. Regenerate with

    python3 tests/data/link_gaps.py

`lane_direction.xodr` comes from `lane_direction.py`, which writes the XML
itself. It has standard, reversed and two-way lanes, and one with a
`direction` the spec doesn't allow, on two linked roads. Regenerate with

    python3 tests/data/lane_direction.py

`junction_priority.xodr` comes from `junction_priority.py`, which writes
the XML itself. It has a junction giving one connecting road priority over
another, and a priority naming a road the file lacks. Regenerate with

    python3 tests/data/junction_priority.py

`lane_visibility.xodr` comes from `lane_visibility.py`, which writes the
XML itself. It has lane visibilities out of order, one without `sOffset`,
and two with a distance the crate can't read, across two lane sections.
Regenerate with

    python3 tests/data/lane_visibility.py

`virtual_junction.xodr` comes from `virtual_junction.py`, which writes the
XML itself. It has a driveway: two connecting roads meeting a main road
part way along it, a lot road, and two deprecated virtual connections, one
naming a road the file lacks. Regenerate with

    python3 tests/data/virtual_junction.py

`railways.xodr` comes from `railways.py`, which writes the XML itself. It
has a switch from a track onto a side track, a switch naming a track the
file lacks, and a station with a platform and a segment whose side the spec
doesn't allow. Regenerate with

    python3 tests/data/railways.py

`road_neighbors.xodr` comes from `road_neighbors.py`, which writes the XML
itself. It has roads naming the roads beside them, the same way and the
other, one naming a road the file lacks, and one with a `side` 1.4 doesn't
allow. Regenerate with

    python3 tests/data/road_neighbors.py

`signal_semantics.xodr` comes from `signal_semantics.py`, which writes the
XML itself. It has a speed sign's semantics with an exception for buses, a
gantry's static board of two signs, a variable message board with two
display areas, and a stop line's priority. Regenerate with

    python3 tests/data/signal_semantics.py

`speed_limits.xodr` comes from `speed_limits.py`, which writes the XML
itself. It has road types with speed limits in km/h, mph and none, lane
speeds that override them, a road with two lane sections whose first type
starts part way along, a road with a type and speeds the crate can't
read, and a lane speed too large for an `f32`. Regenerate with

    python3 tests/data/speed_limits.py

`lane_change.xodr` comes from `lane_change.py`, which writes the XML
itself. It has one road whose marks allow a lane change both ways, one way
and neither way, a border without a mark for its first 20 m, and a
`laneChange` the spec doesn't allow. Regenerate with

    python3 tests/data/lane_change.py

`level_lanes.xodr` comes from `level_lanes.py`, which writes the XML
itself. It has level shoulders and a raised level sidewalk beside a
superelevated road, a level shoulder beside a crowned road, and a lane
outside a level lane that isn't level. Regenerate with

    python3 tests/data/level_lanes.py

`lane_heights.xodr` comes from `lane_heights.py`, which uses
[scenariogeneration] like `objects.py`. It has
raised sidewalks, one flat and one with a kerb ramp, on a flat road, a
banked and climbing road, a road with two lane sections, and an arc. The
flat road also has poles, a sign and road marks on and beside them.
Regenerate with

    uv run --with scenariogeneration==0.16.6 tests/data/lane_heights.py

`lane_borders.xodr` comes from `lane_borders.py`, which writes the XML
itself, since scenariogeneration writes no lane borders. It has a lane
opening out of nothing as in ASAM's `Ex_Lane-Border.xodr`, border and width
lanes stacked on each other, a lane with both, borders under a
`laneOffset`, and a border that crosses the lane inside it. Regenerate with

    python3 tests/data/lane_borders.py

`lateral_shapes.xodr` comes from `lateral_shapes.py` the same way. It has a
crown that breaks on the center line, a parabolic crown flattening over
50 m with a pole and a sign on it, a crown on a banked road, a raised
sidewalk on a crown, and a profile that starts inside the road. Regenerate
with

    python3 tests/data/lateral_shapes.py

[scenariogeneration]: https://github.com/pyoscx/scenariogeneration
