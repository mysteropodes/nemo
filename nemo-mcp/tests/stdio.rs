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
async fn executable_advertises_constructible_command_and_reports_no_running_app() {
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
    let native_metadata: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-property-metadata-v1.schema.json"
    ))
    .unwrap();
    assert_eq!(command_schema["$defs"]["nativePropertyMetadata"], native_metadata);
    assert_eq!(command_schema["type"], "object");
    for keyword in ["allOf", "oneOf", "anyOf", "$ref"] {
        assert!(
            command_schema.get(keyword).is_none(),
            "root has no {keyword}"
        );
    }
    assert_eq!(
        command_schema["if"],
        json!({"required": ["apiVersion"], "properties": {"apiVersion": {"const": 1}}})
    );
    assert_eq!(command_schema["then"], json!({"$ref": "#/$defs/legacy"}));
    assert_eq!(command_schema["else"], json!({"$ref": "#/$defs/native"}));
    assert_eq!(
        command_schema["properties"]["apiVersion"]["enum"],
        json!([1, 2])
    );
    for field in [
        "requestId",
        "instanceId",
        "documentId",
        "expectedRevision",
        "operation",
        "payload",
    ] {
        assert!(
            command_schema["properties"][field].is_object(),
            "{field} is directly advertised"
        );
    }
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
    let metadata_query = json!({
        "apiVersion": 2, "requestId": "metadata-page-0", "instanceId": "instance-a",
        "documentId": "native-object-document-1",
        "operation": "query.document.object.property.metadata",
        "payload": {"atRevision": 0, "documentSnapshotId": "native-object:native-object-document-1:0",
                    "view": {"kind": "targets", "offset": 0, "limit": 2}}
    });
    assert!(validator.is_valid(&metadata_query));
    let mut revision_on_query = metadata_query.clone();
    revision_on_query["expectedRevision"] = json!(0);
    assert!(!validator.is_valid(&revision_on_query));
    for view in [json!({"kind": "targets", "offset": 0}), json!({"kind": "unsupported"})] {
        let mut invalid = metadata_query.clone();
        invalid["payload"]["view"] = view;
        assert!(!validator.is_valid(&invalid), "metadata selector rejects {invalid}");
    }
    for example in command_schema["examples"].as_array().unwrap() {
        assert!(
            validator.is_valid(example),
            "copyable native example: {example}"
        );
        let mut malformed = example.clone();
        malformed["payload"] = json!("{}");
        assert!(!validator.is_valid(&malformed));
        let mut missing_revision = example.clone();
        missing_revision
            .as_object_mut()
            .unwrap()
            .remove("expectedRevision");
        assert!(!validator.is_valid(&missing_revision));
        let mut unsupported = example.clone();
        unsupported["operation"] = json!("command.not_registered");
        assert!(!validator.is_valid(&unsupported));
    }
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
    for (field, value) in [
        ("operation", json!("property.not_registered")),
        ("payload", json!("{}")),
        ("unexpectedField", json!(true)),
    ] {
        let mut invalid = legacy_request.clone();
        invalid[field] = value;
        assert!(!validator.is_valid(&invalid), "legacy rejects {field}");
    }
    let native_apply = &command_schema["examples"][0];
    for payload in [
        json!({"command": "layer.opacity.set", "stableTarget": {"layerId": "wrong-key"}, "value": 25}),
        json!({"command": "layer.opacity.set", "stableTarget": {"layerUid": "layer-a"}, "value": 101}),
        json!({"command": "layer.opacity.set", "stableTarget": {"layerUid": "layer-a"}, "value": 25, "extra": true}),
    ] {
        let mut invalid = native_apply.clone();
        invalid["payload"] = payload;
        assert!(
            !validator.is_valid(&invalid),
            "native rejects malformed body"
        );
    }

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

    // Compare the conditional advertisement with the previously accepted exact
    // union, including cross-version bodies and missing/mistyped envelope fields.
    let mut previous = command_schema.clone();
    for keyword in ["if", "then", "else"] {
        previous.as_object_mut().unwrap().remove(keyword);
    }
    previous["allOf"] =
        json!([{"oneOf": [{"$ref": "#/$defs/legacy"}, {"$ref": "#/$defs/native"}]}]);
    let previous_validator = jsonschema::draft202012::new(&previous).unwrap();
    let mut corpus = command_schema["examples"].as_array().unwrap().clone();
    corpus.extend([legacy_request, native_response]);
    for request in corpus {
        assert_eq!(
            validator.is_valid(&request),
            previous_validator.is_valid(&request)
        );
        for field in [
            "apiVersion",
            "requestId",
            "instanceId",
            "documentId",
            "expectedRevision",
            "operation",
            "payload",
            "cancelledBeforeDispatch",
            "unexpectedField",
        ] {
            let mut missing = request.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert_eq!(
                validator.is_valid(&missing),
                previous_validator.is_valid(&missing),
                "missing {field}: {missing}"
            );
            for value in [
                Value::Null,
                json!(-1),
                json!(1),
                json!(2),
                json!(3),
                json!("2"),
                json!({}),
                json!([]),
            ] {
                let mut changed = request.clone();
                changed[field] = value;
                assert_eq!(
                    validator.is_valid(&changed),
                    previous_validator.is_valid(&changed),
                    "changed {field}: {changed}"
                );
            }
        }
    }

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
    let native = registered_native_capabilities();
    let object = native
        .as_array()
        .unwrap()
        .iter()
        .find(|descriptor| descriptor["id"] == "native.object")
        .unwrap();
    assert_eq!(
        object["operations"],
        json!([
            "query.document.object",
            "command.document.object.layer.visibility.set"
        ])
    );
    let visibility = json!({
        "apiVersion": 2, "requestId": "visibility-stdio-7",
        "instanceId": "instance-A", "documentId": "document-A",
        "expectedRevision": 7,
        "operation": "command.document.object.layer.visibility.set",
        "payload": {
            "command": "object.layer.visibility.set",
            "layerUid": "legacy layer:alpha",
            "visible": false
        }
    });
    assert!(
        validator.is_valid(&visibility),
        "the advertised v2 schema accepts its declared public command"
    );
    let missing_owner = client
        .call_tool(
            CallToolRequestParams::new("nemo_command")
                .with_arguments(visibility.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(missing_owner.is_error, Some(true));
    let error = missing_owner.structured_content.unwrap()["error"].clone();
    assert_eq!(error["code"], "unavailable");
    assert_eq!(
        error["message"],
        "Selected instance is absent; discover again after reconnect"
    );
    assert_eq!(object["availability"]["state"], "available");
    for invalid in [
        {
            let mut value = visibility.clone();
            value.as_object_mut().unwrap().remove("expectedRevision");
            value
        },
        {
            let mut value = visibility.clone();
            value["payload"]["visible"] = json!(0);
            value
        },
        {
            let mut value = visibility.clone();
            value["payload"]["extra"] = json!(true);
            value
        },
        {
            let mut value = visibility.clone();
            value["payload"]["layerUid"] = json!("");
            value
        },
    ] {
        assert!(!validator.is_valid(&invalid));
        let rejected = client
            .call_tool(
                CallToolRequestParams::new("nemo_command")
                    .with_arguments(invalid.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(rejected.is_error, Some(true));
        assert_eq!(
            rejected.structured_content.unwrap()["error"]["code"],
            "invalid_request"
        );
    }
    client.cancel().await.unwrap();
}


#[test]
fn schema_binary_rejects_unsupported_arguments_without_partial_contract_output() {
    for arguments in [
        vec!["--api-version", "3"],
        vec!["--api-version"],
        vec!["--api-version", "2", "extra"],
        vec!["--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_nemo-mcp-schema"))
            .args(&arguments)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            output.stdout.is_empty(),
            "no partial schema for {arguments:?}"
        );
        assert_eq!(output.stderr, b"usage: nemo-mcp-schema [--api-version 2]\n");
    }
}
