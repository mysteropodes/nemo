//! Native video source-frame mapping kernel (G28), pure and I/O free.
//!
//! Mirrors `src/js/native-video-bridge.js` at fbe4b8a3: `_targetExactFor`,
//! `_targetFor`, `_interpMode`, `_blendWeight`, and the frame-selection region
//! of `_layerFrameSyncAsync` (single frame, or the blend pair). Inputs are
//! explicit: nothing reads a layer, a clock, a decoder session or a store, and
//! the time-remap value arrives through [`TimeRemapPort`] (the JS
//! `SMMotion.timeRemapValue`), never recomputed here.
//!
//! Semantics kept on purpose:
//! - the exact source frame is `clamp(frame - (offsetFrames || 0), 0, last)`
//!   with `last = max(0, (frameCount || 1) - 1)`; `||` reads `0`, `-0` and
//!   `NaN` (JS `undefined`/`null` are carried as `NaN`) as absent;
//! - a present `timeRemap` on a layer that has a `nativeVideo` record asks the
//!   port; a value overrides the offset entirely and is clamped to `[0, last]`,
//!   `None` falls through to the offset path; the remap is not consulted for a
//!   bare record or while `timeRemap` is absent;
//! - the rounded frame is JS `Math.round` (ties toward +infinity), applied to
//!   the exact frame;
//! - the interpolation mode needs `nativeVideo` and `timeRemap`; an explicit
//!   `"none"|"blend"|"motion"` wins, any other value falls back to the legacy
//!   `frameBlend` flag (blend when set);
//! - the blend weight is `round((exact - floor(exact)) * 32) / 32` and can
//!   reach `1`; a pair is used only when `weight > 0` and the two frames differ,
//!   otherwise the single rounded frame is shown;
//! - the pair's second frame is `min(max(0, frameCount - 1), floor + 1)`; with
//!   a positive weight the exact frame is already below `last`, so that equals
//!   the clamp above and a NaN `frameCount` cannot reach it.
//!
//! Deliberate typed refusals (the baseline would yield `NaN`, or silently clamp
//! an infinity): a non-finite timeline frame and a non-finite time-remap value.
//! The result is never a silent `0`.
//!
//! Not provided: frame decoding, caching, motion-compensated interpolation and
//! pixel blending; time-remap sampling (the injected port owns it).

/// Why a source frame cannot be derived. Never recovered into a frame number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoTimingRefusal {
    /// The timeline frame was `NaN` or infinite.
    NonFiniteFrame,
    /// The time-remap port answered `NaN` or an infinity.
    NonFiniteTimeRemap,
}

/// Interpolation between source frames, as `_interpMode` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InterpMode {
    None,
    Blend,
    Motion,
}

/// The fields of one video layer that decide its source frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VideoLayerTiming<'a> {
    /// `nativeVideo.frameCount`; `NaN` stands for absent.
    pub frame_count: f64,
    /// `nativeVideo.offsetFrames`; `NaN` stands for absent.
    pub offset_frames: f64,
    /// The layer has a `nativeVideo` record (false: a bare record was passed).
    pub native_video: bool,
    /// The layer's `timeRemap` is present (truthy).
    pub time_remap: bool,
    /// `nativeVideo.interpMode` as authored, `None` when absent.
    pub interp_mode: Option<&'a str>,
    /// Legacy `nativeVideo.frameBlend`.
    pub frame_blend: bool,
}

/// The injected time-remap sampler: `Some(source frame)` or `None` when the
/// remap yields nothing (JS `null`/`undefined`, or the sampler is unavailable).
pub(crate) trait TimeRemapPort {
    fn remap_value(&self, frame: f64) -> Option<f64>;
}

/// A port that never answers; the offset path always applies. Correct only for
/// a layer whose `time_remap` is false: a joining caller whose sampler is
/// unavailable must report that, not hand a remapped layer this port.
pub(crate) struct NoTimeRemap;

impl TimeRemapPort for NoTimeRemap {
    fn remap_value(&self, _frame: f64) -> Option<f64> {
        None
    }
}

/// Unrounded and rounded source frame (`_targetExactFor`, `_targetFor`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SourceFrame {
    pub exact: f64,
    pub rounded: f64,
}

/// Two source frames mixed by `weight` (the share of `to`), in `1/32` steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BlendPlan {
    pub from: f64,
    pub to: f64,
    pub weight: f64,
}

/// What `_layerFrameSyncAsync` shows for one timeline frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FrameSelection {
    Single { frame: f64 },
    Blend(BlendPlan),
}

/// JS `Math.round`: nearest integer, ties toward +infinity, `-0` for `[-0.5, 0)`.
pub(crate) fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let floor = x.floor();
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x < 0.0 {
        -0.0
    } else {
        rounded
    }
}

/// `x || fallback` for a number: `0`, `-0` and `NaN` are falsy.
fn or_fallback(x: f64, fallback: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        fallback
    } else {
        x
    }
}

/// `Math.max(0, x)` for a value that is never `NaN` here.
fn at_least_zero(x: f64) -> f64 {
    if x > 0.0 {
        x
    } else {
        0.0
    }
}

/// The last source frame index, `max(0, (frameCount || 1) - 1)`.
pub(crate) fn last_frame(frame_count: f64) -> f64 {
    at_least_zero(or_fallback(frame_count, 1.0) - 1.0)
}

/// `Math.max(0, Math.min(last, value))` for non-`NaN` operands.
fn clamp_source(value: f64, last: f64) -> f64 {
    at_least_zero(if value < last { value } else { last })
}

/// `_targetExactFor`: the unrounded source frame for a timeline frame.
pub(crate) fn source_frame_exact(
    layer: &VideoLayerTiming<'_>,
    frame: f64,
    port: &dyn TimeRemapPort,
) -> Result<f64, VideoTimingRefusal> {
    if !frame.is_finite() {
        return Err(VideoTimingRefusal::NonFiniteFrame);
    }
    let last = last_frame(layer.frame_count);
    if layer.native_video && layer.time_remap {
        if let Some(value) = port.remap_value(frame) {
            if !value.is_finite() {
                return Err(VideoTimingRefusal::NonFiniteTimeRemap);
            }
            return Ok(clamp_source(value, last));
        }
    }
    let offset = or_fallback(layer.offset_frames, 0.0);
    Ok(clamp_source(frame - offset, last))
}

/// `_targetExactFor` and `_targetFor` together.
pub(crate) fn source_frame(
    layer: &VideoLayerTiming<'_>,
    frame: f64,
    port: &dyn TimeRemapPort,
) -> Result<SourceFrame, VideoTimingRefusal> {
    let exact = source_frame_exact(layer, frame, port)?;
    Ok(SourceFrame {
        exact,
        rounded: js_round(exact),
    })
}

/// `_interpMode`: `none` unless the layer has both `nativeVideo` and `timeRemap`.
pub(crate) fn interp_mode(layer: &VideoLayerTiming<'_>) -> InterpMode {
    if !layer.native_video || !layer.time_remap {
        return InterpMode::None;
    }
    match layer.interp_mode {
        Some("motion") => InterpMode::Motion,
        Some("blend") => InterpMode::Blend,
        Some("none") => InterpMode::None,
        _ if layer.frame_blend => InterpMode::Blend,
        _ => InterpMode::None,
    }
}

/// `_blendWeight`: the fractional part of `exact`, quantized to `1/32`.
pub(crate) fn blend_weight(exact: f64) -> Result<f64, VideoTimingRefusal> {
    if !exact.is_finite() {
        return Err(VideoTimingRefusal::NonFiniteFrame);
    }
    let fraction = exact - exact.floor();
    Ok(js_round(fraction * 32.0) / 32.0)
}

/// The frame-selection region of `_layerFrameSyncAsync`: a blend pair when the
/// mode is not `none`, the weight is positive and the pair differs; otherwise
/// the single rounded frame.
pub(crate) fn select_frame(
    layer: &VideoLayerTiming<'_>,
    frame: f64,
    port: &dyn TimeRemapPort,
) -> Result<FrameSelection, VideoTimingRefusal> {
    let single = source_frame(layer, frame, port)?;
    if interp_mode(layer) == InterpMode::None {
        return Ok(FrameSelection::Single {
            frame: single.rounded,
        });
    }
    let weight = blend_weight(single.exact)?;
    let from = single.exact.floor();
    let upper = at_least_zero(layer.frame_count - 1.0);
    let to = if upper < from + 1.0 {
        upper
    } else {
        from + 1.0
    };
    if weight > 0.0 && to != from {
        Ok(FrameSelection::Blend(BlendPlan { from, to, weight }))
    } else {
        Ok(FrameSelection::Single {
            frame: single.rounded,
        })
    }
}

#[cfg(test)]
#[path = "../tests/object_media_timing_video_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_media_timing_video_pins.rs"]
mod pins;
#[cfg(test)]
#[path = "../tests/object_media_timing_video.rs"]
mod tests;
