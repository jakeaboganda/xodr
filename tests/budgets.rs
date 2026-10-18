//! Performance guardrails on the imported city map.
//!
//! The bound is the point, not the number. Routing is typically answered
//! inline on a simulation tick, so a request that takes long enough stalls
//! every body in the scene, one query becoming everyone's dropped frame.

use std::time::Instant;

use xodr::{load_file, Point};

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");

/// One physics tick at 64 Hz. A routing request has to fit inside a tick with
/// room to spare, or the caller feels it.
const TICK: f64 = 1.0 / 64.0;

#[test]
fn routing_across_town07_stays_under_its_budget() {
    let net = load_file(TOWN).expect("load town07");
    let lanes: Vec<_> = net.driving_lanes().collect();
    assert!(
        lanes.len() > 500,
        "town07 imported only {} driving lanes; the budget means nothing on a \
         small map",
        lanes.len()
    );

    // Spread the sample across the map so the routes are long ones.
    let picks: Vec<Point> = (0..16)
        .map(|i| {
            let lane = lanes[i * lanes.len() / 16];
            lane.center.point_at(lane.center.length() * 0.5)
        })
        .collect();

    let mut worst = 0.0_f64;
    let mut routed = 0;
    for from in &picks {
        for to in &picks {
            let start = Instant::now();
            let route = net.route(*from, *to);
            let elapsed = start.elapsed().as_secs_f64();
            worst = worst.max(elapsed);
            if route.is_some() {
                routed += 1;
            }
        }
    }

    assert!(routed > 0, "no pair in the sample routed at all");
    // Deliberately loose: a shared CI box is slow and this must not flake.
    // A regression to the O(lanes)-per-pop lookup blows past it by orders of
    // magnitude, which is what the guardrail is for.
    assert!(
        worst < TICK,
        "the slowest route took {worst:.4}s, past the {TICK:.4}s tick budget"
    );
    println!("worst route over town07: {worst:.6}s across {routed} routed pairs");
}

/// Time `f` over every probe, returning the total.
fn time(probes: &[Point], mut f: impl FnMut(Point)) -> f64 {
    let start = Instant::now();
    for &p in probes {
        f(p);
    }
    start.elapsed().as_secs_f64()
}

#[test]
fn the_indexed_lookups_beat_the_scans_they_replaced() {
    // An absolute microsecond budget would either flake on a slow shared
    // runner or be too loose to catch anything. Racing the index against the
    // full scan in the same process measures the thing that actually matters
    // (that the index is still pruning), and does it the same way on any
    // hardware.
    const SPEEDUP: f64 = 10.0;

    let net = load_file(TOWN).expect("load town07");
    let lanes: Vec<_> = net.driving_lanes().collect();
    assert!(
        lanes.len() > 500,
        "town07 imported only {} driving lanes; a speedup over a small map \
         proves nothing",
        lanes.len()
    );
    let probes: Vec<Point> = (0..64)
        .map(|i| {
            let lane = lanes[i * lanes.len() / 64];
            lane.center.point_at(lane.center.length() * 0.5)
        })
        .collect();

    let indexed = time(&probes, |p| {
        std::hint::black_box(net.nearest_lane(p));
    });
    let scanned = time(&probes, |p| {
        let hit = net
            .driving_lanes()
            .map(|l| (l.id, l.center.project(p)))
            .min_by(|(_, a), (_, b)| {
                (p - a.point)
                    .length_squared()
                    .total_cmp(&(p - b.point).length_squared())
            });
        std::hint::black_box(hit);
    });
    assert!(
        scanned > indexed * SPEEDUP,
        "nearest_lane is only {:.1}x faster than the full scan it replaced \
         ({indexed:.6}s vs {scanned:.6}s), so the lane index has stopped pruning",
        scanned / indexed.max(f64::MIN_POSITIVE)
    );

    let mesh = net.surface_mesh();
    let sampler = mesh.sampler();
    assert!(
        probes.iter().all(|p| sampler.height_at(p.x, p.y).is_some()),
        "every probe stands on a lane, so the race times hits, not misses"
    );
    let indexed = time(&probes, |p| {
        std::hint::black_box(sampler.height_at(p.x, p.y));
    });
    let scanned = time(&probes, |p| {
        std::hint::black_box(mesh.height_at(p.x, p.y));
    });
    assert!(
        scanned > indexed * SPEEDUP,
        "MeshSampler is only {:.1}x faster than the full triangle scan \
         ({indexed:.6}s vs {scanned:.6}s)",
        scanned / indexed.max(f64::MIN_POSITIVE)
    );
}
