//! Pinned literals for `startTrack` scheduling and the volume curve plan.
//!
//! Inputs are chosen so every expected value is exactly representable and was
//! worked out by hand from `audio-bridge.js` (fbe4b8a3), independently of the
//! kernel; the oracle covers the broad cross-product.

use std::cell::RefCell;

use super::tests::Untouchable;
use super::{
    curve_len, flat_volume, plan_start, AudioTimingRefusal, AudioTrackTiming, NoVolumeSampler,
    PlaybackEnv, SilentReason, StartOutcome, TrackGain, TrackStart, TrackVolume, VolumeSampler,
};

/// A sampler from a closure.
pub(crate) struct FromFn<F>(pub F);

impl<F: Fn(f64) -> Option<f64>> VolumeSampler for FromFn<F> {
    fn volume_at(&self, frame: f64) -> Option<f64> {
        (self.0)(frame)
    }
}

pub(crate) fn env(fps: f64) -> PlaybackEnv {
    PlaybackEnv {
        fps,
        context_isolated: false,
        audio_context: true,
    }
}

/// A decoded 10 s track at the timeline origin, untrimmed, gain 1.
pub(crate) fn track() -> AudioTrackTiming {
    AudioTrackTiming {
        muted: false,
        buffer_duration: Some(10.0),
        offset_frames: f64::NAN,
        trim_start: f64::NAN,
        trim_end: None,
        volume: TrackVolume::Flat(1.0),
    }
}

pub(crate) fn play(e: &PlaybackEnv, t: &AudioTrackTiming, frame: f64) -> TrackStart {
    match plan_start(e, t, frame, &Untouchable) {
        Ok(StartOutcome::Play(s)) => s,
        other => panic!("expected a start, got {other:?}"),
    }
}

fn outcome(e: &PlaybackEnv, t: &AudioTrackTiming, frame: f64) -> StartOutcome {
    plan_start(e, t, frame, &Untouchable).expect("no refusal")
}

#[test]
fn later_start_maps_offset_trim_and_audible_frame() {
    // tSec = (12 - 24) / 24 = -0.5 is before trim.start = 1.5: audible 2 s later.
    let t = AudioTrackTiming {
        offset_frames: 24.0,
        trim_start: 1.5,
        trim_end: Some(4.0),
        ..track()
    };
    let s = play(&env(24.0), &t, 12.0);
    assert!(!s.starts_now);
    assert_eq!(s.delay_s, 2.0);
    assert_eq!(s.offset_s, 1.5);
    assert_eq!(s.duration_s, 2.5);
    assert_eq!(s.start_frame, 60.0);
    assert_eq!(s.gain, TrackGain::Value(1.0));
}

#[test]
fn immediate_start_reads_from_the_timeline_position() {
    // tSec = (72 - 24) / 24 = 2.0 is inside [1.5, 4): play now from 2.0.
    let t = AudioTrackTiming {
        offset_frames: 24.0,
        trim_start: 1.5,
        trim_end: Some(4.0),
        ..track()
    };
    let s = play(&env(24.0), &t, 72.0);
    assert!(s.starts_now);
    assert_eq!(s.delay_s, 0.0);
    assert_eq!(s.offset_s, 2.0);
    assert_eq!(s.duration_s, 2.0);
    assert_eq!(s.start_frame, 72.0);
}

#[test]
fn trim_end_unset_is_the_buffer_duration_and_zero_is_explicit() {
    let e = env(24.0);
    let short = AudioTrackTiming {
        buffer_duration: Some(3.25),
        ..track()
    };
    assert_eq!(play(&e, &short, 0.0).duration_s, 3.25);
    let cut = AudioTrackTiming {
        trim_end: Some(1.0),
        ..short
    };
    assert_eq!(play(&e, &cut, 0.0).duration_s, 1.0);
    // An explicit 0 is a value (JS only treats undefined/null as unset).
    let zero = AudioTrackTiming {
        trim_end: Some(0.0),
        ..short
    };
    assert_eq!(
        outcome(&e, &zero, 0.0),
        StartOutcome::Silent(SilentReason::PastTrimEnd)
    );
    // An empty decoded buffer ends at 0 as well.
    let empty = AudioTrackTiming {
        buffer_duration: Some(0.0),
        ..track()
    };
    assert_eq!(
        outcome(&e, &empty, 0.0),
        StartOutcome::Silent(SilentReason::PastTrimEnd)
    );
}

#[test]
fn end_boundary_is_silent_and_one_frame_earlier_plays() {
    let e = env(4.0);
    let t = AudioTrackTiming {
        trim_end: Some(2.0),
        ..track()
    };
    assert_eq!(
        outcome(&e, &t, 8.0),
        StartOutcome::Silent(SilentReason::PastTrimEnd)
    );
    let s = play(&e, &t, 7.0);
    assert_eq!((s.offset_s, s.duration_s), (1.75, 0.25));
}

#[test]
fn inverted_or_equal_trim_is_an_empty_window_not_playback() {
    let e = env(24.0);
    let inverted = AudioTrackTiming {
        trim_start: 2.0,
        trim_end: Some(1.0),
        ..track()
    };
    assert_eq!(
        outcome(&e, &inverted, 0.0),
        StartOutcome::Silent(SilentReason::EmptyWindow)
    );
    let equal = AudioTrackTiming {
        trim_start: 1.0,
        trim_end: Some(1.0),
        ..track()
    };
    assert_eq!(
        outcome(&e, &equal, 0.0),
        StartOutcome::Silent(SilentReason::EmptyWindow)
    );
    // Past the end the earlier comparison wins.
    assert_eq!(
        outcome(&e, &equal, 48.0),
        StartOutcome::Silent(SilentReason::PastTrimEnd)
    );
}

#[test]
fn playback_gates_run_muted_then_buffer_then_isolated_then_context() {
    // fps NaN would be a refusal: every gate answers before any validation.
    let mut e = PlaybackEnv {
        fps: f64::NAN,
        context_isolated: true,
        audio_context: false,
    };
    let mut t = AudioTrackTiming {
        muted: true,
        buffer_duration: None,
        ..track()
    };
    let silent = |e: &PlaybackEnv, t: &AudioTrackTiming| outcome(e, t, 0.0);
    assert_eq!(silent(&e, &t), StartOutcome::Silent(SilentReason::Muted));
    t.muted = false;
    assert_eq!(silent(&e, &t), StartOutcome::Silent(SilentReason::NoBuffer));
    t.buffer_duration = Some(10.0);
    assert_eq!(
        silent(&e, &t),
        StartOutcome::Silent(SilentReason::ContextIsolated)
    );
    e.context_isolated = false;
    assert_eq!(
        silent(&e, &t),
        StartOutcome::Silent(SilentReason::NoAudioContext)
    );
    e.audio_context = true;
    assert_eq!(
        plan_start(&e, &t, 0.0, &Untouchable),
        Err(AudioTimingRefusal::InvalidFps)
    );
}

#[test]
fn curve_length_is_clamped_to_two_and_two_thousand() {
    assert_eq!(curve_len(0.5, 24.0), 13);
    assert_eq!(curve_len(0.04, 25.0), 2);
    assert_eq!(curve_len(1e-300, 24.0), 2);
    assert_eq!(curve_len(1998.0, 1.0), 1999);
    assert_eq!(curve_len(1998.5, 1.0), 2000);
    assert_eq!(curve_len(1999.0, 1.0), 2000);
    assert_eq!(curve_len(2000.0, 1.0), 2000);
    assert_eq!(curve_len(f64::MAX, 2.0), 2000);
}

#[test]
fn curve_is_sampled_once_per_frame_from_the_audible_frame() {
    // tSec = (12 - 24) / 24 = -0.5: audible 0.5 s later, at frame 12 + 0.5 * 24.
    let t = AudioTrackTiming {
        offset_frames: 24.0,
        buffer_duration: Some(0.25),
        volume: TrackVolume::Animated,
        ..track()
    };
    let seen = RefCell::new(Vec::new());
    let sampler = FromFn(|f: f64| {
        seen.borrow_mut().push(f);
        Some(f - 26.0)
    });
    let start = match plan_start(&env(24.0), &t, 12.0, &sampler) {
        Ok(StartOutcome::Play(s)) => s,
        other => panic!("expected a start, got {other:?}"),
    };
    let frames: Vec<f64> = (24..=30).map(f64::from).collect();
    assert_eq!(*seen.borrow(), frames);
    assert_eq!(start.start_frame, 24.0);
    assert_eq!(start.delay_s, 0.5);
    let TrackGain::Curve(curve) = start.gain else {
        panic!("animated volume must yield a curve");
    };
    // max(0, f - 26): negatives clamp to +0, then rounded to f32.
    assert_eq!(curve.values, vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
    assert!(curve.values.iter().take(3).all(|v| v.to_bits() == 0));
    assert_eq!(curve.duration_s, 0.25);
}

#[test]
fn curve_values_are_f32_and_clamped_at_zero_only() {
    let t = AudioTrackTiming {
        buffer_duration: Some(0.1),
        volume: TrackVolume::Animated,
        ..track()
    };
    let curve_of = |v: f64| match plan_start(&env(24.0), &t, 0.0, &FromFn(|_: f64| Some(v))) {
        Ok(StartOutcome::Play(TrackStart {
            gain: TrackGain::Curve(c),
            ..
        })) => Ok(c.values),
        Err(e) => Err(e),
        other => panic!("unexpected {other:?}"),
    };
    // 0.1 is not an f32: the stored value is the nearest one (0x3dcccccd).
    let tenth = curve_of(0.1).expect("curve");
    assert_eq!(tenth.len(), 4);
    assert!(tenth.iter().all(|v| v.to_bits() == 0x3dcc_cccd));
    assert_ne!(f64::from(tenth[0]), 0.1);
    // Values above 1 are legal gains; only negatives clamp.
    assert!(curve_of(7.5).expect("curve").iter().all(|v| *v == 7.5));
    assert!(curve_of(-0.0)
        .expect("curve")
        .iter()
        .all(|v| v.to_bits() == 0));
    assert!(curve_of(f64::NEG_INFINITY)
        .expect("curve")
        .iter()
        .all(|v| v.to_bits() == 0));
    assert_eq!(
        curve_of(1e39),
        Err(AudioTimingRefusal::NonFiniteVolumeSample)
    );
    assert_eq!(
        curve_of(f64::INFINITY),
        Err(AudioTimingRefusal::NonFiniteVolumeSample)
    );
    assert_eq!(
        curve_of(f64::NAN),
        Err(AudioTimingRefusal::NonFiniteVolumeSample)
    );
}

#[test]
fn animated_volume_without_a_sampler_is_unavailable_never_a_default() {
    let t = AudioTrackTiming {
        buffer_duration: Some(1.0),
        volume: TrackVolume::Animated,
        ..track()
    };
    assert_eq!(
        plan_start(&env(24.0), &t, 0.0, &NoVolumeSampler),
        Err(AudioTimingRefusal::VolumeSamplerUnavailable)
    );
    // Unavailable part-way through the curve is still unavailable.
    let late = FromFn(|f: f64| if f >= 3.0 { None } else { Some(0.5) });
    assert_eq!(
        plan_start(&env(24.0), &t, 0.0, &late),
        Err(AudioTimingRefusal::VolumeSamplerUnavailable)
    );
}

#[test]
fn static_gain_is_resolved_in_baseline_order_and_not_clamped() {
    assert_eq!(flat_volume(Some(-0.5), Some(0.25)), -0.5);
    assert_eq!(flat_volume(Some(0.0), Some(0.5)), 0.0);
    assert_eq!(flat_volume(None, Some(0.0)), 0.0);
    assert_eq!(flat_volume(None, Some(0.25)), 0.25);
    assert_eq!(flat_volume(None, None), 1.0);
    // Flat tracks never ask the sampler (Untouchable panics if asked).
    let t = AudioTrackTiming {
        volume: TrackVolume::Flat(-0.5),
        ..track()
    };
    assert_eq!(play(&env(24.0), &t, 0.0).gain, TrackGain::Value(-0.5));
}

#[test]
fn absent_offset_and_trim_start_follow_js_or() {
    let e = env(24.0);
    let base = outcome(&e, &track(), 24.0);
    for absent in [f64::NAN, 0.0, -0.0] {
        let t = AudioTrackTiming {
            offset_frames: absent,
            trim_start: absent,
            ..track()
        };
        assert_eq!(outcome(&e, &t, 24.0), base);
    }
    let shifted = AudioTrackTiming {
        offset_frames: 24.0,
        ..track()
    };
    assert_ne!(outcome(&e, &shifted, 24.0), base);
}
