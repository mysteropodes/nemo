//! Record-only DTO/metadata controls, authored UNRUN. These do not establish
//! whole-project codec, scalar-curve validation, source admission or ownership.
use super::*;
use serde_json::{json, Value};

fn record(value: Value) -> AudioTrackRecord {
    serde_json::from_value(value).unwrap()
}
fn tracks(value: Value) -> AudioTracks {
    serde_json::from_value(value).unwrap()
}
fn metadata_only(value: &AudioTracks) -> Result<(), &'static str> {
    // These fixtures contain no actual Motion tracks. No fake scalar acceptance.
    value.validate::<()>(|_| panic!("metadata-only fixture reached scalar validator"))
}

#[test]
fn absent_fields_stay_absent_without_invented_defaults_or_duplicate_authorities() {
    let input = json!({"assetId":"source-a"});
    let parsed = record(input.clone());
    assert!(parsed.audio_id.missing());
    assert!(parsed.name.missing());
    assert!(parsed.volume.missing());
    assert!(parsed.muted.missing());
    assert!(parsed.trim_start.missing());
    assert!(parsed.trim_end.missing());
    assert!(parsed.motion.missing());
    assert!(parsed.motion_static.missing());
    assert_eq!(serde_json::to_value(parsed).unwrap(), input);
}

#[test]
fn explicit_null_empty_and_zero_remain_distinct_authored_values() {
    for optional in [
        json!({"trimEnd":null,"motion":null,"motionStatic":null}),
        json!({"trimEnd":0,"motion":{},"motionStatic":{}}),
        json!({"trimStart":0,"volume":0,"muted":false,"name":""}),
    ] {
        let mut input = json!({"assetId":"source-a"});
        input
            .as_object_mut()
            .unwrap()
            .extend(optional.as_object().unwrap().clone());
        let parsed = record(input.clone());
        let rows = tracks(json!([input.clone()]));
        assert_eq!(metadata_only(&rows), Ok(()));
        assert_eq!(serde_json::to_value(parsed).unwrap(), input);
    }
    let nulls =
        record(json!({"assetId":"source-a","trimEnd":null,"motion":null,"motionStatic":null}));
    assert_eq!(nulls.trim_end, Presence::Present(None));
    assert_eq!(nulls.motion, Presence::Present(None));
    assert_eq!(nulls.motion_static, Presence::Present(None));
}

#[test]
fn full_motion_metadata_round_trips_without_claiming_codec_scalar_acceptance() {
    let input = json!({"assetId":"source-a","audioId":"audio:α","name":"声.wav",
        "volume":-0.25,"muted":true,"trimStart":-0.125,"trimEnd":1.75,
        "motion":{"volume":{"keys":[
            {"frame":-0.5,"v":[-0.25],"hold":false,"lockTo":null,
             "curvePoints":[{"x":0,"y":0,"tx":0,"ty":0},{"x":1,"y":1}],
             "hIn":[0,0],"hOut":[0.25,-0.5]},
            {"frame":3.25,"v":[1.5],"hold":true,"curvePoints":null},
            {"frame":6.5,"v":[0],"curvePoints":[]}
        ]}},"motionStatic":{"volume":[1.25]}});
    let parsed = record(input.clone());
    assert_eq!(serde_json::to_value(parsed.clone()).unwrap(), input);
    assert_eq!(parsed.volume.value().unwrap().as_f64(), Some(-0.25));
    assert_eq!(parsed.trim_start.value().unwrap().as_f64(), Some(-0.125));
    let reparsed: AudioTrackRecord =
        serde_json::from_slice(&serde_json::to_vec(&parsed).unwrap()).unwrap();
    assert_eq!(reparsed, parsed);
}

#[test]
fn scalar_null_wrong_types_unknown_fields_and_duplicate_members_are_wire_refusals() {
    for field in ["assetId", "audioId", "name", "volume", "muted", "trimStart"] {
        let mut value = json!({"assetId":"source-a"});
        value[field] = Value::Null;
        assert!(
            serde_json::from_value::<AudioTrackRecord>(value).is_err(),
            "{field}"
        );
    }
    for extra in [
        json!({"offsetFrames":-0.5}),
        json!({"dataB64":"data:audio/wav;base64,AA=="}),
        json!({"_buffer":{}}),
        json!({"unexpected":true}),
    ] {
        let mut value = json!({"assetId":"source-a"});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(serde_json::from_value::<AudioTrackRecord>(value).is_err());
    }
    for raw in [
        r#"{"assetId":"source-a","assetId":"source-b"}"#,
        r#"{"assetId":"source-a","audioId":"a","audioId":"b"}"#,
        r#"{"assetId":"source-a","volume":0,"volume":1}"#,
        r#"{"assetId":"source-a","motion":{"volume":{"keys":[],"keys":[]}}}"#,
        r#"{"name":"missing source"}"#,
        r#"{"assetId":"source-a","volume":"1"}"#,
        r#"{"assetId":"source-a","muted":1}"#,
        r#"{"assetId":"source-a","trimEnd":"0"}"#,
        r#"{"assetId":"source-a","volume":1e400}"#,
    ] {
        assert!(
            serde_json::from_str::<AudioTrackRecord>(raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn metadata_validates_real_id_rules_but_does_not_mint_missing_identity() {
    for id in ["", "   ", "bad\0id"] {
        assert_eq!(
            metadata_only(&tracks(json!([{"assetId":id}]))),
            Err("invalid audio source id")
        );
        assert_eq!(
            metadata_only(&tracks(json!([{"assetId":"source-a","audioId":id}]))),
            Err("invalid or duplicate audio id")
        );
    }
    let too_long = "x".repeat(256);
    assert!(!valid_audio_id(&too_long));
    assert!(valid_audio_id(&"x".repeat(255)));
    assert!(valid_audio_id("audio:声 / original"));
    let input = json!([{"assetId":"source-a"},{"assetId":"source-a"}]);
    let parsed = tracks(input.clone());
    assert_eq!(metadata_only(&parsed), Ok(()));
    assert!(parsed.get("invented-id").is_none());
    assert_eq!(serde_json::to_value(parsed).unwrap(), input);
}

#[test]
fn duplicate_audio_identity_is_refused_while_same_source_and_name_are_preserved() {
    let duplicate = tracks(json!([
        {"assetId":"source-a","audioId":"same","name":"same"},
        {"assetId":"source-b","audioId":"same","name":"same"}
    ]));
    assert_eq!(
        metadata_only(&duplicate),
        Err("invalid or duplicate audio id")
    );
    let input = json!([
        {"assetId":"shared-source","audioId":"A","name":"same","trimStart":0,"trimEnd":0.5},
        {"assetId":"shared-source","audioId":"B","name":"same","trimStart":0.5,"trimEnd":null}
    ]);
    let distinct = tracks(input.clone());
    assert_eq!(metadata_only(&distinct), Ok(()));
    assert_eq!(distinct.records().len(), 2);
    assert_eq!(
        distinct
            .get("B")
            .unwrap()
            .trim_start
            .value()
            .unwrap()
            .as_f64(),
        Some(0.5)
    );
    assert_eq!(serde_json::to_value(distinct).unwrap(), input);
}

#[test]
fn name_bound_is_bytes_and_static_scalar_values_are_not_clamped() {
    for name in [
        "".to_owned(),
        "   ".to_owned(),
        "声".to_owned(),
        "n".repeat(255),
    ] {
        assert_eq!(
            metadata_only(&tracks(json!([{"assetId":"a","name":name}]))),
            Ok(())
        );
    }
    for name in ["bad\0name".to_owned(), "n".repeat(256), "声".repeat(86)] {
        assert_eq!(
            metadata_only(&tracks(json!([{"assetId":"a","name":name}]))),
            Err("invalid audio name")
        );
    }
    let input = json!([{"assetId":"a","volume":2.5,"trimStart":-0.5,"trimEnd":-0.25,
        "motionStatic":{"volume":[-1.25]}}]);
    let parsed = tracks(input.clone());
    assert_eq!(metadata_only(&parsed), Ok(()));
    assert_eq!(serde_json::to_value(parsed).unwrap(), input);
}

#[test]
fn unknown_motion_property_and_wrong_static_dimension_refuse_without_scalar_callback() {
    let unknown = tracks(json!([{"assetId":"a","motion":{"opacity":{"keys":[]}}}]));
    assert_eq!(
        metadata_only(&unknown),
        Err("unsupported audio Motion property")
    );
    for values in [
        json!({"volume":[]}),
        json!({"volume":[0,1]}),
        json!({"opacity":[1]}),
    ] {
        let parsed = tracks(json!([{"assetId":"a","motionStatic":values}]));
        assert_eq!(metadata_only(&parsed), Err("invalid audio static volume"));
    }
}

#[test]
fn scalar_validator_is_forwarded_exactly_and_its_failure_is_not_ignored() {
    let input = json!([{"assetId":"a","motion":{"volume":{"keys":[{"frame":1.25,"v":[0.75]}]}}}]);
    let parsed = tracks(input.clone());
    let mut calls = 0;
    let result = parsed.validate(|actual| {
        calls += 1;
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            input[0]["motion"]["volume"]
        );
        Err::<(), _>("deliberate codec-validator refusal")
    });
    assert_eq!(result, Err("invalid audio volume track"));
    assert_eq!(calls, 1);
    assert_eq!(serde_json::to_value(parsed).unwrap(), input);
}

#[test]
fn transparent_track_table_preserves_empty_and_enforces_its_declared_count_bound() {
    let empty = tracks(json!([]));
    assert!(empty.records().is_empty());
    assert_eq!(serde_json::to_value(empty).unwrap(), json!([]));
    let at_limit = vec![json!({"assetId":"shared"}); 1024];
    assert_eq!(tracks(json!(at_limit)).records().len(), 1024);
    let over_limit = vec![json!({"assetId":"shared"}); 1025];
    assert!(serde_json::from_value::<AudioTracks>(json!(over_limit)).is_err());
    for wrong in [Value::Null, json!({}), json!([null])] {
        assert!(serde_json::from_value::<AudioTracks>(wrong).is_err());
    }
}

#[test]
fn raw_duplicate_motion_and_static_volume_names_are_refused_before_map_collapse() {
    // Parse the original strings directly: a prebuilt Value would already
    // discard the duplicate property and could not exercise this admission.
    for raw in [
        r#"{"assetId":"a","motion":{"volume":{"keys":[{"frame":0,"v":[0]}]},"volume":{"keys":[{"frame":1,"v":[1]}]}}}"#,
        r#"{"assetId":"a","motionStatic":{"volume":[0.25],"volume":[0.75]}}"#,
    ] {
        assert!(
            serde_json::from_str::<AudioTrackRecord>(raw).is_err(),
            "{raw}"
        );
    }
}
