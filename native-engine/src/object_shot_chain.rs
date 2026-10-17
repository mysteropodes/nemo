//! Shot chain edit kernel (L10-B, lane 10 / #2116; IDs I17, I18, I42).
//!
//! Pure port of `chainSplitPoint` and `splitChainClip` of the agreed desktop
//! baseline (`src/js/storyboard.js` at fbe4b8a3) over an explicit montage
//! chain, plus the retime defaults (`symbolDuration`, `ensureRetime`, the
//! "reset retiming" menu item) they rest on. The sound split, the instance /
//! montage creation and the module-id request live in the child [`modules`].
//!
//! The caller hands over a [`ShotHost`]: the montage `playhead` and the chain
//! ids in order, each with what `moduleById` found (dangling id, non-instance
//! module, or an [`InstanceModule`]). No record type is invented for the
//! storyboard; lane 4 `object_storyboard_*` records build this view. A
//! dangling id and a non-instance module are skipped, never an error.
//!
//! The baseline wrote `ensureRetime` defaults back into the stored module as a
//! side effect of reading. This kernel never writes: it returns the resolved
//! [`Retime`] and an edit ([`ChainSplit`]) that carries every changed field.
//! `splitChainClip` has no guard in the baseline; its callers only reach it
//! through `chainSplitPoint`. Here a cut that is not strictly inside a member
//! of positive duration, a member with fewer than two source frames (the left
//! half would get a source length of zero) and a target that is not an
//! instance of the chain are typed errors and nothing partial is returned.
//! A module listed twice in one chain is split at its FIRST occurrence and both
//! occurrences follow the shortened module (`indexOf`, shared object): the
//! chain total then changes, as in the baseline; the oracle pins it.
//!
//! Numbers are JavaScript numbers in the baseline: frames are integers inside
//! 2^53 (`i64` here, anything outside is [`ShotChainError::Overflow`]) and
//! `Math.round` is [`modules::js_round`].

#[path = "object_shot_modules.rs"]
pub(crate) mod modules;

use modules::{js_round, ModuleRef, NewInstance};

/// `symbolDuration` fallback for a missing symbol or one with no length.
pub(crate) const DEFAULT_SYMBOL_DURATION: i64 = 24;
/// Largest integer a JavaScript number holds exactly (2^53 - 1).
pub(crate) const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Typed failures of the shot kernels; no partial output accompanies any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShotChainError {
    /// An integer left the JavaScript-exact range.
    Overflow,
    /// A coordinate was NaN or infinite.
    NonFinite,
    /// The split target is not in the chain.
    MemberNotFound,
    /// The split target is in the chain but is not an instance module.
    MemberNotInstance,
    /// The member's duration is below one frame.
    InvalidDuration { duration: i64 },
    /// The cut is not strictly inside `1..duration`.
    LocalOutOfRange { local: i64, duration: i64 },
    /// The member has fewer than two source frames to share between halves.
    SourceTooShort { src_len: i64 },
    /// The sound module has no cursor (absent or null).
    CursorMissing,
    /// The cursor is not strictly inside the sound range.
    CursorNotInside,
}

/// What the consumer knows about a member's symbol (`state.symbols[id]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SymbolView {
    /// No symbol under that id: duration 24.
    Missing,
    /// `totalFrames` (0 when absent or zero, both falsy) and the frame count of
    /// `layers[0]` (`None` without a first layer).
    Present {
        total_frames: i64,
        first_layer_frame_count: Option<i64>,
    },
}

/// An `instance` module. A `None` trim or duration is JavaScript
/// `null`/`undefined`; `x`/`y` are the module position.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InstanceModule {
    pub symbol_id: String,
    pub x: f64,
    pub y: f64,
    pub trim_in: Option<i64>,
    pub trim_out: Option<i64>,
    pub duration: Option<i64>,
    pub symbol: SymbolView,
}

/// What `moduleById` plus the `type === 'instance'` filter see for a chain id.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EntryKind {
    Dangling,
    NotInstance,
    Instance(InstanceModule),
}

/// One id of `host.chain` with what it resolves to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChainEntry {
    pub module_id: String,
    pub kind: EntryKind,
}

/// A montage: `host.playhead` (`None` reads as `0`, `host.playhead || 0`) and
/// `host.chain` in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShotHost {
    pub playhead: Option<i64>,
    pub chain: Vec<ChainEntry>,
}

/// Resolved `ensureRetime` triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Retime {
    pub trim_in: i64,
    pub trim_out: i64,
    pub duration: i64,
}

fn safe(value: i64) -> Result<i64, ShotChainError> {
    if value.unsigned_abs() > MAX_SAFE_INTEGER as u64 {
        Err(ShotChainError::Overflow)
    } else {
        Ok(value)
    }
}

fn add(left: i64, right: i64) -> Result<i64, ShotChainError> {
    safe(left.checked_add(right).ok_or(ShotChainError::Overflow)?)
}

fn sub(left: i64, right: i64) -> Result<i64, ShotChainError> {
    safe(left.checked_sub(right).ok_or(ShotChainError::Overflow)?)
}

/// `symbolDuration`: `totalFrames || layers[0].frames.length || 24`.
pub(crate) fn symbol_duration(symbol: &SymbolView) -> i64 {
    match symbol {
        SymbolView::Missing => DEFAULT_SYMBOL_DURATION,
        SymbolView::Present {
            total_frames,
            first_layer_frame_count,
        } => {
            if *total_frames != 0 {
                *total_frames
            } else {
                match *first_layer_frame_count {
                    Some(count) if count != 0 => count,
                    _ => DEFAULT_SYMBOL_DURATION,
                }
            }
        }
    }
}

/// `ensureRetime`: `trimIn = 0`, `trimOut = symbolDuration - 1`, `duration =
/// trimOut - trimIn + 1`, each only when absent, in that order.
pub(crate) fn ensure_retime(member: &InstanceModule) -> Result<Retime, ShotChainError> {
    let trim_in = safe(member.trim_in.unwrap_or(0))?;
    let trim_out = match member.trim_out {
        Some(value) => safe(value)?,
        None => sub(symbol_duration(&member.symbol), 1)?,
    };
    let duration = match member.duration {
        Some(value) => safe(value)?,
        None => add(sub(trim_out, trim_in)?, 1)?,
    };
    Ok(Retime {
        trim_in,
        trim_out,
        duration,
    })
}

/// The "reset retiming" menu item: `trimIn = 0`, `trimOut = symbolDuration -
/// 1`, `duration = trimOut + 1`, whatever the module held before.
pub(crate) fn reset_retime(symbol: &SymbolView) -> Result<Retime, ShotChainError> {
    let trim_out = sub(symbol_duration(symbol), 1)?;
    Ok(Retime {
        trim_in: 0,
        trim_out,
        duration: add(trim_out, 1)?,
    })
}

/// `montageTotal`: the sum of the retimed durations of the instance members.
pub(crate) fn chain_total(host: &ShotHost) -> Result<i64, ShotChainError> {
    let mut total = 0i64;
    for entry in &host.chain {
        if let EntryKind::Instance(member) = &entry.kind {
            total = add(total, ensure_retime(member)?.duration)?;
        }
    }
    Ok(total)
}

/// The two comparisons of `chainSplitPoint`, injectable for the mutation
/// controls: which member the frame lies in, and whether the local frame is a
/// real cut.
#[derive(Clone, Copy)]
pub(crate) struct SplitRules {
    /// `f < acc + mb.duration`.
    pub in_member: fn(frame: i64, start: i64, duration: i64) -> bool,
    /// `local > 0 && local < mb.duration`.
    pub strictly_inside: fn(local: i64, duration: i64) -> bool,
}

fn frame_in_member(frame: i64, start: i64, duration: i64) -> bool {
    frame < start + duration
}

fn local_is_cut(local: i64, duration: i64) -> bool {
    local > 0 && local < duration
}

pub(crate) const BASELINE_RULES: SplitRules = SplitRules {
    in_member: frame_in_member,
    strictly_inside: local_is_cut,
};

/// `chainSplitPoint` with explicit rules: the local frame of `target_id` at the
/// playhead, or `None` when the playhead is not strictly inside that member
/// (an edge, no member, or a different member).
pub(crate) fn chain_split_point_with(
    host: &ShotHost,
    target_id: &str,
    rules: &SplitRules,
) -> Result<Option<i64>, ShotChainError> {
    let frame = safe(host.playhead.unwrap_or(0))?;
    let mut start = 0i64;
    for entry in &host.chain {
        let member = match &entry.kind {
            EntryKind::Instance(member) => member,
            _ => continue,
        };
        let duration = ensure_retime(member)?.duration;
        if (rules.in_member)(frame, start, duration) {
            if entry.module_id != target_id {
                return Ok(None);
            }
            let local = sub(frame, start)?;
            return Ok(if (rules.strictly_inside)(local, duration) {
                Some(local)
            } else {
                None
            });
        }
        start = add(start, duration)?;
    }
    Ok(None)
}

/// `chainSplitPoint` on the baseline rules.
pub(crate) fn chain_split_point(
    host: &ShotHost,
    target_id: &str,
) -> Result<Option<i64>, ShotChainError> {
    chain_split_point_with(host, target_id, &BASELINE_RULES)
}

/// Source frames kept by the left half: `min(srcLen - 1, max(1, round(local /
/// speed)))` with `speed = duration / srcLen`. Injectable for the controls.
pub(crate) type LeftSrcLen = fn(src_len: i64, duration: i64, local: i64) -> i64;

pub(crate) fn left_src_len(src_len: i64, duration: i64, local: i64) -> i64 {
    let speed = duration as f64 / src_len as f64;
    let wanted = js_round(local as f64 / speed);
    wanted.max(1.0).min((src_len - 1) as f64) as i64
}

/// The shortened left member: the stored module is edited in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LeftHalf {
    pub module_id: String,
    pub trim_in: i64,
    pub trim_out: i64,
    pub duration: i64,
}

/// `splitChainClip`: the left half, the new right module (request `0`,
/// appended to the module list) and the new chain with [`ModuleRef::New`]
/// inserted right after the first occurrence of the split member.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChainSplit {
    pub left: LeftHalf,
    pub right: NewInstance,
    pub chain: Vec<ModuleRef>,
}

/// `splitChainClip` with an explicit left-source rule; see the module docs for
/// the errors. `host` is never changed.
pub(crate) fn split_chain_clip_with(
    host: &ShotHost,
    target_id: &str,
    local: i64,
    pick: LeftSrcLen,
) -> Result<ChainSplit, ShotChainError> {
    let index = host
        .chain
        .iter()
        .position(|entry| entry.module_id == target_id)
        .ok_or(ShotChainError::MemberNotFound)?;
    let member = match &host.chain[index].kind {
        EntryKind::Instance(member) => member,
        _ => return Err(ShotChainError::MemberNotInstance),
    };
    let retime = ensure_retime(member)?;
    if retime.duration < 1 {
        return Err(ShotChainError::InvalidDuration {
            duration: retime.duration,
        });
    }
    if local < 1 || local >= retime.duration {
        return Err(ShotChainError::LocalOutOfRange {
            local,
            duration: retime.duration,
        });
    }
    let src_len = add(sub(retime.trim_out, retime.trim_in)?, 1)?;
    if src_len < 2 {
        return Err(ShotChainError::SourceTooShort { src_len });
    }
    let kept = pick(src_len, retime.duration, local);
    let left_trim_out = add(retime.trim_in, kept - 1)?;
    let right_trim_in = add(retime.trim_in, kept)?;
    let mut chain: Vec<ModuleRef> = host
        .chain
        .iter()
        .map(|entry| ModuleRef::Existing(entry.module_id.clone()))
        .collect();
    chain.insert(index + 1, ModuleRef::New(0));
    Ok(ChainSplit {
        left: LeftHalf {
            module_id: target_id.to_string(),
            trim_in: retime.trim_in,
            trim_out: left_trim_out,
            duration: local,
        },
        right: NewInstance {
            request: 0,
            symbol_id: member.symbol_id.clone(),
            x: member.x,
            y: member.y,
            trim_in: Some(right_trim_in),
            trim_out: Some(retime.trim_out),
            duration: Some(retime.duration - local),
        },
        chain,
    })
}

/// `splitChainClip` on the baseline rule.
pub(crate) fn split_chain_clip(
    host: &ShotHost,
    target_id: &str,
    local: i64,
) -> Result<ChainSplit, ShotChainError> {
    split_chain_clip_with(host, target_id, local, left_src_len)
}

#[cfg(test)]
#[path = "../tests/object_shot_chain_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/object_shot_chain.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_shot_chain_controls.rs"]
mod controls;
