//! Junction boundaries, elevation grids and cross paths, placed once every
//! road is baked.

use super::{attr_f64, attr_i32, child, BakedRoad, Warning};
use crate::coords::Point;
use crate::junction::{
    CrossPath, CrossPathEnd, ElevationGrid, GridRow, JunctionArea, JunctionGroup, JunctionGroupKind,
};
use crate::network::LaneId;
use crate::road::{Road, RoadSection};

/// How far apart in plan, in metres, two boundary segments may leave off and
/// begin before the boundary counts as open.
const GAP_TOLERANCE: f32 = 0.1;

/// Every junction with a `<boundary>` or an `<elevationGrid>`, in file
/// order, and what went wrong reading them.
pub(super) fn place(
    root: roxmltree::Node,
    roads: &[(roxmltree::Node, BakedRoad)],
) -> (Vec<JunctionArea>, Vec<Warning>) {
    let road = |id: &str| roads.iter().map(|(_, r)| &r.road).find(|r| r.od_id == id);
    let (mut out, mut warnings) = (Vec::new(), Vec::new());
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        let od_id = junction.attribute("id").unwrap_or_default().to_string();
        let boundary = child(junction, "boundary");
        let grid = child(junction, "elevationGrid").and_then(|g| {
            let placed = grid(junction, g);
            if placed.is_none() {
                warnings.push(Warning::ElevationGridDropped {
                    junction_id: od_id.clone(),
                });
            }
            placed
        });
        if boundary.is_none() && grid.is_none() {
            continue;
        }
        let ring = boundary.map_or_else(Vec::new, |b| ring(&od_id, b, &road, &mut warnings));
        if grid.is_some() {
            warnings.push(Warning::ElevationGridNotApplied {
                junction_id: od_id.clone(),
            });
        }
        out.push(JunctionArea {
            od_id,
            boundary: ring,
            grid,
        });
    }
    (out, warnings)
}

/// Every `<junctionGroup>`, in file order, with the junctions it names that
/// the file has, and a warning for each it names that the file lacks, and
/// for a `type` the spec does not allow.
pub(super) fn groups(root: roxmltree::Node) -> (Vec<JunctionGroup>, Vec<Warning>) {
    let junctions: Vec<&str> = root
        .children()
        .filter(|n| n.has_tag_name("junction"))
        .filter_map(|n| n.attribute("id"))
        .collect();
    let (mut out, mut warnings) = (Vec::new(), Vec::new());
    for node in root.children().filter(|n| n.has_tag_name("junctionGroup")) {
        let text = |name| node.attribute(name).unwrap_or_default().to_string();
        let od_id = text("id");
        let kind = match node.attribute("type") {
            Some("roundabout") => JunctionGroupKind::Roundabout,
            Some("complexJunction") => JunctionGroupKind::ComplexJunction,
            Some("highwayInterchange") => JunctionGroupKind::HighwayInterchange,
            Some("unknown") => JunctionGroupKind::Unknown,
            other => {
                warnings.push(Warning::UnknownJunctionGroupType {
                    group_id: od_id.clone(),
                    kind: other.unwrap_or_default().to_string(),
                });
                JunctionGroupKind::Unknown
            }
        };
        let mut members = Vec::new();
        for reference in node
            .children()
            .filter(|n| n.has_tag_name("junctionReference"))
        {
            let junction = reference.attribute("junction").unwrap_or_default();
            if members.iter().any(|m| m == junction) {
                continue;
            }
            if junctions.contains(&junction) {
                members.push(junction.to_string());
            } else {
                warnings.push(Warning::JunctionReferenceDropped {
                    group_id: od_id.clone(),
                    junction_id: junction.to_string(),
                });
            }
        }
        out.push(JunctionGroup {
            od_id,
            name: text("name"),
            kind,
            junctions: members,
        });
    }
    (out, warnings)
}

/// Every `<crossPath>` of every junction, in file order, with its identity,
/// and a warning for each whose roads or lanes the load lacks.
pub(super) fn cross_paths(
    root: roxmltree::Node,
    roads: &[(roxmltree::Node, BakedRoad)],
) -> (
    Vec<CrossPath>,
    Vec<super::CrossPathProvenance>,
    Vec<Warning>,
) {
    let road = |id: Option<&str>| {
        roads
            .iter()
            .map(|(_, r)| &r.road)
            .find(|r| Some(r.od_id.as_str()) == id)
    };
    let (mut out, mut provenance, mut warnings) = (Vec::new(), Vec::new(), Vec::new());
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        let junction_id = junction.attribute("id").unwrap_or_default();
        for node in junction.children().filter(|n| n.has_tag_name("crossPath")) {
            let crossing_id = node.attribute("crossingRoad");
            let crossing = road(crossing_id);
            let end = |at: &str, link: Option<roxmltree::Node>, last: bool| {
                let at_id = node.attribute(at);
                let crossing = crossing.ok_or(crossing_id.unwrap_or_default())?;
                let at = road(at_id).ok_or(at_id.unwrap_or_default())?;
                let link = link.ok_or(at_id.unwrap_or_default())?;
                let s = attr_f64(link, "s")
                    .filter(|s| at.on_road(*s))
                    .ok_or(at_id.unwrap_or_default())?;
                let lane = |section: &RoadSection, name: &str| -> Option<LaneId> {
                    let od_id = attr_i32(link, name)?;
                    section
                        .lanes
                        .iter()
                        .find(|(id, _)| *id == od_id)
                        .map(|&(_, l)| l)
                };
                let mut permanent = crossing
                    .sections
                    .iter()
                    .filter(|s| s.layer == crate::Layer::Permanent);
                let crossing_end = if last {
                    permanent.next_back()
                } else {
                    permanent.next()
                };
                Ok::<_, &str>(CrossPathEnd {
                    lane: at
                        .section_at(s)
                        .and_then(|section| lane(section, "from"))
                        .ok_or(at_id.unwrap_or_default())?,
                    s,
                    crossing_lane: crossing_end
                        .and_then(|section| lane(section, "to"))
                        .ok_or(crossing_id.unwrap_or_default())?,
                })
            };
            let od_id = node.attribute("id").unwrap_or_default().to_string();
            match (
                crossing,
                end("roadAtStart", child(node, "startLaneLink"), false),
                end("roadAtEnd", child(node, "endLaneLink"), true),
            ) {
                (Some(crossing), Ok(start), Ok(end)) => {
                    out.push(CrossPath {
                        crossing: crossing.id,
                        start,
                        end,
                    });
                    provenance.push(super::CrossPathProvenance {
                        junction_id: junction_id.to_string(),
                        od_id,
                    });
                }
                (_, start, end) => warnings.push(Warning::CrossPathDropped {
                    junction_id: junction_id.to_string(),
                    cross_path_id: od_id,
                    road_id: start.err().or(end.err()).unwrap_or_default().to_string(),
                }),
            }
        }
    }
    (out, provenance, warnings)
}

/// A junction's `<elevationGrid>`, on the straight line its `<planView>`
/// gives, or `None` without one, with a line that is not one `<line>`
/// geometry, or without a `gridSpacing` above 0.
///
/// The spec requires `sStart`, and every row's `center`. A missing `sStart`
/// is 0, and a row without a `center` is skipped, as are the heights in a
/// list that are not numbers.
fn grid(junction: roxmltree::Node, node: roxmltree::Node) -> Option<ElevationGrid> {
    let geometries: Vec<_> = child(junction, "planView")?
        .children()
        .filter(|n| n.has_tag_name("geometry"))
        .collect();
    let [line] = geometries[..] else {
        return None;
    };
    child(line, "line")?;
    let spacing = attr_f64(node, "gridSpacing").filter(|s| *s > 0.0)?;
    let at = |name| attr_f64(line, name).unwrap_or(0.0);
    let heading = at("hdg");
    let (sin, cos) = heading.sin_cos();
    let back = at("s");
    let heights = |row: roxmltree::Node, name| -> Vec<f64> {
        row.attribute(name)
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|v| v.parse::<f64>().ok().filter(|v| v.is_finite()))
            .collect()
    };
    Some(ElevationGrid {
        origin: [at("x") - back * cos, at("y") - back * sin],
        heading,
        s_start: attr_f64(node, "sStart").unwrap_or(0.0),
        spacing,
        rows: node
            .children()
            .filter(|n| n.has_tag_name("elevation"))
            .filter_map(|row| {
                Some(GridRow {
                    center: attr_f64(row, "center")?,
                    left: heights(row, "left"),
                    right: heights(row, "right"),
                })
            })
            .collect(),
    })
}

/// A junction's `<boundary>` as a ring of points on the road surface,
/// counter-clockwise, and a warning for each segment it drops, for a ring
/// it had to turn, and for one that does not close.
///
/// A `lane` segment runs along the outer border of its `boundaryLane`, from
/// `sStart` to `sEnd`, at each of the road's lane stations. A `joint`
/// segment runs across its road's `contactPoint`, from the outer border of
/// `jointLaneStart` to that of `jointLaneEnd`, or across the whole road
/// where either is missing. The spec orders a joint by its lanes. Where the
/// segment before it ends nearer its end, the crate turns it round.
fn ring<'a>(
    junction_id: &str,
    boundary: roxmltree::Node,
    road: &impl Fn(&str) -> Option<&'a Road>,
    warnings: &mut Vec<Warning>,
) -> Vec<Point> {
    let mut segments: Vec<Vec<Point>> = Vec::new();
    for node in boundary.children().filter(|n| n.has_tag_name("segment")) {
        let road_id = node.attribute("roadId").unwrap_or_default();
        let points = road(road_id).and_then(|r| match node.attribute("type") {
            Some("lane") => along(r, node),
            Some("joint") => across(r, node),
            _ => None,
        });
        match points {
            Some(points) if !points.is_empty() => segments.push(points),
            _ => warnings.push(Warning::BoundarySegmentDropped {
                junction_id: junction_id.to_string(),
                road_id: road_id.to_string(),
            }),
        }
    }
    let mut ring: Vec<Point> = Vec::new();
    let mut gap = 0.0_f32;
    let reach = |p: Point, q: Point| (p.x - q.x).hypot(p.y - q.y);
    for mut segment in segments.clone() {
        let reverse = match ring.last() {
            Some(&end) => reach(end, segment[segment.len() - 1]) < reach(end, segment[0]),
            None => segments.get(1).is_some_and(|next| {
                let ends = [next[0], next[next.len() - 1]];
                let near = |p: Point| ends.iter().map(|&e| reach(p, e)).fold(f32::MAX, f32::min);
                near(segment[0]) < near(segment[segment.len() - 1])
            }),
        };
        if reverse {
            segment.reverse();
        }
        if let Some(&end) = ring.last() {
            gap = gap.max(reach(end, segment[0]));
        }
        for p in segment {
            if ring.last().is_none_or(|&q| reach(p, q) > 1e-3) {
                ring.push(p);
            }
        }
    }
    if let (Some(&first), Some(&last)) = (ring.first(), ring.last()) {
        gap = gap.max(reach(last, first));
    }
    if ring.len() > 1 && reach(ring[0], ring[ring.len() - 1]) <= 1e-3 {
        ring.pop();
    }
    if gap > GAP_TOLERANCE {
        warnings.push(Warning::BoundaryNotClosed {
            junction_id: junction_id.to_string(),
            gap: f64::from(gap),
        });
    }
    if signed_area(&ring) < 0.0 {
        ring.reverse();
        warnings.push(Warning::BoundaryClockwise {
            junction_id: junction_id.to_string(),
        });
    }
    ring
}

/// Twice the area `ring` encloses in plan: positive when it runs
/// counter-clockwise.
fn signed_area(ring: &[Point]) -> f32 {
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum()
}

/// How far off the ends of its road, in metres, a segment's station may be
/// before it counts as off the road, rather than as rounding.
const OFF_ROAD: f64 = 0.01;

/// Where along `road` a segment's `sStart`, `sEnd` or `contactPoint` is:
/// `begin` or `start` for its start, `end` for its end, or a number. `None`
/// for a number more than [`OFF_ROAD`] off the road.
fn station(road: &Road, value: Option<&str>) -> Option<f64> {
    match value? {
        "begin" | "start" => Some(0.0),
        "end" => Some(road.length),
        v => v
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|s| (-OFF_ROAD..=road.length + OFF_ROAD).contains(s))
            .map(|s| s.clamp(0.0, road.length)),
    }
}

/// The outer border of lane `od_id` at `s`: its `t`, and the point on the
/// surface there. The center lane's border is the line between the sides.
fn border(road: &Road, od_id: i32, s: f64) -> Option<Point> {
    let section = road.section_at(s)?;
    let t = if od_id == 0 {
        road.base(s)
    } else {
        road.across(section, s).find(|a| a.od_id == od_id)?.outer_t
    };
    Some(road.lane_surface(section, od_id, s, t).0)
}

/// A `lane` segment: the outer border of its `boundaryLane`, from `sStart`
/// to `sEnd`.
fn along(road: &Road, node: roxmltree::Node) -> Option<Vec<Point>> {
    let od_id = attr_i32(node, "boundaryLane")?;
    let from = station(road, node.attribute("sStart"))?;
    let to = station(road, node.attribute("sEnd"))?;
    let (low, high) = (from.min(to), from.max(to));
    let mut stations: Vec<f64> = road
        .sections
        .iter()
        .flat_map(|section| section.stations.iter().copied())
        .filter(|&s| s > low && s < high)
        .chain([low, high])
        .collect();
    stations.sort_by(f64::total_cmp);
    stations.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    if from > to {
        stations.reverse();
    }
    stations
        .into_iter()
        .map(|s| border(road, od_id, s))
        .collect()
}

/// A `joint` segment: across the road at its `contactPoint`, over the whole
/// of `jointLaneStart`, `jointLaneEnd` and the lanes between, through each
/// border, or from the outermost right lane to the outermost left one where
/// either is missing. Two lanes on one side end at the inner one's inner
/// border.
fn across(road: &Road, node: roxmltree::Node) -> Option<Vec<Point>> {
    let s = station(road, node.attribute("contactPoint"))?;
    let section = road.section_at(s)?;
    let lane = |name| attr_i32(node, name);
    let ids: Vec<i32> = section
        .right
        .iter()
        .rev()
        .map(|l| l.id)
        .chain([0])
        .chain(section.left.iter().map(|l| l.id))
        .collect();
    let index = |id| ids.iter().position(|&i| i == id);
    let borders = |id: i32| {
        let at = index(id)?;
        Some(match id.signum() {
            -1 => (at, at + 1),
            1 => (at - 1, at),
            _ => (at, at),
        })
    };
    let (from, to) = match (lane("jointLaneStart"), lane("jointLaneEnd")) {
        (Some(a), Some(b)) => (borders(a)?, borders(b)?),
        _ => ((0, 0), (ids.len() - 1, ids.len() - 1)),
    };
    let (low, high) = (from.0.min(to.0), from.1.max(to.1));
    let mut points: Vec<Point> = ids[low..=high]
        .iter()
        .map(|&id| border(road, id, s))
        .collect::<Option<_>>()?;
    if from.0 > to.0 {
        points.reverse();
    }
    Some(points)
}
