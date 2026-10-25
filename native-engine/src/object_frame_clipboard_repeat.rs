//! Sealed Repeat Selection transaction (F13): what the dense editor's `repeatSelection` does to the CONTENT of an
//! `ObjectDocument` (src/js/timeline.js at fbe4b8a3), prepared against one retained snapshot and published through the
//! existing revision owner. The sparse timing half is `object_frame_clipboard_repeat_timing.rs`; the payload is
//! `object_frame_clipboard_repeat_wire.rs`; history depth, request replay and dispatch are in
//! `object_frame_clipboard_repeat_history.rs`.
//!
//! Baseline behavior kept on purpose. The selection is reduced to its bounds (first and last layer by position, first
//! and last frame). `times` is `parseInt(times, 10) || 1` clamped to 1..50. The timeline grows to
//! `max(total, last + 1 + span * times)`: every layer gains the same plain cells at its end, whether or not it is cycled
//! (the work-area out point joins publication in the same project revision). Every layer from the
//! first to the last selected one is cycled over the WHOLE frame range, selected cell or not, and the lock flag is never
//! consulted (the baseline has no lock check here). A destination cell is replaced whole by a copy of its source cell: a
//! key stays a key, an in-between stays an in-between without its manual-edit flag, a hold stays a hold, a blank key stays
//! a blank key; the copies keep the original stroke ids, geometry, fill and (in an ordered document) natural positions and
//! authored order, because the same drawing returns and tween matching depends on it. Reference-scope records, other
//! contexts, the element holders, the tables of layers outside the bounds and every layer field other than the timing are never
//! touched. The layer-level Motion tracks of the cycled layers (and their `order` tracks in the composition-order plan) are cloned at
//! `frame + round * span` by `object_frame_clipboard_tracks.rs` inside the same candidate.
//!
//! Atomic: the candidate is detached, non-Clone and sealed to the retained snapshot; it holds the complete after-document
//! (`None` for a semantic no-op, which publishes nothing and keeps redo) validated by the shared strict drawing admission.
//! A layer in the bounds without a timing record is typed `Unavailable` (nothing is invented). Native component-instance layers are skipped using the admitted component table,
//! including their drawing cells and Motion tracks; global timeline growth still applies.
use crate::commands::object_fill_owner::object_frame_clipboard_paste::{
    finish, layer_position, scene_authored, timing_refusal,
};
use crate::commands::object_fill_owner::object_frame_clipboard_repeat_wire::parse_repeat_payload;
use crate::commands::object_fill_owner::object_frame_clipboard_tracks::repeat_motion_tracks;
use crate::commands::object_fill_owner::object_frame_edit_document::{
    clone_cell, drop_cells, DocumentEdit,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    ObjectDocument, ObjectRecord, OrderElement, OrderScope, Presence, TimingEdit,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use std::collections::BTreeSet;

const CONTEXT: &str = "scene-root";

/// The baseline clamp: `Math.max(1, Math.min(50, parseInt(times, 10) || 1))`.
pub(crate) const MIN_TIMES: i64 = 1;
pub(crate) const MAX_TIMES: i64 = 50;

/// A cycle of the selection bounds: first and last layer (every layer between them in document order is cycled) and
/// the first and last frame, inclusive, with the raw `times` the editor was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepeatRequest {
    pub(crate) from_layer: String,
    pub(crate) to_layer: String,
    pub(crate) from_frame: u32,
    pub(crate) to_frame: u32,
    pub(crate) times: i64,
}

/// The repeat count the baseline runs for a raw argument: zero and negative counts run once, above 50 runs 50 times.
pub(crate) fn clamp_times(times: i64) -> u32 {
    times.clamp(MIN_TIMES, MAX_TIMES) as u32
}

/// The frame count after a cycle: `max(total, last + 1 + span * times)`. `None` when the range is empty or the result
/// does not fit the frame counter.
pub(crate) fn grown_total(total: u32, first: u32, last: u32, times: u32) -> Option<u32> {
    let span = u64::from(last.checked_sub(first)?) + 1;
    let needed = u64::from(last) + 1 + span * u64::from(times);
    u32::try_from(needed).ok().map(|needed| needed.max(total))
}

/// Scene-root composition-order scopes (none for a v1 document).
fn scene_scopes(document: &ObjectDocument) -> impl Iterator<Item = &OrderScope> {
    let contexts = match &document.composition_order {
        Presence::Present(plan) => Some(&plan.contexts),
        Presence::Missing => None,
    };
    contexts
        .into_iter()
        .flatten()
        .filter(|context| context.context_id == CONTEXT)
        .flat_map(|context| context.scopes.iter())
}
fn records_at<'a>(document: &'a ObjectDocument, layer: &str, frame: u32) -> Vec<&'a ObjectRecord> {
    document
        .objects
        .iter()
        .filter(|record| {
            scene_authored(record)
                && record.target.layer_uid == layer
                && record.target.frame_scope.frame == frame
        })
        .collect()
}
fn elements_at<'a>(document: &'a ObjectDocument, layer: &str, frame: u32) -> Vec<&'a OrderElement> {
    scene_scopes(document)
        .filter(|scope| scope.frame_scope.frame == frame)
        .flat_map(|scope| scope.elements.iter())
        .filter(|element| element.layer_uid == layer)
        .collect()
}
/// The frames of `from..=to` where the layer already has authored records or ordered elements.
fn occupied(document: &ObjectDocument, layer: &str, from: u32, to: u32) -> BTreeSet<u32> {
    let inside = |frame: u32| frame >= from && frame <= to;
    let mut frames: BTreeSet<u32> = document
        .objects
        .iter()
        .filter(|record| {
            scene_authored(record)
                && record.target.layer_uid == layer
                && inside(record.target.frame_scope.frame)
        })
        .map(|record| record.target.frame_scope.frame)
        .collect();
    for scope in scene_scopes(document) {
        if inside(scope.frame_scope.frame)
            && scope
                .elements
                .iter()
                .any(|element| element.layer_uid == layer)
        {
            frames.insert(scope.frame_scope.frame);
        }
    }
    frames
}
/// True when the layer's cell at `dst` already is exactly what the cycle writes there: a copy of the records and the
/// ordered elements of `source`, or nothing when the source is a hold. Such a cell is left alone, so a cycle that writes
/// what is already there changes no byte.
fn cell_is_written(document: &ObjectDocument, layer: &str, source: Option<u32>, dst: u32) -> bool {
    let have = records_at(document, layer, dst);
    let (want, want_elements) = match source {
        Some(source) => (
            records_at(document, layer, source),
            elements_at(document, layer, source),
        ),
        None => (Vec::new(), Vec::new()),
    };
    have.len() == want.len()
        && have.iter().zip(&want).all(|(present, wanted)| {
            let mut moved = (*wanted).clone();
            moved.target.frame_scope.frame = dst;
            moved == **present
        })
        && elements_at(document, layer, dst) == want_elements
}

/// Baseline `symbolId` layers are the real scene-root native component instances.
fn is_component_layer(document: &ObjectDocument, uid: &str) -> bool {
    document
        .component_set()
        .is_some_and(|set| set.instance(CONTEXT, uid).is_some())
}

/// repeatSelection over an admitted document. A cycle that changes nothing (every copy equals its destination and the
/// timeline does not grow) is `NoChange`.
pub(crate) fn repeat_cells(
    document: &ObjectDocument,
    request: &RepeatRequest,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let first_layer =
        layer_position(document, &request.from_layer).ok_or(DispatchErrorCode::NotFound)?;
    let last_layer =
        layer_position(document, &request.to_layer).ok_or(DispatchErrorCode::NotFound)?;
    let (first, last) = (request.from_frame, request.to_frame);
    let total = document.total_frames;
    if first_layer > last_layer || first > last || last >= total {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let times = clamp_times(request.times);
    let grown = grown_total(total, first, last, times).ok_or(DispatchErrorCode::InvalidRequest)?;
    let span = last - first + 1;
    // The last destination frame: below `grown` because `grown >= last + 1 + span * times`.
    let window_end = last + span * times;

    // Every layer's timing plan is computed before anything is written, so one refusal leaves nothing behind.
    let mut plans = Vec::new();
    for index in first_layer..=last_layer {
        if is_component_layer(document, &document.layers[index].layer_uid) {
            continue;
        }
        let timing = document.layers[index]
            .timing
            .value()
            .ok_or(DispatchErrorCode::Unavailable)?;
        let plan = timing
            .repeat_range(first, last, times, total, grown)
            .map_err(timing_refusal)?;
        plans.push((index, plan));
    }

    let mut next = document.clone();
    next.total_frames = grown;
    for (index, plan) in plans {
        let uid = next.layers[index].layer_uid.clone();
        if let TimingEdit::Changed(timing) = plan.edit {
            next.layers[index].timing = Presence::Present(timing);
        }
        // A destination is replaced whole, but only where it differs: its old records (and ordered elements) go, then
        // the copy of its source cell is appended. The sources lie before every destination, so they are read unchanged.
        let mut targets = occupied(document, &uid, last + 1, window_end);
        for round in 1..=times {
            for &source in &plan.sources {
                targets.insert(source + span * round);
            }
        }
        let mut rewrite: Vec<(u32, Option<u32>)> = Vec::new();
        for &frame in &targets {
            let source = first + (frame - last - 1) % span;
            let source = plan.sources.binary_search(&source).ok().map(|_| source);
            if !cell_is_written(document, &uid, source, frame) {
                rewrite.push((frame, source));
            }
        }
        let stale: BTreeSet<u32> = rewrite.iter().map(|(frame, _)| *frame).collect();
        drop_cells(&mut next, &uid, &|frame| stale.contains(&frame));
        for (frame, source) in rewrite {
            if let Some(source) = source {
                clone_cell(&mut next, &uid, source, frame);
            }
        }
    }
    // The Motion tracks of the cycled layers (ld.motion keys cloned at k.frame + r * span, L05-J2) join the same candidate; a track
    // that cannot be admitted (a clone on an existing key) refuses the whole cycle, cells included.
    let cycled: Vec<String> = document.layers[first_layer..=last_layer]
        .iter()
        .filter(|layer| !is_component_layer(document, &layer.layer_uid))
        .map(|layer| layer.layer_uid.clone())
        .collect();
    repeat_motion_tracks(&mut next, &cycled, first, last, times)?;
    if next == *document {
        return Ok(DocumentEdit::NoChange);
    }
    finish(next)
}

/// Detached, non-Clone candidate sealed to the snapshot it was prepared from; only this file can build one.
/// `document` is `None` for a semantic no-op, which publishes nothing and keeps redo.
#[derive(Debug)]
pub(crate) struct PreparedFrameRepeat {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    cycled_layers: usize,
    skipped_component_layers: usize,
}

// Typed preparation outcome for the native caller port. The compact/UI result
// still needs its separately owned composition; applied alone means a document
// changed, including global growth when every selected layer is a component.
#[allow(dead_code)]
impl PreparedFrameRepeat {
    pub(crate) fn cycled_layers(&self) -> usize {
        self.cycled_layers
    }
    pub(crate) fn skipped_component_layers(&self) -> usize {
        self.skipped_component_layers
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_frame_repeat(
        &self,
        request: &RepeatRequest,
    ) -> Result<PreparedFrameRepeat, DispatchErrorCode> {
        let origin = self.current_snapshot();
        let document = match repeat_cells(origin.document(), request)? {
            DocumentEdit::Changed(next) => Some(next),
            DocumentEdit::NoChange => None,
        };
        // repeat_cells validated these bounds before preparing the detached result.
        let source = origin.document();
        let first =
            layer_position(source, &request.from_layer).ok_or(DispatchErrorCode::NotFound)?;
        let last = layer_position(source, &request.to_layer).ok_or(DispatchErrorCode::NotFound)?;
        let skipped_component_layers = source.layers[first..=last]
            .iter()
            .filter(|layer| is_component_layer(source, &layer.layer_uid))
            .count();
        let cycled_layers = last - first + 1 - skipped_component_layers;
        Ok(PreparedFrameRepeat {
            origin,
            document,
            cycled_layers,
            skipped_component_layers,
        })
    }

    pub(crate) fn prepare_frame_repeat_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedFrameRepeat, DispatchErrorCode> {
        self.prepare_frame_repeat(&parse_repeat_payload(bytes)?)
    }

    /// Publish a prepared cycle: instance, document, snapshot and content revision must still be the candidate's
    /// (`WrongInstance`, `WrongDocument`, `StaleRevision`), checked before anything is published. A no-op returns the
    /// current revision.
    pub(crate) fn commit_frame_repeat(
        &mut self,
        candidate: PreparedFrameRepeat,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self.revisions.publish_frame_repeat(document),
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_repeat.rs"]
mod tests;
