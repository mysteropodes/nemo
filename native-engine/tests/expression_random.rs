//! nemo-test-harness: test-evaluation => expression_random_tests
use crate::evaluation::expression_noise::{hash_unit, noise_1d};
use crate::evaluation::expression_random::{
    gauss_from_uniforms, property_seed, property_seed_offset, rand01, random_with, RandomBound,
    RandomStream, RandomValue,
};

// Desktop JavaScriptCore oracle bits (#1746). The fixed holder seed stands in
// for the per-session `ensureExprSeed` value, which callers provide.
const HOLDER_SEED: f64 = 12345.0;

/// Exact on macOS, the desktop target, where Rust `sin`/`ln`/`cos` matched
/// JavaScriptCore on every row. Another host's libm may differ by an ulp,
/// which the hash would amplify to about 1e-11, so there the check is bounded.
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

fn position_seed() -> f64 {
    property_seed(HOLDER_SEED, "position")
}

fn stream_at(frame: f64) -> RandomStream {
    RandomStream::new(HOLDER_SEED, "position", frame)
}

fn scalar(value: RandomValue) -> f64 {
    match value {
        RandomValue::Scalar(v) => v,
        RandomValue::Vector(_) => panic!("expected a scalar draw"),
    }
}

fn vector(value: RandomValue) -> Vec<f64> {
    match value {
        RandomValue::Vector(v) => v,
        RandomValue::Scalar(_) => panic!("expected a vector draw"),
    }
}

#[test]
fn property_offset_matches_javascript_wrapping() {
    let rows = [
        ("position", 747804969),
        ("rotation", -40300674),
        ("opacity", -1267206133),
        ("scale", 109250890),
        ("anchor", -1413299531),
        ("", 0),
        ("é", 233),
        ("xc_k3j9q2_Turn", 395623632),
        ("a_very_long_property_name_overflow_check", -992550643),
    ];
    for (prop, expected) in rows {
        assert_eq!(property_seed_offset(prop), expected, "offset {prop:?}");
    }
    assert_eq!(position_seed().to_bits(), 0x41c64964b1000000);
}

#[test]
fn rand01_matches_desktop_oracle() {
    let s = position_seed();
    let rows = [
        (0.0, 0.0, false, false, 0x3fe02bb1931a0000),
        (0.0, 12.0, false, false, 0x3fecfb05dfb38000),
        (3.0, 12.0, false, false, 0x3fcd2579047c0000),
        (1.0, 7.25, false, false, 0x3fec033180020000),
        (0.0, 0.0005, false, false, 0x3fd34536ddb40000),
        (0.0, -2.5005, false, false, 0x3fe043f5bcab0000),
        (2.0, 12.0, false, true, 0x3fe32a99a039e000),
        (2.0, 12.0, true, false, 0x3fe32a99a039e000),
        (0.0, 1e6 + 0.3, false, false, 0x3fe052d74d8d8000),
    ];
    for (counter, frame, timeless, fixed, bits) in rows {
        assert_desktop_bits(
            rand01(s, counter, frame, timeless, fixed),
            bits,
            &format!("rand01 c{counter} f{frame} t{timeless} x{fixed}"),
        );
    }
    assert_desktop_bits(
        rand01(8.0, 0.0, 12.0, false, false),
        0x3fe259819c518000,
        "rand01 seed(7)",
    );
    assert_desktop_bits(
        rand01(
            property_seed(123456789.0, "rotation"),
            5.0,
            30.0,
            false,
            false,
        ),
        0x3fdc0d1d28400000,
        "rand01 large seed",
    );
}

#[test]
fn gauss_matches_desktop_oracle() {
    let rows = [
        (0.25, 0.75, 0x3fdffffffffffffe),
        (0.5, 0.5, 0x3fc22f4355d0cdb4),
        (0.0, 0.3, 0xbfbad319ff054448),
        (1e-12, 0.9, 0x4000aab0580fe330),
        (0.999, 0.001, 0x3fe06f65f71f63a7),
        (0.3715793339360971, 0.1829163520505972, 0x3fe599b003939a89),
    ];
    for (u1, u2, bits) in rows {
        assert_desktop_bits(
            gauss_from_uniforms(u1, u2),
            bits,
            &format!("gauss {u1},{u2}"),
        );
    }
    // NaN propagates as in JavaScript instead of being clamped.
    assert!(gauss_from_uniforms(f64::NAN, 0.3).is_nan());
    // Below the clamp, the first uniform is read as 1e-9.
    assert_eq!(
        gauss_from_uniforms(0.0, 0.3).to_bits(),
        gauss_from_uniforms(1e-9, 0.3).to_bits()
    );
}

#[test]
fn random_with_shapes_match_desktop_oracle() {
    use RandomBound::{Scalar, Vector};
    let draw = |shape: (Option<RandomBound>, Option<RandomBound>)| {
        let mut stream = stream_at(12.0);
        random_with(|| stream.uniform(false), shape.0.as_ref(), shape.1.as_ref())
    };
    assert_desktop_bits(scalar(draw((None, None))), 0x3fecfb05dfb38000, "()");
    assert_desktop_bits(
        scalar(draw((Some(Scalar(10.0)), None))),
        0x40221ce3abd03000,
        "(10)",
    );
    assert_desktop_bits(
        scalar(draw((Some(Scalar(-5.0)), Some(Scalar(5.0))))),
        0x401039c757a06000,
        "(-5,5)",
    );
    let rows: [(Option<RandomBound>, Option<RandomBound>, &[u64]); 4] = [
        (
            Some(Vector(vec![100.0, 50.0])),
            None,
            &[0x4056a41c96c43c00, 0x40464264ea1a7000],
        ),
        (
            Some(Vector(vec![0.0, 0.0])),
            Some(Vector(vec![100.0, 50.0, 20.0])),
            &[0x4056a41c96c43c00, 0x40464264ea1a7000, 0x401f20adf1878000],
        ),
        (
            Some(Scalar(3.0)),
            Some(Vector(vec![10.0, 20.0])),
            &[0x4022add291de8800, 0x4031ceb721aec000],
        ),
        (Some(Vector(vec![])), None, &[]),
    ];
    for (a, b, bits) in rows {
        let label = format!("{a:?},{b:?}");
        let got = vector(draw((a, b)));
        assert_eq!(got.len(), bits.len(), "{label}");
        for (value, expected) in got.into_iter().zip(bits) {
            assert_desktop_bits(value, *expected, &label);
        }
    }
    // Non-finite bounds read as 0, like `_num`: "x" and NaN give 0 + draw * 0.
    assert_eq!(
        scalar(draw((Some(Scalar(f64::NAN)), Some(Scalar(f64::NAN))))).to_bits(),
        0
    );
}

#[test]
fn gauss_draws_match_desktop_oracle() {
    use RandomBound::{Scalar, Vector};
    let mut varying = stream_at(12.0);
    let value = scalar(random_with(
        || varying.gauss(false),
        Some(&Scalar(-5.0)),
        Some(&Scalar(5.0)),
    ));
    assert_desktop_bits(value, 0x3ff0b7e0b9d807b8, "gauss (-5,5)");
    assert_eq!(varying.counter(), 2);

    let mut fixed = stream_at(12.0);
    let values = vector(random_with(
        || fixed.gauss(true),
        Some(&Vector(vec![10.0, 10.0])),
        None,
    ));
    assert_desktop_bits(values[0], 0x3ff928aec8f9fa68, "gaussFixed [0]");
    assert_desktop_bits(values[1], 0x4003cf2dc315d6cb, "gaussFixed [1]");
    assert_eq!(fixed.counter(), 4);
}

#[test]
fn counter_discipline_follows_javascript() {
    let mut stream = stream_at(12.0);
    let first = stream.uniform(false);
    let second = stream.uniform(false);
    assert_ne!(
        first.to_bits(),
        second.to_bits(),
        "draws advance the counter"
    );
    assert_eq!(stream.counter(), 2);

    // Fixed and timeless drop the frame term: same value at every frame for a
    // given counter position, but still a new value per draw.
    let fixed_at = |frame: f64| {
        let mut s = stream_at(frame);
        (s.uniform(true), s.uniform(true))
    };
    let (a0, a1) = fixed_at(0.0);
    for frame in [1.0, 12.0, -3.5, 1e6] {
        let (b0, b1) = fixed_at(frame);
        assert_eq!((a0.to_bits(), a1.to_bits()), (b0.to_bits(), b1.to_bits()));
    }
    assert_ne!(a0.to_bits(), a1.to_bits());
    let mut timeless = stream_at(7.0);
    timeless.seed_random(41.0, true);
    let mut timeless_later = stream_at(90.0);
    timeless_later.seed_random(41.0, true);
    assert_eq!(
        timeless.uniform(false).to_bits(),
        timeless_later.uniform(false).to_bits()
    );

    // seed(n) starts stream n + 1 from counter 0 and keeps the timeless flag.
    stream.reseed(7.0);
    assert_eq!((stream.seed(), stream.counter()), (8.0, 0));
    assert!(!stream.timeless());
    assert_desktop_bits(stream.uniform(false), 0x3fe259819c518000, "after seed(7)");
    stream.reseed(f64::NAN);
    assert_eq!(stream.seed(), 1.0);
    assert!(timeless.timeless());
    timeless.reseed(3.0);
    assert!(timeless.timeless());
}

#[test]
fn properties_draw_independent_streams() {
    let mut position = RandomStream::new(HOLDER_SEED, "position", 12.0);
    let mut rotation = RandomStream::new(HOLDER_SEED, "rotation", 12.0);
    let differing = (0..50)
        .filter(|_| position.uniform(false).to_bits() != rotation.uniform(false).to_bits())
        .count();
    assert_eq!(differing, 50);
}

#[test]
fn noise_is_counter_free_where_random_is_not() {
    // `noise-vs-random-determinism`: identical noise calls agree; identical
    // random calls in one evaluation do not.
    let mut stream = stream_at(12.0);
    assert_eq!(
        noise_1d(HOLDER_SEED, 3.3).to_bits(),
        noise_1d(HOLDER_SEED, 3.3).to_bits()
    );
    assert_ne!(
        stream.uniform(true).to_bits(),
        stream.uniform(true).to_bits()
    );
}

#[test]
fn negative_controls_offset_and_rounding_are_load_bearing() {
    // Without the property offset, the oracle row is not reproduced.
    let unshifted = rand01(HOLDER_SEED, 0.0, 12.0, false, false);
    assert!((unshifted - f64::from_bits(0x3fecfb05dfb38000)).abs() > 1e-3);
    // Rust `round()` takes -2500.5 to -2501 where `Math.round` gives -2500.
    let s = position_seed();
    let rust_rounded = hash_unit(s, (-2500.5f64).round() / 1000.0 * 1013.13 + 0.5);
    assert!((rust_rounded - f64::from_bits(0x3fe043f5bcab0000)).abs() > 1e-3);
}
