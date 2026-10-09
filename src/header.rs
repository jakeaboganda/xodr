//! What a map says about itself in its `<header>`.

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
}
