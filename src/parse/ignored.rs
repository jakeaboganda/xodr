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
        name: "<userData>",
        path: "**/userData",
        when: not_on_an_object,
    },
    Feature {
        name: "<include>",
        path: "**/include",
        when: always,
    },
    Feature {
        name: "<dataQuality>",
        path: "**/dataQuality",
        when: always,
    },
    Feature {
        name: "<license>",
        path: "header/license",
        when: always,
    },
    Feature {
        name: "<defaultRegulations>",
        path: "header/defaultRegulations",
        when: always,
    },
    Feature {
        name: "<vmsGroup>",
        path: "vmsGroup",
        when: always,
    },
    Feature {
        name: "a second <lanes>",
        path: "lanes",
        when: follows_another_lanes,
    },
    Feature {
        name: "<lanes layer=\"temporary\">",
        path: "lanes",
        when: temporary_layer,
    },
    Feature {
        name: "<laneSection singleSide=\"true\">",
        path: "lanes/laneSection",
        when: single_sided,
    },
    Feature {
        name: "<predecessor layer>",
        path: "lanes/laneSection/side/lane/link/predecessor",
        when: names_a_layer,
    },
    Feature {
        name: "<successor layer>",
        path: "lanes/laneSection/side/lane/link/successor",
        when: names_a_layer,
    },
    Feature {
        name: "<validity layer>",
        path: "**/validity",
        when: names_a_layer,
    },
    Feature {
        name: "<laneLink fromLayer or toLayer>",
        path: "connection/laneLink",
        when: crosses_layers,
    },
    Feature {
        name: "<object perpToRoad=\"true\">",
        path: "objects/object",
        when: follows_the_road,
    },
    Feature {
        name: "<repeat bT, cT or dT>",
        path: "objects/object/repeat",
        when: has_cubic_terms,
    },
    Feature {
        name: "<repeat detachFromReferenceLine=\"true\">",
        path: "objects/object/repeat",
        when: detached,
    },
    Feature {
        name: "<object> <surface>",
        path: "objects/object/surface",
        when: always,
    },
    Feature {
        name: "<object> <skeleton>",
        path: "objects/object/skeleton",
        when: always,
    },
    Feature {
        name: "<curveLocal>",
        path: "objects/object/outlines/outline/curveLocal",
        when: always,
    },
    Feature {
        name: "<outline> <markings>",
        path: "objects/object/outlines/outline/markings",
        when: always,
    },
    Feature {
        name: "<sign> <validity>",
        path: "signals/signal/staticBoard/sign/validity",
        when: always,
    },
    Feature {
        name: "<sign> <dependency>",
        path: "signals/signal/staticBoard/sign/dependency",
        when: always,
    },
    Feature {
        name: "<sign> <reference>",
        path: "signals/signal/staticBoard/sign/reference",
        when: always,
    },
    Feature {
        name: "<junction> <objects>",
        path: "objects",
        when: in_a_junction,
    },
    Feature {
        name: "<roadSection>",
        path: "roadSection",
        when: always,
    },
    Feature {
        name: "<segment transitionLength>",
        path: "boundary/segment",
        when: blends,
    },
];

fn always(_: roxmltree::Node) -> bool {
    true
}

fn not_on_an_object(node: roxmltree::Node) -> bool {
    !node
        .parent_element()
        .is_some_and(|p| p.has_tag_name("object"))
}

fn in_a_junction(node: roxmltree::Node) -> bool {
    node.parent_element()
        .is_some_and(|p| p.has_tag_name("junction"))
}

fn follows_another_lanes(node: roxmltree::Node) -> bool {
    node.prev_siblings()
        .skip(1)
        .any(|n| n.has_tag_name("lanes"))
}

fn temporary_layer(node: roxmltree::Node) -> bool {
    node.attribute("layer") == Some("temporary")
}

fn names_a_layer(node: roxmltree::Node) -> bool {
    node.attribute("layer")
        .is_some_and(|layer| layer != "permanent")
}

fn crosses_layers(node: roxmltree::Node) -> bool {
    ["fromLayer", "toLayer"]
        .iter()
        .any(|name| node.attribute(*name).is_some_and(|l| l != "permanent"))
}

fn flag(node: roxmltree::Node, name: &str) -> bool {
    matches!(node.attribute(name), Some("true" | "1"))
}

fn single_sided(node: roxmltree::Node) -> bool {
    flag(node, "singleSide")
}

fn follows_the_road(node: roxmltree::Node) -> bool {
    flag(node, "perpToRoad")
}

fn detached(node: roxmltree::Node) -> bool {
    flag(node, "detachFromReferenceLine")
}

fn has_cubic_terms(node: roxmltree::Node) -> bool {
    ["bT", "cT", "dT"].iter().any(|name| {
        node.attribute(*name)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .is_some_and(|v| v != 0.0)
    })
}

fn blends(node: roxmltree::Node) -> bool {
    node.attribute("transitionLength")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .is_some_and(|v| v > 0.0)
}

/// The path of `node` below `scope`, with `<left>` and `<right>` as `side`.
fn path_below(scope: roxmltree::Node, node: roxmltree::Node) -> String {
    let mut names: Vec<&str> = node
        .ancestors()
        .take_while(|n| *n != scope)
        .map(|n| match n.tag_name().name() {
            "left" | "right" => "side",
            name => name,
        })
        .collect();
    names.reverse();
    names.join("/")
}

fn matches_path(feature: &Feature, path: &str) -> bool {
    match feature.path.strip_prefix("**/") {
        Some(tail) => path == tail || path.ends_with(&format!("/{tail}")),
        None => path == feature.path,
    }
}

/// One warning for each feature `scope` uses, with how many times. `scope`
/// is a `<road>`, a `<junction>`, or, with `document` set, the root, which
/// covers what lies outside both. `ids` are the road's and the junction's
/// `id`, empty where they don't apply.
fn scan(scope: roxmltree::Node, document: bool, ids: (&str, &str), out: &mut Vec<Warning>) {
    let mut counts = vec![0usize; FEATURES.len()];
    for node in scope.descendants().filter(|n| n.is_element()) {
        let path = path_below(scope, node);
        if document && matches!(path.split('/').next(), Some("road" | "junction")) {
            continue;
        }
        for (count, feature) in counts.iter_mut().zip(FEATURES) {
            if matches_path(feature, &path) && (feature.when)(node) {
                *count += 1;
            }
        }
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
    scan(root, true, ("", ""), &mut out);
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        let id = junction.attribute("id").unwrap_or_default();
        scan(junction, false, ("", id), &mut out);
    }
    for road in root.children().filter(|n| n.has_tag_name("road")) {
        let id = road.attribute("id").unwrap_or_default();
        scan(road, false, (id, ""), &mut out);
    }
    out
}
