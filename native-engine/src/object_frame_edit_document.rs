//! Document-level frame remap kernel (F09/F10): what the dense editor's insert-frame, remove-frame(s),
//! F6 / convert-to-keyframes, clear-keyframe, delete-frames and remove-tween do to the CONTENT of an
//! `ObjectDocument`, on top of the per-layer sparse timing edits (`LayerTiming`, L05-A). Behavior source:
//! src/js/app.js and src/js/timeline.js at fbe4b8a3.
//!
//! Every edit takes one admitted document and returns either a detached, fully revalidated replacement,
//! `NoChange` (history must not publish, redo must survive) or a typed refusal; the input is only borrowed, so
//! a refusal cannot leave a partial edit. What moves is exactly what the dense editor moves: the scene-root
//! authored records and, for a v2 document, the scene-root composition-order scopes, both per layer. What
//! never moves, as in the baseline: Reference-scope records, other contexts, `referencePolicy`, the property
//! holder table (the baseline never rewrites Motion tracks on insert or remove), layer fields other than the
//! timing, and every stroke id, natural position and authored order (a copy keeps the originals). Locked/shy
//! policy, markers, in/out points, history and requests belong to callers and later leaves.
use crate::object_document::{
    FrameScope, FrameScopeKind, LayerTiming, LayerTimingError, ObjectDocument, ObjectRecord,
    OrderContext, OrderElement, OrderScope, Presence, TimingEdit,
};
use crate::revision::ObjectSnapshot;

const CONTEXT: &str = "scene-root";

/// A published replacement or nothing to publish.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DocumentEdit {
    Changed(ObjectDocument),
    NoChange,
}

/// Typed refusals; none of them can follow a partial edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameEditError {
    /// Malformed request (cell out of range, empty targets, entry cap) or a result that fails storage admission.
    Invalid,
    /// A layer timing of another format, family or context.
    Unsupported,
    /// A named layer is not in the document.
    UnknownLayer,
    /// An edited layer has no timing record, so its exposure is undefined and nothing is invented.
    MissingTiming,
    /// A record this kernel must leave alone (Reference scope, another context) would fall outside the shrunk range.
    PreservedOutOfRange,
}

impl From<LayerTimingError> for FrameEditError {
    fn from(error: LayerTimingError) -> Self {
        match error {
            LayerTimingError::Invalid => Self::Invalid,
            LayerTimingError::Unsupported => Self::Unsupported,
        }
    }
}

fn scene_authored(record: &ObjectRecord) -> bool {
    record.target.context_id == CONTEXT
        && record.target.frame_scope.kind == FrameScopeKind::Authored
}
fn is_target(targets: &[&str], uid: &str) -> bool {
    targets.iter().any(|target| *target == uid)
}
fn layer_index(document: &ObjectDocument, uid: &str) -> Result<usize, FrameEditError> {
    let found = document
        .layers
        .iter()
        .position(|layer| layer.layer_uid == uid);
    found.ok_or(FrameEditError::UnknownLayer)
}
fn timing_of(document: &ObjectDocument, index: usize) -> Result<&LayerTiming, FrameEditError> {
    document.layers[index]
        .timing
        .value()
        .ok_or(FrameEditError::MissingTiming)
}
fn scene_contexts(document: &mut ObjectDocument) -> impl Iterator<Item = &mut OrderContext> {
    let plan = match &mut document.composition_order {
        Presence::Present(plan) => Some(plan),
        Presence::Missing => None,
    };
    plan.into_iter()
        .flat_map(|plan| plan.contexts.iter_mut())
        .filter(|context| context.context_id == CONTEXT)
}

/// The one exit for a changed document: the shared strict drawing admission (records, timing, order
/// coverage, property holders) runs on the complete result.
fn finish(document: ObjectDocument) -> Result<DocumentEdit, FrameEditError> {
    ObjectSnapshot::validate_drawing(&document).map_err(|_| FrameEditError::Invalid)?;
    Ok(DocumentEdit::Changed(document))
}

/// The scope of an authored frame, created empty when absent. Sorted placement is the caller's job.
fn scope_at(scopes: &mut Vec<OrderScope>, frame: u32) -> &mut OrderScope {
    let at = match scopes
        .iter()
        .position(|scope| scope.frame_scope.frame == frame)
    {
        Some(at) => at,
        None => {
            scopes.push(OrderScope {
                frame_scope: FrameScope {
                    kind: FrameScopeKind::Authored,
                    frame,
                },
                elements: Vec::new(),
            });
            scopes.len() - 1
        }
    };
    &mut scopes[at]
}
fn ascending(scopes: &[OrderScope]) -> bool {
    scopes
        .windows(2)
        .all(|pair| pair[0].frame_scope.frame < pair[1].frame_scope.frame)
}

/// Rebuild a scene-root scope list from a per-layer frame map. A scope is split when its layers land on
/// different frames and merged when two land on one; element order inside a destination follows the
/// source scope order. An element-less scope follows `empty`. A scope list that was ascending stays so.
fn remap_scopes(
    context: &mut OrderContext,
    map: &dyn Fn(&str, u32) -> Option<u32>,
    empty: &dyn Fn(u32) -> Option<u32>,
) -> Result<(), FrameEditError> {
    if context
        .scopes
        .iter()
        .any(|scope| scope.frame_scope.kind != FrameScopeKind::Authored)
    {
        return Err(FrameEditError::Unsupported);
    }
    let sorted = ascending(&context.scopes);
    let mut next: Vec<OrderScope> = Vec::new();
    for scope in std::mem::take(&mut context.scopes) {
        let frame = scope.frame_scope.frame;
        if scope.elements.is_empty() {
            if let Some(to) = empty(frame) {
                scope_at(&mut next, to);
            }
            continue;
        }
        for element in scope.elements {
            if let Some(to) = map(element.layer_uid.as_str(), frame) {
                scope_at(&mut next, to).elements.push(element);
            }
        }
    }
    if sorted {
        next.sort_by_key(|scope| scope.frame_scope.frame);
    }
    context.scopes = next;
    Ok(())
}

/// Apply a per-layer frame map (`None` drops) to the scene-root authored records and order scopes.
fn remap(
    document: &mut ObjectDocument,
    map: &dyn Fn(&str, u32) -> Option<u32>,
    empty: &dyn Fn(u32) -> Option<u32>,
) -> Result<(), FrameEditError> {
    let records = std::mem::take(&mut document.objects);
    document.objects = records
        .into_iter()
        .filter_map(|mut record| {
            if scene_authored(&record) {
                let to = map(
                    record.target.layer_uid.as_str(),
                    record.target.frame_scope.frame,
                )?;
                record.target.frame_scope.frame = to;
            }
            Some(record)
        })
        .collect();
    for context in scene_contexts(document) {
        remap_scopes(context, map, empty)?;
    }
    Ok(())
}

/// Remove one layer's authored records and order elements at the matching frames; emptied scopes stay.
fn drop_cells(document: &mut ObjectDocument, layer: &str, matches: &dyn Fn(u32) -> bool) {
    document.objects.retain(|record| {
        !(scene_authored(record)
            && record.target.layer_uid == layer
            && matches(record.target.frame_scope.frame))
    });
    for context in scene_contexts(document) {
        for scope in context.scopes.iter_mut() {
            if matches(scope.frame_scope.frame) {
                scope.elements.retain(|element| element.layer_uid != layer);
            }
        }
    }
}

/// Copy one layer's authored cell (records appended in order, order elements) to another frame. Stroke ids,
/// geometry, fill, natural positions and authored order are copied as they are.
fn clone_cell(document: &mut ObjectDocument, layer: &str, from: u32, to: u32) {
    let copies: Vec<ObjectRecord> = document
        .objects
        .iter()
        .filter(|record| {
            scene_authored(record)
                && record.target.layer_uid == layer
                && record.target.frame_scope.frame == from
        })
        .cloned()
        .map(|mut record| {
            record.target.frame_scope.frame = to;
            record
        })
        .collect();
    document.objects.extend(copies);
    for context in scene_contexts(document) {
        let elements: Vec<OrderElement> = context
            .scopes
            .iter()
            .filter(|scope| scope.frame_scope.frame == from)
            .flat_map(|scope| {
                scope
                    .elements
                    .iter()
                    .filter(move |element| element.layer_uid == layer)
                    .cloned()
            })
            .collect();
        if elements.is_empty() {
            continue;
        }
        let sorted = ascending(&context.scopes);
        scope_at(&mut context.scopes, to).elements.extend(elements);
        if sorted {
            context.scopes.sort_by_key(|scope| scope.frame_scope.frame);
        }
    }
}

/// F5 (insertFrame): a blank slot at `cf + 1` on every targeted layer, a length pad at the end on every other
/// layer. Targeted layers move every record after `cf` one frame later; the new cell holds. The caller has
/// already applied the lock policy to `targets`; an empty target list is refused (the baseline then targets
/// every layer, which is the caller's decision to make explicit).
pub(crate) fn insert_frame(
    document: &ObjectDocument,
    cf: u32,
    targets: &[&str],
) -> Result<DocumentEdit, FrameEditError> {
    let total = document.total_frames;
    let grown = total.checked_add(1).ok_or(FrameEditError::Invalid)?;
    if cf >= total || targets.is_empty() {
        return Err(FrameEditError::Invalid);
    }
    for uid in targets {
        layer_index(document, uid)?;
    }
    let mut next = document.clone();
    for index in 0..next.layers.len() {
        let targeted = is_target(targets, &next.layers[index].layer_uid);
        let edit = match next.layers[index].timing.value() {
            Some(timing) if targeted => timing.insert_gap(cf, total)?,
            Some(timing) => timing.insert_pad(total)?,
            None if targeted => return Err(FrameEditError::MissingTiming),
            None => continue,
        };
        if let TimingEdit::Changed(timing) = edit {
            next.layers[index].timing = Presence::Present(timing);
        }
    }
    next.total_frames = grown;
    let uniform = next
        .layers
        .iter()
        .all(|layer| is_target(targets, &layer.layer_uid));
    remap(
        &mut next,
        &|layer, frame| {
            Some(if frame > cf && is_target(targets, layer) {
                frame + 1
            } else {
                frame
            })
        },
        &|frame| {
            Some(if uniform && frame > cf {
                frame + 1
            } else {
                frame
            })
        },
    )?;
    finish(next)
}

/// Remove frames `min..=max` on every layer (removeFrame / removeFrameSpan). Records inside vanish, later
/// ones move down by the span length; removing every frame is `NoChange`. Reference-scope and other-context
/// records stay where they are, so one that would fall outside the shrunk range refuses the edit.
pub(crate) fn remove_frames(
    document: &ObjectDocument,
    min: u32,
    max: u32,
) -> Result<DocumentEdit, FrameEditError> {
    let total = document.total_frames;
    if min > max || max >= total {
        return Err(FrameEditError::Invalid);
    }
    let count = max - min + 1;
    if count >= total {
        return Ok(DocumentEdit::NoChange);
    }
    let shrunk = total - count;
    let record_out = document
        .objects
        .iter()
        .any(|record| !scene_authored(record) && record.target.frame_scope.frame >= shrunk);
    let scope_out = match &document.composition_order {
        Presence::Present(plan) => plan
            .contexts
            .iter()
            .filter(|context| context.context_id != CONTEXT)
            .flat_map(|context| context.scopes.iter())
            .any(|scope| scope.frame_scope.frame >= shrunk),
        Presence::Missing => false,
    };
    if record_out || scope_out {
        return Err(FrameEditError::PreservedOutOfRange);
    }
    let mut next = document.clone();
    for index in 0..next.layers.len() {
        if let TimingEdit::Changed(timing) =
            timing_of(&next, index)?.remove_span(min, max, total)?
        {
            next.layers[index].timing = Presence::Present(timing);
        }
    }
    next.total_frames = shrunk;
    let shift = |frame: u32| {
        if frame < min {
            Some(frame)
        } else if frame <= max {
            None
        } else {
            Some(frame - count)
        }
    };
    remap(&mut next, &|_, frame| shift(frame), &shift)?;
    finish(next)
}

/// F6 / convert-to-keyframes on one layer (insertKeyframeAt, convertToKeyframes): each listed cell, in the
/// given order, becomes a key whose content is what the cell shows. A held cell copies the records of the key
/// it shows (a blank key copies nothing, a cell before every key copies nothing); an in-between keeps its own
/// records; a cell that already is a key is left alone. Cells are applied one after another, so a hold promoted
/// after a nearer in-between shows that in-between's content, as the baseline does. Anything already stored at
/// a promoted held cell is replaced by the copy, so targets stay unique.
pub(crate) fn promote_cells(
    document: &ObjectDocument,
    layer_uid: &str,
    cells: &[u32],
) -> Result<DocumentEdit, FrameEditError> {
    let index = layer_index(document, layer_uid)?;
    let total = document.total_frames;
    timing_of(document, index)?;
    if cells.iter().any(|cell| *cell >= total) {
        return Err(FrameEditError::Invalid);
    }
    let mut next = document.clone();
    let mut changed = false;
    for &cell in cells {
        let timing = timing_of(&next, index)?;
        let (kind, source, _) = timing.exposure(cell);
        let TimingEdit::Changed(promoted) = timing.promote_cell(cell, total)? else {
            continue;
        };
        next.layers[index].timing = Presence::Present(promoted);
        changed = true;
        if kind != "interpolated" {
            drop_cells(&mut next, layer_uid, &|frame| frame == cell);
            if let Some(from) = source {
                clone_cell(&mut next, layer_uid, from, cell);
            }
        }
    }
    if !changed {
        return Ok(DocumentEdit::NoChange);
    }
    finish(next)
}

/// Clear Keyframe: a key becomes a hold of the previous key and its records go; any other cell is `NoChange`.
pub(crate) fn clear_key(
    document: &ObjectDocument,
    layer_uid: &str,
    cell: u32,
) -> Result<DocumentEdit, FrameEditError> {
    let index = layer_index(document, layer_uid)?;
    let edit = timing_of(document, index)?.clear_key(cell, document.total_frames)?;
    let TimingEdit::Changed(timing) = edit else {
        return Ok(DocumentEdit::NoChange);
    };
    let mut next = document.clone();
    next.layers[index].timing = Presence::Present(timing);
    drop_cells(&mut next, layer_uid, &|frame| frame == cell);
    finish(next)
}

/// Delete Frames (deleteSelectedFrames): each listed cell, key or in-between, becomes a plain hold and its
/// records go; when every listed cell already was a hold the edit is `NoChange`.
pub(crate) fn clear_cells(
    document: &ObjectDocument,
    layer_uid: &str,
    cells: &[u32],
) -> Result<DocumentEdit, FrameEditError> {
    let index = layer_index(document, layer_uid)?;
    let edit = timing_of(document, index)?.clear_cells(cells, document.total_frames)?;
    let TimingEdit::Changed(timing) = edit else {
        return Ok(DocumentEdit::NoChange);
    };
    let mut next = document.clone();
    next.layers[index].timing = Presence::Present(timing);
    drop_cells(&mut next, layer_uid, &|frame| cells.contains(&frame));
    finish(next)
}

/// Remove Tween (removeTweenSpan): everything strictly between `from` and `to` becomes a hold of the start
/// key and its records go; both endpoints stay, and so does the frame count.
pub(crate) fn demote_span(
    document: &ObjectDocument,
    layer_uid: &str,
    from: u32,
    to: u32,
) -> Result<DocumentEdit, FrameEditError> {
    let index = layer_index(document, layer_uid)?;
    let edit = timing_of(document, index)?.demote_span(from, to, document.total_frames)?;
    let TimingEdit::Changed(timing) = edit else {
        return Ok(DocumentEdit::NoChange);
    };
    let mut next = document.clone();
    next.layers[index].timing = Presence::Present(timing);
    drop_cells(&mut next, layer_uid, &|frame| frame > from && frame < to);
    finish(next)
}

#[cfg(test)]
#[path = "../tests/object_frame_edit_document_build.rs"]
mod build;
#[cfg(test)]
#[path = "../tests/object_frame_edit_document_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_frame_edit_document_literals.rs"]
mod literals;
#[cfg(test)]
#[path = "../tests/object_frame_edit_document_reach.rs"]
mod reach;
#[cfg(test)]
#[path = "../tests/object_frame_edit_document.rs"]
mod tests;
