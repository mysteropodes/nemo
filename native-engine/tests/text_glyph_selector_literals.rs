//! Literal ports of every scenario of `tests/text-selector.test.cjs` at
//! fbe4b8a3 (G38b / L06-G), one Rust test per baseline test, with the numbers
//! the baseline module printed for them (`tools/gen.js` re-asserts the baseline
//! assertions before it writes the arrays in `text_glyph_selector_scenarios.rs`).
//! Range-selector arrays are bit-exact; SMOOTH and Wiggly arrays are the engine's own `Math` answer and
//! are held within `V8_TOLERANCE` (the kernel runs Rust `f64::sin` / `f64::cos`).

use super::oracle::{same, V8_TOLERANCE};
use super::scenarios::{
    EASED, FORWARD, HARD, INDEX_UNITS, OFFSET_0, OFFSET_50, RAMP_DOWN, RAMP_UP, SMOOTH, SOFT,
    SQUARE_45, TRIANGLE, WIGGLY_42, WIGGLY_CORR_0, WIGGLY_CORR_100, WIGGLY_EARLY, WIGGLY_LATER,
};
use super::{
    unit_index_of, weights, BasedOn, Selector, SelectorType, UnitIndices, Units, SHAPE_RAMP_DOWN,
    SHAPE_RAMP_UP, SHAPE_ROUND, SHAPE_SMOOTH, SHAPE_SQUARE, SHAPE_TRIANGLE,
};

fn base(change: impl FnOnce(&mut Selector)) -> Selector {
    let mut sel = Selector::default_selector();
    change(&mut sel);
    sel
}

fn wiggly(seed: f64, change: impl FnOnce(&mut Selector)) -> Selector {
    base(|s| {
        s.selector_type = SelectorType::Wiggly;
        s.seed = seed;
        change(s);
    })
}

fn w(sel: &Selector, units: f64) -> Vec<f64> {
    weights(sel, units, 0.0).expect("weights")
}

fn exact(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len());
    for (i, (g, e)) in got.iter().zip(want).enumerate() {
        assert!(same(*g, *e), "unit {i}: {g:?} want {e:?}");
    }
}

fn near(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len());
    for (i, (g, e)) in got.iter().zip(want).enumerate() {
        assert!((g - e).abs() <= V8_TOLERANCE, "unit {i}: {g} vs {e}");
    }
}

fn four(v: f64) -> String {
    format!("{v:.4}")
}

#[test]
fn a_full_range_selects_every_unit_an_empty_one_selects_none() {
    let full = w(&base(|s| s.end = Some(100.0)), 9.0);
    exact(&full, &[1.0; 9]);
    let empty = w(&base(|s| s.end = Some(0.0)), 9.0);
    exact(&empty, &[0.0; 9]);
}

#[test]
fn every_weight_stays_inside_0_1_across_all_shapes_and_ranges() {
    for shape in [
        SHAPE_SQUARE,
        SHAPE_RAMP_UP,
        SHAPE_RAMP_DOWN,
        SHAPE_TRIANGLE,
        SHAPE_ROUND,
        SHAPE_SMOOTH,
    ] {
        for end in [0.0, 17.0, 45.0, 80.0, 100.0] {
            for offset in [-60.0, 0.0, 35.0] {
                let sel = base(|s| {
                    s.shape = shape;
                    s.end = Some(end);
                    s.offset = offset;
                });
                for v in w(&sel, 9.0) {
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "shape {shape} end {end} offset {offset} -> {v}"
                    );
                }
            }
        }
    }
}

#[test]
fn square_shape_gives_a_hard_range_with_a_partial_edge_unit() {
    // 45% of 9 chars = 4.05 units: four full, then a 0.05 sliver, then nothing
    exact(&w(&base(|s| s.end = Some(45.0)), 9.0), &SQUARE_45);
}

#[test]
fn triangle_peaks_at_the_centre_and_is_symmetric() {
    let got = w(&base(|s| s.shape = SHAPE_TRIANGLE), 9.0);
    exact(&got, &TRIANGLE);
    let peak = got.iter().cloned().fold(f64::MIN, f64::max);
    assert_eq!(got.iter().position(|v| *v == peak), Some(4));
    assert_eq!(four(got[0]), four(got[8]));
    assert_eq!(four(got[1]), four(got[7]));
    assert_eq!(four(got[4]), "1.0000");
}

#[test]
fn smooth_shape_is_a_cosine_bell_also_symmetric() {
    let got = w(&base(|s| s.shape = SHAPE_SMOOTH), 9.0);
    near(&got, &SMOOTH);
    assert_eq!(four(got[0]), four(got[8]));
    assert_eq!(four(got[4]), "1.0000");
    assert!(
        got[2] > got[0] && got[4] > got[2],
        "rises towards the centre"
    );
}

#[test]
fn ramp_up_and_ramp_down_are_mirror_images() {
    let up = w(&base(|s| s.shape = SHAPE_RAMP_UP), 9.0);
    let down = w(&base(|s| s.shape = SHAPE_RAMP_DOWN), 9.0);
    exact(&up, &RAMP_UP);
    exact(&down, &RAMP_DOWN);
    for i in 0..9 {
        assert!(
            (up[i] + down[i] - 1.0).abs() < 5e-7,
            "unit {i} should sum to 1"
        );
    }
}

#[test]
fn offset_sweeps_the_window_along_the_text() {
    let at0 = w(&base(|s| s.end = Some(45.0)), 9.0);
    let at50 = w(
        &base(|s| {
            s.end = Some(45.0);
            s.offset = 50.0;
        }),
        9.0,
    );
    exact(&at0, &OFFSET_0);
    exact(&at50, &OFFSET_50);
    assert!(at0[0] > at50[0] && at50[7] > at0[7]);
}

#[test]
fn a_reversed_selector_start_past_end_is_swapped_not_empty() {
    let fwd = w(
        &base(|s| {
            s.start = 20.0;
            s.end = Some(70.0);
        }),
        9.0,
    );
    let rev = w(
        &base(|s| {
            s.start = 70.0;
            s.end = Some(20.0);
        }),
        9.0,
    );
    exact(&fwd, &FORWARD);
    exact(&rev, &fwd);
}

#[test]
fn index_units_address_whole_characters_instead_of_percentages() {
    let sel = base(|s| {
        s.units = Units::Index;
        s.end = Some(3.0);
    });
    exact(&w(&sel, 9.0), &INDEX_UNITS);
}

#[test]
fn amount_scales_the_whole_animator_down() {
    let full = w(&base(|_| {}), 6.0);
    let half = w(&base(|s| s.amount = Some(50.0)), 6.0);
    for (i, (f, h)) in full.iter().zip(&half).enumerate() {
        assert!(same(*h, f * 0.5), "unit {i}");
    }
}

#[test]
fn smoothness_narrows_the_transition_band() {
    let soft = w(&base(|s| s.shape = SHAPE_RAMP_UP), 9.0);
    let hard = w(
        &base(|s| {
            s.shape = SHAPE_RAMP_UP;
            s.smooth = Some(10.0);
        }),
        9.0,
    );
    exact(&soft, &SOFT);
    exact(&hard, &HARD);
    let mid = |a: &[f64]| a.iter().filter(|v| **v > 0.001 && **v < 0.999).count();
    assert!(
        mid(&hard) < mid(&soft),
        "hard={} soft={}",
        mid(&hard),
        mid(&soft)
    );
}

#[test]
fn ease_high_low_bends_the_weight_ramp_without_leaving_0_1() {
    let eased = w(
        &base(|s| {
            s.shape = SHAPE_RAMP_UP;
            s.ease_high = 80.0;
        }),
        9.0,
    );
    exact(&eased, &EASED);
    assert!(
        eased
            .iter()
            .zip(&RAMP_UP)
            .any(|(e, f)| (e - f).abs() > 0.005),
        "flat != eased"
    );
    assert!(eased.iter().all(|v| (0.0..=1.0).contains(v)));
    // the fixed points are the WEIGHTS 0 and 1: fully-in and fully-out units stay put
    exact(&w(&base(|s| s.ease_high = 80.0), 9.0), &[1.0; 9]);
    exact(
        &w(
            &base(|s| {
                s.end = Some(0.0);
                s.ease_high = 80.0;
            }),
            9.0,
        ),
        &[0.0; 9],
    );
}

#[test]
fn unit_index_of_reads_the_index_the_selector_is_based_on() {
    let sd = UnitIndices {
        char_index: Some(7.0),
        word_index: Some(2.0),
        line_index: Some(1.0),
    };
    assert_eq!(unit_index_of(Some(&sd), BasedOn::Chars), Some(7.0));
    assert_eq!(unit_index_of(Some(&sd), BasedOn::Words), Some(2.0));
    assert_eq!(unit_index_of(Some(&sd), BasedOn::Lines), Some(1.0));
    // falls back to the character index rather than dropping the glyph
    let old = UnitIndices {
        char_index: Some(4.0),
        ..UnitIndices::default()
    };
    assert_eq!(unit_index_of(Some(&old), BasedOn::Words), Some(4.0));
    assert_eq!(unit_index_of(None, BasedOn::Chars), None);
}

#[test]
fn a_single_unit_block_does_not_divide_by_zero() {
    for shape in [
        SHAPE_SQUARE,
        SHAPE_RAMP_UP,
        SHAPE_RAMP_DOWN,
        SHAPE_TRIANGLE,
        SHAPE_ROUND,
        SHAPE_SMOOTH,
    ] {
        let got = w(&base(|s| s.shape = shape), 1.0);
        assert_eq!(got.len(), 1);
        assert!(got[0].is_finite(), "shape {shape} -> {}", got[0]);
    }
    assert!(w(&base(|_| {}), 0.0).is_empty());
}

#[test]
fn wiggly_selector_same_seed_time_and_index_is_deterministic_and_finite() {
    let sel = wiggly(42.0, |_| {});
    let a = weights(&sel, 5.0, 1.37).unwrap();
    let b = weights(&sel, 5.0, 1.37).unwrap();
    exact(&a, &b);
    near(&a, &WIGGLY_42);
    assert!(a.iter().all(|v| v.is_finite()));
}

#[test]
fn wiggly_selector_stays_within_wiggle_min_max_as_a_fraction_of_100() {
    let sel = wiggly(7.0, |s| {
        s.wiggle_min = Some(-30.0);
        s.wiggle_max = Some(60.0);
    });
    for f in 0..40 {
        for v in weights(&sel, 6.0, f64::from(f) * 0.2).unwrap() {
            assert!(
                (-0.3 - 1e-9..=0.6 + 1e-9).contains(&v),
                "{v} outside [-0.3, 0.6]"
            );
        }
    }
}

#[test]
fn wiggly_selector_changes_over_time() {
    let sel = wiggly(3.0, |s| s.wiggles_per_sec = Some(2.0));
    let early = weights(&sel, 4.0, 0.0).unwrap();
    let later = weights(&sel, 4.0, 5.0).unwrap();
    near(&early, &WIGGLY_EARLY);
    near(&later, &WIGGLY_LATER);
    assert!(early.iter().zip(&later).any(|(a, b)| a != b));
}

#[test]
fn wiggly_correlation_100_makes_every_unit_wiggle_in_unison() {
    let sel = wiggly(11.0, |s| s.correlation = Some(100.0));
    let got = weights(&sel, 8.0, 0.6).unwrap();
    for (i, v) in got.iter().enumerate() {
        // the baseline test compares exactly; the last bit of `local + (global - local)` follows the
        // platform sine, so the check is bounded, not exact
        assert!(
            (*v - got[0]).abs() <= V8_TOLERANCE,
            "unit {i} should match unit 0 at full correlation"
        );
    }
    near(&got, &WIGGLY_CORR_100);
}

#[test]
fn wiggly_correlation_0_lets_units_diverge() {
    let sel = wiggly(11.0, |s| s.correlation = Some(0.0));
    let got = weights(&sel, 8.0, 0.6).unwrap();
    assert!(
        !got.iter().all(|v| *v == got[0]),
        "independent units should not all coincide"
    );
    near(&got, &WIGGLY_CORR_0);
}

#[test]
fn a_range_selector_is_unaffected_by_the_time_parameter() {
    let sel = base(|s| s.shape = SHAPE_RAMP_UP);
    let a = weights(&sel, 9.0, 0.0).unwrap();
    let b = weights(&sel, 9.0, 123.456).unwrap();
    exact(&a, &b);
}
