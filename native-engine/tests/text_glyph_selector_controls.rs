//! Controls for the text range-selector kernel (G38b / L06-G): typed refusals
//! that produce no weights. The absent-versus-zero, falsy-default and decoding
//! rules are in `text_glyph_selector_defaults.rs`, the Wiggly properties and the
//! join with the real glyph identity in `text_glyph_selector_wiggly.rs`, the
//! mutation controls in `text_glyph_selector_mutations.rs`.

use super::{weight_at, weights, Selector, SelectorError, SelectorType, MAX_SELECTOR_UNITS};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::MAX_TEXT_UNITS;

fn wiggly() -> Selector {
    Selector {
        selector_type: SelectorType::Wiggly,
        seed: 42.0,
        ..Selector::default_selector()
    }
}

// ---- refusals -----------------------------------------------------------------------------------------------

#[test]
fn a_non_finite_selector_field_is_a_typed_refusal_naming_the_field() {
    let d = Selector::default_selector;
    let cases: Vec<(&str, Selector)> = vec![
        (
            "start",
            Selector {
                start: f64::NAN,
                ..d()
            },
        ),
        (
            "end",
            Selector {
                end: Some(f64::INFINITY),
                ..d()
            },
        ),
        (
            "offset",
            Selector {
                offset: f64::NEG_INFINITY,
                ..d()
            },
        ),
        (
            "shape",
            Selector {
                shape: f64::NAN,
                ..d()
            },
        ),
        (
            "amount",
            Selector {
                amount: Some(f64::NAN),
                ..d()
            },
        ),
        (
            "easeHigh",
            Selector {
                ease_high: f64::INFINITY,
                ..d()
            },
        ),
        (
            "easeLow",
            Selector {
                ease_low: f64::NAN,
                ..d()
            },
        ),
        (
            "smooth",
            Selector {
                smooth: Some(f64::NAN),
                ..d()
            },
        ),
        (
            "wiggleMin",
            Selector {
                wiggle_min: Some(f64::NAN),
                ..d()
            },
        ),
        (
            "wiggleMax",
            Selector {
                wiggle_max: Some(f64::INFINITY),
                ..d()
            },
        ),
        (
            "wigglesPerSec",
            Selector {
                wiggles_per_sec: Some(f64::NAN),
                ..d()
            },
        ),
        (
            "correlation",
            Selector {
                correlation: Some(f64::NEG_INFINITY),
                ..d()
            },
        ),
        (
            "seed",
            Selector {
                seed: f64::NAN,
                ..d()
            },
        ),
    ];
    for (name, sel) in cases {
        let refusal = SelectorError::NonFinite(name);
        assert_eq!(
            weight_at(&sel, 0.0, 9.0, 0.0),
            Err(refusal),
            "weight_at {name}"
        );
        assert_eq!(weights(&sel, 9.0, 0.0), Err(refusal), "weights {name}");
    }
}

#[test]
fn a_non_finite_index_unit_count_or_time_is_refused() {
    let sel = Selector::default_selector();
    assert_eq!(
        weight_at(&sel, f64::NAN, 9.0, 0.0),
        Err(SelectorError::NonFinite("ind"))
    );
    assert_eq!(
        weight_at(&sel, 0.0, f64::INFINITY, 0.0),
        Err(SelectorError::NonFinite("totalUnits"))
    );
    assert_eq!(
        weight_at(&sel, 0.0, 9.0, f64::NAN),
        Err(SelectorError::NonFinite("timeSec"))
    );
    assert_eq!(
        weights(&sel, f64::NAN, 0.0),
        Err(SelectorError::NonFinite("totalUnits"))
    );
    assert_eq!(
        weights(&sel, 9.0, f64::INFINITY),
        Err(SelectorError::NonFinite("timeSec"))
    );
}

#[test]
fn the_unit_bound_matches_one_text_block_and_a_bigger_block_is_refused() {
    assert_eq!(MAX_SELECTOR_UNITS, MAX_TEXT_UNITS);
    let sel = Selector::default_selector();
    assert_eq!(
        weights(&sel, (MAX_SELECTOR_UNITS + 1) as f64, 0.0),
        Err(SelectorError::TooManyUnits)
    );
    assert_eq!(weights(&sel, 1e300, 0.0), Err(SelectorError::TooManyUnits));
    let full = weights(&sel, MAX_SELECTOR_UNITS as f64, 0.0).expect("exactly the bound");
    assert_eq!(full.len(), MAX_SELECTOR_UNITS);
    assert!(full.iter().all(|w| *w == 1.0));
    // a fractional count loops while i < total, like the baseline `for (i = 0; i < totalUnits; i++)`
    assert_eq!(weights(&sel, 2.5, 0.0).unwrap().len(), 3);
    assert!(weights(&sel, -4.0, 0.0).unwrap().is_empty());
    assert!(weights(&sel, 0.0, 0.0).unwrap().is_empty());
}

#[test]
fn an_overflowing_wiggle_range_is_a_typed_non_finite_weight() {
    let sel = Selector {
        wiggle_min: Some(-1e308),
        wiggle_max: Some(1e308),
        ..wiggly()
    };
    assert_eq!(
        weight_at(&sel, 0.0, 5.0, 1.0),
        Err(SelectorError::NonFiniteWeight)
    );
    assert_eq!(weights(&sel, 5.0, 1.0), Err(SelectorError::NonFiniteWeight));
}

#[test]
fn a_non_finite_field_the_selected_branch_never_reads_is_refused() {
    // The baseline range branch never reads `wiggleMin`, and the Wiggly branch never reads `start`:
    // both answer there. The native guard is deliberately wider (see `check_selector`).
    let range = Selector::default_selector();
    let ok = weight_at(&range, 0.0, 9.0, 0.0).expect("finite selector");
    assert_eq!(ok, 1.0);
    let bad_range = Selector {
        wiggle_min: Some(f64::NAN),
        ..range
    };
    assert_eq!(
        weight_at(&bad_range, 0.0, 9.0, 0.0),
        Err(SelectorError::NonFinite("wiggleMin"))
    );
    let bad_wiggly = Selector {
        start: f64::INFINITY,
        ..wiggly()
    };
    assert_eq!(
        weights(&bad_wiggly, 4.0, 1.0),
        Err(SelectorError::NonFinite("start"))
    );
}
