//! Contract pins of the Paper nearest-location port (L03-I): the copied curve primitives against the D30
//! follow-path sampler they were copied from, the root finders on hand cases, and the typed edges of the
//! kernel. Literals come from `tools/literals.cjs` / Paper's static API (`Curve.getLength`, `Curve.getPoint`,
//! `Curve.isStraight`, `Curve.getTimeOf`, `Curve.getNearestTime`, `Numerical.solveQuadratic`/`solveCubic`).

use super::tests::{approx, bits, bowl, exact, hit, horizontal, seg, seg_h, square};
use super::{curve_hit, hand_over, nearest_on_curves, nearest_time, NearestError};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_curve::{
    is_straight, point_at, range_length, Values,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_solve::{
    solve_cubic, solve_quadratic, time_of,
};
use crate::evaluation::property_follow_path_sample::{path_curves, path_length};

const ZERO: Values = [0.0, 0.0, 0.0, 0.0, 100.0, 0.0, 100.0, 0.0];
const COLLINEAR: Values = [0.0, 0.0, 30.0, 0.0, 70.0, 0.0, 100.0, 0.0];
const PAST_CHORD: Values = [0.0, 0.0, 150.0, 0.0, -50.0, 0.0, 100.0, 0.0];
const OPPOSITE: Values = [0.0, 0.0, -40.0, 0.0, 140.0, 0.0, 100.0, 0.0];
const POINT: Values = [7.0; 8];
const POINT_HANDLES: Values = [7.0, 7.0, 17.0, 7.0, 7.0, 17.0, 7.0, 7.0];
const BOWL: Values = [0.0, 0.0, 0.0, 80.0, 100.0, 80.0, 100.0, 0.0];
const DIAGONAL: Values = [10.0, 10.0, 10.0, 10.0, 210.0, 110.0, 210.0, 110.0];

fn rel(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-12 * a.abs().max(b.abs())
}

#[test]
fn the_copied_curve_length_equals_the_follow_path_length_bit_for_bit() {
    for v in [
        ZERO,
        COLLINEAR,
        PAST_CHORD,
        OPPOSITE,
        POINT,
        POINT_HANDLES,
        BOWL,
        DIAGONAL,
    ] {
        assert_eq!(
            bits(range_length(&v, 0.0, 1.0)),
            bits(path_length(&[v])),
            "{v:?}"
        );
    }
}

#[test]
fn curve_lengths_match_paper() {
    assert_eq!(bits(range_length(&ZERO, 0.0, 1.0)), bits(100.0));
    assert_eq!(
        bits(range_length(&ZERO, 0.0, 0.3)),
        bits(21.599999999999998)
    );
    assert_eq!(bits(range_length(&ZERO, 0.25, 0.75)), bits(68.75));
    assert_eq!(
        bits(range_length(&COLLINEAR, 0.0, 0.3)),
        bits(29.159999999999997)
    );
    assert_eq!(bits(range_length(&OPPOSITE, 0.25, 0.75)), bits(91.25));
    assert_eq!(
        bits(range_length(&DIAGONAL, 0.0, 1.0)),
        bits(223.60679774997897)
    );
    assert_eq!(
        bits(range_length(&PAST_CHORD, 0.0, 1.0)),
        bits(138.80776894388677)
    );
    assert_eq!(
        bits(range_length(&BOWL, 0.0, 1.0)),
        bits(174.74190834202128)
    );
    assert_eq!(
        bits(range_length(&BOWL, 0.25, 0.75)),
        bits(77.1586365459966)
    );
    assert_eq!(
        bits(range_length(&POINT_HANDLES, 0.0, 0.3)),
        bits(5.145222671357154)
    );
    for v in [ZERO, BOWL, POINT, DIAGONAL] {
        assert_eq!(range_length(&v, 0.0, 0.0), 0.0, "{v:?}");
    }
}

#[test]
fn is_straight_follows_the_handles_and_the_chord() {
    assert!(is_straight(&ZERO), "zero handles");
    assert!(is_straight(&COLLINEAR), "collinear and inside the chord");
    assert!(is_straight(&DIAGONAL), "zero handles on a diagonal");
    assert!(
        is_straight(&POINT),
        "a zero-length curve with zero handles is straight"
    );
    assert!(!is_straight(&PAST_CHORD), "handles beyond the chord");
    assert!(
        !is_straight(&OPPOSITE),
        "handles pointing away from the chord"
    );
    assert!(
        !is_straight(&POINT_HANDLES),
        "handles on a zero-length chord"
    );
    assert!(!is_straight(&BOWL), "handles off the chord");
}

#[test]
fn point_at_pins() {
    assert_eq!(point_at(&BOWL, 0.0), Some([0.0, 0.0]));
    assert_eq!(point_at(&BOWL, 1.0), Some([100.0, 0.0]));
    assert_eq!(point_at(&BOWL, 0.5), Some([50.0, 60.0]));
    assert_eq!(point_at(&BOWL, 0.25), Some([15.625, 45.0]));
    assert_eq!(point_at(&BOWL, -0.1), None);
    assert_eq!(point_at(&BOWL, 1.0000001), None);
    // A handle within 1e-12 of its anchor collapses onto it.
    let collapsed: Values = [0.0, 0.0, 1e-13, 0.0, 100.0, 0.0, 100.0, 0.0];
    assert_eq!(
        point_at(&collapsed, 1e-9).map(|p| p[0].to_bits()),
        Some(2.999999998000001e-16_f64.to_bits())
    );
}

#[test]
fn time_of_pins() {
    assert_eq!(time_of(&ZERO, [0.0, 0.0]), Some(0.0));
    assert_eq!(
        time_of(&ZERO, [50.0, 1.0]),
        None,
        "a point off the curve has no time"
    );
    assert!(rel(time_of(&ZERO, [100.0, 1e-8]).expect("time"), 1.0));
    assert!(rel(time_of(&ZERO, [100.0000001, 0.0]).expect("time"), 1.0));
    assert!(rel(time_of(&ZERO, [50.0, 0.0]).expect("time"), 0.5));
    assert!(rel(
        time_of(&ZERO, [1e-8, 0.0]).expect("time"),
        0.000005773513803060826
    ));
    assert!(rel(
        time_of(&ZERO, [1e-6, 0.0]).expect("time"),
        0.00005773613808353534
    ));
    assert!(rel(
        nearest_time(&ZERO, [30.0, 40.0]).expect("time"),
        0.36325749109056765
    ));
    assert_eq!(
        nearest_time(&BOWL, [50.0, 40.0]).map(f64::to_bits),
        Some(0.5_f64.to_bits())
    );
    assert_eq!(nearest_time(&POINT, [50.0, 40.0]), Some(0.0));
}

#[test]
fn quadratic_roots_pins() {
    let mut r = [f64::NAN; 3];
    assert_eq!(solve_quadratic(1.0, -3.0, 2.0, &mut r, 0.0, 3.0), 2);
    assert_eq!((r[0], r[1]), (2.0, 1.0));
    assert!(r[2].is_nan(), "a slot past the count is untouched");
    let mut r = [f64::NAN; 3];
    assert_eq!(
        solve_quadratic(1.0, -3.0, 2.0, &mut r, 0.0, 1.5),
        1,
        "a root past the bound is dropped"
    );
    assert_eq!(r[0], 1.0);
    let mut r = [f64::NAN; 3];
    assert_eq!(
        solve_quadratic(1.0, -2.0, 1.0, &mut r, 0.0, 2.0),
        1,
        "a double root counts once"
    );
    assert_eq!(r[0], 1.0);
    let mut r = [f64::NAN; 3];
    assert_eq!(
        solve_quadratic(0.0, 2.0, -1.0, &mut r, 0.0, 1.0),
        1,
        "a linear equation"
    );
    assert_eq!(r[0], 0.5);
    let mut r = [f64::NAN; 3];
    assert_eq!(
        solve_quadratic(0.0, 0.0, 0.0, &mut r, 0.0, 1.0),
        -1,
        "every coefficient zero"
    );
    assert_eq!(solve_quadratic(0.0, 0.0, 3.0, &mut r, 0.0, 1.0), 0);
    assert_eq!(
        solve_quadratic(1.0, 0.0, 1.0, &mut r, -5.0, 5.0),
        0,
        "no real root"
    );
}

#[test]
fn cubic_roots_pins() {
    // (t - 0.25)(t - 0.5)(t - 0.75): the quadratic factor first, the Newton root last.
    let mut r = [f64::NAN; 3];
    assert_eq!(
        solve_cubic(1.0, -1.5, 0.6875, -0.09375, &mut r, 0.0, 1.0),
        3
    );
    for (got, want) in r
        .iter()
        .zip([0.7499999999999996, 0.5000000000000004, 0.24999999999999992])
    {
        assert!(rel(*got, want), "{got} vs {want}");
    }
    // The smoothstep inverse: -2 t^3 + 3 t^2 = v.
    let mut r = [f64::NAN; 3];
    assert_eq!(solve_cubic(-2.0, 3.0, 0.0, -0.3, &mut r, 0.0, 1.0), 1);
    assert!(rel(r[0], 0.3632574910905676), "{}", r[0]);
    let mut r = [f64::NAN; 3];
    assert_eq!(solve_cubic(-2.0, 3.0, 0.0, -1.0, &mut r, 0.0, 1.0), 1);
    assert!(rel(r[0], 1.0), "{}", r[0]);
    // Every root outside the bounds: nothing.
    let mut r = [f64::NAN; 3];
    assert_eq!(
        solve_cubic(1.0, -1.5, 0.6875, -0.09375, &mut r, 0.8, 1.0),
        0
    );
}

#[test]
fn a_stale_roots_entry_hides_the_newton_root() {
    // a = 1: a quadratic root 0.9321... and a Newton root 0.2046...; with the Newton root already in the
    // array the duplicate test drops it, exactly as Paper does with its shared `roots` array.
    let (a, b, c, d) = (
        1.0,
        -2.068944621272385,
        1.2504336539493375,
        -0.1778267660247176,
    );
    let mut clean = [f64::NAN; 3];
    assert_eq!(solve_cubic(a, b, c, d, &mut clean, 0.0, 1.0), 2);
    assert!(rel(clean[0], 0.9321423958055676) && rel(clean[1], 0.2046598296612501));
    let mut stale = [clean[1], clean[1], f64::NAN];
    assert_eq!(solve_cubic(a, b, c, d, &mut stale, 0.0, 1.0), 1);
    assert!(rel(stale[0], 0.9321423958055676));
}

#[test]
fn no_curve_or_no_finite_distance_is_none() {
    let origin = [10.0, 10.0];
    assert_eq!(
        super::nearest_location(&[seg(5.0, 5.0)], false, origin),
        Ok(None)
    );
    assert_eq!(super::nearest_location(&[], false, origin), Ok(None));
    assert_eq!(super::nearest_location(&[], true, origin), Ok(None));
    for q in [[f64::NAN, 1.0], [1.0, f64::NAN], [f64::INFINITY, 0.0]] {
        assert_eq!(
            super::nearest_location(&horizontal(), false, q),
            Ok(None),
            "{q:?}"
        );
        assert_eq!(
            super::nearest_location(&bowl(), false, q),
            Ok(None),
            "{q:?}"
        );
    }
}

#[test]
fn one_closed_segment_is_a_curve_and_coincident_points_are_time_zero() {
    let one = [seg(5.0, 5.0)];
    exact(
        &hit(&one, true, [10.0, 10.0]),
        [5.0, 5.0],
        0.0,
        7.0710678118654755,
        0,
        0.0,
    );
    let same = [seg(7.0, 7.0), seg(7.0, 7.0)];
    exact(
        &hit(&same, false, [10.0, 10.0]),
        [7.0, 7.0],
        0.0,
        4.242640687119285,
        0,
        0.0,
    );
    // One closed segment with handles is a loop curve; its apex is (0, 22.5) at time 0.5.
    let lp = [seg_h(0.0, 0.0, Some([-30.0, 30.0]), Some([30.0, 30.0]))];
    exact(
        &hit(&lp, true, [0.0, 22.5]),
        [0.0, 22.5],
        30.55040843247031,
        0.0,
        0,
        0.5,
    );
    exact(
        &hit(&lp, true, [0.0, 40.0]),
        [0.0, 22.5],
        30.55040843247031,
        17.5,
        0,
        0.5,
    );
    exact(
        &hit(&lp, true, [25.0, 25.0]),
        [8.105892821037783, 17.871014556597203],
        20.662982944666695,
        18.336665204625174,
        0,
        0.27321117401123046,
    );
}

#[test]
fn an_unresolved_winner_is_a_typed_error_and_an_unresolved_loser_is_not() {
    let huge = [
        seg(360198498172777.94, -482254505149207.3),
        seg(359787303874402.94, -481901452771109.25),
    ];
    let q = [360520900182013.94, -481821864446099.06];
    assert_eq!(
        super::nearest_location(&huge, false, q),
        Err(NearestError::TimeUnresolved { curve_index: 0 })
    );
    // The same kind of curve first, then a curve 5 away from the query: the bogus distance of the first
    // location is the query's distance to the origin (22.7), so the resolved curve 1 wins.
    let mixed = [
        seg(458263657754.2871, -449008572380.9898),
        seg(242125505814.3288, 193633568473.16028),
        seg(10.581537996884435, 25.337662707082927),
    ];
    let q2 = [7.581537996884435, 21.337662707082927];
    let first = curve_hit(&path_curves(&mixed, false)[0], q2);
    assert_eq!((first.time, first.point), (None, None));
    assert_eq!(
        bits(first.distance),
        bits((q2[0] * q2[0] + q2[1] * q2[1]).sqrt())
    );
    let l = hit(&mixed, false, q2);
    approx(
        &l,
        [10.581537996884435, 25.337662707082927],
        988045398834.4478,
        5.0,
        1,
        1.0,
    );
}

#[test]
fn prebuilt_curves_and_segments_give_the_same_location() {
    let rest = bowl();
    let curves: Vec<Values> = path_curves(&rest, false);
    for q in [[50.0, 40.0], [-20.0, 10.0], [120.0, 60.0]] {
        assert_eq!(
            nearest_on_curves(&curves, false, q),
            super::nearest_location(&rest, false, q)
        );
    }
    let sq = path_curves(&square(), true);
    assert_eq!(
        nearest_on_curves(&sq, true, [50.0, 1.0]),
        super::nearest_location(&square(), true, [50.0, 1.0])
    );
}

#[test]
fn the_hand_over_table() {
    // (count, closed, index, time) -> (index, time)
    assert_eq!(hand_over(3, false, 0, 0.99999999), (1, 0.0));
    assert_eq!(hand_over(3, false, 0, 0.999999989), (0, 0.999999989));
    assert_eq!(hand_over(3, false, 1, 1.0), (2, 0.0));
    assert_eq!(hand_over(3, false, 2, 1.0), (2, 1.0));
    assert_eq!(hand_over(3, true, 2, 1.0), (0, 0.0));
    assert_eq!(hand_over(1, true, 0, 1.0), (0, 0.0));
    assert_eq!(hand_over(1, false, 0, 1.0), (0, 1.0));
    assert_eq!(hand_over(2, true, 1, 0.5), (1, 0.5));
}

#[test]
fn a_zero_length_chord_and_the_projection_bounds_pick_the_end_times() {
    assert_eq!(nearest_time(&POINT, [100.0, 100.0]), Some(0.0));
    assert_eq!(nearest_time(&ZERO, [-5.0, 3.0]), Some(0.0));
    assert_eq!(nearest_time(&ZERO, [1e-13, 3.0]), Some(0.0));
    assert_eq!(nearest_time(&ZERO, [100.0 - 1e-13, 3.0]), Some(1.0));
    assert_eq!(nearest_time(&ZERO, [150.0, 3.0]), Some(1.0));
}
