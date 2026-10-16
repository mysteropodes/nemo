//! Controls for the component playback kernel: each named deviation from the
//! baseline must be caught by the oracle and by a pinned literal.
//!
//! The kernel has no hook for injecting a mistake, so every deviation is a
//! small model of the playback algorithm with exactly one change. The
//! undeviated model must reproduce every finite oracle row (proving the model
//! is the baseline and nothing else); a deviated model must then miss at least
//! one oracle row and the named pinned case of `object_component_playback.rs`.
//! A real mistake in the kernel is therefore caught by the same rows.

use super::tests::{key, keys, num, resolved, row, rows, same, Row, PINNED};
use super::{OuterKey, PlayMode};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mutant {
    None,
    /// `(frame - speed) * placedAt` instead of `(frame - placedAt) * speed`.
    SwapSpeedPlacement,
    /// No `max(0, ...)` before the speed multiplication.
    NoNegativeElapsedClamp,
    /// `speed ?? 1`: a zero speed stays zero.
    SpeedZeroKept,
    /// Ping-pong cycle `total - 1` instead of `(total - 1) * 2`.
    PingPongCycleSingle,
    /// Ping-pong cycle `total * 2`.
    PingPongCycleDoubleTotal,
    /// No `total < 2 -> 0` guard.
    PingPongUnguarded,
    /// `trimIn + floor(elapsed)` without the `min(trimOut, ...)`.
    OnceNoTrimOutClamp,
    /// `trimOut || total - 1`: an explicit zero reads as unset.
    OnceTrimOutZeroUnset,
    /// Single frame not clamped to the component.
    SingleNoClamp,
    /// `sym.totalFrames` used without `max(1, ...)`.
    TotalNotClamped,
    /// The held-key scan continues past a keyframe without `componentFrame`.
    NoHeldBreak,
    /// Time Remap read before the held componentFrame.
    RemapFirst,
    /// `floor(x + 0.5)` for `Math.round`.
    RemapFloorHalf,
    /// Truncation for `Math.round`.
    RemapTruncate,
}

fn or(v: Option<f64>, d: f64) -> f64 {
    match v {
        Some(x) if x != 0.0 && !x.is_nan() => x,
        _ => d,
    }
}

fn held(m: Mutant, ks: &[OuterKey], frame: i64) -> Option<f64> {
    let mut i = frame.min(ks.len() as i64 - 1);
    while i >= 0 {
        let k = &ks[i as usize];
        if k.is_keyframe && (k.component_frame.is_some() || m != Mutant::NoHeldBreak) {
            return k.component_frame;
        }
        i -= 1;
    }
    None
}

fn remap(m: Mutant, rv: f64, total: f64) -> f64 {
    let f = rv.floor();
    let rounded = match m {
        Mutant::RemapFloorHalf => (rv + 0.5).floor(),
        Mutant::RemapTruncate => rv.trunc(),
        _ => {
            if rv - f >= 0.5 {
                f + 1.0
            } else {
                f
            }
        }
    };
    rounded.min(total - 1.0).max(0.0)
}

fn model(m: Mutant, r: &Row) -> f64 {
    let i = &r.input;
    let total = if m == Mutant::TotalNotClamped {
        r.total
    } else {
        r.total.max(1.0)
    };
    if m == Mutant::RemapFirst {
        if let Some(rv) = r.remap {
            return remap(m, rv, total);
        }
    }
    if let Some(cf) = held(m, &i.outer_keys, r.frame) {
        return cf.floor().max(0.0).min(total - 1.0);
    }
    let placed = or(i.placed_at, 0.0);
    let speed = if m == Mutant::SpeedZeroKept {
        i.speed.unwrap_or(1.0)
    } else {
        or(i.speed, 1.0)
    };
    let (lead_from, scale) = if m == Mutant::SwapSpeedPlacement {
        (speed, placed)
    } else {
        (placed, speed)
    };
    let lead = r.frame as f64 - lead_from;
    let lead = if m == Mutant::NoNegativeElapsedClamp {
        lead
    } else {
        lead.max(0.0)
    };
    let elapsed = lead * scale;
    if let Some(rv) = r.remap {
        return remap(m, rv, total);
    }
    match i.play_mode {
        PlayMode::Single => {
            let v = or(i.single_frame, 0.0).floor().max(0.0);
            if m == Mutant::SingleNoClamp {
                v
            } else {
                v.min(total - 1.0)
            }
        }
        PlayMode::Once => {
            let trim_out = match (m, i.trim_out) {
                (Mutant::OnceTrimOutZeroUnset, Some(t)) if t != 0.0 => t,
                (Mutant::OnceTrimOutZeroUnset, _) => total - 1.0,
                (_, Some(t)) => t,
                (_, None) => total - 1.0,
            };
            let sum = or(i.trim_in, 0.0) + elapsed.floor();
            if m == Mutant::OnceNoTrimOutClamp {
                sum
            } else {
                sum.min(trim_out)
            }
        }
        PlayMode::PingPong => {
            if total < 2.0 && m != Mutant::PingPongUnguarded {
                return 0.0;
            }
            let cycle = match m {
                Mutant::PingPongCycleSingle => total - 1.0,
                Mutant::PingPongCycleDoubleTotal => total * 2.0,
                _ => (total - 1.0) * 2.0,
            };
            let pos = elapsed.floor() % cycle;
            if pos < total {
                pos
            } else {
                cycle - pos
            }
        }
        PlayMode::Loop => elapsed.floor() % total,
    }
}

/// Finite oracle rows; the `NaN` and infinite-overflow rows have no finite
/// model comparison (`f64::min` ignores a `NaN` that `Math.min` propagates).
fn finite_rows() -> Vec<(Row, f64)> {
    rows("R")
        .into_iter()
        .filter(|f| !f.join(" ").contains("NaN"))
        .map(|f| (row(&f[1..11]), num(f[11]).expect("expected")))
        .filter(|(_, e)| !e.is_nan())
        .collect()
}

fn missed(m: Mutant) -> usize {
    finite_rows()
        .iter()
        .filter(|(r, e)| !same(model(m, r), *e))
        .count()
}

fn pinned(name: &str) -> (Row, f64) {
    let (_, fields, expected) = PINNED.iter().find(|p| p.0 == name).expect("pinned case");
    (row(fields), *expected)
}

/// The deviation must be wrong on its pinned case and on the oracle, while
/// the undeviated model is right on both.
fn kills(m: Mutant, name: &str) {
    let (r, expected) = pinned(name);
    assert!(
        same(model(Mutant::None, &r), expected),
        "{name}: undeviated model"
    );
    assert!(!same(model(m, &r), expected), "{m:?} must get {name} wrong");
    assert!(missed(m) > 0, "{m:?} must also miss oracle rows");
}

#[test]
fn the_undeviated_model_reproduces_every_finite_oracle_row() {
    assert!(finite_rows().len() > 1900, "{}", finite_rows().len());
    assert_eq!(missed(Mutant::None), 0);
}

#[test]
fn swapping_speed_and_placement_is_detected() {
    kills(Mutant::SwapSpeedPlacement, "operand_order");
    // The real kernel is just as sensitive: feeding it the operands swapped
    // moves the pinned result off the baseline literal.
    let (mut r, expected) = pinned("operand_order");
    let (speed, placed) = (r.input.speed, r.input.placed_at);
    r.input.speed = placed;
    r.input.placed_at = speed;
    assert!(!same(resolved(&r), expected));
}

#[test]
fn dropping_the_negative_elapsed_clamp_is_detected() {
    kills(Mutant::NoNegativeElapsedClamp, "negative_elapsed");
}

#[test]
fn keeping_a_zero_speed_is_detected() {
    kills(Mutant::SpeedZeroKept, "speed_zero");
}

#[test]
fn a_wrong_pingpong_cycle_is_detected() {
    kills(Mutant::PingPongCycleSingle, "pingpong_cycle");
    kills(Mutant::PingPongCycleDoubleTotal, "pingpong_cycle");
    kills(Mutant::PingPongUnguarded, "pingpong_short");
}

#[test]
fn a_wrong_once_clamp_is_detected() {
    kills(Mutant::OnceNoTrimOutClamp, "once_clamp");
    kills(Mutant::OnceTrimOutZeroUnset, "once_trim_out_zero");
}

#[test]
fn unclamped_single_and_total_are_detected() {
    kills(Mutant::SingleNoClamp, "single_clamp");
    kills(Mutant::TotalNotClamped, "total_clamp");
}

#[test]
fn removing_the_held_key_early_break_is_detected() {
    kills(Mutant::NoHeldBreak, "held_break");
}

#[test]
fn time_remap_order_and_rounding_are_detected() {
    kills(Mutant::RemapFirst, "remap_vs_component");
    kills(Mutant::RemapFloorHalf, "remap_round_half_down");
    kills(Mutant::RemapTruncate, "remap_tie_up");
}

#[derive(Clone, Copy, PartialEq)]
enum BlankMutant {
    None,
    /// Only the entry AT the playhead counts (the pre-fix one-frame blank).
    ExactFrameOnly,
    /// The scan passes a non-blank keyframe and finds an older blank one.
    ScanPastKeyframe,
    /// `blankOverride` present counts as blank, `false` included.
    FalseCountsAsBlank,
}

fn blank_model(m: BlankMutant, ks: &[OuterKey], frame: i64) -> bool {
    let top = frame.min(ks.len() as i64 - 1);
    match m {
        BlankMutant::ExactFrameOnly => {
            frame >= 0
                && (frame as usize) < ks.len()
                && ks[frame as usize].is_keyframe
                && ks[frame as usize].blank_override == Some(true)
        }
        BlankMutant::ScanPastKeyframe => (0..=top)
            .any(|i| ks[i as usize].is_keyframe && ks[i as usize].blank_override == Some(true)),
        _ => {
            let mut i = top;
            while i >= 0 {
                let k = &ks[i as usize];
                if k.is_keyframe {
                    return if m == BlankMutant::FalseCountsAsBlank {
                        k.blank_override.is_some()
                    } else {
                        k.blank_override == Some(true)
                    };
                }
                i -= 1;
            }
            false
        }
    }
}

fn blank_missed(m: BlankMutant) -> usize {
    rows("B")
        .iter()
        .filter(|f| {
            blank_model(m, &keys(f[2]), f[1].parse::<i64>().expect("frame")) != (f[3] == "1")
        })
        .count()
}

#[test]
fn blank_hold_deviations_are_detected() {
    assert_eq!(blank_missed(BlankMutant::None), 0);
    assert!(blank_missed(BlankMutant::ExactFrameOnly) > 0);
    assert!(blank_missed(BlankMutant::ScanPastKeyframe) > 0);
    assert!(blank_missed(BlankMutant::FalseCountsAsBlank) > 0);
    // Pinned: a blank key holds across later empty frames, and `false` is not blank.
    let hold = keys("1:-:1,~,~");
    assert!(!blank_model(BlankMutant::ExactFrameOnly, &hold, 2));
    assert!(blank_model(
        BlankMutant::FalseCountsAsBlank,
        &keys("1:-:0"),
        0
    ));
    assert!(blank_model(
        BlankMutant::ScanPastKeyframe,
        &keys("1:-:1,1:-:-"),
        1
    ));
}

#[derive(Clone, Copy, PartialEq)]
enum AssignMutant {
    None,
    /// The componentFrame write leaves `blankOverride` in place.
    KeepsBlank,
    /// The write also lands on a non-keyframe entry.
    WritesNonKeyframe,
}

fn assign_missed(m: AssignMutant) -> usize {
    rows("A")
        .iter()
        .filter(|f| {
            let mut k = key(&format!("{}:{}:{}", f[1], f[2], f[3]));
            if k.is_keyframe || m == AssignMutant::WritesNonKeyframe {
                k.component_frame = num(f[4]);
                if m != AssignMutant::KeepsBlank {
                    k.blank_override = None;
                }
            }
            k != key(&format!("{}:{}:{}", f[5], f[6], f[7]))
        })
        .count()
}

#[test]
fn component_frame_write_deviations_are_detected() {
    assert_eq!(assign_missed(AssignMutant::None), 0);
    assert!(assign_missed(AssignMutant::KeepsBlank) > 0);
    assert!(assign_missed(AssignMutant::WritesNonKeyframe) > 0);
}

#[test]
fn oracle_covers_the_families_and_edges() {
    let all = rows("R");
    for mode in [
        "-", "loop", "once", "pingpong", "single", "bogus", "Once", "PINGPONG", "\"\"",
    ] {
        assert!(all.iter().any(|f| f[1] == mode), "mode {mode}");
    }
    let outputs: Vec<f64> = all.iter().map(|f| num(f[11]).expect("out")).collect();
    assert!(
        all.iter().filter(|f| f[9] != "-").count() > 50,
        "remap rows"
    );
    assert!(
        all.iter().any(|f| f[9] == "null"),
        "remap flag without value"
    );
    assert!(
        all.iter().filter(|f| f[10] != "-").count() > 300,
        "outer-key rows"
    );
    assert!(
        all.iter().any(|f| f[7].parse::<i64>().expect("total") < 1),
        "empty components"
    );
    assert!(
        all.iter().any(|f| f[8].parse::<i64>().expect("frame") < 0),
        "negative playhead"
    );
    assert!(outputs.iter().any(|v| *v < 0.0), "negative raw frame");
    assert!(
        outputs.iter().any(|v| v.is_finite() && v.fract() != 0.0),
        "fractional raw frame"
    );
    assert!(outputs.iter().any(|v| v.is_nan()), "NaN raw frame");
    assert!(
        outputs.iter().any(|v| v.is_infinite()),
        "infinite raw frame"
    );
    let blank = rows("B");
    assert!(blank.iter().any(|f| f[3] == "1") && blank.iter().any(|f| f[3] == "0"));
    let outcomes = rows("O");
    assert!(outcomes.iter().any(|f| f[11] == "b") && outcomes.iter().any(|f| f[11] != "b"));
    let writes = rows("A");
    assert!(writes.iter().any(|f| f[1] == "1") && writes.iter().any(|f| f[1] == "0"));
}
