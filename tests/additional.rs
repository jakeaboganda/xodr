//! Additional data: `<userData>` and `<dataQuality>` on any element, with
//! where each is, and the header's `<license>`.

use xodr::{load_str_with_provenance, Additional, DataError, DataQuality, License, RawData};

const MAP: &str = r#"<OpenDRIVE>
  <header name="m"><license name="Creative Commons Attribution 4.0" spdxid="CC-BY-4.0" resource="https://example.org"/></header>
  <road id="7" length="20" junction="-1">
    <planView><geometry s="0" x="0" y="0" hdg="0" length="20"><line/></geometry></planView>
    <lanes>
      <laneSection s="0"><center><lane id="0" type="none"/></center><right>
        <lane id="-1" type="driving"><width sOffset="0" a="3" b="0" c="0" d="0"/><userData code="texture" value="asphalt"/></lane>
      </right></laneSection>
      <laneSection s="10"><center><lane id="0" type="none"/></center><right>
        <lane id="-1" type="driving"><width sOffset="0" a="3" b="0" c="0" d="0"/></lane>
      </right><dataQuality><error xyAbsolute="0.1" xyRelative="0.02" zAbsolute="0.2" zRelative="0.05"/>
        <rawData date="2026-01-01" source="sensor" postProcessing="fused"/></dataQuality></laneSection>
    </lanes>
    <userData code="vendor"><anything><userData code="nested"/></anything></userData>
  </road>
  <userData code="top" value="1"/>
</OpenDRIVE>"#;

#[test]
fn additional_data_is_kept_with_the_element_it_is_on() {
    let (net, prov) = load_str_with_provenance(MAP).unwrap();
    let found: Vec<(&str, &str)> = prov
        .additional
        .iter()
        .map(|a| (a.path.as_str(), a.road_id.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            ("road[7]/lanes/laneSection[0]/right/lane[-1]", "7"),
            ("road[7]/lanes/laneSection[1]", "7"),
            ("road[7]", "7"),
            ("", ""),
        ],
        "the vendor's nested <userData> is its own business"
    );
    assert_eq!(
        prov.additional[0].data,
        Additional::UserData {
            code: "texture".into(),
            value: "asphalt".into()
        }
    );
    assert_eq!(
        prov.additional[1].data,
        Additional::DataQuality(DataQuality {
            error: Some(DataError {
                xy_absolute: Some(0.1),
                xy_relative: Some(0.02),
                z_absolute: Some(0.2),
                z_relative: Some(0.05),
            }),
            raw_data: Some(RawData {
                date: "2026-01-01".into(),
                source: "sensor".into(),
                post_processing: "fused".into(),
                ..RawData::default()
            }),
        })
    );
    assert_eq!(
        net.header().license,
        Some(License {
            name: "Creative Commons Attribution 4.0".into(),
            resource: "https://example.org".into(),
            spdx_id: "CC-BY-4.0".into(),
            text: String::new(),
        })
    );
    assert!(prov.warnings.is_empty(), "{:?}", prov.warnings);
}
