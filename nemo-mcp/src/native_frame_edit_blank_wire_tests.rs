use super::*;
use serde_json::json;

#[test]
fn blank_key_wire_qualifies_catalog_receipt_and_rejects_noop_identity_or_revision_drift() {
    let operation = crate::native_contract::frame_edit::KEY_BLANK;
    let catalog = crate::capabilities::native_catalog();
    let descriptor = catalog.capability_for_operation(operation).unwrap();
    let example = descriptor["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|example| example["request"]["operation"] == operation)
        .unwrap();
    let request: NativeApplicationRequest =
        serde_json::from_value(example["request"].clone()).unwrap();
    let response: NativeApplicationResponse =
        serde_json::from_value(example["response"].clone()).unwrap();
    assert!(request.validate_input().is_ok());
    assert!(validate_native_response(&response, &request).is_ok());
    for revision in [
        request.expected_revision.unwrap(),
        response.content_revision + 1,
    ] {
        let mut bad = response.clone();
        bad.content_revision = revision;
        assert!(validate_native_response(&bad, &request).is_err());
    }
    for result in [
        json!({"applied":false,"historyEntriesAdded":0}),
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":false,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":1,"cycledLayers":1,"skippedComponentLayers":0}),
        json!({"applied":true,"historyEntriesAdded":1,"document":{}}),
    ] {
        let mut bad = response.clone();
        bad.result = Some(result);
        assert!(validate_native_response(&bad, &request).is_err());
        bad.content_revision = request.expected_revision.unwrap();
        assert!(validate_native_response(&bad, &request).is_err());
    }
    for field in ["requestId", "instanceId", "documentId"] {
        let mut bad = serde_json::to_value(&response).unwrap();
        bad[field] = json!("foreign");
        let bad: NativeApplicationResponse = serde_json::from_value(bad).unwrap();
        assert!(validate_native_response(&bad, &request).is_err());
    }
    let mut later = request.clone();
    later.expected_revision = Some(response.content_revision);
    assert!(validate_native_response(&response, &later).is_err());
}
