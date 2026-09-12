//! Unit tests for `contract.rs` against its private surface (test seams like
//! `check_payload_with`/`property_payload_example_with` stay crate-private rather
//! than becoming part of the public API just to be reachable from an integration
//! test). Kept a separate file, included via `#[path]`, so this module's size
//! counts against its own budget instead of inflating `contract.rs`'s.
use super::*;
use crate::capabilities::CapabilityCatalog;

fn descriptor(json: &str) -> CapabilityCatalog {
    CapabilityCatalog::from_sources(&[json])
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
