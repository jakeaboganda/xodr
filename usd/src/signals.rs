//! Signals and controllers, and the type classes signal boards inherit.

use std::collections::BTreeSet;
use std::io::{self, Write};

use xodr::{
    Orientation, Point, Provenance, Referenced, RoadNetwork, Semantic, Signal, SignalBoard, Vector,
};

use crate::supports::Placement;
use crate::{close, list, open, quote, tuple, write_mesh, MeshPrim, Paths, Tag};

/// Metres across and up for a board the map gives no size.
pub(crate) const FALLBACK_SIZE: f32 = 0.6;

/// Metres a sign or display sits in front of its signal's box.
const FRONT: f32 = 0.01;

/// One `Xform` per signal under `/Map/Signals`. `placements` says what
/// holds up each signal, in the same order, and where its boards stand.
/// Returns the type classes the boards inherit.
pub(crate) fn signals(
    net: &RoadNetwork,
    provenance: &Provenance,
    paths: &Paths,
    placements: &[Placement],
    out: &mut impl Write,
) -> io::Result<BTreeSet<String>> {
    let mut classes = BTreeSet::new();
    open(out, 1, "def Scope", "Signals", &[], &[])?;
    for (signal, placement) in net.signals().iter().zip(placements) {
        let prov = provenance.signals.iter().find(|p| p.signal == signal.id);
        let (width, height) = (signal.width, signal.height);
        let mut tags = vec![
            ("name", Tag::Text(signal.name.clone())),
            ("country", Tag::Text(signal.country.clone())),
            (
                "countryRevision",
                Tag::Text(signal.country_revision.clone()),
            ),
            ("type", Tag::Text(signal.kind.clone())),
            ("subtype", Tag::Text(signal.subtype.clone())),
            ("text", Tag::Text(signal.text.clone())),
            ("dynamic", Tag::Bool(signal.dynamic)),
            ("invalidated", Tag::Bool(signal.invalidated)),
            ("temporary", Tag::Bool(signal.temporary)),
            (
                "sizeGuessed",
                Tag::Bool(width.is_none() || height.is_none()),
            ),
        ];
        if let Some(value) = signal.value {
            tags.push(("value", Tag::Double(value)));
        }
        if let Some(unit) = signal.unit {
            tags.push(("unit", Tag::Text(unit.as_str().to_string())));
        }
        if let Some(length) = signal.length {
            tags.push(("length", Tag::Float(length)));
        }
        if let Some(p) = prov {
            tags.push(("signalId", Tag::Text(p.od_id.clone())));
            tags.push(("roadId", Tag::Text(p.road_id.clone())));
            tags.push(("s", Tag::Double(p.s)));
            tags.push(("t", Tag::Double(p.t)));
            tags.push(("orientation", Tag::Token(orientation(p.orientation))));
        }
        tags.extend(placement.support.tags());
        tags.push(("appliesAt", Tag::Points(signal.applies_at.clone())));
        let lanes = signal.lanes.iter();
        let lanes = lanes.filter_map(|l| paths.lanes.get(l).cloned());
        tags.push(("lanes", Tag::Targets(lanes.collect())));
        let dependencies = signal.dependencies.iter();
        let dependencies = dependencies.map(|d| (signal_path(d.signal.0), d.kind.clone()));
        let (targets, kinds) = links(dependencies);
        tags.push(("dependencies", Tag::Targets(targets)));
        tags.push(("dependencyTypes", Tag::Texts(kinds)));
        let references = signal.references.iter().filter_map(|r| {
            let path = match r.to {
                Referenced::Signal(id) => Some(signal_path(id.0)),
                Referenced::Object(id) => paths.objects.get(&id).cloned(),
            };
            Some((path?, r.kind.clone()))
        });
        let (targets, kinds) = links(references);
        tags.push(("references", Tag::Targets(targets)));
        tags.push(("referenceTypes", Tag::Texts(kinds)));

        let name = format!("signal_{}", signal.id.0);
        open(out, 2, "def Xform", &name, &[], &tags)?;
        let pad = "            ";
        let [rx, ry, rz] = [signal.roll, signal.pitch, signal.heading].map(f32::to_degrees);
        writeln!(
            out,
            "{pad}double3 xformOp:translate = {}",
            tuple(signal.position.to_array())
        )?;
        writeln!(out, "{pad}float3 xformOp:rotateXYZ = ({rx}, {ry}, {rz})")?;
        writeln!(
            out,
            "{pad}uniform token[] xformOpOrder = [\"xformOp:translate\", \"xformOp:rotateXYZ\"]"
        )?;

        let board = Board {
            country: &signal.country,
            kind: &signal.kind,
            subtype: &signal.subtype,
            semantics: &signal.semantics,
            at: [placement.front, 0.0, 0.0],
            turned: false,
            width,
            height,
        };
        if two_faced(provenance, signal) {
            let front = board;
            let back = Board {
                at: [-placement.back, 0.0, 0.0],
                turned: true,
                ..board
            };
            classes.insert(front.write(out, "board", &[])?);
            classes.insert(back.write(out, "board_back", &[])?);
        } else {
            classes.insert(board.write(out, "board", &[])?);
        }
        let front = placement.front + signal.length.unwrap_or(0.0) / 2.0 + FRONT;
        for (k, b) in signal.boards.iter().enumerate() {
            match b {
                SignalBoard::Static(signs) => {
                    for (j, sign) in signs.iter().enumerate() {
                        let country = match sign.country.as_str() {
                            "" => &signal.country,
                            country => country,
                        };
                        let tags = [
                            ("name", Tag::Text(sign.name.clone())),
                            ("country", Tag::Text(sign.country.clone())),
                            ("type", Tag::Text(sign.kind.clone())),
                            ("subtype", Tag::Text(sign.subtype.clone())),
                            ("text", Tag::Text(sign.text.clone())),
                        ];
                        let mut tags = Vec::from(tags);
                        if let Some(value) = sign.value {
                            tags.push(("value", Tag::Double(value)));
                        }
                        if let Some(unit) = sign.unit {
                            tags.push(("unit", Tag::Text(unit.as_str().to_string())));
                        }
                        let board = Board {
                            country,
                            kind: &sign.kind,
                            subtype: &sign.subtype,
                            semantics: &sign.semantics,
                            at: local(signal, sign.position, front),
                            turned: false,
                            width: sign.width,
                            height: sign.height,
                        };
                        classes.insert(board.write(out, &format!("sign_{k}_{j}"), &tags)?);
                    }
                }
                SignalBoard::Message(m) => {
                    let tags = [("display", Tag::Text(m.display.clone()))];
                    open(out, 3, "def Xform", &format!("display_{k}"), &[], &tags)?;
                    let at = local(signal, m.position, front);
                    let size = (m.width, m.height);
                    write_quad(out, 4, "screen", at, size, 0.1, vec![])?;
                    for (j, area) in m.areas.iter().enumerate() {
                        let at = local(signal, area.position, front + FRONT);
                        let size = (area.width, area.height);
                        let index = area.index.map(|i| ("index", Tag::Int(i.into())));
                        let tags = index.into_iter().collect();
                        write_quad(out, 4, &format!("area_{j}"), at, size, 0.25, tags)?;
                    }
                    close(out, 3)?;
                }
            }
        }
        close(out, 2)?;
    }
    close(out, 1)?;
    Ok(classes)
}

/// One `Scope` per controller under `/Map/Controllers`, linked to its
/// signals.
pub(crate) fn controllers(
    net: &RoadNetwork,
    provenance: &Provenance,
    out: &mut impl Write,
) -> io::Result<()> {
    open(out, 1, "def Scope", "Controllers", &[], &[])?;
    for c in net.controllers() {
        let mut tags = vec![("name", Tag::Text(c.name.clone()))];
        if let Some(p) = provenance.controllers.iter().find(|p| p.controller == c.id) {
            tags.push(("controllerId", Tag::Text(p.od_id.clone())));
        }
        if let Some(sequence) = c.sequence {
            tags.push(("sequence", Tag::Int(sequence.into())));
        }
        let signals = c.signals.iter();
        let (targets, kinds) = links(signals.map(|s| (signal_path(s.signal.0), s.kind.clone())));
        tags.push(("signals", Tag::Targets(targets)));
        tags.push(("controlTypes", Tag::Texts(kinds)));
        open(
            out,
            2,
            "def Scope",
            &format!("controller_{}", c.id.0),
            &[],
            &tags,
        )?;
        close(out, 2)?;
    }
    close(out, 1)
}

/// The type classes under `/_SignalTypes`, each a grey board 1 m across
/// and 1 m up that a catalogue layer can replace.
pub(crate) fn type_classes(classes: &BTreeSet<String>, out: &mut impl Write) -> io::Result<()> {
    writeln!(out)?;
    open(out, 0, "class", "_SignalTypes", &[], &[])?;
    for class in classes {
        open(out, 1, "class Xform", class, &[], &[])?;
        write_quad(
            out,
            2,
            "Board",
            [0.0; 3],
            (Some(1.0), Some(1.0)),
            0.7,
            vec![],
        )?;
        close(out, 1)?;
    }
    close(out, 0)
}

/// A board that inherits a type class: a signal's own, or a sign on it.
#[derive(Clone, Copy)]
struct Board<'a> {
    country: &'a str,
    kind: &'a str,
    subtype: &'a str,
    semantics: &'a [Semantic],
    /// The middle of its bottom edge, in the signal's frame.
    at: [f32; 3],
    /// Whether it faces back, -X in the signal's frame.
    turned: bool,
    width: Option<f32>,
    height: Option<f32>,
}

impl Board<'_> {
    /// Write the board as an `Xform` named `name`, scaled to its size.
    /// Returns its type class.
    fn write(&self, out: &mut impl Write, name: &str, tags: &[(&str, Tag)]) -> io::Result<String> {
        let class = type_class(self.country, self.kind, self.subtype);
        let code = format!("{}:{}:{}", self.country, self.kind, self.subtype);
        let meaning: BTreeSet<String> = self.semantics.iter().map(meaning).collect();
        let mut sets = vec![("opendrive", vec![code])];
        if !meaning.is_empty() {
            sets.push(("meaning", meaning.into_iter().collect()));
        }
        let schemas = sets
            .iter()
            .map(|(set, _)| quote(&format!("SemanticsLabelsAPI:{set}")));
        let meta = [
            format!("inherits = </_SignalTypes/{class}>"),
            format!("prepend apiSchemas = [{}]", list(schemas)),
        ];
        open(out, 3, "def Xform", name, &meta, tags)?;
        let pad = "                ";
        for (set, labels) in &sets {
            let labels = labels.iter().map(|l| quote(l));
            writeln!(
                out,
                "{pad}token[] semantics:labels:{set} = [{}]",
                list(labels)
            )?;
        }
        let size = [
            1.0,
            self.width.unwrap_or(FALLBACK_SIZE),
            self.height.unwrap_or(FALLBACK_SIZE),
        ];
        let mut ops = vec![];
        if self.at != [0.0; 3] {
            writeln!(out, "{pad}double3 xformOp:translate = {}", tuple(self.at))?;
            ops.push("\"xformOp:translate\"");
        }
        if self.turned {
            writeln!(out, "{pad}float xformOp:rotateZ = 180")?;
            ops.push("\"xformOp:rotateZ\"");
        }
        writeln!(out, "{pad}float3 xformOp:scale = {}", tuple(size))?;
        ops.push("\"xformOp:scale\"");
        writeln!(out, "{pad}uniform token[] xformOpOrder = [{}]", list(ops))?;
        close(out, 3)?;
        Ok(class)
    }
}

/// A grey, double-sided quad facing +X, `size` across and up from `at`,
/// the middle of its bottom edge.
fn write_quad(
    out: &mut impl Write,
    depth: usize,
    name: &str,
    at: [f32; 3],
    (width, height): (Option<f32>, Option<f32>),
    grey: f32,
    tags: Vec<(&'static str, Tag)>,
) -> io::Result<()> {
    let (w, h) = (
        width.unwrap_or(FALLBACK_SIZE),
        height.unwrap_or(FALLBACK_SIZE),
    );
    let [x, y, z] = at;
    let corners = [(-w, 0.0), (w, 0.0), (w, h), (-w, h)];
    let points = corners.map(|(v, up)| Point::new(x, y + v / 2.0, z + up));
    let quad = MeshPrim {
        name: name.to_string(),
        tags,
        points: &points,
        normals: &[],
        face_size: 4,
        indices: vec![0, 1, 2, 3],
        colors: vec![[grey; 3]],
        double_sided: true,
    };
    write_mesh(out, depth, &quad)
}

/// The name of the type class for a signal's codes: `DE`, `274` and `55`
/// give `DE_274_55`. Characters USD doesn't allow in a name become `_`, and
/// a name that would start with a digit gets a `_` in front.
fn type_class(country: &str, kind: &str, subtype: &str) -> String {
    let name: String = [country, kind, subtype]
        .join("_")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    }
}

/// A `<semantics>` child as a label: its element name, then its `type` if
/// it has one, such as `speed:maximum`.
fn meaning(semantic: &Semantic) -> String {
    let (name, kind) = match semantic {
        Semantic::Speed { kind, .. } => ("speed", kind.as_str()),
        Semantic::Lane { kind } => ("lane", kind.as_str()),
        Semantic::Priority { kind } => ("priority", kind.as_str()),
        Semantic::Prohibited(_) => ("prohibited", ""),
        Semantic::Warning => ("warning", ""),
        Semantic::Routing => ("routing", ""),
        Semantic::StreetName => ("streetname", ""),
        Semantic::Parking => ("parking", ""),
        Semantic::Tourist => ("tourist", ""),
        Semantic::SupplementaryTime { kind, .. } => ("supplementaryTime", kind.as_str()),
        Semantic::SupplementaryAllows(_) => ("supplementaryAllows", ""),
        Semantic::SupplementaryProhibits(_) => ("supplementaryProhibits", ""),
        Semantic::SupplementaryDistance { kind, .. } => ("supplementaryDistance", kind.as_str()),
        Semantic::SupplementaryEnvironment { kind } => ("supplementaryEnvironment", kind.as_str()),
        Semantic::SupplementaryExplanatory => ("supplementaryExplanatory", ""),
    };
    match kind {
        "" => name.to_string(),
        kind => format!("{name}:{kind}"),
    }
}

/// A relationship's targets and the type of each, in order. USD allows a
/// target once, so a target named again is dropped with its type.
fn links(pairs: impl Iterator<Item = (String, String)>) -> (Vec<String>, Vec<String>) {
    let mut seen = BTreeSet::new();
    pairs
        .filter(|(target, _)| seen.insert(target.clone()))
        .unzip()
}

/// How far behind its position the back of `signal`'s box reaches, across
/// the ground, in metres: half its `length`, and more if it is pitched, since
/// the board leans over its height.
pub(crate) fn back(signal: &Signal) -> f32 {
    let height = signal.height.unwrap_or(FALLBACK_SIZE);
    signal.length.unwrap_or(0.0) / 2.0 + height * signal.pitch.sin().abs()
}

/// Whether `signal` faces both ways: the map gives it `orientation="none"`,
/// so it applies to traffic in both directions.
pub(crate) fn two_faced(provenance: &Provenance, signal: &Signal) -> bool {
    provenance
        .signals
        .iter()
        .find(|p| p.signal == signal.id)
        .is_some_and(|p| p.orientation == Orientation::Both)
}

fn signal_path(id: usize) -> String {
    format!("/Map/Signals/signal_{id}")
}

/// How OpenDRIVE spells an orientation.
fn orientation(o: Orientation) -> &'static str {
    match o {
        Orientation::Positive => "+",
        Orientation::Negative => "-",
        Orientation::Both => "none",
    }
}

/// `point` in `signal`'s frame, moved `front` metres toward its traffic.
fn local(signal: &Signal, point: Point, front: f32) -> [f32; 3] {
    let d = point - signal.position;
    let [x, y, z] = unrotate(signal, d);
    [x + front, y, z]
}

/// `d` turned back by the signal's heading, pitch and roll. The crate turns
/// a board by roll, then pitch, then heading, as USD's `rotateXYZ` does.
pub(crate) fn unrotate(signal: &Signal, d: Vector) -> [f32; 3] {
    let (sh, ch) = signal.heading.sin_cos();
    let (sp, cp) = signal.pitch.sin_cos();
    let (sr, cr) = signal.roll.sin_cos();
    let r = [
        [ch * cp, ch * sp * sr - sh * cr, ch * sp * cr + sh * sr],
        [sh * cp, sh * sp * sr + ch * cr, sh * sp * cr - ch * sr],
        [-sp, cp * sr, cp * cr],
    ];
    let d = d.to_array();
    [0, 1, 2].map(|j| (0..3).map(|i| r[i][j] * d[i]).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sign_lands_where_its_board_puts_it() {
        let net = xodr::load_file("../tests/data/signal_semantics.xodr").expect("map loads");
        let signal = net
            .signals()
            .iter()
            .find(|s| !s.boards.is_empty())
            .expect("a board");
        let SignalBoard::Static(signs) = &signal.boards[0] else {
            panic!("a static board")
        };
        let at: Vec<[f32; 3]> = signs
            .iter()
            .map(|s| local(signal, s.position, 0.0))
            .collect();
        for (got, want) in at.iter().zip([[0.0, 1.0, 0.5], [0.0, -1.0, 0.5]]) {
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() < 1e-5, "{at:?}");
            }
        }
    }

    #[test]
    fn a_type_class_name_is_valid_in_usd() {
        assert_eq!(type_class("DE", "274", "55"), "DE_274_55");
        assert_eq!(type_class("DE", "274", "-1"), "DE_274__1");
        assert_eq!(type_class("", "1000001", ""), "_1000001_");
        assert_eq!(type_class("1", "a.b", "c"), "_1_a_b_c");
    }
}
