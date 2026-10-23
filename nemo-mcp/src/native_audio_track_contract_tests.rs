//! Public metadata shape, Unicode and bound controls; not a substitute for the native owner.
use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::json;

fn envelope(payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"audio-edit-test",
        "instanceId":"instance","documentId":"document","expectedRevision":7,
        "operation":OP,"payload":payload}))
    .unwrap()
}
#[test]
fn exact_metadata_variants_register_with_existing_bounds_and_no_resource_or_volume_authority() {
    for payload in [
        json!({"kind":"setMuted","audioId":"track-a","muted":true}),
        json!({"kind":"setMuted","audioId":"track-a","muted":false}),
        json!({"kind":"rename","audioId":"track-a","name":""}),
        json!({"kind":"rename","audioId":"data:literal-id","name":"blob:literal name"}),
        json!({"kind":"setTrim","audioId":"track-a","trimStart":-0.625}),
        json!({"kind":"setTrim","audioId":"track-a","trimEnd":null}),
        json!({"kind":"setTrim","audioId":"track-a","trimEnd":0}),
        json!({"kind":"setTrim","audioId":"track-a","trimStart":1e100,"trimEnd":-2.75}),
        json!({"kind":"unset","audioId":"track-a","field":"name"}),
        json!({"kind":"unset","audioId":"track-a","field":"muted"}),
        json!({"kind":"unset","audioId":"track-a","field":"trimStart"}),
        json!({"kind":"unset","audioId":"track-a","field":"trimEnd"}),
    ] {
        assert!(request(&payload), "{payload}");
        assert!(envelope(payload).validate().is_ok());
    }
    let descriptor = crate::capabilities::native_catalog()
        .capability_for_operation(OP)
        .unwrap();
    assert_eq!(descriptor["id"], "native.audio-track");
    assert_eq!(descriptor["authority"]["adapterOwnsWritableMirror"], false);
    assert_eq!(descriptor["authority"]["fallback"], "none");
    assert_eq!(crate::wire::native_request_byte_limit(OP), 4096);
    assert_eq!(crate::wire::native_response_byte_limit(OP), 4096);
    let payload = json!({"kind":"setTrim","audioId":"track-a","trimEnd":null});
    let before = payload.clone();
    assert!(request(&payload));
    assert_eq!(payload, before);
    assert!(payload.get("trimStart").is_none());
}
#[test]
fn unicode_identifiers_and_names_follow_utf8_bounds_and_rust_whitespace_without_trimming() {
    for id in [
        "a".repeat(255),
        format!("{}a", "é".repeat(127)),
        "\u{feff}".into(),
        " a ".into(),
    ] {
        assert!(request(&json!({"kind":"rename","audioId":id,"name":""})));
    }
    for id in [
        String::new(),
        "\u{85}\u{2003}".into(),
        "x\0".into(),
        "é".repeat(128),
        "a".repeat(256),
    ] {
        assert!(!request(&json!({"kind":"rename","audioId":id,"name":""})));
    }
    for text in [
        String::new(),
        " \u{85} ".into(),
        format!("{}a", "é".repeat(127)),
    ] {
        assert!(request(
            &json!({"kind":"rename","audioId":"track","name":text})
        ));
    }
    for text in ["é".repeat(128), "a".repeat(256), "a\0b".into()] {
        assert!(!request(
            &json!({"kind":"rename","audioId":"track","name":text})
        ));
    }
}
#[test]
fn unknown_absent_null_coerced_and_out_of_scope_payloads_refuse() {
    for payload in [
        Value::Null,
        json!([]),
        json!(["setMuted", "track", true]),
        json!({"kind":"setMuted","audioId":"track","muted":1}),
        json!({"kind":"setMuted","audioId":"track","muted":"false"}),
        json!({"kind":"setMuted","audioId":"track","muted":null}),
        json!({"kind":"setMuted","audioId":"track"}),
        json!({"kind":"rename","audioId":"track","name":null}),
        json!({"kind":"rename","audioId":"track","name":0}),
        json!({"kind":"rename","name":"take"}),
        json!({"kind":"setTrim","audioId":"track"}),
        json!({"kind":"setTrim","audioId":"track","trimStart":null}),
        json!({"kind":"setTrim","audioId":"track","trimEnd":"0"}),
        json!({"kind":"setTrim","audioId":"track","trimStart":false}),
        json!({"kind":"unset","audioId":"track","field":"volume"}),
        json!({"kind":"unset","audioId":"track","field":null}),
        json!({"kind":"unset","audioId":"track","field":"name","name":"x"}),
        json!({"kind":"setMuted","audioId":"track","muted":true,"offsetFrames":3}),
        json!({"kind":"rename","audioId":"track","name":"x","dataB64":"AAAA"}),
        json!({"kind":"setVolume","audioId":"track","volume":0.5}),
        json!({"kind":"setMotion","audioId":"track","motion":{}}),
        json!({"kind":"remove","audioId":"track"}),
    ] {
        assert!(!request(&payload), "{payload}");
        assert!(envelope(payload).validate_input().is_err());
    }
    let mut missing_revision = envelope(json!({"kind":"unset","audioId":"track","field":"name"}));
    missing_revision.expected_revision = None;
    assert!(missing_revision.validate_input().is_err());
}
#[test]
fn request_byte_bound_stays4096_and_invalid_number_spellings_cannot_become_json_values() {
    let small = envelope(json!({"kind":"rename","audioId":"track","name":""}));
    assert!(small.validate_input().is_ok());
    let mut oversized = small;
    oversized.request_id = "x".repeat(4096);
    assert!(oversized.validate_input().is_err());
    for raw in ["NaN", "Infinity", "-Infinity", "1e999"] {
        assert!(serde_json::from_str::<Value>(raw).is_err());
    }
}
