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
        Some(self.sample_route(&path, start_proj.s, goal_proj.s))
    }

    /// Shortest lane path from `start` to `goal`, each a lane and a distance
    /// along its centerline (Dijkstra), with how the route enters each lane:
    /// successor edges cost the traversed lane's length, lane-change edges a
    /// flat penalty. The route stops on the goal lane only where it enters it
    /// behind the goal. `None` if unreachable.
    fn lane_path(
        &self,
        (start, start_s): (LaneId, f32),
        (goal, goal_s): (LaneId, f32),
        policy: LayerPolicy,
    ) -> Option<Vec<(LaneId, Entry)>> {
        let goal_lane = self.lane(goal)?;
        if start == goal && ahead(goal_lane.direction, start_s, goal_s) {
            return Some(vec![(start, Entry::Change)]);
        }
        let origin = Node {
            lane: start,
            entry: Entry::Change,
            stops: false,
        };
        let stops = |next: Node, at: Option<f32>| {
            next.lane == goal && at.is_none_or(|at| ahead(goal_lane.direction, at, goal_s))
        };
        let mut search = Search::default();
        search.dist.insert(origin, 0.0);
        search.entered.insert(origin, start_s);
        search.heap.push(State {
            cost: 0.0,
            node: origin,
        });
        while let Some(State { cost, node }) = search.heap.pop() {
            if cost > *search.dist.get(&node).unwrap_or(&f32::INFINITY) {
                continue; // a stale, longer entry
            }
            if node.stops {
                return Some(reconstruct(&search.prev, origin, node));
            }
            search.expand(self, node, cost, policy, stops);
        }
        None
    }

    /// Sample the lane path into points: each lane's centerline traversed in its
    /// travel direction, from the first lane's entry projection to the last
    /// lane's exit projection.
    fn sample_route(&self, path: &[(LaneId, Entry)], start_s: f32, goal_s: f32) -> Vec<Point> {
        let last = path.len().saturating_sub(1);
        let mut pts: Vec<Point> = Vec::new();
        // The handoff point between lanes; the next lane starts where this maps
        // onto it (so a lane change enters the neighbor beside where we left).
        let mut cursor: Option<Point> = None;
        for (i, &(lid, _)) in path.iter().enumerate() {
            let Some(lane) = self.lane(lid) else { continue };
            let next = path.get(i + 1);
            let len = lane.center.length();
            // Travel order: a forward lane runs along its polyline, a backward
            // one against it, and a two-way one toward the next lane.
            let forward = match lane.direction {
                Direction::Forward => true,
                Direction::Backward => false,
                Direction::Both => self.nearer_end(lane, next.map(|n| &n.0)),
            };
            let (travel_start, travel_end) = if forward { (0.0, len) } else { (len, 0.0) };
            let entry = match (i, cursor) {
                (0, _) => start_s.clamp(0.0, len),
                (_, Some(c)) => lane.center.project(c).s,
                _ => travel_start,
            };
            // Whether we leave this lane by a lane change: if so, drive only
            // a short way before switching.
            let changes_off = next.is_some_and(|n| n.1 == Entry::Change);
            let exit = if i == last {
                let goal = goal_s.clamp(0.0, len);
                if ahead(lane.direction, entry, goal) {
                    goal
                } else {
                    entry
                }
            } else if changes_off {
                advance(entry, travel_end, LANE_CHANGE_DIST)
            } else {
                travel_end
            };
            sample_segment(lane, entry, exit, &mut pts);
            cursor = Some(lane.center.point_at(exit));
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

/// The lanes of the path from `origin` to `goal` through `prev`, with how
/// the route enters each.
fn reconstruct(prev: &HashMap<Node, Node>, origin: Node, goal: Node) -> Vec<(LaneId, Entry)> {
    let mut path = vec![(goal.lane, goal.entry)];
    let mut cur = goal;
    while cur != origin {
        cur = prev[&cur];
        path.push((cur.lane, cur.entry));
    }
    path.reverse();
    path
}

/// How the route enters a lane: at the start of its travel from the lane
/// before, or part way along from a lane change. The start lane counts as
/// entered by a change, at the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Entry {
    Start,
    Change,
}

/// A lane, how the route enters it, and whether the route stops on it at
/// the goal. Where a stop is possible the search keeps both, so an entry
/// past the goal never blocks one behind it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Node {
    lane: LaneId,
    entry: Entry,
    stops: bool,
}

impl Node {
    /// The key that orders nodes of equal cost, for deterministic paths.
    fn key(&self) -> (usize, Entry, bool) {
        (self.lane.0, self.entry, self.stops)
    }
}

/// A Dijkstra search's state: the best cost to each node, the node it was
/// reached from, where along its lane each node entered by a change is
/// entered, and the frontier.
#[derive(Default)]
struct Search {
    dist: HashMap<Node, f32>,
    prev: HashMap<Node, Node>,
    entered: HashMap<Node, f32>,
    heap: BinaryHeap<State>,
}

impl Search {
    /// Relax the edges out of `node`, reached at `cost`: successors cost the
    /// lane's length, lane changes a flat penalty. A lane change drives
    /// [`LANE_CHANGE_DIST`] on, then enters the neighbour beside that point,
    /// as [`RoadNetwork::sample_route`] drives it. `stops(next, at)` says
    /// whether entering `next`, at `at` for a lane change, reaches the goal.
    fn expand(
        &mut self,
        net: &RoadNetwork,
        node: Node,
        cost: f32,
        policy: LayerPolicy,
        stops: impl Fn(Node, Option<f32>) -> bool,
    ) {
        let Some(l) = net.lane(node.lane) else { return };
        let len = l.center.length();
        let entered = self.entered.get(&node).copied();
        let successors = l
            .successors
            .iter()
            .map(|&n| (n, Entry::Start, cost + len, None));
        let changes = l.neighbors.iter().filter_map(|&n| {
            let next = net.lane(n)?;
            let forward = match l.direction {
                Direction::Forward => true,
                Direction::Backward => false,
                Direction::Both => net.nearer_end(l, Some(&n)),
            };
            let (travel_start, travel_end) = if forward { (0.0, len) } else { (len, 0.0) };
            let exit = advance(
                entered.unwrap_or(travel_start),
                travel_end,
                LANE_CHANGE_DIST,
            );
            let at = next.center.project(l.center.point_at(exit)).s;
            Some((n, Entry::Change, cost + LANE_CHANGE_COST, Some(at)))
        });
        for (lane, entry, nd, at) in successors.chain(changes).collect::<Vec<_>>() {
            if !net.lane(lane).is_some_and(|n| policy.admits(n)) {
                continue;
            }
            let next = Node {
                lane,
                entry,
                stops: false,
            };
            let ends = [
                next,
                Node {
                    stops: true,
                    ..next
                },
            ];
            for next in ends.into_iter().filter(|n| !n.stops || stops(*n, at)) {
                if nd < *self.dist.get(&next).unwrap_or(&f32::INFINITY) {
                    self.dist.insert(next, nd);
                    self.prev.insert(next, node);
                    if let Some(at) = at {
                        self.entered.insert(next, at);
                    }
                    self.heap.push(State {
                        cost: nd,
                        node: next,
                    });
                }
            }
        }
    }
}

/// A Dijkstra frontier entry, ordered as a min-heap on cost (ties by lane id,
/// for deterministic paths).
struct State {
    cost: f32,
    node: Node,
}
impl PartialEq for State {
    fn eq(&self, o: &Self) -> bool {
        self.cost == o.cost && self.node == o.node
    }
}
impl Eq for State {}
impl Ord for State {
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost
            .total_cmp(&self.cost)
            .then_with(|| self.node.key().cmp(&o.node.key()))
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
