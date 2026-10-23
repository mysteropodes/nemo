//! Public DTO, catalog and original-byte/response correlation controls; no owner substitute.
use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::wire::{decode_json_bounded, validate_native_response};
use serde_json::json;

fn envelope(payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"placement-test",
        "instanceId":"instance","documentId":"document","expectedRevision":7,
        "operation":OP,"payload":payload}))
    .unwrap()
}
fn place(target: Value) -> Value {
    json!({"kind":"place","placement":{"id":"p1","assetId":"v1","target":target,"offsetFrames":0}})
}
fn response(applied: bool) -> NativeApplicationResponse {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"placement-test","instanceId":"instance",
        "documentId":"document","contentRevision":if applied {8}else{7},"ok":true,
        "result":{"applied":applied,"historyEntriesAdded":u64::from(applied)}}),
    )
    .unwrap()
}
#[test]
fn exact_four_edit_shapes_register_without_claiming_audio_owner_availability() {
    for payload in [
        place(json!({"kind":"drawing","contextId":"scene-root","layerUid":"L"})),
        place(json!({"kind":"drawing","contextId":"scene-root","layerUid":"L","strokeId":"item"})),
        place(json!({"kind":"audioTrack","trackUid":"track"})),
        json!({"kind":"setOffset","placementId":"p1","offsetFrames":-9007199254740991_i64}),
        json!({"kind":"shiftOffset","placementId":"p1","deltaFrames":9007199254740991_i64}),
        json!({"kind":"remove","placementId":"p1"}),
        json!({"kind":"setOffset","placementId":"p1","offsetFrames":-1.25}),
        json!({"kind":"shiftOffset","placementId":"p1","deltaFrames":0.5}),
    ] {
        assert!(request(&payload));
        let r = envelope(payload);
        assert!(r.validate().is_ok());
        let d = crate::capabilities::native_catalog()
            .capability_for_operation(OP)
            .unwrap();
        assert_eq!(d["id"], "native.project-placement");
        assert_eq!(d["authority"]["adapterOwnsWritableMirror"], false);
        assert_eq!(d["authority"]["fallback"], "none");
        assert_eq!(crate::wire::native_request_byte_limit(OP), 4096);
        assert_eq!(crate::wire::native_response_byte_limit(OP), 4096);
    }
}
#[test]
fn malformed_null_unknown_and_overflow_values_refuse_the_public_dto() {
    let drawing = json!({"kind":"drawing","contextId":"scene-root","layerUid":"L"});
    for payload in [
        json!({"kind":"remove","placementId":"p1","extra":0}),
        json!({"kind":"setOffset","placementId":"p1","offsetFrames":null}),
        {
            let mut p = place(drawing.clone());
            p["placement"]["offsetFrames"] = json!(1.5);
            p
        },
        json!({"kind":"shiftOffset","placementId":"p1","deltaFrames":9007199254740992_u64}),
        json!({"kind":"remove","placementId":"\u{85}"}),
        json!({"kind":"remove","placementId":"a\u{0}"}),
        json!({"kind":"remove","placementId":"é".repeat(128)}),
        place(json!({"kind":"drawing","contextId":"scene-root","layerUid":"L","strokeId":null})),
        place(json!({"kind":"audioTrack","trackUid":"a","path":"file.wav"})),
    ] {
        assert!(!request(&payload));
        assert!(envelope(payload).validate_input().is_err());
    }
    let mut missing = envelope(place(drawing));
    missing.expected_revision = None;
    assert!(missing.validate_input().is_err());
    assert!(envelope(json!({"kind":"remove","placementId":"\u{feff}"}))
        .validate()
        .is_ok());
}
#[test]
fn complete_reply_correlates_owner_and_changed_or_noop_revision() {
    let req = envelope(json!({"kind":"shiftOffset","placementId":"p1","deltaFrames":1}));
    for applied in [false, true] {
        let good = response(applied);
        assert!(validate_native_response(&good, &req).is_ok());
        let mut bad = good.clone();
        bad.content_revision = if applied { 7 } else { 8 };
        assert!(validate_native_response(&bad, &req).is_err());
        for field in ["instance", "document", "request"] {
            let mut bad = good.clone();
            match field {
                "instance" => bad.instance_id = "foreign".into(),
                "document" => bad.document_id = "foreign".into(),
                _ => bad.request_id = "foreign".into(),
            }
            assert!(validate_native_response(&bad, &req).is_err());
        }
        let mut leaked = good;
        leaked.result.as_mut().unwrap()["document"] = json!({});
        assert!(validate_native_response(&leaked, &req).is_err());
    }
}
#[test]
fn raw_duplicate_members_refuse_before_value_collapse_and_valid_bytes_keep_the_same_contract() {
    let req = envelope(json!({"kind":"shiftOffset","placementId":"p1","deltaFrames":1}));
    let valid = serde_json::to_string(&req).unwrap();
    for raw in [
        valid.replace("\"deltaFrames\":1", "\"deltaFrames\":1,\"deltaFrames\":2"),
        valid.replace(
            "\"placementId\":\"p1\"",
            "\"placementId\":\"p1\",\"placementId\":\"p2\"",
        ),
        valid.replace(
            "\"expectedRevision\":7",
            "\"expectedRevision\":7,\"expectedRevision\":8",
        ),
    ] {
        assert_ne!(
            raw, valid,
            "the adversarial duplicate must actually be present"
        );
        assert!(decode_json_bounded::<NativeApplicationRequest>(raw.as_bytes(), 4096).is_err());
    }
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(valid.as_bytes(), 4096)
            .unwrap()
            .validate()
            .is_ok()
    );
    let mut large = req;
    large.payload = json!({"kind":"remove","placementId":"x".repeat(4096)});
    assert!(large.validate_input().is_err());
}
