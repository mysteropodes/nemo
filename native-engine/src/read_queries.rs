//! Revision-pinned reads. Receipts contain selectors, never copied documents or JSON results.

#[cfg(feature = "project_media_codec")]
#[path = "reference_image_resources.rs"]
mod reference_image_resources;
#[cfg(all(test, feature = "project_media_codec"))]
pub(crate) use reference_image_resources::ReadbackBudget;
// L07-G (lane #2113, G06): brush bitmap publication from an admitted embedded PNG asset into the charged brush store.
#[cfg(feature = "project_media_codec")]
#[path = "bitmap_brush_resources_publish.rs"]
#[allow(dead_code)]
mod bitmap_brush_resources_publish;
#[cfg(feature = "project_media_codec")]
#[allow(unused_imports)]
pub(crate) use bitmap_brush_resources_publish::{
    BrushAssetError, BrushAssetPublication, BrushAssetPublisher, BrushAssetRequest,
    PreparedBrushAsset,
};

#[cfg(feature = "project_media_codec")]
pub(crate) struct VisibleReferenceRead {
    pub pixels: ReferencePixels,
    pub canvas: (u32, u32),
    pub opacity: f64,
}

/// Settings come from the current immutable snapshot. Reusing a raster requires
/// identity of its retained encoded allocation, never equal IDs or paths.
#[cfg(feature = "project_media_codec")]
pub(crate) fn visible_reference(
    snapshot: &crate::revision::ObjectSnapshot,
    previous: Option<&ReferencePixels>,
) -> Result<Option<VisibleReferenceRead>, ReferenceResourceError> {
    use crate::object_codec::{ImageAssetLimits, ProjectReferenceState};
    let Some(project) = snapshot.project() else {
        return Ok(None);
    };
    let ProjectReferenceState::StillImage(image) = project.reference_state() else {
        return Ok(None);
    };
    if !image.visible() || image.opacity() == 0.0 {
        return Ok(None);
    }
    let pixels = match previous.filter(|pixels| pixels.matches_image(snapshot)) {
        Some(pixels) => pixels.clone(),
        // Same operating envelope as native project/attachment admission.
        None => decode_snapshot_reference(
            snapshot,
            ImageAssetLimits::new(
                16 * 1024 * 1024,
                256 * 1024 * 1024,
                64 * 1024 * 1024,
                8192,
                8192,
            )?,
        )?,
    };
    Ok(Some(VisibleReferenceRead {
        pixels,
        canvas: project.project().canvas_dimensions(),
        opacity: image.opacity(),
    }))
}

use crate::codec;
use crate::evaluation::{self, EvaluationErrorKind};
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, ReceiptDisposition, OP_QUERY_EVALUATE,
};
use crate::revision::{DocumentSnapshot, RevisionOwner};
use serde::Deserialize;
use serde_json::{json, Value};

/// Full encoded v2 envelope limit, shared with the bundled MCP transport contract.
pub const MAX_READ_RESPONSE_BYTES: usize = 4096;
type ReadError = (DispatchErrorCode, &'static str);

fn transport_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes[1..].iter().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
        })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SerializePayload {
    at_revision: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EvaluatePayload {
    at_revision: u64,
    context_id: String,
    frame: u32,
}

pub(crate) fn prepare(owner: &RevisionOwner, request: &OpacityRequest) -> ReceiptDisposition {
    match prepare_read(owner, request) {
        Ok(receipt) => receipt,
        Err((code, message)) => {
            ReceiptDisposition::failure(owner.content_revision(), code, message)
        }
    }
}

fn prepare_read(
    owner: &RevisionOwner,
    request: &OpacityRequest,
) -> Result<ReceiptDisposition, ReadError> {
    let invalid = (
        DispatchErrorCode::InvalidRequest,
        "Invalid pinned read payload or expectedRevision.",
    );
    if request.expected_revision.is_some() {
        return Err(invalid);
    }
    let (at_revision, context_frame) = if request.operation == OP_QUERY_EVALUATE {
        let payload: EvaluatePayload =
            serde_json::from_value(request.payload.clone()).map_err(|_| invalid)?;
        if !transport_identifier(&payload.context_id) {
            return Err(invalid);
        }
        (
            payload.at_revision,
            Some((payload.context_id, payload.frame)),
        )
    } else {
        let payload: SerializePayload =
            serde_json::from_value(request.payload.clone()).map_err(|_| invalid)?;
        (payload.at_revision, None)
    };
    if at_revision > 9_007_199_254_740_991 {
        return Err(invalid);
    }
    let snapshot = owner.acquire(at_revision).ok_or((
        DispatchErrorCode::NotFound,
        "The selected content revision is unavailable.",
    ))?;
    let response = ReceiptDisposition::success_response(
        request,
        owner.instance_id(),
        owner.document_id(),
        owner.content_revision(),
        result(&snapshot, context_frame.as_ref())?,
    );
    let bytes = serde_json::to_vec(&response).map_err(|_| {
        (
            DispatchErrorCode::Internal,
            "The native read response could not be encoded.",
        )
    })?;
    if bytes.len() > MAX_READ_RESPONSE_BYTES {
        return Err((
            DispatchErrorCode::Unavailable,
            "Native read response exceeds 4096 encoded bytes.",
        ));
    }
    Ok(ReceiptDisposition::ReadQuerySuccess {
        content_revision: owner.content_revision(),
        at_revision,
        context_frame,
    })
}

pub(crate) fn result(
    snapshot: &DocumentSnapshot,
    context_frame: Option<&(String, u32)>,
) -> Result<Value, ReadError> {
    if snapshot
        .document()
        .layers()
        .iter()
        .any(|layer| !transport_identifier(layer.layer_uid()))
    {
        return Err((
            DispatchErrorCode::Unavailable,
            "A native layer identity cannot be represented by the application transport.",
        ));
    }
    let Some((context_id, frame)) = context_frame else {
        let invalid = (
            DispatchErrorCode::Internal,
            "The native document codec invariant failed.",
        );
        let bytes = codec::encode_project(snapshot.document()).map_err(|_| invalid)?;
        let document: Value = serde_json::from_slice(&bytes).map_err(|_| invalid)?;
        return Ok(json!({
            "atRevision": snapshot.content_revision(),
            "documentSnapshotId": snapshot.id(),
            "document": document,
        }));
    };
    let evaluated =
        evaluation::evaluate(snapshot, context_id, *frame).map_err(|error| match error.kind() {
            EvaluationErrorKind::Unsupported => (
                DispatchErrorCode::Unavailable,
                "The native evaluation context or content is unsupported.",
            ),
            EvaluationErrorKind::OutOfRange => (
                DispatchErrorCode::InvalidRequest,
                "The evaluation frame is outside the document range.",
            ),
            EvaluationErrorKind::Invalid => (
                DispatchErrorCode::Internal,
                "The native evaluation invariant failed.",
            ),
        })?;
    Ok(json!({
        "documentSnapshotId": evaluated.document_snapshot_id(),
        "documentId": evaluated.document_id(),
        "contentRevision": evaluated.content_revision(),
        "contextId": evaluated.context_id(),
        "frame": evaluated.frame(),
        "layers": evaluated.layers().iter().map(|layer| json!({
            "layerUid": layer.layer_uid(), "value": layer.value(),
        })).collect::<Vec<_>>(),
    }))
}
