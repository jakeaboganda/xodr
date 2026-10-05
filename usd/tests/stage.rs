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

#[test]
fn every_signal_has_a_board_that_inherits_its_type_class() {
    let (net, _) = load_file_with_provenance("../tests/data/signals.xodr").expect("map loads");
    let stage = stage("signals");
    assert_eq!(
        stage.matches("def Xform \"signal_").count(),
        net.signals().len()
    );
    assert!(stage.contains("inherits = </_SignalTypes/DE_274_55>"));
    assert!(stage.contains("class Xform \"DE_274_55\""));
    assert!(stage.contains("token[] semantics:labels:opendrive = [\"DE:274:55\"]"));
}

#[test]
fn a_signal_links_to_what_it_references() {
    let stage = stage("signals");
    assert!(
        stage.contains("rel xodr:references = [</Map/Signals/signal_4>, </Map/Objects/object_0>]")
    );
    assert!(stage.contains("custom string[] xodr:referenceTypes = [\"stopline\", \"mast\"]"));
}

/// Each signal's `xodr:support`, by its `xodr:name`.
fn supports(stage: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut name = String::new();
    for line in stage.lines().map(str::trim) {
        if let Some(n) = line.strip_prefix("custom string xodr:name = ") {
            name = n.trim_matches('"').to_string();
        }
        if let Some(s) = line.strip_prefix("custom token xodr:support = ") {
            found.push((name.clone(), s.trim_matches('"').to_string()));
        }
    }
    found
}

#[test]
fn a_signal_stands_on_a_pole_unless_it_is_paint() {
    let found = supports(&stage("signals"));
    let support = |name: &str| {
        found
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.as_str())
            .unwrap_or_else(|| panic!("{name} in {found:?}"))
    };
    assert_eq!(support("Light"), "object");
    assert_eq!(support("StopLine"), "none");
    assert_eq!(support("Gantry"), "synthesized");
    assert_eq!(support("SideLight"), "synthesized");
    assert_eq!(support("SpeedLimit50"), "synthesized");
    let stage = stage("signals");
    let speed = stage.find("\"SpeedLimit50\"").expect("the speed limit");
    let plate = stage.find("\"LorriesOnly\"").expect("the plate");
    let pole = "rel xodr:supportPrim = [</Map/Supports/support_0>]";
    assert!(stage[speed..plate].contains(pole) && stage[plate..].contains(pole));
}

#[test]
fn a_signal_finds_the_pole_object_under_it() {
    let found = supports(&stage("straight_500m_signs"));
    assert_eq!(found.len(), 19);
    assert!(found.iter().all(|(_, s)| s == "object"), "{found:?}");
}

#[test]
fn a_signal_controlled_twice_is_linked_once() {
    let map = std::fs::read_to_string("../tests/data/signals.xodr").expect("map reads");
    let control = r#"<control signalId="4" type="0"/>"#;
    let map = map.replace(control, &format!("{control}{control}"));
    let (net, provenance) = xodr::load_str_with_provenance(&map).expect("map loads");
    let mut out = Vec::new();
    write_stage(&net, &provenance, &mut out).expect("stage writes");
    let stage = String::from_utf8(out).expect("utf-8");
    assert!(
        stage.contains("rel xodr:signals = [</Map/Signals/signal_3>, </Map/Signals/signal_10>]")
    );
    assert!(stage.contains("custom string[] xodr:controlTypes = [\"0\", \"0\"]"));
}

#[test]
fn a_sign_for_both_directions_has_a_board_each_way() {
    let stage = stage("signals");
    let start = stage.find("\"RoadWorks30\"").expect("the road works sign");
    let end = stage[start..]
        .find("def Xform \"signal_")
        .map_or(stage.len(), |i| start + i);
    let sign = &stage[start..end];
    assert!(sign.contains("def Xform \"board\""));
    assert!(sign.contains("def Xform \"board_back\""));
    assert_eq!(sign.matches("float xformOp:rotateZ = 180").count(), 1);
    let speed = stage.find("\"SpeedLimit50\"").expect("a one-way sign");
    assert!(!stage[speed..start].contains("board_back"));
}
