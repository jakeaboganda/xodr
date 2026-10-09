//! The importer's behavior on files it should not trust.
//!
//! `.xodr` files come from outside your program, exported by another tool or
//! handed over by whoever wants their map driven on. Everything downstream
//! (the lane polylines, the surface trimesh a physics collider is built from,
//! the routing graph) treats the imported network as sound, so this is the
//! boundary where that has to be made true.

use xodr::{
    load_file, load_file_with_provenance, load_str, load_str_with_provenance, RoadSkipReason,
    Warning,
};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/");
/// The four full maps in `tests/data`: a hand-authored demo, an esmini
/// highway, a purpose-built test track, and a city export.
const FULL_MAPS: [&str; 4] = [
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/demo.xodr"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/e6mini.xodr"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/testtrack.xodr"),
];

fn fixture(name: &str) -> xodr::RoadNetwork {
    load_file(format!("{DATA}{name}")).unwrap_or_else(|e| panic!("loading {name}: {e}"))
}

fn warnings(name: &str) -> Vec<Warning> {
    load_file_with_provenance(format!("{DATA}{name}"))
        .unwrap_or_else(|e| panic!("loading {name}: {e}"))
        .1
        .warnings
}

fn skipped(road_id: &str, reason: RoadSkipReason) -> Warning {
    Warning::RoadSkipped {
        road_id: road_id.into(),
        reason,
    }
}

#[test]
fn malformed_xml_is_an_error_not_a_panic() {
    for junk in [
        "",
        "not xml at all",
        "<OpenDRIVE>",                                 // unclosed
        "<OpenDRIVE></OpenDRIVE>",                     // well-formed, empty
        "<OpenDRIVE><road/></OpenDRIVE>",              // a road with nothing in it
        "\u{feff}<?xml version=\"1.0\"?><OpenDRIVE/>", // BOM + empty
    ] {
        assert!(
            load_str(junk).is_err(),
            "expected an error for {junk:?}, got a network"
        );
    }
    assert!(load_file(format!("{DATA}does_not_exist.xodr")).is_err());
}

#[test]
fn a_road_with_no_geometry_is_skipped_rather_than_panicking() {
    // One unusable road must not cost the other 233 in a city export.
    let net = fixture("no_geometry.xodr");
    assert_eq!(net.driving_lanes().count(), 1, "the good road was lost");
    for lane in net.lanes() {
        assert!(lane.center.points().iter().all(|p| p.is_finite()));
    }
}

#[test]
fn a_zero_length_lane_is_dropped_not_baked_into_a_degenerate_polyline() {
    let net = fixture("zero_length.xodr");
    assert_eq!(
        net.driving_lanes().count(),
        1,
        "the degenerate road survived"
    );
    for lane in net.lanes() {
        assert!(
            lane.center.points().len() >= 2,
            "lane {:?} baked to a single point",
            lane.id
        );
        let length = lane.center.length();
        assert!(
            length.is_finite() && length > 0.0,
            "lane {:?} has length {length}",
            lane.id
        );
    }
}

#[test]
fn a_non_finite_coordinate_never_reaches_the_road_network() {
    // Rust's float parser accepts "NaN" and turns an out-of-range exponent
    // into infinity, so an XML attribute carries either straight through
    // unless the importer refuses it. A NaN vertex is also what makes the
    // road's trimesh collider fail to build.
    let net = fixture("non_finite.xodr");
    for lane in net.lanes() {
        for point in lane.center.points() {
            assert!(
                point.is_finite(),
                "lane {:?} carries a non-finite point {point:?}",
                lane.id
            );
        }
        assert!(lane.width.is_finite() && lane.width > 0.0);
    }
    // And the sound road in the same file still imports.
    assert!(net.driving_lanes().count() >= 1, "the good road was lost");
}

#[test]
fn a_lane_link_to_a_nonexistent_lane_is_dropped_at_import() {
    let net = fixture("dangling_link.xodr");
    for lane in net.lanes() {
        for successor in &lane.successors {
            assert!(
                net.lane(*successor).is_some(),
                "lane {:?} has dangling successor {successor:?}",
                lane.id
            );
        }
        for predecessor in &lane.predecessors {
            assert!(net.lane(*predecessor).is_some());
        }
    }
}

#[test]
fn every_referenced_lane_id_exists() {
    // The invariant the router walks on, checked across every full map:
    // successors, predecessors, and lane-change neighbours all resolve.
    for path in FULL_MAPS {
        let net = load_file(path).unwrap_or_else(|e| panic!("loading {path}: {e}"));
        assert!(
            net.driving_lanes().count() > 0,
            "{path} has no driving lanes"
        );
        for lane in net.lanes() {
            for (kind, ids) in [
                ("successor", &lane.successors),
                ("predecessor", &lane.predecessors),
                ("neighbor", &lane.neighbors),
            ] {
                for id in ids {
                    assert!(
                        net.lane(*id).is_some(),
                        "{path}: lane {:?} has dangling {kind} {id:?}",
                        lane.id
                    );
                }
            }
        }
    }
}

#[test]
fn every_full_map_tessellates_into_a_valid_trimesh() {
    // A consumer builds one static trimesh collider from this exact mesh,
    // usually during scene setup, with nowhere to report a failure. An
    // imported map's vertices trace back to an outside file, so "the mesh is
    // generated geometry we control" only holds if this does.
    for path in FULL_MAPS {
        let net = load_file(path).unwrap_or_else(|e| panic!("loading {path}: {e}"));
        let mesh = net.surface_mesh();
        mesh.validate()
            .unwrap_or_else(|e| panic!("{path} does not tessellate: {e}"));
        assert!(
            mesh.normals.len() == mesh.vertices.len(),
            "{path}: normals and vertices disagree"
        );
    }
}

#[test]
fn a_skipped_road_raises_a_warning_naming_it() {
    assert_eq!(
        warnings("no_geometry.xodr"),
        [skipped("1", RoadSkipReason::NoGeometry)]
    );
}

#[test]
fn non_finite_geometry_and_widths_raise_warnings() {
    assert_eq!(
        warnings("non_finite.xodr"),
        [
            skipped("1", RoadSkipReason::NoGeometry),
            skipped("2", RoadSkipReason::NoGeometry),
            Warning::LaneDropped {
                road_id: "3".into(),
                section: 0,
                lane: -1,
            },
        ]
    );
}

#[test]
fn a_road_without_a_length_or_plan_view_raises_a_warning() {
    let lanes = r#"<lanes><laneSection s="0"><right>
        <lane id="-1" type="driving"><width sOffset="0" a="3"/></lane>
      </right></laneSection></lanes>"#;
    let plan = r#"<planView><geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>"#;
    let xml = format!(
        r#"<OpenDRIVE>
  <road id="a" length="inf">{plan}{lanes}</road>
  <road id="b" length="20">{lanes}</road>
  <road id="c" length="20">{plan}{lanes}</road>
</OpenDRIVE>"#
    );
    let (_, prov) = load_str_with_provenance(&xml).expect("road c loads");
    assert_eq!(
        prov.warnings,
        [
            skipped("a", RoadSkipReason::NoLength),
            skipped("b", RoadSkipReason::NoPlanView),
        ]
    );
}

#[test]
fn a_warning_reads_as_a_sentence() {
    assert_eq!(
        skipped("7", RoadSkipReason::NoPlanView).to_string(),
        r#"road "7" skipped: no <planView>"#
    );
    assert_eq!(
        Warning::LaneDropped {
            road_id: "7".into(),
            section: 2,
            lane: -3,
        }
        .to_string(),
        r#"road "7", lane section 2: lane -3 dropped, it has no usable <width> or <border>"#
    );
    assert_eq!(
        Warning::WidthAndBorder {
            road_id: "7".into(),
            section: 0,
            lane: 2,
        }
        .to_string(),
        r#"road "7", lane section 0: lane 2 has <border>s in a section with <width>s"#
    );
    assert_eq!(
        Warning::BorderWithLaneOffset {
            road_id: "7".into(),
            section: 0,
            lane: 2,
        }
        .to_string(),
        r#"road "7", lane section 0: lane 2 has <border>s under a <laneOffset>, which they ignore"#
    );
    assert_eq!(
        Warning::BorderCrossesInnerLane {
            road_id: "7".into(),
            section: 0,
            lane: -2,
            s: 30.2222,
        }
        .to_string(),
        r#"road "7", lane section 0: lane -2's <border> crosses inside the lane within it at s 30.22 m"#
    );
}

#[test]
fn a_clean_map_raises_no_warnings() {
    for path in FULL_MAPS {
        let (_, prov) =
            load_file_with_provenance(path).unwrap_or_else(|e| panic!("loading {path}: {e}"));
        let faults: Vec<_> = prov.warnings.iter().filter(|w| !w.is_ignored()).collect();
        assert_eq!(faults, Vec::<&Warning>::new(), "{path}");
    }
}

#[test]
fn a_clean_map_raises_only_the_warnings_for_what_it_holds_that_is_ignored() {
    let (_, prov) = load_file_with_provenance("tests/data/e6mini.xodr").unwrap();
    assert_eq!(
        prov.warnings,
        [Warning::Ignored {
            road_id: "0".into(),
            junction_id: String::new(),
            feature: "<userData>".into(),
            count: 15,
        }]
    );
}

#[test]
fn a_geometry_of_no_length_is_dropped_with_a_warning_not_a_panic() {
    let geometry = |s: f64, length: f64, shape: &str| {
        format!(r#"<geometry s="{s}" x="{s}" y="0" hdg="0" length="{length}">{shape}</geometry>"#)
    };
    let spiral = r#"<spiral curvStart="0" curvEnd="0.01"/>"#;
    let xodr = format!(
        r#"<OpenDRIVE><header/><road id="1" length="30" junction="-1"><planView>{}{}{}{}</planView>
        <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>
        <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
        </right></laneSection></lanes></road></OpenDRIVE>"#,
        geometry(0.0, 10.0, "<line/>"),
        geometry(10.0, -1.0, spiral),
        geometry(10.0, 20.0, "<line/>"),
        geometry(30.0, 0.0, spiral),
    );
    let (net, prov) = load_str_with_provenance(&xodr).expect("the road loads");
    assert_eq!(net.lanes().len(), 1);
    let dropped = |s: f64| Warning::GeometryDropped {
        road_id: "1".into(),
        s: Some(s),
    };
    assert_eq!(prov.warnings, vec![dropped(10.0), dropped(30.0)]);
}

#[test]
fn a_huge_length_is_refused_instead_of_exhausting_memory() {
    let road = |id: &str, length: &str, geometry: &str, section: &str| {
        format!(
            r#"<road id="{id}" length="{length}" junction="-1"><planView>
            <geometry s="0" x="0" y="{id}0" hdg="0" length="{geometry}"><spiral curvStart="0" curvEnd="0"/></geometry>
            </planView><lanes><laneSection s="{section}"><center><lane id="0" type="none"/></center><right>
            <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
            </right></laneSection></lanes></road>"#
        )
    };
    let xodr = format!(
        "<OpenDRIVE><header/>{}{}{}{}</OpenDRIVE>",
        road("1", "100", "100", "0"),
        road("2", "1e13", "1e13", "0"),
        road("3", "100", "1e13", "0"),
        road("4", "100", "100", "-1e13"),
    );
    let (net, prov) = load_str_with_provenance(&xodr).expect("the short roads load");
    assert_eq!(
        prov.warnings,
        [
            skipped("2", RoadSkipReason::TooLong),
            skipped("3", RoadSkipReason::NoGeometry),
        ]
    );
    let length = |road: &str| {
        let p = prov.lanes.iter().find(|p| p.road_id == road).unwrap();
        net.lane(p.lane).unwrap().center.length()
    };
    assert!((length("1") - 100.0).abs() < 0.1);
    assert!((length("4") - 100.0).abs() < 0.1);
}

#[test]
fn a_zero_length_lane_section_keeps_the_links_across_it() {
    let section = |s: f64, link: &str| {
        format!(
            r#"<laneSection s="{s}"><center><lane id="0" type="none"/></center><right>
            <lane id="-1" type="driving"><link>{link}</link><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
            </right></laneSection>"#
        )
    };
    let both = r#"<predecessor id="-1"/><successor id="-1"/>"#;
    let road = |id: &str, x: f64, link: &str, sections: &[f64]| {
        let sections: String = sections.iter().map(|&s| section(s, both)).collect();
        format!(
            r#"<road id="{id}" length="100" junction="-1"><link>{link}</link><planView>
            <geometry s="0" x="{x}" y="0" hdg="0" length="100"><line/></geometry>
            </planView><lanes>{sections}</lanes></road>"#
        )
    };
    let xodr = format!(
        "<OpenDRIVE><header/>{}{}</OpenDRIVE>",
        road(
            "A",
            0.0,
            r#"<successor elementType="road" elementId="B" contactPoint="start"/>"#,
            &[0.0, 50.0, 50.0, 100.0],
        ),
        road(
            "B",
            100.0,
            r#"<predecessor elementType="road" elementId="A" contactPoint="end"/>"#,
            &[0.0, 0.0, 100.0],
        ),
    );
    let (net, prov) = load_str_with_provenance(&xodr).expect("the roads load");
    let lane = |road: &str, section: usize| {
        let p = prov
            .lanes
            .iter()
            .find(|p| p.road_id == road && p.section == section);
        p.unwrap_or_else(|| panic!("road {road} section {section}"))
            .lane
    };
    let next = |id| net.lane(id).unwrap().successors.clone();
    assert_eq!(prov.lanes.len(), 3);
    assert_eq!(next(lane("A", 0)), [lane("A", 2)]);
    assert_eq!(next(lane("A", 2)), [lane("B", 1)]);
    assert!(prov.warnings.is_empty(), "{:?}", prov.warnings);
}

#[test]
fn a_lane_section_without_s_starts_at_0_and_overlaps_nothing() {
    let section = |s: &str| {
        format!(
            r#"<laneSection {s}><center><lane id="0" type="none"/></center><right>
            <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
            </right></laneSection>"#
        )
    };
    let xodr = format!(
        r#"<OpenDRIVE><header/><road id="1" length="100" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry>
        </planView><lanes>{}{}</lanes></road></OpenDRIVE>"#,
        section(r#"s="0""#),
        section(""),
    );
    let (net, prov) = load_str_with_provenance(&xodr).expect("the road loads");
    assert_eq!(net.lanes().len(), 1);
    assert!((net.lanes()[0].center.length() - 100.0).abs() < 0.01);
    assert!(prov.warnings.is_empty(), "{:?}", prov.warnings);
}

#[test]
fn a_road_whose_lanes_land_out_of_f32_range_is_skipped() {
    let road = |id: &str, y: f64, profile: &str, width: &str| {
        format!(
            r#"<road id="{id}" length="20" junction="-1">{profile}<planView>
            <geometry s="0" x="0" y="{y}" hdg="0" length="20"><line/></geometry></planView>
            <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>
            <lane id="-1" type="driving"><width sOffset="0" a="{width}" b="0" c="0" d="0"/></lane>
            </right></laneSection></lanes></road>"#
        )
    };
    let elevation =
        r#"<elevationProfile><elevation s="0" a="1e39" b="0" c="0" d="0"/></elevationProfile>"#;
    let bank =
        r#"<lateralProfile><superelevation s="0" a="1e39" b="0" c="0" d="0"/></lateralProfile>"#;
    let xodr = format!(
        "<OpenDRIVE><header/>{}{}{}{}</OpenDRIVE>",
        road("high", 0.0, elevation, "3.5"),
        road("good", 10.0, "", "3.5"),
        road("wide", 20.0, "", "1e39"),
        road("banked", 30.0, bank, "3.5"),
    );
    let (net, prov) = load_str_with_provenance(&xodr).expect("the good road loads");
    assert_eq!(
        prov.warnings,
        [
            skipped("high", RoadSkipReason::OutOfRange),
            skipped("wide", RoadSkipReason::OutOfRange),
            skipped("banked", RoadSkipReason::OutOfRange),
        ]
    );
    assert_eq!(net.lanes().len(), 1);
    assert_eq!(net.lanes()[0].id.0, 0, "lane ids stay dense");
    net.surface_mesh().validate().expect("a valid mesh");
}

#[test]
fn a_lane_with_an_unreadable_id_is_dropped_with_a_warning() {
    let lane = |id: &str| {
        format!(
            r#"<lane id="{id}" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>"#
        )
    };
    let xodr = format!(
        r#"<OpenDRIVE><header/><road id="1" length="20" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>
        <lanes><laneSection s="0"><center><lane id="0" type="none"/></center>
        <right>{}{}{}{}</right></laneSection></lanes></road></OpenDRIVE>"#,
        lane("-1"),
        lane("x"),
        lane("1.0"),
        lane("0"),
    );
    let (net, prov) = load_str_with_provenance(&xodr).unwrap();
    assert_eq!(net.lanes().len(), 1);
    let unreadable = |id: &str| Warning::LaneIdUnreadable {
        road_id: "1".into(),
        section: 0,
        id: id.into(),
    };
    assert_eq!(
        prov.warnings,
        [unreadable("x"), unreadable("1.0"), unreadable("0")]
    );
}

#[test]
fn a_lane_link_or_junction_the_crate_cannot_read_is_dropped_with_a_warning() {
    let road = |id: &str, junction: &str, link: &str, x: f64| {
        format!(
            r#"<road id="{id}" length="10" junction="{junction}"><link>{link}</link><planView>
            <geometry s="0" x="{x}" y="0" hdg="0" length="10"><line/></geometry></planView>
            <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>
            <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
            </right></laneSection></lanes></road>"#
        )
    };
    let xodr = format!(
        r#"<OpenDRIVE><header/>{}{}
        <junction id="J"><connection id="0" incomingRoad="A" connectingRoad="C" contactPoint="start">
          <laneLink from="-1.0" to="-1"/></connection></junction>
        <junction><connection id="7" incomingRoad="A" connectingRoad="C" contactPoint="start"/></junction>
        </OpenDRIVE>"#,
        road(
            "A",
            "-1",
            r#"<successor elementType="junction" elementId="J"/>"#,
            0.0
        ),
        road(
            "C",
            "J",
            r#"<predecessor elementType="road" elementId="A" contactPoint="end"/>"#,
            10.0
        ),
    );
    let (_, prov) = load_str_with_provenance(&xodr).unwrap();
    assert_eq!(
        prov.warnings,
        [
            Warning::LaneLinkDropped {
                incoming_road_id: "A".into(),
                junction_id: "J".into(),
                connection_id: "0".into(),
                from: "-1.0".into(),
                to: "-1".into(),
            },
            Warning::ConnectionDropped {
                incoming_road_id: "A".into(),
                junction_id: String::new(),
                connection_id: "7".into(),
            },
        ]
    );
}
