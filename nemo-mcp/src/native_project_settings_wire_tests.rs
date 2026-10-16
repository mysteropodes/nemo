//! Shared wire validators, no simulated project owner.
use super::*;
use serde_json::json;
fn pair(applied: bool) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let r=serde_json::from_value(json!({"apiVersion":2,"requestId":"settings","instanceId":"instance","documentId":"document",
      "expectedRevision":7,"operation":"command.document.object.project.settings.edit",
      "payload":{"command":"object.project.settings.edit","action":{"kind":"setFps","fps":29.97}}})).unwrap();
    let response=serde_json::from_value(json!({"apiVersion":2,"requestId":"settings","instanceId":"instance","documentId":"document",
      "contentRevision":if applied {8}else{7},"ok":true,"result":{"applied":applied,"historyEntriesAdded":if applied {1}else{0}}})).unwrap();
    (r, response)
}
#[test]
fn settings_wire_validates_changed_and_noop_revision_without_project_payload() {
    for applied in [false, true] {
        let (request, response) = pair(applied);
        assert!(request.validate().is_ok());
        assert!(validate_native_response(&response, &request).is_ok());
        let mut wrong = response.clone();
        wrong.content_revision = if applied { 7 } else { 8 };
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut wrong = response.clone();
        wrong.result.as_mut().unwrap()["path"] = json!("private");
        assert!(validate_native_response(&wrong, &request).is_err());
        let mut failure = response;
        failure.ok = false;
        failure.result = None;
        failure.error = Some(
            serde_json::from_value(
                json!({"code":"stale_revision","message":"Settings expected revision is stale."}),
            )
            .unwrap(),
        );
        assert!(validate_native_response(&failure, &request).is_ok());
    }
}
#[tokio::test]
async fn settings_original_byte_gate_rejects_nested_duplicate_before_value_collapse() {
    let raw=br#"{"apiVersion":2,"requestId":"settings","instanceId":"instance","documentId":"document","expectedRevision":7,"operation":"command.document.object.project.settings.edit","payload":{"command":"object.project.settings.edit","action":{"kind":"setFps","fps":24,"fps":30}}}"#;
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(raw, NATIVE_MAX_MESSAGE_BYTES).is_err()
    );
    let (request, _) = pair(true);
    let bytes = serde_json::to_vec(&request).unwrap();
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(&bytes, NATIVE_MAX_MESSAGE_BYTES)
            .unwrap()
            .validate()
            .is_ok()
    );
}
