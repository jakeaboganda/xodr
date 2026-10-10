//! A network or provenance serialized before the descriptive attributes
//! were added still loads, with each new field at its default.
#![cfg(feature = "serde")]

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use xodr::{
    load_file_with_provenance, Header, LaneAdvisory, LaneProvenance, ObjectProvenance, Partner,
    RoadMarkProvenance,
};

/// `value` as an older version wrote it, without `removed`, loaded again.
fn without<T: Serialize + DeserializeOwned>(value: &T, removed: &[&str]) -> T {
    let mut json = serde_json::to_value(value).unwrap();
    for key in removed {
        let gone = json.as_object_mut().unwrap().remove(*key);
        assert!(gone.is_some(), "the value has no field {key:?} to remove");
    }
    serde_json::from_value::<T>(json).unwrap()
}

fn full() -> (xodr::RoadNetwork, xodr::Provenance) {
    load_file_with_provenance("tests/data/metadata.xodr").unwrap()
}

#[test]
fn a_partner_without_its_name_has_none() {
    let (net, _) = load_file_with_provenance("tests/data/railways.xodr").unwrap();
    let partner: &Partner = net.switches()[0].partner.as_ref().unwrap();
    assert_eq!(partner.name, "Crossover end");
    let old = without(partner, &["name"]);
    assert_eq!((old.id.as_str(), old.name.as_str()), ("8", ""));
}

#[test]
fn provenance_records_without_their_new_fields_read_as_before() {
    let (_, prov) = full();
    let lane: &LaneProvenance = prov.lanes.iter().find(|l| l.road_works).unwrap();
    let old = without(
        lane,
        &[
            "advisory",
            "dynamic_lane_type",
            "dynamic_lane_direction",
            "road_works",
        ],
    );
    assert_eq!(old.advisory, LaneAdvisory::None);
    assert!(!old.road_works && !old.dynamic_lane_type && !old.dynamic_lane_direction);

    let object: &ObjectProvenance = prov
        .objects
        .iter()
        .find(|o| !o.fill_type.is_empty())
        .unwrap();
    let old = without(
        object,
        &[
            "fill_type",
            "outline_lane_type",
            "material_road_mark_colors",
            "marking_weights",
            "invalidated",
            "temporary",
            "junction_id",
        ],
    );
    assert!(old.material_road_mark_colors.is_empty() && old.marking_weights.is_empty());
    assert_eq!((old.fill_type.as_str(), old.outline_lane_type), ("", None));
    assert!(!old.invalidated && !old.temporary);
    assert_eq!(old.junction_id, "");

    let signal = prov.signals.iter().find(|s| !s.signs.is_empty()).unwrap();
    let old = without(signal, &["signs"]);
    assert!(old.signs.is_empty());

    let mark: &RoadMarkProvenance = prov
        .road_marks
        .iter()
        .find(|m| !m.material.is_empty())
        .unwrap();
    let old = without(mark, &["material", "type_name"]);
    assert_eq!((old.material.as_str(), old.type_name.as_str()), ("", ""));
}

#[test]
fn a_header_written_before_it_gained_a_field_still_loads() {
    let header: Header = serde_json::from_value(serde_json::json!({ "name": "old" })).unwrap();
    assert_eq!(header.name, "old");
    assert_eq!(header.rev_major, None);
    let empty: Header = serde_json::from_value(Value::Object(Default::default())).unwrap();
    assert_eq!(empty, Header::default());
}

#[test]
fn a_lane_without_a_layer_is_on_the_permanent_one() {
    let (net, prov) = load_file_with_provenance("tests/data/lane_layers.xodr").unwrap();
    let temporary = net
        .lanes()
        .iter()
        .find(|l| l.layer == xodr::Layer::Temporary)
        .unwrap();
    assert_eq!(without(temporary, &["layer"]).layer, xodr::Layer::Permanent);
    let record = prov
        .lanes
        .iter()
        .find(|p| p.layer == xodr::Layer::Temporary)
        .unwrap();
    assert_eq!(without(record, &["layer"]).layer, xodr::Layer::Permanent);
}
