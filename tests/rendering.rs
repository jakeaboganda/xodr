//! What a renderer needs from a baked map, checked on a real one.
//!
//! Visualization is not built here yet, so these pin the two things that
//! would be expensive to retrofit: the surface mesh can be traced back to the
//! lanes it came from, and the whole network survives a round trip out of the
//! process.

use xodr::load_file;

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");
#[cfg(feature = "serde")]
const SWEEPER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/banked_sweeper.xodr"
);

#[test]
fn every_lane_span_addresses_its_own_slice_of_the_mesh() {
    let net = load_file(TOWN).expect("town07 loads");
    let mesh = net.surface_mesh();

    assert_eq!(
        mesh.lanes.len(),
        net.lanes().len(),
        "one span per tessellated lane"
    );

    let mut covered = 0usize;
    for (span, lane) in mesh.lanes.iter().zip(net.lanes()) {
        assert_eq!(span.lane, lane.id, "spans are in emission order");
        assert!(net.lane(span.lane).is_some(), "span names a real lane");

        // Two ribs per surviving centerline vertex, and up to two triangles
        // per segment, less any with no area, where the lane's width is 0.
        // Welding near-duplicate points can drop some, so a lane keeps at most
        // one rib pair per centerline vertex and always at least two.
        let points = lane.center.points().len();
        let (vertices, indices) = (
            (span.vertices.end - span.vertices.start) as usize,
            (span.indices.end - span.indices.start) as usize,
        );
        assert_eq!(vertices % 2, 0, "lane {:?} has paired ribs", lane.id);
        let ribs = vertices / 2;
        assert!(
            (2..=points).contains(&ribs),
            "lane {:?} kept {ribs} rib pairs from {points} points",
            lane.id
        );
        assert!(
            indices % 3 == 0 && indices <= (ribs - 1) * 6,
            "lane {:?} index count {indices}",
            lane.id
        );

        // The slices are addressable, and every triangle in this lane's index
        // range points inside this lane's vertex range, so a renderer can
        // draw or pick one lane without dragging in its neighbours.
        let _ = &mesh.vertices[span.vertices.start as usize..span.vertices.end as usize];
        let _ = &mesh.normals[span.vertices.start as usize..span.vertices.end as usize];
        for &i in &mesh.indices[span.indices.start as usize..span.indices.end as usize] {
            assert!(
                span.vertices.contains(&i),
                "lane {:?} indexes vertex {i} outside its own span {:?}",
                lane.id,
                span.vertices
            );
        }
        covered += indices;
    }
    assert_eq!(covered, mesh.indices.len(), "the spans tile the whole mesh");
}

#[test]
fn a_span_is_left_and_right_boundary_vertices_in_alternation() {
    // A renderer draws lane edges by taking every other vertex of a span. That
    // only works because `LaneSpan` promises the pairing, so check the promise:
    // in each pair the two vertices straddle the centerline, one to its left
    // and one to its right, no further apart than the lane is wide.
    let net = load_file(TOWN).expect("town07 loads");
    let mesh = net.surface_mesh();

    let mut checked = 0usize;
    for span in &mesh.lanes {
        let lane = net.lane(span.lane).expect("the span names a real lane");
        let slice = &mesh.vertices[span.vertices.start as usize..span.vertices.end as usize];
        for pair in slice.chunks_exact(2) {
            let (left, right) = (pair[0], pair[1]);
            let width = (left - right).length();
            assert!(
                width <= lane.width + 1e-3,
                "lane {:?} rib is {width} m across, wider than the {} m lane",
                lane.id,
                lane.width
            );
            if width < 1e-3 {
                // Where a lane tapers to a point its two ribs coincide, and
                // there is no left and right to tell apart.
                continue;
            }
            checked += 1;
            assert!(
                lane.center.project(left).offset > 0.0,
                "lane {:?} has an even vertex right of its centerline",
                lane.id
            );
            assert!(
                lane.center.project(right).offset < 0.0,
                "lane {:?} has an odd vertex left of its centerline",
                lane.id
            );
        }
    }
    assert!(checked > 1000, "only {checked} ribs had a width to check");
}

#[cfg(feature = "serde")]
#[test]
fn a_network_survives_a_round_trip_and_is_still_queryable() {
    // A viewer typically lives in another process. What it receives has to
    // behave like an imported map, not merely look like one. The arc lengths,
    // tangents and lane index are all derived state that has to come back.
    use xodr::RoadNetwork;

    let net = load_file(SWEEPER).expect("the banked sweeper loads");
    let json = serde_json::to_string(&net).expect("serialize");
    let back: RoadNetwork = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(net, back);
    for lane in net.driving_lanes() {
        let probe = lane.center.point_at(lane.center.length() * 0.5);
        assert_eq!(
            net.sample_near(probe),
            back.sample_near(probe),
            "lane {:?} samples differently after a round trip",
            lane.id
        );
        assert_eq!(net.nearest_lane(probe), back.nearest_lane(probe));
    }
    assert_eq!(net.surface_mesh(), back.surface_mesh());
}

#[cfg(feature = "serde")]
#[test]
fn a_mesh_survives_a_round_trip() {
    let mesh = load_file(SWEEPER).expect("loads").surface_mesh();
    let json = serde_json::to_string(&mesh).expect("serialize");
    let back: xodr::Mesh = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(mesh, back);
}

#[cfg(feature = "serde")]
#[test]
fn a_polyline_of_fewer_than_two_points_is_refused_on_the_way_in() {
    // Polyline serializes as points plus its two boundary tangents, so
    // deserializing is the one place an untrusted peer could hand us a
    // degenerate one.
    use xodr::Polyline;
    let one = r#"{"points":[[0,0,0]],"tangents":[[1,0,0],[1,0,0]]}"#;
    let none = r#"{"points":[],"tangents":[[1,0,0],[1,0,0]]}"#;
    assert!(serde_json::from_str::<Polyline>(one).is_err());
    assert!(serde_json::from_str::<Polyline>(none).is_err());

    let two = r#"{"points":[[0,0,0],[10,0,0]],"tangents":[[1,0,0],[1,0,0]]}"#;
    let line: Polyline = serde_json::from_str(two).expect("two points");
    assert!((line.length() - 10.0).abs() < 1e-5);
    // Derived state really was rebuilt, not defaulted.
    assert!(line.pose_at(5.0).heading.x > 0.9);

    // A boundary tangent that is nonsense degrades to the end chord rather than
    // sinking an otherwise valid polyline.
    let junk = r#"{"points":[[0,0,0],[10,0,0]],"tangents":[[0,0,0],[0,0,0]]}"#;
    let degenerate: Polyline = serde_json::from_str(junk).expect("junk tangents still load");
    assert!(degenerate.pose_at(0.0).heading.x > 0.9);
}
