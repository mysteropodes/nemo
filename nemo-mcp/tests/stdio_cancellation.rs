//! Cancellation crosses the compiled MCP protocol; application state is queried afterward.
//! This fixture does not establish installed-client or application rollback acceptance.
use nemo_mcp::{
    contract::{ApplicationResponse, Operation},
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::{
    model::{CallToolRequest, CallToolRequestParams, ClientRequest},
    service::PeerRequestOptions,
    transport::TokioChildProcess,
    ServiceExt,
};
use serde_json::json;
use std::{future::Future, time::Duration};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::oneshot, task::JoinHandle};

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("MCP lifecycle operation timed out")
}

struct EndpointTask(JoinHandle<()>);
impl Drop for EndpointTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn cancelled_write_releases_connection_and_same_client_can_query_state() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: uuid::Uuid::new_v4().to_string(),
        port: listener.local_addr().unwrap().port(),
        secret: uuid::Uuid::new_v4().to_string(),
        build_id: "cancellation-fixture".into(),
    };
    let _registration = Registration::create(root.path(), &endpoint).unwrap();
    let selected_instance = endpoint.instance_id.clone();
    let (received_tx, received_rx) = oneshot::channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    let mut server = EndpointTask(tokio::spawn(async move {
        let (mut stream, _) = bounded(listener.accept()).await.unwrap();
        let write: wire::WireRequest = bounded(wire::read_json(&mut stream)).await.unwrap();
        assert_eq!(write.secret, endpoint.secret);
        assert_eq!(
            write.request.instance_id.as_deref(),
            Some(endpoint.instance_id.as_str())
        );
        assert_eq!(write.request.request_id, "cancelled-edit");
        assert_eq!(write.request.operation, Operation::PropertySet);
        assert_eq!(write.request.document_id.as_deref(), Some("document"));
        assert_eq!(write.request.expected_revision, Some(8));
        assert_eq!(
            write.request.payload,
            json!({"layerId":"layer", "property":"opacity", "value":35})
        );
        received_tx.send(()).unwrap();
        // No response is sent. Cancellation must release the outstanding IPC stream.
        assert_eq!(bounded(stream.read(&mut [0u8; 1])).await.unwrap(), 0);
        closed_tx.send(()).unwrap();

        let (mut stream, _) = bounded(listener.accept()).await.unwrap();
        let query: wire::WireRequest = bounded(wire::read_json(&mut stream)).await.unwrap();
        assert_eq!(query.secret, endpoint.secret);
        assert_eq!(
            query.request.instance_id.as_deref(),
            Some(endpoint.instance_id.as_str())
        );
        assert_eq!(query.request.operation, Operation::Snapshot);
        // A cancelled write may already have committed. The adapter must query its owner.
        let response = ApplicationResponse {
            api_version: 1,
            request_id: query.request.request_id,
            instance_id: endpoint.instance_id,
            document_id: "document".into(),
            revision: 9,
            ok: true,
            result: Some(json!({"opacity":35})),
            error: None,
        };
        bounded(wire::write_json(&mut stream, &response))
            .await
            .unwrap();
    }));

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    command
        .env("NEMO_MCP_REGISTRY", root.path())
        .kill_on_drop(true);
    let client = bounded(().serve(TokioChildProcess::new(command).unwrap()))
        .await
        .unwrap();
    let request = CallToolRequestParams::new("nemo_command").with_arguments(
        json!({"apiVersion":1, "requestId":"cancelled-edit", "instanceId":selected_instance,
            "documentId":"document", "expectedRevision":8, "operation":"property.set",
            "payload":{"layerId":"layer", "property":"opacity", "value":35}})
        .as_object()
        .unwrap()
        .clone(),
    );
    let pending = bounded(client.send_request_with_option(
        ClientRequest::CallToolRequest(CallToolRequest::new(request)),
        PeerRequestOptions::no_options(),
    ))
    .await
    .unwrap();
    bounded(received_rx).await.unwrap();
    bounded(pending.cancel(Some("fixture cancellation".into())))
        .await
        .unwrap();
    bounded(closed_rx).await.unwrap();

    let snapshot = bounded(
        client.call_tool(
            CallToolRequestParams::new("nemo_query").with_arguments(
                json!({"instance_id":selected_instance, "operation":"snapshot"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ),
    )
    .await
    .unwrap();
    assert_ne!(snapshot.is_error, Some(true));
    let state = snapshot.structured_content.unwrap();
    assert_eq!(state["instanceId"], selected_instance);
    assert_eq!(state["documentId"], "document");
    assert_eq!(state["revision"], 9);
    assert_eq!(state["result"], json!({"opacity":35}));
    bounded(&mut server.0).await.unwrap();
    bounded(client.cancel()).await.unwrap();
}
