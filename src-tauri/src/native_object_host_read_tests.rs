//! Independent actual-owner immutable reads; common availability stays closed.
use crate::{
    application_mcp::ApplicationMcp,
    native_application::{admit_release_request, complete_release},
    native_application_contract::NativeReleaseRequest,
    native_dispatch::{NativeObjectHost, ReleaseAdmission},
    native_object_bootstrap::bootstrap_raw,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]})
}
fn bootstrap(state: &ApplicationMcp, document: &Value) -> Value {
    let raw = json!({"apiVersion":2,"requestId":"bootstrap-read",
        "instanceId":state.instance_id(),"documentJson":document.to_string()});
    serde_json::to_value(bootstrap_raw(state, &raw.to_string()).unwrap()).unwrap()
}
fn request(receipt: &Value, document: &Value, index: usize) -> Value {
    json!({"apiVersion":2,"requestId":"common-read","instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"operation":"query.document.object",
        "payload":{"atRevision":0,"stableTarget":document["objects"][index]["target"]}})
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
fn owner(state: &ApplicationMcp) -> Value {
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    let host = authority
        .active_mut(generation)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap();
    json!({"generation":generation,"documentId":host.history.document_id(),
        "revision":host.history.content_revision(),"depths":host.history.history_depths(),
        "document":host.history.acquire_snapshot(0).unwrap().document()})
}
fn success(response: &Value, req: &Value, expected: &Value) {
    assert_eq!(response["apiVersion"], 2);
    for field in ["requestId", "instanceId", "documentId"] {
        assert_eq!(response[field], req[field]);
    }
    assert_eq!(response["contentRevision"], 0);
    assert_eq!(response["ok"], true);
    assert!(response["error"].is_null());
    assert_eq!(response["result"]["atRevision"], 0);
    assert_eq!(response["result"]["object"], *expected);
    assert_eq!(
        response["result"]["documentSnapshotId"],
        format!("native-object:{}:0", req["documentId"].as_str().unwrap())
    );
    assert!(serde_json::to_vec(response).unwrap().len() <= 4096);
}

#[test]
fn raw_bootstrap_actual_host_queries_return_exact_independent_records_without_mutation() {
    let state = ApplicationMcp::default();
    let document = fixture();
    let receipt = bootstrap(&state, &document);
    assert_eq!(receipt["resourceCount"], 0);
    assert_eq!(receipt["viewportAvailable"], false);
    let before = owner(&state);
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    for index in [0, 1, 0] {
        let req = request(&receipt, &document, index);
        let mut response = dispatch(&state, req.clone());
        success(&response, &req, &document["objects"][index]);
        response["result"]["object"]["fill"]["r"] = json!(1);
        success(
            &dispatch(&state, req.clone()),
            &req,
            &document["objects"][index],
        );
        assert_eq!(owner(&state), before);
    }
    assert!(state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap_err()
        .contains("occupied"));
}

#[test]
fn correlated_query_refusals_preserve_actual_owner_and_revision() {
    let state = ApplicationMcp::default();
    let document = fixture();
    let receipt = bootstrap(&state, &document);
    let valid = request(&receipt, &document, 0);
    let before = owner(&state);
    let mut cases = Vec::new();
    for (path, value, code) in [
        ("/apiVersion", json!(1), "invalid_request"),
        ("/instanceId", json!("different-instance"), "wrong_instance"),
        ("/documentId", json!("different-document"), "wrong_document"),
        ("/payload/atRevision", json!(1), "not_found"),
        ("/payload/atRevision", json!(-1), "invalid_request"),
        (
            "/payload/stableTarget/strokeId",
            json!("missing"),
            "not_found",
        ),
        (
            "/payload/stableTarget/frameScope",
            json!(["authored", 7]),
            "invalid_request",
        ),
        ("/payload", json!([]), "invalid_request"),
        ("/operation", json!("history.undo"), "invalid_request"),
        (
            "/operation",
            json!("command.document.apply"),
            "invalid_request",
        ),
    ] {
        let mut req = valid.clone();
        *req.pointer_mut(path).unwrap() = value;
        cases.push((req, code));
    }
    for (field, value, code) in [
        ("expectedRevision", json!(0), "invalid_request"),
        (
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
    ] {
        let mut req = valid.clone();
        req[field] = value;
        cases.push((req, code));
    }
    let mut extra = valid.clone();
    extra["payload"]["extra"] = json!(true);
    cases.push((extra, "invalid_request"));
    let mut missing = valid.clone();
    missing["payload"]
        .as_object_mut()
        .unwrap()
        .remove("atRevision");
    cases.push((missing, "invalid_request"));
    for (req, code) in cases {
        let response = dispatch(&state, req.clone());
        assert_eq!(response["ok"], false, "{req}");
        assert_eq!(response["error"]["code"], code, "{req}");
        assert_eq!(response["requestId"], req["requestId"]);
        assert_eq!(response["instanceId"], receipt["instanceId"]);
        assert_eq!(response["documentId"], receipt["documentId"]);
        assert_eq!(response["contentRevision"], 0);
        assert!(response["result"].is_null());
        assert_eq!(owner(&state), before);
    }
    let mut bad_id = valid.clone();
    bad_id["requestId"] = json!("invalid id");
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    assert_eq!(
        authority
            .active_mut(generation)
            .unwrap()
            .dispatch(serde_json::from_value(bad_id.clone()).unwrap())
            .unwrap_err(),
        "native object read correlation is invalid"
    );
    drop(authority);
    assert_eq!(owner(&state), before);
}

#[test]
fn complete_request_and_response_byte_limits_refuse_without_truncation() {
    let state = ApplicationMcp::default();
    let document = fixture();
    let receipt = bootstrap(&state, &document);
    let mut req = request(&receipt, &document, 0);
    req["payload"]["stableTarget"]["strokeId"] = json!("🟠".repeat(1100));
    assert!(serde_json::to_vec(&req).unwrap().len() > 4096);
    let before = owner(&state);
    let response = dispatch(&state, req);
    assert_eq!(response["error"]["code"], "invalid_request");
    assert_eq!(owner(&state), before);
    let mut large = fixture();
    let segment = large["objects"][0]["geometry"]["segments"][0].clone();
    large["objects"][0]["geometry"]["segments"] = json!(vec![segment; 100]);
    let large_state = ApplicationMcp::default();
    let large_receipt = bootstrap(&large_state, &large);
    let before = owner(&large_state);
    let response = dispatch(&large_state, request(&large_receipt, &large, 0));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "unavailable");
    assert!(response["result"].is_null());
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    assert_eq!(owner(&large_state), before);
}

#[test]
fn released_authority_refuses_and_reentry_reads_only_new_incarnation() {
    let state = ApplicationMcp::default();
    let document = fixture();
    let old = bootstrap(&state, &document);
    let req = NativeReleaseRequest {
        api_version: 2,
        request_id: "release-read".into(),
        instance_id: state.instance_id().into(),
        document_id: old["documentId"].as_str().unwrap().into(),
        expected_revision: 0,
        cancelled_before_dispatch: false,
    };
    let generation = match admit_release_request(&state.native_state(), &req).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("unexpected retry"),
    };
    complete_release(&state.native_state(), generation, &req, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    assert!(state
        .native_state()
        .lock()
        .unwrap()
        .active_generation()
        .is_err());
    let fresh = bootstrap(&state, &document);
    assert_ne!(fresh["documentId"], old["documentId"]);
    let stale = dispatch(&state, request(&old, &document, 0));
    assert_eq!(stale["error"]["code"], "wrong_document");
    let fresh_req = request(&fresh, &document, 0);
    success(
        &dispatch(&state, fresh_req.clone()),
        &fresh_req,
        &document["objects"][0],
    );
}

#[tokio::test]
async fn common_and_external_object_admission_remain_unavailable() {
    use tokio::{
        io::AsyncReadExt,
        net::{TcpListener, TcpStream},
    };
    let state = ApplicationMcp::default();
    let document = fixture();
    let receipt = bootstrap(&state, &document);
    let req = request(&receipt, &document, 1);
    let before = owner(&state);
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object.json"
    ))
    .unwrap();
    assert_eq!(descriptor["availability"]["state"], "unavailable");
    let reason = descriptor["availability"]["reason"].as_str().unwrap();
    assert_eq!(
        state
            .dispatch_native(serde_json::from_value(req.clone()).unwrap())
            .unwrap_err(),
        reason
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (mut server, _) = listener.accept().await.unwrap();
    let host = &state;
    let query = serde_json::from_value(req.clone()).unwrap();
    let work = async move {
        let refusal = host
            .serve_external_for_test(&mut server, query, |_| panic!("unavailable read notified"))
            .await
            .unwrap_err();
        drop(server);
        refusal
    };
    let mut bytes = Vec::new();
    let receive = async {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            client.read_to_end(&mut bytes),
        )
        .await
        .unwrap()
        .unwrap();
    };
    let (refusal, _) = tokio::join!(work, receive);
    assert_eq!(refusal, reason);
    assert!(bytes.is_empty(), "unavailable route fabricated bytes");
    assert_eq!(owner(&state), before);
}
