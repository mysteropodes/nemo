//! T12: the stored video timing samples like `SMMotion.timeRemapValue`
//! (`evalTrack`, motion.js) and admits exactly what a component remap admits.
use super::*;

fn track(json: &str) -> OrderTrack {
    serde_json::from_str(json).unwrap()
}
fn remap(json: &str) -> VideoTiming {
    VideoTiming {
        time_remap: Some(track(json)),
        ..VideoTiming::default()
    }
}
const LINEAR: &str = r#""curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]"#;

#[test]
fn no_track_or_no_keys_is_the_samplers_null() {
    assert_eq!(VideoTiming::default().remap_value(3.0), None);
    assert!(!VideoTiming::default().has_time_remap());
    let empty = remap(r#"{"keys":[]}"#);
    assert!(empty.has_time_remap() && empty.admitted());
    assert_eq!(empty.remap_value(3.0), None);
}

#[test]
fn ends_clamp_to_the_first_and_last_key_values() {
    let timing = remap(r#"{"keys":[{"frame":4,"v":[10]},{"frame":12,"v":[2]}]}"#);
    assert_eq!(timing.remap_value(-50.0), Some(10.0));
    assert_eq!(timing.remap_value(4.0), Some(10.0));
    assert_eq!(timing.remap_value(12.0), Some(2.0));
    assert_eq!(timing.remap_value(99.0), Some(2.0));
}

#[test]
fn default_ease_and_authored_curves_follow_the_baseline_table() {
    let eased = remap(r#"{"keys":[{"frame":0,"v":[0]},{"frame":8,"v":[16]}]}"#);
    // The default table maps 0.25 -> 0.156 and 0.5 -> 0.5.
    assert_eq!(eased.remap_value(4.0), Some(8.0));
    assert!((eased.remap_value(2.0).unwrap() - 16.0 * 0.156).abs() < 1e-9);
    let linear = remap(&format!(
        r#"{{"keys":[{{"frame":0,"v":[0],{LINEAR}}},{{"frame":8,"v":[16]}}]}}"#
    ));
    assert!((linear.remap_value(2.0).unwrap() - 4.0).abs() < 1e-9);
    assert!((linear.remap_value(6.5).unwrap() - 13.0).abs() < 1e-9);
}

#[test]
fn hold_pins_the_outgoing_key_until_the_next_key() {
    let timing = remap(
        r#"{"keys":[{"frame":0,"v":[3],"hold":true},{"frame":10,"v":[9]},{"frame":20,"v":[1]}]}"#,
    );
    assert_eq!(timing.remap_value(0.0), Some(3.0));
    assert_eq!(timing.remap_value(9.99), Some(3.0));
    assert_eq!(timing.remap_value(10.0), Some(9.0));
    assert_eq!(timing.remap_value(15.0), Some(5.0));
}

#[test]
fn admission_refuses_disordered_tracks_and_malformed_modes() {
    assert!(VideoTiming::default().admitted() && VideoTiming::default().is_empty());
    assert!(!remap(r#"{"keys":[{"frame":5,"v":[0]},{"frame":5,"v":[1]}]}"#).admitted());
    assert!(!remap(r#"{"keys":[{"frame":0,"v":[0,1]}]}"#).admitted());
    for (mode, ok) in [
        ("blend", true),
        ("motion", true),
        ("custom", true),
        ("", false),
        ("a\0b", false),
        (&"m".repeat(MAX_INTERP_MODE), true),
        (&"m".repeat(MAX_INTERP_MODE + 1), false),
    ] {
        let timing = VideoTiming {
            interp_mode: Some(mode.into()),
            ..VideoTiming::default()
        };
        assert_eq!(timing.admitted(), ok, "mode {mode:?}");
        assert!(!timing.is_empty());
    }
    let blend = VideoTiming {
        frame_blend: Some(false),
        ..VideoTiming::default()
    };
    assert!(blend.admitted() && !blend.is_empty() && !blend.has_time_remap());
}
