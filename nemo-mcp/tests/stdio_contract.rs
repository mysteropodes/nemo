//! The payload contract a client actually reads, checked against the compiled
//! executable. Both installed clients failed their first edit against a `payload`
//! advertised as `true` ("any value validates"): one sent a JSON-encoded string,
//! the other guessed `propertyId`. Neither mistake is possible to diagnose from a
//! schema that types nothing, so the schema and the rejection are asserted here.
use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::{json, Map, Value};

async fn client() -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    command
        .env("NEMO_MCP_REGISTRY", root.path())
        .kill_on_drop(true);
    ().serve(TokioChildProcess::new(command).unwrap())
        .await
        .unwrap()
}

fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[tokio::test]
async fn advertised_payload_schema_names_every_key_and_operation_template() {
    let client = client().await;
    let tools = client.list_all_tools().await.unwrap();
    let schema = |name: &str| {
        serde_json::to_value(
            tools
                .iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("{name} is advertised"))
                .input_schema
                .as_ref(),
        )
        .unwrap()
    };

    let command = schema("nemo_command");
    let payload = &command["properties"]["payload"];
    // The defect verbatim: schemars renders `serde_json::Value` as `true`.
    assert_ne!(payload, &json!(true), "payload must carry a real schema");
    assert_eq!(payload["type"], "object");
    assert_eq!(payload["additionalProperties"], false);
    for key in [
        "layerId", "property", "value", "frame", "animated", "request",
    ] {
        assert!(
            payload["properties"][key].is_object(),
            "payload advertises {key}"
        );
    }
    // Each key is optional because requiredness is per operation, so schemars types
    // it as a union with null; what matters is that the primitive is named.
    for (key, primitive) in [
        ("layerId", "string"),
        ("property", "string"),
        ("value", "number"),
        ("frame", "integer"),
        ("animated", "boolean"),
    ] {
        let advertised = &payload["properties"][key]["type"];
        assert!(
            advertised == primitive
                || advertised
                    .as_array()
                    .is_some_and(|types| types.iter().any(|entry| entry == primitive)),
            "{key} is typed {advertised}"
        );
    }

    let description = payload["description"].as_str().unwrap();
    for operation in [
        "property.set",
        "property.key.set",
        "property.key.remove",
        "property.animation.set",
        "history.undo",
        "history.redo",
        "diagnostics.replay",
    ] {
        assert!(description.contains(operation), "{operation} is templated");
    }
    // Capabilities names the property `id`; the payload key is `property`. The one
    // client that recovered lost an attempt to exactly this mismatch.
    assert!(description.contains("capabilities names it `id`"));
    assert!(description.contains("never a JSON-encoded string"));
    let examples = payload["examples"].as_array().unwrap();
    assert_eq!(examples.len(), 7, "one example per write operation");
    assert_eq!(examples[0]["property"], "opacity");

    // Both tools spell instance identity the same way.
    let query = schema("nemo_query");
    assert!(query["properties"]["instanceId"].is_object());
    assert!(query["properties"]["instance_id"].is_null());
    assert_eq!(query["properties"]["payload"]["type"], "object");
    assert!(query["properties"]["payload"]["description"]
        .as_str()
        .unwrap()
        .contains("property.get"));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn rejected_payloads_name_the_shape_the_operation_expects() {
    let client = client().await;
    let base = json!({"apiVersion": 1, "requestId": "contract-1",
        "instanceId": "3f1a5f3c-4a05-4a3f-9d59-2f9f1c65b3ad", "documentId": "document-a",
        "expectedRevision": 4, "operation": "property.set"});
    let with_payload = |payload: Value| {
        let mut input = base.clone();
        input["payload"] = payload;
        arguments(input)
    };

    let cases: Vec<(&str, Value, Vec<&str>)> = vec![
        (
            "stringified",
            json!("{\"layerId\":\"layer-a\",\"property\":\"opacity\",\"value\":37}"),
            vec!["JSON-encoded string", "property.set expects", "layerId"],
        ),
        (
            "misspelled key",
            json!({"propertyId": "opacity", "layerId": "layer-a", "value": 37}),
            vec!["propertyId", "property.set expects"],
        ),
        (
            "missing value",
            json!({"layerId": "layer-a", "property": "opacity"}),
            vec!["missing value", "property.set expects"],
        ),
        (
            "curve editing",
            json!({"layerId": "layer-a", "property": "opacity", "value": 37,
                "curvePoints": [[0, 0], [1, 1]]}),
            vec!["curvePoints is not exposed"],
        ),
    ];
    for (label, payload, fragments) in cases {
        let response = client
            .call_tool(
                CallToolRequestParams::new("nemo_command").with_arguments(with_payload(payload)),
            )
            .await
            .unwrap();
        assert_eq!(response.is_error, Some(true), "{label}");
        let body = response.structured_content.unwrap();
        assert_eq!(body["error"]["code"], "invalid_request", "{label}");
        let message = body["error"]["message"].as_str().unwrap();
        for fragment in fragments {
            assert!(message.contains(fragment), "{label}: {message}");
        }
    }

    // A payload the schema does describe passes validation and fails later, on the
    // absent instance — proving the rejections above are the contract, not the app.
    let response = client
        .call_tool(
            CallToolRequestParams::new("nemo_command").with_arguments(with_payload(
                json!({"layerId": "layer-a", "property": "opacity", "value": 37}),
            )),
        )
        .await
        .unwrap();
    assert_eq!(
        response.structured_content.unwrap()["error"]["code"],
        "unavailable"
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn replay_payload_validates_the_nested_recorded_command() {
    let client = client().await;
    let base = json!({"apiVersion": 1, "requestId": "replay-1",
        "instanceId": "3f1a5f3c-4a05-4a3f-9d59-2f9f1c65b3ad", "documentId": "document-a",
        "expectedRevision": 4, "operation": "diagnostics.replay"});

    // The recorded command is missing its own required keys — CommandPayload
    // deserializes it fine (every field is optional there), so only a recursive
    // check catches this, not the outer "request is present" check.
    let mut incomplete = base.clone();
    incomplete["payload"] = json!({"request": {"operation": "property.set",
        "payload": {"property": "opacity"}}});
    let response = client
        .call_tool(CallToolRequestParams::new("nemo_command").with_arguments(arguments(incomplete)))
        .await
        .unwrap();
    assert_eq!(response.is_error, Some(true));
    let body = response.structured_content.unwrap();
    assert_eq!(body["error"]["code"], "invalid_request");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("recorded request payload is invalid"),
        "{message}"
    );
    assert!(message.contains("missing layerId, value"), "{message}");
    assert!(message.contains("property.set expects"), "{message}");

    // A fully-formed recorded command still passes and fails later on the absent
    // instance, proving the recursive check isn't a false positive.
    let mut complete = base.clone();
    complete["payload"] = json!({"request": {"operation": "property.set",
        "payload": {"layerId": "layer-a", "property": "opacity", "value": 37}}});
    let response = client
        .call_tool(CallToolRequestParams::new("nemo_command").with_arguments(arguments(complete)))
        .await
        .unwrap();
    assert_eq!(
        response.structured_content.unwrap()["error"]["code"],
        "unavailable"
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn reads_keep_their_own_templates_and_identity_spelling() {
    let client = client().await;
    let missing_target = client
        .call_tool(
            CallToolRequestParams::new("nemo_query").with_arguments(arguments(
                json!({"instanceId": "a68d0f2d-6b4f-4f5b-8a4b-6b8f0f2a4d61",
                "operation": "property.get", "payload": {}}),
            )),
        )
        .await
        .unwrap();
    let body = missing_target.structured_content.unwrap();
    assert_eq!(body["error"]["code"], "invalid_request");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("missing layerId, property"), "{message}");
    assert!(message.contains("property.get expects"), "{message}");

    // The pre-camelCase spelling still deserializes, so an existing caller is kept.
    let legacy = client
        .call_tool(CallToolRequestParams::new("nemo_query").with_arguments(arguments(
            json!({"instance_id": "a68d0f2d-6b4f-4f5b-8a4b-6b8f0f2a4d61", "operation": "snapshot"}),
        )))
        .await
        .unwrap();
    assert_eq!(
        legacy.structured_content.unwrap()["error"]["code"],
        "unavailable"
    );

    // A misspelled key is rejected rather than silently dropped, same guard
    // nemo_command's ApplicationRequest already has. This fails during parameter
    // deserialization itself (rmcp's own path, before our error mapping runs), so
    // the message lands in `content`, not the app's usual `structured_content`.
    let typo = client
        .call_tool(CallToolRequestParams::new("nemo_query").with_arguments(arguments(
            json!({"instanceId": "a68d0f2d-6b4f-4f5b-8a4b-6b8f0f2a4d61", "operatoin": "snapshot"}),
        )))
        .await
        .unwrap();
    assert_eq!(typo.is_error, Some(true), "{typo:?}");
    let text = typo.content[0].as_text().unwrap().text.as_str();
    assert!(text.contains("unknown field `operatoin`"), "{text}");
    client.cancel().await.unwrap();
}
