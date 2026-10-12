//! What holds along part of a lane, such as its speed limit, the type of
//! road it is on, or who may use it. Like lanes, these don't depend on the file format.

use crate::LaneId;

/// A value that holds on part of one lane, from `from` to `to` metres along
/// its centerline. Distances start at the lane's first point, as in
/// [`Coverage`](crate::Coverage). A network leaves out a stretch whose `from`
/// or `to` is NaN, since it covers no point.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Along<T> {
    /// The lane it holds on.
    pub lane: LaneId,
    /// Where it starts, in metres along the lane.
    pub from: f32,
    /// Where it ends, in metres along the lane. Above `from`.
    pub to: f32,
    /// What holds there.
    pub value: T,
}

/// The value that holds at `s` metres along `lane`, in `list`, which is
/// sorted by lane, then by `from`. A point where two stretches meet is in
/// the later one.
pub(crate) fn at<T>(list: &[Along<T>], lane: LaneId, s: f32) -> Option<&T> {
    let end = list.partition_point(|a| (a.lane.0, a.from) <= (lane.0, s));
    let found = &list[..end].last()?;
    (found.lane == lane && s <= found.to).then_some(&found.value)
}

/// Sort `list` by lane, then by `from`, as [`at`] needs it. A stretch whose
/// `from` or `to` is NaN covers no point, so it is left out.
pub(crate) fn sorted<T>(mut list: Vec<Along<T>>) -> Vec<Along<T>> {
    list.retain(|a| !a.from.is_nan() && !a.to.is_nan());
    list.sort_by(|a, b| a.lane.0.cmp(&b.lane.0).then(a.from.total_cmp(&b.from)));
    list
}

/// The fastest traffic may drive on a lane.
///
/// An OpenDRIVE import reads it from the map's `<speed>`s and its speed
/// limit signs, which take priority. It drops a `<speed>` whose `max` or
/// `unit` the spec doesn't allow, with a warning.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SpeedLimit {
    /// At most this many metres a second.
    Max(f32),
    /// No limit, such as on a German autobahn.
    Unlimited,
}

/// Who may use a lane, from its OpenDRIVE `<access>`. Each road user is a
/// restriction type as the file names it, such as `bus` or `bicycle`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Access {
    /// Only these road users may use it.
    Allow(Vec<String>),
    /// Every road user but these may use it.
    Deny(Vec<String>),
}

/// How far a driver can see from a lane, in metres, from an OpenDRIVE
/// `<visibility>`: its four distances as the file names them.
///
/// OpenDRIVE 1.9 does not define `<visibility>`. The crate reads it as
/// CARLA does, from the older files that write it. Neither says whether
/// `forward` runs with the lane's traffic or along the road's `+s`, so the
/// crate keeps the file's four distances and turns none of them.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Visibility {
    /// Ahead.
    pub forward: f32,
    /// Behind.
    pub back: f32,
    /// To the left.
    pub left: f32,
    /// To the right.
    pub right: f32,
}

/// What a road is for, which sets the traffic rules on it.
///
/// The OpenDRIVE spec's road types. A type this crate does not recognise
/// bakes as [`RoadType::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RoadType {
    /// The map says `unknown`, or names a type the crate doesn't know.
    Unknown,
    /// A road outside towns.
    Rural,
    /// A motorway.
    Motorway,
    /// A road in a town.
    Town,
    /// A zone with a low speed limit, such as 30 km/h in Germany.
    LowSpeed,
    /// A road for pedestrians.
    Pedestrian,
    /// A road for bicycles.
    Bicycle,
    /// A town road built like a motorway.
    TownExpressway,
    /// A town road that gathers traffic from local roads.
    TownCollector,
    /// A main road through a town.
    TownArterial,
    /// A private road in a town.
    TownPrivate,
    /// A local road in a town.
    TownLocal,
    /// A play street, where children may play on the road.
    TownPlayStreet,
}

impl RoadType {
    /// A stable lowercase name, for a legend, a log line, or a viewer
    /// readout.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Rural => "rural",
            Self::Motorway => "motorway",
            Self::Town => "town",
            Self::LowSpeed => "low-speed",
            Self::Pedestrian => "pedestrian",
            Self::Bicycle => "bicycle",
            Self::TownExpressway => "town-expressway",
            Self::TownCollector => "town-collector",
            Self::TownArterial => "town-arterial",
            Self::TownPrivate => "town-private",
            Self::TownLocal => "town-local",
            Self::TownPlayStreet => "town-play-street",
        }
    }
}

impl std::fmt::Display for RoadType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stretch(lane: usize, from: f32, to: f32, value: u8) -> Along<u8> {
        Along {
            lane: LaneId(lane),
            from,
            to,
            value,
        }
    }

    #[test]
    fn at_finds_the_stretch_under_a_point() {
        let list = sorted(vec![
            stretch(2, 0.0, 5.0, 3),
            stretch(1, 4.0, 10.0, 2),
            stretch(1, 0.0, 4.0, 1),
        ]);
        assert_eq!(at(&list, LaneId(1), 0.0), Some(&1));
        assert_eq!(at(&list, LaneId(1), 4.0), Some(&2));
        assert_eq!(at(&list, LaneId(1), 10.0), Some(&2));
        assert_eq!(at(&list, LaneId(1), 10.5), None);
        assert_eq!(at(&list, LaneId(2), 1.0), Some(&3));
        assert_eq!(at(&list, LaneId(3), 1.0), None);
        assert_eq!(at(&list, LaneId(0), 1.0), None);
    }
}
