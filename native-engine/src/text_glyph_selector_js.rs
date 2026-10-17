//! The JS `||`, `Math.max` and `Math.min` rules the text range selector (G38b /
//! L06-G) leans on, as plain `f64` functions: zero (either sign) and `NaN` are
//! falsy for `||`, a `NaN` operand wins in `Math.max` / `Math.min`, and `-0`
//! orders below `+0`. Rust `f64::max` / `f64::min` would ignore a `NaN` and
//! lose the sign of zero, so they are not used for the baseline expressions.
//!
//! This module imports nothing from the crate.

/// JS `value || default`: zero (either sign) and `NaN` are falsy.
pub(crate) fn js_or(value: f64, default: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        default
    } else {
        value
    }
}

/// `Math.max(a, b)`: `NaN` wins, `+0` beats `-0`.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a > b {
        a
    } else if b > a {
        b
    } else if a == 0.0 && a.is_sign_negative() {
        b
    } else {
        a
    }
}

/// `Math.min(a, b)`: `NaN` wins, `-0` beats `+0`.
pub(crate) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a < b {
        a
    } else if b < a {
        b
    } else if a == 0.0 && a.is_sign_negative() {
        a
    } else {
        b
    }
}

#[cfg(test)]
#[path = "../tests/text_glyph_selector_js.rs"]
mod tests;
