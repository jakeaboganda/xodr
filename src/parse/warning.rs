//! What a load did with the parts of a file it could not read as written.

use std::fmt;

/// One thing a load dropped, or read against a rule of the OpenDRIVE spec,
/// in [`Provenance::warnings`](crate::Provenance::warnings).
///
/// Each variant names where in the file it happened, and `Display` gives the
/// message. New kinds are not a breaking change. [`Warning::Ignored`] names
/// what the crate skips in a file.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Warning {
    /// A feature of the file the crate ignores: an element it skips, such as
    /// `<userData>`, or an attribute that changes a road or a junction and
    /// means nothing to it yet, such as `perpToRoad="true"`. The warning
    /// comes once per feature for each road, junction, or the rest of the
    /// document, with the number of uses, so a map with 2,000 of them
    /// raises one.
    ///
    /// `docs/support.md` and "What the importer ignores" in the crate docs
    /// list the features. A feature leaves the list once the crate reads it.
    Ignored {
        /// The `<road id>` it is on, or `(no id)` for a road without one.
        /// Empty for a feature of a junction or of the document outside
        /// both.
        road_id: String,
        /// The `<junction id>` it is on, or `(no id)` for a junction
        /// without one. Empty for a feature of a road or of the document
        /// outside both.
        junction_id: String,
        /// The feature, such as `<userData>` or `<object perpToRoad="true">`.
        feature: String,
        /// How many times the file uses it there.
        count: usize,
    },
    /// A `<road>` baked nothing, and everything on it went with it.
    RoadSkipped {
        /// Its `<road id>`.
        road_id: String,
        /// What it lacked.
        reason: RoadSkipReason,
    },
    /// A `<planView><geometry>` the crate can't bake: one missing its `s`,
    /// `x`, `y`, `hdg` or `length`, one whose `length` isn't above 0, or one
    /// of a shape it doesn't know, or over [`MAX_LENGTH`](crate::MAX_LENGTH).
    /// The crate drops it, so the geometry before
    /// it runs on over its stretch of road.
    GeometryDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// Its `s`, if it has one.
        s: Option<f64>,
    },
    /// A road mark line the crate can't paint. Either it has too many
    /// dashes: more than 100,000 on the line, or more than are left of
    /// 1,000,000 for the whole load, as when a broken file gives dashes a
    /// fraction of a millimetre long. Or its width or offset is so large its
    /// paint lands out of range. The line keeps its pattern and paints
    /// nothing.
    RoadMarkLineDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// Where its mark starts, in metres along the reference line.
        s: f64,
        /// The `<lane id>` whose outer border it runs along, or 0 for the
        /// center line.
        lane: i32,
    },
    /// An `<object>` that would make more than 100,000 copies or dashes: a
    /// `<repeat>` with a `distance` that short for its `length`, or a dashed
    /// `<marking>`. A broken file can give a distance a fraction of a
    /// millimetre. The crate leaves out the repeat's copies, or the
    /// marking's paint.
    TooManyCopies {
        /// The `<road id>` it is on.
        road_id: String,
        /// Its `<object id>`.
        object_id: String,
    },
    /// A `<lane>` under `<left>` or `<right>` whose `id` isn't a whole
    /// number, or is 0, the center lane's. The crate can't tell which lane
    /// it is, so it drops it.
    LaneIdUnreadable {
        /// The `<road id>` it is on.
        road_id: String,
        /// Its zero-based lane section.
        section: usize,
        /// Its `id`, as the file writes it. Empty if it has none.
        id: String,
    },
    /// A `<lane>` with no `<width>` or `<border>` the crate could read. It
    /// has no extent, so it did not bake, and neither did its road marks. A
    /// lane outside it stacks on the lane inside it.
    LaneDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index, as in
        /// [`LaneProvenance::section`](crate::LaneProvenance::section).
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
    },
    /// A `<lane>` with `<border>`s, in a lane section that also has
    /// `<width>`s, on it or on another lane. The spec makes the two
    /// exclusive, and uses the widths where a section has both, so read
    /// literally a lane with only borders there has no extent. The crate
    /// reads each lane on its own: its widths if it has any, and its borders
    /// if not.
    WidthAndBorder {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
    },
    /// A `<lane>` that takes its extent from `<border>`s, on a road whose
    /// `<laneOffset>` is not 0 in its lane section. The spec forbids the two
    /// together. The crate measures the border from the reference line and
    /// ignores the offset for it.
    BorderWithLaneOffset {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
    },
    /// A `<lane>` whose `<border>` lies inside its inner neighbour's outer
    /// border, which the spec forbids. The crate gives the lane 0 width
    /// there. Checked at the lane's stations.
    BorderCrossesInnerLane {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
        /// The first station where it crosses, in metres along the
        /// reference line.
        s: f64,
    },
    /// A `<lateralProfile>` whose `<shape>`s start inside the road's right
    /// edge. The spec says each profile covers the whole road. The crate
    /// holds the first shape's value from its `t` out to the edge.
    ShapeShortOfRoad {
        /// The `<road id>` it is on.
        road_id: String,
        /// The profile's `s`.
        s: f64,
    },
    /// A `<road>` whose `rule` is neither `RHT` nor `LHT`, the two values
    /// the spec allows. The crate reads it as `RHT`, the spec's default.
    UnknownTrafficRule {
        /// Its `<road id>`.
        road_id: String,
        /// The value of its `rule`.
        rule: String,
    },
    /// A `<lane>` whose `direction` is none of `standard`, `reversed` and
    /// `both`, the values the spec allows. The crate reads it as
    /// `standard`, the spec's default.
    UnknownLaneDirection {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
        /// The value of its `direction`.
        direction: String,
    },
    /// A junction `<connection>` without an `incomingRoad`, or without the
    /// road it leads into: a `connectingRoad`, or a `linkedRoad` in a
    /// `type="direct"` junction, or in a `<junction>` without the `id` the
    /// spec requires. The spec requires both roads. The crate drops the
    /// connection, so no lane links across it.
    ConnectionDropped {
        /// Its `incomingRoad`, empty if it has none.
        incoming_road_id: String,
        /// The `<junction id>` it is in, empty if it has none.
        junction_id: String,
        /// Its `<connection id>`, empty if it has none.
        connection_id: String,
    },
    /// A junction `<laneLink>` whose `from` or `to` isn't a whole number.
    /// The crate drops it, so the lane it names doesn't link across the
    /// junction there.
    LaneLinkDropped {
        /// Its connection's `incomingRoad`.
        incoming_road_id: String,
        /// The `<junction id>` it is in.
        junction_id: String,
        /// Its `<connection id>`, empty if it has none.
        connection_id: String,
        /// Its `from`, as the file writes it.
        from: String,
        /// Its `to`, as the file writes it.
        to: String,
    },
    /// A road that a common junction's `<connection>` names as its
    /// `incomingRoad`, whose own `<link>` names the junction at neither end.
    /// The spec says it must name it at the end that meets it. The crate finds that end
    /// from the connecting road's link back to the road, and links the road
    /// into the junction there, so its lanes drive on through it.
    JunctionLinkMissing {
        /// The incoming road's `<road id>`.
        road_id: String,
        /// The `<junction id>`.
        junction_id: String,
        /// The end of the road the junction was added to.
        end: RoadEnd,
    },
    /// A `<road>` whose `length` differs from where its `<planView>` ends,
    /// by more than a centimetre. The spec says they are the same. The crate
    /// bakes the road to its `length`, as libOpenDRIVE does, so it stops
    /// short of its geometry's end, or runs its last geometry on past it.
    RoadLengthMismatch {
        /// Its `<road id>`.
        road_id: String,
        /// Its `length`, in metres.
        length: f64,
        /// Where its last `<geometry>` ends: its `s` plus its `length`.
        plan_view: f64,
    },
    /// A lane link between lanes that don't meet: the lane's exit and its
    /// successor's entry are more than 10 cm apart, measured across both
    /// lanes from border to border. The crate keeps the link.
    LinkGap {
        /// The `<road id>` of the lane the link leaves.
        road_id: String,
        /// Its zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
        /// The `<road id>` of the lane it leads into.
        to_road_id: String,
        /// That lane's zero-based lane-section index.
        to_section: usize,
        /// That lane's `<lane id>`.
        to_lane: i32,
        /// How far apart they are, in metres.
        gap: f64,
    },
    /// A road `<neighbor>` naming no road the load baked, or whose `side` is
    /// neither `left` nor `right`, or whose `direction` is neither `same`
    /// nor `opposite`. The crate drops it.
    NeighborDropped {
        /// The `<road id>` whose `<link>` it is in.
        road_id: String,
        /// Its `elementId`, empty if it has none.
        neighbor_id: String,
        /// Its `side`, empty if it has none.
        side: String,
        /// Its `direction`, empty if it has none.
        direction: String,
    },
    /// A junction `<priority>` whose `high` or `low` names no road the load
    /// baked. The crate drops it.
    PriorityDropped {
        /// The `<junction id>` it is in.
        junction_id: String,
        /// Its `high`, empty if it has none.
        high: String,
        /// Its `low`, empty if it has none.
        low: String,
        /// The road of the two the load has, or `low` if it has neither.
        road_id: String,
    },
    /// A railway `<switch>` or a platform `<segment>` the crate can't place:
    /// one naming a road the load lacks, an `s` off that road, a segment
    /// whose `sEnd` is before its `sStart`, or a `position`, `dir` or `side`
    /// the spec does not allow. The crate drops
    /// it.
    RailwayDropped {
        /// `switch` or `platform segment`.
        element: String,
        /// The switch's `id`, or the platform's.
        id: String,
        /// The road it is on, or names.
        road_id: String,
    },
    /// A `<junction type="virtual">` with no `mainRoad`, one naming a road
    /// the load lacks, or an `sStart` or `sEnd` missing or off that road. The
    /// spec requires them. The crate keeps the junction's links, without a
    /// main road.
    VirtualJunctionWithoutMainRoad {
        /// The `<junction id>`.
        junction_id: String,
    },
    /// A virtual junction's link the crate can't place: one naming a road
    /// the load lacks, an `elementS` off that road, or an `elementDir` other
    /// than `+` or `-`. The crate drops it.
    VirtualLinkDropped {
        /// The `<junction id>`.
        junction_id: String,
        /// The road whose `<link>` it is, empty for a `type="virtual"`
        /// connection.
        road_id: String,
        /// The connection's `id`, empty for a road's `<link>`.
        connection_id: String,
    },
    /// A lane pair of a virtual junction's link that names a lane its road
    /// doesn't have there. The crate keeps the link without that pair.
    VirtualLaneDropped {
        /// The `<junction id>`.
        junction_id: String,
        /// The road whose `<link>` gives the link, empty for a
        /// `type="virtual"` connection.
        road_id: String,
        /// The connection's `id`, empty for a road's `<link>`.
        connection_id: String,
        /// The pair's first `<lane id>`: on the link's road, or a
        /// `<laneLink>`'s `from`.
        from: i32,
        /// The pair's second `<lane id>`: on the road it meets, or a
        /// `<laneLink>`'s `to`.
        to: i32,
    },
    /// A `<junctionReference>` naming a junction the file does not have. The
    /// crate leaves it out of the group.
    JunctionReferenceDropped {
        /// The `<junctionGroup id>`.
        group_id: String,
        /// The `junction` it names, empty if it has none.
        junction_id: String,
    },
    /// A `<junctionGroup>` whose `type` is none the spec allows, or missing,
    /// though the spec requires it. The crate reads it as `unknown`.
    UnknownJunctionGroupType {
        /// The `<junctionGroup id>`.
        group_id: String,
        /// Its `type`, empty if it has none.
        kind: String,
    },
    /// A `<crossPath>` whose `crossingRoad`, `roadAtStart` or `roadAtEnd`
    /// names no baked road, or whose lane links name a lane, or an `s`, those
    /// roads don't have. The crate drops it.
    CrossPathDropped {
        /// The `<junction id>`.
        junction_id: String,
        /// Its `id`, empty if it has none.
        cross_path_id: String,
        /// The road it can't be placed on: its `crossingRoad`, or the
        /// `roadAtStart` or `roadAtEnd` of the end that fails. Empty if the
        /// file names none.
        road_id: String,
    },
    /// A road with a `<crossSectionSurface>` and also `<shape>`s or a
    /// `<superelevation>`, which the spec forbids. The crate adds them up.
    CrossSectionWithShape {
        /// Its `<road id>`.
        road_id: String,
    },
    /// A cross-section `<strip>` the crate can't use: its `id` is not 1, 2,
    /// -1 or -2, it repeats an `id` before it, or it is an outer strip with
    /// no inner strip beside it, or one without a `<width>`. The spec needs
    /// the inner strip's width to know where the outer one starts. The crate
    /// drops it.
    StripDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// Its `id`, as the file writes it.
        strip: String,
    },
    /// An outer cross-section `<strip>` whose `mode` is neither `independent`
    /// nor `relative`. The crate reads it as `independent`. An inner strip's
    /// `mode` means nothing, and is not read.
    UnknownStripMode {
        /// The `<road id>` it is on.
        road_id: String,
        /// The strip's `id`.
        strip: i32,
        /// Its `mode`.
        mode: String,
    },
    /// A junction's `<elevationGrid>`. The spec says it overrides the height
    /// of the junction's roads, and blends into the roads coming in. The
    /// crate keeps the roads' own heights, and gives the grid's in
    /// [`JunctionArea::height_at`](crate::JunctionArea::height_at) and its
    /// [`JunctionArea::mesh`](crate::JunctionArea::mesh).
    ElevationGridNotApplied {
        /// The `<junction id>`.
        junction_id: String,
    },
    /// A junction `<boundary>` `<segment>` naming no baked road, a lane its
    /// road does not have, a `type` other than `lane` or `joint`, or an
    /// `sStart`, `sEnd` or `contactPoint` off its road. The crate drops it,
    /// so the boundary has a gap there.
    BoundarySegmentDropped {
        /// The `<junction id>`.
        junction_id: String,
        /// The segment's `roadId`, empty if it has none.
        road_id: String,
    },
    /// A junction `<elevationGrid>` the crate can't place: its junction has
    /// no `<planView>`, one that is not a single straight `<line>`, or no
    /// `gridSpacing` above 0. The crate drops it.
    ElevationGridDropped {
        /// The `<junction id>`.
        junction_id: String,
    },
    /// A junction `<boundary>` whose segments don't meet: one leaves off
    /// more than 10 cm in plan from where the next begins, or the last from
    /// where the first begins, as when it has one segment. The spec says they close the boundary. The crate joins
    /// them straight.
    BoundaryNotClosed {
        /// The `<junction id>`.
        junction_id: String,
        /// The widest gap, in metres.
        gap: f64,
    },
    /// A junction `<boundary>` whose segments run clockwise. The spec says
    /// they run counter-clockwise. The crate turns the boundary round.
    BoundaryClockwise {
        /// The `<junction id>`.
        junction_id: String,
    },
    /// A `<speed>` whose `max` or `unit` the crate can't read, or whose
    /// limit is too large for an `f32`. The crate drops it, so the lane's limit there is its road's, or the lane's
    /// `<speed>` before it.
    SpeedLimitDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// Where it starts, in metres along the reference line.
        s: f64,
        /// The `<lane id>` whose `<speed>` it is, or `None` for a road
        /// `<type>`'s.
        lane: Option<i32>,
        /// Its `max`, as the file writes it.
        max: String,
        /// Its `unit`, as the file writes it. Empty if it has none.
        unit: String,
    },
    /// A lane `<access>` whose `rule` is neither `allow` nor `deny`, so it
    /// says nothing about who may use the lane. The lane is open to
    /// everyone from its `sOffset` to the next `<access>`.
    AccessDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// Where it starts, in metres along the reference line.
        s: f64,
        /// The `<lane id>` whose `<access>` it is.
        lane: i32,
        /// Its `rule`, as the file writes it. Empty if it has none.
        rule: String,
    },
    /// A lane `<visibility>` with a distance missing, not a number, below 0,
    /// or too large for an `f32`. The lane has no visibility from its `sOffset` to the next
    /// `<visibility>`.
    VisibilityDropped {
        /// The `<road id>` it is on.
        road_id: String,
        /// Where it starts, in metres along the reference line.
        s: f64,
        /// The `<lane id>` whose `<visibility>` it is.
        lane: i32,
        /// The first distance the crate can't read: `forward`, `back`,
        /// `left` or `right`.
        distance: String,
        /// Its value, as the file writes it. Empty if it has none.
        value: String,
    },
    /// A `<lane>` without `level="true"` outside one with it. The spec
    /// says every lane outside a level lane is level too. The crate holds
    /// it level.
    LaneNotLevel {
        /// The `<road id>` it is on.
        road_id: String,
        /// The zero-based lane-section index.
        section: usize,
        /// Its `<lane id>`.
        lane: i32,
    },
}

/// Why a `<road>` baked nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum RoadSkipReason {
    /// It has no `length`, or one that is not a finite number.
    NoLength,
    /// It has no `<planView>`.
    NoPlanView,
    /// Its `<planView>` has no `<geometry>` the crate can bake: each one is
    /// missing a coordinate, carries one that is not a finite number, or has
    /// a shape the crate does not know.
    NoGeometry,
    /// Its `length` is over [`MAX_LENGTH`](crate::MAX_LENGTH), 100 km.
    TooLong,
    /// Its lanes land outside the range an `f32` holds, from a number such
    /// as an elevation or a lane width of `1e39`.
    OutOfRange,
}

impl Warning {
    /// Whether it only says the crate ignored part of the file, as
    /// [`Warning::Ignored`] does, and the file broke no rule. A map that
    /// loads clean can still raise these, so filter them out to find faults.
    pub fn is_ignored(&self) -> bool {
        matches!(self, Self::Ignored { .. })
    }

    /// The `<road id>` it happened on. Empty for a warning about a junction
    /// as a whole, such as its boundary or its elevation grid.
    pub fn road_id(&self) -> &str {
        match self {
            Self::RoadSkipped { road_id, .. }
            | Self::GeometryDropped { road_id, .. }
            | Self::LaneIdUnreadable { road_id, .. }
            | Self::TooManyCopies { road_id, .. }
            | Self::RoadMarkLineDropped { road_id, .. }
            | Self::LaneDropped { road_id, .. }
            | Self::WidthAndBorder { road_id, .. }
            | Self::BorderWithLaneOffset { road_id, .. }
            | Self::BorderCrossesInnerLane { road_id, .. }
            | Self::ShapeShortOfRoad { road_id, .. }
            | Self::UnknownTrafficRule { road_id, .. }
            | Self::UnknownLaneDirection { road_id, .. }
            | Self::SpeedLimitDropped { road_id, .. }
            | Self::AccessDropped { road_id, .. }
            | Self::VisibilityDropped { road_id, .. }
            | Self::JunctionLinkMissing { road_id, .. }
            | Self::RoadLengthMismatch { road_id, .. }
            | Self::LinkGap { road_id, .. }
            | Self::LaneNotLevel { road_id, .. }
            | Self::PriorityDropped { road_id, .. }
            | Self::NeighborDropped { road_id, .. }
            | Self::BoundarySegmentDropped { road_id, .. }
            | Self::CrossPathDropped { road_id, .. }
            | Self::CrossSectionWithShape { road_id }
            | Self::RailwayDropped { road_id, .. }
            | Self::VirtualLinkDropped { road_id, .. }
            | Self::VirtualLaneDropped { road_id, .. }
            | Self::UnknownStripMode { road_id, .. }
            | Self::StripDropped { road_id, .. }
            | Self::Ignored { road_id, .. }
            | Self::LaneLinkDropped {
                incoming_road_id: road_id,
                ..
            }
            | Self::ConnectionDropped {
                incoming_road_id: road_id,
                ..
            } => road_id,
            Self::JunctionReferenceDropped { .. }
            | Self::VirtualJunctionWithoutMainRoad { .. }
            | Self::UnknownJunctionGroupType { .. }
            | Self::ElevationGridNotApplied { .. }
            | Self::ElevationGridDropped { .. }
            | Self::BoundaryNotClosed { .. }
            | Self::BoundaryClockwise { .. } => "",
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ignored {
                road_id,
                junction_id,
                feature,
                count,
            } => {
                let place = if !road_id.is_empty() {
                    format!("road {road_id:?}")
                } else if !junction_id.is_empty() {
                    format!("junction {junction_id:?}")
                } else {
                    "the document".to_string()
                };
                let uses = if *count == 1 { "use" } else { "uses" };
                write!(f, "{place}: {feature} ignored, {count} {uses}")
            }
            Self::RoadSkipped { road_id, reason } => {
                write!(f, "road {road_id:?} skipped: {reason}")
            }
            Self::LaneDropped {
                road_id,
                section,
                lane,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} dropped, it has no usable <width> or <border>"
            ),
            Self::WidthAndBorder {
                road_id,
                section,
                lane,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} has <border>s in a section with <width>s"
            ),
            Self::BorderWithLaneOffset {
                road_id,
                section,
                lane,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} has <border>s under a <laneOffset>, which they ignore"
            ),
            Self::BorderCrossesInnerLane {
                road_id,
                section,
                lane,
                s,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane}'s <border> crosses inside the lane within it at s {s:.2} m"
            ),
            Self::ShapeShortOfRoad { road_id, s } => write!(
                f,
                "road {road_id:?}: the <shape>s at s {s:.2} m start inside the road's right edge"
            ),
            Self::UnknownTrafficRule { road_id, rule } => write!(
                f,
                "road {road_id:?}: rule {rule:?} is neither \"RHT\" nor \"LHT\", read as \"RHT\""
            ),
            Self::ConnectionDropped {
                incoming_road_id,
                junction_id,
                connection_id,
            } => write!(
                f,
                "junction {junction_id:?}: connection {connection_id:?} from road {incoming_road_id:?} dropped, it lacks the road it comes from or leads into"
            ),
            Self::SpeedLimitDropped {
                road_id,
                s,
                lane,
                max,
                unit,
            } => {
                let whose = lane.map_or("its type's".to_string(), |l| format!("lane {l}'s"));
                write!(
                    f,
                    "road {road_id:?}: {whose} <speed> at s {s:.2} m dropped, max {max:?} unit {unit:?} is not a speed"
                )
            }
            Self::AccessDropped {
                road_id,
                s,
                lane,
                rule,
            } => write!(
                f,
                "road {road_id:?}: lane {lane}'s <access> at s {s:.2} m dropped, rule {rule:?} is neither allow nor deny"
            ),
            Self::GeometryDropped { road_id, s } => match s {
                Some(s) => write!(f, "road {road_id:?}: <geometry> at s {s:.2} m dropped"),
                None => write!(f, "road {road_id:?}: <geometry> without an s dropped"),
            },
            Self::RoadMarkLineDropped { road_id, s, lane } => write!(
                f,
                "road {road_id:?}: a line of the road mark on lane {lane} at s {s:.2} m can't be painted"
            ),
            Self::TooManyCopies { road_id, object_id } => write!(
                f,
                "road {road_id:?}: object {object_id:?} would make over 100,000 copies or dashes, so they were left out"
            ),
            Self::LaneIdUnreadable {
                road_id,
                section,
                id,
            } => write!(
                f,
                "road {road_id:?}: a lane in section {section} with id {id:?} dropped, the id isn't a whole number other than 0"
            ),
            Self::LaneLinkDropped {
                incoming_road_id,
                junction_id,
                connection_id,
                from,
                to,
            } => write!(
                f,
                "junction {junction_id:?}: connection {connection_id:?} from road {incoming_road_id:?} dropped a <laneLink> from {from:?} to {to:?}, not whole numbers"
            ),
            Self::VirtualLaneDropped {
                junction_id,
                road_id,
                connection_id,
                from,
                to,
            } => write!(
                f,
                "junction {junction_id:?}: link from road {road_id:?} connection {connection_id:?} dropped lane pair {from} to {to}, a lane it names isn't there"
            ),
            Self::RoadLengthMismatch {
                road_id,
                length,
                plan_view,
            } => write!(
                f,
                "road {road_id:?}: length {length:.3} m, but its <planView> ends at {plan_view:.3} m"
            ),
            Self::LinkGap {
                road_id,
                section,
                lane,
                to_road_id,
                to_section,
                to_lane,
                gap,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} links to road {to_road_id:?}, lane section {to_section}, lane {to_lane}, {gap:.2} m away"
            ),
            Self::RailwayDropped {
                element,
                id,
                road_id,
            } => write!(f, "road {road_id:?}: {element} {id:?} dropped"),
            Self::VirtualJunctionWithoutMainRoad { junction_id } => write!(
                f,
                "junction {junction_id:?}: virtual junction read without its main road, it lacks the road or where along it it runs"
            ),
            Self::VirtualLinkDropped {
                junction_id,
                road_id,
                connection_id,
            } if road_id.is_empty() => write!(
                f,
                "junction {junction_id:?}: virtual connection {connection_id:?} dropped"
            ),
            Self::VirtualLinkDropped {
                junction_id,
                road_id,
                ..
            } => write!(
                f,
                "junction {junction_id:?}: road {road_id:?}'s link part way along a road dropped"
            ),
            Self::JunctionReferenceDropped {
                group_id,
                junction_id,
            } => write!(
                f,
                "junction group {group_id:?}: <junctionReference> to junction {junction_id:?} dropped, the file has no such junction"
            ),
            Self::UnknownJunctionGroupType { group_id, kind } => write!(
                f,
                "junction group {group_id:?}: type {kind:?} read as unknown"
            ),
            Self::CrossSectionWithShape { road_id } => write!(
                f,
                "road {road_id:?}: <crossSectionSurface> with <shape>s or a <superelevation>, heights added"
            ),
            Self::StripDropped { road_id, strip } => write!(
                f,
                "road {road_id:?}: <strip> {strip:?} dropped, its id is not 1, 2, -1 or -2, repeats one before it, or it is an outer strip with no inner strip of a width beside it"
            ),
            Self::UnknownStripMode {
                road_id,
                strip,
                mode,
            } => write!(
                f,
                "road {road_id:?}: <strip> {strip} has mode {mode:?}, read as independent"
            ),
            Self::CrossPathDropped {
                junction_id,
                cross_path_id,
                road_id,
            } => write!(
                f,
                "junction {junction_id:?}: <crossPath> {cross_path_id:?} dropped, it can't be placed on road {road_id:?}"
            ),
            Self::ElevationGridNotApplied { junction_id } => write!(
                f,
                "junction {junction_id:?}: <elevationGrid> read, but the junction's roads keep their own heights"
            ),
            Self::ElevationGridDropped { junction_id } => write!(
                f,
                "junction {junction_id:?}: <elevationGrid> dropped, it needs one straight <planView> <line> and a gridSpacing above 0"
            ),
            Self::BoundarySegmentDropped {
                junction_id,
                road_id,
            } => write!(
                f,
                "junction {junction_id:?}: <boundary> segment on road {road_id:?} dropped"
            ),
            Self::BoundaryNotClosed { junction_id, gap } => write!(
                f,
                "junction {junction_id:?}: <boundary> segments leave a {gap:.2} m gap, joined straight"
            ),
            Self::BoundaryClockwise { junction_id } => write!(
                f,
                "junction {junction_id:?}: <boundary> runs clockwise, turned round"
            ),
            Self::NeighborDropped {
                road_id,
                neighbor_id,
                side,
                direction,
            } => write!(
                f,
                "road {road_id:?}: <neighbor> road {neighbor_id:?}, side {side:?}, direction {direction:?} dropped"
            ),
            Self::PriorityDropped {
                junction_id,
                high,
                low,
                ..
            } => write!(
                f,
                "junction {junction_id:?}: <priority> of road {high:?} over road {low:?} dropped, the load has no such road"
            ),
            Self::UnknownLaneDirection {
                road_id,
                section,
                lane,
                direction,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} has direction {direction:?}, read as standard"
            ),
            Self::JunctionLinkMissing {
                road_id,
                junction_id,
                end,
            } => write!(
                f,
                "road {road_id:?}: its <link> leaves out junction {junction_id:?} at its {end}, linked from the junction's connecting road"
            ),
            Self::VisibilityDropped {
                road_id,
                s,
                lane,
                distance,
                value,
            } => write!(
                f,
                "road {road_id:?}: lane {lane}'s <visibility> at s {s:.2} m dropped, {distance} {value:?} is not a distance"
            ),
            Self::LaneNotLevel {
                road_id,
                section,
                lane,
            } => write!(
                f,
                "road {road_id:?}, lane section {section}: lane {lane} is outside a level lane but not level, held level"
            ),
        }
    }
}

/// One end of a road: where its `<predecessor>` or its `<successor>` joins
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RoadEnd {
    /// Where `s` is 0, and the `<predecessor>` joins.
    Start,
    /// Where `s` is the road's length, and the `<successor>` joins.
    End,
}

impl fmt::Display for RoadEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Start => "start",
            Self::End => "end",
        })
    }
}

impl fmt::Display for RoadSkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoLength => "no finite length",
            Self::NoPlanView => "no <planView>",
            Self::NoGeometry => "no <geometry> it can bake",
            Self::TooLong => "a length over 100 km",
            Self::OutOfRange => "lanes too far out for an f32",
        })
    }
}
