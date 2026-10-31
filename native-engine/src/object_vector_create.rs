//! L01-F / G23 / G15: vector creates for `object_vector_edit.rs`: one record, or 2..=32 records on one layer
//! and frame as one candidate, placed in front (the baseline's default) or behind the layer's existing
//! artwork (`drawMode === 'behind'`: `insertChild(0, path)` in tools.js/draw-bridge.js @1065e9d8).
use crate::commands::object_fill_owner::vector_edit::{
    check_drawing_layer, check_target, is_vector_record, order, order_code, Edit, VectorEditKind,
    MAX_CREATE_MANY, MAX_SAFE_REVISION,
};
use crate::object_document::{ObjectDocument, ObjectRecord, Presence, TimingEdit};
use crate::commands::object_fill_owner::object_frame_edit_document::{promote_cells, DocumentEdit};
use crate::request_receipts::DispatchErrorCode;
use std::collections::HashSet;

#[path = "object_vector_trim.rs"]
mod trim;

/// Where created records enter the layer's stack in their frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Placement {
    Front,
    Behind,
}
impl Placement {
    /// `placement` is optional on the two create commands only; absent is the baseline's front.
    pub(super) fn parse(command: &str, value: &Presence<String>) -> Result<Self, DispatchErrorCode> {
        let creates = matches!(command, "object.vector.create" | "object.vector.createMany");
        match value.value().map(String::as_str) {
            None => Ok(Self::Front),
            Some("front") if creates => Ok(Self::Front),
            Some("behind") if creates => Ok(Self::Behind),
            _ => Err(DispatchErrorCode::InvalidRequest),
        }
    }
}

/// `trimEnds` (the baseline `trimStrokeEndsMax`) is optional on the two create commands only: a finite
/// overshoot limit above zero. Absent means no trim, as with `trimStrokeEnds` off.
pub(super) fn trim_ends(command: &str, value: &Presence<serde_json::Number>) -> Result<Option<f64>, DispatchErrorCode> {
    let creates = matches!(command, "object.vector.create" | "object.vector.createMany" | "object.vector.batch");
    match value.value() {
        None => Ok(None),
        Some(n) if creates => n.as_f64().filter(|v| v.is_finite() && *v > 0.).map(Some).ok_or(DispatchErrorCode::InvalidRequest),
        Some(_) => Err(DispatchErrorCode::InvalidRequest),
    }
}

/// `keyframe: "copy"` (optional, creates only) keeps the baseline `ensureKeyframe()` of the Labs creators (French curve,
/// speed lines, pose stamps): a held or unexposed cell becomes a key holding a copy of what it showed. Absent is the
/// Draw/Pen/shape `ensureKeyframe(true)`: an empty key.
pub(super) fn copy_key(command: &str, value: &Presence<String>) -> Result<bool, DispatchErrorCode> {
    let creates = matches!(command, "object.vector.create" | "object.vector.createMany" | "object.vector.batch");
    match value.value().map(String::as_str) {
        None => Ok(false),
        Some("copy") if creates => Ok(true),
        Some(_) => Err(DispatchErrorCode::InvalidRequest),
    }
}

/// Revision identity is monotonic across undo/redo; imported IDs are skipped, so neither preparation nor a
/// refused commit consumes an ID.
pub(super) fn allocate_stroke_id(
    document: &ObjectDocument,
    revision: u64,
) -> Result<String, DispatchErrorCode> {
    let next = revision
        .checked_add(1)
        .filter(|next| *next <= MAX_SAFE_REVISION)
        .ok_or(DispatchErrorCode::Unavailable)?;
    let existing: HashSet<&str> = document
        .objects
        .iter()
        .map(|record| record.target.stroke_id())
        .collect();
    (0..=existing.len())
        .map(|suffix| format!("native-stroke:{next}:{suffix}"))
        .find(|id| !existing.contains(id.as_str()))
        .ok_or(DispatchErrorCode::Unavailable)
}

pub(super) fn create(
    base: &ObjectDocument,
    revision: u64,
    record: ObjectRecord,
    placement: Placement,
    trim: Option<f64>,
    copy_key: bool,
) -> Result<Edit, DispatchErrorCode> {
    create_many_or_one(base, revision, vec![record], placement, trim, copy_key, VectorEditKind::Create)
}

/// Admit each record exactly as a single create would be, against the document as it grows: ids and order
/// positions follow the given order. Behind, the k-th record takes natural position k under the layer's
/// existing elements of that frame (the baseline inserts the stroke at index 0 and symmetry copies above it).
fn create_many_or_one(
    base: &ObjectDocument,
    revision: u64,
    records: Vec<ObjectRecord>,
    placement: Placement,
    trim: Option<f64>,
    copy_key: bool,
    kind: VectorEditKind,
) -> Result<Edit, DispatchErrorCode> {
    create_in(base, revision, records, placement, trim, copy_key, kind)
}

/// Trim Stroke Ends (draw-bridge.js @1065e9d8 trims the drawn path before the symmetry copies and the Labs multi-frame
/// stamps exist): the first record's cut against the paths of its own cell, shared by every record, so copies keep the
/// same anchors. Nothing is trimmed when any record could not take the cut.
pub(super) fn trim_records(source: &ObjectDocument, records: &mut [ObjectRecord], max: f64) -> Result<(), DispatchErrorCode> {
    let first = records.first().ok_or(DispatchErrorCode::InvalidRequest)?.target.clone();
    let walls = source.objects.iter().filter(|r| {
        r.target.context_id == first.context_id
            && r.target.layer_uid == first.layer_uid
            && r.target.frame_scope == first.frame_scope
    });
    // A pressure ribbon trims its centreline and rebuilds its outline and its linked fill's.
    if records.iter().any(trim::is_vector_brush) {
        return trim::ribbon(records, walls, max);
    }
    // The baseline trims nothing when a cut would leave fewer than two anchors; neither do the copies.
    let cut = trim::cuts(&records[0], walls, max);
    let trimmed: Option<Vec<_>> = cut.and_then(|cut| records.iter().map(|r| trim::apply(r, cut)).collect());
    for (record, segments) in records.iter_mut().zip(trimmed.into_iter().flatten()) {
        record.geometry.segments = segments;
    }
    Ok(())
}

/// The create kernel against a document being edited; `revision` is the origin's, so ids stay monotonic.
pub(super) fn create_in(
    source: &ObjectDocument,
    revision: u64,
    mut records: Vec<ObjectRecord>,
    placement: Placement,
    trim: Option<f64>,
    copy_key: bool,
    kind: VectorEditKind,
) -> Result<Edit, DispatchErrorCode> {
    let first = records.first().ok_or(DispatchErrorCode::InvalidRequest)?.target.clone();
    if records.iter().any(|r| {
        r.target.context_id != first.context_id
            || r.target.layer_uid != first.layer_uid
            || r.target.frame_scope != first.frame_scope
    }) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    if let Some(max) = trim {
        trim_records(source, &mut records, max)?;
    }
    let mut document = source.clone();
    // Baseline Draw, Pen and shape commits run ensureKeyframe(true) first (draw-bridge.js commitStroke, tools.js
    // @1065e9d8): a held or unexposed target cell becomes an empty key in the same undo step, so the new drawing is not
    // the held one carried forward and does not land under it unseen. An in-between is left as it is. With `copy_key`
    // the key holds a copy of what the cell showed (the Labs creators' ensureKeyframe()).
    let held = document.layers.iter().find(|l| l.layer_uid == first.layer_uid).and_then(|l| l.timing.value())
            .is_some_and(|timing| matches!(timing.exposure(first.frame()).0, "held" | "unexposed"));
    if held && copy_key {
        // The copy key promotes through the scene's frame editor; a component's cells take only Draw's plain key (below).
        if first.context_id != "scene-root" {
            return Err(DispatchErrorCode::Unavailable);
        }
        if let DocumentEdit::Changed(next) = promote_cells(&document, &first.layer_uid, &[first.frame()])
            .map_err(|_| DispatchErrorCode::Unavailable)?
        {
            document = next;
        }
    } else if held {
        let total = document.total_frames;
        if let Some(layer) = document.layers.iter_mut().find(|l| l.layer_uid == first.layer_uid) {
            let edit = match layer.timing.value() {
                Some(timing) if matches!(timing.exposure(first.frame()).0, "held" | "unexposed") => {
                    Some(timing.promote_cell(first.frame(), total).map_err(|_| DispatchErrorCode::InvalidRequest)?)
                }
                _ => None,
            };
            if let Some(TimingEdit::Changed(timing)) = edit {
                layer.timing = Presence::Present(timing);
            }
        }
    }
    // Without an order plan the object array is the stack: behind goes before the layer's first record there.
    let mut at = document
        .objects
        .iter()
        .position(|r| r.target.layer_uid == first.layer_uid && r.target.context_id == first.context_id && r.target.frame_scope == first.frame_scope)
        .unwrap_or(document.objects.len());
    let mut stroke_ids = Vec::with_capacity(records.len());
    for (k, mut record) in records.into_iter().enumerate() {
        if !is_vector_record(&record) || !record.target.stroke_id.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let stroke_id = allocate_stroke_id(&document, revision)?;
        record.target.stroke_id = stroke_id.clone();
        check_target(&document, &record.target)?;
        check_drawing_layer(&document, &record.target)?;
        if let Presence::Present(plan) = &mut document.composition_order {
            match placement {
                Placement::Front => order::insert_element(plan, &record.target),
                Placement::Behind => order::insert_element_at(plan, &record.target, k),
            }
            .map_err(order_code)?;
        }
        if placement == Placement::Behind && document.composition_order.missing() {
            document.objects.insert(at, record);
            at += 1;
        } else {
            document.objects.push(record);
        }
        stroke_ids.push(stroke_id);
    }
    Ok(Edit {
        document,
        kind,
        stroke_id: stroke_ids[0].clone(),
        stroke_ids,
        changed: true,
        skipped: 0,
    })
}

pub(super) fn create_many(
    base: &ObjectDocument,
    revision: u64,
    records: Vec<ObjectRecord>,
    placement: Placement,
    trim: Option<f64>,
    copy_key: bool,
) -> Result<Edit, DispatchErrorCode> {
    if !(2..=MAX_CREATE_MANY).contains(&records.len()) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    create_many_or_one(base, revision, records, placement, trim, copy_key, VectorEditKind::CreateMany)
}
