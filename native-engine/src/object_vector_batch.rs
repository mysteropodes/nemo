//! `object.vector.batch` (#2361, #2362): an ordered list of creates, replaces and deletes on ONE scene-root layer, applied
//! through the single-command kernels to the document as it grows, as one candidate, one revision and one undo entry. It
//! carries the Labs edits that the baseline makes inside one undo step: the knife's cut pieces, sculpt's rewritten
//! strokes, pose stamps and multiframe copies. Ids are allocated from the origin revision, as single creates are, and
//! fill traces regenerate after each step exactly as after the single command. A refused step refuses the batch. A `copy`
//! step creates an earlier create's record again on another frame, so a multiframe stamp does not resend the stroke. A
//! `reshape` gives an existing plain stroke new geometry and a `clone` creates a new stroke from one with new geometry:
//! style stays with the native record, so a client that only reads presented geometry (the Labs knife) can cut. A
//! `reshape` with a `center` instead rebuilds a pressure ribbon on a new centreline (sculpt) and re-syncs its linked fill.
use crate::commands::object_fill_owner::vector_edit::create_ops::{copy_key as keyframe_copy, create_in, trim_records, Placement};
use crate::commands::object_fill_owner::vector_edit::{delete_in, is_vector_record, replace_in, Edit, VectorEditKind};
use crate::commands::object_fill_owner::object_frame_edit_document::{promote_cells, DocumentEdit};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::object_group_kernels::object_fill_trace_ops::regenerate::regenerate;
use crate::object_document::{object_deserialize, CenterSegment, ObjectDocument, ObjectGeometry, ObjectRecord, ObjectTarget, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
#[path = "object_vector_batch_ribbon.rs"]
mod ribbon;

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
    geometry: Presence<ObjectGeometry>,
    offset: Presence<[f64; 2]>,
    center: Presence<Vec<CenterSegment>>,
}
object_deserialize!(BatchOp {
    op: String,
    #[serde(default)] record: Presence<ObjectRecord>,
    #[serde(default)] stable_target: Presence<ObjectTarget>,
    #[serde(default)] placement: Presence<String>,
    #[serde(default)] keyframe: Presence<String>,
    #[serde(default)] from_step: Presence<serde_json::Number>,
    #[serde(default)] frame: Presence<serde_json::Number>,
    #[serde(default)] geometry: Presence<ObjectGeometry>,
    #[serde(default)] offset: Presence<[f64; 2]>,
    #[serde(default)] center: Presence<Vec<CenterSegment>>
});

/// A geometry-less `clone` (#2362 pose stamp): any vector record copied as stored, its points shifted by `offset` (the
/// baseline stamp translated the item, never its stored centreline), its texture group dropped as path bake's copies
/// are, and its linked-fill identity renamed per batch so a stamped ribbon and its fill stay one pair beside the source.
fn shifted_copy(source: &ObjectDocument, target: &ObjectTarget, offset: [f64; 2], links: &mut Vec<(String, String)>, revision: u64)
    -> Result<ObjectRecord, DispatchErrorCode> {
    let existing = source.objects.iter().find(|r| &r.target == target).ok_or(DispatchErrorCode::NotFound)?;
    if !is_vector_record(existing) || !offset.iter().all(|v| v.is_finite()) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let mut record = existing.clone();
    if offset != [0.0, 0.0] {
        for segment in &mut record.geometry.segments {
            for (n, d) in [(&mut segment.point.x, offset[0]), (&mut segment.point.y, offset[1])] {
                *n = n.as_f64().and_then(|v| serde_json::Number::from_f64(v + d)).ok_or(DispatchErrorCode::InvalidRequest)?;
            }
        }
    }
    if let Presence::Present(ext) = &mut record.vector_ext {
        if let Presence::Present(texture) = &mut ext.brush_texture {
            texture.brush_group_id = Presence::Missing;
        }
        if let Presence::Present(link) = &mut ext.linked_fill_id {
            let fresh = match links.iter().find(|(old, _)| old == link) {
                Some((_, fresh)) => fresh.clone(),
                None => {
                    let fresh = format!("native-fill:{revision}:{}", links.len());
                    links.push((link.clone(), fresh.clone()));
                    fresh
                }
            };
            *link = fresh;
        }
    }
    Ok(record)
}

/// The plain vector stroke at `target` with `geometry` in its own spaces; a stroke whose outline is derived (a pressure
/// ribbon from its centreline, a shape from its parameters, a texture from its anchor) is never reshaped here.
fn with_geometry(source: &ObjectDocument, target: &ObjectTarget, geometry: ObjectGeometry) -> Result<ObjectRecord, DispatchErrorCode> {
    let existing = source.objects.iter().find(|r| &r.target == target).ok_or(DispatchErrorCode::NotFound)?;
    // Ribbons, linked fills, fill/parametric shapes and textured or bitmap strokes derive their outline elsewhere.
    let ribbon = existing.vector_ext.value().is_some_and(|ext| {
        !ext.pressure.missing() || !ext.is_linked_fill_companion.missing() || ext.is_fill_shape.value() == Some(&true)
            || !ext.param_shape.missing() || !ext.bitmap_brush.missing() || !ext.brush_texture.missing()
    });
    let old = &existing.geometry;
    if !is_vector_record(existing) || ribbon || geometry.segments.len() < 2
        || (old.kind.as_str(), old.coordinate_space.as_str(), old.handle_space.as_str())
            != (geometry.kind.as_str(), geometry.coordinate_space.as_str(), geometry.handle_space.as_str())
    {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let mut record = existing.clone();
    record.geometry = geometry;
    Ok(record)
}

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
    let mut links: Vec<(String, String)> = Vec::new();
    // A ribbon reshape's linked fill, re-synced in the same step (a derived change, not a receipt entry).
    let mut fills: Vec<Option<ObjectRecord>> = vec![None; ops.len()];
    for at in 0..ops.len() {
        let op = &mut ops[at];
        let (reshape, clone) = (op.op == "reshape", op.op == "clone");
        let shifted = clone && op.geometry.missing();
        if (reshape && op.geometry.missing() == op.center.missing()) || (!(reshape || clone) && !op.geometry.missing())
            || (!shifted && !op.offset.missing()) || (!reshape && !op.center.missing())
        {
            return Err(InvalidRequest);
        }
        if shifted {
            let frame = op.frame.value().and_then(serde_json::Number::as_u64).and_then(|n| u32::try_from(n).ok());
            let (Presence::Present(target), Some(frame)) = (op.stable_target.clone(), frame) else {
                return Err(InvalidRequest);
            };
            if !op.record.missing() || !op.from_step.missing() {
                return Err(InvalidRequest);
            }
            let offset = op.offset.value().copied().unwrap_or([0.0, 0.0]);
            let mut record = shifted_copy(origin.document(), &target, offset, &mut links, origin.content_revision())?;
            (record.target.frame_scope.frame, record.target.stroke_id) = (frame, String::new());
            (op.op, op.record, op.stable_target, op.frame, op.offset) =
                ("create".into(), Presence::Present(record), Presence::Missing, Presence::Missing, Presence::Missing);
            continue;
        }
        if reshape || clone {
            let frame = op.frame.value().and_then(serde_json::Number::as_u64).and_then(|n| u32::try_from(n).ok());
            let Presence::Present(target) = op.stable_target.clone() else {
                return Err(InvalidRequest);
            };
            if !op.record.missing() || !op.from_step.missing() || (!op.frame.missing() && frame.is_none()) || (clone && frame.is_none())
                || (reshape && !(op.placement.missing() && op.keyframe.missing()))
            {
                return Err(InvalidRequest);
            }
            // A reshape's `frame` is the held cell it edits (its copy-keyed copy keeps the stroke id); a clone is new there.
            let (mut record, fill) = match (std::mem::take(&mut op.center), std::mem::take(&mut op.geometry)) {
                (Presence::Present(center), _) => ribbon::reshape(origin.document(), &target, center)?,
                (_, Presence::Present(geometry)) => (with_geometry(origin.document(), &target, geometry)?, None),
                _ => return Err(InvalidRequest),
            };
            record.target.frame_scope.frame = frame.unwrap_or(target.frame_scope.frame);
            if clone {
                record.target.stroke_id = String::new();
            }
            fills[at] = fill.map(|mut fill| {
                fill.target.frame_scope.frame = record.target.frame_scope.frame;
                fill
            });
            (op.op, op.record, op.stable_target, op.frame) =
                ((if clone { "create" } else { "replace" }).into(), Presence::Present(record), Presence::Missing, Presence::Missing);
            continue;
        }
        let copy = op.op == "copy";
        if copy != !op.from_step.missing() || copy != !op.frame.missing() {
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
    for ((op, copy), fill) in ops.into_iter().zip(keyed).zip(fills) {
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
        let mut regenerated = edit.stroke_ids.clone();
        let mut step_changed = edit.changed;
        if let Some(fill) = fill {
            let synced = replace_in(&next, fill)?;
            next = synced.document;
            step_changed |= synced.changed;
            regenerated.extend(synced.stroke_ids);
        }
        if step_changed {
            let scope = Scope {
                context_id: target.context_id.clone(),
                layer_uid: target.layer_uid.clone(),
                frame: target.frame_scope.frame,
            };
            for stroke_id in &regenerated {
                next = regenerate(next, &scope, stroke_id, revision).map_err(|error| ObjectFillOwner::fill_plan_error(&error))?;
            }
        }
        document = next;
        changed |= step_changed;
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
