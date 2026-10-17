//! Tests for the weight easer and the Wiggly noise of the text range-selector
//! kernel (G38b / L06-G): the `B`, `H` and `N` oracle rows plus the properties
//! the baseline relies on (identity easing, integer-time noise, range).

use super::super::oracle::{rows, same, Row, V8_TOLERANCE};
use super::{hash_rand, smooth_noise_1d, Easer};

#[test]
fn oracle_easer_matches_the_baseline_bit_for_bit() {
    let mut checked = 0;
    for row in rows() {
        if let Row::Bezier { p, x, y } = row {
            let got = Easer::new(p[0], p[1], p[2], p[3]).apply(x);
            assert!(same(got, y), "bezier {p:?} at {x}: {got:?} want {y:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 144);
}

#[test]
fn oracle_hash_and_noise_match_the_baseline_within_the_trig_bound() {
    let (mut hashes, mut noises) = (0, 0);
    for row in rows() {
        match row {
            Row::Hash { seed, i, want } => {
                let got = hash_rand(seed, i);
                assert!(
                    (got - want).abs() <= V8_TOLERANCE,
                    "hashRand({seed}, {i}): {got:?} want {want:?}"
                );
                hashes += 1;
            }
            Row::Noise { seed, t, want } => {
                let got = smooth_noise_1d(seed, t);
                assert!(
                    (got - want).abs() <= V8_TOLERANCE,
                    "smoothNoise1D({seed}, {t}): {got:?} want {want:?}"
                );
                noises += 1;
            }
            _ => {}
        }
    }
    assert_eq!((hashes, noises), (70, 33));
}

#[test]
fn the_identity_curve_returns_its_input_untouched() {
    let identity = Easer::new(0.0, 0.0, 1.0, 1.0);
    for x in [0.0, 0.3, 1.0, -2.5, 1e-12] {
        assert!(same(identity.apply(x), x));
    }
    // -0 compares equal to 0, so a signed-zero control point is still the identity
    assert!(same(Easer::new(0.0, -0.0, 1.0, 1.0).apply(0.7), 0.7));
    assert!(identity.apply(f64::NAN).is_nan());
    // any other control point bends the curve, and the endpoints stay put
    let bent = Easer::new(0.0, 0.0, 0.2, 1.0);
    assert!(bent.apply(0.5) != 0.5);
    assert_eq!(bent.apply(0.0), 0.0);
    assert_eq!(bent.apply(1.0), 1.0);
}

#[test]
fn noise_at_an_integer_time_is_the_hash_of_that_integer() {
    // f = 0 gives u = 0, so the interpolation returns v0 = hashRand * 2 - 1 exactly
    for (seed, t) in [(1.0, 0.0), (7.0, 5.0), (9974.0, 37.0), (42.0, -3.0)] {
        let want = hash_rand(seed, t) * 2.0 - 1.0;
        assert!(same(smooth_noise_1d(seed, t), want), "seed {seed} t {t}");
    }
}

#[test]
fn hash_is_a_unit_fraction_and_noise_stays_in_range() {
    for seed in [1.0, 3.0, 42.0, 2.5] {
        for k in 0..200 {
            let h = hash_rand(seed, f64::from(k) - 50.0);
            assert!((0.0..1.0).contains(&h), "hash {h}");
            let n = smooth_noise_1d(seed, f64::from(k) * 0.37);
            assert!(n.abs() <= 1.0 + 1e-12, "noise {n}");
        }
    }
}

#[test]
fn a_large_argument_still_hashes_to_a_unit_fraction() {
    // 1e7 * 78.233 is far beyond one turn of the sine; the baseline answers and so does the kernel
    let h = hash_rand(1.0, 1.0e7);
    assert!((0.0..1.0).contains(&h), "hash {h}");
    assert!(smooth_noise_1d(1.0, 1.0e7).abs() <= 1.0 + 1e-12);
    assert!(
        same(hash_rand(2.0, 5.5e5), hash_rand(2.0, 5.5e5)),
        "deterministic"
    );
}
