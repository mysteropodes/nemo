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
            if action != KeyBatch::Distribute {
                if staying || earlier_selected {
                    return Err(DispatchErrorCode::Unavailable);
                }
                continue;
            }
            if staying || earlier_selected {
                dropped.push(j);
            }
        }
    }
    let mut keys = original.keys.clone();
    for &i in &chosen {
        keys[i].frame = frame_number(frames[i])?;
    }
    // Stable by the new frame: unmoved keys keep their relative order.
    let mut order: Vec<usize> = (0..keys.len()).filter(|i| !dropped.contains(i)).collect();
    order.sort_by(|&a, &b| frames[a].total_cmp(&frames[b]));
    Ok(Some(OrderTrack {
        keys: order.into_iter().map(|i| keys[i].clone()).collect(),
    }))
}
