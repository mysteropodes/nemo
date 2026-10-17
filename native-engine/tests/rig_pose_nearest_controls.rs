//! Negative and mutation controls for the Paper nearest-location kernel (L03-I). Each `Flaw` is one plausible
//! mistake (all are Paper-level mutants the generator proved the oracle catches). The flawed copies run through
//! the SAME row harness as production; with `Flaw::None` the copy must equal production, so a rejection is
//! caused by the flaw alone. The named flaws also fail dedicated hand-pinned tests.

use std::collections::BTreeSet;

use super::flawed::{Flaw, Flawed, FLAWS};
use super::oracle_rows::{failures, rows_of, Ops, Production, ORACLE};
use super::tests::{bowl, corner, hit, horizontal, square};
use super::{NearestError, NearestLocation};
use crate::evaluation::property_follow_path_sample::SegmentIn;

/// The row kinds (`nst`, ...) among a failure list.
fn kinds(bad: &[String]) -> BTreeSet<String> {
    bad.iter()
        .map(|m| m.split(':').next().unwrap_or("").to_string())
        .collect()
}

fn flawed(flaw: Flaw, rest: &[SegmentIn], closed: bool, p: [f64; 2]) -> NearestLocation {
    Flawed(flaw)
        .nearest(rest, closed, p)
        .expect("resolved")
        .expect("a location")
}

#[test]
fn the_flaw_free_copy_equals_production_on_every_row() {
    assert!(failures(&Flawed(Flaw::None)).is_empty());
    assert!(failures(&Production).is_empty());
}

#[test]
fn every_flaw_is_rejected_by_the_row_kind_it_breaks() {
    for (flaw, kind) in FLAWS {
        let bad = failures(&Flawed(flaw));
        assert!(!bad.is_empty(), "{flaw:?} survived every oracle row");
        assert!(
            kinds(&bad).contains(kind),
            "{flaw:?} must fail a {kind} row, failed {:?}",
            kinds(&bad)
        );
    }
}

#[test]
fn constant_functions_cannot_pass_the_rows() {
    struct Nothing;
    impl Ops for Nothing {
        fn nearest(
            &self,
            _: &[SegmentIn],
            _: bool,
            _: [f64; 2],
        ) -> Result<Option<NearestLocation>, NearestError> {
            Ok(None)
        }
    }
    struct Origin;
    impl Ops for Origin {
        fn nearest(
            &self,
            _: &[SegmentIn],
            _: bool,
            _: [f64; 2],
        ) -> Result<Option<NearestLocation>, NearestError> {
            Ok(Some(NearestLocation {
                point: [0.0, 0.0],
                offset: 0.0,
                distance: 0.0,
                curve_index: 0,
                time: 0.0,
            }))
        }
    }
    struct Unresolved;
    impl Ops for Unresolved {
        fn nearest(
            &self,
            _: &[SegmentIn],
            _: bool,
            _: [f64; 2],
        ) -> Result<Option<NearestLocation>, NearestError> {
            Err(NearestError::TimeUnresolved { curve_index: 0 })
        }
    }
    let total = rows_of("nst").count();
    for (name, bad) in [
        ("nothing", failures(&Nothing)),
        ("origin", failures(&Origin)),
        ("unresolved", failures(&Unresolved)),
    ] {
        assert!(
            bad.len() * 10 > total * 9,
            "{name}: only {} of {total} rows fail",
            bad.len()
        );
    }
}

#[test]
fn the_last_equidistant_curve_would_win_with_a_non_strict_comparison() {
    let (want, got) = (
        hit(&corner(), false, [50.0, 50.0]),
        flawed(Flaw::TieLast, &corner(), false, [50.0, 50.0]),
    );
    assert_eq!((want.curve_index, got.curve_index), (0, 1));
    let sq = (
        hit(&square(), true, [50.0, 50.0]).curve_index,
        flawed(Flaw::TieLast, &square(), true, [50.0, 50.0]).curve_index,
    );
    assert_eq!(sq, (0, 3));
}

#[test]
fn skipping_the_hand_over_keeps_the_corner_on_the_first_curve() {
    let want = hit(&corner(), false, [100.0, 0.0]);
    let got = flawed(Flaw::NoHandOver, &corner(), false, [100.0, 0.0]);
    assert_eq!((want.curve_index, want.time), (1, 0.0));
    assert_eq!((got.curve_index, got.time), (0, 1.0));
    let late = flawed(Flaw::HandOverAt09, &corner(), false, [100.0, 50.0]);
    assert_eq!(
        late.curve_index, 1,
        "time 0.5 is below 0.9, only the corner case moves"
    );
    let close = flawed(Flaw::HandOverAt09, &bowl(), false, [105.0, 10.0]);
    assert_eq!(
        close.curve_index, 0,
        "an open path's last curve never hands over"
    );
}

#[test]
fn the_offset_needs_both_the_previous_lengths_and_the_part_length() {
    let want = hit(&corner(), false, [100.0, 50.0]).offset;
    let part = flawed(Flaw::NoPartLength, &corner(), false, [100.0, 50.0]).offset;
    let before = flawed(Flaw::NoPreviousLengths, &corner(), false, [100.0, 50.0]).offset;
    assert!(
        (want - 150.0).abs() < 1e-9 && (part - 100.0).abs() < 1e-9 && (before - 50.0).abs() < 1e-9
    );
}

#[test]
fn a_straight_curve_answers_with_the_smoothstep_time_not_the_fraction() {
    let want = hit(&horizontal(), false, [30.0, 40.0]).time;
    let linear = flawed(Flaw::LinearStraightTime, &horizontal(), false, [30.0, 40.0]).time;
    assert!((want - 0.36325749109056765).abs() < 1e-12);
    assert!((linear - 0.3).abs() < 1e-12);
    let sampled = flawed(Flaw::StraightBySampling, &horizontal(), false, [30.0, 40.0]).time;
    assert!(
        (sampled - want).abs() > 1e-9,
        "sampling cannot reproduce the solved root exactly"
    );
}

#[test]
fn a_missing_or_coarse_refinement_moves_a_curved_result() {
    // Paper samples 101 times and then refines down to a step of 1e-8; without the refinement the time is
    // a multiple of 0.01 (error 4.8e-3 here), with a floor of 1e-3 it is off by 1.6e-4.
    let q = [-20.0, 10.0];
    let want = hit(&bowl(), false, q);
    let no_refine = flawed(Flaw::NoRefine, &bowl(), false, q);
    assert!(
        (no_refine.time - want.time).abs() > 1e-3,
        "{}",
        no_refine.time
    );
    assert!(
        no_refine.distance > want.distance,
        "a coarser time is never nearer"
    );
    let coarse = flawed(Flaw::CoarseRefineFloor, &bowl(), false, q);
    assert!((coarse.time - want.time).abs() > 1e-5, "{}", coarse.time);
    // 11 samples alone are repaired by the refinement on a smooth curve; only some rows notice.
    let few = flawed(Flaw::CoarseSamples, &bowl(), false, q);
    assert!((few.time - want.time).abs() < 1e-6);
    let squared = flawed(Flaw::SquaredDistance, &horizontal(), false, [30.0, 40.0]).distance;
    assert!((squared - 1600.0).abs() < 1e-9, "{squared}");
}

#[test]
fn the_oracle_is_not_vacuous() {
    let outs: Vec<String> = rows_of("nst")
        .map(|(_, r)| r.rsplit(' ').next().unwrap_or("").to_string())
        .collect();
    let nulls = outs.iter().filter(|o| o.as_str() == "null").count();
    let unresolved = outs.iter().filter(|o| o.as_str() == "unres").count();
    assert!(nulls >= 20, "null rows: {nulls}");
    assert!(unresolved >= 5, "unresolved rows: {unresolved}");
    let index = |o: &String| o.split('/').nth(3).and_then(|i| i.parse::<usize>().ok());
    let first = outs.iter().filter(|o| index(o) == Some(0)).count();
    let later = outs
        .iter()
        .filter(|o| index(o).is_some_and(|i| i >= 2))
        .count();
    assert!(first >= 100, "first-curve rows: {first}");
    assert!(later >= 30, "later-curve rows: {later}");
    let closed = rows_of("nst")
        .filter(|(_, r)| r.split(' ').nth(2) == Some("1"))
        .count();
    assert!(closed >= 100, "closed-path rows: {closed}");
    assert!(
        ORACLE.lines().any(|r| r.starts_with("nst x")),
        "bit-exact nst rows"
    );
    assert!(
        ORACLE.lines().any(|r| r.starts_with("nst u")),
        "tolerant nst rows"
    );
}
