//! The profile records of a road the crate repairs as it reads them: one it
//! drops for lacking where it starts, a number it reads as 0 because the
//! file writes one it can't read, and records out of order, which it sorts.

use super::{attr_f64, Warning};

/// One kind of record, under `parent` of a road or of a lane.
///
/// `start` is the attribute that orders the records. A record missing one of
/// `needed`, or carrying one the crate can't read, is dropped. One of `zero`
/// the file writes but the crate can't read is 0. A missing one is 0 too,
/// as libOpenDRIVE reads it, without a warning.
struct Kind {
    parent: &'static str,
    tag: &'static str,
    start: &'static [&'static str],
    needed: &'static [&'static str],
    zero: &'static [&'static str],
}

const CUBIC: &[&str] = &["a", "b", "c", "d"];

/// The kinds under a `<road>`. `<geometry>` is only ordered here: one the
/// crate can't read has its own [`Warning::GeometryDropped`].
const ROAD: &[Kind] = &[
    Kind {
        parent: "planView",
        tag: "geometry",
        start: &["s"],
        needed: &[],
        zero: &[],
    },
    Kind {
        parent: "elevationProfile",
        tag: "elevation",
        start: &["s"],
        needed: &["s"],
        zero: CUBIC,
    },
    Kind {
        parent: "lateralProfile",
        tag: "superelevation",
        start: &["s"],
        needed: &["s"],
        zero: CUBIC,
    },
    Kind {
        parent: "lateralProfile",
        tag: "shape",
        start: &["s", "t"],
        needed: &["s", "t"],
        zero: CUBIC,
    },
    Kind {
        parent: "",
        tag: "type",
        start: &["s"],
        needed: &["s"],
        zero: &[],
    },
];

/// The kinds under each `<lanes>`.
const LANES: &[Kind] = &[
    Kind {
        parent: "",
        tag: "laneOffset",
        start: &["s"],
        needed: &["s"],
        zero: CUBIC,
    },
    Kind {
        parent: "",
        tag: "laneSection",
        start: &["s"],
        needed: &[],
        zero: &["s"],
    },
];

/// The kinds under each `<lane>`.
const LANE: &[Kind] = &[
    Kind {
        parent: "",
        tag: "width",
        start: &["sOffset"],
        needed: &["a"],
        zero: &["sOffset", "b", "c", "d"],
    },
    Kind {
        parent: "",
        tag: "border",
        start: &["sOffset"],
        needed: &["a"],
        zero: &["sOffset", "b", "c", "d"],
    },
    Kind {
        parent: "",
        tag: "height",
        start: &["sOffset"],
        needed: &[],
        zero: &["sOffset", "inner", "outer"],
    },
    Kind {
        parent: "",
        tag: "roadMark",
        start: &["sOffset"],
        needed: &[],
        zero: &["sOffset"],
    },
    Kind {
        parent: "",
        tag: "speed",
        start: &["sOffset"],
        needed: &[],
        zero: &["sOffset"],
    },
    Kind {
        parent: "",
        tag: "material",
        start: &["sOffset"],
        needed: &[],
        zero: &["sOffset"],
    },
    Kind {
        parent: "",
        tag: "access",
        start: &["sOffset"],
        needed: &[],
        zero: &["sOffset"],
    },
    Kind {
        parent: "",
        tag: "rule",
        start: &["sOffset"],
        needed: &[],
        zero: &["sOffset"],
    },
];

/// Where a record is: its road, and for a record of a lane, the lane
/// section and the lane.
#[derive(Clone, Copy)]
struct At<'a> {
    road_id: &'a str,
    lane: Option<(usize, i32)>,
}

/// The warnings for the records of `road` the crate drops, reads as 0, or
/// sorts.
pub(super) fn check(road: roxmltree::Node) -> Vec<Warning> {
    let mut out = Vec::new();
    let at = At {
        road_id: road.attribute("id").unwrap_or_default(),
        lane: None,
    };
    for kind in ROAD {
        let parent = match kind.parent {
            "" => Some(road),
            name => road.children().find(|n| n.has_tag_name(name)),
        };
        if let Some(parent) = parent {
            records(parent, kind, at, &mut out);
        }
    }
    for lanes in road.children().filter(|n| n.has_tag_name("lanes")) {
        for kind in LANES {
            records(lanes, kind, at, &mut out);
        }
        let sections = lanes.children().filter(|n| n.has_tag_name("laneSection"));
        for (index, section) in sections.enumerate() {
            let sides = section.children().filter(|n| n.is_element());
            for lane in sides.flat_map(|side| side.children().filter(|n| n.has_tag_name("lane"))) {
                let Some(id) = lane.attribute("id").and_then(|v| v.trim().parse().ok()) else {
                    continue;
                };
                let at = At {
                    lane: Some((index, id)),
                    ..at
                };
                for kind in LANE {
                    records(lane, kind, at, &mut out);
                }
            }
        }
    }
    out
}

/// Check each record of `kind` under `parent`.
fn records(parent: roxmltree::Node, kind: &Kind, at: At, out: &mut Vec<Warning>) {
    let mut last: Option<Vec<f64>> = None;
    let mut sorted = true;
    for record in parent.children().filter(|n| n.has_tag_name(kind.tag)) {
        let s = kind.start.first().and_then(|name| attr_f64(record, name));
        let missing = kind
            .needed
            .iter()
            .find(|&&name| attr_f64(record, name).is_none());
        if let Some(missing) = missing {
            out.push(dropped(at, kind.tag.into(), s, missing));
            continue;
        }
        for &name in kind.zero {
            if record.attribute(name).is_some() && attr_f64(record, name).is_none() {
                out.push(unreadable(at, kind.tag.into(), s, name));
            }
        }
        let key: Option<Vec<f64>> = kind
            .start
            .iter()
            .map(|name| match kind.zero.contains(name) {
                true => Some(attr_f64(record, name).unwrap_or(0.0)),
                false => attr_f64(record, name),
            })
            .collect();
        if let Some(key) = key {
            if last.as_ref().is_some_and(|last| key < *last) {
                sorted = false;
            }
            last = Some(key);
        }
    }
    if !sorted {
        out.push(Warning::RecordsOutOfOrder {
            road_id: at.road_id.into(),
            record: kind.tag.into(),
            section: at.lane.map(|l| l.0),
            lane: at.lane.map(|l| l.1),
        });
    }
}

fn dropped(at: At, record: String, s: Option<f64>, attribute: &str) -> Warning {
    Warning::RecordDropped {
        road_id: at.road_id.into(),
        record,
        section: at.lane.map(|l| l.0),
        lane: at.lane.map(|l| l.1),
        s,
        attribute: attribute.into(),
    }
}

fn unreadable(at: At, record: String, s: Option<f64>, attribute: &str) -> Warning {
    Warning::RecordValueUnreadable {
        road_id: at.road_id.into(),
        record,
        section: at.lane.map(|l| l.0),
        lane: at.lane.map(|l| l.1),
        s,
        attribute: attribute.into(),
    }
}
