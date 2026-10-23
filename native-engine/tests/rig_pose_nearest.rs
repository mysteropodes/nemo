//! Paper nearest-location kernel (L03-I, I12): oracle runs and hand-pinned cases.
//!
//! Every literal below was produced by `tools/literals.cjs` (the shipped Paper.js 0.12.18 executing
//! `_boneSegsToPath` + `Path.getNearestLocation` from the baseline at fbe4b8a3). Cases on a curve with
//! handles (non straight) use only `+ - * /` and `sqrt` and are compared bit for bit; cases that reach
//! `Curve.getTimeOf` (straight curves) are compared with the oracle's `u`-row tolerance.

use super::oracle_rows::{failures, rows_of, Production, TOL_REL};
use super::prim_rows;
use super::NearestLocation;
use crate::evaluation::property_follow_path_sample::SegmentIn;

/// Rows per kind in `oracle.txt` (2210 in all).
const ROW_COUNTS: [(&str, usize); 11] = [
    ("nst", 651),
    ("wgh", 151),
    ("rad", 74),
    ("abo", 42),
    ("str", 48),
    ("len", 192),
    ("pnt", 288),
    ("tmo", 156),
    ("nti", 208),
    ("slq", 140),
    ("slc", 260),
];

pub(super) fn seg(x: f64, y: f64) -> SegmentIn {
    SegmentIn {
        point: [x, y],
        handle_in: None,
        handle_out: None,
    }
}

pub(super) fn seg_h(x: f64, y: f64, hi: Option<[f64; 2]>, ho: Option<[f64; 2]>) -> SegmentIn {
    SegmentIn {
        point: [x, y],
        handle_in: hi,
        handle_out: ho,
    }
}

pub(super) fn hit(rest: &[SegmentIn], closed: bool, p: [f64; 2]) -> NearestLocation {
    super::nearest_location(rest, closed, p)
        .expect("resolved")
        .expect("a location")
}

pub(super) fn bits(x: f64) -> u64 {
    x.to_bits()
}

/// Bit-exact: point, offset, distance, curve index and time.
pub(super) fn exact(
    l: &NearestLocation,
    point: [f64; 2],
    offset: f64,
    distance: f64,
    index: usize,
    time: f64,
) {
    assert_eq!(
        [bits(l.point[0]), bits(l.point[1])],
        [bits(point[0]), bits(point[1])],
        "point {:?}",
        l.point
    );
    assert_eq!(bits(l.offset), bits(offset), "offset {}", l.offset);
    assert_eq!(bits(l.distance), bits(distance), "distance {}", l.distance);
    assert_eq!(l.curve_index, index, "curve index");
    assert_eq!(bits(l.time), bits(time), "time {}", l.time);
}

pub(super) fn near(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= TOL_REL * a.abs().max(b.abs()).max(100.0)
}

/// Same as [`exact`] with the `u`-row tolerance (the location came through `Curve.getTimeOf`).
pub(super) fn approx(
    l: &NearestLocation,
    point: [f64; 2],
    offset: f64,
    distance: f64,
    index: usize,
    time: f64,
) {
    assert!(
        near(l.point[0], point[0]) && near(l.point[1], point[1]),
        "point {:?}",
        l.point
    );
    assert!(near(l.offset, offset), "offset {}", l.offset);
    assert!(near(l.distance, distance), "distance {}", l.distance);
    assert_eq!(l.curve_index, index, "curve index");
    assert!(near(l.time, time), "time {}", l.time);
}

pub(super) fn horizontal() -> Vec<SegmentIn> {
    vec![seg(0.0, 0.0), seg(100.0, 0.0)]
}

pub(super) fn corner() -> Vec<SegmentIn> {
    vec![seg(0.0, 0.0), seg(100.0, 0.0), seg(100.0, 100.0)]
}

pub(super) fn square() -> Vec<SegmentIn> {
    vec![
        seg(0.0, 0.0),
        seg(100.0, 0.0),
        seg(100.0, 100.0),
        seg(0.0, 100.0),
    ]
}

pub(super) fn bowl() -> Vec<SegmentIn> {
    vec![
        seg_h(0.0, 0.0, None, Some([0.0, 80.0])),
        seg_h(100.0, 0.0, Some([0.0, 80.0]), None),
    ]
}

#[test]
fn oracle_row_counts_are_pinned() {
    let total: usize = ROW_COUNTS.iter().map(|(_, n)| n).sum();
    assert_eq!(total, 2210);
    assert_eq!(super::oracle_rows::ORACLE.lines().count(), total);
    for (kind, n) in ROW_COUNTS {
        assert_eq!(rows_of(kind).count(), n, "{kind} rows");
    }
}

#[test]
fn every_nearest_oracle_row_matches_production() {
    let bad = failures(&Production);
    assert!(
        bad.is_empty(),
        "{} failed, first: {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn every_primitive_oracle_row_matches_production() {
    let bad = prim_rows::failures();
    assert!(
        bad.is_empty(),
        "{} failed, first: {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn a_straight_bone_uses_the_smoothstep_time_not_the_chord_fraction() {
    // (30, 40) projects at chord fraction 0.3, but a zero-handle curve is x(t) = 100 (3t^2 - 2t^3).
    let l = hit(&horizontal(), false, [30.0, 40.0]);
    approx(
        &l,
        [30.000000000000007, 0.0],
        30.000000000000004,
        40.0,
        0,
        0.36325749109056765,
    );
    assert!(
        (l.time - 0.3).abs() > 0.06,
        "the time is not the chord fraction"
    );
    let q = hit(&horizontal(), false, [25.0, 3.0]);
    approx(
        &q,
        [25.0, 0.0],
        24.999999999999996,
        3.0,
        0,
        0.32635182233306964,
    );
    let on = hit(&horizontal(), false, [50.0, 0.0]);
    approx(&on, [50.0, 0.0], 50.0, 0.0, 0, 0.5);
}

#[test]
fn a_chord_projection_beyond_either_end_snaps_to_the_end() {
    exact(
        &hit(&horizontal(), false, [-30.0, 40.0]),
        [0.0, 0.0],
        0.0,
        50.0,
        0,
        0.0,
    );
    // The last curve of an OPEN path has nothing to hand over to: it keeps index 0, time 1.
    exact(
        &hit(&horizontal(), false, [130.0, -40.0]),
        [100.0, 0.0],
        100.0,
        50.0,
        0,
        1.0,
    );
    exact(
        &hit(&corner(), false, [120.0, 130.0]),
        [100.0, 100.0],
        200.0,
        36.05551275463989,
        1,
        1.0,
    );
}

#[test]
fn a_corner_hands_over_to_the_next_curve_with_the_same_offset() {
    // (100, 0) is the end of curve 0 (time 1) and the start of curve 1: strict '<' picks curve 0, then
    // the 0.99999999 hand-over moves it to curve 1 at time 0; the offset is the same place either way.
    exact(
        &hit(&corner(), false, [100.0, 0.0]),
        [100.0, 0.0],
        100.0,
        0.0,
        1,
        0.0,
    );
    exact(
        &hit(&corner(), false, [100.0, 50.0]),
        [100.0, 50.0],
        150.0,
        0.0,
        1,
        0.5,
    );
}

#[test]
fn equidistant_curves_resolve_to_the_first() {
    exact(
        &hit(&corner(), false, [50.0, 50.0]),
        [50.0, 0.0],
        50.0,
        50.0,
        0,
        0.5,
    );
    exact(
        &hit(&square(), true, [50.0, 50.0]),
        [50.0, 0.0],
        50.0,
        50.0,
        0,
        0.5,
    );
}

#[test]
fn closed_paths_use_the_closing_curve_and_wrap_to_curve_zero() {
    // The closing curve (0, 100) -> (0, 0) is curve 3: offset 300 + its part length.
    let l = hit(&square(), true, [-10.0, 40.0]);
    approx(
        &l,
        [0.0, 40.00000000000001],
        360.0,
        10.0,
        3,
        0.5670689228522682,
    );
    let tri = vec![seg(0.0, 0.0), seg(100.0, 0.0), seg(100.0, 100.0)];
    approx(
        &hit(&tri, true, [20.0, 5.0]),
        [20.0, 0.0],
        20.0,
        5.0,
        0,
        0.28714072541674046,
    );
}

#[test]
fn the_last_curve_of_a_closed_path_hands_over_to_the_first() {
    // Linear-parameter curves (handles at a third): the query projects at 1 - 1.1e-10 of the closing
    // curve, so the closed path reports curve 0 at time 0 and offset 0 while the OPEN reading of the same
    // segments keeps the original curve. Found by the generator with a Paper build that has no wrap.
    let rest = vec![
        seg_h(
            103.3,
            181.7,
            Some([9.89666666666667, -24.14333333333333]),
            Some([9.89666666666667, -24.14333333333333]),
        ),
        seg_h(
            132.99,
            109.27,
            Some([-9.89666666666667, 24.14333333333333]),
            Some([-9.89666666666667, 24.14333333333333]),
        ),
    ];
    let q = [139.09782137118094, 196.373993039492];
    let closed = hit(&rest, true, q);
    approx(
        &closed,
        [103.3000000031977, 181.69999999219908],
        0.0,
        38.68862994015217,
        0,
        0.0,
    );
    let open = hit(&rest, false, q);
    approx(
        &open,
        [103.30000000319768, 181.6999999921991],
        8.430844804259546e-9,
        38.688629940152175,
        0,
        1.0770238785620095e-10,
    );
}

#[test]
fn a_curved_bone_is_sampled_then_refined() {
    exact(
        &hit(&bowl(), false, [50.0, 40.0]),
        [50.0, 60.0],
        87.37095417102195,
        20.0,
        0,
        0.5,
    );
    exact(
        &hit(&bowl(), false, [50.0, 100.0]),
        [50.0, 60.0],
        87.37095417102195,
        40.0,
        0,
        0.5,
    );
    exact(
        &hit(&bowl(), false, [-20.0, 10.0]),
        [0.3622306005386586, 8.142320348600537],
        8.15318466960199,
        20.446794582934913,
        0,
        0.03516275405883789,
    );
}

#[test]
fn a_diagonal_zero_handle_bone_and_linear_third_handles() {
    let diag = vec![seg(10.0, 10.0), seg(210.0, 110.0)];
    approx(
        &hit(&diag, false, [110.0, 80.0]),
        [118.00000000000003, 64.00000000000001],
        120.74767078498866,
        17.88854381999832,
        0,
        0.5266920228100265,
    );
    // Handles at a third make the time the chord fraction (a straight curve with a linear parameter).
    let third = 33.333333333333336;
    let thirds = vec![
        seg_h(0.0, 0.0, None, Some([third, 0.0])),
        seg_h(100.0, 0.0, Some([-third, 0.0]), None),
    ];
    approx(
        &hit(&thirds, false, [25.0, -3.0]),
        [25.0, 0.0],
        25.0,
        3.0,
        0,
        0.25,
    );
}
