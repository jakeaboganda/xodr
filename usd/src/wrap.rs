//! One surface over a junction's overlapping lanes.
//!
//! A junction's connecting lanes lie over one another. [`wrap`] lays one
//! surface over them that covers each point once, on the lane that owns the
//! ground there (see [`Ground::owners`]), so it follows each slope and bank
//! rather than flattening them. A lane more than [`CLEARANCE`] over another,
//! such as a bridge over a road, is a level of its own, with its own surface.

use std::collections::{BTreeMap, HashMap, HashSet};

use spade::handles::{FixedFaceHandle, FixedVertexHandle, InnerTag};
use spade::{
    AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation,
};
use xodr::{LaneType, Point, Vector};

use crate::supports::carries_traffic;

/// Metres within which two corners are one point.
const WELD: f64 = 1e-3;

/// Metres within which two lanes are at one height.
const TOLERANCE: f64 = 0.01;

/// Metres a patch may stray from its plane. Laying a patch again from its
/// outline, leaving out a crease between lanes this close, and sharing a
/// corner across a [`SEAM`] each move the wrap off the owner by at most
/// twice this, so [`TOLERANCE`] in all.
const FLAT: f64 = TOLERANCE / 6.0;

/// Metres within which faces on different facets share a corner: as far
/// apart as two facets of a kind get with no crease between them, so no
/// gap opens there.
pub(crate) const SEAM: f64 = 2.0 * FLAT;

/// Metres a lane must clear another to be a level of its own.
const CLEARANCE: f64 = 2.0;

/// Twice the area, in square metres, below which a face is dropped: what
/// `xodr` calls a triangle with no area.
const MIN_AREA: f32 = 1e-6;

/// Twice the area in plan, in square metres, below which a facet stands on
/// edge and covers nothing.
const EDGE_ON: f64 = 1e-12;

/// Metres on a side of a cell of the indexes over facets and outlines.
pub(crate) const CELL: f64 = 4.0;

/// One triangle of a lane, with its corners' normals.
pub(crate) struct Facet {
    pub corners: [Point; 3],
    pub normals: [Vector; 3],
    pub kind: LaneType,
    /// Whether it is ground a junction's `<boundary>` takes in rather than a
    /// lane: it owns only ground no lane in its level covers.
    pub fill: bool,
}

/// The surface over a set of [`Facet`]s.
#[derive(Default)]
pub(crate) struct Wrap {
    pub vertices: Vec<Point>,
    pub normals: Vec<Vector>,
    pub faces: Vec<Face>,
}

/// One triangle of a [`Wrap`], counter-clockwise from above.
pub(crate) struct Face {
    pub corners: [u32; 3],
    /// The facet that owns the ground under it.
    pub facet: usize,
}

impl Wrap {
    /// The wrap of `faces` alone, without the vertices they don't use.
    pub(crate) fn part<'a>(&self, faces: impl IntoIterator<Item = &'a Face>) -> Wrap {
        let mut index = vec![None; self.vertices.len()];
        let mut out = Wrap::default();
        for face in faces {
            let corners = face.corners.map(|k| {
                *index[k as usize].get_or_insert_with(|| {
                    out.vertices.push(self.vertices[k as usize]);
                    out.normals.push(self.normals[k as usize]);
                    out.vertices.len() as u32 - 1
                })
            });
            out.faces.push(Face {
                corners,
                facet: face.facet,
            });
        }
        out
    }
}

type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;
type FaceKey = FixedFaceHandle<InnerTag>;

/// A face of the triangulation on one level: 0 for the highest over it.
type Slot = (FaceKey, usize);

/// The surface over `facets`, within [`TOLERANCE`] of the owner everywhere.
///
/// Every facet edge, and every line where the owner can change between two
/// overlapping facets, is an edge of a triangulation. So across each of its
/// faces, one facet owns each level, and the face is laid on it. Faces on
/// different facets share a vertex only where the facets meet in height, so
/// a step, such as a kerb, stays a step. Then each patch of faces on about
/// one plane, of one lane type, is laid again from its outline alone, which
/// drops the vertices inside it.
pub(crate) fn wrap(facets: &[Facet]) -> Wrap {
    let ground = Ground::new(facets);
    let cdt = triangulate(&ground);
    let mut vertices = Vertices::default();
    let laid = lay(&cdt, &ground, &mut vertices);
    let mut faces = Vec::new();
    for patch in patches(&cdt, &ground, &laid) {
        let facet = laid[&patch[0]].facet;
        for corners in relay(&cdt, &laid, &patch) {
            let [a, b, c] = corners.map(|k| vertices.points[k as usize]);
            if (b - a).cross(c - a).z > MIN_AREA {
                faces.push(Face { corners, facet });
            }
        }
    }
    let all = Wrap {
        vertices: vertices.points,
        normals: vertices
            .normals
            .into_iter()
            .map(|n| n.normalize_or(Vector::Z))
            .collect(),
        faces,
    };
    all.part(&all.faces)
}

/// How many times [`constrain`] splits a piece of an edge at a crossing
/// rounding hid from [`split`], before it gives the piece up.
const RESPLITS: usize = 4;

/// The triangulation with every facet edge and every crease as an edge.
///
/// The edges are split where they cross here, with each crossing welded,
/// rather than by spade, whose splitting fails an assertion on crossings
/// that nearly meet a vertex.
fn triangulate(ground: &Ground) -> Cdt {
    let mut cdt = Cdt::new();
    let reach = (ground.planes.iter().flatten())
        .flat_map(|plane| [plane.low, plane.high])
        .fold(0.0, |r: f64, p| r.max(p.x.abs()).max(p.y.abs()));
    let mut welder = Welder::new(reach);
    let sides = ground.facets.iter().flat_map(|f| {
        let [a, b, c] = f.corners.map(plan);
        [[a, b], [b, c], [c, a]]
    });
    let mut ends = HashSet::new();
    let mut segments = Vec::new();
    for [a, b] in sides.chain(creases(ground)) {
        let (ha, hb) = (welder.insert(&mut cdt, a), welder.insert(&mut cdt, b));
        if ha != hb && ends.insert((ha.min(hb), ha.max(hb))) {
            segments.push([ha, hb].map(|h| cdt.vertex(h).position()));
        }
    }
    let pieces: Vec<Vec<FixedVertexHandle>> = split(&segments, welder.radius)
        .into_iter()
        .map(|points| {
            points
                .into_iter()
                .map(|p| welder.insert(&mut cdt, p))
                .collect()
        })
        .collect();
    for piece in pieces {
        for pair in piece.windows(2) {
            constrain(&mut cdt, &mut welder, pair[0], pair[1], RESPLITS);
        }
    }
    cdt
}

/// Makes `a b` an edge of `cdt`. Where it crosses an edge already there,
/// which rounding can hide from [`split`], both are split at a welded
/// crossing, `depth` times at most.
fn constrain(
    cdt: &mut Cdt,
    welder: &mut Welder,
    a: FixedVertexHandle,
    b: FixedVertexHandle,
    depth: usize,
) {
    if a == b || cdt.exists_constraint(a, b) {
        return;
    }
    if cdt.can_add_constraint(a, b) {
        cdt.add_constraint(a, b);
        return;
    }
    let blocking = cdt
        .get_conflicting_edges_between_vertices(a, b)
        .next()
        .map(|e| (e.as_undirected().fix(), e.from().fix(), e.to().fix()));
    let Some((edge, c, d)) = blocking.filter(|_| depth > 0) else {
        return;
    };
    let at = |v: FixedVertexHandle| cdt.vertex(v).position();
    let (pa, pb, pc, pd) = (at(a), at(b), at(c), at(d));
    let (dc, dd) = (cross(pa, pb, pc), cross(pa, pb, pd));
    let t = dc / (dc - dd);
    let crossing = Point2::new(pc.x + (pd.x - pc.x) * t, pc.y + (pd.y - pc.y) * t);
    let v = welder.insert(cdt, crossing);
    cdt.remove_constraint_edge(edge);
    for (from, to) in [(c, v), (v, d), (a, v), (v, b)] {
        constrain(cdt, welder, from, to, depth - 1);
    }
}

/// Each segment as the points along it, from end to end, where another
/// crosses it or ends within `radius` of it.
fn split(segments: &[[Point2<f64>; 2]], radius: f64) -> Vec<Vec<Point2<f64>>> {
    let mut cells: HashMap<Cell, Vec<usize>> = HashMap::new();
    for (k, &[a, b]) in segments.iter().enumerate() {
        let low = Point2::new(a.x.min(b.x) - radius, a.y.min(b.y) - radius);
        let high = Point2::new(a.x.max(b.x) + radius, a.y.max(b.y) + radius);
        for c in cells_between(low, high, CELL) {
            cells.entry(c).or_default().push(k);
        }
    }
    let mut along: Vec<Vec<(f64, Point2<f64>)>> = segments
        .iter()
        .map(|&[a, b]| vec![(0.0, a), (1.0, b)])
        .collect();
    let mut seen = HashSet::new();
    for near in cells.values() {
        for (n, &i) in near.iter().enumerate() {
            for &j in &near[n + 1..] {
                if !seen.insert((i.min(j), i.max(j))) {
                    continue;
                }
                for (k, t, p) in crossings(segments[i], segments[j], radius) {
                    along[[i, j][k]].push((t, p));
                }
            }
        }
    }
    along
        .into_iter()
        .map(|mut points| {
            points.sort_by(|a, b| {
                (a.0.total_cmp(&b.0))
                    .then(a.1.x.total_cmp(&b.1.x))
                    .then(a.1.y.total_cmp(&b.1.y))
            });
            points.into_iter().map(|(_, p)| p).collect()
        })
        .collect()
}

/// Where segments `a` and `b` meet away from their own ends, an end within
/// `radius` of the other counting: each point, with which of the two (0
/// for `a`) it splits and how far along it.
fn crossings(
    a: [Point2<f64>; 2],
    b: [Point2<f64>; 2],
    radius: f64,
) -> Vec<(usize, f64, Point2<f64>)> {
    let along = |s: [Point2<f64>; 2], p: Point2<f64>| {
        let (dx, dy) = (s[1].x - s[0].x, s[1].y - s[0].y);
        ((p.x - s[0].x) * dx + (p.y - s[0].y) * dy) / (dx * dx + dy * dy)
    };
    let inner = |t: f64| t > 0.0 && t < 1.0;
    let mut out = Vec::new();
    for (k, s, other) in [(0, a, b), (1, b, a)] {
        for p in other {
            let t = along(s, p);
            if inner(t) && distance_2(p, s[0], s[1]) <= radius * radius {
                out.push((k, t, p));
            }
        }
    }
    let (da, db) = (cross(a[0], a[1], b[0]), cross(a[0], a[1], b[1]));
    let (dc, dd) = (cross(b[0], b[1], a[0]), cross(b[0], b[1], a[1]));
    if da * db < 0.0 && dc * dd < 0.0 {
        let t = dc / (dc - dd);
        let p = Point2::new(
            a[0].x + (a[1].x - a[0].x) * t,
            a[0].y + (a[1].y - a[0].y) * t,
        );
        out.push((0, t, p));
        out.push((1, along(b, p), p));
    }
    out
}

/// A face of the triangulation laid on the ground: the facet it lies on,
/// and its corners in the wrap.
struct Laid {
    facet: usize,
    corners: [u32; 3],
}

/// The wrap's vertices, made as faces are laid: one per corner of the
/// triangulation and height, within [`SEAM`].
#[derive(Default)]
struct Vertices {
    points: Vec<Point>,
    /// The sum of the normals of the faces laid on each.
    normals: Vec<Vector>,
    made: HashMap<FixedVertexHandle, Vec<u32>>,
}

impl Vertices {
    /// The vertex at `corner`, at `facet`'s height there.
    fn on(
        &mut self,
        corner: FixedVertexHandle,
        p: Point2<f64>,
        plane: &Plane,
        facet: &Facet,
    ) -> u32 {
        let at = Point::new(p.x as f32, p.y as f32, plane.height(p) as f32);
        let made = self.made.entry(corner).or_default();
        let level = |&k: &u32| f64::from((self.points[k as usize].z - at.z).abs()) <= SEAM;
        let k = match made.iter().copied().find(level) {
            Some(k) => k,
            None => {
                self.points.push(at);
                self.normals.push(Vector::ZERO);
                made.push(self.points.len() as u32 - 1);
                self.points.len() as u32 - 1
            }
        };
        self.normals[k as usize] = self.normals[k as usize] + plane.normal(facet, p);
        k
    }
}

/// Each face of `cdt` that the ground covers, once per level, laid on the
/// facet that owns that level.
fn lay(cdt: &Cdt, ground: &Ground, vertices: &mut Vertices) -> BTreeMap<Slot, Laid> {
    let mut laid = BTreeMap::new();
    for face in cdt.inner_faces() {
        let corners = face.vertices();
        let [a, b, c] = corners.map(|v| v.position());
        let middle = Point2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
        for (level, facet) in ground.owners(middle).into_iter().enumerate() {
            let plane = ground.plane(facet);
            let corners =
                corners.map(|v| vertices.on(v.fix(), v.position(), plane, &ground.facets[facet]));
            laid.insert((face.fix(), level), Laid { facet, corners });
        }
    }
    laid
}

/// The laid faces in patches. A patch is connected, of one lane type,
/// within [`FLAT`] of the plane of its first face, and shares both corners
/// of each edge between its faces, so no step inside.
fn patches(cdt: &Cdt, ground: &Ground, laid: &BTreeMap<Slot, Laid>) -> Vec<Vec<Slot>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (&seed, first) in laid {
        if !seen.insert(seed) {
            continue;
        }
        let plane = ground.plane(first.facet);
        let kind = ground.facets[first.facet].kind;
        let mut patch = vec![seed];
        let mut next = 0;
        while let Some(&at) = patch.get(next) {
            next += 1;
            let here = &laid[&at];
            for (edge, across) in neighbours(cdt, at.0) {
                let Some(across) = across else {
                    continue;
                };
                let vertices = corners(cdt, across);
                for (&slot, other) in laid.range((across, 0)..=(across, usize::MAX)) {
                    let fits = ground.facets[other.facet].kind == kind
                        && edge
                            .iter()
                            .all(|&k| other.corners.contains(&here.corners[k]))
                        && near(
                            &vertices.map(|v| cdt.vertex(v).position()),
                            plane,
                            ground.plane(other.facet),
                        );
                    if fits && seen.insert(slot) {
                        patch.push(slot);
                    }
                }
            }
        }
        out.push(patch);
    }
    out
}

/// The corners of `face`.
fn corners(cdt: &Cdt, face: FaceKey) -> [FixedVertexHandle; 3] {
    cdt.face(face).vertices().map(|v| v.fix())
}

/// Each edge of `face`, as indices into its corners, with the face across
/// it, `None` past the triangulation's hull.
fn neighbours(cdt: &Cdt, face: FaceKey) -> [([usize; 2], Option<FaceKey>); 3] {
    let face = cdt.face(face);
    let vertices = face.vertices().map(|v| v.fix());
    let slot = |v: FixedVertexHandle| vertices.iter().position(|&w| w == v).expect("a corner");
    face.adjacent_edges().map(|edge| {
        let across = edge.rev().face().as_inner().map(|f| f.fix());
        ([slot(edge.from().fix()), slot(edge.to().fix())], across)
    })
}

/// Whether `a` and `b` are within [`FLAT`] of each other at `points`.
fn near(points: &[Point2<f64>; 3], a: &Plane, b: &Plane) -> bool {
    points
        .iter()
        .all(|&p| (a.height(p) - b.height(p)).abs() <= FLAT)
}

/// The faces of `patch`, by their corners in the wrap: laid again from its
/// outline, or as they are for a patch of one face or an outline that
/// rounding has made cross itself.
fn relay(cdt: &Cdt, laid: &BTreeMap<Slot, Laid>, patch: &[Slot]) -> Vec<[u32; 3]> {
    let own = || patch.iter().map(|slot| laid[slot].corners).collect();
    if patch.len() == 1 {
        return own();
    }
    outline(cdt, laid, patch).unwrap_or_else(own)
}

/// A triangulation of `patch`'s outline, by its faces' corners in the wrap.
/// The outline keeps every corner of the patch within [`WELD`] of it, so a
/// vertex where the patch meets a road stays. Spade's refinement, adding no
/// vertices, finds the faces outside the outline. `None` if the outline
/// crosses itself.
fn outline(cdt: &Cdt, laid: &BTreeMap<Slot, Laid>, patch: &[Slot]) -> Option<Vec<[u32; 3]>> {
    let inside: HashSet<FaceKey> = patch.iter().map(|slot| slot.0).collect();
    let mut edges = Vec::new();
    let mut corners = BTreeMap::new();
    for slot in patch {
        let positions = cdt.face(slot.0).vertices().map(|v| v.position());
        for (edge, across) in neighbours(cdt, slot.0) {
            if !across.is_some_and(|n| inside.contains(&n)) {
                edges.push(edge.map(|k| positions[k]));
            }
        }
        corners.extend(laid[slot].corners.into_iter().zip(positions));
    }
    let mut cells: HashMap<Cell, Vec<usize>> = HashMap::new();
    for (k, &[a, b]) in edges.iter().enumerate() {
        let low = Point2::new(a.x.min(b.x) - WELD, a.y.min(b.y) - WELD);
        let high = Point2::new(a.x.max(b.x) + WELD, a.y.max(b.y) + WELD);
        for c in cells_between(low, high, CELL) {
            cells.entry(c).or_default().push(k);
        }
    }
    let on_outline = |p: Point2<f64>| {
        let near = cells.get(&cell(p, CELL)).into_iter().flatten();
        near.map(|&k| edges[k])
            .any(|[a, b]| distance_2(p, a, b) <= WELD * WELD)
    };
    let kept = corners.into_iter().filter(|&(_, p)| on_outline(p));
    let mut outline = Cdt::new();
    let mut wrapped = HashMap::new();
    for (k, p) in kept {
        wrapped.insert(outline.insert(p).expect("a finite corner"), k);
    }
    for [a, b] in edges {
        let [a, b] = [a, b].map(|p| outline.insert(p).expect("a finite corner"));
        if a != b && !outline.exists_constraint(a, b) {
            if !outline.can_add_constraint(a, b) {
                return None;
            }
            outline.add_constraint(a, b);
        }
    }
    let outside: HashSet<_> = outline
        .refine(
            RefinementParameters::<f64>::new()
                .exclude_outer_faces(true)
                .with_max_additional_vertices(0)
                .with_angle_limit(AngleLimit::from_deg(0.0)),
        )
        .excluded_faces
        .into_iter()
        .collect();
    let faces = outline
        .inner_faces()
        .filter(|f| !outside.contains(&f.fix()));
    Some(
        faces
            .map(|f| f.vertices().map(|v| wrapped[&v.fix()]))
            .collect(),
    )
}

/// The square of the distance from `p` to the segment `a b`.
fn distance_2(p: Point2<f64>, a: Point2<f64>, b: Point2<f64>) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (x, y) = (a.x + t * dx - p.x, a.y + t * dy - p.y);
    x * x + y * y
}

/// Merges corners closer than `radius` in plan into one vertex.
struct Welder {
    radius: f64,
    cells: HashMap<Cell, Vec<FixedVertexHandle>>,
}

impl Welder {
    /// A welder for corners as far as `reach` from the origin: within
    /// [`WELD`], or two steps of an `f32` there if that is more, so no two
    /// vertices round to one point.
    fn new(reach: f64) -> Self {
        Self {
            radius: WELD.max(2.0 * f64::from(f32::EPSILON) * reach),
            cells: HashMap::new(),
        }
    }

    /// The vertex at `p`: one already within `radius`, or a new one.
    fn insert(&mut self, cdt: &mut Cdt, p: Point2<f64>) -> FixedVertexHandle {
        let cell = cell(p, self.radius);
        let near = (-1..=1)
            .flat_map(|i| (-1..=1).map(move |j| (cell.0 + i, cell.1 + j)))
            .filter_map(|c| self.cells.get(&c))
            .flatten()
            .find(|&&h| cdt.vertex(h).position().distance_2(p) <= self.radius * self.radius);
        if let Some(&h) = near {
            return h;
        }
        let h = cdt.insert(p).expect("a finite corner");
        self.cells.entry(cell).or_default().push(h);
        h
    }
}

/// A square of a grid over the plan, by its column and row.
pub(crate) type Cell = (i64, i64);

pub(crate) fn cell(p: Point2<f64>, size: f64) -> Cell {
    ((p.x / size).floor() as i64, (p.y / size).floor() as i64)
}

/// Every cell `size` on a side that the box from `low` to `high` touches.
pub(crate) fn cells_between(
    low: Point2<f64>,
    high: Point2<f64>,
    size: f64,
) -> impl Iterator<Item = Cell> {
    let (low, high) = (cell(low, size), cell(high, size));
    (low.0..=high.0).flat_map(move |i| (low.1..=high.1).map(move |j| (i, j)))
}

pub(crate) fn plan(p: Point) -> Point2<f64> {
    Point2::new(f64::from(p.x), f64::from(p.y))
}

/// The facets, seen from above, indexed by where they lie.
struct Ground<'a> {
    facets: &'a [Facet],
    /// Parallel to `facets`. `None` for a facet on edge.
    planes: Vec<Option<Plane>>,
    cells: HashMap<Cell, Vec<usize>>,
}

impl<'a> Ground<'a> {
    fn new(facets: &'a [Facet]) -> Self {
        let planes: Vec<Option<Plane>> = facets.iter().map(Plane::new).collect();
        let mut cells: HashMap<Cell, Vec<usize>> = HashMap::new();
        for (k, plane) in planes.iter().enumerate() {
            let Some(plane) = plane else { continue };
            for c in cells_between(plane.low, plane.high, CELL) {
                cells.entry(c).or_default().push(k);
            }
        }
        Self {
            facets,
            planes,
            cells,
        }
    }

    fn plane(&self, facet: usize) -> &Plane {
        self.planes[facet].as_ref().expect("a facet not on edge")
    }

    fn traffic(&self, facet: usize) -> bool {
        carries_traffic(self.facets[facet].kind)
    }

    /// The facets that own the ground at `p`, one per level, highest first.
    ///
    /// The facets over `p` fall into levels, split where one clears the
    /// next below by more than [`CLEARANCE`]. A level's owner is its
    /// highest facet, or the highest that carries traffic among those
    /// within [`TOLERANCE`] of it, so a road wins a tie with a sidewalk.
    fn owners(&self, p: Point2<f64>) -> Vec<usize> {
        let mut over: Vec<(usize, f64)> = self
            .cells
            .get(&cell(p, CELL))
            .into_iter()
            .flatten()
            .filter_map(|&k| {
                let plane = self.planes[k].as_ref()?;
                plane.covers(p).then(|| (k, plane.height(p)))
            })
            .collect();
        over.sort_by(|a, b| b.1.total_cmp(&a.1));
        over.chunk_by(|a, b| a.1 - b.1 <= CLEARANCE)
            .map(|level| {
                let lanes = level.iter().any(|&(k, _)| !self.facets[k].fill);
                let level: Vec<_> = (level.iter())
                    .filter(|&&(k, _)| !(lanes && self.facets[k].fill))
                    .collect();
                let top = level[0].1;
                let candidates = level.iter().filter(|(_, z)| *z >= top - TOLERANCE);
                let (k, _) = candidates
                    .max_by(|a, b| {
                        (self.traffic(a.0).cmp(&self.traffic(b.0))).then(a.1.total_cmp(&b.1))
                    })
                    .expect("a level has a facet");
                *k
            })
            .collect()
    }
}

/// A facet seen from above: its corners counter-clockwise in plan, and the
/// height across it.
struct Plane {
    corners: [Point2<f64>; 3],
    heights: [f64; 3],
    /// Which of the facet's corners each of `corners` is.
    order: [usize; 3],
    low: Point2<f64>,
    high: Point2<f64>,
}

impl Plane {
    /// `None` for a facet on edge.
    fn new(facet: &Facet) -> Option<Self> {
        let p = facet.corners.map(plan);
        let area = cross(p[0], p[1], p[2]);
        if area.abs() < EDGE_ON {
            return None;
        }
        let order = if area > 0.0 { [0, 1, 2] } else { [0, 2, 1] };
        let (xs, ys) = (p.map(|c| c.x), p.map(|c| c.y));
        let min = |v: [f64; 3]| v.into_iter().fold(f64::INFINITY, f64::min);
        let max = |v: [f64; 3]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
        Some(Self {
            corners: order.map(|k| p[k]),
            heights: order.map(|k| f64::from(facet.corners[k].z)),
            order,
            low: Point2::new(min(xs), min(ys)),
            high: Point2::new(max(xs), max(ys)),
        })
    }

    /// The weights of `p` against the corners, in `corners` order.
    fn weights(&self, p: Point2<f64>) -> [f64; 3] {
        let [a, b, c] = self.corners;
        let area = cross(a, b, c);
        let u = cross(p, b, c) / area;
        let v = cross(a, p, c) / area;
        [u, v, 1.0 - u - v]
    }

    fn covers(&self, p: Point2<f64>) -> bool {
        self.weights(p).iter().all(|&w| w >= 0.0)
    }

    /// The height over `p`, on the facet's plane even outside it.
    fn height(&self, p: Point2<f64>) -> f64 {
        let w = self.weights(p);
        w[0] * self.heights[0] + w[1] * self.heights[1] + w[2] * self.heights[2]
    }

    /// The facet's corner normals blended at `p`, turned up.
    fn normal(&self, facet: &Facet, p: Point2<f64>) -> Vector {
        let w = self.weights(p);
        let mut n = Vector::ZERO;
        for (k, &corner) in self.order.iter().enumerate() {
            n = n + w[k] as f32 * facet.normals[corner];
        }
        let n = n.normalize_or(Vector::Z);
        if n.z < 0.0 {
            -n
        } else {
            n
        }
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.low.x <= other.high.x
            && other.low.x <= self.high.x
            && self.low.y <= other.high.y
            && other.low.y <= self.high.y
    }
}

/// Twice the signed area of `a b c`, positive counter-clockwise.
pub(crate) fn cross(a: Point2<f64>, b: Point2<f64>, c: Point2<f64>) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Each line, in plan, across the overlap of two facets where the owner
/// can change between them (see [`switches`]).
fn creases(ground: &Ground) -> Vec<[Point2<f64>; 2]> {
    let mut sorted: Vec<(usize, &Plane)> = (ground.planes.iter().enumerate())
        .filter_map(|(k, plane)| Some((k, plane.as_ref()?)))
        .collect();
    sorted.sort_by(|a, b| a.1.low.x.total_cmp(&b.1.low.x));
    let mut out = Vec::new();
    for (i, &(j, a)) in sorted.iter().enumerate() {
        for &(k, b) in sorted[i + 1..]
            .iter()
            .take_while(|(_, b)| b.low.x <= a.high.x)
        {
            if !a.overlaps(b) {
                continue;
            }
            let overlap = clip(&a.corners, b);
            let gaps: Vec<f64> = overlap.iter().map(|&p| a.height(p) - b.height(p)).collect();
            let fill = ground.facets[j].fill || ground.facets[k].fill;
            let switches = switches(ground.traffic(j), ground.traffic(k), fill);
            out.extend(
                switches
                    .into_iter()
                    .filter_map(|s| crease(&overlap, &gaps, s)),
            );
        }
    }
    out
}

/// Where the owner can change between facets `a` and `b`, as the gap in
/// height of `a` over `b`, with how far the gap must range across their
/// overlap before the line needs to be an edge. Between a lane that
/// carries traffic and one that doesn't, the owner changes where the other
/// clears it by [`TOLERANCE`]; between two of a kind, where they cross, but
/// only once they part by more than twice [`FLAT`]. Levels part at
/// [`CLEARANCE`] either way. Fill never owns ground a lane in its level
/// covers, so with fill only the levels change, and the lines are those.
fn switches(a: bool, b: bool, fill: bool) -> Vec<(f64, f64)> {
    let tie = match (a, b) {
        (true, false) => (-TOLERANCE, 0.0),
        (false, true) => (TOLERANCE, 0.0),
        _ => (0.0, 2.0 * FLAT),
    };
    let levels = [(CLEARANCE, 0.0), (-CLEARANCE, 0.0)];
    let tie = (!fill).then_some(tie);
    tie.into_iter().chain(levels).collect()
}

/// The line across `overlap` where `gaps`, the gap in height at each of
/// its corners, equals `at`, if the gaps range across `at` by more than
/// `spread`.
fn crease(
    overlap: &[Point2<f64>],
    gaps: &[f64],
    (at, spread): (f64, f64),
) -> Option<[Point2<f64>; 2]> {
    let low = gaps.iter().copied().fold(f64::INFINITY, f64::min);
    let high = gaps.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !(low < at && at < high) || high - low <= spread {
        return None;
    }
    let mut level = Vec::new();
    for k in 0..overlap.len() {
        let next = (k + 1) % overlap.len();
        let (p, q) = (overlap[k], overlap[next]);
        let (gp, gq) = (gaps[k] - at, gaps[next] - at);
        if (gp < 0.0) != (gq < 0.0) {
            let t = gp / (gp - gq);
            level.push(Point2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t));
        }
    }
    let ends = level
        .iter()
        .flat_map(|&p| level.iter().map(move |&q| [p, q]));
    ends.max_by(|a, b| a[0].distance_2(a[1]).total_cmp(&b[0].distance_2(b[1])))
}

/// The part of `polygon` inside `plane`'s triangle, both counter-clockwise.
fn clip(polygon: &[Point2<f64>], plane: &Plane) -> Vec<Point2<f64>> {
    let mut out = polygon.to_vec();
    for k in 0..3 {
        let (a, b) = (plane.corners[k], plane.corners[(k + 1) % 3]);
        let input = std::mem::take(&mut out);
        for (i, &p) in input.iter().enumerate() {
            let q = input[(i + 1) % input.len()];
            let (sp, sq) = (cross(a, b, p), cross(a, b, q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                out.push(Point2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use xodr::LaneType::{Driving, Shoulder, Sidewalk};

    /// A straight strip `width` wide from `a` to `b`, as two facets facing up.
    fn strip(a: Point, b: Point, width: f32, kind: LaneType) -> Vec<Facet> {
        let along = (b - a).normalize_or(Vector::X);
        let side = Vector::new(-along.y, along.x, 0.0) * (width / 2.0);
        let [al, ar, bl, br] = [a + side, a - side, b + side, b - side];
        let up = [Vector::Z; 3];
        vec![
            Facet {
                corners: [al, ar, br],
                normals: up,
                kind,
                fill: false,
            },
            Facet {
                corners: [al, br, bl],
                normals: up,
                kind,
                fill: false,
            },
        ]
    }

    fn p(x: f32, y: f32, z: f32) -> Point {
        Point::new(x, y, z)
    }

    /// The weights of `q` in `t` in plan, if it is inside by more than a hair.
    fn inside(t: [Point; 3], q: (f64, f64)) -> Option<[f64; 3]> {
        let [a, b, c] = t.map(|v| (f64::from(v.x), f64::from(v.y)));
        let cross = |o: (f64, f64), u: (f64, f64), v: (f64, f64)| {
            (u.0 - o.0) * (v.1 - o.1) - (u.1 - o.1) * (v.0 - o.0)
        };
        let area = cross(a, b, c);
        let w = [
            cross(q, b, c) / area,
            cross(a, q, c) / area,
            cross(a, b, q) / area,
        ];
        w.iter().all(|&x| x > 1e-9).then_some(w)
    }

    fn height_in(t: [Point; 3], w: [f64; 3]) -> f64 {
        (0..3).map(|i| w[i] * f64::from(t[i].z)).sum()
    }

    fn corners(wrap: &Wrap, face: &Face) -> [Point; 3] {
        face.corners.map(|k| wrap.vertices[k as usize])
    }

    /// Whether `q` is within 5 mm of an edge of `t`, where rounding can put
    /// it either side.
    fn near_edge(t: [Point; 3], q: (f64, f64)) -> bool {
        (0..3).any(|k| {
            let (a, b) = (plan(t[k]), plan(t[(k + 1) % 3]));
            distance_2(Point2::new(q.0, q.1), a, b) < 25e-6
        })
    }

    /// What the wrap over `q` should be, from the rule in [`Ground::owners`]
    /// worked out afresh: for each level, highest first, the owner's height
    /// and lane type. The type is `None` where two lanes of different types
    /// are close enough in height that either may own the ground. `None`
    /// overall within 2 mm of a line where the levels or the owner change.
    fn expected(facets: &[Facet], q: (f64, f64)) -> Option<Vec<(f64, Option<LaneType>)>> {
        let mut over: Vec<(f64, LaneType, bool)> = facets
            .iter()
            .filter_map(|f| Some((height_in(f.corners, inside(f.corners, q)?), f.kind, f.fill)))
            .collect();
        over.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (i, a) in over.iter().enumerate() {
            for b in &over[i + 1..] {
                let gap = a.0 - b.0;
                let tie = !a.2 && !b.2 && carries_traffic(a.1) != carries_traffic(b.1);
                if (gap - CLEARANCE).abs() < 2e-3 || (tie && (gap - TOLERANCE).abs() < 2e-3) {
                    return None;
                }
            }
        }
        let levels = over.chunk_by(|a, b| a.0 - b.0 <= CLEARANCE).map(|level| {
            let lanes = level.iter().any(|l| !l.2);
            let level: Vec<_> = level.iter().filter(|l| !(lanes && l.2)).collect();
            let top = level[0].0;
            let candidates: Vec<_> = level.iter().filter(|l| l.0 >= top - TOLERANCE).collect();
            let traffic = candidates.iter().find(|l| carries_traffic(l.1));
            let &&&(z, kind, _) = traffic.unwrap_or(&candidates[0]);
            let rival = candidates.iter().any(|l| {
                l.1 != kind
                    && carries_traffic(l.1) == carries_traffic(kind)
                    && (l.0 - z).abs() <= 2.0 * FLAT
            });
            (z, (!rival).then_some(kind))
        });
        Some(levels.collect())
    }

    /// Checks `wrap` against [`expected`] at points `step` apart across
    /// `facets`, skipping points within 5 mm of a facet's edge. Returns how
    /// many points it checked.
    fn check(facets: &[Facet], wrap: &Wrap, step: f64) -> usize {
        check_raised(facets, wrap, step, &|_, _| 0.0)
    }

    /// [`check`], with `raise` added to the height [`expected`] gives.
    fn check_raised(
        facets: &[Facet],
        wrap: &Wrap,
        step: f64,
        raise: &dyn Fn(f64, f64) -> f64,
    ) -> usize {
        let bucket = |x: f64, y: f64| ((x / 2.0).floor() as i64, (y / 2.0).floor() as i64);
        let index = |triangles: &mut dyn Iterator<Item = [Point; 3]>| {
            let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
            for (k, t) in triangles.enumerate() {
                let (xs, ys) = (t.map(|v| f64::from(v.x)), t.map(|v| f64::from(v.y)));
                let low = bucket(
                    xs.into_iter().fold(f64::INFINITY, f64::min),
                    ys.into_iter().fold(f64::INFINITY, f64::min),
                );
                let high = bucket(
                    xs.into_iter().fold(f64::NEG_INFINITY, f64::max),
                    ys.into_iter().fold(f64::NEG_INFINITY, f64::max),
                );
                for i in low.0..=high.0 {
                    for j in low.1..=high.1 {
                        cells.entry((i, j)).or_default().push(k);
                    }
                }
            }
            cells
        };
        let faces = index(&mut wrap.faces.iter().map(|f| corners(wrap, f)));
        let lanes = index(&mut facets.iter().map(|f| f.corners));
        let all: Vec<Point> = facets.iter().flat_map(|f| f.corners).collect();
        let low = all
            .iter()
            .fold((f64::INFINITY, f64::INFINITY), |(x, y), v| {
                (x.min(f64::from(v.x)), y.min(f64::from(v.y)))
            });
        let high = all
            .iter()
            .fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |(x, y), v| {
                (x.max(f64::from(v.x)), y.max(f64::from(v.y)))
            });
        let none = Vec::new();
        let mut checked = 0;
        let mut y = low.1 - 1.0 + step * 0.371;
        while y < high.1 + 1.0 {
            let mut x = low.0 - 1.0 + step * 0.613;
            while x < high.0 + 1.0 {
                let cell = bucket(x, y);
                let here: Vec<Facet> = lanes
                    .get(&cell)
                    .unwrap_or(&none)
                    .iter()
                    .map(|&k| copy(&facets[k]))
                    .collect();
                let want = (!here.iter().any(|f| near_edge(f.corners, (x, y))))
                    .then(|| expected(&here, (x, y)))
                    .flatten();
                if let Some(want) = want {
                    let mut got: Vec<(f64, LaneType)> = faces
                        .get(&cell)
                        .unwrap_or(&none)
                        .iter()
                        .filter_map(|&k| {
                            let t = corners(wrap, &wrap.faces[k]);
                            Some((
                                height_in(t, inside(t, (x, y))?),
                                facets[wrap.faces[k].facet].kind,
                            ))
                        })
                        .collect();
                    got.sort_by(|a, b| b.0.total_cmp(&a.0));
                    assert_eq!(
                        got.len(),
                        want.len(),
                        "({x}, {y}) has {got:?}, not {want:?}"
                    );
                    for ((z, kind), (want_z, want_kind)) in got.into_iter().zip(want) {
                        let want_z = want_z + raise(x, y);
                        assert!(
                            (z - want_z).abs() <= TOLERANCE + 1e-3,
                            "({x}, {y}) is at {z}, its owner at {want_z}"
                        );
                        assert!(
                            want_kind.is_none_or(|k| k == kind),
                            "({x}, {y}) is {kind:?}, not {want_kind:?}"
                        );
                    }
                    checked += 1;
                }
                x += step;
            }
            y += step;
        }
        checked
    }

    fn copy(f: &Facet) -> Facet {
        Facet {
            corners: f.corners,
            normals: f.normals,
            kind: f.kind,
            fill: f.fill,
        }
    }

    fn area(wrap: &Wrap, faces: impl IntoIterator<Item = usize>) -> f64 {
        faces
            .into_iter()
            .map(|k| {
                let [a, b, c] = corners(wrap, &wrap.faces[k]);
                f64::from((b - a).cross(c - a).z) / 2.0
            })
            .sum()
    }

    /// The faces over `(x, y)`, as their heights and lane types, highest first.
    fn over(wrap: &Wrap, facets: &[Facet], x: f64, y: f64) -> Vec<(f64, LaneType)> {
        let mut hits: Vec<(f64, LaneType)> = wrap
            .faces
            .iter()
            .filter_map(|f| {
                let t = corners(wrap, f);
                Some((height_in(t, inside(t, (x, y))?), facets[f.facet].kind))
            })
            .collect();
        hits.sort_by(|a, b| b.0.total_cmp(&a.0));
        hits
    }

    #[test]
    fn crossing_level_strips_cover_their_union_once() {
        let mut facets = strip(p(-10.0, 0.0, 1.0), p(10.0, 0.0, 1.0), 4.0, Driving);
        facets.extend(strip(p(0.0, -10.0, 1.0), p(0.0, 10.0, 1.0), 4.0, Driving));
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.25) > 1000);
        let union = 2.0 * 20.0 * 4.0 - 4.0 * 4.0;
        assert!((area(&wrap, 0..wrap.faces.len()) - union).abs() < 1e-3);
        assert_eq!(wrap.vertices.len(), 12, "the plus is laid from its outline");
        assert_eq!(wrap.faces.len(), 10);
    }

    #[test]
    fn a_crossing_hidden_from_split_still_becomes_an_edge() {
        let mut cdt = Cdt::new();
        let mut welder = Welder::new(10.0);
        let [a, b, c, d] = [(0.0, 0.0), (4.0, 4.0), (0.0, 4.0), (4.0, 0.0)]
            .map(|(x, y)| welder.insert(&mut cdt, Point2::new(x, y)));
        constrain(&mut cdt, &mut welder, c, d, RESPLITS);
        constrain(&mut cdt, &mut welder, a, b, RESPLITS);
        let middle = welder.insert(&mut cdt, Point2::new(2.0, 2.0));
        for end in [a, b, c, d] {
            assert!(cdt.exists_constraint(end, middle));
        }
        assert_eq!(cdt.num_constraints(), 4);
    }

    #[test]
    fn far_from_the_origin_corners_weld_across_an_f32_step() {
        let welded = |reach: f64, gap: f64| {
            let mut cdt = Cdt::new();
            let mut welder = Welder::new(reach);
            let a = welder.insert(&mut cdt, Point2::new(reach, 0.0));
            a == welder.insert(&mut cdt, Point2::new(reach + gap, 0.0))
        };
        assert!(welded(20_000.0, 0.0015));
        assert!(!welded(10.0, 0.0015));
    }

    #[test]
    fn a_ramp_through_a_level_strip_creases_where_they_cross() {
        let mut facets = strip(p(-10.0, 0.0, 0.0), p(10.0, 0.0, 0.0), 4.0, Driving);
        facets.extend(strip(p(0.0, -10.0, -1.0), p(0.0, 10.0, 1.0), 4.0, Driving));
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.1) > 1000);
        for k in 0..13 {
            let x = -1.8 + 0.3 * f64::from(k) + 0.0137;
            for y in [-0.3, -0.05, 0.05, 0.3] {
                let want = f64::max(0.0, y / 10.0);
                let got = over(&wrap, &facets, x, y)[0].0;
                assert!(
                    (got - want).abs() < 1e-4,
                    "({x}, {y}) is at {got}, not {want}"
                );
            }
        }
    }

    #[test]
    fn a_lane_rising_a_hair_over_another_creases_where_it_crosses() {
        let level = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving);
        let mut facets = strip(p(0.0, 0.0, -0.008), p(20.0, 0.0, 0.2), 4.0, Driving);
        facets.extend(level);
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.1) > 1000);
        for x in [0.2, 0.5, 1.0, 2.0, 5.0, 19.0] {
            let want = f64::max(0.0, -0.008 + 0.208 * x / 20.0);
            let got = over(&wrap, &facets, x + 0.0137, 0.3)[0].0;
            assert!(
                (got - want).abs() < 2e-3,
                "({x}, 0.3) is at {got}, not {want}"
            );
        }
    }

    #[test]
    fn a_banked_strip_over_a_level_one_creases_along_its_middle() {
        let level = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving);
        let banked: Vec<Facet> = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving)
            .into_iter()
            .map(|f| Facet {
                corners: f.corners.map(|c| p(c.x, c.y, c.y * 0.05)),
                ..f
            })
            .collect();
        let facets: Vec<Facet> = level.into_iter().chain(banked).collect();
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.1) > 1000);
        for k in 0..10 {
            let x = 1.0 + 2.0 * f64::from(k) + 0.0137;
            for y in [-1.5, -0.1, 0.1, 1.5] {
                let want = f64::max(0.0, 0.05 * y);
                let got = over(&wrap, &facets, x, y)[0].0;
                assert!(
                    (got - want).abs() < 1e-4,
                    "({x}, {y}) is at {got}, not {want}"
                );
            }
        }
    }

    #[test]
    fn a_ring_of_strips_leaves_its_middle_open() {
        let square = [
            p(0.0, 0.0, 0.0),
            p(20.0, 0.0, 0.0),
            p(20.0, 20.0, 0.0),
            p(0.0, 20.0, 0.0),
        ];
        let facets: Vec<Facet> = (0..4)
            .flat_map(|k| strip(square[k], square[(k + 1) % 4], 4.0, Driving))
            .collect();
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.25) > 1000);
        assert!(over(&wrap, &facets, 10.0, 10.0).is_empty());
    }

    #[test]
    fn a_ramp_that_clears_a_road_becomes_a_level_of_its_own() {
        let mut facets = strip(p(-20.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving);
        facets.extend(strip(p(-20.0, 0.0, 0.0), p(20.0, 0.0, 8.0), 4.0, Driving));
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.25) > 1000);
        assert_eq!(over(&wrap, &facets, -15.0, 0.3).len(), 1);
        let both = over(&wrap, &facets, 15.0, 0.3);
        assert_eq!(both.len(), 2);
        assert!(
            (both[0].0 - 7.0).abs() < 1e-3 && both[1].0.abs() < 1e-3,
            "{both:?}"
        );
    }

    #[test]
    fn traffic_owns_ground_it_shares_level_with_a_sidewalk() {
        let mut facets = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving);
        facets.extend(strip(
            p(10.0, -10.0, 0.004),
            p(10.0, 10.0, 0.004),
            2.0,
            Sidewalk,
        ));
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.25) > 500);
        assert_eq!(over(&wrap, &facets, 10.3, 0.7)[0].1, Driving);
        assert_eq!(over(&wrap, &facets, 10.3, 5.0)[0].1, Sidewalk);
    }

    #[test]
    fn a_sidewalk_rising_off_a_road_owns_the_ground_once_it_clears_it() {
        let mut facets = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving);
        facets.extend(strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.05), 2.0, Sidewalk));
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.1) > 1000);
        for x in [1.0, 2.0, 3.5] {
            assert_eq!(
                over(&wrap, &facets, x + 0.0137, 0.3)[0].1,
                Driving,
                "at x = {x}"
            );
        }
        for x in [4.5, 10.0, 19.0] {
            assert_eq!(
                over(&wrap, &facets, x + 0.0137, 0.3)[0].1,
                Sidewalk,
                "at x = {x}"
            );
        }
    }

    #[test]
    fn fill_owns_only_ground_no_lane_covers() {
        let mut facets = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving);
        facets.extend(
            strip(p(10.0, -10.0, 0.5), p(10.0, 10.0, 0.5), 8.0, Driving)
                .into_iter()
                .map(|f| Facet { fill: true, ..f }),
        );
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.25) > 1000);
        let on = |x, y| {
            let hit = over(&wrap, &facets, x, y);
            assert_eq!(hit.len(), 1, "({x}, {y})");
            hit[0].0
        };
        assert!(on(10.3, 0.7).abs() < 1e-4, "the lane owns its ground");
        assert!((on(10.3, 5.0) - 0.5).abs() < 1e-4, "the fill owns the rest");
    }

    #[test]
    fn fill_keeps_the_ground_under_a_bridge() {
        let mut facets = strip(p(-10.0, 0.0, 5.0), p(10.0, 0.0, 5.0), 4.0, Driving);
        facets.extend(
            strip(p(0.0, -10.0, 0.0), p(0.0, 10.0, 0.0), 8.0, Driving)
                .into_iter()
                .map(|f| Facet { fill: true, ..f }),
        );
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.25) > 1000);
        let both = over(&wrap, &facets, 0.3, 0.7);
        assert_eq!(both.len(), 2, "{both:?}");
        assert!(
            (both[0].0 - 5.0).abs() < 1e-4 && both[1].0.abs() < 1e-4,
            "{both:?}"
        );
    }

    #[test]
    fn a_sidewalk_above_traffic_owns_the_ground() {
        let mut facets = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving);
        facets.extend(strip(
            p(10.0, -10.0, 0.15),
            p(10.0, 10.0, 0.15),
            2.0,
            Sidewalk,
        ));
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.25) > 500);
        assert_eq!(over(&wrap, &facets, 10.3, 0.7)[0].1, Sidewalk);
    }

    #[test]
    fn corners_a_hair_apart_are_one_vertex() {
        let mut facets = strip(p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0), 4.0, Driving);
        facets.extend(strip(p(10.0004, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving));
        let wrap = wrap(&facets);
        for (i, a) in wrap.vertices.iter().enumerate() {
            for b in &wrap.vertices[i + 1..] {
                assert!(a.distance_to(*b) > WELD as f32, "{a:?} and {b:?}");
            }
        }
        let smallest = (0..wrap.faces.len())
            .map(|k| area(&wrap, [k]))
            .fold(f64::INFINITY, f64::min);
        assert!(smallest > 1.0, "a sliver of {smallest} m²");
    }

    #[test]
    fn lanes_too_close_to_crease_meet_without_a_seam() {
        let level = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving);
        let tilted = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving)
            .into_iter()
            .map(|f| Facet {
                corners: f.corners.map(|c| p(c.x, c.y, c.y * 0.00075)),
                ..f
            });
        let facets: Vec<Facet> = level.into_iter().chain(tilted).collect();
        let wrap = wrap(&facets);
        assert!(check(&facets, &wrap, 0.1) > 1000);
        let mut uses: HashMap<(u32, u32), usize> = HashMap::new();
        for face in &wrap.faces {
            for k in 0..3 {
                let (a, b) = (face.corners[k], face.corners[(k + 1) % 3]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let rim = |v: Point| {
            (v.y.abs() - 2.0).abs() < 1e-4 || v.x.abs() < 1e-4 || (v.x - 20.0).abs() < 1e-4
        };
        for (&(a, b), &n) in &uses {
            let (a, b) = (wrap.vertices[a as usize], wrap.vertices[b as usize]);
            assert!(n == 2 || rim(a.lerp(b, 0.5)), "a seam from {a:?} to {b:?}");
        }
    }

    #[test]
    fn a_wrap_comes_out_the_same_every_time() {
        let (net, provenance, mesh) = fixture();
        let lanes = crate::junction_lanes(&net, &provenance, &mesh)
            .remove("1")
            .expect("junction 1");
        let [a, b] = [wrap(&lanes.facets), wrap(&lanes.facets)];
        assert_eq!(a.vertices, b.vertices);
        let faces = |w: &Wrap| {
            w.faces
                .iter()
                .map(|f| (f.corners, f.facet))
                .collect::<Vec<_>>()
        };
        assert_eq!(faces(&a), faces(&b));
    }

    #[test]
    fn a_wrap_takes_its_normals_from_its_lanes() {
        let tilted = Vector::new(0.0, -0.1, 1.0).normalize_or(Vector::Z);
        let facets: Vec<Facet> = strip(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), 4.0, Driving)
            .into_iter()
            .map(|f| Facet {
                normals: [tilted; 3],
                ..f
            })
            .collect();
        let wrap = wrap(&facets);
        assert!(!wrap.normals.is_empty());
        for n in &wrap.normals {
            assert!(n.abs_diff_eq(tilted, 1e-5), "{n:?}");
        }
    }

    /// `tests/data/junction_wraps.xodr`, what the load made of it, and its
    /// surface.
    fn fixture() -> (xodr::RoadNetwork, xodr::Provenance, xodr::Mesh) {
        let (net, provenance) =
            xodr::load_file_with_provenance("../tests/data/junction_wraps.xodr")
                .expect("map loads");
        let mesh = net.surface_mesh();
        (net, provenance, mesh)
    }

    #[test]
    fn every_fixture_junction_is_covered_once_per_level_by_its_owner() {
        let (net, provenance, mesh) = fixture();
        let junctions = crate::junction_lanes(&net, &provenance, &mesh);
        assert_eq!(
            junctions.len(),
            13,
            "the direct junction has no lanes of its own"
        );
        for (id, lanes) in &junctions {
            let checked = check_all(id, &lanes.facets, &wrap(&lanes.facets));
            assert!(checked > 50, "junction {id}: {checked} points");
        }
    }

    /// Checks a junction's wrap: [`check`], every face up, and no two
    /// vertices within half of [`WELD`] of each other, which welding should
    /// have merged. Returns how many points [`check`] checked.
    pub(super) fn check_all(id: &str, facets: &[Facet], wrap: &Wrap) -> usize {
        let checked = check(facets, wrap, 0.3);
        for face in &wrap.faces {
            let [a, b, c] = corners(wrap, face);
            let up = (b - a).cross(c - a).z > MIN_AREA;
            assert!(up, "junction {id} has a face down or flat");
        }
        let mut cells: HashMap<(i64, i64), Vec<Point>> = HashMap::new();
        for &v in &wrap.vertices {
            let c = cell(plan(v), WELD);
            let near = (-1..=1).flat_map(|i| (-1..=1).map(move |j| (c.0 + i, c.1 + j)));
            let twin = near.filter_map(|c| cells.get(&c)).flatten().find(|w| {
                plan(**w).distance_2(plan(v)) <= WELD * WELD / 4.0
                    && f64::from((w.z - v.z).abs()) <= WELD
            });
            assert!(twin.is_none(), "junction {id} has {v:?} and {twin:?} apart");
            cells.entry(c).or_default().push(v);
        }
        checked
    }

    #[test]
    fn a_fixture_wrap_keeps_every_vertex_where_it_meets_a_road() {
        let (net, provenance, mesh) = fixture();
        let outside = mesh.lanes.iter().filter(|span| {
            let road = net.road_lane(span.lane).and_then(|at| net.road(at.road));
            !span.indices.is_empty() && road.is_some_and(|r| r.junction().is_none())
        });
        let ends: Vec<Point> = outside
            .flat_map(|s| {
                [
                    s.vertices.start,
                    s.vertices.start + 1,
                    s.vertices.end - 2,
                    s.vertices.end - 1,
                ]
            })
            .map(|v| mesh.vertices[v as usize])
            .collect();
        let close = |a: Point, b: Point| {
            let (dx, dy) = (f64::from(a.x - b.x), f64::from(a.y - b.y));
            dx * dx + dy * dy <= 4.0 * WELD * WELD
        };
        let mut seams = 0;
        for (id, lanes) in crate::junction_lanes(&net, &provenance, &mesh) {
            let wrap = wrap(&lanes.facets);
            let corners: Vec<Point> = lanes.facets.iter().flat_map(|f| f.corners).collect();
            let covered = |x: f64, y: f64| {
                lanes
                    .facets
                    .iter()
                    .any(|f| inside(f.corners, (x, y)).is_some())
            };
            let surrounded = |e: Point| {
                (0..8).all(|k| {
                    let a = f64::from(k) * std::f64::consts::FRAC_PI_4;
                    covered(
                        f64::from(e.x) + 0.01 * a.cos(),
                        f64::from(e.y) + 0.01 * a.sin(),
                    )
                })
            };
            for &end in ends
                .iter()
                .filter(|&&e| corners.iter().any(|&c| close(c, e)))
            {
                if surrounded(end) {
                    continue;
                }
                let heights: Vec<f32> = corners
                    .iter()
                    .filter(|&&c| close(c, end))
                    .map(|c| c.z)
                    .collect();
                let kept = wrap.vertices.iter().any(|&w| {
                    close(w, end)
                        && heights
                            .iter()
                            .any(|&z| f64::from((w.z - z).abs()) <= TOLERANCE)
                });
                assert!(kept, "junction {id} lost the road end {end:?}");
                seams += 1;
            }
        }
        assert!(seams > 100, "{seams} road ends");
    }

    fn junction(id: &str) -> (Vec<Facet>, Wrap) {
        let (net, provenance, mesh) = fixture();
        let lanes = crate::junction_lanes(&net, &provenance, &mesh)
            .remove(id)
            .expect("the junction");
        let wrap = wrap(&lanes.facets);
        (lanes.facets, wrap)
    }

    #[test]
    fn the_crossroads_keeps_its_corner_sidewalks() {
        let (facets, wrap) = junction("1");
        let sidewalk = |k: usize| facets[k].kind == Sidewalk;
        let laid: f64 = (0..facets.len())
            .filter(|&k| sidewalk(k))
            .map(|k| {
                let [a, b, c] = facets[k].corners;
                f64::from((b - a).cross(c - a).z).abs() / 2.0
            })
            .sum();
        let kept = area(
            &wrap,
            (0..wrap.faces.len()).filter(|&k| sidewalk(wrap.faces[k].facet)),
        );
        assert!(laid > 10.0);
        assert!(
            (kept - laid).abs() / laid < 0.01,
            "{kept} m² of {laid} m² of sidewalk"
        );
    }

    #[test]
    fn the_flyover_keeps_the_road_under_it() {
        let (facets, wrap) = junction("10");
        let both = over(&wrap, &facets, 3600.37, 0.41);
        assert_eq!(both.len(), 2, "{both:?}");
        assert!(both[0].0 > 4.5 && both[1].0.abs() < 1e-3, "{both:?}");
    }

    #[test]
    fn the_kerbs_keep_the_raised_sidewalk_and_the_shoulder_under_a_level_one() {
        let (facets, wrap) = junction("11");
        let shoulder = over(&wrap, &facets, 4005.37, -3.5);
        assert_eq!(shoulder.len(), 1);
        assert_eq!(shoulder[0].1, Shoulder);
        let kerb = over(&wrap, &facets, 4005.37, -4.5);
        assert_eq!(kerb.len(), 1);
        assert_eq!(kerb[0].1, Sidewalk);
        assert!((kerb[0].0 - 0.15).abs() < 1e-3, "{kerb:?}");
    }

    #[test]
    fn the_boundary_lays_the_lanes_on_the_grid() {
        let (net, provenance, mesh) = fixture();
        let area = (net.junction_areas().iter())
            .find(|a| a.od_id == "14")
            .expect("junction 14's area");
        let lanes = crate::junction_lanes(&net, &provenance, &mesh)
            .remove("14")
            .expect("junction 14");
        let wrap = crate::junction_wrap(&lanes);
        let grid = |x: f64, y: f64| area.height_at(x, y).expect("on the grid");
        assert!(check_raised(&lanes.facets, &wrap, 0.3, &grid) > 1000);
        let (x, y) = (5200.37, 0.41);
        let middle = over(&wrap, &lanes.facets, x, y);
        assert!((middle[0].0 - grid(x, y)).abs() <= TOLERANCE, "{middle:?}");
        assert!(grid(x, y) > 0.25, "the hump is under the lanes");
        let raise = |p: Point| f64::from(p.z) - grid(f64::from(p.x), f64::from(p.y));
        let mut kerbs = Vec::new();
        let mut gaps = 0;
        for face in &wrap.faces {
            let facet = &lanes.facets[face.facet];
            if facet.kind == Sidewalk {
                kerbs.extend(corners(&wrap, face));
            }
            if facet.fill {
                for p in corners(&wrap, face) {
                    assert!(raise(p).abs() <= TOLERANCE, "fill at {p:?}");
                }
                gaps += 1;
            }
        }
        assert!(gaps > 0, "no fill");
        for &p in &kerbs {
            let r = raise(p);
            assert!(
                (0.1 - TOLERANCE..=0.2 + TOLERANCE).contains(&r),
                "a kerb at {p:?}"
            );
        }
        let out = |p: &&Point| (p.x - 5200.0).hypot(p.y);
        let inner = kerbs
            .iter()
            .min_by(|a, b| out(a).total_cmp(&out(b)))
            .expect("kerbs");
        let outer = kerbs
            .iter()
            .max_by(|a, b| out(a).total_cmp(&out(b)))
            .expect("kerbs");
        assert!(
            (raise(*inner) - 0.1).abs() <= TOLERANCE,
            "the inner edge at {inner:?}"
        );
        assert!(
            (raise(*outer) - 0.2).abs() <= TOLERANCE,
            "the outer edge at {outer:?}"
        );
    }

    #[test]
    fn a_boundary_missing_a_segment_lays_out_as_none() {
        let (net, provenance) =
            xodr::load_file_with_provenance("../tests/data/junction_areas.xodr")
                .expect("map loads");
        assert!(crate::boundary(&net, &provenance, "7").is_some());
        assert!(crate::boundary(&net, &provenance, "8").is_none());
    }
}
