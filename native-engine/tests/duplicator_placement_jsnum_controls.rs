//! Controls for the JavaScript number helpers: a deviation fed through the
//! same comparison as the baseline test must be caught by the oracle rows.

use super::tests::{hex, rows, same, utf8};
use super::{js_round, number_to_string, to_int32};

/// Rows of `kind` whose first value maps to something other than the oracle.
fn round_mismatches(driver: &dyn Fn(f64) -> f64) -> usize {
    rows("rnd")
        .iter()
        .filter(|t| !same(driver(hex(t[0])), hex(t[2])))
        .count()
}

#[test]
fn the_undeviated_helpers_match_every_row() {
    assert_eq!(round_mismatches(&js_round), 0);
    assert_eq!(
        rows("i32")
            .iter()
            .filter(|t| to_int32(hex(t[0])) != t[2].parse::<i32>().unwrap())
            .count(),
        0
    );
    assert_eq!(
        rows("str")
            .iter()
            .filter(|t| number_to_string(hex(t[0])) != utf8(t[2]))
            .count(),
        0
    );
}

#[test]
fn rust_round_half_away_from_zero_is_detected() {
    // Rust's f64::round breaks ties away from zero: -2.5 -> -3, JavaScript -> -2.
    assert_eq!(round_mismatches(&f64::round), 105);
    let negative_tie = rows("rnd")
        .into_iter()
        .find(|t| hex(t[0]) == -2.5)
        .expect("-2.5 row");
    assert!(!same(
        f64::round(hex(negative_tie[0])),
        hex(negative_tie[2])
    ));
}

#[test]
fn round_through_floor_of_x_plus_half_is_detected() {
    // 0.49999999999999994 + 0.5 rounds up to 1.0 in floating point.
    let naive = |x: f64| (x + 0.5).floor();
    assert_eq!(round_mismatches(&naive), 7);
}

#[test]
fn a_round_that_loses_the_negative_zero_is_detected() {
    let positive_zero = |x: f64| {
        let r = js_round(x);
        if r == 0.0 {
            0.0
        } else {
            r
        }
    };
    assert_eq!(round_mismatches(&positive_zero), 5);
}

#[test]
fn to_int32_variants_are_detected() {
    let saturating = |x: f64| x as i32;
    let rows_i32 = rows("i32");
    let bad = rows_i32
        .iter()
        .filter(|t| saturating(hex(t[0])) != t[2].parse::<i32>().unwrap())
        .count();
    assert_eq!(bad, 72, "a saturating cast differs from x|0");
    let no_wrap = |x: f64| {
        if x.is_finite() {
            x.trunc() as i64 as i32
        } else {
            0
        }
    };
    let bad = rows_i32
        .iter()
        .filter(|t| no_wrap(hex(t[0])) != t[2].parse::<i32>().unwrap())
        .count();
    assert!(bad > 0, "a 64-bit truncation differs from x|0");
}

#[test]
fn plain_display_of_a_float_is_not_the_javascript_string() {
    // Rust's `{}` never uses an exponent: 1e21 prints 1000000000000000000000.
    let plain = |x: f64| format!("{x}");
    let bad = rows("str")
        .iter()
        .filter(|t| plain(hex(t[0])) != utf8(t[2]))
        .count();
    assert!(bad > 0);
    assert_eq!(plain(1e21), "1000000000000000000000");
    assert_eq!(number_to_string(1e21), "1e+21");
}
