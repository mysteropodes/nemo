//! Pure staged fill preparation. This does not commit a revision or dispatch a
//! transport request; raw whole-envelope admission remains a separate gate.
use crate::object_document::{object_deserialize, ObjectDocument, ObjectTarget, SolidFill};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

#[derive(Debug)]
struct ObjectFillPayload {
    command: String,
    stable_target: ObjectTarget,
    fill: SolidFill,
}

object_deserialize!(ObjectFillPayload {
    command: String,
    stable_target: ObjectTarget,
    fill: SolidFill
});

/// Detached document value plus its preparation origin, never a new authoritative
/// snapshot. All fields remain private; consumers receive immutable borrows.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedObjectFill {
    document: ObjectDocument,
    target: ObjectTarget,
    base_instance_id: String,
    base_document_id: String,
    base_snapshot_id: String,
    base_content_revision: u64,
    changed: bool,
}

impl PreparedObjectFill {
    pub fn document(&self) -> &ObjectDocument {
        &self.document
    }
    pub fn target(&self) -> &ObjectTarget {
        &self.target
    }
    pub fn base_instance_id(&self) -> &str {
        &self.base_instance_id
    }
    pub fn base_document_id(&self) -> &str {
        &self.base_document_id
    }
    pub fn base_snapshot_id(&self) -> &str {
        &self.base_snapshot_id
    }
    pub fn base_content_revision(&self) -> u64 {
        self.base_content_revision
    }
    pub fn changed(&self) -> bool {
        self.changed
    }
}

fn same_fill(before: &SolidFill, after: &SolidFill) -> bool {
    [
        (&before.r, &after.r),
        (&before.g, &after.g),
        (&before.b, &after.b),
        (&before.a, &after.a),
    ]
    .into_iter()
    .all(|(before, after)| before.as_f64() == after.as_f64())
}

/// Parse exactly the command payload directly from bytes, before a Value parser
/// could discard duplicate members. This is not a JSON-RPC/request envelope or a
/// transport byte-limit port. Existing common-envelope limits stay unchanged.
pub fn prepare_object_fill_json(
    snapshot: &ObjectSnapshot,
    payload_bytes: &[u8],
) -> Result<PreparedObjectFill, DispatchErrorCode> {
    let payload: ObjectFillPayload =
        serde_json::from_slice(payload_bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    if payload.command != "object.fill.set"
        || !payload.stable_target.valid()
        || payload.stable_target.frame() >= snapshot.document().total_frames()
        || payload.fill.kind != "solid"
        || !payload.fill.valid_channels()
    {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let index = snapshot
        .document()
        .objects
        .iter()
        .position(|record| record.target == payload.stable_target)
        .ok_or(DispatchErrorCode::NotFound)?;
    let changed = !same_fill(&snapshot.document().objects[index].fill, &payload.fill);
    let mut document = snapshot.document().clone();
    if changed {
        document.objects[index].fill = payload.fill;
    }
    Ok(PreparedObjectFill {
        document,
        target: payload.stable_target,
        base_instance_id: snapshot.instance_id().to_owned(),
        base_document_id: snapshot.document_id().to_owned(),
        base_snapshot_id: snapshot.snapshot_id().to_owned(),
        base_content_revision: snapshot.content_revision(),
        changed,
    })
}
