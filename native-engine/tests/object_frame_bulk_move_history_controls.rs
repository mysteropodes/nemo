//! Genuine shared-dispatch prepublication refusal and replay controls for ordinary Move.
use super::*;
#[test]
fn cancelled_stale_and_changed_body_move_never_publish_and_retain_original_disposition() {
    let mut o = owner(fixture::document(
        "K=a.1.10;K=b.2.20;.;.|K;.;.;.",
        build::Build::V2Extras,
    ));
    let mut cancelled = moving(&o, "cancel", &[("L0", 0)], "L1", 1);
    cancelled["cancelledBeforeDispatch"] = json!(true);
    let stale = moving(&o, "stale", &[("L0", 1)], "L1", 2);
    let before = state(&o);
    let refusal = dispatch(&mut o, &cancelled);
    assert_eq!(refusal["error"]["code"], "cancelled_before_dispatch");
    assert_eq!(state(&o), before);
    let q = moving(&o, "move", &[("L0", 0)], "L1", 1);
    let reply = dispatch(&mut o, &q);
    success(&reply, json!([{"layerUid":"L1","frame":1}]));
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &cancelled), refusal);
    assert_eq!(state(&o), after);
    assert_eq!(dispatch(&mut o, &stale)["error"]["code"], "stale_revision");
    assert_eq!(state(&o), after);
    let mut changed = q.clone();
    changed["payload"]["frame"] = json!(2);
    assert_eq!(
        dispatch(&mut o, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after);
    history(&mut o, "undo-move", "undo");
    let undone = state(&o);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), undone);
}
#[test]
fn move_wrong_owner_missing_revision_and_closed_payload_refusals_are_atomic() {
    for kind in [
        "instance",
        "document",
        "revision",
        "command",
        "extra",
        "layer",
        "frame",
        "duplicate",
    ] {
        let mut o = owner(fixture::document(
            "K=a.1.10;.;.;.|K;.;.;.",
            build::Build::V1,
        ));
        let mut q = moving(&o, "bad", &[("L0", 0)], "L1", 1);
        let before = state(&o);
        let expected = match kind {
            "instance" => {
                q["instanceId"] = json!("wrong");
                "wrong_instance"
            }
            "document" => {
                q["documentId"] = json!("wrong");
                "wrong_document"
            }
            "revision" => {
                q.as_object_mut().unwrap().remove("expectedRevision");
                "invalid_request"
            }
            "command" => {
                q["payload"]["command"] = json!("object.layer.frames.shift");
                "invalid_request"
            }
            "extra" => {
                q["payload"]["delta"] = json!(1);
                "invalid_request"
            }
            "layer" => {
                q["payload"]["layerUid"] = json!("missing");
                "not_found"
            }
            "frame" => {
                q["payload"]["frame"] = json!(4294967296u64);
                "invalid_request"
            }
            _ => {
                q["payload"]["selection"] =
                    json!([{"layerUid":"L0","frame":0},{"layerUid":"L0","frame":0}]);
                "invalid_request"
            }
        };
        assert_eq!(dispatch(&mut o, &q)["error"]["code"], expected, "{kind}");
        assert_eq!(state(&o), before);
    }
}
#[test]
fn eligible_ripple_with_oversized_actual_selection_refuses_before_any_mutation_and_replays() {
    // A one-cell request legitimately ripples 180 blank keys. The result, not ingress, exceeds4096.
    let row = (0..180)
        .map(|_| "K")
        .chain(std::iter::once("."))
        .collect::<Vec<_>>()
        .join(";");
    let mut o = owner(fixture::document(&row, build::Build::V2));
    let before = state(&o);
    let q = moving(&o, "large-ripple", &[("L0", 0)], "L0", 1);
    assert!(serde_json::to_vec(&q).unwrap().len() < 4096);
    let candidate = o
        .prepare_frame_move_json(&serde_json::to_vec(&q["payload"]).unwrap())
        .unwrap();
    assert!(candidate.checkpoint_required());
    assert_eq!(candidate.selection().unwrap().len(), 180);
    let result = CompactResult::FrameBulkMove {
        applied: true,
        history_entries_added: 1,
        selection: serde_json::to_value(candidate.selection()).unwrap(),
    };
    let prospective = RetainedDisposition::Exact {
        content_revision: 1,
        outcome: Ok(result),
    };
    let typed: OpacityRequest = serde_json::from_value(q.clone()).unwrap();
    assert!(o.object_reply(&typed, &prospective).is_err());
    assert_eq!(state(&o), before);
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["error"]["code"], "invalid_request");
    assert_eq!(state(&o), before);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), before);
}
#[test]
fn move_keeps_inclusive_request_bound_and_stale_prepared_candidate_adds_no_history() {
    let mut o = owner(fixture::document(
        "K=a.1.10;.;.;.|K;.;.;.",
        build::Build::V1Ref,
    ));
    let mut q = moving(&o, "bound-a", &[("L0", 0)], "missing", 1);
    let size = serde_json::to_vec(&q).unwrap().len();
    q["payload"]["layerUid"] = json!(format!("missing{}", "x".repeat(4096 - size)));
    let typed: OpacityRequest = serde_json::from_value(q.clone()).unwrap();
    assert_eq!(serde_json::to_vec(&typed).unwrap().len(), 4096);
    let before = state(&o);
    assert_eq!(dispatch(&mut o, &q)["error"]["code"], "not_found");
    assert_eq!(state(&o), before);
    q["requestId"] = json!("bound-b");
    q["payload"]["layerUid"] = json!(q["payload"]["layerUid"].as_str().unwrap().to_owned() + "x");
    assert_eq!(dispatch(&mut o, &q)["error"]["code"], "invalid_request");
    assert_eq!(state(&o), before);
    let candidate = o
        .prepare_frame_move_json(
            &fixture::moving(&[("L0", 0)], "L1", 1)
                .to_string()
                .into_bytes(),
        )
        .unwrap();
    let q = moving(&o, "other", &[("L0", 0)], "L1", 2);
    assert_eq!(dispatch(&mut o, &q)["ok"], true);
    let after = state(&o);
    assert_eq!(
        o.commit_frame_move(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&o), after);
}
#[test]
fn shift_operation_remains_undeclared_and_cannot_enter_move_history_wrapper() {
    let mut o = owner(fixture::document("K=a.1.10;.;.;.", build::Build::V1));
    let before = state(&o);
    let payload = fixture::shifting("L0", 1);
    assert_eq!(
        o.prepare_frame_move_json(&serde_json::to_vec(&payload).unwrap())
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    let mut q = request(&o, "shift", payload);
    q["operation"] = json!("command.document.object.layer.frames.shift");
    let typed: OpacityRequest = serde_json::from_value(q).unwrap();
    assert!(o.try_dispatch_object_command(&typed).unwrap().is_none());
    assert_eq!(state(&o), before);
}

#[test]
fn absolute_anchor_beyond_timeline_crops_all_sources_and_returns_empty_selection_once() {
    let input = fixture::document("K=a.1.10;.;.;.|K;.;.;.", build::Build::V2Extras);
    let mut o = owner(input.clone());
    let before = bytes(&o);
    let retained = o.acquire_snapshot(0).unwrap();
    // An absolute u32 anchor equal to totalFrames is legal; the source has no later ripple keys.
    let q = moving(&o, "all-cropped", &[("L0", 0)], "L0", 4);
    let reply = dispatch(&mut o, &q);
    success(&reply, json!([]));
    assert_eq!(o.content_revision(), 1);
    assert_eq!(o.history_depths(), (1, 0));
    fixture::assert_rows(&input, &head(&o), ".;.;.;.|K;.;.;.", build::Build::V2Extras);
    let after = bytes(&o);
    assert_ne!(after, before);
    let done = state(&o);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), done);
    history(&mut o, "undo-all-cropped", "undo");
    assert_eq!(bytes(&o), before);
    history(&mut o, "redo-all-cropped", "redo");
    assert_eq!(bytes(&o), after);
    assert_eq!(
        bytes(&owner(decode_storage_project(&after).unwrap())),
        after
    );
    assert_eq!(encode_storage_project(retained.document()).unwrap(), before);
}
