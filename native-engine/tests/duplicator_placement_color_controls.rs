//! Controls for the hue shift: every deviation fed through the real kernel
//! must be caught by the oracle rows.
//!
//! A control wraps or re-parameterizes the real kernel in ONE way and requires
//! that at least one oracle row then mismatches; the unchanged drive is the
//! baseline test of `duplicator_placement_color.rs`.

use super::tests::hh_rows;
use super::{hex_hue_rotate, hex_hue_rotate_with_wrap, HUE_WRAP};

/// Oracle rows the driver does not reproduce.
fn mismatches(driver: &dyn Fn(&str, f64) -> String) -> usize {
    hh_rows()
        .iter()
        .filter(|(input, deg, want)| driver(input, *deg) != *want)
        .count()
}

#[test]
fn the_undeviated_drive_matches_every_row() {
    assert_eq!(mismatches(&hex_hue_rotate), 0);
    assert_eq!(
        mismatches(&|h, d| hex_hue_rotate_with_wrap(h, d, HUE_WRAP)),
        0
    );
}

#[test]
fn a_hue_period_off_by_one_is_detected() {
    // the wrap at 360 is what makes -240, 480 and 840 equal 120
    for (wrap, rows) in [
        (359.0, 438),
        (361.0, 428),
        (360.5, 417),
        (180.0, 455),
        (720.0, 115),
    ] {
        let wrong = move |h: &str, d: f64| hex_hue_rotate_with_wrap(h, d, wrap);
        assert_eq!(mismatches(&wrong), rows, "wrap {wrap}");
    }
}

#[test]
fn a_hue_that_never_wraps_is_detected() {
    // an infinite period keeps `(h + deg) % wrap` equal to `h + deg`, so a hue
    // past 360 or below 0 reaches the channel function unwrapped
    let unwrapped = |h: &str, d: f64| hex_hue_rotate_with_wrap(h, d, f64::INFINITY);
    assert_eq!(mismatches(&unwrapped), 235);
}

#[test]
fn a_reversed_rotation_direction_is_detected() {
    let reversed = |h: &str, d: f64| hex_hue_rotate(h, -d);
    assert_eq!(mismatches(&reversed), 733);
    // the pinned case: red + 120 is green, red - 120 is blue
    assert_eq!(hex_hue_rotate("#ff0000", 120.0), "#00ff00");
    assert_eq!(hex_hue_rotate("#ff0000", -120.0), "#0000ff");
}

#[test]
fn a_dropped_alpha_byte_is_detected() {
    let dropped = |h: &str, d: f64| {
        let r = hex_hue_rotate(h, d);
        if r.len() == 9 {
            r[..7].to_string()
        } else {
            r
        }
    };
    // counted in UTF-8 bytes: 262 rows by JS string length, plus the six rows
    // of the two 9-byte (8 UTF-16 unit) passthrough strings '#ff0000é' and
    // 'é#ff0000'
    assert_eq!(mismatches(&dropped), 268);
}

#[test]
fn a_case_folded_alpha_is_detected() {
    let folded = |h: &str, d: f64| hex_hue_rotate(h, d).to_ascii_lowercase();
    assert_eq!(mismatches(&folded), 53, "the alpha byte keeps its case");
}

#[test]
fn a_case_sensitive_color_test_is_detected() {
    // the baseline regular expression is case-insensitive
    let sensitive = |h: &str, d: f64| {
        if h.bytes().any(|b| b.is_ascii_uppercase()) {
            h.to_string()
        } else {
            hex_hue_rotate(h, d)
        }
    };
    assert_eq!(mismatches(&sensitive), 57);
}

#[test]
fn a_rotation_that_ignores_the_degree_is_detected() {
    let identity = |h: &str, _d: f64| h.to_string();
    assert_eq!(mismatches(&identity), 799);
}
