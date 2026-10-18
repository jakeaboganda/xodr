//! Tunnels and bridges, as the stretches of lanes that run through or over
//! them. Like lanes, they don't depend on the file format. A structure names
//! the lanes it covers and how far along each, and has no geometry of its
//! own.

use crate::LaneId;

/// An opaque structure identifier. Not a position in
/// [`RoadNetwork::structures`](crate::RoadNetwork::structures), so look
/// structures up with
/// [`RoadNetwork::structure`](crate::RoadNetwork::structure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StructureId(pub usize);

/// Whether a structure is a tunnel or a bridge, and what describes each.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum StructureKind {
    /// The lanes run through a tunnel.
    Tunnel {
        /// What sort of tunnel, such as `standard` or `underpass`. Free text,
        /// and empty if the map gives none.
        kind: String,
        /// How well lit it is, from 0 (dark) to 1 (fully lit), if the map
        /// says.
        lighting: Option<f32>,
        /// How much daylight gets in, from 0 to 1, if the map says.
        daylight: Option<f32>,
    },
    /// The lanes run over a bridge.
    Bridge {
        /// What it is built of, such as `concrete` or `steel`. Free text, and
        /// empty if the map gives none.
        kind: String,
    },
}

/// The part of one lane a structure covers, from `from` to `to` metres along
/// the lane's centerline. Distances start at the lane's first point, as in
/// [`Polyline::point_at`](crate::Polyline::point_at).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Coverage {
    /// The lane covered.
    pub lane: LaneId,
    /// Where the covered part starts, in metres along the lane.
    pub from: f32,
    /// Where it ends, in metres along the lane. At least `from`.
    pub to: f32,
}

/// A tunnel or a bridge, and the lanes it covers.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Structure {
    /// This structure's identity.
    pub id: StructureId,
    /// A tunnel or a bridge.
    pub kind: StructureKind,
    /// The name the map gives it. Empty if it has none.
    pub name: String,
    /// Each lane it covers, with the part of it covered. A lane appears
    /// once.
    pub lanes: Vec<Coverage>,
}
