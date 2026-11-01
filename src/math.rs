//! Transcendental functions through `libm`.
//!
//! Platform math libraries differ in the last bits of `ln`, `exp`, `sin` and
//! friends, and that can flip how a serialized coordinate rounds. `libm` is
//! pure Rust, so every platform computes the same bits. The
//! square root and the basic operators are correctly rounded by IEEE 754 and
//! need no wrapper.

/// A full turn in radians.
pub const TAU: f64 = core::f64::consts::TAU;

/// The natural logarithm.
#[inline]
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// `e` to the power `x`.
#[inline]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

/// The sine of `x` radians.
#[inline]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

/// The cosine of `x` radians.
#[inline]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// The angle of the point `(x, y)` in radians, in `(-pi, pi]`.
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

/// The cube root, defined for negative inputs too.
#[inline]
pub fn cbrt(x: f64) -> f64 {
    libm::cbrt(x)
}

/// `x` to the power `y`.
#[inline]
pub fn pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

/// Degrees to radians.
#[inline]
pub fn rad(degrees: f64) -> f64 {
    degrees * (core::f64::consts::PI / 180.0)
}

/// Radians to degrees.
#[inline]
pub fn deg(radians: f64) -> f64 {
    radians * (180.0 / core::f64::consts::PI)
}

/// Linear interpolation from `a` (at `t = 0`) to `b` (at `t = 1`).
#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// An angle in degrees, folded into `[0, 360)`.
#[inline]
pub fn wrap_degrees(degrees: f64) -> f64 {
    degrees.rem_euclid(360.0)
}

/// The smallest distance between two angles in degrees, in `[0, 180]`.
#[inline]
pub fn angle_between(a: f64, b: f64) -> f64 {
    let d = wrap_degrees(a - b);
    if d > 180.0 { 360.0 - d } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_fold_and_compare_the_short_way_round() {
        assert_eq!(wrap_degrees(-30.0), 330.0);
        assert_eq!(wrap_degrees(725.0), 5.0);
        assert_eq!(angle_between(350.0, 10.0), 20.0);
        assert_eq!(angle_between(90.0, 270.0), 180.0);
    }

    #[test]
    fn the_wrappers_agree_with_the_textbook_values() {
        assert!((ln(core::f64::consts::E) - 1.0).abs() < 1e-15);
        assert!((sin(rad(30.0)) - 0.5).abs() < 1e-15);
        assert!((cbrt(-27.0) + 3.0).abs() < 1e-15);
    }
}
