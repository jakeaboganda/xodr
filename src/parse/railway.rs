//! Railway switches and stations, read once every road is baked.

use super::{attr_f64, child, BakedRoad, Warning};
use crate::railway::{
    Partner, Platform, PlatformSegment, Station, Switch, SwitchPosition, TrackPoint,
};
use crate::road::{Road, Side};

/// Every road's `<railroad><switch>`es, road by road in file order, and a
/// warning for each the crate drops for naming a road the load lacks, or
/// for missing its tracks, or for a `position` or `dir` the spec does not
/// allow.
pub(super) fn switches(roads: &[(roxmltree::Node, BakedRoad)]) -> (Vec<Switch>, Vec<Warning>) {
    let find = |id: &str| find(roads, id);
    let (mut out, mut warnings) = (Vec::new(), Vec::new());
    for (node, baked) in roads {
        let switches = child(*node, "railroad")
            .into_iter()
            .flat_map(|r| r.children())
            .filter(|n| n.has_tag_name("switch"));
        for switch in switches {
            let text = |name| switch.attribute(name).unwrap_or_default().to_string();
            let track = |node: Option<roxmltree::Node>| {
                let node = node?;
                let road = find(node.attribute("id")?)?;
                let forward = match node.attribute("dir")? {
                    "+" => true,
                    "-" => false,
                    _ => return None,
                };
                let s = attr_f64(node, "s").filter(|s| road.on_road(*s))?;
                Some(TrackPoint {
                    road: road.id,
                    s,
                    forward,
                })
            };
            let position = match switch.attribute("position") {
                Some("dynamic") => Some(SwitchPosition::Dynamic),
                Some("straight") => Some(SwitchPosition::Straight),
                Some("turn") => Some(SwitchPosition::Turn),
                Some(_) => Some(SwitchPosition::Unknown),
                None => None,
            };
            match (
                position,
                track(child(switch, "mainTrack")),
                track(child(switch, "sideTrack")),
            ) {
                (Some(position), Some(main), Some(side)) => out.push(Switch {
                    road: baked.road.id,
                    od_id: text("id"),
                    name: text("name"),
                    position,
                    main,
                    side,
                    partner: child(switch, "partner").and_then(|p| {
                        Some(Partner {
                            id: p.attribute("id")?.to_string(),
                            name: p.attribute("name").unwrap_or_default().to_string(),
                        })
                    }),
                }),
                _ => warnings.push(Warning::RailwayDropped {
                    element: "switch".into(),
                    id: text("id"),
                    road_id: baked.road.od_id.clone(),
                }),
            }
        }
    }
    (out, warnings)
}

/// Every `<station>`, in file order, with each platform segment on a baked
/// road, and a warning for each segment dropped for naming a road the load
/// lacks, a stretch off its ends or with `sEnd` before `sStart`, or a
/// `side` the spec does not allow.
pub(super) fn stations(
    root: roxmltree::Node,
    roads: &[(roxmltree::Node, BakedRoad)],
) -> (Vec<Station>, Vec<Warning>) {
    let (mut out, mut warnings) = (Vec::new(), Vec::new());
    for station in root.children().filter(|n| n.has_tag_name("station")) {
        let text = |n: roxmltree::Node, name| n.attribute(name).unwrap_or_default().to_string();
        let platforms = station
            .children()
            .filter(|n| n.has_tag_name("platform"))
            .map(|platform| Platform {
                od_id: text(platform, "id"),
                name: text(platform, "name"),
                segments: platform
                    .children()
                    .filter(|n| n.has_tag_name("segment"))
                    .filter_map(|segment| {
                        let placed = (|| {
                            let road = find(roads, segment.attribute("roadId")?)?;
                            let s_start =
                                attr_f64(segment, "sStart").filter(|s| road.on_road(*s))?;
                            let s_end = attr_f64(segment, "sEnd")
                                .filter(|s| road.on_road(*s) && *s >= s_start)?;
                            let side = match segment.attribute("side")? {
                                "left" => Side::Left,
                                "right" => Side::Right,
                                _ => return None,
                            };
                            Some(PlatformSegment {
                                road: road.id,
                                s_start,
                                s_end,
                                side,
                            })
                        })();
                        if placed.is_none() {
                            warnings.push(Warning::RailwayDropped {
                                element: "platform segment".into(),
                                id: text(platform, "id"),
                                road_id: text(segment, "roadId"),
                            });
                        }
                        placed
                    })
                    .collect(),
            })
            .collect();
        out.push(Station {
            od_id: text(station, "id"),
            name: text(station, "name"),
            kind: text(station, "type"),
            platforms,
        });
    }
    (out, warnings)
}

/// The baked road with `<road id>` `id`.
fn find<'a>(roads: &'a [(roxmltree::Node, BakedRoad)], id: &str) -> Option<&'a Road> {
    roads.iter().map(|(_, r)| &r.road).find(|r| r.od_id == id)
}
