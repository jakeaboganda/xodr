//! Edge cases for `Mesh::height_at`, the vertical-ray surface sampler that
//! drapes a body onto the meshed road.
//!
//! These pin the corners the inline unit tests don't: a query exactly on a
//! shared edge/vertex, dead-centre of a quad, just outside, a degenerate/empty
//! mesh, overlapping (stacked) surfaces, and the returned normal being a unit
//! up-vector for either winding order.
//!
//! Companion to the inline tests in `src/mesh.rs`.

use xodr::{Mesh, Point, Vector};

/// A single flat quad at height `z` spanning x∈[0,2], y∈[0,2], split on the
/// (0,0)-(2,2) diagonal. `flip` reverses the winding of both triangles.
fn flat_quad(z: f32, flip: bool) -> Mesh {
    let vertices = vec![
        Point::new(0.0, 0.0, z), // 0
        Point::new(2.0, 0.0, z), // 1
        Point::new(2.0, 2.0, z), // 2
        Point::new(0.0, 2.0, z), // 3
    ];
    let indices = if flip {
        vec![0, 2, 1, 0, 3, 2]
    } else {
        vec![0, 1, 2, 0, 2, 3]
    };
    Mesh {
        vertices,
        normals: vec![Vector::Z; 4],
        indices,
        ..Default::default()
    }
}

#[test]
fn dead_centre_of_a_quad_resolves() {
    let mesh = flat_quad(3.0, false);
    // (1,1) is the quad centre. It lies on the shared diagonal, so it must be
    // claimed by (at least) one triangle, not fall through the split.
    let (z, n) = mesh.height_at(1.0, 1.0).expect("centre of the quad");
    assert!((z - 3.0).abs() < 1e-5, "z {z}");
    assert!(n.abs_diff_eq(Vector::Z, 1e-6), "n {n:?}");
}

#[test]
fn a_point_on_a_shared_edge_and_on_a_vertex_resolves() {
    let mesh = flat_quad(1.0, false);
    // A point squarely on the shared diagonal edge (not the midpoint), inside
    // the barycentric tolerance of both triangles.
    assert!(
        mesh.height_at(0.5, 0.5).is_some(),
        "a point on the shared edge must resolve, not fall through the crack"
    );
    // Exactly on a shared vertex.
    assert!(
        mesh.height_at(2.0, 2.0).is_some(),
        "a shared vertex must resolve"
    );
    // On an outer edge midpoint.
    assert!(
        mesh.height_at(1.0, 0.0).is_some(),
        "outer edge must resolve"
    );
}

#[test]
fn a_point_just_outside_returns_none() {
    let mesh = flat_quad(0.0, false);
    // Comfortably clear of the quad footprint on every side.
    assert!(mesh.height_at(2.5, 1.0).is_none(), "past +x edge");
    assert!(mesh.height_at(-0.5, 1.0).is_none(), "past -x edge");
    assert!(mesh.height_at(1.0, 2.5).is_none(), "past +y edge");
    assert!(mesh.height_at(1.0, -0.5).is_none(), "past -y edge");
}

#[test]
fn a_degenerate_or_empty_mesh_returns_none() {
    // Empty: no triangles at all.
    assert!(Mesh::default().height_at(0.0, 0.0).is_none());

    // Degenerate: a triangle with zero XY footprint (all three vertices on a
    // vertical line) has an edge-on projection and must be skipped, not
    // divide-by-zero into a bogus hit.
    let edge_on = Mesh {
        vertices: vec![
            Point::new(1.0, 1.0, 0.0),
            Point::new(1.0, 1.0, 5.0),
            Point::new(1.0, 1.0, 2.0),
        ],
        normals: vec![Vector::Z; 3],
        indices: vec![0, 1, 2],
        ..Default::default()
    };
    assert!(edge_on.height_at(1.0, 1.0).is_none());
}

#[test]
fn overlapping_surfaces_return_the_higher_one() {
    // Two stacked quads over the same XY footprint (a bridge over a road): the
    // sampler must return the surface you'd stand on, the higher one.
    let mut low = flat_quad(0.0, false);
    let high = flat_quad(5.0, false);
    let base = low.vertices.len() as u32;
    low.vertices.extend(high.vertices);
    low.normals.extend(high.normals);
    low.indices.extend(high.indices.iter().map(|i| i + base));

    let (z, _) = low.height_at(1.0, 0.5).expect("both quads cover the point");
    assert!(
        (z - 5.0).abs() < 1e-5,
        "should pick the higher surface, got {z}"
    );

    // Order-independence: stack them the other way and still get the higher.
    let mut high_first = flat_quad(5.0, false);
    let low2 = flat_quad(0.0, false);
    let base = high_first.vertices.len() as u32;
    high_first.vertices.extend(low2.vertices);
    high_first.normals.extend(low2.normals);
    high_first
        .indices
        .extend(low2.indices.iter().map(|i| i + base));
    let (z, _) = high_first.height_at(1.0, 0.5).expect("covered");
    assert!(
        (z - 5.0).abs() < 1e-5,
        "higher regardless of order, got {z}"
    );
}

#[test]
fn the_returned_normal_is_a_unit_up_vector_for_both_windings() {
    for flip in [false, true] {
        let mesh = flat_quad(2.0, flip);
        let (_, n) = mesh.height_at(1.5, 0.5).expect("on the quad");
        assert!(
            (n.length() - 1.0).abs() < 1e-6,
            "normal {n:?} not unit (flip={flip})"
        );
        assert!(n.z > 0.0, "normal {n:?} must point up (flip={flip})");
        // A flat quad's normal is exactly +Z whichever way it is wound.
        assert!(
            n.abs_diff_eq(Vector::Z, 1e-6),
            "flat normal {n:?} should be +Z (flip={flip})"
        );
    }
}

#[test]
fn a_tilted_surface_returns_its_interpolated_leaning_normal_for_both_windings() {
    // height_at returns the mesh's own (interpolated) vertex normals, not a flat
    // per-facet normal, so a body draped on it doesn't snap at facet edges. A
    // surface tilted about the X axis carries a leaning up-normal at every vertex;
    // the query must return that lean, unit and upward, whichever way it is wound.
    let vertices = vec![
        Point::new(0.0, 0.0, 0.0),
        Point::new(2.0, 0.0, 0.0),
        Point::new(2.0, 2.0, 1.0),
        Point::new(0.0, 2.0, 1.0),
    ];
    // The surface's true up-normal (perpendicular to the plane, pointing up).
    let up = (vertices[1] - vertices[0])
        .cross(vertices[3] - vertices[0])
        .normalize_or_zero();
    let up = if up.z < 0.0 { -up } else { up };
    assert!(up.z < 0.999, "the test surface should actually tilt");
    for flip in [false, true] {
        let indices = if flip {
            vec![0, 2, 1, 0, 3, 2]
        } else {
            vec![0, 1, 2, 0, 2, 3]
        };
        let mesh = Mesh {
            vertices: vertices.clone(),
            normals: vec![up; 4],
            indices,
            ..Default::default()
        };
        let (_, n) = mesh.height_at(1.0, 1.0).expect("on the tilted surface");
        assert!(
            (n.length() - 1.0).abs() < 1e-6,
            "tilted normal {n:?} not unit (flip={flip})"
        );
        assert!(n.z > 0.0, "tilted normal {n:?} must point up (flip={flip})");
        assert!(
            n.abs_diff_eq(up, 1e-5),
            "should return the interpolated vertex normal {up:?}, got {n:?} (flip={flip})"
        );
    }
}

#[test]
fn an_extreme_query_returns_none_rather_than_a_nan_height() {
    // The barycentric arithmetic overflows to infinity out here, and every
    // comparison against the resulting NaN is false, so an outside-the-
    // triangle test written as `l < -eps` waves it through and reports a NaN
    // height, which lands in whatever is being draped onto the road.
    let mesh = flat_quad(1.0, false);
    for bad in [f32::MAX, f32::MIN, 1.0e30, -1.0e30, f32::INFINITY, f32::NAN] {
        for (x, y) in [(bad, 1.0), (1.0, bad), (bad, bad)] {
            let hit = mesh.height_at(x, y);
            assert!(
                hit.is_none_or(|(z, n)| z.is_finite() && n.is_finite()),
                "height_at({x}, {y}) returned {hit:?}"
            );
        }
    }
}

/// The surface normal of a straight road climbing a 10 % grade leans back
/// against the climb, as the face it lies on does.
#[test]
fn the_normal_on_a_climbing_road_tilts_with_the_grade() {
    let xodr = r#"<OpenDRIVE><header/><road id="1" length="100" junction="-1">
        <planView><geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>
        <elevationProfile><elevation s="0" a="0" b="0.1" c="0" d="0"/></elevationProfile>
        <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>
        <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
        </right></laneSection></lanes></road></OpenDRIVE>"#;
    let mesh = xodr::load_str(xodr).unwrap().surface_mesh();
    let pitch = 0.1_f32.atan();
    let want = Vector::new(-pitch.sin(), 0.0, pitch.cos());
    let (z, up) = mesh.height_at(50.0, -1.75).expect("on the road");
    assert!((z - 5.0).abs() < 1e-3, "z {z}");
    assert!((up - want).length() < 1e-4, "up {up:?}, want {want:?}");
}

#[test]
fn validate_rejects_a_triangle_with_no_area_or_too_few_corners() {
    use xodr::MeshError;
    let mesh = |indices: Vec<u32>| Mesh {
        vertices: vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(2.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
        ],
        normals: vec![Vector::Z; 4],
        indices,
        lanes: Vec::new(),
        objects: Vec::new(),
    };
    assert_eq!(mesh(vec![0, 1, 3]).validate(), Ok(()));
    assert_eq!(
        mesh(vec![0, 1, 2]).validate(),
        Err(MeshError::DegenerateTriangle(0)),
        "corners in a line"
    );
    assert_eq!(
        mesh(vec![0, 1, 3, 0]).validate(),
        Err(MeshError::DegenerateTriangle(1)),
        "a leftover index"
    );
}

#[test]
fn a_lane_opening_out_of_nothing_tessellates_into_a_valid_mesh() {
    let net = xodr::load_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/lane_borders.xodr"
    ))
    .unwrap();
    net.surface_mesh()
        .validate()
        .expect("no triangle without area");
}

#[test]
fn a_triangle_naming_a_missing_vertex_covers_nothing() {
    let mesh = Mesh {
        vertices: vec![
            Point::new(0.0, 0.0, 1.0),
            Point::new(2.0, 0.0, 1.0),
            Point::new(0.0, 2.0, 1.0),
        ],
        normals: vec![Vector::Z; 3],
        indices: vec![9, 9, 9, 0, 1, 2],
        lanes: Vec::new(),
        objects: Vec::new(),
    };
    let (z, _) = mesh.height_at(0.5, 0.5).expect("the good triangle");
    assert_eq!(z, 1.0);
    let (z, _) = mesh
        .sampler()
        .height_at(0.5, 0.5)
        .expect("the good triangle");
    assert_eq!(z, 1.0);
}

/// `MeshSampler` answers what `Mesh::height_at` answers, at the edges too:
/// a point a hair outside a triangle, and one on the far edge of the mesh.
#[test]
fn the_sampler_agrees_with_height_at_on_the_edges() {
    let mesh = Mesh {
        vertices: vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
        ],
        normals: vec![Vector::Z; 3],
        indices: vec![0, 1, 2],
        lanes: Vec::new(),
        objects: Vec::new(),
    };
    let outside = (-5e-5, 0.5);
    assert!(mesh.height_at(outside.0, outside.1).is_some());
    assert!(mesh.sampler().height_at(outside.0, outside.1).is_some());

    let length = 1000.3_f32;
    let quads = 1000;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for k in 0..=quads {
        let x = length * k as f32 / quads as f32;
        vertices.extend([Point::new(x, 0.0, 0.0), Point::new(x, 1.0, 0.0)]);
        if k > 0 {
            let b = 2 * (k as u32 - 1);
            indices.extend([b, b + 2, b + 3, b, b + 3, b + 1]);
        }
    }
    let strip = Mesh {
        normals: vec![Vector::Z; vertices.len()],
        vertices,
        indices,
        lanes: Vec::new(),
        objects: Vec::new(),
    };
    let sampler = strip.sampler();
    let far = strip.vertices.last().unwrap().x;
    for y in [0.0, 0.25, 0.5, 0.75, 1.0] {
        assert_eq!(
            sampler.height_at(far, y).is_some(),
            strip.height_at(far, y).is_some(),
            "x {far} y {y}"
        );
    }
}

#[test]
fn the_sampler_finds_a_point_just_past_a_long_thin_triangle() {
    let mut vertices = vec![
        Point::new(0.0, 0.0, 1.0),
        Point::new(100.0, 0.0, 1.0),
        Point::new(100.0, 1.0, 1.0),
    ];
    let mut indices = vec![0, 1, 2];
    for k in 1..=200u32 {
        let x = 0.5 * k as f32;
        vertices.extend([
            Point::new(x, 50.0, 0.0),
            Point::new(x + 0.3, 50.0, 0.0),
            Point::new(x + 0.3, 51.0, 0.0),
        ]);
        indices.extend([3 * k, 3 * k + 1, 3 * k + 2]);
    }
    let mesh = Mesh {
        normals: vec![Vector::Z; vertices.len()],
        vertices,
        indices,
        ..Default::default()
    };
    let (x, y) = (-0.015, -0.000075);
    assert!(mesh.height_at(x, y).is_some());
    assert_eq!(
        mesh.sampler().height_at(x, y).map(|h| h.0),
        mesh.height_at(x, y).map(|h| h.0)
    );
}
