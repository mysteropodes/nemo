//! Stored-record sampling via actual native curve math and VolumeSampler.
//! Authored UNRUN. No synthetic holder, sampler implementation or resource owner.
use super::*;
use serde_json::{json, Value};

fn track(value: Value) -> AudioTrackRecord {
    serde_json::from_value(value).unwrap()
}
fn close(actual: Option<f64>, expected: f64) {
    let value = actual.expect("stored scalar should resolve");
    assert!(
        (value - expected).abs() < 1e-12,
        "got {value}, expected {expected}"
    );
}
fn linear_keys() -> Value {
    json!({"keys":[
        {"frame":-0.5,"v":[-0.25],"curvePoints":[
            {"x":0,"y":0,"tx":1,"ty":1},{"x":1,"y":1,"tx":1,"ty":1}]},
        {"frame":3.5,"v":[1.75]}
    ]})
}
fn animated(keys: Value) -> AudioTrackRecord {
    track(json!({"assetId":"source-a","audioId":"track-a","volume":99,
        "motionStatic":{"volume":[88]},"motion":{"volume":keys}}))
}

#[test]
fn stored_baseline_precedence_is_animated_then_motion_static_then_legacy_then_one() {
    let fallback = track(json!({"assetId":"a"}));
    close(fallback.sample_volume(0.5), 1.0);
    let legacy = track(json!({"assetId":"a","volume":0}));
    close(legacy.sample_volume(0.5), 0.0);
    let static_value = track(json!({"assetId":"a","volume":0.9,"motionStatic":{"volume":[0.25]}}));
    close(static_value.sample_volume(0.5), 0.25);
    let keyed = animated(linear_keys());
    close(keyed.sample_volume(0.5), 0.25); // 1/4 of the authored four-frame interval.
}

#[test]
fn null_empty_and_empty_key_motion_fall_back_without_rewriting_authored_presence() {
    for optional in [
        json!({"motion":null,"motionStatic":null}),
        json!({"motion":{},"motionStatic":{}}),
        json!({"motion":{"volume":{"keys":[]}},"motionStatic":null}),
    ] {
        let mut input = json!({"assetId":"a","volume":-0.75,"trimEnd":null});
        input
            .as_object_mut()
            .unwrap()
            .extend(optional.as_object().unwrap().clone());
        let parsed = track(input.clone());
        close(parsed.sample_volume(0.125), -0.75);
        assert_eq!(serde_json::to_value(parsed).unwrap(), input);
    }
    let input = json!({"assetId":"a","volume":0.9,"motion":{"volume":{"keys":[]}},
        "motionStatic":{"volume":[0]},"trimEnd":0});
    let parsed = track(input.clone());
    close(parsed.sample_volume(1.25), 0.0);
    assert_eq!(serde_json::to_value(parsed).unwrap(), input);
}

#[test]
fn flat_legacy_and_static_scalar_volume_remain_unclamped() {
    for input in [
        json!({"assetId":"a","volume":-0.25}),
        json!({"assetId":"a","motionStatic":{"volume":[-0.25]}}),
    ] {
        close(track(input).sample_volume(2.0), -0.25);
    }
    for input in [
        json!({"assetId":"a","volume":1.75}),
        json!({"assetId":"a","volume":0,"motionStatic":{"volume":[1.75]}}),
    ] {
        close(track(input).sample_volume(2.0), 1.75);
    }
}

#[test]
fn fractional_keys_values_and_linear_curve_have_literal_independent_results() {
    let parsed = animated(linear_keys());
    let sampler: &dyn VolumeSampler = &parsed;
    for (frame, expected) in [
        (-100., -0.25),
        (-0.5, -0.25),
        (0.5, 0.25),
        (1.5, 0.75),
        (2.5, 1.25),
        (3.5, 1.75),
        (100., 1.75),
    ] {
        close(parsed.sample_volume(frame), expected);
        close(sampler.volume_at(frame), expected);
    }
}

#[test]
fn hold_key_keeps_its_value_until_exact_next_fractional_key() {
    let mut keys = linear_keys();
    keys["keys"][0]["hold"] = json!(true);
    let parsed = animated(keys);
    for (frame, expected) in [
        (-1., -0.25),
        (-0.5, -0.25),
        (0.5, -0.25),
        (3.499, -0.25),
        (3.5, 1.75),
        (4., 1.75),
    ] {
        close(parsed.sample_volume(frame), expected);
    }
}

#[test]
fn explicit_flat_tangents_use_actual_native_cubic_with_known_quarter_values() {
    let parsed = animated(json!({"keys":[
        {"frame":0,"v":[0],"curvePoints":[
            {"x":0,"y":0,"tx":1,"ty":0},{"x":1,"y":1,"tx":1,"ty":0}]},
        {"frame":4,"v":[2]}
    ]}));
    // With linear x and zero y endpoint slopes, quarter/mid/three-quarter
    // eased values are exactly 0.15625, 0.5 and 0.84375; the value span is 2.
    close(parsed.sample_volume(1.0), 0.3125);
    close(parsed.sample_volume(2.0), 1.0);
    close(parsed.sample_volume(3.0), 1.6875);
}

#[test]
fn default_curve_missing_or_null_keeps_symmetric_midpoint_and_empty_curve_is_linear() {
    for curve in [None, Some(Value::Null), Some(json!([]))] {
        let mut first = json!({"frame":0,"v":[-0.5]});
        if let Some(value) = curve {
            first["curvePoints"] = value;
        }
        let parsed = animated(json!({"keys":[first,{"frame":4,"v":[1.5]}]}));
        close(parsed.sample_volume(2.0), 0.5);
    }
    let parsed = animated(json!({"keys":[
        {"frame":0,"v":[-0.5],"curvePoints":[]},{"frame":4,"v":[1.5]}
    ]}));
    close(parsed.sample_volume(1.0), 0.0);
    close(parsed.sample_volume(3.0), 1.0);
}

#[test]
fn single_stored_key_and_multiple_segments_use_the_actual_record() {
    let single = animated(json!({"keys":[{"frame":2.25,"v":[1.125]}]}));
    for at in [-10., 2.25, 20.] {
        close(single.sample_volume(at), 1.125);
    }
    let segmented = animated(json!({"keys":[
        {"frame":0,"v":[0],"hold":true},
        {"frame":2,"v":[1],"curvePoints":[]},{"frame":4,"v":[2]}
    ]}));
    for (at, expected) in [(1., 0.), (2., 1.), (3., 1.5), (4., 2.)] {
        close(segmented.sample_volume(at), expected);
    }
}

#[test]
fn nonfinite_query_refuses_even_when_a_flat_fallback_exists() {
    let parsed = track(json!({"assetId":"a","volume":0.5}));
    let sampler: &dyn VolumeSampler = &parsed;
    for at in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(parsed.sample_volume(at), None);
        assert_eq!(sampler.volume_at(at), None);
    }
}

#[test]
fn defensive_scalar_dimension_refusal_does_not_masquerade_as_static_fallback() {
    // Deliberately unadmitted DTOs exercise defensive sampling only; the real
    // codec's scalar-validator acceptance/refusal remains a parent-owned test.
    let bad_keys = animated(json!({"keys":[{"frame":0,"v":[0,1]}]}));
    assert_eq!(bad_keys.sample_volume(0.), None);
    for values in [json!([]), json!([0, 1])] {
        let parsed = track(json!({"assetId":"a","volume":0.5,"motionStatic":{"volume":values}}));
        assert_eq!(parsed.sample_volume(0.), None);
    }
}

#[test]
fn sampling_and_trait_calls_leave_full_serialized_record_unchanged() {
    let input = json!({"assetId":"source-a","audioId":"track-a","name":"voice",
        "trimStart":0.125,"trimEnd":null,"muted":true,"volume":0,
        "motionStatic":{"volume":[-0.5]},"motion":{"volume":linear_keys()}});
    let parsed = track(input.clone());
    let sampler: &dyn VolumeSampler = &parsed;
    for at in [-1., 0.5, 1.5, 3.5, 8.] {
        let _ = sampler.volume_at(at);
    }
    assert_eq!(serde_json::to_value(parsed).unwrap(), input);
}
