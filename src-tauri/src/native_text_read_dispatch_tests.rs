//! Real public owner/transport text reads; no installed client acceptance.
use super::{invoke, saved};
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};
const OP: &str = "query.document.object.text";
fn target() -> Value {
    json!({"contextId":"scene-root","layerUid":"L0","frameScope":{"kind":"authored","frame":0},"strokeId":"native-text:reopened"})
}
fn install_text(state: &ApplicationMcp) -> (String, String) {
    let text = "A雪🙂\\\"\n".repeat(700);
    let source = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
            "layers":[{"layerUid":"L0"}],"objects":[],"items":{"text":[{"target":target(),
                "block":{"text":text,"font":{"kind":"bundled","fontKey":"Roboto-Regular"},
                    "size":24,"color":"#11223380","align":"left","fixedWidth":null,
                    "anchorTopLeft":{"x":10,"y":20}}}]}}});
    let receipt = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"text-install",
        "instanceId":state.instance_id(),"documentJson":source.to_string()})
        .to_string(),
    )
    .unwrap();
    (
        serde_json::to_value(receipt).unwrap()["documentId"]
            .as_str()
            .unwrap()
            .into(),
        text,
    )
}
fn query_text(state: &ApplicationMcp, id: &str, view: Value) -> NativeApplicationRequest {
    let info = state.native_project_info(id).unwrap();
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"text-public-read","instanceId":state.instance_id(),
        "documentId":id,"operation":OP,"payload":{"atRevision":info["contentRevision"],
            "documentSnapshotId":info["documentSnapshotId"],"view":view}}),
    )
    .unwrap()
}
fn result(state: &ApplicationMcp, q: NativeApplicationRequest, external: bool) -> Value {
    let expected = q.clone();
    let delivery = invoke(state, q, external);
    assert!(delivery.notification.is_none());
    let r = delivery.response;
    assert!(r.ok, "{r:?}");
    assert_eq!(r.request_id, expected.request_id);
    assert_eq!(r.instance_id, expected.instance_id);
    assert_eq!(r.document_id, expected.document_id);
    assert_eq!(
        r.content_revision,
        expected.payload["atRevision"].as_u64().unwrap()
    );
    assert!(serde_json::to_vec(&r).unwrap().len() <= 4096);
    let v = r.result.unwrap();
    assert_eq!(v["instanceId"], expected.instance_id);
    assert_eq!(v["documentId"], expected.document_id);
    assert_eq!(v["atRevision"], expected.payload["atRevision"]);
    assert_eq!(
        v["documentSnapshotId"],
        expected.payload["documentSnapshotId"]
    );
    assert!(serde_json::to_vec(&v).unwrap().len() <= 3072);
    v
}
#[test]
fn text_public_dispatch_preserves_native_targets_presence_and_bounded_utf8_chunks() {
    let state = ApplicationMcp::default();
    let (id, original) = install_text(&state);
    let before = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    for external in [false, true] {
        let page = result(
            &state,
            query_text(
                &state,
                &id,
                json!({"kind":"targets",
            "scope":{"contextId":"scene-root","layerUid":"L0","frameScope":{"kind":"authored","frame":0}},
            "offset":0,"limit":8}),
            ),
            external,
        );
        assert_eq!(page["items"], json!([{"target":target()}]));
        assert_eq!(page["total"], 1);
        assert!(page["nextOffset"].is_null());
        let meta = result(
            &state,
            query_text(&state, &id, json!({"kind":"inspect","target":target()})),
            external,
        );
        assert_eq!(meta["target"], target());
        assert_eq!(meta["textBytes"], original.len());
        assert!(meta["block"].get("text").is_none());
        assert!(meta["block"].get("fixedWidth").unwrap().is_null());
        assert!(meta["block"].get("fixedHeight").is_none());
        let (mut joined, mut offset, mut chunks) = (String::new(), 0usize, 0);
        loop {
            let chunk = result(
                &state,
                query_text(
                    &state,
                    &id,
                    json!({"kind":"text","target":target(),"offset":offset,"limit":1024}),
                ),
                external,
            );
            assert_eq!(chunk["target"], target());
            assert_eq!(chunk["offset"], offset);
            assert_eq!(chunk["textBytes"], original.len());
            let part = chunk["text"].as_str().unwrap();
            assert!(!part.is_empty());
            joined.push_str(part);
            chunks += 1;
            if chunk["nextOffset"].is_null() {
                break;
            }
            let next = chunk["nextOffset"].as_u64().unwrap() as usize;
            assert_eq!(next, offset + part.len());
            assert!(next > offset && next < original.len() && original.is_char_boundary(next));
            offset = next;
            assert!(chunks <= original.len());
        }
        assert!(chunks > 1);
        assert_eq!(joined, original);
    }
    assert_eq!(saved(&state, &id), before);
    assert_eq!(state.native_project_info(&id).unwrap(), info);
}
#[test]
fn text_public_refusals_preserve_owner_and_storage() {
    let state = ApplicationMcp::default();
    let (id, _) = install_text(&state);
    let before = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let q = query_text(&state, &id, json!({"kind":"inspect","target":target()}));
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
    r.payload["view"]["target"]["strokeId"] = json!("absent");
    cases.push((r, "not_found"));
    let r = query_text(
        &state,
        &id,
        json!({"kind":"text","target":target(),"offset":2,"limit":8}),
    );
    cases.push((r, "invalid_request")); // offset2 is inside the first multibyte scalar.
    for (r, code) in cases {
        let delivery = invoke(&state, r, true);
        assert!(delivery.notification.is_none());
        assert!(!delivery.response.ok);
        assert!(delivery.response.result.is_none());
        assert_eq!(delivery.response.error.unwrap().code, code);
        assert_eq!(saved(&state, &id), before);
        assert_eq!(state.native_project_info(&id).unwrap(), info);
    }
}
#[tokio::test]
async fn text_actual_tcp_reads_committed_head_while_ui_ack_fences_writes() {
    use nemo_mcp::{
        registry::Endpoint,
        wire::{self, AuthenticatedWireRequest},
    };
    use tokio::net::TcpListener;
    let state = ApplicationMcp::default();
    let (id, original) = install_text(&state);
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let mut command: NativeApplicationRequest = serde_json::from_value(json!({"apiVersion":2,
        "requestId":"text-pending-write","instanceId":state.instance_id(),"documentId":id,"expectedRevision":0,
        "operation":"command.document.object.layer.visibility.set","payload":{"command":"object.layer.visibility.set","layerUid":"L0","visible":false}})).unwrap();
    let committed = invoke(&state, command.clone(), true);
    assert!(committed.response.ok);
    let _held = committed
        .notification
        .expect("real ACK receiver must remain alive");
    let before = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "isolated-text-read".into(),
        build_id: "text-read-test".into(),
    };
    for view in [
        json!({"kind":"inspect","target":target()}),
        json!({"kind":"text","target":target(),"offset":0,"limit":64}),
    ] {
        let q = query_text(&state, &id, view);
        let server = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let msg: AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
            let AuthenticatedWireRequest::Native(msg) = msg else {
                panic!("native envelope required")
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
        .expect("text read must not wait for write ACK");
        let response: NativeApplicationResponse = response.unwrap();
        assert!(response.ok, "{response:?}");
        assert_eq!(response.content_revision, 1);
        assert_eq!(response.request_id, q.request_id);
        assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
        assert_eq!(response.result.unwrap()["textBytes"], original.len());
    }
    command.request_id = "text-still-fenced".into();
    command.expected_revision = Some(1);
    let blocked = invoke(&state, command, true);
    assert!(!blocked.response.ok);
    assert_eq!(blocked.response.error.unwrap().code, "unavailable");
    assert!(blocked.notification.is_none());
    assert_eq!(saved(&state, &id), before);
    assert_eq!(state.native_project_info(&id).unwrap(), info);
}
