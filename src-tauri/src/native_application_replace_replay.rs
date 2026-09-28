//! Request-identity admission and non-reexecuting host replacement retrieval.

use super::*;
use crate::native_dispatch::{ReplacementAdmission, ReplacementIdentity, ReplacementReplay};

pub(super) fn replay_result(replay: ReplacementReplay) -> HostResult<NativeReplacementReceipt> {
    match replay {
        ReplacementReplay::Pending(progress) => {
            let mut error =
                host_error("replacement_pending", "native replacement is still pending");
            error.details = Some(serde_json::json!({
                "disposition": "pending", "retryExecution": false, "replacement": progress,
            }));
            Err(error)
        }
        ReplacementReplay::Succeeded(value) => {
            let mut receipt: NativeReplacementReceipt =
                serde_json::from_value(value).map_err(|_| {
                    host_error("unavailable", "retained replacement receipt is invalid")
                })?;
            receipt.retrieved = true;
            Ok(receipt)
        }
        ReplacementReplay::Fenced(progress) => Err(replace_commands::fenced_error(progress)),
        ReplacementReplay::Expired => {
            let mut error = host_error(
                "replacement_expired",
                "native replacement receipt expired after release",
            );
            error.details =
                Some(serde_json::json!({"disposition":"expired", "retryExecution":false}));
            Err(error)
        }
    }
}

pub(super) async fn replace(
    app: &tauri::AppHandle,
    state: &ApplicationMcp,
    request: NativeReplacementRequest,
) -> HostResult<NativeReplacementReceipt> {
    let fingerprint = replacement_fingerprint(&request)?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let native = state.native_state();
    let lookup = {
        let authority = native
            .lock()
            .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
        authority.lookup_replace_replay(&request.request_id, &fingerprint)
    };
    if let Some(replay) =
        lookup.map_err(|message| replace_commands::admission_error(&native, message))?
    {
        return replay_result(replay);
    }
    let admitted = admit_project(&request.projection, &request.resources)?;
    let prepared = DesktopNativeApplication::prepare_replacement(admitted)?;
    let admission = state
        .reserve_native_replacement(
            ReplacementIdentity {
                request_id: &request.request_id,
                fingerprint: &fingerprint,
                instance_id: &request.instance_id,
                document_id: &request.document_id,
                expected_revision: request.expected_revision,
            },
            |application| {
                if application.as_any_mut().is::<DesktopNativeApplication>() {
                    Ok(())
                } else {
                    Err("unavailable:native desktop host is unavailable".into())
                }
            },
        )
        .map_err(|message| replace_commands::admission_error(&native, message))?;
    let ReplacementAdmission::Execute(generation) = admission else {
        let ReplacementAdmission::Replay(replay) = admission else {
            unreachable!()
        };
        return replay_result(replay);
    };
    let instance = request.instance_id;
    let committed = native.clone();
    let fallback = native.clone();
    match viewport_host::on_main_thread_replacement(
        app,
        move || replace_commands::complete_replacement(&committed, generation, &instance, prepared),
        move |failure| {
            let _ = replace_commands::fence_executor_failure(&fallback, generation, failure);
        },
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(replace_commands::retained_executor_error(
            &native, generation,
        )),
    }
}
