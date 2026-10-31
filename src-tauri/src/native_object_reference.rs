//! Ordinary reference controls send paths/settings, never media bytes in JS.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, require_main, HostResult,
};
use native_engine::{
    commands::OpacityRequest, object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fs::File, io::Read, path::Path};
use tauri_plugin_fs::FsExt;

#[path = "native_object_project_route.rs"]
mod project_route;
pub(super) use project_route::RecoverySnapshot;
pub(crate) use project_route::PROJECT_INFO_LIMIT_BYTES;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReferenceRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    lifecycle_generation: Option<u64>,
    expected_revision: Option<u64>,
    document_snapshot_id: Option<String>,
    action: ReferenceAction,
    #[serde(default)]
    cancelled_before_dispatch: bool,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ReferenceAction {
    Inspect,
    Attach {
        id: String,
        #[serde(rename = "sourcePath")]
        source_path: String,
    },
    Settings {
        name: String,
        visible: bool,
        opacity: f64,
        #[serde(rename = "offsetFrames")]
        offset_frames: i64,
    },
    Remove,
}

fn read_png(path: &str, allowed: impl Fn(&Path) -> bool) -> HostResult<(String, Vec<u8>)> {
    if path.contains('\0') || !Path::new(path).is_absolute() {
        return Err(host_error(
            "invalid_request",
            "invalid reference source path",
        ));
    }
    let resolved = Path::new(path)
        .canonicalize()
        .map_err(|_| host_error("unavailable", "reference file unavailable"))?;
    if !allowed(&resolved) {
        return Err(host_error(
            "unavailable",
            "reference file outside filesystem scope",
        ));
    }
    if !resolved.metadata().is_ok_and(|m| m.is_file()) {
        return Err(host_error(
            "invalid_request",
            "reference input must be a regular file",
        ));
    }
    let file = File::open(&resolved)
        .map_err(|_| host_error("unavailable", "reference file cannot be opened"))?;
    if !file
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() <= 16 * 1024 * 1024)
    {
        return Err(host_error(
            "invalid_request",
            "reference file exceeds PNG admission limit",
        ));
    }
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty() && n.len() <= 255)
        .ok_or_else(|| host_error("invalid_request", "invalid reference name"))?
        .to_owned();
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| host_error("unavailable", "reference file read failed"))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(host_error(
            "invalid_request",
            "reference file exceeds PNG admission limit",
        ));
    }
    Ok((name, bytes))
}

pub(super) fn reference_request(
    state: &ApplicationMcp,
    raw: &str,
    allowed: impl Fn(&Path) -> bool,
) -> HostResult<Value> {
    let invalid = || host_error("invalid_request", "invalid native reference request");
    if raw.len() > 4096 || !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    let request: ReferenceRequest = serde_json::from_str(raw).map_err(|_| invalid())?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    if !bounded_id(&request.request_id) || !bounded_id(&request.document_id) {
        return Err(invalid());
    }
    if request.cancelled_before_dispatch {
        return Err(host_error(
            "cancelled_before_dispatch",
            "reference request cancelled",
        ));
    }
    if matches!(&request.action, ReferenceAction::Inspect) {
        if request.lifecycle_generation.is_some()
            || request.expected_revision.is_some()
            || request.document_snapshot_id.is_some()
        {
            return Err(invalid());
        }
        let mut info = state
            .native_project_info(&request.document_id)
            .map_err(|_| host_error("unavailable", "native project metadata unavailable"))?;
        info["requestId"] = json!(request.request_id);
        return Ok(info);
    }
    let generation = request
        .lifecycle_generation
        .filter(|v| *v > 0 && *v <= 9_007_199_254_740_991)
        .ok_or_else(invalid)?;
    let revision = request
        .expected_revision
        .filter(|v| *v <= 9_007_199_254_740_991)
        .ok_or_else(invalid)?;
    let snapshot = request
        .document_snapshot_id
        .as_deref()
        .filter(|v| bounded_id(v))
        .ok_or_else(invalid)?;
    let pins = ObjectFrameSelector {
        instance_id: &request.instance_id,
        document_id: &request.document_id,
        content_revision: revision,
        document_snapshot_id: snapshot,
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    // Refuse stale identity before reading a possibly large file. The actual
    // commit repeats admission under the original native/revision guards.
    let info = state
        .native_project_info(&request.document_id)
        .map_err(|_| host_error("unavailable", "native reference owner unavailable"))?;
    if info["contentRevision"] != revision
        || info["lifecycleGeneration"] != generation
        || info["documentSnapshotId"] != snapshot
    {
        return Err(host_error(
            "stale_revision",
            "native reference selection is stale",
        ));
    }
    let (operation, payload, bytes) = match request.action {
        ReferenceAction::Attach { id, source_path } => {
            if id.is_empty() || id.len() > 255 || id.contains('\0') {
                return Err(invalid());
            }
            let (name, bytes) = read_png(&source_path, allowed)?;
            ("attach", json!({"id":id,"name":name}), Some(bytes))
        }
        ReferenceAction::Settings {
            name,
            visible,
            opacity,
            offset_frames,
        } => (
            "settings",
            json!({"name":name,"visible":visible,"opacity":opacity,"offsetFrames":offset_frames}),
            None,
        ),
        ReferenceAction::Remove => ("remove", json!({}), None),
        ReferenceAction::Inspect => unreachable!(),
    };
    let command = OpacityRequest::history_stage(
        request.request_id,
        &request.instance_id,
        &request.document_id,
        Some(revision),
        format!("command.document.reference.{operation}"),
        payload,
    );
    let receipt = state
        .native_project_reference(generation, &pins, &command, bytes.as_deref())
        .map_err(|_| host_error("unavailable", "native reference command refused"))?;
    serde_json::to_value(receipt)
        .map_err(|_| host_error("internal", "reference receipt unavailable"))
}

#[tauri::command]
pub(crate) fn nemo_native_object_reference(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
    request_json: String,
) -> HostResult<Value> {
    require_main(&window)?;
    reference_request(&state, &request_json, |path| {
        window.try_fs_scope().is_some_and(|s| s.is_allowed(path))
    })
}

#[cfg(test)]
#[path = "native_object_reference_tests.rs"]
mod tests;
