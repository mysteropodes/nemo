//! Raw admission and immutable persistent field-presence controls.
use super::*;
use serde_json::{json, Value};
fn tracks() -> AudioTracks {
    serde_json::from_value(
        json!([{"assetId":"wave-a","audioId":"track-a","name":"original",
        "volume":0,"trimEnd":null,"motionStatic":{"volume":[0.75]}}]),
    )
    .unwrap()
}
fn edited(input: &AudioTracks, raw: &str) -> AudioTracks {
    apply(input, AudioTrackEdit::parse(raw.as_bytes()).unwrap())
        .unwrap()
        .unwrap()
}
#[test]
fn mute_and_trim_edit_only_named_fields_and_keep_fractional_values_and_null() {
    let before = tracks();
    let original = serde_json::to_value(&before).unwrap();
    let next = edited(
        &before,
        r#"{"kind":"setMuted","audioId":"track-a","muted":true}"#,
    );
    let mut expected = original.clone();
    expected[0]["muted"] = json!(true);
    assert_eq!(serde_json::to_value(&next).unwrap(), expected);
    assert_eq!(serde_json::to_value(&before).unwrap(), original);
    let next = edited(
        &next,
        r#"{"kind":"setTrim","audioId":"track-a","trimStart":0.125,"trimEnd":1.875}"#,
    );
    expected[0]["trimStart"] = json!(0.125);
    expected[0]["trimEnd"] = json!(1.875);
    assert_eq!(serde_json::to_value(&next).unwrap(), expected);
    let next = edited(
        &next,
        r#"{"kind":"setTrim","audioId":"track-a","trimEnd":null}"#,
    );
    expected[0]["trimEnd"] = Value::Null;
    assert_eq!(serde_json::to_value(next).unwrap(), expected);
}
#[test]
fn absence_false_and_explicit_unset_have_no_coercion_or_accidental_authorship() {
    let before = tracks();
    assert!(apply(
        &before,
        AudioTrackEdit::parse(br#"{"kind":"setMuted","audioId":"track-a","muted":false}"#).unwrap()
    )
    .unwrap()
    .is_none());
    assert!(apply(
        &before,
        AudioTrackEdit::parse(br#"{"kind":"setTrim","audioId":"track-a","trimEnd":null}"#).unwrap()
    )
    .unwrap()
    .is_none());
    let empty = edited(
        &before,
        r#"{"kind":"rename","audioId":"track-a","name":""}"#,
    );
    assert_eq!(serde_json::to_value(&empty).unwrap()[0]["name"], "");
    let next = edited(
        &empty,
        r#"{"kind":"unset","audioId":"track-a","field":"name"}"#,
    );
    assert!(serde_json::to_value(next).unwrap()[0].get("name").is_none());
    let next = edited(
        &before,
        r#"{"kind":"unset","audioId":"track-a","field":"trimEnd"}"#,
    );
    assert!(serde_json::to_value(next).unwrap()[0]
        .get("trimEnd")
        .is_none());
}
#[test]
fn raw_positional_duplicate_unknown_wrong_type_and_source_offset_motion_writes_refuse() {
    for raw in [
        r#"["setMuted","track-a",true]"#,
        r#"{"kind":"setMuted","audioId":"track-a","audioId":"other","muted":true}"#,
        r#"{"kind":"setMuted","audioId":"track-a","muted":false,"muted":true}"#,
        r#"{"kind":"setMuted","audioId":"track-a","muted":1}"#,
        r#"{"kind":"setTrim","audioId":"track-a"}"#,
        r#"{"kind":"setTrim","audioId":"track-a","trimStart":null}"#,
        r#"{"kind":"setTrim","audioId":"track-a","trimEnd":"1"}"#,
        r#"{"kind":"rename","audioId":"track-a","name":"n","assetId":"other"}"#,
        r#"{"kind":"unset","audioId":"track-a","field":"offsetFrames"}"#,
        r#"{"kind":"unset","audioId":"track-a","field":"motion"}"#,
    ] {
        assert!(
            matches!(
                AudioTrackEdit::parse(raw.as_bytes()),
                Err(AudioTrackEditError::Invalid)
            ),
            "{raw}"
        );
    }
}
#[test]
fn actual_id_resolution_and_utf8_byte_limit_preserve_original_table_on_refusal() {
    let before = tracks();
    let original = serde_json::to_value(&before).unwrap();
    let edit =
        AudioTrackEdit::parse(br#"{"kind":"rename","audioId":"missing","name":"x"}"#).unwrap();
    assert_eq!(
        apply(&before, edit).err(),
        Some(AudioTrackEditError::NotFound)
    );
    let good = json!({"kind":"rename","audioId":"track-a","name":"é".repeat(127)}).to_string();
    assert!(AudioTrackEdit::parse(good.as_bytes()).is_ok());
    let bad = json!({"kind":"rename","audioId":"track-a","name":"é".repeat(128)}).to_string();
    assert!(matches!(
        AudioTrackEdit::parse(bad.as_bytes()),
        Err(AudioTrackEditError::Invalid)
    ));
    assert_eq!(serde_json::to_value(before).unwrap(), original);
}

#[test]
fn trim_number_equivalence_preserves_spelling_without_collapsing_large_exact_integers() {
    let n = |raw: &str| serde_json::from_str::<Number>(raw).unwrap();
    for (a, b, equal) in [
        ("0", "0.0", true),
        ("0", "-0.0", true),
        ("5", "5.0", true),
        ("9007199254740992", "9007199254740992.0", true),
        ("9007199254740993", "9007199254740992", false),
        ("9007199254740993", "9007199254740992.0", false),
        ("9223372036854775807", "9223372036854775808.0", false),
        ("-9223372036854775808", "-9223372036854775808.0", true),
        ("18446744073709551615", "18446744073709551616.0", false),
        ("18446744073709549568", "18446744073709549568.0", true),
    ] {
        assert_eq!(number_equal(&n(a), &n(b)), equal, "{a} vs {b}");
        assert_eq!(number_equal(&n(b), &n(a)), equal, "{b} vs {a}");
    }
    let before = tracks();
    let authored = edited(
        &before,
        r#"{"kind":"setTrim","audioId":"track-a","trimStart":0.0}"#,
    );
    // Missing start stays distinct from a newly authored zero.
    assert_eq!(
        serde_json::to_value(&authored).unwrap()[0]["trimStart"],
        json!(0.0)
    );
    let raw = serde_json::to_vec(&authored).unwrap();
    let equal =
        AudioTrackEdit::parse(br#"{"kind":"setTrim","audioId":"track-a","trimStart":0}"#).unwrap();
    assert!(apply(&authored, equal).unwrap().is_none());
    assert_eq!(serde_json::to_vec(authored).unwrap(), raw);
}
