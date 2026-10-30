//! Main-window project replacement in the original authority and object host.
use crate::native_application_contract::{host_error, HostResult};
use crate::native_dispatch::{
    NativeDispatch, NativeObjectHost, NativeState, ReplacementReplay, ReplacementStage,
};
use crate::native_object_bootstrap::ObjectBootstrapReceipt;
use native_engine::{history::PreparedProjectReplacement, resource_leases::WorkId};
use serde_json::json;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::native_object_bootstrap::admission::replay;
pub(super) use crate::native_object_bootstrap::admission::{prepare, Admission};

/// The callback owns its failure hook independently of the invoking waiter.
/// A discarded callback terminalizes the reserved authority even after cancel.
pub(super) fn submit(
    native: NativeState,
    generation: u64,
    request_id: String,
    fingerprint: Vec<u8>,
    prepared: PreparedProjectReplacement,
    schedule: impl FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), ()>,
    viewport_release: impl FnOnce() -> HostResult<(Vec<WorkId>, &'static str)> + Send + 'static,
) -> Result<
    tokio::sync::oneshot::Receiver<HostResult<ObjectBootstrapReceipt>>,
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
                prepared,
                viewport_release,
            )
        },
        move |_| {
            let _ = reconcile_executor(&native, generation, &request_id, &fingerprint);
        },
    )
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

#[cfg(test)]
#[path = "native_status_lifecycle_tests.rs"]
mod status_lifecycle_tests;

#[cfg(test)]
mod callback_tests {
    use super::tests::Fixture;
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn cpu_callback_owner_settles_without_waiter_and_preserves_committed_success() {
        // Rejected; queued then discarded; queued then completed after cancel;
        // completed synchronously despite a scheduler returning an error.
        for scenario in 0..4 {
            let f = Fixture::new();
            let (generation, id, fingerprint, prepared) = f.execute();
            let cleanup = Arc::new(AtomicUsize::new(0));
            let callback_cleanup = cleanup.clone();
            let mut queued = None;
            let waiter = submit(
                f.state.native_state(),
                generation,
                id.clone(),
                fingerprint.clone(),
                prepared,
                |callback| match scenario {
                    0 => {
                        drop(callback);
                        Err(())
                    }
                    3 => {
                        callback();
                        Err(())
                    }
                    _ => {
                        queued = Some(callback);
                        Ok(())
                    }
                },
                move || {
                    callback_cleanup.fetch_add(1, Ordering::SeqCst);
                    Ok((Vec::new(), "disposed"))
                },
            );
            assert_eq!(waiter.is_err(), matches!(scenario, 0 | 3));
            // No receiver remains to run any awaited reconciliation branch.
            drop(waiter);
            if scenario == 2 {
                queued.take().unwrap()();
            }
            drop(queued);
            assert_eq!(cleanup.load(Ordering::SeqCst), usize::from(scenario >= 2));
            let native = f.state.native_state();
            let authority = native.lock().unwrap();
            let retained = authority
                .lookup_replace_replay(&id, &fingerprint)
                .unwrap()
                .unwrap();
            if scenario < 2 {
                assert!(
                    matches!(retained, ReplacementReplay::Fenced(_)),
                    "callback loss must not remain pending"
                );
                assert!(authority.active().is_none());
            } else {
                assert!(
                    matches!(retained, ReplacementReplay::Succeeded(_)),
                    "committed success must not be overwritten by an executor error"
                );
                assert_eq!(authority.active_generation().unwrap(), generation);
                assert_ne!(
                    authority.active().unwrap().1.document_id(),
                    f.request["replace"]["documentId"].as_str().unwrap()
                );
            }
            drop(authority);
            let settled = reconcile_executor(&native, generation, &id, &fingerprint);
            if scenario < 2 {
                assert_eq!(settled.unwrap_err().code, "replacement_indeterminate");
                assert_eq!(
                    f.prepare(&f.request).err().unwrap().code,
                    "replacement_indeterminate"
                );
            } else {
                assert!(settled.is_ok());
                assert!(matches!(
                    f.prepare(&f.request).unwrap(),
                    Admission::Replay(_)
                ));
            }
        }
    }
}
