//! `tests/data/city.xodr`, a seaside town built by `tests/data/city.py`,
//! loads clean, and its signs, lights, junction grounds and slope come
//! through.

use std::sync::OnceLock;

use xodr::{
    load_file_with_provenance, Extent, LaneType, ObjectType, Provenance, RoadNetwork, Shape,
    SpeedLimit,
};

fn city() -> &'static (RoadNetwork, Provenance) {
    static CITY: OnceLock<(RoadNetwork, Provenance)> = OnceLock::new();
    CITY.get_or_init(|| load_file_with_provenance("tests/data/city.xodr").expect("loads"))
}

#[test]
fn the_city_loads_without_a_warning() {
    let (net, provenance) = city();
    assert_eq!(provenance.warnings, vec![]);
    net.surface_mesh().validate().expect("a valid mesh");
    net.object_mesh().validate().expect("a valid object mesh");
}

#[test]
fn every_city_street_takes_the_limit_of_its_signs() {
    let (net, _) = city();
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
fn every_city_light_has_one_controller_and_every_junction_its_ground() {
    let (net, _) = city();
    let lights: Vec<_> = net.signals().iter().filter(|s| s.dynamic).collect();
    assert!(lights.len() > 50, "{} lights", lights.len());
    assert!(lights.iter().all(|s| s.controllers.len() == 1));
    assert!(net.controllers().iter().all(|c| !c.signals.is_empty()));
    let areas = net.junction_areas();
    assert!(areas.len() > 20);
    assert!(areas
        .iter()
        .all(|a| a.grid.is_some() && !a.boundary.is_empty()));
    let give_way = net
        .signals()
        .iter()
        .filter(|s| s.country == "DE" && s.kind == "205")
        .count();
    assert_eq!(give_way, 5, "one at each entry of the roundabout");
}

#[test]
fn the_city_has_trams_parking_and_buildings_on_rising_land() {
    let (net, _) = city();
    let has = |kind| net.lanes().iter().any(|l| l.kind == kind);
    assert!(has(LaneType::Tram) && has(LaneType::Parking) && has(LaneType::Median));
    let count = |kind| net.objects().iter().filter(|o| o.kind == kind).count();
    assert!(count(ObjectType::Building) > 250);
    assert_eq!(
        count(ObjectType::Tree),
        count(ObjectType::Vegetation),
        "a crown a trunk"
    );
    let mesh = net.surface_mesh();
    let top = mesh.vertices.iter().map(|p| p.z).fold(f32::MIN, f32::max);
    let bottom = mesh.vertices.iter().map(|p| p.z).fold(f32::MAX, f32::min);
    assert!(top - bottom > 10.0, "the land rises from {bottom} to {top}");
}

#[test]
fn no_city_tree_stands_in_a_fountain_or_a_monument() {
    let (net, _) = city();
    let round: Vec<_> = (net.objects().iter())
        .filter_map(|o| match &o.shape {
            Shape::Solid {
                position,
                extent: Some(Extent::Cylinder { radius, .. }),
                ..
            } => Some((position.x, position.y, *radius, o)),
            _ => None,
        })
        .collect();
    let landmarks = round
        .iter()
        .filter(|r| ["fountain", "monument"].contains(&r.3.subtype.as_str()));
    for (x, y, radius, landmark) in landmarks {
        let trees = (round.iter())
            .filter(|r| matches!(r.3.kind, ObjectType::Tree | ObjectType::Vegetation));
        for (tx, ty, tr, tree) in trees {
            let apart = (x - tx).hypot(y - ty);
            assert!(apart >= radius + tr, "{:?} in {:?}", tree.id, landmark.id);
        }
    }
}
