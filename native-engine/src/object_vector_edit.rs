//! L01-F (G14): sealed vector create / replace / delete through the one revision owner. Strict original-byte
//! payload, one detached candidate document sealed to the immutable origin snapshot, full storage admission
//! before anything can publish, and publication only through the existing `ObjectRevisionOwner` (history,
//! request replay and dispatch live in their existing owners). Nothing here is an authority of its own.
//!
//! Wire (operation `command.document.object.vector.edit`, payload member `command` selects the action):
//! - `object.vector.create`: `record` is a complete `editable-vector-path` record whose `target.strokeId` is
//!   the empty string; the owner allocates `native-stroke:{next revision}:{n}` and the caller cannot choose it.
//! - `object.vector.replace`: `record` replaces the record with the same full target, in place.
//! - `object.vector.delete`: `stableTarget` names the record to remove.
//! - `object.vector.mask.edit`: `stableTarget` and `maskEdit` change only original Path mask tags.
//! - `object.vector.createMany`: `records` (2..=32 create records on one layer and frame) land in ONE
//!   revision and one undo entry, ids allocated in order (G23: one symmetry stroke undo removes all copies).
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    object_deserialize, FrameScopeKind, ObjectDocument, ObjectRecord, ObjectTarget, Presence,
    VECTOR_FAMILY,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::{json, Value};

#[path = "object_mask_edit.rs"]
mod mask_edit;
#[path = "object_vector_edit_order.rs"]
pub(crate) mod order;
#[path = "object_vector_create.rs"]
mod create_ops;
use create_ops::{create, create_many, Placement};
use crate::object_document::object_group_kernels::object_fill_trace_ops::regenerate::regenerate;
use order::OrderEditError;

/// Operation-specific bound on the FULL serialized request envelope of the vector edit operation only
/// (identifiers and metadata included). Every other object operation keeps its 4096-byte bound.
pub(crate) const MAX_VECTOR_REQUEST_BYTES: usize = 65_536;
pub(super) const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;
/// Bound on one atomic multi-create (symmetry copies): enough for an 8-way radial with mirrors.
pub(crate) const MAX_CREATE_MANY: usize = 32;

#[derive(Debug)]
struct VectorPayload {
    command: String,
    record: Presence<ObjectRecord>,
    records: Presence<Vec<ObjectRecord>>,
    placement: Presence<String>,
    stable_target: Presence<ObjectTarget>,
    mask_edit: Presence<mask_edit::MaskEdit>,
}
object_deserialize!(VectorPayload {
    command: String,
    #[serde(default)] record: Presence<ObjectRecord>,
    #[serde(default)] records: Presence<Vec<ObjectRecord>>,
    #[serde(default)] placement: Presence<String>,
    #[serde(default)] stable_target: Presence<ObjectTarget>,
    #[serde(default)] mask_edit: Presence<mask_edit::MaskEdit>
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VectorEditKind {
    Create,
    CreateMany,
    Replace,
    Delete,
    Mask,
}
impl VectorEditKind {
    fn label(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::CreateMany => "createMany",
            Self::Replace => "replace",
            Self::Delete => "delete",
            Self::Mask => "mask.edit",
        }
    }
}

/// Compact result retained by the request receipt and echoed in the response.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VectorEditResult {
    kind: VectorEditKind,
    stroke_id: String,
    stroke_ids: Vec<String>,
    applied: bool,
}
impl VectorEditResult {
    pub(crate) fn result(&self) -> Value {
        let mut value = json!({
            "applied": self.applied,
            "historyEntriesAdded": u64::from(self.applied),
            "operation": self.kind.label(),
            "strokeId": self.stroke_id,
        });
        if self.kind == VectorEditKind::CreateMany {
            value["strokeIds"] = json!(self.stroke_ids);
        }
        value
    }
}

/// Detached candidate sealed to the origin snapshot; its fields are private to this module, so a caller
/// cannot forge an origin, a document or a result. It is not an authoritative snapshot.
#[derive(Debug, Clone)]
pub(crate) struct PreparedVectorEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    result: VectorEditResult,
}
impl PreparedVectorEdit {
    /// The result the commit will report (`applied` is false for an exact no-op).
    pub(crate) fn result(&self) -> VectorEditResult {
        self.result.clone()
    }
}

pub(super) struct Edit {
    pub(super) document: ObjectDocument,
    pub(super) kind: VectorEditKind,
    pub(super) stroke_id: String,
    pub(super) stroke_ids: Vec<String>,
    pub(super) changed: bool,
}

pub(super) fn order_code(error: OrderEditError) -> DispatchErrorCode {
    match error {
        OrderEditError::ContextAbsent => DispatchErrorCode::NotFound,
        OrderEditError::ReferenceScope | OrderEditError::PositionOverflow => {
            DispatchErrorCode::Unavailable
        }
        OrderEditError::ElementAbsent => DispatchErrorCode::InvalidRequest,
    }
}

/// Vector drawing edits require an authored target; reference records remain readable storage.
pub(super) fn check_target(
    document: &ObjectDocument,
    target: &ObjectTarget,
) -> Result<(), DispatchErrorCode> {
    if !document.valid_target(target) || target.frame() >= document.total_frames() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    if target.frame_scope.kind == FrameScopeKind::Reference {
        return Err(DispatchErrorCode::Unavailable);
    }
    Ok(())
}

/// Reuse the persisted F06 flag for Drawing edits only. Shy and visibility do not lock content,
/// and this helper never participates in generic Motion property/track preparation. A layer whose
/// ordered path-effect stack has an active entry is refused too (baseline `editRefusalReason`): the
/// geometry on screen is the deformed one, so a drawing edit there would be written over the original.
pub(super) fn check_drawing_layer(
    document: &ObjectDocument,
    target: &ObjectTarget,
) -> Result<(), DispatchErrorCode> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.layer_uid() == target.layer_uid())
        .ok_or(DispatchErrorCode::NotFound)?;
    if document.layer_is_locked(layer.layer_uid()) {
        return Err(DispatchErrorCode::Unavailable);
    }
    if crate::commands::object_fill_owner::path_effects_edit::layer_has_active_path_fx(layer) {
        return Err(DispatchErrorCode::Unavailable);
    }
    Ok(())
}

pub(super) fn is_vector_record(record: &ObjectRecord) -> bool {
    record.schema_version == 1 && record.family == VECTOR_FAMILY
}

/// Compare mixed integer/float representations only when the float is exactly that integer.
/// The exclusive upper bounds avoid saturating casts at u64::MAX and i64::MAX.
fn same_integer_float(integer: &serde_json::Number, float: f64) -> bool {
    if let Some(n) = integer.as_u64() {
        if n == 0 {
            return float.to_bits() == 0.0_f64.to_bits();
        }
        return float >= 0.0
            && float < 18_446_744_073_709_551_616.0
            && float.fract() == 0.0
            && float as u64 == n;
    }
    integer.as_i64().is_some_and(|n| {
        float >= -9_223_372_036_854_775_808.0
            && float < 9_223_372_036_854_775_808.0
            && float.fract() == 0.0
            && float as i64 == n
    })
}

/// Exact integers retain their precision; 10 and 10.0 compare equal, but 0 and -0.0 do not.
/// Every other member compares structurally, extension members included.
pub(super) fn same_value(before: &Value, after: &Value) -> bool {
    match (before, after) {
        (Value::Number(x), Value::Number(y)) => {
            if let (Some(x), Some(y)) = (x.as_u64(), y.as_u64()) {
                return x == y;
            }
            if let (Some(x), Some(y)) = (x.as_i64(), y.as_i64()) {
                return x == y;
            }
            match (x.as_f64(), y.as_f64()) {
                (Some(a), Some(b)) if x.is_f64() && y.is_f64() => a.to_bits() == b.to_bits(),
                (_, Some(b)) if y.is_f64() => same_integer_float(x, b),
                (Some(a), _) if x.is_f64() => same_integer_float(y, a),
                _ => false,
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same_value(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(key, x)| y.get(key).is_some_and(|y| same_value(x, y)))
        }
        _ => before == after,
    }
}
pub(super) fn same_record(before: &ObjectRecord, after: &ObjectRecord) -> bool {
    match (serde_json::to_value(before), serde_json::to_value(after)) {
        (Ok(before), Ok(after)) => same_value(&before, &after),
        _ => false,
    }
}

fn replace(origin: &ObjectSnapshot, record: ObjectRecord) -> Result<Edit, DispatchErrorCode> {
    if !is_vector_record(&record) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    check_target(origin.document(), &record.target)?;
    let index = origin
        .document()
        .objects
        .iter()
        .position(|existing| existing.target == record.target)
        .ok_or(DispatchErrorCode::NotFound)?;
    if !is_vector_record(&origin.document().objects[index]) {
        return Err(DispatchErrorCode::Unavailable);
    }
    check_drawing_layer(origin.document(), &record.target)?;
    let stroke_id = record.target.stroke_id.clone();
    let mut document = origin.document().clone();
    let changed = !same_record(&document.objects[index], &record);
    if changed {
        document.objects[index] = record;
    }
    Ok(Edit {
        document,
        kind: VectorEditKind::Replace,
        stroke_ids: vec![stroke_id.clone()],
        stroke_id,
        changed,
    })
}

/// Property holders identify an element across frames (context, layer, strokeId). Removing the last drawing
/// of an element that a holder still animates would orphan the holder; that is refused, never silently
/// dropped (undo would restore it, but a refused edit loses nothing).
fn holder_depends(document: &ObjectDocument, target: &ObjectTarget) -> bool {
    document
        .property_holders()
        .into_iter()
        .flatten()
        .any(|holder| {
            holder.target.context_id == target.context_id
                && holder.target.layer_uid == target.layer_uid
                && holder.target.stroke_id.value() == Some(&target.stroke_id)
        })
}

fn delete(origin: &ObjectSnapshot, target: ObjectTarget) -> Result<Edit, DispatchErrorCode> {
    check_target(origin.document(), &target)?;
    let index = origin
        .document()
        .objects
        .iter()
        .position(|existing| existing.target == target)
        .ok_or(DispatchErrorCode::NotFound)?;
    if !is_vector_record(&origin.document().objects[index]) {
        return Err(DispatchErrorCode::Unavailable);
    }
    check_drawing_layer(origin.document(), &target)?;
    let mut document = origin.document().clone();
    document.objects.remove(index);
    let element_remains = document.objects.iter().any(|record| {
        record.target.frame_scope.kind == FrameScopeKind::Authored
            && record.target.context_id == target.context_id
            && record.target.layer_uid == target.layer_uid
            && record.target.stroke_id == target.stroke_id
    });
    if !element_remains && holder_depends(&document, &target) {
        return Err(DispatchErrorCode::Unavailable);
    }
    if let Presence::Present(plan) = &mut document.composition_order {
        order::remove_element(plan, &target).map_err(order_code)?;
    }
    Ok(Edit {
        document,
        kind: VectorEditKind::Delete,
        stroke_ids: vec![target.stroke_id.clone()],
        stroke_id: target.stroke_id,
        changed: true,
    })
}

impl ObjectFillOwner {
    /// Strict payload preparation directly from the original bytes (a `Value` would erase duplicate members).
    /// The returned candidate is sealed to this owner's current immutable snapshot and already passed the
    /// complete storage admission of the edited document; nothing is published.
    pub(crate) fn prepare_vector_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedVectorEdit, DispatchErrorCode> {
        if bytes.len() > MAX_VECTOR_REQUEST_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: VectorPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let origin = self.current_snapshot();
        let target = payload
            .record
            .value()
            .map(|r| r.target.clone())
            .or_else(|| payload.records.value().and_then(|r| r.first()).map(|r| r.target.clone()))
            .or_else(|| payload.stable_target.value().cloned());
        let many = payload.command == "object.vector.createMany";
        if many != payload.records.value().is_some() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let placement = Placement::parse(&payload.command, &payload.placement)?;
        let mut edit = match (
            payload.command.as_str(),
            payload.record,
            payload.stable_target,
            payload.mask_edit,
        ) {
            ("object.vector.createMany", Presence::Missing, Presence::Missing, Presence::Missing) => {
                let Presence::Present(records) = payload.records else {
                    return Err(DispatchErrorCode::InvalidRequest);
                };
                create_many(&origin, records, placement)?
            }
            (
                "object.vector.create",
                Presence::Present(record),
                Presence::Missing,
                Presence::Missing,
            ) => create(&origin, record, placement)?,
            (
                "object.vector.replace",
                Presence::Present(record),
                Presence::Missing,
                Presence::Missing,
            ) => replace(&origin, record)?,
            (
                "object.vector.delete",
                Presence::Missing,
                Presence::Present(target),
                Presence::Missing,
            ) => delete(&origin, target)?,
            (
                "object.vector.mask.edit",
                Presence::Missing,
                Presence::Present(target),
                Presence::Present(intent),
            ) => mask_edit::apply(&origin, target, intent)?,
            _ => return Err(DispatchErrorCode::InvalidRequest),
        };
        // Mask tags do not move source geometry or invalidate fill provenance.
        if edit.changed && edit.kind != VectorEditKind::Mask {
            let target = target.ok_or(DispatchErrorCode::InvalidRequest)?;
            let scope =
                crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope {
                    context_id: target.context_id,
                    layer_uid: target.layer_uid,
                    frame: target.frame_scope.frame,
                };
            for stroke_id in &edit.stroke_ids {
                edit.document = regenerate(edit.document, &scope, stroke_id, origin.content_revision())
                    .map_err(|error| Self::fill_plan_error(&error))?;
            }
        }
        ObjectSnapshot::validate_drawing(&edit.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedVectorEdit {
            origin,
            document: edit.document,
            result: VectorEditResult {
                kind: edit.kind,
                stroke_id: edit.stroke_id,
                stroke_ids: edit.stroke_ids,
                applied: edit.changed,
            },
        })
    }

    /// Publish a sealed candidate if and only if the origin pins (instance, document, snapshot, content
    /// revision) still match the current head. An exact no-op returns the unchanged head revision.
    pub(crate) fn commit_vector_edit(
        &mut self,
        candidate: PreparedVectorEdit,
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
        if !candidate.result.applied {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}

#[cfg(test)]
#[path = "../tests/object_vector_edit.rs"]
mod tests;
