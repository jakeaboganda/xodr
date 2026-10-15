//! What a file uses that is newer than the OpenDRIVE version its header
//! declares. The crate reads it all the same: a declared version is often
//! older than the file, and an element means the same whatever the header
//! says. The warning tells the caller the header is wrong.

use super::introduced::INTRODUCED;
use super::Warning;
use std::collections::BTreeMap;

/// The `revMinor` the header declares, when it declares OpenDRIVE 1 and a
/// minor version older than the newest the table knows. A file without a
/// version, or with one the crate can't read, declares nothing to compare.
fn declared(root: roxmltree::Node) -> Option<u32> {
    let header = root.children().find(|n| n.has_tag_name("header"))?;
    let revision = |name| header.attribute(name)?.trim().parse::<u32>().ok();
    let newest = INTRODUCED.iter().map(|&(_, _, minor)| minor).max()?;
    let minor = revision("revMinor")?;
    (revision("revMajor")? == 1 && minor < newest).then_some(minor)
}

/// The feature a row of the table names, as a warning shows it.
fn feature(path: &[&str], attribute: Option<&str>) -> String {
    let leaf = path.last().copied().unwrap_or_default();
    match attribute {
        Some(attribute) => format!("<{leaf} {attribute}>"),
        None => format!("<{leaf}>"),
    }
}

/// Counts each newer feature under `node`, by feature, into `counts`.
fn visit<'a>(
    node: roxmltree::Node<'a, 'a>,
    trail: &mut Vec<&'a str>,
    declared: u32,
    counts: &mut BTreeMap<String, (u32, usize)>,
) {
    let name = node.tag_name().name();
    trail.push(name);
    for &(path, attribute, minor) in INTRODUCED {
        let used = match attribute {
            Some(attribute) => node.attribute(attribute).is_some(),
            None => true,
        };
        if minor > declared && used && path.last() == Some(&name) && path == trail.as_slice() {
            counts
                .entry(feature(path, attribute))
                .or_insert((minor, 0))
                .1 += 1;
        }
    }
    for child in node.children().filter(roxmltree::Node::is_element) {
        visit(child, trail, declared, counts);
    }
    trail.pop();
}

fn scan<'a>(
    nodes: impl Iterator<Item = roxmltree::Node<'a, 'a>>,
    (road_id, junction_id): (&str, &str),
    declared: u32,
    out: &mut Vec<Warning>,
) {
    let mut counts = BTreeMap::new();
    for node in nodes.filter(roxmltree::Node::is_element) {
        visit(node, &mut Vec::new(), declared, &mut counts);
    }
    for (feature, (introduced, count)) in counts {
        out.push(Warning::NewerThanDeclared {
            road_id: road_id.to_string(),
            junction_id: junction_id.to_string(),
            feature,
            introduced,
            declared,
            count,
        });
    }
}

/// The warnings for everything in the file newer than its declared
/// version: the document's own elements first, then each junction's, then
/// each road's.
pub(super) fn place(root: roxmltree::Node) -> Vec<Warning> {
    let mut out = Vec::new();
    let Some(declared) = declared(root) else {
        return out;
    };
    let others = root
        .children()
        .filter(|n| !n.has_tag_name("road") && !n.has_tag_name("junction"));
    scan(others, ("", ""), declared, &mut out);
    for (tag, by) in [("junction", false), ("road", true)] {
        for node in root.children().filter(|n| n.has_tag_name(tag)) {
            let id = node.attribute("id").unwrap_or("(no id)");
            let scope = if by { (id, "") } else { ("", id) };
            scan(std::iter::once(node), scope, declared, &mut out);
        }
    }
    out
}
