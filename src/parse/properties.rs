//! What holds along each baked lane: its road's `<type>`s, its speed
//! limits from its own `<speed>`s or its road's, and its own `<rule>`s,
//! `<access>`es, `<material>`s and `<visibility>`s.

use super::{along, attr_f64, material, BakedRoad, Warning};
use crate::coords::Point;
use crate::road::RoadSection;
use crate::{Access, Along, Lane, LaneId, Material, RoadType, SpeedLimit, Visibility};

/// A `<speed>` as the file gives it.
#[derive(Clone, PartialEq)]
pub(super) enum SpeedDef {
    Limit(SpeedLimit),
    Undefined,
    Unreadable { max: String, unit: String },
}

impl SpeedDef {
    /// A `<speed>`'s `max` in its `unit`. `words` allows the road's `no
    /// limit` and `undefined`, which a lane's `max` may not be.
    ///
    /// The spec says a missing unit is m/s. A `max` below 0, a unit other
    /// than `m/s`, `km/h` or `mph`, or a limit too large for an `f32` is
    /// unreadable.
    fn parse(node: roxmltree::Node, words: bool) -> Self {
        let max = node.attribute("max").unwrap_or_default();
        match max {
            "no limit" if words => return Self::Limit(SpeedLimit::Unlimited),
            "undefined" if words => return Self::Undefined,
            _ => {}
        }
        let scale = match node.attribute("unit") {
            None | Some("m/s") => Some(1.0),
            Some("km/h") => Some(1.0 / 3.6),
            Some("mph") => Some(0.44704),
            Some(_) => None,
        };
        let limit = attr_f64(node, "max")
            .filter(|v| *v >= 0.0)
            .zip(scale)
            .map(|(v, k)| (v * k) as f32)
            .filter(|v| v.is_finite());
        match limit {
            Some(mps) => Self::Limit(SpeedLimit::Max(mps)),
            None => Self::Unreadable {
                max: max.to_string(),
                unit: node.attribute("unit").unwrap_or_default().to_string(),
            },
        }
    }

    fn limit(&self) -> Option<SpeedLimit> {
        match self {
            Self::Limit(limit) => Some(*limit),
            Self::Undefined | Self::Unreadable { .. } => None,
        }
    }
}

/// A road's `<type>`: what the road is from `s` on, the country whose rules
/// it follows, and the speed limit it sets, if it has a `<speed>`.
pub(super) struct TypeDef {
    s: f64,
    kind: RoadType,
    country: String,
    speed: Option<SpeedDef>,
}

/// A lane's `<speed>`: its limit from `s_offset` into its lane section on.
#[derive(Clone)]
pub(super) struct LaneSpeedDef {
    pub(super) s_offset: f64,
    speed: SpeedDef,
}

/// A road's `<type>`s, in ascending `s`.
///
/// The spec requires `s` and `type`, and says the types come in ascending
/// `s`. A type without `s` is skipped, and types out of order are sorted.
pub(super) fn road_types(road: roxmltree::Node) -> Vec<TypeDef> {
    let mut types: Vec<TypeDef> = road
        .children()
        .filter(|n| n.has_tag_name("type"))
        .filter_map(|n| {
            Some(TypeDef {
                s: attr_f64(n, "s")?,
                kind: road_type(n.attribute("type")),
                country: n.attribute("country").unwrap_or_default().to_string(),
                speed: n
                    .children()
                    .find(|c| c.has_tag_name("speed"))
                    .map(|c| SpeedDef::parse(c, true)),
            })
        })
        .collect();
    types.sort_by(|a, b| a.s.total_cmp(&b.s));
    types
}

/// A lane's `<speed>`s, in ascending `sOffset`.
///
/// The spec requires `sOffset` and says it is at least 0, and says the
/// entries come in ascending `sOffset`. A missing or negative one is 0, as
/// for a `<height>`, and entries out of order are sorted.
pub(super) fn lane_speeds(lane: roxmltree::Node) -> Vec<LaneSpeedDef> {
    let mut speeds: Vec<LaneSpeedDef> = lane
        .children()
        .filter(|n| n.has_tag_name("speed"))
        .map(|n| LaneSpeedDef {
            s_offset: attr_f64(n, "sOffset").unwrap_or(0.0).max(0.0),
            speed: SpeedDef::parse(n, false),
        })
        .collect();
    speeds.sort_by(|a, b| a.s_offset.total_cmp(&b.s_offset));
    speeds
}

/// A lane's `<access>` as the file gives it.
#[derive(Clone, PartialEq)]
pub(super) enum AccessDef {
    Restricted(Access),
    Unrestricted,
    Unreadable { rule: String },
}

impl AccessDef {
    /// An `<access>`. Its road users are its `<restriction>`s' `type`s,
    /// after the `restriction` attribute of files before 1.8.
    fn parse(node: roxmltree::Node) -> Self {
        let users: Vec<String> = node
            .attribute("restriction")
            .into_iter()
            .chain(
                node.children()
                    .filter(|n| n.has_tag_name("restriction"))
                    .filter_map(|n| n.attribute("type")),
            )
            .filter(|u| !u.is_empty())
            .map(String::from)
            .collect();
        match node.attribute("rule") {
            Some("deny") if users.iter().any(|u| u == "none") => Self::Unrestricted,
            Some("allow") => Self::Restricted(Access::Allow(users)),
            Some("deny") => Self::Restricted(Access::Deny(users)),
            rule => Self::Unreadable {
                rule: rule.unwrap_or_default().to_string(),
            },
        }
    }

    fn access(&self) -> Option<Access> {
        match self {
            Self::Restricted(access) => Some(access.clone()),
            Self::Unrestricted | Self::Unreadable { .. } => None,
        }
    }
}

/// `lane`'s children named `tag` that `read` reads, each at its `sOffset`,
/// in ascending `sOffset`. A missing or negative `sOffset` is 0, as for a
/// `<height>`, and entries out of order are sorted.
fn by_offset<T>(
    lane: roxmltree::Node,
    tag: &str,
    read: impl Fn(roxmltree::Node) -> Option<T>,
) -> Vec<(f64, T)> {
    let mut out: Vec<(f64, T)> = lane
        .children()
        .filter(|n| n.has_tag_name(tag))
        .filter_map(|n| Some((attr_f64(n, "sOffset").unwrap_or(0.0).max(0.0), read(n)?)))
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// A lane's `<rule>` values. A `<rule>` without a `value` is skipped. Rules
/// at the same `sOffset` all apply, so their values join with `"; "`.
pub(super) fn lane_rules(lane: roxmltree::Node) -> Vec<(f64, String)> {
    let mut out: Vec<(f64, String)> = Vec::new();
    for (s, rule) in by_offset(lane, "rule", |n| n.attribute("value").map(String::from)) {
        match out.last_mut() {
            Some((at, last)) if *at == s => {
                last.push_str("; ");
                last.push_str(&rule);
            }
            _ => out.push((s, rule)),
        }
    }
    out
}

/// A lane's `<material>`s.
pub(super) fn lane_materials(lane: roxmltree::Node) -> Vec<(f64, Material)> {
    by_offset(lane, "material", |n| Some(material(n)))
}

/// A lane's `<access>`es. Those at the same `sOffset` apply together, as
/// files before 1.8 write them, one road user per `<access>`. Two of the
/// same rule merge their road users. An `allow` and a `deny` merge into the
/// `allow`, less the users the `deny` names: only those may use the lane.
pub(super) fn lane_access(lane: roxmltree::Node) -> Vec<(f64, AccessDef)> {
    let mut out: Vec<(f64, AccessDef)> = Vec::new();
    for (s, def) in by_offset(lane, "access", |n| Some(AccessDef::parse(n))) {
        if let (Some((at, AccessDef::Restricted(last))), AccessDef::Restricted(next)) =
            (out.last_mut(), &def)
        {
            if *at == s {
                *last = merge(last, next);
                continue;
            }
        }
        out.push((s, def));
    }
    out
}

/// Two `<access>`es that apply together, as one.
fn merge(a: &Access, b: &Access) -> Access {
    let less = |users: &[String], gone: &[String]| {
        users
            .iter()
            .filter(|u| !gone.contains(u))
            .cloned()
            .collect()
    };
    match (a, b) {
        (Access::Allow(a), Access::Allow(b)) => Access::Allow([&a[..], b].concat()),
        (Access::Deny(a), Access::Deny(b)) => Access::Deny([&a[..], b].concat()),
        (Access::Allow(allow), Access::Deny(deny)) | (Access::Deny(deny), Access::Allow(allow)) => {
            Access::Allow(less(allow, deny))
        }
    }
}

/// A lane's `<visibility>` as the file gives it.
#[derive(Clone, PartialEq)]
pub(super) enum VisibilityDef {
    Seen(Visibility),
    /// The first distance the crate can't read, and its text. Empty if the
    /// file leaves it out.
    Unreadable {
        distance: &'static str,
        value: String,
    },
}

impl VisibilityDef {
    /// A `<visibility>`. A distance missing, not a number, or below 0 is
    /// unreadable.
    fn parse(node: roxmltree::Node) -> Self {
        let distance = |name: &'static str| {
            attr_f64(node, name)
                .map(|v| v as f32)
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or(name)
        };
        let read = || -> Result<Visibility, &'static str> {
            Ok(Visibility {
                forward: distance("forward")?,
                back: distance("back")?,
                left: distance("left")?,
                right: distance("right")?,
            })
        };
        match read() {
            Ok(seen) => Self::Seen(seen),
            Err(distance) => Self::Unreadable {
                distance,
                value: node.attribute(distance).unwrap_or_default().to_string(),
            },
        }
    }

    fn visibility(&self) -> Option<Visibility> {
        match self {
            Self::Seen(seen) => Some(*seen),
            Self::Unreadable { .. } => None,
        }
    }
}

/// A lane's `<visibility>`s.
pub(super) fn lane_visibility(lane: roxmltree::Node) -> Vec<(f64, VisibilityDef)> {
    by_offset(lane, "visibility", |n| Some(VisibilityDef::parse(n)))
}

/// The [`RoadType`] an OpenDRIVE `<type>` names. An absent or unrecognised
/// type is [`RoadType::Unknown`].
fn road_type(od_type: Option<&str>) -> RoadType {
    match od_type.unwrap_or_default() {
        "rural" => RoadType::Rural,
        "motorway" => RoadType::Motorway,
        "town" => RoadType::Town,
        "lowSpeed" => RoadType::LowSpeed,
        "pedestrian" => RoadType::Pedestrian,
        "bicycle" => RoadType::Bicycle,
        "townExpressway" => RoadType::TownExpressway,
        "townCollector" => RoadType::TownCollector,
        "townArterial" => RoadType::TownArterial,
        "townPrivate" => RoadType::TownPrivate,
        "townLocal" => RoadType::TownLocal,
        "townPlayStreet" => RoadType::TownPlayStreet,
        _ => RoadType::Unknown,
    }
}

/// The speed limits and road types along every baked lane of `roads`.
#[derive(Default)]
pub(super) struct Placed {
    pub speed_limits: Vec<Along<SpeedLimit>>,
    pub road_types: Vec<Along<RoadType>>,
    pub road_countries: Vec<Along<String>>,
    pub lane_rules: Vec<Along<String>>,
    pub lane_access: Vec<Along<Access>>,
    pub lane_materials: Vec<Along<Material>>,
    pub lane_visibility: Vec<Along<Visibility>>,
}

/// Place the speed limits, road types, rules, access and materials along
/// `lanes`, the baked lanes of `roads`.
///
/// A road's type holds from its `s` to the next, or to the end of the road.
/// A lane's `<speed>` holds from its `sOffset` to the next, or to the end of
/// its lane section, and overrides the road's. Where the lane has none, the
/// speed of its road's type holds. A lane's `<rule>`, `<access>` or
/// `<material>` holds from its `sOffset` to the next of its kind, or to the
/// end of its lane section.
pub(super) fn place<'a>(roads: impl Iterator<Item = &'a BakedRoad>, lanes: &[Lane]) -> Placed {
    let mut out = Placed::default();
    for road in roads {
        let type_at = |s: f64| road.types.iter().rev().find(|t| t.s <= s + 1e-9);
        for (sec, sec_def) in road.sections() {
            for &(od_id, id) in &sec.lanes {
                let Some(def) = sec_def.lane(od_id) else {
                    continue;
                };
                let speeds: Vec<(f64, SpeedLimit)> = def
                    .speeds
                    .iter()
                    .filter_map(|l| Some((sec.start + l.s_offset, l.speed.limit()?)))
                    .collect();
                let own = |s: f64| speeds.iter().rev().find(|(at, _)| *at <= s + 1e-9);
                let type_starts = road.types.iter().map(|t| t.s);
                let points = lanes[id.0].center.points();
                let kinds = stretches(sec, type_starts.clone(), |s| type_at(s).map(|t| t.kind));
                let countries = stretches(sec, type_starts.clone(), |s| {
                    type_at(s)
                        .map(|t| t.country.clone())
                        .filter(|c| !c.is_empty())
                });
                let limits = stretches(sec, type_starts.chain(speeds.iter().map(|l| l.0)), |s| {
                    own(s).map(|l| l.1).or_else(|| {
                        type_at(s)
                            .and_then(|t| t.speed.as_ref())
                            .and_then(SpeedDef::limit)
                    })
                });
                out.road_types
                    .extend(on_lane(id, points, &sec.stations, kinds));
                out.road_countries
                    .extend(on_lane(id, points, &sec.stations, countries));
                out.speed_limits
                    .extend(on_lane(id, points, &sec.stations, limits));
                let rules = own_stretches(sec, &def.rules, |r| Some(r.clone()));
                let access = own_stretches(sec, &def.access, AccessDef::access);
                let materials = own_stretches(sec, &def.materials, |m| Some(m.clone()));
                let visibility = own_stretches(sec, &def.visibility, VisibilityDef::visibility);
                out.lane_rules
                    .extend(on_lane(id, points, &sec.stations, rules));
                out.lane_access
                    .extend(on_lane(id, points, &sec.stations, access));
                out.lane_materials
                    .extend(on_lane(id, points, &sec.stations, materials));
                out.lane_visibility
                    .extend(on_lane(id, points, &sec.stations, visibility));
            }
        }
    }
    out
}

/// `stretches` of road, each `(from, to, value)` in metres along the
/// reference line, as stretches of `lane`, whose centerline `points` stand
/// at the road `stations`. One that is empty along the lane is left out.
fn on_lane<'a, T: 'a>(
    lane: LaneId,
    points: &'a [Point],
    stations: &'a [f64],
    stretches: Vec<(f64, f64, T)>,
) -> impl Iterator<Item = Along<T>> + 'a {
    stretches
        .into_iter()
        .map(move |(from, to, value)| Along {
            lane,
            from: along(points, stations, from),
            to: along(points, stations, to),
            value,
        })
        .filter(|a| a.to > a.from)
}

/// The stretches of `sec` over which each of a lane's `entries`, at their
/// `sOffset`s in ascending order, holds `value`, up to the next entry or
/// the section's end. An entry whose `value` is `None` holds nothing.
fn own_stretches<E, T: PartialEq>(
    sec: &RoadSection,
    entries: &[(f64, E)],
    value: impl Fn(&E) -> Option<T>,
) -> Vec<(f64, f64, T)> {
    let at = |s: f64| {
        entries
            .iter()
            .rev()
            .find(|(offset, _)| sec.start + offset <= s + 1e-9)
            .and_then(|(_, e)| value(e))
    };
    stretches(
        sec,
        entries.iter().map(|(offset, _)| sec.start + offset),
        at,
    )
}

/// The stretches of `sec` over which `value_at` holds one value, given the
/// stations `cuts` where it may change. Neighbours with the same value
/// merge, and a stretch with no value is left out.
fn stretches<T: PartialEq>(
    sec: &RoadSection,
    cuts: impl Iterator<Item = f64>,
    value_at: impl Fn(f64) -> Option<T>,
) -> Vec<(f64, f64, T)> {
    let mut cuts: Vec<f64> = cuts
        .filter(|&s| s > sec.start + 1e-6 && s < sec.end - 1e-6)
        .chain([sec.start, sec.end])
        .collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut out: Vec<(f64, f64, T)> = Vec::new();
    for w in cuts.windows(2) {
        let Some(value) = value_at(w[0]) else {
            continue;
        };
        match out.last_mut() {
            Some(last) if last.1 == w[0] && last.2 == value => last.1 = w[1],
            _ => out.push((w[0], w[1], value)),
        }
    }
    out
}

impl BakedRoad {
    /// Each lane `<access>` whose `rule` the crate can't read, section by
    /// section.
    pub(super) fn access_warnings(&self) -> Vec<Warning> {
        self.sections()
            .flat_map(|(sec, def)| {
                def.left.iter().chain(&def.right).flat_map(move |l| {
                    l.access.iter().filter_map(move |(offset, def)| match def {
                        AccessDef::Unreadable { rule } => Some(Warning::AccessDropped {
                            road_id: self.road.od_id.clone(),
                            s: sec.start + offset,
                            lane: l.id,
                            rule: rule.clone(),
                        }),
                        AccessDef::Restricted(_) | AccessDef::Unrestricted => None,
                    })
                })
            })
            .collect()
    }

    /// Each lane `<visibility>` with a distance the crate can't read,
    /// section by section.
    pub(super) fn visibility_warnings(&self) -> Vec<Warning> {
        self.sections()
            .flat_map(|(sec, def)| {
                def.left.iter().chain(&def.right).flat_map(move |l| {
                    l.visibility
                        .iter()
                        .filter_map(move |(offset, def)| match def {
                            VisibilityDef::Unreadable { distance, value } => {
                                Some(Warning::VisibilityDropped {
                                    road_id: self.road.od_id.clone(),
                                    s: sec.start + offset,
                                    lane: l.id,
                                    distance: distance.to_string(),
                                    value: value.clone(),
                                })
                            }
                            VisibilityDef::Seen(_) => None,
                        })
                })
            })
            .collect()
    }

    /// Each `<speed>` whose `max` or `unit` the crate can't read: the
    /// road's, then each lane's, section by section.
    pub(super) fn speed_warnings(&self) -> Vec<Warning> {
        let dropped = |s, lane, speed: &SpeedDef| match speed {
            SpeedDef::Unreadable { max, unit } => Some(Warning::SpeedLimitDropped {
                road_id: self.road.od_id.clone(),
                s,
                lane,
                max: max.clone(),
                unit: unit.clone(),
            }),
            SpeedDef::Limit(_) | SpeedDef::Undefined => None,
        };
        let road = self
            .types
            .iter()
            .filter_map(|t| dropped(t.s, None, t.speed.as_ref()?));
        let lanes = self.sections().flat_map(|(sec, def)| {
            def.left.iter().chain(&def.right).flat_map(move |l| {
                l.speeds
                    .iter()
                    .map(move |sp| (sec.start + sp.s_offset, l.id, &sp.speed))
            })
        });
        road.chain(lanes.filter_map(|(s, id, speed)| dropped(s, Some(id), speed)))
            .collect()
    }
}
