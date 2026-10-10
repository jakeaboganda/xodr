//! Lane sections that give one side of the road only.
//!
//! A `<laneSection singleSide="true">` gives the lanes of the sides it has
//! an element for, and the other sides carry on from the sections before it.
//! The importer splits each into a full section, so each lane section has
//! both sides. A side carried into a later section is the same lanes, read
//! as they stand where it starts: their widths, heights and marks are
//! `sOffset`s from the section they were given in.

use super::links::LaneRef;
use super::{child, CenterDef, LaneDef, SectionDef};
use crate::road::{HeightDef, LaneExtent, LaneGeom};

/// The lanes of one side, as the section that gave them has them, and where
/// that section starts.
#[derive(Clone, Default)]
struct Side {
    lanes: Vec<LaneDef>,
    geoms: Vec<LaneGeom>,
    origin: f64,
}

/// The center lane, likewise.
#[derive(Clone, Default)]
struct Center {
    marks: Vec<super::MarkDef>,
    lane: Option<CenterDef>,
    origin: f64,
}

/// Each of `sections`, in ascending order of `start`, as a section with both
/// sides. `start` is where each begins along the road.
pub(super) fn full_sections(sections: &[roxmltree::Node], start: &[f64]) -> Vec<SectionDef> {
    let (mut left, mut right, mut center) = (Side::default(), Side::default(), Center::default());
    let mut out: Vec<SectionDef> = Vec::new();
    for (node, &at) in sections.iter().zip(start) {
        let mut own = SectionDef::parse(*node);
        let single = matches!(node.attribute("singleSide"), Some("true" | "1"));
        let gives = |tag| !single || child(*node, tag).is_some();
        let [own_left, own_right] = std::mem::take(&mut own.geoms);
        let (gives_left, gives_center, gives_right) =
            (gives("left"), gives("center"), gives("right"));
        if gives_left {
            left = Side {
                lanes: std::mem::take(&mut own.left),
                geoms: own_left,
                origin: at,
            };
        }
        if gives_right {
            right = Side {
                lanes: std::mem::take(&mut own.right),
                geoms: own_right,
                origin: at,
            };
        }
        if gives_center {
            center = Center {
                marks: std::mem::take(&mut own.center),
                lane: own.center_lane,
                origin: at,
            };
        }
        let previous = out.last_mut();
        let (left_lanes, left_geoms) = carry(&left, at, gives_left, previous.map(|p| &mut p.left));
        let previous = out.last_mut();
        let (right_lanes, right_geoms) =
            carry(&right, at, gives_right, previous.map(|p| &mut p.right));
        let (marks, lane) = if gives_center {
            (center.marks.clone(), center.lane)
        } else {
            let by = at - center.origin;
            (moved_marks(&center.marks, by), center.lane)
        };
        out.push(SectionDef {
            left: left_lanes,
            right: right_lanes,
            geoms: [left_geoms, right_geoms],
            center: marks,
            center_lane: lane,
            ..own
        });
    }
    out
}

/// A side's lanes and where they lie, for a section starting at `at`. A side
/// the section gave is as given. One carried on is moved to start at `at`,
/// each lane linking back to itself, and the section before linking on to it.
fn carry(
    side: &Side,
    at: f64,
    given: bool,
    previous: Option<&mut Vec<LaneDef>>,
) -> (Vec<LaneDef>, Vec<LaneGeom>) {
    if given {
        return (side.lanes.clone(), side.geoms.clone());
    }
    let by = at - side.origin;
    if let Some(previous) = previous {
        for lane in previous.iter_mut() {
            lane.succ_links = vec![LaneRef {
                id: lane.id,
                layer: None,
            }];
        }
    }
    let lanes = side
        .lanes
        .iter()
        .map(|lane| LaneDef {
            pred_links: vec![LaneRef {
                id: lane.id,
                layer: None,
            }],
            marks: moved_marks(&lane.marks, by),
            speeds: moved(&lane.speeds, by, |s| s.s_offset, |s, o| s.s_offset = o),
            rules: moved(&lane.rules, by, |r| r.0, |r, o| r.0 = o),
            access: moved(&lane.access, by, |r| r.0, |r, o| r.0 = o),
            materials: moved(&lane.materials, by, |r| r.0, |r, o| r.0 = o),
            visibility: moved(&lane.visibility, by, |r| r.0, |r, o| r.0 = o),
            ..lane.clone()
        })
        .collect();
    let geoms = side
        .geoms
        .iter()
        .map(|geom| LaneGeom {
            extent: match &geom.extent {
                LaneExtent::Width(w) => LaneExtent::Width(moved_cubics(w, by)),
                LaneExtent::Border(b) => LaneExtent::Border(moved_cubics(b, by)),
            },
            heights: moved_heights(&geom.heights, by),
            ..geom.clone()
        })
        .collect();
    (lanes, geoms)
}

fn moved_marks(marks: &[super::MarkDef], by: f64) -> Vec<super::MarkDef> {
    moved(marks, by, |m| m.s_offset, |m, o| m.s_offset = o)
}

/// The step records `items`, in ascending order of the offset `key` gives,
/// as they read `by` metres on: those before the new start dropped but the
/// one in force there, which starts at 0.
fn moved<T: Clone>(
    items: &[T],
    by: f64,
    key: impl Fn(&T) -> f64,
    set: impl Fn(&mut T, f64),
) -> Vec<T> {
    let first = items
        .iter()
        .rposition(|item| key(item) - by <= 1e-9)
        .unwrap_or(0);
    items[first..]
        .iter()
        .map(|item| {
            let mut item = item.clone();
            let offset = (key(&item) - by).max(0.0);
            set(&mut item, offset);
            item
        })
        .collect()
}

/// `cubics`, in ascending order of `start`, as they read `by` metres on.
fn moved_cubics(cubics: &[crate::road::Cubic], by: f64) -> Vec<crate::road::Cubic> {
    let first = cubics
        .iter()
        .rposition(|c| c.start - by <= 1e-9)
        .unwrap_or(0);
    cubics[first..].iter().map(|c| c.moved_on(by)).collect()
}

/// `<height>`s as they read `by` metres on. The height goes straight from one
/// record to the next, so where the new section starts between two it
/// starts at the height between them.
fn moved_heights(heights: &[HeightDef], by: f64) -> Vec<HeightDef> {
    let Some(first) = heights.iter().rposition(|h| h.s_offset - by <= 1e-9) else {
        return heights
            .iter()
            .map(|h| HeightDef {
                s_offset: h.s_offset - by,
                ..h.clone()
            })
            .collect();
    };
    let mut out: Vec<HeightDef> = heights[first..]
        .iter()
        .map(|h| HeightDef {
            s_offset: h.s_offset - by,
            ..h.clone()
        })
        .collect();
    if let Some(next) = heights.get(first + 1) {
        let (a, b) = (&heights[first], next);
        let span = b.s_offset - a.s_offset;
        let f = if span > 0.0 {
            (by - a.s_offset) / span
        } else {
            0.0
        };
        out[0] = HeightDef {
            s_offset: 0.0,
            inner: a.inner + (b.inner - a.inner) * f,
            outer: a.outer + (b.outer - a.outer) * f,
        };
    } else {
        out[0].s_offset = 0.0;
    }
    out
}
