//! Montage timing kernel (L10-A, lane 10 / #2116; IDs I18, I19, I20, I36).
//!
//! Pure port of the StoryBoard montage time model of the agreed desktop
//! baseline (fbe4b8a3): `symbolDuration`, `ensureRetime`, `chainMods`,
//! `montageTotal` and `montageStrokesAt` (`src/js/storyboard.js`), the looping
//! montage-layer frame of `getEffectiveStrokes` and the per-member segment
//! `enterMontageView` builds (`src/js/app.js`), plus the `once` branch of
//! `resolveSymbolFrameIdx` that a montage-view segment is replayed through.
//!
//! Two source-frame formulas exist in the baseline and are KEPT APART on
//! purpose, each for its own caller. The montage preview/layer uses
//! `trimIn + min(srcLen - 1, floor(local * srcLen / duration))`; a montage-view
//! segment is replayed with `min(trimOut, trimIn + floor(local * (srcLen /
//! duration)))`. They disagree where float rounding of `srcLen / duration`
//! lands just below an integer; `tests/object_montage_timing.rs` pins every such
//! point of its grid. Neither is "fixed" here.
//!
//! No storyboard, module, symbol or component record is defined by this file.
//! The caller (lane 4 `object_storyboard_*` records) implements
//! [`MontageModules`] and hands over [`MontageMember`] values; a dangling chain
//! id and a non-`instance` module are skipped, never an error. `ensureRetime`
//! persisted its defaults on the stored module in the baseline; this kernel
//! returns the resolved [`Retime`] and never writes, so persisting a default is
//! a decision of the consumer's own transaction. Numbers are JavaScript numbers
//! in the baseline: frame and trim values are integers well inside 2^53, so the
//! float expressions below are bit-identical to the baseline. A negative
//! montage frame is unreachable there (the layer path returns before it) and is
//! a typed error here instead of the baseline's meaningless result.

/// `symbolDuration` fallback for a missing symbol or one with no length.
pub(crate) const DEFAULT_SYMBOL_DURATION: i64 = 24;
/// Segment name stem for a member whose symbol is missing.
pub(crate) const MISSING_SYMBOL_NAME: &str = "Composant";
/// Suffix of a montage-view segment name.
pub(crate) const VIEW_NAME_SUFFIX: &str = " \u{2014} montage";

/// Typed failures of the kernel; no partial output accompanies any of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MontageTimingError {
    /// An integer expression left the `i64` range.
    Overflow,
    /// A montage frame below zero (never produced by the baseline callers).
    NegativeFrame,
    /// `enterMontageView` on a montage with no instance member (`hsMontageEmpty`).
    EmptyChain,
}

/// What the consumer knows about a member's symbol (`state.symbols[id]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SymbolView {
    /// No symbol under that id: duration 24, name `Composant`.
    Missing,
    /// `totalFrames` (0 when absent or zero, both falsy), the frame count of
    /// `layers[0]` (`None` without a first layer) and the optional `name`.
    Present {
        name: Option<String>,
        total_frames: i64,
        first_layer_frame_count: Option<i64>,
    },
}

/// An `instance` module of a montage chain, as the baseline stores it. A `None`
/// trim or duration is JavaScript `null`/`undefined` (`== null`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MontageMember {
    pub symbol_id: String,
    pub trim_in: Option<i64>,
    pub trim_out: Option<i64>,
    pub duration: Option<i64>,
    pub symbol: SymbolView,
}

/// What `moduleById` plus the `type === 'instance'` filter see for a chain id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChainSlot {
    /// The id names no module (a deleted module).
    Dangling,
    /// The module exists but is not an `instance` (sound, montage, ...).
    NotInstance,
    Instance(MontageMember),
}

/// Consumer port: lane 4 storyboard records implement the module lookup.
pub(crate) trait MontageModules {
    fn slot(&self, module_id: &str) -> ChainSlot;
}

/// Resolved `ensureRetime` triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Retime {
    pub trim_in: i64,
    pub trim_out: i64,
    pub duration: i64,
}

/// A chain member after the skip rule and `ensureRetime`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChainMember {
    pub module_id: String,
    pub symbol_id: String,
    pub retime: Retime,
    pub symbol: SymbolView,
}

/// `symbolDuration`: `totalFrames || layers[0].frames.length || 24`.
pub(crate) fn symbol_duration(symbol: &SymbolView) -> i64 {
    match symbol {
        SymbolView::Missing => DEFAULT_SYMBOL_DURATION,
        SymbolView::Present {
            total_frames,
            first_layer_frame_count,
            ..
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
/// trimOut - trimIn + 1`, each only when absent (lazily, in that order).
pub(crate) fn ensure_retime(member: &MontageMember) -> Result<Retime, MontageTimingError> {
    let trim_in = member.trim_in.unwrap_or(0);
    let trim_out = match member.trim_out {
        Some(value) => value,
        None => symbol_duration(&member.symbol)
            .checked_sub(1)
            .ok_or(MontageTimingError::Overflow)?,
    };
    let duration = match member.duration {
        Some(value) => value,
        None => trim_out
            .checked_sub(trim_in)
            .and_then(|span| span.checked_add(1))
            .ok_or(MontageTimingError::Overflow)?,
    };
    Ok(Retime {
        trim_in,
        trim_out,
        duration,
    })
}

/// `chainMods` with `ensureRetime` applied: dangling ids and non-instance
/// modules are skipped; an absent `m.chain` is the empty slice.
pub(crate) fn resolve_chain<M: MontageModules>(
    chain: &[String],
    modules: &M,
) -> Result<Vec<ChainMember>, MontageTimingError> {
    let mut members = Vec::new();
    for module_id in chain {
        if let ChainSlot::Instance(member) = modules.slot(module_id) {
            let retime = ensure_retime(&member)?;
            members.push(ChainMember {
                module_id: module_id.clone(),
                symbol_id: member.symbol_id,
                retime,
                symbol: member.symbol,
            });
        }
    }
    Ok(members)
}

/// `montageTotal`: the sum of the member durations (zero for an empty chain).
pub(crate) fn montage_total(members: &[ChainMember]) -> Result<i64, MontageTimingError> {
    let mut total: i64 = 0;
    for member in members {
        total = total
            .checked_add(member.retime.duration)
            .ok_or(MontageTimingError::Overflow)?;
    }
    Ok(total)
}

/// `srcLen = trimOut - trimIn + 1` of a retimed member.
pub(crate) fn source_length(retime: &Retime) -> Result<i64, MontageTimingError> {
    retime
        .trim_out
        .checked_sub(retime.trim_in)
        .and_then(|span| span.checked_add(1))
        .ok_or(MontageTimingError::Overflow)
}

/// The montage formula: `trimIn + min(srcLen - 1, floor(local * srcLen /
/// duration))`, evaluated in doubles exactly as the baseline does.
pub(crate) fn storyboard_source_frame(
    trim_in: i64,
    src_len: i64,
    local: i64,
    duration: i64,
) -> Result<i64, MontageTimingError> {
    let scaled = (local as f64 * src_len as f64 / duration as f64).floor();
    let cap = src_len.checked_sub(1).ok_or(MontageTimingError::Overflow)? as f64;
    let picked = if scaled < cap { scaled } else { cap };
    trim_in
        .checked_add(picked as i64)
        .ok_or(MontageTimingError::Overflow)
}

/// Which source frame of which member a montage frame shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceFrame {
    pub member_index: usize,
    pub symbol_id: String,
    pub src_frame: i64,
}

/// A source-frame formula (`trim_in`, `src_len`, `local`, `duration`).
pub(crate) type SourcePick = fn(i64, i64, i64, i64) -> Result<i64, MontageTimingError>;

/// `montageStrokesAt` with the formula injected, so the controls can run a
/// deliberately wrong one through the same chain walk. `None` is the baseline's
/// empty result for a frame at or beyond the total.
pub(crate) fn montage_source_frame_with(
    members: &[ChainMember],
    frame: i64,
    pick: SourcePick,
) -> Result<Option<SourceFrame>, MontageTimingError> {
    if frame < 0 {
        return Err(MontageTimingError::NegativeFrame);
    }
    let mut acc: i64 = 0;
    for (member_index, member) in members.iter().enumerate() {
        let duration = member.retime.duration;
        let end = acc
            .checked_add(duration)
            .ok_or(MontageTimingError::Overflow)?;
        if frame < end {
            // frame >= acc here, so duration >= 1 and local >= 0.
            let local = frame.checked_sub(acc).ok_or(MontageTimingError::Overflow)?;
            let src_len = source_length(&member.retime)?;
            let src_frame = pick(member.retime.trim_in, src_len, local, duration)?;
            return Ok(Some(SourceFrame {
                member_index,
                symbol_id: member.symbol_id.clone(),
                src_frame,
            }));
        }
        acc = end;
    }
    Ok(None)
}

/// `montageStrokesAt`'s frame choice with the baseline formula.
pub(crate) fn montage_source_frame(
    members: &[ChainMember],
    frame: i64,
) -> Result<Option<SourceFrame>, MontageTimingError> {
    montage_source_frame_with(members, frame, storyboard_source_frame)
}

/// Why a montage layer shows nothing at a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerNone {
    /// `montageTotal` is zero (`!mtot`).
    ZeroTotal,
    /// `frame - (symPlacedAt || 0)` is negative.
    BeforePlacement,
}

/// The looping montage frame of a montage layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerFrame {
    None(LayerNone),
    /// `mlf % montageTotal`, the montage-local frame to resolve.
    Local(i64),
}

/// `getEffectiveStrokes`' montage branch: zero total first, then `mlf = frame -
/// (symPlacedAt || 0)`, `mlf < 0` is empty, else `mlf % montageTotal`.
pub(crate) fn montage_layer_frame(
    frame: i64,
    placed_at: Option<i64>,
    total: i64,
) -> Result<LayerFrame, MontageTimingError> {
    if total == 0 {
        return Ok(LayerFrame::None(LayerNone::ZeroTotal));
    }
    let mlf = frame
        .checked_sub(placed_at.unwrap_or(0))
        .ok_or(MontageTimingError::Overflow)?;
    if mlf < 0 {
        return Ok(LayerFrame::None(LayerNone::BeforePlacement));
    }
    // total != 0 and mlf >= 0, so the remainder cannot trap.
    Ok(LayerFrame::Local(mlf % total))
}

/// The whole montage-layer path: total, looping frame, then source frame.
pub(crate) fn montage_layer_source(
    members: &[ChainMember],
    frame: i64,
    placed_at: Option<i64>,
) -> Result<Option<SourceFrame>, MontageTimingError> {
    let total = montage_total(members)?;
    match montage_layer_frame(frame, placed_at, total)? {
        LayerFrame::Local(local) => montage_source_frame(members, local),
        LayerFrame::None(_) => Ok(None),
    }
}

/// One layer `enterMontageView` creates per chain member (the empty `frames`
/// array of `total_frames` entries, first one a keyframe, is the consumer's).
#[derive(Clone, Debug)]
pub(crate) struct ViewSegment {
    pub name: String,
    pub symbol_id: String,
    pub locked: bool,
    pub visible: bool,
    pub sym_play_mode: &'static str,
    /// `srcLen / duration` as a JavaScript division (Infinity or NaN at duration 0).
    pub sym_speed: f64,
    pub sym_placed_at: i64,
    pub sym_trim_in: i64,
    pub sym_trim_out: i64,
    pub sym_single_frame: i64,
    pub in_point: i64,
    pub out_point: i64,
}

/// `enterMontageView`'s layer set for a montage.
#[derive(Clone, Debug)]
pub(crate) struct MontageViewSpec {
    pub total_frames: i64,
    pub segments: Vec<ViewSegment>,
}

/// The segments of `enterMontageView`: an empty chain is `EmptyChain` (the
/// `hsMontageEmpty` toast); each member plays `once` at `symSpeed = srcLen /
/// duration` from `symPlacedAt = acc`, `inPoint = acc`, `outPoint = acc +
/// duration - 1`.
pub(crate) fn montage_view_segments(
    members: &[ChainMember],
) -> Result<MontageViewSpec, MontageTimingError> {
    if members.is_empty() {
        return Err(MontageTimingError::EmptyChain);
    }
    let total_frames = montage_total(members)?;
    let mut segments = Vec::with_capacity(members.len());
    let mut acc: i64 = 0;
    for member in members {
        let retime = &member.retime;
        let src_len = source_length(retime)?;
        let stem = match &member.symbol {
            SymbolView::Missing => MISSING_SYMBOL_NAME,
            // A present symbol without a name concatenates as "undefined".
            SymbolView::Present { name, .. } => name.as_deref().unwrap_or("undefined"),
        };
        let end = acc
            .checked_add(retime.duration)
            .ok_or(MontageTimingError::Overflow)?;
        segments.push(ViewSegment {
            name: format!("{stem}{VIEW_NAME_SUFFIX}"),
            symbol_id: member.symbol_id.clone(),
            locked: true,
            visible: true,
            sym_play_mode: "once",
            sym_speed: src_len as f64 / retime.duration as f64,
            sym_placed_at: acc,
            sym_trim_in: retime.trim_in,
            sym_trim_out: retime.trim_out,
            sym_single_frame: 0,
            in_point: acc,
            out_point: end.checked_sub(1).ok_or(MontageTimingError::Overflow)?,
        });
        acc = end;
    }
    Ok(MontageViewSpec {
        total_frames,
        segments,
    })
}

/// The fields of a segment layer that `resolveSymbolFrameIdx` reads in `once`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OnceLayer {
    pub sym_trim_in: Option<i64>,
    pub sym_trim_out: Option<i64>,
    pub sym_placed_at: Option<i64>,
    pub sym_speed: Option<f64>,
}

impl ViewSegment {
    /// The `once` layer this segment becomes.
    pub(crate) fn once_layer(&self) -> OnceLayer {
        OnceLayer {
            sym_trim_in: Some(self.sym_trim_in),
            sym_trim_out: Some(self.sym_trim_out),
            sym_placed_at: Some(self.sym_placed_at),
            sym_speed: Some(self.sym_speed),
        }
    }
}

/// `Math.min` (a NaN operand gives NaN).
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// `resolveSymbolFrameIdx` for `symPlayMode === 'once'` with no Time Remap and
/// no outer `componentFrame` key: `elapsed = max(0, main - (placed || 0)) *
/// (speed || 1)`, then `min(trimOut, trimIn + floor(elapsed))` where `trimOut`
/// defaults to `max(1, totalFrames) - 1`. Returned as the JavaScript number.
pub(crate) fn once_symbol_frame(total_frames: i64, layer: &OnceLayer, main_frame: i64) -> f64 {
    let placed = layer.sym_placed_at.unwrap_or(0);
    let speed = match layer.sym_speed {
        Some(value) if value != 0.0 && !value.is_nan() => value,
        _ => 1.0,
    };
    let elapsed = (main_frame as f64 - placed as f64).max(0.0) * speed;
    let total = total_frames.max(1);
    let trim_in = layer.sym_trim_in.unwrap_or(0);
    let trim_out = layer.sym_trim_out.unwrap_or(total - 1);
    js_min(trim_out as f64, trim_in as f64 + elapsed.floor())
}

#[cfg(test)]
#[path = "../tests/object_montage_timing_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/object_montage_timing.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_montage_timing_controls.rs"]
mod controls;
