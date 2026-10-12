//! The documented element list has to match the parser.
//!
//! A reference list of supported tags is the kind of prose that rots the
//! first time someone adds an element and forgets the docs. So the list is
//! checked against the source instead of trusted: every tag the parser looks
//! for appears in the crate-docs table, every tag in the table is one the
//! parser looks for, and nothing named as ignored is secretly read.

const PARSER: &str = include_str!("../src/parse/mod.rs");
const LINKS: &str = include_str!("../src/parse/links.rs");
const SIGNALS: &str = include_str!("../src/parse/signals.rs");
const ROAD_MARKS: &str = include_str!("../src/parse/road_marks.rs");
const RAILWAY: &str = include_str!("../src/parse/railway.rs");
const VIRTUAL_JUNCTIONS: &str = include_str!("../src/parse/virtual_junctions.rs");
const PROPERTIES: &str = include_str!("../src/parse/properties.rs");
const JUNCTION_AREAS: &str = include_str!("../src/parse/junction_areas.rs");
const CRATE_DOCS: &str = include_str!("../src/lib.rs");

/// Tags read in one place and ignored in another, which a scan of tag names
/// alone cannot tell apart. A road's `<surface>` is read, an object's is not.
/// An object's `<userData>` is read, a signal's is not. A road's `<objects>`
/// are read, a junction's are not. An object's `<markings>` are read, an
/// outline's are not.
const READ_IN_ANOTHER_PLACE: [&str; 4] = ["surface", "userData", "objects", "markings"];

/// `<left>` and `<right>` reach `child()` through a loop variable rather than
/// a literal, and a signal's `<semantics>` children are matched by name in
/// one `match`, so no scan of the source can see them.
const LOOKED_UP_BY_VARIABLE: [&str; 20] = [
    "left",
    "right",
    "speed",
    "lane",
    "priority",
    "prohibited",
    "warning",
    "routing",
    "streetname",
    "parking",
    "tourist",
    "supplementaryTime",
    "supplementaryAllows",
    "supplementaryProhibits",
    "supplementaryDistance",
    "supplementaryEnvironment",
    "supplementaryExplanatory",
    "animal",
    "person",
    "vehicle",
];

/// Every XML tag name the parser asks for by literal.
fn tags_the_parser_reads() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for source in [
        PARSER,
        LINKS,
        SIGNALS,
        ROAD_MARKS,
        PROPERTIES,
        JUNCTION_AREAS,
        RAILWAY,
        VIRTUAL_JUNCTIONS,
    ] {
        // Each of these takes the tag as its first (or only) string literal.
        for marker in [
            "has_tag_name(",
            "child(",
            "children(",
            "cubics_in(",
            "by_offset(",
        ] {
            let mut rest = source;
            while let Some(at) = rest.find(marker) {
                rest = &rest[at + marker.len()..];
                // Only this call's own text. A lookup that takes its tag
                // from a variable has no literal, and without the bound the
                // scan would run on and take the next attribute name it saw.
                let Some(close) = rest.find(')') else {
                    continue;
                };
                let call = &rest[..close];
                let Some(open) = call.find('"') else { continue };
                let Some(len) = call[open + 1..].find('"') else {
                    continue;
                };
                let tag = &call[open + 1..open + 1 + len];
                if !tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric()) {
                    found.push(tag.to_string());
                }
            }
        }
    }
    found.extend(LOOKED_UP_BY_VARIABLE.iter().map(|s| s.to_string()));
    found.sort();
    found.dedup();
    found
}

/// Every `<tag>` named in the crate docs' element table.
fn tags_the_docs_list() -> Vec<String> {
    let table = section(
        CRATE_DOCS,
        "# Which elements",
        "# What the importer ignores",
    );
    let mut found: Vec<String> = table
        .lines()
        .filter(|l| l.contains('|'))
        .flat_map(angle_bracketed)
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Every `<tag>` the docs name as ignored.
fn tags_the_docs_call_ignored() -> Vec<String> {
    let mut found: Vec<String> = angle_bracketed(section(
        CRATE_DOCS,
        "# What the importer ignores",
        "# Kept for the caller",
    ))
    .collect();
    found.sort();
    found.dedup();
    found
}

fn section<'a>(text: &'a str, from: &str, to: &str) -> &'a str {
    let start = text
        .find(from)
        .unwrap_or_else(|| panic!("no section {from:?}"));
    let end = text[start..]
        .find(to)
        .unwrap_or_else(|| panic!("no section {to:?} after {from:?}"));
    &text[start..start + end]
}

/// Pull `name` out of every `<name>` in some text.
fn angle_bracketed(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('<').skip(1).filter_map(|piece| {
        let name = piece.split('>').next()?;
        let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric());
        ok.then(|| name.to_string())
    })
}

#[test]
fn the_documented_element_table_matches_the_parser() {
    let read = tags_the_parser_reads();
    let documented = tags_the_docs_list();
    assert!(
        read.len() > 15,
        "the source scan found only {read:?}; the scan itself is broken"
    );

    let undocumented: Vec<_> = read.iter().filter(|t| !documented.contains(t)).collect();
    assert!(
        undocumented.is_empty(),
        "the parser reads {undocumented:?}, which the crate docs table does not list"
    );

    let overclaimed: Vec<_> = documented.iter().filter(|t| !read.contains(t)).collect();
    assert!(
        overclaimed.is_empty(),
        "the crate docs table lists {overclaimed:?}, which the parser never reads"
    );
}

#[test]
fn nothing_named_as_ignored_is_actually_read() {
    let read = tags_the_parser_reads();
    let ignored = tags_the_docs_call_ignored();
    assert!(
        ignored.len() > 5,
        "the ignored list came out as {ignored:?}; the scan itself is broken"
    );
    let contradictions: Vec<_> = ignored
        .iter()
        .filter(|t| read.contains(t) && !READ_IN_ANOTHER_PLACE.contains(&t.as_str()))
        .collect();
    assert!(
        contradictions.is_empty(),
        "the crate docs call {contradictions:?} ignored, but the parser reads them"
    );
}

/// One straight road with a single driving lane, minus its `<OpenDRIVE>`
/// wrapper so a header can be varied around it.
const ROAD: &str = r#"
  <road name="r" length="20.0" id="1" junction="-1">
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
  </road>"#;

#[test]
fn the_declared_version_changes_nothing() {
    // The docs promise the importer never inspects revMajor or revMinor. So
    // the same road has to import identically with no header at all, with the
    // oldest and newest real revisions, and with a version that will never
    // exist.
    let headers = [
        "",
        r#"<header revMajor="1" revMinor="4" name="oldest"/>"#,
        r#"<header revMajor="1" revMinor="9" name="newest"/>"#,
        r#"<header revMajor="7" revMinor="3" name="not a real revision"/>"#,
        r#"<header revMajor="banana" revMinor=""/>"#,
    ];
    let baseline = xodr::load_str(&format!("<OpenDRIVE>{ROAD}</OpenDRIVE>"))
        .expect("a road with no header imports");
    for header in headers {
        let net = xodr::load_str(&format!("<OpenDRIVE>{header}{ROAD}</OpenDRIVE>"))
            .unwrap_or_else(|e| panic!("header {header:?} was rejected: {e}"));
        // The header keeps what it says. Only the baked road has to match.
        let net = net.with_header(baseline.header().clone());
        assert_eq!(net, baseline, "header {header:?} changed the baked road");
    }
}
