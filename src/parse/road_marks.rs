//! Road marks, placed along the lane borders of each baked road.

use super::{
    active, attr_f32, attr_f64, child, cubics_in, side_borders, BakedRoad, Cubic,
    RoadMarkProvenance, SectionDef, Warning, MAX_REPEAT_INSTANCES,
};
use crate::coords::Point;
use crate::road::{LaneGeom, MarkSpan, Road, RoadSection};
use crate::{
    LaneChange, LaneId, LinePattern, LineRule, RoadMark, RoadMarkId, RoadMarkLine, RoadMarkType,
    RoadMarkWeight,
};

/// The width of a standard and a bold line where the map gives none, in
/// metres, as libOpenDRIVE has them.
const STANDARD_WIDTH: f64 = 0.12;
const BOLD_WIDTH: f64 = 0.25;

/// The dashes of a `broken` mark the map describes by its type alone, in
/// metres, as esmini draws them.
const BROKEN_LENGTH: f64 = 4.0;
const BROKEN_SPACE: f64 = 8.0;

/// A `<roadMark>` as the file gives it, before it is placed.
#[derive(Clone)]
pub(super) struct MarkDef {
    /// Where it starts, in metres from the start of its lane section.
    pub(super) s_offset: f64,
    kind: RoadMarkType,
    weight: RoadMarkWeight,
    color: String,
    /// Its `material`, and its `<type>`'s `name`. Empty if it has none.
    material: String,
    type_name: String,
    /// Its width, if the map gives one above 0.
    width: Option<f64>,
    height: Option<f64>,
    lane_change: LaneChange,
    /// Its `<type>`'s width, if the map gives one above 0.
    type_width: Option<f64>,
    /// Its `<type>`'s `<line>`s, then its `<explicit>` ones.
    lines: Vec<LineDef>,
    /// Its `<sway>`s, each starting `ds` metres after the mark does.
    sways: Vec<Cubic>,
}

/// A line of a road mark, before it is placed: one of its `<type><line>`s
/// or `<explicit><line>`s, or one of the lines that stand in for its type.
#[derive(Clone)]
struct LineDef {
    pattern: LinePattern,
    s_offset: f64,
    t_offset: f64,
    rule: LineRule,
    /// Its width and colour, if it has its own.
    width: Option<f64>,
    color: Option<String>,
}

impl LineDef {
    /// A `<type><line>`, taking `type_width` if it gives no width of its
    /// own. The caller passes `type_width` only for a type of one line, as
    /// the type's width spans all its lines and the spaces between them.
    /// One with a `length` and `space` of 0 is continuous.
    fn repeating(line: roxmltree::Node, type_width: Option<f64>) -> Self {
        let length = attr_f32(line, "length").map_or(0.0, f64::from);
        let space = attr_f32(line, "space").map_or(0.0, f64::from);
        let pattern = if length == 0.0 && space == 0.0 {
            LinePattern::Continuous
        } else {
            LinePattern::Dashed {
                length: length as f32,
                space: space as f32,
            }
        };
        let mut def = Self::parse(line, pattern);
        def.width = def.width.or(type_width);
        def
    }

    /// An `<explicit><line>`, painted once.
    fn explicit(line: roxmltree::Node) -> Self {
        let length = attr_f32(line, "length").map_or(0.0, f64::from) as f32;
        Self::parse(line, LinePattern::Single { length })
    }

    /// What either kind of `<line>` gives besides its pattern. A missing
    /// `rule` is `none`.
    fn parse(line: roxmltree::Node, pattern: LinePattern) -> Self {
        Self {
            pattern,
            s_offset: attr_f32(line, "sOffset").map_or(0.0, f64::from).max(0.0),
            t_offset: attr_f32(line, "tOffset").map_or(0.0, f64::from),
            rule: match line.attribute("rule") {
                None | Some("none") => LineRule::None,
                Some("no passing") => LineRule::NoPassing,
                Some("caution") => LineRule::Caution,
                Some(_) => LineRule::Unknown,
            },
            width: attr_f32(line, "width").map(f64::from).filter(|w| *w > 0.0),
            color: line.attribute("color").map(str::to_string),
        }
    }

    /// A line standing in for a type, `t_offset` from the border.
    fn stand_in(t_offset: f64, pattern: LinePattern) -> Self {
        Self {
            pattern,
            s_offset: 0.0,
            t_offset,
            rule: LineRule::None,
            width: None,
            color: None,
        }
    }
}

/// The most dashes the crate paints across one load. Real maps paint a few
/// tens of thousands. A broken file can ask for millions from a few bytes,
/// with dashes a fraction of a millimetre long.
const MAX_DASHES: f64 = 1_000_000.0;

/// The road marks baked so far, the provenance of each, in step, each line
/// left unpainted, and how many more dashes the load may paint.
pub(super) struct RoadMarks {
    pub baked: Vec<RoadMark>,
    pub provenance: Vec<RoadMarkProvenance>,
    pub warnings: Vec<Warning>,
    dashes_left: f64,
}

impl Default for RoadMarks {
    fn default() -> Self {
        Self {
            baked: Vec::new(),
            provenance: Vec::new(),
            warnings: Vec::new(),
            dashes_left: MAX_DASHES,
        }
    }
}

/// The `<roadMark>`s of one `<lane>`, in order along it.
///
/// The spec says they come in ascending `sOffset`. Ones out of order are
/// sorted, as lane sections are, rather than dropped. A missing `sOffset` is
/// 0, a missing `type` is `none` and a missing `color` is `standard`, though
/// the spec requires all three. A negative `sOffset` on a mark or a line is
/// 0, as libOpenDRIVE and esmini read it.
pub(super) fn parse<'a>(lane: roxmltree::Node<'a, 'a>) -> Vec<MarkDef> {
    let mut marks: Vec<MarkDef> = lane
        .children()
        .filter(|n| n.has_tag_name("roadMark"))
        .map(|m| {
            let kind = child(m, "type");
            let type_width = kind
                .and_then(|t| attr_f32(t, "width"))
                .map(f64::from)
                .filter(|w| *w > 0.0);
            let lines_of = |parent: Option<roxmltree::Node<'a, 'a>>| {
                parent
                    .into_iter()
                    .flat_map(|n| n.children())
                    .filter(|n| n.has_tag_name("line"))
            };
            let single = lines_of(kind).count() == 1;
            MarkDef {
                s_offset: attr_f64(m, "sOffset").unwrap_or(0.0).max(0.0),
                kind: mark_type(m.attribute("type")),
                weight: match m.attribute("weight") {
                    Some("bold") => RoadMarkWeight::Bold,
                    _ => RoadMarkWeight::Standard,
                },
                color: m.attribute("color").unwrap_or("standard").to_string(),
                material: m.attribute("material").unwrap_or_default().to_string(),
                type_name: kind
                    .and_then(|t| t.attribute("name"))
                    .unwrap_or_default()
                    .to_string(),
                width: attr_f32(m, "width").map(f64::from).filter(|w| *w > 0.0),
                height: attr_f32(m, "height").map(f64::from),
                lane_change: lane_change(m.attribute("laneChange")),
                type_width,
                lines: lines_of(kind)
                    .map(|l| LineDef::repeating(l, type_width.filter(|_| single)))
                    .chain(lines_of(child(m, "explicit")).map(LineDef::explicit))
                    .collect(),
                sways: cubics_in(m, "sway", "ds"),
            }
        })
        .collect();
    marks.sort_by(|a, b| a.s_offset.total_cmp(&b.s_offset));
    marks
}

/// Bake the road marks of every lane section of every road in `roads`.
pub(super) fn place(roads: &mut [(roxmltree::Node, BakedRoad)]) -> RoadMarks {
    let mut out = RoadMarks::default();
    for (_, road) in roads.iter_mut() {
        let mut spans = Vec::new();
        for (section, def) in road.sections() {
            spans.push(place_section(&road.road, section, def, &mut out));
        }
        for (section, marks) in road.road.sections.iter_mut().zip(spans) {
            section.marks = marks;
        }
    }
    out
}

/// One lane border of a section, and the marks along it.
struct Border<'a> {
    /// The `<lane id>` whose marks these are, 0 for the center lane.
    od_lane_id: i32,
    marks: &'a [MarkDef],
    /// The lanes between the center and the border, and which way they
    /// stack: 1.0 toward +t, -1.0 toward -t.
    inner: &'a [LaneGeom],
    sign: f64,
    /// Which way the inside of the road is, for ordering double lines: -t on
    /// the left side, and +t on the right side and the center lane, whose
    /// lines go left to right.
    inside: f64,
    left: Option<LaneId>,
    right: Option<LaneId>,
}

impl Border<'_> {
    /// The border's `t` at road station `s`.
    fn t(&self, road: &Road, section: &RoadSection, s: f64) -> f64 {
        let base = road.base_of(section.layer, s);
        side_borders(self.inner, self.sign, base, s - section.start)
            .last()
            .map_or(base, |b| b.outer)
    }
}

/// Bake every mark along the borders of one section: the center lane's, then
/// each left lane's outer border, then each right lane's.
/// Bake one section's marks into `out`, and return the stretch each runs
/// along.
fn place_section(
    road: &Road,
    section: &RoadSection,
    def: &SectionDef,
    out: &mut RoadMarks,
) -> Vec<MarkSpan> {
    let mut spans = Vec::new();
    let (left, right) = (&section.left, &section.right);
    let baked = |lane: Option<&LaneGeom>| {
        let lane = lane?;
        section
            .lanes
            .iter()
            .find(|&&(od_id, _)| od_id == lane.id)
            .map(|&(_, id)| id)
    };
    let mut borders = vec![Border {
        od_lane_id: 0,
        marks: &def.center,
        inner: &[],
        sign: 0.0,
        inside: 1.0,
        left: baked(left.first()),
        right: baked(right.first()),
    }];
    for (i, (lane, lane_def)) in left.iter().zip(&def.left).enumerate() {
        borders.push(Border {
            od_lane_id: lane.id,
            marks: &lane_def.marks,
            inner: &left[..=i],
            sign: 1.0,
            inside: -1.0,
            left: baked(left.get(i + 1)),
            right: baked(Some(lane)),
        });
    }
    for (i, (lane, lane_def)) in right.iter().zip(&def.right).enumerate() {
        borders.push(Border {
            od_lane_id: lane.id,
            marks: &lane_def.marks,
            inner: &right[..=i],
            sign: -1.0,
            inside: 1.0,
            left: baked(Some(lane)),
            right: baked(right.get(i + 1)),
        });
    }
    for border in &borders {
        for (k, mark) in border.marks.iter().enumerate() {
            let start = section.start + mark.s_offset;
            let end = border
                .marks
                .get(k + 1)
                .map_or(section.end, |next| section.start + next.s_offset)
                .min(section.end);
            if end - start < 1e-6 {
                continue;
            }
            let id = RoadMarkId(out.baked.len());
            let baked = bake(road, section, border, mark, id, (start, end), out);
            out.baked.push(baked);
            out.provenance.push(RoadMarkProvenance {
                road_mark: id,
                road_id: road.od_id.clone(),
                section: section.index,
                od_lane_id: border.od_lane_id,
                s: start,
                length: end - start,
                material: mark.material.clone(),
                type_name: mark.type_name.clone(),
            });
            spans.push(MarkSpan {
                od_lane_id: border.od_lane_id,
                start,
                end,
                mark: id,
            });
        }
    }
    spans
}

/// One mark along `border` over the stretch `[start, end]` of road. A line
/// with more dashes than `out` has left, or whose paint lands out of range,
/// paints nothing, with a warning.
fn bake(
    road: &Road,
    section: &RoadSection,
    border: &Border,
    mark: &MarkDef,
    id: RoadMarkId,
    (start, end): (f64, f64),
    out: &mut RoadMarks,
) -> RoadMark {
    let width = mark.width.or(mark.type_width).unwrap_or(match mark.weight {
        RoadMarkWeight::Standard => STANDARD_WIDTH,
        RoadMarkWeight::Bold => BOLD_WIDTH,
    });
    let stand_ins;
    let defs = if mark.lines.is_empty() || mark.kind == RoadMarkType::None {
        stand_ins = stand_in_lines(mark.kind, width * border.inside);
        &stand_ins
    } else {
        &mark.lines
    };
    let sway = |s: f64| active(&mark.sways, s - start).map_or(0.0, |c| c.eval(s - start));
    let mut lines = Vec::new();
    for line in defs {
        let width = line.width.unwrap_or(width);
        let from = start + line.s_offset;
        let stretches = match painted(line.pattern, from, (start, end), out.dashes_left) {
            Some(stretches) => {
                out.dashes_left -= stretches.len() as f64;
                stretches
            }
            None => {
                out.warnings.push(Warning::RoadMarkLineDropped {
                    road_id: road.od_id.clone(),
                    s: start,
                    lane: border.od_lane_id,
                });
                Vec::new()
            }
        };
        let mut pieces = paint(
            road,
            section,
            border.od_lane_id,
            |s| border.t(road, section, s) + line.t_offset + sway(s),
            width / 2.0,
            &stretches,
        );
        if !pieces
            .iter()
            .flatten()
            .all(|p| p.to_array().iter().all(|c| c.is_finite()))
        {
            pieces.clear();
            out.warnings.push(Warning::RoadMarkLineDropped {
                road_id: road.od_id.clone(),
                s: start,
                lane: border.od_lane_id,
            });
        }
        lines.push(RoadMarkLine {
            color: line.color.clone().unwrap_or_else(|| mark.color.clone()),
            width: width as f32,
            t_offset: line.t_offset as f32,
            s_offset: line.s_offset as f32,
            pattern: line.pattern,
            rule: line.rule,
            pieces,
        });
    }
    RoadMark {
        id,
        kind: mark.kind,
        weight: mark.weight,
        color: mark.color.clone(),
        width: width as f32,
        height: mark.height.map(|h| h as f32),
        lane_change: mark.lane_change,
        left: border.left,
        right: border.right,
        lines,
    }
}

/// The lines esmini draws for a mark of type `kind` with no lines of its
/// own. A double type's first line is `inside` from the border, and its
/// second as far the other way.
fn stand_in_lines(kind: RoadMarkType, inside: f64) -> Vec<LineDef> {
    let solid = LinePattern::Continuous;
    let broken = LinePattern::Dashed {
        length: BROKEN_LENGTH as f32,
        space: BROKEN_SPACE as f32,
    };
    let double = |first, second| {
        vec![
            LineDef::stand_in(inside, first),
            LineDef::stand_in(-inside, second),
        ]
    };
    match kind {
        RoadMarkType::Solid => vec![LineDef::stand_in(0.0, solid)],
        RoadMarkType::Broken => vec![LineDef::stand_in(0.0, broken)],
        RoadMarkType::SolidSolid => double(solid, solid),
        RoadMarkType::SolidBroken => double(solid, broken),
        RoadMarkType::BrokenSolid => double(broken, solid),
        RoadMarkType::BrokenBroken => double(broken, broken),
        _ => Vec::new(),
    }
}

/// The stretches of road a line with `pattern` paints within its mark's
/// `[start, end]`, starting at `from`. Empty for dashes of no length. `None`
/// for more than [`MAX_REPEAT_INSTANCES`] dashes, or more than `left`.
fn painted(
    pattern: LinePattern,
    from: f64,
    (start, end): (f64, f64),
    left: f64,
) -> Option<Vec<(f64, f64)>> {
    let once = |to: f64| {
        let stretch: Vec<_> = [(from.max(start), to.min(end))]
            .into_iter()
            .filter(|(a, b)| b - a > 1e-6)
            .collect();
        (stretch.len() as f64 <= left).then_some(stretch)
    };
    let (length, space) = match pattern {
        LinePattern::Continuous => return once(end),
        LinePattern::Single { length } => return once(from + f64::from(length)),
        LinePattern::Dashed { length, space } => (length, space),
    };
    let length = f64::from(length);
    let period = length + f64::from(space.max(0.0));
    if length <= 0.0 {
        return Some(Vec::new());
    }
    let dashes = (end - from.min(start)) / period;
    if dashes > MAX_REPEAT_INSTANCES || dashes > left {
        return None;
    }
    let first = ((start - from) / period).floor().max(0.0);
    Some(
        (0..)
            .map(|k| from + (first + k as f64) * period)
            .take_while(|&a| a < end)
            .map(|a| (a.max(start), (a + length).min(end)))
            .filter(|(a, b)| b - a > 1e-6)
            .collect(),
    )
}

/// Quads `half_width` either side of the line at `t(s)`, over each stretch of
/// road in `stretches`. The line's edges are placed on the surface of the
/// lane `od_lane_id` at each of the section's stations, and straight between
/// them, so the paint lies on the facets of the lane mesh. So a mark on a
/// kerb, the outer border of the lane below it, stays on the road.
fn paint(
    road: &Road,
    section: &RoadSection,
    od_lane_id: i32,
    t: impl Fn(f64) -> f64,
    half_width: f64,
    stretches: &[(f64, f64)],
) -> Vec<[Point; 4]> {
    let stations = &section.stations;
    let edges = |i: usize| {
        let (s, t) = (stations[i], t(stations[i]));
        (
            road.lane_surface(section, od_lane_id, s, t - half_width).0,
            road.lane_surface(section, od_lane_id, s, t + half_width).0,
        )
    };
    let at = |s: f64| {
        let i = stations
            .partition_point(|&x| x <= s)
            .clamp(1, stations.len() - 1);
        let ((r0, l0), (r1, l1)) = (edges(i - 1), edges(i));
        let f = ((s - stations[i - 1]) / (stations[i] - stations[i - 1])) as f32;
        (r0.lerp(r1, f), l0.lerp(l1, f))
    };
    let mut pieces = Vec::new();
    for &(from, to) in stretches {
        let mut along = vec![from];
        along.extend(
            stations
                .iter()
                .copied()
                .filter(|&s| s > from + 1e-6 && s < to - 1e-6),
        );
        along.push(to);
        for w in along.windows(2) {
            let ((r0, l0), (r1, l1)) = (at(w[0]), at(w[1]));
            pieces.push([r0, r1, l1, l0]);
        }
    }
    pieces
}

fn mark_type(od_type: Option<&str>) -> RoadMarkType {
    match od_type {
        None | Some("none") => RoadMarkType::None,
        Some("solid") => RoadMarkType::Solid,
        Some("broken") => RoadMarkType::Broken,
        Some("solid solid") => RoadMarkType::SolidSolid,
        Some("solid broken") => RoadMarkType::SolidBroken,
        Some("broken solid") => RoadMarkType::BrokenSolid,
        Some("broken broken") => RoadMarkType::BrokenBroken,
        Some("botts dots") => RoadMarkType::BottsDots,
        Some("grass") => RoadMarkType::Grass,
        Some("curb") => RoadMarkType::Curb,
        Some("edge") => RoadMarkType::Edge,
        Some("custom") => RoadMarkType::Custom,
        Some(_) => RoadMarkType::Unknown,
    }
}

/// A missing `laneChange` is `both`, the spec's default.
fn lane_change(value: Option<&str>) -> LaneChange {
    match value {
        None | Some("both") => LaneChange::Both,
        Some("increase") => LaneChange::Increase,
        Some("decrease") => LaneChange::Decrease,
        Some("none") => LaneChange::None,
        Some(_) => LaneChange::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        load_str_with_provenance, LinePattern, LineRule, RoadMark, RoadMarkType, RoadMarkWeight,
    };

    /// A 20 m road heading +X, or along an arc of `curvature`, with a
    /// section whose `<right>` holds `right`.
    fn road(curvature: f64, right: &str) -> String {
        let shape = if curvature == 0.0 {
            "<line/>".to_string()
        } else {
            format!(r#"<arc curvature="{curvature}"/>"#)
        };
        format!(
            r#"<OpenDRIVE><road id="1" length="20" junction="-1">
              <planView><geometry s="0" x="0" y="0" hdg="0" length="20">{shape}</geometry></planView>
              <lanes><laneSection s="0">
                <center><lane id="0" type="none"/></center>
                <right>{right}</right>
              </laneSection></lanes>
            </road></OpenDRIVE>"#
        )
    }

    /// A driving lane `id` 3.5 m wide carrying `marks`.
    fn lane(id: i32, marks: &str) -> String {
        format!(r#"<lane id="{id}" type="driving"><width sOffset="0" a="3.5"/>{marks}</lane>"#)
    }

    /// The marks, each with where it starts and how long it is.
    fn marks_of(xml: &str) -> Vec<(RoadMark, f64, f64)> {
        let (net, prov) = load_str_with_provenance(xml).expect("the road loads");
        net.road_marks()
            .iter()
            .zip(&prov.road_marks)
            .map(|(m, p)| (m.clone(), p.s, p.length))
            .collect()
    }

    fn only(xml: &str) -> RoadMark {
        let marks = marks_of(xml);
        assert_eq!(marks.len(), 1, "marks");
        marks[0].0.clone()
    }

    #[test]
    fn a_mark_without_a_colour_is_standard() {
        let m = only(&road(
            0.0,
            &lane(-1, r#"<roadMark sOffset="0" type="solid"/>"#),
        ));
        assert_eq!(m.color, "standard");
        assert_eq!(m.lines[0].color, "standard");
    }

    #[test]
    fn a_mark_without_a_type_is_none_and_paints_nothing() {
        let m = only(&road(
            0.0,
            &lane(-1, r#"<roadMark sOffset="0" color="white"/>"#),
        ));
        assert_eq!(m.kind, RoadMarkType::None);
        assert!(m.lines.is_empty());
    }

    #[test]
    fn an_unrecognised_type_is_unknown_and_paints_nothing() {
        let m = only(&road(
            0.0,
            &lane(-1, r#"<roadMark sOffset="0" type="zigzag"/>"#),
        ));
        assert_eq!(m.kind, RoadMarkType::Unknown);
        assert!(m.lines.is_empty());
    }

    #[test]
    fn a_mark_without_an_s_offset_starts_at_its_section() {
        let marks = marks_of(&road(0.0, &lane(-1, r#"<roadMark type="solid"/>"#)));
        assert_eq!((marks[0].1, marks[0].2), (0.0, 20.0));
    }

    #[test]
    fn a_weight_the_map_does_not_give_is_standard() {
        let m = only(&road(
            0.0,
            &lane(-1, r#"<roadMark sOffset="0" type="solid"/>"#),
        ));
        assert_eq!((m.weight, m.width), (RoadMarkWeight::Standard, 0.12));
    }

    #[test]
    fn a_width_of_zero_falls_back_to_the_weight() {
        let mark = r#"<roadMark sOffset="0" type="solid" weight="bold" width="0"/>"#;
        let m = only(&road(0.0, &lane(-1, mark)));
        assert_eq!((m.width, m.lines[0].width), (0.25, 0.25));
    }

    /// A mark `width` wide whose `<type>` is `type_width` wide, holding
    /// `line`.
    fn with_line(width: &str, type_width: &str, line: &str) -> String {
        format!(
            r#"<roadMark sOffset="0" type="solid" weight="bold" width="{width}">
                 <type name="x" width="{type_width}">{line}</type>
               </roadMark>"#
        )
    }

    #[test]
    fn a_line_width_of_zero_falls_back_to_the_type_then_the_mark_then_the_weight() {
        let width = |mark: &str, kind: &str, line: &str| {
            let line = format!(r#"<line length="0" space="0" width="{line}"/>"#);
            let m = only(&road(0.0, &lane(-1, &with_line(mark, kind, &line))));
            (m.width, m.lines[0].width)
        };
        assert_eq!(width("0.3", "0.2", "0.1"), (0.3, 0.1));
        assert_eq!(width("0.3", "0.2", "0"), (0.3, 0.2));
        assert_eq!(width("0.3", "0", "0"), (0.3, 0.3));
        assert_eq!(width("0", "0.2", "0"), (0.2, 0.2));
        assert_eq!(width("0", "0", "0"), (0.25, 0.25));
    }

    #[test]
    fn the_lines_of_a_type_of_several_do_not_each_take_its_whole_width() {
        let lines = r#"<line length="0" space="0" tOffset="-0.1"/><line length="0" space="0" tOffset="0.1"/>"#;
        let m = only(&road(0.0, &lane(-1, &with_line("0.1", "0.3", lines))));
        assert_eq!((m.lines[0].width, m.lines[1].width), (0.1, 0.1));
    }

    #[test]
    fn a_line_with_no_length_or_space_is_continuous() {
        let line = r#"<line length="0" space="0" width="0.1"/>"#;
        let m = only(&road(0.0, &lane(-1, &with_line("0", "0", line))));
        assert_eq!(m.lines[0].pattern, LinePattern::Continuous);
        let ends: Vec<f32> = m.lines[0]
            .pieces
            .iter()
            .flat_map(|q| [q[0].x, q[1].x])
            .collect();
        let (first, last) = (ends[0], ends[ends.len() - 1]);
        assert!(first.abs() < 1e-4 && (last - 20.0).abs() < 1e-4);
    }

    #[test]
    fn a_none_mark_paints_nothing_even_with_lines() {
        let line = r#"<line length="0" space="0" width="0"/>"#;
        let mark = format!(
            r#"<roadMark sOffset="0" type="none"><type name="none">{line}</type></roadMark>"#
        );
        let m = only(&road(0.0, &lane(-1, &mark)));
        assert_eq!(m.kind, RoadMarkType::None);
        assert!(m.lines.is_empty());
    }

    #[test]
    fn a_positive_sway_moves_a_line_toward_plus_t_on_either_side() {
        let sway = r#"<roadMark sOffset="0" type="solid"><sway ds="0" a="0.5"/></roadMark>"#;
        let left =
            format!(r#"<lane id="1" type="driving"><width sOffset="0" a="3.5"/>{sway}</lane>"#);
        let xml = road(0.0, &lane(-1, sway)).replace(
            r#"<center><lane id="0" type="none"/></center>"#,
            &format!(r#"<left>{left}</left><center><lane id="0" type="none"/></center>"#),
        );
        let (net, prov) = load_str_with_provenance(&xml).expect("the road loads");
        for (m, p) in net.road_marks().iter().zip(&prov.road_marks) {
            let border = 3.5 * p.od_lane_id.signum() as f32;
            let middle = m.lines[0].pieces[0][0].lerp(m.lines[0].pieces[0][3], 0.5);
            assert!(
                (middle.y - border - 0.5).abs() < 1e-4,
                "lane {}",
                p.od_lane_id
            );
        }
    }

    #[test]
    fn a_negative_s_offset_is_0() {
        let line = r#"<line length="1" space="3" sOffset="-2"/>"#;
        let mark = with_line("0", "0", line).replace(r#"sOffset="0""#, r#"sOffset="-5""#);
        let marks = marks_of(&road(0.0, &lane(-1, &mark)));
        assert_eq!((marks[0].1, marks[0].2), (0.0, 20.0));
        let first = marks[0].0.lines[0].pieces[0];
        assert!(first[0].x.abs() < 1e-5 && (first[1].x - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_line_without_a_rule_has_none() {
        let line = r#"<line length="1" space="1"/>"#;
        let m = only(&road(0.0, &lane(-1, &with_line("0", "0", line))));
        assert_eq!(m.lines[0].rule, LineRule::None);
    }

    #[test]
    fn a_positive_t_offset_moves_a_line_toward_plus_t_on_either_side() {
        let line = r#"<line length="0" space="0" tOffset="0.5" width="0.1"/>"#;
        let mark = with_line("0", "0", line);
        let left =
            format!(r#"<lane id="1" type="driving"><width sOffset="0" a="3.5"/>{mark}</lane>"#);
        let xml = road(0.0, &lane(-1, &mark)).replace(
            r#"<center><lane id="0" type="none"/></center>"#,
            &format!(r#"<left>{left}</left><center><lane id="0" type="none"/></center>"#),
        );
        let (net, prov) = load_str_with_provenance(&xml).expect("the road loads");
        for (m, p) in net.road_marks().iter().zip(&prov.road_marks) {
            let border = 3.5 * p.od_lane_id.signum() as f32;
            let middle = m.lines[0].pieces[0][0].lerp(m.lines[0].pieces[0][3], 0.5);
            assert!(
                (middle.y - border - 0.5).abs() < 1e-4,
                "lane {}: line at {} for a border at {border}",
                p.od_lane_id,
                middle.y
            );
        }
        assert_eq!(prov.road_marks.len(), 2);
    }

    #[test]
    fn marks_out_of_order_are_sorted_rather_than_dropped() {
        let marks = r#"<roadMark sOffset="12" type="broken"/><roadMark sOffset="0" type="solid"/>"#;
        let got: Vec<(RoadMarkType, f64, f64)> = marks_of(&road(0.0, &lane(-1, marks)))
            .into_iter()
            .map(|(m, s, length)| (m.kind, s, length))
            .collect();
        assert_eq!(
            got,
            [
                (RoadMarkType::Solid, 0.0, 12.0),
                (RoadMarkType::Broken, 12.0, 8.0)
            ]
        );
    }

    #[test]
    fn a_lane_without_a_width_takes_its_marks_with_it() {
        let lanes = lane(-1, r#"<roadMark sOffset="0" type="solid"/>"#)
            + r#"<lane id="-2" type="driving"><roadMark sOffset="0" type="solid"/></lane>"#;
        let (net, prov) = load_str_with_provenance(&road(0.0, &lanes)).expect("the road loads");
        let lanes: Vec<i32> = prov.road_marks.iter().map(|p| p.od_lane_id).collect();
        assert_eq!(lanes, [-1]);
        assert_eq!(net.road_marks()[0].right, None);
        assert_eq!(
            prov.warnings,
            [crate::Warning::LaneDropped {
                road_id: "1".into(),
                section: 0,
                lane: -2
            }]
        );
    }

    #[test]
    fn dashes_are_measured_along_the_reference_line() {
        // A left turn of radius 50 m, so the border of lane -1, 3.5 m to the
        // right, is on a radius of 53.5 m. A 4 m dash of s turns 0.08 rad.
        let m = only(&road(
            0.02,
            &lane(-1, r#"<roadMark sOffset="0" type="broken"/>"#),
        ));
        let pieces = &m.lines[0].pieces;
        let middle = |q: &[crate::Point; 4], a: usize, b: usize| q[a].lerp(q[b], 0.5);
        let first_dash = 1 + pieces
            .windows(2)
            .take_while(|w| w[0][1].distance_to(w[1][0]) < 1e-4)
            .count();
        let start = middle(&pieces[0], 0, 3);
        let end = middle(&pieces[first_dash - 1], 1, 2);
        let chord = 2.0 * 53.5 * 0.04_f32.sin();
        assert!(
            (start.distance_to(end) - chord).abs() < 1e-2,
            "dash chord {} want {chord}",
            start.distance_to(end)
        );
    }
}
