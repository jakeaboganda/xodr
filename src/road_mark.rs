//! Road marks: the lines painted along lane borders, and what they tell
//! traffic. Like lanes, they don't depend on the file format. A mark names
//! the lanes either side of it, and its lines are already placed on the road.

use crate::coords::Point;
use crate::LaneId;

/// An opaque road mark identifier. Not a position in
/// [`RoadNetwork::road_marks`](crate::RoadNetwork::road_marks), so look marks
/// up with [`RoadNetwork::road_mark`](crate::RoadNetwork::road_mark).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadMarkId(pub usize);

/// What a road mark is.
///
/// A double type names its lines from the inside of the road out, or left
/// to right on the line between the two sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RoadMarkType {
    /// No mark.
    None,
    /// One solid line.
    Solid,
    /// One broken line.
    Broken,
    /// Two solid lines.
    SolidSolid,
    /// A solid line inside a broken one.
    SolidBroken,
    /// A broken line inside a solid one.
    BrokenSolid,
    /// Two broken lines.
    BrokenBroken,
    /// Raised pavement markers.
    BottsDots,
    /// A grass edge.
    Grass,
    /// A kerb.
    Curb,
    /// The edge of the road surface, with no paint.
    Edge,
    /// A pattern the map describes line by line.
    Custom,
    /// A type this crate does not recognise.
    Unknown,
}

impl RoadMarkType {
    /// A stable lowercase name, for a legend, a log line, or a viewer readout.
    /// Format-neutral, so it is not necessarily the token any particular map
    /// format spells the type with.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Solid => "solid",
            Self::Broken => "broken",
            Self::SolidSolid => "solid-solid",
            Self::SolidBroken => "solid-broken",
            Self::BrokenSolid => "broken-solid",
            Self::BrokenBroken => "broken-broken",
            Self::BottsDots => "botts-dots",
            Self::Grass => "grass",
            Self::Curb => "curb",
            Self::Edge => "edge",
            Self::Custom => "custom",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for RoadMarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How heavy a road mark's lines are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RoadMarkWeight {
    /// The usual width.
    #[default]
    Standard,
    /// Wider than usual.
    Bold,
}

impl RoadMarkWeight {
    /// `standard` or `bold`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Bold => "bold",
        }
    }
}

impl std::fmt::Display for RoadMarkWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which way traffic may cross a road mark. Lanes are numbered up toward the
/// left of the road's reference direction, so crossing toward higher numbers
/// is crossing to the left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LaneChange {
    /// Either way.
    Both,
    /// Only from [`RoadMark::right`] to [`RoadMark::left`].
    Increase,
    /// Only from [`RoadMark::left`] to [`RoadMark::right`].
    Decrease,
    /// Neither way.
    None,
    /// A value this crate does not recognise.
    Unknown,
}

impl LaneChange {
    /// A stable lowercase name, for a legend, a log line, or a viewer readout.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Increase => "increase",
            Self::Decrease => "decrease",
            Self::None => "none",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for LaneChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a line tells traffic about crossing it from the inside of the road,
/// or from the left on the line between the two sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LineRule {
    /// Crossing is not allowed.
    NoPassing,
    /// Cross with care.
    Caution,
    /// No rule.
    None,
    /// A rule this crate does not recognise.
    Unknown,
}

impl LineRule {
    /// A stable lowercase name, for a legend, a log line, or a viewer readout.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoPassing => "no-passing",
            Self::Caution => "caution",
            Self::None => "none",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for LineRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a road mark's line is broken up along the road.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LinePattern {
    /// One unbroken line. The OpenDRIVE spec makes a line's `length` the
    /// part painted, so a line with a `length` and a `space` of 0 paints
    /// nothing. It is continuous here, as esmini writes it for a solid line.
    /// So is a line with a `length` and no `space`, whose dashes would touch.
    Continuous,
    /// Dashes `length` metres long with `space` metres between them, from
    /// the start of the line, to the end of the mark. The OpenDRIVE spec does not say what they are
    /// measured along. They are measured along the road's reference line, as
    /// libOpenDRIVE and esmini do, so a dash on the outside of a bend is
    /// longer.
    Dashed {
        /// Metres painted.
        length: f32,
        /// Metres left bare.
        space: f32,
    },
    /// One dash `length` metres long, from the start of the line, measured
    /// the same way. What an `<explicit>` line in OpenDRIVE is.
    Single {
        /// Metres painted.
        length: f32,
    },
}

/// One painted line of a road mark.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadMarkLine {
    /// The colour the map names, such as `white`, or `standard`, meaning
    /// white. Free text. The mark's colour where the line gives none.
    pub color: String,
    /// Metres across. The line's own width, or its `<type>`'s if the type
    /// has one line, or the `<roadMark>`'s `width`, the first given above 0. The
    /// OpenDRIVE spec says each is above 0, and esmini writes 0 on all three.
    pub width: f32,
    /// How far the middle of the line is from the lane border, in metres,
    /// toward the left of the road's reference direction. The OpenDRIVE spec
    /// says only that it is a lateral offset from the border. This reads it
    /// along +t on either side of the road, as libOpenDRIVE and esmini do, so
    /// a positive offset moves a right lane's line inward and a left lane's
    /// outward. A mark's sway moves the pieces further, and is not in this.
    pub t_offset: f32,
    /// How far along the road the line starts after the start of its mark,
    /// in metres. The OpenDRIVE spec says it is at least 0, and a negative
    /// one is 0, as esmini reads it.
    pub s_offset: f32,
    /// Whether it is solid or dashed.
    pub pattern: LinePattern,
    /// What it tells traffic about crossing it. `None` where the map gives
    /// no rule, which the OpenDRIVE spec gives no default for.
    pub rule: LineRule,
    /// The painted pieces, in order along the road, in the network's frame.
    /// Each is four corners going anticlockwise seen from above, lying in the
    /// surface of the mark's lane: a dash, or the part of one between two of
    /// the lane's stations. So a sidewalk's mark rises with it, and a kerb's,
    /// the lane below's, stays on the road. The spec does not say which.
    pub pieces: Vec<[Point; 4]>,
}

/// One road mark: the paint along a stretch of one lane border, and what it
/// tells traffic about crossing it.
///
/// A mark with a [`kind`](Self::kind) that isn't paint, such as a kerb, or
/// no mark at all, keeps its meaning and has no lines.
///
/// A lane the map gives neither a width nor a border does not bake, and nor
/// do its marks. The OpenDRIVE spec would draw them on its border, which is
/// then its inner border.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadMark {
    /// This mark's identity.
    pub id: RoadMarkId,
    /// What it is. The OpenDRIVE spec requires a type. A mark the map gives
    /// none is `None`, as libOpenDRIVE reads it.
    pub kind: RoadMarkType,
    /// How heavy its lines are. `Standard` if the map does not say, which
    /// the OpenDRIVE spec gives no default for, or names a weight other
    /// than `standard` or `bold`.
    pub weight: RoadMarkWeight,
    /// The colour the map names, such as `yellow`, or `standard`, meaning
    /// white. Free text. The OpenDRIVE spec requires one. A mark the map
    /// gives none is `standard`, as libOpenDRIVE and esmini read it.
    pub color: String,
    /// Metres across. Its `<type>`'s width, which the spec says supersedes
    /// the mark's, or the mark's, or 0.12 for a standard weight and 0.25 for
    /// a bold one where it gives neither, as libOpenDRIVE has them. The OpenDRIVE spec says a width is above 0,
    /// and a width of 0 counts as none, since esmini writes one on every
    /// mark.
    pub width: f32,
    /// The paint's thickness in metres, if the map gives it.
    pub height: Option<f32>,
    /// Which way traffic may cross it.
    pub lane_change: LaneChange,
    /// The lane on its left, looking along the road's reference direction.
    /// `None` at the edge of the road, or where that lane did not bake.
    pub left: Option<LaneId>,
    /// The lane on its right, looking the same way. `None` at the edge of
    /// the road, or where that lane did not bake.
    pub right: Option<LaneId>,
    /// Its painted lines. Empty for a mark that paints nothing.
    ///
    /// A mark's `<sway>`s move every line sideways, each by its cubic from
    /// its `ds` along the mark to the next, and not before the first. The
    /// OpenDRIVE spec does not say which way. This reads it along +t, as
    /// [`RoadMarkLine::t_offset`] is. The pieces follow the lane's stations,
    /// so a sway is straight between them.
    ///
    /// A mark with `<type><line>`s or `<explicit><line>`s paints those,
    /// whatever its type, except
    /// a mark of type `None`, which paints nothing. The OpenDRIVE spec gives
    /// no exception, but esmini writes a line of no width under every `none`
    /// mark and does not draw it. The spec does not say how a type looks. A mark the map
    /// describes by its type alone gets the lines esmini draws for it: one
    /// continuous line for `Solid`, dashes 4 m long with 8 m between for
    /// `Broken`, and for a double type two lines, one width either side of
    /// the border. Every other type paints nothing.
    pub lines: Vec<RoadMarkLine>,
}
