//! Reconnect and selected-instance routing through one compiled MCP process.
//! Packaged desktop and installed client acceptance remain separate gates.
use nemo_mcp::{
    contract::{ApplicationResponse, Operation},
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::{
    model::{CallToolRequestParams, CallToolResult},
    transport::TokioChildProcess,
    ServiceExt,
};
use serde_json::{json, Value};
use std::{future::Future, time::Duration};
use tokio::{net::TcpListener, task::JoinSet, time::timeout};

const DEADLINE: Duration = Duration::from_secs(5);
const SELECTED: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";
const OTHER: &str = "00000000-0000-4000-8000-000000000000";

async fn bounded<F: Future>(future: F) -> F::Output {
    timeout(DEADLINE, future)
        .await
        .expect("operation timed out")
}

async fn endpoint(
    servers: &mut JoinSet<()>,
    instance_id: &str,
    build_id: &str,
    document_id: &'static str,
    revision: u64,
    operations: Vec<Operation>,
) -> Endpoint {
    let listener = bounded(TcpListener::bind("127.0.0.1:0")).await.unwrap();
    let endpoint = Endpoint {
        instance_id: instance_id.into(),
        port: listener.local_addr().unwrap().port(),
        secret: uuid::Uuid::new_v4().to_string(),
        build_id: build_id.into(),
    };
    let expected = endpoint.clone();
    servers.spawn(async move {
        for operation in operations {
            let (mut stream, _) = bounded(listener.accept()).await.unwrap();
            let message: wire::WireRequest = bounded(wire::read_json(&mut stream)).await.unwrap();
            assert_eq!(message.secret, expected.secret);
            assert_eq!(
                message.request.instance_id.as_deref(),
                Some(expected.instance_id.as_str())
            );
            assert_eq!(message.request.operation, operation);
            assert_eq!(message.request.api_version, 1);
            assert_eq!(message.request.document_id, None);
            assert_eq!(message.request.expected_revision, None);
            assert_eq!(message.request.payload, json!({}));
            assert!(uuid::Uuid::parse_str(&message.request.request_id).is_ok());
            let response = ApplicationResponse {
                api_version: 1,
                request_id: message.request.request_id,
                instance_id: expected.instance_id.clone(),
                document_id: document_id.into(),
                revision,
                ok: true,
                result: Some(json!({"buildId": expected.build_id})),
                error: None,
            };
            bounded(wire::write_json(&mut stream, &response))
                .await
                .unwrap();
        }
    });
    endpoint
}

fn discovered(endpoint: &Endpoint, document_id: &str, revision: u64) -> Value {
    json!({
        "instanceId": endpoint.instance_id,
        "buildId": endpoint.build_id,
        "documentId": document_id,
        "revision": revision,
        "capabilities": {"buildId": endpoint.build_id}
    })
}

fn query(instance_id: &str) -> CallToolRequestParams {
    CallToolRequestParams::new("nemo_query").with_arguments(
        json!({"instance_id": instance_id, "operation": "snapshot"})
            .as_object()
            .unwrap()
            .clone(),
    )
}

fn assert_snapshot(result: CallToolResult, endpoint: &Endpoint, document_id: &str, revision: u64) {
    assert_ne!(result.is_error, Some(true));
    let response: ApplicationResponse =
        serde_json::from_value(result.structured_content.unwrap()).unwrap();
    assert!(response.ok);
    assert_eq!(response.api_version, 1);
    assert!(uuid::Uuid::parse_str(&response.request_id).is_ok());
    assert_eq!(response.instance_id, endpoint.instance_id);
    assert_eq!(response.document_id, document_id);
    assert_eq!(response.revision, revision);
    assert_eq!(response.result, Some(json!({"buildId": endpoint.build_id})));
    assert!(response.error.is_none());
}

#[tokio::test]
async fn executable_rediscovers_replaced_registration_without_restarting() {
    use Operation::{Capabilities, Snapshot};
    let registry = tempfile::tempdir().unwrap();
    let mut servers = JoinSet::new();
    let initial = endpoint(
        &mut servers,
        SELECTED,
        "initial-build",
        "initial-document",
        7,
        vec![Capabilities, Snapshot],
    )
    .await;
    // Bind both generations before serving requests so their ports cannot be reused.
    let replacement = endpoint(
        &mut servers,
        SELECTED,
        "replacement-build",
        "replacement-document",
        23,
        vec![Capabilities, Snapshot],
    )
    .await;
    let other = endpoint(
        &mut servers,
        OTHER,
        "other-build",
        "other-document",
        41,
        vec![Capabilities, Capabilities, Capabilities, Snapshot],
    )
    .await;
    assert_ne!(initial.port, replacement.port);
    assert_ne!(initial.secret, replacement.secret);

    // A separate task lets assertion failures reach cleanup before being rethrown.
    let mut scenario = tokio::spawn(async move {
        let initial_registration = Registration::create(registry.path(), &initial).unwrap();
        let _other_registration = Registration::create(registry.path(), &other).unwrap();
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
        command
            .env("NEMO_MCP_REGISTRY", registry.path())
            .kill_on_drop(true);
        let transport = TokioChildProcess::new(command).unwrap();
        let client = bounded(().serve(transport)).await.unwrap();

        let discovery = bounded(client.call_tool(CallToolRequestParams::new("nemo_discover")))
            .await
            .unwrap();
        assert_eq!(
            discovery.structured_content,
            Some(json!({"apiVersion": 1, "instances": [
                discovered(&other, "other-document", 41), discovered(&initial, "initial-document", 7)
            ]}))
        );
        assert_snapshot(
            bounded(client.call_tool(query(SELECTED))).await.unwrap(),
            &initial,
            "initial-document",
            7,
        );

        drop(initial_registration);
        let unavailable = bounded(client.call_tool(query(SELECTED))).await.unwrap();
        assert_eq!(unavailable.is_error, Some(true));
        assert_eq!(
            unavailable.structured_content.unwrap()["error"]["code"],
            "unavailable"
        );
        let discovery = bounded(client.call_tool(CallToolRequestParams::new("nemo_discover")))
            .await
            .unwrap();
        assert_eq!(
            discovery.structured_content,
            Some(json!({"apiVersion": 1, "instances": [
                discovered(&other, "other-document", 41)
            ]}))
        );

        let _replacement_registration =
            Registration::create(registry.path(), &replacement).unwrap();
        let discovery = bounded(client.call_tool(CallToolRequestParams::new("nemo_discover")))
            .await
            .unwrap();
        assert_eq!(
            discovery.structured_content,
            Some(json!({"apiVersion": 1, "instances": [
                discovered(&other, "other-document", 41), discovered(&replacement, "replacement-document", 23)
            ]}))
        );
        assert_snapshot(
            bounded(client.call_tool(query(SELECTED))).await.unwrap(),
            &replacement,
            "replacement-document",
            23,
        );
        assert_snapshot(
            bounded(client.call_tool(query(OTHER))).await.unwrap(),
            &other,
            "other-document",
            41,
        );
        bounded(client.cancel()).await.unwrap();
    });

    let result = timeout(Duration::from_secs(30), &mut scenario).await;
    if result.is_err() {
        scenario.abort();
        let _ = bounded(&mut scenario).await;
    }
    if !matches!(&result, Ok(Ok(()))) {
        servers.abort_all();
    }
    let mut server_errors = Vec::new();
    while !servers.is_empty() {
        match timeout(DEADLINE, servers.join_next()).await {
            Ok(Some(Err(error))) if !error.is_cancelled() => server_errors.push(error.to_string()),
            Ok(_) => {}
            Err(_) => {
                servers.abort_all();
                server_errors.push("endpoint did not finish its expected requests".into());
            }
        }
    }
    assert!(server_errors.is_empty(), "{server_errors:?}");
    result
        .expect("reconnect scenario timed out")
        .expect("reconnect scenario failed");
}
