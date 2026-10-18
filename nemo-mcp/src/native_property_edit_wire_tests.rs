//! D36 original byte admission and exact request/result identity correlation.
use crate::contract::{
    NativeApplicationRequest, NativeApplicationResponse, MAX_SAFE_REVISION,
    NATIVE_MAX_MESSAGE_BYTES,
};
use crate::wire::{decode_json_bounded, validate_native_response};
use serde_json::json;
fn pair(applied: bool) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let request = serde_json::from_value(json!({"apiVersion":2,"requestId":"track","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":"command.document.object.property.edit","payload":{
            "command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"data:opaque layer 雪","strokeId":"blob:opaque stroke"},"property":"data:opacity",
            "action":{"kind":"setValue","frame":0,"values":[25]}}})).unwrap();
    let response = serde_json::from_value(
        json!({"apiVersion":2,"requestId":"track","instanceId":"instance","documentId":"document",
        "contentRevision":if applied {8}else{7},"ok":true,
        "result":{"applied":applied,"historyEntriesAdded":if applied {1}else{0}}}),
    )
    .unwrap();
    (request, response)
}
#[test]
fn property_edit_wire_rejects_foreign_open_and_wrong_revision_successes_and_preserves_error_contract(
) {
    for applied in [true, false] {
        let (request, response) = pair(applied);
        assert!(request.validate().is_ok());
        assert!(validate_native_response(&response, &request).is_ok());
        for key in ["requestId", "instanceId", "documentId"] {
            let mut altered = serde_json::to_value(&response).unwrap();
            altered[key] = json!("foreign");
            assert!(
                validate_native_response(&serde_json::from_value(altered).unwrap(), &request)
                    .is_err()
            );
        }
        let mut wrong = response.clone();
        wrong.content_revision = if applied { 7 } else { 8 };
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut wrong = response.clone();
        wrong.result.as_mut().unwrap()["extra"] = json!({});
        assert!(validate_native_response(&wrong, &request).is_err());
        for code in [
            "stale_revision",
            "cancelled_before_dispatch",
            "unavailable",
            "not_found",
            "invalid_request",
            "wrong_instance",
            "wrong_document",
        ] {
            let mut failure = response.clone();
            failure.ok = false;
            failure.result = None;
            failure.error = Some(
                serde_json::from_value(json!({"code":code,"message":"Native owner refused"}))
                    .unwrap(),
            );
            if code == "wrong_document" {
                failure.document_id = "replacement".into();
            }
            assert!(validate_native_response(&failure, &request).is_ok());
            failure.request_id = "foreign".into();
            assert!(validate_native_response(&failure, &request).is_err());
        }
        let mut huge = response;
        huge.ok = false;
        huge.result = None;
        huge.error = Some(
            serde_json::from_value(json!({"code":"unavailable","message":"x".repeat(4096)}))
                .unwrap(),
        );
        assert!(validate_native_response(&huge, &request).is_err());
    }
    let (request, response) = pair(true);
    let mut wrong_document = response.clone();
    wrong_document.ok = false;
    wrong_document.result = None;
    wrong_document.document_id = "replacement".into();
    wrong_document.error =
        Some(serde_json::from_value(json!({"code":"unavailable","message":"refused"})).unwrap());
    assert!(validate_native_response(&wrong_document, &request).is_err());
    let (mut request, mut response) = pair(true);
    request.expected_revision = Some(MAX_SAFE_REVISION);
    response.content_revision = MAX_SAFE_REVISION + 1;
    assert!(validate_native_response(&response, &request).is_err());
}
#[test]
fn property_edit_original_bytes_reject_nested_duplicate_members_before_value_collapse() {
    let (request, _) = pair(true);
    for action in [
        json!({"kind":"setValue","frame":0,"values":[25]}),
        json!({"kind":"setKey","frame":-0.5,"values":[25,50]}),
        json!({"kind":"removeKey","frame":0}),
        json!({"kind":"setAnimated","frame":0,"animated":true}),
    ] {
        let mut request = request.clone();
        request.payload["action"] = action;
        let raw = serde_json::to_string(&request).unwrap();
        let mut members = vec![
            ("requestId", json!(request.request_id)),
            ("instanceId", json!(request.instance_id)),
            ("documentId", json!(request.document_id)),
            ("operation", json!(request.operation)),
            ("expectedRevision", json!(request.expected_revision)),
            ("command", request.payload["command"].clone()),
            ("target", request.payload["target"].clone()),
            ("property", request.payload["property"].clone()),
            ("action", request.payload["action"].clone()),
        ];
        for object in [&request.payload["target"], &request.payload["action"]] {
            members.extend(
                object
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.clone())),
            );
        }
        for (key, value) in members {
            let needle = format!("\"{key}\":{value}");
            assert_eq!(raw.matches(&needle).count(), 1);
            let duplicate = raw.replacen(&needle, &format!("{needle},{needle}"), 1);
            assert!(
                decode_json_bounded::<NativeApplicationRequest>(
                    duplicate.as_bytes(),
                    NATIVE_MAX_MESSAGE_BYTES
                )
                .is_err(),
                "duplicate {key}"
            );
        }
        assert!(decode_json_bounded::<NativeApplicationRequest>(
            raw.as_bytes(),
            NATIVE_MAX_MESSAGE_BYTES
        )
        .unwrap()
        .validate()
        .is_ok());
    }
}
#[test]
fn property_edit_complete_envelope_retains_inclusive_4096_and_rejects_4097_original_bytes() {
    let (mut request, _) = pair(true);
    let size = serde_json::to_vec(&request).unwrap().len();
    let prefix = request.payload["target"]["layerUid"].as_str().unwrap();
    request.payload["target"]["layerUid"] = json!(prefix.to_owned() + &"x".repeat(4096 - size));
    let mut raw = serde_json::to_vec(&request).unwrap();
    assert_eq!(raw.len(), 4096);
    assert!(request.validate().is_ok());
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(&raw, NATIVE_MAX_MESSAGE_BYTES)
            .unwrap()
            .validate()
            .is_ok()
    );
    raw.push(b' ');
    assert_eq!(raw.len(), 4097);
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(&raw, NATIVE_MAX_MESSAGE_BYTES).is_err()
    );
    let uid = request.payload["target"]["layerUid"]
        .as_str()
        .unwrap()
        .to_owned();
    request.payload["target"]["layerUid"] = json!(uid + "x");
    assert_eq!(serde_json::to_vec(&request).unwrap().len(), 4097);
    assert!(request.validate_input().is_err());
    let (request, _) = pair(true);
    let valid = serde_json::to_string(&request).unwrap();
    let nonfinite = valid.replacen("\"frame\":0", "\"frame\":1e309", 1);
    assert_ne!(nonfinite, valid);
    assert!(decode_json_bounded::<NativeApplicationRequest>(
        nonfinite.as_bytes(),
        NATIVE_MAX_MESSAGE_BYTES
    )
    .is_err());
}

#[test]
fn property_edit_original_reply_duplicates_and_complete_error_byte_bound_are_enforced() {
    let (request, response) = pair(true);
    let raw = serde_json::to_string(&request).unwrap();
    let escaped = raw.replacen("\"kind\":", "\"kind\":\"setValue\",\"\\u006bind\":", 1);
    assert_ne!(escaped, raw);
    assert!(decode_json_bounded::<NativeApplicationRequest>(escaped.as_bytes(), 4096).is_err());
    let raw = serde_json::to_string(&response).unwrap();
    let repeated = raw.replacen("\"applied\":true", "\"applied\":true,\"applied\":true", 1);
    assert_ne!(repeated, raw);
    assert!(decode_json_bounded::<NativeApplicationResponse>(repeated.as_bytes(), 4096).is_err());
    let mut failure = response;
    failure.ok = false;
    failure.result = None;
    failure.error =
        Some(serde_json::from_value(json!({"code":"unavailable","message":"雪"})).unwrap());
    let size = serde_json::to_vec(&failure).unwrap().len();
    failure.error.as_mut().unwrap().message += &"x".repeat(4096 - size);
    assert_eq!(serde_json::to_vec(&failure).unwrap().len(), 4096);
    assert!(validate_native_response(&failure, &request).is_ok());
    failure.error.as_mut().unwrap().message.push('x');
    assert_eq!(serde_json::to_vec(&failure).unwrap().len(), 4097);
    assert!(validate_native_response(&failure, &request).is_err());
}
