//! Main-thread callback ownership survives cancellation and a lost waiter.
use crate::native_application_contract::{host_error, HostResult};
use crate::native_dispatch::{
    NativeObjectHost, NativeState, PreparedRetainedActivation, ReplacementReplay,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
const MAX_SAFE: u64 = 9_007_199_254_740_991;
use crate::native_dispatch::ReplacementStage;
use native_engine::resource_leases::WorkId;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RetainedPins {
    pub(super) document_id: String,
    pub(super) content_revision: u64,
    pub(super) document_snapshot_id: String,
    pub(super) preparation_token: String,
    pub(super) preparation_request_id: String,
    pub(super) document_digest: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ActivationReceipt {
    pub(super) api_version: u32,
    pub(super) request_id: String,
    pub(super) instance_id: String,
    pub(super) document_id: String,
    pub(super) content_revision: u64,
    pub(super) document_snapshot_id: String,
    pub(super) lifecycle_generation: u64,
    pub(super) inactive_project: RetainedPins,
    pub(super) viewport_available: bool,
    pub(super) retry_execution: bool,
}

pub(super) fn replay(value: ReplacementReplay, request_id: &str) -> HostResult<Value> {
    match value {
        ReplacementReplay::Succeeded(value) => {
            let valid = serde_json::to_vec(&value).is_ok_and(|v| v.len() <= 4096)
                && serde_json::from_value::<ActivationReceipt>(value.clone()).is_ok_and(|v| {
                    v.api_version == 2
                        && v.request_id == request_id
                        && !v.retry_execution
                        && !v.viewport_available
                        && v.lifecycle_generation > 0
                        && v.lifecycle_generation <= MAX_SAFE
                });
            if valid {
                Ok(value)
            } else {
                Err(indeterminate("retained activation receipt is invalid"))
            }
        }
        ReplacementReplay::Pending(progress) | ReplacementReplay::Fenced(progress) => {
            let pending = progress.failure.is_none();
            let mut error = host_error(
                if pending {
                    "replacement_pending"
                } else {
                    "replacement_indeterminate"
                },
                "retained activation has not completed",
            );
            error.details = Some(
                json!({"disposition":if pending {"pending"}else{"indeterminate"},
                "retryExecution":false,"replacement":progress}),
            );
            Err(error)
        }
        ReplacementReplay::Expired => Err(indeterminate("retained activation receipt expired")),
    }
}
fn indeterminate(message: &str) -> nemo_mcp::contract::NativeApplicationError {
    let mut error = host_error("replacement_indeterminate", message);
    error.details = Some(json!({"disposition":"indeterminate","retryExecution":false}));
    error
}
pub(super) fn submit(
    native: NativeState,
    generation: u64,
    request_id: String,
    fingerprint: Vec<u8>,
    plan: PreparedRetainedActivation,
    receipt: Value,
    schedule: impl FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), ()>,
    viewport_release: impl FnOnce() -> HostResult<(Vec<WorkId>, &'static str)> + Send + 'static,
) -> Result<
    tokio::sync::oneshot::Receiver<HostResult<Value>>,
    crate::native_application_viewport::ReplacementCallbackFailure,
> {
    let callback_native = native.clone();
    let callback_id = request_id.clone();
    crate::native_application_viewport::submit_replacement(
        schedule,
        move || {
            complete(
                &callback_native,
                generation,
                &callback_id,
                plan,
                receipt,
                viewport_release,
            )
        },
        move |_| {
            let _ = reconcile(&native, generation, &request_id, &fingerprint);
        },
    )
}
pub(super) fn complete(
    native: &NativeState,
    generation: u64,
    request_id: &str,
    plan: PreparedRetainedActivation,
    receipt: Value,
    viewport_release: impl FnOnce() -> HostResult<(Vec<WorkId>, &'static str)>,
) -> HostResult<Value> {
    let mut authority = native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if authority.replacement_request_id(generation).as_deref() != Some(request_id) {
        return Err(indeterminate("retained callback identity changed"));
    }
    let outcome = {
        let (application, progress) = authority
            .replacing_mut(generation)
            .map_err(|_| indeterminate("retained callback is stale or fenced"))?;
        progress.callback = ReplacementStage::Running;
        catch_unwind(AssertUnwindSafe(|| -> HostResult<()> {
            let owner = application
                .as_any_mut()
                .downcast_mut::<NativeObjectHost>()
                .ok_or_else(|| host_error("unavailable", "original object host unavailable"))?;
            progress.preview = ReplacementStage::Running;
            progress.resources = ReplacementStage::Running;
            owner
                .activate_retained_project(plan, || {
                    // N2 calls this only after actual preview cleanup has completed.
                    progress.preview = ReplacementStage::Complete;
                    progress.viewport = ReplacementStage::Running;
                    let (_, status) =
                        viewport_release().map_err(|_| "native viewport release failed")?;
                    if !matches!(status, "disposed" | "already_absent") {
                        return Err("native viewport release incomplete".into());
                    }
                    progress.viewport = ReplacementStage::Complete;
                    progress.resources = ReplacementStage::Complete;
                    progress.core = ReplacementStage::Running;
                    Ok(())
                })
                .map_err(|_| {
                    host_error("cleanup_failed", "retained activation did not complete")
                })?;
            // N2 returned success only AFTER moving the exact checked histories.
            // No fallible serialization or metadata query follows that move.
            progress.core = ReplacementStage::Complete;
            progress.callback = ReplacementStage::Complete;
            Ok(())
        }))
    };
    match outcome {
        Ok(Ok(())) => {
            authority
                .activate_replace(generation)
                .map_err(|_| indeterminate("retained activation publication is uncertain"))?;
            authority.record_replace_success(generation, receipt.clone());
            Ok(receipt)
        }
        Ok(Err(_)) | Err(_) => {
            let progress = authority
                .fence_replace(
                    generation,
                    "callback_failed",
                    "retained activation cleanup or callback failed",
                )
                .map_err(|_| indeterminate("retained activation fence unavailable"))?;
            replay(ReplacementReplay::Fenced(progress), request_id)
        }
    }
}
pub(super) fn reconcile(
    native: &NativeState,
    generation: u64,
    request_id: &str,
    fingerprint: &[u8],
) -> HostResult<Value> {
    let mut authority = native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match authority.lookup_replace_replay(request_id, fingerprint) {
        Ok(Some(value @ (ReplacementReplay::Succeeded(_) | ReplacementReplay::Fenced(_)))) => {
            return replay(value, request_id);
        }
        Err(_) => return Err(indeterminate("retained callback correlation changed")),
        _ => (),
    }
    let progress = authority
        .fence_replace(
            generation,
            "callback_unavailable",
            "retained activation callback did not complete",
        )
        .map_err(|_| indeterminate("retained callback requires reconciliation"))?;
    replay(ReplacementReplay::Fenced(progress), request_id)
}
