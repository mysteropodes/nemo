//! Main-window project replacement in the original authority and object host.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, HostResult,
};
use crate::native_dispatch::{
    NativeAuthority, NativeDispatch, NativeObjectHost, NativeState, ReplacementAdmission,
    ReplacementIdentity, ReplacementReplay, ReplacementStage,
};
use crate::native_object_bootstrap::{
    reopen::{self, FileRequest},
    ObjectBootstrapReceipt,
};
use native_engine::{history::PreparedProjectReplacement, resource_leases::WorkId};
use nemo_mcp::contract::NativeApplicationError;
use serde_json::json;
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
};

pub(super) enum Admission {
    Replay(ObjectBootstrapReceipt),
    Execute {
        generation: u64,
        request_id: String,
        fingerprint: Vec<u8>,
        prepared: PreparedProjectReplacement,
    },
}
fn refused(error: NativeApplicationError) -> NativeApplicationError {
    let mut result = host_error("replacement_refused", error.message);
    result.details =
        Some(json!({"disposition":"notDispatched","retryExecution":false,"reasonCode":error.code}));
    result
}
fn source_error(message: String) -> NativeApplicationError {
    let (code, message) = message.split_once(':').unwrap_or(("unavailable", &message));
    refused(host_error(code, message))
}
fn replay(value: ReplacementReplay) -> HostResult<ObjectBootstrapReceipt> {
    match value {
        ReplacementReplay::Succeeded(value) => serde_json::from_value(value).map_err(|_| {
            host_error(
                "replacement_indeterminate",
                "retained replacement receipt is invalid",
            )
        }),
        ReplacementReplay::Pending(progress) | ReplacementReplay::Fenced(progress) => {
            let pending = progress.failure.is_none();
            let mut error = host_error(
                if pending {
                    "replacement_pending"
                } else {
                    "replacement_indeterminate"
                },
                "native replacement has not completed",
            );
            error.details = Some(
                json!({"disposition":if pending {"pending"}else{"indeterminate"},
                "retryExecution":false,"replacement":progress}),
            );
            Err(error)
        }
        ReplacementReplay::Expired => {
            let mut error = host_error(
                "replacement_indeterminate",
                "native replacement receipt expired",
            );
            error.details = Some(json!({"disposition":"indeterminate","retryExecution":false}));
            Err(error)
        }
    }
}
fn current_owner<'a>(
    authority: &'a mut NativeAuthority,
    request: &FileRequest,
) -> HostResult<&'a mut NativeObjectHost> {
    let pins = request
        .replace
        .as_ref()
        .ok_or_else(|| host_error("invalid_request", "replacement pins missing"))?;
    let application = authority
        .active_mut(pins.lifecycle_generation)
        .map_err(|_| host_error("stale_revision", "native lifecycle generation changed"))?;
    if application.instance_id() != request.instance_id {
        return Err(host_error("wrong_instance", "native instance changed"));
    }
    if application.document_id() != pins.document_id {
        return Err(host_error("wrong_document", "native document changed"));
    }
    if application.content_revision() != pins.expected_revision {
        return Err(host_error("stale_revision", "native revision changed"));
    }
    let owner = application
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .ok_or_else(|| host_error("unavailable", "native object project owner unavailable"))?;
    if !owner.matches_project_snapshot(&pins.document_snapshot_id) {
        return Err(host_error(
            "stale_revision",
            "native project snapshot changed",
        ));
    }
    Ok(owner)
}

pub(super) fn prepare(
    state: &ApplicationMcp,
    request: FileRequest,
    allowed: impl Fn(&Path) -> bool,
) -> HostResult<Admission> {
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )
    .map_err(refused)?;
    let pins = request
        .replace
        .as_ref()
        .ok_or_else(|| refused(host_error("invalid_request", "replacement pins missing")))?;
    if !request.project_only
        || !bounded_id(&request.request_id)
        || !bounded_id(&pins.document_id)
        || !bounded_id(&pins.document_snapshot_id)
        || pins.expected_revision > 9_007_199_254_740_991
        || pins.lifecycle_generation == 0
        || pins.lifecycle_generation > 9_007_199_254_740_991
    {
        return Err(refused(host_error(
            "invalid_request",
            "invalid project replacement request",
        )));
    }
    let fingerprint = serde_json::to_vec(&request)
        .map_err(|_| refused(host_error("invalid_request", "invalid request identity")))?;
    if fingerprint.len() > 4096 {
        return Err(refused(host_error(
            "invalid_request",
            "replacement request exceeds byte limit",
        )));
    }
    let native = state.native_state();
    {
        let mut authority = native.lock().map_err(|_| {
            refused(host_error(
                "unavailable",
                "native authority lock unavailable",
            ))
        })?;
        if let Some(value) = authority
            .lookup_replace_replay(&request.request_id, &fingerprint)
            .map_err(source_error)?
        {
            return replay(value).map(Admission::Replay);
        }
        if request.cancelled_before_dispatch {
            return Err(refused(host_error(
                "cancelled_before_dispatch",
                "native replacement cancelled",
            )));
        }
        current_owner(&mut authority, &request).map_err(refused)?;
    }
    let bytes = reopen::read_file(state, &request, allowed).map_err(refused)?;
    let prepared = {
        let mut authority = native.lock().map_err(|_| {
            refused(host_error(
                "unavailable",
                "native authority lock unavailable",
            ))
        })?;
        current_owner(&mut authority, &request)
            .map_err(refused)?
            .prepare_project_replacement(&pins.document_snapshot_id, &bytes)
            .map_err(|_| {
                refused(host_error(
                    "invalid_request",
                    "native project replacement file refused",
                ))
            })?
    };
    drop(bytes);
    let admission = state
        .reserve_native_replacement_at_generation(
            pins.lifecycle_generation,
            ReplacementIdentity {
                request_id: &request.request_id,
                fingerprint: &fingerprint,
                instance_id: &request.instance_id,
                document_id: &pins.document_id,
                expected_revision: pins.expected_revision,
            },
            |application| {
                application
                    .as_any_mut()
                    .downcast_mut::<NativeObjectHost>()
                    .ok_or_else(|| "native project owner changed".to_string())?
                    .validate_project_replacement(&prepared)
            },
        )
        .map_err(source_error)?;
    match admission {
        ReplacementAdmission::Replay(value) => replay(value).map(Admission::Replay),
        ReplacementAdmission::Execute(generation) => Ok(Admission::Execute {
            generation,
            request_id: request.request_id,
            fingerprint,
            prepared,
        }),
    }
}

pub(super) fn complete(
    native: &NativeState,
    generation: u64,
    request_id: &str,
    prepared: PreparedProjectReplacement,
    viewport_release: impl FnOnce() -> HostResult<(Vec<WorkId>, &'static str)>,
) -> HostResult<ObjectBootstrapReceipt> {
    let mut authority = native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if authority.replacement_request_id(generation).as_deref() != Some(request_id) {
        return Err(host_error(
            "replacement_indeterminate",
            "replacement callback identity changed",
        ));
    }
    let result = {
        let (application, progress) = authority.replacing_mut(generation).map_err(|_| {
            host_error(
                "replacement_indeterminate",
                "replacement is stale or fenced",
            )
        })?;
        progress.callback = ReplacementStage::Running;
        catch_unwind(AssertUnwindSafe(
            || -> HostResult<ObjectBootstrapReceipt> {
                let owner = application
                    .as_any_mut()
                    .downcast_mut::<NativeObjectHost>()
                    .ok_or_else(|| host_error("unavailable", "original object host unavailable"))?;
                owner
                    .validate_project_replacement(&prepared)
                    .map_err(|_| host_error("stale_revision", "replacement origin changed"))?;
                progress.preview = ReplacementStage::Running;
                progress.resources = ReplacementStage::Running;
                let released = owner
                    .release_project()
                    .map_err(|_| host_error("cleanup_failed", "native project cleanup failed"))?;
                if !released.reconciliation_complete() {
                    return Err(host_error(
                        "cleanup_failed",
                        "native project cleanup incomplete",
                    ));
                }
                progress.preview = ReplacementStage::Complete;
                progress.viewport = ReplacementStage::Running;
                let (_, status) = viewport_release()?;
                if !matches!(status, "disposed" | "already_absent") {
                    return Err(host_error(
                        "cleanup_failed",
                        "native viewport cleanup incomplete",
                    ));
                }
                progress.viewport = ReplacementStage::Complete;
                progress.resources = ReplacementStage::Complete;
                progress.core = ReplacementStage::Running;
                owner
                    .commit_project_replacement(prepared)
                    .map_err(|_| host_error("unavailable", "native replacement commit failed"))?;
                progress.core = ReplacementStage::Complete;
                let info = owner.project_file_info().map_err(|_| {
                    host_error("unavailable", "native replacement metadata unavailable")
                })?;
                Ok(ObjectBootstrapReceipt {
                    api_version: 2,
                    request_id: request_id.into(),
                    instance_id: owner.instance_id().into(),
                    document_id: owner.document_id().into(),
                    content_revision: owner.content_revision(),
                    resource_count: usize::from(!info["reference"].is_null()),
                    viewport_available: false,
                })
            },
        ))
    };
    match result {
        Ok(Ok(receipt)) => {
            let (_, progress) = authority.replacing_mut(generation).map_err(|_| {
                host_error(
                    "replacement_indeterminate",
                    "replacement completion became stale",
                )
            })?;
            progress.callback = ReplacementStage::Complete;
            authority.activate_replace(generation).map_err(|_| {
                host_error(
                    "replacement_indeterminate",
                    "replacement activation refused",
                )
            })?;
            authority.record_replace_success(
                generation,
                serde_json::to_value(&receipt).expect("bounded replacement receipt"),
            );
            Ok(receipt)
        }
        Ok(Err(_)) | Err(_) => {
            let progress = authority
                .fence_replace(
                    generation,
                    "callback_failed",
                    "native replacement cleanup or commit failed",
                )
                .map_err(|_| {
                    host_error(
                        "replacement_indeterminate",
                        "native replacement fence unavailable",
                    )
                })?;
            replay(ReplacementReplay::Fenced(progress))
        }
    }
}

pub(super) fn reconcile_executor(
    native: &NativeState,
    generation: u64,
    request_id: &str,
    fingerprint: &[u8],
) -> HostResult<ObjectBootstrapReceipt> {
    let mut authority = native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Ok(Some(value @ (ReplacementReplay::Succeeded(_) | ReplacementReplay::Fenced(_)))) =
        authority.lookup_replace_replay(request_id, fingerprint)
    {
        return replay(value);
    }
    let progress = authority.fence_replace(
        generation,
        "callback_unavailable",
        "native replacement callback did not complete",
    );
    match progress {
        Ok(progress) => replay(ReplacementReplay::Fenced(progress)),
        Err(_) => {
            let mut error = host_error(
                "replacement_indeterminate",
                "native replacement outcome requires reconciliation",
            );
            error.details = Some(json!({"disposition":"indeterminate","retryExecution":false}));
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "native_object_project_replace_tests.rs"]
mod tests;
