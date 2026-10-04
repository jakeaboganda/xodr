use xodr::load_file_with_provenance;
use xodr_usd::write_stage;

fn stage(map: &str) -> String {
    let (net, provenance) =
        load_file_with_provenance(format!("../tests/data/{map}.xodr")).expect("map loads");
    let mut out = Vec::new();
    write_stage(&net, &provenance, &mut out).expect("stage writes");
    String::from_utf8(out).expect("utf-8")
}

fn meshes(stage: &str, prefix: &str) -> usize {
    stage.matches(&format!("def Mesh \"{prefix}_")).count()
}

#[test]
fn a_stage_is_z_up_in_metres_rooted_at_map() {
    let stage = stage("demo");
    assert!(stage.starts_with("#usda 1.0\n"));
    for line in [
        "defaultPrim = \"Map\"",
        "metersPerUnit = 1",
        "upAxis = \"Z\"",
    ] {
        assert!(stage.contains(line), "{line}");
    }
}

#[test]
fn every_lane_mark_and_object_with_geometry_is_a_mesh() {
    let (net, _) = load_file_with_provenance("../tests/data/objects.xodr").expect("map loads");
    let stage = stage("objects");
    let lanes = net.surface_mesh().lanes;
    let objects = net.object_mesh().objects;
    let marks = net
        .road_marks()
        .iter()
        .filter(|m| m.lines.iter().any(|l| !l.pieces.is_empty()));
    assert_eq!(
        meshes(&stage, "lane"),
        lanes.iter().filter(|s| !s.indices.is_empty()).count()
    );
    assert_eq!(
        meshes(&stage, "object"),
        objects.iter().filter(|s| !s.indices.is_empty()).count()
    );
    assert_eq!(meshes(&stage, "mark"), marks.count());
    assert!(meshes(&stage, "object") > 0 && meshes(&stage, "mark") > 0);
}

#[test]
fn a_lane_names_its_road_and_lane_as_opendrive_does() {
    let stage = stage("demo");
    assert!(stage.contains("custom string xodr:roadId = \"1\""));
    assert!(stage.contains("custom int xodr:laneId = 1"));
    assert!(stage.contains("custom string xodr:laneType = \"driving\""));
}

#[test]
fn a_stage_records_its_schema_version() {
    let stage = stage("demo");
    let version = format!("int schemaVersion = {}", xodr_usd::SCHEMA_VERSION);
    assert!(stage.contains(&version));
}
