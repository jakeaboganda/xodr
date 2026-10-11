//! OpenDRIVE connectivity: collect road/lane links and junctions while parsing,
//! then resolve them into **drive-direction** lane adjacency on the baked
//! lanes. "Successor" here means "a lane you can drive into when leaving this
//! lane's travel-direction exit end", already accounting for lane sign and the
//! link `contactPoint` (start/end). It is a routing graph, not a raw mirror
//! of the file's `+s` links.

use std::collections::HashMap;

use super::{RoadEnd, Warning};
use crate::{Direction, Lane, LaneId, Layer};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Contact {
    Start,
    End,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ElemType {
    Road,
    Junction,
}

/// A road-level `<predecessor>`/`<successor>`: what a road connects to at one
/// end, and by which end of that neighbor.
#[derive(Clone)]
pub(crate) struct LinkTarget {
    pub elem: ElemType,
    pub id: String,
    pub contact: Contact,
    /// Whether the file gives the `contactPoint`, rather than it reading as
    /// the start.
    pub contact_given: bool,
    /// Whether it gives an `elementS`: a place part way along the road,
    /// not an end.
    pub along: bool,
}

/// A lane section that baked: where it is on its road, and on which layer.
pub(crate) struct SectionInfo {
    pub index: usize,
    pub layer: Layer,
    pub start: f64,
    pub end: f64,
}

/// A lane `<link>`'s `<predecessor>` or `<successor>`: the lane it names, and
/// the layer it says that lane is on, if it says.
#[derive(Clone, Copy)]
pub(crate) struct LaneRef {
    pub id: i32,
    pub layer: Option<Layer>,
}

/// A junction `<laneLink>`.
pub(crate) struct JunctionLaneLink {
    pub from: i32,
    pub to: i32,
    pub from_layer: Layer,
    pub to_layer: Layer,
}

pub(crate) struct RoadInfo {
    /// Each section that baked, permanent layer first, each layer in order.
    /// A section shorter than a millimetre bakes no lanes, so links step
    /// over it.
    pub sections: Vec<SectionInfo>,
    /// How long the road is.
    pub length: f64,
    pub predecessor: Option<LinkTarget>,
    pub successor: Option<LinkTarget>,
}

/// One `<junction>` `<connection>`: an incoming road flows into a connecting
/// road, with per-lane `from -> to` links. In a `type="direct"` junction the
/// connecting road is the `linkedRoad`, which the incoming road runs
/// straight into.
pub(crate) struct JunctionConn {
    pub incoming_road: String,
    pub connecting_road: String,
    pub contact: Contact,
    pub lane_links: Vec<JunctionLaneLink>,
}

/// Everything about one emitted lane needed to resolve its links: its assigned
/// id, where it came from (road/section/OpenDRIVE lane id), its travel
/// direction, and its lane-level `<link>` neighbor ids.
pub(crate) struct LaneMeta {
    pub id: LaneId,
    pub road: String,
    pub section: usize,
    pub od_id: i32,
    pub direction: Direction,
    pub layer: Layer,
    pub succ_links: Vec<LaneRef>,
    pub pred_links: Vec<LaneRef>,
    pub heights: Vec<super::LaneHeight>,
    pub flags: super::LaneFlags,
}

/// Accumulated during parsing; consumed by [`resolve`].
#[derive(Default)]
pub(crate) struct Topology {
    /// Next `LaneId` to hand out, so ids stay unique across all roads.
    pub next_id: usize,
    pub registry: HashMap<(String, usize, i32), LaneId>,
    pub metas: Vec<LaneMeta>,
    pub roads: HashMap<String, RoadInfo>,
    pub junctions: HashMap<String, Vec<JunctionConn>>,
}

/// How far, in metres, a section may stand from where a link leaves for the
/// link to reach it.
const REACH: f64 = 1e-2;

impl Topology {
    /// The index of `road`'s first permanent lane section with lanes.
    pub(crate) fn first_section(&self, road: &str) -> usize {
        self.end_section(road, Layer::Permanent, Contact::Start)
            .unwrap_or(0)
    }

    /// The index of `road`'s last permanent lane section with lanes.
    pub(crate) fn last_section(&self, road: &str) -> usize {
        self.end_section(road, Layer::Permanent, Contact::End)
            .unwrap_or(0)
    }

    /// The index of `road`'s first (at its start) or last (at its end) lane
    /// section on `layer`, if it has one.
    fn end_section(&self, road: &str, layer: Layer, contact: Contact) -> Option<usize> {
        let length = self.roads.get(road)?.length;
        let mut on = self.sections(road).iter().filter(|s| s.layer == layer);
        match contact {
            Contact::Start => on.next().filter(|s| s.start <= REACH),
            Contact::End => on.next_back().filter(|s| s.end >= length - REACH),
        }
        .map(|s| s.index)
    }

    /// The index of the section of `layer` a link reaches leaving `road` at
    /// `at` metres along it, forward or backward, if there is one. It is
    /// the section that starts there or covers the place going forward, and
    /// the one that ends there or covers the place just before going
    /// backward.
    fn reach(&self, road: &str, layer: Layer, at: f64, forward: bool) -> Option<usize> {
        let mut on = self.sections(road).iter().filter(|s| s.layer == layer);
        // The permanent layer runs under everything, so a link may land
        // part way along one of its sections. A temporary section is
        // entered where it starts, and left where it ends.
        let covers = layer == Layer::Permanent;
        if forward {
            on.find(|s| {
                s.end > at + 1e-6 && s.start <= at + REACH && (covers || s.start >= at - REACH)
            })
        } else {
            on.rfind(|s| {
                s.start < at - 1e-6 && s.end >= at - REACH && (covers || s.end <= at + REACH)
            })
        }
        .map(|s| s.index)
    }

    fn section_info(&self, road: &str, index: usize) -> Option<&SectionInfo> {
        self.sections(road).iter().find(|s| s.index == index)
    }

    fn sections(&self, road: &str) -> &[SectionInfo] {
        self.roads.get(road).map_or(&[], |r| &r.sections)
    }
}

/// Fill each lane's `successors`/`predecessors` from the collected topology.
/// Successors are computed per lane; predecessors are their inverse, so the two
/// are always consistent.
///
/// Two two-way lanes that join run into each other both ways, even where
/// only one of them finds the other through its links, as a connecting
/// road does through its own road link.
pub(crate) fn resolve(lanes: &mut [Lane], topo: &Topology) {
    let mut succ: HashMap<LaneId, Vec<LaneId>> = HashMap::new();
    for meta in &topo.metas {
        let s = successors_of(meta, topo);
        if !s.is_empty() {
            succ.insert(meta.id, s);
        }
    }
    let two_way: std::collections::HashSet<LaneId> = topo
        .metas
        .iter()
        .filter(|m| m.direction == Direction::Both)
        .map(|m| m.id)
        .collect();
    let back: Vec<(LaneId, LaneId)> = succ
        .iter()
        .filter(|(from, _)| two_way.contains(from))
        .flat_map(|(from, tos)| {
            tos.iter()
                .filter(|to| two_way.contains(to))
                .map(|to| (*to, *from))
        })
        .collect();
    for (from, to) in back {
        let tos = succ.entry(from).or_default();
        if !tos.contains(&to) {
            tos.push(to);
        }
    }
    let mut pred: HashMap<LaneId, Vec<LaneId>> = HashMap::new();
    for (from, tos) in &succ {
        for to in tos {
            pred.entry(*to).or_default().push(*from);
        }
    }
    for lane in lanes.iter_mut() {
        if let Some(s) = succ.get(&lane.id) {
            lane.successors = s.clone();
            lane.successors.sort_by_key(|l| l.0);
        }
        if let Some(p) = pred.get(&lane.id) {
            lane.predecessors = p.clone();
            lane.predecessors.sort_by_key(|l| l.0);
        }
    }
}

/// The lanes reachable by driving off `meta`'s exit end, or ends: both of a
/// [`Direction::Both`] lane's.
///
/// From a two-way lane, only lanes whose traffic runs away from the joint
/// count: a lane entered at its road's start that runs forward, one entered
/// at its end that runs backward, or another two-way lane. A one-way lane's
/// links are the file's, as they are.
fn successors_of(meta: &LaneMeta, topo: &Topology) -> Vec<LaneId> {
    match meta.direction {
        Direction::Both => {
            let away = |&(id, entry): &(LaneId, Contact)| {
                let direction = topo
                    .metas
                    .get(id.0)
                    .filter(|m| m.id == id)
                    .or_else(|| topo.metas.iter().find(|m| m.id == id))
                    .map(|m| m.direction);
                matches!(
                    (direction, entry),
                    (Some(Direction::Both), _)
                        | (Some(Direction::Forward), Contact::Start)
                        | (Some(Direction::Backward), Contact::End)
                )
            };
            let mut out: Vec<LaneId> = exits(meta, topo, Direction::Forward)
                .into_iter()
                .chain(exits(meta, topo, Direction::Backward))
                .filter(away)
                .map(|(id, _)| id)
                .collect();
            out.sort_by_key(|l| l.0);
            out.dedup();
            out
        }
        direction => exits(meta, topo, direction)
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
    }
}

/// The lanes reachable by driving off `meta` travelling `direction`, each
/// with the end of its road it is entered at. Forward exits at the road's
/// `+s` end (next section, or the road's successor); backward at the `-s`
/// (start) end (previous section, or the predecessor).
///
/// A lane may name several lanes to go on to, each on a layer. A link with
/// no `layer` reads as the permanent layer, as the spec says, except from a
/// lane of the temporary layer to a section of the temporary layer that
/// follows it, which a file that links its temporary lanes one to the next
/// leaves unsaid.
fn exits(meta: &LaneMeta, topo: &Topology, direction: Direction) -> Vec<(LaneId, Contact)> {
    let road = topo.roads.get(&meta.road);
    let forward = direction == Direction::Forward;
    let (links, contact) = if forward {
        (&meta.succ_links, Contact::Start)
    } else {
        (&meta.pred_links, Contact::End)
    };
    let Some(info) = topo.section_info(&meta.road, meta.section) else {
        return Vec::new();
    };
    let place = if forward { info.end } else { info.start };
    let at_road_end = road.is_some_and(|r| {
        if forward {
            place >= r.length - REACH
        } else {
            place <= REACH
        }
    });
    let target = || {
        road.and_then(|r| {
            if forward {
                r.successor.clone()
            } else {
                r.predecessor.clone()
            }
        })
    };
    let mut out = Vec::new();
    if links.is_empty() {
        if at_road_end {
            out.extend(cross(topo, meta, target(), None, meta.layer));
        }
        return out;
    }
    let own = topo
        .reach(&meta.road, meta.layer, place, forward)
        .map(|_| meta.layer);
    for link in links {
        let layer = link.layer.or(own).unwrap_or(Layer::Permanent);
        match topo.reach(&meta.road, layer, place, forward) {
            Some(next) => out.extend(at(contact, lookup(topo, &meta.road, next, Some(link.id)))),
            None if at_road_end => out.extend(cross(topo, meta, target(), Some(link.id), layer)),
            None => {}
        }
    }
    // Several links may lead through one junction, which they would each
    // cross whole.
    let mut seen = std::collections::HashSet::new();
    out.retain(|&(id, _)| seen.insert(id));
    out
}

/// Each of `lanes`, entered at `contact`.
fn at(contact: Contact, lanes: Vec<LaneId>) -> Vec<(LaneId, Contact)> {
    lanes.into_iter().map(|id| (id, contact)).collect()
}

/// The lane with OpenDRIVE id `od` in (`road`, `section`), if it exists.
fn lookup(topo: &Topology, road: &str, section: usize, od: Option<i32>) -> Vec<LaneId> {
    od.and_then(|o| topo.registry.get(&(road.to_string(), section, o)).copied())
        .into_iter()
        .collect()
}

/// Resolve a cross-road boundary: follow the road's link target (another road,
/// or a junction) to the lane(s) it leads into, each with the end of its road
/// it is entered at. `lane_link` is this lane's own `<link>` neighbor id (used
/// for a direct road link), and `layer` the layer it names or reads as.
fn cross(
    topo: &Topology,
    meta: &LaneMeta,
    target: Option<LinkTarget>,
    lane_link: Option<i32>,
    layer: Layer,
) -> Vec<(LaneId, Contact)> {
    let Some(target) = target else {
        return Vec::new();
    };
    match target.elem {
        // Direct road-to-road: the neighbor's section is its first (start
        // contact) or last (end contact) on the layer, or on the permanent
        // layer if the neighbor has none there. The lane is our own <link> id.
        ElemType::Road => {
            let section = topo
                .end_section(&target.id, layer, target.contact)
                .or_else(|| topo.end_section(&target.id, Layer::Permanent, target.contact));
            let Some(section) = section else {
                return Vec::new();
            };
            at(target.contact, lookup(topo, &target.id, section, lane_link))
        }
        // Through a junction: each connection out of our road maps our lane id
        // to a connecting-road lane via its laneLinks. May fan out.
        ElemType::Junction => {
            let Some(conns) = topo.junctions.get(&target.id) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for c in conns.iter().filter(|c| c.incoming_road == meta.road) {
                for link in c
                    .lane_links
                    .iter()
                    .filter(|l| l.from == meta.od_id && l.from_layer == meta.layer)
                {
                    let Some(section) = topo
                        .end_section(&c.connecting_road, link.to_layer, c.contact)
                        .or_else(|| {
                            topo.end_section(&c.connecting_road, Layer::Permanent, c.contact)
                        })
                    else {
                        continue;
                    };
                    out.extend(at(
                        c.contact,
                        lookup(topo, &c.connecting_road, section, Some(link.to)),
                    ));
                }
            }
            out
        }
    }
}

// --- Parsing (roxmltree -> the types above) ----------------------------------

fn contact_of(node: roxmltree::Node) -> Contact {
    match node.attribute("contactPoint") {
        Some("end") => Contact::End,
        _ => Contact::Start,
    }
}

/// Parse a road's `<link>` predecessor/successor targets.
pub(crate) fn road_link(road: roxmltree::Node) -> (Option<LinkTarget>, Option<LinkTarget>) {
    let Some(link) = road.children().find(|n| n.has_tag_name("link")) else {
        return (None, None);
    };
    let target = |tag: &str| {
        link.children().find(|n| n.has_tag_name(tag)).and_then(|n| {
            let elem = match n.attribute("elementType") {
                Some("junction") => ElemType::Junction,
                _ => ElemType::Road,
            };
            Some(LinkTarget {
                elem,
                id: n.attribute("elementId")?.to_string(),
                contact: contact_of(n),
                contact_given: n.attribute("contactPoint").is_some(),
                along: n.attribute("elementS").is_some(),
            })
        })
    };
    (target("predecessor"), target("successor"))
}

/// A lane's `<link>` predecessor/successor lane ids, the first of each.
pub(crate) fn lane_link(lane: roxmltree::Node) -> (Option<i32>, Option<i32>) {
    let (predecessors, successors) = lane_refs(lane);
    (
        predecessors.first().map(|r| r.id),
        successors.first().map(|r| r.id),
    )
}

/// Every lane a lane's `<link>` names before and after it, each with the
/// layer it says it is on. A lane links to one lane on each layer, so a
/// `<link>` may hold several of each.
pub(crate) fn lane_refs(lane: roxmltree::Node) -> (Vec<LaneRef>, Vec<LaneRef>) {
    let Some(link) = lane.children().find(|n| n.has_tag_name("link")) else {
        return (Vec::new(), Vec::new());
    };
    let refs = |tag: &str| {
        link.children()
            .filter(|n| n.has_tag_name(tag))
            .filter_map(|n| {
                Some(LaneRef {
                    id: n.attribute("id")?.parse().ok()?,
                    layer: n.attribute("layer").map(|l| Layer::from_attr(Some(l))),
                })
            })
            .collect()
    };
    (refs("predecessor"), refs("successor"))
}

/// Parse every `<junction>` in the document into `id -> connections`, and
/// warn of each connection dropped for lacking a road. `roads` gives each
/// road's links, to find where a direct junction's incoming road meets it.
/// A common junction's incoming road whose `<link>` leaves the junction out
/// gets it, with a warning. See [`missing_links`].
///
/// A virtual junction's roads meet its main road part way along it, which
/// the lane graph can't, so links with an `elementS` are dropped from it,
/// on any road, as are its connections from the main road and its deprecated
/// `type="virtual"` connections, and its incoming roads are not linked into
/// it.
pub(crate) fn junctions(
    root: roxmltree::Node,
    roads: &mut HashMap<String, RoadInfo>,
) -> (HashMap<String, Vec<JunctionConn>>, Vec<Warning>) {
    let mut out = HashMap::new();
    let mut warnings = Vec::new();
    for road in roads.values_mut() {
        for slot in [&mut road.predecessor, &mut road.successor] {
            if slot.as_ref().is_some_and(|t| t.along) {
                *slot = None;
            }
        }
    }
    for j in root.children().filter(|n| n.has_tag_name("junction")) {
        let Some(jid) = j.attribute("id") else {
            warnings.extend(
                j.children()
                    .filter(|n| n.has_tag_name("connection"))
                    .map(|c| Warning::ConnectionDropped {
                        incoming_road_id: c
                            .attribute("incomingRoad")
                            .unwrap_or_default()
                            .to_string(),
                        junction_id: String::new(),
                        connection_id: c.attribute("id").unwrap_or_default().to_string(),
                    }),
            );
            continue;
        };
        let direct = j.attribute("type") == Some("direct");
        let is_virtual = j.attribute("type") == Some("virtual");
        let target = if direct {
            "linkedRoad"
        } else {
            "connectingRoad"
        };
        let mut conns = Vec::new();
        for c in j.children().filter(|n| n.has_tag_name("connection")) {
            let from_main = c.attribute("incomingRoad").is_some()
                && c.attribute("incomingRoad") == j.attribute("mainRoad");
            if is_virtual && (c.attribute("type") == Some("virtual") || from_main) {
                continue;
            }
            let (Some(incoming), Some(connecting)) =
                (c.attribute("incomingRoad"), c.attribute(target))
            else {
                warnings.push(Warning::ConnectionDropped {
                    incoming_road_id: c.attribute("incomingRoad").unwrap_or_default().to_string(),
                    junction_id: jid.to_string(),
                    connection_id: c.attribute("id").unwrap_or_default().to_string(),
                });
                continue;
            };
            let mut lane_links = Vec::new();
            for l in c.children().filter(|n| n.has_tag_name("laneLink")) {
                let id = |name| super::attr_i32(l, name);
                let layer = |name| Layer::from_attr(l.attribute(name));
                match (id("from"), id("to")) {
                    (Some(from), Some(to)) => lane_links.push(JunctionLaneLink {
                        from,
                        to,
                        from_layer: layer("fromLayer"),
                        to_layer: layer("toLayer"),
                    }),
                    _ => warnings.push(Warning::LaneLinkDropped {
                        incoming_road_id: incoming.to_string(),
                        junction_id: jid.to_string(),
                        connection_id: c.attribute("id").unwrap_or_default().to_string(),
                        from: l.attribute("from").unwrap_or_default().to_string(),
                        to: l.attribute("to").unwrap_or_default().to_string(),
                    }),
                }
            }
            conns.push(JunctionConn {
                incoming_road: incoming.to_string(),
                connecting_road: connecting.to_string(),
                contact: contact_of(c),
                lane_links,
            });
        }
        if direct {
            let back = reversed(jid, &conns, roads);
            conns.extend(back);
        } else if !is_virtual {
            warnings.extend(missing_links(jid, &conns, roads));
        }
        out.insert(jid.to_string(), conns);
    }
    (out, warnings)
}

/// Link each incoming road of common junction `jid` whose `<link>` names the
/// junction at neither end into it, at the end the connecting road's own
/// link names, and warn of each. That end is the `contactPoint` of the
/// connecting road's `<predecessor>` or `<successor>`, whichever the
/// connection's `contactPoint` picks, where it names the incoming road. A
/// link back with no `contactPoint` names no end, and an end that already
/// links elsewhere is left as it is: neither is linked.
fn missing_links(
    jid: &str,
    conns: &[JunctionConn],
    roads: &mut HashMap<String, RoadInfo>,
) -> Vec<Warning> {
    let mut warnings = Vec::new();
    for c in conns {
        let Some(road) = roads.get(&c.incoming_road) else {
            continue;
        };
        let names_junction = |t: &Option<LinkTarget>| {
            t.as_ref()
                .is_some_and(|t| t.elem == ElemType::Junction && t.id == jid)
        };
        if names_junction(&road.predecessor) || names_junction(&road.successor) {
            continue;
        }
        let back = roads.get(&c.connecting_road).and_then(|r| match c.contact {
            Contact::Start => r.predecessor.as_ref(),
            Contact::End => r.successor.as_ref(),
        });
        let Some(back) =
            back.filter(|b| b.elem == ElemType::Road && b.id == c.incoming_road && b.contact_given)
        else {
            continue;
        };
        let end = back.contact;
        let Some(road) = roads.get_mut(&c.incoming_road) else {
            continue;
        };
        let slot = match end {
            Contact::Start => &mut road.predecessor,
            Contact::End => &mut road.successor,
        };
        if slot.is_some() {
            continue;
        }
        *slot = Some(LinkTarget {
            elem: ElemType::Junction,
            id: jid.to_string(),
            contact: end,
            contact_given: true,
            along: false,
        });
        warnings.push(Warning::JunctionLinkMissing {
            road_id: c.incoming_road.clone(),
            junction_id: jid.to_string(),
            end: match end {
                Contact::Start => RoadEnd::Start,
                Contact::End => RoadEnd::End,
            },
        });
    }
    warnings
}

/// The connections of direct junction `jid` run the other way, from the
/// linked road back into the incoming road, where `conns` does not already
/// give that pair. The incoming road meets the junction at the end whose
/// road link names it, as esmini finds it. A pair whose incoming road names
/// the junction at neither end gets none.
fn reversed(
    jid: &str,
    conns: &[JunctionConn],
    roads: &HashMap<String, RoadInfo>,
) -> Vec<JunctionConn> {
    let names_junction = |target: &Option<LinkTarget>| {
        target
            .as_ref()
            .is_some_and(|t| t.elem == ElemType::Junction && t.id == jid)
    };
    conns
        .iter()
        .filter(|c| {
            !conns.iter().any(|o| {
                o.incoming_road == c.connecting_road && o.connecting_road == c.incoming_road
            })
        })
        .filter_map(|c| {
            let road = roads.get(&c.incoming_road)?;
            let contact = if names_junction(&road.successor) {
                Contact::End
            } else if names_junction(&road.predecessor) {
                Contact::Start
            } else {
                return None;
            };
            Some(JunctionConn {
                incoming_road: c.connecting_road.clone(),
                connecting_road: c.incoming_road.clone(),
                contact,
                lane_links: c
                    .lane_links
                    .iter()
                    .map(|l| JunctionLaneLink {
                        from: l.to,
                        to: l.from,
                        from_layer: l.to_layer,
                        to_layer: l.from_layer,
                    })
                    .collect(),
            })
        })
        .collect()
}
