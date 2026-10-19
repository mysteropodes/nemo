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
        || request.operation == crate::native_contract::metadata::OP
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
