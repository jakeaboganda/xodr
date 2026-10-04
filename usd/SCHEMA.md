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
        int schemaVersion = 1
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
| `/Map/Roads/road_<n>/lane_<n>` | `Mesh` | One lane's surface, with normals. |
| `/Map/RoadMarks/mark_<n>` | `Mesh` | One painted road mark, 5 mm above its lane. One quad per piece of paint. |
| `/Map/Objects/object_<n>` | `Mesh` | One object, double-sided. Objects with no volume have no prim. |
| `/Map/Supports/support_<n>` | `Mesh` | One pole the exporter added. See [Poles](#poles). |
| `/Map/Signals/signal_<n>` | `Xform` | One signal, at its board's position and turn. See [Signals](#signals). |
| `/Map/Controllers/controller_<n>` | `Scope` | One controller. |
| `/_SignalTypes/<type class>` | class `Xform` | One per signal type. See [Signal types](#signal-types). |

`<n>` is the id `xodr` gives the prim's road, lane, mark, object, signal or
controller, or a count for added poles. It's not the OpenDRIVE id, which can
hold characters USD doesn't allow in a name. The OpenDRIVE ids are in the
attributes below.

Lanes and marks with no triangles have no prim.

## Signals

A signal prim sits at the middle of its board's bottom edge. Its local +X
points toward the traffic the board faces, +Y to the board's left, and +Z
up. `xformOp:rotateXYZ` holds roll, pitch and heading, in degrees.

Each signal has these children:

| Child | Type | Contents |
| --- | --- | --- |
| `board` | `Xform` | The signal's own board. It inherits the signal's type class and is scaled to the board's size. |
| `sign_<b>_<k>` | `Xform` | Sign `k` on static board `b`, like `board` but for the sign's own type. 1 cm in front of `board`. |
| `display_<b>` | `Xform` | Variable message board `b`: a `screen` mesh, and an `area_<k>` mesh for each display area, in metres. |

A board's scale is (1, width, height). X isn't scaled, so a type class can
give its board a depth in metres. A board the map gives no size is 0.6 m
across and 0.6 m up, and its signal has `xodr:sizeGuessed = 1`.

Every board has up to two label sets:

- `opendrive`: the codes, as `country:type:subtype`, such as `DE:274:55`.
- `meaning`: what the map's `<semantics>` say, such as `speed:maximum`.
  Each label is the element name, then its `type` if it has one. Boards
  with no `<semantics>` have no `meaning` set.

A sign with no `country` uses its signal's for its type class and code.

## Poles

Each signal records what holds it up, in `xodr:support`:

- `object`: a pole object from the map. `xodr:supportPrim` links to it.
- `synthesized`: a pole the exporter added. `xodr:supportPrim` links to it.
- `none`: nothing holds it up.

The exporter takes the first rule that applies:

1. A board less than 0.1 m above the road is paint, such as a stop line. It
   gets `none`.
2. A pole object the signal's `<reference>`s name gets `object`.
3. A pole object within 0.5 m of the board, measured across the ground,
   gets `object`.
4. A board over a driving lane, such as on a gantry, gets `none`.
5. Anything else gets `synthesized`. The added pole goes from the road
   under the board up to the board's middle, just behind it. Signals within
   0.5 m of each other share one pole.

Added poles are grey, 8 cm across, and have `xodr:synthesized = 1`.

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
X is in metres and isn't scaled. +X faces the traffic.

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
| signal | `token xodr:support`, `rel xodr:supportPrim` | What holds it up. See [Poles](#poles). |
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

A relationship skips a target with no prim, such as an object with no
volume. Its matching `...Types` array skips the same entries. An empty
array or relationship is left out.

## Colour

Every mesh has a `primvars:displayColor` and no material. Driving lanes are
dark grey, sidewalks and curbs light grey, and other lanes mid grey. Paint
uses the colour its line names, or white. Objects and signal boards are
grey.
