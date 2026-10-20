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

#[test]
fn expression_bake_actual_public_host_commits_once_undoes_and_reopens_without_js_sampling() {
    for code in ["value + frame", "(()=>{throw new Error('bake')})()"] {
        let state = ApplicationMcp::default();
        let project = json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
            "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
            "layers":[{"layerUid":"L0"}],"objects":[],"propertyHolders":[{
                "target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{
                    "property":"opacity","static":[17],"expression":{"enabled":true,"code":code}}]}]}});
        let receipt = serde_json::to_value(bootstrap_raw(&state,&json!({"apiVersion":2,
            "requestId":"bake-install","instanceId":state.instance_id(),"documentJson":project.to_string()}).to_string()).unwrap()).unwrap();
        let id = receipt["documentId"].as_str().unwrap();
        let snapshot = |state: &ApplicationMcp, id: &str| {
            let info = state.native_project_info(id).unwrap();
            state
                .native_project_file_snapshot(
                    id,
                    info["contentRevision"].as_u64().unwrap(),
                    info["documentSnapshotId"].as_str().unwrap(),
                )
                .unwrap()
                .unwrap()
        };
        let initial = snapshot(&state, id);
        let q = json!({"apiVersion":2,"requestId":"bake-public","instanceId":state.instance_id(),
            "documentId":id,"expectedRevision":0,"operation":"command.document.object.property.edit",
            "payload":{"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L0"},
                "property":"opacity","action":{"kind":"bake"}}});
        let first = state
            .dispatch_native(serde_json::from_value(q.clone()).unwrap())
            .unwrap();
        let response = serde_json::to_value(&first).unwrap();
        if code.starts_with("value") {
            assert_eq!(response["ok"], true, "{response}");
            assert_eq!(
                response["result"],
                json!({"applied":true,"historyEntriesAdded":1})
            );
            assert_eq!(response["contentRevision"], 1);
            let changed = snapshot(&state, id);
            let saved: Value = serde_json::from_slice(&changed).unwrap();
            let property = &saved["drawing"]["propertyHolders"][0]["properties"][0];
            assert_eq!(property["expression"], json!({"enabled":false,"code":code}));
            assert_eq!(property["track"]["keys"].as_array().unwrap().len(), 2);
            assert_eq!(property["track"]["keys"][1]["v"], json!([32.]));
            assert_eq!(
                state
                    .dispatch_native(serde_json::from_value(q).unwrap())
                    .unwrap(),
                first
            );
            assert_eq!(snapshot(&state, id), changed);
            assert_eq!(content(&state)["depths"], json!([1, 0]));
            let undo = json!({"apiVersion":2,"requestId":"undo-bake","instanceId":state.instance_id(),
                "documentId":id,"expectedRevision":1,"operation":"command.document.object.undo",
                "payload":{"command":"object.undo"}});
            assert_eq!(dispatch(&state, undo)["ok"], true);
            assert_eq!(snapshot(&state, id), initial);
            let fresh = ApplicationMcp::default();
            let reopened = serde_json::to_value(
                bootstrap_raw(
                    &fresh,
                    &json!({"apiVersion":2,
                "requestId":"reopen-bake","instanceId":fresh.instance_id(),
                "documentJson":String::from_utf8(changed.clone()).unwrap()})
                    .to_string(),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(
                snapshot(&fresh, reopened["documentId"].as_str().unwrap()),
                changed
            );
        } else {
            assert_eq!(response["ok"], false);
            assert_eq!(snapshot(&state, id), initial);
            assert_eq!(content(&state)["depths"], json!([0, 0]));
        }
    }
}
