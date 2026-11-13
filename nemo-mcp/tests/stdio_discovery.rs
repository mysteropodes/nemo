//! Compact discovery through one compiled MCP process against a stub host that
//! reports an open object document. Size limits are the contract under test.
use nemo_mcp::{
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
use tokio::{net::TcpListener, time::timeout};

const INSTANCE: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";
const DOCUMENT: &str = "native-object-document-1";

async fn bounded<F: Future>(future: F) -> F::Output {
    timeout(Duration::from_secs(10), future)
        .await
        .expect("operation timed out")
}

async fn host(listener: TcpListener, secret: String, kind: &'static str) {
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(message) = wire::read_json::<wire::AuthenticatedWireRequest>(&mut stream).await
        else {
            continue;
        };
        match message {
            wire::AuthenticatedWireRequest::Legacy(message) => {
                assert_eq!(message.secret, secret);
                let operations: Vec<&str> = if kind == "object" {
                    vec!["capabilities", "snapshot"]
                } else {
                    vec![]
                };
                wire::write_json(&mut stream, &json!({"apiVersion": 1, "requestId": message.request.request_id,
                    "instanceId": INSTANCE, "documentId": DOCUMENT, "revision": 3, "ok": true,
                    "result": {"operations": operations, "nativeApiVersion": 2, "properties": [], "descriptors": []}}))
                .await
                .unwrap();
            }
            wire::AuthenticatedWireRequest::NativeStatus(message) => {
                assert_eq!(message.secret, secret);
                wire::write_json(&mut stream, &json!({"apiVersion": 2, "requestId": message.native_status.request_id,
                    "instanceId": INSTANCE, "available": true, "documentId": DOCUMENT, "contentRevision": 3,
                    "lifecycleGeneration": 1, "documentKind": kind, "pid": 4242}))
                .await
                .unwrap();
            }
            wire::AuthenticatedWireRequest::Native(_) => {
                panic!("discovery sends no native request")
            }
        }
    }
}

fn arguments(call: Value) -> CallToolRequestParams {
    CallToolRequestParams::new("nemo_discover").with_arguments(call.as_object().unwrap().clone())
}

fn text_len(result: &CallToolResult) -> usize {
    serde_json::to_string(&result.content).unwrap().len()
}

#[tokio::test]
async fn default_answer_is_compact_and_the_rest_is_on_demand() {
    let registry = tempfile::tempdir().unwrap();
    let listener = bounded(TcpListener::bind("127.0.0.1:0")).await.unwrap();
    let endpoint = Endpoint {
        instance_id: INSTANCE.into(),
        port: listener.local_addr().unwrap().port(),
        secret: uuid::Uuid::new_v4().to_string(),
        build_id: "0.7.0-alpha.4:0123456789abcdef".into(),
    };
    let _registration = Registration::create(registry.path(), &endpoint).unwrap();
    let server = tokio::spawn(host(listener, endpoint.secret.clone(), "object"));
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    command
        .env("NEMO_MCP_REGISTRY", registry.path())
        .kill_on_drop(true);
    let client = bounded(().serve(TokioChildProcess::new(command).unwrap()))
        .await
        .unwrap();

    // The tool descriptions tell the workflow in a few plain sentences.
    let tools = bounded(client.list_all_tools()).await.unwrap();
    for tool in &tools {
        let description = tool.description.as_deref().unwrap_or_default();
        if tool.name == "nemo_discover" {
            let properties = tool
                .input_schema
                .get("properties")
                .and_then(Value::as_object)
                .expect("discover arguments are declared");
            for argument in [
                "detail",
                "operation",
                "family",
                "page",
                "apiVersion",
                "instanceId",
            ] {
                assert!(properties.contains_key(argument), "{argument}");
            }
            assert!(
                tool.input_schema.get("required").is_none(),
                "every discover argument is optional"
            );
            assert!(
                description.starts_with("Start here") && description.contains("start-here guide")
            );
        }
        if tool.name == "nemo_command" {
            assert!(
                description.contains("call nemo_discover first")
                    && description.contains("expectedRevision")
            );
        }
    }

    let default = bounded(client.call_tool(CallToolRequestParams::new("nemo_discover")))
        .await
        .unwrap();
    assert!(
        text_len(&default) <= 12_000,
        "default answer is {} characters",
        text_len(&default)
    );
    let body = default.structured_content.unwrap();
    let instance = &body["instances"][0];
    assert_eq!(body["instances"].as_array().unwrap().len(), 1);
    for (field, expected) in [
        ("instanceId", json!(INSTANCE)),
        ("pid", json!(4242)),
        ("documentId", json!(DOCUMENT)),
        ("documentKind", json!("object")),
        ("revision", json!(3)),
        ("buildId", json!(endpoint.build_id)),
    ] {
        assert_eq!(instance[field], expected, "{field}");
    }
    assert!(body.get("registeredNativeCapabilities").is_none());
    let steps: Vec<&str> = body["startHere"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    for needed in [
        "snapshot",
        "pin",
        "read-property",
        "edit-property",
        "undo",
        "redo",
        "history",
        "save",
        "open",
        "export",
    ] {
        assert!(steps.contains(&needed), "{needed}");
    }
    let edit = body["startHere"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "edit-property")
        .unwrap();
    assert_eq!(edit["request"]["expectedRevision"], 3);
    assert_eq!(edit["request"]["instanceId"], INSTANCE);

    let one = bounded(client.call_tool(arguments(
        json!({"operation": "command.document.object.property.edit"}),
    )))
    .await
    .unwrap();
    let one = one.structured_content.unwrap();
    assert_eq!(one["availability"]["state"], "available");
    assert!(
        one["requestSchema"].is_object()
            && one["example"].is_object()
            && one["refusalCodes"].is_array()
    );

    let refused =
        bounded(client.call_tool(arguments(json!({"operation": "command.document.apply"}))))
            .await
            .unwrap();
    let refused = refused.structured_content.unwrap();
    assert_eq!(refused["availability"]["state"], "not-for-open-document");
    assert_eq!(
        refused["availability"]["useInstead"],
        "command.document.object.property.edit"
    );

    let page = bounded(client.call_tool(arguments(json!({"detail": "operations", "page": 2}))))
        .await
        .unwrap();
    assert!(text_len(&page) <= 20_000, "{}", text_len(&page));
    assert_eq!(page.structured_content.unwrap()["page"], 2);
    let family = bounded(client.call_tool(arguments(
        json!({"detail": "operations", "family": "history"}),
    )))
    .await
    .unwrap();
    let family = family.structured_content.unwrap();
    assert!(family["operations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["family"] == "history"));

    let bad = bounded(client.call_tool(arguments(json!({"operation": "no.such.operation"}))))
        .await
        .unwrap();
    assert_eq!(bad.is_error, Some(true));
    assert_eq!(
        bad.structured_content.unwrap()["error"]["code"],
        "invalid_request"
    );

    let full = bounded(client.call_tool(arguments(json!({"detail": "full"}))))
        .await
        .unwrap();
    let full = full.structured_content.unwrap();
    assert!(
        full["registeredNativeCapabilities"]
            .as_array()
            .unwrap()
            .len()
            > 40
    );
    assert_eq!(full["instances"][0]["documentId"], DOCUMENT);

    bounded(client.cancel()).await.unwrap();
    server.abort();
}
