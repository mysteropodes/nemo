//! Pinned literals for the `scrubAt` blip, the start arguments' time base and
//! the typed refusals.
//!
//! Hand-derived from `audio-bridge.js` (fbe4b8a3); the oracle covers the broad
//! cross-product. The 40 ms scrub throttle (`_lastScrubT`) is deliberately not
//! in the kernel: it is a clock concern of the adapter.

use super::pins::{env, play, track, FromFn};
use super::tests::Untouchable;
use super::{
    plan_scrub, plan_start, AudioTimingRefusal, AudioTrackTiming, NoVolumeSampler, PlaybackEnv,
    ScrubBlip, ScrubOutcome, SilentReason, StartOutcome, TrackVolume,
};

fn blip(e: &PlaybackEnv, t: &AudioTrackTiming, frame: f64) -> ScrubBlip {
    match plan_scrub(e, t, frame, &Untouchable) {
        Ok(ScrubOutcome::Blip(b)) => b,
        other => panic!("expected a blip, got {other:?}"),
    }
}

fn scrub(
    e: &PlaybackEnv,
    t: &AudioTrackTiming,
    frame: f64,
) -> Result<ScrubOutcome, AudioTimingRefusal> {
    plan_scrub(e, t, frame, &Untouchable)
}

fn silent(why: SilentReason) -> Result<ScrubOutcome, AudioTimingRefusal> {
    Ok(ScrubOutcome::Silent(why))
}

#[test]
fn blip_window_is_half_open_on_the_trim_range() {
    // fps 4, trim [1, 2): frames 4..=7 blip at tSec 1.0 .. 1.75.
    let e = env(4.0);
    let t = AudioTrackTiming {
        trim_start: 1.0,
        trim_end: Some(2.0),
        ..track()
    };
    assert_eq!(scrub(&e, &t, 3.0), silent(SilentReason::BeforeTrimStart));
    assert_eq!(blip(&e, &t, 4.0).offset_s, 1.0);
    assert_eq!(blip(&e, &t, 7.0).offset_s, 1.75);
    assert_eq!(scrub(&e, &t, 8.0), silent(SilentReason::PastTrimEnd));
    // The blip reads from tSec, not from the start of the trim window.
    let shifted = AudioTrackTiming {
        offset_frames: 4.0,
        ..t
    };
    assert_eq!(blip(&e, &shifted, 8.0).offset_s, 1.0);
}

#[test]
fn blip_duration_has_a_45_ms_floor() {
    let t = track();
    assert_eq!(blip(&env(24.0), &t, 24.0).duration_s, 0.045);
    assert_eq!(blip(&env(1000.0), &t, 24.0).duration_s, 0.045);
    assert_eq!(blip(&env(20.0), &t, 20.0).duration_s, 0.05);
    assert_eq!(blip(&env(10.0), &t, 10.0).duration_s, 0.1);
    assert_eq!(blip(&env(1.0), &t, 1.0).duration_s, 1.0);
}

#[test]
fn scrub_gates_run_in_a_different_order_than_playback() {
    // Muted AND isolated: scrub reports the context, playback the mute.
    let t = AudioTrackTiming {
        muted: true,
        ..track()
    };
    let isolated = PlaybackEnv {
        context_isolated: true,
        ..env(24.0)
    };
    assert_eq!(
        scrub(&isolated, &t, 0.0),
        silent(SilentReason::ContextIsolated)
    );
    assert_eq!(
        plan_start(&isolated, &t, 0.0, &Untouchable),
        Ok(StartOutcome::Silent(SilentReason::Muted))
    );
    // Muted without a context: scrub reports the missing context first.
    let none = PlaybackEnv {
        audio_context: false,
        ..env(24.0)
    };
    assert_eq!(scrub(&none, &t, 0.0), silent(SilentReason::NoAudioContext));
    assert_eq!(scrub(&env(24.0), &t, 0.0), silent(SilentReason::Muted));
    let bufferless = AudioTrackTiming {
        muted: false,
        buffer_duration: None,
        ..t
    };
    assert_eq!(
        scrub(&env(24.0), &bufferless, 0.0),
        silent(SilentReason::NoBuffer)
    );
}

#[test]
fn scrub_gain_is_the_unclamped_value_at_the_scrub_frame() {
    // currentVolume(track, frame): no max(0, .) here, and no curve.
    let animated = AudioTrackTiming {
        volume: TrackVolume::Animated,
        ..track()
    };
    let seen = std::cell::RefCell::new(Vec::new());
    let sampler = FromFn(|f: f64| {
        seen.borrow_mut().push(f);
        Some(-0.25)
    });
    match plan_scrub(&env(24.0), &animated, 12.0, &sampler) {
        Ok(ScrubOutcome::Blip(b)) => assert_eq!(b.gain, -0.25),
        other => panic!("expected a blip, got {other:?}"),
    }
    assert_eq!(*seen.borrow(), vec![12.0]);
    let flat = AudioTrackTiming {
        volume: TrackVolume::Flat(-0.5),
        ..track()
    };
    assert_eq!(blip(&env(24.0), &flat, 12.0).gain, -0.5);
}

#[test]
fn scrub_animated_gain_needs_a_finite_sample() {
    let t = AudioTrackTiming {
        volume: TrackVolume::Animated,
        ..track()
    };
    let e = env(24.0);
    assert_eq!(
        plan_scrub(&e, &t, 0.0, &NoVolumeSampler),
        Err(AudioTimingRefusal::VolumeSamplerUnavailable)
    );
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            plan_scrub(&e, &t, 0.0, &FromFn(|_: f64| Some(bad))),
            Err(AudioTimingRefusal::NonFiniteVolumeSample)
        );
    }
}

#[test]
fn scrub_keeps_no_state_between_calls() {
    let e = env(24.0);
    let t = track();
    assert_eq!(scrub(&e, &t, 24.0), scrub(&e, &t, 24.0));
}

#[test]
fn scrub_passes_a_negative_zero_time_through_like_the_baseline() {
    // src.start(0, tSec, ...) receives tSec = (-0 - 0) / fps = -0 unchanged.
    let b = blip(&env(24.0), &track(), -0.0);
    assert_eq!(b.offset_s.to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn silence_is_reported_before_any_refusal() {
    let bad = PlaybackEnv {
        fps: f64::NAN,
        ..env(24.0)
    };
    let muted = AudioTrackTiming {
        muted: true,
        ..track()
    };
    assert_eq!(scrub(&bad, &muted, f64::NAN), silent(SilentReason::Muted));
    assert_eq!(
        plan_start(&bad, &muted, f64::NAN, &Untouchable),
        Ok(StartOutcome::Silent(SilentReason::Muted))
    );
}

/// Both entry points must refuse the same malformed timing the same way.
fn both(e: &PlaybackEnv, t: &AudioTrackTiming, frame: f64, want: AudioTimingRefusal) {
    assert_eq!(scrub(e, t, frame), Err(want), "scrub {e:?} {t:?} {frame}");
    assert_eq!(
        plan_start(e, t, frame, &Untouchable),
        Err(want),
        "start {e:?} {t:?} {frame}"
    );
}

#[test]
fn malformed_timing_is_a_typed_refusal() {
    for fps in [0.0, -24.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        both(&env(fps), &track(), 0.0, AudioTimingRefusal::InvalidFps);
    }
    let e = env(24.0);
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        both(&e, &track(), frame, AudioTimingRefusal::NonFiniteFrame);
    }
    for inf in [f64::INFINITY, f64::NEG_INFINITY] {
        let offset = AudioTrackTiming {
            offset_frames: inf,
            ..track()
        };
        both(&e, &offset, 0.0, AudioTimingRefusal::NonFiniteOffset);
        let start = AudioTrackTiming {
            trim_start: inf,
            ..track()
        };
        both(&e, &start, 0.0, AudioTimingRefusal::NonFiniteTrimStart);
    }
    for end in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let t = AudioTrackTiming {
            trim_end: Some(end),
            ..track()
        };
        both(&e, &t, 0.0, AudioTimingRefusal::NonFiniteTrimEnd);
    }
    let bad_buffer = AudioTrackTiming {
        buffer_duration: Some(f64::NAN),
        ..track()
    };
    both(
        &e,
        &bad_buffer,
        0.0,
        AudioTimingRefusal::NonFiniteBufferDuration,
    );
    // The buffer duration is only read when trimEnd is unset.
    let explicit_end = AudioTrackTiming {
        trim_end: Some(5.0),
        ..bad_buffer
    };
    assert!(scrub(&e, &explicit_end, 0.0).is_ok());
    for v in [f64::NAN, f64::INFINITY] {
        let t = AudioTrackTiming {
            volume: TrackVolume::Flat(v),
            ..track()
        };
        both(&e, &t, 0.0, AudioTimingRefusal::NonFiniteStaticVolume);
    }
}

#[test]
fn derived_times_that_overflow_are_refused() {
    let e = env(1.0);
    // tSec = (1e308 - -1e308) / 1 overflows.
    let t = AudioTrackTiming {
        offset_frames: -1e308,
        ..track()
    };
    both(&e, &t, 1e308, AudioTimingRefusal::TimeOutOfRange);
    // A later start whose delay (1e308 - -1e308) overflows.
    let far = AudioTrackTiming {
        trim_start: 1e308,
        trim_end: Some(1.7e308),
        ..track()
    };
    assert_eq!(
        plan_start(&e, &far, -1e308, &Untouchable),
        Err(AudioTimingRefusal::TimeOutOfRange)
    );
    // The same window as a scrub is simply before its start.
    assert_eq!(
        scrub(&e, &far, -1e308),
        silent(SilentReason::BeforeTrimStart)
    );
}

#[test]
fn negative_zero_frame_plays_from_positive_zero() {
    // Math.max(-0, 0) is +0: the start argument must not carry the sign.
    let s = play(&env(24.0), &track(), -0.0);
    assert!(s.starts_now);
    assert_eq!(s.offset_s.to_bits(), 0);
}

#[test]
fn audio_time_and_source_start_arguments() {
    let t = AudioTrackTiming {
        offset_frames: 24.0,
        trim_start: 1.5,
        ..track()
    };
    let later = play(&env(24.0), &t, 12.0);
    assert_eq!((later.audio_time(8.0), later.src_when(8.0)), (10.0, 10.0));
    let now = play(&env(24.0), &t, 72.0);
    // `src.start(0, ...)` for now, but the curve is anchored at currentTime.
    assert_eq!((now.audio_time(8.0), now.src_when(8.0)), (8.0, 0.0));
}
