use crate::codec::decode_project;
use crate::commands::{
    DispatchErrorCode, NativeOpacityApplication, OpacityRequest, ResponseEnvelope, OP_QUERY_OPACITY,
};
use crate::history::NativeOpacityHistory;
use crate::transaction::{
    OP_HISTORY_REDO, OP_HISTORY_UNDO, OP_TRANSACTION_BEGIN, OP_TRANSACTION_CANCEL,
    OP_TRANSACTION_COMMIT, OP_TRANSACTION_STATUS, OP_TRANSACTION_UPDATE,
};
use serde_json::{json, Value};

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
const LAYER: &str = "r08_curve_layer";

fn history() -> NativeOpacityHistory {
    NativeOpacityHistory::new("native-history-a", decode_project(PROJECT).unwrap()).unwrap()
}

fn direct() -> NativeOpacityApplication {
    NativeOpacityApplication::new("native-direct-a", decode_project(PROJECT).unwrap()).unwrap()
}

fn set(app: &NativeOpacityHistory, id: &str, value: i64) -> OpacityRequest {
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

fn stage(
    app: &NativeOpacityHistory,
    id: &str,
    expected_revision: Option<u64>,
    operation: &str,
    payload: Value,
) -> OpacityRequest {
    OpacityRequest::history_stage(
        id,
        app.instance_id(),
        app.document_id(),
        expected_revision,
        operation,
        payload,
    )
}

fn begin(app: &NativeOpacityHistory, id: &str, revision: u64) -> OpacityRequest {
    stage(
        app,
        id,
        Some(revision),
        OP_TRANSACTION_BEGIN,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    )
}

fn tx_stage(
    app: &NativeOpacityHistory,
    request_id: &str,
    operation: &str,
    transaction_id: &str,
) -> OpacityRequest {
    stage(
        app,
        request_id,
        None,
        operation,
        json!({ "transactionId": transaction_id }),
    )
}

fn update(
    app: &NativeOpacityHistory,
    request_id: &str,
    transaction_id: &str,
    value: i64,
) -> OpacityRequest {
    stage(
        app,
        request_id,
        None,
        OP_TRANSACTION_UPDATE,
        json!({ "transactionId": transaction_id, "value": value }),
    )
}

fn opacity(app: &mut NativeOpacityHistory, id: &str) -> f64 {
    let request = OpacityRequest::query(
        id,
        app.instance_id(),
        app.document_id(),
        OP_QUERY_OPACITY,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    );
    app.handle(request).result().unwrap()["value"]
        .as_f64()
        .unwrap()
}

fn transaction_id(response: &ResponseEnvelope) -> String {
    response.result().unwrap()["transactionId"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn assert_error(response: &ResponseEnvelope, code: DispatchErrorCode, revision: u64) {
    assert!(!response.is_ok());
    assert_eq!(response.error().unwrap().code(), code);
    assert_eq!(response.content_revision(), revision);
}

#[test]
fn standalone_commands_add_real_entries_and_undo_redo_use_immutable_snapshots() {
    let mut direct = direct();
    let direct_request = OpacityRequest::command(
        "direct-40",
        direct.instance_id(),
        direct.document_id(),
        0,
        json!({
            "command": "layer.opacity.set",
            "stableTarget": { "layerUid": LAYER },
            "value": 40
        }),
    );
    assert_eq!(
        direct.handle(direct_request).result(),
        Some(&json!({ "applied": true, "historyEntriesAdded": 0 }))
    );

    let mut app = history();
    let document_id = app.document_id().to_owned();
    let first_request = set(&app, "set-40", 40);
    let first = app.handle(first_request.clone());
    assert_eq!(first.content_revision(), 1);
    assert_eq!(
        first.result(),
        Some(&json!({ "applied": true, "historyEntriesAdded": 1 }))
    );
    let second = app.handle(set(&app, "set-60", 60));
    assert_eq!(second.content_revision(), 2);
    assert_eq!(opacity(&mut app, "at-60"), 60.0);
    assert_eq!(app.handle(first_request), first);
    assert_eq!(app.content_revision(), 2);

    let undo = stage(&app, "undo", Some(2), OP_HISTORY_UNDO, json!({}));
    let undone = app.handle(undo.clone());
    assert_eq!(undone.content_revision(), 3);
    assert_eq!(
        undone.result(),
        Some(&json!({ "applied": true, "historyEntriesAdded": 0 }))
    );
    assert_eq!(opacity(&mut app, "after-undo"), 40.0);
    let redo = stage(&app, "redo", Some(3), OP_HISTORY_REDO, json!({}));
    let redone = app.handle(redo.clone());
    assert_eq!(redone.content_revision(), 4);
    assert_eq!(
        redone.result(),
        Some(&json!({ "applied": true, "historyEntriesAdded": 0 }))
    );
    assert_eq!(opacity(&mut app, "after-redo"), 60.0);
    assert_eq!(
        app.handle(undo),
        undone,
        "undo retry must not execute after redo"
    );
    assert_eq!(app.handle(redo), redone);
    assert_eq!(
        (app.content_revision(), opacity(&mut app, "after-retries")),
        (4, 60.0)
    );
    assert_eq!(app.history_depths(), (2, 0));
    let same = app.handle(set(&app, "same-60", 60));
    assert_eq!(same.content_revision(), 4);
    assert_eq!(
        same.result(),
        Some(&json!({ "applied": false, "historyEntriesAdded": 0 }))
    );
    assert_eq!(app.history_depths(), (2, 0));
    assert_eq!(app.document_id(), document_id);

    for (revision, expected) in [(0, 25.0), (1, 40.0), (2, 60.0)] {
        assert_eq!(
            app.acquire_snapshot(revision)
                .unwrap()
                .static_opacity(LAYER),
            Some(expected)
        );
        assert_eq!(
            app.acquire_snapshot(revision).unwrap().document().layers()[0].layer_uid(),
            LAYER
        );
    }
}

#[test]
fn transaction_stages_pin_exact_shapes_and_commit_once() {
    let mut app = history();
    let begin = begin(&app, "begin", 0);
    let begun = app.handle(begin.clone());
    let id = transaction_id(&begun);
    assert_eq!(
        begun.result(),
        Some(&json!({
            "transactionId": "native-transaction-1",
            "baseRevision": 0,
            "workingGeneration": 0,
            "workingState": { "stableTarget": { "layerUid": LAYER }, "value": 25 },
            "committedRevision": null,
            "terminalDisposition": null,
            "historyEntriesAdded": 0
        }))
    );
    assert_eq!(app.handle(begin.clone()), begun);

    let update = update(&app, "update", &id, 60);
    let updated = app.handle(update.clone());
    assert_eq!(updated.result().unwrap()["workingGeneration"], 1);
    assert_eq!(updated.result().unwrap()["workingState"]["value"], 60);
    assert_eq!(opacity(&mut app, "query-open"), 25.0);
    assert_eq!(app.content_revision(), 0);
    assert_eq!(app.handle(update.clone()), updated);
    let mut changed: Value = serde_json::to_value(&update).unwrap();
    changed["payload"]["value"] = json!(80);
    assert_error(
        &app.handle(serde_json::from_value(changed).unwrap()),
        DispatchErrorCode::InvalidRequest,
        0,
    );

    let status = tx_stage(&app, "status-open", OP_TRANSACTION_STATUS, &id);
    let open_status = app.handle(status.clone());
    assert_eq!(open_status.result(), updated.result());
    let commit = tx_stage(&app, "commit", OP_TRANSACTION_COMMIT, &id);
    let committed = app.handle(commit.clone());
    assert_eq!(committed.content_revision(), 1);
    assert_eq!(committed.result().unwrap()["committedRevision"], 1);
    assert_eq!(
        committed.result().unwrap()["terminalDisposition"],
        "succeeded"
    );
    assert_eq!(committed.result().unwrap()["historyEntriesAdded"], 1);
    assert_eq!(app.handle(commit), committed);
    assert_eq!(app.handle(begin), begun);
    assert_eq!(app.handle(update), updated);
    assert_eq!(app.handle(status), open_status);
    assert_eq!(app.content_revision(), 1);
    assert_eq!(opacity(&mut app, "after-commit"), 60.0);
    assert_eq!(app.history_depths(), (1, 0));
    assert_eq!(
        app.acquire_snapshot(0).unwrap().static_opacity(LAYER),
        Some(25.0)
    );
}

#[test]
fn cancel_is_revision_neutral_and_only_identical_retry_replays() {
    let mut app = history();
    let begin = begin(&app, "cancel-begin", 0);
    let id = transaction_id(&app.handle(begin));
    let update = update(&app, "cancel-update", &id, 40);
    app.handle(update);
    let cancel = tx_stage(&app, "cancel", OP_TRANSACTION_CANCEL, &id);
    let cancelled = app.handle(cancel.clone());
    assert_eq!(cancelled.content_revision(), 0);
    assert_eq!(
        cancelled.result().unwrap()["terminalDisposition"],
        "cancelled"
    );
    assert_eq!(cancelled.result().unwrap()["historyEntriesAdded"], 0);
    assert_eq!(app.handle(cancel), cancelled);
    assert_eq!(app.history_depths(), (0, 0));
    assert_eq!(opacity(&mut app, "after-cancel"), 25.0);

    for (operation, suffix) in [
        (OP_TRANSACTION_UPDATE, json!({ "value": 60 })),
        (OP_TRANSACTION_COMMIT, json!({})),
        (OP_TRANSACTION_CANCEL, json!({})),
    ] {
        let mut payload = suffix;
        payload["transactionId"] = json!(id);
        let fresh = stage(
            &app,
            &format!("fresh-{operation}"),
            None,
            operation,
            payload,
        );
        assert_error(&app.handle(fresh), DispatchErrorCode::NotFound, 0);
    }
    let terminal_status = tx_stage(&app, "terminal-status", OP_TRANSACTION_STATUS, &id);
    assert_eq!(
        app.handle(terminal_status).result().unwrap()["terminalDisposition"],
        "cancelled"
    );
}

#[test]
fn open_transaction_blocks_conflicting_mutations_without_queueing() {
    let mut app = history();
    let owner_begin = begin(&app, "owner-begin", 0);
    let id = transaction_id(&app.handle(owner_begin));
    let second_begin = begin(&app, "other-begin", 0);
    let command = set(&app, "busy-command", 60);
    let undo = stage(&app, "busy-undo", Some(0), OP_HISTORY_UNDO, json!({}));
    let redo = stage(&app, "busy-redo", Some(0), OP_HISTORY_REDO, json!({}));
    for request in [second_begin, command, undo, redo] {
        assert_error(&app.handle(request), DispatchErrorCode::BusyConflict, 0);
    }
    assert_eq!(app.content_revision(), 0);
    assert_eq!(opacity(&mut app, "still-base"), 25.0);
    let cancel = tx_stage(&app, "owner-cancel", OP_TRANSACTION_CANCEL, &id);
    assert!(app.handle(cancel).is_ok());
    assert_eq!(opacity(&mut app, "never-queued"), 25.0);
}

#[test]
fn unchanged_commit_and_inactive_stages_fail_without_effect() {
    let mut app = history();
    let missing_update = stage(
        &app,
        "missing-update",
        None,
        OP_TRANSACTION_UPDATE,
        json!({ "transactionId": "missing", "value": 60 }),
    );
    let missing_commit = stage(
        &app,
        "missing-commit",
        None,
        OP_TRANSACTION_COMMIT,
        json!({ "transactionId": "missing" }),
    );
    for request in [missing_update, missing_commit] {
        assert_error(&app.handle(request), DispatchErrorCode::NotFound, 0);
    }
    let begin = begin(&app, "unchanged-begin", 0);
    let id = transaction_id(&app.handle(begin));
    let commit = tx_stage(&app, "unchanged-commit", OP_TRANSACTION_COMMIT, &id);
    assert_error(&app.handle(commit), DispatchErrorCode::InvalidRequest, 0);
    assert_eq!(app.history_depths(), (0, 0));
    assert_eq!(opacity(&mut app, "unchanged-value"), 25.0);
}

#[test]
fn begin_distinguishes_missing_target_from_admitted_keyed_only_layer() {
    let mut project: Value = serde_json::from_slice(PROJECT).unwrap();
    project["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("motionStatic");
    let document = decode_project(&serde_json::to_vec(&project).unwrap()).unwrap();
    assert!(document.layers()[0].static_opacity().is_none());
    assert!(document.layers()[0].opacity_keys().is_some());
    let mut app = NativeOpacityHistory::new("native-keyed-a", document).unwrap();

    for (request_id, layer_uid, code) in [
        (
            "missing-begin",
            "missing-layer",
            DispatchErrorCode::NotFound,
        ),
        ("keyed-begin", LAYER, DispatchErrorCode::Unavailable),
    ] {
        let request = stage(
            &app,
            request_id,
            Some(0),
            OP_TRANSACTION_BEGIN,
            json!({ "stableTarget": { "layerUid": layer_uid } }),
        );
        assert_error(&app.handle(request), code, 0);
    }
}

#[test]
fn stale_begin_and_stale_transaction_base_are_rejected() {
    let mut app = history();
    let stale_begin = begin(&app, "stale-begin", 1);
    let stale_failure = app.handle(stale_begin.clone());
    assert_error(&stale_failure, DispatchErrorCode::StaleRevision, 0);
    let begin = begin(&app, "base-begin", 0);
    let id = transaction_id(&app.handle(begin));
    app.handle(update(&app, "base-update", &id, 60));
    let behind_facade = OpacityRequest::command(
        "behind-facade",
        app.instance_id(),
        app.document_id(),
        0,
        json!({
            "command": "layer.opacity.set",
            "stableTarget": { "layerUid": LAYER },
            "value": 40
        }),
    );
    assert!(app.test_application_mut().handle(behind_facade).is_ok());
    assert_eq!(app.handle(stale_begin), stale_failure);
    let commit = tx_stage(&app, "stale-base-commit", OP_TRANSACTION_COMMIT, &id);
    assert_error(&app.handle(commit), DispatchErrorCode::StaleRevision, 1);
    assert_eq!(app.history_depths(), (0, 0));
    assert_eq!(opacity(&mut app, "stale-base-value"), 40.0);
}

#[test]
fn retries_are_global_changed_body_is_rejected_and_cancellation_is_unretained() {
    let mut app = history();
    let query = OpacityRequest::query(
        "shared-id",
        app.instance_id(),
        app.document_id(),
        OP_QUERY_OPACITY,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    );
    assert!(app.handle(query).is_ok());
    let changed_stage = stage(
        &app,
        "shared-id",
        Some(0),
        OP_TRANSACTION_BEGIN,
        json!({ "stableTarget": { "layerUid": LAYER } }),
    );
    assert_error(
        &app.handle(changed_stage),
        DispatchErrorCode::InvalidRequest,
        0,
    );

    let begin = begin(&app, "cancel-before", 0);
    assert_error(
        &app.handle(begin.clone().cancelled()),
        DispatchErrorCode::CancelledBeforeDispatch,
        0,
    );
    assert!(app.handle(begin).is_ok());
}

#[test]
fn replacement_clears_transactions_history_and_receipts_before_old_retry_lookup() {
    let mut app = history();
    let old_document = app.document_id().to_owned();
    let old = set(&app, "retained-old", 40);
    assert!(app.handle(old.clone()).is_ok());
    let begin = begin(&app, "open-old", 1);
    let old_transaction = transaction_id(&app.handle(begin));
    app.replace_document(decode_project(PROJECT).unwrap())
        .unwrap();
    assert_ne!(app.document_id(), old_document);
    let replaced = app.handle(old);
    assert_error(&replaced, DispatchErrorCode::WrongDocument, 0);
    assert_eq!(
        serde_json::to_value(replaced).unwrap()["error"]["details"]["requestedDocumentId"],
        old_document
    );
    assert_eq!(app.history_depths(), (0, 0));
    let stale_transaction = tx_stage(
        &app,
        "old-status-new-document",
        OP_TRANSACTION_STATUS,
        &old_transaction,
    );
    assert_error(
        &app.handle(stale_transaction),
        DispatchErrorCode::NotFound,
        0,
    );
    let undo = stage(&app, "empty-new-undo", Some(0), OP_HISTORY_UNDO, json!({}));
    assert_error(&app.handle(undo), DispatchErrorCode::Unavailable, 0);
}
