//! Lane positions: `RoadNetwork::lane_point` and `lane_position` place a
//! point by its lane, the road `s`, and an offset from the lane's center.

use xodr::{load_file, load_str, LaneId, LanePosition, Point, RoadNetwork, RoadPosition, Vector};

/// A 60 m arc from the origin heading +X, curving left on a 40 m radius. Its
/// `<laneOffset>` grows from 0 to 1 m over the road, so the lanes move off
/// the reference line as they go. Lane 1 and lane -1 are 3.5 m driving
/// lanes, and lane -2 a 2 m sidewalk 0.15 m up.
const CURVED: &str = r#"<OpenDRIVE>
  <road id="1" length="60" junction="-1">
    <planView><geometry s="0" x="0" y="0" hdg="0" length="60"><arc curvature="0.025"/></geometry></planView>
    <lanes>
      <laneOffset s="0" a="0" b="0.0166666667" c="0" d="0"/>
      <laneSection s="0">
        <left><lane id="1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane></left>
        <center><lane id="0" type="none"/></center>
        <right>
          <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
          <lane id="-2" type="sidewalk"><width sOffset="0" a="2" b="0" c="0" d="0"/>
            <height sOffset="0" inner="0.15" outer="0.15"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

fn curved() -> RoadNetwork {
    load_str(CURVED).expect("the road loads")
}

/// The baked lane with `<lane id>` `od_id`.
fn lane(net: &RoadNetwork, od_id: i32) -> LaneId {
    net.lanes()
        .iter()
        .find(|l| net.road_lane(l.id).unwrap().od_id == od_id)
        .unwrap()
        .id
}

fn near(a: Point, b: Point, tolerance: f32) -> bool {
    (a - b).length() < tolerance
}

#[test]
fn a_lane_position_matches_the_road_position_under_it() {
    let net = curved();
    let road = net.roads()[0].id();
    for (s, t) in [
        (5.0, 1.2),
        (20.0, -1.0),
        (33.3, -2.9),
        (47.0, 2.5),
        (58.0, -5.8),
    ] {
        let p = net.road_point(RoadPosition { road, s, t }).unwrap();
        let at = net.lane_position(p).unwrap();
        assert!((at.s - s).abs() < 1e-6, "s {} for {s}", at.s);
        let base = s / 60.0;
        let (od_id, center) = match t {
            t if t > base => (1, base + 1.75),
            t if t > base - 3.5 => (-1, base - 1.75),
            _ => (-2, base - 4.5),
        };
        assert_eq!(at.lane, lane(&net, od_id), "lane at t {t}");
        assert!(
            (at.offset - (t - center)).abs() < 1e-6,
            "offset {}",
            at.offset
        );
        let back = net.lane_point(at).unwrap();
        assert!(near(back, p, 1e-4), "{back:?} != {p:?}");
    }
}

#[test]
fn a_lane_point_at_no_offset_is_on_the_baked_centerline() {
    for name in [
        "town07",
        "lane_heights",
        "lateral_shapes",
        "level_lanes",
        "e6mini",
    ] {
        let net = load_file(format!("tests/data/{name}.xodr")).unwrap();
        for lane in net.lanes() {
            let points = lane.center.points();
            for &p in &points[1..points.len() - 1] {
                let road = net.road_lane(lane.id).unwrap().road;
                let s = net.road_position_on(road, p).unwrap().s;
                let center = LanePosition {
                    lane: lane.id,
                    s,
                    offset: 0.0,
                };
                let exact = net.lane_point(center).unwrap();
                assert!(near(exact, p, 1e-3), "{name}: {exact:?} != {p:?}");
            }
        }
    }
}

#[test]
fn the_centerline_s_of_a_lane_position_is_its_distance_along_the_lane() {
    let net = curved();
    let road = net.roads()[0].id();
    for od_id in [1, -1, -2] {
        let id = lane(&net, od_id);
        let center = &net.lane(id).unwrap().center;
        let along = |s| {
            net.centerline_s(LanePosition {
                lane: id,
                s,
                offset: 0.0,
            })
            .unwrap()
        };
        assert_eq!(along(0.0), 0.0);
        assert!((along(60.0) - center.length()).abs() < 1e-3);
        // Lane 1 is inside the bend and lane -2 outside it, so each is
        // shorter or longer than the reference line by its t over the radius.
        let t = |s: f64| {
            let base = s / 60.0;
            match od_id {
                1 => base + 1.75,
                -1 => base - 1.75,
                _ => base - 4.5,
            }
        };
        let arc = |s: f64| s - (t(0.0) * s + (t(s) - t(0.0)) * s / 2.0) * 0.025;
        for s in [10.0, 30.0, 50.0] {
            assert!(
                (f64::from(along(s)) - arc(s)).abs() < 0.01,
                "lane {od_id} s {s}"
            );
        }
        // A point projected onto the lane has the lane's own s, which is the
        // centerline_s of its lane position.
        let p = net
            .road_point(RoadPosition {
                road,
                s: 25.0,
                t: t(25.0),
            })
            .unwrap();
        let projected = center.project(p).s;
        let at = net.lane_position(p).unwrap();
        assert!((net.centerline_s(at).unwrap() - projected).abs() < 1e-3);
    }
    let off = LanePosition {
        lane: lane(&net, 1),
        s: 60.5,
        offset: 0.0,
    };
    assert!(net.centerline_s(off).is_none());
    assert!(net.lane_point(off).is_none());
}

#[test]
fn an_offset_or_t_that_is_not_finite_has_no_point() {
    let net = curved();
    let id = lane(&net, 1);
    for bad in [f64::NAN, f64::INFINITY] {
        let at = LanePosition {
            lane: id,
            s: 10.0,
            offset: bad,
        };
        assert!(net.lane_point(at).is_none());
        let road = net.road_lane(id).unwrap().road;
        assert!(net
            .road_point(RoadPosition {
                road,
                s: 10.0,
                t: bad
            })
            .is_none());
    }
}

#[test]
fn an_offset_past_the_edge_stays_level_with_the_lane() {
    let net = curved();
    let sidewalk = lane(&net, -2);
    let at = |offset| {
        net.lane_point(LanePosition {
            lane: sidewalk,
            s: 30.0,
            offset,
        })
        .unwrap()
    };
    assert!((at(0.0).z - 0.15).abs() < 1e-5);
    // 3 m toward +t is over lane -1, but the sidewalk's surface carries on.
    assert!((at(3.0).z - 0.15).abs() < 1e-5);
}

#[test]
fn a_millimetre_either_side_of_a_border_is_on_the_lane_that_side() {
    let net = curved();
    let road = net.roads()[0].id();
    let on = |t| {
        let p = net.road_point(RoadPosition { road, s: 30.0, t }).unwrap();
        net.lane_position(p + Vector::Z * 0.01).unwrap().lane
    };
    let base = 0.5;
    assert_eq!(on(base + 0.001), lane(&net, 1));
    assert_eq!(on(base - 0.001), lane(&net, -1));
    assert_eq!(on(base - 3.499), lane(&net, -1));
    assert_eq!(on(base - 3.501), lane(&net, -2));
    // Past the outermost lane, on it.
    assert_eq!(on(base - 9.0), lane(&net, -2));
}

#[test]
fn a_network_without_roads_has_no_lane_positions() {
    let net = RoadNetwork::new(curved().lanes().to_vec());
    assert!(net.lane_position(Point::new(10.0, 1.0, 0.0)).is_none());
    let at = LanePosition {
        lane: LaneId(0),
        s: 1.0,
        offset: 0.0,
    };
    assert!(net.lane_point(at).is_none());
    assert!(net.centerline_s(at).is_none());
}

/// A 60 m straight heading +X with lane -1, 3.5 m, and lane -2, a 2 m
/// sidewalk rising from 0 at its inner border to 0.3 m at its outer one. Its
/// only lane section starts at `s` 10.
const SLOPED: &str = r#"<OpenDRIVE>
  <road id="1" length="60" junction="-1">
    <planView><geometry s="0" x="0" y="0" hdg="0" length="60"><line/></geometry></planView>
    <lanes>
      <laneSection s="10">
        <center><lane id="0" type="none"/></center>
        <right>
          <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
          <lane id="-2" type="sidewalk"><width sOffset="0" a="2" b="0" c="0" d="0"/>
            <height sOffset="0" inner="0" outer="0.3"/></lane>
        </right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn past_a_sloped_lane_s_outer_border_the_surface_holds_the_border_s_height() {
    let net = load_str(SLOPED).expect("the road loads");
    let road = net.roads()[0].id();
    for t in [-7.0, -9.0] {
        let point = net.road_point(RoadPosition { road, s: 30.0, t }).unwrap();
        assert!((point.z - 0.3).abs() < 1e-5);
        let at = net.lane_position(point).unwrap();
        assert_eq!(at.lane, lane(&net, -2));
        assert!(near(net.lane_point(at).unwrap(), point, 1e-3));
    }
}

#[test]
fn a_point_where_no_lane_section_runs_has_no_lane_position() {
    let net = load_str(SLOPED).expect("the road loads");
    let road = net.roads()[0].id();
    let point = net
        .road_point(RoadPosition {
            road,
            s: 2.0,
            t: -1.0,
        })
        .unwrap();
    assert_eq!(net.lane_position(point), None);
}

#[test]
fn a_point_a_rounding_error_short_of_a_lane_section_has_a_lane_position() {
    // The second section starts 5e-10 m past x = 50, which an f32 holds
    // exactly, so the point's road s lands just short of it.
    let section = |s: &str| {
        format!(
            r#"<laneSection s="{s}"><center><lane id="0" type="none"/></center><right>
            <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
            </right></laneSection>"#
        )
    };
    let net = load_str(&format!(
        r#"<OpenDRIVE><road id="1" length="100" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>
        <lanes>{}{}</lanes></road></OpenDRIVE>"#,
        section("0"),
        section("50.0000000005")
    ))
    .unwrap();
    let at = net.lane_position(Point::new(50.0, -1.75, 0.0)).unwrap();
    assert!(net.centerline_s(at).is_some());
}
