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

`<n>` is the id `xodr` gives the road, lane, mark or object. It's not the
OpenDRIVE id, which can hold characters USD doesn't allow in a name. The
OpenDRIVE ids are in the attributes below.

Lanes and marks with no triangles have no prim.

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

## Colour

Every mesh has a `primvars:displayColor` and no material. Driving lanes are
dark grey, sidewalks and curbs light grey, and other lanes mid grey. Paint
uses the colour its line names, or white. Objects are grey.
