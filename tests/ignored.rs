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
        of("", "100", "<junction> <objects>", 1),
        of("", "100", "<roadSection>", 2),
        of("", "100", "<segment transitionLength>", 2),
        of("1", "", "<userData>", 3),
        of("1", "", "<include>", 1),
        of("1", "", "a repeated <lanes> of one layer", 1),
        of("2", "", "<userData>", 1),
        of("2", "", "<object> <surface>", 1),
        of("2", "", "<object> <skeleton>", 1),
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
fn a_repeated_lanes_of_one_layer_is_ignored() {
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
    assert_eq!(features, ["a repeated <lanes> of one layer"]);
}

#[test]
fn only_the_ignored_warnings_say_they_are() {
    let (_, provenance) = load_file_with_provenance("tests/data/ignored.xodr").unwrap();
    let ignored = provenance
        .warnings
        .iter()
        .filter(|w| w.is_ignored())
        .count();
    assert_eq!(ignored, 20);
    assert!(
        provenance.warnings.len() > ignored,
        "the fixture's boundary doesn't close, which is a fault"
    );
    assert!(provenance
        .warnings
        .iter()
        .all(|w| w.is_ignored() == matches!(w, Warning::Ignored { .. })));
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

#[test]
fn what_a_vendor_puts_inside_user_data_counts_for_nothing() {
    let found = ignored("tests/data/ignored.xodr");
    let road5: Vec<_> = found.iter().filter(|f| f.0 == "5").collect();
    assert_eq!(road5, [&of("5", "", "<userData>", 500)]);
}

#[test]
fn a_road_or_junction_without_an_id_is_named_so() {
    let xml = r#"<OpenDRIVE>
      <road length="20" junction="-1">
        <planView><geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>
        <lanes><laneSection s="0"><right><lane id="-1" type="driving"><width sOffset="0" a="3"/></lane></right></laneSection></lanes>
        <userData code="a"/>
      </road>
      <junction><userData code="b"/></junction>
      <userData code="c"/>
    </OpenDRIVE>"#;
    let (_, provenance) = load_str_with_provenance(xml).unwrap();
    let messages: Vec<String> = provenance
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::Ignored { .. }))
        .map(|w| w.to_string())
        .collect();
    assert_eq!(
        messages,
        [
            "the document: <userData> ignored, 1 use",
            "junction \"(no id)\": <userData> ignored, 1 use",
            "road \"(no id)\": <userData> ignored, 1 use",
        ]
    );
}
