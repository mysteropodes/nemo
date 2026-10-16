//! Component playback frame kernel (L04-A, I15/D33).
//!
//! Mirrors `resolveSymbolFrameIdx` (`src/js/app.js`) and the blank-hold loop
//! of `getEffectiveStrokes`' `symbolId` branch at baseline `fbe4b8a3`, plus the
//! `componentFrame` write of `setSymbolSingleFrame` (`src/js/timeline.js`).
//! It answers one question for the caller: which internal frame of a
//! component does an instance (or an `lfsSettings` sub-object, which carries
//! the same fields) show at an outer frame, or nothing at all.
//!
//! The kernel owns no document, snapshot or expression runtime. The Time
//! Remap value is a typed port (`Option<f64>`): the caller samples it
//! (`SMMotion.timeRemapValue` in the baseline) and passes `None` when the
//! layer is not remapped (or the sampler returned `null`). If the layer carries a
//! Time Remap and no sampler is wired, the caller must report typed unavailability
//! instead of passing `None`. The arithmetic is done on `f64` with JavaScript
//! `Math.min`/`Math.max`/`Math.round`/`%` semantics, so an unusual field
//! (`NaN`, a fractional trim, a negative speed) yields the same raw value the
//! baseline did. A raw value is a position in `sym.layers[i].frames`; use
//! [`ResolvedFrame::index`] to read it the way JavaScript indexes an array.

/// `layer.symPlayMode`. Any label other than the three below, including an
/// absent one, plays as a loop (the baseline's final `return`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayMode {
    #[default]
    Loop,
    Once,
    PingPong,
    Single,
}

impl PlayMode {
    /// The baseline's `===` comparisons: exact, case-sensitive labels.
    pub fn from_js(label: Option<&str>) -> PlayMode {
        match label {
            Some("single") => PlayMode::Single,
            Some("once") => PlayMode::Once,
            Some("pingpong") => PlayMode::PingPong,
            _ => PlayMode::Loop,
        }
    }
}

/// One entry of the instance layer's OUTER `frames[]` timing row. The
/// baseline reads only these three fields; a `null` entry is equivalent to
/// `OuterKey::default()` (not a keyframe).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OuterKey {
    pub is_keyframe: bool,
    /// `componentFrame`; `null` and absent are both `None` (`!= null` test).
    pub component_frame: Option<f64>,
    /// `blankOverride`: absent, `false` and `true` stay three distinct values.
    pub blank_override: Option<bool>,
}

impl OuterKey {
    /// `if(f&&f.isKeyframe){f.componentFrame=picked;delete f.blankOverride;}`
    /// (timeline.js, comp-preview.js and lipsync.js write the same pair).
    /// Returns whether the key was written; a non-keyframe is left untouched,
    /// blank flag included.
    pub fn assign_component_frame(&mut self, picked: f64) -> bool {
        if !self.is_keyframe {
            return false;
        }
        self.component_frame = Some(picked);
        self.blank_override = None;
        true
    }
}

/// The fields `resolveSymbolFrameIdx` reads off `layer`. An absent field is
/// `None`; for an `lfsSettings` sub-object `outer_keys` is simply empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaybackInput {
    pub play_mode: PlayMode,
    /// `symSpeed`, read as `symSpeed || 1`.
    pub speed: Option<f64>,
    /// `symPlacedAt`, read as `symPlacedAt || 0`.
    pub placed_at: Option<f64>,
    /// `symSingleFrame`, read as `symSingleFrame || 0`.
    pub single_frame: Option<f64>,
    /// `symTrimIn`, read as `symTrimIn || 0`.
    pub trim_in: Option<f64>,
    /// `symTrimOut`, read with `!= null`: an explicit `0` is a real trim.
    pub trim_out: Option<f64>,
    pub outer_keys: Vec<OuterKey>,
}

/// The raw number `resolveSymbolFrameIdx` returned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedFrame(f64);

impl ResolvedFrame {
    pub fn raw(self) -> f64 {
        self.0
    }

    /// `frames[raw]`: `Some` only for an integer in `0..len` (`-0` is `0`);
    /// negative, fractional, infinite and `NaN` values name no frame.
    pub fn index(self, len: usize) -> Option<usize> {
        let v = self.0;
        if v >= 0.0 && v.fract() == 0.0 && v < len as f64 {
            Some(v as usize)
        } else {
            None
        }
    }
}

/// What an instance contributes at an outer frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaybackOutcome {
    /// A held blank outer key suppresses the component entirely.
    Blank,
    Frame(ResolvedFrame),
}

/// The held outer key: scanning down from `min(frame, len - 1)`, the first
/// entry that is a keyframe. Non-keyframes and absent entries are skipped; the
/// scan never continues past a keyframe, whatever fields it carries.
pub fn nearest_outer_key(keys: &[OuterKey], main_frame: i64) -> Option<&OuterKey> {
    let mut i = main_frame.min(keys.len() as i64 - 1);
    while i >= 0 {
        let key = &keys[i as usize];
        if key.is_keyframe {
            return Some(key);
        }
        i -= 1;
    }
    None
}

/// `if(ownF.blankOverride)return[];` over the held key: only an explicit
/// `true` hides the component; absent and `false` do not.
pub fn is_blank_held(keys: &[OuterKey], main_frame: i64) -> bool {
    nearest_outer_key(keys, main_frame).is_some_and(|k| k.blank_override == Some(true))
}

/// `getEffectiveStrokes`' order for a component layer: the blank hold first,
/// then `resolveSymbolFrameIdx`.
pub fn evaluate_component_playback(
    input: &PlaybackInput,
    total_frames: f64,
    main_frame: i64,
    time_remap: Option<f64>,
) -> PlaybackOutcome {
    if is_blank_held(&input.outer_keys, main_frame) {
        return PlaybackOutcome::Blank;
    }
    PlaybackOutcome::Frame(resolve_component_frame(
        input,
        total_frames,
        main_frame,
        time_remap,
    ))
}

/// `resolveSymbolFrameIdx(sym, layer, mainFrameIdx)`: `total_frames` is
/// `sym.totalFrames`, `time_remap` the sampled Time Remap value (`None` when
/// the layer has none or the sampler returned `null`).
pub fn resolve_component_frame(
    input: &PlaybackInput,
    total_frames: f64,
    main_frame: i64,
    time_remap: Option<f64>,
) -> ResolvedFrame {
    let total = js_max(1.0, total_frames);
    // A held outer key may pick the component frame for its whole span.
    if let Some(cf) =
        nearest_outer_key(&input.outer_keys, main_frame).and_then(|k| k.component_frame)
    {
        return ResolvedFrame(js_min(total - 1.0, js_max(0.0, cf.floor())));
    }
    let placed = or_default(input.placed_at, 0.0);
    let speed = or_default(input.speed, 1.0);
    let elapsed = js_max(0.0, main_frame as f64 - placed) * speed;
    // Time Remap names the internal frame directly and overrides every mode.
    if let Some(rv) = time_remap {
        return ResolvedFrame(js_max(0.0, js_min(total - 1.0, js_round(rv))));
    }
    let value = match input.play_mode {
        PlayMode::Single => js_min(
            total - 1.0,
            js_max(0.0, or_default(input.single_frame, 0.0).floor()),
        ),
        PlayMode::Once => {
            let trim_in = or_default(input.trim_in, 0.0);
            let trim_out = input.trim_out.unwrap_or(total - 1.0);
            js_min(trim_out, trim_in + elapsed.floor())
        }
        PlayMode::PingPong => {
            if total < 2.0 {
                0.0
            } else {
                let cycle = (total - 1.0) * 2.0;
                let pos = elapsed.floor() % cycle;
                if pos < total {
                    pos
                } else {
                    cycle - pos
                }
            }
        }
        PlayMode::Loop => elapsed.floor() % total,
    };
    ResolvedFrame(value)
}

/// JavaScript `v || default` for a number: absent, `NaN` and either zero read
/// as the default.
fn or_default(v: Option<f64>, default: f64) -> f64 {
    match v {
        Some(x) if !x.is_nan() && x != 0.0 => x,
        _ => default,
    }
}

/// `Math.min`: a `NaN` operand yields `NaN` (`f64::min` would ignore it). Equal under
/// `==` to JS; the sign of a zero tie is not reproduced.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.max` with the same `NaN` rule.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// `Math.round`: nearest integer, ties toward `+Infinity`. Not `x.round()`
/// (ties away from zero) and not `(x + 0.5).floor()` (wrong for
/// `0.49999999999999994`). `x - floor(x)` is exact below 2^52 and zero above.
fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

#[cfg(test)]
#[path = "../tests/object_component_playback_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_component_playback.rs"]
mod tests;
