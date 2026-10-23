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
        "capabilities": {"buildId": endpoint.build_id},
        "nativeHostStatus": {"apiVersion": 2, "available": false,
            "instanceId": endpoint.instance_id,
            "reason": "endpoint does not advertise nativeApiVersion 2"}
    })
}

fn normalize_status_request_ids(discovery: &mut Value) {
    for instance in discovery["instances"].as_array_mut().unwrap() {
        let request_id = instance["nativeHostStatus"]
            .as_object_mut()
            .unwrap()
            .remove("requestId")
            .unwrap();
        assert!(uuid::Uuid::parse_str(request_id.as_str().unwrap()).is_ok());
    }
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
        let mut discovery = discovery.structured_content.unwrap();
        normalize_status_request_ids(&mut discovery);
        assert_eq!(discovery["apiVersion"], 1);
        assert!(discovery["registeredCapabilities"].is_array());
        assert_eq!(discovery["nativeApiVersion"], 2);
        let mut opacity_capabilities = discovery["registeredNativeCapabilities"]
            .as_array()
            .expect("native capability catalog must be an array")
            .iter()
            .filter(|capability| capability["id"] == "native.opacity");
        assert!(
            opacity_capabilities.next().is_some(),
            "native.opacity is advertised"
        );
        assert!(
            opacity_capabilities.next().is_none(),
            "native.opacity is unique"
        );
        assert_eq!(
            discovery["instances"],
            json!([
                discovered(&other, "other-document", 41),
                discovered(&initial, "initial-document", 7)
            ])
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
        let mut discovery = discovery.structured_content.unwrap();
        normalize_status_request_ids(&mut discovery);
        assert_eq!(discovery["apiVersion"], 1);
        assert!(discovery["registeredCapabilities"].is_array());
        assert_eq!(
            discovery["instances"],
            json!([discovered(&other, "other-document", 41)])
        );

        let _replacement_registration =
            Registration::create(registry.path(), &replacement).unwrap();
        let discovery = bounded(client.call_tool(CallToolRequestParams::new("nemo_discover")))
            .await
            .unwrap();
        let mut discovery = discovery.structured_content.unwrap();
        normalize_status_request_ids(&mut discovery);
        assert_eq!(discovery["apiVersion"], 1);
        assert!(discovery["registeredCapabilities"].is_array());
        assert_eq!(
            discovery["instances"],
            json!([
                discovered(&other, "other-document", 41),
                discovered(&replacement, "replacement-document", 23)
            ])
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

// Real TCP + compiled stdio regression for the native object owner's refusal of
// the legacy document API. The original v1-only reconnect fixture above is intact.
async fn native_only_discovery(legacy_silent: bool, native_reply: &'static str) -> Value {
    use tokio::io::AsyncReadExt;
    let registry = tempfile::tempdir().unwrap();
    let listener = bounded(TcpListener::bind("127.0.0.1:0")).await.unwrap();
    let endpoint = Endpoint {
        instance_id: SELECTED.into(),
        port: listener.local_addr().unwrap().port(),
        secret: uuid::Uuid::new_v4().to_string(),
        build_id: "native-only-build".into(),
    };
    let _registration = Registration::create(registry.path(), &endpoint).unwrap();
    let expected = endpoint.clone();
    let host = tokio::spawn(async move {
        let (mut legacy, _) = bounded(listener.accept()).await.unwrap();
        let message: wire::WireRequest = bounded(wire::read_json(&mut legacy)).await.unwrap();
        assert_eq!(message.secret, expected.secret);
        assert_eq!(message.request.operation, Operation::Capabilities);
        assert_eq!(message.request.api_version, 1);
        assert_eq!(message.request.instance_id.as_deref(), Some(SELECTED));
        assert_eq!(message.request.document_id, None);
        assert_eq!(message.request.expected_revision, None);
        assert_eq!(message.request.payload, json!({}));
        if !legacy_silent {
            bounded(wire::write_json(&mut legacy, &json!({
                "apiVersion":1,"requestId":message.request.request_id,
                "instanceId":SELECTED,"documentId":"legacy-decoy","revision":999,
                "ok":false,"error":{"code":"unavailable","message":"native object owns document"}
            }))).await.unwrap();
        }
        // Keep an unanswered legacy connection OPEN while native discovery is
        // attempted. Admission must survive the real legacy timeout, not EOF.
        let (mut native, _) = bounded(listener.accept()).await.unwrap();
        let native_message: wire::AuthenticatedWireRequest =
            bounded(wire::read_json(&mut native)).await.unwrap();
        let wire::AuthenticatedWireRequest::NativeStatus(native_message) = native_message else {
            panic!("discovery must use the existing native-status request");
        };
        assert_eq!(native_message.secret, expected.secret);
        let request = native_message.native_status;
        assert_eq!(request.api_version, 2);
        assert_eq!(request.instance_id, SELECTED);
        assert!(uuid::Uuid::parse_str(&request.request_id).is_ok());
        if native_reply == "silent" {
            let mut byte = [0u8; 1];
            assert_eq!(bounded(native.read(&mut byte)).await.unwrap(), 0);
        } else {
            let mut status = json!({"apiVersion":2,"requestId":request.request_id,
                "instanceId":SELECTED,"available":true,
                "documentId":"native-document","contentRevision":23,"lifecycleGeneration":37});
            match native_reply {
                "available" => {}
                "unavailable" => {
                    status["available"] = json!(false);
                    status
                        .as_object_mut()
                        .unwrap()
                        .remove("lifecycleGeneration");
                    status["reason"] = json!("native authority inactive");
                    // The closed status DTO permits optional pins when unavailable;
                    // discovery must still expose no active document/revision.
                    status["documentId"] = json!("inactive-decoy");
                    status["contentRevision"] = json!(777);
                }
                "wrong-request" => status["requestId"] = json!("foreign-request"),
                "wrong-instance" => status["instanceId"] = json!(OTHER),
                "missing-document" => {
                    status.as_object_mut().unwrap().remove("documentId");
                }
                "unknown-member" => status["injected"] = json!(true),
                "unsafe-revision" => status["contentRevision"] = json!(9_007_199_254_740_992u64),
                "missing-generation" => {
                    status
                        .as_object_mut()
                        .unwrap()
                        .remove("lifecycleGeneration");
                }
                "null-generation" => status["lifecycleGeneration"] = Value::Null,
                "zero-generation" => status["lifecycleGeneration"] = json!(0),
                "fractional-generation" => status["lifecycleGeneration"] = json!(1.5),
                "string-generation" => status["lifecycleGeneration"] = json!("37"),
                "unsafe-generation" => {
                    status["lifecycleGeneration"] = json!(9_007_199_254_740_992u64)
                }
                "unavailable-generation" => {
                    status["available"] = json!(false);
                    status["reason"] = json!("native authority inactive");
                }
                "oversized" => status["reason"] = json!("x".repeat(4096)),
                other => panic!("unknown fixture {other}"),
            }
            bounded(wire::write_json(&mut native, &status))
                .await
                .unwrap();
        }
        drop(legacy);
    });
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    command
        .env("NEMO_MCP_REGISTRY", registry.path())
        .kill_on_drop(true);
    let client = bounded(().serve(TokioChildProcess::new(command).unwrap()))
        .await
        .unwrap();
    let result = bounded(client.call_tool(CallToolRequestParams::new("nemo_discover")))
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
    let body = result.structured_content.unwrap();
    bounded(client.cancel()).await.unwrap();
    bounded(host)
        .await
        .expect("native discovery fixture must complete its two requests");
    body
}

#[tokio::test]
async fn executable_discovers_real_native_status_after_unanswered_or_refused_legacy_capabilities() {
    for silent in [true, false] {
        let mut body = native_only_discovery(silent, "available").await;
        normalize_status_request_ids(&mut body);
        assert_eq!(body["apiVersion"], 1);
        assert_eq!(body["nativeApiVersion"], 2);
        assert_eq!(
            body["instances"],
            json!([{
                "instanceId":SELECTED,"buildId":"native-only-build",
                "documentId":"native-document","revision":23,"capabilities":null,
                "nativeHostStatus":{"apiVersion":2,"instanceId":SELECTED,"available":true,
                    "documentId":"native-document","contentRevision":23,"lifecycleGeneration":37}
            }])
        );
        let legacy: Value =
            serde_json::to_value(nemo_mcp::capabilities::catalog().descriptors()).unwrap();
        let native: Value =
            serde_json::to_value(nemo_mcp::capabilities::native_catalog().descriptors()).unwrap();
        assert_eq!(body["registeredCapabilities"], legacy);
        assert_eq!(body["registeredNativeCapabilities"], native);
    }
}

#[tokio::test]
async fn executable_native_only_discovery_keeps_authenticated_unavailable_host_without_active_pins()
{
    let mut body = native_only_discovery(false, "unavailable").await;
    normalize_status_request_ids(&mut body);
    assert_eq!(
        body["instances"],
        json!([{
            "instanceId":SELECTED,"buildId":"native-only-build",
            "documentId":null,"revision":null,"capabilities":null,
            "nativeHostStatus":{"apiVersion":2,"instanceId":SELECTED,"available":false,
                "documentId":"inactive-decoy","contentRevision":777,"reason":"native authority inactive"}
        }])
    );
}

#[tokio::test]
async fn executable_native_only_discovery_rejects_uncorrelated_malformed_oversized_or_silent_status(
) {
    for reply in [
        "wrong-request",
        "wrong-instance",
        "missing-document",
        "unknown-member",
        "unsafe-revision",
        "missing-generation",
        "null-generation",
        "zero-generation",
        "fractional-generation",
        "string-generation",
        "unsafe-generation",
        "unavailable-generation",
        "oversized",
        "silent",
    ] {
        let body = native_only_discovery(false, reply).await;
        assert_eq!(
            body["instances"],
            json!([]),
            "{reply} cannot invent an authenticated owner"
        );
    }
}
