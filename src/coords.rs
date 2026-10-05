//! The two coordinate types: a place, and a displacement.
//!
//! Every geometry library reaches for one three-float vector and uses it for
//! positions, directions and surface normals alike. That makes
//! `nearest_lane(sample.up)` compile, and `a + b` on two positions compile,
//! neither of which means anything.
//!
//! So a position is a [`Point`] and everything else is a [`Vector`], with the
//! arithmetic that relates them and no arithmetic that does not. Subtracting
//! two places gives the displacement between them; adding a displacement to a
//! place gives another place; adding two places is not defined.
//!
//! Both are plain `#[repr(C)]` structs of three `f32`s with public fields, so
//! handing one to a renderer or another math library is a field read or
//! [`Point::to_array`].

use std::ops::{Add, Mul, Neg, Sub};

/// A place in the world: right-handed, Z-up, metres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct Point {
    /// Easting, metres.
    pub x: f32,
    /// Northing, metres. Positive is left of a +X heading.
    pub y: f32,
    /// Elevation, metres.
    pub z: f32,
}

/// A displacement between places, or a direction. Metres, or dimensionless
/// when it is a unit direction.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct Vector {
    /// Easting component.
    pub x: f32,
    /// Northing component.
    pub y: f32,
    /// Vertical component.
    pub z: f32,
}

impl Point {
    /// The coordinate origin.
    pub const ORIGIN: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// A place at these coordinates.
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// The same value on all three axes.
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v, v)
    }

    /// Read from `[x, y, z]`.
    pub const fn from_array(a: [f32; 3]) -> Self {
        Self::new(a[0], a[1], a[2])
    }

    /// Write to `[x, y, z]`, the handoff to a vertex buffer or another math
    /// library.
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    /// Whether every component is finite. False for any `NaN` or infinity.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Straight-line distance to `other`.
    pub fn distance_to(self, other: Self) -> f32 {
        (other - self).length()
    }

    /// Squared straight-line distance to `other`, skipping the square root.
    pub fn distance_squared_to(self, other: Self) -> f32 {
        (other - self).length_squared()
    }

    /// The place `t` of the way from here to `other`. `t` is not clamped.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }

    /// Whether every component is within `tolerance` of `other`'s.
    pub fn abs_diff_eq(self, other: Self, tolerance: f32) -> bool {
        (self - other).abs_diff_eq(Vector::ZERO, tolerance)
    }

    /// The displacement from the origin to here.
    pub const fn to_vector(self) -> Vector {
        Vector::new(self.x, self.y, self.z)
    }
}

impl Vector {
    /// No displacement.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    /// The unit vector along +X.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// The unit vector along +Y.
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// The unit vector along +Z, which is up.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    /// A displacement with these components.
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// The same value on all three axes.
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v, v)
    }

    /// Read from `[x, y, z]`.
    pub const fn from_array(a: [f32; 3]) -> Self {
        Self::new(a[0], a[1], a[2])
    }

    /// Write to `[x, y, z]`.
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    /// Whether every component is finite. False for any `NaN` or infinity.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Length, in whatever unit the components carry.
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Squared length, skipping the square root.
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Dot product.
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Cross product, right-handed: `X.cross(Y) == Z`.
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// This direction at unit length, or `fallback` if it has no direction to
    /// speak of (zero length, or non-finite).
    ///
    /// There is no partial `normalize`: a zero vector has no unit form, and a
    /// function that answers anyway is one more `NaN` waiting to reach the
    /// caller.
    pub fn normalize_or(self, fallback: Self) -> Self {
        let len = self.length();
        if len.is_finite() && len > 0.0 {
            self * (1.0 / len)
        } else {
            fallback
        }
    }

    /// This direction at unit length, or [`Vector::ZERO`] if it has none.
    pub fn normalize_or_zero(self) -> Self {
        self.normalize_or(Self::ZERO)
    }

    /// Whether this is within a millionth of unit length.
    pub fn is_normalized(self) -> bool {
        (self.length_squared() - 1.0).abs() < 1e-6
    }

    /// The displacement `t` of the way from here to `other`. `t` is not
    /// clamped.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }

    /// Whether every component is within `tolerance` of `other`'s.
    pub fn abs_diff_eq(self, other: Self, tolerance: f32) -> bool {
        (self.x - other.x).abs() <= tolerance
            && (self.y - other.y).abs() <= tolerance
            && (self.z - other.z).abs() <= tolerance
    }

    /// This vector turned `radians` about `axis`, right-handed.
    ///
    /// Rodrigues' rotation. `axis` is normalized first, and an axis with no
    /// direction turns nothing rather than returning `NaN`.
    pub fn rotate_about(self, axis: Self, radians: f32) -> Self {
        let k = axis.normalize_or_zero();
        if k == Self::ZERO {
            return self;
        }
        let (sin, cos) = radians.sin_cos();
        self * cos + k.cross(self) * sin + k * (k.dot(self) * (1.0 - cos))
    }

    /// The place reached by applying this displacement to the origin.
    pub const fn to_point(self) -> Point {
        Point::new(self.x, self.y, self.z)
    }
}

// --- Arithmetic -------------------------------------------------------------
//
// Only the operations that mean something: a place plus a displacement is a
// place, two places differ by a displacement, and displacements form a vector
// space. Adding two places is deliberately absent.

impl Sub for Point {
    type Output = Vector;
    fn sub(self, other: Self) -> Vector {
        Vector::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Add<Vector> for Point {
    type Output = Point;
    fn add(self, v: Vector) -> Point {
        Point::new(self.x + v.x, self.y + v.y, self.z + v.z)
    }
}

impl Sub<Vector> for Point {
    type Output = Point;
    fn sub(self, v: Vector) -> Point {
        Point::new(self.x - v.x, self.y - v.y, self.z - v.z)
    }
}

impl Add for Vector {
    type Output = Vector;
    fn add(self, v: Self) -> Self {
        Self::new(self.x + v.x, self.y + v.y, self.z + v.z)
    }
}

impl Sub for Vector {
    type Output = Vector;
    fn sub(self, v: Self) -> Self {
        Self::new(self.x - v.x, self.y - v.y, self.z - v.z)
    }
}

impl Mul<f32> for Vector {
    type Output = Vector;
    fn mul(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k)
    }
}

impl Mul<Vector> for f32 {
    type Output = Vector;
    fn mul(self, v: Vector) -> Vector {
        v * self
    }
}

impl Neg for Vector {
    type Output = Vector;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl From<[f32; 3]> for Point {
    fn from(a: [f32; 3]) -> Self {
        Self::from_array(a)
    }
}

impl From<Point> for [f32; 3] {
    fn from(p: Point) -> Self {
        p.to_array()
    }
}

impl From<[f32; 3]> for Vector {
    fn from(a: [f32; 3]) -> Self {
        Self::from_array(a)
    }
}

impl From<Vector> for [f32; 3] {
    fn from(v: Vector) -> Self {
        v.to_array()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn places_and_displacements_compose() {
        let a = Point::new(1.0, 2.0, 3.0);
        let b = Point::new(4.0, 6.0, 3.0);
        // Two places differ by a displacement.
        assert_eq!(b - a, Vector::new(3.0, 4.0, 0.0));
        assert_eq!((b - a).length(), 5.0);
        // Applying that displacement to the first place reaches the second.
        assert_eq!(a + (b - a), b);
        assert_eq!(b - (b - a), a);
        assert_eq!(a.distance_to(b), 5.0);
        assert_eq!(a.distance_squared_to(b), 25.0);
    }

    #[test]
    fn lerp_interpolates_and_extrapolates() {
        let a = Point::new(0.0, 0.0, 0.0);
        let b = Point::new(10.0, 0.0, 0.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.25), Point::new(2.5, 0.0, 0.0));
        // Not clamped: a sampler that walks past the end gets a sane answer.
        assert_eq!(a.lerp(b, 2.0), Point::new(20.0, 0.0, 0.0));
        assert_eq!(Vector::X.lerp(Vector::Y, 0.5), Vector::new(0.5, 0.5, 0.0));
    }

    #[test]
    fn the_cross_product_is_right_handed() {
        assert_eq!(Vector::X.cross(Vector::Y), Vector::Z);
        assert_eq!(Vector::Y.cross(Vector::Z), Vector::X);
        assert_eq!(Vector::Z.cross(Vector::X), Vector::Y);
        // Anti-commutative, and zero against itself.
        assert_eq!(Vector::Y.cross(Vector::X), -Vector::Z);
        assert_eq!(Vector::X.cross(Vector::X), Vector::ZERO);
    }

    #[test]
    fn dot_measures_alignment() {
        assert_eq!(Vector::X.dot(Vector::X), 1.0);
        assert_eq!(Vector::X.dot(Vector::Y), 0.0);
        assert_eq!(Vector::X.dot(-Vector::X), -1.0);
        assert_eq!(Vector::new(3.0, 4.0, 0.0).length_squared(), 25.0);
    }

    #[test]
    fn normalizing_is_total() {
        let v = Vector::new(0.0, 3.0, 4.0);
        assert!(v
            .normalize_or_zero()
            .abs_diff_eq(Vector::new(0.0, 0.6, 0.8), 1e-6));
        assert!(v.normalize_or_zero().is_normalized());
        // A vector with no direction has no unit form, so the fallback stands
        // in rather than a NaN reaching the caller.
        assert_eq!(Vector::ZERO.normalize_or_zero(), Vector::ZERO);
        assert_eq!(Vector::ZERO.normalize_or(Vector::Z), Vector::Z);
        for bad in [f32::NAN, f32::INFINITY, -f32::INFINITY] {
            assert_eq!(Vector::splat(bad).normalize_or(Vector::Z), Vector::Z);
            assert_eq!(
                Vector::new(bad, 0.0, 0.0).normalize_or(Vector::Z),
                Vector::Z
            );
        }
    }

    #[test]
    fn rotate_about_turns_right_handed_and_keeps_length() {
        // +Y about +X by 90 degrees is +Z.
        let r = Vector::Y.rotate_about(Vector::X, FRAC_PI_2);
        assert!(r.abs_diff_eq(Vector::Z, 1e-6), "{r:?}");
        // +Z about +X by 90 degrees is -Y.
        let r = Vector::Z.rotate_about(Vector::X, FRAC_PI_2);
        assert!(r.abs_diff_eq(-Vector::Y, 1e-6), "{r:?}");
        // A turn about the vector's own axis changes nothing.
        assert!(Vector::X
            .rotate_about(Vector::X, 1.234)
            .abs_diff_eq(Vector::X, 1e-6));
        // Length is preserved, and an unnormalized axis is normalized first.
        let v = Vector::new(1.0, 2.0, -3.0);
        let turned = v.rotate_about(Vector::new(0.0, 0.0, 7.0), 0.9);
        assert!((turned.length() - v.length()).abs() < 1e-5);
        // An axis with no direction turns nothing, rather than yielding NaN.
        assert_eq!(v.rotate_about(Vector::ZERO, 0.9), v);
    }

    #[test]
    fn scalar_multiplication_works_from_either_side() {
        let v = Vector::new(1.0, -2.0, 3.0);
        assert_eq!(v * 2.0, Vector::new(2.0, -4.0, 6.0));
        assert_eq!(2.0 * v, v * 2.0);
        assert_eq!(-v, v * -1.0);
    }

    #[test]
    fn arrays_round_trip_for_interop() {
        // Handing a place to a renderer or another math library is one call.
        let p = Point::new(1.5, -2.5, 3.5);
        assert_eq!(p.to_array(), [1.5, -2.5, 3.5]);
        assert_eq!(Point::from_array(p.to_array()), p);
        assert_eq!(Point::from([1.5, -2.5, 3.5]), p);
        assert_eq!(<[f32; 3]>::from(p), [1.5, -2.5, 3.5]);

        let v = Vector::new(1.5, -2.5, 3.5);
        assert_eq!(Vector::from_array(v.to_array()), v);
        assert_eq!(<[f32; 3]>::from(v), [1.5, -2.5, 3.5]);
        assert_eq!(p.to_vector(), v);
        assert_eq!(v.to_point(), p);
    }

    #[test]
    fn finiteness_and_approximate_equality() {
        assert!(Point::new(1.0, 2.0, 3.0).is_finite());
        assert!(!Point::new(1.0, f32::NAN, 3.0).is_finite());
        assert!(!Vector::new(f32::INFINITY, 0.0, 0.0).is_finite());

        let a = Point::new(1.0, 2.0, 3.0);
        assert!(a.abs_diff_eq(Point::new(1.0, 2.0, 3.000_05), 1e-4));
        assert!(!a.abs_diff_eq(Point::new(1.0, 2.0, 3.01), 1e-4));
    }

    #[test]
    fn the_origin_and_the_axes_are_what_they_say() {
        assert_eq!(Point::splat(2.0), Point::new(2.0, 2.0, 2.0));
        assert_eq!(Point::ORIGIN + Vector::Z, Point::new(0.0, 0.0, 1.0));
        assert_eq!(Vector::X + Vector::Y + Vector::Z, Vector::splat(1.0));
        assert_eq!(Vector::X - Vector::X, Vector::ZERO);
    }
}
