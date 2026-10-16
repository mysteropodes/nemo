//! Native audio-track timing kernel (G30), pure and I/O free.
//!
//! Mirrors `src/js/audio-bridge.js` at fbe4b8a3: `trimOf`, the scheduling half
//! of `startTrack`, the scrub-blip half of `scrubAt` and the static half of
//! `currentVolume`. Inputs are explicit: nothing reads a track list, a clock,
//! an `AudioContext` or a motion store. The animated-volume value arrives
//! through [`VolumeSampler`] (the JS `SMMotion.valueAtFrame(track, 'volume',
//! frame)[0]`); this kernel never evaluates an expression or a curve itself.
//!
//! Semantics kept on purpose:
//! - `tSec = (fromFrame - (offsetFrames || 0)) / fps`; `trimStart || 0`;
//!   `trimEnd` defaults to the decoded buffer duration when unset (`undefined`
//!   and `null` alike, carried as `None`). `||` reads `0`, `-0` and `NaN`
//!   (JS `undefined`/`null` carried as `NaN`) as absent;
//! - playback is silent when `tSec >= trim.end` or the window
//!   `trim.end - max(tSec, trim.start)` is not positive. Otherwise it plays
//!   from `playFrom = max(tSec, trim.start)` now (`tSec >= trim.start`), or
//!   `trim.start - tSec` seconds later from `trim.start`, with the animated
//!   curve sampled from `startFrame` (the frame audio becomes audible);
//! - an animated volume becomes a `Float32` curve of
//!   `n = max(2, min(2000, ceil(playDuration * fps) + 1))` samples, one per
//!   frame, each `max(0, sample)`, spanning `(n - 1) / fps` seconds. A static
//!   volume is one gain value that is NOT clamped (`motionStatic.volume[0]`,
//!   else `track.volume`, else `1`);
//! - a scrub blip plays `[tSec, tSec + max(1/fps, 0.045))` when
//!   `trim.start <= tSec < trim.end`, at the unclamped `currentVolume` of the
//!   scrub frame. The 40 ms scrub throttle is a clock concern of the adapter.
//!
//! Deliberate typed refusals (the baseline would hand the Web Audio API a
//! `NaN`/infinity, which it rejects): a non-positive or non-finite `fps`, a
//! non-finite frame, offset, trim, buffer duration, derived time, static
//! volume or volume sample, and an animated volume with no sampler. They are
//! never recovered into a silent default. Silence itself is typed too
//! ([`SilentReason`]), in the baseline's check order.
//!
//! Not provided: buffer decoding, source/gain node creation, the throttle,
//! `currentTime` (see [`TrackStart::audio_time`]) and volume sampling.

/// Why a track makes no sound. Checked in the listed order per entry point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SilentReason {
    Muted,
    NoBuffer,
    /// A component or montage view is active (`_audioContextSuspended`).
    ContextIsolated,
    /// `ensureCtx()` has no `AudioContext` to give.
    NoAudioContext,
    /// Scrub only: `tSec < trim.start`.
    BeforeTrimStart,
    /// `tSec >= trim.end`.
    PastTrimEnd,
    /// Playback only: `trim.end - playFrom <= 0`.
    EmptyWindow,
}

/// Why the timing cannot be derived. Never recovered into a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AudioTimingRefusal {
    InvalidFps,
    NonFiniteFrame,
    NonFiniteOffset,
    NonFiniteTrimStart,
    NonFiniteTrimEnd,
    NonFiniteBufferDuration,
    /// A derived time or frame left the finite range.
    TimeOutOfRange,
    NonFiniteStaticVolume,
    /// The volume is animated and no sampler value was available.
    VolumeSamplerUnavailable,
    NonFiniteVolumeSample,
}

/// What the call site knows besides the track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlaybackEnv {
    pub fps: f64,
    /// `state.activeSymbolId || state.activeMontageViewId`.
    pub context_isolated: bool,
    /// `ensureCtx()` returns a context.
    pub audio_context: bool,
}

/// How a track's gain is decided.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TrackVolume {
    /// `SMMotion.isAnimated(track, 'volume')`: sampled through the port.
    Animated,
    /// The static `currentVolume` (see [`flat_volume`]).
    Flat(f64),
}

/// The fields of one audio track that decide its timing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AudioTrackTiming {
    pub muted: bool,
    /// `track._buffer.duration`; `None` when the buffer is not decoded.
    pub buffer_duration: Option<f64>,
    /// `track.offsetFrames`; `NaN` stands for absent.
    pub offset_frames: f64,
    /// `track.trimStart`; `NaN` stands for absent.
    pub trim_start: f64,
    /// `track.trimEnd`; `None` for `undefined`/`null`.
    pub trim_end: Option<f64>,
    pub volume: TrackVolume,
}

/// The injected volume sampler: `Some(value)`, or `None` when unavailable.
pub(crate) trait VolumeSampler {
    fn volume_at(&self, frame: f64) -> Option<f64>;
}

/// A sampler that never answers; only correct for [`TrackVolume::Flat`].
pub(crate) struct NoVolumeSampler;

impl VolumeSampler for NoVolumeSampler {
    fn volume_at(&self, _frame: f64) -> Option<f64> {
        None
    }
}

/// The animated gain envelope handed to `setValueCurveAtTime`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VolumeCurve {
    pub values: Vec<f32>,
    pub duration_s: f64,
}

/// What drives the gain node.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TrackGain {
    Value(f64),
    Curve(VolumeCurve),
}

/// A scheduled track start (`startTrack`'s `src.start` and gain arguments).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrackStart {
    /// `tSec >= trim.start`: audible immediately.
    pub starts_now: bool,
    /// Seconds from now until audible (`0` when `starts_now`).
    pub delay_s: f64,
    /// Buffer-local seconds to start reading from.
    pub offset_s: f64,
    pub duration_s: f64,
    /// Timeline frame at which audio becomes audible.
    pub start_frame: f64,
    pub gain: TrackGain,
}

impl TrackStart {
    /// `AudioContext` time of audibility; also the curve start time.
    pub(crate) fn audio_time(&self, context_time: f64) -> f64 {
        if self.starts_now {
            context_time
        } else {
            context_time + self.delay_s
        }
    }

    /// The `when` argument of `src.start`: `0` now, else `audio_time`.
    pub(crate) fn src_when(&self, context_time: f64) -> f64 {
        if self.starts_now {
            0.0
        } else {
            self.audio_time(context_time)
        }
    }
}

/// `startTrack` outcome.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StartOutcome {
    Silent(SilentReason),
    Play(TrackStart),
}

/// One scrub blip (`scrubAt`'s `src.start(0, tSec, dur)` and gain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrubBlip {
    pub offset_s: f64,
    pub duration_s: f64,
    pub gain: f64,
}

/// `scrubAt` outcome for one track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ScrubOutcome {
    Silent(SilentReason),
    Blip(ScrubBlip),
}

/// The static part of `currentVolume`: `motionStatic.volume[0]` when defined,
/// else `track.volume` when defined (even `0`), else `1`.
pub(crate) fn flat_volume(motion_static: Option<f64>, legacy: Option<f64>) -> f64 {
    motion_static.or(legacy).unwrap_or(1.0)
}

/// Curve length for a playback window (`n` of `startTrack`).
pub(crate) fn curve_len(play_duration: f64, fps: f64) -> usize {
    ((play_duration * fps).ceil() + 1.0).clamp(2.0, 2000.0) as usize
}

/// JS `x || 0` for a number (`NaN`, `0` and `-0` read as absent).
fn or_zero(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        0.0
    } else {
        x
    }
}

/// JS `Math.max` for non-`NaN` operands (`+0` beats `-0`).
fn js_max(a: f64, b: f64) -> f64 {
    if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_negative() {
        b
    } else {
        a
    }
}

fn finite(x: f64, why: AudioTimingRefusal) -> Result<f64, AudioTimingRefusal> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(why)
    }
}

/// The validated trim window and buffer-local time of one frame.
struct Window {
    fps: f64,
    t_sec: f64,
    start: f64,
    end: f64,
}

fn window(
    env: &PlaybackEnv,
    track: &AudioTrackTiming,
    buffer_duration: f64,
    frame: f64,
) -> Result<Window, AudioTimingRefusal> {
    if !(env.fps.is_finite() && env.fps > 0.0) {
        return Err(AudioTimingRefusal::InvalidFps);
    }
    let frame = finite(frame, AudioTimingRefusal::NonFiniteFrame)?;
    let offset = finite(
        or_zero(track.offset_frames),
        AudioTimingRefusal::NonFiniteOffset,
    )?;
    let start = finite(
        or_zero(track.trim_start),
        AudioTimingRefusal::NonFiniteTrimStart,
    )?;
    let end = match track.trim_end {
        Some(e) => finite(e, AudioTimingRefusal::NonFiniteTrimEnd)?,
        None => finite(buffer_duration, AudioTimingRefusal::NonFiniteBufferDuration)?,
    };
    let t_sec = finite(
        (frame - offset) / env.fps,
        AudioTimingRefusal::TimeOutOfRange,
    )?;
    Ok(Window {
        fps: env.fps,
        t_sec,
        start,
        end,
    })
}

fn flat_gain(value: f64) -> Result<f64, AudioTimingRefusal> {
    finite(value, AudioTimingRefusal::NonFiniteStaticVolume)
}

fn sample(sampler: &dyn VolumeSampler, frame: f64) -> Result<f64, AudioTimingRefusal> {
    sampler
        .volume_at(frame)
        .ok_or(AudioTimingRefusal::VolumeSamplerUnavailable)
}

fn curve_of(
    sampler: &dyn VolumeSampler,
    start_frame: f64,
    play_duration: f64,
    fps: f64,
) -> Result<VolumeCurve, AudioTimingRefusal> {
    let n = curve_len(play_duration, fps);
    let mut values = Vec::with_capacity(n);
    for i in 0..n {
        let raw = sample(sampler, start_frame + i as f64)?;
        if raw.is_nan() {
            return Err(AudioTimingRefusal::NonFiniteVolumeSample);
        }
        let floored = if raw > 0.0 { raw } else { 0.0 };
        let value = floored as f32;
        if !value.is_finite() {
            return Err(AudioTimingRefusal::NonFiniteVolumeSample);
        }
        values.push(value);
    }
    Ok(VolumeCurve {
        values,
        duration_s: (n - 1) as f64 / fps,
    })
}

fn silent_start(why: SilentReason) -> Result<StartOutcome, AudioTimingRefusal> {
    Ok(StartOutcome::Silent(why))
}

fn silent_scrub(why: SilentReason) -> Result<ScrubOutcome, AudioTimingRefusal> {
    Ok(ScrubOutcome::Silent(why))
}

/// `startTrack(track, fromFrame)`: when and how the track starts playing.
pub(crate) fn plan_start(
    env: &PlaybackEnv,
    track: &AudioTrackTiming,
    from_frame: f64,
    sampler: &dyn VolumeSampler,
) -> Result<StartOutcome, AudioTimingRefusal> {
    if track.muted {
        return silent_start(SilentReason::Muted);
    }
    let Some(buffer_duration) = track.buffer_duration else {
        return silent_start(SilentReason::NoBuffer);
    };
    if env.context_isolated {
        return silent_start(SilentReason::ContextIsolated);
    }
    if !env.audio_context {
        return silent_start(SilentReason::NoAudioContext);
    }
    let w = window(env, track, buffer_duration, from_frame)?;
    if w.t_sec >= w.end {
        return silent_start(SilentReason::PastTrimEnd);
    }
    let play_from = js_max(w.t_sec, w.start);
    let play_duration = finite(w.end - play_from, AudioTimingRefusal::TimeOutOfRange)?;
    if play_duration <= 0.0 {
        return silent_start(SilentReason::EmptyWindow);
    }
    let starts_now = w.t_sec >= w.start;
    let (delay_s, start_frame) = if starts_now {
        (0.0, from_frame)
    } else {
        let delay = finite(w.start - w.t_sec, AudioTimingRefusal::TimeOutOfRange)?;
        let frame = finite(
            from_frame + delay * w.fps,
            AudioTimingRefusal::TimeOutOfRange,
        )?;
        (delay, frame)
    };
    let gain = match track.volume {
        TrackVolume::Animated => {
            TrackGain::Curve(curve_of(sampler, start_frame, play_duration, w.fps)?)
        }
        TrackVolume::Flat(v) => TrackGain::Value(flat_gain(v)?),
    };
    Ok(StartOutcome::Play(TrackStart {
        starts_now,
        delay_s,
        offset_s: if starts_now { play_from } else { w.start },
        duration_s: play_duration,
        start_frame,
        gain,
    }))
}

/// `scrubAt(frame)` for one track: the one-frame blip, if any.
pub(crate) fn plan_scrub(
    env: &PlaybackEnv,
    track: &AudioTrackTiming,
    frame: f64,
    sampler: &dyn VolumeSampler,
) -> Result<ScrubOutcome, AudioTimingRefusal> {
    if env.context_isolated {
        return silent_scrub(SilentReason::ContextIsolated);
    }
    if !env.audio_context {
        return silent_scrub(SilentReason::NoAudioContext);
    }
    if track.muted {
        return silent_scrub(SilentReason::Muted);
    }
    let Some(buffer_duration) = track.buffer_duration else {
        return silent_scrub(SilentReason::NoBuffer);
    };
    let w = window(env, track, buffer_duration, frame)?;
    if w.t_sec < w.start {
        return silent_scrub(SilentReason::BeforeTrimStart);
    }
    if w.t_sec >= w.end {
        return silent_scrub(SilentReason::PastTrimEnd);
    }
    let duration_s = finite(
        js_max(1.0 / w.fps, 0.045),
        AudioTimingRefusal::TimeOutOfRange,
    )?;
    let gain = match track.volume {
        TrackVolume::Animated => finite(
            sample(sampler, frame)?,
            AudioTimingRefusal::NonFiniteVolumeSample,
        )?,
        TrackVolume::Flat(v) => flat_gain(v)?,
    };
    Ok(ScrubOutcome::Blip(ScrubBlip {
        offset_s: w.t_sec,
        duration_s,
        gain,
    }))
}

#[cfg(test)]
#[path = "../tests/object_media_timing_audio_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_media_timing_audio_pins.rs"]
mod pins;
#[cfg(test)]
#[path = "../tests/object_media_timing_audio_scrub.rs"]
mod scrub;
#[cfg(test)]
#[path = "../tests/object_media_timing_audio.rs"]
mod tests;
