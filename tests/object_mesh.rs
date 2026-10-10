//! The object mesh encloses what each object's shape says, facing out.
//!
//! The volume a closed mesh encloses is the sum over its triangles of
//! `a · (b × c) / 6`. That sum only comes out as the shape's volume when every
//! face is there and wound outward, so it checks both at once.

use xodr::{load_file, Mesh, ObjectSpan, ObjectType, Point, RoadNetwork, Shape};

const OBJECTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/objects.xodr");
const E6MINI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/e6mini.xodr");

fn triangles<'a>(mesh: &'a Mesh, span: &ObjectSpan) -> impl Iterator<Item = [Point; 3]> + 'a {
    let range = span.indices.start as usize..span.indices.end as usize;
    mesh.indices[range]
        .chunks_exact(3)
        .map(|t| [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize]))
}

fn volume(mesh: &Mesh, span: &ObjectSpan) -> f32 {
    triangles(mesh, span)
        .map(|[a, b, c]| a.to_vector().dot(b.to_vector().cross(c.to_vector())) / 6.0)
        .sum()
}

/// The span of the first object matching `pick`, if it has one.
fn span_of<'a>(
    net: &RoadNetwork,
    mesh: &'a Mesh,
    pick: impl Fn(&xodr::Object) -> bool,
) -> Option<&'a ObjectSpan> {
    let object = net.objects().iter().find(|o| pick(o)).expect("object");
    mesh.objects.iter().find(|s| s.object == object.id)
}

fn named<'a>(net: &RoadNetwork, mesh: &'a Mesh, name: &str) -> Option<&'a ObjectSpan> {
    span_of(net, mesh, |o| o.name == name)
}

fn of_kind<'a>(net: &RoadNetwork, mesh: &'a Mesh, kind: ObjectType) -> Option<&'a ObjectSpan> {
    span_of(net, mesh, |o| o.kind == kind)
}

#[test]
fn the_object_mesh_is_sound_and_its_spans_tile_it() {
    let net = load_file(OBJECTS).expect("objects.xodr loads");
    let mesh = net.object_mesh();
    mesh.validate().expect("a collider can build it");
    assert!(mesh.lanes.is_empty());
    assert_eq!(mesh.normals.len(), mesh.vertices.len());
    assert!(mesh.normals.iter().all(|n| n.is_normalized()));

    let (mut vertices, mut indices) = (0, 0);
    for span in &mesh.objects {
        assert_eq!(
            (span.vertices.start, span.indices.start),
            (vertices, indices)
        );
        (vertices, indices) = (span.vertices.end, span.indices.end);
        assert!(net.object(span.object).is_some());
    }
    assert_eq!(vertices as usize, mesh.vertices.len());
    assert_eq!(indices as usize, mesh.indices.len());

    // Each triangle's winding agrees with its vertices' normal.
    for tri in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize]);
        let n = mesh.normals[tri[0] as usize];
        assert!(
            (b - a).cross(c - a).dot(n) > 0.0,
            "triangle {tri:?} faces in"
        );
    }
}

#[test]
fn closed_shapes_enclose_their_volume() {
    let net = load_file(OBJECTS).expect("objects.xodr loads");
    let mesh = net.object_mesh();
    let near = |got: f32, want: f32, what: &str| {
        assert!(
            (got - want).abs() < want * 1e-3,
            "{what}: {got} m³, expected {want}"
        );
    };

    near(
        volume(&mesh, named(&net, &mesh, "Shed").unwrap()),
        8.0 * 4.0 * 3.0,
        "shed",
    );
    near(
        volume(&mesh, named(&net, &mesh, "Gate").unwrap()),
        4.0 * 0.2 * 1.0,
        "gate",
    );
    near(
        volume(&mesh, named(&net, &mesh, "House").unwrap()),
        6.0 * 4.0 * 5.0,
        "house",
    );
    // A 10 x 8 m block round a 4 x 3 m well, 4 m tall. The volume only
    // comes out right if the well's walls face into it.
    near(
        volume(&mesh, named(&net, &mesh, "Courtyard").unwrap()),
        (10.0 * 8.0 - 4.0 * 3.0) * 4.0,
        "courtyard",
    );
    // A 16-sided prism inscribed in the tree's radius 1.5, 7 m tall.
    let prism = 8.0 * 1.5_f32.powi(2) * (std::f32::consts::TAU / 16.0).sin() * 7.0;
    near(
        volume(&mesh, of_kind(&net, &mesh, ObjectType::Tree).unwrap()),
        prism,
        "tree",
    );

    // A 16-sided tube along 40 m of s, 16 m right of a reference line
    // curving left at radius 200, its radius growing from 0.3 m to 0.5 m.
    // Its cross-section's area grows as the square of the radius.
    let per_r2 = 8.0 * (std::f32::consts::TAU / 16.0).sin();
    let mean_r2 = (0.3_f32 * 0.3 + 0.3 * 0.5 + 0.5 * 0.5) / 3.0;
    let want = per_r2 * mean_r2 * 40.0 * (1.0 + 16.0 / 200.0);
    let got = volume(&mesh, named(&net, &mesh, "Pipe").unwrap());
    assert!(
        (got - want).abs() < want * 0.01,
        "pipe: {got} m³, expected about {want}"
    );

    // The barrier widens from 0.5 m to 0.875 m over the 30 m of it on the
    // road, 1 m tall. It runs 8.5 m right of a reference line curving left at
    // radius 200, so it is longer than its 30 m of s by a factor of about
    // 1 + 8.5 / 200.
    let barrier = net
        .objects()
        .iter()
        .find(|o| o.kind == ObjectType::Barrier && o.name.is_empty())
        .expect("barrier");
    let span = mesh
        .objects
        .iter()
        .find(|s| s.object == barrier.id)
        .unwrap();
    let want = (0.5 + 0.875) / 2.0 * 30.0 * (1.0 + 8.5 / 200.0);
    let got = volume(&mesh, span);
    assert!(
        (got - want).abs() < want * 0.01,
        "barrier: {got} m³, expected about {want}"
    );
}

#[test]
fn flat_and_open_shapes_are_single_sheets() {
    let net = load_file(OBJECTS).expect("objects.xodr loads");
    let mesh = net.object_mesh();
    let count = |span: &ObjectSpan| (span.indices.end - span.indices.start) as usize / 3;

    // The fence is two walls, one quad each, with no lid.
    assert_eq!(count(named(&net, &mesh, "Fence").unwrap()), 2 * 2);
    // The railing has no width: one wall between each two sections.
    let railing = net
        .objects()
        .iter()
        .find(|o| o.kind == ObjectType::Railing)
        .unwrap();
    let Shape::Sweep { sections, .. } = &railing.shape else {
        panic!("a sweep");
    };
    assert_eq!(
        count(of_kind(&net, &mesh, ObjectType::Railing).unwrap()),
        (sections.len() - 1) * 2
    );
}

#[test]
fn objects_with_no_volume_have_no_span() {
    let net = load_file(OBJECTS).expect("objects.xodr loads");
    let mesh = net.object_mesh();
    // The marker has no size, and the guide post a height with no footprint.
    assert!(of_kind(&net, &mesh, ObjectType::None).is_none());
    assert!(of_kind(&net, &mesh, ObjectType::Unknown).is_none());
}

#[test]
fn a_real_file_meshes_its_railings_and_not_its_bare_posts() {
    let net = load_file(E6MINI).expect("e6mini loads");
    let mesh = net.object_mesh();
    mesh.validate().expect("a collider can build it");
    // Only the two railings have any area. Every post is a height alone.
    assert_eq!(mesh.objects.len(), 2);
    assert!(mesh
        .objects
        .iter()
        .all(|s| net.object(s.object).unwrap().kind == ObjectType::Railing));
}

/// One object on a straight, flat 40 m road, and its span.
fn one_object(object: &str) -> (Mesh, ObjectSpan) {
    let xml = format!(
        r#"<OpenDRIVE><road length="40" id="1" junction="-1">
          <planView><geometry s="0" x="0" y="0" hdg="0" length="40"><line/></geometry></planView>
          <lanes><laneSection s="0"><right><lane id="-1" type="driving">
            <width sOffset="0" a="3.5"/></lane></right></laneSection></lanes>
          <objects>{object}</objects></road></OpenDRIVE>"#
    );
    let mesh = xodr::load_str(&xml).expect("loads").object_mesh();
    mesh.validate().expect("a collider can build it");
    let span = mesh.objects[0].clone();
    (mesh, span)
}

/// An `<outline>` of `cornerLocal`s at `(u, v)`, each `height` tall.
fn ring(attrs: &str, corners: &[(f32, f32)], height: f32) -> String {
    let corners: String = corners
        .iter()
        .map(|(u, v)| format!(r#"<cornerLocal u="{u}" v="{v}" height="{height}"/>"#))
        .collect();
    format!("<outline {attrs}>{corners}</outline>")
}

#[test]
fn outlines_and_holes_enclose_their_volume_whichever_way_they_run() {
    let square = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)];
    let well = [(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)];
    let reversed = |r: &[(f32, f32)]| r.iter().rev().copied().collect::<Vec<_>>();
    for outer in [square.to_vec(), reversed(&square)] {
        for hole in [well.to_vec(), reversed(&well)] {
            let (mesh, span) = one_object(&format!(
                r#"<object id="o" s="10" t="0"><outlines>{}{}</outlines></object>"#,
                ring("", &outer, 2.0),
                ring(r#"outer="false""#, &hole, 2.0),
            ));
            let got = volume(&mesh, &span);
            assert!((got - 24.0).abs() < 1e-3, "{outer:?} {hole:?}: {got} m³");
        }
    }
}

#[test]
fn an_outline_standing_on_its_edge_gets_a_lid() {
    // Pitched a right angle back, so u points up: a 2 m tall, 1 m wide
    // board, its height a 0.1 m thickness behind it.
    let board = |height| {
        one_object(&format!(
            r#"<object id="o" s="10" t="0" pitch="{}"><outlines>{}</outlines></object>"#,
            -std::f64::consts::FRAC_PI_2,
            ring(
                "",
                &[(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (0.0, 1.0)],
                height
            ),
        ))
    };
    let (mesh, span) = board(0.1);
    let got = volume(&mesh, &span);
    assert!((got - 0.2).abs() < 1e-4, "board: {got} m³");

    // With no thickness it is one sheet, its lid.
    let (mesh, span) = board(0.0);
    let area: f32 = triangles(&mesh, &span)
        .map(|[a, b, c]| (b - a).cross(c - a).length() / 2.0)
        .sum();
    assert!((area - 2.0).abs() < 1e-4, "sign: {area} m²");
}

/// A rail swept three quarters of the way round a circle has a cap at each
/// end facing away from the rail, though its ends' chord runs back against
/// its last step. Its volume, cross-section times the arc its middle runs
/// along, shows it.
#[test]
fn a_sweep_round_a_bend_caps_both_ends_outward() {
    let road = |object: &str| {
        format!(
            r#"<OpenDRIVE><road length="94.247780" id="1" junction="-1">
              <planView><geometry s="0" x="0" y="0" hdg="0" length="94.247780"><arc curvature="0.05"/></geometry></planView>
              <lanes><laneSection s="0"><right><lane id="-1" type="driving">
                <width sOffset="0" a="3.5"/></lane></right></laneSection></lanes>
              <objects>{object}</objects></road></OpenDRIVE>"#
        )
    };
    let rail = r#"<object id="rail" s="0" t="-4" zOffset="0" height="1" width="0.4">
        <repeat s="0" length="94.247780" distance="0" tStart="-4" tEnd="-4" widthStart="0.4" widthEnd="0.4"
          heightStart="1" heightEnd="1" zOffsetStart="0" zOffsetEnd="0"/></object>"#;
    let mesh = xodr::load_str(&road(rail)).unwrap().object_mesh();
    mesh.validate().expect("a collider can build it");
    let want = 0.4 * 1.0 * 1.5 * std::f32::consts::PI * 24.0;
    let got = volume(&mesh, &mesh.objects[0]);
    assert!((got - want).abs() < 0.02 * want, "{got} m³, want {want}");

    let pipe = r#"<object id="pipe" s="0" t="-4" zOffset="0" radius="0.5">
        <repeat s="0" length="94.247780" distance="0" tStart="-4" tEnd="-4" radiusStart="0.5" radiusEnd="0.5"
          zOffsetStart="0" zOffsetEnd="0"/></object>"#;
    let mesh = xodr::load_str(&road(pipe)).unwrap().object_mesh();
    let got = volume(&mesh, &mesh.objects[0]);
    assert!(
        got > 0.9 * 0.5 * 0.5 * std::f32::consts::PI * 0.9 * std::f32::consts::PI * 24.0,
        "{got} m³"
    );
}

#[test]
fn an_outline_with_no_corners_meshes_to_nothing() {
    let object = |closed: bool| xodr::Object {
        id: xodr::ObjectId(0),
        kind: ObjectType::Barrier,
        subtype: String::new(),
        name: String::new(),
        dynamic: false,
        lanes: Vec::new(),
        markings: Vec::new(),
        borders: Vec::new(),
        parking_space: None,
        materials: Vec::new(),
        user_data: Vec::new(),
        shape: Shape::Outline {
            corners: Vec::new(),
            closed,
            holes: Vec::new(),
        },
    };
    for closed in [false, true] {
        let net = RoadNetwork::new(Vec::new()).with_objects(vec![object(closed)]);
        assert!(net.object_mesh().indices.is_empty());
    }
}
