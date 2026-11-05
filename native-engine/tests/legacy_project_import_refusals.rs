//! Legacy project reader: typed refusals, the baseline migrations, and import -> save -> reopen -> edit -> undo/redo through the
//! real owner and codec (shares the fixtures and builders of the mapping tests).
use super::unit_tests::*;
use super::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

#[test]
fn content_with_no_native_owner_refuses_naming_the_exact_path() {
    for (mutate, path) in [
        (
            json!({"layers": [{"frames": [], "parentKeys": [{"frame": 0}]}]}),
            "layers[0].parentKeys",
        ),
        (
            json!({"layers": [{"frames": [], "parentsMore": [{"uid": "x"}]}]}),
            "layers[0].parentsMore",
        ),
        (
            json!({"layers": [{"frames": [], "blendKeys": [{"frame": 0}]}]}),
            "layers[0].blendKeys",
        ),
        (
            json!({"layers": [{"frames": []}], "trackRoles": {"r": {"name": "a"}}}),
            "trackRoles",
        ),
        (
            json!({"layers": [{"frames": []}], "exprGlobals": "var a = 1;"}),
            "exprGlobals",
        ),
    ] {
        let error = refusal(mutate);
        assert_eq!(
            (error.code(), error.path()),
            (LEGACY_UNSUPPORTED_FIELD, path),
            "{path}"
        );
        assert!(error.message().contains(path));
    }
    for (member, value) in [
        ("strokeGradientAlongPath", json!(true)),
        ("elemHidden", json!(true)),
        ("trackRoleId", json!("r")),
        ("isFillCloseLine", json!(true)),
    ] {
        let mut stroke = base_stroke();
        stroke[member] = value;
        let error = refusal(plain(stroke));
        assert_eq!(
            (error.code(), error.path().to_owned()),
            (
                LEGACY_UNSUPPORTED_FIELD,
                format!("layers[0].frames[0].strokes[0].{member}")
            )
        );
    }
}

#[test]
fn shapes_the_native_record_cannot_hold_refuse_instead_of_degrading() {
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({"segments": [{"point": [0, 0]}], "closed": false}),
            "layers[0].frames[0].strokes[0].segments",
        ),
        (
            json!({"segments": [{"point": [0, 0]}, {"point": [1, 1]}], "strokeCap": "flat"}),
            "layers[0].frames[0].strokes[0].strokeCap",
        ),
        (
            json!({"segments": [{"point": [0, 0]}, {"point": [1, 1]}], "fillColor": "red"}),
            "layers[0].frames[0].strokes[0].fillColor",
        ),
        (
            json!({"segments": [{"point": [0, 0]}, {"point": [1, 1]}], "paintOrder": "sideways"}),
            "layers[0].frames[0].strokes[0].paintOrder",
        ),
        (
            json!({"segments": [{"point": [0, 0]}, {"point": [1, 1]}], "strokeWidth": -2}),
            "layers[0].frames[0].strokes[0].strokeWidth",
        ),
    ];
    for (stroke, path) in cases {
        let error = refusal(plain(stroke));
        assert_eq!(
            (error.code(), error.path()),
            (LEGACY_UNSUPPORTED_FIELD, path)
        );
    }
    for (value, code, path) in [
        (json!("{"), LEGACY_INVALID, "$"),
        (json!({"version": 13}), LEGACY_INVALID, "layers"),
        (json!({"layers": []}), LEGACY_INVALID, "layers"),
        (
            json!({"layers": [{"name": "x"}]}),
            LEGACY_INVALID,
            "layers[0].frames",
        ),
        (
            json!({"layers": [{"frames": [{"isKeyframe": true}]}]}),
            LEGACY_INVALID,
            "layers[0].frames[0].strokes",
        ),
        (
            json!({"layers": [{"frames": [], "layerUid": "a"}, {"frames": [], "layerUid": "a"}]}),
            LEGACY_INVALID,
            "layers[1].layerUid",
        ),
        (
            json!({"layers": [{"frames": [], "layerUid": "a", "parentLayerUid": "b"}, {"frames": [], "layerUid": "b", "parentLayerUid": "a"}]}),
            LEGACY_UNSUPPORTED_FIELD,
            "layers[0].parentLayerUid",
        ),
        (
            json!({"layers": [{"frames": []}], "canvasW": 12.5}),
            LEGACY_UNSUPPORTED_FIELD,
            "canvasW",
        ),
    ] {
        let mut value = value;
        if let Some(map) = value.as_object_mut() {
            map.entry("totalFrames").or_insert(json!(4));
        }
        let bytes = if value.is_string() {
            b"{".to_vec()
        } else {
            value.to_string().into_bytes()
        };
        let error = convert(&bytes).err().expect("refused");
        assert_eq!((error.code(), error.path()), (code, path));
    }
}

#[test]
fn detection_recognises_baseline_projects_only() {
    assert!(
        looks_like_legacy_project(ORIGINAL.as_bytes())
            && looks_like_legacy_project(OLDER.as_bytes())
    );
    for other in [
        r#"{"format":"nemo.native-project-document"}"#,
        r#"{"format":"nemo.native-object-document","layers":[]}"#,
        r#"{"a":1}"#,
        "[1]",
        "not json",
    ] {
        assert!(!looks_like_legacy_project(other.as_bytes()), "{other}");
    }
}

#[test]
fn baseline_migrations_reproduce_the_pinned_expectations() {
    let mut doc: serde_json::Map<String, Value> = serde_json::from_str(MIGRATION).unwrap();
    let mut ctx = Ctx::default();
    migrate::run(&mut doc, &mut ctx).unwrap();
    let layers = doc["layers"].as_array().unwrap();
    let default_curve = json!([{"x": 0, "y": 0}, {"x": 0.25, "y": 0.156}, {"x": 0.5, "y": 0.5}, {"x": 0.75, "y": 0.844}, {"x": 1, "y": 1}]);
    assert_eq!(
        layers[0]["motion"]["position"]["keys"][0]["curvePoints"], default_curve,
        "the untouched legacy step curve is replaced"
    );
    assert_eq!(
        layers[0]["motion"]["position"]["keys"][1]["curvePoints"],
        json!([{"x": 0, "y": 0}, {"x": 0.4, "y": 0.1, "tx": 0.2}, {"x": 1, "y": 1}]),
        "an edited curve stays"
    );
    assert_eq!(
        layers[2]["motionStatic"],
        json!({"timeLinkInOffset": [3], "timeLinkOutOffset": [-2]})
    );
    assert_eq!(
        layers[3]["duplicator"]["effectors"][0]["channels"],
        json!([{"prop": "position", "value": [10, 0]}, {"prop": "rotation", "value": [5]}])
    );
    assert_eq!(layers[4]["matteSourceLayerUid"], "ly_mig_above");
    assert_eq!(
        doc["tweenOverrides"],
        json!({"ly_mig_curve:0-6": {"mode": "auto"}}),
        "an index-keyed span now starts with the layer uid"
    );
    assert_eq!(
        ctx.migrations,
        [
            "legacyCurve:1",
            "timeLinkOffsets:2",
            "effectorChannels:1",
            "matteSourceUid:1",
            "tweenSpanKeys:1"
        ]
    );
    let again = serde_json::to_string(&doc).unwrap();
    let mut doc2: serde_json::Map<String, Value> = serde_json::from_str(&again).unwrap();
    let mut second = Ctx::default();
    migrate::run(&mut doc2, &mut second).unwrap();
    assert!(
        second.migrations.is_empty(),
        "migrations are idempotent: {:?}",
        second.migrations
    );
}

pub(super) fn saved(owner: &NativeObjectHistory) -> Vec<u8> {
    let pin = owner.acquire_snapshot(owner.content_revision()).unwrap();
    owner
        .serialize_project_file(owner.content_revision(), pin.snapshot_id())
        .unwrap()
}

#[test]
fn import_save_reopen_is_identical_and_edits_undo_and_redo() {
    let converted = import(ORIGINAL);
    let mut owner =
        NativeObjectHistory::from_project_file("legacy-roundtrip", &converted.project_file)
            .unwrap();
    let first = saved(&owner);
    let reopened = NativeObjectHistory::from_project_file("legacy-roundtrip-2", &first).unwrap();
    assert_eq!(
        saved(&reopened),
        first,
        "save -> reopen -> save is byte identical"
    );
    let wire: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(wire["drawing"]["objects"].as_array().unwrap().len(), 4);
    assert_eq!(wire["drawing"]["layers"][1]["parentLayerUid"], "ly_ink");
    assert_eq!(wire["project"]["canvasW"], 640);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0)),
        "an opened legacy file starts with an empty history"
    );
    let payload =
        json!({"command": "object.layer.visibility.set", "layerUid": "ly_ink", "visible": false});
    let prepared = owner
        .prepare_layer_visibility_json(payload.to_string().as_bytes())
        .unwrap();
    let edited = owner.commit_layer_visibility(prepared).unwrap();
    assert_eq!(
        owner.history_depths(),
        (1, 0),
        "one gesture, one history entry"
    );
    let hidden: Value = serde_json::from_slice(&saved(&owner)).unwrap();
    assert_eq!(hidden["drawing"]["layers"][0]["visible"], false);
    let undone = owner.undo(edited).unwrap();
    assert_eq!(
        saved(&owner),
        first,
        "undo restores the imported document exactly"
    );
    owner.redo(undone).unwrap();
    let redone: Value = serde_json::from_slice(&saved(&owner)).unwrap();
    assert_eq!(redone, hidden);
}

#[test]
fn a_rejected_group_is_dropped_with_a_warning_and_a_rejected_drawing_refuses() {
    let mut bad = serde_json::from_str::<Value>(ORIGINAL).unwrap();
    bad["customBrushPresets"] = json!([1, 2, 3]);
    let converted =
        convert_admitted(bad.to_string().as_bytes()).unwrap_or_else(|e| panic!("{}", e.message()));
    assert!(
        converted
            .report
            .warnings
            .iter()
            .any(|w| w.path == "customBrushPresets" && w.code == "not_carried_over"),
        "{:?}",
        converted.report.warnings
    );
    assert!(NativeObjectHistory::from_project_file("x", &converted.project_file).is_ok());
    // Two refused groups at once are both dropped, each named; the drawing itself is untouched.
    bad["perspectiveVPs"] = json!("not a list");
    bad["comments"] = json!({"not": "a list"});
    let both =
        convert_admitted(bad.to_string().as_bytes()).unwrap_or_else(|e| panic!("{}", e.message()));
    let dropped: Vec<&str> = both
        .report
        .warnings
        .iter()
        .filter(|w| w.code == "not_carried_over")
        .map(|w| w.path.as_str())
        .collect();
    assert!(
        dropped.contains(&"perspectiveVPs") && dropped.contains(&"comments"),
        "{dropped:?}"
    );
    assert_eq!(
        file(&both)["drawing"]["objects"].as_array().unwrap().len(),
        4
    );
}

#[test]
fn imported_keyframes_hold_and_paint_through_the_real_frame_pipeline() {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
    use crate::object_render_scene::{prepare_object_paint_plan, prepare_object_render_scene};
    let owner =
        NativeObjectHistory::from_project_file("legacy-paint", &import(ORIGINAL).project_file)
            .unwrap();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let painted = |frame: u32| {
        let pins = ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: 0,
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame,
        };
        let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
        let scene = prepare_object_render_scene(&packet, &pins).unwrap();
        prepare_object_paint_plan(&scene, &pins)
            .unwrap()
            .commands()
            .len()
    };
    // Ink keys at 0 (rect, line) and 8 (ribbon) hold between and after them; the hidden Paint layer never paints.
    assert_eq!([0, 4, 7, 8, 20, 23].map(painted), [2, 2, 2, 1, 1, 1]);
}
