//! Tests for the JS operators of the text range-selector kernel (G38b /
//! L06-G): `||` falsiness, `Math.max` / `Math.min` with signed zeros and `NaN`.

use super::super::oracle::same;
use super::{js_max, js_min, js_or};

#[test]
fn js_operators_follow_ecmascript() {
    // `value || default`: +0, -0 and NaN are falsy, everything else (even a tiny value) is kept
    assert_eq!(js_or(0.0, 5.0), 5.0);
    assert_eq!(js_or(-0.0, 5.0), 5.0);
    assert_eq!(js_or(f64::NAN, 5.0), 5.0);
    assert_eq!(js_or(2.0, 5.0), 2.0);
    assert_eq!(js_or(-3.0, 5.0), -3.0);
    assert_eq!(js_or(1e-300, 5.0), 1e-300);
    // Math.max(+0, -0) and Math.max(-0, +0) are +0; Math.min of the same pair is -0
    assert!(same(js_max(0.0, -0.0), 0.0) && same(js_max(-0.0, 0.0), 0.0));
    assert!(same(js_min(0.0, -0.0), -0.0) && same(js_min(-0.0, 0.0), -0.0));
    assert!(same(js_max(-0.0, -0.0), -0.0) && same(js_min(0.0, 0.0), 0.0));
    assert_eq!((js_max(1.0, 2.0), js_max(2.0, 1.0)), (2.0, 2.0));
    assert_eq!((js_min(1.0, 2.0), js_min(2.0, 1.0)), (1.0, 1.0));
    // a NaN operand wins on either side (std `f64::max` would ignore it)
    assert!(js_max(f64::NAN, 1.0).is_nan() && js_max(1.0, f64::NAN).is_nan());
    assert!(js_min(f64::NAN, 1.0).is_nan() && js_min(1.0, f64::NAN).is_nan());
}
