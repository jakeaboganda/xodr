//! `<crossfall>`, from OpenDRIVE 1.4: the road falls away from its
//! reference line at an angle, on one side or both.

use xodr::load_str;

/// A flat 100 m road along +X with lanes 1 and -1, 4 m wide, and
/// `crossfall` under its `<lateralProfile>`.
fn road(crossfall: &str) -> xodr::RoadNetwork {
    let lane = |id: i32| {
        format!(
            r#"<lane id="{id}" type="driving"><width sOffset="0" a="4" b="0" c="0" d="0"/></lane>"#
        )
    };
    load_str(&format!(
        r#"<OpenDRIVE><header revMajor="1" revMinor="4"/><road id="1" length="100" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>
        <lateralProfile>{crossfall}</lateralProfile>
        <lanes><laneSection s="0"><left>{}</left><center><lane id="0" type="none"/></center>
        <right>{}</right></laneSection></lanes></road></OpenDRIVE>"#,
        lane(1),
        lane(-1)
    ))
    .unwrap()
}

/// The height of the middle of the lane on `side`, positive for the left.
fn middle(net: &xodr::RoadNetwork, side: f32) -> f32 {
    let lane = net
        .lanes()
        .iter()
        .find(|l| l.center.points()[0].y * side > 0.0)
        .unwrap();
    lane.center.point_at(50.0).z
}

#[test]
fn a_crossfall_drops_each_side_away_from_the_reference_line() {
    let net = road(r#"<crossfall side="both" s="0" a="0.05" b="0" c="0" d="0"/>"#);
    let want = -(0.05_f32).tan() * 2.0;
    for side in [1.0, -1.0] {
        assert!(
            (middle(&net, side) - want).abs() < 1e-4,
            "{}",
            middle(&net, side)
        );
    }
}

#[test]
fn a_crossfall_on_one_side_leaves_the_other_flat() {
    let net = road(r#"<crossfall side="left" s="0" a="0.05" b="0" c="0" d="0"/>"#);
    assert!(middle(&net, 1.0) < -0.05);
    assert!(middle(&net, -1.0).abs() < 1e-6);
}
