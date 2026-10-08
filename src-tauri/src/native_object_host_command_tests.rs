//! Independent command oracle through the actual raw-installed authority.
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
    let raw = json!({"apiVersion":2,"requestId":"bootstrap-command",
        "instanceId":state.instance_id(),"documentJson":document().to_string()});
    serde_json::to_value(bootstrap_raw(state, &raw.to_string()).unwrap()).unwrap()
}
fn command(receipt: &Value, id: &str, revision: u64) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"expectedRevision":revision,
        "operation":"command.document.object.fill.set","payload":cases()["command"]["request"]["payload"]})
}
fn dispatch(state: &ApplicationMcp, request: Value) -> Value {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    let response = authority
        .active_mut(generation)
        .unwrap()
        .dispatch(serde_json::from_value(request).unwrap())
        .unwrap();
    serde_json::to_value(response).unwrap()
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
        "depths":host.history.history_depths(),"document":host.history.acquire_snapshot(host.history.content_revision()).unwrap().document()})
}

#[test]
fn command_on_actual_raw_owner_commits_one_revision_and_history_entry() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let request = command(&receipt, "actual-fill", 0);
    let response = dispatch(&state, request.clone());
    assert_eq!(
        response["ok"], true,
        "actual staged fill must commit through its real owner"
    );
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    for field in ["requestId", "instanceId", "documentId"] {
        assert_eq!(response[field], request[field]);
    }
    let after = content(&state);
    assert_eq!(after["revision"], 1);
    assert_eq!(after["depths"], json!([1, 0]));
    assert_eq!(
        after["document"]["objects"][0],
        cases()["command"]["afterRecord"]
    );
    assert_eq!(after["document"]["objects"][1], document()["objects"][1]);
    let read = json!({"apiVersion":2,"requestId":"read-after-command",
        "instanceId":receipt["instanceId"],"documentId":receipt["documentId"],
        "operation":"query.document.object","payload":{"atRevision":1,"stableTarget":cases()["records"][0]["target"]}});
    assert_eq!(
        dispatch(&state, read)["result"]["object"],
        cases()["command"]["afterRecord"]
    );
    assert_eq!(dispatch(&state, request.clone()), response);
    assert_eq!(content(&state), after);
}

#[test]
fn private_commands_do_not_activate_public_opacity_or_object_routes() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let request = command(&receipt, "public-denied", 0);
    let before = content(&state);
    let error = state
        .dispatch_native(serde_json::from_value(request.clone()).unwrap())
        .unwrap_err();
    assert!(error.contains("not declared"));
    assert_eq!(content(&state), before);
    let mut opacity = request.clone();
    opacity["operation"] = json!("command.document.apply");
    assert_eq!(
        dispatch(&state, opacity)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(content(&state), before);
    let mut changed = request.clone();
    let first = dispatch(&state, request);
    let after = content(&state);
    changed["payload"]["fill"]["b"] = json!(0.9);
    assert_eq!(
        dispatch(&state, changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(content(&state), after);
    assert_eq!(first["contentRevision"], 1);
}

#[test]
fn malformed_stale_cancelled_or_wrong_incarnation_preserve_actual_host() {
    for (path, value, code) in [
        ("/expectedRevision", json!(1), "stale_revision"),
        ("/payload/fill/a", json!(-1), "invalid_request"),
        (
            "/payload/stableTarget/strokeId",
            json!("absent"),
            "not_found",
        ),
        ("/instanceId", json!("different-instance"), "wrong_instance"),
        ("/documentId", json!("different-document"), "wrong_document"),
    ] {
        let state = ApplicationMcp::default();
        let receipt = install(&state);
        let mut request = command(&receipt, "refused", 0);
        *request.pointer_mut(path).unwrap() = value;
        let before = content(&state);
        let first = dispatch(&state, request.clone());
        assert_eq!(first["ok"], false);
        assert_eq!(first["error"]["code"], code);
        assert_eq!(dispatch(&state, request), first);
        assert_eq!(content(&state), before);
    }
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let mut request = command(&receipt, "cancel", 0);
    request["cancelledBeforeDispatch"] = json!(true);
    let before = content(&state);
    assert_eq!(
        dispatch(&state, request)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(content(&state), before);
}

#[test]
fn command_receipt_cannot_cross_terminal_release_and_new_owner() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let old = command(&receipt, "old-command", 0);
    dispatch(&state, old.clone());
    let request = NativeReleaseRequest {
        api_version: 2,
        request_id: "release-command".into(),
        instance_id: state.instance_id().into(),
        document_id: receipt["documentId"].as_str().unwrap().into(),
        expected_revision: 1,
        cancelled_before_dispatch: false,
    };
    let generation = match admit_release_request(&state.native_state(), &request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("unexpected retry"),
    };
    complete_release(&state.native_state(), generation, &request, || {
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
    let new = command(&fresh, "old-command", 0);
    assert_eq!(dispatch(&state, new)["contentRevision"], 1);
}
