//! The purpose-built banked sweeper imports as the canted road it describes.
//!
//! Superelevation is the reason this file exists: before the `<lateralProfile>`
//! importer, a banked road could not survive import (the tessellator couldn't
//! bank), so this exercises the whole real parse path from disk. Geometry and
//! superelevation become a canted, collider-ready mesh, an angle that peaks
//! through the arc and is flat on the straights, and a sample that leans.

use xodr::load_file;
use xodr::{Point, RoadNetwork};

const SWEEPER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/banked_sweeper.xodr"
);

/// The greatest `bank_at` over a lane, scanned finely along its length.
fn peak_bank(lane: &xodr::Lane) -> f32 {
    let len = lane.center.length();
    let mut peak = 0.0f32;
    let mut s = 0.0;
    while s <= len {
        peak = peak.max(lane.bank_at(s).abs());
        s += 1.0;
    }
    peak
}

#[test]
fn the_banked_sweeper_imports_as_a_canted_road() {
    let net = load_file(SWEEPER).expect("the banked sweeper loads");
    // Two lanes each side.
    assert_eq!(net.driving_lanes().count(), 4, "two lanes each direction");

    // Every lane is flat at its start (both straights meet the cant at 0) and
    // peaks near 0.2 rad through the arc.
    for lane in net.driving_lanes() {
        assert!(
            lane.bank_at(0.0).abs() < 1e-3,
            "lane {:?} should start flat, got {}",
            lane.id,
            lane.bank_at(0.0)
        );
        let peak = peak_bank(lane);
        assert!(
            (peak - 0.2).abs() < 0.02,
            "lane {:?} should bank to ~0.2, peaked at {peak}",
            lane.id
        );
    }
}

#[test]
fn the_banked_sweeper_tessellates_to_a_valid_trimesh() {
    // The whole point: a banked import must still build the static collider. This
    // is exactly the guard the flat testtrack could not provide.
    let net = load_file(SWEEPER).expect("the banked sweeper loads");
    let mesh = net.surface_mesh();
    mesh.validate().expect("the banked sweeper tessellates");
    // Every surface normal still points generally up, cant and all.
    assert!(
        mesh.normals.iter().all(|n| n.z > 0.9),
        "a banked normal points sideways/down"
    );
    // The cant is real: some normal leans measurably off vertical.
    assert!(
        mesh.normals.iter().any(|n| n.z < 0.99),
        "no normal is tilted, so the arc didn't bank"
    );
}

#[test]
fn the_outer_left_lane_rides_above_the_inner_through_the_cant() {
    // Reference-line pivot: with the left side raised, the outer left lane (id 2,
    // further from the reference line) climbs higher than the inner (id 1).
    let net = load_file(SWEEPER).expect("the banked sweeper loads");
    let max_z = |dir| {
        net.driving_lanes()
            .filter(|l| l.direction == dir)
            .map(|l| {
                l.center
                    .points()
                    .iter()
                    .map(|p| p.z)
                    .fold(f32::MIN, f32::max)
            })
            .collect::<Vec<_>>()
    };
    // Left lanes travel Backward under RHT; there are two of them.
    let mut left = max_z(xodr::Direction::Backward);
    left.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(left.len(), 2, "two left lanes");
    assert!(
        left[1] - left[0] > 0.3,
        "outer left lane {} should ride well above inner {}",
        left[1],
        left[0]
    );
}

#[test]
fn sample_near_leans_on_the_banked_arc() {
    let net = load_file(SWEEPER).expect("the banked sweeper loads");
    // The arc apex on the reference line (~45 deg through a R=50 left turn from
    // (50,0)): OD (85.36, 14.66), which the baked frame keeps verbatim.
    let apex = Point::new(85.36, 14.66, 0.0);
    let s = net.sample_near(apex).expect("a sample on the arc");
    assert!(s.point.is_finite(), "sample point {:?}", s.point);
    assert!((s.bank.abs() - 0.2).abs() < 0.03, "apex bank {}", s.bank);
    // The up-normal leans off vertical but still points up.
    assert!(s.up.z < 0.99 && s.up.z > 0.9, "apex up {:?}", s.up);
    assert!(s.up.is_normalized(), "up not unit: {:?}", s.up);
}

#[test]
fn the_banked_import_is_deterministic() {
    let a = load_file(SWEEPER).expect("load a");
    let b = load_file(SWEEPER).expect("load b");
    assert_eq!(
        a, b,
        "two imports of the same banked file must be identical"
    );
    // And RoadNetwork's PartialEq actually compares the baked lanes.
    assert_eq!(
        a,
        RoadNetwork::new(b.lanes().to_vec())
            .with_roads(b.roads().to_vec())
            .with_header(b.header().clone())
    );
}
