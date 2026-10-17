//! Pinned-literal tests for the audio export disposition (G50, L09-F): the
//! decisions the oracle rows do not name, one hand-computed value each.

use super::{
    plan_audio_export, AudioDisposition, AudioExportError, AudioExportMode, AudioManifest,
    AudioTrackInput, TrackAudio,
};
use crate::object_document::media_timing::object_media_timing_audio::{
    plan_start, AudioTimingRefusal, AudioTrackTiming, NoVolumeSampler, PlaybackEnv, SilentReason,
    StartOutcome, TrackGain, TrackVolume, VolumeSampler,
};
use std::cell::RefCell;

fn env() -> PlaybackEnv {
    PlaybackEnv {
        fps: 24.0,
        context_isolated: false,
        audio_context: true,
    }
}

/// A 10 s track, no trim, no offset, flat gain 1: sounds at once from frame 0.
fn flat() -> AudioTrackTiming {
    AudioTrackTiming {
        muted: false,
        buffer_duration: Some(10.0),
        offset_frames: 0.0,
        trim_start: 0.0,
        trim_end: None,
        volume: TrackVolume::Flat(1.0),
    }
}

fn input<'a>(
    id: &str,
    timing: AudioTrackTiming,
    sampler: &'a dyn VolumeSampler,
) -> AudioTrackInput<'a> {
    AudioTrackInput {
        id: id.to_string(),
        timing,
        sampler,
    }
}

fn plan_one(timing: AudioTrackTiming, env: &PlaybackEnv, start: u32) -> AudioManifest {
    let tracks = [input("a", timing, &NoVolumeSampler)];
    plan_audio_export(AudioExportMode::SilentBaseline, env, start, 24, &tracks).expect("plan")
}

fn span(manifest: &AudioManifest, index: usize) -> super::AudibleSpan {
    match &manifest.tracks()[index] {
        TrackAudio::Audible(span) => span.clone(),
        other => panic!("expected an audible span, got {other:?}"),
    }
}

fn try_plan(
    start: u32,
    frames: u64,
    tracks: &[AudioTrackInput<'_>],
) -> Result<AudioManifest, AudioExportError> {
    plan_audio_export(
        AudioExportMode::SilentBaseline,
        &env(),
        start,
        frames,
        tracks,
    )
}

struct Constant(f64);

impl VolumeSampler for Constant {
    fn volume_at(&self, _frame: f64) -> Option<f64> {
        Some(self.0)
    }
}

struct Recording(RefCell<Vec<f64>>);

impl VolumeSampler for Recording {
    fn volume_at(&self, frame: f64) -> Option<f64> {
        self.0.borrow_mut().push(frame);
        Some(1.0)
    }
}

#[test]
fn mux_request_is_typed_unavailable_before_any_validation() {
    let tracks = [
        input("a", flat(), &NoVolumeSampler),
        input("a", flat(), &NoVolumeSampler),
    ];
    // An empty range and duplicate ids would also be refused, but the mux decision comes first.
    let result = plan_audio_export(AudioExportMode::Mux, &env(), 0, 0, &tracks);
    assert_eq!(result.unwrap_err(), AudioExportError::MuxUnavailable);
}

#[test]
fn every_manifest_reports_the_silent_baseline_even_with_audible_spans() {
    let sounding = plan_one(flat(), &env(), 0);
    assert_eq!(sounding.audible().len(), 1);
    assert_eq!(sounding.disposition(), AudioDisposition::SilentBaseline);
    let none = plan_audio_export(AudioExportMode::SilentBaseline, &env(), 0, 1, &[]).unwrap();
    assert!(none.tracks().is_empty() && none.audible().is_empty());
    assert_eq!(none.disposition(), AudioDisposition::SilentBaseline);
}

#[test]
fn muted_and_trimmed_out_tracks_are_excluded_with_the_kernel_reason() {
    let mut muted = flat();
    muted.muted = true;
    let mut undecoded = flat();
    undecoded.buffer_duration = None;
    let mut ended = flat();
    ended.trim_end = Some(1.0);
    let mut inverted = flat();
    inverted.trim_start = 2.0;
    inverted.trim_end = Some(1.0);
    let mut isolated = env();
    isolated.context_isolated = true;
    let mut silent_ctx = env();
    silent_ctx.audio_context = false;
    let cases = [
        (muted, env(), 0, SilentReason::Muted),
        (undecoded, env(), 0, SilentReason::NoBuffer),
        // Frame 24 at 24 fps is exactly 1 s: `tSec >= trim.end`.
        (ended, env(), 24, SilentReason::PastTrimEnd),
        (inverted, env(), 0, SilentReason::EmptyWindow),
        (flat(), isolated, 0, SilentReason::ContextIsolated),
        (flat(), silent_ctx, 0, SilentReason::NoAudioContext),
    ];
    for (timing, env, start, reason) in cases {
        let manifest = plan_one(timing, &env, start);
        assert_eq!(
            manifest.tracks(),
            [TrackAudio::Excluded {
                track_id: "a".to_string(),
                reason
            }]
        );
        assert!(manifest.audible().is_empty());
    }
}

#[test]
fn audible_span_is_exactly_the_audio_kernel_answer() {
    let StartOutcome::Play(kernel) = plan_start(&env(), &flat(), 0.0, &NoVolumeSampler).unwrap()
    else {
        panic!("the kernel plays this track");
    };
    let sounding = span(&plan_one(flat(), &env(), 0), 0);
    assert_eq!(sounding.start, kernel, "no second timing computation");
    assert!(kernel.starts_now);
    assert_eq!(kernel.gain, TrackGain::Value(1.0));
    assert_eq!(kernel.start_frame, 0.0);
    assert_eq!((sounding.start_s, sounding.end_s), (0.0, 10.0));
    assert_eq!(sounding.track_id, "a");
}

#[test]
fn delayed_span_uses_the_trim_delay_and_ends_after_the_playing_duration() {
    let mut track = flat();
    track.trim_start = 1.5;
    track.trim_end = Some(4.0);
    let delayed = span(&plan_one(track, &env(), 0), 0);
    assert!(!delayed.start.starts_now);
    assert_eq!(delayed.start.offset_s, 1.5);
    assert_eq!(delayed.start.duration_s, 2.5);
    assert_eq!(delayed.start.start_frame, 36.0);
    assert_eq!((delayed.start_s, delayed.end_s), (1.5, 4.0));
}

#[test]
fn range_start_frame_is_the_frame_the_kernel_starts_from() {
    let mut track = flat();
    track.offset_frames = 24.0;
    // The track begins one second into the timeline: delayed when the export starts at 0.
    let at0 = span(&plan_one(track, &env(), 0), 0);
    assert_eq!((at0.start_s, at0.end_s), (1.0, 11.0));
    assert_eq!((at0.start.offset_s, at0.start.start_frame), (0.0, 24.0));
    // At frame 24 it starts at once, from the buffer start.
    let at24 = span(&plan_one(track, &env(), 24), 0);
    assert_eq!(
        (at24.start_s, at24.end_s, at24.start.offset_s),
        (0.0, 10.0, 0.0)
    );
    // At frame 48 one second of it has already played: read from 1 s for the remaining 9 s.
    let at48 = span(&plan_one(track, &env(), 48), 0);
    assert!(at48.start.starts_now);
    assert_eq!((at48.start_s, at48.end_s), (0.0, 9.0));
    assert_eq!((at48.start.offset_s, at48.start.duration_s), (1.0, 9.0));
}

#[test]
fn entries_keep_input_order_and_the_stable_track_ids() {
    let mut muted = flat();
    muted.muted = true;
    let tracks = [
        input("zeta", muted, &NoVolumeSampler),
        input("alpha", flat(), &NoVolumeSampler),
        input("mid", flat(), &NoVolumeSampler),
    ];
    let manifest =
        plan_audio_export(AudioExportMode::SilentBaseline, &env(), 0, 24, &tracks).unwrap();
    let ids: Vec<&str> = manifest.tracks().iter().map(TrackAudio::track_id).collect();
    assert_eq!(ids, ["zeta", "alpha", "mid"]);
    let spans = manifest.audible();
    let sounding: Vec<&str> = spans.iter().map(|s| s.track_id.as_str()).collect();
    assert_eq!(sounding, ["alpha", "mid"]);
}

#[test]
fn animated_volume_without_a_sampler_value_is_unresolved_not_defaulted() {
    let mut animated = flat();
    animated.buffer_duration = Some(1.0);
    animated.volume = TrackVolume::Animated;
    let manifest = plan_one(animated, &env(), 0);
    assert_eq!(
        manifest.tracks(),
        [TrackAudio::Unresolved {
            track_id: "a".to_string(),
            refusal: AudioTimingRefusal::VolumeSamplerUnavailable
        }]
    );
    // With a value port the same track sounds with a one-second, 25-sample envelope.
    let constant = Constant(0.5);
    let tracks = [input("a", animated, &constant)];
    let manifest =
        plan_audio_export(AudioExportMode::SilentBaseline, &env(), 0, 24, &tracks).unwrap();
    let envelope = span(&manifest, 0);
    let TrackGain::Curve(curve) = &envelope.start.gain else {
        panic!("an animated volume is a curve");
    };
    assert_eq!(curve.values, vec![0.5_f32; 25]);
    assert_eq!((curve.duration_s, envelope.end_s), (1.0, 1.0));
}

#[test]
fn sampler_is_asked_from_the_audible_frame_by_the_kernel_alone() {
    let mut track = flat();
    track.trim_start = 1.5;
    track.trim_end = Some(2.5);
    track.volume = TrackVolume::Animated;
    let recording = Recording(RefCell::new(Vec::new()));
    let tracks = [input("a", track, &recording)];
    let manifest =
        plan_audio_export(AudioExportMode::SilentBaseline, &env(), 0, 24, &tracks).unwrap();
    // Audio becomes audible at frame 36 (1.5 s at 24 fps); one sample per frame for 1 s.
    let asked = recording.0.borrow();
    assert_eq!(asked.len(), 25);
    assert_eq!((asked[0], asked[24]), (36.0, 60.0));
    assert_eq!(span(&manifest, 0).start.start_frame, 36.0);
}

#[test]
fn span_end_overflow_is_unresolved_never_an_infinite_span() {
    let mut track = flat();
    track.offset_frames = 1e308;
    track.trim_end = Some(1.7e308);
    let one_fps = PlaybackEnv { fps: 1.0, ..env() };
    let StartOutcome::Play(kernel) = plan_start(&one_fps, &track, 0.0, &NoVolumeSampler).unwrap()
    else {
        panic!("the kernel itself accepts this track");
    };
    assert_eq!((kernel.delay_s, kernel.duration_s), (1e308, 1.7e308));
    assert_eq!(
        plan_one(track, &one_fps, 0).tracks(),
        [TrackAudio::Unresolved {
            track_id: "a".to_string(),
            refusal: AudioTimingRefusal::TimeOutOfRange
        }]
    );
}

#[test]
fn invalid_ranges_and_track_ids_are_typed_refusals() {
    let one = [input("a", flat(), &NoVolumeSampler)];
    let plan = try_plan;
    assert_eq!(plan(0, 0, &one).unwrap_err(), AudioExportError::EmptyRange);
    assert_eq!(
        plan(u32::MAX, 2, &one).unwrap_err(),
        AudioExportError::RangeOverflow {
            start: u32::MAX,
            frames: 2
        }
    );
    assert_eq!(
        plan(5, u64::MAX, &one).unwrap_err(),
        AudioExportError::RangeOverflow {
            start: 5,
            frames: u64::MAX
        }
    );
    // The last u32 frame alone, and the whole u32 space, fit.
    assert!(plan(u32::MAX, 1, &one).is_ok());
    assert!(plan(0, u64::from(u32::MAX) + 1, &one).is_ok());
    let blank = [
        input("a", flat(), &NoVolumeSampler),
        input("", flat(), &NoVolumeSampler),
    ];
    assert_eq!(
        plan(0, 1, &blank).unwrap_err(),
        AudioExportError::EmptyTrackId { index: 1 }
    );
    let twice = [
        input("a", flat(), &NoVolumeSampler),
        input("b", flat(), &NoVolumeSampler),
        input("a", flat(), &NoVolumeSampler),
    ];
    assert_eq!(
        plan(0, 1, &twice).unwrap_err(),
        AudioExportError::DuplicateTrackId {
            id: "a".to_string()
        }
    );
}

#[test]
fn kernel_refusals_stay_per_track_and_do_not_fail_the_export() {
    let mut muted = flat();
    muted.muted = true;
    let bad_fps = PlaybackEnv { fps: 0.0, ..env() };
    let tracks = [
        input("muted", muted, &NoVolumeSampler),
        input("live", flat(), &NoVolumeSampler),
    ];
    let manifest =
        plan_audio_export(AudioExportMode::SilentBaseline, &bad_fps, 0, 24, &tracks).unwrap();
    // The kernel answers silence before it validates, so a muted track stays excluded.
    assert_eq!(
        manifest.tracks(),
        [
            TrackAudio::Excluded {
                track_id: "muted".to_string(),
                reason: SilentReason::Muted
            },
            TrackAudio::Unresolved {
                track_id: "live".to_string(),
                refusal: AudioTimingRefusal::InvalidFps
            },
        ]
    );
    assert_eq!(manifest.disposition(), AudioDisposition::SilentBaseline);
}

#[test]
fn manifest_carries_the_range_and_frame_rate() {
    let manifest = plan_one(flat(), &env(), 7);
    assert_eq!(
        (
            manifest.range_start(),
            manifest.range_frames(),
            manifest.fps()
        ),
        (7, 24, 24.0)
    );
}
