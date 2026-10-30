//! Official SDK + compiled stdio transport contract; live-owner semantics are host tests.
use nemo_mcp::{
    contract::NativeApplicationResponse,
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::{json, Value};
use std::{future::Future, time::Duration};
use tokio::net::TcpListener;
async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(8), future)
        .await
        .expect("history stdio fixture timed out")
}
fn request(instance: &str, id: &str) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":instance,"documentId":"document",
        "operation":"query.document.object.history","payload":{"atRevision":7,
        "documentSnapshotId":"native-object:document:7"}})
}
#[tokio::test]
async fn official_sdk_stdio_admits_history_and_refuses_tampered_reply_and_malformed_pins() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: uuid::Uuid::new_v4().to_string(),
        port: listener.local_addr().unwrap().port(),
        secret: uuid::Uuid::new_v4().to_string(),
        build_id: "history-stdio".into(),
    };
    let _registration = Registration::create(root.path(), &endpoint).unwrap();
    let expected = endpoint.clone();
    let peer = tokio::spawn(async move {
        for tamper in [false, true] {
            let (mut stream, _) = bounded(listener.accept()).await.unwrap();
            let message: wire::NativeWireRequest =
                bounded(wire::read_json(&mut stream)).await.unwrap();
            assert_eq!(message.secret, expected.secret);
            assert_eq!(
                message.native_request.operation,
                "query.document.object.history"
            );
            assert!(message.native_request.expected_revision.is_none());
            let snapshot = if tamper {
                "foreign-snapshot"
            } else {
                "native-object:document:7"
            };
            let response = NativeApplicationResponse {
                api_version: 2,
                request_id: message.native_request.request_id,
                instance_id: expected.instance_id.clone(),
                document_id: "document".into(),
                content_revision: 7,
                ok: true,
                error: None,
                result: Some(
                    json!({"atRevision":7,"documentSnapshotId":snapshot,"undoDepth":2,"redoDepth":1,
                        "offset":0,"entries":[{"index":0,"label":"Draw"},{"index":1,"label":"Rename Layer"},
                        {"index":2,"label":"Edit"}]}),
                ),
            };
            bounded(wire::write_json(&mut stream, &response))
                .await
                .unwrap();
        }
        // Malformed SDK input must fail before opening another app connection.
        assert!(
            tokio::time::timeout(Duration::from_millis(250), listener.accept())
                .await
                .is_err()
        );
    });
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    process
        .env("NEMO_MCP_REGISTRY", root.path())
        .kill_on_drop(true);
    let client = bounded(().serve(TokioChildProcess::new(process).unwrap()))
        .await
        .unwrap();
    for tamper in [false, true] {
        let input = request(
            &endpoint.instance_id,
            if tamper { "tampered" } else { "history-read" },
        );
        let response = bounded(
            client.call_tool(
                CallToolRequestParams::new("nemo_command")
                    .with_arguments(input.as_object().unwrap().clone()),
            ),
        )
        .await
        .unwrap();
        let body = response.structured_content.unwrap();
        if tamper {
            assert_eq!(response.is_error, Some(true));
            assert_eq!(body["error"]["code"], "unavailable");
        } else {
            assert_ne!(response.is_error, Some(true));
            assert_eq!(body["ok"], true);
            assert_eq!(
                body["result"],
                json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","undoDepth":2,"redoDepth":1,
                    "offset":0,"entries":[{"index":0,"label":"Draw"},{"index":1,"label":"Rename Layer"},
                    {"index":2,"label":"Edit"}]})
            );
        }
    }
    let mut bad = request(&endpoint.instance_id, "bad-history");
    bad["payload"]["extra"] = json!(true);
    let rejected = bounded(client.call_tool(
        CallToolRequestParams::new("nemo_command").with_arguments(bad.as_object().unwrap().clone()),
    ))
    .await;
    assert!(rejected.is_err() || rejected.unwrap().is_error == Some(true));
    bounded(peer).await.unwrap();
    bounded(client.cancel()).await.unwrap();
}
