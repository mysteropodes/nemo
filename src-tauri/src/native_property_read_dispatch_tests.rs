//! D11 real owner reads and authenticated pending-ACK TCP; no installed claim.
use crate::application_mcp::{revision_sync::Delivery, ApplicationMcp};
use crate::native_object_bootstrap::bootstrap_raw;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const OP: &str = "query.document.object.property.read";
fn install(state: &ApplicationMcp) -> String {
    let source = json!({"format":"nemo.native-project-document","formatVersion":1,
    "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
    "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
    "layers":[{"layerUid":"L0","locked":true},{"layerUid":"L1"},{"layerUid":"L2"}],"objects":[],"propertyHolders":[
        {"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{"property":"opacity","static":[17],
            "expression":{"enabled":true,"code":"999"},"track":{"keys":[
            {"frame":0,"v":[0],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}],"hOut":[3,4]},
            {"frame":4,"v":[80],"hold":false,"curvePoints":null,"hIn":[0,0]},
            {"frame":12,"v":[0],"curvePoints":[]}]}}]},
        {"target":{"contextId":"scene-root","layerUid":"L1"},"properties":[{"property":"opacity","static":[17]}]},
        {"target":{"contextId":"scene-root","layerUid":"L2"},"properties":[{"property":"opacity","track":{"keys":[]}}]}
    ]}});
    let receipt=bootstrap_raw(state,&json!({"apiVersion":2,"requestId":"read-install","instanceId":state.instance_id(),"documentJson":source.to_string()}).to_string()).unwrap();
    serde_json::to_value(receipt).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .into()
}
fn query(state: &ApplicationMcp, id: &str, keys: bool, layer: &str) -> NativeApplicationRequest {
    let info = state.native_project_info(id).unwrap();
    let mut view = json!({"kind":"rawSample","target":{"contextId":"scene-root","layerUid":layer},"property":"opacity","frame":2});
    if keys {
        view.as_object_mut().unwrap().remove("frame");
        view["kind"] = json!("authoredKeys");
        view["offset"] = json!(0);
        view["limit"] = json!(2);
    }
    serde_json::from_value(json!({"apiVersion":2,"requestId":"property-read","instanceId":state.instance_id(),"documentId":id,"operation":OP,
        "payload":{"atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":view}})).unwrap()
}
fn invoke(state: &ApplicationMcp, q: NativeApplicationRequest, external: bool) -> Delivery {
    q.validate().unwrap();
    state
        .revisions
        .dispatch_admitted(state.instance_id(), &state.native, q, external, false)
        .unwrap()
}
fn saved(state: &ApplicationMcp, id: &str) -> Vec<u8> {
    let info = state.native_project_info(id).unwrap();
    state
        .native_project_file_snapshot(
            id,
            info["contentRevision"].as_u64().unwrap(),
            info["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
fn success(state: &ApplicationMcp, q: NativeApplicationRequest) -> Value {
    let expected = q.clone();
    let delivery = invoke(state, q, true);
    assert!(delivery.notification.is_none());
    let r = delivery.response;
    assert!(r.ok, "{r:?}");
    assert!(serde_json::to_vec(&r).unwrap().len() <= 4096);
    assert_eq!(r.request_id, expected.request_id);
    assert_eq!(r.instance_id, expected.instance_id);
    assert_eq!(r.document_id, expected.document_id);
    assert_eq!(
        r.content_revision,
        expected.payload["atRevision"].as_u64().unwrap()
    );
    let v = r.result.unwrap();
    assert!(serde_json::to_vec(&v).unwrap().len() <= 3072);
    assert_eq!(v["instanceId"], expected.instance_id);
    assert_eq!(v["documentId"], expected.document_id);
    for name in ["atRevision", "documentSnapshotId"] {
        assert_eq!(v[name], expected.payload[name]);
    }
    for name in ["kind", "target", "property"] {
        assert_eq!(v[name], expected.payload["view"][name]);
    }
    assert_eq!(v["dimension"], 1);
    v
}
#[test]
fn property_read_public_raw_samples_ignore_expressions_and_preserve_owner_and_bytes() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let before = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    for (frame, value) in [
        (-0.5, 0.0),
        (0.5, 10.0),
        (2.0, 40.0),
        (4.0, 80.0),
        (16.0, 0.0),
    ] {
        let mut q = query(&state, &id, false, "L0");
        q.payload["view"]["frame"] = json!(frame);
        let result = success(&state, q);
        assert_eq!(result["frame"].as_f64(), Some(frame));
        assert!((result["value"][0].as_f64().unwrap() - value).abs() < 1e-10);
    }
    assert_eq!(
        success(&state, query(&state, &id, false, "L1"))["value"],
        json!([17])
    );
    assert_eq!(saved(&state, &id), before);
    assert_eq!(state.native_project_info(&id).unwrap(), info);
}
#[test]
fn property_read_public_authored_pages_preserve_exact_missing_null_empty_and_track_presence() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let before = saved(&state, &id);
    let source: Value = serde_json::from_slice(&before).unwrap();
    let original = &source["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"];
    let first = success(&state, query(&state, &id, true, "L0"));
    assert_eq!(first["trackPresent"], true);
    assert_eq!(first["items"], json!([original[0], original[1]]));
    assert_eq!(first["nextOffset"], 2);
    assert!(first["items"][0].get("hold").is_none());
    assert!(first["items"][0].get("hIn").is_none());
    assert!(first["items"][1]["curvePoints"].is_null());
    let mut q = query(&state, &id, true, "L0");
    q.payload["view"]["offset"] = json!(2);
    let last = success(&state, q.clone());
    assert_eq!(last["items"], json!([original[2]]));
    assert!(last["nextOffset"].is_null());
    assert_eq!(last["items"][0]["curvePoints"], json!([]));
    q.payload["view"]["offset"] = json!(3);
    let empty = success(&state, q.clone());
    assert_eq!(empty["items"], json!([]));
    assert!(empty["nextOffset"].is_null());
    assert_eq!(empty["trackPresent"], true);
    // The closed shape admits any u32 offset; the native owner refuses past-end offsets.
    q.payload["view"]["offset"] = json!(u32::MAX);
    let refused = invoke(&state, q, true);
    assert!(refused.notification.is_none());
    assert!(!refused.response.ok);
    assert_eq!(refused.response.error.unwrap().code, "invalid_request");
    assert_eq!(saved(&state, &id), before);
    for (layer, present) in [("L1", false), ("L2", true)] {
        let r = success(&state, query(&state, &id, true, layer));
        assert_eq!(r["trackPresent"], present);
        assert_eq!(r["items"], json!([]));
        assert!(r["nextOffset"].is_null());
    }
    assert_eq!(saved(&state, &id), before);
}
#[test]
fn property_read_public_refusals_do_not_change_snapshot_or_storage() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let before = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let q = query(&state, &id, false, "L0");
    let mut cases = Vec::new();
    let mut r = q.clone();
    r.cancelled_before_dispatch = true;
    cases.push((r, "cancelled_before_dispatch"));
    let mut r = q.clone();
    r.instance_id = "foreign".into();
    cases.push((r, "wrong_instance"));
    let mut r = q.clone();
    r.document_id = "foreign".into();
    cases.push((r, "wrong_document"));
    let mut r = q.clone();
    r.payload["atRevision"] = json!(1);
    cases.push((r, "stale_revision"));
    let mut r = q.clone();
    r.payload["documentSnapshotId"] = json!("stale");
    cases.push((r, "stale_revision"));
    let mut r = q.clone();
    r.payload["view"]["target"]["layerUid"] = json!("absent");
    cases.push((r, "not_found"));
    for property in ["order", "timeRemap", "absent"] {
        let mut r = q.clone();
        r.payload["view"]["property"] = json!(property);
        cases.push((r, "unavailable"));
    }
    for (r, code) in cases {
        let delivery = invoke(&state, r, true);
        assert!(delivery.notification.is_none());
        assert!(!delivery.response.ok);
        assert_eq!(delivery.response.error.unwrap().code, code);
        assert_eq!(saved(&state, &id), before);
        assert_eq!(state.native_project_info(&id).unwrap(), info);
    }
}
#[tokio::test]
async fn property_read_actual_tcp_can_read_committed_head_while_ui_ack_still_fences_writes() {
    use nemo_mcp::{
        registry::Endpoint,
        wire::{self, AuthenticatedWireRequest},
    };
    use tokio::net::TcpListener;
    let state = ApplicationMcp::default();
    let id = install(&state);
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let mut command:NativeApplicationRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"read-pending-write","instanceId":state.instance_id(),"documentId":id,"expectedRevision":0,"operation":"command.document.object.layer.visibility.set","payload":{"command":"object.layer.visibility.set","layerUid":"L0","visible":false}})).unwrap();
    let committed = invoke(&state, command.clone(), true);
    assert!(committed.response.ok);
    let _held = committed
        .notification
        .expect("real ACK receiver must stay alive");
    let before = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "isolated-property-read".into(),
        build_id: "property-read-test".into(),
    };
    for keys in [false, true] {
        let q = query(&state, &id, keys, "L0");
        let server = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let msg: AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
            let AuthenticatedWireRequest::Native(msg) = msg else {
                panic!("native read envelope required")
            };
            assert_eq!(msg.secret, endpoint.secret);
            assert_eq!(msg.native_request, q);
            state
                .serve_external_for_test(&mut stream, msg.native_request, |_| {
                    panic!("read must not publish")
                })
                .await
                .unwrap();
        };
        let client = wire::call_native(&endpoint, q.clone(), wire::CancellationToken::new());
        let (_, response) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(server, client)
        })
        .await
        .expect("read must not wait for write ACK");
        let response: NativeApplicationResponse = response.unwrap();
        assert!(response.ok);
        assert_eq!(response.content_revision, 1);
        assert_eq!(response.request_id, q.request_id);
        assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    }
    command.request_id = "read-still-fenced".into();
    command.expected_revision = Some(1);
    let blocked = invoke(&state, command, true);
    assert!(!blocked.response.ok);
    assert_eq!(blocked.response.error.unwrap().code, "unavailable");
    assert!(blocked.notification.is_none());
    assert_eq!(saved(&state, &id), before);
    assert_eq!(state.native_project_info(&id).unwrap(), info);
}

#[test]
fn property_read_oversized_authored_page_refuses_without_truncation_or_mutation() {
    let original = ApplicationMcp::default();
    let old = install(&original);
    let mut source: Value = serde_json::from_slice(&saved(&original, &old)).unwrap();
    let points: Vec<Value> = (0..=200)
        .map(|i| json!({"x":i as f64/200.0,"y":i as f64/200.0}))
        .collect();
    source["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["curvePoints"] =
        json!(points);
    let state = ApplicationMcp::default();
    let opened=bootstrap_raw(&state,&json!({"apiVersion":2,"requestId":"large-key","instanceId":state.instance_id(),"documentJson":source.to_string()}).to_string()).unwrap();
    let id = serde_json::to_value(opened).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let q = query(&state, &id, true, "L0");
    assert!(serde_json::to_vec(&q).unwrap().len() < 4096);
    let delivery = invoke(&state, q, true);
    assert!(delivery.notification.is_none());
    assert!(!delivery.response.ok);
    assert!(delivery.response.result.is_none());
    assert_eq!(delivery.response.error.unwrap().code, "unavailable");
    assert_eq!(saved(&state, &id), before);
    assert_eq!(state.native_project_info(&id).unwrap(), info);
}
