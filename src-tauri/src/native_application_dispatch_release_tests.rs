//! Generic cleanup through the production host, with no object activation.
use crate::{
    native_application::{admit_release_request, complete_release},
    native_application_contract::{NativeReleaseReceipt, NativeReleaseRequest, HOST_API_VERSION},
    native_dispatch::{NativeAuthority, NativePhase, NativeState, ReleaseAdmission},
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const INSTANCE: &str = "native-release-fixture";

fn release_request(document_id: &str, request_id: &str) -> NativeReleaseRequest {
    NativeReleaseRequest {
        api_version: HOST_API_VERSION,
        request_id: request_id.into(),
        instance_id: INSTANCE.into(),
        document_id: document_id.into(),
        expected_revision: 0,
        cancelled_before_dispatch: false,
    }
}

fn admit_and_complete(
    native: &NativeState,
    request: &NativeReleaseRequest,
) -> NativeReleaseReceipt {
    let generation = admit_generation(native, request);
    complete_release(native, generation, request, || {
        Ok((Vec::new(), "already_absent"))
    })
    .unwrap()
}

fn admit_generation(native: &NativeState, request: &NativeReleaseRequest) -> u64 {
    match admit_release_request(native, request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        ReleaseAdmission::Retry { .. } => panic!("first release unexpectedly retried"),
    }
}

fn retrieved_receipt(value: Value) -> NativeReleaseReceipt {
    let mut receipt: NativeReleaseReceipt = serde_json::from_value(value).unwrap();
    assert!(receipt.retrieved);
    receipt.retrieved = false;
    receipt
}

// This owner is intentionally not DesktopNativeApplication or NativeApplication.
// Its synthetic progress verifies the host protocol, not object installation.
#[derive(Clone, Copy)]
enum CleanupMode {
    Complete,
    Unsupported,
    PanicBefore,
    PanicAfter,
    ProgressPanic,
    TransactionUnknown,
    ExportsUnknown,
    PreviewUnknown,
    WrongIdentity,
}
struct CleanupOwner {
    calls: Arc<std::sync::atomic::AtomicUsize>,
    mode: CleanupMode,
    started: bool,
}
impl CleanupOwner {
    fn evidence(&self) -> crate::native_dispatch::NativeReleaseProgress {
        use native_engine::{
            application::ApplicationReleaseReceipt,
            export_job::{ExportReleaseReconciliation, ReconciliationStage as Stage},
        };
        let partial = matches!(self.mode, CleanupMode::PanicAfter);
        crate::native_dispatch::NativeReleaseProgress {
            application: ApplicationReleaseReceipt {
                instance_id: if matches!(self.mode, CleanupMode::WrongIdentity) {
                    "wrong"
                } else {
                    INSTANCE
                }
                .into(),
                document_id: "generic-document".into(),
                content_revision: 7,
                cancelled_transaction_id: Some("generic-transaction".into()),
                cancelled_transaction: Some(json!({"workingState":{"value":41}})),
                undo_depth: 3,
                redo_depth: 2,
                transaction_stage: if matches!(self.mode, CleanupMode::TransactionUnknown) {
                    Stage::Unknown
                } else {
                    Stage::Complete
                },
                export_stage: if partial {
                    Stage::Pending
                } else if matches!(self.mode, CleanupMode::ExportsUnknown) {
                    Stage::Unknown
                } else {
                    Stage::Complete
                },
                exports: ExportReleaseReconciliation {
                    receipts: Vec::new(),
                    cleanup_complete: !partial,
                },
            },
            cancelled_preview: Vec::new(),
            unresolved_preview: Vec::new(),
            preview_error: None,
            preview_stage: if partial {
                Stage::Pending
            } else if matches!(self.mode, CleanupMode::PreviewUnknown) {
                Stage::Unknown
            } else {
                Stage::Complete
            },
        }
    }
}
impl crate::native_dispatch::NativeDispatch for CleanupOwner {
    fn instance_id(&self) -> &str {
        INSTANCE
    }
    fn document_id(&self) -> &str {
        "generic-document"
    }
    fn content_revision(&self) -> u64 {
        7
    }
    fn dispatch(
        &mut self,
        _: native_engine::commands::OpacityRequest,
    ) -> native_engine::commands::ResponseEnvelope {
        unreachable!()
    }
    fn replace_document(
        &mut self,
        _: native_engine::document::OpacityDocument,
    ) -> Result<Vec<native_engine::export_job::JobReceipt>, String> {
        unreachable!()
    }
    fn start_next_export_frame(
        &mut self,
        _: &str,
    ) -> Result<Option<native_engine::export_job::PendingFrame>, String> {
        unreachable!()
    }
    fn finish_export_frame(
        &mut self,
        _: native_engine::export_job::PendingFrame,
    ) -> Result<native_engine::export_job::JobReceipt, String> {
        unreachable!()
    }
    fn release_project(&mut self) -> Result<crate::native_dispatch::NativeReleaseProgress, String> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.started = true;
        match self.mode {
            CleanupMode::Unsupported => Err("unsupported cleanup".into()),
            CleanupMode::PanicBefore | CleanupMode::ProgressPanic => panic!("before progress"),
            CleanupMode::PanicAfter => panic!("after transaction"),
            _ => Ok(self.evidence()),
        }
    }
    fn release_progress(&self) -> Option<crate::native_dispatch::NativeReleaseProgress> {
        if matches!(self.mode, CleanupMode::ProgressPanic) {
            panic!("progress unavailable");
        }
        (self.started && matches!(self.mode, CleanupMode::PanicAfter)).then(|| self.evidence())
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
fn generic_owner(
    mode: CleanupMode,
) -> (
    NativeState,
    NativeReleaseRequest,
    Arc<std::sync::atomic::AtomicUsize>,
    u64,
) {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut authority = NativeAuthority::default();
    let generation = authority.reserve_install().unwrap();
    authority
        .install(
            generation,
            Box::new(CleanupOwner {
                calls: calls.clone(),
                mode,
                started: false,
            }),
        )
        .unwrap();
    let mut request = release_request("generic-document", "generic-release");
    request.expected_revision = 7;
    (Arc::new(Mutex::new(authority)), request, calls, generation)
}
fn assert_generic_identity(receipt: &NativeReleaseReceipt) {
    assert_eq!(
        (
            receipt.instance_id.as_str(),
            receipt.document_id.as_str(),
            receipt.content_revision
        ),
        (INSTANCE, "generic-document", 7)
    );
    assert!(receipt.authority_removal_completed);
}
#[test]
fn non_desktop_owner_releases_through_the_host_and_replays_once() {
    use std::sync::atomic::Ordering;
    let (native, request, calls, original_generation) = generic_owner(CleanupMode::Complete);
    let receipt = admit_and_complete(&native, &request);
    assert_generic_identity(&receipt);
    assert_eq!(receipt.status, "succeeded");
    assert!(receipt.reentry_available);
    assert!(receipt.error.is_none());
    assert_eq!(
        receipt.cancelled_transaction_id.as_deref(),
        Some("generic-transaction")
    );
    assert_eq!(
        receipt.cancelled_transaction,
        Some(json!({"workingState":{"value":41}}))
    );
    assert_eq!((receipt.undo_depth, receipt.redo_depth), (3, 2));
    assert_eq!(
        receipt.reconciliation_stages,
        json!({"transaction":"complete","exports":"complete","preview":"complete"})
    );
    assert!(receipt.reconciled_exports.is_empty());
    assert!(receipt.cancelled_preview_work_ids.is_empty());
    assert_eq!(receipt.unresolved_preview_work_ids, Some(Vec::new()));
    assert!(receipt.disposed_viewport_work_ids.is_empty());
    assert_eq!(receipt.viewport_status, "already_absent");
    let mut authority = native.lock().unwrap();
    assert!(authority.active().is_none());
    let next = authority.reserve_install().unwrap();
    assert!(
        next > receipt.lifecycle_generation && receipt.lifecycle_generation > original_generation
    );
    assert!(authority.reserve_install().is_err());
    authority
        .install(
            next,
            Box::new(CleanupOwner {
                calls: calls.clone(),
                mode: CleanupMode::Complete,
                started: false,
            }),
        )
        .unwrap();
    assert!(authority.reserve_install().is_err());
    drop(authority);
    let ReleaseAdmission::Retry {
        receipt: retained, ..
    } = admit_release_request(&native, &request).unwrap()
    else {
        panic!("release reran");
    };
    assert_eq!(retrieved_receipt(retained), receipt);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(native.lock().unwrap().active_generation().unwrap(), next);
    let mut changed = request.clone();
    changed.expected_revision = 8;
    assert_eq!(
        admit_release_request(&native, &changed).unwrap_err().code,
        "invalid_request"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn generic_terminal_failures_keep_identity_progress_and_exclusive_tombstones() {
    use std::sync::atomic::Ordering;
    for mode in [
        CleanupMode::Unsupported,
        CleanupMode::PanicBefore,
        CleanupMode::PanicAfter,
        CleanupMode::ProgressPanic,
        CleanupMode::TransactionUnknown,
        CleanupMode::ExportsUnknown,
        CleanupMode::PreviewUnknown,
        CleanupMode::WrongIdentity,
    ] {
        let (native, request, calls, _) = generic_owner(mode);
        let generation = admit_generation(&native, &request);
        let viewport_calls = std::sync::atomic::AtomicUsize::new(0);
        let receipt = complete_release(&native, generation, &request, || {
            viewport_calls.fetch_add(1, Ordering::SeqCst);
            Ok((Vec::new(), "already_absent"))
        })
        .unwrap();
        assert_generic_identity(&receipt);
        assert_eq!(receipt.status, "indeterminate");
        assert_eq!(receipt.error.as_ref().unwrap().code, "cleanup_failed");
        assert!(!receipt.reentry_available);
        if matches!(mode, CleanupMode::PanicAfter) {
            assert_eq!(
                receipt.reconciliation_stages,
                json!({"transaction":"complete","exports":"unknown","preview":"unknown"})
            );
            assert_eq!(
                receipt.cancelled_transaction_id.as_deref(),
                Some("generic-transaction")
            );
            assert_eq!((receipt.undo_depth, receipt.redo_depth), (3, 2));
        }
        if matches!(
            mode,
            CleanupMode::Unsupported
                | CleanupMode::PanicBefore
                | CleanupMode::PanicAfter
                | CleanupMode::ProgressPanic
        ) {
            assert_eq!(viewport_calls.load(Ordering::SeqCst), 0);
        }
        assert!(native.lock().unwrap().reserve_install().is_err());
        let ReleaseAdmission::Retry {
            receipt: retained, ..
        } = admit_release_request(&native, &request).unwrap()
        else {
            panic!("failed release reran");
        };
        assert_eq!(retrieved_receipt(retained), receipt);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
#[test]
fn generic_admission_and_generation_failures_do_not_run_cleanup() {
    use std::sync::atomic::Ordering;
    let (native, request, calls, original) = generic_owner(CleanupMode::Complete);
    for field in 0..4 {
        let mut wrong = request.clone();
        match field {
            0 => wrong.instance_id = "wrong".into(),
            1 => wrong.document_id = "wrong".into(),
            2 => wrong.expected_revision = 8,
            _ => wrong.cancelled_before_dispatch = true,
        }
        assert_eq!(
            admit_release_request(&native, &wrong).unwrap_err().code,
            [
                "wrong_instance",
                "wrong_document",
                "stale_revision",
                "cancelled_before_dispatch"
            ][field]
        );
        assert_eq!(
            native.lock().unwrap().active_generation().unwrap(),
            original
        );
    }
    let generation = admit_generation(&native, &request);
    assert!(
        complete_release(&native, generation + 1, &request, || panic!(
            "stale viewport cleanup"
        ))
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        native.lock().unwrap().phase,
        NativePhase::Releasing { .. }
    ));
    assert_eq!(
        admit_and_complete_retry_generation(&native, generation, &request).status,
        "succeeded"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
fn admit_and_complete_retry_generation(
    native: &NativeState,
    generation: u64,
    request: &NativeReleaseRequest,
) -> NativeReleaseReceipt {
    complete_release(native, generation, request, || {
        Ok((Vec::new(), "already_absent"))
    })
    .unwrap()
}
#[test]
fn generic_poison_keeps_cleanup_evidence_but_denies_reentry() {
    let (native, request, calls, _) = generic_owner(CleanupMode::Complete);
    let generation = admit_generation(&native, &request);
    let poisoned = native.clone();
    let _ = std::panic::catch_unwind(move || {
        let _lock = poisoned.lock().unwrap();
        panic!("poison");
    });
    let receipt = admit_and_complete_retry_generation(&native, generation, &request);
    assert_generic_identity(&receipt);
    assert_eq!(receipt.status, "indeterminate");
    assert_eq!(receipt.error.as_ref().unwrap().code, "cleanup_failed");
    assert_eq!(
        receipt.reconciliation_stages,
        json!({"transaction":"complete","exports":"complete","preview":"complete"})
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .reserve_install()
        .is_err());
    let ReleaseAdmission::Retry {
        receipt: retained, ..
    } = admit_release_request(&native, &request).unwrap()
    else {
        panic!("poison receipt lost");
    };
    assert_eq!(retrieved_receipt(retained), receipt);
}
