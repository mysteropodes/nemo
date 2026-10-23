//! Composition metadata oracles through the immutable model and existing owners.
use super::{ProjectMetadata, ProjectMetadataError};

#[test]
fn composition_presence_order_spelling_and_finite_domains_are_storage_values() {
    let original = ProjectMetadata::new(320, 180, 24.0).unwrap();
    assert_eq!(original.composition_markers(), None);
    assert_eq!(original.composition_bpm(), (None, None, None));
    let markers = vec![
        (20.0, Some("X".into()), Some("".into())),
        (0.0, Some("X".into()), None),
        (-2.5, None, Some("MiXeD".into())),
    ];
    let value = original
        .clone()
        .with_composition_metadata(
            Some(markers.clone()),
            Some(-140.25),
            Some(-7.5),
            Some(false),
        )
        .unwrap();
    assert_eq!(value.composition_markers(), Some(markers.as_slice()));
    assert_eq!(
        value.composition_bpm(),
        (Some(-140.25), Some(-7.5), Some(false))
    );
    assert_eq!(original.composition_markers(), None);
    let empty = original
        .clone()
        .with_composition_metadata(Some(vec![]), None, None, None)
        .unwrap();
    assert_ne!(original, empty);
    assert_eq!(empty.composition_markers(), Some(&[][..]));
    for finite in [-f64::MAX, -0.0, 0.0, 0.5, f64::MAX] {
        let value = original
            .clone()
            .with_composition_metadata(
                Some(vec![(finite, None, None)]),
                Some(finite),
                Some(finite),
                Some(true),
            )
            .unwrap();
        assert_eq!(value.composition_markers().unwrap()[0].0, finite);
        assert_eq!(
            value.composition_bpm(),
            (Some(finite), Some(finite), Some(true))
        );
    }
}

#[test]
fn composition_model_bounds_are_utf8_bytes_and_refuse_without_changing_old_value() {
    let old = ProjectMetadata::new(320, 180, 24.0).unwrap();
    let full = vec![(0.0, Some("é".repeat(512)), Some("z".repeat(1024))); 4096];
    assert!(old
        .clone()
        .with_composition_metadata(Some(full.clone()), None, None, None)
        .is_ok());
    let mut too_many = full;
    too_many.push((0.0, None, None));
    assert_eq!(
        old.clone()
            .with_composition_metadata(Some(too_many), None, None, None)
            .unwrap_err(),
        ProjectMetadataError::InvalidCompositionMetadata
    );
    for text in ["é".repeat(513), "z".repeat(1025), "bad\0name".into()] {
        for (name, color) in [(Some(text.clone()), None), (None, Some(text.clone()))] {
            assert_eq!(
                old.clone()
                    .with_composition_metadata(Some(vec![(0.0, name, color)]), None, None, None)
                    .unwrap_err(),
                ProjectMetadataError::InvalidCompositionMetadata
            );
        }
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (markers, bpm, offset) in [
            (Some(vec![(bad, None, None)]), None, None),
            (None, Some(bad), None),
            (None, None, Some(bad)),
        ] {
            assert_eq!(
                old.clone()
                    .with_composition_metadata(markers, bpm, offset, None)
                    .unwrap_err(),
                ProjectMetadataError::InvalidCompositionMetadata
            );
        }
    }
    assert_eq!(old.composition_markers(), None);
    assert_eq!(old.composition_bpm(), (None, None, None));
}

#[cfg(feature = "project_media_codec")]
mod storage {
    use crate::object_codec::project_settings_test_codec::{
        decode_project, encode_project, ProjectWireLimits,
    };
    use serde_json::{json, Value};
    pub(super) fn wire(version: u32, project: Value) -> Value {
        let mut wire = json!({"format":"nemo.native-project-document","formatVersion":version,
            "project":project,"drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":21,"layers":[{"layerUid":"L"}],"objects":[]},"viewportReference":null,"assets":[]});
        if version == 2 {
            wire["assetInventory"] = json!({"assetIds":[],"placements":[]});
        }
        wire
    }
    fn limits() -> ProjectWireLimits {
        ProjectWireLimits::new(32 * 1024 * 1024, 8 * 1024 * 1024).unwrap()
    }
    #[test]
    fn composition_legacy_and_catalog_roundtrip_distinguishes_every_optional_presence() {
        for version in [1, 2] {
            for extras in [
                json!({}),
                json!({"markers":[]}),
                json!({"bpm":0.0}),
                json!({"bpmOffset":-2.5}),
                json!({"bpmShow":false}),
                json!({"markers":[{"frame":20.0,"name":"X","color":""},{"frame":0.0,"name":"X"},
                    {"frame":-2.5,"name":"","color":"MiXeD"}],"bpm":140.0,"bpmOffset":0.0,"bpmShow":true}),
            ] {
                let mut project = json!({"canvasW":320,"canvasH":180,"fps":24.0});
                project
                    .as_object_mut()
                    .unwrap()
                    .extend(extras.as_object().unwrap().clone());
                let expected = wire(version, project);
                let admitted =
                    decode_project(&serde_json::to_vec(&expected).unwrap(), limits()).unwrap();
                let bytes = encode_project(&admitted, limits()).unwrap();
                assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
                assert_eq!(decode_project(&bytes, limits()).unwrap(), admitted);
            }
        }
    }
    #[test]
    fn composition_storage_rejects_null_unknown_duplicate_array_and_nonfinite_members() {
        for version in [1, 2] {
            for bad in [
                json!({"markers":null}),
                json!({"markers":{}}),
                json!({"markers":[[]]}),
                json!({"markers":[{"frame":0,"extra":1}]}),
                json!({"markers":[{"name":"missing"}]}),
                json!({"markers":[{"frame":0,"name":null}]}),
                json!({"markers":[{"frame":0,"color":false}]}),
                json!({"markers":[{"frame":"1"}]}),
                json!({"bpm":null}),
                json!({"bpm":true}),
                json!({"bpmOffset":null}),
                json!({"bpmOffset":"0"}),
                json!({"bpmShow":null}),
                json!({"bpmShow":0}),
            ] {
                let mut project = json!({"canvasW":320,"canvasH":180,"fps":24.0});
                project
                    .as_object_mut()
                    .unwrap()
                    .extend(bad.as_object().unwrap().clone());
                assert!(decode_project(
                    &serde_json::to_vec(&wire(version, project)).unwrap(),
                    limits()
                )
                .is_err());
            }
            for suffix in [
                r#", "markers":[],"markers":[]"#,
                r#", "bpm":1,"bpm":2"#,
                r#", "bpmOffset":1,"bpmOffset":2"#,
                r#", "bpmShow":true,"bpmShow":false"#,
                r#", "markers":[{"frame":0,"frame":1}]"#,
                r#", "markers":[{"frame":1e999}]"#,
                r#", "markers":[{"frame":0,"name":"x","name":"y"}]"#,
                r#", "bpm":1e999"#,
                r#", "bpmOffset":1e999"#,
            ] {
                let base =
                    wire(version, json!({"canvasW":320,"canvasH":180,"fps":24.0})).to_string();
                let raw = base.replace("\"fps\":24.0}", &format!("\"fps\":24.0{suffix}}}"));
                assert_ne!(raw, base);
                assert!(decode_project(raw.as_bytes(), limits()).is_err());
            }
        }
    }
}

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "project_composition_metadata_owner.rs"]
mod owner;

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "project_composition_metadata_budget.rs"]
mod budget;
