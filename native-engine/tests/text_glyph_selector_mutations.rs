//! Mutation controls for the text range-selector kernel (G38b / L06-G): each
//! test re-implements one rule with a single deliberate slip (truncation for
//! floor, linear for smoothstep, 8 halvings for 24, no swap of a reversed range,
//! no word/line fallback, a clamped Wiggly weight, a null amount read as 0) and
//! requires the slipped rule to disagree with the oracle on enough rows, while
//! the production rule equals the oracle on every one of them. Rows that reach
//! `sin` / `cos` are compared within `V8_TOLERANCE` (production) and `MUTANT_GAP`
//! (a mutant counts as caught only when it is farther than that).

use super::curve::{hash_rand, smooth_noise_1d};
use super::oracle::{matches_baseline, rows, same, uses_trig, Row, MUTANT_GAP, V8_TOLERANCE};
use super::{resolve_range, unit_index_of, weight_at, BasedOn, Selector, SelectorType, Units};

// ---- mutation controls: a slip in one rule must disagree with the oracle ------------------------------------------------

/// Runs `mutant` on every row; it answers `None` for a row of another kind and
/// `Some(true)` when the mutated rule disagrees with the oracle. Returns the number of disagreements.
fn differs(rows_checked: usize, mut mutant: impl FnMut(&Row) -> Option<bool>) -> usize {
    let (mut seen, mut disagreements) = (0, 0);
    for row in rows() {
        if let Some(d) = mutant(&row) {
            seen += 1;
            disagreements += usize::from(d);
        }
    }
    assert_eq!(
        seen, rows_checked,
        "the mutant harness saw every row of its kind"
    );
    disagreements
}

#[test]
fn mutation_hash_fraction_by_truncation_instead_of_floor_is_caught() {
    let n = differs(70, |row| match row {
        Row::Hash { seed, i, want } => {
            let x = (seed * 12.9898 + i * 78.233).sin() * 43758.5453;
            assert!(
                (hash_rand(*seed, *i) - want).abs() <= V8_TOLERANCE,
                "the unmutated hash equals the oracle"
            );
            Some((x - x.trunc() - want).abs() > MUTANT_GAP)
        }
        _ => None,
    });
    assert!(n >= 5, "negative arguments must expose trunc: {n}");
}

#[test]
fn mutation_linear_noise_instead_of_smoothstep_is_caught() {
    let n = differs(33, |row| match row {
        Row::Noise { seed, t, want } => {
            let i0 = t.floor();
            let (v0, v1) = (
                hash_rand(*seed, i0) * 2.0 - 1.0,
                hash_rand(*seed, i0 + 1.0) * 2.0 - 1.0,
            );
            assert!((smooth_noise_1d(*seed, *t) - want).abs() <= V8_TOLERANCE);
            Some((v0 + (v1 - v0) * (t - i0) - want).abs() > MUTANT_GAP)
        }
        _ => None,
    });
    assert!(
        n >= 15,
        "fractional times must expose the linear blend: {n}"
    );
}

#[test]
fn mutation_eight_halvings_instead_of_twenty_four_is_caught() {
    let bisect = |p: &[f64; 4], x: f64, rounds: u32| {
        let calc = |a: f64, b: f64, t: f64| {
            let u = 1.0 - t;
            3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
        };
        let (mut lo, mut hi, mut t) = (0.0, 1.0, x);
        for _ in 0..rounds {
            let cx = calc(p[0], p[2], t);
            if (cx - x).abs() < 1e-9 {
                break;
            }
            if cx < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
        calc(p[1], p[3], t)
    };
    let (mut full_ok, mut short_wrong, mut curved) = (0, 0, 0);
    for row in rows() {
        if let Row::Bezier { p, x, y } = row {
            if p == [0.0, 0.0, 1.0, 1.0] {
                continue;
            }
            curved += 1;
            full_ok += usize::from(same(bisect(&p, x, 24), y));
            short_wrong += usize::from(!same(bisect(&p, x, 8), y));
        }
    }
    assert_eq!(
        full_ok, curved,
        "the re-implemented 24-halving easer is the oracle"
    );
    assert!(
        short_wrong >= curved / 3,
        "8 halvings disagree on {short_wrong} of {curved} rows"
    );
}

#[test]
fn mutation_range_without_the_swap_is_caught() {
    let n = differs(120, |row| match row {
        Row::Range { sel, total, s, e } => {
            let divisor = if sel.units == Units::Index {
                1.0
            } else {
                100.0 / total.max(1.0)
            };
            let o = sel.offset / divisor;
            let (ms, me) = (
                sel.start / divisor + o,
                sel.end.unwrap_or(100.0) / divisor + o,
            );
            let (gs, ge) = resolve_range(sel, *total);
            assert!(same(gs, *s) && same(ge, *e));
            Some(!(same(ms, *s) && same(me, *e)))
        }
        _ => None,
    });
    assert!(n >= 10, "reversed ranges must expose the missing swap: {n}");
}

#[test]
fn mutation_no_fallback_for_word_and_line_indices_is_caught() {
    let n = differs(108, |row| match row {
        Row::UnitIndex { based_on, sd, want } => {
            let got = unit_index_of(Some(sd), *based_on);
            assert_eq!(got, *want);
            let mutant = match based_on {
                BasedOn::Words => sd.word_index,
                BasedOn::Lines => sd.line_index,
                BasedOn::Chars => sd.char_index,
            };
            Some(mutant != *want)
        }
        _ => None,
    });
    assert!(
        n >= 8,
        "rows with a null word or line index must expose it: {n}"
    );
}

#[test]
fn mutation_clamping_the_wiggly_weight_or_dropping_a_null_amount_is_caught() {
    let (mut clamped, mut amount_none, mut wiggly_rows) = (0, 0, 0);
    for row in rows() {
        if let Row::Weight {
            sel,
            ind,
            total,
            time,
            want,
        } = row
        {
            let got = weight_at(&sel, ind, total, time).unwrap();
            assert!(matches_baseline(uses_trig(&sel), got, want));
            if sel.selector_type == SelectorType::Wiggly {
                wiggly_rows += 1;
                clamped += usize::from((got.clamp(0.0, 1.0) - want).abs() > MUTANT_GAP);
            } else if sel.amount.is_none() {
                let zero = Selector {
                    amount: Some(0.0),
                    ..sel
                };
                let slipped = weight_at(&zero, ind, total, time).unwrap();
                amount_none += usize::from((slipped - want).abs() > MUTANT_GAP);
            }
        }
    }
    assert!(wiggly_rows >= 650);
    assert!(
        clamped >= 100,
        "negative wiggle weights must expose a 0..1 clamp: {clamped}"
    );
    assert!(
        amount_none >= 10,
        "rows with a null amount must expose reading it as 0: {amount_none}"
    );
}
