//! Current live history projection from the sole owner; no read receipts or writes.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use serde_json::{json, Value};
const MAX_SAFE: u64 = 9_007_199_254_740_991;
fn identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(b))
}
impl NativeObjectHistory {
    /// Called under the common dispatch owner lock, before command/receipt dispatch.
    /// History is live state: even retained snapshots cannot select an older stack.
    pub fn dispatch_history_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        use DispatchErrorCode::*;
        let failure = |code| ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: false,
            result: None,
            error: Some(DispatchError {
                code,
                message: "Current native history read refused.".into(),
                details: (code == WrongDocument)
                    .then(|| json!({"requestedDocumentId":request.document_id})),
            }),
        };
        if !identifier(&request.request_id)
            || !identifier(&request.instance_id)
            || !identifier(&request.document_id)
            || !identifier(self.instance_id())
            || !identifier(self.document_id())
        {
            return Err(InvalidRequest);
        }
        let payload = request.payload.as_object();
        let revision = request.payload.get("atRevision").and_then(Value::as_u64);
        let snapshot_id = request
            .payload
            .get("documentSnapshotId")
            .and_then(Value::as_str);
        let code = if request.api_version != APPLICATION_API_VERSION
            || request.operation() != "query.document.object.history"
            || request.expected_revision.is_some()
            || !serde_json::to_vec(request).is_ok_and(|b| b.len() <= 4096)
            || payload.is_none_or(|p| {
                p.len() != 2
                    || !p.contains_key("atRevision")
                    || !p.contains_key("documentSnapshotId")
            })
            || revision.is_none_or(|r| r > MAX_SAFE)
            || snapshot_id.is_none_or(|s| !identifier(s))
        {
            Some(InvalidRequest)
        } else if request.instance_id != self.instance_id() {
            Some(WrongInstance)
        } else if request.document_id != self.document_id() {
            Some(WrongDocument)
        } else if request.cancelled_before_dispatch {
            Some(CancelledBeforeDispatch)
        } else if revision != Some(self.content_revision()) {
            Some(StaleRevision)
        } else {
            None
        };
        if let Some(code) = code {
            return Ok(failure(code));
        }
        let Some(snapshot) = self.acquire_snapshot(self.content_revision()) else {
            return Ok(failure(Unavailable));
        };
        if snapshot_id != Some(snapshot.snapshot_id()) {
            return Ok(failure(StaleRevision));
        }
        let (undo, redo) = self.history_depths();
        let undo = u64::try_from(undo)
            .ok()
            .filter(|n| *n <= MAX_SAFE)
            .ok_or(Unavailable)?;
        let redo = u64::try_from(redo)
            .ok()
            .filter(|n| *n <= MAX_SAFE)
            .ok_or(Unavailable)?;
        let response = ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            error: None,
            result: Some(json!({"atRevision":self.content_revision(),
                "documentSnapshotId":snapshot.snapshot_id(),"undoDepth":undo,"redoDepth":redo})),
        };
        if !serde_json::to_vec(&response).is_ok_and(|b| b.len() <= 4096) {
            return Ok(failure(Unavailable));
        }
        Ok(response)
    }
}
