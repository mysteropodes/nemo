//! nemo-test-harness: test-evaluation => expression_noise_tests
use crate::evaluation::expression_noise::{
    hash_unit, noise_1d, noise_2d, value_noise_1d, wiggle, wiggle_octave_count, Wiggle, WiggleDim,
};

// Desktop JavaScriptCore oracle bits (#1675). The fixed seed stands in for the
// per-session `ensureExprSeed` value, which callers provide.
const SEED: f64 = 12345.0;

/// Exact on macOS, the desktop target, where Rust `sin` matches
/// JavaScriptCore. Other hosts' libm may differ by an ulp of `sin`, which the
/// hash amplifies to about 1e-11; there the check is bounded, not exact.
fn assert_desktop_bits(actual: f64, expected_bits: u64, label: &str) {
    let expected = f64::from_bits(expected_bits);
    if cfg!(target_os = "macos") {
        assert_eq!(
            actual.to_bits(),
            expected_bits,
            "{label}: {actual:?} != {expected:?}"
        );
    } else {
        assert!(
            (actual - expected).abs() < 1e-9,
            "{label}: {actual:?} vs {expected:?}"
        );
    }
}

fn scalar(value: Wiggle) -> f64 {
    match value {
        Wiggle::Scalar(v) => v,
        Wiggle::Pair(_) => panic!("expected a scalar wiggle"),
    }
}

fn pair(value: Wiggle) -> [f64; 2] {
    match value {
        Wiggle::Pair(v) => v,
        Wiggle::Scalar(_) => panic!("expected a pair wiggle"),
    }
}

#[test]
fn hash_unit_matches_desktop_oracle() {
    assert_desktop_bits(
        hash_unit(1.0, 0.0),
        0.1829163520505972f64.to_bits(),
        "hash 1,0",
    );
    assert_desktop_bits(
        hash_unit(12345.0, 2.5),
        0.3715793339360971f64.to_bits(),
        "hash 12345,2.5",
    );
    assert_desktop_bits(
        hash_unit(123456789.0, -3.0),
        0.6950665551048587f64.to_bits(),
        "hash 123456789,-3",
    );
}

#[test]
fn hash_unit_keeps_floor_semantics_for_negative_products() {
    // sin(arg) < 0 here, so the product is negative: floor wraps into 0..1
    // where `fract()` would return a negative value.
    let v = (2.5f64 * 12.9898 + 12345.0 * 78.233).sin() * 43758.5453;
    assert!(v < 0.0);
    let h = hash_unit(12345.0, 2.5);
    assert_eq!(h.to_bits(), (v - v.floor()).to_bits());
    assert!((0.0..1.0).contains(&h));
    assert_ne!(h.to_bits(), v.fract().to_bits());
}

#[test]
fn noise_1d_matches_desktop_oracle() {
    let rows = [
        (0.0, 0.8798176499258261f64.to_bits()),
        (0.25, 0.7469014926707587f64.to_bits()),
        (1.5, 0.3120696015012072f64.to_bits()),
        // JavaScriptCore value; V8 gives 0.9639008453150382 here.
        (-2.75, 0x3feed84695e53000),
    ];
    for (x, bits) in rows {
        assert_desktop_bits(noise_1d(SEED, x), bits, &format!("noise1d {x}"));
    }
}

#[test]
fn noise_2d_matches_desktop_oracle() {
    assert_desktop_bits(
        noise_2d(SEED, 0.5, 0.5),
        0.2309008685497247f64.to_bits(),
        "noise2d 0.5,0.5",
    );
    assert_desktop_bits(
        noise_2d(SEED, 1.25, -3.5),
        0.17017739236121088f64.to_bits(),
        "noise2d 1.25,-3.5",
    );
}

#[test]
fn wiggle_matches_desktop_oracle() {
    let one = |t, f, a, o| scalar(wiggle(SEED, t, f, a, o, WiggleDim::One));
    assert_desktop_bits(
        one(0.0, 2.0, 10.0, 1.0),
        6.5472671296447515f64.to_bits(),
        "wiggle1d t0",
    );
    assert_desktop_bits(
        one(0.5, 2.0, 10.0, 1.0),
        (-4.914466961054131f64).to_bits(),
        "wiggle1d t0.5",
    );
    assert_desktop_bits(
        one(1.25, 2.0, 10.0, 3.0),
        (-4.072305676018004f64).to_bits(),
        "wiggle1d o3",
    );

    let [x, y] = pair(wiggle(SEED, 0.5, 2.0, 10.0, 1.0, WiggleDim::Two));
    assert_desktop_bits(x, (-4.914466961054131f64).to_bits(), "wiggle2d t0.5 x");
    assert_desktop_bits(y, (-3.704864770261338f64).to_bits(), "wiggle2d t0.5 y");
    let [x, y] = pair(wiggle(SEED, 1.25, 3.0, 50.0, 4.0, WiggleDim::Two));
    assert_desktop_bits(x, 5.318944347485133f64.to_bits(), "wiggle2d o4 x");
    assert_desktop_bits(y, 15.787517639610089f64.to_bits(), "wiggle2d o4 y");
}

#[test]
fn wiggle_dimension_and_decorrelation() {
    for octaves in [1.0, 2.0, 4.0] {
        for step in 0..200 {
            let t = f64::from(step) * 0.037;
            let s = scalar(wiggle(SEED, t, 2.0, 10.0, octaves, WiggleDim::One));
            let [x, y] = pair(wiggle(SEED, t, 2.0, 10.0, octaves, WiggleDim::Two));
            assert_eq!(s.to_bits(), x.to_bits(), "dim 1 is axis 0 of dim 2");
            assert!(
                s.abs() <= 10.0 && y.abs() <= 10.0,
                "|wiggle| <= amp at t {t}"
            );
        }
    }
    let diverging = (0..200)
        .map(|step| {
            pair(wiggle(
                SEED,
                f64::from(step) * 0.037,
                2.0,
                10.0,
                1.0,
                WiggleDim::Two,
            ))
        })
        .filter(|[x, y]| x != y)
        .count();
    assert!(diverging > 190, "axes are independent streams");
}

#[test]
fn noise_is_deterministic_and_counter_free() {
    let first = (
        noise_1d(SEED, 3.3),
        noise_2d(SEED, -1.1, 7.9),
        wiggle(SEED, 2.2, 3.0, 5.0, 2.0, WiggleDim::Two),
    );
    for _ in 0..3 {
        let again = (
            noise_1d(SEED, 3.3),
            noise_2d(SEED, -1.1, 7.9),
            wiggle(SEED, 2.2, 3.0, 5.0, 2.0, WiggleDim::Two),
        );
        assert_eq!(format!("{first:?}"), format!("{again:?}"));
    }
}

#[test]
fn octave_count_follows_javascript_loop_bounds() {
    assert_eq!(wiggle_octave_count(f64::NAN), 1);
    assert_eq!(wiggle_octave_count(0.0), 1);
    assert_eq!(wiggle_octave_count(-3.0), 1);
    assert_eq!(wiggle_octave_count(0.4), 1);
    assert_eq!(wiggle_octave_count(1.0), 1);
    assert_eq!(wiggle_octave_count(2.5), 3);
    assert_eq!(wiggle_octave_count(4.0), 4);
    assert_eq!(wiggle_octave_count(f64::INFINITY), 1025);
    let at = |o| scalar(wiggle(SEED, 1.25, 2.0, 10.0, o, WiggleDim::One));
    assert_eq!(at(2.5).to_bits(), at(3.0).to_bits());
    // Settled amplitude long before overflow, as in JavaScript.
    assert_eq!(at(60.0).to_bits(), at(1000.0).to_bits());
    // At t = 1.25 and freq 2 the lattice coordinate overflows near octave
    // 1019; JavaScript returns NaN from then on for any input, and the cap
    // keeps +Infinity from looping.
    assert!(at(1024.0).is_nan() && at(1025.0).is_nan() && at(f64::INFINITY).is_nan());
}

#[test]
fn negative_controls_offsets_are_load_bearing() {
    // Without the +7717 noise offset, the oracle row is not reproduced.
    let unshifted = value_noise_1d(SEED, 0.25) * 2.0 - 1.0;
    assert!((unshifted - 0.7469014926707587).abs() > 1e-3);
    // Without the axis*101 offset, Y would replay X exactly.
    let [x, y] = pair(wiggle(SEED, 0.5, 2.0, 10.0, 1.0, WiggleDim::Two));
    let same_seed_y = (value_noise_1d(SEED, 0.5 * 2.0) - 0.5) * 2.0 * 10.0;
    assert_eq!(same_seed_y.to_bits(), x.to_bits());
    assert!((y - same_seed_y).abs() > 1e-3);
    // Without the o*977 offset, octave 1 would replay octave 0's stream.
    let two_octaves = scalar(wiggle(SEED, 0.5, 2.0, 10.0, 2.0, WiggleDim::One));
    let shared_seed = |x: f64| (value_noise_1d(SEED, x) - 0.5) * 2.0;
    let no_octave_offset = (shared_seed(1.0) + shared_seed(2.0) * 0.5) / 1.5 * 10.0;
    assert!((two_octaves - no_octave_offset).abs() > 1e-3);
}
