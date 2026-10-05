//! A banked oval racetrack: a closed stadium loop, flat on the straights and
//! super-elevated (canted) through the two curves. A plain [`RoadNetwork`]:
//! the single lane carries the cant in its `bank` profile, so `surface_mesh`
//! tessellates a canted surface and `sample_near` reads the lean, the same
//! paths an imported banked `.xodr` uses, without needing the parser. Its
//! closed loop also exercises a cyclic lane graph.

use std::f32::consts::PI;

use crate::coords::{Point, Vector};

use crate::geometry::Polyline;
use crate::network::{Direction, Lane, LaneId, LaneType, RoadNetwork};

const STRAIGHT: f32 = 70.0; // length of each straight (m)
const RADIUS: f32 = 26.0; // curve radius at the centerline (m)
const WIDTH: f32 = 10.0; // lane width (m), wide so the cant reads
const STEP: f32 = 2.5; // centerline sample spacing (m)
/// Superelevation at the curve apex (rad, ~12 deg). Positive raises the LEFT
/// edge; the oval turns consistently right, so its left edge is always the outer
/// (raised) one.
const PEAK_BANK: f32 = 0.21;

/// Smoothstep from `e0` to `e1` (works when `e0 > e1`, i.e. decreasing).
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bank envelope across a curve, as a fraction `f` in `[0, 1]`: rises over the
/// first quarter, holds at 1, falls over the last quarter, so the cant meets
/// the flat straights at 0 at both curve ends.
fn curve_ramp(f: f32) -> f32 {
    const E: f32 = 0.25;
    smoothstep(0.0, E, f) * smoothstep(1.0, 1.0 - E, f)
}

/// Build the closed, banked oval described in the module docs: one canted
/// driving lane whose successor wraps to itself.
pub(crate) fn banked_oval() -> RoadNetwork {
    let half_s = STRAIGHT * 0.5;
    let curve_pts = ((PI * RADIUS) / STEP).ceil() as usize;
    let mut raw: Vec<(Point, f32)> = Vec::new();

    // Phase 1. The y = +R straight, heading +X, flat.
    let mut x = -half_s;
    while x <= half_s + 1e-3 {
        raw.push((Point::new(x, RADIUS, 0.0), 0.0));
        x += STEP;
    }
    // Phase 2. Right curve, center (half_s, 0, 0), theta -pi/2 to +pi/2.
    for k in 1..=curve_pts {
        let f = k as f32 / curve_pts as f32;
        let theta = -PI / 2.0 + f * PI;
        let p = Point::new(half_s + RADIUS * theta.cos(), -RADIUS * theta.sin(), 0.0);
        raw.push((p, PEAK_BANK * curve_ramp(f)));
    }
    // Phase 3. The y = -R straight, heading -X, flat.
    let mut x = half_s;
    while x >= -half_s - 1e-3 {
        raw.push((Point::new(x, -RADIUS, 0.0), 0.0));
        x -= STEP;
    }
    // Phase 4. Left curve, center (-half_s, 0, 0), theta +pi/2 to +3pi/2.
    for k in 1..=curve_pts {
        let f = k as f32 / curve_pts as f32;
        let theta = PI / 2.0 + f * PI;
        let p = Point::new(-half_s + RADIUS * theta.cos(), -RADIUS * theta.sin(), 0.0);
        raw.push((p, PEAK_BANK * curve_ramp(f)));
    }
    // Close the loop back onto the first point.
    raw.push((Point::new(-half_s, RADIUS, 0.0), 0.0));

    // Dedup consecutive near-equal points (phase joins land on the same point).
    let mut points: Vec<Point> = Vec::new();
    let mut bank: Vec<f32> = Vec::new();
    for (p, b) in raw {
        if points.last().is_none_or(|l| (*l - p).length() >= 1e-3) {
            points.push(p);
            bank.push(b);
        }
    }

    RoadNetwork::new(vec![Lane {
        id: LaneId(0),
        kind: LaneType::Driving,
        direction: Direction::Forward,
        center: Polyline::new(points),
        width: WIDTH,
        widths: Vec::new(),
        // The cant, per centerline vertex. surface_mesh tilts the ribs by it
        // and sample_near reads it, so no bespoke banked bundle is needed.
        bank,
        // A closed loop: driving off the exit end re-enters the same lane.
        successors: vec![LaneId(0)],
        predecessors: vec![LaneId(0)],
        neighbors: Vec::new(),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The oval's single lane's road-surface sample nearest a world point.
    fn sample_near(net: &RoadNetwork, p: Point) -> crate::RoadSample {
        net.sample_near(p).expect("the oval has a lane")
    }

    #[test]
    fn oval_is_one_closed_driving_lane() {
        let net = banked_oval();
        let lanes: Vec<_> = net.driving_lanes().collect();
        assert_eq!(lanes.len(), 1);
        // Geometrically closed: first and last centerline points coincide.
        let pts = lanes[0].center.points();
        assert!(
            (pts[0] - pts[pts.len() - 1]).length() < 1e-3,
            "loop should close: {:?} vs {:?}",
            pts[0],
            pts[pts.len() - 1]
        );
    }

    #[test]
    fn straights_are_flat_and_curves_are_banked() {
        let net = banked_oval();
        // A point partway along the y = +R straight: near-zero bank.
        let on_straight = sample_near(&net, Point::new(0.0, RADIUS, 0.0));
        assert!(
            on_straight.bank.abs() < 0.01,
            "straight bank {}",
            on_straight.bank
        );
        // The right-curve apex (max +X): near the peak bank.
        let half_s = STRAIGHT * 0.5;
        let apex = sample_near(&net, Point::new(half_s + RADIUS, 0.0, 0.0));
        assert!(
            apex.bank > 0.9 * PEAK_BANK,
            "apex bank {} should approach {}",
            apex.bank,
            PEAK_BANK
        );
    }

    #[test]
    fn straight_up_is_vertical_curve_up_is_tilted() {
        let net = banked_oval();
        let straight = sample_near(&net, Point::new(0.0, RADIUS, 0.0));
        assert!(straight.up.abs_diff_eq(Vector::Z, 1e-3));
        let half_s = STRAIGHT * 0.5;
        let apex = sample_near(&net, Point::new(half_s + RADIUS, 0.0, 0.0));
        // Tilted: the up-normal leans off vertical but still points up.
        assert!(apex.up.z < 0.99 && apex.up.z > 0.9, "up.z {}", apex.up.z);
        assert!(
            (apex.up - Vector::Z).length() > 0.05,
            "curve up should be visibly tilted"
        );
    }

    #[test]
    fn surface_mesh_raises_the_outer_edge_at_a_curve_apex() {
        // The shared tessellator cants the ribs from the lane's bank, exactly
        // what the retired `banked_mesh` did.
        let mesh = banked_oval().surface_mesh();
        mesh.validate().expect("banked oval is a valid trimesh");
        // Ribs are pushed as [left, right] pairs. Find the rib nearest the
        // right-curve apex and check the left (outer) vertex rides above the
        // right (inner) one; on a straight rib they are level.
        let half_s = STRAIGHT * 0.5;
        let apex_xy = Point::new(half_s + RADIUS, 0.0, 0.0);
        let mut best = 0usize;
        let mut best_d = f32::INFINITY;
        for i in (0..mesh.vertices.len()).step_by(2) {
            let mid = mesh.vertices[i].lerp(mesh.vertices[i + 1], 0.5);
            let d = (Point::new(mid.x, mid.y, 0.0) - apex_xy).length();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        let left = mesh.vertices[best];
        let right = mesh.vertices[best + 1];
        assert!(
            left.z - right.z > 0.5,
            "outer (left) edge {} should ride above inner (right) {} at the apex",
            left.z,
            right.z
        );

        // A straight rib (near the y = +R straight's centre) is level.
        let straight_xy = Point::new(0.0, RADIUS, 0.0);
        let mut s_best = 0usize;
        let mut s_d = f32::INFINITY;
        for i in (0..mesh.vertices.len()).step_by(2) {
            let mid = mesh.vertices[i].lerp(mesh.vertices[i + 1], 0.5);
            let d = (Point::new(mid.x, mid.y, 0.0) - straight_xy).length();
            if d < s_d {
                s_d = d;
                s_best = i;
            }
        }
        assert!(
            (mesh.vertices[s_best].z - mesh.vertices[s_best + 1].z).abs() < 0.05,
            "straight rib should be level"
        );
    }

    #[test]
    fn sample_near_tracks_heading_around_the_loop() {
        let net = banked_oval();
        // On the y = +R straight the car heads +X; on the y = -R straight, -X.
        let near = sample_near(&net, Point::new(0.0, RADIUS, 0.0));
        assert!(near.heading.x > 0.9, "+R heading {:?}", near.heading);
        let far = sample_near(&net, Point::new(0.0, -RADIUS, 0.0));
        assert!(far.heading.x < -0.9, "-R heading {:?}", far.heading);
    }
}
