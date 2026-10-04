//! Immutable staged object reads, including common envelopes; no host activation.
use crate::object_codec::{
    self, object_deserialize, ObjectCodecError, ObjectCodecErrorKind, ObjectDocument, ObjectRecord,
    ObjectTarget,
};
use crate::request_receipts::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

static NEXT_OBJECT_DOCUMENT: AtomicU64 = AtomicU64::new(1);
const MAX_MESSAGE_BYTES: usize = 4096;

fn envelope_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
}

#[derive(Debug, Clone)]
pub struct ObjectSnapshot {
    instance_id: String,
    document_id: String,
    snapshot_id: String,
    document: Arc<ObjectDocument>,
}

#[derive(Debug)]
struct ReadRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    operation: String,
    payload: ReadPayload,
}
#[derive(Debug)]
struct ReadPayload {
    at_revision: u64,
    stable_target: ObjectTarget,
}

object_deserialize!(ReadRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    operation: String,
    payload: ReadPayload
});
object_deserialize!(ReadPayload {
    at_revision: u64,
    stable_target: ObjectTarget
});

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectReadResponse {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    content_revision: u64,
    ok: bool,
    result: ObjectReadResult,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObjectReadResult {
    at_revision: u64,
    document_snapshot_id: String,
    object: ObjectRecord,
}

impl ObjectSnapshot {
    /// Constructor revalidates even a caller's directly deserialized struct.
    /// Each admitted instance is a fresh incarnation; nothing edits revision 0.
    pub fn new(
        instance_id: impl Into<String>,
        document: ObjectDocument,
    ) -> Result<Self, ObjectCodecError> {
        object_codec::validate(&document)?;
        let instance_id = instance_id.into();
        if instance_id.is_empty() {
            return Err(ObjectCodecError::new(
                ObjectCodecErrorKind::Invalid,
                "instanceId must be nonempty",
            ));
        }
        let sequence = NEXT_OBJECT_DOCUMENT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| {
                ObjectCodecError::new(
                    ObjectCodecErrorKind::Invalid,
                    "object document identity sequence exhausted",
                )
            })?;
        let document_id = format!("native-object-document-{sequence}");
        Ok(Self {
            instance_id,
            snapshot_id: format!("native-object:{document_id}:0"),
            document_id,
            document: Arc::new(document),
        })
    }
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn document_id(&self) -> &str {
        &self.document_id
    }
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }
    pub fn content_revision(&self) -> u64 {
        0
    }
    pub fn document(&self) -> &ObjectDocument {
        &self.document
    }

    /// Staged shared-envelope read only. Unrepresentable correlation identities
    /// return a typed error rather than echoing unbounded IDs or fabricating IDs.
    /// This typed entry cannot detect duplicate members already lost in a Value.
    pub fn dispatch(&self, request: OpacityRequest) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if [
            request.request_id.as_str(),
            request.instance_id.as_str(),
            request.document_id.as_str(),
            self.instance_id(),
            self.document_id(),
        ]
        .iter()
        .any(|value| !envelope_identifier(value))
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let encoded =
            serde_json::to_vec(&request).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let failure = |code, message| Ok(self.read_failure(&request, code, message));
        if request.api_version != APPLICATION_API_VERSION
            || !envelope_identifier(&request.operation)
            || !request.payload.is_object()
            || encoded.len() > MAX_MESSAGE_BYTES
        {
            return failure(
                DispatchErrorCode::InvalidRequest,
                "Invalid bounded object read request.",
            );
        }
        if request.instance_id != self.instance_id {
            return failure(
                DispatchErrorCode::WrongInstance,
                "Object read targets a different instance.",
            );
        }
        if request.document_id != self.document_id {
            return failure(
                DispatchErrorCode::WrongDocument,
                "Object read targets a different document.",
            );
        }
        if request.cancelled_before_dispatch {
            return failure(
                DispatchErrorCode::CancelledBeforeDispatch,
                "Object read was cancelled before dispatch.",
            );
        }
        if request.operation != "query.document.object" || request.expected_revision.is_some() {
            return failure(
                DispatchErrorCode::InvalidRequest,
                "Only pinned object reads without expectedRevision are supported.",
            );
        }
        let read = match self.query_json(&encoded) {
            Ok(read) => read,
            Err(code) => return failure(code, "Object read selector is invalid or unavailable."),
        };
        let response = ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id.clone(),
            document_id: self.document_id.clone(),
            content_revision: 0,
            ok: true,
            result: Some(
                serde_json::to_value(read.result).map_err(|_| DispatchErrorCode::Internal)?,
            ),
            error: None,
        };
        if !matches!(
            serde_json::to_vec(&response).map(|bytes| bytes.len()),
            Ok(0..=MAX_MESSAGE_BYTES)
        ) {
            return failure(
                DispatchErrorCode::Unavailable,
                "Complete object read exceeds the 4096-byte response limit.",
            );
        }
        Ok(response)
    }

    fn read_failure(
        &self,
        request: &OpacityRequest,
        code: DispatchErrorCode,
        message: &str,
    ) -> ResponseEnvelope {
        ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id.clone(),
            document_id: self.document_id.clone(),
            content_revision: 0,
            ok: false,
            result: None,
            error: Some(DispatchError {
                code,
                message: message.into(),
                details: (code == DispatchErrorCode::WrongDocument)
                    .then(|| serde_json::json!({"requestedDocumentId":request.document_id})),
            }),
        }
    }

    /// Direct bytes preserve duplicate-member rejection. Typed errors must be
    /// wrapped by a later admitted transport; this is not the active dispatcher.
    pub fn query_json(&self, bytes: &[u8]) -> Result<ObjectReadResponse, DispatchErrorCode> {
        let request: ReadRequest =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if request.api_version != 2
            || request.operation != "query.document.object"
            || request.request_id.is_empty()
            || request.instance_id.is_empty()
            || request.document_id.is_empty()
            || request.payload.at_revision > 9_007_199_254_740_991
            || !request.payload.stable_target.valid()
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if request.instance_id != self.instance_id {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if request.document_id != self.document_id {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if request.payload.at_revision != 0 {
            return Err(DispatchErrorCode::NotFound);
        }
        let object = self
            .document
            .objects()
            .iter()
            .find(|object| object.target() == &request.payload.stable_target)
            .ok_or(DispatchErrorCode::NotFound)?
            .clone();
        Ok(ObjectReadResponse {
            api_version: 2,
            request_id: request.request_id,
            instance_id: self.instance_id.clone(),
            document_id: self.document_id.clone(),
            content_revision: 0,
            ok: true,
            result: ObjectReadResult {
                at_revision: 0,
                document_snapshot_id: self.snapshot_id.clone(),
                object,
            },
        })
    }
}
