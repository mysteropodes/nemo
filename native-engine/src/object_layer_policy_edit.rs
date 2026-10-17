//! Private persisted Drawing flags; no public capability or generic Motion lock policy.
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ObjectDocument, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

#[derive(Debug)]
struct LayerPolicyPayload {
    command: String,
    layer_uid: String,
    flag: String,
    value: bool,
}
object_deserialize!(LayerPolicyPayload {
    command: String,
    layer_uid: String,
    flag: String,
    value: bool
});

#[derive(Debug)]
enum LayerPolicyIntent {
    Locked(bool),
    Shy(bool),
}

/// Detached, non-Clone candidate: only this child can construct its sealed origin.
#[derive(Debug)]
pub(crate) struct PreparedLayerPolicy {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_policy_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerPolicy, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: LayerPolicyPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.layer.policy.edit" || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let intent = match payload.flag.as_str() {
            "locked" => LayerPolicyIntent::Locked(payload.value),
            "shy" => LayerPolicyIntent::Shy(payload.value),
            _ => return Err(DispatchErrorCode::InvalidRequest),
        };
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        let layer = document
            .layers
            .iter_mut()
            .find(|layer| layer.layer_uid() == payload.layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        let (field, value) = match intent {
            LayerPolicyIntent::Locked(value) => (&mut layer.locked, value),
            LayerPolicyIntent::Shy(value) => (&mut layer.shy, value),
        };
        // Baseline absent flags read as false. A no-op must not author false.
        let changed = field.value().copied().unwrap_or(false) != value;
        if changed {
            *field = Presence::Present(value);
        }
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedLayerPolicy {
            origin,
            document,
            changed,
        })
    }

    pub(crate) fn commit_layer_policy(
        &mut self,
        candidate: PreparedLayerPolicy,
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
        if !candidate.changed {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
