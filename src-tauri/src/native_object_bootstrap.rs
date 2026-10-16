//! Strict raw object installation through the shared authority; no feature activation.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, require_main, HostResult, HOST_API_VERSION,
};
use native_engine::object_codec::decode_storage_project;
use serde::{Deserialize, Serialize};
use tauri_plugin_fs::FsExt;

#[path = "native_object_reopen.rs"]
mod reopen;

#[path = "native_object_inactive_project.rs"]
mod inactive;
#[allow(unused_imports)]
pub(crate) use inactive::{
    __cmd__nemo_native_object_inactive_project,
    __tauri_command_name_nemo_native_object_inactive_project, nemo_native_object_inactive_project,
};

#[path = "native_object_project_replace.rs"]
mod replace;

#[path = "native_object_project_admission.rs"]
mod admission;

#[path = "native_object_reference.rs"]
mod reference;
#[path = "native_object_save.rs"]
mod save;
#[allow(unused_imports)]
pub(crate) use reference::{
    __cmd__nemo_native_object_reference, __tauri_command_name_nemo_native_object_reference,
    nemo_native_object_reference,
};
#[allow(unused_imports)]
pub(crate) use save::{
    __cmd__nemo_native_object_save_as, __tauri_command_name_nemo_native_object_save_as,
    nemo_native_object_save_as,
};

#[cfg(test)]
#[path = "native_object_save_tests.rs"]
mod save_tests;

#[cfg(test)]
#[path = "native_object_v2_persistence_tests.rs"]
mod v2_tests;

#[cfg(test)]
#[path = "native_object_project_tests.rs"]
mod project_tests;

#[cfg(test)]
#[path = "native_object_new_project_tests.rs"]
mod new_project_tests;

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

#[derive(Debug, Serialize, Deserialize)]
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
    if let Some(request) = admission::new_request(request_json) {
        return admission::bootstrap_new(state, request);
    }
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
    bootstrap_file_bytes(state, request.request_id, request.document_json.as_bytes())
}

pub(super) fn bootstrap_file_bytes(
    state: &ApplicationMcp,
    request_id: String,
    bytes: &[u8],
) -> HostResult<ObjectBootstrapReceipt> {
    #[derive(Deserialize)]
    struct Format {
        format: String,
    }
    // This only chooses between two strict native codecs; it never reconstructs
    // input bytes or accepts the project. The selected codec validates all keys.
    let format: Format = serde_json::from_slice(bytes)
        .map_err(|_| host_error("invalid_request", "invalid native project format"))?;
    let (document_id, content_revision, resource_count) =
        if format.format == "nemo.native-project-document" {
            state
                .install_native_project_file(bytes)
                .map_err(|_| host_error("unavailable", "native project installation refused"))?
        } else {
            if bytes.len() >= MAX_OBJECT_BOOTSTRAP_BYTES {
                return Err(host_error(
                    "invalid_request",
                    "bare object file exceeds byte limit",
                ));
            }
            let document = decode_storage_project(bytes)
                .map_err(|_| host_error("invalid_request", "invalid native object document"))?;
            let (document, revision) = state
                .install_native_object(document)
                .map_err(|_| host_error("unavailable", "native object installation unavailable"))?;
            (document, revision, 0)
        };
    Ok(ObjectBootstrapReceipt {
        api_version: HOST_API_VERSION,
        request_id,
        instance_id: state.instance_id().to_owned(),
        document_id,
        content_revision,
        resource_count,
        viewport_available: false,
    })
}

#[tauri::command]
pub(crate) async fn nemo_native_object_bootstrap(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request_json: String,
) -> HostResult<ObjectBootstrapReceipt> {
    require_main(&window)?;
    let admitted = if let Some(request) =
        admission::new_request(&request_json).filter(|request| request.replace.is_some())
    {
        Some(admission::prepare_blank(&state, request)?)
    } else if let Some(request) =
        reopen::file_request(&request_json).filter(|request| request.replace.is_some())
    {
        Some(replace::prepare(&state, request, |path| {
            window
                .try_fs_scope()
                .is_some_and(|scope| scope.is_allowed(path))
        })?)
    } else {
        None
    };
    if let Some(admitted) = admitted {
        let replace::Admission::Execute {
            generation,
            request_id,
            fingerprint,
            prepared,
        } = admitted
        else {
            let replace::Admission::Replay(receipt) = admitted else {
                unreachable!()
            };
            return Ok(receipt);
        };
        state.invalidate_native_subscriber(&app);
        let native = state.native_state();
        let instance = state.instance_id().to_owned();
        let waiter = replace::submit(
            native.clone(),
            generation,
            request_id.clone(),
            fingerprint.clone(),
            prepared,
            |callback| app.run_on_main_thread(callback).map_err(|_| ()),
            move || {
                crate::native_application_viewport::release(&instance)
                    .map(|released| (released.work_ids, released.status))
            },
        );
        let result = match waiter {
            Ok(waiter) => waiter.await.map_err(|_| ()),
            Err(_) => Err(()),
        };
        return result.unwrap_or_else(|_| {
            replace::reconcile_executor(&native, generation, &request_id, &fingerprint)
        });
    }
    reopen::bootstrap_request(&state, &request_json, |path| {
        window
            .try_fs_scope()
            .is_some_and(|scope| scope.is_allowed(path))
    })
}

// Test fixtures without a viewport still use the real replacement admission,
// cleanup and commit path. Initial-only bootstrap_raw must keep rejecting pins.
#[cfg(test)]
pub(crate) fn replace_blank_without_viewport_for_test(
    state: &ApplicationMcp,
    request_json: &str,
) -> HostResult<ObjectBootstrapReceipt> {
    let request = admission::new_request(request_json)
        .ok_or_else(|| host_error("invalid_request", "invalid blank replacement fixture"))?;
    match admission::prepare_blank(state, request)? {
        admission::Admission::Replay(receipt) => Ok(receipt),
        admission::Admission::Execute { generation, request_id, prepared, .. } => {
            replace::complete(&state.native_state(), generation, &request_id, prepared, || {
                Ok((Vec::new(), "already_absent"))
            })
        }
    }
}

#[cfg(test)]
#[path = "native_object_bootstrap_tests.rs"]
mod tests;
