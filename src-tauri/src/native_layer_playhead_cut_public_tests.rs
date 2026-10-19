//! Real sole-host/bootstrap/history/raw-client/authenticated wire, no installed credit.
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const OP: &str = "command.document.object.layer.cut";
fn drawing() -> Value {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut records = Vec::new();
    for (uid, stroke, frame) in [("a", "ink", 0), ("a", "ink", 15), ("b", "background", 0)] {
        let mut record = corpus["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","layerUid":uid,"strokeId":stroke,"frameScope":{"kind":"authored","frame":frame}});
        records.push(record);
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":24,
        "layers":[{"layerUid":"a","name":"Ink","locked":true,"range":{"inPoint":2,"outPoint":20,"markers":[{"frame":12,"name":"keep"}]},
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0},{"frame":15}],"samples":[]}},
            {"layerUid":"b","name":"Background","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
        "objects":records,"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"a","naturalPosition":0,"order":{}},{"layerUid":"b","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"a","strokeId":"ink","naturalPosition":0,"order":{}},{"layerUid":"b","strokeId":"background","naturalPosition":0,"order":{}}]},
                    {"frameScope":{"kind":"authored","frame":15},"elements":[{"layerUid":"a","strokeId":"ink","naturalPosition":0,"order":{}}]}]}]}})
}
fn install(state: &ApplicationMcp, video_layer: Option<&str>) -> Value {
    let mut project = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":drawing(),"viewportReference":null});
    if let Some(uid) = video_layer {
        project["assets"] = json!([{"id":"v1","name":"take","kind":"video","source":{"kind":"linked","locator":"clip.mov"}}]);
        project["assetInventory"] = json!({"assetIds":["v1"],"placements":[{"id":"p1","assetId":"v1",
            "target":{"kind":"drawing","contextId":"scene-root","layerUid":uid},"offsetFrames":5}]});
    }
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"cut-install",
        "instanceId":state.instance_id(),"documentJson":project.to_string()})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn request(receipt: &Value, id: &str, revision: u64, frame: u32) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],"documentId":receipt["documentId"],
        "expectedRevision":revision,"operation":OP,"payload":{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":frame}})
}
fn dispatch(state: &ApplicationMcp, request: Value) -> Value {
    serde_json::to_value(
        state
            .dispatch_native(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn saved(state: &ApplicationMcp, receipt: &Value) -> Vec<u8> {
    let document = receipt["documentId"].as_str().unwrap();
    let info = state.native_project_info(document).unwrap();
    state
        .native_project_file_snapshot(
            document,
            info["contentRevision"].as_u64().unwrap(),
            info["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
fn info(state: &ApplicationMcp, receipt: &Value) -> Value {
    state
        .native_project_info(receipt["documentId"].as_str().unwrap())
        .unwrap()
}
fn undo(state: &ApplicationMcp, receipt: &Value, revision: u64, id: &str) {
    let mut req = request(receipt, id, revision, 10);
    req["operation"] = json!("command.document.object.undo");
    req["payload"] = json!({"command":"object.undo"});
    assert_eq!(dispatch(state, req)["ok"], true);
}
#[test]
fn public_cut_dispatch_uses_real_owner_and_full_project_save_replay_after_undo_and_noop_redo() {
    let state = ApplicationMcp::default();
    let receipt = install(&state, None);
    let before = saved(&state, &receipt);
    let req = request(&receipt, "public-cut", 0, 10);
    let first = dispatch(&state, req.clone());
    assert_eq!(first["ok"], true);
    assert_eq!(first["contentRevision"], 1);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:1:0"})
    );
    for key in ["requestId", "instanceId", "documentId"] {
        assert_eq!(first[key], req[key]);
    }
    let changed = saved(&state, &receipt);
    let project: Value = serde_json::from_slice(&changed).unwrap();
    assert_eq!(project["drawing"]["layers"].as_array().unwrap().len(), 3);
    assert_eq!(project["drawing"]["layers"][0]["range"]["outPoint"], 9);
    assert_eq!(project["drawing"]["layers"][1]["range"]["inPoint"], 10);
    assert_eq!(project["drawing"]["layers"][1]["locked"], false);
    assert_eq!(info(&state, &receipt)["undoDepth"], 1);
    assert_eq!(dispatch(&state, req.clone()), first);
    assert_eq!(saved(&state, &receipt), changed);
    let mut altered = req.clone();
    altered["payload"]["frame"] = json!(11);
    assert_eq!(
        dispatch(&state, altered)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(saved(&state, &receipt), changed);
    undo(&state, &receipt, 1, "cut-undo");
    assert_eq!(saved(&state, &receipt), before);
    assert_eq!(dispatch(&state, req), first);
    assert_eq!(saved(&state, &receipt), before);
    let noop = dispatch(&state, request(&receipt, "cut-outside", 2, 2));
    assert_eq!(
        noop["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(info(&state, &receipt)["contentRevision"], 2);
    assert_eq!(info(&state, &receipt)["redoDepth"], 1);
    // A normal full-project bootstrap of the changed save retains the result.
    let reopened = ApplicationMcp::default();
    let open=serde_json::to_value(bootstrap_raw(&reopened,&json!({"apiVersion":2,"requestId":"cut-reopen",
        "instanceId":reopened.instance_id(),"documentJson":String::from_utf8(changed.clone()).unwrap()}).to_string()).unwrap()).unwrap();
    assert_eq!(saved(&reopened, &open), changed);
}
#[test]
fn public_cut_raw_original_bytes_refuse_duplicates_size_cancel_stale_foreign_nonroot_and_video_without_partial_state(
) {
    let state = ApplicationMcp::default();
    let receipt = install(&state, None);
    let before = saved(&state, &receipt);
    let req = request(&receipt, "raw-cut", 0, 10);
    let raw = req.to_string();
    let duplicate = raw.replace("\"frame\":10", "\"frame\":10,\"frame\":11");
    assert_ne!(duplicate, raw);
    assert!(state.dispatch_object_request_json(&duplicate).is_err());
    let mut limit = request(&receipt, "raw-noop", 0, 2).to_string();
    limit.push_str(&" ".repeat(4096 - limit.len()));
    assert_eq!(
        state
            .dispatch_object_request_json(&limit)
            .unwrap()
            .result
            .unwrap(),
        json!({"applied":false,"historyEntriesAdded":0})
    );
    limit.push(' ');
    assert!(state.dispatch_object_request_json(&limit).is_err());
    for (id, key, value, code) in [
        (
            "cut-cancel",
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        ("cut-stale", "expectedRevision", json!(1), "stale_revision"),
        (
            "cut-instance",
            "instanceId",
            json!("foreign-instance"),
            "wrong_instance",
        ),
        (
            "cut-document",
            "documentId",
            json!("foreign-document"),
            "wrong_document",
        ),
    ] {
        let mut r = request(&receipt, id, 0, 10);
        r[key] = value;
        let response = dispatch(&state, r);
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], code);
        assert_eq!(saved(&state, &receipt), before);
        assert_eq!(info(&state, &receipt)["undoDepth"], 0);
    }
    let mut nested = request(&receipt, "cut-nonroot", 0, 10);
    nested["payload"]["contextId"] = json!("component context/α");
    let typed: NativeApplicationRequest = serde_json::from_value(nested.clone()).unwrap();
    assert!(typed.validate().is_ok());
    assert_eq!(dispatch(&state, nested)["error"]["code"], "unavailable");
    assert_eq!(saved(&state, &receipt), before);
    assert_eq!(state.dispatch_object_request_json(&raw).unwrap().ok, true);
    let changed = saved(&state, &receipt);
    assert_ne!(changed, before);
    for target in ["a", "b"] {
        let video = ApplicationMcp::default();
        let receipt = install(&video, Some(target));
        let before = saved(&video, &receipt);
        let reply = dispatch(&video, request(&receipt, "video-cut", 0, 10));
        if target == "a" {
            assert_eq!(reply["error"]["code"], "unavailable");
            assert_eq!(saved(&video, &receipt), before);
        } else {
            assert_eq!(reply["ok"], true);
            let old: Value = serde_json::from_slice(&before).unwrap();
            let new: Value = serde_json::from_slice(&saved(&video, &receipt)).unwrap();
            for field in ["assets", "assetInventory", "project"] {
                assert_eq!(new[field], old[field]);
            }
        }
    }
}

// The same production authenticated TCP handler/authority used by the existing
// placement tests; no invented NativeWireServer type or mock ACK store.
async fn external(
    state: &ApplicationMcp,
    request: Value,
    binding: Option<&Value>,
) -> (NativeApplicationResponse, usize) {
    use crate::application_mcp::revision_sync::RevisionControl;
    use nemo_mcp::{registry::Endpoint, wire};
    use std::time::Duration;
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "isolated-cut-native-wire".into(),
        build_id: "cut-wire-test".into(),
    };
    let mut acknowledgments = 0;
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let envelope: wire::AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
        let wire::AuthenticatedWireRequest::Native(message) = envelope else {
            panic!("cut must retain native authenticated envelope")
        };
        assert_eq!(message.secret, endpoint.secret);
        assert_eq!(
            serde_json::to_value(&message.native_request).unwrap(),
            request
        );
        state
            .serve_external_for_test(&mut stream, message.native_request, |event| {
                let binding =
                    binding.expect("refusal/noop/replay must not publish a revision event");
                let observed = serde_json::to_value(&event).unwrap();
                for key in ["requestId", "instanceId", "documentId"] {
                    assert_eq!(observed[key], request[key]);
                }
                assert_eq!(
                    observed["lifecycleGeneration"],
                    binding["lifecycleGeneration"]
                );
                assert_eq!(observed["fromRevision"], request["expectedRevision"]);
                assert_eq!(observed["toRevision"], 1);
                assert_eq!(info(state, &request)["contentRevision"], 1);
                let project: Value = serde_json::from_slice(&saved(state, &request)).unwrap();
                assert_eq!(
                    project["drawing"]["layers"][1]["layerUid"],
                    "native-layer:1:0"
                );
                state.revisions.control(
                    &state.native,
                    RevisionControl::Acknowledge {
                        subscription_id: binding["subscriptionId"].as_str().unwrap().to_owned(),
                        event,
                    },
                )?;
                acknowledgments += 1;
                Ok(())
            })
            .await
            .unwrap();
    };
    let client = wire::call_native(
        &endpoint,
        serde_json::from_value(request.clone()).unwrap(),
        wire::CancellationToken::new(),
    );
    let (_, response) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    (response.unwrap(), acknowledgments)
}
#[tokio::test]
async fn authenticated_cut_requires_actual_subscriber_then_acknowledges_one_real_revision_and_replays_after_undo(
) {
    use crate::application_mcp::revision_sync::RevisionControl;
    let state = ApplicationMcp::default();
    let receipt = install(&state, None);
    let before = saved(&state, &receipt);
    let (refused, events) =
        external(&state, request(&receipt, "cut-no-subscriber", 0, 10), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &receipt), before);
    let binding = state
        .revisions
        .control(&state.native, RevisionControl::Binding)
        .unwrap();
    state
        .revisions
        .control(
            &state.native,
            serde_json::from_value(json!({"action":"subscribe","binding":binding})).unwrap(),
        )
        .unwrap();
    let req = request(&receipt, "external-cut", 0, 10);
    let (first, events) = external(&state, req.clone(), Some(&binding)).await;
    assert!(first.ok, "{first:?}");
    assert_eq!(first.content_revision, 1);
    assert_eq!(events, 1);
    assert_eq!(
        first.result.as_ref().unwrap(),
        &json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:1:0"})
    );
    let changed = saved(&state, &receipt);
    let (replay, events) = external(&state, req.clone(), None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &receipt), changed);
    undo(&state, &receipt, 1, "wire-cut-undo");
    let (replay, events) = external(&state, req, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &receipt), before);
}
