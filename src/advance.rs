//! Moving a lane position along the lanes: a distance down the lane graph, and
//! across to the lane beside it.

use std::collections::HashSet;

use crate::coords::Point;
use crate::network::{Direction, Lane, LaneId, LayerPolicy, RoadNetwork};
use crate::road::{station_at, LanePosition, RoadSection};
use crate::LaneChange;

/// Where one branch of [`RoadNetwork::advance`] ended.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Advance {
    /// The branch went the whole distance, to here.
    Reached(LanePosition),
    /// The branch ran out of lanes: the lane it was on ends with nothing
    /// after it. `at` is the end of that lane, and `short` how far short of
    /// the distance it stopped, in metres.
    DeadEnd {
        /// Where the lane ends.
        at: LanePosition,
        /// The distance left, in metres.
        short: f64,
    },
}

impl Advance {
    /// Where the branch ended, whether it went the whole distance or not.
    pub fn position(&self) -> LanePosition {
        match *self {
            Self::Reached(at) | Self::DeadEnd { at, .. } => at,
        }
    }
}

/// One branch in flight: the lane it is on, how far along its centerline,
/// which way it moves along it, its offset to the left of the traffic, and
/// the distance still to go.
struct Branch {
    lane: LaneId,
    arc: f32,
    up: bool,
    left: f64,
    remaining: f64,
}

impl RoadNetwork {
    /// Every place `distance` metres on from `from` along the lanes, one per
    /// branch, as CARLA's `Waypoint::GetNext` gives them. The caller picks.
    ///
    /// A positive distance goes the way the lane's traffic runs, onto its
    /// [`successors`](crate::Lane::successors), and a negative one back
    /// against it, onto its predecessors. The distance is measured along the
    /// lanes' centerlines, so it is how far a vehicle on them travels.
    /// esmini's `MoveAlongS` and CARLA step by road `s` instead, which is
    /// shorter on the outside of a bend. The offset keeps its side of the
    /// traffic, so it flips sign onto a road that runs the other way.
    ///
    /// A [`Direction::Both`] lane runs either way. From one, a branch goes
    /// each way, and one that drives onto one carries on away from where it
    /// came in. On such a lane, the side of the traffic is the side of the
    /// way the branch moves when the distance is positive.
    ///
    /// A branch that reaches a lane with nothing after it stops there, as an
    /// [`Advance::DeadEnd`]. Branches that meet again, entering the same
    /// lane at the same end with the same distance left to within a
    /// millimetre, go on as one, and give their place once. Every other fork
    /// splits the branches, and on a map with loops paths that meet again
    /// have rarely come the same length, so the branches grow exponentially
    /// with the distance: see [Moving along the lanes](crate#moving-along-the-lanes).
    /// Step a few metres at a time. Empty if `from` is not on a lane of the
    /// network's roads, or `distance` is not finite.
    pub fn advance(&self, from: LanePosition, distance: f64) -> Vec<Advance> {
        self.advance_with(from, distance, LayerPolicy::default())
    }

    /// [`Self::advance`] under `policy`: with [`LayerPolicy::PermanentOnly`]
    /// a branch never goes on to a lane of the temporary layer.
    pub fn advance_with(
        &self,
        from: LanePosition,
        distance: f64,
        policy: LayerPolicy,
    ) -> Vec<Advance> {
        let (Some(lane), Some(arc)) = (self.lane(from.lane), self.centerline_s(from)) else {
            return Vec::new();
        };
        if !distance.is_finite() {
            return Vec::new();
        }
        let millimetres = |m: f64| (m * 1000.0).round() as i64;
        let mut entered: HashSet<(LaneId, u32, i64, i64)> = HashSet::new();
        let mut placed: HashSet<(LaneId, i64, i64, Option<i64>)> = HashSet::new();
        let ahead = distance >= 0.0;
        // Which way the traffic runs along the centerline, for a branch
        // moving `up` it: 1.0 along it.
        let side = move |up: bool| if up == ahead { 1.0 } else { -1.0 };
        let branch = |up: bool| Branch {
            lane: lane.id,
            arc,
            up,
            left: from.offset * side(up),
            remaining: distance.abs(),
        };
        let mut todo = match lane.direction {
            Direction::Both => vec![branch(false), branch(true)],
            Direction::Forward => vec![branch(ahead)],
            Direction::Backward => vec![branch(!ahead)],
        };
        let mut out: Vec<Advance> = Vec::new();
        while let Some(Branch {
            lane,
            arc,
            up,
            left,
            remaining,
        }) = todo.pop()
        {
            let Some(lane) = self.lane(lane) else {
                continue;
            };
            let length = lane.center.length();
            let room = f64::from(if up { length - arc } else { arc });
            let exit = lane.center.point_at(if up { length } else { 0.0 });
            let linked = if ahead {
                &lane.successors
            } else {
                &lane.predecessors
            };
            let next: Vec<&Lane> = linked
                .iter()
                .filter_map(|&id| self.lane(id))
                .filter(|next| policy.admits(next))
                .filter(|next| lane.direction != Direction::Both || self.at_end(lane, next, up))
                .collect();
            if remaining > room && !next.is_empty() {
                todo.extend(next.iter().rev().filter_map(|next| {
                    let up = match next.direction {
                        Direction::Forward => ahead,
                        Direction::Backward => !ahead,
                        Direction::Both => {
                            let points = next.center.points();
                            exit.distance_to(points[0])
                                <= exit.distance_to(points[points.len() - 1])
                        }
                    };
                    let branch = Branch {
                        lane: next.id,
                        arc: if up { 0.0 } else { next.center.length() },
                        up,
                        left,
                        remaining: remaining - room,
                    };
                    let key = (
                        next.id,
                        branch.arc.to_bits(),
                        millimetres(branch.remaining),
                        millimetres(left),
                    );
                    entered.insert(key).then_some(branch)
                }));
                continue;
            }
            let moved = remaining.min(room) as f32;
            let arc = if up { arc + moved } else { arc - moved };
            let Some(at) = self.on_centerline(lane.id, arc, left * side(up)) else {
                continue;
            };
            let end = if remaining > room {
                Advance::DeadEnd {
                    at,
                    short: remaining - room,
                }
            } else {
                Advance::Reached(at)
            };
            let short = match end {
                Advance::DeadEnd { short, .. } => Some(millimetres(short)),
                Advance::Reached(_) => None,
            };
            if placed.insert((at.lane, millimetres(at.s), millimetres(at.offset), short)) {
                out.push(end);
            }
        }
        out
    }

    /// Whether `next` joins `lane` at the end a branch moving `up` its
    /// centerline leaves by, rather than the other. What tells a two-way
    /// lane's links at one end from those at the other.
    fn at_end(&self, lane: &Lane, next: &Lane, up: bool) -> bool {
        let points = lane.center.points();
        let (exit, other) = if up {
            (points[points.len() - 1], points[0])
        } else {
            (points[0], points[points.len() - 1])
        };
        let away = |p: Point| (next.center.project(p).point - p).length_squared();
        away(exit) <= away(other)
    }

    /// The lane beside `at`'s on the left of its traffic, at the same `s`
    /// and at its center, or `None` if there is none. The lane of any type,
    /// running either way, so the left of a lane beside the center line is
    /// the first lane of the other side. Whether traffic may change into it
    /// is the caller's to decide.
    pub fn left_of(&self, at: LanePosition) -> Option<LanePosition> {
        self.beside(at, 1.0)
    }

    /// The lane beside `at`'s on the right of its traffic, as
    /// [`Self::left_of`] finds the left.
    pub fn right_of(&self, at: LanePosition) -> Option<LanePosition> {
        self.beside(at, -1.0)
    }

    /// Whether the road mark between `at`'s lane and the lane on the left
    /// of its traffic lets a vehicle cross into that lane, the lane
    /// [`Self::left_of`] gives. `None` where there is no lane there, no
    /// mark on that border at `at`'s `s`, or a mark whose `laneChange` the
    /// crate does not recognise. Where two marks meet, the one starting
    /// there answers.
    ///
    /// It reads only the mark's [`LaneChange`], whatever its type. Whether
    /// the lane runs the other way, or is one traffic may use at all, is
    /// the caller's to decide.
    pub fn may_change_left(&self, at: LanePosition) -> Option<bool> {
        self.may_change(at, 1.0)
    }

    /// Whether the road mark between `at`'s lane and the lane on the right
    /// of its traffic lets a vehicle cross into that lane, as
    /// [`Self::may_change_left`] reads the left.
    pub fn may_change_right(&self, at: LanePosition) -> Option<bool> {
        self.may_change(at, -1.0)
    }

    /// Whether the mark on the border to the lane on the `side` of `at`'s
    /// traffic, 1.0 for the left, lets a vehicle cross it.
    fn may_change(&self, at: LanePosition, side: f64) -> Option<bool> {
        let (section, here, there) = self.across(at, side)?;
        let border = if here.signum() != there.signum() {
            0
        } else if here.abs() < there.abs() {
            here
        } else {
            there
        };
        let span = section
            .marks
            .iter()
            .rfind(|m| m.od_lane_id == border && m.start <= at.s && at.s <= m.end)?;
        let increase = there > here;
        match self.road_mark(span.mark)?.lane_change {
            LaneChange::Both => Some(true),
            LaneChange::Increase => Some(increase),
            LaneChange::Decrease => Some(!increase),
            LaneChange::None => Some(false),
            LaneChange::Unknown => None,
        }
    }

    /// The lane beside `at`'s on the `side` of its traffic, 1.0 for the left.
    fn beside(&self, at: LanePosition, side: f64) -> Option<LanePosition> {
        let (section, _, there) = self.across(at, side)?;
        let &(_, id) = section.lanes.iter().find(|&&(od_id, _)| od_id == there)?;
        Some(LanePosition {
            lane: id,
            s: at.s,
            offset: 0.0,
        })
    }

    /// `at`'s lane section, and the `<lane id>`s of `at`'s lane and of the
    /// baked lane beside it on the `side` of its traffic, 1.0 for the left.
    fn across(&self, at: LanePosition, side: f64) -> Option<(&RoadSection, i32, i32)> {
        let lane = self.lane(at.lane)?;
        let on = self.road_lane(at.lane)?;
        let section = self.road(on.road)?.section(on.section)?;
        if !(section.start..=section.end).contains(&at.s) {
            return None;
        }
        let mut across: Vec<i32> = section.lanes.iter().map(|&(od_id, _)| od_id).collect();
        across.sort_by_key(|&od_id| std::cmp::Reverse(od_id));
        let here = across.iter().position(|&od_id| od_id == on.od_id)?;
        let there = if side * travel(lane.direction) > 0.0 {
            here.checked_sub(1)?
        } else {
            here + 1
        };
        Some((section, on.od_id, *across.get(there)?))
    }

    /// The lane position `arc` metres along `lane`'s centerline, `offset`
    /// from its center toward `+t`.
    fn on_centerline(&self, lane: LaneId, arc: f32, offset: f64) -> Option<LanePosition> {
        let on = self.road_lane(lane)?;
        let section = self.road(on.road)?.section(on.section)?;
        let points = self.lane(lane)?.center.points();
        Some(LanePosition {
            lane,
            s: station_at(points, &section.stations, arc),
            offset,
        })
    }
}

/// 1.0 for a lane whose traffic runs along `+s`, or either way, and -1.0
/// against it.
fn travel(direction: Direction) -> f64 {
    match direction {
        Direction::Forward | Direction::Both => 1.0,
        Direction::Backward => -1.0,
    }
}
