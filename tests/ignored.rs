//! The features the crate ignores raise a `Warning::Ignored`, once per
//! feature for each road, junction, or the rest of the document.
//!
//! `tests/data/ignored.py` writes `ignored.xodr`, which uses each feature in
//! counts that tell the uses apart, beside the values that look like them
//! and mean nothing. Road 4 holds only those, and must raise nothing.

use xodr::{load_file_with_provenance, load_str_with_provenance, Warning};

/// `(road, junction, feature, count)` of every `Ignored` warning, in order.
fn ignored(path: &str) -> Vec<(String, String, String, usize)> {
    let (_, provenance) = load_file_with_provenance(path).expect("the fixture loads");
    provenance
        .warnings
        .into_iter()
        .filter_map(|w| match w {
            Warning::Ignored {
                road_id,
                junction_id,
                feature,
                count,
            } => Some((road_id, junction_id, feature, count)),
            _ => None,
        })
        .collect()
}

fn of(road: &str, junction: &str, feature: &str, count: usize) -> (String, String, String, usize) {
    (road.into(), junction.into(), feature.into(), count)
}

#[test]
fn every_ignored_feature_is_counted_where_the_file_uses_it() {
    let want = vec![
        of("", "", "<userData>", 1),
        of("", "", "<include>", 1),
        of("", "", "<dataQuality>", 1),
        of("", "", "<license>", 1),
        of("", "", "<defaultRegulations>", 1),
        of("", "", "<vmsGroup>", 1),
        of("", "100", "<laneLink fromLayer or toLayer>", 2),
        of("", "100", "<junction> <objects>", 1),
        of("", "100", "<roadSection>", 2),
        of("", "100", "<segment transitionLength>", 2),
        of("1", "", "<userData>", 3),
        of("1", "", "<include>", 1),
        of("1", "", "a second <lanes>", 1),
        of("1", "", "<lanes layer=\"temporary\">", 1),
        of("1", "", "<laneSection singleSide=\"true\">", 1),
        of("1", "", "<predecessor layer>", 2),
        of("1", "", "<successor layer>", 1),
        of("1", "", "<validity layer>", 1),
        of("2", "", "<userData>", 1),
        of("2", "", "<object perpToRoad=\"true\">", 2),
        of("2", "", "<repeat bT, cT or dT>", 2),
        of("2", "", "<repeat detachFromReferenceLine=\"true\">", 1),
        of("2", "", "<object> <surface>", 1),
        of("2", "", "<object> <skeleton>", 1),
        of("2", "", "<curveLocal>", 1),
        of("2", "", "<outline> <markings>", 2),
        of("3", "", "<userData>", 1),
        of("3", "", "<sign> <validity>", 2),
        of("3", "", "<sign> <dependency>", 2),
        of("3", "", "<sign> <reference>", 2),
        of("5", "", "<userData>", 500),
    ];
    assert_eq!(ignored("tests/data/ignored.xodr"), want);
}

#[test]
fn values_that_mean_nothing_raise_no_warning() {
    let found = ignored("tests/data/ignored.xodr");
    assert!(
        found.iter().all(|(road, ..)| road != "4"),
        "road 4 uses only values that mean nothing: {found:?}"
    );
}

#[test]
fn a_feature_is_charged_to_the_road_that_uses_it() {
    // A junction's <objects> and a road's are different elements: only the
    // junction's is ignored.
    let found = ignored("tests/data/ignored.xodr");
    let objects: Vec<_> = found
        .iter()
        .filter(|(.., f, _)| f.contains("<objects>"))
        .collect();
    assert_eq!(objects, [&of("", "100", "<junction> <objects>", 1)]);
}

#[test]
fn a_second_lanes_is_ignored_even_before_it_is_marked_temporary() {
    let xml = r#"<OpenDRIVE>
      <road length="20" id="1" junction="-1">
        <planView><geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>
        <lanes><laneSection s="0"><right><lane id="-1" type="driving"><width sOffset="0" a="3"/></lane></right></laneSection></lanes>
        <lanes><laneSection s="0"><right><lane id="-1" type="driving"><width sOffset="0" a="3"/></lane></right></laneSection></lanes>
      </road></OpenDRIVE>"#;
    let (_, provenance) = load_str_with_provenance(xml).unwrap();
    let features: Vec<_> = provenance
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::Ignored { feature, .. } => Some(feature.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(features, ["a second <lanes>"]);
}

#[test]
fn the_message_names_the_place_and_the_count() {
    let (_, provenance) = load_file_with_provenance("tests/data/ignored.xodr").unwrap();
    let messages: Vec<String> = provenance
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::Ignored { .. }))
        .map(|w| w.to_string())
        .collect();
    assert!(messages.contains(&"road \"5\": <userData> ignored, 500 uses".to_string()));
    assert!(messages.contains(&"the document: <vmsGroup> ignored, 1 use".to_string()));
    assert!(messages.contains(&"junction \"100\": <roadSection> ignored, 2 uses".to_string()));
}

#[test]
fn road_id_names_the_road_or_nothing() {
    let (_, provenance) = load_file_with_provenance("tests/data/ignored.xodr").unwrap();
    for w in &provenance.warnings {
        if let Warning::Ignored {
            road_id,
            junction_id,
            ..
        } = w
        {
            assert_eq!(w.road_id(), road_id);
            assert!(road_id.is_empty() || junction_id.is_empty());
        }
    }
}
