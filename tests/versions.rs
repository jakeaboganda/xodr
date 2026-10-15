//! A file that uses an element or attribute newer than its header's
//! `revMinor` raises a `Warning::NewerThanDeclared` and loads it anyway.

use xodr::{load_str_with_provenance, Warning};

fn map(header: &str) -> String {
    format!(
        r#"<OpenDRIVE>
  <header {header}/>
  <road id="1" length="20" junction="-1">
    <planView><geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>
    <lanes><laneSection s="0">
      <left><lane id="1" type="driving" advisory="none"><width sOffset="0" a="3" b="0" c="0" d="0"/></lane></left>
      <center><lane id="0" type="none"/></center>
      <right><lane id="-1" type="driving" advisory="none"><width sOffset="0" a="3" b="0" c="0" d="0"/></lane></right>
    </laneSection></lanes>
    <objects>
      <object id="7" s="5" t="-5" type="pole" temporary="false"/>
    </objects>
    <signals>
      <signal id="9" s="5" t="-5" orientation="+" dynamic="no" length="0.5"/>
    </signals>
  </road>
  <junction id="5"><boundary/></junction>
</OpenDRIVE>"#
    )
}

/// `(road, junction, feature, introduced, declared, count)` of every
/// `NewerThanDeclared` warning, in order.
type Newer = (String, String, String, u32, u32, usize);

fn newer(header: &str) -> Vec<Newer> {
    let (_, provenance) = load_str_with_provenance(&map(header)).expect("the map loads");
    provenance
        .warnings
        .into_iter()
        .filter_map(|w| match w {
            Warning::NewerThanDeclared {
                road_id,
                junction_id,
                feature,
                introduced,
                declared,
                count,
            } => Some((road_id, junction_id, feature, introduced, declared, count)),
            _ => None,
        })
        .collect()
}

fn of(
    road: &str,
    junction: &str,
    feature: &str,
    introduced: u32,
    declared: u32,
    count: usize,
) -> Newer {
    (
        road.into(),
        junction.into(),
        feature.into(),
        introduced,
        declared,
        count,
    )
}

#[test]
fn a_file_declaring_1_6_hears_of_everything_1_8_and_1_9_introduced() {
    let want = vec![
        of("", "5", "<boundary>", 8, 6, 1),
        of("1", "", "<lane advisory>", 8, 6, 2),
        of("1", "", "<object temporary>", 9, 6, 1),
        of("1", "", "<signal length>", 8, 6, 1),
    ];
    assert_eq!(newer(r#"revMajor="1" revMinor="6""#), want);
}

#[test]
fn a_file_declaring_1_8_hears_only_of_what_1_9_introduced() {
    let want = vec![of("1", "", "<object temporary>", 9, 8, 1)];
    assert_eq!(newer(r#"revMajor="1" revMinor="8""#), want);
}

#[test]
fn a_file_declaring_1_9_or_no_version_hears_nothing() {
    for header in [
        r#"revMajor="1" revMinor="9""#,
        r#"revMajor="1""#,
        "",
        r#"revMajor="2" revMinor="0""#,
    ] {
        assert_eq!(newer(header), vec![], "{header}");
    }
}

#[test]
fn a_newer_feature_still_loads() {
    let old = r#"revMajor="1" revMinor="6""#;
    let (net, _) = load_str_with_provenance(&map(old)).expect("the map loads");
    let (now, _) = load_str_with_provenance(&map(r#"revMajor="1" revMinor="9""#)).expect("loads");
    assert_eq!(net.signals().len(), now.signals().len());
    assert_eq!(net.lanes().len(), now.lanes().len());
    assert!(!net.signals().is_empty());
}

#[test]
fn the_warning_reads_plainly() {
    let w = Warning::NewerThanDeclared {
        road_id: "1".into(),
        junction_id: String::new(),
        feature: "<signal length>".into(),
        introduced: 8,
        declared: 6,
        count: 2,
    };
    assert_eq!(
        w.to_string(),
        "road \"1\" uses <signal length> (2 uses), which OpenDRIVE 1.8 introduced, \
         but the header declares 1.6; read anyway"
    );
}
