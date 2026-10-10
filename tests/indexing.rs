//! The indexed lookups must answer exactly what a full scan answers.
//!
//! `nearest_lane` and `MeshSampler::height_at` prune candidates by a
//! ground-plane grid. Pruning is where this kind of optimisation goes wrong,
//! silently, on the one query that mattered. So both are checked against the
//! brute-force answer on a real city export, which has the stacked roads,
//! dead ground, and far-off-map queries a hand-written fixture would not.

use xodr::{load_file, LaneId, Point, Projection, RoadNetwork, Vector};

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");

/// What `nearest_lane` did before the index: project onto every driving lane
/// and keep the closest in 3D, first one winning a tie.
fn scan_nearest(net: &RoadNetwork, point: Point) -> Option<(LaneId, Projection)> {
    net.driving_lanes()
        .map(|lane| (lane.id, lane.center.project(point)))
        .min_by(|(_, a), (_, b)| {
            let d = |p: &Projection| (point - p.point).length_squared();
            d(a).total_cmp(&d(b))
        })
}

/// Query points covering the cases the grid has to get right: on the road,
/// beside it, high above and far below it, out past the map, and at the
/// coordinate extremes.
fn probes(net: &RoadNetwork) -> Vec<Point> {
    let lanes: Vec<_> = net.driving_lanes().collect();
    let mut out = Vec::new();
    for i in 0..48 {
        let lane = lanes[i * lanes.len() / 48];
        let on = lane.center.point_at(lane.center.length() * 0.37);
        out.push(on);
        out.push(on + Vector::new(7.5, 0.0, -7.5)); // off to one side
        out.push(on + Vector::new(0.0, 60.0, 0.0)); // well above the surface
        out.push(on + Vector::new(0.0, -60.0, 0.0)); // well below it
        out.push(on + Vector::new(400.0, 0.0, 250.0)); // out past the map
    }
    out.extend([
        Point::ORIGIN,
        Point::new(1.0e9, 0.0, -1.0e9),
        Point::new(-1.0e9, 1.0e9, 0.0),
        Point::new(f32::MAX, 0.0, f32::MIN),
    ]);
    out
}

#[test]
fn the_lane_index_agrees_with_a_full_scan() {
    let net = load_file(TOWN).expect("town07 loads");
    for point in probes(&net) {
        let indexed = net.nearest_lane(point);
        let scanned = scan_nearest(&net, point);
        assert_eq!(
            indexed.map(|(id, _)| id),
            scanned.map(|(id, _)| id),
            "different lane at {point:?}"
        );
        assert_eq!(
            indexed.map(|(_, p)| p.point),
            scanned.map(|(_, p)| p.point),
            "different projection at {point:?}"
        );
    }
}

#[test]
fn a_non_finite_query_returns_rather_than_panicking() {
    // A NaN reaches here from a diverged integrator, not from a file. It has
    // no nearest anything; it must not take the process down on the way to
    // saying so.
    let net = load_file(TOWN).expect("town07 loads");
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let _ = net.nearest_lane(Point::new(bad, 0.0, 0.0));
        let _ = net.nearest_lane(Point::new(0.0, bad, 0.0));
        let _ = net.nearest_lane(Point::splat(bad));
        let _ = net.sample_near(Point::new(0.0, 0.0, bad));
    }
}

#[test]
fn the_mesh_sampler_agrees_with_a_full_scan() {
    // Town07 has overlapping surfaces, so this also pins that the sampler
    // picks the same one of two stacked roads that the scan does.
    let net = load_file(TOWN).expect("town07 loads");
    let mesh = net.surface_mesh();
    let sampler = mesh.sampler();
    let mut hits = 0;
    for point in probes(&net) {
        let indexed = sampler.height_at(point.x, point.z);
        let scanned = mesh.height_at(point.x, point.z);
        assert_eq!(indexed, scanned, "different surface under {point:?}");
        hits += usize::from(indexed.is_some());
    }
    assert!(hits > 40, "only {hits} probes landed on the road at all");
}

#[test]
fn an_empty_network_indexes_and_answers_nothing() {
    let empty = RoadNetwork::default();
    assert!(empty.nearest_lane(Point::ORIGIN).is_none());
    assert!(empty.sample_near(Point::ORIGIN).is_none());
    assert!(empty
        .route(Point::ORIGIN, Point::new(1.0, 0.0, 0.0))
        .is_none());

    let mesh = empty.surface_mesh();
    assert!(mesh.sampler().height_at(0.0, 0.0).is_none());
}

#[test]
fn nan_never_finds_or_wins_a_lane() {
    use xodr::{Direction, Lane, LaneId, LaneType, Polyline, RoadNetwork};
    let lane = |id: usize, points: Vec<Point>| Lane {
        id: LaneId(id),
        kind: LaneType::Driving,
        layer: xodr::Layer::Permanent,
        direction: Direction::Forward,
        center: Polyline::new(points),
        width: 3.5,
        widths: Vec::new(),
        bank: Vec::new(),
        successors: Vec::new(),
        predecessors: Vec::new(),
        neighbors: Vec::new(),
    };
    let net = RoadNetwork::new(vec![
        lane(
            0,
            vec![Point::new(0.0, 0.0, f32::NAN), Point::new(10.0, 0.0, 0.0)],
        ),
        lane(
            1,
            vec![Point::new(0.0, 3.0, 0.0), Point::new(10.0, 3.0, 0.0)],
        ),
    ]);
    assert!(net
        .nearest_lane(Point::new(f32::NAN, f32::NAN, 0.0))
        .is_none());
    assert!(net.sample_near(Point::new(5.0, f32::NAN, 0.0)).is_none());
    let (id, _) = net.nearest_lane(Point::new(5.0, 3.0, 0.0)).unwrap();
    assert_eq!(id, LaneId(1), "the NaN vertex doesn't win");
}

#[test]
fn the_lowest_point_skips_a_nan_height() {
    use xodr::{Direction, Lane, LaneType, Polyline};
    let lane = |id: usize, z: f32| Lane {
        id: LaneId(id),
        kind: LaneType::Driving,
        layer: xodr::Layer::Permanent,
        direction: Direction::Forward,
        center: Polyline::new(vec![Point::new(0.0, 0.0, z), Point::new(10.0, 0.0, z)]),
        width: 3.5,
        widths: Vec::new(),
        bank: Vec::new(),
        successors: Vec::new(),
        predecessors: Vec::new(),
        neighbors: Vec::new(),
    };
    let net = RoadNetwork::new(vec![lane(0, f32::NAN), lane(1, -5.0)]);
    assert_eq!(net.min_elevation(), Some(-5.0));
}
