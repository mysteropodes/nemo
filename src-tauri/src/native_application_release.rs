//! Terminal release admission and fail-closed host reconciliation.

use crate::{
    native_application::DesktopNativeApplication,
    native_application_contract::*,
    native_dispatch::{
        NativeAuthority, NativePhase, NativeState, ReleaseAdmission, ReleaseTombstone,
    },
};
use native_engine::{export_job::ReconciliationStage, resource_leases::WorkId};
use std::collections::BTreeSet;

fn catch_unwind_message<T>(operation: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("native release cleanup panicked")
            .to_string()
    })
}

pub(crate) fn admit_release_request(
    native: &NativeState,
    request: &NativeReleaseRequest,
) -> HostResult<ReleaseAdmission> {
    let fingerprint = release_fingerprint(request)?;
    native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .admit_release(
            &request.request_id,
            &request.instance_id,
            &request.document_id,
            request.expected_revision,
            &fingerprint,
            request.cancelled_before_dispatch,
        )
        .map_err(|message| {
            let (code, message) = message
                .split_once(':')
                .unwrap_or(("internal", message.as_str()));
            host_error(code, message)
        })
}

pub(crate) fn complete_release<F>(
    native: &NativeState,
    generation: u64,
    request: &NativeReleaseRequest,
    viewport_release: F,
) -> HostResult<NativeReleaseReceipt>
where
    F: FnOnce() -> HostResult<(Vec<WorkId>, &'static str)>,
{
    let (mut authority, authority_poisoned) = match native.lock() {
        Ok(authority) => (authority, false),
        Err(poisoned) => (poisoned.into_inner(), true),
    };
    let phase = std::mem::replace(&mut authority.phase, NativePhase::Vacant);
    let NativePhase::Releasing {
        generation: active_generation,
        request_id: retained_request_id,
        fingerprint,
        application,
    } = phase
    else {
        authority.phase = phase;
        return Err(host_error("unavailable", "stale native release generation"));
    };
    if active_generation != generation || retained_request_id != request.request_id {
        authority.phase = NativePhase::Releasing {
            generation: active_generation,
            request_id: retained_request_id,
            fingerprint,
            application,
        };
        return Err(host_error("unavailable", "stale native release generation"));
    }
    let Some(mut application) = application else {
        return Ok(retain_failed_release(
            &mut authority,
            generation,
            retained_request_id,
            fingerprint,
            (
                request.instance_id.clone(),
                request.document_id.clone(),
                request.expected_revision,
            ),
            "native release application was unavailable during cleanup",
        ));
    };
    let retained_identity = (
        application.instance_id().to_owned(),
        application.document_id().to_owned(),
        application.content_revision(),
    );
    let Some(desktop) = application
        .as_any_mut()
        .downcast_mut::<DesktopNativeApplication>()
    else {
        drop(application);
        return Ok(retain_failed_release(
            &mut authority,
            generation,
            retained_request_id,
            fingerprint,
            retained_identity,
            "native desktop host was unavailable during cleanup",
        ));
    };
    let (mut released, core_error) = match catch_unwind_message(|| desktop.release_project()) {
        Ok(released) => (released, None),
        Err(message) => {
            let Some(released) = desktop.release_progress() else {
                drop(application);
                return Ok(retain_failed_release(
                    &mut authority,
                    generation,
                    retained_request_id,
                    fingerprint,
                    retained_identity,
                    &message,
                ));
            };
            (released, Some(host_error("cleanup_failed", message)))
        }
    };
    if core_error.is_some() {
        if released.application.transaction_stage == ReconciliationStage::Pending {
            released.application.transaction_stage = ReconciliationStage::Unknown;
        }
        if released.application.export_stage == ReconciliationStage::Pending {
            released.application.export_stage = ReconciliationStage::Unknown;
        }
    }
    let viewport = core_error.clone().map_or_else(
        || {
            catch_unwind_message(viewport_release)
                .unwrap_or_else(|message| Err(host_error("cleanup_failed", message)))
        },
        Err,
    );
    drop(application);
    let mut cleanup_error = authority_poisoned
        .then(|| host_error("cleanup_failed", "native authority lock was poisoned"))
        .or(core_error)
        .or_else(|| {
            released
                .preview_error
                .map(|message| host_error("cleanup_failed", message))
        })
        .or_else(|| viewport.as_ref().err().cloned());
    if cleanup_error.is_none() && !released.application.cleanup_complete() {
        cleanup_error = Some(host_error(
            "cleanup_failed",
            "native export cleanup remained indeterminate",
        ));
    }
    let cleanup_complete = released.application.cleanup_complete()
        && released.preview_stage == ReconciliationStage::Complete
        && cleanup_error.is_none();
    let mut preview_ids: BTreeSet<WorkId> = released.cancelled_preview.into_iter().collect();
    if let Ok(viewport) = &viewport {
        preview_ids.extend(viewport.0.iter().copied());
    }
    let receipt = NativeReleaseReceipt {
        api_version: HOST_API_VERSION,
        request_id: request.request_id.clone(),
        instance_id: released.application.instance_id,
        document_id: released.application.document_id,
        content_revision: released.application.content_revision,
        lifecycle_generation: generation,
        status: if cleanup_complete {
            "succeeded"
        } else {
            "indeterminate"
        }
        .into(),
        retrieved: false,
        authority_removal_completed: true,
        cancelled_transaction_id: released.application.cancelled_transaction_id,
        cancelled_transaction: released.application.cancelled_transaction,
        undo_depth: released.application.undo_depth,
        redo_depth: released.application.redo_depth,
        reconciled_exports: released
            .application
            .exports
            .receipts
            .iter()
            .map(reconcile_export)
            .collect(),
        cancelled_preview_work_ids: preview_ids.into_iter().map(work_label).collect(),
        unresolved_preview_work_ids: Some(
            released
                .unresolved_preview
                .into_iter()
                .map(work_label)
                .collect(),
        ),
        reconciliation_stages: serde_json::json!({
            "transaction": released.application.transaction_stage.label(),
            "exports": released.application.export_stage.label(),
            "preview": released.preview_stage.label(),
        }),
        viewport_status: viewport
            .as_ref()
            .map(|value| value.1)
            .unwrap_or("cleanup_failed")
            .into(),
        reentry_available: cleanup_complete,
        error: cleanup_error,
    };
    authority.finish_release(ReleaseTombstone {
        generation,
        request_id: request.request_id.clone(),
        fingerprint,
        receipt: receipt.retained_value(),
        succeeded: cleanup_complete,
    });
    Ok(receipt)
}

fn retain_failed_release(
    authority: &mut NativeAuthority,
    generation: u64,
    request_id: String,
    fingerprint: Vec<u8>,
    identity: (String, String, u64),
    message: &str,
) -> NativeReleaseReceipt {
    let receipt =
        NativeReleaseReceipt::indeterminate(request_id.clone(), identity, generation, message);
    authority.finish_release(ReleaseTombstone {
        generation,
        request_id,
        fingerprint,
        receipt: receipt.retained_value(),
        succeeded: false,
    });
    receipt
}

#[cfg(test)]
mod tests {
    use super::super::release_tests::{
        admit_generation, begin_update, desktop, dispatch_history, install, release_request,
        Scratch,
    };
    use super::*;
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[test]
    fn core_panic_retains_completed_stage_and_marks_unreconciled_stages_unknown() {
        let scratch = Scratch::new();
        let mut application = desktop(&scratch, None);
        let document = application.document_id().to_owned();
        let committed = begin_update(&mut application, "panic-committed", 41);
        assert_eq!(
            dispatch_history(
                &mut application,
                "panic-commit",
                "transaction.commit",
                json!({"transactionId": committed}),
            )["contentRevision"],
            1
        );
        let working = begin_update(&mut application, "panic-working", 73);
        application.inject_release_panic_after_transaction();
        let (native, _) = install(application);
        let mut request = release_request(&document, "release-core-panic");
        request.expected_revision = 1;
        let generation = admit_generation(&native, &request);
        let viewport_called = Arc::new(AtomicBool::new(false));
        let called = Arc::clone(&viewport_called);
        let receipt = complete_release(&native, generation, &request, move || {
            called.store(true, Ordering::SeqCst);
            Ok((Vec::new(), "already_absent"))
        })
        .unwrap();
        assert!(!viewport_called.load(Ordering::SeqCst));
        assert_eq!(
            receipt.error.as_ref().unwrap().message,
            "injected core release panic after transaction reconciliation"
        );
        assert_eq!(
            receipt.reconciliation_stages,
            json!({"transaction":"complete","exports":"unknown","preview":"unknown"})
        );
        assert_eq!((receipt.content_revision, receipt.undo_depth), (1, 1));
        assert_eq!(
            receipt.cancelled_transaction_id.as_deref(),
            Some(working.as_str())
        );
        assert_eq!(
            receipt.cancelled_transaction.as_ref().unwrap()["terminalDisposition"],
            "cancelled"
        );
        assert!(native.lock().unwrap().reserve_install().is_err());
    }

    #[test]
    fn export_panic_retains_completed_and_unresolved_job_evidence() {
        let scratch = Scratch::new();
        let mut application = desktop(&scratch, Some(scratch.0.join("partial-output")));
        let document = application.document_id().to_owned();
        let begin = |application: &mut DesktopNativeApplication, request_id: &str| {
            dispatch_history(
                application,
                request_id,
                "job.export.png.begin",
                json!({"contextId":"scene-root","quality":"final","outputHandle":"output","frames":[
                    {"sourceFrame":0,"geometryHandle":{"resourceId":"geometry","resourceVersion":"v1"}}
                ]}),
            )["result"]["jobId"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let unresolved = begin(&mut application, "unresolved-export");
        let stage = std::fs::read_dir(scratch.0.join("staging"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let blocker = stage.join("cleanup-blocker");
        std::fs::write(&blocker, b"retain stage").unwrap();
        let failed = dispatch_history(
            &mut application,
            "cancel-unresolved",
            "job.export.png.cancel",
            json!({"jobId": unresolved}),
        );
        assert_eq!(failed["result"]["cleanup"]["status"], "failed");
        std::fs::remove_file(blocker).unwrap();
        let completed = begin(&mut application, "completed-during-release");
        application.inject_release_panic_after_export_jobs(1);
        let (native, _) = install(application);
        let request = release_request(&document, "release-partial-export-panic");
        let generation = admit_generation(&native, &request);
        let viewport_called = Arc::new(AtomicBool::new(false));
        let called = Arc::clone(&viewport_called);
        let receipt = complete_release(&native, generation, &request, move || {
            called.store(true, Ordering::SeqCst);
            Ok((Vec::new(), "already_absent"))
        })
        .unwrap();
        assert!(!viewport_called.load(Ordering::SeqCst));
        assert_eq!(
            receipt.error.as_ref().unwrap().message,
            "injected core release panic after 1 export job"
        );
        assert_eq!(receipt.reconciliation_stages["exports"], "unknown");
        let completed = receipt
            .reconciled_exports
            .iter()
            .find(|job| job.job_id == completed)
            .unwrap();
        assert_eq!(
            (completed.status.as_str(), completed.cleanup_status.as_str()),
            ("cancelled", "complete")
        );
        let unresolved = receipt
            .reconciled_exports
            .iter()
            .find(|job| job.job_id == unresolved)
            .unwrap();
        assert_eq!(
            (
                unresolved.status.as_str(),
                unresolved.cleanup_status.as_str()
            ),
            ("failed", "failed")
        );
        let ReleaseAdmission::Retry { receipt: retry, .. } =
            admit_release_request(&native, &request).unwrap()
        else {
            panic!("release receipt was not retained")
        };
        let mut retry: NativeReleaseReceipt = serde_json::from_value(retry).unwrap();
        assert!(retry.retrieved);
        retry.retrieved = false;
        assert_eq!(retry, receipt);
        assert!(native.lock().unwrap().reserve_install().is_err());
    }

    #[test]
    fn preview_panic_separates_cancelled_from_unresolved_work() {
        let scratch = Scratch::new();
        let mut application = desktop(&scratch, None);
        let document = application.document_id().to_owned();
        let instance = application.instance_id().to_owned();
        let snapshot = application.core.acquire_snapshot(0).unwrap();
        for frame in [0, 1] {
            let request: NativePreviewRequest = serde_json::from_value(json!({
                "apiVersion": HOST_API_VERSION, "instanceId": instance,
                "documentId": document, "contentRevision": 0,
                "documentSnapshotId": snapshot.id(), "contextId": "scene-root",
                "frame": frame, "quality": "final",
                "outputSpec": {"kind":"frame","format":"rgba8","width":320,"height":180,"colorInterpretation":"srgb","alphaMode":"straight"},
                "geometryHandle": {"resourceId":"geometry","resourceVersion":"v1"}
            })).unwrap();
            application.prepare_preview(&request).unwrap();
        }
        application.inject_release_panic_after_preview_jobs(1);
        let (native, _) = install(application);
        let request = release_request(&document, "release-partial-preview-panic");
        let generation = admit_generation(&native, &request);
        let viewport_called = Arc::new(AtomicBool::new(false));
        let called = Arc::clone(&viewport_called);
        let receipt = complete_release(&native, generation, &request, move || {
            called.store(true, Ordering::SeqCst);
            Ok((Vec::new(), "already_absent"))
        })
        .unwrap();
        assert!(!viewport_called.load(Ordering::SeqCst));
        assert_eq!(
            receipt.error.as_ref().unwrap().message,
            "injected core release panic after 1 preview job"
        );
        assert_eq!(receipt.cancelled_preview_work_ids.len(), 1);
        assert_eq!(
            receipt.unresolved_preview_work_ids.as_ref().unwrap().len(),
            1
        );
        assert_ne!(
            receipt.cancelled_preview_work_ids,
            *receipt.unresolved_preview_work_ids.as_ref().unwrap()
        );
        let ReleaseAdmission::Retry { receipt: retry, .. } =
            admit_release_request(&native, &request).unwrap()
        else {
            panic!("release receipt was not retained")
        };
        let mut retry: NativeReleaseReceipt = serde_json::from_value(retry).unwrap();
        retry.retrieved = false;
        assert_eq!(retry, receipt);
        assert!(native.lock().unwrap().reserve_install().is_err());
    }
}
