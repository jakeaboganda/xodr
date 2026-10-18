//! Railway switches and stations.

use crate::road::{RoadId, Side};

/// A railway switch, from a road's `<railroad><switch>`: where a train on
/// the main track may turn onto the side track.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Switch {
    /// The road whose `<railroad>` it is in.
    pub road: RoadId,
    /// Its `id`.
    pub od_id: String,
    /// Its `name`, empty if it has none.
    pub name: String,
    /// Which way it is set.
    pub position: SwitchPosition,
    /// Where it is on the main track.
    pub main: TrackPoint,
    /// Where the side track leaves it.
    pub side: TrackPoint,
    /// Its partner switch, such as the other end of a crossover, if it has
    /// one.
    pub partner: Option<Partner>,
}

/// A switch's `<partner>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Partner {
    /// The partner switch's `id`.
    pub id: String,
    /// The `name` the map gives the partner. Empty if it has none.
    #[cfg_attr(feature = "serde", serde(default))]
    pub name: String,
}

/// Which way a [`Switch`] is set, from its `position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum SwitchPosition {
    /// A scenario sets it.
    Dynamic,
    /// Along the main track.
    Straight,
    /// Onto the side track.
    Turn,
    /// A `position` this crate does not recognise.
    Unknown,
}

impl SwitchPosition {
    /// Its name, as the spec spells it, or `unknown`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dynamic => "dynamic",
            Self::Straight => "straight",
            Self::Turn => "turn",
            Self::Unknown => "unknown",
        }
    }
}

/// A place on a track: a road, the `s` along it, and which way along it
/// the switch leads.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackPoint {
    /// The track's road.
    pub road: RoadId,
    /// How far along its reference line.
    pub s: f64,
    /// Whether the switch leads along the road's `+s`, from `dir="+"`.
    pub forward: bool,
}

/// A railway station, from a `<station>`, and its platforms.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Station {
    /// Its `id`.
    pub od_id: String,
    /// Its `name`, empty if it has none.
    pub name: String,
    /// Its `type`: `small`, `medium` or `large`. Empty if it has none.
    pub kind: String,
    /// Its platforms, in file order.
    pub platforms: Vec<Platform>,
}

/// One `<platform>` of a [`Station`], along one or more stretches of track.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Platform {
    /// Its `id`.
    pub od_id: String,
    /// Its `name`, empty if it has none.
    pub name: String,
    /// The stretches of track it runs beside, in file order.
    pub segments: Vec<PlatformSegment>,
}

/// A stretch of track a [`Platform`] runs beside.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlatformSegment {
    /// The track's road.
    pub road: RoadId,
    /// Where the platform starts along it.
    pub s_start: f64,
    /// Where it ends.
    pub s_end: f64,
    /// Which side of the road it is on, looking along `+s`.
    pub side: Side,
}
