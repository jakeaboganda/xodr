//! Profile records the crate repairs as it reads them: one dropped for
//! lacking where it starts, a number it can't read taken as 0, and records
//! out of order, sorted. Each raises a warning naming the record.

use xodr::{load_str_with_provenance, Warning};

/// A straight 100 m road with `profile` before its lanes and `lane` inside
/// its lane -1, which has a width of 3.5 m.
fn road(profile: &str, lane: &str) -> Vec<Warning> {
    let xodr = format!(
        r#"<OpenDRIVE><header/><road id="7" length="100" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>
        {profile}<lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>
        <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/>{lane}</lane>
        </right></laneSection></lanes></road></OpenDRIVE>"#
    );
    load_str_with_provenance(&xodr).unwrap().1.warnings
}

#[test]
fn a_clean_profile_raises_nothing() {
    let profile = r#"<elevationProfile><elevation s="0" a="0" b="0.01" c="0" d="0"/>
        <elevation s="50" a="0.5" b="0" c="0" d="0"/></elevationProfile>"#;
    assert!(road(profile, r#"<height sOffset="0" inner="0.1" outer="0.1"/>"#).is_empty());
}

#[test]
fn a_record_without_where_it_starts_is_dropped_with_a_warning() {
    let profile = r#"<elevationProfile><elevation a="1" b="0" c="0" d="0"/></elevationProfile>"#;
    assert_eq!(
        road(profile, ""),
        [Warning::RecordDropped {
            road_id: "7".into(),
            record: "elevation".into(),
            section: None,
            lane: None,
            s: None,
            attribute: "s".into(),
        }]
    );
}

#[test]
fn a_number_the_crate_cannot_read_is_read_as_0_with_a_warning() {
    let profile =
        r#"<lateralProfile><superelevation s="10" a="0" b="NaN" c="0" d="0"/></lateralProfile>"#;
    assert_eq!(
        road(profile, r#"<height sOffset="5" inner="x" outer="0.1"/>"#),
        [
            Warning::RecordValueUnreadable {
                road_id: "7".into(),
                record: "superelevation".into(),
                section: None,
                lane: None,
                s: Some(10.0),
                attribute: "b".into(),
            },
            Warning::RecordValueUnreadable {
                road_id: "7".into(),
                record: "height".into(),
                section: Some(0),
                lane: Some(-1),
                s: Some(5.0),
                attribute: "inner".into(),
            },
        ]
    );
}

#[test]
fn records_out_of_order_are_sorted_with_a_warning() {
    let lane = r#"<width sOffset="60" a="3" b="0" c="0" d="0"/><width sOffset="30" a="3.2" b="0" c="0" d="0"/>"#;
    let warnings = road("", lane);
    assert_eq!(
        warnings,
        [Warning::RecordsOutOfOrder {
            road_id: "7".into(),
            record: "width".into(),
            section: Some(0),
            lane: Some(-1),
        }]
    );
    assert_eq!(
        warnings[0].to_string(),
        r#"road "7", lane section 0, lane -1: <width> records out of order, sorted"#
    );
}

#[test]
fn a_lane_section_past_the_road_end_is_dropped_with_a_warning() {
    let section = |s: &str| {
        format!(
            r#"<laneSection s="{s}"><center><lane id="0" type="none"/></center><right>
            <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
            </right></laneSection>"#
        )
    };
    let xodr = format!(
        r#"<OpenDRIVE><header/><road id="7" length="100" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="100"><line/></geometry></planView>
        <lanes>{}{}{}{}</lanes></road></OpenDRIVE>"#,
        section("0"),
        section("50"),
        section("50.0000001"),
        section("120")
    );
    let (net, prov) = load_str_with_provenance(&xodr).unwrap();
    assert_eq!(net.lanes().len(), 2, "the sub-millimetre section goes quietly");
    assert_eq!(
        prov.warnings,
        [Warning::LaneSectionDropped {
            road_id: "7".into(),
            section: 3,
            s: 120.0,
        }]
    );
}
