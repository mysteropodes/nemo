//! Defaults and decoding controls of the text range-selector kernel (G38b /
//! L06-G): the absent-versus-zero rule of the `== null` fields, the falsy
//! default of the `||` fields, the baseline string comparisons and the default
//! selector. The typed refusals are in `text_glyph_selector_controls.rs`.

use super::{
    resolve_range, weights, BasedOn, Selector, SelectorType, Units, SHAPE_RAMP_UP, SHAPE_SQUARE,
};

fn wiggly() -> Selector {
    Selector {
        selector_type: SelectorType::Wiggly,
        seed: 42.0,
        ..Selector::default_selector()
    }
}

// ---- absent versus zero, and the JS `||` fields ------------------------------------------------------------------

#[test]
fn an_explicit_zero_is_not_an_absent_value_for_the_null_checked_fields() {
    let d = Selector::default_selector();
    let all = weights(&d, 6.0, 0.0).unwrap();
    // amount: null -> 100, but an explicit 0 silences the animator
    let absent = Selector { amount: None, ..d };
    assert_eq!(weights(&absent, 6.0, 0.0).unwrap(), all);
    let zero = Selector {
        amount: Some(0.0),
        ..d
    };
    assert_eq!(weights(&zero, 6.0, 0.0).unwrap(), vec![0.0; 6]);
    // end: null -> 100, an explicit 0 selects nothing
    assert_eq!(
        weights(&Selector { end: None, ..d }, 6.0, 0.0).unwrap(),
        all
    );
    assert_eq!(
        weights(
            &Selector {
                end: Some(0.0),
                ..d
            },
            6.0,
            0.0
        )
        .unwrap(),
        vec![0.0; 6]
    );
    // smooth: null -> 100 (soft), an explicit 0 becomes the 1e-8 band (a hard step)
    let ramp = Selector {
        shape: SHAPE_RAMP_UP,
        ..d
    };
    let soft = weights(
        &Selector {
            smooth: None,
            ..ramp
        },
        6.0,
        0.0,
    )
    .unwrap();
    let hard = weights(
        &Selector {
            smooth: Some(0.0),
            ..ramp
        },
        6.0,
        0.0,
    )
    .unwrap();
    assert_eq!(soft, weights(&ramp, 6.0, 0.0).unwrap());
    assert!(soft.iter().any(|v| *v > 0.0 && *v < 1.0));
    assert!(hard.iter().all(|v| *v == 0.0 || *v == 1.0), "{hard:?}");
    // wigglesPerSec: null -> 2 (moves with time), an explicit 0 freezes the wiggle
    let moving = Selector {
        wiggles_per_sec: None,
        ..wiggly()
    };
    let frozen = Selector {
        wiggles_per_sec: Some(0.0),
        ..wiggly()
    };
    assert_ne!(
        weights(&moving, 4.0, 0.0).unwrap(),
        weights(&moving, 4.0, 5.0).unwrap()
    );
    assert_eq!(
        weights(&frozen, 4.0, 0.0).unwrap(),
        weights(&frozen, 4.0, 5.0).unwrap()
    );
    assert_eq!(
        weights(&moving, 4.0, 3.0).unwrap(),
        weights(
            &Selector {
                wiggles_per_sec: Some(2.0),
                ..wiggly()
            },
            4.0,
            3.0
        )
        .unwrap()
    );
}

#[test]
fn a_zero_or_unknown_value_falls_back_for_the_or_fields() {
    let d = Selector::default_selector();
    let square = weights(
        &Selector {
            end: Some(45.0),
            ..d
        },
        9.0,
        0.0,
    )
    .unwrap();
    // `sel.shape || SQUARE`: 0 is SQUARE, and a number that is no shape id also lands in the SQUARE branch
    for shape in [0.0, -0.0, 7.0, 2.5, -3.0, SHAPE_SQUARE] {
        let sel = Selector {
            shape,
            end: Some(45.0),
            ..d
        };
        assert_eq!(weights(&sel, 9.0, 0.0).unwrap(), square, "shape {shape}");
    }
    // `sel.seed || 1`
    assert_eq!(
        weights(
            &Selector {
                seed: 0.0,
                ..wiggly()
            },
            4.0,
            1.0
        )
        .unwrap(),
        weights(
            &Selector {
                seed: 1.0,
                ..wiggly()
            },
            4.0,
            1.0
        )
        .unwrap()
    );
    assert_ne!(
        weights(
            &Selector {
                seed: 2.0,
                ..wiggly()
            },
            4.0,
            1.0
        )
        .unwrap(),
        weights(
            &Selector {
                seed: 1.0,
                ..wiggly()
            },
            4.0,
            1.0
        )
        .unwrap()
    );
    // `sel.start || 0` and `sel.offset || 0` need no fallback test: 0 is already the identity
    assert_eq!(
        resolve_range(
            &Selector {
                start: -0.0,
                offset: -0.0,
                ..d
            },
            9.0
        ),
        resolve_range(&d, 9.0)
    );
}

#[test]
fn baseline_strings_decode_like_the_javascript_comparisons() {
    assert_eq!(SelectorType::from_baseline("wiggly"), SelectorType::Wiggly);
    assert_eq!(SelectorType::from_baseline("range"), SelectorType::Range);
    assert_eq!(SelectorType::from_baseline("Wiggly"), SelectorType::Range);
    assert_eq!(Units::from_baseline("index"), Units::Index);
    assert_eq!(Units::from_baseline("percent"), Units::Percent);
    assert_eq!(Units::from_baseline(""), Units::Percent);
    assert_eq!(BasedOn::from_baseline("words"), BasedOn::Words);
    assert_eq!(BasedOn::from_baseline("lines"), BasedOn::Lines);
    assert_eq!(BasedOn::from_baseline("chars"), BasedOn::Chars);
    assert_eq!(BasedOn::from_baseline("bogus"), BasedOn::Chars);
}

#[test]
fn the_default_selector_is_the_baseline_default_selector() {
    let d = Selector::default_selector();
    assert_eq!(
        (d.selector_type, d.units, d.based_on),
        (SelectorType::Range, Units::Percent, BasedOn::Chars)
    );
    assert_eq!(
        (d.start, d.end, d.offset, d.shape),
        (0.0, Some(100.0), 0.0, 1.0)
    );
    assert_eq!(
        (d.amount, d.ease_high, d.ease_low, d.smooth),
        (Some(100.0), 0.0, 0.0, Some(100.0))
    );
    assert_eq!((d.wiggle_min, d.wiggle_max), (Some(-50.0), Some(50.0)));
    assert_eq!(
        (d.wiggles_per_sec, d.correlation, d.seed),
        (Some(2.0), Some(50.0), 1.0)
    );
}
