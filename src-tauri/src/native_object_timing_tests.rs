//! Actual shared timing authority and MCP wire core; no installed webview claim.
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use nemo_mcp::{contract::NativeApplicationResponse, registry::Endpoint, wire};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpListener;

fn oracle() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-layer-timing-v1/cases.json"
    ))
    .unwrap()
}
fn timed_document(uid: &str) -> Value {
    let values = oracle();
    let original: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let objects: Vec<_> = values["drawing"]["objects"].as_array().unwrap().iter().map(|entry| {
            let mut record = original["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","layerUid":uid,"strokeId":entry["strokeId"],
            "frameScope":{"kind":"authored","frame":entry["frame"]}});
        record
    }).collect();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
        "layers":[{"layerUid":uid,"timing":values["drawing"]["timing"]}],"objects":objects})
}
fn install_timed(state: &ApplicationMcp, document: &Value) -> String {
    let project = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24.0},"drawing":document,
        "viewportReference":null,"assets":[]});
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"timing-install",
        "instanceId":state.instance_id(),"documentJson":project.to_string()})
        .to_string(),
    )
    .unwrap();
    serde_json::to_value(receipt).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .into()
}
fn timing_query(state: &ApplicationMcp, id: &str, uid: &str, frame: u32) -> Value {
    let info = state.native_project_info(id).unwrap();
    json!({"apiVersion":2,"requestId":"timing-query","instanceId":state.instance_id(),"documentId":id,
        "operation":"query.document.object.layer.timing","payload":{"atRevision":info["contentRevision"],
            "documentSnapshotId":info["documentSnapshotId"],"contextId":"scene-root","layerUid":uid,"frame":frame}})
}
pub(super) fn public(state: &ApplicationMcp, request: Value) -> Value {
    serde_json::to_value(
        state
            .dispatch_native(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap()
}
async fn round_trip(state: &ApplicationMcp, request: Value) -> NativeApplicationResponse {
    round_trip_with_consumer(state, request, false).await
}
async fn round_trip_with_consumer(
    state: &ApplicationMcp,
    request: Value,
    acknowledge: bool,
) -> NativeApplicationResponse {
    use crate::application_mcp::revision_sync::RevisionControl;
    let consumer = acknowledge.then(|| {
        let binding = state
            .revisions
            .control(&state.native, RevisionControl::Binding)
            .unwrap();
        state
            .revisions
            .control(
                &state.native,
                serde_json::from_value(json!({
                    "action":"subscribe", "binding":binding
                }))
                .unwrap(),
            )
            .unwrap();
        binding
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "isolated-timing-test".into(),
        build_id: "timing-core-test".into(),
    };
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let envelope: wire::AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
        let request = match envelope {
            wire::AuthenticatedWireRequest::Native(message) => {
                assert_eq!(message.secret, endpoint.secret);
                message.native_request
            }
            _ => panic!("timing query did not use the existing native wire envelope"),
        };
        state
            .serve_external_for_test(&mut stream, request, |event| {
                let Some(binding) = &consumer else {
                    panic!("read or refused command emitted a revision event")
                };
                let observed = serde_json::to_value(&event).unwrap();
                let info = state
                    .native_project_info(observed["documentId"].as_str().unwrap())
                    .unwrap();
                assert_eq!(info["contentRevision"], observed["toRevision"]);
                state
                    .revisions
                    .control(
                        &state.native,
                        RevisionControl::Acknowledge {
                            subscription_id: binding["subscriptionId"].as_str().unwrap().to_owned(),
                            event,
                        },
                    )
                    .map(|_| ())
            })
            .await
            .unwrap();
    };
    let client = wire::call_native(
        &endpoint,
        serde_json::from_value(request).unwrap(),
        wire::CancellationToken::new(),
    );
    let (_, response) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    let response = response.unwrap();
    if let Some(binding) = consumer {
        state
            .revisions
            .control(
                &state.native,
                RevisionControl::Disconnect {
                    subscription_id: binding["subscriptionId"].as_str().unwrap().to_owned(),
                },
            )
            .unwrap();
    }
    response
}

#[tokio::test]
async fn timing_public_wire_matches_the_fixed_oracle_without_history_or_publication() {
    let state = ApplicationMcp::default();
    let uid = "data:opaque 雪";
    let id = install_timed(&state, &timed_document(uid));
    let before = state.native_project_info(&id).unwrap();
    for expected in oracle()["drawing"]["expected"].as_array().unwrap() {
        let request = timing_query(&state, &id, uid, expected["frame"].as_u64().unwrap() as u32);
        let typed = public(&state, request.clone());
        let wire = serde_json::to_value(round_trip(&state, request.clone()).await).unwrap();
        assert_eq!(wire, typed);
        assert_eq!(wire["ok"], true);
        let result = &wire["result"];
        assert_eq!(result.as_object().unwrap().len(), 11);
        for key in [
            "frame",
            "cellKind",
            "sourceFrame",
            "hasContent",
            "manualEdit",
        ] {
            assert_eq!(result[key], expected[key]);
        }
        assert_eq!(result["instanceId"], state.instance_id());
        assert_eq!(result["documentId"], id);
        assert_eq!(result["documentSnapshotId"], before["documentSnapshotId"]);
        assert_eq!(result["layerUid"], uid);
    }
    assert_eq!(state.native_project_info(&id).unwrap(), before);
    assert_eq!(before["undoDepth"], 0);
}

#[tokio::test]
async fn timing_new_public_create_and_reopened_project_reach_the_same_public_reader() {
    let state = ApplicationMcp::default();
    let receipt = serde_json::to_value(
        bootstrap_raw(
            &state,
            &json!({"apiVersion":2,"requestId":"timing-new",
        "instanceId":state.instance_id(),"newProject":{"canvasW":64,"canvasH":48,"fps":24.0}})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let id = receipt["documentId"].as_str().unwrap();
    let read = round_trip(&state, timing_query(&state, id, "layer-1", 119)).await;
    assert!(read.ok);
    assert_eq!(read.result.unwrap()["sourceFrame"], 0);
    let create = json!({"apiVersion":2,"requestId":"public-create-fixture","instanceId":state.instance_id(),
        "documentId":id,"expectedRevision":0,"operation":"command.document.object.layer.create",
        "payload":{"command":"object.layer.create","name":"Timing fixture"}});
    let refused = round_trip(&state, create.clone()).await;
    assert!(!refused.ok);
    let error = refused.error.unwrap();
    assert_eq!(error.code, "unavailable");
    assert_eq!(error.details.unwrap()["committed"], false);
    assert_eq!(state.native_project_info(id).unwrap()["contentRevision"], 0);
    let created = round_trip_with_consumer(&state, create, true).await;
    assert!(created.ok, "{created:?}");
    let uid = created.result.unwrap()["layerUid"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = round_trip(&state, timing_query(&state, id, &uid, 0)).await;
    assert!(response.ok);
    assert_eq!(response.result.unwrap()["cellKind"], "key");
    let info = state.native_project_info(id).unwrap();
    let bytes = state
        .native_project_file_snapshot(id, 1, info["documentSnapshotId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let reopened = ApplicationMcp::default();
    let (fresh, _, _) = reopened.install_native_project_file(&bytes).unwrap();
    assert_ne!(fresh, id);
    let response = round_trip(&reopened, timing_query(&reopened, &fresh, &uid, 119)).await;
    assert!(response.ok);
    assert_eq!(response.result.unwrap()["sourceFrame"], 0);
    let imported = ApplicationMcp::default();
    let imported_id = install_timed(&imported, &timed_document("L"));
    let pins = imported.native_project_info(&imported_id).unwrap();
    let bytes = imported
        .native_project_file_snapshot(
            &imported_id,
            0,
            pins["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap();
    let loaded = ApplicationMcp::default();
    let (loaded_id, _, _) = loaded.install_native_project_file(&bytes).unwrap();
    let response = round_trip(&loaded, timing_query(&loaded, &loaded_id, "L", 6)).await;
    assert!(response.ok);
    assert_eq!(response.result.unwrap()["manualEdit"], true);
}

#[tokio::test]
async fn timing_committed_head_read_during_pending_ack_neither_settles_nor_advances() {
    use crate::application_mcp::object_client::admit_raw;
    let state = ApplicationMcp::default();
    let id = install_timed(&state, &timed_document("L"));
    let old = timing_query(&state, &id, "L", 2);
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let command = json!({"apiVersion":2,"requestId":"external-hide","instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":0,"operation":"command.document.object.layer.visibility.set",
        "payload":{"command":"object.layer.visibility.set","layerUid":"L","visible":false}});
    let delivery = state
        .revisions
        .dispatch_object(
            state.instance_id(),
            &state.native,
            admit_raw(&command.to_string()).unwrap(),
            true,
        )
        .unwrap();
    assert!(delivery.response.ok);
    assert_eq!(delivery.response.content_revision, 1);
    let before = state.native_project_info(&id).unwrap();
    let response = round_trip(&state, timing_query(&state, &id, "L", 2)).await;
    assert!(response.ok);
    assert_eq!(response.content_revision, 1);
    assert_eq!(response.result.unwrap()["sourceFrame"], 1);
    assert_eq!(state.native_project_info(&id).unwrap(), before);
    assert_eq!(public(&state, old)["error"]["code"], "stale_revision");
    let mut next = command;
    next["requestId"] = json!("blocked-after-read");
    next["expectedRevision"] = json!(1);
    next["payload"]["visible"] = json!(true);
    let blocked = public(&state, next);
    assert_eq!(blocked["error"]["code"], "unavailable");
    assert_eq!(blocked["error"]["details"]["retryExecution"], false);
}

#[tokio::test]
async fn timing_wrong_owner_missing_stale_and_retired_reads_remain_explicit_refusals() {
    let state = ApplicationMcp::default();
    let document = timed_document("L");
    let id = install_timed(&state, &document);
    for (key, value, code) in [
        ("documentSnapshotId", json!("retired"), "stale_revision"),
        ("atRevision", json!(1), "stale_revision"),
        ("contextId", json!("component"), "unavailable"),
        ("layerUid", json!("missing"), "not_found"),
        ("frame", json!(8), "invalid_request"),
    ] {
        let mut request = timing_query(&state, &id, "L", 2);
        request["payload"][key] = value;
        let response = round_trip(&state, request).await;
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, code);
    }
    for (key, value, code) in [
        ("instanceId", "other-instance", "wrong_instance"),
        ("documentId", "other-document", "wrong_document"),
    ] {
        let mut request = timing_query(&state, &id, "L", 2);
        request[key] = json!(value);
        assert_eq!(public(&state, request)["error"]["code"], code);
    }
    let request = timing_query(&state, &id, "L", 2);
    state.retire_native_for_test(json!({"cleanupComplete":true}));
    assert_eq!(public(&state, request)["error"]["code"], "unavailable");
    let missing = ApplicationMcp::default();
    let mut document = document;
    document["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("timing");
    let id = install_timed(&missing, &document);
    let response = round_trip(&missing, timing_query(&missing, &id, "L", 0)).await;
    assert!(!response.ok);
    assert_eq!(response.error.unwrap().code, "unavailable");
}

#[test]
fn timing_invalid_public_input_refuses_before_borrowing_native_authority() {
    use std::sync::{mpsc, Arc};
    let state = Arc::new(ApplicationMcp::default());
    let id = install_timed(&state, &timed_document("L"));
    let mut request = timing_query(&state, &id, "L", 2);
    request["payload"]["frame"] = Value::Null;
    let before = state.native_project_info(&id).unwrap();
    let authority = state.native.lock().unwrap();
    let other = Arc::clone(&state);
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        send.send(other.dispatch_native(serde_json::from_value(request).unwrap()))
            .unwrap()
    });
    let result = receive.recv_timeout(Duration::from_secs(1));
    drop(authority);
    worker.join().unwrap();
    assert!(
        matches!(result, Ok(Err(_))),
        "invalid input must refuse before the locked native owner"
    );
    assert_eq!(state.native_project_info(&id).unwrap(), before);
}

#[tokio::test]
async fn timing_public_response_refuses_oversized_full_envelope_and_keeps_history() {
    let mut proved = false;
    for length in (3500..3900).step_by(10) {
        let state = ApplicationMcp::default();
        let uid = format!("data:雪{}", "x".repeat(length));
        let id = install_timed(&state, &timed_document(&uid));
        let request = timing_query(&state, &id, &uid, 2);
        if serde_json::to_vec(&request).unwrap().len() > 4096 {
            continue;
        }
        let info = state.native_project_info(&id).unwrap();
        let expected = json!({"apiVersion":2,"requestId":request["requestId"],"instanceId":state.instance_id(),
            "documentId":id,"contentRevision":0,"ok":true,"result":{"instanceId":state.instance_id(),"documentId":id,
                "atRevision":0,"documentSnapshotId":info["documentSnapshotId"],"contextId":"scene-root","layerUid":uid,
                "frame":2,"cellKind":"held","sourceFrame":1,"hasContent":true,"manualEdit":false}});
        if serde_json::to_vec(&expected).unwrap().len() <= 4096 {
            continue;
        }
        let response = round_trip(&state, request).await;
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "unavailable");
        assert!(response.result.is_none());
        assert_eq!(state.native_project_info(&id).unwrap(), info);
        proved = true;
        break;
    }
    assert!(
        proved,
        "bounded public selector must exercise oversized complete success envelope"
    );
}

#[test]
fn timing_original_host_bytes_reject_duplicates_unknown_null_and_positional_input() {
    let state = ApplicationMcp::default();
    let id = install_timed(&state, &timed_document("L"));
    let request = timing_query(&state, &id, "L", 2);
    let raw = request.to_string();
    let before = state.native_project_info(&id).unwrap();
    for bad in [
        raw.replace("\"frame\":2", "\"frame\":2,\"frame\":2"),
        raw.replace("\"frame\":2", "\"fr\\u0061me\":2,\"frame\":2"),
        raw.replace("\"payload\":", "\"lifecycleGeneration\":1,\"payload\":"),
        raw.replace("\"payload\":", "\"expectedRevision\":null,\"payload\":"),
        "[2,\"request\",\"instance\",\"document\"]".into(),
    ] {
        assert_ne!(bad, raw);
        assert!(
            wire::decode_json_bounded::<nemo_mcp::contract::NativeApplicationRequest>(
                bad.as_bytes(),
                4096
            )
            .is_err()
        );
    }
    let mut cancelled = request;
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        public(&state, cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(state.native_project_info(&id).unwrap(), before);
}

#[tokio::test]
async fn timing_public_wire_refuses_non_object_owner_before_its_dispatch_port() {
    use crate::native_dispatch::TerminalPump;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let mut state = ApplicationMcp::default();
    state.instance_id = "pump-fixture".into();
    let reservation = state.reserve_native_install().unwrap();
    let generation = reservation.generation();
    state
        .install_dispatch(generation, Box::new(TerminalPump(Arc::clone(&calls))))
        .unwrap();
    let request = json!({"apiVersion":2,"requestId":"timing-wrong-owner","instanceId":state.instance_id(),
        "documentId":"document-fixture","operation":"query.document.object.layer.timing",
        "payload":{"atRevision":0,"documentSnapshotId":"native-object:document-fixture:0","contextId":"scene-root","layerUid":"L","frame":0}});
    let response = round_trip(&state, request).await;
    assert!(!response.ok);
    let error = response.error.unwrap();
    assert_eq!(error.code, "unavailable");
    assert_eq!(error.details.unwrap()["disposition"], "notDispatched");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        state.native.lock().unwrap().active_generation(),
        Ok(generation)
    );
}
