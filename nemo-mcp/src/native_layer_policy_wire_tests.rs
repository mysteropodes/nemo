//! F06 original-byte admission and correlated command receipts, no simulated owner.
use super::*;
use serde_json::json;
fn pair(applied: bool) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let request=serde_json::from_value(json!({"apiVersion":2,"requestId":"policy","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":"command.document.object.layer.policy.edit",
        "payload":{"command":"object.layer.policy.edit","layerUid":"legacy layer","flag":"locked","value":true}})).unwrap();
    let response=serde_json::from_value(json!({"apiVersion":2,"requestId":"policy","instanceId":"instance","documentId":"document",
        "contentRevision":if applied {8}else{7},"ok":true,"result":{"applied":applied,"historyEntriesAdded":if applied {1}else{0}}})).unwrap();
    (request, response)
}
#[test]
fn policy_wire_rejects_bad_revision_identity_size_and_open_success_receipts() {
    for applied in [true, false] {
        let (request, response) = pair(applied);
        assert!(request.validate().is_ok());
        assert!(validate_native_response(&response, &request).is_ok());
        for key in ["requestId", "instanceId", "documentId"] {
            let mut value = serde_json::to_value(&response).unwrap();
            value[key] = json!("foreign");
            let wrong = serde_json::from_value(value).unwrap();
            assert!(validate_native_response(&wrong, &request).is_err());
        }
        let mut wrong = response.clone();
        wrong.content_revision = if applied { 7 } else { 8 };
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut wrong = response.clone();
        wrong.result.as_mut().unwrap()["extra"] = json!(1);
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut wrong = response.clone();
        wrong.ok = false;
        wrong.result = None;
        wrong.error = Some(
            serde_json::from_value(json!({"code":"unavailable","message":"x".repeat(4096)}))
                .unwrap(),
        );
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut failure = response;
        failure.ok = false;
        failure.result = None;
        failure.error = Some(
            serde_json::from_value(
                json!({"code":"stale_revision","message":"Policy revision stale"}),
            )
            .unwrap(),
        );
        assert!(validate_native_response(&failure, &request).is_ok());
    }
}
#[test]
fn policy_original_byte_gate_rejects_duplicates_before_typed_payload_collapse() {
    let (request, _) = pair(true);
    let valid = serde_json::to_string(&request).unwrap();
    for key in ["command", "layerUid", "flag", "value"] {
        let value = &request.payload[key];
        let needle = format!("\"{key}\":{value}");
        assert_eq!(valid.matches(&needle).count(), 1);
        let raw = valid.replacen(&needle, &format!("{needle},{needle}"), 1);
        assert!(decode_json_bounded::<NativeApplicationRequest>(
            raw.as_bytes(),
            NATIVE_MAX_MESSAGE_BYTES
        )
        .is_err());
    }
    assert!(decode_json_bounded::<NativeApplicationRequest>(
        valid.as_bytes(),
        NATIVE_MAX_MESSAGE_BYTES
    )
    .unwrap()
    .validate()
    .is_ok());
}
