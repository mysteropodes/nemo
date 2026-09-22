//! Unit tests for `contract.rs` against its private surface (test seams like
//! `check_payload_with`/`property_payload_example_with` stay crate-private rather
//! than becoming part of the public API just to be reachable from an integration
//! test). Kept a separate file, included via `#[path]`, so this module's size
//! counts against its own budget instead of inflating `contract.rs`'s.
use super::*;
use crate::capabilities::{CapabilityCatalog, NativeCapabilityCatalog};

fn descriptor(json: &str) -> CapabilityCatalog {
    CapabilityCatalog::from_sources(&[json])
}

fn native_request(operation: &str, payload: Value) -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: 2,
        request_id: "native-request-1".into(),
        instance_id: "instance-a".into(),
        document_id: "native-document-1".into(),
        expected_revision: None,
        operation: operation.into(),
        payload,
        cancelled_before_dispatch: false,
    }
}

#[test]
fn native_catalog_rejects_duplicate_ids_and_operations() {
    let first = r#"{"schemaVersion":2,"apiVersion":2,"id":"first","operations":["query.first"]}"#;
    let duplicate_id =
        r#"{"schemaVersion":2,"apiVersion":2,"id":"first","operations":["query.second"]}"#;
    let duplicate_operation =
        r#"{"schemaVersion":2,"apiVersion":2,"id":"second","operations":["query.first"]}"#;
    assert!(
        NativeCapabilityCatalog::from_sources(&[first, duplicate_id])
            .unwrap_err()
            .contains("duplicate native capability id")
    );
    assert!(
        NativeCapabilityCatalog::from_sources(&[first, duplicate_operation])
            .unwrap_err()
            .contains("duplicate native operation")
    );
}

#[test]
fn native_request_resolution_is_descriptor_driven_and_bounded() {
    let valid = native_request(
        "query.document.opacity",
        json!({
            "stableTarget": {"layerUid": "r08_curve_layer"}
        }),
    );
    assert!(valid.validate().is_ok());

    let unknown = native_request("query.feature.not.registered", json!({}));
    assert_eq!(unknown.validate().unwrap_err().code(), "invalid_request");

    let malformed = native_request("query.document.opacity", json!("encoded"));
    assert_eq!(
        malformed.validate().unwrap_err().code(),
        "malformed_payload"
    );

    let oversized = native_request(
        "query.document.opacity",
        json!({"padding": "x".repeat(NATIVE_MAX_MESSAGE_BYTES)}),
    );
    assert!(oversized.validate().unwrap_err().message().contains("4096"));

    let mut bad_identity = valid;
    bad_identity.request_id = "-not-leading-alphanumeric".into();
    assert!(bad_identity.validate().is_err());
}

#[test]
fn native_pinned_reads_are_discovered_without_transport_operation_variants() {
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-opacity.json"
    ))
    .unwrap();
    assert_eq!(descriptor["availability"]["state"], "available");
    assert!(descriptor["availability"]["reason"].is_null());
    assert_eq!(
        descriptor["resourceBoundary"]["maxReadResponseBytes"],
        NATIVE_MAX_MESSAGE_BYTES
    );
    for (operation, payload) in [
        ("query.document.serialize", json!({"atRevision": 0})),
        (
            "query.document.evaluate",
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 10}),
        ),
    ] {
        assert!(crate::capabilities::native_catalog()
            .capability_for_operation(operation)
            .is_some());
        let request = native_request(operation, payload);
        assert!(request.validate().is_ok());
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["operation"], operation);
        assert!(encoded.get("expectedRevision").is_none());
        assert_eq!(
            serde_json::from_value::<NativeApplicationRequest>(encoded.clone()).unwrap(),
            request
        );
        let mut explicit_null = encoded;
        explicit_null["expectedRevision"] = Value::Null;
        assert!(serde_json::from_value::<NativeApplicationRequest>(explicit_null).is_err());
    }
    for (operation, payload) in [
        ("query.document.serialize", json!({})),
        (
            "query.document.serialize",
            json!({"atRevision": 0, "extra": true}),
        ),
        (
            "query.document.evaluate",
            json!({"atRevision": 0, "contextId": "bad id", "frame": 10}),
        ),
        (
            "query.document.evaluate",
            json!({"atRevision": 0, "contextId": "scene-root", "frame": 4294967296_u64}),
        ),
    ] {
        assert_eq!(
            native_request(operation, payload)
                .validate()
                .unwrap_err()
                .code(),
            "malformed_payload"
        );
    }
    let mut with_revision = native_request("query.document.serialize", json!({"atRevision": 0}));
    with_revision.expected_revision = Some(0);
    assert_eq!(
        with_revision.validate().unwrap_err().code(),
        "invalid_request"
    );
}

#[test]
fn json_kind_names_every_json_shape() {
    assert_eq!(json_kind(&json!(null)), "null");
    assert_eq!(json_kind(&json!(true)), "a boolean");
    assert_eq!(json_kind(&json!(1)), "a number");
    assert_eq!(json_kind(&json!("s")), "a JSON-encoded string");
    assert_eq!(json_kind(&json!([])), "an array");
    assert_eq!(json_kind(&json!({})), "an object");
}

#[test]
fn unavailable_capability_is_a_typed_error_naming_its_reason() {
    // No registered descriptor is both property-family and unavailable today
    // (export-job.json is job-family, opacity.json is available) — a synthetic
    // one is the only way to exercise this resolution outcome for real.
    let catalog = descriptor(
        r#"{
            "id": "scale",
            "input": {"type": "object", "required": ["layerId"]},
            "effects": {"lifecycle": ["property.get", "property.set"]},
            "availability": {"state": "unavailable", "reason": "feature-flag-off"},
            "fixture": {"input": {"layerId": "layer-1", "value": 40}}
        }"#,
    );
    let payload = json!({"layerId": "layer-1", "property": "scale", "value": 40});
    let error = Operation::PropertySet
        .check_payload_with(&payload, &catalog)
        .unwrap_err();
    assert_eq!(error.code(), "unavailable");
    assert!(error.message().contains("scale"));
    assert!(error.message().contains("feature-flag-off"));
    // The Display impl is a thin wrapper over `message()`, exercised here.
    assert_eq!(error.to_string(), error.message());
}

#[test]
fn generic_property_projection_accepts_descriptor_owned_string_and_custom_fields() {
    // This deliberately differs from opacity: `value` is a string and `mode` is a
    // required custom field. Adding it needs no Operation variant, payload struct
    // field, or central dispatch arm — only the descriptor projection changes.
    let catalog = descriptor(
        r#"{
            "id": "label",
            "input": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "layerId": {"type": "string"},
                    "value": {"type": "string"},
                    "mode": {"type": "string"}
                },
                "required": ["layerId", "value", "mode"]
            },
            "effects": {"lifecycle": ["property.set"]},
            "availability": {"state": "available", "reason": null},
            "fixture": {"input": {"layerId": "layer-1", "value": "Title", "mode": "replace"}}
        }"#,
    );
    let valid = json!({
        "layerId": "layer-1",
        "property": "label",
        "value": "Title",
        "mode": "replace"
    });
    assert!(Operation::PropertySet
        .check_payload_with(&valid, &catalog)
        .is_ok());

    let wrong_type = json!({
        "layerId": "layer-1",
        "property": "label",
        "value": 42,
        "mode": "replace"
    });
    let error = Operation::PropertySet
        .check_payload_with(&wrong_type, &catalog)
        .unwrap_err();
    assert_eq!(error.code(), "malformed_payload");
    assert!(error.message().contains("payload.value has type a number"));

    let missing_custom = json!({
        "layerId": "layer-1",
        "property": "label",
        "value": "Title"
    });
    let error = Operation::PropertySet
        .check_payload_with(&missing_custom, &catalog)
        .unwrap_err();
    assert_eq!(error.code(), "malformed_payload");
    assert!(error.message().contains("missing mode"));
}

#[test]
fn fixed_payload_rejections_keep_the_legacy_control_vocabulary_typed() {
    let unknown = Operation::Snapshot.check_payload(&json!({"layerUid": "layer-1"}));
    assert_eq!(unknown.unwrap_err().code(), "malformed_payload");

    let wrong_type = Operation::Snapshot.check_payload(&json!({"value": true}));
    assert_eq!(wrong_type.unwrap_err().code(), "malformed_payload");

    let non_object_replay = Operation::DiagnosticsReplay.check_payload(&json!({"request": false}));
    assert_eq!(non_object_replay.unwrap_err().code(), "malformed_payload");

    let incomplete_replay = Operation::DiagnosticsReplay.check_payload(&json!({
        "request": {"operation": "snapshot"}
    }));
    assert_eq!(incomplete_replay.unwrap_err().code(), "malformed_payload");

    let unknown_replay_operation = Operation::DiagnosticsReplay.check_payload(&json!({
        "request": {"operation": "not.an.operation", "payload": {}}
    }));
    assert_eq!(
        unknown_replay_operation.unwrap_err().code(),
        "malformed_payload"
    );

    let extra_replay_envelope = Operation::DiagnosticsReplay.check_payload(&json!({
        "request": {"operation": "snapshot", "payload": {}},
        "source": "trace"
    }));
    let error = extra_replay_envelope.unwrap_err();
    assert_eq!(error.code(), "malformed_payload");
    assert!(error.message().contains("expects only request"));
}

#[test]
fn fixed_payload_compatibility_accepts_every_legacy_nullable_wire_type() {
    // Fixed transport operations retain the original nullable field vocabulary.
    // Exercise both integer representations because JSON can carry values beyond i64.
    for payload in [
        json!({
            "layerId": "layer-1",
            "property": "opacity",
            "value": 37.5,
            "frame": -1,
            "animated": true,
            "request": {"operation": "snapshot"}
        }),
        json!({
            "layerId": null,
            "property": null,
            "value": null,
            "frame": u64::MAX,
            "animated": null,
            "request": null
        }),
        json!({"frame": null}),
    ] {
        assert!(
            Operation::Snapshot.check_payload(&payload).is_ok(),
            "{payload}"
        );
    }
}

#[test]
fn descriptor_schema_projects_query_and_command_branches() {
    assert_eq!(
        Operation::labels().len(),
        11,
        "transport-v1 operation count"
    );
    let mut generator = schemars::SchemaGenerator::default();
    let command = serde_json::to_value(command_payload_schema(&mut generator)).unwrap();
    assert!(command["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .any(|branch| { branch["properties"]["property"]["const"] == "opacity" }));
    assert!(command["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .any(|branch| { branch["required"] == json!(["request"]) }));

    let mut generator = schemars::SchemaGenerator::default();
    let query = serde_json::to_value(query_payload_schema(&mut generator)).unwrap();
    assert_eq!(query["x-nemo-registeredCapabilities"][0]["id"], "opacity");
}

#[test]
fn descriptor_projection_accepts_each_declared_primitive_without_a_payload_struct() {
    let catalog = descriptor(
        r#"{
            "id": "metadata",
            "input": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "layerId": {"type": "string"},
                    "value": {"type": ["array", "null"]},
                    "frame": {"type": "integer"},
                    "animated": {"type": "boolean"}
                },
                "required": ["layerId"]
            },
            "effects": {"lifecycle": ["property.set", "property.key.set", "property.animation.set"]},
            "availability": {"state": "available", "reason": null},
            "fixture": {"input": {"layerId": "layer-1", "value": []}}
        }"#,
    );
    assert!(Operation::PropertySet
        .check_payload_with(
            &json!({"layerId": "layer-1", "property": "metadata", "value": []}),
            &catalog,
        )
        .is_ok());
    assert!(Operation::PropertyKeySet
        .check_payload_with(
            &json!({"layerId": "layer-1", "property": "metadata", "value": null, "frame": 1}),
            &catalog,
        )
        .is_ok());
    assert!(Operation::PropertyAnimationSet
        .check_payload_with(
            &json!({"layerId": "layer-1", "property": "metadata", "animated": true}),
            &catalog,
        )
        .is_ok());
}

#[test]
fn descriptor_projection_accepts_object_and_unconstrained_custom_fields() {
    // JSON Schema permits both object-valued fields and an unconstrained property
    // schema. Neither requires a new Rust payload field or operation variant.
    let catalog = descriptor(
        r#"{
            "id": "annotations",
            "input": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "layerId": {"type": "string"},
                    "value": {"type": "object"},
                    "token": {}
                },
                "required": ["layerId", "value", "token"]
            },
            "effects": {"lifecycle": ["property.set"]},
            "availability": {"state": "available", "reason": null},
            "fixture": {"input": {
                "layerId": "layer-1", "value": {"tag": "draft"}, "token": 7
            }}
        }"#,
    );
    let payload = json!({
        "layerId": "layer-1",
        "property": "annotations",
        "value": {"tag": "draft"},
        "token": 7
    });
    assert!(Operation::PropertySet
        .check_payload_with(&payload, &catalog)
        .is_ok());
}

#[test]
fn descriptor_projection_keeps_open_input_fragments_and_nonstring_selectors_distinct() {
    let catalog = descriptor(
        r#"{
            "id": "open.metadata",
            "input": {"type": "object", "additionalProperties": true, "required": ["layerId"]},
            "effects": {"lifecycle": ["property.set"]},
            "availability": {"state": "available", "reason": null},
            "fixture": {"input": {"layerId": "layer-1", "value": {"tag": "draft"}}}
        }"#,
    );
    assert!(Operation::PropertySet
        .check_payload_with(
            &json!({
                "layerId": "layer-1",
                "property": "open.metadata",
                "value": {"tag": "draft"},
                "owner": "editor"
            }),
            &catalog,
        )
        .is_ok());
    let error = Operation::PropertySet
        .check_payload_with(
            &json!({"layerId": "layer-1", "property": 42, "value": "ignored"}),
            &catalog,
        )
        .unwrap_err();
    assert_eq!(error.code(), "malformed_payload");
    assert!(error.message().contains("missing property"));
}

#[test]
fn a_number_literal_out_of_f64_range_never_survives_json_parsing() {
    // The invariant `check_payload_with`'s `.expect()` relies on for
    // `serde_json::to_value(&recorded.payload)`: a value that failed to
    // deserialize can never reach a `CommandPayload` field in the first place,
    // because serde_json's own parser rejects an out-of-range number at parse
    // time (not just at Rust-literal compile time, where `1e400` would trip
    // `overflowing_literals` before this test even ran).
    let parsed = serde_json::from_str::<Value>(r#"{"value": 1e400}"#);
    assert!(parsed.is_err(), "expected a parse error, got {parsed:?}");
}

#[test]
fn property_example_falls_back_when_no_capability_answers_the_stage() {
    let empty = CapabilityCatalog::from_sources(&[]);
    let example = Operation::PropertySet.property_payload_example_with(&empty);
    assert_eq!(
        example["property"],
        "<id of a registered property capability>"
    );
}

#[test]
fn property_example_falls_back_to_the_fixture_when_no_example_carries_this_verbs_keys() {
    // Registered, and it does support property.set — but its only example (the
    // fixture) never demonstrates a "value", so `property.set`'s own required-key
    // search (via `required_payload_keys`) cannot find a satisfying example and
    // must fall back to the fixture anyway, keys missing and all.
    let catalog = descriptor(
        r#"{
            "id": "rotation",
            "input": {"type": "object", "required": ["layerId"]},
            "effects": {"lifecycle": ["property.get", "property.set"]},
            "availability": {"state": "available", "reason": null},
            "fixture": {"input": {"layerId": "layer-1"}}
        }"#,
    );
    let example = Operation::PropertySet.property_payload_example_with(&catalog);
    assert_eq!(example["property"], "rotation");
    assert_eq!(example["layerId"], "layer-1");
    assert!(example.get("value").is_none());
}
