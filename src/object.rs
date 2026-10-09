//! Roadside objects: the poles, buildings, barriers and trees placed along a
//! road. Format-agnostic, like the lanes: an importer resolves each object to
//! a world pose, and nothing here knows where it came from.

use crate::coords::Point;
use crate::{LaneId, RoadMarkWeight};

/// `[u, v, z]` turned by yaw about Z, then pitch about the turned Y, then
/// roll about the turned X: an offset in an object's own frame, in the
/// network's axes.
pub(crate) fn orient(yaw: f64, pitch: f64, roll: f64, [u, v, z]: [f64; 3]) -> [f64; 3] {
    let (sr, cr) = roll.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let (v, z) = (v * cr - z * sr, v * sr + z * cr);
    let (u, z) = (u * cp + z * sp, -u * sp + z * cp);
    let (u, v) = (u * cy - v * sy, u * sy + v * cy);
    [u, v, z]
}

/// An opaque object identifier. Not a position in
/// [`RoadNetwork::objects`](crate::RoadNetwork::objects), so look objects up
/// with [`RoadNetwork::object`](crate::RoadNetwork::object).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectId(pub usize);

/// What an object is.
///
/// The set is the static object vocabulary maps are written in. Anything
/// else, a vendor's own name or a moving participant that older formats
/// allowed here, is [`ObjectType::Unknown`], and the object still bakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ObjectType {
    /// An object with no category assigned.
    None,
    /// Something in the way, such as a rock or a bollard.
    Obstacle,
    /// A post: a sign post, a guide post, a bollard.
    Pole,
    /// A single tree.
    Tree,
    /// Plants other than a single tree, such as a hedge.
    Vegetation,
    /// A crash barrier or guard rail.
    Barrier,
    /// A building.
    Building,
    /// A marked parking bay.
    ParkingSpace,
    /// A patch of different surface, such as a pothole repair.
    Patch,
    /// A railing.
    Railing,
    /// A raised island separating traffic.
    TrafficIsland,
    /// A pedestrian crossing.
    Crosswalk,
    /// A street light.
    StreetLamp,
    /// A gantry spanning the road.
    Gantry,
    /// A noise barrier.
    SoundBarrier,
    /// Paint on the road surface that is not a lane marking.
    RoadMark,
    /// An object on the road that traffic can pass over. The spec has it
    /// replace the deprecated `patch`.
    RoadSurface,
    /// A type this crate does not recognise.
    Unknown,
}

impl ObjectType {
    /// A stable lowercase name, for a legend, a log line, or a viewer readout.
    /// Format-neutral, so it is not necessarily the token any particular map
    /// format spells the type with.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Obstacle => "obstacle",
            Self::Pole => "pole",
            Self::Tree => "tree",
            Self::Vegetation => "vegetation",
            Self::Barrier => "barrier",
            Self::Building => "building",
            Self::ParkingSpace => "parking-space",
            Self::Patch => "patch",
            Self::Railing => "railing",
            Self::TrafficIsland => "traffic-island",
            Self::Crosswalk => "crosswalk",
            Self::StreetLamp => "street-lamp",
            Self::Gantry => "gantry",
            Self::SoundBarrier => "sound-barrier",
            Self::RoadMark => "road-mark",
            Self::RoadSurface => "road-surface",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The volume a placed object occupies, in its own frame: `length` along its
/// heading, `width` across it, `height` up from its origin.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Extent {
    /// A box centred on the origin in plan, rising from it. A dimension the
    /// map does not give is 0, so a post given only a height is a box with no
    /// footprint, and a parking bay given no height is a flat one.
    Box {
        /// Along the heading (metres).
        length: f32,
        /// Across the heading (metres).
        width: f32,
        /// Up from the origin (metres).
        height: f32,
    },
    /// An upright cylinder centred on the origin, rising from it.
    Cylinder {
        /// Metres.
        radius: f32,
        /// Up from the origin (metres).
        height: f32,
    },
}

/// One point of an outline or a sweep: where it meets the ground and where
/// its top edge is, both in the network's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Corner {
    /// The bottom of the corner.
    pub base: Point,
    /// The top of the corner. The same as `base` where the height is 0.
    pub top: Point,
}

/// One cross-section of a [`Shape::Sweep`]: its two edges at one station.
/// `left` is the edge toward +t, left of the road's reference line heading.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Section {
    /// The +t edge.
    pub left: Corner,
    /// The -t edge. The same as `left` for a sweep with no width, such as a
    /// fence.
    pub right: Corner,
}

/// Where an object is and what volume it fills.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Shape {
    /// A solid placed at a pose. The orientation is three angles in
    /// radians, in the network's axes, applied as yaw, then pitch, then roll.
    /// They take in the grade and bank of the road under the object.
    Solid {
        /// The object's origin, the base of its extent.
        position: Point,
        /// Yaw about +Z, counter-clockwise from +X.
        heading: f32,
        /// Pitch about the turned Y, nose down for a positive angle.
        pitch: f32,
        /// Roll about the turned X, left side up for a positive angle.
        roll: f32,
        /// The volume it occupies, or `None` for an object the map gives no
        /// size.
        extent: Option<Extent>,
    },
    /// A footprint polygon, walled from each corner's base to its top. Already
    /// in the network's frame, so there is no pose to apply.
    Outline {
        /// The polygon, in order. At least two.
        corners: Vec<Corner>,
        /// Whether the last corner joins the first. A closed outline encloses
        /// an area, such as a building. An open one is a line of wall, such as
        /// a fence.
        closed: bool,
        /// Rings cut out of a closed outline, such as a courtyard, each
        /// walled like the outline and at least three corners. Always empty
        /// for an open one.
        holes: Vec<Vec<Corner>>,
    },
    /// A cross-section swept along a road, such as a guard rail, a wall or
    /// a pipe. Already in the network's frame. Consecutive sections are
    /// joined by straight walls.
    Sweep {
        /// The cross-sections in order along the road. At least two, at most
        /// 10 m apart, and close enough where it bends that the walls stray
        /// under 1 cm from it.
        sections: Vec<Section>,
        /// Whether the cross-section is the ellipse inscribed in each
        /// section, rather than the section itself. A pipe's sections are
        /// squares round it.
        round: bool,
    },
}

/// Paint on an object, such as the stripes of a crosswalk: a strip along
/// edges of its outline or one side of its box, solid or dashed, centred on
/// the edges.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Marking {
    /// Which side of the object the map puts the marking on, such as `left`
    /// or `front`. Free text, and empty if the map gives none. The pieces
    /// are already placed, so nothing here needs it to place them.
    pub side: String,
    /// The colour the map names, such as `white`, or `standard` for the
    /// usual road-marking colour. Free text, and empty if the map gives none.
    pub color: String,
    /// How heavy its paint is, from `weight`. `Standard` if the map does
    /// not say, or says something else. It sets no width: `width` does.
    #[cfg_attr(feature = "serde", serde(default))]
    pub weight: RoadMarkWeight,
    /// Metres across.
    pub width: f32,
    /// The length of each painted dash, in metres.
    pub line_length: f32,
    /// The gap between dashes, in metres. With it or `line_length` 0 the
    /// strip is one solid line.
    pub space_length: f32,
    /// The painted pieces, in order along the edges, in the network's frame.
    /// Each is four corners going anticlockwise seen from above: a dash, or
    /// the part of one on a single edge. It lies in the surface under the
    /// edge, raised by the marking's `zOffset`.
    pub pieces: Vec<[Point; 4]>,
}

/// A band along edges of an object's outline, such as the kerb round a
/// traffic island, centred on the edges.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Border {
    /// What the border is, such as `curb`, `concrete` or `paint`. Free text,
    /// and empty if the map gives none.
    pub kind: String,
    /// Metres across.
    pub width: f32,
    /// The band, one piece per edge, in order along the edges, in the
    /// network's frame. Each is four corners going anticlockwise seen from
    /// above, lying in the surface under the edge.
    pub pieces: Vec<[Point; 4]>,
}

/// Who may use a parking space, and on what terms.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParkingSpace {
    /// Who may park there, such as `all`, `handicapped` or `electric`. Free
    /// text, and empty if the map gives none.
    pub access: String,
    /// Any further terms, such as a time limit. Free text, and empty if the
    /// map gives none.
    pub restrictions: String,
}

/// What an object's or a lane's surface is made of.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Material {
    /// The surface, such as `asphalt` or `concrete`. Free text, and empty if
    /// the map gives none.
    pub surface: String,
    /// The friction coefficient, if the map gives one.
    pub friction: Option<f32>,
    /// The surface roughness, in metres, if the map gives one.
    pub roughness: Option<f32>,
    /// The colour of the road marks on the surface, from an object's
    /// `roadMarkColor`, such as `white`. Free text. Empty if the map gives
    /// none, and for a lane's material, which has no such attribute.
    #[cfg_attr(feature = "serde", serde(default))]
    pub road_mark_color: String,
}

/// One piece of vendor data a map attaches to an object, kept as text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UserData {
    /// What the data is, in the vendor's own terms.
    pub code: String,
    /// The data. Empty if the map gives no value.
    pub value: String,
}

/// One object in the world.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Object {
    /// This object's identity.
    pub id: ObjectId,
    /// What the object is.
    pub kind: ObjectType,
    /// The map's finer category within `kind`, such as a kind of barrier.
    /// Free text, and empty if the map gives none.
    pub subtype: String,
    /// The name the map gives it, often a model file. Empty if it has none.
    pub name: String,
    /// Whether the object can move, such as a gate or a barrier arm. Its
    /// shape is where it stands in the map.
    pub dynamic: bool,
    /// Whether the map marks the object as no longer valid, from
    /// `invalidated`. A removed road sign is one. The crate still bakes it.
    #[cfg_attr(feature = "serde", serde(default))]
    pub invalidated: bool,
    /// Whether the object is there for a while only, such as a cone at road
    /// works, from `temporary`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub temporary: bool,
    /// The lanes the object applies to, such as the lanes a crosswalk
    /// crosses. Taken from the lanes alongside the stretch of road the object
    /// spans, and every one of them unless the map narrows it down. Empty if
    /// there are no lanes there.
    pub lanes: Vec<LaneId>,
    /// The paint on the object. Only an [`Shape::Outline`] and a
    /// [`Shape::Solid`] with an extent have any.
    pub markings: Vec<Marking>,
    /// The borders along the object's edges. Only an [`Shape::Outline`] has
    /// any.
    pub borders: Vec<Border>,
    /// Who may park there, if the map describes the object as a parking
    /// space.
    pub parking_space: Option<ParkingSpace>,
    /// What its surface is made of, in the order the map gives them. Usually
    /// one, or none.
    pub materials: Vec<Material>,
    /// Vendor data the map attaches to the object, in the order it gives it.
    pub user_data: Vec<UserData>,
    /// Where it is and what it fills. On a lane raised by its `<height>`s,
    /// it stands on the lane. The spec does not say whether it stands there
    /// or on the road below.
    pub shape: Shape,
}
