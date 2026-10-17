//! Audio disposition of a native export (G50), pure and I/O free.
//!
//! The baseline exports carry no audio: `exportMP4ToPath` (`src/js/export.js`
//! at fbe4b8a3) feeds ffmpeg one image-sequence input and no audio option, the
//! browser `MediaRecorder` path records a `canvas.captureStream(0)` video
//! track only, and the SVG/PNG outputs cannot hold a stream. The human owner
//! decided (package #2106) that the native export does not mux audio either,
//! so every manifest reports [`AudioDisposition::SilentBaseline`] and a mux
//! request is the typed [`AudioExportError::MuxUnavailable`], never a silent
//! success.
//!
//! What this module adds is the *audible-span manifest* of an export range,
//! for the day a mux is authorized and for diagnostics now. It performs no
//! second timing computation: the baseline entry that starts every track from
//! a frame is `restartAt(frame)` (`SMAudio.onPlayStart`), which calls
//! `startTrack` per track, so each track's span is exactly the audio timing
//! kernel's `plan_start` at the first exported frame, with the audio context
//! origin at output time zero. Per track the manifest records one of:
//! - [`TrackAudio::Audible`]: the kernel's [`TrackStart`] verbatim plus the
//!   span `[start_s, end_s)` in seconds from the start of the output
//!   (`end_s = start_s + duration_s`; a non-finite end is
//!   `TimeOutOfRange`, never an infinite span). Spans are NOT clipped to the
//!   export length: a future mux clips them;
//! - [`TrackAudio::Excluded`]: the kernel's [`SilentReason`] (muted, no decoded
//!   buffer, trimmed out as `PastTrimEnd`/`EmptyWindow`, component or montage
//!   isolation, no audio context);
//! - [`TrackAudio::Unresolved`]: the kernel's [`AudioTimingRefusal`] (for an
//!   animated volume without a sampler value: `VolumeSamplerUnavailable`).
//!
//! [`verify_silent_argv`] is the guard the later export job runs on the ffmpeg
//! argument vector so the "no FFmpeg change" decision cannot drift: exactly
//! one `-i` input and no audio stream option.
//!
//! Not provided: audio mux, audio decoding, a sampler for animated volume
//! (Root D11/D12), the track record or any ffmpeg/process/file access.

use crate::object_document::media_timing::object_media_timing_audio::{
    plan_start, AudioTimingRefusal, AudioTrackTiming, PlaybackEnv, SilentReason, StartOutcome,
    TrackStart, VolumeSampler,
};

/// What the caller asks of the audio side of an export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AudioExportMode {
    /// The baseline: the output carries no audio.
    SilentBaseline,
    /// Mix the audible spans into the output. Not authorized.
    Mux,
}

/// What every export of the pinned snapshot does with audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AudioDisposition {
    /// The baseline: exports are silent, whatever the document holds.
    SilentBaseline,
}

/// Why a manifest cannot be planned. No variant recovers into a default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AudioExportError {
    /// Audio mux is not part of the native export (lead decision).
    MuxUnavailable,
    EmptyRange,
    /// `start + frames - 1` leaves the `u32` frame space.
    RangeOverflow {
        start: u32,
        frames: u64,
    },
    EmptyTrackId {
        index: usize,
    },
    DuplicateTrackId {
        id: String,
    },
}

/// One audio track of the pinned snapshot, as the caller resolved it.
pub(crate) struct AudioTrackInput<'a> {
    /// Stable track identity (never a list index).
    pub(crate) id: String,
    pub(crate) timing: AudioTrackTiming,
    /// The animated-volume value port; ignored for a flat volume.
    pub(crate) sampler: &'a dyn VolumeSampler,
}

/// A track that sounds in the export.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AudibleSpan {
    pub(crate) track_id: String,
    /// Seconds from the start of the output until the track is audible.
    pub(crate) start_s: f64,
    /// `start_s + start.duration_s`, finite.
    pub(crate) end_s: f64,
    /// The audio timing kernel's answer, unchanged.
    pub(crate) start: TrackStart,
}

/// The outcome of one track in an export range.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TrackAudio {
    Audible(AudibleSpan),
    Excluded {
        track_id: String,
        reason: SilentReason,
    },
    Unresolved {
        track_id: String,
        refusal: AudioTimingRefusal,
    },
}

impl TrackAudio {
    pub(crate) fn track_id(&self) -> &str {
        match self {
            TrackAudio::Audible(span) => &span.track_id,
            TrackAudio::Excluded { track_id, .. } | TrackAudio::Unresolved { track_id, .. } => {
                track_id
            }
        }
    }
}

/// The audio side of one export range: a disposition and one entry per track.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AudioManifest {
    range_start: u32,
    range_frames: u64,
    fps: f64,
    tracks: Vec<TrackAudio>,
}

impl AudioManifest {
    /// Always [`AudioDisposition::SilentBaseline`].
    pub(crate) fn disposition(&self) -> AudioDisposition {
        AudioDisposition::SilentBaseline
    }

    pub(crate) fn range_start(&self) -> u32 {
        self.range_start
    }

    pub(crate) fn range_frames(&self) -> u64 {
        self.range_frames
    }

    pub(crate) fn fps(&self) -> f64 {
        self.fps
    }

    /// One entry per input track, in input order.
    pub(crate) fn tracks(&self) -> &[TrackAudio] {
        &self.tracks
    }

    /// The tracks that would sound, in input order.
    pub(crate) fn audible(&self) -> Vec<&AudibleSpan> {
        self.tracks
            .iter()
            .filter_map(|entry| match entry {
                TrackAudio::Audible(span) => Some(span),
                _ => None,
            })
            .collect()
    }
}

fn validate(
    range_start: u32,
    range_frames: u64,
    tracks: &[AudioTrackInput<'_>],
) -> Result<(), AudioExportError> {
    if range_frames == 0 {
        return Err(AudioExportError::EmptyRange);
    }
    match u64::from(range_start).checked_add(range_frames - 1) {
        Some(end) if end <= u64::from(u32::MAX) => {}
        _ => {
            return Err(AudioExportError::RangeOverflow {
                start: range_start,
                frames: range_frames,
            })
        }
    }
    for (index, track) in tracks.iter().enumerate() {
        if track.id.is_empty() {
            return Err(AudioExportError::EmptyTrackId { index });
        }
        if tracks[..index].iter().any(|earlier| earlier.id == track.id) {
            return Err(AudioExportError::DuplicateTrackId {
                id: track.id.clone(),
            });
        }
    }
    Ok(())
}

fn classify(env: &PlaybackEnv, from_frame: f64, track: &AudioTrackInput<'_>) -> TrackAudio {
    let track_id = track.id.clone();
    match plan_start(env, &track.timing, from_frame, track.sampler) {
        Ok(StartOutcome::Silent(reason)) => TrackAudio::Excluded { track_id, reason },
        Err(refusal) => TrackAudio::Unresolved { track_id, refusal },
        Ok(StartOutcome::Play(start)) => {
            let start_s = start.audio_time(0.0);
            let end_s = start_s + start.duration_s;
            if end_s.is_finite() {
                TrackAudio::Audible(AudibleSpan {
                    track_id,
                    start_s,
                    end_s,
                    start,
                })
            } else {
                TrackAudio::Unresolved {
                    track_id,
                    refusal: AudioTimingRefusal::TimeOutOfRange,
                }
            }
        }
    }
}

/// The audio manifest of an export of `range_frames` frames from `range_start`.
///
/// `env` is the caller's playback environment (frame rate and the two context
/// flags); every track is timed as the baseline's `restartAt(range_start)`
/// would start it.
pub(crate) fn plan_audio_export(
    mode: AudioExportMode,
    env: &PlaybackEnv,
    range_start: u32,
    range_frames: u64,
    tracks: &[AudioTrackInput<'_>],
) -> Result<AudioManifest, AudioExportError> {
    if mode == AudioExportMode::Mux {
        return Err(AudioExportError::MuxUnavailable);
    }
    validate(range_start, range_frames, tracks)?;
    let from_frame = f64::from(range_start);
    Ok(AudioManifest {
        range_start,
        range_frames,
        fps: env.fps,
        tracks: tracks
            .iter()
            .map(|track| classify(env, from_frame, track))
            .collect(),
    })
}

/// ffmpeg options that select, encode, filter or map an audio stream.
pub(crate) const AUDIO_ARGV_OPTIONS: [&str; 10] = [
    "-c:a",
    "-acodec",
    "-codec:a",
    "-b:a",
    "-ab",
    "-ar",
    "-ac",
    "-af",
    "-filter:a",
    "-map",
];

/// Why an ffmpeg argument vector is not the silent baseline shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SilentArgvError {
    /// No `-i` input before the output path.
    NoInput,
    /// A second `-i` (the only way to bring in another stream).
    ExtraInput {
        position: usize,
    },
    AudioOption {
        position: usize,
        option: String,
    },
}

/// The baseline `exportMP4ToPath` argument vector has exactly one `-i` input
/// and no audio stream option; the last argument is the output path and is
/// not scanned. Anything else is not a silent export.
pub(crate) fn verify_silent_argv(args: &[String]) -> Result<(), SilentArgvError> {
    let scanned = args.len().saturating_sub(1);
    let mut inputs = 0usize;
    for (position, arg) in args[..scanned].iter().enumerate() {
        if arg == "-i" {
            inputs += 1;
            if inputs == 2 {
                return Err(SilentArgvError::ExtraInput { position });
            }
        } else if AUDIO_ARGV_OPTIONS.contains(&arg.as_str()) {
            return Err(SilentArgvError::AudioOption {
                position,
                option: arg.clone(),
            });
        }
    }
    if inputs == 0 {
        return Err(SilentArgvError::NoInput);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/native_media_export_audio_baseline.rs"]
mod baseline;
#[cfg(test)]
#[path = "../tests/native_media_export_audio_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/native_media_export_audio_pins.rs"]
mod pins;
#[cfg(test)]
#[path = "../tests/native_media_export_audio.rs"]
mod tests;
