//! Public compiled-stdio and real TCP error paths, with owned registry fixtures.
use nemo_mcp::{
    contract::{ApplicationResponse, NativeApplicationError, NativeApplicationResponse},
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::{
    model::{CallToolRequest, CallToolRequestParams, ClientRequest},
    service::PeerRequestOptions,
    transport::TokioChildProcess,
    ServiceExt,
};
use serde_json::{json, Value};
use std::{future::Future, path::Path, time::Duration};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::oneshot};

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(8), future)
        .await
        .expect("owned MCP fixture timed out")
}
async fn client(root: &Path) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    command.env("NEMO_MCP_REGISTRY", root).kill_on_drop(true);
    bounded(().serve(TokioChildProcess::new(command).unwrap()))
        .await
        .unwrap()
}
fn args(name: &'static str, value: Value) -> CallToolRequestParams {
    CallToolRequestParams::new(name).with_arguments(value.as_object().unwrap().clone())
}
fn create(instance: &str, id: &str) -> CallToolRequestParams {
    args(
        "nemo_command",
        json!({"apiVersion":2,"requestId":id,"instanceId":instance,
        "documentId":"document","expectedRevision":0,"operation":"command.document.object.layer.create",
        "payload":{"command":"object.layer.create","name":"Fixture"}}),
    )
}
async fn endpoint() -> (TcpListener, Endpoint) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: uuid::Uuid::new_v4().to_string(),
        port: listener.local_addr().unwrap().port(),
        secret: uuid::Uuid::new_v4().to_string(),
        build_id: "failure-fixture".into(),
    };
    (listener, endpoint)
}

#[tokio::test]
async fn registry_that_is_a_file_reports_unavailable_for_discovery_legacy_and_native() {
    let root = tempfile::tempdir().unwrap();
    let blocked = root.path().join("registry-file");
    std::fs::write(&blocked, b"not a directory").unwrap();
    let client = client(&blocked).await;
    for request in [
        CallToolRequestParams::new("nemo_discover"),
        args(
            "nemo_query",
            json!({"instanceId":"instance","operation":"capabilities"}),
        ),
        create("instance", "registry-native"),
    ] {
        let result = bounded(client.call_tool(request)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let body = result.structured_content.unwrap();
        assert_eq!(body["error"]["code"], "unavailable");
        assert_eq!(
            body["error"]["message"],
            "Nemo discovery registry is unavailable"
        );
    }
    bounded(client.cancel()).await.unwrap();
    assert_eq!(std::fs::read(&blocked).unwrap(), b"not a directory");
}

#[tokio::test]
async fn native_stdio_preserves_typed_errors_transport_failure_and_receipt_fenced_cancellation() {
    let root = tempfile::tempdir().unwrap();
    let (listener, endpoint) = endpoint().await;
    let _registration = Registration::create(root.path(), &endpoint).unwrap();
    let expected = endpoint.clone();
    let (received_tx, received_rx) = oneshot::channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    let peer = tokio::spawn(async move {
        let mut received_tx = Some(received_tx);
        let mut closed_tx = Some(closed_tx);
        for disposition in ["typed", "disconnect", "cancel", "recovery"] {
            let (mut stream, _) = bounded(listener.accept()).await.unwrap();
            let message: wire::NativeWireRequest =
                bounded(wire::read_json(&mut stream)).await.unwrap();
            assert_eq!(message.secret, expected.secret);
            assert_eq!(
                message.native_request.operation,
                "command.document.object.layer.create"
            );
            match disposition {
                "disconnect" => continue,
                "cancel" => {
                    received_tx.take().unwrap().send(()).unwrap();
                    assert_eq!(bounded(stream.read(&mut [0])).await.unwrap(), 0);
                    closed_tx.take().unwrap().send(()).unwrap();
                    continue;
                }
                _ => {}
            }
            let ok = disposition == "recovery";
            bounded(wire::write_json(&mut stream,&NativeApplicationResponse {
                api_version:2,request_id:message.native_request.request_id,instance_id:expected.instance_id.clone(),
                document_id:"document".into(),content_revision:if ok {1}else{0},ok,
                result:ok.then(||json!({"applied":true,"historyEntriesAdded":1,"layerUid":"created-layer"})),
                error:(!ok).then(||NativeApplicationError {code:"busy_conflict".into(),message:"fixture owns command slot".into(),details:Some(json!({}))}),
            })).await.unwrap();
        }
    });
    let client = client(root.path()).await;
    let typed = bounded(client.call_tool(create(&endpoint.instance_id, "typed")))
        .await
        .unwrap();
    assert_eq!(typed.is_error, Some(true));
    let body = typed.structured_content.unwrap();
    assert_eq!(body["requestId"], "typed");
    assert_eq!(body["error"]["code"], "busy_conflict");
    let disconnected = bounded(client.call_tool(create(&endpoint.instance_id, "disconnect")))
        .await
        .unwrap();
    assert_eq!(disconnected.is_error, Some(true));
    assert_eq!(
        disconnected.structured_content.unwrap()["error"]["code"],
        "unavailable"
    );
    let pending = bounded(client.send_request_with_option(
        ClientRequest::CallToolRequest(CallToolRequest::new(create(
            &endpoint.instance_id,
            "cancel",
        ))),
        PeerRequestOptions::no_options(),
    ))
    .await
    .unwrap();
    bounded(received_rx).await.unwrap();
    bounded(pending.cancel(Some("fixture after-receipt cancellation".into())))
        .await
        .unwrap();
    bounded(closed_rx).await.unwrap();
    let recovery = bounded(client.call_tool(create(&endpoint.instance_id, "recovery")))
        .await
        .unwrap();
    assert_ne!(recovery.is_error, Some(true));
    assert_eq!(
        recovery.structured_content.unwrap()["result"]["layerUid"],
        "created-layer"
    );
    bounded(peer).await.unwrap();
    bounded(client.cancel()).await.unwrap();
}

async fn discovery_status_fixture(timeout: bool) {
    let root = tempfile::tempdir().unwrap();
    let (listener, endpoint) = endpoint().await;
    let _registration = Registration::create(root.path(), &endpoint).unwrap();
    let expected = endpoint.clone();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = bounded(listener.accept()).await.unwrap();
        let message: wire::WireRequest = bounded(wire::read_json(&mut stream)).await.unwrap();
        assert_eq!(message.secret, expected.secret);
        bounded(wire::write_json(
            &mut stream,
            &ApplicationResponse {
                api_version: 1,
                request_id: message.request.request_id,
                instance_id: expected.instance_id.clone(),
                document_id: "legacy-document".into(),
                revision: 7,
                ok: true,
                result: Some(json!({"nativeApiVersion":2})),
                error: None,
            },
        ))
        .await
        .unwrap();
        drop(stream);
        let (mut stream, _) = bounded(listener.accept()).await.unwrap();
        let status: Value = bounded(wire::read_json(&mut stream)).await.unwrap();
        assert_eq!(status["secret"], expected.secret);
        assert_eq!(status["nativeStatus"]["instanceId"], expected.instance_id);
        if timeout {
            // Wait for the real two-second discovery deadline to close this
            // exact received request; no sleep or narrow timing assertion.
            assert_eq!(bounded(stream.read(&mut [0])).await.unwrap(), 0);
        } else {
            bounded(wire::write_json(
                &mut stream,
                &json!({"apiVersion":2,
                "requestId":status["nativeStatus"]["requestId"],"instanceId":expected.instance_id,
                "available":true,"lifecycleGeneration":41}),
            ))
            .await
            .unwrap();
        }
    });
    let client = client(root.path()).await;
    let result = bounded(client.call_tool(CallToolRequestParams::new("nemo_discover")))
        .await
        .unwrap();
    let body = result.structured_content.unwrap();
    assert_eq!(body["instances"].as_array().unwrap().len(), 1);
    let instance = &body["instances"][0];
    assert_eq!(instance["instanceId"], endpoint.instance_id);
    assert_eq!(instance["documentId"], "legacy-document");
    assert_eq!(instance["revision"], 7);
    let status = &instance["nativeHostStatus"];
    assert_eq!(status["available"], false);
    assert!(status.get("documentId").is_none());
    assert!(status.get("contentRevision").is_none());
    assert!(status.get("lifecycleGeneration").is_none());
    let reason = status["reason"].as_str().unwrap();
    if timeout {
        assert_eq!(reason, "native host status timed out");
    } else {
        assert!(
            reason.starts_with("native host status unavailable: "),
            "{reason}"
        );
    }
    bounded(peer).await.unwrap();
    bounded(client.cancel()).await.unwrap();
}
#[tokio::test]
async fn native_discovery_preserves_legacy_identity_when_status_is_invalid() {
    discovery_status_fixture(false).await;
}
#[tokio::test]
async fn native_discovery_reports_its_real_status_deadline_without_inventing_native_identity() {
    discovery_status_fixture(true).await;
}

#[test]
fn command_schema_has_a_version_distinct_cache_identity() {
    use schemars::JsonSchema;
    assert_eq!(
        nemo_mcp::server::CommandRequest::schema_id(),
        "nemo::CommandRequest:v2-extension"
    );
    assert_ne!(
        nemo_mcp::server::CommandRequest::schema_id(),
        nemo_mcp::contract::ApplicationRequest::schema_id()
    );
}
