//! Public native transport contract controls; the loopback peer is deliberately
//! untrusted. No running Nemo owner or installed acceptance is asserted.
use nemo_mcp::{
    contract::{NativeApplicationRequest, NativeApplicationResponse},
    registry::Endpoint,
    wire::{self, CancellationToken},
};
use serde_json::{json, Value};
use tokio::net::TcpListener;

fn request(operation: &str, payload: Value) -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: 2,
        request_id: "contract-boundary".into(),
        instance_id: "instance".into(),
        document_id: "document".into(),
        expected_revision: None,
        operation: operation.into(),
        payload,
        cancelled_before_dispatch: false,
    }
}

async fn accepts(request: &NativeApplicationRequest, result: Value) -> bool {
    // Inputs must be valid: otherwise no connection is made and awaiting a mock
    // accept would conceal the difference between ingress and reply rejection.
    request
        .validate()
        .expect("test sends an admitted public request");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: request.instance_id.clone(),
        port: listener.local_addr().unwrap().port(),
        secret: "private-test-secret".into(),
        build_id: "contract-fixture".into(),
    };
    let expected = request.clone();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let received: wire::NativeWireRequest = wire::read_json(&mut stream).await.unwrap();
        assert_eq!(received.secret, "private-test-secret");
        assert_eq!(received.native_request, expected);
        wire::write_json(
            &mut stream,
            &NativeApplicationResponse {
                api_version: 2,
                request_id: expected.request_id,
                instance_id: expected.instance_id,
                document_id: expected.document_id,
                content_revision: 0,
                ok: true,
                result: Some(result),
                error: None,
            },
        )
        .await
        .unwrap();
    });
    let accepted = wire::call_native(&endpoint, request.clone(), CancellationToken::new())
        .await
        .is_ok();
    peer.await.unwrap();
    accepted
}

fn key(frame: u32) -> Value {
    json!({"frame":frame,"v":[25],"hOut":[0,0],"hIn":[0,0],"curvePoints":[
        {"x":0,"y":0},{"x":0.25,"y":0.156},{"x":0.5,"y":0.5},
        {"x":0.75,"y":0.844},{"x":1,"y":1}]})
}
fn keyed_document() -> Value {
    json!({"format":"nemo.native-opacity-document","formatVersion":1,"totalFrames":5,
        "layers":[{"layerUid":"layer-a","motion":{"opacity":{"keys":[key(0),key(4)]}}}]})
}
fn replace(value: &Value, pointer: &str, replacement: Value) -> Value {
    let mut changed = value.clone();
    *changed
        .pointer_mut(pointer)
        .expect("literal fixture pointer") = replacement;
    changed
}
fn serialized(document: Value) -> Value {
    json!({"atRevision":0,"documentSnapshotId":"native-opacity:document:0","document":document})
}

#[tokio::test]
async fn serialized_document_transport_rejects_invalid_layer_and_track_structure() {
    let req = request("query.document.serialize", json!({"atRevision":0}));
    let valid = keyed_document();
    assert!(accepts(&req, serialized(valid.clone())).await);
    for (pointer, replacement) in [
        ("/totalFrames", json!(0)),
        ("/totalFrames", json!(null)),
        ("/formatVersion", json!(2)),
        ("/layers", json!({})),
        ("/layers", json!([])),
        ("/layers/0", json!({"layerUid":"layer-a"})),
        (
            "/layers/0",
            json!({"layerUid":7,"motionStatic":{"opacity":[25]}}),
        ),
        ("/layers/0", json!({"layerUid":"layer-a","extra":true})),
        ("/layers/0/motion", json!({})),
        ("/layers/0/motion/opacity", json!({})),
        ("/layers/0/motion/opacity/keys", json!({})),
        ("/layers/0/motion/opacity/keys", json!([])),
        (
            "/layers/0/motion/opacity/keys/0",
            json!({"frame":0,"v":[25]}),
        ),
        ("/layers/0/motion/opacity/keys/0/frame", json!(5)),
        ("/layers/0/motion/opacity/keys/0/frame", json!(null)),
        ("/layers/0/motion/opacity/keys/1/frame", json!(0)),
    ] {
        assert!(
            !accepts(&req, serialized(replace(&valid, pointer, replacement))).await,
            "{pointer}"
        );
    }
    for layer in [
        json!({"layerUid":"layer-a","motionStatic":{}}),
        json!({"layerUid":"layer-a","motionStatic":{"opacity":[-1]}}),
    ] {
        assert!(!accepts(&req, serialized(replace(&valid, "/layers/0", layer))).await);
    }
}

#[tokio::test]
async fn serialized_key_interpolation_never_accepts_changed_curves_or_nonzero_handles() {
    let req = request("query.document.serialize", json!({"atRevision":0}));
    let valid = keyed_document();
    for (pointer, replacement) in [
        (
            "/layers/0/motion/opacity/keys/0/curvePoints",
            json!([{"x":0,"y":0}]),
        ),
        (
            "/layers/0/motion/opacity/keys/0/curvePoints/1",
            json!({"x":0.25,"y":0.15}),
        ),
        (
            "/layers/0/motion/opacity/keys/0/curvePoints/1",
            json!([0.25, 0.156]),
        ),
        ("/layers/0/motion/opacity/keys/0/hOut", json!([0, 1])),
        ("/layers/0/motion/opacity/keys/0/hIn", json!([1, 0])),
        ("/layers/0/motion/opacity/keys/0/hOut", json!([0])),
        ("/layers/0/motion/opacity/keys/0/hIn", Value::Null),
    ] {
        assert!(
            !accepts(&req, serialized(replace(&valid, pointer, replacement))).await,
            "{pointer}"
        );
    }
}

#[tokio::test]
async fn evaluated_layers_require_unique_complete_numeric_values_and_pinned_coordinates() {
    let req = request(
        "query.document.evaluate",
        json!({"atRevision":0,"contextId":"scene-root","frame":2}),
    );
    let valid = json!({"documentSnapshotId":"native-opacity:document:0","documentId":"document",
        "contentRevision":0,"contextId":"scene-root","frame":2,
        "layers":[{"layerUid":"layer-a","value":0},{"layerUid":"layer-b","value":100}]});
    assert!(accepts(&req, valid.clone()).await);
    for (pointer, replacement) in [
        ("/layers/1/layerUid", json!("layer-a")),
        ("/layers/1/value", json!(101)),
        ("/layers/1/value", json!("25")),
        ("/layers/1", json!({"layerUid":"layer-b"})),
        (
            "/layers/1",
            json!({"layerUid":"layer-b","value":25,"private":true}),
        ),
        ("/layers", json!({})),
        ("/documentSnapshotId", json!("bad id")),
        ("/documentId", json!("replacement")),
        ("/contentRevision", json!(1)),
        ("/contextId", json!("other-context")),
        ("/frame", json!(3)),
    ] {
        assert!(
            !accepts(&req, replace(&valid, pointer, replacement)).await,
            "{pointer}"
        );
    }
}

#[tokio::test]
async fn diagnostics_transport_accepts_each_typed_failure_without_exposing_payloads() {
    let req = request("query.diagnostics.recent", json!({}));
    for code in [
        "invalid_request",
        "wrong_instance",
        "wrong_document",
        "stale_revision",
        "busy_conflict",
        "unavailable",
        "not_found",
        "cancelled_before_dispatch",
        "internal",
    ] {
        let result = json!({"records":[{"sequence":1,"requestIdRedacted":true,"targetId":"safe-target",
            "operation":"transaction.cancel","contentRevision":0,"ok":false,"errorCode":code}],"truncated":false});
        assert!(accepts(&req, result).await, "{code}");
    }
    let valid = json!({"records":[{"sequence":1,"requestIdRedacted":true,
        "operation":"history.undo","contentRevision":0,"ok":false,"errorCode":"internal"}],"truncated":false});
    for (pointer, replacement) in [
        ("/records", json!({})),
        ("/records/0/ok", Value::Null),
        ("/records/0/errorCode", json!("new_unregistered_code")),
        ("/records/0/errorCode", json!(7)),
    ] {
        assert!(
            !accepts(&req, replace(&valid, pointer, replacement)).await,
            "{pointer}"
        );
    }
}

#[tokio::test]
async fn reproduction_replay_admission_and_response_use_the_same_closed_bundle_contract() {
    let bundle = json!({"format":"nemo.native-opacity-reproduction","formatVersion":1,"apiVersion":2,
        "fixture":{"id":"native-opacity-static","version":1,"sha256":"a".repeat(64)},
        "command":"layer.opacity.set","stableTarget":{"layerUid":"layer-a"},"clock":null,"seed":null,
        "versions":{"nativeEngine":"0.1.0"},
        "commands":[{"id":1,"expectedRevision":0,"value":25,"revision":1,"applied":true}]});
    let req = request("query.reproduction.replay", json!({"bundle":bundle}));
    assert!(req.validate().is_ok());
    for payload in [
        json!({}),
        json!({"bundle":{}}),
        json!({"bundle":null}),
        json!({"bundle":req.payload["bundle"],"extra":true}),
    ] {
        let mut invalid = req.clone();
        invalid.payload = payload;
        assert!(invalid.validate().is_err());
    }
    let state = json!({"revision":0,"opacity":25,"undoDepth":0,"redoDepth":0});
    let result = json!({"initial":state,"steps":[{"id":1,"expectedRevision":0,"applied":true,
        "state":{"revision":1,"opacity":25,"undoDepth":1,"redoDepth":0}}]});
    assert!(accepts(&req, result.clone()).await);
    assert!(!accepts(&req, replace(&result, "/steps/0/id", json!(2))).await);
    assert!(
        !accepts(
            &req,
            replace(&result, "/steps/0/state/undoDepth", json!(33))
        )
        .await
    );
}

#[tokio::test]
async fn timing_transport_rejects_untyped_manual_flags_and_unrecognized_cell_kinds() {
    let payload = json!({"atRevision":0,"documentSnapshotId":"native-object:document:0",
        "contextId":"scene-root","layerUid":"layer-a","frame":2});
    let req = request("query.document.object.layer.timing", payload.clone());
    let mut valid = payload;
    valid.as_object_mut().unwrap().extend(
        json!({"instanceId":"instance","documentId":"document",
        "cellKind":"held","sourceFrame":1,"hasContent":true,"manualEdit":false})
        .as_object()
        .unwrap()
        .clone(),
    );
    assert!(accepts(&req, valid.clone()).await);
    for (pointer, replacement) in [
        ("/manualEdit", Value::Null),
        ("/manualEdit", json!(0)),
        ("/cellKind", json!("future-cell-kind")),
        ("/cellKind", Value::Null),
    ] {
        assert!(
            !accepts(&req, replace(&valid, pointer, replacement)).await,
            "{pointer}"
        );
    }
}
