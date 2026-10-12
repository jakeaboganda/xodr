//! What a map says about itself in its `<header>`.

use crate::{RoadType, Semantic};

/// A map's `<header>`, as the file gives it. The crate reads none of it to
/// decide how to read the map, so the version in particular changes nothing
/// about what bakes. See [Which OpenDRIVE version](crate#which-opendrive-version).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
#[non_exhaustive]
pub struct Header {
    /// The `revMajor`, or `None` if the file has none or it isn't a whole
    /// number.
    pub rev_major: Option<u32>,
    /// The `revMinor`, read as `revMajor` is.
    pub rev_minor: Option<u32>,
    /// The map's `name`. Empty if it has none.
    pub name: String,
    /// The map's own `version`, not the format's. Empty if it has none.
    pub version: String,
    /// The `date` it was made, as the file writes it. Empty if it has none.
    pub date: String,
    /// The tool that wrote it, from `vendor`. Empty if it has none.
    pub vendor: String,
    /// The `north` edge of the map's bounds, in the file's frame, if the file
    /// gives one.
    pub north: Option<f64>,
    /// The `south` edge, as `north`.
    pub south: Option<f64>,
    /// The `east` edge, as `north`.
    pub east: Option<f64>,
    /// The `west` edge, as `north`.
    pub west: Option<f64>,
    /// The rules the map sets once for all its roads and signs, from its
    /// `<defaultRegulations>`.
    pub default_regulations: DefaultRegulations,
}

/// The rules a map sets once for all its roads and signs, from its
/// `<defaultRegulations>`, such as the speed limit on a rural road where it
/// is drawn. A road's, a lane's or a signal's own rules override them.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct DefaultRegulations {
    /// The rules for each type of road, from its `<roadRegulations>`, in
    /// file order.
    pub roads: Vec<RoadRegulation>,
    /// The rules for each type of sign, from its `<signalRegulations>`, in
    /// file order, such as whether traffic may turn right on red.
    pub signals: Vec<SignalRegulation>,
}

/// The rules for one type of road, from a `<roadRegulations>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoadRegulation {
    /// The type of road, its `type`.
    pub road_type: RoadType,
    /// The rules, from its `<semantics>`, in file order.
    pub semantics: Vec<Semantic>,
}

/// The rules for one type of sign, from a `<signalRegulations>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignalRegulation {
    /// The sign's `type`, as a [`Signal::kind`](crate::Signal::kind).
    pub kind: String,
    /// Its `subtype`, as a [`Signal::subtype`](crate::Signal::subtype).
    pub subtype: String,
    /// The rules, from its `<semantics>`, in file order.
    pub semantics: Vec<Semantic>,
}
