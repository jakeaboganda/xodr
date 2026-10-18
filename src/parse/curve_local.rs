//! `<curveLocal>` outline elements: lines, arcs and paramPoly3 curves in an
//! object's own `(u, v)` plane, sampled to corners.

use std::f64::consts::{PI, TAU};

use super::{attr_f64, child, MAX_LENGTH};
use crate::road::{eval_poly3, GeomRec, GeomShape};

/// The longest edge a curve is cut into, in metres.
const STEP: f64 = 0.25;

/// The most an arc turns along one edge.
const TURN: f64 = PI / 32.0;

/// The most edges one curve is cut into.
const MAX_EDGES: f64 = 1024.0;

/// How far a curve may end from the next one's start before it is a gap, in
/// metres.
pub(super) const GAP: f64 = 0.01;

/// One `<curveLocal>` sampled in the object's own frame.
pub(super) struct Curve {
    /// `[u, v, z, height]` at its start and along it. Its end is left out
    /// where the next corner starts there, and where it has no length.
    pub points: Vec<[f64; 4]>,
    /// How far its end is from the next corner's start, where that is more
    /// than [`GAP`]. 0 otherwise.
    pub gap: f64,
    /// Its heading at its end, for a next curve with no `hdg` to go on from.
    pub end_heading: f64,
    /// Whether it set aside its `hdg` to join up with the curves either side.
    pub turned: bool,
}

/// Sample the `<curveLocal>` `node`. `next` is the corner after it, in the
/// object's frame too, if there is one: the first for the last of a closed
/// outline. `previous` is the heading the curve before it ends at, if it
/// follows one.
///
/// The curve starts at `(u, v)` heading `hdg` and runs `length`. Without a
/// `length` it runs to `next`: a line straight there, an arc round to it,
/// unless that is over [`MAX_LENGTH`], when it is only its start. Without a
/// `next` it is only its start. A line without a `hdg` heads for
/// `next`. Any other curve without one goes on at `previous`, so the
/// outline stays smooth, or else heads along `u`. `z` and `height` go
/// straight from the curve's to `next`'s along it.
///
/// The spec says each curve ends where the next starts. Where a curve's
/// `hdg` leaves a gap that going on at `previous` would close, the curve
/// goes on at `previous` instead, and is [`Curve::turned`]. ASAM's
/// `Ex_SmoothObjectOutline_traffic_island` needs it.
///
/// `None` for a curve missing its `u` or `v`, one that is not a line, an arc
/// or a paramPoly3, and a paramPoly3 of `pRange="arcLength"` with no
/// `length`.
pub(super) fn sample(
    node: roxmltree::Node,
    next: Option<roxmltree::Node>,
    previous: Option<f64>,
) -> Option<Curve> {
    let given = attr_f64(node, "hdg");
    let curve = trace(node, next, given, previous)?;
    if curve.gap == 0.0 || given.is_none() {
        return Some(curve);
    }
    match previous.and_then(|hdg| trace(node, next, Some(hdg), previous)) {
        Some(joined) if joined.gap == 0.0 => Some(Curve {
            turned: true,
            ..joined
        }),
        _ => Some(curve),
    }
}

/// [`sample`] with `hdg` for the curve's own.
fn trace(
    node: roxmltree::Node,
    next: Option<roxmltree::Node>,
    hdg: Option<f64>,
    previous: Option<f64>,
) -> Option<Curve> {
    let corner = |n: roxmltree::Node| -> Option<[f64; 4]> {
        let z = attr_f64(n, "z").unwrap_or(0.0);
        let height = attr_f64(n, "height").unwrap_or(0.0);
        Some([attr_f64(n, "u")?, attr_f64(n, "v")?, z, height])
    };
    let start = corner(node)?;
    let end = next.and_then(corner);
    let [u, v, ..] = start;
    let given = attr_f64(node, "length").filter(|l| *l > 0.0 && *l <= MAX_LENGTH);
    let (path, edges) = if child(node, "line").is_some() {
        let hdg = hdg
            .or_else(|| end.map(|[eu, ev, ..]| (ev - v).atan2(eu - u)))
            .or(previous)
            .unwrap_or(0.0);
        let length = given
            .or_else(|| end.map(|[eu, ev, ..]| (eu - u).hypot(ev - v)))
            .unwrap_or(0.0);
        let line = GeomRec::new(0.0, u, v, hdg, length, GeomShape::Line);
        (Path::Geom(line), 1.0)
    } else if let Some(arc) = child(node, "arc") {
        let curvature = attr_f64(arc, "curvature")?;
        let hdg = hdg.or(previous).unwrap_or(0.0);
        let length = given
            .or_else(|| {
                end.map(|[eu, ev, ..]| round_to(u, v, hdg, curvature, eu, ev))
                    .filter(|l| *l <= MAX_LENGTH)
            })
            .unwrap_or(0.0);
        let arc = GeomRec::new(0.0, u, v, hdg, length, GeomShape::Arc { curvature });
        let edges = (length / STEP).max(curvature.abs() * length / TURN);
        (Path::Geom(arc), edges)
    } else {
        let pp = child(node, "paramPoly3")?;
        let coeff = |n: &str| attr_f64(pp, n).unwrap_or(0.0);
        let p_max = match pp.attribute("pRange").map(str::to_ascii_lowercase) {
            Some(r) if r == "arclength" => given?,
            _ => 1.0,
        };
        let poly = Path::Poly {
            origin: (u, v),
            hdg: hdg.or(previous).unwrap_or(0.0),
            u: [coeff("aU"), coeff("bU"), coeff("cU"), coeff("dU")],
            v: [coeff("aV"), coeff("bV"), coeff("cV"), coeff("dV")],
            p_max,
        };
        let estimate = given.unwrap_or_else(|| {
            (0..16)
                .map(|k| {
                    let (a, b) = (poly.at(k as f64 / 16.0), poly.at((k + 1) as f64 / 16.0));
                    (b.0 - a.0).hypot(b.1 - a.1)
                })
                .sum()
        });
        (poly, (estimate / STEP).max(8.0))
    };
    let edges = edges.ceil().clamp(1.0, MAX_EDGES) as usize;
    let to = end.unwrap_or(start);
    let point = |f: f64| {
        let (pu, pv) = path.at(f);
        let lerp = |i: usize| start[i] + (to[i] - start[i]) * f;
        [pu, pv, lerp(2), lerp(3)]
    };
    let mut points: Vec<[f64; 4]> = (0..edges).map(|k| point(k as f64 / edges as f64)).collect();
    let last = point(1.0);
    let gap = end.map_or(0.0, |[eu, ev, ..]| (last[0] - eu).hypot(last[1] - ev));
    let runs_on = end.is_none() && (last[0] - u).hypot(last[1] - v) > GAP;
    if runs_on || gap > GAP {
        points.push(last);
    }
    Some(Curve {
        points,
        gap: if gap > GAP { gap } else { 0.0 },
        end_heading: path.heading_at_end(),
        turned: false,
    })
}

/// The shape of one curve.
enum Path {
    /// A line or an arc.
    Geom(GeomRec),
    /// A paramPoly3 from `origin` heading `hdg`, over `p` in `[0, p_max]`.
    Poly {
        origin: (f64, f64),
        hdg: f64,
        u: [f64; 4],
        v: [f64; 4],
        p_max: f64,
    },
}

impl Path {
    /// The curve's heading at its end.
    fn heading_at_end(&self) -> f64 {
        match self {
            Self::Geom(geom) => geom.pose(geom.length).2,
            Self::Poly {
                hdg, u, v, p_max, ..
            } => {
                let slope = |c: &[f64; 4]| c[1] + p_max * (2.0 * c[2] + 3.0 * c[3] * p_max);
                hdg + slope(v).atan2(slope(u))
            }
        }
    }

    /// Where the curve is a fraction `f` of the way along it.
    fn at(&self, f: f64) -> (f64, f64) {
        match self {
            Self::Geom(geom) => {
                let (u, v, _) = geom.pose(f * geom.length);
                (u, v)
            }
            Self::Poly {
                origin,
                hdg,
                u,
                v,
                p_max,
            } => {
                let (sin, cos) = hdg.sin_cos();
                let (pu, pv) = (eval_poly3(*u, f * p_max), eval_poly3(*v, f * p_max));
                (
                    origin.0 + pu * cos - pv * sin,
                    origin.1 + pu * sin + pv * cos,
                )
            }
        }
    }
}

/// How far an arc from `(u, v)` heading `hdg` turns round its circle before
/// it comes level with `(to_u, to_v)`, in metres: a full turn if that is its
/// start. Straight there for a curvature of 0.
fn round_to(u: f64, v: f64, hdg: f64, curvature: f64, to_u: f64, to_v: f64) -> f64 {
    if curvature.abs() < 1e-9 {
        return (to_u - u).hypot(to_v - v);
    }
    let (sin, cos) = hdg.sin_cos();
    let (cu, cv) = (u - sin / curvature, v + cos / curvature);
    let (a, b) = ((u - cu, v - cv), (to_u - cu, to_v - cv));
    let turned = (a.0 * b.1 - a.1 * b.0).atan2(a.0 * b.0 + a.1 * b.1) * curvature.signum();
    let turned = if turned <= 1e-9 { turned + TAU } else { turned };
    turned / curvature.abs()
}
