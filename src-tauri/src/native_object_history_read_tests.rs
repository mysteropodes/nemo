//! Public common dispatcher over the actual history owner; source-only acceptance.
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use serde_json::{json, Value};
fn install(state: &ApplicationMcp) -> Value {
    let document = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":20,
        "layers":[{"layerUid":"L"}],"objects":[]});
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"install-history-read",
        "instanceId":state.instance_id(),"documentJson":document.to_string()})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn request(receipt: &Value, revision: u64, snapshot: &str) -> Value {
    json!({"apiVersion":2,"requestId":"history-read","instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"operation":"query.document.object.history",
        "payload":{"atRevision":revision,"documentSnapshotId":snapshot}})
}
fn dispatch(state: &ApplicationMcp, request: Value) -> Value {
    serde_json::to_value(
        state
            .dispatch_native(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn owner(state: &ApplicationMcp) -> (String, String) {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    let host = authority
        .active_mut(generation)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap();
    (
        format!("{:?}", host.history),
        host.history
            .acquire_snapshot(host.content_revision())
            .unwrap()
            .snapshot_id()
            .into(),
    )
}
fn command(receipt: &Value, revision: u64, operation: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":format!("{operation}-{revision}"),"instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"expectedRevision":revision,"operation":operation,"payload":payload})
}
#[test]
fn common_public_history_observes_real_edit_undo_redo_and_repeated_reads_preserve_owner() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let initial = owner(&state);
    let old = request(&receipt, 0, &initial.1);
    assert_eq!(dispatch(&state, old.clone())["result"]["undoDepth"], 0);
    for (revision, value) in [(0, 42), (1, 73)] {
        let edit = command(
            &receipt,
            revision,
            "command.document.object.property.edit",
            json!({
            "command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L"},
            "property":"opacity","action":{"kind":"setValue","frame":0,"values":[value]}}),
        );
        assert_eq!(dispatch(&state, edit)["ok"], true);
    }
    let undo = command(
        &receipt,
        2,
        "command.document.object.undo",
        json!({"command":"object.undo"}),
    );
    assert_eq!(dispatch(&state, undo)["ok"], true);
    let before = owner(&state);
    let r = request(&receipt, 3, &before.1);
    for _ in 0..4 {
        let response = dispatch(&state, r.clone());
        assert_eq!(
            response["result"],
            json!({"atRevision":3,"documentSnapshotId":before.1,"undoDepth":1,"redoDepth":1})
        );
    }
    assert_eq!(
        owner(&state),
        before,
        "common public reads cannot retain receipts or change state"
    );
    assert_eq!(dispatch(&state, old)["error"]["code"], "stale_revision");
    let mut bad = r.clone();
    bad["payload"]["documentSnapshotId"] = json!(initial.1);
    assert_eq!(dispatch(&state, bad)["error"]["code"], "stale_revision");
    let mut cancelled = r.clone();
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        dispatch(&state, cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(owner(&state), before);
    let redo = command(
        &receipt,
        3,
        "command.document.object.redo",
        json!({"command":"object.redo"}),
    );
    assert_eq!(dispatch(&state, redo)["ok"], true);
    let after = owner(&state);
    let response = dispatch(&state, request(&receipt, 4, &after.1));
    assert_eq!(response["result"]["undoDepth"], 2);
    assert_eq!(response["result"]["redoDepth"], 0);
}
#[test]
fn wrong_owner_and_released_host_cannot_observe_current_history() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let before = owner(&state);
    let r = request(&receipt, 0, &before.1);
    for (field, code) in [
        ("instanceId", "wrong_instance"),
        ("documentId", "wrong_document"),
    ] {
        let mut bad = r.clone();
        bad[field] = json!("foreign");
        let native = state.native_state();
        let mut authority = native.lock().unwrap();
        let generation = authority.active_generation().unwrap();
        let response = authority
            .active_mut(generation)
            .unwrap()
            .dispatch(serde_json::from_value(bad).unwrap())
            .unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap()["error"]["code"],
            code
        );
    }
    assert_eq!(owner(&state), before);
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    let host = authority
        .active_mut(generation)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap();
    host.release_project().unwrap();
    assert!(host.dispatch(serde_json::from_value(r).unwrap()).is_err());
}

#[tokio::test]
async fn bundled_public_wire_reads_actual_nonzero_history_without_notifications() {
    use nemo_mcp::{
        registry::Endpoint,
        wire::{self, AuthenticatedWireRequest},
    };
    use tokio::net::TcpListener;
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let edit = command(
        &receipt,
        0,
        "command.document.object.property.edit",
        json!({
        "command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L"},
        "property":"opacity","action":{"kind":"setValue","frame":0,"values":[42]}}),
    );
    assert_eq!(dispatch(&state, edit)["ok"], true);
    let undo = command(
        &receipt,
        1,
        "command.document.object.undo",
        json!({"command":"object.undo"}),
    );
    assert_eq!(dispatch(&state, undo)["ok"], true);
    let before = owner(&state);
    let req = request(&receipt, 2, &before.1);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "history-read-wire".into(),
        build_id: "history-read-wire".into(),
    };
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let message: AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
        let query = match message {
            AuthenticatedWireRequest::Native(m) => {
                assert_eq!(m.secret, endpoint.secret);
                m.native_request
            }
            _ => panic!("non-native read"),
        };
        state
            .serve_external_for_test(&mut stream, query, |_| {
                panic!("history read emitted a revision notification")
            })
            .await
            .unwrap();
    };
    let client = wire::call_native(
        &endpoint,
        serde_json::from_value(req).unwrap(),
        wire::CancellationToken::new(),
    );
    let (_, response) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    let response = serde_json::to_value(response.unwrap()).unwrap();
    assert_eq!(response["result"]["undoDepth"], 0);
    assert_eq!(response["result"]["redoDepth"], 1);
    assert_eq!(owner(&state), before);
}

#[test]
fn released_and_replaced_common_owner_refuses_old_history_pins() {
    use crate::{
        native_application::{admit_release_request, complete_release},
        native_application_contract::NativeReleaseRequest,
        native_dispatch::ReleaseAdmission,
    };
    let state = ApplicationMcp::default();
    let old = install(&state);
    let before = owner(&state);
    let old_query = request(&old, 0, &before.1);
    let release = NativeReleaseRequest {
        api_version: 2,
        request_id: "release-history".into(),
        instance_id: state.instance_id().into(),
        document_id: old["documentId"].as_str().unwrap().into(),
        expected_revision: 0,
        cancelled_before_dispatch: false,
    };
    let generation = match admit_release_request(&state.native_state(), &release).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("unexpected replay"),
    };
    complete_release(&state.native_state(), generation, &release, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    assert_eq!(
        dispatch(&state, old_query.clone())["error"]["code"],
        "unavailable"
    );
    let fresh = install(&state);
    assert_ne!(old["documentId"], fresh["documentId"]);
    assert_eq!(
        dispatch(&state, old_query)["error"]["code"],
        "wrong_document"
    );
    let current = owner(&state);
    let response = dispatch(&state, request(&fresh, 0, &current.1));
    assert_eq!(response["result"]["undoDepth"], 0);
    assert_eq!(response["result"]["redoDepth"], 0);
}
