//! `tests/data/city.xodr`, a small town built by `tests/data/city.py`,
//! loads clean, and its signs, lights and junction grounds come through.

use xodr::{load_file_with_provenance, ObjectType, SpeedLimit};

#[test]
fn the_city_loads_without_a_warning() {
    let (net, provenance) = load_file_with_provenance("tests/data/city.xodr").expect("loads");
    assert_eq!(provenance.warnings, vec![]);
    net.surface_mesh().validate().expect("a valid mesh");
    net.object_mesh().validate().expect("a valid object mesh");
}

#[test]
fn every_city_street_takes_the_limit_of_its_signs() {
    let (net, _) = load_file_with_provenance("tests/data/city.xodr").expect("loads");
    let kmh = |l: &SpeedLimit| match l {
        SpeedLimit::Max(v) => (v * 3.6).round() as i32,
        SpeedLimit::Unlimited => 0,
    };
    let mut limits: Vec<i32> = net.speed_limits().iter().map(|a| kmh(&a.value)).collect();
    limits.sort();
    limits.dedup();
    assert_eq!(limits, vec![30, 50]);
}

#[test]
fn each_city_crossroads_has_lights_controllers_and_ground() {
    let (net, _) = load_file_with_provenance("tests/data/city.xodr").expect("loads");
    let lights = net.signals().iter().filter(|s| s.dynamic).count();
    assert_eq!(lights, 24, "a light on each arm of six crossroads");
    assert_eq!(net.controllers().len(), 12, "two phases a crossroads");
    assert!(net.controllers().iter().all(|c| c.signals.len() == 2));
    let areas = net.junction_areas();
    assert_eq!(areas.len(), 6);
    assert!(areas
        .iter()
        .all(|a| a.grid.is_some() && !a.boundary.is_empty()));
    let count = |kind| net.objects().iter().filter(|o| o.kind == kind).count();
    assert!(count(ObjectType::Building) > 50);
    assert_eq!(
        count(ObjectType::Tree),
        count(ObjectType::Vegetation),
        "a crown a trunk"
    );
}
