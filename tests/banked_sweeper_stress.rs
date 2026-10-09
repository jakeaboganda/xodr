//! Independent stress pass over the superelevation import->bake->consume path.
//!
//! The unit tests cover the pivot math and `sample_at`/`sample_near`/mesh cant
//! in isolation, and `banked_sweeper.rs` loads one purpose-built banked
//! `.xodr`. This file pushes on what a *real* banked file carries: grade and
//! bank composing together, a right-hand (opposite-sign) arc, a full sweep of
//! `sample_near` around the arc, the meshed outer rib end-to-end from disk, a
//! superelevation record that starts mid-road, multi-section + bank, laneOffset +
//! bank, a steep bank, and an empty `<lateralProfile>`.

use xodr::{load_file, load_str};
use xodr::{Direction, RoadNetwork};

const SWEEPER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/banked_sweeper.xodr"
);
const CLIMBING: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/climbing_banked.xodr"
);
const RIGHT_SWEEPER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/right_banked_sweeper.xodr"
);
const MID_ROAD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/mid_road_super.xodr"
);

/// Vertex index whose bank is largest in magnitude on a lane.
fn peak_bank_vertex(lane: &xodr::Lane) -> usize {
    let mut best = 0usize;
    let mut peak = 0.0f32;
    for (i, b) in lane.bank.iter().enumerate() {
        if b.abs() > peak {
            peak = b.abs();
            best = i;
        }
    }
    best
}

// ---------------------------------------------------------------------------
// 1. Superelevation + elevation grade compose through the importer.
// ---------------------------------------------------------------------------

#[test]
fn grade_and_bank_compose_to_elev_plus_t_sin_phi() {
    let net = load_file(CLIMBING).expect("climbing_banked loads");
    assert_eq!(net.driving_lanes().count(), 2, "one lane each side");

    let sin_phi = 0.2_f32.sin();
    // Single lane each side: left t = +2.0, right t = -2.0.
    let mut left = None;
    let mut right = None;
    for lane in net.driving_lanes() {
        match lane.direction {
            Direction::Backward => left = Some(lane),
            Direction::Forward => right = Some(lane),
            Direction::Both => panic!("no two-way lanes here"),
        }
    }
    let left = left.expect("a left lane");
    let right = right.expect("a right lane");

    // Vertex index 20 = road s = 40 (SAMPLE_STEP 2.0). Grade elev(40) = 2.0.
    // Height is exact at a vertex: no arc-length reparam involved.
    let i = 20usize;
    let want_grade = 0.05_f32 * 40.0;
    let want_off = 2.0 * sin_phi; // t = +/-2.0
    let ly = left.center.points()[i].z;
    let ry = right.center.points()[i].z;
    assert!(
        (ly - (want_grade + want_off)).abs() < 2e-3,
        "left height {ly} != grade {want_grade} + t*sin phi {want_off}"
    );
    assert!(
        (ry - (want_grade - want_off)).abs() < 2e-3,
        "right height {ry} != grade {want_grade} - t*sin phi {want_off}"
    );
    // The two decompose cleanly: their mean is the pure grade (the reference-line
    // pivot sits at elev), their half-difference is the pure bank term.
    assert!(
        ((ly + ry) / 2.0 - want_grade).abs() < 2e-3,
        "mean height {} should be the grade {want_grade}",
        (ly + ry) / 2.0
    );
    assert!(
        ((ly - ry) / 2.0 - want_off).abs() < 2e-3,
        "half-diff {} should be t*sin phi {want_off}",
        (ly - ry) / 2.0
    );
    // And the bank angle itself survived import.
    assert!(
        (left.bank_at(80.0) - 0.2).abs() < 1e-4,
        "left bank {}",
        left.bank_at(80.0)
    );
    assert!(
        (right.bank_at(80.0) - 0.2).abs() < 1e-4,
        "right bank {}",
        right.bank_at(80.0)
    );
}

// ---------------------------------------------------------------------------
// 2. Right-hand banked arc: opposite-sign cant raises the opposite edge.
// ---------------------------------------------------------------------------

#[test]
fn right_hand_arc_raises_the_opposite_edge_from_the_left_sweeper() {
    let net = load_file(RIGHT_SWEEPER).expect("right_banked_sweeper loads");
    assert_eq!(net.driving_lanes().count(), 4, "two lanes each direction");

    // Bank profile still peaks near 0.2 in magnitude, flat at the ends, but the
    // sign is negative (a right turn rolled the other way).
    for lane in net.driving_lanes() {
        assert!(
            lane.bank_at(0.0).abs() < 1e-3,
            "lane {:?} starts flat",
            lane.id
        );
        let apex = lane.bank[peak_bank_vertex(lane)];
        assert!(
            (apex + 0.2).abs() < 0.02,
            "lane {:?} apex bank {apex} should be ~-0.2",
            lane.id
        );
    }

    // Peak height per lane centerline.
    let peak_z = |dir: Direction| {
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
    let min_z = |dir: Direction| {
        net.driving_lanes()
            .filter(|l| l.direction == dir)
            .map(|l| {
                l.center
                    .points()
                    .iter()
                    .map(|p| p.z)
                    .fold(f32::MAX, f32::min)
            })
            .collect::<Vec<_>>()
    };

    // Right (Forward) lanes are the raised side here, the mirror of the left
    // sweeper, where the left (Backward) lanes rose.
    let mut right = peak_z(Direction::Forward);
    right.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(right.len(), 2, "two right lanes");
    assert!(
        right[0] > 0.3,
        "the raised right lanes should climb: {right:?}"
    );
    assert!(
        right[1] - right[0] > 0.3,
        "outer right lane {} should ride above inner {}",
        right[1],
        right[0]
    );

    // The left (Backward) lanes dip BELOW the reference line, the opposite edge
    // to the original left sweeper, where they climbed.
    let left = min_z(Direction::Backward);
    assert_eq!(left.len(), 2, "two left lanes");
    assert!(
        left.iter().all(|&y| y < -0.3),
        "left lanes should dip below the reference line, got {left:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. sample_near swept around the whole arc: bank rises 0 -> 0.2 -> 0, up stays
//    a unit, up.z > 0.9 everywhere, no NaN.
// ---------------------------------------------------------------------------

#[test]
fn sample_near_swept_around_the_arc_is_continuous_and_upright() {
    let net = load_file(SWEEPER).expect("banked sweeper loads");
    // Sweep along one lane's own surface points; bank is shared across lanes so
    // any lane's centerline traces the whole 0 -> 0.2 -> 0 profile.
    let lane = net
        .driving_lanes()
        .find(|l| l.direction == Direction::Forward)
        .expect("a forward lane");

    let mut banks = Vec::new();
    for p in lane.center.points() {
        let s = net.sample_near(*p).expect("a sample near a surface point");
        assert!(s.point.is_finite(), "non-finite sample point {:?}", s.point);
        assert!(s.bank.is_finite(), "non-finite bank {}", s.bank);
        assert!(s.up.is_normalized(), "up not unit: {:?}", s.up);
        assert!(
            s.up.z > 0.9,
            "up.z fell to {}, so the surface tipped over",
            s.up.z
        );
        banks.push(s.bank);
    }

    // Flat at both ends (the straights), banked in the middle (the arc).
    assert!(
        banks.first().unwrap().abs() < 0.02,
        "entry not flat: {}",
        banks[0]
    );
    assert!(
        banks.last().unwrap().abs() < 0.02,
        "exit not flat: {}",
        banks.last().unwrap()
    );
    let peak = banks.iter().cloned().fold(0.0f32, |m, b| m.max(b.abs()));
    assert!((peak - 0.2).abs() < 0.02, "peak bank {peak} should be ~0.2");

    // Unimodal enough: the max is interior, and no single step between adjacent
    // stations jumps more than the ramp physically can (ramp is ~0.02/m over 2m
    // steps ~= 0.04, plus slack).
    let (peak_idx, _) = banks
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
        .unwrap();
    assert!(
        peak_idx > 0 && peak_idx < banks.len() - 1,
        "peak at an endpoint: {peak_idx}"
    );
    for w in banks.windows(2) {
        assert!(
            (w[1].abs() - w[0].abs()).abs() < 0.06,
            "bank jumped {} -> {} between adjacent stations",
            w[0],
            w[1]
        );
    }
}

// ---------------------------------------------------------------------------
// 4. The banked mesh outer rib on the real fixture, end-to-end from disk.
// ---------------------------------------------------------------------------

#[test]
fn meshed_outer_rib_rides_above_inner_near_the_apex_on_disk() {
    let net = load_file(SWEEPER).expect("banked sweeper loads");
    // Per lane, mesh it alone (so vertex indices are local), find the peak-bank
    // rib pair, and assert the raised (+t / left) rib rides above the other.
    for lane in net.driving_lanes() {
        let solo = RoadNetwork::new(vec![lane.clone()]);
        let mesh = solo.surface_mesh();
        mesh.validate().expect("a banked lane tessellates");
        let i = peak_bank_vertex(lane);
        let left = mesh.vertices[2 * i].z; // +t rib
        let right = mesh.vertices[2 * i + 1].z; // -t rib
                                                // +bank raises the +t rib; the fixture's arc banks positive.
        assert!(
            left - right > 0.5,
            "lane {:?} apex rib: raised edge {left} should ride above {right}",
            lane.id
        );
        // The rib-pair separation matches 2*half*sin(bank) within tolerance.
        let want = lane.width * lane.bank[i].sin();
        assert!(
            ((left - right) - want).abs() < 0.05,
            "lane {:?} rib gap {} != width*sin(bank) {want}",
            lane.id,
            left - right
        );
    }
}

// ---------------------------------------------------------------------------
// 5. A superelevation record that starts mid-road; earlier stations flat.
// ---------------------------------------------------------------------------

#[test]
fn mid_road_superelevation_is_flat_before_its_start() {
    let net = load_file(MID_ROAD).expect("mid_road_super loads");
    let sin_phi = 0.15_f32.sin();
    for lane in net.driving_lanes() {
        // Before s=30: no active record -> flat.
        assert!(
            lane.bank_at(10.0).abs() < 1e-6,
            "lane {:?} should be flat at s=10",
            lane.id
        );
        assert!(
            lane.bank_at(28.0).abs() < 1e-6,
            "lane {:?} should be flat at s=28",
            lane.id
        );
        // From s=30 on: constant 0.15.
        assert!(
            (lane.bank_at(50.0) - 0.15).abs() < 1e-4,
            "lane {:?} bank at 50 {}",
            lane.id,
            lane.bank_at(50.0)
        );
        // The bank vector is kept (non-empty), not collapsed to the flat sentinel.
        assert!(
            !lane.bank.is_empty(),
            "a partly-banked road keeps its profile"
        );
    }
    // Heights: flat region rides at 0, banked region at +/- 2*sin(0.15).
    for lane in net.driving_lanes() {
        let pts = lane.center.points();
        // s=10 -> vertex 5.
        assert!(
            pts[5].z.abs() < 1e-4,
            "flat-region height {} should be 0",
            pts[5].z
        );
        // s=50 -> vertex 25.
        let want = 2.0 * sin_phi;
        assert!(
            (pts[25].z.abs() - want).abs() < 2e-3,
            "banked-region |height| {} should be {want}",
            pts[25].z.abs()
        );
    }
}

// ---------------------------------------------------------------------------
// 6. Odds and ends a real file invites.
// ---------------------------------------------------------------------------

// Multiple lane sections, each carries the shared road-level bank profile.
const TWO_SECTIONS_BANKED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <header revMajor="1" revMinor="7" name="two_sections" version="1.00"/>
  <road name="two_sections" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.1" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
      <laneSection s="20.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn multiple_lane_sections_each_carry_the_bank() {
    let net = load_str(TWO_SECTIONS_BANKED).expect("import");
    assert_eq!(net.driving_lanes().count(), 2, "one lane per section");
    for lane in net.driving_lanes() {
        assert!(!lane.bank.is_empty(), "section lane lost its bank");
        // Constant 0.1 everywhere on the lane.
        let mid = lane.center.length() / 2.0;
        assert!(
            (lane.bank_at(mid).abs() - 0.1).abs() < 1e-4,
            "section bank {}",
            lane.bank_at(mid)
        );
    }
}

// laneOffset + superelevation: the lateral offset the bank pivots is
// laneOffset + the lane's own stack, so height is (base + t_lane)*sin(phi).
const OFFSET_BANKED: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <header revMajor="1" revMinor="7" name="offset_banked" version="1.00"/>
  <road name="offset_banked" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.2" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneOffset s="0.0" a="2.0" b="0.0" c="0.0" d="0.0"/>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn lane_offset_shifts_the_bank_pivot() {
    let net = load_str(OFFSET_BANKED).expect("import");
    let lane = net.driving_lanes().next().expect("a lane");
    // Right lane t_lane = -sign*(width/2) = -2.0; laneOffset base = +2.0.
    // Total t = 2.0 + (-2.0) = 0.0, so the lane rides exactly on the pivot: its
    // height is ~0 despite the bank. This is the composition laneOffset feeds.
    let z = lane.center.points()[10].z;
    assert!(
        z.abs() < 2e-3,
        "offset lane at the pivot should ride at 0, got {z}"
    );
    // But the surface is still canted (bank angle carried through).
    assert!(
        (lane.bank_at(20.0) - 0.2).abs() < 1e-4,
        "bank {}",
        lane.bank_at(20.0)
    );
}

// A steep bank (0.6 rad ~= 34 deg): must bake finite and faithfully carry the
// angle. Note the surface normal legitimately drops to cos(0.6) ~= 0.825, i.e.
// BELOW the 0.9 "generally up" bar the gentle fixtures pass. That is physics,
// not a bug, so this test asserts the true angle, not up.z > 0.9.
const STEEP_BANK: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <header revMajor="1" revMinor="7" name="steep_bank" version="1.00"/>
  <road name="steep_bank" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.6" b="0.0" c="0.0" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <left><lane id="1" type="driving"><width sOffset="0.0" a="4.0"/></lane></left>
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn a_steep_bank_bakes_finite_and_carries_the_angle() {
    let net = load_str(STEEP_BANK).expect("import");
    let mesh = net.surface_mesh();
    mesh.validate()
        .expect("steep bank still tessellates to a valid trimesh");
    // The angle survived: bank ~= 0.6.
    for lane in net.driving_lanes() {
        assert!(
            (lane.bank_at(20.0).abs() - 0.6).abs() < 1e-4,
            "steep bank {}",
            lane.bank_at(20.0)
        );
        let up = lane.sample_at(20.0).up;
        assert!(up.is_normalized(), "up not unit {up:?}");
        // cos(0.6) ~= 0.8253, so the normal has genuinely leaned past 0.9.
        assert!(
            (up.z - 0.6_f32.cos()).abs() < 1e-3,
            "up.z {} should be cos(0.6)",
            up.z
        );
    }
    // Every mesh normal is still upright (positive z) and finite, just tilted.
    assert!(mesh.normals.iter().all(|n| n.z > 0.5 && n.is_finite()));
    assert!(
        mesh.normals.iter().any(|n| n.z < 0.9),
        "no normal leaned past 0.9, so the steep bank was lost"
    );
}

// A <lateralProfile> that is present but empty: no superelevation records, so
// the road is flat, so bank collapses to the empty sentinel, byte-identical to a
// road with no lateralProfile at all.
const EMPTY_LATERAL: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <header revMajor="1" revMinor="7" name="empty_lateral" version="1.00"/>
  <road name="empty_lateral" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <lateralProfile></lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

const NO_LATERAL: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <header revMajor="1" revMinor="7" name="no_lateral" version="1.00"/>
  <road name="no_lateral" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn empty_lateral_profile_leaves_the_road_flat() {
    let net = load_str(EMPTY_LATERAL).expect("import");
    let lane = net.driving_lanes().next().expect("a lane");
    assert!(
        lane.bank.is_empty(),
        "empty lateralProfile must leave bank the flat sentinel"
    );
    assert!(
        lane.bank_at(20.0).abs() < 1e-9,
        "flat road should have zero bank"
    );
    // Identical to a road that declares no lateralProfile at all, down to
    // the names the map gives itself and its road.
    let bare = load_str(&NO_LATERAL.replace("no_lateral", "empty_lateral")).expect("import");
    assert_eq!(
        net, bare,
        "empty <lateralProfile> must bake identically to none"
    );
}

// A cubic (non-linear) superelevation profile: bank must follow a+b*ds+c*ds^2
// through the importer, not just the linear ramps the other fixtures use.
const CUBIC_BANK: &str = r#"<?xml version="1.0"?>
<OpenDRIVE>
  <header revMajor="1" revMinor="7" name="cubic_bank" version="1.00"/>
  <road name="cubic_bank" length="40.0" id="1" junction="-1">
    <planView>
      <geometry s="0.0" x="0.0" y="0.0" hdg="0.0" length="40.0"><line/></geometry>
    </planView>
    <lateralProfile>
      <superelevation s="0.0" a="0.05" b="0.01" c="0.0001" d="0.0"/>
    </lateralProfile>
    <lanes>
      <laneSection s="0.0">
        <right><lane id="-1" type="driving"><width sOffset="0.0" a="4.0"/></lane></right>
      </laneSection>
    </lanes>
  </road>
</OpenDRIVE>"#;

#[test]
fn cubic_superelevation_follows_the_polynomial() {
    let net = load_str(CUBIC_BANK).expect("import");
    let lane = net.driving_lanes().next().expect("a lane");
    // phi(s) = 0.05 + 0.01*s + 0.0001*s^2. Check a couple of interior stations
    // against the closed form (bank_at interpolates a 2m-spaced sampling, so
    // allow a little slack for the piecewise-linear read of a quadratic).
    for &s in &[8.0_f32, 24.0, 36.0] {
        let want = 0.05 + 0.01 * s + 0.0001 * s * s;
        let got = lane.bank_at(s);
        assert!(
            (got - want).abs() < 4e-3,
            "s={s}: bank {got} != poly {want}"
        );
    }
}
