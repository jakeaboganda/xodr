//! Signals, placed once every road is baked.

use std::collections::HashMap;
use std::f64::consts::PI;

use super::{
    attr_f32, attr_f64, child, orientation, validity, BakedRoad, ControllerProvenance,
    JunctionControllerProvenance, ObjectProvenance, Orientation, SignProvenance, SignalProvenance,
    SignalReferenceProvenance, Warning,
};
use crate::coords::{Point, Vector};
use crate::object::orient;
use crate::road::{Road, Validity};
use crate::{
    BoardSign, Control, Controller, ControllerId, Dependency, Direction, DisplayArea, Lane, LaneId,
    MessageBoard, ObjectId, Reference, Referenced, RoadUser, Semantic, Signal, SignalBoard,
    SignalId, Unit,
};

/// The signals and controllers baked so far, and the provenance of each, in
/// step. Each is at the position its id names.
#[derive(Default)]
pub(super) struct Signals {
    pub baked: Vec<Signal>,
    pub provenance: Vec<SignalProvenance>,
    pub controllers: Vec<Controller>,
    pub controller_provenance: Vec<ControllerProvenance>,
    pub warnings: Vec<Warning>,
}

/// Bake every `<signal>` of every road, link each to the signals and
/// objects it names, apply each to the roads whose `<signalReference>`s name
/// it, and bake the `<controller>`s over them. `roads` pairs each baked road
/// with its `<road>`, `lanes` are the lanes baked, and `objects` finds the
/// objects an `<object id>` baked to.
///
/// Links, references and controls name a signal by its id. Where two
/// signals share one, they name the first. One naming an id no signal has is
/// skipped, with a [`Warning::SignalLinkDropped`], and so is a signal the
/// crate can't place, with a [`Warning::SignalDropped`].
pub(super) fn place(
    root: roxmltree::Node,
    roads: &[(roxmltree::Node, BakedRoad)],
    lanes: &[Lane],
    objects: &[ObjectProvenance],
) -> Signals {
    let mut road_by_id = HashMap::new();
    for (_, road) in roads {
        road_by_id
            .entry(road.road.od_id.as_str())
            .or_insert(&road.road);
    }
    let entries = || {
        roads.iter().flat_map(|(node, road)| {
            child(*node, "signals")
                .into_iter()
                .flat_map(|n| n.children())
                .map(move |n| (n, &road.road))
        })
    };
    let mut out = Signals::default();
    let mut nodes = Vec::new();
    for (signal, road) in entries().filter(|(n, _)| n.has_tag_name("signal")) {
        if place_signal(signal, road, lanes, &road_by_id, &mut out) {
            nodes.push(signal);
        } else {
            out.warnings.push(Warning::SignalDropped {
                road_id: road.od_id.clone(),
                signal_id: signal.attribute("id").unwrap_or_default().to_string(),
            });
        }
    }
    let mut placed = HashMap::new();
    for p in &out.provenance {
        placed.entry(p.od_id.clone()).or_insert(p.signal);
    }
    let mut baked_from: HashMap<&str, Vec<_>> = HashMap::new();
    for p in objects {
        baked_from
            .entry(p.od_id.as_str())
            .or_default()
            .push(p.object);
    }
    for (i, node) in nodes.into_iter().enumerate() {
        let road_id = out.provenance[i].road_id.clone();
        for (element, id) in link(node, &placed, &baked_from, &mut out.baked[i]) {
            out.warnings
                .push(link_dropped(&road_id, element, id.unwrap_or_default()));
        }
    }
    for (reference, road) in entries().filter(|(n, _)| n.has_tag_name("signalReference")) {
        let id = reference.attribute("id");
        let applied = id
            .and_then(|id| placed.get(id))
            .is_some_and(|&SignalId(i)| {
                apply_reference(
                    reference,
                    road,
                    lanes,
                    &mut out.baked[i],
                    &mut out.provenance[i],
                )
            });
        if !applied {
            let dropped = link_dropped(&road.od_id, "signalReference", id.unwrap_or_default());
            out.warnings.push(dropped);
        }
    }
    place_controllers(root, &placed, &mut out);
    out
}

/// Bake every top-level `<controller>` into `out`, and tell each signal it
/// controls. Then add each `<junction>`'s `<controller>` list to the
/// provenance of the controllers it names. `placed` finds a signal by its id.
fn place_controllers(root: roxmltree::Node, placed: &HashMap<String, SignalId>, out: &mut Signals) {
    let sequence = |node: roxmltree::Node| node.attribute("sequence")?.parse().ok();
    let text = |node: roxmltree::Node, name| node.attribute(name).unwrap_or_default().to_string();
    let mut controller_by_id = HashMap::new();
    for node in root.children().filter(|n| n.has_tag_name("controller")) {
        let id = ControllerId(out.controllers.len());
        let mut signals = Vec::new();
        for control in node.children().filter(|n| n.has_tag_name("control")) {
            let named = control.attribute("signalId");
            let Some(&signal) = named.and_then(|s| placed.get(s)) else {
                let dropped = link_dropped("", "control", named.unwrap_or_default());
                out.warnings.push(dropped);
                continue;
            };
            signals.push(Control {
                signal,
                kind: text(control, "type"),
            });
            let controllers = &mut out.baked[signal.0].controllers;
            if !controllers.contains(&id) {
                controllers.push(id);
            }
        }
        out.controllers.push(Controller {
            id,
            name: text(node, "name"),
            sequence: sequence(node),
            signals,
        });
        let od_id = text(node, "id");
        controller_by_id.entry(od_id.clone()).or_insert(id.0);
        out.controller_provenance.push(ControllerProvenance {
            controller: id,
            od_id,
            junctions: Vec::new(),
        });
    }
    for junction in root.children().filter(|n| n.has_tag_name("junction")) {
        for node in junction.children().filter(|n| n.has_tag_name("controller")) {
            let Some(&i) = node.attribute("id").and_then(|id| controller_by_id.get(id)) else {
                continue;
            };
            out.controller_provenance[i]
                .junctions
                .push(JunctionControllerProvenance {
                    junction_id: text(junction, "id"),
                    kind: text(node, "type"),
                    sequence: sequence(node),
                });
        }
    }
}

/// Apply `signal` where the `<signalReference>` `node` on `road` says too: add
/// the lanes there and the point it names, and say whether it did. One
/// missing `s` or `t`, or off the ends of the road, is skipped.
fn apply_reference(
    node: roxmltree::Node,
    road: &Road,
    lanes: &[Lane],
    signal: &mut Signal,
    provenance: &mut SignalProvenance,
) -> bool {
    let (Some(s), Some(t)) = (attr_f64(node, "s"), attr_f64(node, "t")) else {
        return false;
    };
    if !road.on_road(s) {
        return false;
    }
    let orientation = orientation(node);
    for lane in signal_lanes(road, lanes, s, orientation, validity(node)) {
        if !signal.lanes.contains(&lane) {
            signal.lanes.push(lane);
        }
    }
    signal.applies_at.push(road.surface(s, t).0);
    provenance.references.push(SignalReferenceProvenance {
        road_id: road.od_id.clone(),
        s,
        t,
        orientation,
    });
    true
}

/// The warning for a link of `element`, on `road_id`, naming `id`, which
/// the crate drops.
fn link_dropped(road_id: &str, element: &str, id: &str) -> Warning {
    Warning::SignalLinkDropped {
        road_id: road_id.to_string(),
        element: element.to_string(),
        id: id.to_string(),
    }
}

/// Where a signal's board stands, and how it is turned.
struct Board {
    position: Point,
    heading: f64,
    pitch: f64,
    roll: f64,
}

/// Bake one `<signal>` on `road` into `out`, and say whether it did.
/// `roads` finds the road a `<positionRoad>` names. One missing `s` or `t`,
/// or off the ends of the road, is skipped.
fn place_signal(
    node: roxmltree::Node,
    road: &Road,
    lanes: &[Lane],
    roads: &HashMap<&str, &Road>,
    out: &mut Signals,
) -> bool {
    let (Some(s), Some(t)) = (attr_f64(node, "s"), attr_f64(node, "t")) else {
        return false;
    };
    let orientation = orientation(node);
    let Some(own) = standing(node, road, orientation) else {
        return false;
    };
    let elsewhere = || {
        let at = child(node, "positionRoad")?;
        standing(at, roads.get(at.attribute("roadId")?)?, orientation)
    };
    let board = child(node, "positionInertial")
        .and_then(inertial)
        .or_else(elsewhere)
        .unwrap_or(own);
    let text = |name| node.attribute(name).unwrap_or_default().to_string();
    let flag = |name| matches!(node.attribute(name), Some("true" | "1"));
    let size = |name| attr_f32(node, name);
    let id = SignalId(out.baked.len());
    out.baked.push(Signal {
        id,
        name: text("name"),
        dynamic: node.attribute("dynamic") == Some("yes"),
        country: text("country"),
        country_revision: text("countryRevision"),
        kind: text("type"),
        subtype: text("subtype"),
        value: attr_f64(node, "value"),
        unit: node.attribute("unit").and_then(unit),
        text: text("text"),
        invalidated: flag("invalidated"),
        temporary: flag("temporary"),
        controllers: Vec::new(),
        dependencies: Vec::new(),
        references: Vec::new(),
        lanes: signal_lanes(road, lanes, s, orientation, validity(node)),
        applies_at: vec![road.surface(s, t).0],
        position: board.position,
        heading: angle(board.heading),
        pitch: angle(board.pitch),
        roll: angle(board.roll),
        length: size("length"),
        width: size("width"),
        height: size("height"),
        semantics: semantics(node),
        boards: boards(node, &board),
    });
    out.provenance.push(SignalProvenance {
        signal: id,
        road_id: road.od_id.clone(),
        od_id: text("id"),
        s,
        t,
        orientation,
        references: Vec::new(),
        signs: signs(node),
    });
    true
}

/// What each `<sign>` of `node`'s static boards says about itself that the
/// crate builds nothing from, in file order. `board` counts the baked boards,
/// which are the `<staticBoard>`s and `<vmsBoard>`s.
fn signs(node: roxmltree::Node) -> Vec<SignProvenance> {
    let flag = |n: roxmltree::Node, name| matches!(n.attribute(name), Some("true" | "1"));
    let turn = |n: roxmltree::Node, name| angle(attr_f64(n, name).unwrap_or(0.0));
    let boards = node
        .children()
        .filter(|b| b.has_tag_name("staticBoard") || b.has_tag_name("vmsBoard"));
    let mut out = Vec::new();
    for (board, b) in boards.enumerate() {
        let signs = b.children().filter(|n| n.has_tag_name("sign"));
        for (sign, node) in signs.enumerate() {
            out.push(SignProvenance {
                board,
                sign,
                id: node.attribute("id").unwrap_or_default().to_string(),
                country_revision: node
                    .attribute("countryRevision")
                    .unwrap_or_default()
                    .to_string(),
                dynamic: node.attribute("dynamic") == Some("yes"),
                invalidated: flag(node, "invalidated"),
                temporary: flag(node, "temporary"),
                orientation: orientation(node),
                h_offset: turn(node, "hOffset"),
                pitch: turn(node, "pitch"),
                roll: turn(node, "roll"),
                length: attr_f32(node, "length"),
            });
        }
    }
    out
}

/// Give `signal` the `<dependency>`s and `<reference>`s of its `<signal>`,
/// `node`. `placed` finds a signal by its id, and `baked_from` the objects
/// an `<object id>` baked to. A reference to an object names each of them.
fn link<'a>(
    node: roxmltree::Node<'a, 'a>,
    placed: &HashMap<String, SignalId>,
    baked_from: &HashMap<&str, Vec<ObjectId>>,
    signal: &mut Signal,
) -> Vec<(&'static str, Option<&'a str>)> {
    let text = |node: roxmltree::Node, name| node.attribute(name).unwrap_or_default().to_string();
    let named = |id: Option<&str>| id.and_then(|id| placed.get(id)).copied();
    let mut dropped = Vec::new();
    for dependency in node.children().filter(|n| n.has_tag_name("dependency")) {
        match named(dependency.attribute("id")) {
            Some(named) => signal.dependencies.push(Dependency {
                signal: named,
                kind: text(dependency, "type"),
            }),
            None => dropped.push(("dependency", dependency.attribute("id"))),
        }
    }
    for reference in node.children().filter(|n| n.has_tag_name("reference")) {
        let id = reference.attribute("elementId");
        let to: Vec<Referenced> = match reference.attribute("elementType") {
            Some("signal") => named(id).map(Referenced::Signal).into_iter().collect(),
            Some("object") => id
                .and_then(|id| baked_from.get(id))
                .into_iter()
                .flatten()
                .map(|&o| Referenced::Object(o))
                .collect(),
            _ => Vec::new(),
        };
        if to.is_empty() {
            dropped.push(("reference", id));
        }
        let kind = text(reference, "type");
        signal.references.extend(to.into_iter().map(|to| Reference {
            to,
            kind: kind.clone(),
        }));
    }
    dropped
}

/// A board standing `zOffset` straight above `road`'s surface at the `s` and
/// `t` of `node`, a `<signal>` or a `<positionRoad>`, or `None` if it is
/// missing either or off the ends of the road.
///
/// The board faces the traffic `orientation` names, against `+s` for `+`
/// and along it otherwise, turned `hOffset` counter-clockwise. `pitch` and
/// `roll` are against the horizontal, not the road, so a board on a banked
/// road stays upright.
fn standing(node: roxmltree::Node, road: &Road, orientation: Orientation) -> Option<Board> {
    let (s, t) = (attr_f64(node, "s")?, attr_f64(node, "t")?);
    if !road.on_road(s) {
        return None;
    }
    let number = |name| attr_f64(node, name).unwrap_or(0.0);
    let (ground, road_heading) = road.surface(s, t);
    let facing = match orientation {
        Orientation::Positive => PI,
        Orientation::Negative | Orientation::Both => 0.0,
    };
    Some(Board {
        position: ground + Vector::new(0.0, 0.0, attr_f32(node, "zOffset").unwrap_or(0.0)),
        heading: road_heading + facing + number("hOffset"),
        pitch: number("pitch"),
        roll: number("roll"),
    })
}

/// What `node`'s `<semantics>` say, in file order. A child the spec does not
/// name is skipped.
fn semantics(node: roxmltree::Node) -> Vec<Semantic> {
    let text = |n: roxmltree::Node, name| n.attribute(name).unwrap_or_default().to_string();
    let users = |n: roxmltree::Node| -> Vec<RoadUser> {
        n.children()
            .filter_map(|u| {
                let category = || {
                    u.children()
                        .find(|c| c.has_tag_name("type"))
                        .and_then(|c| c.text())
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                };
                match u.tag_name().name() {
                    "animal" => Some(RoadUser::Animal),
                    "person" => Some(RoadUser::Person(category())),
                    "vehicle" => Some(RoadUser::Vehicle(category())),
                    _ => None,
                }
            })
            .collect()
    };
    child(node, "semantics")
        .into_iter()
        .flat_map(|s| s.children())
        .filter_map(|n| {
            Some(match n.tag_name().name() {
                "speed" => Semantic::Speed {
                    kind: text(n, "type"),
                    value: attr_f64(n, "value"),
                    unit: n.attribute("unit").and_then(unit),
                },
                "lane" => Semantic::Lane {
                    kind: text(n, "type"),
                },
                "priority" => Semantic::Priority {
                    kind: text(n, "type"),
                },
                "prohibited" => Semantic::Prohibited(users(n)),
                "warning" => Semantic::Warning,
                "routing" => Semantic::Routing,
                "streetname" => Semantic::StreetName,
                "parking" => Semantic::Parking,
                "tourist" => Semantic::Tourist,
                "supplementaryTime" => Semantic::SupplementaryTime {
                    kind: text(n, "type"),
                    value: attr_f64(n, "value"),
                },
                "supplementaryAllows" => Semantic::SupplementaryAllows(users(n)),
                "supplementaryProhibits" => Semantic::SupplementaryProhibits(users(n)),
                "supplementaryDistance" => Semantic::SupplementaryDistance {
                    kind: text(n, "type"),
                    value: attr_f64(n, "value"),
                    unit: n.attribute("unit").and_then(unit),
                },
                "supplementaryEnvironment" => Semantic::SupplementaryEnvironment {
                    kind: text(n, "type"),
                },
                "supplementaryExplanatory" => Semantic::SupplementaryExplanatory,
                _ => return None,
            })
        })
        .collect()
}

/// `node`'s `<staticBoard>`s and `<vmsBoard>`s, in file order, placed on
/// `board`: `v` across it, to the left of its heading, and `z` up it,
/// turned by its heading, pitch and roll.
fn boards(node: roxmltree::Node, board: &Board) -> Vec<SignalBoard> {
    let at = |n: roxmltree::Node| {
        let local = [
            0.0,
            attr_f32(n, "v").map_or(0.0, f64::from),
            attr_f32(n, "z").map_or(0.0, f64::from),
        ];
        let [x, y, z] = orient(board.heading, board.pitch, board.roll, local);
        let placed = board.position + Vector::new(x as f32, y as f32, z as f32);
        if placed.to_array().iter().all(|c| c.is_finite()) {
            placed
        } else {
            board.position
        }
    };
    let size = |n: roxmltree::Node, name| {
        n.attribute(name)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .map(|v| v as f32)
            .filter(|v| v.is_finite())
    };
    let text = |n: roxmltree::Node, name| n.attribute(name).unwrap_or_default().to_string();
    node.children()
        .filter_map(|b| {
            if b.has_tag_name("staticBoard") {
                let signs = b
                    .children()
                    .filter(|n| n.has_tag_name("sign"))
                    .map(|sign| BoardSign {
                        name: text(sign, "name"),
                        country: text(sign, "country"),
                        kind: text(sign, "type"),
                        subtype: text(sign, "subtype"),
                        value: attr_f64(sign, "value"),
                        unit: sign.attribute("unit").and_then(unit),
                        text: text(sign, "text"),
                        semantics: semantics(sign),
                        position: at(sign),
                        width: size(sign, "width"),
                        height: size(sign, "height"),
                    })
                    .collect();
                Some(SignalBoard::Static(signs))
            } else if b.has_tag_name("vmsBoard") {
                Some(SignalBoard::Message(MessageBoard {
                    display: text(b, "displayType"),
                    position: at(b),
                    width: size(b, "displayWidth"),
                    height: size(b, "displayHeight"),
                    areas: b
                        .children()
                        .filter(|n| n.has_tag_name("displayArea"))
                        .map(|area| DisplayArea {
                            index: area.attribute("index").and_then(|v| v.trim().parse().ok()),
                            position: at(area),
                            width: size(area, "width"),
                            height: size(area, "height"),
                        })
                        .collect(),
                }))
            } else {
                None
            }
        })
        .collect()
}

/// A board at a `<positionInertial>`, facing its `hdg`, or `None` if it is
/// missing `x`, `y` or `z`, or one is too large for an `f32`.
fn inertial(node: roxmltree::Node) -> Option<Board> {
    let number = |name| attr_f64(node, name).unwrap_or(0.0);
    let (x, y, z) = (
        attr_f32(node, "x")?,
        attr_f32(node, "y")?,
        attr_f32(node, "z")?,
    );
    Some(Board {
        position: Point::new(x, y, z),
        heading: number("hdg"),
        pitch: number("pitch"),
        roll: number("roll"),
    })
}

/// The lanes at `s` on `road` a signal applies to: those in its `<validity>`
/// ranges if it has any, else those whose traffic runs the way its
/// `orientation` names, `+s` for `+` and against it for `-`. A two-way lane
/// carries traffic both ways. `none` names every lane. `lanes` are the lanes
/// baked, which say which way each runs.
fn signal_lanes(
    road: &Road,
    lanes: &[Lane],
    s: f64,
    orientation: Orientation,
    validity: Vec<Validity>,
) -> Vec<LaneId> {
    let found = road.lanes((s, s), &validity);
    let wanted = match (validity.is_empty(), orientation) {
        (false, _) | (_, Orientation::Both) => return found,
        (_, Orientation::Positive) => Direction::Forward,
        (_, Orientation::Negative) => Direction::Backward,
    };
    found
        .into_iter()
        .filter(|id| {
            debug_assert_eq!(lanes[id.0].id, *id, "a lane's id is its place in `lanes`");
            let direction = lanes[id.0].direction;
            direction == wanted || direction == Direction::Both
        })
        .collect()
}

/// `angle` as an `f32` in `(-π, π]`. The `f32` nearest π is a little over
/// it, so an angle just over -π rounds onto -π, and is turned to π.
fn angle(angle: f64) -> f32 {
    let wrapped = wrap(angle) as f32;
    if wrapped <= -std::f32::consts::PI {
        std::f32::consts::PI
    } else {
        wrapped
    }
}

/// `angle` in `(-π, π]`.
fn wrap(angle: f64) -> f64 {
    let wrapped = angle.rem_euclid(2.0 * PI);
    if wrapped > PI {
        wrapped - 2.0 * PI
    } else {
        wrapped
    }
}

/// The unit a `unit` attribute names. `None` for an empty one.
fn unit(name: &str) -> Option<Unit> {
    Some(match name {
        "" => return None,
        "m" => Unit::Metre,
        "km" => Unit::Kilometre,
        "ft" => Unit::Foot,
        "mile" => Unit::Mile,
        "m/s" => Unit::MetrePerSecond,
        "mph" => Unit::MilePerHour,
        "km/h" => Unit::KilometrePerHour,
        "kg" => Unit::Kilogram,
        "t" => Unit::Tonne,
        "%" => Unit::Percent,
        _ => Unit::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_str;

    /// A flat 20 m road heading +X with lane -1, carrying `signals`.
    fn road_with(signals: &str) -> String {
        format!(
            r#"<OpenDRIVE>
  <road name="r" length="20.0" id="1" junction="-1">
    <planView><geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="20.0"><line/></geometry></planView>
    <lanes><laneSection s="0.0"><right><lane id="-1" type="driving">
      <width sOffset="0.0" a="3.5"/>
    </lane></right></laneSection></lanes>
    <signals>{signals}</signals>
  </road>
</OpenDRIVE>"#
        )
    }

    fn signals_of(signals: &str) -> Vec<Signal> {
        load_str(&road_with(signals))
            .expect("import")
            .signals()
            .to_vec()
    }

    #[test]
    fn a_signal_without_a_station_or_off_the_road_is_skipped() {
        let signals = signals_of(
            r#"<signal id="a" t="0" orientation="+"/>
               <signal id="b" s="5" orientation="+"/>
               <signal id="c" s="25" t="0" orientation="+"/>
               <signal id="d" s="20" t="0" orientation="+"/>"#,
        );
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].position, crate::Point::new(20.0, 0.0, 0.0));
    }

    #[test]
    fn a_position_that_cannot_be_placed_leaves_the_board_at_its_station() {
        let signals = signals_of(
            r#"<signal s="5" t="-2" zOffset="1" orientation="-">
                 <positionRoad roadId="9" s="1" t="0" zOffset="0" hOffset="0"/>
               </signal>
               <signal s="5" t="-2" zOffset="1" orientation="-">
                 <positionInertial x="1" y="2" hdg="0"/>
               </signal>
               <signal s="5" t="-2" zOffset="1" orientation="-">
                 <positionRoad roadId="1" s="30" t="0" zOffset="0" hOffset="0"/>
               </signal>"#,
        );
        for signal in &signals {
            assert_eq!(signal.position, crate::Point::new(5.0, -2.0, 1.0));
        }
        assert_eq!(signals.len(), 3);
    }

    #[test]
    fn a_link_to_nothing_is_skipped() {
        let signals = signals_of(
            r#"<signal id="a" s="1" t="0">
                 <dependency id="zz"/>
                 <reference elementType="signal" elementId="zz"/>
                 <reference elementType="object" elementId="zz"/>
                 <reference elementType="junction" elementId="a"/>
                 <reference elementId="a"/>
                 <dependency id="a"/>
               </signal>"#,
        );
        let signal = &signals[0];
        assert_eq!(signal.references, []);
        assert_eq!(
            signal.dependencies,
            [Dependency {
                signal: signal.id,
                kind: String::new()
            }]
        );
    }

    #[test]
    fn an_unrecognised_unit_is_unknown_and_an_empty_one_is_none() {
        let signals = signals_of(
            r#"<signal s="1" t="0" value="3" unit="furlong"/>
               <signal s="1" t="0" value="3" unit=""/>
               <signal s="1" t="0" value="x" unit="mph"/>"#,
        );
        let units: Vec<_> = signals.iter().map(|s| (s.value, s.unit)).collect();
        assert_eq!(
            units,
            [
                (Some(3.0), Some(Unit::Unknown)),
                (Some(3.0), None),
                (None, Some(Unit::MilePerHour)),
            ]
        );
    }

    #[test]
    fn an_angle_wraps_into_the_half_open_turn() {
        for (angle, want) in [
            (0.0, 0.0),
            (PI, PI),
            (-PI, PI),
            (3.0 * PI / 2.0, -PI / 2.0),
            (14.0 * PI + 0.1, 0.1),
            (-14.0 * PI - 0.1, -0.1),
        ] {
            assert!(
                (wrap(angle) - want).abs() < 1e-9,
                "{angle} wrapped to {}",
                wrap(angle)
            );
        }
    }
}
