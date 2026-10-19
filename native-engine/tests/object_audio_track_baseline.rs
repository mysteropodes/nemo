//! Candidate normalization controls; actual source admission/history lives in owner tests.
use super::*;
fn input() -> Value {
    json!({"dataB64":"data:audio/wav;base64,UklGRg==","audioId":"track-a",
        "offsetFrames":-1.25,"name":"","volume":0,"muted":false,
        "trimStart":0,"trimEnd":0,"motion":null,"motionStatic":{}})
}
fn normalize(v: &Value) -> Result<NormalizedAudioTrack, AudioBaselineError> {
    normalize_baseline_track(v.to_string().as_bytes(), "source-a", "place-a", 4096)
}
#[test]
fn normalization_uses_one_source_and_one_lossless_offset_without_defaulting_authored_values() {
    let original = input();
    let n = normalize(&original).unwrap();
    let mut metadata = serde_json::to_value(&n.record).unwrap();
    assert_eq!(
        metadata.as_object_mut().unwrap().remove("assetId"),
        Some(json!("source-a"))
    );
    let mut expected = original.clone();
    expected.as_object_mut().unwrap().remove("dataB64");
    expected.as_object_mut().unwrap().remove("offsetFrames");
    assert_eq!(metadata, expected);
    assert_eq!(n.placement.offset_frames().as_f64(), Some(-1.25));
    assert_eq!(n.placement.asset_id().as_str(), "source-a");
    assert_eq!(
        n.placement.target(),
        &AssetTarget::AudioTrack {
            track_uid: "track-a".into()
        }
    );
    assert_eq!(
        n.source_wire,
        json!({"id":"source-a","name":"source-a","kind":"audio",
        "source":{"kind":"embedded","mediaType":"audio/wav","encoding":"base64","data":"UklGRg=="}})
    );
    // This short payload is deliberately NOT a valid admitted WAV. The producer
    // constructs a candidate; the existing managed codec owns byte validation.
}
#[test]
fn authored_missing_null_empty_and_zero_remain_distinct() {
    let mut minimal = input();
    for key in [
        "name",
        "volume",
        "muted",
        "trimStart",
        "trimEnd",
        "motion",
        "motionStatic",
    ] {
        minimal.as_object_mut().unwrap().remove(key);
    }
    let n = normalize(&minimal).unwrap();
    assert_eq!(
        serde_json::to_value(n.record).unwrap(),
        json!({"assetId":"source-a","audioId":"track-a"})
    );
    let mut nullable = input();
    nullable["trimEnd"] = Value::Null;
    nullable["motionStatic"] = Value::Null;
    let n = normalize(&nullable).unwrap();
    let metadata = serde_json::to_value(n.record).unwrap();
    assert_eq!(metadata["trimEnd"], Value::Null);
    assert_eq!(metadata["motionStatic"], Value::Null);
    assert_eq!(metadata["volume"], 0);
    assert_eq!(metadata["name"], "");
}
#[test]
fn absent_or_invalid_original_audio_id_is_unavailable_without_minting() {
    for id in [
        None,
        Some(json!("")),
        Some(json!(" ")),
        Some(json!("bad\u{0}id")),
    ] {
        let mut value = input();
        value.as_object_mut().unwrap().remove("audioId");
        if let Some(id) = id {
            value["audioId"] = id;
        }
        assert!(matches!(
            normalize(&value),
            Err(AudioBaselineError::Unavailable)
        ));
    }
}
#[test]
fn unsupported_original_formats_remain_unavailable_and_never_get_relabeled_as_wave() {
    for source in [
        "data:audio/mpeg;base64,AAAA",
        "data:audio/ogg;base64,AAAA",
        "AAAA",
        "https://example.test/a.wav",
    ] {
        let mut value = input();
        value["dataB64"] = json!(source);
        assert!(matches!(
            normalize(&value),
            Err(AudioBaselineError::Unavailable)
        ));
    }
    let mut empty = input();
    empty["dataB64"] = json!("data:audio/wav;base64,");
    assert!(matches!(
        normalize(&empty),
        Err(AudioBaselineError::Invalid)
    ));
}
#[test]
fn strict_original_input_rejects_duplicate_fields_motion_properties_and_unknown_fields() {
    for raw in [
        r#"{"dataB64":"data:audio/wav;base64,AAAA","audioId":"a","audioId":"b","offsetFrames":0}"#,
        r#"{"dataB64":"data:audio/wav;base64,AAAA","audioId":"a","offsetFrames":0,"motion":{"volume":{"keys":[]},"volume":{"keys":[]}}}"#,
        r#"{"dataB64":"data:audio/wav;base64,AAAA","audioId":"a","offsetFrames":0,"motionStatic":{"volume":[1],"volume":[0]}}"#,
        r#"{"dataB64":"data:audio/wav;base64,AAAA","audioId":"a","offsetFrames":0,"unknown":true}"#,
    ] {
        assert!(matches!(
            normalize_baseline_track(raw.as_bytes(), "source-a", "place-a", 4096),
            Err(AudioBaselineError::Invalid)
        ));
    }
}
#[test]
fn input_size_and_placement_bounds_refuse_before_creating_an_owner() {
    let bytes = input().to_string();
    for cap in [0, bytes.len() - 1] {
        assert!(matches!(
            normalize_baseline_track(bytes.as_bytes(), "source-a", "place-a", cap),
            Err(AudioBaselineError::Limit)
        ));
    }
    assert!(normalize_baseline_track(bytes.as_bytes(), "source-a", "place-a", bytes.len()).is_ok());
    let mut value = input();
    value["offsetFrames"] = json!(9007199254740992_u64);
    assert!(matches!(
        normalize(&value),
        Err(AudioBaselineError::Invalid)
    ));
    assert!(matches!(
        normalize_baseline_track(bytes.as_bytes(), "", "place-a", 4096),
        Err(AudioBaselineError::Invalid)
    ));
}

#[test]
fn positional_struct_array_and_other_top_level_shapes_refuse_on_original_bytes() {
    // Every field is type-correct, and the source is a complete two-frame PCM16 WAV.
    // Derived visit_seq could otherwise admit this exact positional baseline.
    const ARRAY: &str = r#"["data:audio/wav;base64,UklGRiwAAABXQVZFZm10IBAAAAABAAIAgLsAAADuAgAEABAAZGF0YQgAAAAAAP9/AIABAA==",0,"title","track-a",1,false,0,null,null,null]"#;
    for raw in [ARRAY, "[]", "null", "true", "0", r#""text""#, " "] {
        assert!(
            matches!(
                normalize_baseline_track(raw.as_bytes(), "source-a", "place-a", 4096),
                Err(AudioBaselineError::Invalid)
            ),
            "wrong top-level shape: {raw}"
        );
    }
    let object = r#" 
 {"dataB64":"data:audio/wav;base64,UklGRiwAAABXQVZFZm10IBAAAAABAAIAgLsAAADuAgAEABAAZGF0YQgAAAAAAP9/AIABAA==","offsetFrames":0,"name":"title","audioId":"track-a","volume":1,"muted":false,"trimStart":0,"trimEnd":null,"motion":null,"motionStatic":null}"#;
    let admitted =
        normalize_baseline_track(object.as_bytes(), "source-a", "place-a", 4096).unwrap();
    assert_eq!(
        admitted.record.audio_id.value().map(String::as_str),
        Some("track-a")
    );
    assert_eq!(admitted.placement.offset_frames().as_i64(), Some(0));
}
