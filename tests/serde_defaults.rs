//! A network or provenance serialized before the descriptive attributes
//! were added still loads, with each new field at its default.
#![cfg(feature = "serde")]

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use xodr::{
    load_file_with_provenance, BoardSign, Header, LaneAdvisory, LaneProvenance, Marking, Material,
    Object, ObjectProvenance, Orientation, RoadMarkProvenance, RoadMarkWeight, SignalBoard, Switch,
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
fn an_object_without_its_new_flags_is_neither_invalidated_nor_temporary() {
    let (net, prov) = full();
    let flagged = net
        .objects()
        .iter()
        .find(|o| o.invalidated && o.temporary)
        .expect("an object with both");
    let old: Object = without(flagged, &["invalidated", "temporary"]);
    assert!(!old.invalidated && !old.temporary);
    assert_eq!(old.kind, flagged.kind);
    assert!(!prov.objects.is_empty());
}

#[test]
fn a_marking_and_a_material_without_their_new_fields_read_as_before() {
    let (net, _) = full();
    let marked = net
        .objects()
        .iter()
        .find(|o| !o.markings.is_empty())
        .unwrap();
    let bold: &Marking = marked
        .markings
        .iter()
        .find(|m| m.weight == RoadMarkWeight::Bold)
        .expect("a bold marking");
    assert_eq!(without(bold, &["weight"]).weight, RoadMarkWeight::Standard);
    let painted: &Material = net
        .objects()
        .iter()
        .flat_map(|o| &o.materials)
        .find(|m| !m.road_mark_color.is_empty())
        .expect("a material with a mark colour");
    assert_eq!(without(painted, &["road_mark_color"]).road_mark_color, "");
}

#[test]
fn a_sign_without_its_new_fields_has_the_defaults() {
    let (net, _) = full();
    let sign: &BoardSign = net
        .signals()
        .iter()
        .flat_map(|s| &s.boards)
        .find_map(|b| match b {
            SignalBoard::Static(signs) => signs.first(),
            _ => None,
        })
        .unwrap();
    assert!(sign.dynamic && sign.invalidated && sign.temporary);
    let old: BoardSign = without(
        sign,
        &[
            "id",
            "country_revision",
            "dynamic",
            "invalidated",
            "temporary",
            "orientation",
            "h_offset",
            "pitch",
            "roll",
            "length",
        ],
    );
    assert_eq!(old.id, "");
    assert_eq!(old.orientation, Orientation::Both);
    assert!(!old.dynamic && !old.invalidated && !old.temporary);
    assert_eq!(
        (old.h_offset, old.pitch, old.roll, old.length),
        (0.0, 0.0, 0.0, None)
    );
    assert_eq!(old.kind, sign.kind);
}

#[test]
fn a_switch_without_its_partner_name_has_none() {
    let (net, _) = load_file_with_provenance("tests/data/railways.xodr").unwrap();
    let switch: &Switch = &net.switches()[0];
    assert_eq!(switch.partner_name, "Crossover end");
    assert_eq!(without(switch, &["partner_name"]).partner_name, "");
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
    let old = without(object, &["fill_type", "outline_lane_type"]);
    assert_eq!((old.fill_type.as_str(), old.outline_lane_type), ("", None));

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
