use crate::animation_curve::evaluate_curve;
use crate::evaluation::property_expression_bake::{optimize, BakeError, BakedTrack};
use std::cell::Cell;

fn series(end: u32, value: impl Fn(f64) -> Vec<f64>) -> Vec<Vec<f64>> {
    (0..=end).map(|f| value(f64::from(f))).collect()
}

fn sample(track: &BakedTrack, frame: f64) -> Vec<f64> {
    if let Some(value) = &track.static_value {
        return value.clone();
    }
    let pair = track
        .keys
        .windows(2)
        .find(|keys| frame <= f64::from(keys[1].frame))
        .unwrap_or_else(|| &track.keys[track.keys.len() - 2..]);
    let a = &pair[0];
    let b = &pair[1];
    let t = (frame - f64::from(a.frame)) / f64::from(b.frame - a.frame);
    let y = evaluate_curve(&a.curve, t);
    a.values
        .iter()
        .zip(&b.values)
        .map(|(a, b)| a + (b - a) * y)
        .collect()
}

fn assert_reconstruction(track: &BakedTrack, start: u32, samples: &[Vec<f64>], fraction: f64) {
    let epsilon: Vec<_> = (0..samples[0].len())
        .map(|d| {
            let lo = samples.iter().map(|v| v[d]).fold(f64::INFINITY, f64::min);
            let hi = samples
                .iter()
                .map(|v| v[d])
                .fold(f64::NEG_INFINITY, f64::max);
            ((hi - lo) * fraction).max(1e-4)
        })
        .collect();
    for (i, expected) in samples.iter().enumerate() {
        for (d, actual) in sample(track, f64::from(start) + i as f64)
            .iter()
            .enumerate()
        {
            assert!(
                (actual - expected[d]).abs() <= epsilon[d] + 1e-8,
                "sample {i} axis {d}: {actual} != {} within {}",
                expected[d],
                epsilon[d]
            );
        }
    }
}

#[test]
fn constant_expression_uses_static_value_and_no_keys() {
    let samples = series(100, |_| vec![42.0, -7.0]);
    let baked = optimize(0, &samples, 0.005, &|| false).unwrap();
    assert_eq!(baked.static_value, Some(vec![42.0, -7.0]));
    assert!(baked.keys.is_empty());
    assert_eq!((baked.sampled_frames, baked.curved_segments), (101, 0));
}

#[test]
fn linear_vector_ramp_has_exact_endpoints_and_linear_fractional_samples() {
    let samples = series(48, |f| vec![10.0 + 3.5 * f, 20.0 - 2.0 * f]);
    let baked = optimize(12, &samples, 0.005, &|| false).unwrap();
    assert_eq!(baked.keys.len(), 2);
    assert_eq!((baked.keys[0].frame, baked.keys[1].frame), (12, 60));
    assert_eq!(baked.keys[0].values, samples[0]);
    assert_eq!(baked.keys[1].values, samples[48]);
    assert_eq!(baked.curved_segments, 0);
    let value = sample(&baked, 31.25);
    assert!((value[0] - (10.0 + 3.5 * 19.25)).abs() < 1e-9);
    assert!((value[1] - (20.0 - 2.0 * 19.25)).abs() < 1e-9);
    assert_reconstruction(&baked, 12, &samples, 0.005);
}

#[test]
fn overshoot_is_retained_in_unclamped_curve_output() {
    let samples = series(24, |f| {
        let t = f / 24.0;
        vec![if f == 0.0 {
            0.0
        } else {
            100.0 * (1.0 + 0.3 * (t * std::f64::consts::PI).sin() * (1.0 - t))
        }]
    });
    let baked = optimize(0, &samples, 0.005, &|| false).unwrap();
    assert!(baked.keys.iter().flat_map(|k| &k.curve).any(|p| p.y > 1.0));
    assert_reconstruction(&baked, 0, &samples, 0.005);
}

#[test]
fn sine_cycle_reduces_real_keys_and_bounds_every_original_sample() {
    let samples = series(24, |f| {
        vec![100.0 * (f * std::f64::consts::TAU / 24.0).sin()]
    });
    let baked = optimize(0, &samples, 0.005, &|| false).unwrap();
    assert!(
        baked.keys.len() < 16,
        "curve merging must earn a real reduction"
    );
    assert_reconstruction(&baked, 0, &samples, 0.005);
}

#[test]
fn axes_with_distinct_extrema_cannot_hide_each_others_error() {
    let samples = series(48, |f| vec![50.0 * (f / 8.0).sin(), 0.01 * (f / 5.0).cos()]);
    let baked = optimize(0, &samples, 0.005, &|| false).unwrap();
    assert_reconstruction(&baked, 0, &samples, 0.005);
    assert!(baked.keys.iter().all(|key| key.curve.len() <= 24));
}

#[test]
fn near_constant_uses_the_original_twice_epsilon_rule() {
    let samples = vec![vec![3.0], vec![3.00005], vec![3.0001]];
    let baked = optimize(0, &samples, 0.005, &|| false).unwrap();
    assert_eq!(baked.static_value, Some(vec![3.0]));
    assert!(baked.keys.is_empty());
}

#[test]
fn cancellation_during_reduction_returns_no_candidate() {
    let samples = series(100, |f| vec![(f / 2.0).sin()]);
    let calls = Cell::new(0);
    let cancelled = || {
        calls.set(calls.get() + 1);
        calls.get() > 7
    };
    assert_eq!(
        optimize(0, &samples, 0.005, &cancelled),
        Err(BakeError::Cancelled)
    );
    assert_eq!(samples, series(100, |f| vec![(f / 2.0).sin()]));
    assert_eq!(
        optimize(0, &samples, 0.005, &|| true),
        Err(BakeError::Cancelled)
    );
}

#[test]
fn malformed_samples_and_frame_overflow_cannot_become_tracks() {
    for samples in [
        vec![],
        vec![vec![1.0]],
        vec![vec![], vec![]],
        vec![vec![1.0], vec![2.0, 3.0]],
        vec![vec![1.0], vec![f64::NAN]],
    ] {
        assert_eq!(
            optimize(0, &samples, 0.005, &|| false),
            Err(BakeError::InvalidSamples)
        );
    }
    assert_eq!(
        optimize(u32::MAX, &[vec![0.0], vec![1.0]], 0.005, &|| false),
        Err(BakeError::InvalidSamples)
    );
    assert_eq!(
        optimize(0, &[vec![0.0], vec![1.0]], f64::INFINITY, &|| false),
        Err(BakeError::InvalidSamples)
    );
}
