//! D12 original byte admission and exact request/result identity correlation.
use super::*;
use serde_json::json;
fn pair(applied: bool) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let request = serde_json::from_value(json!({"apiVersion":2,"requestId":"track","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":"command.document.object.property.track.edit","payload":{
            "command":"object.property.track.edit","selections":[{"target":{"contextId":"scene-root","layerUid":"opaque layer","strokeId":"opaque stroke"},"property":"opacity","frame":0}],
            "action":{"kind":"preset","preset":"ease"}}})).unwrap();
    let response = serde_json::from_value(
        json!({"apiVersion":2,"requestId":"track","instanceId":"instance","documentId":"document",
        "contentRevision":if applied {8}else{7},"ok":true,
        "result":{"applied":applied,"historyEntriesAdded":if applied {1}else{0}}}),
    )
    .unwrap();
    (request, response)
}
#[test]
fn track_wire_rejects_foreign_open_and_wrong_revision_successes_and_preserves_error_contract() {
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
fn track_original_bytes_reject_nested_duplicate_members_before_value_collapse() {
    let (request, _) = pair(true);
    for action in [
        json!({"kind":"preset","preset":"ease"}),
        json!({"kind":"influence","side":"out","percent":-500}),
        json!({"kind":"subdivide"}),
    ] {
        let mut request = request.clone();
        request.payload["action"] = action;
        let raw = serde_json::to_string(&request).unwrap();
        let mut members = vec![
            ("command", request.payload["command"].clone()),
            ("selections", request.payload["selections"].clone()),
            ("action", request.payload["action"].clone()),
        ];
        for object in [
            &request.payload["selections"][0],
            &request.payload["selections"][0]["target"],
            &request.payload["action"],
        ] {
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
fn track_complete_envelope_retains_inclusive_4096_and_rejects_4097_original_bytes() {
    let (mut request, _) = pair(true);
    let size = serde_json::to_vec(&request).unwrap().len();
    let current = request.payload["selections"][0]["target"]["layerUid"]
        .as_str()
        .unwrap()
        .len();
    request.payload["selections"][0]["target"]["layerUid"] =
        json!("x".repeat(current + 4096 - size));
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
    let uid = request.payload["selections"][0]["target"]["layerUid"]
        .as_str()
        .unwrap()
        .to_owned();
    request.payload["selections"][0]["target"]["layerUid"] = json!(uid + "x");
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
