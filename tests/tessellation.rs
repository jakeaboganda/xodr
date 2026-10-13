//! `surface_mesh` must not fold. A road surface is Z-up and its banking is
//! gentle, so every non-degenerate triangle faces up. A downward-facing one is
//! an inverted bowtie where the inner rib crossed itself. Two shapes cause it,
//! and both are guarded. A stub segment a few cm long at a lane-section joint
//! tessellates to a near-collinear sliver whose winding flips. A corner tighter
//! than the lane half-width lets the inner offset push past the curve's center.

use xodr::{
    load_file, load_file_with_provenance, Direction, Lane, LaneId, LaneType, Point, Polyline,
    RoadNetwork, Vector,
};

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");

/// Up-component of a triangle's geometric normal, from its vertex winding.
fn facet_up(a: Point, b: Point, c: Point) -> f32 {
    (b - a).cross(c - a).normalize_or(Vector::ZERO).z
}

/// Count of downward-facing, inverted triangles. Zero-area facets from a
/// zero-width lane are not inversions, so they are left out.
fn inverted(mesh: &xodr::Mesh) -> usize {
    mesh.indices
        .chunks_exact(3)
        .filter(|t| {
            facet_up(
                mesh.vertices[t[0] as usize],
                mesh.vertices[t[1] as usize],
                mesh.vertices[t[2] as usize],
            ) < -1e-4
        })
        .count()
}

#[test]
fn town07_surface_has_no_inverted_triangles() {
    let net = load_file(TOWN).expect("load town07");
    let mesh = net.surface_mesh();
    let bad = inverted(&mesh);
    assert_eq!(
        bad,
        0,
        "town07 tessellated {bad} inverted triangles out of {}; a lane rib folded",
        mesh.indices.len() / 3,
    );
}

/// A quarter circle of radius 1 m carries a 3 m lane, so the half-width of
/// 1.5 m exceeds the radius. An unclamped inner rib would push past the arc's
/// center and fold. The clamp pinches it instead, leaving no inversion.
#[test]
fn a_corner_tighter_than_the_half_width_does_not_fold() {
    let r = 1.0_f32;
    let width = 3.0_f32;
    // About 17 deg per step gives ~0.3 m chords, well above the weld threshold,
    // so this exercises the tight curve, not a stub segment.
    let pts: Vec<Point> = (0..=6)
        .map(|k| {
            let a = k as f32 * 0.3;
            Point::new(r * a.cos(), r * a.sin(), 0.0)
        })
        .collect();
    let net = RoadNetwork::new(vec![Lane {
        id: LaneId(0),
        kind: LaneType::Driving,
        layer: xodr::Layer::Permanent,
        direction: Direction::Forward,
        center: Polyline::new(pts),
        width,
        widths: Vec::new(),
        bank: Vec::new(),
        successors: Vec::new(),
        predecessors: Vec::new(),
        neighbors: Vec::new(),
    }]);
    let mesh = net.surface_mesh();

    assert_eq!(inverted(&mesh), 0, "tight curve folded the inner rib");

    // Confirm the clamp engaged. Some rib pair is pinched below the full width,
    // so this is a real regression, not a curve that never needed it.
    let min_rib = mesh
        .vertices
        .chunks_exact(2)
        .map(|p| (p[0] - p[1]).length())
        .fold(f32::INFINITY, f32::min);
    assert!(
        min_rib < width - 0.01,
        "inner rib was never pinched (min width {min_rib}); clamp did nothing",
    );
}

/// Build a one-lane network from bare centerline points.
fn lane_of(width: f32, pts: &[[f32; 3]]) -> RoadNetwork {
    RoadNetwork::new(vec![Lane {
        id: LaneId(0),
        kind: LaneType::Driving,
        layer: xodr::Layer::Permanent,
        direction: Direction::Forward,
        center: Polyline::new(pts.iter().map(|p| Point::from_array(*p)).collect()),
        width,
        widths: Vec::new(),
        bank: Vec::new(),
        successors: Vec::new(),
        predecessors: Vec::new(),
        neighbors: Vec::new(),
    }])
}

/// Ribs in the mesh, one pair of vertices each.
fn ribs(mesh: &xodr::Mesh) -> usize {
    mesh.vertices.len() / 2
}

#[test]
fn a_stub_last_segment_is_welded_away_and_the_end_kept() {
    // 10 cm past a 4 m sample is a section end landing on top of the sample
    // before it, not a sample of its own. It goes, and the vertex that absorbs
    // it lands exactly on the lane's endpoint so the next section still meets
    // this one.
    let net = lane_of(3.0, &[[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.1, 0.0, 0.0]]);
    let mesh = net.surface_mesh();
    assert_eq!(ribs(&mesh), 2, "the stub should have been welded away");
    let last = mesh.vertices[mesh.vertices.len() - 2];
    assert!(
        (last.x - 4.1).abs() < 1e-5,
        "the surviving rib sits at {}, not on the endpoint",
        last.x
    );
}

#[test]
fn evenly_spaced_short_segments_are_all_kept() {
    // The same 10 cm step among other 10 cm steps is the sampling, not a stub.
    // Welding it would flatten exactly the tight curves that need the detail.
    let pts: Vec<[f32; 3]> = (0..=5).map(|k| [k as f32 * 0.3, 0.0, 0.0]).collect();
    let mesh = lane_of(3.0, &pts).surface_mesh();
    assert_eq!(ribs(&mesh), pts.len(), "no sample should have been welded");
}

/// The most a lane's centerline turns from one chord to the next, over the
/// lanes `keep` picks.
fn worst_turn(net: &RoadNetwork, keep: impl Fn(&Lane) -> bool) -> f32 {
    let flat = |a: Point, b: Point| Vector::new(b.x - a.x, b.y - a.y, 0.0);
    let mut worst = 0.0_f32;
    for lane in net.lanes().iter().filter(|l| keep(l)) {
        let chords: Vec<Vector> = lane
            .center
            .points()
            .windows(2)
            .map(|w| flat(w[0], w[1]))
            .filter(|d| d.length() > 0.1)
            .map(|d| d.normalize_or(Vector::ZERO))
            .collect();
        for w in chords.windows(2) {
            worst = worst.max(w[0].dot(w[1]).clamp(-1.0, 1.0).acos());
        }
    }
    worst
}

#[test]
fn a_tight_curve_is_sampled_finely_enough_to_look_round() {
    // Road 483 turns on a 3.2 m radius. Sampled every 2 m it turned 0.6 rad
    // at a sample, which reads as a polygon. Its lanes now turn a few
    // degrees, and nowhere in the town does a lane turn anything like that.
    let (net, prov) = load_file_with_provenance(TOWN).expect("load town07");
    let on_483 = |lane: &Lane| {
        prov.lanes
            .iter()
            .any(|p| p.lane == lane.id && p.road_id == "483")
    };
    let tight = worst_turn(&net, on_483);
    assert!(tight < 0.08, "road 483 turns {tight} rad at a sample");
    let anywhere = worst_turn(&net, |_| true);
    assert!(anywhere < 0.3, "a lane turns {anywhere} rad at a sample");
}

#[test]
fn a_sharp_kink_in_the_lane_offset_folds_no_triangle() {
    // Road 82's <laneOffset> turns from -0.176 to +0.248 m/m at s = 7, so its
    // lanes turn about 24 degrees there, across two samples centimetres apart.
    let offset =
        |s: f64, a: f64, b: f64| format!(r#"<laneOffset s="{s}" a="{a}" b="{b}" c="0" d="0"/>"#);
    let lane = |id: i32| {
        format!(
            r#"<lane id="{id}" type="driving"><width sOffset="0" a="3.7" b="0" c="0" d="0"/></lane>"#
        )
    };
    let net = xodr::load_str(&format!(
        r#"<OpenDRIVE><header/><road id="82" length="20" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="-1.4363693224820402" length="20"><line/></geometry></planView>
        <lanes>{}{}<laneSection s="0"><left>{}</left><center><lane id="0" type="none"/></center>
        <right>{}{}</right></laneSection></lanes></road></OpenDRIVE>"#,
        offset(0.0, -2.5, -0.17644285714285715),
        offset(7.0, -3.7351, 0.24787142857142858),
        lane(1),
        lane(-1),
        lane(-2)
    ))
    .unwrap();
    let mesh = net.surface_mesh();
    for t in mesh.indices.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|k| mesh.vertices[t[k] as usize]);
        let twice_area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        assert!(
            twice_area >= -1e-6,
            "a folded triangle at {a:?} {b:?} {c:?}"
        );
    }
}
