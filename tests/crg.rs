//! OpenCRG surfaces: the `<CRG>` records, and heights from each mode.
//!
//! The CRG files here are generated planes, `z = A*u + B*v + C`, which a
//! bilinear grid holds exactly. So each mode's expected height is a formula
//! from the ASAM OpenDRIVE 1.9 road surface section.

use opencrg::CrgGrid;
use xodr::{
    load_file, load_str, CrgAlong, CrgMode, CrgPurpose, RoadNetwork, RoadSurface, SurfaceHint,
    SurfaceSample,
};

const A: f64 = 0.01;
const B: f64 = 0.02;
const C: f64 = 0.005;

/// The grid height of [`plane_crg`].
fn plane(u: f64, v: f64) -> f64 {
    A * u + B * v + C
}

/// The elevation of [`plane_crg`]. With no `$ROAD_CRG_MODS` block, OpenCRG
/// shifts a file so its reference line starts at height 0, as the C-API does.
fn elevation(u: f64, v: f64) -> f64 {
    plane(u, v) - C
}

/// A CRG file of [`plane`], `length` metres long and `2 * half` wide, on a
/// straight reference line placed by `header`.
fn plane_crg(length: f64, half: f64, header: &str) -> CrgGrid {
    let (du, dv) = (0.5, 0.5);
    let columns = (2.0 * half / dv) as usize + 1;
    let rows = (length / du) as usize + 1;
    let mut text = format!(
        "$ROAD_CRG\nREFERENCE_LINE_INCREMENT = {du}\nLONG_SECTION_V_RIGHT = {}\n\
         LONG_SECTION_V_INCREMENT = {dv}\n{header}$\n$KD_Definition\n#:LRFI\n",
        -half
    );
    for c in 0..columns {
        text += &format!("D:long section {},m\n", c + 1);
    }
    text += "$\n$$$$\n";
    // Each record is lines of up to 8 values, 10 characters each.
    for r in 0..rows {
        for c in 0..columns {
            let z = plane(r as f64 * du, -half + c as f64 * dv);
            text += &format!("{z:10.6}");
            if c % 8 == 7 || c == columns - 1 {
                text += "\n";
            }
        }
    }
    CrgGrid::from_bytes(text.as_bytes()).expect("the generated CRG loads")
}

/// One road with a lane either side, `3.5 m` wide.
fn road(length: f64, geometry: &str, profiles: &str, surface: &str) -> String {
    let lane = |id: i32| {
        format!(
            r#"<lane id="{id}" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>"#
        )
    };
    format!(
        r#"<OpenDRIVE>
  <road id="1" length="{length}" junction="-1">
    <planView><geometry s="0" x="10" y="20" hdg="0.4" length="{length}">{geometry}</geometry></planView>
    {profiles}
    <lanes><laneSection s="0">
      <left>{}</left><center><lane id="0" type="none"/></center><right>{}</right>
    </laneSection></lanes>
    <surface>{surface}</surface>
  </road>
</OpenDRIVE>"#,
        lane(1),
        lane(-1)
    )
}

const LINE: &str = "<line/>";
const ARC: &str = r#"<arc curvature="0.01"/>"#;
const GRADE: &str =
    r#"<elevationProfile><elevation s="0" a="2" b="0.03" c="0" d="0"/></elevationProfile>"#;
const BANK: &str =
    r#"<lateralProfile><superelevation s="0" a="0.05" b="0" c="0" d="0"/></lateralProfile>"#;

/// Where road `(s, t)` is, on the geometry of [`road`].
fn xy(geometry: &str, s: f64, t: f64, bank: f64) -> (f64, f64) {
    let (x0, y0, h0) = (10.0, 20.0, 0.4_f64);
    let (x, y, h) = if geometry == ARC {
        let k = 0.01;
        let h = h0 + k * s;
        (
            x0 + (h.sin() - h0.sin()) / k,
            y0 - (h.cos() - h0.cos()) / k,
            h,
        )
    } else {
        (x0 + s * h0.cos(), y0 + s * h0.sin(), h0)
    };
    let t = t * bank.cos();
    (x - t * h.sin(), y + t * h.cos())
}

fn surface<'a>(net: &'a RoadNetwork, mesh: &'a xodr::Mesh, grid: CrgGrid) -> RoadSurface<'a> {
    let mut grid = Some(grid);
    RoadSurface::new(net, mesh, move |_| grid.take())
}

fn at(surface: &RoadSurface, (x, y): (f64, f64)) -> SurfaceSample {
    surface
        .sample(x, y, &mut SurfaceHint::default())
        .expect("on the road")
}

fn assert_near(got: f64, want: f64, tolerance: f64) {
    assert!(
        (got - want).abs() <= tolerance,
        "got {got}, want {want} within {tolerance}"
    );
}

/// The normal agrees with the slope of the heights around it.
fn assert_normal_follows_heights(surface: &RoadSurface, (x, y): (f64, f64)) {
    let h = 1e-3;
    let z = |dx: f64, dy: f64| at(surface, (x + dx, y + dy)).z;
    let gx = (z(h, 0.0) - z(-h, 0.0)) / (2.0 * h);
    let gy = (z(0.0, h) - z(0.0, -h)) / (2.0 * h);
    let length = (gx * gx + gy * gy + 1.0).sqrt();
    let want = [-gx / length, -gy / length, 1.0 / length];
    let got = at(surface, (x, y)).normal;
    for c in 0..3 {
        assert_near(got[c], want[c], 1e-6);
    }
}

#[test]
fn a_road_crg_is_read_into_a_record() {
    let xml = road(
        40.0,
        LINE,
        "",
        r#"<CRG file="bumps.crg" sStart="5" sEnd="30" orientation="opposite" mode="attached"
             sOffset="1" tOffset="-0.5" zOffset="0.1" zScale="2"/>
           <CRG file="grip.crg" sStart="0" sEnd="40" orientation="same" mode="attached0"
             purpose="friction" zOffset="3" zScale="4"/>
           <CRG file="nomode.crg" sStart="0" sEnd="40" orientation="same"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let crg = net.crg_surfaces();
    assert_eq!(crg.len(), 2, "the record with no mode is skipped");

    assert_eq!(crg[0].file, "bumps.crg");
    assert_eq!(crg[0].purpose, CrgPurpose::Elevation);
    assert_eq!(
        crg[0].mode,
        CrgMode::Attached(CrgAlong {
            s_offset: 1.0,
            t_offset: -0.5,
            opposite: true
        })
    );
    assert_eq!((crg[0].z_offset, crg[0].z_scale), (0.1, 2.0));
    assert_eq!(crg[0].lanes.len(), 2);

    assert_eq!(crg[1].purpose, CrgPurpose::Friction);
    assert_eq!((crg[1].z_offset, crg[1].z_scale), (0.0, 1.0));
}

#[test]
fn a_junction_crg_covers_the_roads_in_the_junction() {
    let xml = road(40.0, LINE, "", "")
        .replace(r#"junction="-1""#, r#"junction="7""#)
        .replace(
            "</OpenDRIVE>",
            r#"<junction id="7">
                 <surface>
                   <CRG file="square.crg" mode="global" xOffset="1" yOffset="2" hOffset="0.3"/>
                   <CRG file="along.crg" mode="attached" sStart="0" sEnd="1"/>
                 </surface>
               </junction></OpenDRIVE>"#,
        );
    let net = load_str(&xml).unwrap();
    let crg = net.crg_surfaces();
    assert_eq!(
        crg.len(),
        1,
        "a junction has no reference line to attach to"
    );
    assert_eq!(crg[0].lanes.len(), 2);
    let CrgMode::Global { origin } = crg[0].mode else {
        panic!("{:?}", crg[0].mode);
    };
    assert_eq!((origin.x, origin.y, origin.heading), (1.0, 2.0, 0.3));
}

#[test]
fn attached_adds_the_grid_to_the_road() {
    for (geometry, profiles, bank) in [
        (LINE, GRADE.to_string(), 0.0),
        (ARC, format!("{GRADE}{BANK}"), 0.05),
    ] {
        let xml = road(
            60.0,
            geometry,
            &profiles,
            r#"<CRG file="p.crg" sStart="10" sEnd="50" orientation="same" mode="attached"
                 sOffset="8" tOffset="0.5" zOffset="0.1" zScale="2"/>"#,
        );
        let net = load_str(&xml).unwrap();
        let mesh = net.surface_mesh();
        let surface = surface(&net, &mesh, plane_crg(45.0, 4.0, ""));
        for (s, t) in [(12.0, 0.0), (25.3, 2.9), (48.0, -3.1)] {
            let got = at(&surface, xy(geometry, s, t, bank));
            let road = 2.0 + 0.03 * s + t * bank.sin();
            let bump = plane(s - 8.0, t - 0.5);
            assert_near(got.z, road + 2.0 * bump + 0.1, 1e-6);
            assert_near(got.crg_height.unwrap(), 2.0 * bump, 1e-6);
            assert_normal_follows_heights(&surface, xy(geometry, s, t, bank));
        }
    }
}

#[test]
fn opposite_runs_the_file_against_s() {
    let xml = road(
        60.0,
        LINE,
        "",
        r#"<CRG file="p.crg" sStart="0" sEnd="40" orientation="opposite" mode="attached"
             sOffset="45" tOffset="0.5"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let surface = surface(&net, &mesh, plane_crg(45.0, 4.0, ""));
    let (s, t) = (20.0, 1.5);
    let got = at(&surface, xy(LINE, s, t, 0.0));
    assert_near(got.z, plane(45.0 - s, 0.5 - t), 1e-5);
    assert_normal_follows_heights(&surface, xy(LINE, s, t, 0.0));
}

#[test]
fn attached0_ignores_the_road_height() {
    let xml = road(
        60.0,
        ARC,
        &format!("{GRADE}{BANK}"),
        r#"<CRG file="p.crg" sStart="0" sEnd="40" orientation="same" mode="attached0"
             zOffset="-1" zScale="3"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let surface = surface(&net, &mesh, plane_crg(45.0, 4.0, ""));
    let (s, t) = (30.0, -2.0);
    let got = at(&surface, xy(ARC, s, t, 0.05));
    assert_near(got.z, 3.0 * elevation(s, t) - 1.0, 1e-6);
    assert_normal_follows_heights(&surface, xy(ARC, s, t, 0.05));
}

#[test]
fn genuine_starts_the_files_reference_line_on_the_road() {
    let xml = road(
        60.0,
        LINE,
        GRADE,
        r#"<CRG file="p.crg" sStart="0" sEnd="60" mode="genuine"
             sOffset="5" tOffset="-1" hOffset="0.1" zOffset="0.2" zScale="2"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let header = "REFERENCE_LINE_START_X = 100\nREFERENCE_LINE_START_Y = -50\n\
                  REFERENCE_LINE_START_PHI = 1.2\n";
    let surface = surface(&net, &mesh, plane_crg(45.0, 4.0, header));

    let (sx, sy) = xy(LINE, 5.0, -1.0, 0.0);
    let heading = 0.4_f64 + 0.1;
    let (u, v) = (20.0, 1.0);
    let point = (
        sx + u * heading.cos() - v * heading.sin(),
        sy + u * heading.sin() + v * heading.cos(),
    );
    let got = at(&surface, point);
    assert_near(got.z, 2.0 * elevation(u, v) + 0.2, 1e-5);
    assert_normal_follows_heights(&surface, point);
}

#[test]
fn global_moves_the_file_by_its_offsets() {
    let xml = road(
        60.0,
        LINE,
        GRADE,
        r#"<CRG file="p.crg" sStart="0" sEnd="60" mode="global"
             xOffset="12" yOffset="18" hOffset="0.5" zOffset="-0.3"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let surface = surface(&net, &mesh, plane_crg(45.0, 4.0, ""));

    let (u, v) = (20.0, 2.0);
    let heading = 0.5_f64;
    let point = (
        12.0 + u * heading.cos() - v * heading.sin(),
        18.0 + u * heading.sin() + v * heading.cos(),
    );
    let got = at(&surface, point);
    assert_near(got.z, elevation(u, v) - 0.3, 1e-5);
    assert_normal_follows_heights(&surface, point);
}

/// Friction is the file's value, without the shift that starts heights at 0.
#[test]
fn friction_comes_with_the_height_under_it() {
    let xml = road(
        60.0,
        LINE,
        GRADE,
        r#"<CRG file="p.crg" sStart="10" sEnd="40" orientation="same" mode="attached0"
             purpose="friction"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let surface = surface(&net, &mesh, plane_crg(45.0, 4.0, ""));

    let got = at(&surface, xy(LINE, 20.0, 1.0, 0.0));
    assert_near(got.friction.unwrap(), plane(20.0, 1.0), 1e-6);
    assert_eq!(got.crg_height, None);
    assert_near(got.z, 2.0 + 0.03 * 20.0, 1e-3);
}

/// The spec measures a lane's height from the road including its surface.
/// An `attached` CRG adds its grid to the road without it, so over a raised
/// lane it answers at road level, and the mesh beyond it at the lane's.
#[test]
fn an_attached_crg_over_a_raised_lane_answers_at_road_level() {
    let flat_lane = r#"a="3.5" b="0" c="0" d="0"/></lane>"#;
    let xml = road(
        60.0,
        LINE,
        GRADE,
        r#"<CRG file="p.crg" sStart="10" sEnd="40" orientation="same" mode="attached"/>"#,
    );
    let right = xml.rfind(flat_lane).unwrap() + flat_lane.len() - "</lane>".len();
    let xml = format!(
        r#"{}<height sOffset="0" inner="0.1" outer="0.1"/>{}"#,
        &xml[..right],
        &xml[right..]
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let surface = surface(&net, &mesh, plane_crg(45.0, 4.0, ""));
    let covered = at(&surface, xy(LINE, 20.0, -1.75, 0.0));
    assert_near(covered.z, 2.0 + 0.03 * 20.0 + plane(20.0, -1.75), 1e-6);
    let beyond = at(&surface, xy(LINE, 50.0, -1.75, 0.0));
    assert_eq!(beyond.crg_height, None);
    assert_near(beyond.z, 2.0 + 0.03 * 50.0 + 0.1, 1e-3);
}

#[test]
fn off_the_stretch_or_without_the_file_the_mesh_answers() {
    let xml = road(
        60.0,
        LINE,
        GRADE,
        r#"<CRG file="p.crg" sStart="10" sEnd="40" orientation="same" mode="attached"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let sampler = mesh.sampler();

    let with_file = surface(&net, &mesh, plane_crg(45.0, 4.0, ""));
    let without = RoadSurface::new(&net, &mesh, |_| None);
    for (s, covered) in [(5.0, false), (20.0, true), (45.0, false)] {
        let point = xy(LINE, s, 1.0, 0.0);
        let (z, _) = sampler.height_at(point.0 as f32, point.1 as f32).unwrap();
        assert_eq!(
            at(&with_file, point).crg_height.is_some(),
            covered,
            "s = {s}"
        );
        assert_eq!(at(&without, point).crg_height, None);
        assert_eq!(at(&without, point).z, f64::from(z));
    }
    assert_eq!(
        with_file.sample(0.0, 0.0, &mut SurfaceHint::default()),
        None
    );
}

#[test]
fn each_file_loads_once() {
    let xml = road(
        60.0,
        LINE,
        "",
        r#"<CRG file="p.crg" sStart="0" sEnd="30" orientation="same" mode="attached"/>
           <CRG file="p.crg" sStart="30" sEnd="60" orientation="same" mode="attached"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let mut loads = Vec::new();
    let surface = RoadSurface::new(&net, &mesh, |file| {
        loads.push(file.to_string());
        Some(plane_crg(80.0, 4.0, ""))
    });
    assert_eq!(loads, ["p.crg"]);
    let got = at(&surface, xy(LINE, 50.0, 0.0, 0.0));
    assert_near(got.z, plane(50.0, 0.0), 1e-5);
}

#[test]
fn a_hint_follows_a_moving_point_to_the_same_answers() {
    let xml = road(
        200.0,
        ARC,
        &format!("{GRADE}{BANK}"),
        r#"<CRG file="p.crg" sStart="0" sEnd="200" orientation="same" mode="attached"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let mesh = net.surface_mesh();
    let surface = surface(&net, &mesh, plane_crg(200.0, 4.0, ""));
    let mut hint = SurfaceHint::default();
    for step in 0..400 {
        let s = 0.5 * f64::from(step) + 0.1;
        let point = xy(ARC, s, 1.0, 0.05);
        let moving = surface.sample(point.0, point.1, &mut hint).unwrap();
        assert_eq!(moving, at(&surface, point), "s = {s}");
    }
}

#[cfg(feature = "serde")]
#[test]
fn a_network_keeps_its_crg_records_through_serde() {
    let xml = road(
        60.0,
        ARC,
        BANK,
        r#"<CRG file="p.crg" sStart="0" sEnd="40" orientation="same" mode="attached"/>"#,
    );
    let net = load_str(&xml).unwrap();
    let json = serde_json::to_string(&net).unwrap();
    let back: RoadNetwork = serde_json::from_str(&json).unwrap();
    assert_eq!(back.crg_surfaces(), net.crg_surfaces());
}

/// `crg.xodr` lays the same file on four roads, one per mode. Each puts the
/// crest of its speed bump, 9.8 m into the file, where the mode says.
#[test]
fn the_fixture_map_lays_its_bump_on_every_road() {
    let net = load_file("tests/data/crg.xodr").unwrap();
    let mesh = net.surface_mesh();
    let surface = RoadSurface::new(&net, &mesh, |file| {
        CrgGrid::from_path(format!("tests/data/{file}")).ok()
    });
    let crest = 0.07;
    let (s, radius): (f64, f64) = (45.0 - 9.8, 1.0 / 0.0166667);
    let arc = (
        100.0 + radius * (s / radius).sin(),
        radius * (1.0 - (s / radius).cos()),
    );
    for (road, point) in [
        (1, (19.8, 0.0)),
        (2, arc),
        (3, (19.8, -30.0)),
        (4, (19.8, -60.0)),
    ] {
        let got = at(&surface, point);
        assert_near(got.crg_height.expect("on the bump"), crest, 1e-6);
        if road != 1 {
            assert_near(got.z, crest, 1e-6);
        }
    }
    let wet = at(&surface, (60.0, -2.0));
    assert!(wet.friction.unwrap() < 0.5, "{wet:?}");
    let dry = at(&surface, (50.0, 2.0));
    assert_near(dry.friction.unwrap(), 0.9, 1e-3);
}

#[test]
fn road_crg_records_past_the_budget_are_dropped_before_they_are_sampled() {
    let records = r#"<CRG file="a.crg" sStart="0" sEnd="1e9" mode="attached"/>"#.repeat(200);
    let xodr = format!(
        r#"<OpenDRIVE><header/><road id="1" length="100000" junction="-1"><planView>
        <geometry s="0" x="0" y="0" hdg="0" length="100000"><line/></geometry></planView>
        <lanes><laneSection s="0"><center><lane id="0" type="none"/></center><right>
        <lane id="-1" type="driving"><width sOffset="0" a="3.5" b="0" c="0" d="0"/></lane>
        </right></laneSection></lanes><surface>{records}</surface></road></OpenDRIVE>"#
    );
    let start = std::time::Instant::now();
    let net = xodr::load_str(&xodr).unwrap();
    assert_eq!(net.lanes().len(), 1);
    assert!(start.elapsed().as_secs() < 30, "took {:?}", start.elapsed());
}
