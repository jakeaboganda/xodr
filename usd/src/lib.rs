//! Export a map loaded by `xodr` as an OpenUSD stage (`.usda`).
//!
//! Call [`write_stage`]. `usd/SCHEMA.md` describes what the stage holds.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};

use xodr::{
    JunctionArea, Lane, LaneId, LaneProvenance, LaneSpan, LaneType, Mesh, ObjectId, Point,
    Provenance, RoadId, RoadNetwork, Vector, Warning,
};

mod area;
mod signals;
mod structures;
mod supports;
mod wrap;

/// The version of `usd/SCHEMA.md` this writer follows.
pub const SCHEMA_VERSION: u32 = 2;

/// Metres between a road mark and its lane, so the lane doesn't hide it.
const LIFT: f32 = 0.005;

/// Write `net` to `out` as a `.usda` stage. `provenance` gives each object's
/// OpenDRIVE id and road.
pub fn write_stage(
    net: &RoadNetwork,
    provenance: &Provenance,
    out: &mut impl Write,
) -> io::Result<()> {
    writeln!(out, "#usda 1.0")?;
    writeln!(out, "(")?;
    writeln!(out, "    customLayerData = {{")?;
    writeln!(out, "        dictionary xodr = {{")?;
    writeln!(out, "            int schemaVersion = {SCHEMA_VERSION}")?;
    writeln!(out, "        }}")?;
    writeln!(out, "    }}")?;
    writeln!(out, "    defaultPrim = \"Map\"")?;
    writeln!(out, "    metersPerUnit = 1")?;
    writeln!(out, "    upAxis = \"Z\"")?;
    writeln!(out, ")")?;
    writeln!(out)?;
    writeln!(out, "def Xform \"Map\"")?;
    writeln!(out, "{{")?;
    let surface = net.surface_mesh();
    let lanes = roads(net, &surface, out)?;
    junctions(net, provenance, &surface, &lanes, out)?;
    road_marks(net, out)?;
    let objects = objects(net, provenance, &net.object_mesh(), out)?;
    let paths = Paths { lanes, objects };
    let (placements, structures) = supports::supports(net, provenance, &paths, &surface);
    structures::write(&structures, out)?;
    let classes = signals::signals(net, provenance, &paths, &placements, out)?;
    signals::controllers(net, provenance, out)?;
    writeln!(out, "}}")?;
    signals::type_classes(&classes, out)
}

/// The path of each lane and object prim in the stage.
struct Paths {
    lanes: HashMap<LaneId, String>,
    objects: HashMap<ObjectId, String>,
}

/// One `Scope` per road, with one `Mesh` per lane. A lane in a junction is
/// a `Scope` with no geometry, since the junction's wrap covers it. Returns
/// each lane's path.
fn roads(
    net: &RoadNetwork,
    mesh: &Mesh,
    out: &mut impl Write,
) -> io::Result<HashMap<LaneId, String>> {
    let mut paths = HashMap::new();
    let mut by_road: BTreeMap<usize, Vec<&LaneSpan>> = BTreeMap::new();
    for span in mesh.lanes.iter().filter(|s| !s.indices.is_empty()) {
        if let Some(at) = net.road_lane(span.lane) {
            by_road.entry(at.road.0).or_default().push(span);
        }
    }
    open(out, 1, "def Scope", "Roads", &[], &[])?;
    for (road, spans) in by_road {
        let road = net
            .road(RoadId(road))
            .expect("a lane's road is on the network");
        let mut tags = vec![("roadId", Tag::Text(road.od_id().to_string()))];
        if let Some(junction) = road.junction() {
            tags.push(("junction", Tag::Text(junction.to_string())));
        }
        let road_name = format!("road_{}", road.id().0);
        open(out, 2, "def Scope", &road_name, &[], &tags)?;
        for span in spans {
            let (lane, at) = (net.lane(span.lane), net.road_lane(span.lane));
            let (lane, at) = (lane.expect("a span's lane"), at.expect("a span's lane"));
            let name = format!("lane_{}", span.lane.0);
            paths.insert(span.lane, format!("/Map/Roads/{road_name}/{name}"));
            let tags = vec![
                ("section", Tag::Int(at.section as i64)),
                ("laneId", Tag::Int(at.od_id.into())),
                ("laneType", Tag::Text(lane.kind.as_str().to_string())),
            ];
            if road.junction().is_some() {
                open(out, 3, "def Scope", &name, &[], &tags)?;
                close(out, 3)?;
                continue;
            }
            let vertices = span.vertices.start as usize..span.vertices.end as usize;
            let indices = &mesh.indices[span.indices.start as usize..span.indices.end as usize];
            write_mesh(
                out,
                3,
                &MeshPrim {
                    name,
                    tags,
                    points: &mesh.vertices[vertices.clone()],
                    normals: &mesh.normals[vertices],
                    face_size: 3,
                    indices: indices.iter().map(|i| i - span.vertices.start).collect(),
                    colors: vec![lane_color(lane.kind)],
                    double_sided: false,
                },
            )?;
        }
        close(out, 2)?;
    }
    close(out, 1)?;
    Ok(paths)
}

/// One `Scope` per junction, with one `Mesh` per lane type: the part of the
/// [`wrap`](wrap::wrap) over the junction's lanes that lanes of that type
/// own, linked to the junction's lanes of that type.
fn junctions(
    net: &RoadNetwork,
    provenance: &Provenance,
    mesh: &Mesh,
    paths: &HashMap<LaneId, String>,
    out: &mut impl Write,
) -> io::Result<()> {
    open(out, 1, "def Scope", "Junctions", &[], &[])?;
    let junctions = junction_lanes(net, provenance, mesh);
    for (k, (junction, lanes)) in junctions.into_iter().enumerate() {
        let tags = [("junction", Tag::Text(junction.to_string()))];
        open(out, 2, "def Scope", &format!("junction_{k}"), &[], &tags)?;
        let wrap = junction_wrap(&lanes);
        let mut by_kind: BTreeMap<&str, (LaneType, Vec<&wrap::Face>)> = BTreeMap::new();
        for face in &wrap.faces {
            let kind = lanes.facets[face.facet].kind;
            by_kind
                .entry(kind.as_str())
                .or_insert((kind, Vec::new()))
                .1
                .push(face);
        }
        for (kind, faces) in by_kind.into_values() {
            let part = wrap.part(faces);
            let targets = (lanes.lanes.iter())
                .filter(|&&(k, _)| k == kind)
                .filter_map(|(_, lane)| paths.get(lane).cloned());
            write_mesh(
                out,
                3,
                &MeshPrim {
                    name: kind.as_str().replace('-', "_"),
                    tags: vec![
                        ("laneType", Tag::Text(kind.as_str().to_string())),
                        ("lanes", Tag::Targets(targets.collect())),
                    ],
                    points: &part.vertices,
                    normals: &part.normals,
                    face_size: 3,
                    indices: part.faces.iter().flat_map(|f| f.corners).collect(),
                    colors: vec![lane_color(kind)],
                    double_sided: false,
                },
            )?;
        }
        close(out, 2)?;
    }
    close(out, 1)
}

/// The triangles of one junction's lanes, and its lanes with their types.
#[derive(Default)]
struct JunctionLanes<'n> {
    facets: Vec<wrap::Facet>,
    lanes: Vec<(LaneType, LaneId)>,
    /// The junction's area, if its facets are set to be raised onto its
    /// elevation grid (see [`area::lay`]).
    grid: Option<&'n JunctionArea>,
}

/// The wrap over `lanes`, on the junction's elevation grid if it has one.
fn junction_wrap(lanes: &JunctionLanes) -> wrap::Wrap {
    let wrap = wrap::wrap(&lanes.facets);
    match lanes.grid {
        Some(area) => area::raise(area, &wrap),
        None => wrap,
    }
}

/// The [`JunctionLanes`] of each junction with lanes in `mesh`, by its
/// OpenDRIVE id. A junction with a [`boundary`] has its facets laid out as
/// the spec gives its ground, by [`area::lay`].
fn junction_lanes<'n>(
    net: &'n RoadNetwork,
    provenance: &Provenance,
    mesh: &Mesh,
) -> BTreeMap<&'n str, JunctionLanes<'n>> {
    let records: HashMap<LaneId, &LaneProvenance> =
        provenance.lanes.iter().map(|p| (p.lane, p)).collect();
    let mut out: BTreeMap<&str, JunctionLanes<'n>> = BTreeMap::new();
    let mut lifts: BTreeMap<&str, Vec<[f32; 3]>> = BTreeMap::new();
    for span in &mesh.lanes {
        let road = net.road_lane(span.lane).and_then(|at| net.road(at.road));
        let Some(junction) = road.and_then(|r| r.junction()) else {
            continue;
        };
        let lane = net.lane(span.lane).expect("a span's lane");
        let lanes = out.entry(junction).or_default();
        lanes.lanes.push((lane.kind, span.lane));
        let heights = records
            .get(&span.lane)
            .map_or_else(Vec::new, |record| heights(lane, record, span, mesh));
        let lift = |v: u32| {
            let k = (v - span.vertices.start) as usize;
            heights.get(k).copied().unwrap_or(0.0)
        };
        let indices = &mesh.indices[span.indices.start as usize..span.indices.end as usize];
        for t in indices.chunks_exact(3) {
            lanes.facets.push(wrap::Facet {
                corners: [0, 1, 2].map(|k| mesh.vertices[t[k] as usize]),
                normals: [0, 1, 2].map(|k| mesh.normals[t[k] as usize]),
                kind: lane.kind,
                fill: false,
            });
            lifts
                .entry(junction)
                .or_default()
                .push([0, 1, 2].map(|k| lift(t[k])));
        }
    }
    for (junction, lanes) in &mut out {
        if let Some(area) = boundary(net, provenance, junction) {
            let facets = std::mem::take(&mut lanes.facets);
            let lifts = lifts.get(junction).map_or(&[][..], Vec::as_slice);
            let (facets, raised) = area::lay(area, facets, lifts);
            lanes.facets = facets;
            lanes.grid = raised.then_some(area);
        }
    }
    out
}

/// The area of `junction`, if it has a `<boundary>` the load placed with
/// every segment. One with a segment dropped lays out as a junction without
/// one, since the load joins straight across where the segment was.
fn boundary<'n>(
    net: &'n RoadNetwork,
    provenance: &Provenance,
    junction: &str,
) -> Option<&'n JunctionArea> {
    let broken = provenance.warnings.iter().any(|w| {
        matches!(w, Warning::BoundarySegmentDropped { junction_id, .. } if junction_id == junction)
    });
    (net.junction_areas().iter())
        .find(|a| a.od_id == junction && !a.boundary.is_empty())
        .filter(|_| !broken)
}

/// The `<height>` of `lane` at each vertex of `span`, parallel to the span's
/// vertices: its inner height on its inner edge and its outer height on its
/// outer edge, at the centerline vertex nearest each rib.
fn heights(lane: &Lane, record: &LaneProvenance, span: &LaneSpan, mesh: &Mesh) -> Vec<f32> {
    let centre = lane.center.points();
    if record.heights.len() != centre.len() {
        return Vec::new();
    }
    let inner_left = record.od_id < 0;
    (span.vertices.start..span.vertices.end)
        .map(|v| {
            let left = (v - span.vertices.start).is_multiple_of(2);
            let rib = if left { v } else { v - 1 } as usize;
            let middle = mesh.vertices[rib].lerp(mesh.vertices[rib + 1], 0.5);
            let distance = |i: &usize| centre[*i].distance_squared_to(middle);
            let nearest = (0..centre.len())
                .min_by(|a, b| distance(a).total_cmp(&distance(b)))
                .expect("a centerline");
            let height = record.heights[nearest];
            if left == inner_left {
                height.inner
            } else {
                height.outer
            }
        })
        .collect()
}

/// One `Mesh` per painted road mark: a quad per piece, [`LIFT`] above the lane.
fn road_marks(net: &RoadNetwork, out: &mut impl Write) -> io::Result<()> {
    open(out, 1, "def Scope", "RoadMarks", &[], &[])?;
    for mark in net.road_marks() {
        let pieces: Vec<(&String, &[Point; 4])> = mark
            .lines
            .iter()
            .flat_map(|l| l.pieces.iter().map(move |p| (&l.color, p)))
            .collect();
        if pieces.is_empty() {
            continue;
        }
        let points: Vec<Point> = pieces
            .iter()
            .flat_map(|(_, quad)| quad.map(|p| p + Vector::Z * LIFT))
            .collect();
        write_mesh(
            out,
            2,
            &MeshPrim {
                name: format!("mark_{}", mark.id.0),
                tags: vec![
                    ("type", Tag::Text(mark.kind.as_str().to_string())),
                    ("weight", Tag::Text(mark.weight.as_str().to_string())),
                    ("color", Tag::Text(mark.color.clone())),
                    (
                        "laneChange",
                        Tag::Text(mark.lane_change.as_str().to_string()),
                    ),
                ],
                points: &points,
                normals: &[],
                face_size: 4,
                indices: (0..points.len() as u32).collect(),
                colors: pieces.iter().map(|(color, _)| paint(color)).collect(),
                double_sided: false,
            },
        )?;
    }
    close(out, 1)
}

/// One double-sided `Mesh` per object, since [`RoadNetwork::object_mesh`]
/// gives flat shapes one face. Returns each object's path.
fn objects(
    net: &RoadNetwork,
    provenance: &Provenance,
    mesh: &Mesh,
    out: &mut impl Write,
) -> io::Result<HashMap<ObjectId, String>> {
    let mut paths = HashMap::new();
    open(out, 1, "def Scope", "Objects", &[], &[])?;
    for span in mesh.objects.iter().filter(|s| !s.indices.is_empty()) {
        let object = net.object(span.object).expect("a span's object");
        let mut tags = vec![
            ("type", Tag::Text(object.kind.as_str().to_string())),
            ("subtype", Tag::Text(object.subtype.clone())),
            ("name", Tag::Text(object.name.clone())),
        ];
        if let Some(p) = provenance.objects.iter().find(|p| p.object == object.id) {
            tags.push(("objectId", Tag::Text(p.od_id.clone())));
            tags.push(("roadId", Tag::Text(p.road_id.clone())));
        }
        let vertices = span.vertices.start as usize..span.vertices.end as usize;
        let indices = &mesh.indices[span.indices.start as usize..span.indices.end as usize];
        let name = format!("object_{}", object.id.0);
        paths.insert(object.id, format!("/Map/Objects/{name}"));
        write_mesh(
            out,
            2,
            &MeshPrim {
                name,
                tags,
                points: &mesh.vertices[vertices.clone()],
                normals: &mesh.normals[vertices],
                face_size: 3,
                indices: indices.iter().map(|i| i - span.vertices.start).collect(),
                colors: vec![[0.6, 0.6, 0.6]],
                double_sided: true,
            },
        )?;
    }
    close(out, 1)?;
    Ok(paths)
}

/// Dark grey for driving lanes, light for sidewalks and curbs, mid otherwise.
fn lane_color(kind: LaneType) -> [f32; 3] {
    match kind {
        _ if kind.is_drivable() => [0.2, 0.2, 0.2],
        LaneType::Sidewalk | LaneType::Curb => [0.6, 0.6, 0.6],
        _ => [0.4, 0.4, 0.4],
    }
}

/// The colour of an OpenDRIVE paint name. Unknown names are white.
fn paint(name: &str) -> [f32; 3] {
    match name {
        "yellow" => [0.95, 0.75, 0.1],
        "red" => [0.8, 0.1, 0.1],
        "blue" => [0.1, 0.3, 0.8],
        "green" => [0.1, 0.6, 0.2],
        "orange" => [0.95, 0.5, 0.1],
        "violet" => [0.5, 0.2, 0.7],
        _ => [0.9, 0.9, 0.9],
    }
}

/// The value of an `xodr:` attribute, or the targets of an `xodr:`
/// relationship.
enum Tag {
    Text(String),
    Token(&'static str),
    Int(i64),
    Float(f32),
    Double(f64),
    Bool(bool),
    Texts(Vec<String>),
    Points(Vec<Point>),
    Targets(Vec<String>),
}

/// One `Mesh` prim with `face_size` indices per face. `colors` holds one
/// colour for the whole mesh or one per face. Empty `normals` lets the
/// renderer compute them.
struct MeshPrim<'a> {
    name: String,
    tags: Vec<(&'static str, Tag)>,
    points: &'a [Point],
    normals: &'a [Vector],
    face_size: usize,
    indices: Vec<u32>,
    colors: Vec<[f32; 3]>,
    double_sided: bool,
}

fn write_mesh(out: &mut impl Write, depth: usize, m: &MeshPrim) -> io::Result<()> {
    open(out, depth, "def Mesh", &m.name, &[], &m.tags)?;
    let pad = indent(depth + 1);
    let (low, high) = extent(m.points);
    writeln!(
        out,
        "{pad}float3[] extent = [{}, {}]",
        tuple(low),
        tuple(high)
    )?;
    let faces = m.indices.len() / m.face_size;
    writeln!(
        out,
        "{pad}int[] faceVertexCounts = [{}]",
        list(std::iter::repeat_n(m.face_size, faces))
    )?;
    writeln!(out, "{pad}int[] faceVertexIndices = [{}]", list(&m.indices))?;
    let points = m.points.iter().map(|p| tuple(p.to_array()));
    writeln!(out, "{pad}point3f[] points = [{}]", list(points))?;
    if !m.normals.is_empty() {
        let normals = m.normals.iter().map(|n| tuple(n.to_array()));
        writeln!(out, "{pad}normal3f[] normals = [{}] (", list(normals))?;
        writeln!(out, "{pad}    interpolation = \"vertex\"")?;
        writeln!(out, "{pad})")?;
    }
    let interpolation = if m.colors.len() == 1 {
        "constant"
    } else {
        "uniform"
    };
    let colors = m.colors.iter().map(|&c| tuple(c));
    writeln!(
        out,
        "{pad}color3f[] primvars:displayColor = [{}] (",
        list(colors)
    )?;
    writeln!(out, "{pad}    interpolation = \"{interpolation}\"")?;
    writeln!(out, "{pad})")?;
    if m.double_sided {
        writeln!(out, "{pad}uniform bool doubleSided = 1")?;
    }
    writeln!(out, "{pad}uniform token subdivisionScheme = \"none\"")?;
    close(out, depth)
}

/// Open a prim, such as `def Scope "Roads"`, with its metadata lines and
/// its `xodr:` attributes. Empty arrays and relationships are left out.
fn open(
    out: &mut impl Write,
    depth: usize,
    head: &str,
    name: &str,
    meta: &[String],
    tags: &[(&str, Tag)],
) -> io::Result<()> {
    let pad = indent(depth);
    if meta.is_empty() {
        writeln!(out, "{pad}{head} \"{name}\"")?;
    } else {
        writeln!(out, "{pad}{head} \"{name}\" (")?;
        for line in meta {
            writeln!(out, "{pad}    {line}")?;
        }
        writeln!(out, "{pad})")?;
    }
    writeln!(out, "{pad}{{")?;
    for (key, value) in tags {
        let line = match value {
            Tag::Text(s) => format!("custom string xodr:{key} = {}", quote(s)),
            Tag::Token(s) => format!("custom token xodr:{key} = {}", quote(s)),
            Tag::Int(n) => format!("custom int xodr:{key} = {n}"),
            Tag::Float(x) => format!("custom float xodr:{key} = {x}"),
            Tag::Double(x) => format!("custom double xodr:{key} = {x}"),
            Tag::Bool(b) => format!("custom bool xodr:{key} = {}", u8::from(*b)),
            Tag::Texts(v) if v.is_empty() => continue,
            Tag::Texts(v) => format!(
                "custom string[] xodr:{key} = [{}]",
                list(v.iter().map(|s| quote(s)))
            ),
            Tag::Points(v) if v.is_empty() => continue,
            Tag::Points(v) => format!(
                "custom point3f[] xodr:{key} = [{}]",
                list(v.iter().map(|p| tuple(p.to_array())))
            ),
            Tag::Targets(v) if v.is_empty() => continue,
            Tag::Targets(v) => format!(
                "rel xodr:{key} = [{}]",
                list(v.iter().map(|p| format!("<{p}>")))
            ),
        };
        writeln!(out, "{pad}    {line}")?;
    }
    Ok(())
}

fn close(out: &mut impl Write, depth: usize) -> io::Result<()> {
    writeln!(out, "{}}}", indent(depth))
}

fn indent(depth: usize) -> String {
    "    ".repeat(depth)
}

/// The corners of the box around `points`.
fn extent(points: &[Point]) -> ([f32; 3], [f32; 3]) {
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for p in points {
        for (k, v) in p.to_array().into_iter().enumerate() {
            low[k] = low[k].min(v);
            high[k] = high[k].max(v);
        }
    }
    (low, high)
}

fn tuple([x, y, z]: [f32; 3]) -> String {
    format!("({x}, {y}, {z})")
}

fn list<T: std::fmt::Display>(items: impl IntoIterator<Item = T>) -> String {
    items
        .into_iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `s` as a USD string literal.
fn quote(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}
