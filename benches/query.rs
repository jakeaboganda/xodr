//! Per-query cost on a city map: the lookups a consumer runs every tick, for
//! every body, forever. These are the numbers that decide whether a map is
//! usable at scale, not import time.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use xodr::{load_file, Point, RoadNetwork};

const TOWN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/town07.xodr");

fn town() -> RoadNetwork {
    load_file(TOWN).expect("town07 loads")
}

/// Points spread across the map, each sitting on some lane's centerline, the
/// realistic case where a query does land on the road.
fn probes(net: &RoadNetwork, count: usize) -> Vec<Point> {
    let lanes: Vec<_> = net.driving_lanes().collect();
    (0..count)
        .map(|i| {
            let lane = lanes[i * lanes.len() / count];
            lane.center.point_at(lane.center.length() * 0.5)
        })
        .collect()
}

fn bench_queries(c: &mut Criterion) {
    let net = town();
    let probes = probes(&net, 64);
    let ids: Vec<_> = net.driving_lanes().map(|l| l.id).collect();

    let mut group = c.benchmark_group("town07");
    group.bench_function("lane_by_id", |b| {
        b.iter(|| {
            for id in &ids {
                black_box(net.lane(black_box(*id)));
            }
        })
    });
    group.bench_function("nearest_lane", |b| {
        b.iter(|| {
            for p in &probes {
                black_box(net.nearest_lane(black_box(*p)));
            }
        })
    });
    group.bench_function("sample_near", |b| {
        b.iter(|| {
            for p in &probes {
                black_box(net.sample_near(black_box(*p)));
            }
        })
    });
    group.finish();
}

fn bench_route(c: &mut Criterion) {
    let net = town();
    let probes = probes(&net, 8);
    c.bench_function("town07/route", |b| {
        b.iter(|| {
            for from in &probes {
                for to in &probes {
                    black_box(net.route(black_box(*from), black_box(*to)));
                }
            }
        })
    });
}

fn bench_mesh(c: &mut Criterion) {
    let net = town();
    let mesh = net.surface_mesh();
    let probes = probes(&net, 16);
    let sampler = mesh.sampler();
    let mut group = c.benchmark_group("town07");
    group.bench_function("surface_mesh", |b| b.iter(|| black_box(net.surface_mesh())));
    group.bench_function("height_at", |b| {
        b.iter(|| {
            for p in &probes {
                black_box(mesh.height_at(black_box(p.x), black_box(p.y)));
            }
        })
    });
    group.bench_function("height_at_indexed", |b| {
        b.iter(|| {
            for p in &probes {
                black_box(sampler.height_at(black_box(p.x), black_box(p.y)));
            }
        })
    });
    group.bench_function("build_sampler", |b| b.iter(|| black_box(mesh.sampler())));
    group.finish();
}

criterion_group!(benches, bench_queries, bench_route, bench_mesh);
criterion_main!(benches);
