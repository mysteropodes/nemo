//! U35 (#2362) Labs timing actions over the document: `SMLabs.pingpongCycle`, `retimeExposure` and `intervalAssistant`
//! (`src/js/labs/pingpong-cycle.js`, `retime-exposure.js`, `interval-assistant.js` at 1065e9d8). The timing halves are
//! `object_labs_timing.rs`; this file moves the records with them. Component layers are skipped (the baseline skips
//! `symbolId`), locks are not consulted (the baseline does not), and the timeline grows like Repeat.
use crate::commands::object_fill_owner::object_frame_clipboard_paste::{
    finish, layer_position, timing_refusal,
};
use crate::commands::object_fill_owner::object_frame_edit_document::{
    clone_cell, drop_cells, promote_cells, remap, DocumentEdit, FrameEditError,
};
use crate::commands::object_fill_owner::object_labs_generate::{Ease, CONTEXT};
use crate::object_document::{ObjectDocument, Presence, TimingEdit};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

fn component(document: &ObjectDocument, uid: &str) -> bool {
    document
        .component_set()
        .is_some_and(|set| set.instance(CONTEXT, uid).is_some())
}
/// The selected layer rows `from..=to` by stable id.
fn rows(
    document: &ObjectDocument,
    from: &str,
    to: &str,
) -> Result<std::ops::RangeInclusive<usize>, DispatchErrorCode> {
    let a = layer_position(document, from).ok_or(DispatchErrorCode::NotFound)?;
    let b = layer_position(document, to).ok_or(DispatchErrorCode::NotFound)?;
    if a > b {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok(a..=b)
}
fn grown(total: u32, needed: u64) -> Result<u32, DispatchErrorCode> {
    u32::try_from(needed.max(u64::from(total))).map_err(|_| DispatchErrorCode::InvalidRequest)
}
fn frame_refusal(error: FrameEditError) -> DispatchErrorCode {
    match error {
        FrameEditError::Unsupported | FrameEditError::MissingTiming => {
            DispatchErrorCode::Unavailable
        }
        FrameEditError::UnknownLayer => DispatchErrorCode::NotFound,
        _ => DispatchErrorCode::InvalidRequest,
    }
}

/// pingpongCycle: per selected layer, the block after the range is the range played back and forth `times` times.
pub(super) fn pingpong(
    origin: &ObjectSnapshot,
    from: &str,
    to: &str,
    first: u32,
    last: u32,
    times: u32,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let document = origin.document();
    let total = document.total_frames;
    let rows = rows(document, from, to)?;
    if first >= last || last >= total {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let block = last - first;
    let window_end = u64::from(last) + u64::from(block) * u64::from(times);
    let grown = grown(total, window_end + 1)?;
    let mut plans = Vec::new();
    for index in rows {
        let uid = document.layers[index].layer_uid.clone();
        if component(document, &uid) {
            continue;
        }
        let timing = document.layers[index]
            .timing
            .value()
            .ok_or(DispatchErrorCode::Unavailable)?;
        plans.push((
            index,
            uid,
            timing
                .pingpong_range(first, last, times, total, grown)
                .map_err(timing_refusal)?,
        ));
    }
    let mut next = document.clone();
    next.total_frames = grown;
    for (index, uid, (edit, pairs)) in plans {
        if let TimingEdit::Changed(timing) = edit {
            next.layers[index].timing = Presence::Present(timing);
        }
        // Destinations are replaced whole; every source lies inside the range, before every destination.
        drop_cells(&mut next, &uid, &|frame| {
            frame > last && u64::from(frame) <= window_end
        });
        for (source, destination) in pairs {
            clone_cell(&mut next, &uid, source, destination);
        }
    }
    if next == *document {
        return Ok(DocumentEdit::NoChange);
    }
    finish(next)
}

/// retimeExposure: per selected layer with a key in the range, the keys are re-spaced from `first` by `spacing`.
pub(super) fn retime(
    origin: &ObjectSnapshot,
    from: &str,
    to: &str,
    first: u32,
    last: u32,
    spacing: u32,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let document = origin.document();
    let total = document.total_frames;
    let rows = rows(document, from, to)?;
    if first > last || last >= total {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    // The baseline grows once, to the furthest re-spaced key of every selected layer.
    let mut needed = 0u64;
    for index in rows.clone() {
        let layer = &document.layers[index];
        if component(document, &layer.layer_uid) {
            continue;
        }
        if let Some(timing) = layer.timing.value() {
            let keys = (first..=last)
                .filter(|f| timing.exposure(*f).0 == "key")
                .count() as u64;
            if keys > 0 {
                needed = needed.max(u64::from(first) + (keys - 1) * u64::from(spacing) + 1);
            }
        }
    }
    let grown = grown(total, needed)?;
    let mut plans = Vec::new();
    for index in rows {
        let uid = document.layers[index].layer_uid.clone();
        if component(document, &uid) {
            continue;
        }
        let timing = document.layers[index]
            .timing
            .value()
            .ok_or(DispatchErrorCode::Unavailable)?;
        if let Some(plan) = timing
            .retime_keys(first, last, spacing, total, grown)
            .map_err(timing_refusal)?
        {
            plans.push((index, uid, plan));
        }
    }
    if plans.is_empty() {
        return Err(DispatchErrorCode::NotFound);
    }
    let mut next = document.clone();
    next.total_frames = grown;
    for (index, uid, ((edit, moves), wipe_end)) in plans {
        if let TimingEdit::Changed(timing) = edit {
            next.layers[index].timing = Presence::Present(timing);
        }
        let map = |layer: &str, frame: u32| {
            if layer != uid {
                return Some(frame);
            }
            match moves.iter().find(|(old, _)| *old == frame) {
                Some((_, new)) => Some(*new),
                None if frame >= first && frame <= wipe_end => None,
                None => Some(frame),
            }
        };
        remap(&mut next, &map, &Some).map_err(frame_refusal)?;
    }
    if next == *document {
        return Ok(DocumentEdit::NoChange);
    }
    finish(next)
}

/// intervalAssistant: `count` eased breakdown slots strictly inside `first..last` (`first + round(ease(k / (count + 1))
/// * span)`), each non-key slot promoted to a key holding the drawing it showed (the native ensureKeyframe kernel).
/// Linked keyframe folders are not stored natively, so there is no sibling to synchronize.
pub(super) fn interval(
    origin: &ObjectSnapshot,
    layer_uid: &str,
    first: u32,
    last: u32,
    count: u32,
    ease: Ease,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let document = origin.document();
    let index = layer_position(document, layer_uid).ok_or(DispatchErrorCode::NotFound)?;
    if component(document, layer_uid) {
        return Err(DispatchErrorCode::Unavailable);
    }
    let span = last
        .checked_sub(first)
        .filter(|span| *span >= 2)
        .ok_or(DispatchErrorCode::InvalidRequest)?;
    if last >= document.total_frames || count == 0 || count > span - 1 {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let timing = document.layers[index].timing.value();
    // A layer without timing has no held cells: every frame is its own key, so no slot is free.
    let is_key = |frame: u32| timing.is_none_or(|timing| timing.exposure(frame).0 == "key");
    let mut slots = Vec::new();
    for k in 1..=count {
        let frame =
            first + (ease.at(f64::from(k) / f64::from(count + 1)) * f64::from(span)).round() as u32;
        if frame > first && frame < last && !slots.contains(&frame) && !is_key(frame) {
            slots.push(frame);
        }
    }
    if slots.is_empty() {
        return Err(DispatchErrorCode::NotFound);
    }
    promote_cells(document, layer_uid, &slots).map_err(frame_refusal)
}

#[cfg(test)]
#[path = "../tests/object_labs_timing_actions.rs"]
mod tests;
