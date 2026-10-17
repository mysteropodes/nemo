//! Immutable staged object reads, including common envelopes; no host activation.
use crate::object_codec::{
    self, object_deserialize, ObjectCodecError, ObjectCodecErrorKind, ObjectDocument, ObjectRecord,
    ObjectTarget,
};
use crate::request_receipts::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
pub use crate::revision::ObjectSnapshot;
#[cfg(all(test, feature = "test-commands"))]
#[path = "../tests/object_property_metadata_dispatch.rs"]
mod property_metadata_dispatch_tests;
#[path = "object_property_metadata_read.rs"]
mod property_metadata_read;
#[cfg(all(test, feature = "test-commands"))]
#[path = "../tests/object_property_metadata_read.rs"]
mod property_metadata_tests;
#[path = "object_property_read.rs"]
mod property_read;
#[path = "object_timing_snapshot.rs"]
mod timing;
use serde::{Deserialize, Serialize};
pub use timing::LayerTimingReadResponse;
const MAX_MESSAGE_BYTES: usize = 4096;

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "object_project_admission.rs"]
mod project_admission;
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) use project_admission::PreparedReferenceAttachment;

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "object_project_wire_admission.rs"]
mod project_wire_admission;

// L06-E (lane #2112): immutable per-asset decoded-pixel handle (reads only; no owner authority).
#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "object_raster_asset_resources.rs"]
#[allow(dead_code)]
pub(crate) mod object_raster_asset_resources;

#[cfg(all(feature = "history", feature = "project_media_codec"))]
use crate::revision::{
    shared_image_resource_budget, ImageResourceBudget, ImageResourceCharge, ImageResourceError,
};
#[cfg(all(feature = "history", feature = "project_media_codec"))]
use object_codec::PngWorkspace;
// Shared within the read-query layer, available without the application feature.
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) struct ChargedPngWorkspace {
    storage: Box<PngWorkspace>,
    _charge: ImageResourceCharge,
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
impl AsMut<PngWorkspace> for ChargedPngWorkspace {
    fn as_mut(&mut self) -> &mut PngWorkspace {
        &mut self.storage
    }
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
pub(crate) fn charged_png_workspace(
    budget: &ImageResourceBudget,
    initialize: impl FnOnce() -> Result<PngWorkspace, ImageResourceError>,
) -> Result<ChargedPngWorkspace, ImageResourceError> {
    // Fields destroy storage before charge, including error/unwind in initialization.
    let mut charge = budget.reserve(std::mem::size_of::<PngWorkspace>())?;
    let storage = charge.allocate_value(initialize)?;
    Ok(ChargedPngWorkspace {
        storage,
        _charge: charge,
    })
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
fn validate_png_in_domain(
    source: &[u8],
    png: object_codec::ImageAssetLimits,
    cpu: &ImageResourceBudget,
) -> Result<(), object_codec::ProjectWireError> {
    object_codec::validate_png_with_buffers(
        source,
        png,
        || {
            charged_png_workspace(cpu, || Ok(PngWorkspace::new()))
                .map_err(|_| object_codec::ProjectWireError::Budget)
        },
        |n| {
            cpu.allocate_bytes(n)
                .map_err(|_| object_codec::ProjectWireError::Budget)
        },
    )
    .map(|_| ())
}

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_project_admission.rs"]
mod managed_tests;

fn envelope_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
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
    /// Shared strict drawing admission for the private composite-owner entry.
    pub(crate) fn validate_drawing(document: &ObjectDocument) -> Result<(), ObjectCodecError> {
        object_codec::validate_storage(document)
    }
    /// Constructor revalidates even a caller's directly deserialized struct.
    /// Each admitted instance is a fresh incarnation; nothing edits revision 0.
    pub fn new(
        instance_id: impl Into<String>,
        document: ObjectDocument,
    ) -> Result<Self, ObjectCodecError> {
        object_codec::validate_storage(&document)?;
        let instance_id = instance_id.into();
        if instance_id.is_empty() {
            return Err(ObjectCodecError::new(
                ObjectCodecErrorKind::Invalid,
                "instanceId must be nonempty",
            ));
        }
        Self::from_admitted(instance_id, document)
            .map_err(|message| ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message))
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
        if request.instance_id != self.instance_id() {
            return failure(
                DispatchErrorCode::WrongInstance,
                "Object read targets a different instance.",
            );
        }
        if request.document_id != self.document_id() {
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
        if request.expected_revision.is_some() {
            return failure(
                DispatchErrorCode::InvalidRequest,
                "Only pinned object reads without expectedRevision are supported.",
            );
        }
        if request.operation == "query.document.object.serialize" {
            return self.serialize_response(&request);
        }
        if request.operation == "query.document.object.layer.timing" {
            return self.dispatch_layer_timing(&request);
        }
        if request.operation == "query.document.object.property.metadata" {
            return self.dispatch_property_metadata(&request);
        }
        if request.operation != "query.document.object" {
            return failure(
                DispatchErrorCode::InvalidRequest,
                "Unsupported object read operation.",
            );
        }
        let read = match self.query_json(&encoded) {
            Ok(read) => read,
            Err(code) => return failure(code, "Object read selector is invalid or unavailable."),
        };
        let response = ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().to_owned(),
            document_id: self.document_id().to_owned(),
            content_revision: self.content_revision(),
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

    fn serialize_response(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let failure = |code, message| Ok(self.read_failure(request, code, message));
        let revision = request
            .payload
            .get("atRevision")
            .and_then(serde_json::Value::as_u64);
        if request.payload.as_object().is_none_or(|p| p.len() != 1)
            || !revision.is_some_and(|r| r <= 9_007_199_254_740_991)
        {
            return failure(
                DispatchErrorCode::InvalidRequest,
                "Object serialization requires only atRevision.",
            );
        }
        if revision != Some(self.content_revision()) {
            return failure(
                DispatchErrorCode::NotFound,
                "Object serialization revision is unavailable.",
            );
        }
        if self.project().is_some() {
            return failure(
                DispatchErrorCode::Unavailable,
                "Project persistence is unavailable through drawing-only serialization.",
            );
        }
        let bytes = match object_codec::encode_storage_project(self.document()) {
            Ok(bytes) => bytes,
            Err(_) => {
                return failure(
                    DispatchErrorCode::Internal,
                    "Object serialization codec failed.",
                )
            }
        };
        let document_json = match String::from_utf8(bytes) {
            Ok(json) => json,
            Err(_) => {
                return failure(
                    DispatchErrorCode::Internal,
                    "Object serialization UTF-8 is invalid.",
                )
            }
        };
        let response = ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            error: None,
            result: Some(serde_json::json!({"atRevision":self.content_revision(),
                "documentSnapshotId":self.snapshot_id(),"documentJson":document_json})),
        };
        if !matches!(
            serde_json::to_vec(&response).map(|bytes| bytes.len()),
            Ok(0..=MAX_MESSAGE_BYTES)
        ) {
            return failure(
                DispatchErrorCode::Unavailable,
                "Complete object serialization exceeds the 4096-byte response limit.",
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
            instance_id: self.instance_id().to_owned(),
            document_id: self.document_id().to_owned(),
            content_revision: self.content_revision(),
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
            || !self.document().valid_target(&request.payload.stable_target)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if request.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if request.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if self
            .document()
            .reference_unavailable(&request.payload.stable_target)
        {
            return Err(DispatchErrorCode::Unavailable);
        }
        if request.payload.at_revision != self.content_revision() {
            return Err(DispatchErrorCode::NotFound);
        }
        let object = self
            .document()
            .objects()
            .iter()
            .find(|object| object.target() == &request.payload.stable_target)
            .ok_or(DispatchErrorCode::NotFound)?
            .clone();
        Ok(ObjectReadResponse {
            api_version: 2,
            request_id: request.request_id,
            instance_id: self.instance_id().to_owned(),
            document_id: self.document_id().to_owned(),
            content_revision: self.content_revision(),
            ok: true,
            result: ObjectReadResult {
                at_revision: self.content_revision(),
                document_snapshot_id: self.snapshot_id().to_owned(),
                object,
            },
        })
    }
}
