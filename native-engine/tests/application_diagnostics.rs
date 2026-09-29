//! Authority-boundary oracles; fixture ports cannot render, publish or resolve assets.
use super::*;
use crate::codec::{decode_project, encode_project};
use crate::export_job::{ExportArtifact, ExportReadback};
use crate::render_scene::RenderScene;
use serde_json::{json, Value};

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
const QUERY: &str = "query.diagnostics.recent";
const LAYER: &str = "r08_curve_layer";
struct Unused;
impl StagedArtifactPort for Unused {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        panic!("staging")
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        panic!("write")
    }
    fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
        panic!("publish")
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}
impl ExportCompositor for Unused {
    type Composition = ();
    fn compose(&mut self, _: &RenderScene) -> Result<(), String> {
        panic!("compose")
    }
    fn readback_rgba8(&self, _: &()) -> Result<ExportReadback, String> {
        panic!("readback")
    }
}
impl ExportResourceResolver for Unused {
    fn resolve_geometry(
        &mut self,
        _: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        panic!("resolve")
    }
}
type App = NativeApplication<Unused, Unused, Unused>;
fn app() -> App {
    NativeApplication::new(
        "diagnostics-fixture",
        decode_project(PROJECT).unwrap(),
        Unused,
        Unused,
        Unused,
    )
    .unwrap()
}
fn query(app: &App, id: &str) -> OpacityRequest {
    OpacityRequest::query(id, app.instance_id(), app.document_id(), QUERY, json!({}))
}
fn set(app: &App, id: &str, value: u32) -> OpacityRequest {
    OpacityRequest::command(
        id,
        app.instance_id(),
        app.document_id(),
        app.content_revision(),
        json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":LAYER}, "value":value}),
    )
}
fn recent(app: &mut App) -> Value {
    let response = app.dispatch(query(app, "inspect"));
    assert!(response.is_ok(), "{response:?}");
    response.result().unwrap().clone()
}
fn snapshot(app: &App) -> (Vec<u8>, u64, (usize, usize), String, String) {
    (
        encode_project(
            app.acquire_snapshot(app.content_revision())
                .unwrap()
                .document(),
        )
        .unwrap(),
        app.content_revision(),
        app.history.history_depths(),
        format!("{:?}", app.requests),
        format!("{:?}", app.history),
    )
}

#[test]
fn diagnostics_orders_terminal_authority_results_and_preserves_retry_revision() {
    let mut app = app();
    let first = set(&app, "ui-40", 40);
    let result = app.dispatch(first.clone());
    assert!(result.is_ok());
    assert!(app.dispatch(set(&app, "mcp-60", 60)).is_ok());
    assert_eq!(app.dispatch(first.clone()), result);
    let mut changed = first;
    changed.payload["value"] = json!(70);
    assert_eq!(
        app.dispatch(changed).error().unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
    for operation in ["history.undo", "history.redo"] {
        let request = OpacityRequest::history_stage(
            operation,
            app.instance_id(),
            app.document_id(),
            Some(app.content_revision()),
            operation,
            json!({}),
        );
        assert!(app.dispatch(request).is_ok());
    }
    let value = recent(&mut app);
    let records = value["records"].as_array().unwrap();
    assert_eq!(records.len(), 6);
    assert_eq!(
        records
            .iter()
            .map(|r| r["sequence"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    assert_eq!(
        records
            .iter()
            .map(|r| r["contentRevision"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2, 1, 2, 3, 4]
    );
    assert_eq!(
        records[0],
        json!({"sequence":1,"requestId":"ui-40","operation":"command.document.apply","targetId":LAYER,"contentRevision":1,"ok":true})
    );
    assert_eq!(records[3]["errorCode"], "invalid_request");
    assert_eq!(records[3]["ok"], false);
    assert_eq!(records[4]["operation"], "history.undo");
    assert_eq!(value["truncated"], false);
}

#[test]
fn diagnostics_queries_never_retain_or_change_document_history_or_trace() {
    let mut app = app();
    assert!(app.dispatch(set(&app, "write", 40)).is_ok());
    let before = snapshot(&app);
    let trace = recent(&mut app);
    for i in 0..100 {
        assert_eq!(
            app.dispatch(query(&app, &format!("query-{i}"))).result(),
            Some(&trace)
        );
    }
    assert_eq!(snapshot(&app), before);
    let collision = app.dispatch(query(&app, "write"));
    assert_eq!(
        collision.error().unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
    for mut invalid in [
        query(&app, "bad-payload"),
        query(&app, "bad-revision"),
        query(&app, "cancelled").cancelled(),
    ] {
        if invalid.request_id == "bad-payload" {
            invalid.payload = json!({"path":"/private/secret"});
        }
        if invalid.request_id == "bad-revision" {
            invalid.expected_revision = Some(1);
        }
        assert!(!app.dispatch(invalid).is_ok());
    }
    assert_eq!(recent(&mut app), trace);
    assert_eq!(snapshot(&app), before);
}

#[test]
fn diagnostics_ring_and_full_envelope_are_bounded_without_slicing_identifiers() {
    let mut app = app();
    for i in 0..65 {
        assert!(app
            .dispatch(set(&app, &format!("edit-{i}"), 30 + i % 50))
            .is_ok());
    }
    assert_eq!(app.diagnostics.retained_len(), 32);
    let value = recent(&mut app);
    let records = value["records"].as_array().unwrap();
    assert!(records.len() <= 32);
    assert_eq!(records.last().unwrap()["sequence"], 65);
    assert_eq!(value["truncated"], true);
    for i in 0..40 {
        let id = format!("x{:0>127}", i);
        assert!(app.dispatch(set(&app, &id, 40)).is_ok());
    }
    let request = query(&app, &"q".repeat(128));
    let response = app.dispatch(request);
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    let records = response.result().unwrap()["records"].as_array().unwrap();
    assert!(!records.is_empty());
    assert!(records
        .iter()
        .all(|r| r["requestId"].as_str().unwrap().len() == 128));
    assert_eq!(records.last().unwrap()["sequence"], 105);
}

#[test]
fn diagnostics_replacement_isolates_a_b_b_and_denies_old_identity() {
    let mut app = app();
    let old = set(&app, "old-a", 40);
    assert!(app.dispatch(old.clone()).is_ok());
    let mut previous = app.document_id().to_owned();
    for _ in 0..2 {
        app.replace_document(decode_project(PROJECT).unwrap())
            .unwrap();
        assert_ne!(app.document_id(), previous);
        previous = app.document_id().to_owned();
        assert_eq!(recent(&mut app), json!({"records":[],"truncated":false}));
        assert_eq!(
            app.dispatch(old.clone()).error().unwrap().code(),
            DispatchErrorCode::WrongDocument
        );
        let mut wrong = set(&app, "other-instance", 50);
        wrong.instance_id = "elsewhere".into();
        assert_eq!(
            app.dispatch(wrong).error().unwrap().code(),
            DispatchErrorCode::WrongInstance
        );
        assert_eq!(recent(&mut app), json!({"records":[],"truncated":false}));
        assert!(app.dispatch(set(&app, "b", 60)).is_ok());
        assert_eq!(recent(&mut app)["records"][0]["sequence"], 1);
    }
}

#[test]
fn diagnostics_allowlist_excludes_payload_errors_reads_and_export_metadata() {
    let mut app = app();
    let mut invalid = set(&app, "bad-value", 101);
    invalid.payload["privatePath"] = json!("/private/secret");
    assert!(!app.dispatch(invalid).is_ok());
    let read = OpacityRequest::query(
        "read",
        app.instance_id(),
        app.document_id(),
        "query.document.revision",
        json!({}),
    );
    assert!(app.dispatch(read).is_ok());
    let job = OpacityRequest::query(
        "job",
        app.instance_id(),
        app.document_id(),
        "job.export.png.status",
        json!({"jobId":"private-job"}),
    );
    assert!(!app.dispatch(job).is_ok());
    let value = recent(&mut app);
    assert_eq!(value["records"].as_array().unwrap().len(), 1);
    let encoded = serde_json::to_string(&value).unwrap();
    for forbidden in ["private", "payload", "value", "message", "details", "jobId"] {
        // request identity is the only caller label retained.
        assert!(!encoded.replace("bad-value", "bad").contains(forbidden));
    }
}

#[test]
fn diagnostics_never_retains_unresolved_payload_targets_or_invalid_request_labels() {
    let mut app = app();
    let mut invalid = set(&app, "bad-target", 40);
    invalid.payload["stableTarget"]["layerUid"] = json!("file:/private/secret");
    assert!(!app.dispatch(invalid).is_ok());
    assert!(recent(&mut app)["records"][0].get("targetId").is_none());
    let invalid_label = set(&app, "-invalid", 40);
    app.dispatch(invalid_label);
    assert_eq!(recent(&mut app)["records"].as_array().unwrap().len(), 1);
}

#[test]
fn diagnostics_omits_imported_path_targets_but_preserves_opaque_request_correlation() {
    let path_uid = "file:/private/secret";
    let project = std::str::from_utf8(PROJECT)
        .unwrap()
        .replace(LAYER, path_uid);
    let mut app = NativeApplication::new(
        "diagnostics-fixture",
        decode_project(project.as_bytes()).unwrap(),
        Unused,
        Unused,
        Unused,
    )
    .unwrap();
    let mut edit = set(&app, "caller:/opaque/correlation", 40);
    edit.payload["stableTarget"]["layerUid"] = json!(path_uid);
    assert!(app.dispatch(edit).is_ok());
    let begin = OpacityRequest::history_stage(
        "path-begin",
        app.instance_id(),
        app.document_id(),
        Some(1),
        "transaction.begin",
        json!({"stableTarget":{"layerUid":path_uid}}),
    );
    let begun = app.dispatch(begin);
    assert!(begun.is_ok());
    let update = OpacityRequest::history_stage(
        "path-update",
        app.instance_id(),
        app.document_id(),
        None,
        "transaction.update",
        json!({"transactionId":begun.result().unwrap()["transactionId"], "value":60}),
    );
    assert!(app.dispatch(update).is_ok());
    let trace = recent(&mut app);
    assert_eq!(trace["records"].as_array().unwrap().len(), 3);
    assert_eq!(
        trace["records"][0]["requestId"],
        "caller:/opaque/correlation"
    );
    assert!(trace["records"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r.get("targetId").is_none()));
    assert!(!serde_json::to_string(&trace).unwrap().contains(path_uid));
}

#[test]
fn diagnostics_transaction_stages_capture_target_and_cancel_without_history_growth() {
    let mut app = app();
    let begin = OpacityRequest::history_stage(
        "begin",
        app.instance_id(),
        app.document_id(),
        Some(0),
        "transaction.begin",
        json!({"stableTarget":{"layerUid":LAYER}}),
    );
    let begun = app.dispatch(begin);
    let transaction = begun.result().unwrap()["transactionId"].clone();
    let update = OpacityRequest::history_stage(
        "update",
        app.instance_id(),
        app.document_id(),
        None,
        "transaction.update",
        json!({"transactionId":transaction, "value":60}),
    );
    assert!(app.dispatch(update).is_ok());
    let status = OpacityRequest::history_stage(
        "status",
        app.instance_id(),
        app.document_id(),
        None,
        "transaction.status",
        json!({"transactionId":transaction}),
    );
    assert!(app.dispatch(status).is_ok());
    let cancel = OpacityRequest::history_stage(
        "cancel",
        app.instance_id(),
        app.document_id(),
        None,
        "transaction.cancel",
        json!({"transactionId":transaction}),
    );
    assert!(app.dispatch(cancel).is_ok());
    let cancelled = set(&app, "cancelled-write", 80).cancelled();
    assert_eq!(
        app.dispatch(cancelled).error().unwrap().code(),
        DispatchErrorCode::CancelledBeforeDispatch
    );
    let trace = recent(&mut app);
    let records = trace["records"].as_array().unwrap();
    assert_eq!(records.len(), 4);
    assert!(records
        .iter()
        .all(|record| record["targetId"] == LAYER && record["contentRevision"] == 0));
    assert_eq!(records[3]["errorCode"], "cancelled_before_dispatch");
    assert_eq!(app.history.history_depths(), (0, 0));
    app.release_authority();
    assert_eq!(
        app.dispatch(query(&app, "released"))
            .error()
            .unwrap()
            .code(),
        DispatchErrorCode::Unavailable
    );
}
