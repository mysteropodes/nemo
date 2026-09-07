//! Compiled MCP stdio roundtrip through discovery and the application wire boundary.
use nemo_mcp::{
    contract::{ApplicationResponse, Operation},
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::json;
use std::time::Duration;
use tokio::{net::TcpListener, process::Command};

#[tokio::test]
async fn compiled_stdio_roundtrip_preserves_identity_and_errors() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint {
            instance_id: "11111111-1111-4111-8111-111111111111".into(),
            port: listener.local_addr().unwrap().port(),
            secret: "22222222-2222-4222-8222-222222222222".into(),
            build_id: "stdio-roundtrip".into(),
        };
        let _registration = Registration::create(root.path(), &endpoint).unwrap();
        let expected_instance = endpoint.instance_id.clone();
        let expected_secret = endpoint.secret.clone();
        let application = tokio::spawn(async move {
            for expected_operation in [
                Operation::Capabilities,
                Operation::Snapshot,
                Operation::PropertySet,
                Operation::PropertySet,
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request: wire::WireRequest = wire::read_json(&mut stream).await.unwrap();
                assert_eq!(request.secret, expected_secret);
                assert_eq!(request.request.operation, expected_operation);
                let stale = request.request.expected_revision == Some(6);
                let response = ApplicationResponse {
                    api_version: 1,
                    request_id: request.request.request_id,
                    instance_id: expected_instance.clone(),
                    document_id: "document-7".into(),
                    revision: if stale { 8 } else { 7 },
                    ok: !stale,
                    result: (!stale).then(|| json!({"revision": 7, "documentId": "document-7"})),
                    error: stale.then(|| nemo_mcp::contract::ApplicationError {
                        code: "stale_revision".into(),
                        message: "expected revision is stale".into(),
                    }),
                };
                wire::write_json(&mut stream, &response).await.unwrap();
            }
        });

        let mut command = Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
        command.env("NEMO_MCP_REGISTRY", root.path());
        let transport = TokioChildProcess::new(command).unwrap();
        let client = ().serve(transport).await.unwrap();

        let discover = client
            .call_tool(CallToolRequestParams::new("nemo_discover"))
            .await
            .unwrap();
        let discovered = &discover.structured_content.unwrap()["instances"][0];
        assert_eq!(discovered["instanceId"], endpoint.instance_id);
        assert_eq!(discovered["buildId"], "stdio-roundtrip");
        assert!(serde_json::to_string(discovered)
            .unwrap()
            .find(&endpoint.secret)
            .is_none());

        let snapshot = client
            .call_tool(
                CallToolRequestParams::new("nemo_query").with_arguments(
                    json!({"instance_id": endpoint.instance_id, "operation": "snapshot"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(snapshot.structured_content.as_ref().unwrap()["revision"], 7);
        assert_eq!(
            snapshot.structured_content.as_ref().unwrap()["documentId"],
            "document-7"
        );

        let command_args = json!({
            "apiVersion": 1,
            "requestId": "opacity-command-1",
            "instanceId": endpoint.instance_id,
            "documentId": "document-7",
            "expectedRevision": 7,
            "operation": "property.set",
            "payload": {"layerId": "layer-1", "property": "opacity", "value": 25}
        });
        let success = client
            .call_tool(
                CallToolRequestParams::new("nemo_command")
                    .with_arguments(command_args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        let success_value = success.structured_content.unwrap();
        assert_eq!(success_value["requestId"], "opacity-command-1");
        assert_eq!(success_value["instanceId"], endpoint.instance_id);
        assert_eq!(success_value["documentId"], "document-7");
        assert_eq!(success_value["result"]["revision"], 7);

        let stale_args = json!({
            "apiVersion": 1,
            "requestId": "opacity-command-stale",
            "instanceId": endpoint.instance_id,
            "documentId": "document-7",
            "expectedRevision": 6,
            "operation": "property.set",
            "payload": {"layerId": "layer-1", "property": "opacity", "value": 30}
        });
        let stale = client
            .call_tool(
                CallToolRequestParams::new("nemo_command")
                    .with_arguments(stale_args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(stale.is_error, Some(true));
        let stale_value = stale.structured_content.unwrap();
        assert_eq!(stale_value["requestId"], "opacity-command-stale");
        assert_eq!(stale_value["instanceId"], endpoint.instance_id);
        assert_eq!(stale_value["documentId"], "document-7");
        assert_eq!(stale_value["error"]["code"], "stale_revision");

        client.cancel().await.unwrap();
        application.await.unwrap();
    })
    .await
    .unwrap();
}
