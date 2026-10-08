# Stage schema

This is the layout of the stages `xodr_usd` writes. Build on it to read
these stages in another tool, or to write them from another exporter.

## USD terms

- A **stage** is one USD scene. Here it's one `.usda` text file.
- A **prim** is a node in the stage, named by a path such as `/Map/Roads`.
- An **attribute** is a named value on a prim, such as `xodr:roadId`.
- A **relationship** is a named link from one prim to others, by path.
- `Scope` groups prims. `Xform` groups prims and moves them. `Mesh` holds
  triangles or quads.
- An `xformOp` attribute moves (`translate`), turns (`rotate...`) or scales
  (`scale`) a prim. `xformOpOrder` lists them in the order they apply.
- A **layer** is one file of scene description. A stage can stack layers.
  Where two layers set the same value, the stronger layer wins.
- An **over** changes a prim that another layer defines.
- A **class** is a prim that isn't drawn. A prim that **inherits** a class
  gets the class's children and attributes. Values the prim sets itself win.
- A **label** names what a prim is, for tools such as a perception
  pipeline. Labels come in named sets, using OpenUSD's `SemanticsLabelsAPI`.

## Version

The stage records the schema version in its layer metadata:

```usda
customLayerData = {
    dictionary xodr = {
        int schemaVersion = 2
    }
}
```

The version goes up when a path or an attribute is renamed or removed, or
changes meaning. Adding a prim or an attribute doesn't change it.

## Frame

The stage is Z-up and in metres (`upAxis = "Z"`, `metersPerUnit = 1`).
Coordinates are the map's own, with no geo reference applied. The default
prim is `/Map`.

## Prims

| Path | Type | Contents |
| --- | --- | --- |
| `/Map` | `Xform` | Everything below. |
| `/Map/Roads/road_<n>` | `Scope` | One per road. |
| `/Map/Roads/road_<n>/lane_<n>` | `Mesh` or `Scope` | One lane's surface, with normals. A lane inside a junction is a `Scope` with no geometry, since its junction's wrap covers it. |
| `/Map/Junctions` | `Scope` | Every junction's wrap. |
| `/Map/Junctions/junction_<n>` | `Scope` | One per junction with lanes. |
| `/Map/Junctions/junction_<n>/<laneType>` | `Mesh` | The part of the junction's wrap that lanes of one type own, with normals. See [Junctions](#junctions). |
| `/Map/RoadMarks/mark_<n>` | `Mesh` | One painted road mark, 5 mm above its lane. One quad per piece of paint. |
| `/Map/Objects/object_<n>` | `Mesh` | One object, double-sided. Objects with no volume have no prim. |
| `/Map/Supports/support_<n>` | `Mesh` | One structure the exporter added. See [Structures](#structures). |
| `/Map/Signals/signal_<n>` | `Xform` | One signal, at its board's position and turn. See [Signals](#signals). |
| `/Map/Controllers/controller_<n>` | `Scope` | One controller. |
| `/_SignalTypes/<type class>` | class `Xform` | One per signal type. See [Signal types](#signal-types). |

`<n>` is the id `xodr` gives the road, lane, mark, object, signal or
controller. For an added structure it's a count. For a junction it's a
count in order of the junctions' OpenDRIVE ids, compared as text. It isn't
the OpenDRIVE id, which can hold characters USD doesn't allow in a name.
The OpenDRIVE ids are in the attributes below.

`<laneType>` is the lane type, such as `driving` or `on_ramp`, with `_` for
`-`.

Lanes and marks with no triangles have no prim.

## Junctions

A junction's connecting lanes lie over one another. The stage replaces
them with a wrap: one surface over the junction's lanes that covers each
point once, on the lane that owns the ground there. It follows every slope
and bank, and creases where two lanes cross in height.

- The highest lane owns the ground. But a lane that carries traffic owns
  it over a higher one that doesn't, such as a sidewalk, when that is less
  than 1 cm higher.
- A lane more than 2 m over another, such as a bridge, is a level of its
  own. Each level is covered once, so a road under a bridge keeps its
  ground.
- The wrap is within 1 cm of the lane that owns each point.
- The wrap is split into one mesh per lane type, by the type of the lane
  that owns each part. The meshes meet edge to edge.
- Where one lane ends on top of a lower one, the wrap has a step, with no
  wall.
- Each vertex where a road meets the edge of a junction's lanes is a
  vertex of the wrap.

## Signals

A signal prim sits at the middle of the bottom of its box. The box is
`length` deep, centred on the prim, as OpenDRIVE places it. Its local +X
points toward the traffic the board faces, +Y to the board's left, and +Z
up. `xformOp:rotateXYZ` holds roll, pitch and heading, in degrees.

Each signal has these children:

| Child | Type | Contents |
| --- | --- | --- |
| `board` | `Xform` | The signal's own board. It inherits the signal's type class and is scaled to the board's size. |
| `board_back` | `Xform` | Only on a signal the map gives `orientation="none"`, which applies both ways. Like `board`, but turned to face -X. |
| `sign_<b>_<k>` | `Xform` | Sign `k` on static board `b`, like `board` but for the sign's own type. 1 cm in front of the box. |
| `display_<b>` | `Xform` | Variable message board `b`: a `screen` mesh, and an `area_<k>` mesh for each display area, in metres. |

A board's scale is (1, width, height). X isn't scaled, so a type class can
give its board a depth in metres. A board the map gives no size is 0.6 m
across and 0.6 m up, and its signal has `xodr:sizeGuessed = 1`.

On a signal with `board_back`, the two boards stand back to back. Their
boxes have a gap between them, at least 5 cm, for what holds them up.

Every board has up to two label sets:

- `opendrive`: the codes, as `country:type:subtype`, such as `DE:274:55`.
- `meaning`: what the map's `<semantics>` say, such as `speed:maximum`.
  Each label is the element name, then its `type` if it has one. Boards
  with no `<semantics>` have no `meaning` set.

A sign with no `country` uses its signal's for its type class and code.

## Structures

Each signal records what holds it up, in `xodr:support`:

- `object`: a pole object from the map. `xodr:supportPrim` links to it.
- `synthesized`: a structure the exporter added. `xodr:supportPrim` links
  to it.
- `none`: nothing holds it up.

The exporter takes the first rule that applies:

1. A board less than 0.1 m above the road is paint, such as a stop line. It
   gets `none`.
2. A pole object that the signal's `<reference>`s name gets `object`.
3. A pole object within 0.5 m of the board, across the ground, gets
   `object`.
4. Anything else gets `synthesized`.

### Which signals share one

Signals share a structure if they face the same way or opposite ways. Each
must stand within 0.5 m of the group's first signal, along the way it
faces. Beside the road it must also stand within 0.5 m across. Over
traffic it may stand up to 40 m across, as signs on one gantry do.

### Which kind it is

A structure never stands on a lane that carries traffic. Every lane
carries traffic except types `sidewalk`, `border`, `curb`, `median` and
`none`.

| `xodr:structure` | Used when | Shape |
| --- | --- | --- |
| `pole` | No lane that carries traffic is under the signals. | A pole from the ground to the top of the highest board. |
| `cantilever` | The arm is at most 13 m. The signs are at most 20 m². The arm crosses at most 2 driving lanes. | A pole beside the road, in line with the boards. It rises, bends and runs behind the boards at the height of the highest board's middle. |
| `gantry` | A cantilever doesn't fit. The span is at most 27.5 m. The signs are at most 55 m². The span crosses at most 5 lanes. | A leg each side of the road, and a box truss 0.6 m deep and 0.8 m high between them. |
| `spaceFrame` | A gantry doesn't fit either. | A tower of two braced columns each side, and a box truss 1.2 m deep and 1.5 m high. |
| `arm` | At most one side has room for a leg, and no cantilever fits. | A pole at the nearest spot with room, within 15 m. Its arm runs 0.25 m above the highest board. |

The limits come from the AASHTO LRFD Specifications for Structural Supports
for Highway Signs. AASHTO sets US highway standards. US state road
agencies, such as Washington's (WSDOT) and Delaware's (DelDOT), apply these
limits.

The exporter measures them like this:

- An arm runs from the leg to the far edge of the boards.
- A span runs from leg to leg.
- A leg stands at least 0.5 m from any lane that carries traffic.
- A cantilever counts the driving lanes under its arm.
- A gantry counts every lane under its span that carries traffic,
  shoulders included.
- Lanes inside a junction don't count, because its connecting lanes
  overlap.
- The sign area is the width times the height of each board. Boards back
  to back count once.

If no structure has room, the signals get `none`.

### How boards sit on it

Boards lower than an arm or truss hang from it on a hanger.

A board may move toward its traffic to clear its structure. The board's
`xformOp:translate` holds the move. After it, the back of the board's box
is this far from the structure's centre line:

- 5 cm for a `pole`, `cantilever` or `arm`;
- 0.37 m for a `gantry`;
- 0.67 m for a `spaceFrame`.

A board that is already far enough doesn't move.

### What the prim holds

Each structure prim lists where it meets the ground in `xodr:feet`, and its
kind in `xodr:structure`. It's grey and has `xodr:synthesized = 1`.

## Signal types

A type class holds what every signal of one type looks like. The writer
makes one for each type the map uses, with one child: `Board`, a grey,
double-sided quad 1 m across and 1 m up, facing +X.

The class name joins the codes with `_`. Each character other than a letter
or digit becomes `_`, and a name that starts with a digit gets a `_` in
front. For example, `DE`, `274` and `-1` give `DE_274__1`. Two types whose
codes differ only in those characters share a class.

To change how a type looks, write a [catalogue](#catalogues).

## Catalogues

A catalogue is a layer that sets how signal types look and what they are.
It holds one `over` per type class under `/_SignalTypes`. Every signal of
that type picks up the change.

Type classes use board units. Y runs from -0.5 to 0.5 across the board and
Z from 0 to 1 up it. Each board scales these to its own width and height.
X is in metres and isn't scaled. +X faces the traffic. Centre a board's
depth on X = 0, and make it as deep as the map's `length`. Then the
structure behind it stays clear.

In a type class, a catalogue can:

- replace the grey quad by setting `Board`'s `points`, `faceVertexCounts`,
  `faceVertexIndices`, `extent` and `primvars:displayColor`;
- add more meshes next to `Board`;
- add labels in the `class` set, such as `traffic_sign`.

This catalogue turns every German 50 km/h sign into a red disc:

```usda
#usda 1.0

over "_SignalTypes"
{
    over "DE_274_55" (
        prepend apiSchemas = ["SemanticsLabelsAPI:class"]
    )
    {
        token[] semantics:labels:class = ["traffic_sign", "speed_limit"]

        over "Board"
        {
            float3[] extent = [(0, -0.5, 0), (0, 0.5, 1)]
            int[] faceVertexCounts = [8]
            int[] faceVertexIndices = [0, 1, 2, 3, 4, 5, 6, 7]
            point3f[] points = [(0, 0.5, 0.5), (0, 0.35, 0.85), (0, 0, 1), (0, -0.35, 0.85), (0, -0.5, 0.5), (0, -0.35, 0.15), (0, 0, 0), (0, 0.35, 0.15)]
            color3f[] primvars:displayColor = [(0.8, 0.1, 0.1)]
        }
    }
}
```

To use a catalogue, stack it over the stage, catalogue first:

```usda
#usda 1.0
(
    subLayers = [@catalogue.usda@, @town07.usda@]
)
```

[`catalogues/sample.usda`](catalogues/sample.usda) covers every `DE` and
`OPENDRIVE` type in the test maps.

## Attributes

Every attribute this schema adds starts with `xodr:`.

| Prim | Attribute | Meaning |
| --- | --- | --- |
| road | `string xodr:roadId` | The `<road id>`. |
| road | `string xodr:junction` | The `<junction id>`. Only on roads inside a junction. |
| lane | `int xodr:section` | The lane section, counting from 0 along the road. |
| lane | `int xodr:laneId` | The `<lane id>`: negative on the right, positive on the left. |
| lane | `string xodr:laneType` | The lane `type`, such as `driving`. |
| junction | `string xodr:junction` | The `<junction id>`. |
| wrap | `string xodr:laneType` | The lane type of the lanes that own it. |
| wrap | `rel xodr:lanes` | The junction's lanes of that type. |
| mark | `string xodr:type` | The mark `type`, such as `solid`. |
| mark | `string xodr:weight` | `standard` or `bold`. |
| mark | `string xodr:color` | The mark `color`, such as `yellow`. |
| mark | `string xodr:laneChange` | Which way traffic may cross it. |
| object | `string xodr:type` | The object `type`, such as `pole`. |
| object | `string xodr:subtype` | The object `subtype`. |
| object | `string xodr:name` | The object `name`. |
| object | `string xodr:objectId` | The `<object id>`. |
| object | `string xodr:roadId` | The `<road id>` the object is on. |
| signal | `string xodr:signalId` | The `<signal id>`. |
| signal | `string xodr:roadId` | The `<road id>` the signal is on. |
| signal | `double xodr:s`, `xodr:t` | Where the signal is along and across its road. |
| signal | `token xodr:orientation` | `+`, `-` or `none`, as the map gives it. |
| signal | `string xodr:name` | The signal `name`. |
| signal | `string xodr:country`, `xodr:countryRevision` | The catalogue the codes come from, such as `DE`, and its year. |
| signal | `string xodr:type`, `xodr:subtype` | The codes, such as `274` and `55`. |
| signal | `double xodr:value`, `string xodr:unit` | The number the signal shows, such as `50` `km/h`. Only if the map gives one. |
| signal | `string xodr:text` | The text the signal shows. |
| signal | `bool xodr:dynamic` | Whether it can change what it shows, such as a traffic light. |
| signal | `bool xodr:invalidated`, `xodr:temporary` | Whether it's struck out, and whether it's temporary. |
| signal | `bool xodr:sizeGuessed` | Whether the map gives no width or no height. |
| signal | `float xodr:length` | The board's thickness. Only if the map gives one. |
| signal | `point3f[] xodr:appliesAt` | The points on the road where the signal applies: its own, then one per `<signalReference>`. |
| signal | `rel xodr:lanes` | The lanes it applies to. |
| signal | `token xodr:support`, `rel xodr:supportPrim` | What holds it up. See [Structures](#structures). |
| signal | `rel xodr:dependencies`, `string[] xodr:dependencyTypes` | The signals its `<dependency>`s name, and each `type`. |
| signal | `rel xodr:references`, `string[] xodr:referenceTypes` | The signals and objects its `<reference>`s name, and each `type`. |
| sign | `string xodr:name`, `xodr:country`, `xodr:type`, `xodr:subtype`, `xodr:text` | As on a signal. |
| sign | `double xodr:value`, `string xodr:unit` | As on a signal. |
| display | `string xodr:display` | The `displayType`, such as `LED`. |
| area | `int xodr:index` | The display area's `index`. |
| controller | `string xodr:controllerId` | The `<controller id>`. |
| controller | `string xodr:name` | The controller `name`. |
| controller | `int xodr:sequence` | Its `sequence`. Only if the map gives one. |
| controller | `rel xodr:signals`, `string[] xodr:controlTypes` | The signals it controls, and each `<control type>`. |
| structure | `token xodr:structure` | `pole`, `cantilever`, `gantry`, `spaceFrame` or `arm`. |
| structure | `point3f[] xodr:feet` | Where it meets the ground. |
| structure | `bool xodr:synthesized` | Always 1: the exporter added it. |

A relationship skips a target with no prim, such as an object with no
volume. It lists each target once, even if the map names it twice. Its
matching `...Types` array skips the same entries, and keeps the first type.
An empty array or relationship is left out.

## Colour

Every mesh has a `primvars:displayColor` and no material. Lanes and wraps
take the colour of their lane type: driving dark grey, sidewalks and curbs
light grey, and other types mid grey. Paint
uses the colour its line names, or white. Objects and signal boards are
grey.
