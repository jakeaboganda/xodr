//! Speed limit signs, laid along the lanes they apply to once every lane is
//! linked.

use std::collections::{HashMap, HashSet};

use super::signals::Signals;
use super::{properties, BakedRoad, Orientation, Warning};
use crate::along::sorted;
use crate::road::{along, RoadSection};
use crate::{Along, Direction, Lane, LaneId, Semantic, SignalBoard, SpeedLimit};

/// What a speed sign does to the lanes it applies to: set a limit, or end
/// the one a sign before it set.
#[derive(Clone, Copy, PartialEq)]
enum Rule {
    Set(SpeedLimit),
    End,
}

/// One speed sign at one place it applies: its road and `s` there, the
/// lanes there it applies to, and which way its traffic runs on a two-way
/// lane.
struct Sign {
    road: usize,
    s: f64,
    lanes: Vec<LaneId>,
    rule: Rule,
    orientation: Orientation,
}

/// Where a lane lies: its road, as a position in `roads`, and its lane
/// section there.
type Place<'a> = (usize, &'a RoadSection);

/// `speed_limits`, the limits the map's `<speed>` records set, with the
/// limits its speed signs set laid over them, and a warning for each sign
/// whose limit a record contradicts.
///
/// A sign sets a limit with a `<speed>` semantic of type `maximum` or
/// `zone`, and ends one with `maximumEnd` or `zoneEnd`, on its own lanes or
/// on those its board's signs name. The limit runs from where the sign
/// stands, and from each place a `<signalReference>` applies it, along each
/// lane it applies to in the lane's direction of travel, on into the lanes
/// after it on the same road, to the next speed sign on that lane or the end
/// of the road. A two-way lane takes the way the sign faces.
pub(super) fn signs_over(
    speed_limits: Vec<Along<SpeedLimit>>,
    roads: &[(roxmltree::Node, BakedRoad)],
    lanes: &[Lane],
    signals: &Signals,
) -> (Vec<Along<SpeedLimit>>, Vec<Warning>) {
    let road_index: HashMap<&str, usize> = roads
        .iter()
        .enumerate()
        .map(|(k, (_, r))| (r.road.od_id.as_str(), k))
        .collect();
    let mut place: HashMap<LaneId, Place> = HashMap::new();
    for (k, (_, road)) in roads.iter().enumerate() {
        for section in &road.road.sections {
            for &(_, id) in &section.lanes {
                place.insert(id, (k, section));
            }
        }
    }
    let signs = speed_signs(signals, &road_index, &place);
    if signs.is_empty() {
        return (speed_limits, Vec::new());
    }
    let mut set = Vec::new();
    let mut warnings = Vec::new();
    for (k, sign) in signs.iter().enumerate() {
        let road = &roads[sign.road].1.road;
        for &lane in &sign.lanes {
            let reach = reach(k, lane, &signs, lanes, &place);
            let Rule::Set(limit) = sign.rule else {
                continue;
            };
            let stretches: Vec<Along<SpeedLimit>> = reach
                .into_iter()
                .map(|(lane, from, to)| Along {
                    lane,
                    from,
                    to,
                    value: limit,
                })
                .collect();
            let against = stretches.iter().find_map(|s| contradicts(s, &speed_limits));
            if let Some(record) = against {
                let od_lane = place[&lane].1.lanes.iter().find(|(_, id)| *id == lane);
                warnings.push(Warning::SpeedLimitDisagrees {
                    road_id: road.od_id.clone(),
                    s: sign.s,
                    lane: od_lane.map_or(0, |l| l.0),
                    sign: limit,
                    record,
                });
            }
            set.extend(stretches);
        }
    }
    (overlay(speed_limits, set), warnings)
}

/// Every speed sign and board sign, at its own place and at each place a
/// `<signalReference>` applies it, with the lanes there it applies to.
fn speed_signs(
    signals: &Signals,
    road_index: &HashMap<&str, usize>,
    place: &HashMap<LaneId, Place>,
) -> Vec<Sign> {
    let mut out = Vec::new();
    for (signal, prov) in signals.baked.iter().zip(&signals.provenance) {
        let mut rules = vec![(rule(&signal.semantics), &signal.lanes)];
        for board in &signal.boards {
            if let SignalBoard::Static(signs) = board {
                rules.extend(signs.iter().map(|s| (rule(&s.semantics), &s.lanes)));
            }
        }
        let at = std::iter::once((prov.road_id.as_str(), prov.s, prov.orientation))
            .chain((prov.references.iter()).map(|r| (r.road_id.as_str(), r.s, r.orientation)));
        for (road_id, s, orientation) in at {
            let Some(&road) = road_index.get(road_id) else {
                continue;
            };
            for (rule, lanes) in &rules {
                let Some(rule) = *rule else {
                    continue;
                };
                let here: Vec<LaneId> = (lanes.iter())
                    .copied()
                    .filter(|l| {
                        place.get(l).is_some_and(|(r, section)| {
                            *r == road && section.start <= s + 1e-9 && s <= section.end + 1e-9
                        })
                    })
                    .collect();
                if !here.is_empty() {
                    out.push(Sign {
                        road,
                        s,
                        lanes: here,
                        rule,
                        orientation,
                    });
                }
            }
        }
    }
    out
}

/// The speed rule `semantics` give, from their first `<speed>` of type
/// `maximum`, `zone`, `maximumEnd` or `zoneEnd`.
fn rule(semantics: &[Semantic]) -> Option<Rule> {
    semantics.iter().find_map(|s| match s {
        Semantic::Speed { kind, .. } if kind == "maximumEnd" || kind == "zoneEnd" => {
            Some(Rule::End)
        }
        Semantic::Speed { kind, value, unit } if kind == "maximum" || kind == "zone" => {
            let as_maximum = Semantic::Speed {
                kind: "maximum".into(),
                value: *value,
                unit: *unit,
            };
            properties::speed_limit(&[as_maximum]).map(Rule::Set)
        }
        _ => None,
    })
}

/// How far along `lane` and the lanes after it on its road sign `k` of `signs`
/// reaches, as `(lane, from, to)` stretches in metres along each lane: from
/// the sign to the next of `signs` on the way, or to the end of the road.
fn reach(
    k: usize,
    lane: LaneId,
    signs: &[Sign],
    lanes: &[Lane],
    place: &HashMap<LaneId, Place>,
) -> Vec<(LaneId, f32, f32)> {
    let sign = &signs[k];
    let forward = |lane: &Lane| match lane.direction {
        Direction::Forward => true,
        Direction::Backward => false,
        Direction::Both => sign.orientation != Orientation::Negative,
    };
    let at = |lane: LaneId, s: f64| {
        let (_, section) = place[&lane];
        along(lanes[lane.0].center.points(), &section.stations, s)
    };
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut todo = vec![(lane, Some(at(lane, sign.s)))];
    while let Some((id, start)) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        let current = &lanes[id.0];
        let ahead = forward(current);
        let length = current.center.length();
        let from = start.unwrap_or(if ahead { 0.0 } else { length });
        let next = (signs.iter().enumerate())
            .filter(|&(j, other)| j != k && other.road == sign.road && other.lanes.contains(&id))
            .map(|(_, other)| at(id, other.s))
            .filter(|&p| match ahead {
                true => p > from + 1e-4,
                false => p < from - 1e-4,
            })
            .min_by(|a, b| match ahead {
                true => a.total_cmp(b),
                false => b.total_cmp(a),
            });
        let end = next.unwrap_or(if ahead { length } else { 0.0 });
        let (low, high) = (from.min(end), from.max(end));
        if high - low > 1e-4 {
            out.push((id, low, high));
        }
        if next.is_none() {
            let on_road = |n: &&LaneId| place.get(n).is_some_and(|(r, _)| *r == sign.road);
            todo.extend(
                current
                    .successors
                    .iter()
                    .filter(on_road)
                    .map(|&n| (n, None)),
            );
        }
    }
    out
}

/// The record limit in `records` a sign's `stretch` contradicts, if one on
/// its lane overlaps it with another value.
fn contradicts(stretch: &Along<SpeedLimit>, records: &[Along<SpeedLimit>]) -> Option<SpeedLimit> {
    records
        .iter()
        .find(|r| {
            r.lane == stretch.lane
                && r.from < stretch.to
                && stretch.from < r.to
                && r.value != stretch.value
        })
        .map(|r| r.value)
}

/// `records` with `set` laid over them: each record cut where a stretch of
/// `set` on its lane covers it. Where two of `set` overlap, the later wins.
fn overlay(records: Vec<Along<SpeedLimit>>, set: Vec<Along<SpeedLimit>>) -> Vec<Along<SpeedLimit>> {
    let mut by_lane: HashMap<LaneId, Vec<Along<SpeedLimit>>> = HashMap::new();
    for stretch in set {
        let laid = by_lane.entry(stretch.lane).or_default();
        *laid = cut(std::mem::take(laid), stretch.from, stretch.to);
        laid.push(stretch);
    }
    let mut out = Vec::new();
    for record in records {
        match by_lane.get(&record.lane) {
            Some(laid) => {
                let mut pieces = vec![record];
                for s in laid {
                    pieces = cut(pieces, s.from, s.to);
                }
                out.extend(pieces);
            }
            None => out.push(record),
        }
    }
    out.extend(by_lane.into_values().flatten());
    sorted(out)
}

/// `stretches` with `[from, to]` cut out of each.
fn cut<T: Clone>(stretches: Vec<Along<T>>, from: f32, to: f32) -> Vec<Along<T>> {
    let mut out = Vec::new();
    for s in stretches {
        if s.to <= from || to <= s.from {
            out.push(s);
            continue;
        }
        if s.from < from {
            out.push(Along {
                to: from,
                ..s.clone()
            });
        }
        if to < s.to {
            out.push(Along { from: to, ..s });
        }
    }
    out
}
