//! Frame reads and detached replacement through the existing authored records and order helper.
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectRecord, Presence};
use crate::request_receipts::DispatchErrorCode;
use crate::tween_kernels::*;
pub(super) fn in_cell(r: &ObjectRecord, layer: &str, frame: u32) -> bool {
    r.target.context_id == "scene-root"
        && r.target.frame_scope.kind == FrameScopeKind::Authored
        && r.target.layer_uid == layer
        && r.target.frame_scope.frame == frame
}
pub(super) fn frame_records(doc: &ObjectDocument, layer: &str, frame: u32) -> Vec<ObjectRecord> {
    let mut records: Vec<_> = doc
        .objects
        .iter()
        .filter(|r| in_cell(r, layer, frame))
        .cloned()
        .collect();
    if let Some(plan) = doc.composition_order.value() {
        if let Some(scope) = plan
            .contexts
            .iter()
            .find(|c| c.context_id == "scene-root")
            .and_then(|c| {
                c.scopes.iter().find(|s| {
                    s.frame_scope.kind == FrameScopeKind::Authored && s.frame_scope.frame == frame
                })
            })
        {
            records.sort_by_key(|r| {
                scope
                    .elements
                    .iter()
                    .find(|e| e.layer_uid == layer && e.stroke_id == r.target.stroke_id)
                    .map(|e| e.natural_position)
                    .unwrap_or(u32::MAX)
            });
        }
    }
    records
}
pub(super) fn replace_frame(
    doc: &mut ObjectDocument,
    layer: &str,
    frame: u32,
    records: Vec<ObjectRecord>,
) -> R<()> {
    replace_mixed(
        doc,
        layer,
        frame,
        records.into_iter().map(Held::Object).collect(),
    )
}
/// Id stamping updates scoped order identities atomically with records, keeping authored order bytes.
pub(super) fn stamp_frame(
    doc: &mut ObjectDocument,
    layer: &str,
    frame: u32,
    before: &[ObjectRecord],
    after: &[ObjectRecord],
) -> R<()> {
    if before.len() != after.len() {
        return Err(DispatchErrorCode::Internal);
    }
    let changes: Vec<_> = before
        .iter()
        .zip(after)
        .map(|(a, b)| (&a.target.stroke_id, &b.target.stroke_id))
        .collect();
    for r in doc.objects.iter_mut().filter(|r| in_cell(r, layer, frame)) {
        if let Some((_, id)) = changes.iter().find(|(old, _)| **old == r.target.stroke_id) {
            r.target.stroke_id = (*id).clone();
        }
    }
    if let Presence::Present(plan) = &mut doc.composition_order {
        for e in plan
            .contexts
            .iter_mut()
            .filter(|c| c.context_id == "scene-root")
            .flat_map(|c| &mut c.scopes)
            .filter(|s| {
                s.frame_scope.kind == FrameScopeKind::Authored && s.frame_scope.frame == frame
            })
            .flat_map(|s| &mut s.elements)
            .filter(|e| e.layer_uid == layer)
        {
            if let Some((_, id)) = changes.iter().find(|(old, _)| **old == e.stroke_id) {
                e.stroke_id = (*id).clone();
            }
        }
    }
    Ok(())
}
