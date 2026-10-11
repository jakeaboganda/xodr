//! Road coordinates: `RoadNetwork::road_point` turns `(road, s, t)` into a
//! point on the road surface, and `road_position` turns a point back.

use xodr::{
    load_file, load_file_with_provenance, load_str, Point, RoadId, RoadNetwork, RoadPosition,
};

/// Every committed map with road geometry of each kind: lines, arcs,
/// spirals, paramPoly3, lane offsets, superelevation, lane heights, lateral
/// shapes and borders.
const MAPS: [&str; 10] = [
    "town07",
    "e6mini",
    "testtrack",
    "spiral",
    "banked_sweeper",
    "lane_heights",
    "lateral_shapes",
    "lane_borders",
    "level_lanes",
    "signals",
];

fn map(name: &str) -> RoadNetwork {
    load_file(format!("tests/data/{name}.xodr")).expect("the map loads")
}

/// A millimetre, the round trip's budget.
const MM: f32 = 1e-3;

/// A lane's first and last points are left out. Where the surface steps at
/// a lane section seam, the seam belongs to the section that starts there.
#[test]
fn a_point_on_every_lane_round_trips_through_road_coordinates_to_a_millimetre() {
    for name in MAPS {
        let net = map(name);
        let mut checked = 0;
        for lane in net.lanes() {
            let home = net.road_lane(lane.id).expect("every lane is on a road");
            let points = lane.center.points();
            for &p in points[1..points.len() - 1].iter().step_by(2) {
                let at = net
                    .road_position(p)
                    .expect("a point on a lane is on a road");
                let back = net.road_point(at).expect("the position is on its road");
                assert!(
                    (back - p).length() < MM,
                    "{name}: {p:?} came back as {back:?} from {at:?}"
                );
                if at.road == home.road {
                    let again = net.road_position(back).expect("again");
                    assert!(
                        (again.s - at.s).abs() < 1e-3 && (again.t - at.t).abs() < 1e-3,
                        "{name}: {at:?} then {again:?}"
                    );
                }
                checked += 1;
            }
        }
        assert!(checked > 10, "{name}: only {checked} points");
    }
}

/// A straight 100 m road along +X from `(10, 20)`, climbing 2 %, banked 0.1
/// rad, with its lanes pushed 1 m left by `<laneOffset>`: a 3.5 m driving
/// lane either side.
const BANKED: &str = r#"<OpenDRIVE>
  <road id="7" length="100" junction="-1">
    <planView><geometry s="0" x="10" y="20" hdg="0" length="100"><line/></geometry></planView>
    <elevationProfile><elevation s="0" a="5" b="0.02" c="0" d="0"/></elevationProfile>
    <lateralProfile><superelevation s="0" a="0.1" b="0" c="0" d="0"/></lateralProfile>
    <lanes>
      <laneOffset s="0" a="1" b="0" c="0" d="0"/>
      <laneSection s="0">
        <left><lane id="1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></left>
        <center><lane id="0" type="none"/></center>
        <right><lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn a_road_point_stands_on_the_banked_cross_section_at_t_from_the_reference_line() {
    let net = load_str(BANKED).unwrap();
    let road = net.road_by_od_id("7").expect("road 7");
    assert_eq!(road.length(), 100.0);
    let at = |s, t| {
        net.road_point(RoadPosition {
            road: road.id(),
            s,
            t,
        })
        .unwrap()
    };
    let near = |got: Point, want: [f64; 3]| {
        let want = Point::new(want[0] as f32, want[1] as f32, want[2] as f32);
        assert!((got - want).length() < 1e-4, "{got:?} != {want:?}");
    };
    let (sin, cos) = 0.1_f64.sin_cos();
    near(at(0.0, 0.0), [10.0, 20.0, 5.0]);
    near(at(50.0, 0.0), [60.0, 20.0, 6.0]);
    // t is measured from the reference line, not the offset lane 0, and
    // along the tilted cross-section.
    near(at(50.0, 2.0), [60.0, 20.0 + 2.0 * cos, 6.0 + 2.0 * sin]);
    near(at(50.0, -2.0), [60.0, 20.0 - 2.0 * cos, 6.0 - 2.0 * sin]);
}

#[test]
fn a_road_position_is_the_road_s_and_t_under_a_point() {
    let net = load_str(BANKED).unwrap();
    let road = net.road_by_od_id("7").unwrap().id();
    for (s, t) in [
        (0.0, 0.0),
        (12.5, 4.0),
        (50.0, -2.5),
        (99.0, 0.3),
        (100.0, -4.4),
    ] {
        let p = net.road_point(RoadPosition { road, s, t }).unwrap();
        let at = net.road_position(p).unwrap();
        assert_eq!(at.road, road);
        assert!(
            (at.s - s).abs() < 1e-4 && (at.t - t).abs() < 1e-4,
            "{at:?} for ({s}, {t})"
        );
        // Above the surface, the same place.
        let above = net.road_position(p + xodr::Vector::Z * 2.0).unwrap();
        assert!(
            (above.s - s).abs() < 1e-3 && (above.t - t).abs() < 1e-3,
            "{above:?}"
        );
    }
}

#[test]
fn a_point_past_the_end_of_a_road_is_held_to_its_end() {
    let net = load_str(BANKED).unwrap();
    let at = net.road_position(Point::new(120.0, 21.0, 7.0)).unwrap();
    assert_eq!(at.s, 100.0);
    assert!((at.t - 1.0 / 0.1_f64.cos()).abs() < 1e-3, "{at:?}");
    let before = net.road_position(Point::new(0.0, 20.0, 5.0)).unwrap();
    assert_eq!(before.s, 0.0);
}

#[test]
fn a_road_point_off_the_ends_of_its_road_or_on_no_road_is_none() {
    let net = load_str(BANKED).unwrap();
    let road = net.road_by_od_id("7").unwrap().id();
    let point = |road, s| net.road_point(RoadPosition { road, s, t: 0.0 });
    assert!(point(road, 100.0).is_some());
    assert!(point(road, 100.01).is_none());
    assert!(point(road, -0.01).is_none());
    assert!(point(RoadId(1), 5.0).is_none());
    assert!(net.road(RoadId(1)).is_none());
    assert!(net.road_by_od_id("8").is_none());
}

/// Road 1 runs along +X on the ground. Road 2 crosses over it along +Y,
/// 6 m up.
const BRIDGE: &str = r#"<OpenDRIVE>
  <road id="1" length="100" junction="-1">
    <planView><geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>
    <lanes><laneSection s="0">
      <center><lane id="0" type="none"/></center>
      <right><lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></right>
    </laneSection></lanes>
  </road>
  <road id="2" length="100" junction="-1">
    <planView><geometry s="0" x="50" y="-50" hdg="1.5707963267948966" length="100"><line/></geometry></planView>
    <elevationProfile><elevation s="0" a="6" b="0" c="0" d="0"/></elevationProfile>
    <lanes><laneSection s="0">
      <center><lane id="0" type="none"/></center>
      <right><lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></right>
    </laneSection></lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn a_point_on_a_bridge_finds_the_bridge_and_one_under_it_the_road_below() {
    let net = load_str(BRIDGE).unwrap();
    let od_id = |at: RoadPosition| net.road(at.road).unwrap().od_id().to_string();
    let on_bridge = net.road_position(Point::new(51.0, -1.0, 6.2)).unwrap();
    assert_eq!(od_id(on_bridge), "2");
    assert!((on_bridge.s - 49.0).abs() < 1e-4 && (on_bridge.t + 1.0).abs() < 1e-4);
    let below = net.road_position(Point::new(51.0, -1.0, 0.3)).unwrap();
    assert_eq!(od_id(below), "1");
    assert!((below.s - 51.0).abs() < 1e-4 && (below.t + 1.0).abs() < 1e-4);
}

#[test]
fn each_lane_names_its_road_section_and_lane_id_as_its_provenance_does() {
    let (net, prov) = load_file_with_provenance("tests/data/town07.xodr").unwrap();
    assert_eq!(net.roads().len(), 234);
    for p in &prov.lanes {
        let at = net.road_lane(p.lane).expect("on a road");
        let road = net.road(at.road).unwrap();
        assert_eq!(road.id(), at.road);
        assert_eq!(road.od_id(), p.road_id);
        assert_eq!((at.section, at.od_id), (p.section, p.od_id));
    }
}

#[test]
fn a_signal_stands_where_road_point_puts_its_station() {
    let (net, prov) = load_file_with_provenance("tests/data/signals.xodr").unwrap();
    for p in &prov.signals {
        let road = net.road_by_od_id(&p.road_id).unwrap().id();
        let want = net
            .road_point(RoadPosition {
                road,
                s: p.s,
                t: p.t,
            })
            .unwrap();
        let signal = net.signal(p.signal).unwrap();
        assert_eq!(signal.applies_at[0], want, "signal {}", p.od_id);
    }
}

#[test]
fn a_network_without_roads_answers_none() {
    let lanes = load_str(BANKED).unwrap().lanes().to_vec();
    let net = RoadNetwork::new(lanes);
    assert!(net.roads().is_empty());
    assert!(net.road_position(Point::new(50.0, 20.0, 6.0)).is_none());
    assert!(net
        .road_point(RoadPosition {
            road: RoadId(0),
            s: 1.0,
            t: 0.0
        })
        .is_none());
}

#[cfg(feature = "serde")]
#[test]
fn roads_survive_a_serde_round_trip_and_answer_the_same() {
    let net = map("e6mini");
    let json = serde_json::to_string(&net).unwrap();
    let back: RoadNetwork = serde_json::from_str(&json).unwrap();
    assert_eq!(back, net);
    for lane in net.lanes().iter().step_by(7) {
        let p = lane.center.point_at(lane.center.length() / 3.0);
        let at = net.road_position(p).unwrap();
        assert_eq!(back.road_position(p), Some(at));
        assert_eq!(back.road_point(at), net.road_point(at));
    }
}

#[cfg(feature = "serde")]
#[test]
fn a_network_serialized_before_roads_were_kept_deserializes_with_none() {
    let net = load_str(BANKED).unwrap();
    let mut json: serde_json::Value = serde_json::to_value(&net).unwrap();
    json.as_object_mut().unwrap().remove("roads");
    let back: RoadNetwork = serde_json::from_value(json).unwrap();
    assert!(back.roads().is_empty());
    assert_eq!(back.lanes(), net.lanes());
    assert!(back.road_position(Point::new(50.0, 20.0, 6.0)).is_none());
}

#[test]
fn a_point_where_roads_overlap_is_found_on_the_road_asked_for() {
    let net = map("town07");
    let mut overlaps = 0;
    for lane in net.lanes() {
        let home = net.road_lane(lane.id).unwrap().road;
        let points = lane.center.points();
        for &p in &points[1..points.len() - 1] {
            if net.road_position(p).unwrap().road == home {
                continue;
            }
            overlaps += 1;
            let at = net.road_position_on(home, p).expect("on its own road");
            assert_eq!(at.road, home);
            let back = net.road_point(at).unwrap();
            assert!((back - p).length() < MM, "{p:?} came back as {back:?}");
        }
    }
    assert!(overlaps > 100, "only {overlaps} points where roads overlap");
    assert!(net.road_position_on(RoadId(9999), Point::ORIGIN).is_none());
}

#[test]
fn roads_that_do_not_match_the_lanes_answer_none_instead_of_panicking() {
    let net = map("town07");
    let point = net.lanes()[5].center.points()[3];
    let truncated = RoadNetwork::new(net.lanes().to_vec()).with_roads(net.roads()[1..].to_vec());
    assert_eq!(truncated.road_position(point), None);
    let mut reversed = net.roads().to_vec();
    reversed.reverse();
    let reordered = RoadNetwork::new(net.lanes().to_vec()).with_roads(reversed);
    assert_eq!(reordered.road_position(point), None);
}

#[test]
fn a_param_poly3_turning_past_half_a_turn_keeps_its_lanes_on_their_side() {
    // A U-turn whose tangent ends at exactly +π in its own frame.
    let net = xodr::load_str(
        r#"<OpenDRIVE><header/><road id="1" length="24.9657" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="24.9657"><paramPoly3 pRange="normalized"
          aU="0" bU="20" cU="-20" dU="0" aV="0" bV="0" cV="-61.8" dV="41.2"/></geometry></planView>
        <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>
        <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
        </right></laneSection></lanes></road></OpenDRIVE>"#,
    )
    .unwrap();
    let road = net.roads()[0].id();
    let length = net.roads()[0].length();
    let at = |s: f64, t: f64| net.road_point(xodr::RoadPosition { road, s, t }).unwrap();
    for k in 0..=40 {
        let s = length - 0.5 + k as f64 * 0.0125;
        let (behind, ahead) = (
            at((s - 0.01).max(0.0), 0.0),
            at((s + 0.01).min(length), 0.0),
        );
        let (along, across) = (ahead - behind, at(s, -3.5) - at(s, 0.0));
        let cross = along.x * across.y - along.y * across.x;
        assert!(cross < 0.0, "s = {s}: lane -1 is not on the right");
    }
}
