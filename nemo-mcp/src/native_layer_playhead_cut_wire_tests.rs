//! Complete native-envelope byte and correlation validation for cut.
use super::*;
use serde_json::{json, Value};
const OP: &str = "command.document.object.layer.cut";
fn pair(applied: bool) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let request = serde_json::from_value(json!({"apiVersion":2,"requestId":"cut-wire","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":OP,"payload":{"command":"object.layer.cut","contextId":"scene-root","layerUid":"a","frame":10}})).unwrap();
    let result = if applied {
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"})
    } else {
        json!({"applied":false,"historyEntriesAdded":0})
    };
    let response = serde_json::from_value(json!({"apiVersion":2,"requestId":"cut-wire","instanceId":"instance","documentId":"document",
        "contentRevision":if applied {8}else{7},"ok":true,"result":result})).unwrap();
    (request, response)
}
#[test]
fn cut_wire_rejects_foreign_owner_request_revision_or_open_receipt_shape() {
    for applied in [false, true] {
        let (request, response) = pair(applied);
        assert!(request.validate().is_ok());
        assert!(validate_native_response(&response, &request).is_ok());
        for key in ["requestId", "instanceId", "documentId"] {
            let mut value = serde_json::to_value(&response).unwrap();
            value[key] = json!("foreign");
            let wrong = serde_json::from_value(value).unwrap();
            assert!(validate_native_response(&wrong, &request).is_err(), "{key}");
        }
        for result in [
            json!({"applied":true,"historyEntriesAdded":1}),
            json!({"applied":false,"historyEntriesAdded":0,"layerUid":"native-layer:8:0"}),
            json!({"applied":applied,"historyEntriesAdded":if applied {0}else{1}}),
            json!({"applied":applied,"historyEntriesAdded":if applied {1}else{0},"extra":0}),
        ] {
            let mut wrong = response.clone();
            wrong.result = Some(result);
            assert!(validate_native_response(&wrong, &request).is_err());
        }
        let mut wrong = response.clone();
        wrong.content_revision = if applied { 7 } else { 8 };
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut wrong = response.clone();
        wrong.result.as_mut().unwrap()["padding"] = json!("x".repeat(4096));
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut unavailable = response.clone();
        unavailable.ok = false;
        unavailable.result = None;
        unavailable.error = Some(
            serde_json::from_value(
                json!({"code":"unavailable","message":"Source provider unavailable"}),
            )
            .unwrap(),
        );
        assert!(validate_native_response(&unavailable, &request).is_ok());
        unavailable.document_id = "replaced-document".into();
        assert!(validate_native_response(&unavailable, &request).is_err());
        unavailable.error = Some(
            serde_json::from_value(json!({"code":"wrong_document","message":"Replaced owner"}))
                .unwrap(),
        );
        assert!(
            validate_native_response(&unavailable, &request).is_ok(),
            "only established wrong_document replacement exception"
        );
        unavailable.instance_id = "foreign-instance".into();
        assert!(validate_native_response(&unavailable, &request).is_err());
    }
}
#[test]
fn cut_original_bytes_reject_recursive_duplicates_and_overbound_complete_messages() {
    let (request, _) = pair(true);
    let valid = serde_json::to_string(&request).unwrap();
    for key in ["command", "contextId", "layerUid", "frame"] {
        let needle = format!("\"{key}\":{}", request.payload[key]);
        assert_eq!(valid.matches(&needle).count(), 1);
        let duplicate = valid.replacen(&needle, &format!("{needle},{needle}"), 1);
        assert!(decode_json_bounded::<NativeApplicationRequest>(
            duplicate.as_bytes(),
            NATIVE_MAX_MESSAGE_BYTES
        )
        .is_err());
    }
    let repeated = valid.replacen(
        "\"expectedRevision\":7",
        "\"expectedRevision\":7,\"expectedRevision\":7",
        1,
    );
    assert_ne!(repeated, valid);
    assert!(decode_json_bounded::<NativeApplicationRequest>(
        repeated.as_bytes(),
        NATIVE_MAX_MESSAGE_BYTES
    )
    .is_err());
    let mut bytes = valid.into_bytes();
    bytes.resize(4096, b' ');
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(&bytes, NATIVE_MAX_MESSAGE_BYTES)
            .unwrap()
            .validate()
            .is_ok()
    );
    bytes.push(b' ');
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(&bytes, NATIVE_MAX_MESSAGE_BYTES).is_err()
    );
    let mut missing = serde_json::to_value(request).unwrap();
    missing.as_object_mut().unwrap().remove("expectedRevision");
    assert!(decode_json_bounded::<NativeApplicationRequest>(
        &serde_json::to_vec(&missing).unwrap(),
        NATIVE_MAX_MESSAGE_BYTES
    )
    .unwrap()
    .validate_input()
    .is_err());
    missing["expectedRevision"] = Value::Null;
    assert!(decode_json_bounded::<NativeApplicationRequest>(
        &serde_json::to_vec(&missing).unwrap(),
        NATIVE_MAX_MESSAGE_BYTES
    )
    .is_err());
}
