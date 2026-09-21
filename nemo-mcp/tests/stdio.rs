//! Protocol regression against the compiled executable, with isolated discovery.
//! Installed Codex/Claude acceptance is a separate gate.
use nemo_mcp::contract::ApplicationRequest;
use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::{json, Value};
use std::process::Command;

/// Expected set, READ from the crate's declaration rather than copied.
/// A copied list drifts silently: #1310 added one descriptor and four
/// hand-maintained copies failed at once, in a crate no job ran (#1318).
/// The assertion is unchanged — the spawned executable must advertise exactly
/// what the crate declares — but there is no longer a second list to forget.
fn registered_capabilities() -> Value {
    Value::Array(
        nemo_mcp::capabilities::CAPABILITY_SOURCES
            .iter()
            .map(|source| serde_json::from_str::<Value>(source).unwrap())
            .collect(),
    )
}

fn registered_native_capabilities() -> Value {
    Value::Array(
        nemo_mcp::capabilities::NATIVE_CAPABILITY_SOURCES
            .iter()
            .map(|source| serde_json::from_str::<Value>(source).unwrap())
            .collect(),
    )
}

#[test]
fn schema_binary_preserves_v1_bytes_and_prints_the_accepted_v2_contract() {
    let v1 = Command::new(env!("CARGO_BIN_EXE_nemo-mcp-schema"))
        .output()
        .unwrap();
    assert!(v1.status.success());
    assert_eq!(
        v1.stdout,
        include_bytes!("../../engineering/application/transport-v1.schema.json")
    );

    let v2 = Command::new(env!("CARGO_BIN_EXE_nemo-mcp-schema"))
        .args(["--api-version", "2"])
        .output()
        .unwrap();
    assert!(v2.status.success());
    assert_eq!(
        v2.stdout,
        include_bytes!("../../engineering/application/native-transport-v2.schema.json")
    );
}

#[tokio::test]
async fn executable_advertises_valid_command_union_and_reports_no_running_app() {
    let root = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    command.env("NEMO_MCP_REGISTRY", root.path());
    let transport = TokioChildProcess::new(command).unwrap();
    let client = ().serve(transport).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    let command_schema = serde_json::to_value(
        tools
            .iter()
            .find(|tool| tool.name == "nemo_command")
            .unwrap()
            .input_schema
            .as_ref(),
    )
    .unwrap();
    let mut expected_legacy =
        serde_json::to_value(schemars::schema_for!(ApplicationRequest)).unwrap();
    let legacy_object = expected_legacy.as_object_mut().unwrap();
    legacy_object.remove("title");
    legacy_object.insert("$id".into(), json!("urn:nemo:mcp:command-request:v1"));
    legacy_object.insert(
        "allOf".into(),
        json!([{
            "type": "object",
            "required": ["apiVersion"],
            "properties": {"apiVersion": {"const": 1}}
        }]),
    );
    assert_eq!(command_schema["$defs"]["legacy"], expected_legacy);

    let native_transport = serde_json::from_str::<Value>(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let mut expected_native = native_transport.clone();
    let native_object = expected_native.as_object_mut().unwrap();
    native_object.remove("oneOf");
    native_object.insert("$ref".into(), json!("#/$defs/Request"));
    assert_eq!(command_schema["$defs"]["native"], expected_native);
    assert_eq!(
        command_schema["oneOf"],
        json!([
            {"$ref": "#/$defs/legacy"},
            {"$ref": "#/$defs/native"}
        ])
    );
    assert_eq!(command_schema["x-nemo-nativeApiVersion"], 2);
    assert_eq!(command_schema["x-nemo-nativeTransportV2"], native_transport);
    assert_eq!(
        command_schema["x-nemo-registeredNativeCapabilities"],
        registered_native_capabilities()
    );
    jsonschema::draft202012::meta::validate(&command_schema)
        .expect("advertised command schema is valid Draft 2020-12");
    let validator = jsonschema::draft202012::new(&command_schema)
        .expect("advertised command schema compiles with a standard validator");
    let legacy_request = json!({
        "apiVersion": 1,
        "requestId": "fixture-v1",
        "instanceId": "instance-a",
        "documentId": "document-a",
        "expectedRevision": 0,
        "operation": "property.set",
        "payload": {"layerId": "layer-a", "property": "opacity", "value": 25}
    });
    assert!(validator.is_valid(&legacy_request));

    let mut native_requests = native_transport["$defs"]["Request"]["examples"]
        .as_array()
        .unwrap()
        .clone();
    for source in nemo_mcp::capabilities::NATIVE_CAPABILITY_SOURCES {
        let descriptor: Value = serde_json::from_str(source).unwrap();
        native_requests.extend(
            descriptor["examples"]
                .as_array()
                .unwrap()
                .iter()
                .map(|example| example["request"].clone()),
        );
    }
    for request in native_requests {
        assert!(
            validator.is_valid(&request),
            "valid native request: {request}"
        );
        let mut wrong_version = request.clone();
        wrong_version["apiVersion"] = json!(3);
        assert!(!validator.is_valid(&wrong_version));
        let mut unknown_field = request;
        unknown_field
            .as_object_mut()
            .unwrap()
            .insert("unexpectedNativeField".into(), json!(true));
        assert!(!validator.is_valid(&unknown_field));
    }
    let mut wrong_legacy_version = legacy_request.clone();
    wrong_legacy_version["apiVersion"] = json!(3);
    assert!(!validator.is_valid(&wrong_legacy_version));
    let native_response = native_transport["$defs"]["Response"]["examples"][0].clone();
    assert!(!validator.is_valid(&native_response));

    let mut tools = tools
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    tools.sort();
    assert_eq!(tools, ["nemo_command", "nemo_discover", "nemo_query"]);
    let discover = client
        .call_tool(CallToolRequestParams::new("nemo_discover"))
        .await
        .unwrap();
    assert_eq!(
        discover.structured_content,
        Some(json!({
            "apiVersion": 1,
            "registeredCapabilities": registered_capabilities(),
            "nativeApiVersion": 2,
            "registeredNativeCapabilities": registered_native_capabilities(),
            "instances": []
        }))
    );
    let unavailable = client
        .call_tool(
            CallToolRequestParams::new("nemo_query").with_arguments(
                json!({"instance_id":"missing", "operation":"snapshot"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(unavailable.is_error, Some(true));
    assert_eq!(
        unavailable.structured_content.unwrap()["error"]["code"],
        "unavailable"
    );
    client.cancel().await.unwrap();
}
