//! `<include>`: files a document reads in where it names them.
//!
//! `tests/data/include/main.xodr` reads its plan view and lanes from files in
//! `parts/`, the lanes nesting a further include, and names a missing file,
//! one with the wrong root, one out of its folder, and one including itself.

use xodr::{load_file_with_provenance, load_str_with, IncludeProblem, Warning};

const MAIN: &str = "tests/data/include/main.xodr";

#[test]
fn a_file_reads_its_includes_from_its_folder() {
    let (net, prov) = load_file_with_provenance(MAIN).unwrap();
    assert_eq!(net.lanes().len(), 1);
    assert!((net.roads()[0].length() - 100.0).abs() < 1e-9);
    let problems: Vec<_> = prov
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::IncludeDropped { file, problem } => Some((file.as_str(), problem.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(problems.len(), 4, "{problems:?}");
    assert!(problems.contains(&("parts/wrong_root.xml", IncludeProblem::WrongRoot)));
    assert!(problems.contains(&("cycle.xml", IncludeProblem::Cycle)));
    for file in ["missing.xml", "../include/main.xodr"] {
        assert!(
            problems
                .iter()
                .any(|(f, p)| *f == file && matches!(p, IncludeProblem::Unreadable(_))),
            "{file}: {problems:?}"
        );
    }
}

#[test]
fn a_string_reads_its_includes_with_the_reader_it_is_given() {
    let xml = r#"<OpenDRIVE><header/><road id="1" length="10" junction="-1">
        <planView><include file="plan.xml"/></planView>
        <lanes><laneSection s="0"><center><lane id="0" type="none"/></center>
        <right><lane id="-1" type="driving"><width sOffset="0" a="3" b="0" c="0" d="0"/></lane></right>
        </laneSection></lanes></road></OpenDRIVE>"#;
    let mut asked = Vec::new();
    let (net, prov) = load_str_with(xml, |file| {
        asked.push(file.to_string());
        Ok(r#"<planView><geometry s="0" x="5" y="0" hdg="0" length="10"><line/></geometry></planView>"#.into())
    })
    .unwrap();
    assert_eq!(asked, ["plan.xml"]);
    assert_eq!(net.lanes()[0].center.points()[0].x, 5.0);
    assert!(prov.warnings.is_empty(), "{:?}", prov.warnings);
    let plain = xml.replace(
        r#"<include file="plan.xml"/>"#,
        r#"<geometry s="0" x="0" y="0" hdg="0" length="10"><line/></geometry><include file="more.xml"/>"#,
    );
    let (_, prov) = xodr::load_str_with_provenance(&plain).unwrap();
    assert!(matches!(
        &prov.warnings[..],
        [Warning::IncludeDropped {
            problem: IncludeProblem::Unreadable(_),
            ..
        }]
    ));
}

#[test]
fn an_included_file_nested_too_deep_is_left_out_not_a_stack_overflow() {
    let xml = r#"<OpenDRIVE><header/><road id="1" length="10" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="10"><line/></geometry></planView>
        <lanes><laneSection s="0"><center><lane id="0" type="none"/></center>
        <right><lane id="-1" type="driving"><width sOffset="0" a="3" b="0" c="0" d="0"/></lane></right>
        </laneSection></lanes><objects><include file="deep.xml"/></objects></road></OpenDRIVE>"#;
    let deep = format!(
        "<objects>{}{}</objects>",
        "<a>".repeat(20_000),
        "</a>".repeat(20_000)
    );
    let (net, prov) = load_str_with(xml, |_| Ok(deep.clone())).unwrap();
    assert_eq!(net.lanes().len(), 1);
    assert!(matches!(
        &prov.warnings[..],
        [Warning::IncludeDropped {
            problem: IncludeProblem::NotXml,
            ..
        }]
    ));
}
