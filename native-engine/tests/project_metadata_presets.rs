//! Custom preset storage, not brush rendering or an alternate catalog authority.
use super::*;
use serde_json::{json, Value};
fn metadata() -> ProjectMetadata {
    ProjectMetadata::new(320, 180, 24.0).unwrap()
}

#[test]
fn custom_catalog_retains_original_insertion_order_and_rejects_duplicate_ids() {
    let raw = r#"{"zebra":{"label":"Z"},"alpha":{"label":"A"}}"#;
    let catalog: ProjectPresetCatalog = serde_json::from_str(raw).unwrap();
    assert_eq!(
        catalog
            .entries()
            .iter()
            .map(|v| v.0.as_str())
            .collect::<Vec<_>>(),
        vec!["zebra", "alpha"]
    );
    assert_eq!(serde_json::to_string(&catalog).unwrap(), raw);
    assert!(serde_json::from_str::<ProjectPresetCatalog>(r#"{"p":{},"p":{}}"#).is_err());
    assert!(
        serde_json::from_str::<ProjectPresetCatalog>(r#"{"p":{"label":"a","label":"b"}}"#).is_err()
    );
}

#[test]
fn custom_preset_parameters_and_captured_stamp_are_retained_without_defaults_or_clamps() {
    let mut preset = serde_json::Map::new();
    for (i, key) in [
        "bristleCount",
        "dashGap",
        "edgeNoise",
        "fixedAngle",
        "grain",
        "nibSize",
        "noise",
        "opacity",
        "opacityJitter",
        "polySides",
        "pressureEnd",
        "pressureMid",
        "pressureStart",
        "rotationJitter",
        "roughness",
        "roundness",
        "scatter",
        "scribbleAngleSpread",
        "scribbleCount",
        "scribbleLen",
        "scribbleLenJitter",
        "scribbleSpread",
        "scribbleWidth",
        "sharpness",
        "sizeJitter",
        "spaceJitter",
        "spacing",
        "tipCorner",
        "valueJitter",
        "savedBrushSize",
    ]
    .iter()
    .enumerate()
    {
        preset.insert((*key).into(), json!(-(i as f64) - 0.25));
    }
    preset.extend(json!({"rotationMode":"fixed","scatterDistribution":"normal","tipShape":"custom","label":"",
        "markerTip":false,"customStamp":{"segments":[{"point":[-0.5,0.25],"handleIn":[0.0,0.0],"handleOut":[-0.25,0.5]},
            {"point":[0.5,-0.25]}],"closed":false,"pointCount":2.0}}).as_object().unwrap().clone());
    let raw = json!({"p":preset});
    let catalog: ProjectPresetCatalog = serde_json::from_value(raw.clone()).unwrap();
    let value = metadata().with_custom_brush_presets(Some(catalog)).unwrap();
    assert_eq!(
        serde_json::to_value(value.custom_brush_presets().unwrap()).unwrap(),
        raw
    );
    let partial: ProjectPresetCatalog = serde_json::from_value(json!({"p":{"label":""}})).unwrap();
    assert_eq!(
        serde_json::to_value(partial).unwrap(),
        json!({"p":{"label":""}})
    );
}

#[test]
fn domain_refuses_nonfinite_geometry_numbers_and_duplicate_ids_and_retains_other_allocations() {
    let before = metadata()
        .with_composition_metadata(Some(vec![(2.5, None, None)]), None, None, None)
        .unwrap();
    let ptr = before.composition_markers().unwrap().as_ptr();
    let empty = before
        .clone()
        .with_custom_brush_presets(Some(ProjectPresetCatalog::default()))
        .unwrap();
    assert_eq!(empty.composition_markers().unwrap().as_ptr(), ptr);
    assert!(before.custom_brush_presets().is_none());
    let duplicated = ProjectPresetCatalog(vec![
        ("p".into(), BrushPresetRecord::default()),
        ("p".into(), BrushPresetRecord::default()),
    ]);
    assert_eq!(
        before.clone().with_custom_brush_presets(Some(duplicated)),
        Err(ProjectMetadataError::InvalidPresets)
    );
    for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut scalar = BrushPresetRecord::default();
        scalar.nib_size = Presence::Present(n);
        let mut geometry = BrushPresetRecord::default();
        geometry.custom_stamp = Presence::Present(CustomStamp {
            segments: vec![StampSegment {
                point: [n, 0.0],
                handle_in: Presence::Missing,
                handle_out: Presence::Missing,
            }],
            closed: Presence::Missing,
            point_count: Presence::Missing,
        });
        for record in [scalar, geometry] {
            assert_eq!(
                before
                    .clone()
                    .with_custom_brush_presets(Some(ProjectPresetCatalog(vec![(
                        "p".into(),
                        record
                    )]))),
                Err(ProjectMetadataError::InvalidPresets)
            );
        }
    }
}

#[cfg(all(feature = "history", feature = "project_media_codec"))]
mod wire {
    use super::*;
    use crate::history::NativeObjectHistory;
    fn input(version: u32, catalog: Option<&str>) -> Vec<u8> {
        let mut project = json!({"canvasW":320,"canvasH":180,"fps":24});
        if catalog.is_some() {
            project["customBrushPresets"] = json!("__PRESET_CATALOG__");
        }
        let mut data = json!({"format":"nemo.native-project-document","formatVersion":version,"project":project,
            "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]},"viewportReference":null,"assets":[]});
        if version == 2 {
            data["assetInventory"] = json!({"assetIds":[],"placements":[]});
        }
        let raw = data.to_string();
        catalog
            .map_or(raw.clone(), |catalog| {
                raw.replace("\"__PRESET_CATALOG__\"", catalog)
            })
            .into_bytes()
    }
    fn save(owner: &NativeObjectHistory) -> Vec<u8> {
        let pin = owner.acquire_snapshot(owner.content_revision()).unwrap();
        owner
            .serialize_project_file(pin.content_revision(), pin.snapshot_id())
            .unwrap()
    }

    #[test]
    fn preset_catalog_both_versions_preserve_order_across_real_history_and_save_fresh_reopen() {
        let raw = r#"{"zebra":{"label":"Z","savedBrushSize":-3.5,"customStamp":{"segments":[{"point":[0.5,-0.5]}],"closed":false}},"alpha":{"label":"A"}}"#;
        for version in [1, 2] {
            let mut owner = NativeObjectHistory::from_project_file(
                "preset-storage",
                &input(version, Some(raw)),
            )
            .unwrap();
            let before = save(&owner);
            let pin = owner.acquire_snapshot(0).unwrap();
            let ptr = pin
                .project()
                .unwrap()
                .project()
                .custom_brush_presets()
                .unwrap() as *const _;
            let edit=owner.prepare_composition_delta_json(br#"{"command":"object.project.composition.edit","action":{"kind":"setBpm","bpm":132}}"#).unwrap();
            owner.commit_project_settings(edit).unwrap();
            let current = owner.acquire_snapshot(1).unwrap();
            assert_eq!(
                current
                    .project()
                    .unwrap()
                    .project()
                    .custom_brush_presets()
                    .unwrap() as *const _,
                ptr
            );
            let after = save(&owner);
            let text = String::from_utf8(after.clone()).unwrap();
            assert!(text.find("\"zebra\"").unwrap() < text.find("\"alpha\"").unwrap());
            let decoded: Value = serde_json::from_slice(&after).unwrap();
            assert_eq!(
                decoded["project"]["customBrushPresets"],
                serde_json::from_str::<Value>(raw).unwrap()
            );
            let reopened = NativeObjectHistory::from_project_file("preset-fresh", &after).unwrap();
            assert_eq!(save(&reopened), after);
            owner.undo(1).unwrap();
            assert_eq!(save(&owner), before);
            owner.redo(2).unwrap();
            assert_eq!(save(&owner), after);
        }
    }

    #[test]
    fn absent_and_empty_custom_catalogs_never_seed_builtin_presets() {
        for catalog in [None, Some("{}")] {
            let owner =
                NativeObjectHistory::from_project_file("preset-presence", &input(1, catalog))
                    .unwrap();
            let saved = save(&owner);
            let json: Value = serde_json::from_slice(&saved).unwrap();
            assert_eq!(
                json["project"].get("customBrushPresets").is_some(),
                catalog.is_some()
            );
            if catalog.is_some() {
                assert_eq!(json["project"]["customBrushPresets"], json!({}));
            }
            assert_eq!(owner.history_depths(), (0, 0));
            assert_eq!(save(&owner), saved);
        }
    }

    #[test]
    fn preset_codec_rejects_unknown_fields_bad_types_positional_records_and_duplicates() {
        for raw in [
            "null",
            "[]",
            r#"{"p":[]}"#,
            r#"{"p":{"unknown":1}}"#,
            r#"{"p":{"nibSize":null}}"#,
            r#"{"p":{"markerTip":0}}"#,
            r#"{"p":{"customStamp":[]}}"#,
            r#"{"p":{"customStamp":{"segments":[[0,0]]}}}"#,
            r#"{"p":{"customStamp":{"segments":[{"point":[0,0,0]}]}}}"#,
            r#"{"p":{"customStamp":{"segments":[],"unknown":1}}}"#,
            r#"{"p":{},"p":{}}"#,
            r#"{"p":{"label":"one","label":"two"}}"#,
        ] {
            assert!(
                NativeObjectHistory::from_project_file("preset-invalid", &input(1, Some(raw)))
                    .is_err(),
                "{raw}"
            );
        }
    }
}
