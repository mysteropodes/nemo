//! Frozen freshness oracles against the same authority as real writes.
use super::*;

fn report(app: &mut App, revision: u64, sequence: u64) -> ResponseEnvelope {
    app.dispatch(request(
        app,
        "report",
        "query.reproduction.report",
        json!({
            "expectedContentRevision": revision, "expectedSequence": sequence
        }),
    ))
}

fn unchanged_rejection(app: &mut App, query: OpacityRequest, code: DispatchErrorCode) {
    let before = stores(app);
    let trace = app.dispatch(request(app, "trace", "query.diagnostics.recent", json!({})));
    let response = app.dispatch(query);
    assert_eq!(response.error().unwrap().code(), code, "{response:?}");
    assert!(response.result().is_none());
    assert_eq!(stores(app), before);
    assert_eq!(
        app.dispatch(request(app, "trace", "query.diagnostics.recent", json!({}))),
        trace
    );
}

fn report_query(app: &App, revision: u64, sequence: u64) -> OpacityRequest {
    request(
        app,
        "report",
        "query.reproduction.report",
        json!({
            "expectedContentRevision": revision, "expectedSequence": sequence
        }),
    )
}

#[test]
fn report_requires_current_revision_and_attempt_sequence_without_mutating_any_store() {
    let mut app = armed();
    let first = set(&app, "first", json!(40));
    let first_result = app.dispatch(first.clone());
    assert!(first_result.is_ok());
    assert!(app.dispatch(set(&app, "second", json!(60))).is_ok());
    assert!(app.dispatch(set(&app, "noop", json!(60))).is_ok());
    let bundle = exported(&app);
    let before = stores(&app);
    let response = report(&mut app, 2, 3);
    assert!(response.is_ok(), "{response:?}");
    assert_eq!(response.content_revision(), 2);
    assert_eq!(
        response.result(),
        Some(&json!({
            "bundle": bundle, "verifiedContentRevision": 2, "verifiedSequence": 3
        }))
    );
    assert_eq!(stores(&app), before);
    assert_eq!(app.dispatch(first), first_result);
    assert_eq!(app.content_revision(), 2);
    assert_eq!(app.history.history_depths(), (2, 0));
    let before = stores(&app);
    assert!(!report(&mut app, 2, 3).is_ok());
    assert!(!report(&mut app, 1, 4).is_ok());
    assert_eq!(report(&mut app, 2, 4).result().unwrap()["bundle"], bundle);
    assert_eq!(stores(&app), before);
}

#[test]
fn report_invalid_tokens_shapes_cancellation_and_write_collisions_are_non_mutating() {
    let mut app = armed();
    assert!(app.dispatch(set(&app, "write", json!(40))).is_ok());
    for payload in [
        json!({}),
        json!([]),
        json!({"expectedContentRevision":1}),
        json!({"expectedContentRevision":1,"expectedSequence":1,"extra":true}),
        json!({"expectedContentRevision":1,"expectedSequence":1.5}),
        json!({"expectedContentRevision":null,"expectedSequence":1}),
        json!({"expectedContentRevision":1,"expectedSequence":-1}),
        json!({"expectedContentRevision":1,"expectedSequence":9007199254740992_u64}),
        json!({"expectedContentRevision":9007199254740992_u64,"expectedSequence":1}),
        json!({"expectedContentRevision":1,"expectedSequence":1,"private": "é".repeat(3000)}),
    ] {
        let query = request(&app, "malformed", "query.reproduction.report", payload);
        unchanged_rejection(&mut app, query, DispatchErrorCode::InvalidRequest);
    }
    for scenario in 0..6 {
        let mut query = report_query(&app, 1, 1);
        let code = match scenario {
            0 => {
                query.expected_revision = Some(1);
                DispatchErrorCode::InvalidRequest
            }
            1 => {
                query.request_id = "write".into();
                DispatchErrorCode::InvalidRequest
            }
            2 => {
                query.cancelled_before_dispatch = true;
                DispatchErrorCode::CancelledBeforeDispatch
            }
            3 => {
                query.document_id = "old-document".into();
                DispatchErrorCode::WrongDocument
            }
            4 => {
                query.instance_id = "other-instance".into();
                DispatchErrorCode::WrongInstance
            }
            _ => {
                query.api_version = 1;
                DispatchErrorCode::InvalidRequest
            }
        };
        unchanged_rejection(&mut app, query, code);
    }
    assert!(report(&mut app, 1, 1).is_ok());
}

#[test]
fn invalid_journal_denies_even_a_matching_revision_and_sequence() {
    for operation in [
        "job.export.png.status",
        "history.undo",
        "history.redo",
        "transaction.begin",
    ] {
        let mut app = armed();
        assert!(app.dispatch(set(&app, "first", json!(40))).is_ok());
        let mut transition = request(
            &app,
            "transition",
            operation,
            if operation == "transaction.begin" {
                json!({"stableTarget":{"layerUid":TARGET}})
            } else if operation.starts_with("job.") {
                json!({"jobId":"absent"})
            } else {
                json!({})
            },
        );
        if !operation.starts_with("job.") {
            transition.expected_revision = Some(1);
        }
        app.dispatch(transition);
        let revision = if operation == "history.undo" { 2 } else { 1 };
        let sequence = if operation.starts_with("job.") { 1 } else { 2 };
        assert_eq!(app.content_revision(), revision);
        assert_eq!(app.diagnostics.report_sequence(), Some(sequence));
        let query = report_query(&app, revision, sequence);
        unchanged_rejection(&mut app, query, DispatchErrorCode::Unavailable);
    }
    for changed_retry in [false, true] {
        let mut app = armed();
        let first = set(&app, "first", json!(40));
        assert!(app.dispatch(first.clone()).is_ok());
        let mut failed = if changed_retry {
            first
        } else {
            set(&app, "failed", json!(101))
        };
        failed.payload["value"] = json!(101);
        assert!(!app.dispatch(failed).is_ok());
        let query = report_query(&app, 1, 2);
        unchanged_rejection(&mut app, query, DispatchErrorCode::Unavailable);
    }
}

#[test]
fn successful_history_and_transaction_stages_never_revalidate_a_report() {
    let mut app = armed();
    assert!(app.dispatch(set(&app, "first", json!(40))).is_ok());
    for (operation, revision, sequence) in [("history.undo", 2, 2), ("history.redo", 3, 3)] {
        let mut command = request(&app, operation, operation, json!({}));
        command.expected_revision = Some(app.content_revision());
        assert!(app.dispatch(command).is_ok());
        assert_eq!(app.content_revision(), revision);
        let query = report_query(&app, revision, sequence);
        unchanged_rejection(&mut app, query, DispatchErrorCode::Unavailable);
    }
    for commit in [false, true] {
        let mut app = armed();
        assert!(app.dispatch(set(&app, "first", json!(40))).is_ok());
        let mut begin = request(
            &app,
            "begin",
            "transaction.begin",
            json!({"stableTarget":{"layerUid":TARGET}}),
        );
        begin.expected_revision = Some(1);
        let begun = app.dispatch(begin);
        assert!(begun.is_ok());
        let id = begun.result().unwrap()["transactionId"].clone();
        let query = report_query(&app, 1, 2);
        unchanged_rejection(&mut app, query, DispatchErrorCode::Unavailable);
        assert!(app
            .dispatch(request(
                &app,
                "update",
                "transaction.update",
                json!({"transactionId":id,"value":60})
            ))
            .is_ok());
        let query = report_query(&app, 1, 3);
        unchanged_rejection(&mut app, query, DispatchErrorCode::Unavailable);
        let operation = if commit {
            "transaction.commit"
        } else {
            "transaction.cancel"
        };
        assert!(app
            .dispatch(request(
                &app,
                "finish",
                operation,
                json!({"transactionId":id})
            ))
            .is_ok());
        let revision = if commit { 2 } else { 1 };
        assert_eq!(app.content_revision(), revision);
        let query = report_query(&app, revision, 4);
        unchanged_rejection(&mut app, query, DispatchErrorCode::Unavailable);
    }
}

#[test]
fn report_denies_ordinary_import_empty_disabled_released_replaced_and_old_reentry() {
    let mut ordinary = App::new(
        "private-instance",
        decode_project(include_bytes!("../fixtures/reproduction-opacity-v1.json")).unwrap(),
        Port::default(),
        Compositor,
        Resolver,
    )
    .unwrap();
    let query = report_query(&ordinary, 0, 0);
    unchanged_rejection(&mut ordinary, query, DispatchErrorCode::Unavailable);
    for mut app in [app(), armed()] {
        let query = report_query(&app, 0, 0);
        unchanged_rejection(&mut app, query, DispatchErrorCode::Unavailable);
    }
    let mut old = armed();
    assert!(old.dispatch(set(&old, "first", json!(40))).is_ok());
    let late = report_query(&old, 1, 1);
    old.release_authority();
    assert!(!old.dispatch(late.clone()).is_ok());
    let mut fresh = armed();
    let before = stores(&fresh);
    assert_eq!(
        fresh.dispatch(late.clone()).error().unwrap().code(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(stores(&fresh), before);
    assert!(fresh.dispatch(set(&fresh, "fresh", json!(60))).is_ok());
    assert!(report(&mut fresh, 1, 1).is_ok());
    fresh
        .replace_document(decode_project(PROJECT).unwrap())
        .unwrap();
    assert!(!report(&mut fresh, 0, 0).is_ok());
    assert!(!fresh.dispatch(late).is_ok());
}

#[test]
fn report_refuses_sequence_ceiling_before_and_after_unrecordable_attempts() {
    let mut app = armed();
    let first = set(&app, "first", json!(40));
    assert!(app.dispatch(first.clone()).is_ok());
    app.diagnostics.set_sequence_for_test(9_007_199_254_740_990);
    assert!(report(&mut app, 1, 9_007_199_254_740_990).is_ok());
    assert!(app.dispatch(first.clone()).is_ok());
    for _ in 0..2 {
        let query = report_query(&app, 1, 9_007_199_254_740_991);
        unchanged_rejection(&mut app, query, DispatchErrorCode::Unavailable);
        assert!(app.dispatch(first.clone()).is_ok());
    }
    assert_eq!(
        app.reproduction_status().command_count,
        1,
        "A4 remains exportable"
    );
    assert!(app.export_reproduction_bundle().is_ok());
}

#[test]
fn report_maximum_bundle_envelope_and_adjacent_f64_are_exact() {
    let mut app = App::from_reproduction_fixture(
        "i".repeat(128),
        REPRODUCTION_FIXTURE,
        Port::default(),
        Compositor,
        Resolver,
    )
    .unwrap();
    app.opt_in_reproduction().unwrap();
    let tiny: f64 = "1.2500000000000003e-300".parse().unwrap();
    let next: f64 = "1.2500000000000005e-300".parse().unwrap();
    assert_ne!(tiny, next);
    for (index, value) in [tiny, next, tiny].into_iter().enumerate() {
        assert!(app
            .dispatch(set(&app, &format!("float-{index}"), json!(value)))
            .is_ok());
    }
    let response = report(&mut app, 3, 3);
    let bytes = serde_json::to_vec(&response).unwrap();
    let decoded: Value = serde_json::from_slice(&bytes).unwrap();
    for (command, value) in decoded["result"]["bundle"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .zip([tiny, next, tiny])
    {
        assert_eq!(command["value"].as_f64(), Some(value));
    }
    let mut app = armed();
    for index in 0..32 {
        assert!(app
            .dispatch(set(&app, &format!("noop-{index}"), json!(25)))
            .is_ok());
    }
    let mut query = report_query(&app, 0, 32);
    query.request_id = "r".repeat(128);
    let response = app.dispatch(query);
    assert!(response.is_ok());
    let bundle_bytes = serde_json::to_vec(&response.result().unwrap()["bundle"])
        .unwrap()
        .len();
    assert!(bundle_bytes <= 3072);
    let mut envelope = serde_json::to_value(response).unwrap();
    envelope["documentId"] = json!("d".repeat(128));
    envelope["instanceId"] = json!("i".repeat(128));
    envelope["contentRevision"] = json!(9_007_199_254_740_991_u64);
    envelope["result"]["verifiedContentRevision"] = envelope["contentRevision"].clone();
    envelope["result"]["verifiedSequence"] = json!(9_007_199_254_740_990_u64);
    assert!(serde_json::to_vec(&envelope).unwrap().len() - bundle_bytes + 3072 <= 4096);
    assert!(app.dispatch(set(&app, "overflow", json!(25))).is_ok());
    assert!(!report(&mut app, 0, 33).is_ok());
    let mut app = armed();
    for sequence in 1..=32 {
        let value = if sequence % 2 == 0 { tiny } else { next };
        assert!(app
            .dispatch(set(&app, &format!("bytes-{sequence}"), json!(value)))
            .is_ok());
        if app.reproduction_status().state == ReproductionState::Invalid {
            assert_invalid(&app, ReproductionReason::ByteLimit);
            assert!(!report(&mut app, sequence, sequence).is_ok());
            return;
        }
        assert!(report(&mut app, sequence, sequence).is_ok());
    }
    panic!("long commands must exhaust bytes before command count");
}

#[test]
fn authority_lock_serializes_report_against_same_revision_retry_in_both_orders() {
    use std::sync::{mpsc, Mutex};
    let mut app = armed();
    let first = set(&app, "first", json!(40));
    assert!(app.dispatch(first.clone()).is_ok());
    let authority = Mutex::new(app);
    std::thread::scope(|scope| {
        let authority = &authority;
        let mut writer = authority.lock().unwrap();
        let (ready, started) = mpsc::channel();
        let query = scope.spawn(move || {
            ready.send(()).unwrap();
            report(&mut authority.lock().unwrap(), 1, 1)
        });
        started.recv().unwrap();
        assert!(writer.dispatch(first.clone()).is_ok());
        drop(writer);
        assert_eq!(
            query.join().unwrap().error().unwrap().code(),
            DispatchErrorCode::BusyConflict
        );
    });
    std::thread::scope(|scope| {
        let authority = &authority;
        let mut reader = authority.lock().unwrap();
        let (ready, started) = mpsc::channel();
        let writer = scope.spawn(move || {
            ready.send(()).unwrap();
            authority.lock().unwrap().dispatch(first)
        });
        started.recv().unwrap();
        let accepted = report(&mut reader, 1, 2);
        assert_eq!(accepted.result().unwrap()["verifiedSequence"], 2);
        drop(reader);
        assert!(writer.join().unwrap().is_ok());
        assert!(!report(&mut authority.lock().unwrap(), 1, 2).is_ok());
    });
}
