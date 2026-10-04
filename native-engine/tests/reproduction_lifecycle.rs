use super::*;

#[test]
fn changed_retry_invalidates_before_any_read_whitelist_and_cannot_be_rearmed() {
    for change in 0..6 {
        let mut app = armed();
        let original = set(&app, "shared-id", json!(40));
        assert!(app.dispatch(original.clone()).is_ok());
        let mut changed = original;
        match change {
            0 => changed.payload["value"] = json!(60),
            1 => changed.expected_revision = Some(1),
            2 => changed.cancelled_before_dispatch = true,
            3 => {
                changed.operation = "query.diagnostics.recent".into();
                changed.expected_revision = None;
                changed.payload = json!({});
            }
            4 => {
                changed.operation = "query.document.revision".into();
                changed.expected_revision = None;
                changed.payload = json!({});
            }
            _ => {
                changed.operation = "query.diagnostics.recent".into();
                changed.payload = json!("malformed private label");
            }
        }
        assert!(!app.dispatch(changed).is_ok());
        assert_invalid(&app, ReproductionReason::ChangedRetry);
        assert_eq!(
            app.opt_in_reproduction(),
            Err(ReproductionReason::AlreadyOptedIn)
        );
        assert!(app
            .dispatch(set(&app, "after-rejection", json!(60)))
            .is_ok());
        assert_eq!(app.content_revision(), 2);
    }
}

#[test]
fn owned_malformed_failed_cancelled_and_oversize_mutations_disable_capture_not_editing() {
    for change in 0..11 {
        let mut app = armed();
        let mut failed = set(&app, "invalid", json!(40));
        match change {
            0 => failed.payload["value"] = json!(101),
            1 => failed.payload["value"] = json!(-1),
            2 => failed.payload["value"] = Value::Null,
            3 => failed.payload["stableTarget"]["layerUid"] = json!("private/target-label"),
            4 => failed.payload["private/path"] = json!("private-secret"),
            5 => failed.cancelled_before_dispatch = true,
            6 => failed.expected_revision = Some(9),
            7 => failed.expected_revision = None,
            8 => failed.api_version = 1,
            9 => failed.payload = Value::String("é\\\"private".repeat(800)),
            _ => failed.payload["stableTarget"]["label"] = json!("private-label"),
        }
        assert!(!app.dispatch(failed).is_ok(), "case {change}");
        assert_invalid(&app, ReproductionReason::FailedMutation);
        assert!(app.dispatch(set(&app, "next-edit", json!(60))).is_ok());
        assert_eq!(app.history.history_depths(), (1, 0));
        let status = serde_json::to_string(&app.reproduction_status()).unwrap();
        assert!(!status.contains("private"));
    }
}

#[test]
fn current_identity_oversize_request_is_denied_at_the_existing_4096_preflight() {
    let mut app = armed();
    let mut oversized = set(&app, "too-large", json!(40));
    oversized.payload["privateExtra"] = json!("é".repeat(3000));
    assert!(serde_json::to_vec(&oversized).unwrap().len() > 4096);
    let response = app.dispatch(oversized);
    assert_eq!(
        response.error().unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(app.content_revision(), 0);
    assert!(app.requests.is_empty());
    assert_invalid(&app, ReproductionReason::FailedMutation);
    assert!(app.dispatch(set(&app, "normal", json!(40))).is_ok());
}

#[test]
fn unknown_queries_transactions_history_and_jobs_are_unsupported_transitions() {
    for operation in [
        "query.private-label",
        "transaction.begin",
        "transaction.update",
        "transaction.commit",
        "transaction.cancel",
        "history.undo",
        "history.redo",
        "job.export.png.begin",
        "job.export.png.status",
        "job.export.png.cancel",
    ] {
        let mut app = armed();
        assert!(app.dispatch(set(&app, "first", json!(40))).is_ok());
        let mut unsupported = request(&app, "unsupported", operation, json!({}));
        unsupported.expected_revision = Some(app.content_revision());
        app.dispatch(unsupported);
        assert_invalid(&app, ReproductionReason::UnsupportedTransition);
        assert!(app.dispatch(set(&app, "after", json!(60))).is_ok());
    }
}

#[test]
fn a_successful_transaction_begin_invalidates_before_following_stages() {
    let mut app = armed();
    let mut begin = request(
        &app,
        "begin",
        "transaction.begin",
        json!({"stableTarget":{"layerUid":TARGET}}),
    );
    begin.expected_revision = Some(0);
    let result = app.dispatch(begin);
    assert!(result.is_ok());
    assert_invalid(&app, ReproductionReason::UnsupportedTransition);
    let id = result.result().unwrap()["transactionId"].clone();
    let cancel = request(
        &app,
        "cancel",
        "transaction.cancel",
        json!({"transactionId":id}),
    );
    assert!(app.dispatch(cancel).is_ok());
    assert!(app
        .dispatch(set(&app, "after-transaction", json!(60)))
        .is_ok());
}

#[test]
fn foreign_identity_cannot_contaminate_even_with_changed_id_and_private_malformed_body() {
    let mut app = armed();
    let original = set(&app, "shared-id", json!(40));
    assert!(app.dispatch(original.clone()).is_ok());
    let before = stores(&app);
    for foreign_instance in [true, false] {
        let mut foreign = original.clone();
        if foreign_instance {
            foreign.instance_id = "other-instance".into();
        } else {
            foreign.document_id = "old-document".into();
        }
        foreign.api_version = 1;
        foreign.operation = "private/unsupported".into();
        foreign.payload = Value::String("private é\\\"".repeat(1000));
        assert!(!app.dispatch(foreign).is_ok());
        assert_eq!(stores(&app), before);
    }
}

#[test]
fn release_at_each_public_stage_disables_export_without_retaining_old_entries() {
    for stage in 0..3 {
        let mut app = armed();
        assert!(app.dispatch(set(&app, "before-release", json!(40))).is_ok());
        match stage {
            0 => {
                app.release_transaction_stage();
            }
            1 => {
                app.release_export_stage();
            }
            _ => {
                app.release_authority();
            }
        }
        assert_invalid(&app, ReproductionReason::Released);
        assert_eq!(
            app.opt_in_reproduction(),
            Err(ReproductionReason::AlreadyOptedIn)
        );
        assert!(!app.dispatch(set(&app, "after-release", json!(60))).is_ok());
    }
}

#[test]
fn replacement_a_b_b_discards_capture_and_late_a_requests_cannot_touch_fresh_b() {
    let mut a = armed();
    let late = set(&a, "late-a", json!(40));
    assert!(a.dispatch(late.clone()).is_ok());
    for _ in 0..2 {
        a.replace_document(decode_project(PROJECT).unwrap())
            .unwrap();
        assert_invalid(&a, ReproductionReason::Replaced);
        assert_eq!(
            a.opt_in_reproduction(),
            Err(ReproductionReason::AlreadyOptedIn)
        );
        assert!(!a.dispatch(late.clone()).is_ok());
        assert_invalid(&a, ReproductionReason::Replaced);
    }
    let mut b = armed();
    let before = stores(&b);
    assert!(!b.dispatch(late).is_ok());
    assert_eq!(stores(&b), before);
    assert!(b.dispatch(set(&b, "fresh-b", json!(60))).is_ok());
    assert_eq!(
        exported(&b)["commands"][0],
        json!({"id":1,"expectedRevision":0,"value":60,"revision":1,"applied":true})
    );
}

#[test]
fn failed_and_unwound_replacement_clear_capture_before_external_cleanup() {
    for panic_cleanup in [false, true] {
        let mut app = app_with(Port {
            fail_cleanup: true,
            panic_cleanup,
        });
        app.opt_in_reproduction().unwrap();
        assert!(app.dispatch(set(&app, "before-failure", json!(40))).is_ok());
        // Inject an export directly into the existing manager so capture remains
        // armed until replace_document itself runs; dispatching a job invalidates earlier.
        let geometry = GeometryPaintInput::new(
            "synthetic-geometry",
            "v1",
            vec![LayerGeometry::new(
                TARGET,
                [20.0, 60.0, 40.0, 80.0],
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                OpaqueSrgbPaint::new(255, 0, 0),
            )
            .unwrap()],
        )
        .unwrap();
        app.exports
            .begin(
                &app.history,
                crate::export_job::ExportBegin {
                    request_id: "synthetic-export".into(),
                    expected_revision: 1,
                    context_id: "synthetic".into(),
                    quality: "final".into(),
                    target: "synthetic-output".into(),
                    frames: vec![crate::export_job::ExportFrameInput::new(0, geometry)],
                },
            )
            .unwrap();
        assert_eq!(
            app.reproduction_status().state,
            ReproductionState::Recording
        );
        let old = app.document_id().to_owned();
        assert!(app
            .replace_document(decode_project(PROJECT).unwrap())
            .is_err());
        assert_eq!(app.document_id(), old);
        assert!(app.replacement_progress().is_some());
        assert_invalid(&app, ReproductionReason::Replaced);
        assert!(app
            .replace_document(decode_project(PROJECT).unwrap())
            .is_err());
        assert_invalid(&app, ReproductionReason::Replaced);
    }
}
