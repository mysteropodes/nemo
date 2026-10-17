//! Motion-track half of Repeat Selection (F13): what the dense editor's `repeatSelection` does to the `ld.motion`
//! tables of the cycled layers (src/js/timeline.js at fbe4b8a3), expressed on the native property tracks. The drawing-cell
//! half is `object_frame_clipboard_repeat.rs` (L05-J1), which calls `repeat_motion_tracks` on the document it is about to
//! publish, so cells, tracks and the timeline length land in ONE candidate, ONE history entry and ONE Undo.
//!
//! Baseline behavior kept on purpose. For every layer from the first to the last selected one, every table of
//! `ld.motion` that holds keys is walked: a key with `minF <= frame <= maxF` is deep-cloned `times` times, the clone of
//! round `r` sits at `frame + r * span` (`span = maxF - minF + 1`), the clones are appended and the whole key list is
//! sorted by frame with a STABLE sort (keys on one frame keep their order, the original first). A clone keeps every field
//! of its key verbatim (values, hold flag, curve points, in/out handles), so the ease UI reads the repeated key like a
//! hand-placed one. Keys outside the range, static values, expressions, tables of layers outside the bounds and the
//! `elementMotion` of single elements are not touched (the baseline loop never reads them).
//!
//! Native places: a layer-level property track is a property holder of the scene-root context without a stroke id
//! (`PropertyHolderTarget`), and the layer-level `order` table, which the property holder admission leaves to its own
//! owner, is the `order.track` of the layer in the scene-root composition-order plan. Both are `OrderTrack`s of the same
//! `OrderKey`s, so one kernel serves both. Element holders (a stroke id) and other contexts are never read.
//!
//! One deliberate refusal. The baseline keeps two keys on one frame when a clone lands on an existing key (its evaluator
//! then divides by zero between them); a native track must be strictly increasing, so that cycle is refused as
//! `InvalidRequest` before anything is written (decision in HANDOFF.md). Every proposal is computed against the unchanged
//! document first, so a refusal leaves the document exactly as it was.
use crate::object_document::{ObjectDocument, OrderKey, OrderTrack, Presence};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;
use std::cmp::Ordering;

const CONTEXT: &str = "scene-root";
/// The most rounds one cycle runs: the baseline clamps `times` to `1..=50` before it clones anything. The caller (L05-J1)
/// has clamped already; a larger count is a malformed request, refused before any clone is made.
pub(crate) const MAX_ROUNDS: u32 = 50;
type R<T> = Result<T, DispatchErrorCode>;

fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn finite(number: &Number) -> R<f64> {
    number
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(invalid)
}

/// `frame + by` as the baseline prints it after its JSON round trip: a whole result is an integer number (`7`, never
/// `7.0`), a fractional one stays a float. Integers are added exactly; a result that does not fit refuses.
fn shifted(frame: &Number, by: u64) -> R<Number> {
    if let Some(whole) = frame.as_i64() {
        let by = i64::try_from(by).map_err(|_| invalid())?;
        return whole.checked_add(by).map(Number::from).ok_or_else(invalid);
    }
    if let Some(whole) = frame.as_u64() {
        return whole.checked_add(by).map(Number::from).ok_or_else(invalid);
    }
    let sum = finite(frame)? + by as f64;
    if !sum.is_finite() {
        return Err(invalid());
    }
    if sum.fract() == 0.0 && sum.abs() < 9_007_199_254_740_992.0 {
        return Ok(Number::from(sum as i64));
    }
    Number::from_f64(sum).ok_or_else(invalid)
}

/// One track of the cycle: the keys inside `first..=last` are cloned `times` times at `frame + round * span`, merged and
/// sorted by frame (stable). `None` when no key lies in the range (or `times` is 0): the track is returned unchanged by
/// the caller. A clone on a frame that already holds a key refuses (`InvalidRequest`).
pub(crate) fn repeat_track(
    track: &OrderTrack,
    first: u32,
    last: u32,
    times: u32,
) -> R<Option<OrderTrack>> {
    if times > MAX_ROUNDS {
        return Err(invalid());
    }
    let span = u64::from(last.checked_sub(first).ok_or_else(invalid)?) + 1;
    let (low, high) = (f64::from(first), f64::from(last));
    let mut clones: Vec<(f64, OrderKey)> = Vec::new();
    for key in &track.keys {
        let at = finite(&key.frame)?;
        if at < low || at > high {
            continue;
        }
        for round in 1..=u64::from(times) {
            let by = span.checked_mul(round).ok_or_else(invalid)?;
            let mut clone = key.clone();
            clone.frame = shifted(&key.frame, by)?;
            clones.push((finite(&clone.frame)?, clone));
        }
    }
    if clones.is_empty() {
        return Ok(None);
    }
    let mut merged: Vec<(f64, OrderKey)> = Vec::with_capacity(track.keys.len() + clones.len());
    for key in &track.keys {
        merged.push((finite(&key.frame)?, key.clone()));
    }
    merged.extend(clones);
    // Array.prototype.sort is stable, and `a.frame - b.frame` treats equal frames as equal.
    merged.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    if merged.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(invalid());
    }
    Ok(Some(OrderTrack {
        keys: merged.into_iter().map(|(_, key)| key).collect(),
    }))
}

/// The Motion half of repeatSelection over the layers named by `layers` (the ones whose cells the caller cycles):
/// every scene-root layer-level property track and the layer's `order` track in the scene-root composition-order plan.
/// All proposals are computed first, so an error leaves `document` untouched.
pub(crate) fn repeat_motion_tracks(
    document: &mut ObjectDocument,
    layers: &[String],
    first: u32,
    last: u32,
    times: u32,
) -> R<()> {
    if first > last || times > MAX_ROUNDS {
        return Err(invalid());
    }
    let covered = |uid: &str| layers.iter().any(|layer| layer.as_str() == uid);
    let mut holder_edits: Vec<(usize, usize, OrderTrack)> = Vec::new();
    if let Presence::Present(holders) = &document.property_holders {
        for (h, holder) in holders.iter().enumerate() {
            let target = &holder.target;
            if target.context_id != CONTEXT
                || target.stroke_id.value().is_some()
                || !covered(&target.layer_uid)
            {
                continue;
            }
            for (p, record) in holder.properties.iter().enumerate() {
                let Some(track) = record.track.value() else {
                    continue;
                };
                if let Some(next) = repeat_track(track, first, last, times)? {
                    holder_edits.push((h, p, next));
                }
            }
        }
    }
    let mut order_edits: Vec<(usize, usize, OrderTrack)> = Vec::new();
    if let Presence::Present(plan) = &document.composition_order {
        for (c, context) in plan.contexts.iter().enumerate() {
            if context.context_id != CONTEXT {
                continue;
            }
            for (l, layer) in context.layers.iter().enumerate() {
                if !covered(&layer.layer_uid) {
                    continue;
                }
                let Some(track) = layer.order.track.value() else {
                    continue;
                };
                if let Some(next) = repeat_track(track, first, last, times)? {
                    order_edits.push((c, l, next));
                }
            }
        }
    }
    if let Presence::Present(holders) = &mut document.property_holders {
        for (h, p, track) in holder_edits {
            holders[h].properties[p].track = Presence::Present(track);
        }
    }
    if let Presence::Present(plan) = &mut document.composition_order {
        for (c, l, track) in order_edits {
            plan.contexts[c].layers[l].order.track = Presence::Present(track);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_tracks.rs"]
mod tests;
