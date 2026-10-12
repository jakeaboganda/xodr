//! What a file holds that the crate does not read: the elements it skips,
//! and the attributes that change a road or a junction and mean nothing to
//! it yet.

use super::Warning;

/// A feature of OpenDRIVE the crate ignores.
///
/// `path` runs from the element a warning is about (a `<road>`, a
/// `<junction>`, or the document) down to the element it names, with
/// `<left>` and `<right>` both written `side`. A leading `**/` matches the
/// element wherever it stands. `when` narrows the match to the elements
/// that use the feature.
struct Feature {
    name: &'static str,
    path: &'static str,
    when: fn(roxmltree::Node) -> bool,
}

/// Every feature the crate ignores, as the docs list them under "What the
/// importer ignores". Delete a row once the crate reads the feature.
const FEATURES: &[Feature] = &[
    Feature {
        name: "a repeated <lanes> of one layer",
        path: "lanes",
        when: repeats_a_layer,
    },
    Feature {
        name: "<roadSection>",
        path: "roadSection",
        when: not_in_a_crossing,
    },
];

fn not_in_a_crossing(node: roxmltree::Node) -> bool {
    node.parent_element()
        .is_none_or(|p| p.attribute("type") != Some("crossing"))
}

fn repeats_a_layer(node: roxmltree::Node) -> bool {
    let temporary = |n: &roxmltree::Node| n.attribute("layer") == Some("temporary");
    node.prev_siblings()
        .skip(1)
        .any(|n| n.has_tag_name("lanes") && temporary(&n) == temporary(&node))
}

/// What a `<road>` or `<junction>` without an `id` is called in a warning.
const NO_ID: &str = "(no id)";

fn leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn matches_trail(feature: &Feature, trail: &[&str]) -> bool {
    match feature.path.strip_prefix("**/") {
        Some(_) => true,
        None => feature.path.split('/').eq(trail.iter().copied()),
    }
}

/// Count each feature `node` and what lies under it use, in `counts`. `trail`
/// is the names of the elements above `node` below the scope, with `<left>`
/// and `<right>` as `side`. A `<userData>` holds the vendor's own elements,
/// so it is counted and not entered.
fn visit<'a>(node: roxmltree::Node<'a, 'a>, trail: &mut Vec<&'a str>, counts: &mut [usize]) {
    let name = node.tag_name().name();
    trail.push(match name {
        "left" | "right" => "side",
        name => name,
    });
    for (count, feature) in counts.iter_mut().zip(FEATURES) {
        if leaf(feature.path) == name && matches_trail(feature, trail) && (feature.when)(node) {
            *count += 1;
        }
    }
    if name != "userData" {
        for child in node.children().filter(|n| n.is_element()) {
            visit(child, trail, counts);
        }
    }
    trail.pop();
}

/// One warning for each feature used under `nodes`, with how many times.
/// `ids` are the road's and the junction's `id`, empty where they don't
/// apply.
fn scan<'a>(
    nodes: impl Iterator<Item = roxmltree::Node<'a, 'a>>,
    ids: (&str, &str),
    out: &mut Vec<Warning>,
) {
    let mut counts = vec![0usize; FEATURES.len()];
    let mut trail = Vec::new();
    for node in nodes.filter(|n| n.is_element()) {
        visit(node, &mut trail, &mut counts);
    }
    for (feature, count) in FEATURES.iter().zip(counts) {
        if count > 0 {
            out.push(Warning::Ignored {
                road_id: ids.0.to_string(),
                junction_id: ids.1.to_string(),
                feature: feature.name.to_string(),
                count,
            });
        }
    }
}

/// The warnings for everything in the file the crate ignores: the
/// document's own elements first, then each junction's, then each road's.
pub(super) fn place(root: roxmltree::Node) -> Vec<Warning> {
    let mut out = Vec::new();
    let others = root
        .children()
        .filter(|n| !n.has_tag_name("road") && !n.has_tag_name("junction"));
    scan(others, ("", ""), &mut out);
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        let id = junction.attribute("id").unwrap_or(NO_ID);
        scan(junction.children(), ("", id), &mut out);
    }
    for road in root.children().filter(|n| n.has_tag_name("road")) {
        let id = road.attribute("id").unwrap_or(NO_ID);
        scan(road.children(), (id, ""), &mut out);
    }
    out
}
