//! Oracle tests for the text range-selector kernel (G38b / L06-G): every row of
//! `fixtures/text-glyph-selector/oracle.txt` (grammar and provenance in
//! `text_glyph_selector_oracle.rs`) against the production functions.
//!
//! A row that never reaches `sin` / `cos` equals the baseline bit for bit; a
//! SMOOTH or Wiggly row stays within `V8_TOLERANCE` of the baseline run on
//! Node's V8 (the kernel runs Rust `f64::sin` / `f64::cos`, see the oracle reader).

use super::oracle::{matches_baseline, rows, same, uses_trig, Row};
use super::{resolve_range, unit_count, unit_index_of, weight_at, weights, SelectorType};

#[test]
fn oracle_weight_at_matches_the_baseline() {
    let mut checked = 0;
    for row in rows() {
        if let Row::Weight {
            sel,
            ind,
            total,
            time,
            want,
        } = row
        {
            let got = weight_at(&sel, ind, total, time)
                .unwrap_or_else(|e| panic!("{sel:?} {ind} {total} {time}: refused {e:?}"));
            assert!(
                matches_baseline(uses_trig(&sel), got, want),
                "{sel:?} ind {ind}: got {got:?} want {want:?}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 2987);
}

#[test]
fn oracle_resolve_range_matches_the_baseline() {
    let mut checked = 0;
    for row in rows() {
        if let Row::Range { sel, total, s, e } = row {
            let (gs, ge) = resolve_range(&sel, total);
            assert!(
                same(gs, s) && same(ge, e),
                "{sel:?} {total}: ({gs}, {ge}) want ({s}, {e})"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 120);
}

#[test]
fn oracle_weights_loop_matches_the_baseline() {
    let mut checked = 0;
    for row in rows() {
        if let Row::Weights {
            sel,
            total,
            time,
            want,
        } = row
        {
            let got = weights(&sel, total, time).expect("weights");
            assert_eq!(got.len(), want.len(), "{sel:?} {total}: length");
            for (i, (g, w)) in got.iter().zip(&want).enumerate() {
                assert!(
                    matches_baseline(uses_trig(&sel), *g, *w),
                    "{sel:?} {total} unit {i}: {g:?} want {w:?}"
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 72);
}

#[test]
fn oracle_unit_index_and_unit_count_match_the_baseline() {
    let (mut indexed, mut counted) = (0, 0);
    for row in rows() {
        match row {
            Row::UnitIndex { based_on, sd, want } => {
                assert_eq!(
                    unit_index_of(Some(&sd), based_on),
                    want,
                    "{sd:?} {based_on:?}"
                );
                indexed += 1;
            }
            Row::UnitCount {
                based_on,
                glyphs,
                want,
            } => {
                assert!(
                    same(unit_count(&glyphs, based_on), want),
                    "{glyphs:?} {based_on:?}"
                );
                counted += 1;
            }
            _ => {}
        }
    }
    assert_eq!((indexed, counted), (108, 40));
}

#[test]
fn oracle_has_not_been_hollowed_out() {
    let all = rows();
    let weights_rows: Vec<&Row> = all
        .iter()
        .filter(|r| matches!(r, Row::Weight { .. }))
        .collect();
    let wiggly = weights_rows
        .iter()
        .filter(
            |r| matches!(r, Row::Weight { sel, .. } if sel.selector_type == SelectorType::Wiggly),
        )
        .count();
    assert!(wiggly >= 650, "wiggly rows {wiggly}");
    let mut shapes = [0usize; 8];
    for r in &weights_rows {
        if let Row::Weight { sel, want, .. } = r {
            if sel.selector_type == SelectorType::Range && *want > 0.0 && *want < 1.0 {
                shapes[(sel.shape.max(0.0) as usize).min(7)] += 1;
            }
        }
    }
    // a fractional weight exists for every one of the six shapes (and the unknown shapes 0, 7, 2.5)
    for shape in 1..=6 {
        assert!(
            shapes[shape] >= 20,
            "shape {shape}: {} fractional rows",
            shapes[shape]
        );
    }
    let nonzero = weights_rows
        .iter()
        .filter(|r| matches!(r, Row::Weight { want, .. } if *want != 0.0))
        .count();
    assert!(nonzero >= 1500, "nonzero weights {nonzero}");
    assert!(all.len() >= 3500, "rows {}", all.len());
}
