//! Closed owner correlation of the existing native wire; endpoint admission remains in call_native.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
pub(super) fn correlated(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    let policy_command = request.operation == crate::native_contract::layer_policy::OP;
    let track_command = request.operation == crate::native_contract::property_track::OP;
    let property_edit = request.operation == crate::native_contract::property_edit::OP;
    let property_read = request.operation == crate::native_contract::property_read::OP;
    let frame_edit_command = crate::native_contract::frame_edit::declares(&request.operation);
    let image_owner = crate::native_contract::item::placed_png_owner_matches(request, response);
    let correlated_operation = request.operation == crate::native_contract::text_read::OP
        || matches!(
            request.operation.as_str(),
            crate::native_contract::composition::READ
                | crate::native_contract::composition::EDIT
                | crate::native_contract::palette::READ
                | crate::native_contract::palette::EDIT
        )
        || request.operation == crate::native_contract::metadata::OP
        || request.operation == crate::native_contract::placement::OP
        || request.operation == crate::native_contract::audio_track::OP
        || request.operation == crate::native_contract::layer_cut::OP
        || policy_command
        || track_command
        || property_edit
        || property_read
        || frame_edit_command
        || crate::native_contract::object_edit::admitted(&request.operation)
        || crate::native_contract::object_rig::admitted(&request.operation)
        || crate::native_contract::object_component::admitted(&request.operation)
        || crate::native_contract::import::admitted(&request.operation)
        || image_owner.is_some();
    let correlated_identity = !correlated_operation
        || (response.request_id == request.request_id
            && crate::native_contract::text_read::owner_matches(request, response)
                .or(image_owner)
                .unwrap_or(
                    response.instance_id == request.instance_id
                        && (response.document_id == request.document_id
                            || (!response.ok
                                && response
                                    .error
                                    .as_ref()
                                    .is_some_and(|e| e.code == "wrong_document"))),
                )
            && serde_json::to_vec(response).is_ok_and(|b| {
                b.len() <= crate::wire::native_response_byte_limit(&request.operation)
            }));
    correlated_identity
        && (request.operation != crate::native_contract::audio_track::OP
            || !response.ok
            || response.result.as_ref().is_some_and(|result| {
                crate::native_contract::audio_track::revision_matches(
                    request.expected_revision,
                    response.content_revision,
                    result,
                )
            }))
        && (request.operation != crate::native_contract::layer_cut::OP
            || !response.ok
            || response.result.as_ref().is_some_and(|result| {
                crate::native_contract::layer_cut::revision_matches(
                    request.expected_revision,
                    response.content_revision,
                    result,
                )
            }))
        && (request.operation != crate::native_contract::placement::OP
            || !response.ok
            || response.result.as_ref().is_some_and(|result| {
                crate::native_contract::placement::revision_matches(
                    request.expected_revision,
                    response.content_revision,
                    result,
                )
            }))
}
pub(super) fn reports_document(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    !response.ok
        && response.error.as_ref().is_some_and(|error| {
            error.code == "wrong_document"
                || crate::native_contract::text_read::wrong_instance_refusal(request, response)
                || crate::native_contract::item::placed_png_wrong_instance_refusal(
                    request, response,
                )
        })
}

/// Keep successful pinned-read headers and inner ownership on the same revision.
pub(super) fn pinned_read_correlated(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
    result: &serde_json::Value,
) -> bool {
    (!matches!(
        request.operation.as_str(),
        "query.document.object"
            | "query.document.object.layer.timing"
            | crate::native_contract::metadata::OP
            | crate::native_contract::text_read::OP
            | crate::native_contract::property_read::OP
            | crate::native_contract::composition::READ
            | crate::native_contract::palette::READ
    ) || result.get("atRevision").and_then(serde_json::Value::as_u64)
        == Some(response.content_revision))
        && (!matches!(
            request.operation.as_str(),
            "query.document.object.layer.timing"
                | crate::native_contract::metadata::OP
                | crate::native_contract::text_read::OP
                | crate::native_contract::property_read::OP
                | crate::native_contract::composition::READ
                | crate::native_contract::palette::READ
        ) || (response.instance_id == request.instance_id
            && response.document_id == request.document_id
            && result.get("instanceId").and_then(serde_json::Value::as_str)
                == Some(response.instance_id.as_str())
            && result.get("documentId").and_then(serde_json::Value::as_str)
                == Some(response.document_id.as_str())))
}
