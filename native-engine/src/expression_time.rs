//! Pure expression time and loop-timing kernel (N22F).
//!
//! Mirrors `src/js/domain/animation/expression-time.js` (`stepTime`,
//! `toFrames`, `toSeconds`) and the frame arithmetic of `loopRaw` in
//! `src/js/motion.js` (`loopAfter`/`loopBefore`). The caller owns the
//! evaluation context, the keyframe track and value sampling; this module
//! only decides which frames to sample and how to combine them.

/// `stepTime(frame, everyNFrames)`: snap down to a multiple of `n`. `n` is
/// the caller's `Number(everyNFrames)`; a non-finite or non-positive step
/// leaves the frame untouched.
pub fn step_time(frame: f64, every_n_frames: f64) -> f64 {
    let n = every_n_frames;
    if !n.is_finite() || n <= 0.0 {
        return frame;
    }
    (frame / n).floor() * n
}

/// `toFrames(seconds, fps)`.
pub fn to_frames(seconds: f64, fps: f64) -> f64 {
    seconds * fps
}

/// `toSeconds(frames, fps)`: a zero rate (either sign) reads as `+0`.
pub fn to_seconds(frames: f64, fps: f64) -> f64 {
    if fps == 0.0 {
        0.0
    } else {
        frames / fps
    }
}

/// How a property repeats outside its keyed range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    Cycle,
    PingPong,
    Offset,
    Continue,
}

impl LoopMode {
    /// `String(mode == null ? 'cycle' : mode).toLowerCase()`; any other
    /// string replays like `cycle`. `None` is JavaScript `null`/`undefined`.
    pub fn from_js(mode: Option<&str>) -> LoopMode {
        match mode.map(str::to_lowercase).as_deref() {
            Some("pingpong") => LoopMode::PingPong,
            Some("offset") => LoopMode::Offset,
            Some("continue") => LoopMode::Continue,
            _ => LoopMode::Cycle,
        }
    }
}

/// Which side of the keyed range repeats: `loopAfter` or `loopBefore`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopSide {
    After,
    Before,
}

/// The frames `loopRaw` samples, and how it combines them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoopPlan {
    /// Inside the keyed range, fewer than two keys, or an empty segment:
    /// the raw value at this frame.
    Raw(f64),
    /// `edge + (edge - inner) * dist`, per component.
    Continue { edge: f64, inner: f64, dist: f64 },
    /// The value at `sample`; for `offset`, plus `(to - from) * cycles`.
    Repeat {
        sample: f64,
        offset: Option<(f64, f64, f64)>,
    },
}

/// Frame arithmetic of `loopRaw`. `key_frames` are the track's keyframe
/// frames in stored order; `keyframes` is the raw count argument before
/// `Math.max(0, Math.floor(_num(keyframes)))`.
pub fn loop_plan(
    key_frames: &[f64],
    frame: f64,
    mode: LoopMode,
    keyframes: f64,
    side: LoopSide,
) -> LoopPlan {
    if key_frames.len() < 2 {
        return LoopPlan::Raw(frame);
    }
    let forward = side == LoopSide::After;
    let first = key_frames[0];
    let last = key_frames[key_frames.len() - 1];
    if if forward {
        frame <= last
    } else {
        frame >= first
    } {
        return LoopPlan::Raw(frame);
    }
    let n = js_max0(num(keyframes).floor());
    let len = key_frames.len() as f64;
    let (seg_start, seg_end) = if forward {
        let start = if n > 0.0 && n < len {
            key_frames[key_frames.len() - 1 - n as usize]
        } else {
            first
        };
        (start, last)
    } else {
        let end = if n > 0.0 && n < len {
            key_frames[n as usize]
        } else {
            last
        };
        (first, end)
    };
    if mode == LoopMode::Continue {
        return if forward {
            LoopPlan::Continue {
                edge: last,
                inner: last - 1.0,
                dist: frame - last,
            }
        } else {
            LoopPlan::Continue {
                edge: first,
                inner: first + 1.0,
                dist: first - frame,
            }
        };
    }
    let span = seg_end - seg_start;
    if span <= 0.0 {
        return LoopPlan::Raw(frame);
    }
    let d = if forward {
        frame - seg_start
    } else {
        seg_end - frame
    };
    let cycles = (d / span).floor();
    let mut r = d - cycles * span;
    if mode == LoopMode::PingPong && cycles % 2.0 == 1.0 {
        r = span - r;
    }
    let sample = if forward { seg_start + r } else { seg_end - r };
    let offset = (mode == LoopMode::Offset).then(|| {
        if forward {
            (seg_start, seg_end, cycles)
        } else {
            (seg_end, seg_start, cycles)
        }
    });
    LoopPlan::Repeat { sample, offset }
}

/// Runs `plan` against `sample` (the track's raw value at a frame), with the
/// per-component arithmetic of `loopRaw`.
pub fn loop_value(plan: LoopPlan, mut sample: impl FnMut(f64) -> Vec<f64>) -> Vec<f64> {
    match plan {
        LoopPlan::Raw(frame) => sample(frame),
        LoopPlan::Continue { edge, inner, dist } => {
            let ve = sample(edge);
            let vi = sample(inner);
            ve.iter()
                .enumerate()
                .map(|(c, &e)| e + (e - or0(vi.get(c).copied())) * dist)
                .collect()
        }
        LoopPlan::Repeat { sample: at, offset } => {
            let base = sample(at);
            let Some((from_f, to_f, cycles)) = offset else {
                return base;
            };
            let from = sample(from_f);
            let to = sample(to_f);
            base.iter()
                .enumerate()
                .map(|(k, &b)| b + (or0(to.get(k).copied()) - or0(from.get(k).copied())) * cycles)
                .collect()
        }
    }
}

/// JavaScript `_num`: non-finite reads as 0.
fn num(x: f64) -> f64 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

/// `Math.max(0, x)` for a finite `x`: `-0` loses to `+0`.
fn js_max0(x: f64) -> f64 {
    if x > 0.0 {
        x
    } else {
        0.0
    }
}

/// JavaScript `x || 0`: missing, NaN and either zero read as `+0`.
fn or0(x: Option<f64>) -> f64 {
    match x {
        Some(v) if !v.is_nan() && v != 0.0 => v,
        _ => 0.0,
    }
}
