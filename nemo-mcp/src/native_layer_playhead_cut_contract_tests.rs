//! Public cut DTO contract only; semantic source/range admission belongs to native.
use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::{json, Value};
fn payload() -> Value {
    json!({"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":10})
}
fn envelope(value: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"cut-contract","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":OP,"payload":value})).unwrap()
}
#[test]
fn cut_payload_is_closed_bounded_and_preserves_nonroot_opaque_identities_for_native_refusal() {
    for (context, layer, frame) in [
        ("scene-root".to_owned(), "a".to_owned(), 0),
        (
            "component context/α".into(),
            " layer:opaque / β ".into(),
            u32::MAX,
        ),
        ("é".repeat(64), "x".repeat(128), 10),
    ] {
        let value = json!({"command":"object.layer.cut","contextId":context,"layerUid":layer,"frame":frame});
        assert!(request(&value));
        assert!(envelope(value).validate().is_ok());
    }
    for key in ["command", "contextId", "layerUid", "frame"] {
        let mut missing = payload();
        missing.as_object_mut().unwrap().remove(key);
        assert!(!request(&missing));
        let mut null = payload();
        null[key] = Value::Null;
        assert!(!request(&null));
    }
    for (key, value) in [
        ("command", json!("object.layer.shift")),
        ("contextId", json!("")),
        ("layerUid", json!("")),
        ("contextId", json!("é".repeat(65))),
        ("layerUid", json!("x".repeat(129))),
        ("frame", json!(-1)),
        ("frame", json!(0.5)),
        ("frame", json!(4294967296_u64)),
        ("frame", json!("10")),
        ("extra", json!(true)),
    ] {
        let mut value_payload = payload();
        value_payload[key] = value;
        assert!(!request(&value_payload), "{key}");
        assert!(envelope(value_payload).validate_input().is_err());
    }
    assert!(!request(&json!(["object.layer.cut", "a", 10])));
    let mut missing_revision = envelope(payload());
    missing_revision.expected_revision = None;
    assert!(missing_revision.validate_input().is_err());
}
#[test]
fn cut_success_shapes_distinguish_created_layer_from_noop_and_match_exact_revision() {
    let changed = json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    assert!(result(&changed));
    assert!(result(&noop));
    assert!(revision_matches(Some(7), 8, &changed));
    assert!(revision_matches(Some(7), 7, &noop));
    assert!(!revision_matches(Some(7), 7, &changed));
    assert!(!revision_matches(Some(7), 8, &noop));
    assert!(!revision_matches(None, 8, &changed));
    assert!(!revision_matches(Some(u64::MAX), 0, &changed));
    for bad in [
        json!({"applied":true,"historyEntriesAdded":1}),
        json!({"applied":false,"historyEntriesAdded":0,"layerUid":"native-layer:8:0"}),
        json!({"applied":true,"historyEntriesAdded":0,"layerUid":"native-layer:8:0"}),
        json!({"applied":false,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":null}),
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"layer with spaces"}),
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"x".repeat(129)}),
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0","document":{}}),
    ] {
        assert!(!result(&bad), "{bad}");
    }
}
#[test]
fn cut_catalog_examples_are_admitted_through_the_actual_closed_operation() {
    let descriptor = crate::capabilities::native_catalog()
        .capability_for_operation(OP)
        .unwrap();
    assert_eq!(descriptor["id"], "native.layer-playhead-cut");
    assert_eq!(descriptor["operations"], json!([OP]));
    assert_eq!(descriptor["authority"]["fallback"], "none");
    assert_eq!(descriptor["authority"]["adapterOwnsWritableMirror"], false);
    let examples = descriptor["examples"].as_array().unwrap();
    assert!(!examples.is_empty());
    for example in examples {
        let req: NativeApplicationRequest =
            serde_json::from_value(example["request"].clone()).unwrap();
        assert!(req.validate().is_ok());
        assert_eq!(req.operation, OP);
        assert!(request(&req.payload));
        assert!(result(&example["response"]["result"]));
        assert!(revision_matches(
            req.expected_revision,
            example["response"]["contentRevision"].as_u64().unwrap(),
            &example["response"]["result"]
        ));
    }
}
