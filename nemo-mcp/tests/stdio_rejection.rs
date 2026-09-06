//! Invalid MCP writes must be rejected before reaching the document owner.
use nemo_mcp::{
    contract::ApplicationResponse,
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    net::TcpListener,
    time::{timeout, Duration},
};

struct EndpointTask(tokio::task::JoinHandle<()>);
impl Drop for EndpointTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn executable_rejects_invalid_writes_before_application_transport() {
    timeout(Duration::from_secs(15), async {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint {
            instance_id: "96925c1b-2ebd-46a3-adc2-3cad240895cd".into(),
            port: listener.local_addr().unwrap().port(),
            secret: "d7ad6ee3-6325-43d3-85c4-105218878cc8".into(),
            build_id: "protocol-fixture".into(),
        };
        let registration = Registration::create(root.path(), &endpoint).unwrap();
        let observed = Arc::new(AtomicUsize::new(0));
        let calls = observed.clone();
        let instance = endpoint.instance_id.clone();
        // Deliberately accept anything delivered: rejection must come from MCP.
        let app = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request: wire::WireRequest = wire::read_json(&mut stream).await.unwrap();
                calls.fetch_add(1, Ordering::SeqCst);
                let response = ApplicationResponse {
                    api_version: 1,
                    request_id: request.request.request_id,
                    instance_id: instance.clone(),
                    document_id: "document-a".into(),
                    revision: 4,
                    ok: true,
                    result: Some(json!({"ownerReached": true})),
                    error: None,
                };
                wire::write_json(&mut stream, &response).await.unwrap();
            }
        });
        // Abort the endpoint task even if an assertion or outer timeout fails.
        let _app_guard = EndpointTask(app);
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
        command.env("NEMO_MCP_REGISTRY", root.path()).kill_on_drop(true);
        let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
        let query = json!({"instance_id": endpoint.instance_id, "operation": "snapshot"});
        let control = client.call_tool(CallToolRequestParams::new("nemo_query")
            .with_arguments(query.as_object().unwrap().clone())).await.unwrap();
        assert_eq!(control.structured_content.unwrap()["result"]["ownerReached"], true);
        assert_eq!(observed.load(Ordering::SeqCst), 1, "positive endpoint control");

        let valid = json!({"apiVersion": 1, "requestId": "edit-a", "instanceId": endpoint.instance_id,
            "documentId": "document-a", "expectedRevision": 4, "operation": "property.set",
            "payload": {"layerId": "layer-a", "property": "opacity", "value": 25}});
        let mut cases: Vec<(&str, Value)> = Vec::new();
        for field in ["instanceId", "documentId", "expectedRevision"] {
            let mut input = valid.clone();
            input.as_object_mut().unwrap().remove(field);
            cases.push((field, input));
        }
        for (field, value) in [("apiVersion", json!(2)), ("requestId", json!("")),
            ("payload", json!(null)), ("operation", json!("snapshot"))] {
            let mut input = valid.clone();
            input[field] = value;
            cases.push((field, input));
        }
        for (label, input) in cases {
            let response = client.call_tool(CallToolRequestParams::new("nemo_command")
                .with_arguments(input.as_object().unwrap().clone())).await.unwrap();
            assert_eq!(response.is_error, Some(true), "{label}");
            assert_eq!(response.structured_content.unwrap()["error"]["code"], "invalid_request", "{label}");
            assert_eq!(observed.load(Ordering::SeqCst), 1, "{label} reached document owner");
        }
        let mutation_query = json!({"instance_id": endpoint.instance_id, "operation": "property.set",
            "payload": {"layerId": "layer-a", "property": "opacity", "value": 25}});
        let response = client.call_tool(CallToolRequestParams::new("nemo_query")
            .with_arguments(mutation_query.as_object().unwrap().clone())).await.unwrap();
        assert_eq!(response.is_error, Some(true));
        assert_eq!(response.structured_content.unwrap()["error"]["code"], "invalid_request");
        assert_eq!(observed.load(Ordering::SeqCst), 1, "query bypass reached document owner");
        client.cancel().await.unwrap();
        drop(registration);
        assert!(nemo_mcp::registry::read_endpoints(root.path()).unwrap().is_empty());
    }).await.expect("compiled MCP rejection check timed out");
}
