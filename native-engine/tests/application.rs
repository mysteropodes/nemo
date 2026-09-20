use crate::application::{
    ExportResourceResolver, NativeApplication, ResourceResolutionError,
    ResourceResolutionErrorKind,
};
use crate::codec::decode_project;
use crate::commands::{
    DispatchErrorCode, OpacityRequest, OP_QUERY_OPACITY, OP_QUERY_REVISION, OP_QUERY_SNAPSHOT,
};
use crate::export_job::{
    ExportArtifact, ExportCompositor, ExportReadback, JobStatus, StagedArtifactPort,
};
use crate::protocol::{
    OpaqueResourceHandle, OP_JOB_EXPORT_PNG_BEGIN, OP_JOB_EXPORT_PNG_CANCEL,
    OP_JOB_EXPORT_PNG_STATUS,
};
use crate::render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint, RenderScene};
use crate::transaction::{
    OP_HISTORY_REDO, OP_HISTORY_UNDO, OP_TRANSACTION_BEGIN, OP_TRANSACTION_CANCEL,
    OP_TRANSACTION_COMMIT, OP_TRANSACTION_STATUS, OP_TRANSACTION_UPDATE,
};
use serde_json::{json, Value};
use std::cell::Cell;
use std::rc::Rc;

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
const LAYER: &str = "r08_curve_layer";

#[derive(Default)]
struct Port {
    staged: Vec<Vec<u8>>,
    published: Vec<ExportArtifact>,
    cleanup_calls: usize,
    fail_write: bool,
    fail_cleanup: bool,
}

impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, bytes: &[u8]) -> Result<(), String> {
        if self.fail_write {
            return Err("injected write failure".into());
        }
        self.staged.push(bytes.to_vec());
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        let artifact = ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        };
        self.staged.clear();
        self.published.push(artifact.clone());
        Ok(artifact)
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        self.cleanup_calls += 1;
        if self.fail_cleanup {
            Err("injected cleanup failure".into())
        } else {
            self.staged.clear();
            Ok(())
        }
    }
}

struct Compositor;

impl ExportCompositor for Compositor {
    type Composition = ExportReadback;
    fn compose(&mut self, scene: &RenderScene) -> Result<Self::Composition, String> {
        Ok(ExportReadback {
            width: 320,
            height: 180,
            bytes: [scene.content_revision() as u8, 20, 30, 255].repeat(320 * 180),
            document_snapshot_id: scene.document_snapshot_id().into(),
            document_id: scene.document_id().into(),
            content_revision: scene.content_revision(),
            context_id: scene.context_id().into(),
            source_frame: scene.frame(),
            quality: scene.quality().into(),
        })
    }
    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String> {
        Ok(result.clone())
    }
}

struct Resolver {
    calls: Rc<Cell<usize>>,
    unavailable: bool,
}

impl ExportResourceResolver for Resolver {
    fn resolve_geometry(
        &mut self,
        handle: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        self.calls.set(self.calls.get() + 1);
        if self.unavailable {
            return Err(ResourceResolutionError::new(
                ResourceResolutionErrorKind::Unavailable,
                "geometry lease is unavailable",
            ));
        }
        GeometryPaintInput::new(
            handle.resource_id(),
            handle.resource_version(),
            vec![LayerGeometry::new(
                LAYER,
                [20.0, 60.0, 40.0, 80.0],
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                OpaqueSrgbPaint::new(255, 0, 0),
            )
            .unwrap()],
        )
        .map_err(|error| {
            ResourceResolutionError::new(ResourceResolutionErrorKind::Internal, error.to_string())
        })
    }
}

type App = NativeApplication<Port, Compositor, Resolver>;

fn app_with(port: Port, unavailable: bool) -> (App, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    let app = NativeApplication::new(
        "application-fixture",
        decode_project(PROJECT).unwrap(),
        port,
        Compositor,
        Resolver {
            calls: Rc::clone(&calls),
            unavailable,
        },
    )
    .unwrap();
    (app, calls)
}

fn query(app: &App, id: &str, operation: &str, payload: Value) -> OpacityRequest {
    OpacityRequest::query(id, app.instance_id(), app.document_id(), operation, payload)
}

fn stage(
    app: &App,
    id: &str,
    revision: Option<u64>,
    operation: &str,
    payload: Value,
) -> OpacityRequest {
    OpacityRequest::history_stage(
        id,
        app.instance_id(),
        app.document_id(),
        revision,
        operation,
        payload,
    )
}

fn set(app: &App, id: &str, value: i64) -> OpacityRequest {
    OpacityRequest::command(
        id,
        app.instance_id(),
        app.document_id(),
        app.content_revision(),
        json!({
            "command": "layer.opacity.set",
            "stableTarget": { "layerUid": LAYER },
            "value": value
        }),
    )
}

fn begin(app: &App, id: &str, frames: &[u32]) -> OpacityRequest {
    stage(
        app,
        id,
        Some(app.content_revision()),
        OP_JOB_EXPORT_PNG_BEGIN,
        json!({
            "contextId": "scene-root",
            "quality": "final",
            "outputHandle": "output/fixture",
            "frames": frames.iter().map(|frame| json!({
                "sourceFrame": frame,
                "geometryHandle": {
                    "resourceId": "geometry/r08",
                    "resourceVersion": "v1"
                }
            })).collect::<Vec<_>>()
        }),
    )
}

fn job_id(response: &crate::commands::ResponseEnvelope) -> String {
    response.result().unwrap()["jobId"].as_str().unwrap().into()
}

fn assert_error(response: &crate::commands::ResponseEnvelope, code: DispatchErrorCode) {
    assert!(!response.is_ok());
    assert_eq!(response.error().unwrap().code(), code);
}

#[test]
fn common_dispatch_reuses_history_for_commands_queries_transactions_and_undo() {
    let (mut app, _) = app_with(Port::default(), false);
    assert_eq!(app.dispatch(set(&app, "set-40", 40)).content_revision(), 1);
    let opacity = query(
        &app,
        "opacity",
        OP_QUERY_OPACITY,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    );
    assert_eq!(
        app.dispatch(opacity).result().unwrap()["value"].as_f64(),
        Some(40.0)
    );
    assert_eq!(
        app.dispatch(query(&app, "revision", OP_QUERY_REVISION, json!({})))
            .result()
            .unwrap()["contentRevision"],
        1
    );
    assert!(app
        .dispatch(query(&app, "snapshot", OP_QUERY_SNAPSHOT, json!({})))
        .result()
        .unwrap()["documentSnapshotId"]
        .as_str()
        .is_some());
    let begun = app.dispatch(stage(
        &app,
        "tx-begin",
        Some(1),
        OP_TRANSACTION_BEGIN,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    ));
    let transaction = begun.result().unwrap()["transactionId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_error(&app.dispatch(set(&app, "busy", 60)), DispatchErrorCode::BusyConflict);
    assert!(app
        .dispatch(stage(
            &app,
            "tx-update",
            None,
            OP_TRANSACTION_UPDATE,
            json!({ "transactionId": transaction, "value": 60 }),
        ))
        .is_ok());
    let status = app.dispatch(stage(
        &app,
        "tx-status",
        None,
        OP_TRANSACTION_STATUS,
        json!({ "transactionId": transaction }),
    ));
    assert_eq!(status.result().unwrap()["workingGeneration"], 1);
    assert_eq!(
        app.dispatch(stage(
            &app,
            "tx-commit",
            None,
            OP_TRANSACTION_COMMIT,
            json!({ "transactionId": transaction }),
        ))
        .content_revision(),
        2
    );
    assert_eq!(
        app.dispatch(stage(&app, "undo", Some(2), OP_HISTORY_UNDO, json!({})))
            .content_revision(),
        3
    );
    let opacity = query(
        &app,
        "after-undo",
        OP_QUERY_OPACITY,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    );
    assert_eq!(
        app.dispatch(opacity).result().unwrap()["value"].as_f64(),
        Some(40.0)
    );
    assert_eq!(
        app.dispatch(stage(&app, "redo", Some(3), OP_HISTORY_REDO, json!({})))
            .content_revision(),
        4
    );
    let cancelled = app.dispatch(stage(
        &app,
        "cancel-begin",
        Some(4),
        OP_TRANSACTION_BEGIN,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    ));
    let cancelled_id = cancelled.result().unwrap()["transactionId"].as_str().unwrap();
    assert_eq!(
        app.dispatch(stage(
            &app,
            "cancel",
            None,
            OP_TRANSACTION_CANCEL,
            json!({ "transactionId": cancelled_id }),
        ))
        .result()
        .unwrap()["terminalDisposition"],
        "cancelled"
    );
    assert_eq!(app.content_revision(), 4);
}

#[test]
fn retries_are_global_changed_body_rejected_and_first_cancellation_unretained() {
    let (mut app, calls) = app_with(Port::default(), false);
    let request = begin(&app, "retry", &[1]);
    let cancelled = app.dispatch(request.clone().cancelled());
    assert_error(&cancelled, DispatchErrorCode::CancelledBeforeDispatch);
    let begun = app.dispatch(request.clone());
    assert_eq!(calls.get(), 1);
    let id = job_id(&begun);
    let terminal = app.run_export_to_completion(&id).unwrap();
    assert_eq!(terminal.status, JobStatus::Succeeded);
    assert_eq!(app.dispatch(set(&app, "retry-advance", 40)).content_revision(), 1);
    let retried = app.dispatch(request.clone());
    assert_eq!(retried, begun, "begin retry replays its original receipt");
    assert_eq!(retried.result().unwrap()["status"], "running");
    let status = app.dispatch(query(
        &app,
        "retry-status",
        OP_JOB_EXPORT_PNG_STATUS,
        json!({ "jobId": id }),
    ));
    assert_eq!(status.result().unwrap()["status"], "succeeded");
    assert_eq!(status.content_revision(), 1);
    assert_eq!(calls.get(), 1, "identical begin retry cannot resolve again");
    let mut changed = serde_json::to_value(request).unwrap();
    changed["payload"]["frames"][0]["sourceFrame"] = json!(2);
    assert_error(
        &app.dispatch(serde_json::from_value(changed).unwrap()),
        DispatchErrorCode::InvalidRequest,
    );
    let collision = query(&app, "retry", OP_QUERY_REVISION, json!({}));
    assert_error(&app.dispatch(collision), DispatchErrorCode::InvalidRequest);
}

#[test]
fn revision_rules_wrong_identity_stale_unknown_and_malformed_fail_closed() {
    let (mut app, _) = app_with(Port::default(), false);
    let stale = begin(&app, "stale", &[1]);
    assert!(app.dispatch(set(&app, "advance", 40)).is_ok());
    assert_error(&app.dispatch(stale), DispatchErrorCode::StaleRevision);
    let status_with_revision = stage(
        &app,
        "status-revision",
        Some(1),
        OP_JOB_EXPORT_PNG_STATUS,
        json!({ "jobId": "native-export-job-1" }),
    );
    assert_error(
        &app.dispatch(status_with_revision),
        DispatchErrorCode::InvalidRequest,
    );
    assert_error(
        &app.dispatch(query(&app, "unknown", "unknown.operation", json!({}))),
        DispatchErrorCode::InvalidRequest,
    );
    let raw = stage(
        &app,
        "raw-geometry",
        Some(1),
        OP_JOB_EXPORT_PNG_BEGIN,
        json!({
            "contextId": "scene-root", "quality": "final", "outputHandle": "output/a",
            "frames": [{
                "sourceFrame": 1,
                "geometryHandle": { "resourceId": "geometry/r08", "resourceVersion": "v1" },
                "geometry": { "layers": [] }
            }]
        }),
    );
    assert_error(&app.dispatch(raw), DispatchErrorCode::InvalidRequest);
    let wrong = OpacityRequest::query(
        "wrong-instance",
        "other-instance",
        app.document_id(),
        OP_QUERY_REVISION,
        json!({}),
    );
    assert_error(&app.dispatch(wrong), DispatchErrorCode::WrongInstance);
}

#[test]
fn jobs_pin_r_while_commands_advance_to_r_plus_one_and_terminal_receipts_replay() {
    let (mut app, _) = app_with(Port::default(), false);
    let begun = app.dispatch(begin(&app, "pin", &[10, 11]));
    let id = job_id(&begun);
    assert!(app.dispatch(set(&app, "edit", 40)).is_ok());
    assert_eq!(app.content_revision(), 1);
    let running_status = query(
        &app,
        "running-status",
        OP_JOB_EXPORT_PNG_STATUS,
        json!({ "jobId": id }),
    );
    let retained_running = app.dispatch(running_status.clone());
    let first_frame = app.start_next_export_frame(&id).unwrap().unwrap();
    assert_eq!(app.finish_export_frame(first_frame).unwrap().progress, 0.5);
    assert_eq!(app.dispatch(running_status), retained_running);
    let terminal = app.run_export_to_completion(&id).unwrap();
    assert_eq!((terminal.status, terminal.pinned_revision), (JobStatus::Succeeded, 0));
    let status = query(
        &app,
        "terminal-status",
        OP_JOB_EXPORT_PNG_STATUS,
        json!({ "jobId": id }),
    );
    let first = app.dispatch(status.clone());
    assert_eq!(first.result().unwrap()["status"], "succeeded");
    assert_eq!(app.dispatch(status), first);
    let cancel = query(
        &app,
        "terminal-cancel",
        OP_JOB_EXPORT_PNG_CANCEL,
        json!({ "jobId": id }),
    );
    assert_eq!(app.dispatch(cancel).result(), first.result());
}

#[test]
fn unknown_jobs_and_resource_resolution_failures_map_to_closed_errors() {
    let (mut app, _) = app_with(Port::default(), false);
    let missing = query(
        &app,
        "missing-job",
        OP_JOB_EXPORT_PNG_STATUS,
        json!({ "jobId": "native-export-job-999" }),
    );
    assert_error(&app.dispatch(missing), DispatchErrorCode::NotFound);
    let (mut unavailable, _) = app_with(Port::default(), true);
    let request = begin(&unavailable, "unavailable", &[1]);
    assert_error(
        &unavailable.dispatch(request),
        DispatchErrorCode::Unavailable,
    );
}

#[test]
fn replacement_reconciles_jobs_rejects_old_document_and_contains_late_completion() {
    let (mut app, _) = app_with(Port::default(), false);
    let old_document = app.document_id().to_owned();
    let old_query = query(&app, "old-query", OP_QUERY_REVISION, json!({}));
    let begun = app.dispatch(begin(&app, "replace", &[7]));
    let id = job_id(&begun);
    let late = app.start_next_export_frame(&id).unwrap().unwrap();
    let reconciled = app
        .replace_document(decode_project(PROJECT).unwrap())
        .unwrap();
    assert_eq!(reconciled[0].status, JobStatus::Cancelled);
    assert_ne!(app.document_id(), old_document);
    let rejected = app.dispatch(old_query);
    assert_error(&rejected, DispatchErrorCode::WrongDocument);
    assert_eq!(
        serde_json::to_value(rejected).unwrap()["error"]["details"]["requestedDocumentId"],
        old_document
    );
    assert_eq!(app.finish_export_frame(late).unwrap(), reconciled[0]);
    assert!(app.artifact_port().published.is_empty());
}

#[test]
fn cleanup_failure_is_a_successful_dispatch_with_exact_failed_job_receipt() {
    let port = Port {
        fail_write: true,
        fail_cleanup: true,
        ..Port::default()
    };
    let (mut app, _) = app_with(port, false);
    let begun = app.dispatch(begin(&app, "cleanup", &[1]));
    let id = job_id(&begun);
    let failed = app.run_export_to_completion(&id).unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    let status = query(
        &app,
        "cleanup-status",
        OP_JOB_EXPORT_PNG_STATUS,
        json!({ "jobId": id }),
    );
    let response = app.dispatch(status);
    assert!(response.is_ok());
    let result = response.result().unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(result["cleanup"]["status"], "failed");
    assert_eq!(result["cleanup"]["error"]["code"], "cleanup_failed");
    assert_eq!(result["externalEffectDisposition"], "indeterminate");
    assert!(result["artifact"].is_null());
    assert_eq!(app.artifact_port().cleanup_calls, 1);
}
