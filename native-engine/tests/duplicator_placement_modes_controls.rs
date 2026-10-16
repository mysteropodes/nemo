//! Controls for the duplicator count and mode-offset kernel: every deviation
//! fed through the real kernel must be caught by the oracle rows.
//!
//! A control changes ONE input of the real kernel (a ceiling, the column
//! clamp, the operand order, the angle kind, the flag) and requires that at
//! least one oracle row then mismatches; the unchanged run is the baseline
//! test of `duplicator_placement_modes.rs`. A real mistake in the kernel of
//! the same kind therefore fails the same rows.

use super::tests::{
    bits, close, cnt_mismatches, hex, off_mismatches, parse_candidate, pth_mismatches,
    resolve_owned, rows, ulps, TRIG_ULPS,
};
use super::{JsNumber, PathInfo};

fn caught(what: &str, bad: Vec<String>) {
    assert!(
        !bad.is_empty(),
        "{what}: the oracle must reject this deviation"
    );
}

#[test]
fn wrong_count_ceilings_are_detected() {
    assert!(cnt_mismatches(900.0, 500.0).is_empty());
    caught("grid cap 899", cnt_mismatches(899.0, 500.0));
    caught("grid cap 901", cnt_mismatches(901.0, 500.0));
    caught("other cap 499", cnt_mismatches(900.0, 499.0));
    caught("other cap 501", cnt_mismatches(900.0, 501.0));
    caught("grid cap 500", cnt_mismatches(500.0, 500.0));
    caught("no cap", cnt_mismatches(f64::INFINITY, f64::INFINITY));
}

#[test]
fn an_unclamped_column_count_is_detected() {
    assert!(off_mismatches(&|_| {}).is_empty());
    caught("cols 0 (k % 0 is NaN)", off_mismatches(&|p| p.cols = 0.0));
    caught(
        "cols raw without max(1, ..)",
        off_mismatches(&|p| p.cols = p.spec.cols.or_default(1.0)),
    );
    caught("cols from the count", off_mismatches(&|p| p.cols = p.count));
}

#[test]
fn grid_operand_mistakes_are_detected() {
    caught(
        "spacing x and y swapped",
        off_mismatches(&|p| std::mem::swap(&mut p.spec.spacing_x, &mut p.spec.spacing_y)),
    );
    caught(
        "the next clone",
        off_mismatches(&|p| p.k = p.k.saturating_add(1)),
    );
}

#[test]
fn radial_angle_mistakes_are_detected() {
    caught(
        "start and end angles swapped",
        off_mismatches(&|p| std::mem::swap(&mut p.spec.start_angle, &mut p.spec.end_angle)),
    );
    caught(
        "an undefined start angle read as null",
        off_mismatches(&|p| {
            if p.spec.start_angle == JsNumber::Undefined {
                p.spec.start_angle = JsNumber::Null;
            }
        }),
    );
    caught(
        "an end angle that is ignored",
        off_mismatches(&|p| p.spec.end_angle = JsNumber::Undefined),
    );
    caught(
        "the orient flag flipped",
        off_mismatches(&|p| p.spec.radial_orient = !p.spec.radial_orient),
    );
    caught(
        "the radius ignored",
        off_mismatches(&|p| p.spec.radius = JsNumber::Value(1.0)),
    );
    caught("the count off by one", off_mismatches(&|p| p.count += 1.0));
}

#[test]
fn path_parameter_mistakes_are_detected() {
    assert!(pth_mismatches(&|_| {}).is_empty());
    caught(
        "t = k / count on an open path",
        pth_mismatches(&|p| p.count += 1.0),
    );
    caught(
        "t = k / (count - 1) on a closed path",
        pth_mismatches(&|p| p.count -= 0.5),
    );
    caught(
        "the tangent flag flipped",
        pth_mismatches(&|p| p.spec.path_align_tangent = !p.spec.path_align_tangent),
    );
    caught("the next clone", pth_mismatches(&|p| p.k += 1));
}

#[test]
fn resolution_rule_mistakes_are_detected() {
    let infos_with = |tweak: &dyn Fn(&mut super::tests::OwnedCandidate)| -> usize {
        let mut bad = 0;
        for t in rows("rsv") {
            let cand = rows("cand")
                .into_iter()
                .find(|c| c[0] == t[0])
                .expect("cand row");
            let mut owned = parse_candidate(cand[1]);
            if let Some(c) = owned.as_mut() {
                tweak(c);
            }
            let same = match (resolve_owned(&owned), &t[2..]) {
                (Ok(None), ["-"]) => true,
                (Ok(Some(i)), [len, ..]) => u64::from_str_radix(len, 16)
                    .map_or(false, |b| bits(i.length) == bits(f64::from_bits(b))),
                (Err(_), ["throw"]) => true,
                _ => false,
            };
            if !same {
                bad += 1;
            }
        }
        bad
    };
    assert_eq!(infos_with(&|_| {}), 0);
    assert!(
        infos_with(&|c| c.has_duplicator = false) >= 1,
        "a duplicator source must refuse"
    );
    assert!(
        infos_with(&|c| c.strokes.reverse()) >= 1,
        "the FIRST qualifying stroke is the source"
    );
    assert!(
        infos_with(&|c| c.strokes.retain(|s| !s.is_raster)) == 0,
        "raster strokes carry no segments, so dropping them changes nothing"
    );
    assert!(
        infos_with(&|c| c.strokes.truncate(0)) >= 5,
        "no stroke, no path"
    );
}

#[test]
fn the_comparison_does_not_hide_real_differences() {
    let sample = 86.60254037844388_f64;
    assert!(close(sample, sample, 0));
    assert!(!close(sample * (1.0 + 1e-9), sample, TRIG_ULPS));
    assert!(!close(0.0, -0.0, 0), "bit-exact comparison tells -0 from 0");
    assert!(close(0.0, -0.0, TRIG_ULPS), "the trig tolerance does not");
    assert_eq!(ulps(1.0, f64::from_bits(1.0_f64.to_bits() + 1)), 1);
    assert_eq!(ulps(-1.0, f64::from_bits((-1.0_f64).to_bits() + 1)), 1);
    assert!(close(f64::NAN, f64::NAN, 0) && !close(f64::NAN, 0.0, TRIG_ULPS));
}

#[test]
fn resolved_paths_are_not_empty_shells() {
    let all: Vec<PathInfo> = super::tests::infos().into_values().collect();
    assert!(all.len() >= 25);
    assert!(all.iter().all(|i| i.length > 0.0 && !i.curves.is_empty()));
    assert!(all.iter().any(|i| i.closed) && all.iter().any(|i| !i.closed));
}
