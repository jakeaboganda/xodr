//! Additional data: the `<userData>` and `<dataQuality>` a file may put on
//! any element, and the header's `<license>`.

use super::attr_f64;
use crate::header::License;

/// One `<userData>` or `<dataQuality>`, with where in the file it is.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdditionalData {
    /// The element it is on, from the document's root: each element's name,
    /// with its `id` in brackets where it has one, or else its place among
    /// its parent's elements of that name where there is more than one, such
    /// as `road[7]/lanes/laneSection[1]/right/lane[-1]`. Empty for one on
    /// the root.
    pub path: String,
    /// The `<road id>` it is in, or empty outside a road.
    pub road_id: String,
    /// What it says.
    pub data: Additional,
}

/// What an [`AdditionalData`] says.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Additional {
    /// A `<userData>`: data for an application, in its own terms.
    UserData {
        /// Its `code`. Empty if it has none.
        code: String,
        /// Its `value`. Empty if it has none.
        value: String,
    },
    /// A `<dataQuality>`: how good the measured data behind the element is.
    DataQuality(DataQuality),
}

/// A `<dataQuality>`: its `<error>` and `<rawData>`, where it gives them.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DataQuality {
    /// Its `<error>`.
    pub error: Option<DataError>,
    /// Its `<rawData>`.
    pub raw_data: Option<RawData>,
}

/// The error range of measured data, in metres, from an `<error>`. A
/// number the file leaves out or can't give is `None`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DataError {
    /// `xyAbsolute`.
    pub xy_absolute: Option<f64>,
    /// `xyRelative`, between neighbouring entries.
    pub xy_relative: Option<f64>,
    /// `zAbsolute`.
    pub z_absolute: Option<f64>,
    /// `zRelative`, between neighbouring entries.
    pub z_relative: Option<f64>,
}

/// Where measured data came from, from a `<rawData>`, as the file writes
/// it. A value it leaves out is empty.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawData {
    /// `date`, in ISO 8601.
    pub date: String,
    /// `source`, such as `sensor` or `cadaster`.
    pub source: String,
    /// `sourceComment`.
    pub source_comment: String,
    /// `postProcessing`, such as `raw` or `fused`.
    pub post_processing: String,
    /// `postProcessingComment`.
    pub post_processing_comment: String,
}

/// Every `<userData>` and `<dataQuality>` under `root`, in file order, but
/// those inside a `<userData>`, which holds a vendor's own elements.
pub(super) fn collect(root: roxmltree::Node) -> Vec<AdditionalData> {
    let mut out = Vec::new();
    let text = |n: roxmltree::Node, name| n.attribute(name).unwrap_or_default().to_string();
    let inside_user_data =
        |n: &roxmltree::Node| n.ancestors().skip(1).any(|a| a.has_tag_name("userData"));
    for node in root
        .descendants()
        .filter(|n| n.is_element() && !inside_user_data(n))
    {
        let data = if node.has_tag_name("userData") {
            Additional::UserData {
                code: text(node, "code"),
                value: text(node, "value"),
            }
        } else if node.has_tag_name("dataQuality") {
            Additional::DataQuality(data_quality(node))
        } else {
            continue;
        };
        let Some(owner) = node.parent_element() else {
            continue;
        };
        let road_id = owner
            .ancestors()
            .find(|a| a.has_tag_name("road"))
            .map_or_else(String::new, |r| text(r, "id"));
        out.push(AdditionalData {
            path: path(root, owner),
            road_id,
            data,
        });
    }
    out
}

/// A `<dataQuality>`'s `<error>` and `<rawData>`.
fn data_quality(node: roxmltree::Node) -> DataQuality {
    let child = |tag| node.children().find(|n| n.has_tag_name(tag));
    let text = |n: roxmltree::Node, name| n.attribute(name).unwrap_or_default().to_string();
    DataQuality {
        error: child("error").map(|e| DataError {
            xy_absolute: attr_f64(e, "xyAbsolute"),
            xy_relative: attr_f64(e, "xyRelative"),
            z_absolute: attr_f64(e, "zAbsolute"),
            z_relative: attr_f64(e, "zRelative"),
        }),
        raw_data: child("rawData").map(|r| RawData {
            date: text(r, "date"),
            source: text(r, "source"),
            source_comment: text(r, "sourceComment"),
            post_processing: text(r, "postProcessing"),
            post_processing_comment: text(r, "postProcessingComment"),
        }),
    }
}

/// `node`'s path from `root`, as [`AdditionalData::path`] gives it.
fn path(root: roxmltree::Node, node: roxmltree::Node) -> String {
    let mut steps: Vec<String> = node
        .ancestors()
        .take_while(|a| *a != root)
        .filter(|a| a.is_element())
        .map(|a| {
            let name = a.tag_name().name();
            if let Some(id) = a.attribute("id") {
                return format!("{name}[{id}]");
            }
            let parent = a.parent_element();
            let same = |n: &roxmltree::Node| n.is_element() && n.tag_name().name() == name;
            let siblings = parent.map_or(1, |p| p.children().filter(same).count());
            match siblings > 1 {
                true => {
                    let place = a.prev_siblings().filter(same).count() - 1;
                    format!("{name}[{place}]")
                }
                false => name.to_string(),
            }
        })
        .collect();
    steps.reverse();
    steps.join("/")
}

/// A header's `<license>`, or `None` without one.
pub(super) fn license(header: roxmltree::Node) -> Option<License> {
    let node = header.children().find(|n| n.has_tag_name("license"))?;
    let text = |name| node.attribute(name).unwrap_or_default().to_string();
    Some(License {
        name: text("name"),
        resource: text("resource"),
        spdx_id: text("spdxid"),
        text: text("text"),
    })
}
