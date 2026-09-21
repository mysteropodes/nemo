use crate::{
    native_application::*,
    native_application_contract::{admit_project, GeometryResourceInput, NativePreviewRequest},
    native_application_ports::{DesktopArtifactPort, SharedCompositor},
};
use native_engine::compositor::Compositor;
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

const PROJECT: &[u8] = include_bytes!("../../native-engine/tests/fixtures/opacity-v2/project.json");
const INSTANCE: &str = "native-release-fixture";
const LAYER: &str = "r08_curve_layer";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("nemo-native-release-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project() -> Value {
    serde_json::from_slice(PROJECT).unwrap()
}

fn resource() -> Value {
    json!({
        "resourceId": "geometry",
        "resourceVersion": "v1",
        "layers": [{
            "layerUid": LAYER,
            "bounds": [20.0, 60.0, 40.0, 80.0],
            "transform": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            "paint": { "red": 255, "green": 0, "blue": 0 }
        }]
    })
}

fn desktop(scratch: &Scratch, output: Option<PathBuf>) -> DesktopNativeApplication {
    let resources: Vec<GeometryResourceInput> =
        serde_json::from_value(json!([resource()])).unwrap();
    let admitted = admit_project(&project(), &resources).unwrap();
    let bindings = output
        .map(|path| vec![("output".to_string(), path)])
        .unwrap_or_default();
    let artifacts = DesktopArtifactPort::new(scratch.0.join("staging"), bindings).unwrap();
    DesktopNativeApplication::new(
        INSTANCE.into(),
        admitted,
        artifacts,
        SharedCompositor::new(Compositor::new().expect("native GPU context")),
    )
    .unwrap()
}

fn install(application: DesktopNativeApplication) -> (NativeState, u64) {
    let mut authority = NativeAuthority::default();
    let generation = authority.reserve_install().unwrap();
    authority
        .install(generation, Box::new(application))
        .unwrap();
    (Arc::new(Mutex::new(authority)), generation)
}

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

fn dispatch_history(
    application: &mut DesktopNativeApplication,
    request_id: &str,
    operation: &str,
    payload: Value,
) -> Value {
    let expected_revision = matches!(
        operation,
        "transaction.begin" | "history.undo" | "history.redo" | "job.export.png.begin"
    )
    .then_some(application.content_revision());
    let mut request = json!({
        "apiVersion": 2,
        "requestId": request_id,
        "instanceId": application.instance_id(),
        "documentId": application.document_id(),
        "operation": operation,
        "payload": payload,
        "cancelledBeforeDispatch": false
    });
    if let Some(expected_revision) = expected_revision {
        request["expectedRevision"] = json!(expected_revision);
    }
    let request = serde_json::from_value(request).unwrap();
    serde_json::to_value(application.core.dispatch(request)).unwrap()
}

fn begin_update(application: &mut DesktopNativeApplication, prefix: &str, value: i64) -> String {
    let begun = dispatch_history(
        application,
        &format!("{prefix}-begin"),
        "transaction.begin",
        json!({"stableTarget": {"layerUid": LAYER}}),
    );
    let id = begun["result"]["transactionId"]
        .as_str()
        .unwrap()
        .to_string();
    let updated = dispatch_history(
        application,
        &format!("{prefix}-update"),
        "transaction.update",
        json!({"transactionId": id, "value": value}),
    );
    assert_eq!(updated["result"]["workingState"]["value"], value);
    id
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

fn rejects(native: &NativeState, request: &NativeReleaseRequest, code: &str, generation: u64) {
    let error = admit_release_request(native, request).unwrap_err();
    let active = native.lock().unwrap().active_generation().unwrap();
    assert_eq!((error.code.as_str(), active), (code, generation));
}

#[test]
fn idle_release_is_strict_retained_and_allows_exactly_one_reentry() {
    let scratch = Scratch::new();
    let application = desktop(&scratch, None);
    let document = application.document_id().to_owned();
    let (native, active_generation) = install(application);

    let mut cancelled = release_request(&document, "release-idle");
    cancelled.cancelled_before_dispatch = true;
    rejects(
        &native,
        &cancelled,
        "cancelled_before_dispatch",
        active_generation,
    );

    let mut stale = release_request(&document, "release-stale");
    stale.expected_revision = 1;
    rejects(&native, &stale, "stale_revision", active_generation);
    let mut wrong = release_request(&document, "release-wrong");
    wrong.instance_id = "wrong-instance".into();
    rejects(&native, &wrong, "wrong_instance", active_generation);
    wrong = release_request("wrong-document", "release-wrong-document");
    rejects(&native, &wrong, "wrong_document", active_generation);
    wrong = release_request(&document, "release-wrong-version");
    wrong.api_version += 1;
    rejects(&native, &wrong, "invalid_request", active_generation);
    wrong = release_request(&document, " bad");
    rejects(&native, &wrong, "invalid_request", active_generation);
    let mut unknown = serde_json::to_value(release_request(&document, "unknown-field")).unwrap();
    unknown["extra"] = json!(true);
    assert!(serde_json::from_value::<NativeReleaseRequest>(unknown).is_err());

    let request = release_request(&document, "release-idle");
    let generation = admit_generation(&native, &request);
    let receipt = complete_release(&native, generation, &request, || {
        Ok((Vec::new(), "already_absent"))
    })
    .unwrap();
    assert_eq!(receipt.status, "succeeded");
    assert!(!receipt.retrieved);
    assert!(receipt.authority_removal_completed);
    assert!(receipt.reentry_available);

    let retry = admit_release_request(&native, &request).unwrap();
    let ReleaseAdmission::Retry { receipt: value, .. } = retry else {
        panic!("retry was not retained")
    };
    assert_eq!(retrieved_receipt(value), receipt);
    let mut changed_cancel = request.clone();
    changed_cancel.cancelled_before_dispatch = true;
    assert_eq!(
        admit_release_request(&native, &changed_cancel)
            .unwrap_err()
            .code,
        "invalid_request"
    );
    let mut changed = request.clone();
    changed.expected_revision = 1;
    assert_eq!(
        admit_release_request(&native, &changed).unwrap_err().code,
        "invalid_request"
    );

    let next = desktop(&scratch, None);
    let next_document = next.document_id().to_owned();
    let mut guard = native.lock().unwrap();
    let reentry_generation = guard.reserve_install().unwrap();
    assert!(guard.reserve_install().is_err());
    guard.install(reentry_generation, Box::new(next)).unwrap();
    drop(guard);

    let ReleaseAdmission::Retry {
        receipt: old_receipt,
        ..
    } = admit_release_request(&native, &request).unwrap()
    else {
        panic!("old release receipt was not retained across re-entry")
    };
    assert_eq!(retrieved_receipt(old_receipt), receipt);
    assert_eq!(
        native.lock().unwrap().active_generation().unwrap(),
        reentry_generation
    );
    let mut changed_after_reentry = request.clone();
    changed_after_reentry.expected_revision = 1;
    assert_eq!(
        admit_release_request(&native, &changed_after_reentry)
            .unwrap_err()
            .code,
        "invalid_request"
    );
    assert_eq!(
        native.lock().unwrap().active_generation().unwrap(),
        reentry_generation
    );

    let final_receipt = admit_and_complete(
        &native,
        &release_request(&next_document, "release-after-reentry"),
    );
    assert_eq!(final_receipt.status, "succeeded");
    assert!(!final_receipt.reentry_available);
    assert!(native.lock().unwrap().reserve_install().is_err());
}

#[test]
fn viewport_panic_preserves_transaction_export_and_preview_reconciliation() {
    let scratch = Scratch::new();
    let mut application = desktop(&scratch, Some(scratch.0.join("partial-output")));
    let document = application.document_id().to_owned();
    let committed_id = begin_update(&mut application, "committed", 41);
    assert_eq!(
        dispatch_history(
            &mut application,
            "committed-finish",
            "transaction.commit",
            json!({"transactionId": committed_id}),
        )["contentRevision"],
        1
    );
    assert_eq!(
        dispatch_history(
            &mut application,
            "committed-undo",
            "history.undo",
            json!({}),
        )["contentRevision"],
        2
    );
    let committed_value = application
        .core
        .acquire_snapshot(2)
        .unwrap()
        .static_opacity(LAYER)
        .unwrap();
    let export = dispatch_history(
        &mut application,
        "partial-export",
        "job.export.png.begin",
        json!({"contextId":"scene-root","quality":"final","outputHandle":"output","frames":[
            {"sourceFrame":0,"geometryHandle":{"resourceId":"geometry","resourceVersion":"v1"}}
        ]}),
    );
    let export_id = export["result"]["jobId"].as_str().unwrap().to_string();
    application
        .core
        .run_export_to_completion(&export_id)
        .unwrap();
    let transaction_id = begin_update(&mut application, "working", 73);

    let snapshot = application.core.acquire_snapshot(2).unwrap();
    let preview: NativePreviewRequest = serde_json::from_value(json!({
        "apiVersion": HOST_API_VERSION,
        "instanceId": INSTANCE,
        "documentId": document,
        "contentRevision": 2,
        "documentSnapshotId": snapshot.id(),
        "contextId": "scene-root",
        "frame": 0,
        "quality": "final",
        "outputSpec": {"kind":"frame","format":"rgba8","width":320,"height":180,"colorInterpretation":"srgb","alphaMode":"straight"},
        "geometryHandle": {"resourceId":"geometry","resourceVersion":"v1"}
    })).unwrap();
    let prepared = application.prepare_preview(&preview).unwrap();
    let deferred_id = prepared.identity.work_id();
    application
        .finish_preview(deferred_id, "deferred-timeout")
        .unwrap();

    let (native, _) = install(application);
    let mut request = release_request(&document, "release-work");
    request.expected_revision = 2;
    let generation = admit_generation(&native, &request);
    let receipt = complete_release(&native, generation, &request, || {
        panic!("injected viewport release panic")
    })
    .unwrap();
    assert_eq!(receipt.status, "indeterminate");
    assert_eq!(
        receipt.error.as_ref().unwrap().message,
        "injected viewport release panic"
    );
    assert_eq!(
        receipt.cancelled_transaction_id.as_deref(),
        Some(transaction_id.as_str())
    );
    assert_eq!(receipt.content_revision, 2);
    assert_eq!((receipt.undo_depth, receipt.redo_depth), (0, 1));
    let terminal = receipt.cancelled_transaction.as_ref().unwrap();
    assert_eq!(terminal["transactionId"], transaction_id);
    assert_eq!(terminal["terminalDisposition"], "cancelled");
    assert_eq!(terminal["workingState"]["value"], 73);
    assert_eq!(terminal["committedValue"].as_f64(), Some(committed_value));
    assert_eq!(terminal["historyEntriesAdded"], 0);
    assert_eq!(receipt.reconciled_exports[0].job_id, export_id);
    assert_eq!(receipt.reconciled_exports[0].status, "succeeded");
    assert!(receipt
        .cancelled_preview_work_ids
        .contains(&work_label(deferred_id)));
    assert!(native.lock().unwrap().reserve_install().is_err());
    let ReleaseAdmission::Retry { receipt: value, .. } =
        admit_release_request(&native, &request).unwrap()
    else {
        unreachable!()
    };
    assert_eq!(retrieved_receipt(value), receipt);
}

#[test]
fn release_between_export_frames_cleans_stage_and_preserves_published_artifacts() {
    let scratch = Scratch::new();
    let pending_target = scratch.0.join("pending-output");
    let mut pending_app = desktop(&scratch, Some(pending_target.clone()));
    let pending_document = pending_app.document_id().to_owned();
    let begun = dispatch_history(
        &mut pending_app,
        "export-pending",
        "job.export.png.begin",
        json!({"contextId":"scene-root","quality":"final","outputHandle":"output","frames":[
            {"sourceFrame":0,"geometryHandle":{"resourceId":"geometry","resourceVersion":"v1"}}
        ]}),
    );
    let job_id = begun["result"]["jobId"].as_str().unwrap().to_string();
    let (native, active_generation) = install(pending_app);
    let request = release_request(&pending_document, "release-export");
    let staging_root = scratch.0.join("staging");
    let next_scratch = Scratch::new();
    let next_target = next_scratch.0.join("published-output");
    let mut next_app = desktop(&next_scratch, Some(next_target.clone()));
    let next_document = next_app.document_id().to_owned();
    let begun = dispatch_history(
        &mut next_app,
        "export-pending",
        "job.export.png.begin",
        json!({"contextId":"scene-root","quality":"final","outputHandle":"output","frames":[
            {"sourceFrame":0,"geometryHandle":{"resourceId":"geometry","resourceVersion":"v1"}}
        ]}),
    );
    assert_eq!(begun["result"]["jobId"], job_id);
    let next_pending = next_app
        .core
        .start_next_export_frame(&job_id)
        .unwrap()
        .unwrap();
    let mut next_app = Some(next_app);
    let mut release_receipt = None;
    let mut reentry_generation = 0;
    run_export_pump_interleaved(native.clone(), active_generation, job_id.clone(), || {
        let generation = admit_generation(&native, &request);
        release_receipt = Some(
            complete_release(&native, generation, &request, || {
                assert_eq!(fs::read_dir(&staging_root).unwrap().count(), 0);
                Ok((Vec::new(), "already_absent"))
            })
            .unwrap(),
        );
        let mut authority = native.lock().unwrap();
        reentry_generation = authority.reserve_install().unwrap();
        authority
            .install(reentry_generation, Box::new(next_app.take().unwrap()))
            .unwrap();
    });
    let receipt = release_receipt.unwrap();
    let export = receipt
        .reconciled_exports
        .iter()
        .find(|value| value.job_id == job_id)
        .unwrap();
    assert_eq!(
        (export.status.as_str(), export.cleanup_status.as_str()),
        ("cancelled", "complete")
    );
    assert!(
        !next_target.exists(),
        "old pump touched the reentered application"
    );
    let _finished = native
        .lock()
        .unwrap()
        .active_mut(reentry_generation)
        .unwrap()
        .finish_export_frame(next_pending)
        .unwrap();
    assert!(next_target.exists());
    let receipt = admit_and_complete(
        &native,
        &release_request(&next_document, "release-published"),
    );
    let retained = receipt
        .reconciled_exports
        .iter()
        .find(|value| value.job_id == job_id)
        .unwrap();
    assert_eq!(retained.status, "succeeded");
    assert_eq!(retained.external_effect_disposition, "committed");
    assert!(next_target.exists());
}

#[test]
fn poisoned_lock_alone_retains_an_indeterminate_terminal_receipt() {
    let scratch = Scratch::new();
    let application = desktop(&scratch, None);
    let document = application.document_id().to_owned();
    let (native, _) = install(application);
    let request = release_request(&document, "release-poisoned");
    let generation = admit_generation(&native, &request);
    let poisoned = Arc::clone(&native);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard = poisoned.lock().unwrap();
        panic!("inject native authority lock poison");
    }));

    let receipt = complete_release(&native, generation, &request, || {
        Ok((Vec::new(), "already_absent"))
    })
    .unwrap();
    assert_eq!(receipt.error.as_ref().unwrap().code, "cleanup_failed");
    let ReleaseAdmission::Retry { receipt: value, .. } =
        admit_release_request(&native, &request).unwrap()
    else {
        panic!("poisoned release receipt was not retained")
    };
    assert_eq!(retrieved_receipt(value), receipt);
    assert!(native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .reserve_install()
        .is_err());
}

#[test]
fn missing_admitted_application_becomes_an_exclusive_terminal_tombstone() {
    let scratch = Scratch::new();
    let application = desktop(&scratch, None);
    let document = application.document_id().to_owned();
    let (native, _) = install(application);
    let request = release_request(&document, "release-missing-application");
    let generation = admit_generation(&native, &request);
    let mut guard = native.lock().unwrap();
    let NativePhase::Releasing { application, .. } = &mut guard.phase else {
        unreachable!()
    };
    *application = None;
    drop(guard);

    let viewport_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called = Arc::clone(&viewport_called);
    let receipt = complete_release(&native, generation, &request, move || {
        called.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok((Vec::new(), "already_absent"))
    })
    .unwrap();
    assert_eq!(receipt.status, "indeterminate");
    assert!(!viewport_called.load(std::sync::atomic::Ordering::SeqCst));
    assert!(native.lock().unwrap().reserve_install().is_err());
    assert!(matches!(
        admit_release_request(&native, &request).unwrap(),
        ReleaseAdmission::Retry { .. }
    ));
}
