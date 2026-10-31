//! Main-window inactive-file requests retain B in the original object host.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, require_main, HostResult,
};
use crate::native_dispatch::NativeObjectHost;
use crate::native_object_bootstrap::reopen::{read_file, FileRequest};
use native_engine::{object_document::FrameScopeKind, object_frame_packet::ObjectFrameSelector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tauri_plugin_fs::FsExt;

#[path = "native_object_retained_activation.rs"]
mod activation;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    expected_revision: u64,
    document_snapshot_id: String,
    lifecycle_generation: u64,
    action: Action,
    #[serde(default)]
    cancelled_before_dispatch: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    Prepare {
        #[serde(rename = "sourcePath")]
        source_path: String,
    },
    Inspect {
        #[serde(rename = "preparationToken")]
        preparation_token: Option<String>,
        #[serde(rename = "recoverRequestId")]
        recover_request_id: Option<String>,
    },
    Close {
        #[serde(rename = "preparationToken")]
        preparation_token: String,
    },
}
enum Admission {
    Reply(Value),
    Read,
}

fn error(message: String, disposition: &str) -> nemo_mcp::contract::NativeApplicationError {
    let (code, detail) = message
        .split_once(':')
        .filter(|(code, _)| {
            !code.is_empty() && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        })
        .unwrap_or(("unavailable", &message));
    let mut error = host_error(code, detail);
    let disposition = match code {
        "inactive_pending" => "pending",
        "inactive_indeterminate" => "indeterminate",
        "preparation_closed" => "closed",
        _ => disposition,
    };
    error.details = Some(json!({"disposition":disposition,"retryExecution":false}));
    error
}
fn encode(result: HostResult<Value>) -> Value {
    match result {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => json!({"ok":false,"error":error}),
    }
}
fn decode(value: Value) -> HostResult<Value> {
    if serde_json::to_vec(&value).map_or(true, |v| v.len() > 68 * 1024) {
        return Err(error(
            "inactive_indeterminate:inactive receipt exceeds limit".into(),
            "indeterminate",
        ));
    }
    if value["ok"] == true {
        return Ok(value["value"].clone());
    }
    Err(
        serde_json::from_value(value["error"].clone()).unwrap_or_else(|_| {
            error(
                "inactive_indeterminate:retained inactive result is invalid".into(),
                "indeterminate",
            )
        }),
    )
}
fn receipt(request: &Request, state: &str, project: Option<Value>) -> Value {
    let token = match &request.action {
        Action::Close { preparation_token } => json!(preparation_token),
        _ => project
            .as_ref()
            .map(|p| p["preparationToken"].clone())
            .unwrap_or(Value::Null),
    };
    json!({"apiVersion":2,"requestId":request.request_id,"instanceId":request.instance_id,
        "documentId":request.document_id,"contentRevision":request.expected_revision,
        "documentSnapshotId":request.document_snapshot_id,"lifecycleGeneration":request.lifecycle_generation,
        "state":state,"preparationToken":token,"project":project,"retryExecution":false})
}
fn with_owner<T>(
    state: &ApplicationMcp,
    request: &Request,
    operation: impl FnOnce(&mut NativeObjectHost) -> Result<T, String>,
) -> Result<T, String> {
    let pins = ObjectFrameSelector {
        instance_id: &request.instance_id,
        document_id: &request.document_id,
        content_revision: request.expected_revision,
        document_snapshot_id: &request.document_snapshot_id,
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    state.with_object_preview_owner(request.lifecycle_generation, &pins, |owner| {
        if !owner.matches_project_snapshot(&request.document_snapshot_id) {
            return Err("stale_revision:inactive project snapshot changed".into());
        }
        operation(owner)
    })
}

fn request_with(
    state: &ApplicationMcp,
    raw: &str,
    window: &str,
    allowed: impl Fn(&Path) -> bool,
    before_store: impl FnOnce(),
    after_effect: impl FnOnce(),
) -> HostResult<Value> {
    if window != "main" {
        return Err(host_error(
            "wrong_instance",
            "inactive project requires main window",
        ));
    }
    let invalid = || host_error("invalid_request", "invalid native inactive project request");
    if raw.len() > 4096 || !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    let request: Request = serde_json::from_str(raw).map_err(|_| invalid())?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    if [
        &request.request_id,
        &request.document_id,
        &request.document_snapshot_id,
    ]
    .iter()
    .any(|s| !bounded_id(s))
        || request.expected_revision > 9_007_199_254_740_991
        || request.lifecycle_generation == 0
        || request.lifecycle_generation > 9_007_199_254_740_991
    {
        return Err(invalid());
    }
    match &request.action {
        Action::Prepare { source_path }
            if source_path.contains('\0') || !Path::new(source_path).is_absolute() =>
        {
            return Err(invalid())
        }
        Action::Inspect {
            preparation_token: Some(token),
            ..
        }
        | Action::Close {
            preparation_token: token,
        } if !bounded_id(token) => return Err(invalid()),
        Action::Inspect {
            recover_request_id: Some(id),
            ..
        } if !bounded_id(id) => return Err(invalid()),
        _ => (),
    }
    let fingerprint = serde_json::to_vec(&request).map_err(|_| invalid())?;
    if fingerprint.len() > 4096 {
        return Err(invalid());
    }
    let mut after_effect = Some(after_effect);
    let mut entered = false;
    let admission = with_owner(state, &request, |owner| {
        if let Some(reply) = owner.replay_inactive_request(&request.request_id, &fingerprint)? {
            return Ok(Admission::Reply(reply));
        }
        if let Action::Inspect {
            preparation_token,
            recover_request_id,
        } = &request.action
        {
            if request.cancelled_before_dispatch {
                return Ok(Admission::Reply(encode(Err(error(
                    "cancelled_before_dispatch:inactive inspection cancelled".into(),
                    "notDispatched",
                )))));
            }
            let project = owner.current_inactive_project()?;
            if preparation_token.as_ref().is_some_and(|token| {
                project
                    .as_ref()
                    .is_none_or(|p| p["preparationToken"] != *token)
            }) {
                return Err("preparation_expired:inactive token is unavailable".into());
            }
            let mut value = receipt(
                &request,
                if project.is_some() {
                    "inactive-retained"
                } else {
                    "inactive-absent"
                },
                project,
            );
            if let Some(id) = owner.pending_inactive_request_id() {
                value["state"] = json!("inactive-pending");
                value["pendingRequestId"] = json!(id);
            }
            if let Some(id) = recover_request_id {
                value["recoveredRequest"] =
                    json!({"requestId":id,"outcome":owner.inactive_request_outcome(id)});
            }
            return Ok(Admission::Reply(encode(Ok(value))));
        }
        let close_token = match &request.action {
            Action::Close { preparation_token } if !request.cancelled_before_dispatch => {
                Some(preparation_token.as_str())
            }
            _ => None,
        };
        owner.begin_inactive_request(
            &request.request_id,
            &fingerprint,
            matches!(&request.action, Action::Prepare { .. }) && !request.cancelled_before_dispatch,
            close_token,
        )?;
        if request.cancelled_before_dispatch {
            let reply = encode(Err(error(
                "cancelled_before_dispatch:inactive request cancelled".into(),
                "notDispatched",
            )));
            owner.finish_inactive_request(&request.request_id, &fingerprint, reply.clone())?;
            return Ok(Admission::Reply(reply));
        }
        if let Action::Close { preparation_token } = &request.action {
            entered = true;
            let result = owner
                .close_inactive_project(
                    &request.instance_id,
                    &request.document_id,
                    request.expected_revision,
                    &request.document_snapshot_id,
                    preparation_token,
                )
                .map(|_| receipt(&request, "inactive-closed", None))
                .map_err(|e| error(e, "notDispatched"));
            after_effect.take().unwrap()();
            let reply = encode(result);
            owner.finish_inactive_request(&request.request_id, &fingerprint, reply.clone())?;
            return Ok(Admission::Reply(reply));
        }
        Ok(Admission::Read)
    })
    .map_err(|e| {
        error(
            e,
            if entered {
                "indeterminate"
            } else {
                "notDispatched"
            },
        )
    })?;
    if let Admission::Reply(reply) = admission {
        return decode(reply);
    }
    let Action::Prepare { source_path } = &request.action else {
        unreachable!()
    };
    let input = FileRequest {
        api_version: request.api_version,
        request_id: request.request_id.clone(),
        instance_id: request.instance_id.clone(),
        source_path: source_path.clone(),
        project_only: true,
        cancelled_before_dispatch: false,
        replace: None,
    };
    // File I/O holds neither the native owner nor revision lock.
    let bytes = match read_file(state, &input, allowed) {
        Ok(bytes) => bytes,
        Err(mut failure) => {
            failure.details = Some(json!({"disposition":"notDispatched","retryExecution":false}));
            let reply = encode(Err(failure));
            let _ = with_owner(state, &request, |owner| {
                owner.finish_inactive_request(&request.request_id, &fingerprint, reply.clone())
            });
            return decode(reply);
        }
    };
    before_store();
    entered = false;
    let reply = with_owner(state, &request, |owner| {
        if !owner.pending_inactive_request(&request.request_id, &fingerprint) {
            return Err("inactive_indeterminate:inactive preparation is no longer pending".into());
        }
        entered = true;
        let result = owner
            .prepare_inactive_project(
                &request.instance_id,
                &request.document_id,
                request.expected_revision,
                &request.document_snapshot_id,
                &bytes,
            )
            .map(|project| receipt(&request, "inactive-prepared", Some(project)))
            .map_err(|e| error(e, "notDispatched"));
        after_effect.take().unwrap()();
        let reply = encode(result);
        owner.finish_inactive_request(&request.request_id, &fingerprint, reply.clone())?;
        Ok(reply)
    })
    .map_err(|e| {
        error(
            e,
            if entered {
                "indeterminate"
            } else {
                "notDispatched"
            },
        )
    })?;
    decode(reply)
}

/// #2269: a tab opened from a recent entry names a project file no dialog granted this session; admit that one file as
/// an MCP `project.open` does. A refused path stays refused by the route's own scope check.
fn admit_source<R: tauri::Runtime>(app: &tauri::AppHandle<R>, state: &ApplicationMcp, raw: &str) {
    if raw.len() > 4096 {
        return;
    }
    if let Ok(Request {
        instance_id,
        cancelled_before_dispatch: false,
        action: Action::Prepare { source_path },
        ..
    }) = serde_json::from_str::<Request>(raw)
    {
        if instance_id == state.instance_id() {
            crate::application_mcp::project_scope::admit_open_file(app, &source_path);
        }
    }
}

#[tauri::command]
pub(crate) async fn nemo_native_object_inactive_project(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request_json: String,
) -> HostResult<Value> {
    require_main(&window)?;
    admit_source(&app, &state, &request_json);
    if activation::is_request(&request_json) {
        return activation::execute(&app, &state, &request_json, window.label()).await;
    }
    request_with(
        &state,
        &request_json,
        window.label(),
        |path| {
            window
                .try_fs_scope()
                .is_some_and(|scope| scope.is_allowed(path))
        },
        || {},
        || {},
    )
}

#[cfg(test)]
#[path = "native_object_inactive_project_tests.rs"]
mod tests;
