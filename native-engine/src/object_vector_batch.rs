//! `object.vector.batch` (#2361, #2362): an ordered list of creates, replaces and deletes on ONE scene-root layer, applied
//! through the single-command kernels to the document as it grows, as one candidate, one revision and one undo entry. It
//! carries the Labs edits that the baseline makes inside one undo step: the knife's cut pieces, sculpt's rewritten
//! strokes, pose stamps and multiframe copies. Ids are allocated from the origin revision, as single creates are, and
//! fill traces regenerate after each step exactly as after the single command. A refused step refuses the batch. A `copy`
//! step creates an earlier create's record again on another frame, so a multiframe stamp does not resend the stroke.
use crate::commands::object_fill_owner::vector_edit::create_ops::{copy_key as keyframe_copy, create_in, trim_records, Placement};
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
    keyframe: Presence<String>,
    from_step: Presence<serde_json::Number>,
    frame: Presence<serde_json::Number>,
}
object_deserialize!(BatchOp {
    op: String,
    #[serde(default)] record: Presence<ObjectRecord>,
    #[serde(default)] stable_target: Presence<ObjectTarget>,
    #[serde(default)] placement: Presence<String>,
    #[serde(default)] keyframe: Presence<String>,
    #[serde(default)] from_step: Presence<serde_json::Number>,
    #[serde(default)] frame: Presence<serde_json::Number>
});

fn target_of(op: &BatchOp) -> Option<ObjectTarget> {
    op.record.value().map(|r| r.target.clone()).or_else(|| op.stable_target.value().cloned())
}

pub(super) fn batch(origin: &ObjectSnapshot, mut ops: Vec<BatchOp>, copy_key: bool, trim: Option<f64>) -> Result<Edit, DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    if !(1..=MAX_BATCH_OPS).contains(&ops.len()) {
        return Err(InvalidRequest);
    }
    // `copy` {fromStep, frame}: the record of an EARLIER create (or copy) step, retargeted to `frame` and then a create in
    // every respect; a stroke and its companion stamped on n frames cost one record each, not n.
    for at in 0..ops.len() {
        let copy = ops[at].op == "copy";
        if copy != !ops[at].from_step.missing() || copy != !ops[at].frame.missing() {
            return Err(InvalidRequest);
        }
        if !copy {
            continue;
        }
        let from = ops[at].from_step.value().and_then(serde_json::Number::as_u64).and_then(|n| usize::try_from(n).ok());
        let frame = ops[at].frame.value().and_then(serde_json::Number::as_u64).and_then(|n| u32::try_from(n).ok());
        let (Some(from), Some(frame)) = (from.filter(|&n| n < at), frame) else {
            return Err(InvalidRequest);
        };
        if !ops[at].record.missing() || !ops[at].stable_target.missing() || ops[from].op != "create" {
            return Err(InvalidRequest);
        }
        let mut record = ops[from].record.value().cloned().ok_or(InvalidRequest)?;
        record.target.frame_scope.frame = frame;
        let op = &mut ops[at];
        (op.op, op.record, op.from_step, op.frame) = ("create".into(), Presence::Present(record), Presence::Missing, Presence::Missing);
    }
    let first = target_of(&ops[0]).ok_or(InvalidRequest)?;
    if first.context_id != "scene-root" {
        return Err(InvalidRequest);
    }
    let revision = origin.content_revision();
    let mut document = origin.document().clone();
    // `trimEnds` (all-create batches only): the first create is cut against its own cell before any step lands, and every
    // other create takes the same cut, as the baseline stamps and mirrors the already trimmed path.
    if let Some(max) = trim {
        if ops.iter().any(|op| op.op != "create") {
            return Err(InvalidRequest);
        }
        let mut records: Vec<ObjectRecord> = ops.iter().map(|op| op.record.value().cloned().ok_or(InvalidRequest)).collect::<Result<_, _>>()?;
        trim_records(&document, &mut records, max)?;
        for (op, record) in ops.iter_mut().zip(records) {
            op.record = Presence::Present(record);
        }
    }
    // A create's own `keyframe: "copy"` keys its cell with a copy even when the batch's does not: multiframe draw keys
    // the drawn frame as Draw does (an empty key) and every stamped frame with what it showed.
    let mut keyed = Vec::with_capacity(ops.len());
    for op in &ops {
        let own = keyframe_copy("object.vector.batch", &op.keyframe)?;
        if own && op.op != "create" {
            return Err(InvalidRequest);
        }
        keyed.push(copy_key || own);
    }
    // With the copy key, every held or unexposed cell a step targets is keyed first with what it showed before the
    // batch (the baseline freezes all its targets before stamping any): a later step never copies an earlier one, and a
    // replace or delete on a held cell edits that cell's copy (same stroke ids), never the key it held.
    if keyed.contains(&true) {
        let timing = document.layers.iter().find(|l| l.layer_uid == first.layer_uid).and_then(|l| l.timing.value());
        let mut plain: Vec<u32> = Vec::new();
        for op in ops.iter().zip(&keyed).filter(|(_, keyed)| **keyed).map(|(op, _)| op) {
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
    for (op, copy) in ops.into_iter().zip(keyed) {
        let target = target_of(&op).ok_or(InvalidRequest)?;
        if target.context_id != first.context_id || target.layer_uid != first.layer_uid {
            return Err(InvalidRequest);
        }
        let edit = match (op.op.as_str(), op.record, op.stable_target) {
            ("create", Presence::Present(record), Presence::Missing) => {
                let placement = Placement::parse("object.vector.create", &op.placement)?;
                create_in(&document, revision, vec![record], placement, None, copy, VectorEditKind::Create)?
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
