//! T12: the stored timing of one video placement, the native owner of the
//! baseline `ld.timeRemap` and `ld.nativeVideo.interpMode` / `frameBlend`
//! (`src/js/native-video-bridge.js` `_targetExactFor` / `_interpMode`,
//! `src/js/motion.js` `timeRemapValue`, saved by `timeline.js` exportJSON).
//!
//! Each member keeps absence distinct from a value, like `offsetFrames`:
//! - `time_remap` is the keyed Motion track in source frames (no static form,
//!   the baseline has none); admitted exactly like a component instance remap;
//! - `interp_mode` is the string as authored (`_interpMode` reads only
//!   `none|blend|motion` and falls back to `frame_blend` for anything else);
//! - `frame_blend` is the legacy boolean.
//!
//! Only a drawing placement may carry timing; that the asset is a video is the
//! catalog's check (`object_media_link_track::validate`, the edit owner).
use crate::object_document::object_component_record_kernels::object_component_records::validate_remap;
#[cfg(feature = "history")]
use crate::object_document::OrderCurvePoint;
use crate::object_document::OrderTrack;
use serde_json::Number;

/// The longest authored `interpMode` kept (the baseline values are four to six bytes).
pub(crate) const MAX_INTERP_MODE: usize = 32;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct VideoTiming {
    pub(crate) time_remap: Option<OrderTrack>,
    pub(crate) interp_mode: Option<String>,
    pub(crate) frame_blend: Option<bool>,
}

impl VideoTiming {
    pub(crate) fn is_empty(&self) -> bool {
        self.time_remap.is_none() && self.interp_mode.is_none() && self.frame_blend.is_none()
    }

    /// The admission every writer shares: a finite, strictly increasing remap
    /// track of one value per key, and a bounded, non-empty mode string.
    pub(crate) fn admitted(&self) -> bool {
        self.time_remap
            .as_ref()
            .is_none_or(|track| validate_remap(track).is_ok())
            && self.interp_mode.as_ref().is_none_or(|mode| {
                !mode.is_empty() && mode.len() <= MAX_INTERP_MODE && !mode.contains('\0')
            })
    }

    /// `enableTimeRemap` (motion.js): `{in, v:[0]}` and `{max(in + 1, out),
    /// v:[last]}`, `CURVE_LINEAR`, zero handles. Integral numbers keep an
    /// integer spelling, like the JS numbers a saved file carries.
    pub(crate) fn enabled_track(
        in_frame: &Number,
        out_frame: &Number,
        last: &Number,
    ) -> Option<OrderTrack> {
        let finite = |n: &Number| n.as_f64().filter(|v| v.is_finite());
        let (start, out, last) = (finite(in_frame)?, finite(out_frame)?, finite(last)?);
        let n = |v: f64| match v.fract() == 0.0 && v.abs() <= 9_007_199_254_740_991.0 {
            true => serde_json::Value::from(v as i64),
            false => serde_json::Value::from(v),
        };
        let key = |frame: f64, value: f64| {
            serde_json::json!({"frame":n(frame),"v":[n(value)],
                "curvePoints":[{"x":0,"y":0},{"x":1,"y":1}],"hOut":[0,0],"hIn":[0,0]})
        };
        let keys = [key(start, 0.0), key((start + 1.0).max(out), last.max(0.0))];
        serde_json::from_value(serde_json::json!({ "keys": keys })).ok()
    }

    /// Apply one bounded remap intent; `with_timing` admits the result.
    pub(crate) fn apply_remap(&mut self, intent: RemapIntent) -> Result<(), RemapRefusal> {
        let finite = |n: &Number| n.as_f64().filter(|v| v.is_finite());
        match intent {
            RemapIntent::Enable {
                in_frame,
                out_frame,
                last_source_frame,
            } => {
                if self.time_remap.is_some() {
                    return Err(RemapRefusal::Conflict);
                }
                let track = Self::enabled_track(&in_frame, &out_frame, &last_source_frame);
                self.time_remap = Some(track.ok_or(RemapRefusal::Invalid)?);
            }
            RemapIntent::Disable => self.time_remap = None,
            RemapIntent::RemoveKey { frame } => {
                let (Some(track), Some(at)) = (self.time_remap.as_mut(), finite(&frame)) else {
                    return Err(RemapRefusal::Invalid);
                };
                track.keys.retain(|key| key.frame.as_f64() != Some(at));
            }
            // The shared property key kernel: an existing key keeps its spelling
            // and handles, a new one gets the default curve and zero handles.
            #[cfg(feature = "history")]
            RemapIntent::SetKey {
                frame,
                value,
                curve,
            } => {
                let track = self.time_remap.as_mut().ok_or(RemapRefusal::Invalid)?;
                if finite(&frame).is_none() || finite(&value).is_none() {
                    return Err(RemapRefusal::Invalid);
                }
                track.scalar_intent_key(frame, vec![value], curve);
            }
        }
        Ok(())
    }

    /// Whether the time-remap sampler is consulted (`ld.timeRemap` present).
    pub(crate) fn has_time_remap(&self) -> bool {
        self.time_remap.is_some()
    }
}

/// The bounded `timeRemap` edits of the baseline UI (motion.js), each one
/// request under the 4 KiB command budget; no whole-track write exists.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RemapIntent {
    /// `enableTimeRemap`: the two linear keys; refused when a remap exists.
    Enable {
        in_frame: Number,
        out_frame: Number,
        last_source_frame: Number,
    },
    /// `disableTimeRemap`: `delete ld.timeRemap`.
    Disable,
    /// Remove the keys at `frame` (the property `removeKey` rule).
    RemoveKey { frame: Number },
    /// Set the key at `frame` to `value` source frames, optionally its curve;
    /// it exists with the history-owned property key kernel it calls.
    #[cfg(feature = "history")]
    SetKey {
        frame: Number,
        value: Number,
        curve: Option<Vec<OrderCurvePoint>>,
    },
}

/// Why a remap intent was refused before admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemapRefusal {
    Invalid,
    Conflict,
}

// The sampler needs the shared curve kernel, an evaluation module.
#[cfg(feature = "evaluation")]
#[path = "object_media_video_remap.rs"]
mod remap;

#[cfg(all(test, feature = "evaluation"))]
#[path = "../tests/object_media_video_timing.rs"]
mod tests;
