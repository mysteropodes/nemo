//! `object.vector.batch` (#2361, #2362): an ordered list of creates, replaces and deletes on ONE scene-root layer, applied
//! through the single-command kernels to the document as it grows, as one candidate, one revision and one undo entry. It
//! carries the Labs edits that the baseline makes inside one undo step: the knife's cut pieces, sculpt's rewritten
//! strokes, pose stamps and multiframe copies. Ids are allocated from the origin revision, as single creates are, and
//! fill traces regenerate after each step exactly as after the single command. A refused step refuses the batch.
use crate::commands::object_fill_owner::vector_edit::create_ops::{create_in, Placement};
use crate::commands::object_fill_owner::vector_edit::{delete_in, replace_in, Edit, VectorEditKind};
use crate::commands::object_fill_owner::object_frame_edit_document::{promote_cells, DocumentEdit};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::object_group_kernels::object_fill_trace_ops::regenerate::regenerate;
use crate::object_document::{object_deserialize, ObjectRecord, ObjectTarget, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

/// One batch is a bounded gesture, never a document rewrite; the encoded request stays under the vector budget.
pub(crate) const MAX_BATCH_OPS: usize = 64;

#[derive(Debug)]
pub(super) struct BatchOp {
    op: String,
    record: Presence<ObjectRecord>,
    stable_target: Presence<ObjectTarget>,
    placement: Presence<String>,
}
object_deserialize!(BatchOp {
    op: String,
    #[serde(default)] record: Presence<ObjectRecord>,
    #[serde(default)] stable_target: Presence<ObjectTarget>,
    #[serde(default)] placement: Presence<String>
});

fn target_of(op: &BatchOp) -> Option<ObjectTarget> {
    op.record.value().map(|r| r.target.clone()).or_else(|| op.stable_target.value().cloned())
}

pub(super) fn batch(origin: &ObjectSnapshot, ops: Vec<BatchOp>, copy_key: bool) -> Result<Edit, DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    if !(1..=MAX_BATCH_OPS).contains(&ops.len()) {
        return Err(InvalidRequest);
    }
    let first = target_of(&ops[0]).ok_or(InvalidRequest)?;
    if first.context_id != "scene-root" {
        return Err(InvalidRequest);
    }
    let revision = origin.content_revision();
    let mut document = origin.document().clone();
    // With the copy key, every held or unexposed cell a create targets is keyed first with what it showed before the
    // batch (the baseline freezes all its targets before stamping any): a later step never copies an earlier one.
    if copy_key {
        let timing = document.layers.iter().find(|l| l.layer_uid == first.layer_uid).and_then(|l| l.timing.value());
        let mut plain: Vec<u32> = Vec::new();
        for op in ops.iter().filter(|op| op.op == "create") {
            let frame = target_of(op).ok_or(InvalidRequest)?.frame_scope.frame;
            let shown = timing.is_some_and(|t| matches!(t.exposure(frame).0, "held" | "unexposed"));
            if shown && frame < document.total_frames && !plain.contains(&frame) {
                plain.push(frame);
            }
        }
        if !plain.is_empty() {
            if let DocumentEdit::Changed(next) =
                promote_cells(&document, &first.layer_uid, &plain).map_err(|_| InvalidRequest)?
            {
                document = next;
            }
        }
    }
    let (mut stroke_ids, mut changed) = (Vec::new(), false);
    for op in ops {
        let target = target_of(&op).ok_or(InvalidRequest)?;
        if target.context_id != first.context_id || target.layer_uid != first.layer_uid {
            return Err(InvalidRequest);
        }
        let edit = match (op.op.as_str(), op.record, op.stable_target) {
            ("create", Presence::Present(record), Presence::Missing) => {
                let placement = Placement::parse("object.vector.create", &op.placement)?;
                create_in(&document, revision, vec![record], placement, None, copy_key, VectorEditKind::Create)?
            }
            ("replace", Presence::Present(record), Presence::Missing) if op.placement.missing() => {
                replace_in(&document, record)?
            }
            ("delete", Presence::Missing, Presence::Present(target)) if op.placement.missing() => {
                delete_in(&document, target)?
            }
            _ => return Err(InvalidRequest),
        };
        let mut next = edit.document;
        if edit.changed {
            let scope = Scope {
                context_id: target.context_id.clone(),
                layer_uid: target.layer_uid.clone(),
                frame: target.frame_scope.frame,
            };
            for stroke_id in &edit.stroke_ids {
                next = regenerate(next, &scope, stroke_id, revision).map_err(|error| ObjectFillOwner::fill_plan_error(&error))?;
            }
        }
        document = next;
        changed |= edit.changed;
        stroke_ids.extend(edit.stroke_ids);
    }
    Ok(Edit {
        document,
        kind: VectorEditKind::Batch,
        stroke_id: stroke_ids.first().cloned().unwrap_or_default(),
        stroke_ids,
        changed,
    })
}
