//! #2357 G44: the native After Effects camera script against the REAL baseline `exportAECamera`
//! (src/js/ae-camera-export.js @1065e9d8, run in a Node vm by gen-ae-camera-oracle.cjs). Each case feeds the same
//! keys, fps and canvas width to both writers; the generated JSX must be byte-identical.
use super::{
    build_ae_camera_script, build_ae_camera_script_for, js_number, AeCameraExportError,
    NO_ACTIVE_CAMERA,
};
use crate::object_document::ProjectCameraKey;
use serde_json::{json, Value};

const ORACLE: &str = include_str!("fixtures/native-ae-camera-export/oracle.json");

fn keys(value: &Value) -> Vec<ProjectCameraKey> {
    serde_json::from_value(value.clone()).expect("camera keys")
}

#[test]
fn every_oracle_case_matches_the_baseline_writer() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["commit"].as_str().unwrap().len(), 40);
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let k = keys(&case["keys"]);
        let got = build_ae_camera_script(
            Some(&k),
            Some(case["layerOn"].as_bool().unwrap()),
            case["fps"].as_f64().unwrap(),
            case["canvasW"].as_u64().unwrap() as u32,
        );
        let expected = &case["expected"];
        if expected["ok"] == true {
            let out = got.unwrap_or_else(|e| panic!("{id}: {e:?}"));
            assert_eq!(out.script, expected["script"].as_str().unwrap(), "{id}");
            assert_eq!(out.key_count as u64, expected["keyCount"], "{id}");
        } else {
            assert_eq!(
                got,
                Err(AeCameraExportError::Refused(NO_ACTIVE_CAMERA)),
                "{id}"
            );
            assert_eq!(expected["error"], NO_ACTIVE_CAMERA, "{id}");
        }
    }
}

#[test]
fn key_counts_rotation_and_time_follow_the_baseline_rules() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let by_id = |id: &str| {
        oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()["expected"]["script"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let rotation = by_id("rotation-zero-negative-zero-and-absent");
    assert!(rotation.contains("rotZ.setValuesAtTimes(times, [0,0,0,-45.125]);"));
    let unsorted = by_id("unsorted-keys");
    assert!(unsorted.contains("var times = [0,1,2];"));
    assert!(unsorted.contains("Zoom\").setValue(1280);"));
    assert!(by_id("many-keys").contains("setValuesAtTimes(times, [[960,540,0]"));
}

#[test]
fn refusals_and_missing_state_are_typed() {
    let one = keys(&json!([{"frame":0,"x":1,"y":2,"w":3}]));
    assert_eq!(
        build_ae_camera_script(None, None, 24.0, 100),
        Err(AeCameraExportError::Unavailable)
    );
    for (k, on) in [
        (Some(&one[..]), Some(false)),
        (Some(&one[..]), None),
        (Some(&[][..]), Some(true)),
        (None, Some(true)),
    ] {
        assert_eq!(
            build_ae_camera_script(k, on, 24.0, 100),
            Err(AeCameraExportError::Refused(NO_ACTIVE_CAMERA))
        );
    }
    let huge = keys(&json!([{"frame":1e308,"x":0,"y":0,"w":1}]));
    assert_eq!(
        build_ae_camera_script(Some(&huge), Some(true), 1e-300, 100),
        Err(AeCameraExportError::NotFinite)
    );
}

#[test]
fn negative_zero_prints_like_javascript() {
    let k = keys(&json!([{"frame":0,"x":-0.0,"y":0,"w":0,"rot":-0.0}]));
    let out = build_ae_camera_script(Some(&k), Some(true), 24.0, 10).unwrap();
    assert!(out
        .script
        .contains("pos.setValuesAtTimes(times, [[0,0,0]]);"));
    assert!(out.script.contains("rotZ.setValuesAtTimes(times, [0]);"));
}

#[test]
fn js_number_matches_javascript_string_conversion() {
    for (x, text) in [
        (0.0, "0"),
        (-0.0, "0"),
        (1.0, "1"),
        (-2.5, "-2.5"),
        (0.000001, "0.000001"),
        (1e-7, "1e-7"),
        (1.5e-10, "1.5e-10"),
        (123456789012345680000.0, "123456789012345680000"),
        (1e21, "1e+21"),
        (1.25e22, "1.25e+22"),
        (-1e21, "-1e+21"),
        (0.1 + 0.2, "0.30000000000000004"),
        (1.0 / 24.0, "0.041666666666666664"),
        (2e300, "2e+300"),
    ] {
        assert_eq!(js_number(x), text, "{x:e}");
    }
}

#[test]
fn script_for_a_pinned_project_reads_its_stored_camera() {
    use crate::object_document::ProjectMetadata;
    let k = keys(&json!([{"frame":24,"x":10,"y":20,"w":30,"rot":5}]));
    let project = ProjectMetadata::new(1920, 1080, 24.0)
        .unwrap()
        .with_camera(Some(k), Some(true))
        .unwrap();
    let out = build_ae_camera_script_for(&project).unwrap();
    assert_eq!(out.key_count, 1);
    assert!(out.script.contains("var times = [1];"));
    assert!(out.script.contains("setValue(1920);"));
    let none = ProjectMetadata::new(1920, 1080, 24.0).unwrap();
    assert_eq!(
        build_ae_camera_script_for(&none),
        Err(AeCameraExportError::Unavailable)
    );
}
