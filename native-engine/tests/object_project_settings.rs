//! Actual-owner oracles for project settings, attached to the existing history module.
use super::*;
use serde_json::{json, Value};
fn project_wire() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":29.97},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,"assets":[]})
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("settings-test", project_wire().to_string().as_bytes())
        .unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    let snapshot = o.acquire_snapshot(o.content_revision()).unwrap();
    serde_json::from_slice(
        &o.serialize_project_file(o.content_revision(), snapshot.snapshot_id())
            .unwrap(),
    )
    .unwrap()
}
fn apply(o: &mut NativeObjectHistory, action: Value) -> Result<u64, DispatchErrorCode> {
    let bytes = json!({"command":"object.project.settings.edit","action":action}).to_string();
    let candidate = o.prepare_project_settings_json(bytes.as_bytes())?;
    o.commit_project_settings(candidate)
}
#[test]
fn settings_are_atomic_persistent_and_old_snapshot_is_immutable() {
    let mut o = owner();
    let pin = o.acquire_snapshot(0).unwrap();
    let before = o.serialize_project_file(0, pin.snapshot_id()).unwrap();
    assert_eq!(
        apply(
            &mut o,
            json!({"kind":"setCanvasSize","width":1920,"height":1080})
        )
        .unwrap(),
        1
    );
    assert_eq!(
        apply(
            &mut o,
            json!({"kind":"setCanvasBackground","color":"#AbCdEF80"})
        )
        .unwrap(),
        2
    );
    assert_eq!(
        apply(
            &mut o,
            json!({"kind":"setWorkArea","inFrame":2.5,"outFrame":45.25})
        )
        .unwrap(),
        3
    );
    let saved = wire(&o);
    assert_eq!(
        saved["project"],
        json!({"canvasW":1920,"canvasH":1080,"fps":29.97,"canvasBg":"#AbCdEF80","waIn":2.5,"waOut":45.25})
    );
    assert_eq!(saved["drawing"], project_wire()["drawing"]);
    assert_eq!(o.history_depths(), (3, 0));
    assert_eq!(
        o.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        before
    );
    let reopened =
        NativeObjectHistory::from_project_file("reopened", saved.to_string().as_bytes()).unwrap();
    assert_eq!(wire(&reopened), saved);
    assert_eq!(o.undo(3).unwrap(), 4);
    assert!(wire(&o)["project"].get("waIn").is_none());
    assert_eq!(o.redo(4).unwrap(), 5);
    assert_eq!(wire(&o), saved);
}
#[test]
fn fps_command_clamps_without_retiming_and_storage_preserves_fractional_values() {
    let mut source = project_wire();
    source["project"]["fps"] = json!(0.5);
    let mut o =
        NativeObjectHistory::from_project_file("fps-test", source.to_string().as_bytes()).unwrap();
    assert_eq!(wire(&o)["project"]["fps"], json!(0.5));
    for (input, expected) in [(-5., 1.), (61., 60.), (29.97, 29.97)] {
        apply(&mut o, json!({"kind":"setFps","fps":input})).unwrap();
        assert_eq!(wire(&o)["project"]["fps"], json!(expected));
        assert_eq!(wire(&o)["drawing"], source["drawing"]);
    }
}
#[test]
fn no_op_preserves_revision_bytes_and_redo() {
    let mut o = owner();
    apply(&mut o, json!({"kind":"setFps","fps":30})).unwrap();
    o.undo(1).unwrap();
    let before = wire(&o);
    assert_eq!(
        apply(&mut o, json!({"kind":"setFps","fps":29.97})).unwrap(),
        2
    );
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(wire(&o), before);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o)["project"]["fps"], json!(30.));
}
#[test]
fn invalid_settings_and_duplicate_raw_members_are_atomic() {
    let mut o = owner();
    let before = wire(&o);
    for action in [
        json!({"kind":"setCanvasSize","width":0,"height":180}),
        json!({"kind":"setCanvasBackground","color":"#ggg"}),
        json!({"kind":"setFps","fps":"24"}),
        json!({"kind":"setWorkArea","inFrame":0,"outFrame":null}),
        json!({"kind":"setTotalFrames","frames":5}),
    ] {
        assert_eq!(
            apply(&mut o, action),
            Err(DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(wire(&o), before);
        assert_eq!(o.content_revision(), 0);
        assert_eq!(o.history_depths(), (0, 0));
    }
    for raw in [
        r#"{"command":"object.project.settings.edit","command":"object.project.settings.edit","action":{"kind":"setFps","fps":24}}"#,
        r#"{"command":"object.project.settings.edit","action":{"kind":"setFps","fps":24,"fps":30}}"#,
    ] {
        assert_eq!(
            o.prepare_project_settings_json(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
    }
}
#[test]
fn stale_and_foreign_prepared_settings_cannot_publish() {
    let mut o = owner();
    let bytes =
        json!({"command":"object.project.settings.edit","action":{"kind":"setFps","fps":30}})
            .to_string();
    let stale = o.prepare_project_settings_json(bytes.as_bytes()).unwrap();
    let foreign = owner()
        .prepare_project_settings_json(bytes.as_bytes())
        .unwrap();
    apply(
        &mut o,
        json!({"kind":"setCanvasSize","width":640,"height":360}),
    )
    .unwrap();
    assert_eq!(
        o.commit_project_settings(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        o.commit_project_settings(foreign),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(o.content_revision(), 1);
    assert_eq!(o.history_depths(), (1, 0));
}

fn f04_request(o: &NativeObjectHistory, id: &str, revision: u64, action: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
      "documentId":o.document_id(),"expectedRevision":revision,
      "operation":"command.document.object.project.settings.edit",
      "payload":{"command":"object.project.settings.edit","action":action}})
}
fn f04_dispatch(o: &mut NativeObjectHistory, request: &Value) -> Value {
    let typed: crate::commands::OpacityRequest = serde_json::from_value(request.clone()).unwrap();
    serde_json::to_value(
        o.try_dispatch_object_command(&typed)
            .expect("bounded identifier admission")
            .expect("Root must join the staged project settings operation"),
    )
    .unwrap()
}
fn f04_state(o: &NativeObjectHistory) -> Value {
    // Failure receipt retention may change; authored state/history must not.
    let snapshot = o.acquire_snapshot(o.content_revision()).unwrap();
    json!({"revision":o.content_revision(),"depths":o.history_depths(),
      "documentId":o.document_id(),"snapshotId":snapshot.snapshot_id(),"file":wire(o)})
}
fn f04_success(response: &Value, request: &Value, revision: u64, applied: bool, entries: u64) {
    assert_eq!(response["ok"], true);
    for key in ["apiVersion", "requestId", "instanceId", "documentId"] {
        assert_eq!(response[key], request[key], "success {key}");
    }
    assert_eq!(response["contentRevision"], revision);
    assert_eq!(
        response["result"],
        json!({"applied":applied,"historyEntriesAdded":entries})
    );
    assert!(response.get("error").is_none());
}
fn f04_failure(
    response: &Value,
    o: &NativeObjectHistory,
    request: &Value,
    revision: u64,
    code: &str,
) {
    assert_eq!(response["apiVersion"], 2);
    assert_eq!(response["requestId"], request["requestId"]);
    // Actual RetainedDisposition::response binds replies to the current owner,
    // even when a preflight request supplied the wrong instance/document.
    assert_eq!(response["instanceId"], o.instance_id());
    assert_eq!(response["documentId"], o.document_id());
    assert_eq!(response["contentRevision"], revision);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], code);
    assert!(response.get("result").is_none());
    assert!(
        response["error"].get("details").is_none(),
        "CompactFailure details=None is omitted, not an invented retry/commit disposition"
    );
}

#[test]
fn f04_all_four_actions_use_real_owner_command_receipts_and_preserve_drawing() {
    let mut o = owner();
    let original = wire(&o);
    for (index, action) in [
        json!({"kind":"setCanvasSize","width":1920,"height":1080}),
        json!({"kind":"setFps","fps":24}),
        json!({"kind":"setCanvasBackground","color":"#AbCdEF80"}),
        json!({"kind":"setWorkArea","inFrame":2.5,"outFrame":45.25}),
    ]
    .into_iter()
    .enumerate()
    {
        let request = f04_request(
            &o,
            &format!("settings-action-{index}"),
            index as u64,
            action,
        );
        let response = f04_dispatch(&mut o, &request);
        f04_success(&response, &request, index as u64 + 1, true, 1);
        assert_eq!(o.history_depths(), (index + 1, 0));
        let current = wire(&o);
        for key in ["drawing", "viewportReference", "assets"] {
            assert_eq!(current[key], original[key], "{key}");
        }
    }
    assert_eq!(
        wire(&o)["project"],
        json!({"canvasW":1920,"canvasH":1080,"fps":24.0,
      "canvasBg":"#AbCdEF80","waIn":2.5,"waOut":45.25})
    );
}

#[test]
fn f04_exact_replay_is_once_only_and_changed_body_cannot_replace_retained_success() {
    let mut o = owner();
    let old = o.acquire_snapshot(0).unwrap();
    let old_bytes = o.serialize_project_file(0, old.snapshot_id()).unwrap();
    let request = f04_request(
        &o,
        "settings-retained",
        0,
        json!({"kind":"setFps","fps":30}),
    );
    let first = f04_dispatch(&mut o, &request);
    f04_success(&first, &request, 1, true, 1);
    let once = f04_state(&o);
    assert_eq!(f04_dispatch(&mut o, &request), first);
    assert_eq!(f04_state(&o), once);
    assert_eq!(o.content_revision(), 1);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        o.serialize_project_file(0, old.snapshot_id()).unwrap(),
        old_bytes
    );
    apply(&mut o, json!({"kind":"setCanvasBackground","color":"#abc"})).unwrap();
    let later = f04_state(&o);
    assert_eq!(o.content_revision(), 2);
    assert_eq!(
        f04_dispatch(&mut o, &request),
        first,
        "replay precedes stale expectedRevision"
    );
    assert_eq!(f04_state(&o), later);
    let mut changed = request.clone();
    changed["expectedRevision"] = json!(2);
    changed["payload"]["action"]["fps"] = json!(24);
    let refused = f04_dispatch(&mut o, &changed);
    f04_failure(&refused, &o, &changed, 2, "invalid_request");
    assert_eq!(f04_state(&o), later);
    assert_eq!(f04_dispatch(&mut o, &request), first);
    assert_eq!(f04_state(&o), later);
}

#[test]
fn f04_cancelled_stale_missing_revision_and_bad_payload_are_atomic_and_retained() {
    for fault in [
        "cancelled-and-stale",
        "stale",
        "missing-revision",
        "bad-action",
    ] {
        let mut o = owner();
        apply(&mut o, json!({"kind":"setFps","fps":30})).unwrap();
        o.undo(1).unwrap();
        assert_eq!(o.content_revision(), 2);
        assert_eq!(o.history_depths(), (0, 1));
        let mut request = f04_request(&o, "settings-failure", 2, json!({"kind":"setFps","fps":24}));
        let code = match fault {
            "cancelled-and-stale" => {
                request["cancelledBeforeDispatch"] = json!(true);
                request["expectedRevision"] = json!(999);
                "cancelled_before_dispatch"
            }
            "stale" => {
                request["expectedRevision"] = json!(1);
                "stale_revision"
            }
            "missing-revision" => {
                request.as_object_mut().unwrap().remove("expectedRevision");
                "invalid_request"
            }
            _ => {
                request["payload"]["action"] =
                    json!({"kind":"setCanvasSize","width":0,"height":180});
                "invalid_request"
            }
        };
        let before = f04_state(&o);
        let first = f04_dispatch(&mut o, &request);
        f04_failure(&first, &o, &request, 2, code);
        assert_eq!(f04_state(&o), before);
        assert_eq!(f04_dispatch(&mut o, &request), first);
        assert_eq!(f04_state(&o), before);
        let corrected = f04_request(&o, "settings-failure", 2, json!({"kind":"setFps","fps":24}));
        let denied = f04_dispatch(&mut o, &corrected);
        f04_failure(&denied, &o, &corrected, 2, "invalid_request");
        assert_eq!(f04_state(&o), before);
        o.redo(2).unwrap();
        let later = f04_state(&o);
        assert_eq!(o.content_revision(), 3);
        assert_eq!(
            f04_dispatch(&mut o, &request),
            first,
            "retained failure keeps its historical revision/code"
        );
        assert_eq!(f04_state(&o), later);
    }
}

#[test]
fn f04_identity_mismatch_is_preflight_only_and_cannot_poison_a_valid_request_id() {
    for (field, value, code) in [
        ("instanceId", "another-instance", "wrong_instance"),
        ("documentId", "another-document", "wrong_document"),
    ] {
        let mut o = owner();
        let valid = f04_request(
            &o,
            "settings-identity",
            0,
            json!({"kind":"setFps","fps":24}),
        );
        let mut bad = valid.clone();
        bad[field] = json!(value);
        let before = f04_state(&o);
        let refused = f04_dispatch(&mut o, &bad);
        f04_failure(&refused, &o, &bad, 0, code);
        assert_eq!(f04_state(&o), before);
        // Existing dispatch checks identity before fingerprint lookup/retention.
        let accepted = f04_dispatch(&mut o, &valid);
        f04_success(&accepted, &valid, 1, true, 1);
        assert_eq!(o.history_depths(), (1, 0));
        assert_eq!(wire(&o)["project"]["fps"].as_f64(), Some(24.));
    }
}

#[test]
fn f04_noop_preserves_redo_and_exact_old_receipt_after_redo() {
    let mut o = owner();
    apply(&mut o, json!({"kind":"setFps","fps":30})).unwrap();
    o.undo(1).unwrap();
    let before = f04_state(&o);
    assert_eq!(o.history_depths(), (0, 1));
    let request = f04_request(&o, "settings-noop", 2, json!({"kind":"setFps","fps":29.97}));
    let first = f04_dispatch(&mut o, &request);
    f04_success(&first, &request, 2, false, 0);
    assert_eq!(f04_state(&o), before);
    assert_eq!(f04_dispatch(&mut o, &request), first);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o)["project"]["fps"].as_f64(), Some(30.));
    let after = f04_state(&o);
    assert_eq!(f04_dispatch(&mut o, &request), first);
    assert_eq!(f04_state(&o), after);
}

#[test]
fn f04_invalid_payload_receipt_remains_invalid_after_later_independent_settings_edit() {
    let mut o = owner();
    let bad = f04_request(
        &o,
        "settings-invalid-kept",
        0,
        json!({"kind":"setCanvasSize","width":0,"height":180}),
    );
    let before = f04_state(&o);
    let first = f04_dispatch(&mut o, &bad);
    f04_failure(&first, &o, &bad, 0, "invalid_request");
    assert_eq!(f04_state(&o), before);
    apply(&mut o, json!({"kind":"setCanvasBackground","color":"#abc"})).unwrap();
    let after = f04_state(&o);
    assert_eq!(o.content_revision(), 1);
    assert_eq!(f04_dispatch(&mut o, &bad), first);
    assert_eq!(f04_state(&o), after);
    let corrected = f04_request(
        &o,
        "settings-invalid-kept",
        1,
        json!({"kind":"setCanvasSize","width":640,"height":360}),
    );
    let refused = f04_dispatch(&mut o, &corrected);
    f04_failure(&refused, &o, &corrected, 1, "invalid_request");
    assert_eq!(f04_state(&o), after);
    assert_eq!(f04_dispatch(&mut o, &bad), first);
    assert_eq!(f04_state(&o), after);
}

#[test]
fn settings_share_all_existing_media_allocations_and_preserve_catalog_identity() {
    use crate::object_document::ProjectReferenceState;
    use std::sync::Arc;
    let mut source = crate::object_codec::encoded_wire_test::wire();
    let png = crate::object_codec::encoded_wire_test::PNG;
    source["formatVersion"] = json!(2);
    source["assets"] = json!([
        {"id":"media-1","name":"Original plate","kind":"image","source":
            {"kind":"embedded","mediaType":"image/png","encoding":"base64","data":png}},
        {"id":"media-2","name":"Second plate","kind":"image","source":
            {"kind":"embedded","mediaType":"image/png","encoding":"base64","data":png}},
        {"id":"font-1","name":"Linked font","kind":"font","source":
            {"kind":"linked","locator":"../Fonts/Mixed CASE.otf"}}
    ]);
    source["assetInventory"] = json!({"assetIds":["font-1","media-2","media-1"],"placements":[]});
    let mut o =
        NativeObjectHistory::from_project_file("settings-media", source.to_string().as_bytes())
            .unwrap();
    let old = o.acquire_snapshot(0).unwrap();
    let old_project = old.project().unwrap();
    let old_inventory = old_project.asset_inventory().unwrap();
    let old_bytes = o.serialize_project_file(0, old.snapshot_id()).unwrap();
    let ProjectReferenceState::StillImage(old_reference) = old_project.reference_state() else {
        panic!("literal fixture has a viewport reference")
    };
    apply(
        &mut o,
        json!({"kind":"setCanvasBackground","color":"#aBcD09eF"}),
    )
    .unwrap();
    let next = o.acquire_snapshot(1).unwrap();
    let next_project = next.project().unwrap();
    assert_eq!(next.document_id(), old.document_id());
    assert!(
        std::ptr::eq(next_project.drawing(), old_project.drawing()),
        "metadata publication must share the existing drawing Arc"
    );
    let next_inventory = next_project.asset_inventory().unwrap();
    assert_eq!(next_inventory, old_inventory);
    for (before, after) in old_inventory.records().iter().zip(next_inventory.records()) {
        match (
            before.source().encoded_allocation(),
            after.source().encoded_allocation(),
        ) {
            (Some(before), Some(after)) => assert!(Arc::ptr_eq(before, after)),
            (None, None) => assert_eq!(before.source().locator(), after.source().locator()),
            _ => panic!("settings edit changed the asset source"),
        }
    }
    let ProjectReferenceState::StillImage(next_reference) = next_project.reference_state() else {
        panic!("settings edit removed the viewport reference")
    };
    assert!(Arc::ptr_eq(
        old_reference.encoded_allocation(),
        next_reference.encoded_allocation()
    ));
    assert_eq!(
        o.serialize_project_file(0, old.snapshot_id()).unwrap(),
        old_bytes
    );
    let saved = wire(&o);
    for key in ["drawing", "viewportReference", "assets", "assetInventory"] {
        let canonical_before: Value = serde_json::from_slice(&old_bytes).unwrap();
        assert_eq!(saved[key], canonical_before[key]);
    }
    assert_eq!(saved["project"]["canvasBg"], "#aBcD09eF");
    let reopened = NativeObjectHistory::from_project_file(
        "settings-media-reopened",
        saved.to_string().as_bytes(),
    )
    .unwrap();
    assert_eq!(wire(&reopened), saved);
    assert_eq!(o.undo(1).unwrap(), 2);
    let undo = o.acquire_snapshot(2).unwrap();
    assert!(std::ptr::eq(
        undo.project().unwrap().drawing(),
        old_project.drawing()
    ));
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o), saved);
}
