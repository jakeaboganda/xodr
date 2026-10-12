//! Signals: the traffic signs, traffic lights and road paint that tell
//! traffic what to do. Like objects, they don't depend on the file format. A
//! signal keeps the catalogue codes the map gives it and looks nothing up.

use crate::coords::Point;
use crate::{LaneId, ObjectId};

/// An opaque signal identifier. Not a position in
/// [`RoadNetwork::signals`](crate::RoadNetwork::signals), so look signals up
/// with [`RoadNetwork::signal`](crate::RoadNetwork::signal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignalId(pub usize);

/// An opaque controller identifier. Not a position in
/// [`RoadNetwork::controllers`](crate::RoadNetwork::controllers), so look
/// controllers up with
/// [`RoadNetwork::controller`](crate::RoadNetwork::controller).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ControllerId(pub usize);

/// The unit of a signal's [`value`](Signal::value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Unit {
    /// Metres, `m`.
    Metre,
    /// Kilometres, `km`.
    Kilometre,
    /// Feet, `ft`.
    Foot,
    /// Miles, `mile`.
    Mile,
    /// Metres per second, `m/s`.
    MetrePerSecond,
    /// Miles per hour, `mph`.
    MilePerHour,
    /// Kilometres per hour, `km/h`.
    KilometrePerHour,
    /// Kilograms, `kg`.
    Kilogram,
    /// Metric tons, `t`.
    Tonne,
    /// A percentage, such as a grade, `%`.
    Percent,
    /// A unit this crate does not recognise.
    Unknown,
}

impl Unit {
    /// The unit's usual symbol, such as `km/h`, or `unknown`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Metre => "m",
            Self::Kilometre => "km",
            Self::Foot => "ft",
            Self::Mile => "mile",
            Self::MetrePerSecond => "m/s",
            Self::MilePerHour => "mph",
            Self::KilometrePerHour => "km/h",
            Self::Kilogram => "kg",
            Self::Tonne => "t",
            Self::Percent => "%",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One signal: what it means, the lanes it applies to, and the board it is.
///
/// Where a signal applies and where it stands can differ. A sign on a gantry
/// over one road can apply to another. `lanes` and `applies_at` say where it
/// applies, and the pose says where the board is.
///
/// The board is a box in its own frame: `length` along its heading, `width`
/// across it, and `height` up from its origin. The origin is the middle of
/// its bottom edge.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Signal {
    /// This signal's identity.
    pub id: SignalId,
    /// The name the map gives it. Empty if it has none.
    pub name: String,
    /// Whether it can change what it shows, such as a traffic light.
    pub dynamic: bool,
    /// The country whose catalogue `kind` and `subtype` come from, such as
    /// `DE`. Free text, since maps also use names such as `OpenDRIVE` for a
    /// generic catalogue. Empty if the map gives none.
    pub country: String,
    /// The year of that country's rules. Empty if the map gives none.
    pub country_revision: String,
    /// The signal's type in the country's catalogue, such as `274` for a
    /// German speed limit. Free text.
    pub kind: String,
    /// The subtype within `kind`. Free text, and often `-1` for none.
    pub subtype: String,
    /// A number the signal shows, such as a speed limit.
    pub value: Option<f64>,
    /// The unit of `value`, if the map gives one.
    pub unit: Option<Unit>,
    /// Text the signal shows, such as a town name. Empty if it has none.
    pub text: String,
    /// Whether the signal has been struck out, such as a crossed-out sign.
    pub invalidated: bool,
    /// Whether the signal is temporary, such as a sign at road works.
    pub temporary: bool,
    /// The controllers the signal belongs to. Usually none, or one.
    pub controllers: Vec<ControllerId>,
    /// The signals this one is linked to by a dependency, such as the plate
    /// that limits a speed sign to lorries.
    pub dependencies: Vec<Dependency>,
    /// The signals and objects this one relates to, such as the stop line of
    /// a traffic light.
    pub references: Vec<Reference>,
    /// The lanes the signal applies to, on every road the map applies it
    /// to. A lane appears once. Without a `<validity>`, these are the lanes
    /// whose traffic runs the way the signal's `orientation` names.
    pub lanes: Vec<LaneId>,
    /// The points on the road surface where the signal takes effect: its
    /// own first, then one for each further place the map applies it. On a
    /// lane raised by its `<height>`s, a point is on the lane.
    pub applies_at: Vec<Point>,
    /// The middle of the board's bottom edge. On a lane raised by its
    /// `<height>`s, the board stands on the lane. The spec does not say
    /// whether it stands there or on the road below.
    pub position: Point,
    /// The direction the board faces, toward the traffic it addresses: yaw
    /// about +Z, counter-clockwise from +X, in radians in `(-π, π]`.
    pub heading: f32,
    /// Pitch about the turned Y, in radians, nose down for a positive angle.
    pub pitch: f32,
    /// Roll about the turned X, in radians, left side up for a positive
    /// angle.
    pub roll: f32,
    /// The board's thickness along its heading, in metres, if the map gives
    /// it.
    pub length: Option<f32>,
    /// Metres across, if the map gives it.
    pub width: Option<f32>,
    /// Metres up from the origin, if the map gives it.
    pub height: Option<f32>,
    /// What the signal means, from its `<semantics>`, in file order. Empty
    /// where the map says only its catalogue codes.
    pub semantics: Vec<Semantic>,
    /// The boards the signal is, from its `<staticBoard>`s and
    /// `<vmsBoard>`s: a gantry of signs, or a variable message display.
    pub boards: Vec<SignalBoard>,
}

/// One thing a signal means, from an OpenDRIVE `<semantics>` child. Each
/// `kind` is the file's `type`, such as `maximum` for a speed, empty if it
/// gives none.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Semantic {
    /// A speed rule, such as a limit.
    Speed {
        /// Its `type`, such as `maximum` or `zoneBegin`.
        kind: String,
        /// The speed.
        value: Option<f64>,
        /// Its unit.
        unit: Option<Unit>,
    },
    /// A lane rule, such as no overtaking.
    Lane {
        /// Its `type`.
        kind: String,
    },
    /// A priority rule, such as a stop line or right of way.
    Priority {
        /// Its `type`.
        kind: String,
    },
    /// The road users who may not enter.
    Prohibited(Vec<RoadUser>),
    /// A warning.
    Warning,
    /// Routing information.
    Routing,
    /// A street's name.
    StreetName,
    /// A parking rule.
    Parking,
    /// Tourist information.
    Tourist,
    /// When another sign applies. Means nothing on its own.
    SupplementaryTime {
        /// Its `type`.
        kind: String,
        /// Its `value`.
        value: Option<f64>,
    },
    /// Who another sign does not apply to. Means nothing on its own.
    SupplementaryAllows(Vec<RoadUser>),
    /// Who another sign applies to only. Means nothing on its own.
    SupplementaryProhibits(Vec<RoadUser>),
    /// How far off or along another sign applies. Means nothing on its own.
    SupplementaryDistance {
        /// Its `type`.
        kind: String,
        /// The distance.
        value: Option<f64>,
        /// Its unit.
        unit: Option<Unit>,
    },
    /// In what weather another sign applies. Means nothing on its own.
    SupplementaryEnvironment {
        /// Its `type`.
        kind: String,
    },
    /// An explanation of another sign. Means nothing on its own.
    SupplementaryExplanatory,
}

/// A road user a [`Semantic`] names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RoadUser {
    /// An animal.
    Animal,
    /// A person, of the category the file names, such as `pedestrian`.
    Person(String),
    /// A vehicle, of the category the file names, such as `car`.
    Vehicle(String),
}

/// A board a signal is, from a `<staticBoard>` or a `<vmsBoard>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SignalBoard {
    /// A board of fixed signs, such as a gantry, with each of its signs.
    Static(Vec<BoardSign>),
    /// A variable message board, which shows what a scenario tells it to.
    Message(MessageBoard),
}

/// One `<sign>` on a static board: its catalogue codes and meaning, as on a
/// [`Signal`], and where it is.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BoardSign {
    /// The name the map gives it. Empty if it has none.
    pub name: String,
    /// As [`Signal::country`].
    pub country: String,
    /// As [`Signal::kind`].
    pub kind: String,
    /// As [`Signal::subtype`].
    pub subtype: String,
    /// As [`Signal::value`].
    pub value: Option<f64>,
    /// As [`Signal::unit`].
    pub unit: Option<Unit>,
    /// As [`Signal::text`].
    pub text: String,
    /// What it means.
    pub semantics: Vec<Semantic>,
    /// Where on the board it is, `v` across the board and `z` up it, turned
    /// with the board: the middle of its bottom edge.
    pub position: Point,
    /// Metres across, if the map gives it.
    pub width: Option<f32>,
    /// Metres up, if the map gives it.
    pub height: Option<f32>,
    /// The lanes it applies to: those of its own `<validity>`, which
    /// overrides the signal's, as the spec says, or the signal's
    /// [`Signal::lanes`] if it has none. Its own validity applies where the
    /// signal stands, not where a `<signalReference>` applies the signal too.
    pub lanes: Vec<LaneId>,
    /// What it depends on: its own `<dependency>`s, which override the
    /// signal's, or the signal's if it has none.
    pub dependencies: Vec<SignDependency>,
    /// What it relates to: its own `<reference>`s, which override the
    /// signal's, or the signal's [`Signal::references`] if it has none.
    pub references: Vec<Reference>,
}

/// A dependency of a [`BoardSign`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignDependency {
    /// What it names.
    pub to: SignTarget,
    /// What the dependency is. Free text, and empty if the map gives none.
    pub kind: String,
}

/// What a [`SignDependency`] names: a signal, or a sign on the same board,
/// as the spec's example of a speed sign and the plates under it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SignTarget {
    /// A signal.
    Signal(SignalId),
    /// The sign at this position among the same board's signs.
    Sign(usize),
}

/// A `<vmsBoard>`: a display that shows what a scenario tells it to.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MessageBoard {
    /// Its `displayType`, such as `LED`. Empty if it has none.
    pub display: String,
    /// Where on the signal it is, as a [`BoardSign`]'s position.
    pub position: Point,
    /// Metres across, if the map gives it.
    pub width: Option<f32>,
    /// Metres up, if the map gives it.
    pub height: Option<f32>,
    /// The places on it a signal can be shown, in file order.
    pub areas: Vec<DisplayArea>,
}

/// A `<displayArea>` of a [`MessageBoard`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DisplayArea {
    /// Its `index`, if the map gives one.
    pub index: Option<i32>,
    /// Where on the board it is, as a [`BoardSign`]'s position.
    pub position: Point,
    /// Metres across, if the map gives it.
    pub width: Option<f32>,
    /// Metres up, if the map gives it.
    pub height: Option<f32>,
    /// The lanes it applies to, as a [`BoardSign`]'s, from its own
    /// `<validity>`. The spec's text gives a display area one, though its
    /// schema does not, and the crate reads it.
    pub lanes: Vec<LaneId>,
    /// What it depends on: its own `<dependency>`s, which override the
    /// signal's, or the signal's [`Signal::dependencies`] if it has none.
    /// Like its validity, from the spec's text.
    pub dependencies: Vec<Dependency>,
}

/// The variable message boards on one gantry, from a `<vmsGroup>`, which a
/// scenario treats as one display.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VmsGroup {
    /// Its `id`, empty if it has none.
    pub od_id: String,
    /// Each board in it, from its `<vmsBoardReference>`s, in file order.
    pub boards: Vec<VmsBoardReference>,
}

/// One board of a [`VmsGroup`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VmsBoardReference {
    /// The signal the board is on.
    pub signal: SignalId,
    /// Which of the signal's boards it is, its `vmsIndex`, as the file
    /// numbers them, if the file gives a whole number.
    pub vms_index: Option<i32>,
    /// Its place in the group, its `groupIndex`, if the file gives a whole
    /// number.
    pub group_index: Option<i32>,
}

/// A group of signals that always show the same state, such as the lights of
/// one approach to a junction. The map does not say what they show, or when.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Controller {
    /// This controller's identity.
    pub id: ControllerId,
    /// The name the map gives it. Empty if it has none.
    pub name: String,
    /// Its priority among the other controllers, if the map gives one.
    pub sequence: Option<u32>,
    /// The signals it controls, in the order the map gives them.
    pub signals: Vec<Control>,
}

/// One signal a [`Controller`] controls.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Control {
    /// The signal.
    pub signal: SignalId,
    /// How the controller controls it. Free text, and empty if the map gives
    /// none.
    pub kind: String,
}

/// A signal another depends on.
///
/// The map's `<dependency>` names it. OpenDRIVE 1.7 calls it the signal
/// controlled, and 1.9 the one controlling, so this keeps the link as the
/// map gives it and reads nothing into its direction.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Dependency {
    /// The signal named.
    pub signal: SignalId,
    /// What the dependency is. Free text, and empty if the map gives none.
    pub kind: String,
}

/// Something a signal relates to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Reference {
    /// The signal or object.
    pub to: Referenced,
    /// What the relation is, such as `stopline`. Free text, and empty if the
    /// map gives none.
    pub kind: String,
}

/// What a [`Reference`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Referenced {
    /// A signal.
    Signal(SignalId),
    /// An object.
    Object(ObjectId),
}
