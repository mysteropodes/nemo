//! #2256 (review of the entered-component writes): the COPY KEY of a held cell in any declared context.
//!
//! The scene's copy key (`promote_cells`, the baseline `ensureKeyframe()`) keys a held or unexposed cell with a copy of what it
//! showed, so a Select move / restyle / delete there edits that cell's copy and never the key every held frame carries. That kernel is
//! bound to the scene root (its record, order and item filters name `scene-root`). A component is edited through a VIEW of the document
//! (`object_vector_edit_context`): its layers, records and frame count are the component's, and its order scopes live in the plan under
//! the component's id. This module runs the same promotion over that view: the timing key is the scene's own (`promote_cell`), and the
//! cell's records and order elements are re-homed with the same rules (a cell that is a key is left alone, anything stored at a promoted
//! held cell is replaced by the copy, an in-between is promoted without a copy). Components hold no raster or text items, so there is
//! nothing else to copy. The whole result is admitted by the one storage validator when the edit is restored into the document.
use crate::commands::object_fill_owner::object_frame_edit_document::{promote_cells, DocumentEdit, FrameEditError};
use crate::object_document::{FrameScope, FrameScopeKind, ObjectDocument, ObjectRecord, OrderElement, OrderScope, Presence, TimingEdit};

const SCENE: &str = "scene-root";

fn in_cell(record: &ObjectRecord, context: &str, layer: &str, frame: u32) -> bool {
    record.target.context_id == context
        && record.target.frame_scope.kind == FrameScopeKind::Authored
        && record.target.layer_uid == layer
        && record.target.frame_scope.frame == frame
}

/// Re-home the order elements of `layer` from frame `from` to frame `to` of the plan context `context` (and drop what `to` held).
fn move_elements(document: &mut ObjectDocument, context: &str, layer: &str, from: Option<u32>, to: u32) {
    let Presence::Present(plan) = &mut document.composition_order else { return };
    for ctx in plan.contexts.iter_mut().filter(|c| c.context_id == context) {
        for scope in ctx.scopes.iter_mut().filter(|s| s.frame_scope.frame == to) {
            scope.elements.retain(|element| element.layer_uid != layer);
        }
        let Some(from) = from else { continue };
        let copies: Vec<OrderElement> = ctx
            .scopes
            .iter()
            .filter(|s| s.frame_scope.frame == from)
            .flat_map(|s| s.elements.iter().filter(|e| e.layer_uid == layer).cloned())
            .collect();
        if copies.is_empty() {
            continue;
        }
        let sorted = ctx.scopes.windows(2).all(|pair| pair[0].frame_scope.frame < pair[1].frame_scope.frame);
        match ctx.scopes.iter().position(|s| s.frame_scope.frame == to) {
            Some(at) => ctx.scopes[at].elements.extend(copies),
            None => ctx.scopes.push(OrderScope { frame_scope: FrameScope { kind: FrameScopeKind::Authored, frame: to }, elements: copies }),
        }
        if sorted {
            ctx.scopes.sort_by_key(|s| s.frame_scope.frame);
        }
    }
}

/// Key every listed cell of `layer_uid` with a copy of what it showed, in `context` (`document` is that context's view).
pub(super) fn promote(document: &ObjectDocument, context: &str, layer_uid: &str, cells: &[u32]) -> Result<DocumentEdit, FrameEditError> {
    if context == SCENE {
        return promote_cells(document, layer_uid, cells);
    }
    let index = document.layers.iter().position(|layer| layer.layer_uid == layer_uid).ok_or(FrameEditError::UnknownLayer)?;
    let total = document.total_frames;
    if cells.iter().any(|cell| *cell >= total) {
        return Err(FrameEditError::Invalid);
    }
    let mut next = document.clone();
    let mut changed = false;
    for &cell in cells {
        let timing = next.layers[index].timing.value().ok_or(FrameEditError::MissingTiming)?;
        let (kind, source, _) = timing.exposure(cell);
        let TimingEdit::Changed(promoted) = timing.promote_cell(cell, total)? else { continue };
        next.layers[index].timing = Presence::Present(promoted);
        changed = true;
        if kind == "interpolated" {
            continue;
        }
        next.objects.retain(|record| !in_cell(record, context, layer_uid, cell));
        if let Some(from) = source {
            let copies: Vec<ObjectRecord> = next
                .objects
                .iter()
                .filter(|record| in_cell(record, context, layer_uid, from))
                .cloned()
                .map(|mut record| {
                    record.target.frame_scope.frame = cell;
                    record
                })
                .collect();
            next.objects.extend(copies);
        }
        move_elements(&mut next, context, layer_uid, source, cell);
    }
    Ok(if changed { DocumentEdit::Changed(next) } else { DocumentEdit::NoChange })
}
