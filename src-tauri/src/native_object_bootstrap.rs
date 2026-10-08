//! Strict raw object installation through the shared authority; no feature activation.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, require_main, HostResult, HOST_API_VERSION,
};
use native_engine::object_codec::decode_project;
use serde::{Deserialize, Serialize};
use tauri_plugin_fs::FsExt;

#[path = "native_object_reopen.rs"]
mod reopen;

// This bounds the complete original wrapper, including escaped document bytes.
const MAX_OBJECT_BOOTSTRAP_BYTES: usize = 1_048_576;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObjectBootstrapRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_json: String,
    #[serde(default)]
    cancelled_before_dispatch: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObjectBootstrapReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    content_revision: u64,
    resource_count: usize,
    viewport_available: bool,
}

/// A commit receipt, not a claim about a later UI, dispatch or presentation state.
pub(crate) fn bootstrap_raw(
    state: &ApplicationMcp,
    request_json: &str,
) -> HostResult<ObjectBootstrapReceipt> {
    if request_json.len() > MAX_OBJECT_BOOTSTRAP_BYTES {
        return Err(host_error(
            "invalid_request",
            "object bootstrap exceeds byte limit",
        ));
    }
    if !request_json.trim_start().starts_with('{') {
        return Err(host_error(
            "invalid_request",
            "object bootstrap requires a JSON object",
        ));
    }
    // A Value intermediate would erase duplicate decoded wrapper/document keys.
    let request: ObjectBootstrapRequest = serde_json::from_str(request_json)
        .map_err(|_| host_error("invalid_request", "invalid raw object bootstrap request"))?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    if !bounded_id(&request.request_id) {
        return Err(host_error(
            "invalid_request",
            "invalid object bootstrap request identity",
        ));
    }
    if request.cancelled_before_dispatch {
        return Err(host_error(
            "cancelled_before_dispatch",
            "object bootstrap was cancelled",
        ));
    }
    let document = decode_project(request.document_json.as_bytes())
        .map_err(|_| host_error("invalid_request", "invalid native object document"))?;
    let (document_id, content_revision) = state
        .install_native_object(document)
        .map_err(|_| host_error("unavailable", "native object installation unavailable"))?;
    Ok(ObjectBootstrapReceipt {
        api_version: HOST_API_VERSION,
        request_id: request.request_id,
        instance_id: state.instance_id().to_owned(),
        document_id,
        content_revision,
        resource_count: 0,
        viewport_available: false,
    })
}

#[tauri::command]
pub(crate) fn nemo_native_object_bootstrap(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
    request_json: String,
) -> HostResult<ObjectBootstrapReceipt> {
    require_main(&window)?;
    reopen::bootstrap_request(&state, &request_json, |path| {
        window
            .try_fs_scope()
            .is_some_and(|scope| scope.is_allowed(path))
    })
}

#[cfg(test)]
#[path = "native_object_bootstrap_tests.rs"]
mod tests;
