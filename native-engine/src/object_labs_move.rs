//! U34 (#2362) `SMLabs.moveSelectionToLayer` (`src/js/labs/move-to-layer.js` @1065e9d8): the selected strokes of the
//! active layer's cell, each with its linked-fill companion and its brush-texture dabs (the baseline moves
//! `p.data.linkedFill` and `p.data.brushCompanions` with `p`), move to the same frame of another layer with their
//! identities, appended above its drawing in the baseline order (fill, dabs, stroke). A plain target cell first becomes
//! a key holding the drawing it showed (the baseline `getEffectiveStrokes` copy; an in-between stays one). A held source
//! cell is promoted the same way before the move, so the key it showed keeps its strokes and nothing is duplicated.
use crate::commands::object_fill_owner::object_frame_clipboard_paste::{finish, layer_position};
use crate::commands::object_fill_owner::object_frame_edit_document::{promote_cells, DocumentEdit};
use crate::commands::object_fill_owner::object_labs_generate::CONTEXT;
use crate::commands::{insert_order_element, remove_order_element};
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectRecord, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

fn in_cell(record: &ObjectRecord, layer: &str, frame: u32) -> bool {
    record.target.context_id == CONTEXT
        && record.target.layer_uid == layer
        && record.target.frame_scope.kind == FrameScopeKind::Authored
        && record.target.frame_scope.frame == frame
}
fn ext_flag(record: &ObjectRecord, companion: bool) -> bool {
    let ext = record.vector_ext.value();
    if companion {
        ext.and_then(|e| e.is_linked_fill_companion.value())
            .is_some_and(|v| *v)
    } else {
        ext.and_then(|e| e.brush_texture.value())
            .and_then(|b| b.is_brush_texture_copy.value())
            .is_some_and(|v| *v)
    }
}
fn linked_fill(record: &ObjectRecord) -> Option<&String> {
    record
        .vector_ext
        .value()
        .and_then(|e| e.linked_fill_id.value())
}
fn brush_group(record: &ObjectRecord) -> Option<&String> {
    record
        .vector_ext
        .value()
        .and_then(|e| e.brush_texture.value())
        .and_then(|b| b.brush_group_id.value())
}
/// A held or unexposed cell becomes a key holding what it showed.
fn promoted(
    document: ObjectDocument,
    layer: &str,
    frame: u32,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let plain = document
        .layers
        .iter()
        .find(|l| l.layer_uid == layer)
        .and_then(|l| l.timing.value())
        .is_some_and(|timing| matches!(timing.exposure(frame).0, "held" | "unexposed"));
    if !plain {
        return Ok(document);
    }
    match promote_cells(&document, layer, &[frame]).map_err(|_| DispatchErrorCode::Unavailable)? {
        DocumentEdit::Changed(next) => Ok(next),
        DocumentEdit::NoChange => Ok(document),
    }
}

pub(super) fn move_to_layer(
    origin: &ObjectSnapshot,
    from: &str,
    to: &str,
    frame: u32,
    stroke_ids: &[String],
) -> Result<DocumentEdit, DispatchErrorCode> {
    let document = origin.document();
    layer_position(document, from).ok_or(DispatchErrorCode::NotFound)?;
    layer_position(document, to).ok_or(DispatchErrorCode::NotFound)?;
    if from == to || frame >= document.total_frames {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let component = |uid: &str| {
        document
            .component_set()
            .is_some_and(|set| set.instance(CONTEXT, uid).is_some())
    };
    if document.layer_is_locked(to) || component(to) || component(from) {
        return Err(DispatchErrorCode::Unavailable);
    }
    let mut next = promoted(promoted(document.clone(), from, frame)?, to, frame)?;
    let cell: Vec<ObjectRecord> = next
        .objects
        .iter()
        .filter(|r| in_cell(r, from, frame))
        .cloned()
        .collect();
    let mut moved: Vec<String> = Vec::new();
    for id in stroke_ids {
        let stroke = cell
            .iter()
            .find(|r| &r.target.stroke_id == id)
            .ok_or(DispatchErrorCode::NotFound)?;
        let fills = linked_fill(stroke)
            .filter(|_| !ext_flag(stroke, true))
            .map_or(Vec::new(), |link| {
                cell.iter()
                    .filter(|r| ext_flag(r, true) && linked_fill(r) == Some(link))
                    .collect()
            });
        let dabs = brush_group(stroke)
            .filter(|_| !ext_flag(stroke, false))
            .map_or(Vec::new(), |group| {
                cell.iter()
                    .filter(|r| ext_flag(r, false) && brush_group(r) == Some(group))
                    .collect()
            });
        for record in fills.into_iter().chain(dabs).chain(std::iter::once(stroke)) {
            if !moved.contains(&record.target.stroke_id) {
                moved.push(record.target.stroke_id.clone());
            }
        }
    }
    if next
        .objects
        .iter()
        .any(|r| in_cell(r, to, frame) && moved.contains(&r.target.stroke_id))
    {
        return Err(DispatchErrorCode::Unavailable);
    }
    let order_error = |_| DispatchErrorCode::Unavailable;
    for id in &moved {
        let at = next
            .objects
            .iter()
            .position(|r| in_cell(r, from, frame) && &r.target.stroke_id == id)
            .ok_or(DispatchErrorCode::Internal)?;
        let mut record = next.objects.remove(at);
        if let Presence::Present(plan) = &mut next.composition_order {
            remove_order_element(plan, &record.target).map_err(order_error)?;
        }
        record.target.layer_uid = to.to_owned();
        if let Presence::Present(plan) = &mut next.composition_order {
            insert_order_element(plan, &record.target).map_err(order_error)?;
        }
        next.objects.push(record);
    }
    finish(next)
}

#[cfg(test)]
#[path = "../tests/object_labs_move.rs"]
mod tests;
