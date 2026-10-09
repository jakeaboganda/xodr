//! Road surfaces from OpenCRG files. [`CrgSurface`] is a `<CRG>` record from
//! the map, and [`RoadSurface`] evaluates the files under a point.

use std::cmp::Reverse;
use std::collections::HashMap;

use opencrg::{CrgGrid, SearchHint, Uv, Xy};

use crate::{LaneId, Mesh, MeshSampler, RoadNetwork};

/// What the values in a CRG file are, from `<CRG purpose>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CrgPurpose {
    /// Heights, in metres. Also what a missing `purpose` reads as.
    Elevation,
    /// Friction coefficients.
    Friction,
}

/// How a CRG file's `(u, v)` follow the road's `(s, t)` in the `attached`
/// and `attached0` modes: `u = s - s_offset` and `v = t - t_offset`, both
/// negated if `opposite`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrgAlong {
    /// `sOffset`, in metres.
    pub s_offset: f64,
    /// `tOffset`, in metres.
    pub t_offset: f64,
    /// `orientation="opposite"`: the file runs against `s`.
    pub opposite: bool,
}

/// A position and heading in the map's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrgPose {
    /// X, in metres.
    pub x: f64,
    /// Y, in metres.
    pub y: f64,
    /// Heading, in radians counter-clockwise from +X.
    pub heading: f64,
}

/// How a CRG file lies on the map, from `<CRG mode>`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CrgMode {
    /// `attached`: the file's grid, laid along the road's reference line, is
    /// added to the road's own height. The file's reference line, with its
    /// height, slope and bank, plays no part.
    Attached(CrgAlong),
    /// `attached0`: laid along the road like `Attached`, but the file's
    /// elevation replaces the road's height.
    Attached0(CrgAlong),
    /// `genuine`: the file's own reference line, started at `start`, the
    /// road's `(sOffset, tOffset)` turned by `hOffset`. Its elevation replaces
    /// the road's height.
    Genuine {
        /// Where the file's reference line starts.
        start: CrgPose,
    },
    /// `global`: the file in its own coordinates, moved to `origin` by
    /// `xOffset`, `yOffset` and `hOffset`. Its elevation replaces the road's
    /// height.
    Global {
        /// Where the file's origin lands.
        origin: CrgPose,
    },
}

/// One `<CRG>` under a `<road>` or `<junction>` `<surface>`.
///
/// The network keeps the record. [`RoadSurface::new`] loads the file.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrgSurface {
    /// The `file` attribute, as written.
    pub file: String,
    /// What the file's values are.
    pub purpose: CrgPurpose,
    /// How the file lies on the map.
    pub mode: CrgMode,
    /// `zOffset`, in metres. 0 for friction.
    pub z_offset: f64,
    /// `zScale`. 1 for friction.
    pub z_scale: f64,
    /// The lanes it applies to: those of the road's lane sections between
    /// `sStart` and `sEnd`, or of every road in a junction.
    pub lanes: Vec<LaneId>,
    /// The road's reference line from `sStart` to `sEnd`. `None` on a
    /// junction.
    pub(crate) road: Option<Stretch>,
}

/// A stretch of a road's reference line, sampled every 0.25 m. Between two
/// stations the line is the arc that turns from one heading to the other.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Stretch {
    pub stations: Vec<RefPoint>,
}

/// The reference line at one station.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct RefPoint {
    pub s: f64,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    /// Elevation, and its rate along `s`.
    pub z: f64,
    pub grade: f64,
    /// Superelevation, and its rate along `s`.
    pub bank: f64,
    pub bank_rate: f64,
}

/// Where a point is on a [`Stretch`], and the reference line there.
#[derive(Debug, Clone, Copy)]
struct OnRoad {
    s: f64,
    /// Along the banked cross-section, as OpenDRIVE measures it.
    t: f64,
    /// The road's height at `(s, t)`.
    z: f64,
    grade: f64,
    bank: f64,
    bank_rate: f64,
    /// `s` and `t` as a function of world `(x, y)`.
    ds: [f64; 2],
    dt: [f64; 2],
}

/// A previous position further than this starts a search from scratch, as in
/// OpenCRG's C-API.
const FAR: f64 = 2.2;

impl Stretch {
    /// Where `(x, y)` is along the stretch. `None` before its start or past
    /// its end.
    fn locate(&self, x: f64, y: f64, hint: &mut PatchHint) -> Option<OnRoad> {
        let st = &self.stations;
        let last = st.len().checked_sub(2)?;
        let near = hint
            .last
            .is_some_and(|(px, py)| (px - x).powi(2) + (py - y).powi(2) < FAR * FAR);
        let mut i = if near {
            hint.station.min(last)
        } else {
            (0..st.len())
                .min_by(|&a, &b| {
                    let d = |k: usize| (st[k].x - x).powi(2) + (st[k].y - y).powi(2);
                    d(a).total_cmp(&d(b))
                })?
                .min(last)
        };
        let mut came_from = None;
        let (f, d) = loop {
            let (f, d) = foot(&st[i], &st[i + 1], x, y);
            let next = if f < 0.0 && i > 0 {
                i - 1
            } else if f > 1.0 && i < last {
                i + 1
            } else {
                break (f, d);
            };
            if came_from == Some(next) {
                break (f.clamp(0.0, 1.0), d);
            }
            came_from = Some(i);
            i = next;
        };
        hint.last = Some((x, y));
        hint.station = i;
        if !(0.0..=1.0).contains(&f) {
            return None;
        }

        let (a, b) = (&st[i], &st[i + 1]);
        let lerp = |p: f64, q: f64| p + (q - p) * f;
        let heading = a.heading + wrap(b.heading - a.heading) * f;
        let along = [heading.cos(), heading.sin()];
        let left = [-along[1], along[0]];
        let bank = lerp(a.bank, b.bank);
        let t = d / bank.cos();
        let curvature = wrap(b.heading - a.heading) / (b.s - a.s);
        let stretch = 1.0 - curvature * d;
        Some(OnRoad {
            s: lerp(a.s, b.s),
            t,
            z: lerp(a.z, b.z) + t * bank.sin(),
            grade: lerp(a.grade, b.grade),
            bank,
            bank_rate: lerp(a.bank_rate, b.bank_rate),
            ds: along.map(|c| c / stretch),
            dt: left.map(|c| c / bank.cos()),
        })
    }
}

/// Where the normal through `(x, y)` meets the arc from `a` to `b`: how far
/// along, 0 at `a` and 1 at `b`, and how far `(x, y)` is to its left.
fn foot(a: &RefPoint, b: &RefPoint, x: f64, y: f64) -> (f64, f64) {
    let length = b.s - a.s;
    let turn = wrap(b.heading - a.heading);
    let (sin, cos) = a.heading.sin_cos();
    let (wx, wy) = (x - a.x, y - a.y);
    if turn.abs() < 1e-12 {
        return ((wx * cos + wy * sin) / length, wy * cos - wx * sin);
    }
    let radius = length / turn;
    let (cx, cy) = (-radius * sin, radius * cos);
    let (vx, vy) = (cx - wx, cy - wy);
    let rho = vx.hypot(vy).copysign(radius);
    let heading = (-vx / rho).atan2(vy / rho);
    (wrap(heading - a.heading) / turn, radius - rho)
}

/// An angle wrapped into `[-pi, pi)`.
fn wrap(angle: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    angle - TAU * ((angle + PI) / TAU).floor()
}

impl CrgAlong {
    /// The file's `(u, v)` at road `(s, t)`, and the sign `u` and `v` change
    /// with `s` and `t`.
    fn uv(&self, s: f64, t: f64) -> (Uv, f64) {
        let sign = if self.opposite { -1.0 } else { 1.0 };
        let uv = Uv {
            u: sign * (s - self.s_offset),
            v: sign * (t - self.t_offset),
        };
        (uv, sign)
    }
}

/// The road surface a vehicle drives on: the surface mesh, refined by the
/// map's OpenCRG files where they cover it.
///
/// Answers in `f64`, since a CRG resolves millimetres and an `f32` world
/// coordinate 100 km from the origin is only good to 8 mm.
///
/// Holds no state between queries, so one surface can serve many threads.
/// Each moving point keeps its own [`SurfaceHint`].
///
/// ```no_run
/// use xodr::{load_file, opencrg::CrgGrid, RoadSurface, SurfaceHint};
///
/// let net = load_file("maps/track.xodr")?;
/// let mesh = net.surface_mesh();
/// let surface = RoadSurface::new(&net, &mesh, |file| {
///     CrgGrid::from_path(format!("maps/{file}")).ok()
/// });
/// let mut wheel = SurfaceHint::default();
/// if let Some(ground) = surface.sample(12.0, -30.0, &mut wheel) {
///     println!("{} m, friction {:?}", ground.z, ground.friction);
/// }
/// # Ok::<(), xodr::ImportError>(())
/// ```
#[derive(Debug)]
pub struct RoadSurface<'a> {
    net: &'a RoadNetwork,
    mesh: MeshSampler<'a>,
    grids: Vec<CrgGrid>,
    /// Parallel to [`RoadNetwork::crg_surfaces`]. `None` where the file did
    /// not load.
    patches: Vec<Option<Patch>>,
    /// Indices into `patches` for each lane, junction surfaces first.
    by_lane: HashMap<LaneId, Vec<usize>>,
}

/// One CRG record whose file loaded.
#[derive(Debug)]
struct Patch {
    /// Index into `RoadSurface::grids`.
    grid: usize,
    /// For `genuine` and `global`: where a map `(x, y)` is in the file.
    rigid: Option<Rigid>,
}

/// Map `(x, y)` to a file's `(x, y)`: turned by `-angle` about `from`, then
/// moved to `to`.
#[derive(Debug, Clone, Copy)]
struct Rigid {
    from: [f64; 2],
    to: [f64; 2],
    angle: f64,
}

impl Rigid {
    fn apply(&self, x: f64, y: f64) -> Xy {
        let (sin, cos) = self.angle.sin_cos();
        let (dx, dy) = (x - self.from[0], y - self.from[1]);
        Xy {
            x: cos * dx + sin * dy + self.to[0],
            y: -sin * dx + cos * dy + self.to[1],
        }
    }

    /// A direction in the file's frame, back in the map's.
    fn back(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let (sin, cos) = self.angle.sin_cos();
        [cos * x - sin * y, sin * x + cos * y]
    }
}

/// Search state for one moving point, such as a wheel, passed to
/// [`RoadSurface::sample`]. Each query starts where the last one ended, so a
/// point that moves a few metres per step is found without scanning the whole
/// road. `SurfaceHint::default()` is empty.
#[derive(Debug, Clone, Default)]
pub struct SurfaceHint {
    patches: Vec<PatchHint>,
}

#[derive(Debug, Clone, Default)]
struct PatchHint {
    last: Option<(f64, f64)>,
    station: usize,
    crg: SearchHint,
}

/// The road surface under a point, from [`RoadSurface::sample`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceSample {
    /// Height, in metres.
    pub z: f64,
    /// Unit up-normal.
    pub normal: [f64; 3],
    /// The CRG grid's own height here, times `zScale`: the bumps, without
    /// the file's reference-line height or bank. `Some` where an elevation
    /// CRG sets `z`.
    pub crg_height: Option<f64>,
    /// The friction coefficient from a friction CRG, if one covers the
    /// point.
    pub friction: Option<f64>,
}

/// Where a finite difference steps, in metres.
const STEP: f64 = 1e-3;

impl<'a> RoadSurface<'a> {
    /// The surface of `net`, whose [`RoadNetwork::surface_mesh`] is `mesh`.
    ///
    /// `load` gets each distinct [`CrgSurface::file`] once, as written in the
    /// map, and returns the grid or `None`. A file it returns `None` for is
    /// left out, and the mesh answers where it would have. Border modes,
    /// smoothing and includes are for `load` to choose, as with any
    /// [`CrgGrid`].
    pub fn new(
        net: &'a RoadNetwork,
        mesh: &'a Mesh,
        mut load: impl FnMut(&str) -> Option<CrgGrid>,
    ) -> Self {
        let mut grids = Vec::new();
        let mut files: HashMap<&str, Option<usize>> = HashMap::new();
        let mut patches = Vec::with_capacity(net.crg_surfaces().len());
        let mut by_lane: HashMap<LaneId, Vec<usize>> = HashMap::new();
        for (i, record) in net.crg_surfaces().iter().enumerate() {
            let loaded = *files.entry(record.file.as_str()).or_insert_with(|| {
                let grid = load(&record.file)?;
                grids.push(grid);
                Some(grids.len() - 1)
            });
            let Some(index) = loaded else {
                patches.push(None);
                continue;
            };
            let grid: &CrgGrid = &grids[index];
            let rigid = match record.mode {
                CrgMode::Genuine { start } => {
                    let first = Uv {
                        u: grid.u_range().0,
                        v: 0.0,
                    };
                    let origin = grid.xy_from_uv(first);
                    Some(Rigid {
                        from: [start.x, start.y],
                        to: [origin.x, origin.y],
                        angle: start.heading - grid.heading_at_uv(first).phi,
                    })
                }
                CrgMode::Global { origin } => Some(Rigid {
                    from: [origin.x, origin.y],
                    to: [0.0, 0.0],
                    angle: origin.heading,
                }),
                CrgMode::Attached(_) | CrgMode::Attached0(_) => None,
            };
            patches.push(Some(Patch { grid: index, rigid }));
            for &lane in &record.lanes {
                by_lane.entry(lane).or_default().push(i);
            }
        }
        for list in by_lane.values_mut() {
            list.sort_by_key(|&i| Reverse(i));
        }
        Self {
            net,
            mesh: mesh.sampler(),
            grids,
            patches,
            by_lane,
        }
    }

    /// The surface under `(x, y)`: the last elevation CRG, in map order,
    /// that covers the point, or the surface mesh. Friction is the last
    /// friction CRG that covers it, or `None`. The spec has the last of
    /// the records that overlap for one purpose win. The map lists a
    /// junction's records after its roads', so they win. `None` off the
    /// road, where the mesh has no triangle.
    ///
    /// Where surfaces stack, as on a bridge, the lane is the one the mesh
    /// finds highest.
    pub fn sample(&self, x: f64, y: f64, hint: &mut SurfaceHint) -> Option<SurfaceSample> {
        let (height, normal, lane) = self.mesh.top(x as f32, y as f32)?;
        let records = self.net.crg_surfaces();
        if hint.patches.len() < records.len() {
            hint.patches.resize_with(records.len(), PatchHint::default);
        }
        let covering = lane.and_then(|l| self.by_lane.get(&l));
        let mut sample = SurfaceSample {
            z: f64::from(height),
            normal: normal.to_array().map(f64::from),
            crg_height: None,
            friction: None,
        };
        for &i in covering.into_iter().flatten() {
            let Some(patch) = &self.patches[i] else {
                continue;
            };
            let at = At {
                record: &records[i],
                grid: &self.grids[patch.grid],
                rigid: patch.rigid,
            };
            let hint = &mut hint.patches[i];
            match at.record.purpose {
                CrgPurpose::Elevation if sample.crg_height.is_none() => {
                    if let Some((z, normal, bump)) = at.elevation(x, y, hint) {
                        sample.z = z;
                        sample.normal = normal;
                        sample.crg_height = Some(bump);
                    }
                }
                CrgPurpose::Friction if sample.friction.is_none() => {
                    sample.friction = at.friction(x, y, hint);
                }
                _ => {}
            }
        }
        Some(sample)
    }
}

/// Where a map `(x, y)` is in a file, and how to evaluate it there.
enum Found {
    /// Along the road, in `attached` modes.
    Along { on: OnRoad, uv: Uv, sign: f64 },
    /// In the file's own frame, in `genuine` and `global` modes.
    Placed { uv: Uv, rigid: Rigid },
}

/// One loaded CRG record, ready to evaluate.
struct At<'r> {
    record: &'r CrgSurface,
    grid: &'r CrgGrid,
    rigid: Option<Rigid>,
}

impl At<'_> {
    /// Find `(x, y)` in the file. `None` outside the stretch of road it
    /// applies to, or where the file cannot place it.
    fn find(&self, x: f64, y: f64, hint: &mut PatchHint) -> Option<Found> {
        let on = match &self.record.road {
            Some(road) => Some(road.locate(x, y, hint)?),
            None => None,
        };
        match (self.record.mode, self.rigid) {
            (CrgMode::Attached(along) | CrgMode::Attached0(along), _) => {
                let on = on?;
                let (uv, sign) = along.uv(on.s, on.t);
                Some(Found::Along { on, uv, sign })
            }
            (_, Some(rigid)) => {
                let uv = self
                    .grid
                    .uv_from_xy_near(rigid.apply(x, y), &mut hint.crg)?;
                Some(Found::Placed { uv, rigid })
            }
            (_, None) => None,
        }
    }

    /// Height, up-normal and scaled grid height at `(x, y)`.
    fn elevation(&self, x: f64, y: f64, hint: &mut PatchHint) -> Option<(f64, [f64; 3], f64)> {
        let grid = self.grid;
        let k = self.record.z_scale;
        let (z, normal, bump) = match self.find(x, y, hint)? {
            Found::Along { on, uv, sign } => {
                let bump = grid.grid_at_uv(uv)?;
                let (z, dz_ds, dz_dt) = if matches!(self.record.mode, CrgMode::Attached(_)) {
                    (
                        on.z + k * bump.z,
                        on.grade + on.t * on.bank.cos() * on.bank_rate + k * sign * bump.dz_du,
                        on.bank.sin() + k * sign * bump.dz_dv,
                    )
                } else {
                    let z = grid.elevation_at_uv(uv)?;
                    let (dz_du, dz_dv) = slopes(grid, uv, z);
                    (k * z, k * sign * dz_du, k * sign * dz_dv)
                };
                let gradient = [0, 1].map(|c| dz_ds * on.ds[c] + dz_dt * on.dt[c]);
                (z, unit([-gradient[0], -gradient[1], 1.0]), bump.z)
            }
            Found::Placed { uv, rigid } => {
                let z = grid.elevation_at_uv(uv)?;
                let n = grid.normal_at_uv(uv)?;
                let [nx, ny] = rigid.back([n.x, n.y]);
                let bump = grid.grid_at_uv(uv).map_or(0.0, |g| g.z);
                (k * z, unit([k * nx, k * ny, n.z]), bump)
            }
        };
        Some((z + self.record.z_offset, normal, k * bump))
    }

    /// The friction coefficient at `(x, y)`, the grid value exactly as the
    /// file has it. OpenCRG's default modifiers shift a file's values so its
    /// reference line starts at height 0. That suits heights, not friction.
    fn friction(&self, x: f64, y: f64, hint: &mut PatchHint) -> Option<f64> {
        let uv = match self.find(x, y, hint)? {
            Found::Along { uv, .. } | Found::Placed { uv, .. } => uv,
        };
        self.grid.grid_at_uv(uv).map(|g| g.z)
    }
}

/// The slopes of a grid's elevation along `u` and `v` at `uv`, where it is
/// `z`, by central differences where both sides have a value.
fn slopes(grid: &CrgGrid, uv: Uv, z: f64) -> (f64, f64) {
    let slope = |du: f64, dv: f64| {
        let at = |sign: f64| {
            grid.elevation_at_uv(Uv {
                u: uv.u + sign * du,
                v: uv.v + sign * dv,
            })
        };
        match (at(-1.0), at(1.0)) {
            (Some(a), Some(b)) => (b - a) / (2.0 * STEP),
            (None, Some(b)) => (b - z) / STEP,
            (Some(a), None) => (z - a) / STEP,
            (None, None) => 0.0,
        }
    };
    (slope(STEP, 0.0), slope(0.0, STEP))
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|c| c * c).sum::<f64>().sqrt();
    v.map(|c| c / length)
}
