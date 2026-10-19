//! Original native byte ingress and reply-owner/revision controls for Audio edits.
use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::wire::{decode_json_bounded, validate_native_response};
use serde_json::json;
fn envelope() -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"audio-edit-wire","instanceId":"instance",
        "documentId":"document","expectedRevision":7,"operation":OP,
        "payload":{"kind":"setMuted","audioId":"track-a","muted":true}}),
    )
    .unwrap()
}
fn response(applied: bool) -> NativeApplicationResponse {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"audio-edit-wire","instanceId":"instance",
        "documentId":"document","contentRevision":if applied{8}else{7},"ok":true,
        "result":{"applied":applied,"historyEntriesAdded":u64::from(applied)}}),
    )
    .unwrap()
}
#[test]
fn original_request_bytes_reject_duplicate_fields_before_value_collapse() {
    let r = envelope();
    let raw = serde_json::to_string(&r).unwrap();
    for duplicate in [
        raw.replace("\"muted\":true", "\"muted\":true,\"muted\":false"),
        raw.replace(
            "\"audioId\":\"track-a\"",
            "\"audioId\":\"track-a\",\"audioId\":\"other\"",
        ),
        raw.replace(
            "\"expectedRevision\":7",
            "\"expectedRevision\":7,\"expectedRevision\":8",
        ),
    ] {
        assert_ne!(duplicate, raw);
        assert!(
            decode_json_bounded::<NativeApplicationRequest>(duplicate.as_bytes(), 4096).is_err()
        );
    }
    let decoded: NativeApplicationRequest = decode_json_bounded(raw.as_bytes(), 4096).unwrap();
    assert_eq!(decoded, r);
    assert!(decoded.validate().is_ok());
}
#[test]
fn complete_audio_reply_is_correlated_to_request_instance_document_and_changed_or_noop_revision() {
    let request = envelope();
    for applied in [false, true] {
        let good = response(applied);
        assert!(validate_native_response(&good, &request).is_ok());
        for field in ["request", "instance", "document", "revision"] {
            let mut bad = good.clone();
            match field {
                "request" => bad.request_id = "foreign".into(),
                "instance" => bad.instance_id = "foreign".into(),
                "document" => bad.document_id = "foreign".into(),
                _ => bad.content_revision = if applied { 7 } else { 8 },
            }
            assert!(validate_native_response(&bad, &request).is_err());
        }
        let mut leaked = good;
        leaked.result.as_mut().unwrap()["audioTracks"] = json!([]);
        assert!(validate_native_response(&leaked, &request).is_err());
    }
}
#[test]
fn actual_command_uses_existing_request_response_bounds_and_no_missing_revision_admission() {
    let mut request = envelope();
    assert_eq!(crate::wire::native_request_byte_limit(OP), 4096);
    assert_eq!(crate::wire::native_response_byte_limit(OP), 4096);
    request.expected_revision = None;
    assert!(request.validate_input().is_err());
    let mut request = envelope();
    request.payload["dataB64"] = json!("data:audio/wav;base64,AAAA");
    assert!(request.validate_input().is_err());
}
