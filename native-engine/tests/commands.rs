use nemo_native_engine::codec::decode_project;
use nemo_native_engine::commands::{
    DispatchErrorCode, NativeOpacityApplication, OpacityRequest, ResponseEnvelope,
    APPLICATION_API_VERSION, OP_QUERY_OPACITY, OP_QUERY_REVISION, OP_QUERY_SNAPSHOT,
};
use serde_json::{json, Value};

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
const LAYER: &str = "r08_curve_layer";

fn application() -> NativeOpacityApplication {
    NativeOpacityApplication::new("native-instance-a", decode_project(PROJECT).unwrap()).unwrap()
}

fn application_with_initial_opacity(value: i64) -> NativeOpacityApplication {
    let mut project: Value = serde_json::from_slice(PROJECT).unwrap();
    project["layers"][0]["motionStatic"]["opacity"][0] = json!(value);
    NativeOpacityApplication::new(
        "native-instance-sequence",
        decode_project(&serde_json::to_vec(&project).unwrap()).unwrap(),
    )
    .unwrap()
}

fn query(
    app: &NativeOpacityApplication,
    id: &str,
    operation: &str,
    payload: Value,
) -> OpacityRequest {
    OpacityRequest::query(id, app.instance_id(), app.document_id(), operation, payload)
}

fn set(app: &NativeOpacityApplication, id: &str, value: i64) -> OpacityRequest {
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

fn value(response: &ResponseEnvelope) -> f64 {
    response.result().unwrap()["value"].as_f64().unwrap()
}

fn stored(app: &mut NativeOpacityApplication, id: &str, at_revision: Option<u64>) -> f64 {
    let mut payload = json!({ "stableTarget": { "layerUid": LAYER } });
    if let Some(revision) = at_revision {
        payload["atRevision"] = json!(revision);
    }
    let request = query(app, id, OP_QUERY_OPACITY, payload);
    value(&app.handle(request))
}

#[test]
fn commands_advance_once_and_keep_old_snapshots_immutable() {
    let mut app = application();
    let old = app.acquire_snapshot(0).unwrap();
    assert_eq!(old.static_opacity(LAYER), Some(25.0));

    let first = set(&app, "set-40", 40);
    let first_response = app.handle(first);
    assert!(first_response.is_ok());
    assert_eq!(first_response.content_revision(), 1);
    assert_eq!(
        first_response.result(),
        Some(&json!({ "applied": true, "historyEntriesAdded": 0 }))
    );
    // N08 freezes the command-result field but N09 exclusively owns actual
    // history entries and transactions, so this staged command reports zero.
    assert_eq!(stored(&mut app, "get-40", None), 40.0);

    let second = set(&app, "set-60", 60);
    assert_eq!(app.handle(second).content_revision(), 2);
    assert_eq!(stored(&mut app, "get-60", None), 60.0);
    assert_eq!(old.static_opacity(LAYER), Some(25.0));
    assert_eq!(
        app.acquire_snapshot(1).unwrap().static_opacity(LAYER),
        Some(40.0)
    );
    assert_eq!(stored(&mut app, "get-old", Some(0)), 25.0);
}

#[test]
fn changed_commands_prove_the_complete_25_to_40_to_60_revision_sequence() {
    let mut app = application_with_initial_opacity(0);
    for (index, opacity) in [25, 40, 60].into_iter().enumerate() {
        let revision = index as u64 + 1;
        let command = set(&app, &format!("set-{opacity}"), opacity);
        let response = app.handle(command);
        assert!(response.is_ok());
        assert_eq!(response.content_revision(), revision);
        assert_eq!(
            response.result(),
            Some(&json!({ "applied": true, "historyEntriesAdded": 0 }))
        );
        assert_eq!(
            stored(&mut app, &format!("get-{opacity}"), None),
            opacity as f64
        );
        assert_eq!(app.content_revision(), revision);
    }
}

#[test]
fn same_value_is_an_explicit_no_op_without_a_revision_or_history_entry() {
    let mut app = application();
    let response = app.handle(set(&app, "same-25", 25));
    assert!(response.is_ok());
    assert_eq!(response.content_revision(), 0);
    assert_eq!(
        response.result(),
        Some(&json!({ "applied": false, "historyEntriesAdded": 0 }))
    );
    assert_eq!(app.content_revision(), 0);
}

#[test]
fn identical_retry_returns_exact_receipt_and_changed_body_is_rejected() {
    let mut app = application();
    let original = set(&app, "retained-command", 40);
    let retained = app.handle(original.clone());
    let later = set(&app, "later-command", 60);
    assert_eq!(app.handle(later).content_revision(), 2);

    assert_eq!(app.handle(original.clone()), retained);
    assert_eq!(app.content_revision(), 2, "retry must not execute again");

    let cancelled_reuse = app.handle(original.clone().cancelled());
    assert_eq!(
        cancelled_reuse.error().unwrap().code(),
        DispatchErrorCode::InvalidRequest,
        "changed cancellation state is changed-body reuse, not fresh cancellation"
    );

    let mut changed = serde_json::to_value(original).unwrap();
    changed["payload"]["value"] = json!(80);
    let changed: OpacityRequest = serde_json::from_value(changed).unwrap();
    let rejected = app.handle(changed);
    assert_eq!(
        rejected.error().unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(app.content_revision(), 2);
    assert_eq!(stored(&mut app, "after-reuse", None), 60.0);
}

#[test]
fn stale_wrong_instance_and_replaced_document_fail_with_authoritative_identity() {
    let mut app = application();
    let stale = set(&app, "stale", 40);
    let advance = set(&app, "advance", 60);
    app.handle(advance);
    let stale_response = app.handle(stale.clone());
    assert_eq!(
        stale_response.error().unwrap().code(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(stale_response.content_revision(), 1);
    let later = set(&app, "later", 40);
    app.handle(later);
    assert_eq!(app.handle(stale.clone()), stale_response);
    let mut changed_stale = serde_json::to_value(stale).unwrap();
    changed_stale["payload"]["value"] = json!(80);
    let changed_stale: OpacityRequest = serde_json::from_value(changed_stale).unwrap();
    assert_eq!(
        app.handle(changed_stale).error().unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );

    let wrong_instance = OpacityRequest::query(
        "wrong-instance",
        "another-instance",
        app.document_id(),
        OP_QUERY_REVISION,
        json!({}),
    );
    assert_eq!(
        app.handle(wrong_instance).error().unwrap().code(),
        DispatchErrorCode::WrongInstance
    );

    let old_document = app.document_id().to_owned();
    let old_request = OpacityRequest::query(
        "old-document",
        app.instance_id(),
        &old_document,
        OP_QUERY_REVISION,
        json!({}),
    );
    app.replace_document(decode_project(PROJECT).unwrap())
        .unwrap();
    let replacement_document = app.document_id().to_owned();
    assert_ne!(replacement_document, old_document);
    let replaced = app.handle(old_request);
    let encoded = serde_json::to_value(&replaced).unwrap();
    assert_eq!(
        replaced.error().unwrap().code(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(replaced.content_revision(), 0);
    assert_eq!(encoded["documentId"], replacement_document);
    assert_eq!(
        encoded["error"]["details"]["requestedDocumentId"],
        old_document
    );
}

#[test]
fn first_pre_dispatch_cancellation_does_not_reserve_the_request_id() {
    let mut app = application();
    let command = set(&app, "cancel-then-send", 40);
    let cancelled = app.handle(command.clone().cancelled());
    assert_eq!(
        cancelled.error().unwrap().code(),
        DispatchErrorCode::CancelledBeforeDispatch
    );
    assert_eq!(app.content_revision(), 0);
    assert!(app.handle(command).is_ok());
    assert_eq!(app.content_revision(), 1);
}

#[test]
fn queries_and_frame_context_view_inputs_never_advance_content() {
    let mut app = application();
    for (id, payload) in [
        (
            "revision-a",
            json!({ "frame": 0, "contextId": "scene-a", "viewGeneration": 1 }),
        ),
        (
            "revision-b",
            json!({ "frame": 20, "contextId": "scene-b", "viewGeneration": 9 }),
        ),
    ] {
        let request = query(&app, id, OP_QUERY_REVISION, payload);
        let response = app.handle(request);
        assert!(response.is_ok());
        assert_eq!(response.content_revision(), 0);
    }
    let snapshot = query(&app, "snapshot", OP_QUERY_SNAPSHOT, json!({}));
    let response = app.handle(snapshot);
    assert_eq!(response.content_revision(), 0);
    assert_eq!(
        response.result().unwrap()["documentSnapshotId"],
        app.acquire_snapshot(0).unwrap().id()
    );
    assert_eq!(stored(&mut app, "opacity", None), 25.0);
    assert_eq!(app.content_revision(), 0);
}

#[test]
fn opacity_revision_and_snapshot_queries_replay_exactly_after_mutation() {
    let mut app = application();
    let opacity = query(
        &app,
        "retained-opacity-query",
        OP_QUERY_OPACITY,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    );
    let revision = query(
        &app,
        "retained-revision-query",
        OP_QUERY_REVISION,
        json!({ "frame": 0, "contextId": "scene-a", "viewGeneration": 1 }),
    );
    let snapshot = query(
        &app,
        "retained-snapshot-query",
        OP_QUERY_SNAPSHOT,
        json!({}),
    );
    let opacity_receipt = app.handle(opacity.clone());
    let revision_receipt = app.handle(revision.clone());
    let snapshot_receipt = app.handle(snapshot.clone());
    assert_eq!(value(&opacity_receipt), 25.0);

    let mutation = set(&app, "query-replay-mutation", 60);
    assert_eq!(app.handle(mutation).content_revision(), 1);
    assert_eq!(app.handle(opacity.clone()), opacity_receipt);
    assert_eq!(app.handle(revision.clone()), revision_receipt);
    assert_eq!(app.handle(snapshot.clone()), snapshot_receipt);
    assert_eq!(app.content_revision(), 1, "query retries cannot mutate");

    for mut changed in [
        serde_json::to_value(opacity).unwrap(),
        serde_json::to_value(revision).unwrap(),
        serde_json::to_value(snapshot).unwrap(),
    ] {
        changed["payload"]["atRevision"] = json!(0);
        let changed: OpacityRequest = serde_json::from_value(changed).unwrap();
        assert_eq!(
            app.handle(changed).error().unwrap().code(),
            DispatchErrorCode::InvalidRequest
        );
    }
    assert_eq!(stored(&mut app, "query-replay-current", None), 60.0);
}

#[test]
fn envelopes_have_exact_v2_shape_and_the_dispatch_code_set_is_frozen() {
    let mut app = application();
    let request = query(&app, "shape", OP_QUERY_REVISION, json!({}));
    let response = serde_json::to_value(app.handle(request)).unwrap();
    assert_eq!(response["apiVersion"], APPLICATION_API_VERSION);
    assert_eq!(response["ok"], true);
    assert!(response.get("result").is_some());
    assert!(response.get("error").is_none());

    let cancelled = query(&app, "cancelled", OP_QUERY_REVISION, json!({})).cancelled();
    let response = serde_json::to_value(app.handle(cancelled)).unwrap();
    assert_eq!(response["ok"], false);
    assert!(response.get("result").is_none());
    assert_eq!(response["error"]["code"], "cancelled_before_dispatch");

    assert_eq!(
        DispatchErrorCode::ALL
            .iter()
            .map(|code| serde_json::to_value(code).unwrap())
            .collect::<Vec<_>>(),
        [
            "invalid_request",
            "wrong_instance",
            "wrong_document",
            "stale_revision",
            "busy_conflict",
            "unavailable",
            "not_found",
            "cancelled_before_dispatch",
            "internal"
        ]
        .map(Value::from)
    );
}

#[test]
fn malformed_targets_operations_and_revisions_have_no_effect() {
    let mut app = application();
    let cases = [
        OpacityRequest::command(
            "missing-layer",
            app.instance_id(),
            app.document_id(),
            0,
            json!({
                "command": "layer.opacity.set",
                "stableTarget": { "layerUid": "missing" },
                "value": 40
            }),
        ),
        OpacityRequest::command(
            "unsupported-command",
            app.instance_id(),
            app.document_id(),
            0,
            json!({
                "command": "layer.position.set",
                "stableTarget": { "layerUid": LAYER },
                "value": 40
            }),
        ),
        OpacityRequest::query(
            "unsupported-operation",
            app.instance_id(),
            app.document_id(),
            "transaction.begin",
            json!({}),
        ),
    ];
    for request in cases {
        assert!(!app.handle(request).is_ok());
        assert_eq!(app.content_revision(), 0);
    }
    let oversized = query(
        &app,
        "oversized",
        OP_QUERY_REVISION,
        json!({ "contextId": "x".repeat(4096) }),
    );
    assert_eq!(
        app.handle(oversized).error().unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(stored(&mut app, "still-25", None), 25.0);
}
