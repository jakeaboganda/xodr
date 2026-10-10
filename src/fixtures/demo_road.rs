use std::f32::consts::FRAC_PI_2;

use crate::coords::Point;

use crate::geometry::{left_normal, Polyline};
use crate::network::{Direction, Lane, LaneId, LaneType, RoadNetwork};

const LANE_WIDTH: f32 = 3.5;
const STRAIGHT: f32 = 40.0;
const RADIUS: f32 = 30.0;
const STEP: f32 = 2.0;
const GRADE: f32 = 0.04; // gentle 4% uphill, so the road is genuinely 3D

/// A hand-authored two-lane road: a straight along +X, then a 90° left curve,
/// climbing at a constant grade. `tests/data/demo.xodr` is its OpenDRIVE twin,
/// so the importer can be checked against a road whose shape is known in code.
pub(crate) fn demo_road() -> RoadNetwork {
    let reference = reference_line();
    // Two opposing lanes, offset ±half a lane width from the reference line.
    let forward = offset_line(&reference, -LANE_WIDTH / 2.0);
    let backward = offset_line(&reference, LANE_WIDTH / 2.0);
    RoadNetwork::new(vec![
        Lane {
            id: LaneId(0),
            kind: LaneType::Driving,
            layer: crate::Layer::Permanent,
            direction: Direction::Forward,
            center: forward,
            width: LANE_WIDTH,
            widths: Vec::new(),
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        },
        Lane {
            id: LaneId(1),
            kind: LaneType::Driving,
            layer: crate::Layer::Permanent,
            direction: Direction::Backward,
            center: backward,
            width: LANE_WIDTH,
            widths: Vec::new(),
            bank: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            neighbors: Vec::new(),
        },
    ])
}

/// The road's reference centerline, sampled to points.
fn reference_line() -> Polyline {
    let mut points = Vec::new();

    // Straight along +X.
    let mut x = 0.0;
    while x <= STRAIGHT {
        points.push(Point::new(x, 0.0, x * GRADE));
        x += STEP;
    }

    // A 90° left arc off the end of the straight. Heading rotates +X → +Y; the
    // arc centre sits a radius to the left, at (STRAIGHT, +RADIUS, ·). Sample
    // evenly and land exactly on 90° at the end (skip angle 0. it duplicates
    // the straight's last point).
    let steps = (FRAC_PI_2 * RADIUS / STEP).ceil() as usize;
    for k in 1..=steps {
        let angle = FRAC_PI_2 * k as f32 / steps as f32;
        let s = STRAIGHT + RADIUS * angle;
        points.push(Point::new(
            STRAIGHT + RADIUS * angle.sin(),
            RADIUS * (1.0 - angle.cos()),
            s * GRADE,
        ));
    }

    Polyline::new(points)
}

/// Offset a polyline laterally by `offset` (positive = left of travel), along
/// each vertex's bisector normal so lane width stays consistent across vertices.
fn offset_line(line: &Polyline, offset: f32) -> Polyline {
    let shifted = line
        .points()
        .iter()
        .zip(line.tangents())
        .map(|(point, tangent)| *point + left_normal(*tangent) * offset)
        .collect();
    Polyline::new(shifted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_road_has_two_driving_lanes() {
        assert_eq!(demo_road().driving_lanes().count(), 2);
    }

    #[test]
    fn forward_lane_starts_plus_x_ends_plus_y_and_climbs() {
        let net = demo_road();
        let lane = net.lane(LaneId(0)).expect("forward lane");
        let start = lane.center.pose_at(0.0);
        let end = lane.center.pose_at(lane.center.length());
        // Straight start faces +X; after the 90° left curve it faces +Y.
        assert!(start.heading.x > 0.9, "start {:?}", start.heading);
        assert!(end.heading.y > 0.9, "end {:?}", end.heading);
        // The constant grade climbs.
        assert!(end.position.z > start.position.z + 1.0);
    }

    #[test]
    fn lane_runs_past_the_straight_into_the_curve() {
        let lane_len = demo_road().lane(LaneId(0)).unwrap().center.length();
        assert!(lane_len > STRAIGHT, "length {lane_len}");
    }

    #[test]
    fn nearest_lane_resolves_each_side_of_the_road() {
        let net = demo_road();
        // Forward lane (offset −w/2 by left_normal(+X)=+Y) lands on the −Y side;
        // backward lane on the +Y side.
        let (near_minus_y, _) = net.nearest_lane(Point::new(1.0, -1.6, 0.0)).unwrap();
        let (near_plus_y, _) = net.nearest_lane(Point::new(1.0, 1.6, 0.0)).unwrap();
        assert_eq!(near_minus_y, LaneId(0));
        assert_eq!(near_plus_y, LaneId(1));
    }

    #[test]
    fn lanes_keep_width_and_dont_invert_through_the_curve() {
        let net = demo_road();
        let forward = &net.lane(LaneId(0)).unwrap().center;
        let backward = &net.lane(LaneId(1)).unwrap().center;
        // Across the whole road (straight and curve), the two lane centers stay
        // ~one lane width apart --] no inversion or width collapse on the arc.
        for k in 0..=10 {
            let s = forward.length() * k as f32 / 10.0;
            let p = forward.point_at(s);
            let gap = (p - backward.project(p).point).length();
            assert!((gap - LANE_WIDTH).abs() < 0.4, "gap {gap} at s {s}");
        }
    }
}
