//! Whole-selection key batches on one track: shift (nudge and drag), delete, flip, distribute.
//! Pure over a retained track; the track-edit owner resolves selections and publishes once.
use crate::object_document::{OrderKey, OrderTrack};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;
type R<T> = Result<T, DispatchErrorCode>;

/// Largest accepted shift, far beyond any composition length.
pub(crate) const MAX_SHIFT: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyBatch {
    Shift(i64),
    Delete,
    Flip,
    Distribute,
}

fn frame_of(key: &OrderKey) -> R<f64> {
    key.frame
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(DispatchErrorCode::InvalidRequest)
}

/// Whole frames keep their integer wire form so an unchanged key stays byte-identical.
fn frame_number(v: f64) -> R<Number> {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        Ok(Number::from(v as i64))
    } else {
        Number::from_f64(v).ok_or(DispatchErrorCode::InvalidRequest)
    }
}

/// `selected` indexes the ORIGINAL track and is ascending (a track is ordered by frame).
/// `Ok(None)` means every key was deleted: the caller clears the track.
pub(crate) fn apply(
    action: KeyBatch,
    original: &OrderTrack,
    selected: &[usize],
    total_frames: u32,
) -> R<Option<OrderTrack>> {
    let mut frames = original
        .keys
        .iter()
        .map(frame_of)
        .collect::<R<Vec<f64>>>()?;
    let mut chosen = selected.to_vec();
    chosen.sort_unstable();
    chosen.dedup();
    if chosen.iter().any(|&i| i >= frames.len()) {
        return Err(DispatchErrorCode::NotFound);
    }
    let before = frames.clone();
    match action {
        KeyBatch::Delete => {
            let keys: Vec<OrderKey> = original
                .keys
                .iter()
                .enumerate()
                .filter(|(i, _)| chosen.binary_search(i).is_err())
                .map(|(_, k)| k.clone())
                .collect();
            return Ok(if keys.is_empty() {
                None
            } else {
                Some(OrderTrack { keys })
            });
        }
        KeyBatch::Shift(delta) => {
            if delta.abs() > MAX_SHIFT {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            for &i in &chosen {
                frames[i] += delta as f64;
                if frames[i] < 0.0 || frames[i] > f64::from(total_frames) - 1.0 {
                    return Err(DispatchErrorCode::Unavailable);
                }
            }
        }
        KeyBatch::Flip => {
            // Each selected key takes the slot of its mirror; keys carry their own value and curve.
            for (pos, &i) in chosen.iter().enumerate() {
                frames[i] = before[chosen[chosen.len() - 1 - pos]];
            }
        }
        KeyBatch::Distribute => {
            if chosen.len() >= 2 {
                let first = before[chosen[0]];
                let step = (before[chosen[chosen.len() - 1]] - first) / (chosen.len() - 1) as f64;
                for (pos, &i) in chosen.iter().enumerate() {
                    // Math.round of the baseline distributeKeys: halves round up.
                    frames[i] = (first + step * pos as f64 + 0.5).floor();
                }
            }
        }
    }
    let landing = if action == KeyBatch::Distribute { Landing::Replace } else { Landing::Refuse };
    settle(original, &chosen, &before, frames, landing)
}

/// What happens when a selected key lands on the frame of another key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    /// Shift and flip: any other key on the landing frame refuses the whole edit.
    Refuse,
    /// Distribute (baseline `distributeKeys` assigns frames): the landing key replaces the one it lands on.
    Replace,
    /// The box gestures (baseline `onDragMove`): selected keys may pile onto one frame, a key that stays still blocks.
    Collapse,
}

/// Applies the new `frames` of the `chosen` keys. A track holds one key per frame, so a landing on a key's frame is settled by `landing`.
fn settle(
    original: &OrderTrack,
    chosen: &[usize],
    before: &[f64],
    frames: Vec<f64>,
    landing: Landing,
) -> R<Option<OrderTrack>> {
    if frames == before {
        return Ok(Some(original.clone()));
    }
    // Baseline `nudgeSelectedKeys` refuses a shift onto a key that stays; `distributeKeys` and `flipKeys` just assign frames. A track holds
    // one key per frame, so a distribute that lands on another key's frame replaces that key (among the selected keys the later one wins).
    // Flip permutes the selected frames among themselves and a shift moves them together, so neither can share a frame among selected keys.
    let mut dropped: Vec<usize> = Vec::new();
    for (a, &i) in chosen.iter().enumerate() {
        for (j, f) in frames.iter().enumerate() {
            if j == i || *f != frames[i] {
                continue;
            }
            let earlier_selected = chosen[..a].contains(&j);
            let staying = chosen.binary_search(&j).is_err();
            match landing {
                Landing::Refuse if staying || earlier_selected => return Err(DispatchErrorCode::Unavailable),
                Landing::Collapse if staying => return Err(DispatchErrorCode::Unavailable),
                Landing::Replace if staying || earlier_selected => dropped.push(j),
                _ => {}
            }
        }
    }
    if landing == Landing::Collapse {
        collapse(chosen, &frames, &mut dropped);
    }
    let mut keys = original.keys.clone();
    for &i in chosen {
        keys[i].frame = frame_number(frames[i])?;
    }
    // Stable by the new frame: unmoved keys keep their relative order.
    let mut order: Vec<usize> = (0..keys.len()).filter(|i| !dropped.contains(i)).collect();
    order.sort_by(|&a, &b| frames[a].total_cmp(&frames[b]));
    Ok(Some(OrderTrack {
        keys: order.into_iter().map(|i| keys[i].clone()).collect(),
    }))
}

/// Keeps ONE key of each frame the selected keys piled onto: the key the baseline evaluation (`rawValueAtFrame`, over keys sorted
/// stably by frame) answers with on that frame. A frame at or before the first key answers with the first key of the pile; every other
/// frame, interior or at the end, answers with the last key of the pile (`segmentIndexAtFrame` steps past an equal-frame left key).
fn collapse(chosen: &[usize], frames: &[f64], dropped: &mut Vec<usize>) {
    let first_frame = frames.iter().copied().fold(f64::INFINITY, f64::min);
    let mut seen: Vec<f64> = Vec::new();
    for &i in chosen {
        if seen.contains(&frames[i]) {
            continue;
        }
        seen.push(frames[i]);
        let pile: Vec<usize> = chosen.iter().copied().filter(|&j| frames[j] == frames[i]).collect();
        if pile.len() < 2 {
            continue;
        }
        let keep = if frames[i] == first_frame { pile[0] } else { pile[pile.len() - 1] };
        dropped.extend(pile.into_iter().filter(|&j| j != keep));
    }
}

/// The box gestures of a multi-track key selection (baseline `startSkewDrag` + `onDragMove`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WarpMode {
    /// Top edge: the first row moves the whole distance, the last stays, rows between interpolate.
    Top,
    /// Bottom edge: the mirror of `Top`.
    Bottom,
    /// Left edge: the last frame of the selection moves the whole distance, the first stays.
    Left,
    /// Right edge: the mirror of `Left`.
    Right,
    /// Gaussian falloff centred on frame `grab`.
    Liquify { grab: f64 },
}

/// Where the keys of one row sit in the whole selection: `row` of `rows`, over the selected frames `min..=max`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WarpFrame {
    pub(crate) row: usize,
    pub(crate) rows: usize,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

/// The share of the drag `total` that moves a key originally at `at`: the factor of the baseline, per key.
pub(crate) fn warp_factor(mode: WarpMode, at: f64, frame: WarpFrame) -> f64 {
    let (n, r) = (frame.rows as f64, frame.row as f64);
    match mode {
        WarpMode::Top | WarpMode::Bottom if frame.rows < 2 => 1.0,
        WarpMode::Top => (n - 1.0 - r) / (n - 1.0),
        WarpMode::Bottom => r / (n - 1.0),
        WarpMode::Left | WarpMode::Right => {
            let span = frame.max - frame.min;
            if span <= 0.0 {
                0.0
            } else if mode == WarpMode::Right {
                (at - frame.min) / span
            } else {
                (frame.max - at) / span
            }
        }
        WarpMode::Liquify { grab } => {
            let radius = (frame.max - frame.min).max(1.0) * 0.35;
            let dist = (at - grab) / radius.max(2.0);
            (-dist * dist).exp()
        }
    }
}

/// Moves every selected key of one row by `round(total * factor)` from its ORIGINAL frame. All keys of the gesture land inside
/// `0..total_frames` and on no key that stays, or the whole gesture is refused.
pub(crate) fn warp(
    mode: WarpMode,
    total: f64,
    original: &OrderTrack,
    selected: &[usize],
    frame: WarpFrame,
    total_frames: u32,
) -> R<Option<OrderTrack>> {
    if !total.is_finite() || total.abs() > MAX_SHIFT as f64 {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let mut frames = original
        .keys
        .iter()
        .map(frame_of)
        .collect::<R<Vec<f64>>>()?;
    let mut chosen = selected.to_vec();
    chosen.sort_unstable();
    chosen.dedup();
    if chosen.iter().any(|&i| i >= frames.len()) {
        return Err(DispatchErrorCode::NotFound);
    }
    let before = frames.clone();
    for &i in &chosen {
        // Math.round of the baseline: halves round up.
        let by = (total * warp_factor(mode, before[i], frame) + 0.5).floor();
        frames[i] = before[i] + by;
        if frames[i] < 0.0 || frames[i] > f64::from(total_frames) - 1.0 {
            return Err(DispatchErrorCode::Unavailable);
        }
    }
    settle(original, &chosen, &before, frames, Landing::Collapse)
}
