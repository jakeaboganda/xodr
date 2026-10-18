//! Routing over the baked lane graph: a shortest lane path, sampled into a
//! drivable sequence of positions.
//!
//! Routes over both **longitudinal** (successor) and **lane-change** (lateral
//! neighbor) edges. Dijkstra with a flat lane-change penalty, then centerline
//! sampling in travel order (a lane change drives a short way, then hops to the
//! neighbor beside it for a path tracker to smooth). Speed is intentionally
//! absent: the route is geometry, and the caller stamps its own speed onto the
//! waypoints.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::coords::Point;

use crate::network::{Direction, Lane, LaneId, LayerPolicy, RoadNetwork};

/// Waypoint spacing (meters) when sampling a route's lane centerlines.
const ROUTE_STEP: f32 = 2.0;
/// Cost (meters-equivalent) charged for a lane change, so the router prefers
/// staying in lane but will change when needed to reach the goal.
const LANE_CHANGE_COST: f32 = 30.0;
/// How far to drive along a lane before a lane change, so the transition isn't
/// an instantaneous sideways hop (the path tracker smooths the remaining jump).
const LANE_CHANGE_DIST: f32 = 10.0;

impl RoadNetwork {
    /// A drivable sequence of positions from `from` to `to` along the lane
    /// graph, or `None` if unreachable. Snaps each endpoint to its nearest
    /// driving lane, finds the shortest lane path over successor + lane-change
    /// edges, and samples the lane centerlines (in travel order) into points.
    /// The route never runs against a lane's traffic: a goal behind the start
    /// on a one-way lane is reached round the network, or not at all.
    pub fn route(&self, from: Point, to: Point) -> Option<Vec<Point>> {
        self.route_with(from, to, LayerPolicy::default())
    }

    /// [`Self::route`] under `policy`: with [`LayerPolicy::PermanentOnly`] the
    /// route snaps to, and runs over, the permanent layer's lanes alone.
    pub fn route_with(&self, from: Point, to: Point, policy: LayerPolicy) -> Option<Vec<Point>> {
        let (start, start_proj) = self.nearest_lane_with(from, policy)?;
        let (goal, goal_proj) = self.nearest_lane_with(to, policy)?;
        let path = self.lane_path((start, start_proj.s), (goal, goal_proj.s), policy)?;
        Some(self.sample_route(&path, goal_proj.s))
    }

    /// Shortest lane path from `start` to `goal`, each a lane and a distance
    /// along its centerline, with how and where the route enters each lane.
    /// An edge costs the metres driven on the lane it leaves, from where the
    /// route entered it, and a lane change a flat penalty on top. Stopping
    /// costs the metres left to the goal. The route stops on the goal lane
    /// only where it enters it behind the goal. `None` if unreachable.
    ///
    /// Where a lane change lands decides where the next one can, so the
    /// search is over labels, each a way into a lane with its cost and where
    /// it enters, and drops a label only for one as cheap that enters no
    /// later.
    fn lane_path(
        &self,
        (start, start_s): (LaneId, f32),
        (goal, goal_s): (LaneId, f32),
        policy: LayerPolicy,
    ) -> Option<Vec<(LaneId, Entry, f32)>> {
        let goal_lane = self.lane(goal)?;
        if start == goal && ahead(goal_lane.direction, start_s, goal_s) {
            return Some(vec![(start, Entry::Change, start_s)]);
        }
        let mut search = Search::default();
        search.add(
            self,
            Label {
                lane: start,
                entry: Entry::Change,
                at: start_s,
                stops: false,
                cost: 0.0,
                prev: None,
            },
        );
        while let Some(State { label, .. }) = search.heap.pop() {
            if search.dropped[label] {
                continue;
            }
            if search.labels[label].stops {
                return Some(search.path(label));
            }
            for mut next in self.edges(&search.labels[label], policy) {
                next.prev = Some(label);
                if next.lane == goal && ahead(goal_lane.direction, next.at, goal_s) {
                    search.add(
                        self,
                        Label {
                            stops: true,
                            cost: next.cost + (goal_s - next.at).abs(),
                            ..next
                        },
                    );
                }
                search.add(self, next);
            }
        }
        None
    }

    /// The labels one edge on from `from`: a successor entered at the start
    /// of its travel, or a neighbour entered beside the point
    /// [`LANE_CHANGE_DIST`] on, as [`Self::sample_route`] drives it. Each is
    /// yet to be linked to `from`. A two-way lane entered at one end is left
    /// only by the successors at its other end, so a route never turns back
    /// on it, and a two-way successor is entered at the end it meets.
    fn edges(&self, from: &Label, policy: LayerPolicy) -> Vec<Label> {
        let Some(l) = self.lane(from.lane) else {
            return Vec::new();
        };
        let len = l.center.length();
        let label = |lane, entry, at, cost| Label {
            lane,
            entry,
            at,
            stops: false,
            cost,
            prev: None,
        };
        let leaves_at_end = |n: &LaneId| match (l.direction, from.entry) {
            (Direction::Forward, _) => true,
            (Direction::Backward, _) => false,
            (Direction::Both, Entry::Start) => from.at < len / 2.0,
            (Direction::Both, Entry::Change) => self.nearer_end(l, Some(n)),
        };
        let successors = l.successors.iter().filter_map(|&n| {
            let next = self.lane(n)?;
            let at_end = leaves_at_end(&n);
            if l.direction == Direction::Both && at_end != self.nearer_end(l, Some(&n)) {
                return None;
            }
            let at = match next.direction {
                Direction::Forward => 0.0,
                Direction::Backward => next.center.length(),
                Direction::Both => {
                    let exit = l.center.point_at(if at_end { len } else { 0.0 });
                    let points = next.center.points();
                    let first = points[0].distance_squared_to(exit);
                    let last = points[points.len() - 1].distance_squared_to(exit);
                    if first <= last {
                        0.0
                    } else {
                        next.center.length()
                    }
                }
            };
            let driven = (if at_end { len } else { 0.0 } - from.at).abs();
            Some(label(n, Entry::Start, at, from.cost + driven))
        });
        let changes = l.neighbors.iter().filter_map(|&n| {
            let next = self.lane(n)?;
            let forward = match l.direction {
                Direction::Forward => true,
                Direction::Backward => false,
                Direction::Both => self.nearer_end(l, Some(&n)),
            };
            let travel_end = if forward { len } else { 0.0 };
            let exit = advance(from.at, travel_end, LANE_CHANGE_DIST);
            let at = next.center.project(l.center.point_at(exit)).s;
            let driven = (exit - from.at).abs();
            Some(label(
                n,
                Entry::Change,
                at,
                from.cost + driven + LANE_CHANGE_COST,
            ))
        });
        successors
            .chain(changes)
            .filter(|next| self.lane(next.lane).is_some_and(|n| policy.admits(n)))
            .collect()
    }

    /// Sample the lane path into points: each lane's centerline traversed in its
    /// travel direction, from where the search entered it to where the next
    /// lane leaves it, or on the last lane to the goal.
    fn sample_route(&self, path: &[(LaneId, Entry, f32)], goal_s: f32) -> Vec<Point> {
        let last = path.len().saturating_sub(1);
        let mut pts: Vec<Point> = Vec::new();
        for (i, &(lid, how, at)) in path.iter().enumerate() {
            let Some(lane) = self.lane(lid) else { continue };
            let next = path.get(i + 1);
            let len = lane.center.length();
            let entry = at.clamp(0.0, len);
            let forward = match (lane.direction, how) {
                (Direction::Forward, _) => true,
                (Direction::Backward, _) => false,
                (Direction::Both, Entry::Start) => entry < len / 2.0,
                (Direction::Both, Entry::Change) => self.nearer_end(lane, next.map(|n| &n.0)),
            };
            let travel_end = if forward { len } else { 0.0 };
            let exit = if i == last {
                let goal = goal_s.clamp(0.0, len);
                if ahead(lane.direction, entry, goal) {
                    goal
                } else {
                    entry
                }
            } else if next.is_some_and(|n| n.1 == Entry::Change) {
                advance(entry, travel_end, LANE_CHANGE_DIST)
            } else {
                travel_end
            };
            sample_segment(lane, entry, exit, &mut pts);
        }
        pts
    }
}

impl RoadNetwork {
    /// Whether `next` lies nearer the end of `lane`'s centerline than its
    /// start, so a two-way lane is driven along its centerline to reach it.
    /// A lane with nothing after it on the route is driven along it.
    fn nearer_end(&self, lane: &Lane, next: Option<&LaneId>) -> bool {
        let Some(next) = next.and_then(|&id| self.lane(id)) else {
            return true;
        };
        let points = lane.center.points();
        let away = |p: Point| (next.center.project(p).point - p).length_squared();
        away(points[points.len() - 1]) <= away(points[0])
    }
}

/// Whether `to` is reachable from `from` along a lane running `direction`,
/// both distances along its centerline.
fn ahead(direction: Direction, from: f32, to: f32) -> bool {
    match direction {
        Direction::Forward => to >= from,
        Direction::Backward => to <= from,
        Direction::Both => true,
    }
}

/// Move `dist` from `from` toward `toward`, without overshooting `toward`.
fn advance(from: f32, toward: f32, dist: f32) -> f32 {
    let step = (toward - from).signum() * dist;
    (from + step).clamp(from.min(toward), from.max(toward))
}

/// Append points along `lane`'s centerline from arc length `entry` to `exit`
/// (either direction), spaced ~`ROUTE_STEP`, deduping the join with prior lanes.
fn sample_segment(lane: &Lane, entry: f32, exit: f32, pts: &mut Vec<Point>) {
    let span = exit - entry;
    if span.abs() < 1e-4 {
        push_dedup(pts, lane.center.point_at(entry));
        return;
    }
    let steps = (span.abs() / ROUTE_STEP).ceil().max(1.0) as usize;
    for k in 0..=steps {
        let s = entry + span * (k as f32 / steps as f32);
        push_dedup(pts, lane.center.point_at(s));
    }
}

fn push_dedup(pts: &mut Vec<Point>, p: Point) {
    if pts.last().is_none_or(|q| q.distance_squared_to(p) > 1e-6) {
        pts.push(p);
    }
}

/// How the route enters a lane: at the start of its travel from the lane
/// before, or part way along from a lane change. The start lane counts as
/// entered by a change, at the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Entry {
    Start,
    Change,
}

/// One way into a lane the search has found: how and where along its
/// centerline it enters, whether the route stops there at the goal, what it
/// costs, and the label it came from.
#[derive(Clone, Copy)]
struct Label {
    lane: LaneId,
    entry: Entry,
    at: f32,
    stops: bool,
    cost: f32,
    prev: Option<usize>,
}

/// The most labels the search keeps for one lane, entry and stop, so a
/// network of many parallel lanes stays cheap.
const MAX_LABELS: usize = 8;

/// A label search's state: every label, whether each was dropped for a
/// better one, the labels kept for each lane, entry and stop, and the
/// frontier.
#[derive(Default)]
struct Search {
    labels: Vec<Label>,
    dropped: Vec<bool>,
    kept: HashMap<(LaneId, Entry, bool), Vec<usize>>,
    heap: BinaryHeap<State>,
}

impl Search {
    /// Keep `label` unless one kept for its lane is as cheap and enters no
    /// later along its travel, dropping those it beats in turn. One that
    /// beats none is left out once [`MAX_LABELS`] are kept.
    fn add(&mut self, net: &RoadNetwork, label: Label) {
        let Some(lane) = net.lane(label.lane) else {
            return;
        };
        let beats = |a: &Label, b: &Label| a.cost <= b.cost && ahead(lane.direction, a.at, b.at);
        let kept = self
            .kept
            .entry((label.lane, label.entry, label.stops))
            .or_default();
        if kept.iter().any(|&k| beats(&self.labels[k], &label)) {
            return;
        }
        kept.retain(|&k| {
            let beaten = beats(&label, &self.labels[k]);
            self.dropped[k] |= beaten;
            !beaten
        });
        if kept.len() >= MAX_LABELS {
            return;
        }
        let id = self.labels.len();
        kept.push(id);
        self.labels.push(label);
        self.dropped.push(false);
        self.heap.push(State {
            cost: label.cost,
            label: id,
            key: (label.lane.0, label.entry, label.stops, id),
        });
    }

    /// The lanes from the first label to `label`, with how and where along
    /// its centerline the route enters each.
    fn path(&self, label: usize) -> Vec<(LaneId, Entry, f32)> {
        let mut path = Vec::new();
        let mut cur = Some(label);
        while let Some(k) = cur {
            let l = &self.labels[k];
            path.push((l.lane, l.entry, l.at));
            cur = l.prev;
        }
        path.reverse();
        path
    }
}

/// A frontier entry, ordered as a min-heap on cost, ties broken by lane,
/// entry, stop and label for deterministic paths.
struct State {
    cost: f32,
    label: usize,
    key: (usize, Entry, bool, usize),
}
impl PartialEq for State {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for State {}
impl Ord for State {
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost
            .total_cmp(&self.cost)
            .then_with(|| o.key.cmp(&self.key))
    }
}
impl PartialOrd for State {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Polyline;
    use crate::network::LaneType;

    fn lane(id: usize, pts: &[[f32; 3]], dir: Direction, succ: &[usize], nbrs: &[usize]) -> Lane {
        Lane {
            id: LaneId(id),
            kind: LaneType::Driving,
            layer: crate::Layer::Permanent,
            direction: dir,
            center: Polyline::new(pts.iter().map(|p| Point::from_array(*p)).collect()),
            width: 3.5,
            widths: Vec::new(),
            bank: Vec::new(),
            successors: succ.iter().map(|&s| LaneId(s)).collect(),
            predecessors: Vec::new(),
            neighbors: nbrs.iter().map(|&s| LaneId(s)).collect(),
        }
    }

    #[test]
    fn a_route_costs_what_it_drives_not_the_whole_lane_it_starts_on() {
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0]],
                Direction::Forward,
                &[1],
                &[2],
            ),
            lane(
                1,
                &[[1000.0, 0.0, 0.0], [1010.0, 0.0, 0.0]],
                Direction::Forward,
                &[3],
                &[],
            ),
            lane(
                2,
                &[[980.0, 3.5, 0.0], [1000.0, 3.5, 0.0]],
                Direction::Forward,
                &[3],
                &[],
            ),
            lane(
                3,
                &[[1010.0, 0.0, 0.0], [1100.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        let path = net
            .route(Point::new(990.0, 0.0, 0.0), Point::new(1050.0, 0.0, 0.0))
            .expect("route");
        assert!(
            path.iter().all(|p| p.y.abs() < 1.0),
            "stays in lane rather than changing for a shorter lane: {path:?}"
        );
    }

    #[test]
    fn same_lane_route() {
        let net = RoadNetwork::new(vec![lane(
            0,
            &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
            Direction::Forward,
            &[],
            &[],
        )]);
        let path = net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(15.0, 0.0, 0.0))
            .expect("route");
        assert!(path.first().unwrap().x < 4.0, "starts near x=2");
        assert!(path.last().unwrap().x > 13.0, "ends near x=15");
        for w in path.windows(2) {
            assert!(w[1].x >= w[0].x - 0.01, "monotonic forward");
        }
    }

    #[test]
    fn two_lane_successor_route() {
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[1],
                &[],
            ),
            lane(
                1,
                &[[20.0, 0.0, 0.0], [40.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        let path = net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(38.0, 0.0, 0.0))
            .expect("route");
        assert!(path.first().unwrap().x < 4.0);
        assert!(path.last().unwrap().x > 36.0);
        assert!(path.iter().any(|p| p.x > 25.0), "crosses into lane B");
    }

    /// Two one-way lanes side by side from x = 0 to 100, each a neighbour of
    /// the other, with nothing after them.
    fn side_by_side() -> RoadNetwork {
        let a = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]];
        let b = [[0.0, -3.5, 0.0], [100.0, -3.5, 0.0]];
        RoadNetwork::new(vec![
            lane(0, &a, Direction::Forward, &[], &[1]),
            lane(1, &b, Direction::Forward, &[], &[0]),
        ])
    }

    #[test]
    fn a_lane_change_that_lands_past_the_goal_does_not_reach_it() {
        let net = side_by_side();
        let from = Point::new(50.0, 0.0, 0.0);
        assert_eq!(net.route(from, Point::new(40.0, 0.0, 0.0)), None);
        assert_eq!(net.route(from, Point::new(55.0, -3.5, 0.0)), None);
        let ahead = net.route(from, Point::new(80.0, -3.5, 0.0)).expect("route");
        let end = *ahead.last().unwrap();
        assert!(
            (end.x - 80.0).abs() < 0.01 && (end.y + 3.5).abs() < 0.01,
            "{end:?}"
        );
    }

    /// A loop from `(100, y)` round to `(0, y)` through `y + 50`.
    fn loop_back(y: f32) -> Vec<[f32; 3]> {
        vec![
            [100.0, y, 0.0],
            [100.0, y + 50.0, 0.0],
            [0.0, y + 50.0, 0.0],
            [0.0, y, 0.0],
        ]
    }

    fn ends_at(route: Option<Vec<Point>>, x: f32, y: f32) {
        let end = *route.expect("a route").last().unwrap();
        assert!(
            (end.x - x).abs() < 0.01 && (end.y - y).abs() < 0.01,
            "{end:?}"
        );
    }

    #[test]
    fn a_lane_change_past_the_goal_does_not_hide_one_behind_it_round_a_loop() {
        let a = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]];
        let g = [[0.0, -3.5, 0.0], [100.0, -3.5, 0.0]];
        let net = RoadNetwork::new(vec![
            lane(0, &a, Direction::Forward, &[1], &[2]),
            lane(1, &loop_back(0.0), Direction::Forward, &[0], &[]),
            lane(2, &g, Direction::Forward, &[], &[0]),
        ]);
        let route = net.route(Point::new(50.0, 0.0, 0.0), Point::new(55.0, -3.5, 0.0));
        ends_at(route, 55.0, -3.5);
    }

    #[test]
    fn a_goal_behind_the_start_is_reached_by_changing_back_onto_the_start_lane() {
        let a = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]];
        let g = [[0.0, -3.5, 0.0], [100.0, -3.5, 0.0]];
        let net = RoadNetwork::new(vec![
            lane(0, &a, Direction::Forward, &[], &[1]),
            lane(1, &g, Direction::Forward, &[2], &[0]),
            lane(2, &loop_back(-3.5), Direction::Forward, &[1], &[]),
        ]);
        let route = net.route(Point::new(50.0, 0.0, 0.0), Point::new(20.0, 0.0, 0.0));
        ends_at(route, 20.0, 0.0);
    }

    #[test]
    fn a_lane_change_entered_past_where_the_next_one_must_start_hides_no_route() {
        // Driving A 85 -> X 95 -> G 100 passes the goal at G 60. Round the
        // loop, A 0 -> X 10 -> G 20 reaches it.
        let at = |y: f32| [[0.0, y, 0.0], [100.0, y, 0.0]];
        let net = RoadNetwork::new(vec![
            lane(0, &at(0.0), Direction::Forward, &[1], &[2]),
            lane(1, &loop_back(0.0), Direction::Forward, &[0], &[]),
            lane(2, &at(-3.5), Direction::Forward, &[], &[0, 3]),
            lane(3, &at(-7.0), Direction::Forward, &[], &[2]),
        ]);
        let route = net.route(Point::new(85.0, 0.0, 0.0), Point::new(60.0, -7.0, 0.0));
        ends_at(route, 60.0, -7.0);
    }

    #[test]
    fn a_goal_behind_on_a_one_way_lane_is_reached_round_the_loop_or_not_at_all() {
        let straight = lane(
            0,
            &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
            Direction::Forward,
            &[],
            &[],
        );
        let net = RoadNetwork::new(vec![straight.clone()]);
        let (from, to) = (Point::new(15.0, 0.0, 0.0), Point::new(5.0, 0.0, 0.0));
        assert_eq!(net.route(from, to), None, "no driving backwards");

        let back = lane(
            1,
            &[
                [20.0, 0.0, 0.0],
                [20.0, 30.0, 0.0],
                [0.0, 30.0, 0.0],
                [0.0, 0.0, 0.0],
            ],
            Direction::Forward,
            &[0],
            &[],
        );
        let net = RoadNetwork::new(vec![
            Lane {
                successors: vec![LaneId(1)],
                ..straight
            },
            back,
        ]);
        let path = net.route(from, to).expect("round the loop");
        assert!(
            (path[0].x - 15.0).abs() < 0.1 && path[1].x > path[0].x,
            "sets off forward"
        );
        assert!(path.iter().any(|p| p.y > 29.0), "goes round");
        assert!(
            (path.last().unwrap().x - 5.0).abs() < 0.1,
            "ends at the goal"
        );
    }

    #[test]
    fn a_goal_behind_on_a_two_way_lane_is_reached_directly() {
        let net = RoadNetwork::new(vec![lane(
            0,
            &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
            Direction::Both,
            &[],
            &[],
        )]);
        let path = net
            .route(Point::new(15.0, 0.0, 0.0), Point::new(5.0, 0.0, 0.0))
            .expect("route");
        assert!((path.last().unwrap().x - 5.0).abs() < 0.1);
        assert!(path.iter().all(|p| p.x <= 15.01));
    }

    #[test]
    fn no_route_when_disconnected() {
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
            lane(
                1,
                &[[100.0, 0.0, 0.0], [120.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        assert!(net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(110.0, 0.0, 0.0))
            .is_none());
    }

    /// Two separate two-lane strips, 500 m apart: nothing links them.
    fn two_components() -> RoadNetwork {
        RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[1],
                &[],
            ),
            lane(
                1,
                &[[20.0, 0.0, 0.0], [40.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
            lane(
                2,
                &[[500.0, 0.0, 0.0], [520.0, 0.0, 0.0]],
                Direction::Forward,
                &[3],
                &[],
            ),
            lane(
                3,
                &[[520.0, 0.0, 0.0], [540.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ])
    }

    #[test]
    fn a_route_between_disconnected_components_is_none() {
        // `None` means "no path", and the caller decides what to do about it.
        // Anything else, an empty plan or a straight line through the void,
        // would read as a route the vehicle could drive.
        let net = two_components();
        assert!(net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(538.0, 0.0, 0.0))
            .is_none());
        // Unreachable in both directions, not just one.
        assert!(net
            .route(Point::new(502.0, 0.0, 0.0), Point::new(38.0, 0.0, 0.0))
            .is_none());
        // And each component still routes within itself, so the graph is sound
        // and it really is the gap that stopped it.
        assert!(net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(38.0, 0.0, 0.0))
            .is_some());
        assert!(net
            .route(Point::new(502.0, 0.0, 0.0), Point::new(538.0, 0.0, 0.0))
            .is_some());
    }

    #[test]
    fn a_cyclic_lane_graph_terminates() {
        // A roundabout is a cycle, and every real map has them. Dijkstra
        // handles this by construction; the point is that nothing here walks
        // the graph naively, on the map where a hang costs the caller its
        // whole tick.
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[1],
                &[],
            ),
            lane(
                1,
                &[[20.0, 0.0, 0.0], [20.0, 0.0, 20.0]],
                Direction::Forward,
                &[2],
                &[],
            ),
            lane(
                2,
                &[[20.0, 0.0, 20.0], [0.0, 0.0, 0.0]],
                Direction::Forward,
                &[0],
                &[],
            ),
            // Hangs off the ring but leads back nowhere.
            lane(
                3,
                &[[100.0, 0.0, 0.0], [120.0, 0.0, 0.0]],
                Direction::Forward,
                &[0],
                &[],
            ),
        ]);
        assert!(net
            .route(Point::new(1.0, 0.0, 0.0), Point::new(20.0, 0.0, 18.0))
            .is_some());
        // All the way round, back to where it started.
        assert!(net
            .route(Point::new(1.0, 0.0, 0.0), Point::new(19.0, 0.0, 0.0))
            .is_some());
        // Into the one-way spur: unreachable, and the cycle must not spin.
        assert!(net
            .route(Point::new(1.0, 0.0, 0.0), Point::new(118.0, 0.0, 0.0))
            .is_none());
    }

    #[test]
    fn routing_the_same_pair_twice_gives_an_identical_path() {
        // Two equal-cost routes to the same goal. The `total_cmp` + lane-id
        // tie-break in `State`'s `Ord` is what makes the choice between them
        // stable; without it the winner would follow heap/hash order, and two
        // identical requests could disagree.
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[1, 2],
                &[],
            ),
            lane(
                1,
                &[[20.0, 0.0, 0.0], [40.0, 0.0, 5.0]],
                Direction::Forward,
                &[3],
                &[],
            ),
            lane(
                2,
                &[[20.0, 0.0, 0.0], [40.0, 0.0, -5.0]],
                Direction::Forward,
                &[3],
                &[],
            ),
            lane(
                3,
                &[[40.0, 0.0, 0.0], [60.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        let first = net
            .route(Point::new(1.0, 0.0, 0.0), Point::new(58.0, 0.0, 0.0))
            .expect("route");
        for _ in 0..20 {
            let again = net
                .route(Point::new(1.0, 0.0, 0.0), Point::new(58.0, 0.0, 0.0))
                .expect("route");
            assert_eq!(first, again, "the same request produced a different path");
        }
    }

    #[test]
    fn a_route_never_contains_a_non_finite_point() {
        // Waypoints go straight out as a plan, and a non-finite one reaches
        // whatever applies forces from there.
        let net = two_components();
        let route = net
            .route(Point::new(2.0, 0.0, 0.0), Point::new(38.0, 0.0, 0.0))
            .expect("route");
        assert!(route.iter().all(|p| p.is_finite()));

        // Including from a caller asking about somewhere absurd: the endpoints
        // snap to the nearest lane, so a far-away request is still answered in
        // real coordinates.
        let far = net.route(
            Point::new(1.0e30, 0.0, -1.0e30),
            Point::new(-1.0e30, 0.0, 1.0e30),
        );
        assert!(far.is_none_or(|r| r.iter().all(|p| p.is_finite())));

        // The built-in road, sampled end to end.
        let demo = crate::fixtures::demo_road();
        let (first, last) = (
            demo.lanes()[0].center.point_at(0.0),
            demo.lanes()[0]
                .center
                .point_at(demo.lanes()[0].center.length()),
        );
        if let Some(route) = demo.route(first, last) {
            assert!(route.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn route_changes_lanes_to_reach_the_exit() {
        // Lane A (z=0) is a dead end; its neighbor B (z=-3.5) leads to C. The
        // goal is on C, so the route must change A -> B, then drive B -> C.
        let net = RoadNetwork::new(vec![
            lane(
                0,
                &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]],
                Direction::Forward,
                &[],
                &[1],
            ),
            lane(
                1,
                &[[0.0, 0.0, -3.5], [20.0, 0.0, -3.5]],
                Direction::Forward,
                &[2],
                &[0],
            ),
            lane(
                2,
                &[[20.0, 0.0, -3.5], [40.0, 0.0, -3.5]],
                Direction::Forward,
                &[],
                &[],
            ),
        ]);
        let path = net
            .route(Point::new(0.0, 0.0, 0.0), Point::new(38.0, 0.0, -3.5))
            .expect("route");
        assert!(
            path.iter().any(|p| p.z.abs() < 0.5),
            "starts on lane A (z~0)"
        );
        assert!(
            path.iter().any(|p| (p.z + 3.5).abs() < 0.5),
            "moves onto the z=-3.5 lanes"
        );
        assert!(path.last().unwrap().x > 36.0, "reaches C");
    }
}
