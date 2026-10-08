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
fn a_signal_is_held_up_unless_it_is_paint() {
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
    let prim = |name: &str| {
        let start = stage.find(&format!("\"{name}\"")).expect(name);
        let end = stage[start..]
            .find("def Xform \"signal_")
            .map_or(stage.len(), |i| start + i);
        &stage[start..end]
    };
    let pole = "rel xodr:supportPrim = [</Map/Supports/support_0>]";
    assert!(
        prim("SpeedLimit50").contains(pole),
        "the speed limit's pole"
    );
    assert!(prim("LorriesOnly").contains(pole), "the plate shares it");
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

#[test]
fn each_road_of_the_structures_map_gets_its_structure() {
    let stage = stage("structures");
    let kinds: Vec<&str> = stage
        .lines()
        .filter_map(|l| l.trim().strip_prefix("custom token xodr:structure = "))
        .map(|k| k.trim_matches('"'))
        .collect();
    assert_eq!(kinds, ["cantilever", "gantry", "spaceFrame", "gantry"]);
}

#[test]
fn the_gantries_map_gets_its_gantries() {
    let stage = stage("gantries");
    let kinds: Vec<&str> = stage
        .lines()
        .filter_map(|l| l.trim().strip_prefix("custom token xodr:structure = "))
        .map(|k| k.trim_matches('"'))
        .collect();
    assert_eq!(kinds, ["gantry", "spaceFrame", "gantry", "gantry"]);
}

/// The text of the prim named `name`, from its `def` line to its closing
/// brace.
fn prim<'a>(stage: &'a str, name: &str) -> &'a str {
    let at = stage.find(&format!("\"{name}\"\n")).expect("the prim");
    let line = stage[..at].rfind('\n').map_or(0, |k| k + 1);
    let indent = stage[line..at].len() - stage[line..at].trim_start().len();
    let close = format!("\n{}}}", " ".repeat(indent));
    let end = stage[at..].find(&close).expect("a closing brace");
    &stage[line..at + end + close.len()]
}

/// Each junction's OpenDRIVE id, with the names of its wrap's meshes.
fn wraps(stage: &str) -> Vec<(String, Vec<String>)> {
    let junctions = prim(stage, "Junctions");
    let mut out = Vec::new();
    for k in 0.. {
        let name = format!("junction_{k}");
        if !junctions.contains(&format!("\"{name}\"\n")) {
            break;
        }
        let junction = prim(junctions, &name);
        let id = junction.split("xodr:junction = \"").nth(1).expect("an id");
        let names = junction.split("def Mesh \"").skip(1);
        let names = names.map(|m| m[..m.find('"').expect("a name")].to_string());
        out.push((
            id[..id.find('"').expect("an id")].to_string(),
            names.collect(),
        ));
    }
    out
}

#[test]
fn each_junction_gets_a_wrap_per_lane_type() {
    let wraps = wraps(&stage("junction_wraps"));
    assert_eq!(
        wraps.len(),
        12,
        "the direct junction has no lanes of its own"
    );
    for (id, names) in wraps {
        let want = match id.as_str() {
            "1" => vec!["driving", "sidewalk"],
            "11" => vec!["border", "driving", "shoulder", "sidewalk"],
            _ => vec!["driving"],
        };
        assert_eq!(names, want, "junction {id}");
    }
}

#[test]
fn a_wrap_links_to_its_junction_lanes_of_its_type() {
    let stage = stage("junction_wraps");
    let junction = prim(&stage, "junction_0");
    assert!(junction.contains("xodr:junction = \"1\""));
    let sidewalk = prim(junction, "sidewalk");
    let line = sidewalk
        .lines()
        .find(|l| l.contains("rel xodr:lanes"))
        .expect("links to lanes");
    let targets: Vec<&str> = line.split(['<', '>']).skip(1).step_by(2).collect();
    assert_eq!(targets.len(), 4, "one corner sidewalk per corner");
    for target in targets {
        let name = target.rsplit('/').next().expect("a name");
        let lane = prim(&stage, name);
        assert!(lane.contains("xodr:laneType = \"sidewalk\""), "{target}");
    }
}

#[test]
fn a_lane_in_a_junction_is_a_prim_without_geometry() {
    let (net, _) =
        load_file_with_provenance("../tests/data/junction_wraps.xodr").expect("map loads");
    let stage = stage("junction_wraps");
    let in_junction = |lane| {
        net.road_lane(lane)
            .and_then(|at| net.road(at.road))
            .is_some_and(|r| r.junction().is_some())
    };
    let spans = net.surface_mesh().lanes;
    let (inside, outside): (Vec<_>, Vec<_>) = spans
        .iter()
        .filter(|s| !s.indices.is_empty())
        .partition(|s| in_junction(s.lane));
    assert!(!inside.is_empty());
    for span in inside {
        assert!(stage.contains(&format!("def Scope \"lane_{}\"", span.lane.0)));
    }
    assert_eq!(meshes(&stage, "lane"), outside.len());
}
