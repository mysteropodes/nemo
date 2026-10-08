//! Independent history-command oracle through the actual raw-installed owner.
use crate::{
    application_mcp::ApplicationMcp,
    native_application::{admit_release_request, complete_release},
    native_application_contract::NativeReleaseRequest,
    native_dispatch::{NativeObjectHost, ReleaseAdmission},
    native_object_bootstrap::bootstrap_raw,
};
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn document() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases()["records"]})
}
fn install(state: &ApplicationMcp) -> Value {
    let raw = json!({"apiVersion":2,"requestId":"bootstrap-history",
        "instanceId":state.instance_id(),"documentJson":document().to_string()});
    serde_json::to_value(bootstrap_raw(state, &raw.to_string()).unwrap()).unwrap()
}
fn request(receipt: &Value, id: &str, revision: u64, operation: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"expectedRevision":revision,
        "operation":operation,"payload":payload})
}
fn history(receipt: &Value, id: &str, revision: u64, action: &str) -> Value {
    request(
        receipt,
        id,
        revision,
        &format!("command.document.object.{action}"),
        json!({"command":format!("object.{action}")}),
    )
}
fn dispatch(state: &ApplicationMcp, request: Value) -> Value {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    serde_json::to_value(
        authority
            .active_mut(generation)
            .unwrap()
            .dispatch(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn content(state: &ApplicationMcp) -> Value {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    let host = authority
        .active_mut(generation)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap();
    json!({"generation":generation,"revision":host.history.content_revision(),
        "depths":host.history.history_depths(),"document":host.history
            .acquire_snapshot(host.history.content_revision()).unwrap().document()})
}

#[test]
fn history_on_actual_raw_owner_restores_full_document_and_moves_one_entry() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let fill = request(
        &receipt,
        "fill-before-undo",
        0,
        "command.document.object.fill.set",
        cases()["command"]["request"]["payload"].clone(),
    );
    assert_eq!(dispatch(&state, fill)["ok"], true);
    let after_fill = content(&state);
    assert_eq!(after_fill["revision"], 1);
    assert_eq!(after_fill["depths"], json!([1, 0]));
    assert_eq!(
        after_fill["document"]["objects"][0],
        cases()["command"]["afterRecord"]
    );
    let undo = history(&receipt, "actual-undo", 1, "undo");
    let response = dispatch(&state, undo.clone());
    assert_eq!(
        response["ok"], true,
        "actual staged undo must move the real object's history"
    );
    assert_eq!(response["contentRevision"], 2);
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":0})
    );
    let restored = content(&state);
    assert_eq!(restored["document"], document());
    assert_eq!(restored["revision"], 2);
    assert_eq!(restored["depths"], json!([0, 1]));
    assert_eq!(dispatch(&state, undo), response);
    assert_eq!(content(&state), restored);
}

fn fill(state: &ApplicationMcp, receipt: &Value) {
    let r = request(
        receipt,
        "fill",
        0,
        "command.document.object.fill.set",
        cases()["command"]["request"]["payload"].clone(),
    );
    assert_eq!(dispatch(state, r)["ok"], true);
}

#[test]
fn actual_redo_readback_and_late_retries_preserve_complete_content() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    fill(&state, &receipt);
    let undo = history(&receipt, "undo", 1, "undo");
    let undo_reply = dispatch(&state, undo.clone());
    let redo = history(&receipt, "redo", 2, "redo");
    let redo_reply = dispatch(&state, redo.clone());
    assert_eq!(redo_reply["contentRevision"], 3);
    assert_eq!(
        redo_reply["result"],
        json!({"applied":true,"historyEntriesAdded":0})
    );
    let mut expected = document();
    expected["objects"][0] = cases()["command"]["afterRecord"].clone();
    let after = content(&state);
    assert_eq!(after["document"], expected);
    assert_eq!(after["depths"], json!([1, 0]));
    let read = json!({"apiVersion":2,"requestId":"read-redo","instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"operation":"query.document.object",
        "payload":{"atRevision":3,"stableTarget":cases()["records"][0]["target"]}});
    assert_eq!(
        dispatch(&state, read)["result"]["object"],
        cases()["command"]["afterRecord"]
    );
    assert_eq!(dispatch(&state, undo.clone()), undo_reply);
    assert_eq!(dispatch(&state, redo.clone()), redo_reply);
    assert_eq!(content(&state), after);
    let changed = history(&receipt, "undo", 3, "redo");
    assert_eq!(
        dispatch(&state, changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(content(&state), after);
    assert_eq!(
        dispatch(&state, history(&receipt, "later-undo", 3, "undo"))["contentRevision"],
        4
    );
    let before = content(&state);
    assert_eq!(dispatch(&state, redo), redo_reply);
    assert_eq!(content(&state), before);
    assert_eq!(before["document"], document());
}

#[test]
fn actual_history_refusals_preserve_owner_and_are_exactly_replayed() {
    for action in ["undo", "redo"] {
        for (field, value, code) in [
            ("expectedRevision", json!(0), "stale_revision"),
            (
                "payload",
                json!({"command":"object.undo","extra":true}),
                "invalid_request",
            ),
            ("documentId", json!("other-document"), "wrong_document"),
            (
                "cancelledBeforeDispatch",
                json!(true),
                "cancelled_before_dispatch",
            ),
        ] {
            let state = ApplicationMcp::default();
            let receipt = install(&state);
            fill(&state, &receipt);
            let mut r = history(&receipt, "refused", 1, action);
            r[field] = value;
            let before = content(&state);
            let response = dispatch(&state, r.clone());
            assert_eq!(response["error"]["code"], code);
            assert_eq!(dispatch(&state, r), response);
            assert_eq!(content(&state), before);
        }
        let state = ApplicationMcp::default();
        let receipt = install(&state);
        fill(&state, &receipt);
        let mut missing = history(&receipt, "missing", 1, action);
        missing.as_object_mut().unwrap().remove("expectedRevision");
        let before = content(&state);
        let reply = dispatch(&state, missing.clone());
        assert_eq!(reply["error"]["code"], "invalid_request");
        assert_eq!(dispatch(&state, missing), reply);
        assert_eq!(content(&state), before);
    }
}

#[test]
fn public_history_routes_remain_unregistered_and_opacity_aliases_cannot_mutate() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    fill(&state, &receipt);
    let before = content(&state);
    for action in ["undo", "redo"] {
        let r = history(&receipt, "public", 1, action);
        assert!(state
            .dispatch_native(serde_json::from_value(r.clone()).unwrap())
            .unwrap_err()
            .contains("not declared"));
        let mut alias = r;
        alias["operation"] = json!(format!("command.document.{action}"));
        assert_eq!(dispatch(&state, alias)["error"]["code"], "invalid_request");
        assert_eq!(content(&state), before);
    }
}

#[test]
fn actual_history_receipt_cannot_cross_terminal_release_and_new_incarnation() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    fill(&state, &receipt);
    let old = history(&receipt, "undo", 1, "undo");
    assert_eq!(dispatch(&state, old.clone())["contentRevision"], 2);
    let release = NativeReleaseRequest {
        api_version: 2,
        request_id: "release-history".into(),
        instance_id: state.instance_id().into(),
        document_id: receipt["documentId"].as_str().unwrap().into(),
        expected_revision: 2,
        cancelled_before_dispatch: false,
    };
    let generation = match admit_release_request(&state.native_state(), &release).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("unexpected retry"),
    };
    complete_release(&state.native_state(), generation, &release, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    assert!(state
        .native_state()
        .lock()
        .unwrap()
        .active_generation()
        .is_err());
    let fresh = install(&state);
    assert_ne!(fresh["documentId"], receipt["documentId"]);
    let before = content(&state);
    assert_eq!(dispatch(&state, old)["error"]["code"], "wrong_document");
    assert_eq!(content(&state), before);
    fill(&state, &fresh);
    assert_eq!(
        dispatch(&state, history(&fresh, "undo", 1, "undo"))["contentRevision"],
        2
    );
    assert_eq!(content(&state)["document"], document());
}
