//! Authored storage values, not derived UI defaults or a second state owner.
use super::*;
fn metadata() -> ProjectMetadata {
    ProjectMetadata::new(320, 180, 24.0).unwrap()
}

#[test]
fn palette_and_shadow_metadata_preserve_presence_order_duplicates_and_spelling() {
    let before = metadata();
    assert_eq!(before.palette_metadata(), (None, None));
    let palettes = vec![
        ("p".into(), "".into(), vec!["#AbC".into(), "".into()]),
        ("p".into(), "same id".into(), vec!["rgba(1,2,3,.5)".into()]),
    ];
    let value = before
        .clone()
        .with_palette_metadata(Some(palettes.clone()), Some(-0.5))
        .unwrap()
        .with_shadow_palette_metadata(
            Some(vec![("s".into(), "#AA00cc".into())]),
            Some("missing-id".into()),
        )
        .unwrap();
    assert_eq!(
        value.palette_metadata(),
        (Some(palettes.as_slice()), Some(-0.5))
    );
    assert_eq!(value.shadow_palette_metadata().1, Some("missing-id"));
    assert_eq!(before.palette_metadata(), (None, None));
    let empty = before.with_palette_metadata(Some(vec![]), None).unwrap();
    assert_eq!(empty.palette_metadata(), (Some([].as_slice()), None));
}

#[test]
fn guides_and_perspective_preserve_missing_null_empty_fractional_and_locked_presence() {
    let before = metadata();
    assert_eq!(before.guide_metadata(), None);
    assert_eq!(before.perspective_metadata(), (None, None, None, None));
    let value = before
        .clone()
        .with_guide_metadata(Some((None, Some(vec![-0.5, 4.25, -0.5]))))
        .unwrap()
        .with_perspective_metadata(Some(false), Some("fisheye".into()), Some(-3.5), Some(None))
        .unwrap();
    assert_eq!(
        value.guide_metadata(),
        Some((None, Some([-0.5, 4.25, -0.5].as_slice())))
    );
    assert_eq!(
        value.perspective_metadata(),
        (Some(false), Some("fisheye"), Some(-3.5), Some(None))
    );
    let empty = value
        .clone()
        .with_perspective_metadata(None, None, None, Some(Some(vec![])))
        .unwrap();
    assert_eq!(empty.perspective_metadata().3, Some(Some([].as_slice())));
    let points = vec![(-4.5, 200.25, None), (10.0, -9.0, Some(false))];
    let supplied = value
        .with_perspective_metadata(None, None, None, Some(Some(points.clone())))
        .unwrap();
    assert_eq!(
        supplied.perspective_metadata().3,
        Some(Some(points.as_slice()))
    );
    assert_eq!(before.guide_metadata(), None);
}

#[test]
fn unrelated_marker_allocation_survives_visual_edits_and_invalid_values_refuse() {
    let before = metadata()
        .with_composition_metadata(Some(vec![(1.5, None, None)]), Some(132.0), None, None)
        .unwrap();
    let ptr = before.composition_markers().unwrap().as_ptr();
    let edited = before
        .clone()
        .with_guide_metadata(Some((Some(vec![0.0]), None)))
        .unwrap();
    assert_eq!(edited.composition_markers().unwrap().as_ptr(), ptr);
    assert_eq!(edited.composition_bpm(), (Some(132.0), None, None));
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        assert_eq!(
            before.clone().with_palette_metadata(None, Some(value)),
            Err(invalid())
        );
        assert_eq!(
            before
                .clone()
                .with_guide_metadata(Some((Some(vec![value]), None))),
            Err(invalid())
        );
        let bad = before.clone().with_perspective_metadata(
            None,
            None,
            None,
            Some(Some(vec![(0.0, value, None)])),
        );
        assert_eq!(bad, Err(invalid()));
    }
    assert_eq!(
        before
            .clone()
            .with_shadow_palette_metadata(None, Some("bad\0id".into())),
        Err(invalid())
    );
    assert_eq!(
        before.with_palette_metadata(Some(vec![("p".into(), "é".repeat(513), vec![])]), None),
        Err(invalid())
    );
}

#[test]
fn symmetry_settings_store_supplied_values_and_refuse_non_finite_or_invalid_text() {
    let before = metadata();
    assert_eq!(before.symmetry_metadata(), &SymmetrySettings::default());
    let settings = SymmetrySettings {
        enabled: Some(true),
        mode: Some("radial".into()),
        axis: Some(None),
        radial_center: Some(Some((-0.5, 400.25))),
        radial_sectors: Some(30.5),
        extend: Some(false),
    };
    let edited = before.clone().with_symmetry_metadata(settings.clone()).unwrap();
    assert_eq!(edited.symmetry_metadata(), &settings);
    assert_eq!(edited.palette_metadata(), before.palette_metadata());
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        for bad in [
            SymmetrySettings { radial_sectors: Some(value), ..settings.clone() },
            SymmetrySettings { axis: Some(Some((0.0, 0.0, value, 1.0))), ..settings.clone() },
            SymmetrySettings { radial_center: Some(Some((value, 0.0))), ..settings.clone() },
        ] {
            assert_eq!(before.clone().with_symmetry_metadata(bad), Err(invalid()));
        }
    }
    for mode in ["bad\0mode".to_owned(), "é".repeat(513)] {
        let bad = SymmetrySettings { mode: Some(mode), ..settings.clone() };
        assert_eq!(before.clone().with_symmetry_metadata(bad), Err(invalid()));
    }
}

#[cfg(all(feature = "history", feature = "project_media_codec"))]
mod wire {
    use crate::history::NativeObjectHistory;
    use serde_json::{json, Value};
    fn input(version: u32, fields: Value) -> Vec<u8> {
        let mut project = json!({"canvasW":320,"canvasH":180,"fps":24});
        project
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let mut value = json!({"format":"nemo.native-project-document","formatVersion":version,"project":project,
            "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]},
            "viewportReference":null,"assets":[]});
        if version == 2 {
            value["assetInventory"] = json!({"assetIds":[],"placements":[]});
        }
        serde_json::to_vec(&value).unwrap()
    }
    fn save(owner: &NativeObjectHistory) -> Vec<u8> {
        let pin = owner.acquire_snapshot(owner.content_revision()).unwrap();
        owner
            .serialize_project_file(pin.content_revision(), pin.snapshot_id())
            .unwrap()
    }
    fn project(owner: &NativeObjectHistory) -> Value {
        serde_json::from_slice::<Value>(&save(owner)).unwrap()["project"].clone()
    }
    const FIELDS: [&str; 15] = [
        "palettes",
        "activePaletteIdx",
        "shadowPalette",
        "shadowActiveId",
        "guides",
        "perspectiveEnabled",
        "perspectiveMode",
        "perspectiveDensity",
        "perspectiveVPs",
        "symmetryEnabled",
        "symmetryMode",
        "symmetryAxis",
        "symmetryRadialCenter",
        "symmetryRadialSectors",
        "symmetryExtend",
    ];

    #[test]
    fn visual_metadata_both_wire_versions_survive_history_and_save_fresh_reopen() {
        let fields = json!({"palettes":[{"id":"p","name":"","colors":["#AbC",""]},{"id":"p","name":"X","colors":[]}],
            "activePaletteIdx":-0.5,"shadowPalette":[{"id":"s","color":"#AA00cc"}],"shadowActiveId":"missing-id",
            "guides":{"v":[-0.5,4.25,-0.5]},"perspectiveEnabled":false,"perspectiveMode":"fisheye",
            "perspectiveDensity":-3.5,"perspectiveVPs":null,"symmetryEnabled":true,"symmetryMode":"radial",
            "symmetryAxis":{"x1":-0.5,"y1":0.0,"x2":320.25,"y2":180.0},"symmetryRadialCenter":null,
            "symmetryRadialSectors":7.5,"symmetryExtend":false});
        for version in [1, 2] {
            let mut owner = NativeObjectHistory::from_project_file(
                "visual-fields",
                &input(version, fields.clone()),
            )
            .unwrap();
            let before = save(&owner);
            let raw = project(&owner);
            for field in FIELDS {
                assert_eq!(raw.get(field), fields.get(field), "{version}:{field}");
            }
            let candidate=owner.prepare_composition_delta_json(br#"{"command":"object.project.composition.edit","action":{"kind":"setBpm","bpm":132}}"#).unwrap();
            owner.commit_project_settings(candidate).unwrap();
            assert_eq!(owner.history_depths(), (1, 0));
            let after = save(&owner);
            let changed = project(&owner);
            for field in FIELDS {
                assert_eq!(
                    changed.get(field),
                    fields.get(field),
                    "tempo edit must retain {field}"
                );
            }
            let reopened = NativeObjectHistory::from_project_file("visual-fresh", &after).unwrap();
            assert_eq!(save(&reopened), after);
            owner.undo(1).unwrap();
            assert_eq!(save(&owner), before);
            owner.redo(2).unwrap();
            assert_eq!(save(&owner), after);
        }
    }

    #[test]
    fn optional_visual_missing_null_empty_axes_and_point_lock_presence_remain_distinct() {
        for fields in [
            json!({}),
            json!({"palettes":[],"shadowPalette":[],"guides":{},"perspectiveVPs":[]}),
            json!({"guides":{"h":[],"v":[]},"perspectiveVPs":null}),
            json!({"perspectiveVPs":[{"x":-0.25,"y":4.5},{"x":8.0,"y":-3.0,"locked":false}]}),
        ] {
            let owner = NativeObjectHistory::from_project_file(
                "visual-presence",
                &input(1, fields.clone()),
            )
            .unwrap();
            let actual = project(&owner);
            for field in FIELDS {
                assert_eq!(actual.get(field), fields.get(field), "{field}");
            }
            let reopened =
                NativeObjectHistory::from_project_file("visual-presence-fresh", &save(&owner))
                    .unwrap();
            assert_eq!(save(&reopened), save(&owner));
        }
    }

    #[test]
    fn visual_nested_records_are_object_only_and_reject_null_unknown_and_duplicate_fields() {
        for fields in [
            json!({"palettes":null}),
            json!({"palettes":[["p","P",[]]]}),
            json!({"palettes":[{"id":"p","name":"P","colors":[],"other":0}]}),
            json!({"guides":[]}),
            json!({"guides":{"h":null}}),
            json!({"guides":{"h":[],"depth":1}}),
            json!({"perspectiveVPs":[{"x":null,"y":0}]}),
            json!({"perspectiveVPs":[{"x":0,"y":0,"locked":null}]}),
            json!({"perspectiveVPs":[[0,0,false]]}),
            json!({"perspectiveEnabled":null}),
        ] {
            assert!(
                NativeObjectHistory::from_project_file("visual-invalid", &input(1, fields))
                    .is_err()
            );
        }
        let raw = String::from_utf8(input(
            1,
            json!({"palettes":[{"id":"p","name":"P","colors":[]}]}),
        ))
        .unwrap();
        let duplicate = raw.replace("\"id\":\"p\"", "\"id\":\"p\",\"id\":\"other\"");
        assert_ne!(duplicate, raw);
        assert!(
            NativeObjectHistory::from_project_file("visual-duplicate", duplicate.as_bytes())
                .is_err()
        );
    }

    #[test]
    fn flat_palette_wraps_once_with_original_precedence_and_canonical_only_reopen() {
        let canonical = json!([{"id":"kept","name":"Original","colors":["#CaF",""]}]);
        let wrapped = json!([{"id":"p0","name":"Palette 1","colors":["#AbC","", "#AbC"]}]);
        for version in [1, 2] {
            for (fields, expected) in [
                (json!({"colorPalette":["#AbC","", "#AbC"]}), wrapped.clone()),
                (
                    json!({"palettes":[],"colorPalette":["#AbC","", "#AbC"]}),
                    wrapped.clone(),
                ),
                (
                    json!({"palettes":canonical,"colorPalette":["ignored"]}),
                    canonical.clone(),
                ),
                (
                    json!({"colorPalette":[]}),
                    json!([{"id":"p0","name":"Palette 1","colors":[]}]),
                ),
                (json!({"palettes":[]}), json!([])),
            ] {
                let mut fields = fields;
                fields["activePaletteIdx"] = json!(-0.5);
                fields["comments"] = json!([{"text":"retained attribution","authorName":"Artist"}]);
                let owner = NativeObjectHistory::from_project_file(
                    "flat-palette",
                    &input(version, fields.clone()),
                )
                .unwrap();
                let actual = project(&owner);
                assert_eq!(actual["palettes"], expected, "{version}:{fields}");
                assert_eq!(actual["activePaletteIdx"], fields["activePaletteIdx"]);
                assert_eq!(actual["comments"], fields["comments"]);
                assert!(actual.get("colorPalette").is_none());
                assert_eq!(owner.content_revision(), 0);
                assert_eq!(owner.history_depths(), (0, 0));
                let bytes = save(&owner);
                let reopened =
                    NativeObjectHistory::from_project_file("flat-palette-fresh", &bytes).unwrap();
                assert_eq!(save(&reopened), bytes);
                assert_eq!(project(&reopened)["palettes"], expected);
            }
        }
    }

    #[test]
    fn flat_palette_two_document_replacement_keeps_absence_empty_and_old_snapshot_isolated() {
        for version in [1, 2] {
            let mut owner = NativeObjectHistory::from_project_file(
                "palette-replacement",
                &input(
                    version,
                    json!({"colorPalette":["#original"],"activePaletteIdx":4.5}),
                ),
            )
            .unwrap();
            let old = owner.acquire_snapshot(owner.content_revision()).unwrap();
            let first_bytes = save(&owner);
            for fields in [json!({}), json!({"palettes":[]}), json!({})] {
                let current = owner.acquire_snapshot(owner.content_revision()).unwrap();
                let next = owner
                    .prepare_project_replacement(
                        current.snapshot_id(),
                        &input(version, fields.clone()),
                    )
                    .unwrap();
                owner.commit_project_replacement(next).unwrap();
                let actual = project(&owner);
                assert_eq!(actual.get("palettes"), fields.get("palettes"));
                assert!(actual.get("colorPalette").is_none());
                assert!(actual.get("activePaletteIdx").is_none());
                assert_eq!(owner.content_revision(), 0);
                assert_eq!(owner.history_depths(), (0, 0));
                let reopened = NativeObjectHistory::from_project_file(
                    "palette-replacement-fresh",
                    &save(&owner),
                )
                .unwrap();
                assert_eq!(save(&reopened), save(&owner));
            }
            assert_eq!(old.serialize_project_file().unwrap(), first_bytes);
        }
    }

    // F05: opening another project must not leak any record of the previous one.
    fn full_records(tag: &str) -> Value {
        json!({"palettes":[{"id":tag,"name":tag,"colors":["#AbC"]}],"activePaletteIdx":1.5,
            "shadowPalette":[{"id":tag,"color":"#AA00cc"}],"shadowActiveId":tag,
            "guides":{"h":[-0.25],"v":[4.5]},"perspectiveEnabled":true,"perspectiveMode":"custom",
            "perspectiveDensity":-3.5,"perspectiveVPs":[{"x":1.0,"y":2.0,"locked":true}],
            "symmetryEnabled":true,"symmetryMode":tag,"symmetryAxis":null,
            "symmetryRadialCenter":{"x":1.5,"y":-2.5},"symmetryRadialSectors":6.0,"symmetryExtend":true,
            "workspace":{"panelOrder":null,"motionColumns":tag},
            "comments":[{"id":tag,"authorName":"Writer","text":tag,"createdAt":1.5,"resolved":false}]})
    }

    #[test]
    fn full_record_two_document_replacement_leaks_no_previous_palette_aid_workspace_or_comment() {
        for version in [1, 2] {
            let mut owner = NativeObjectHistory::from_project_file(
                "records-replacement",
                &input(version, full_records("first")),
            )
            .unwrap();
            let old = owner.acquire_snapshot(owner.content_revision()).unwrap();
            let first_bytes = save(&owner);
            for fields in [json!({}), full_records("second"), json!({"guides":{"h":[],"v":[]}})] {
                let current = owner.acquire_snapshot(owner.content_revision()).unwrap();
                let next = owner
                    .prepare_project_replacement(current.snapshot_id(), &input(version, fields.clone()))
                    .unwrap();
                owner.commit_project_replacement(next).unwrap();
                let actual = project(&owner);
                for field in FIELDS.iter().copied().chain(["workspace", "comments"]) {
                    assert_eq!(actual.get(field), fields.get(field), "{version}:{field}:{fields}");
                }
                assert_eq!(owner.history_depths(), (0, 0));
                let reopened =
                    NativeObjectHistory::from_project_file("records-fresh", &save(&owner)).unwrap();
                assert_eq!(save(&reopened), save(&owner));
            }
            assert_eq!(old.serialize_project_file().unwrap(), first_bytes);
        }
    }

    // Baseline exportJSON (timeline.js @1065e9d8) writes `guides` but never the ruler/guide
    // toggles, the undo stacks or the action log.
    const SESSION_ONLY: [&str; 9] = [
        "rulersOn", "guidesLocked", "guidesSnap", "showTweenCurves", "selectedStrokeIndices",
        "undoStack", "redoStack", "undoLabels", "actionLog",
    ];

    #[test]
    fn session_only_flags_never_enter_the_saved_file_and_are_not_admitted_from_one() {
        for version in [1, 2] {
            let owner = NativeObjectHistory::from_project_file(
                "records-session",
                &input(version, full_records("kept")),
            )
            .unwrap();
            let saved = save(&owner);
            let text = String::from_utf8(saved.clone()).unwrap();
            let project = project(&owner);
            for flag in SESSION_ONLY {
                assert!(project.get(flag).is_none(), "{version}:{flag}");
                assert!(!text.contains(&format!("\"{flag}\"")), "{version}:{flag}");
                let mut fields = full_records("kept");
                fields[flag] = json!(true);
                assert!(
                    NativeObjectHistory::from_project_file("records-flag", &input(version, fields))
                        .is_err(),
                    "{version}:{flag} must not be admitted as a persistent record"
                );
            }
        }
    }

    #[test]
    fn flat_palette_alias_retains_strict_value_duplicate_unknown_and_envelope_guards() {
        for version in [1, 2] {
            for invalid in [
                json!({"colorPalette":null}),
                json!({"colorPalette":42}),
                json!({"colorPalette":{}}),
                json!({"colorPalette":[null]}),
                json!({"colorPalette":[1]}),
                json!({"palettes":null,"colorPalette":[]}),
                json!({"colorPalette":[],"unrelatedUnknown":true}),
                json!({"palettes":[{"id":"p","name":"P","colors":[]}],"colorPalette":false}),
            ] {
                assert!(NativeObjectHistory::from_project_file(
                    "flat-invalid",
                    &input(version, invalid)
                )
                .is_err());
            }
            let raw = String::from_utf8(input(version, json!({"colorPalette":[]}))).unwrap();
            let duplicated = raw.replace(
                "\"colorPalette\":[]",
                "\"colorPalette\":[],\"colorPalette\":[\"duplicate\"]",
            );
            assert_ne!(raw, duplicated);
            assert!(NativeObjectHistory::from_project_file(
                "flat-duplicate",
                duplicated.as_bytes()
            )
            .is_err());
            for (field, value) in [
                ("format", json!("legacy-project")),
                ("formatVersion", json!(3)),
            ] {
                let mut invalid: Value =
                    serde_json::from_slice(&input(version, json!({"colorPalette":[]}))).unwrap();
                invalid[field] = value;
                assert!(NativeObjectHistory::from_project_file(
                    "flat-envelope",
                    &serde_json::to_vec(&invalid).unwrap()
                )
                .is_err());
            }
        }
    }

    #[test]
    fn symmetry_members_keep_null_and_missing_distinct_and_refuse_malformed_records() {
        for fields in [
            json!({}),
            json!({"symmetryAxis":null,"symmetryMode":""}),
            json!({"symmetryRadialCenter":null,"symmetryAxis":{"x1":1.0,"y1":2.0,"x2":3.0,"y2":4.0}}),
        ] {
            for version in [1, 2] {
                let owner =
                    NativeObjectHistory::from_project_file("symmetry", &input(version, fields.clone()))
                        .unwrap();
                let raw = project(&owner);
                for field in &FIELDS[9..] {
                    assert_eq!(raw.get(*field), fields.get(*field), "{version}:{field}:{fields}");
                }
            }
        }
        for invalid in [
            json!({"symmetryEnabled":"true"}),
            json!({"symmetryEnabled":null}),
            json!({"symmetryMode":7}),
            json!({"symmetryMode":null}),
            json!({"symmetryAxis":[0.0,0.0,1.0,1.0]}),
            json!({"symmetryAxis":{"x1":0.0,"y1":0.0,"x2":1.0}}),
            json!({"symmetryAxis":{"x1":0.0,"y1":0.0,"x2":1.0,"y2":1.0,"z":0.0}}),
            json!({"symmetryAxis":{"x1":"0","y1":0.0,"x2":1.0,"y2":1.0}}),
            json!({"symmetryRadialCenter":[1.0,2.0]}),
            json!({"symmetryRadialCenter":{"x":1.0}}),
            json!({"symmetryRadialSectors":"6"}),
            json!({"symmetryRadialSectors":null}),
            json!({"symmetryExtend":1}),
            json!({"symmetryRadialCentre":{"x":1.0,"y":2.0}}),
        ] {
            for version in [1, 2] {
                assert!(
                    NativeObjectHistory::from_project_file("symmetry-bad", &input(version, invalid.clone()))
                        .is_err(),
                    "{version}:{invalid}"
                );
            }
        }
        let raw = String::from_utf8(input(1, json!({"symmetryExtend":true}))).unwrap();
        let duplicated =
            raw.replace("\"symmetryExtend\":true", "\"symmetryExtend\":true,\"symmetryExtend\":false");
        assert_ne!(raw, duplicated);
        assert!(NativeObjectHistory::from_project_file("symmetry-dup", duplicated.as_bytes()).is_err());
    }
}
