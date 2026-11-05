//! Legacy project reader (G-legacy-import slice 1): fixed expected values for every pinned fixture, the typed refusals,
//! the baseline migrations, and import -> save -> reopen -> edit -> undo/redo through the real owner and codec.
use super::*;
use serde_json::{json, Value};

pub(super) const ORIGINAL: &str = include_str!("../../tests/fixtures/legacy-import/original.json");
pub(super) const OLDER: &str = include_str!("../../tests/fixtures/legacy-import/older.json");
pub(super) const SINGLE: &str =
    include_str!("../../tests/fixtures/legacy-import/single-frame.json");
pub(super) const MIXED: &str = include_str!("../../tests/fixtures/legacy-import/mixed-model.json");
pub(super) const FUTURE: &str =
    include_str!("../../tests/fixtures/legacy-import/future-version.json");
pub(super) const UNKNOWN: &str =
    include_str!("../../tests/fixtures/legacy-import/unknown-field.json");
pub(super) const MIGRATION: &str = include_str!("../../tests/fixtures/migration/project.json");

pub(super) fn file(converted: &Converted) -> Value {
    serde_json::from_slice(&converted.project_file).unwrap()
}
pub(super) fn import(text: &str) -> Converted {
    convert_admitted(text.as_bytes()).unwrap_or_else(|e| panic!("{}", e.message()))
}
pub(super) fn refusal(mut value: Value) -> LegacyImportError {
    value
        .as_object_mut()
        .unwrap()
        .entry("totalFrames")
        .or_insert(json!(4));
    convert(value.to_string().as_bytes())
        .err()
        .expect("refused")
}
pub(super) fn warnings(converted: &Converted) -> Vec<(String, &'static str)> {
    converted
        .report
        .warnings
        .iter()
        .map(|w| (w.path.clone(), w.code))
        .collect()
}
pub(super) fn plain(stroke: Value) -> Value {
    json!({"version": 13, "layers": [{"name": "L", "frames": [{"strokes": [stroke], "isKeyframe": true}]}]})
}
pub(super) fn base_stroke() -> Value {
    json!({"segments": [{"point": [0, 0]}, {"point": [1, 1]}], "closed": false, "strokeColor": "#000000", "hasRealStroke": true})
}

#[test]
fn original_project_maps_settings_layers_keyframes_and_strokes_literally() {
    let converted = import(ORIGINAL);
    let wire = file(&converted);
    assert_eq!(wire["format"], "nemo.native-project-document");
    let project = &wire["project"];
    for (key, value) in [
        ("canvasW", json!(640)),
        ("canvasH", json!(360)),
        ("fps", json!(24)),
        ("canvasBg", json!("#ffffff")),
        ("waIn", json!(0)),
        ("waOut", json!(23)),
        ("activePaletteIdx", json!(0)),
        ("bpm", json!(120)),
    ] {
        assert_eq!(project[key], value, "{key}");
    }
    let drawing = &wire["drawing"];
    assert_eq!(
        (
            drawing["formatVersion"].clone(),
            drawing["totalFrames"].clone()
        ),
        (json!(2), json!(24))
    );
    assert_eq!(
        drawing["layers"][0],
        json!({"layerUid": "ly_ink", "name": "Ink", "visible": true, "locked": false, "color": "#4A9EFF", "parentLayerUid": null,
        "timing": {"format": "nemo.native-layer-timing", "formatVersion": 1, "family": "drawing", "contextId": "scene-root", "keys": [{"frame": 0}, {"frame": 8}], "samples": []}})
    );
    assert_eq!(
        drawing["layers"][1],
        json!({"layerUid": "ly_paint", "name": "Paint", "visible": false, "locked": true, "color": "#F0A585", "parentLayerUid": "ly_ink", "shy": true,
        "timing": {"format": "nemo.native-layer-timing", "formatVersion": 1, "family": "drawing", "contextId": "scene-root", "keys": [{"frame": 0}, {"frame": 12}], "samples": []}})
    );
    let objects = drawing["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 4);
    assert_eq!(
        objects[0],
        json!({"schemaVersion": 1, "family": "editable-vector-path",
        "target": {"contextId": "scene-root", "frameScope": {"kind": "authored", "frame": 0}, "layerUid": "ly_ink", "strokeId": "st_rect"},
        "geometry": {"kind": "cubic-path", "closed": true, "coordinateSpace": "document-world", "handleSpace": "relative", "segments": [
            {"point": {"x": 20, "y": 80}, "handleIn": {"x": 0, "y": 0}, "handleOut": {"x": 0, "y": 0}},
            {"point": {"x": 20, "y": 60}, "handleIn": {"x": 0, "y": 0}, "handleOut": {"x": 0, "y": 0}},
            {"point": {"x": 40, "y": 60}, "handleIn": {"x": 0, "y": 0}, "handleOut": {"x": 0, "y": 0}},
            {"point": {"x": 40, "y": 80}, "handleIn": {"x": 0, "y": 0}, "handleOut": {"x": 0, "y": 0}}]},
        "fill": {"kind": "solid", "r": 1, "g": 0, "b": 0, "a": 1},
        "vector": {"stroke": null, "hasRealStroke": false, "strokeWidth": 1, "strokeCap": "butt", "strokeJoin": "miter", "miterLimit": 10, "dashOffset": 0, "opacity": 1}})
    );
    assert_eq!(
        objects[1]["vector"],
        json!({"stroke": {"kind": "solid", "r": 0.06666666666666667, "g": 0.13333333333333333, "b": 0.2, "a": 0.8}, "hasRealStroke": true,
        "strokeWidth": 4.5, "strokeCap": "round", "strokeJoin": "round", "miterLimit": 10, "dashArray": [6, 3], "dashOffset": 2, "opacity": 1})
    );
    assert_eq!(objects[1]["fill"], Value::Null);
    assert_eq!(objects[1]["geometry"]["closed"], false);
    assert_eq!(
        objects[2]["vectorExt"],
        json!({"pressure": {"centerSegments": [
        {"point": {"x": 100, "y": 106}, "handleIn": {"x": 0, "y": 0}, "handleOut": {"x": 10, "y": -2}, "width": 2},
        {"point": {"x": 180, "y": 106}, "handleIn": {"x": -10, "y": 2}, "handleOut": {"x": 0, "y": 0}, "width": 6}],
        "widthProfile": [{"t": 0, "width": 2}, {"t": 1, "width": 6}], "strokeProfile": "taper-both", "profileBase": 6}})
    );
    assert_eq!(objects[2]["target"]["frameScope"]["frame"], 8);
    assert_eq!(
        objects[3]["fill"],
        json!({"kind": "solid", "r": 0.2, "g": 0.4, "b": 0.6, "a": 0.5019607843137255})
    );
    assert_eq!(objects[3]["vector"]["opacity"], 0.5);
    let report = converted.report.to_json();
    assert_eq!(
        report["counts"],
        json!({"layers": 2, "keyframes": 4, "strokes": 4})
    );
    assert_eq!(
        (
            report["sourceVersion"].clone(),
            report["migrations"].clone(),
            report["warnings"].clone()
        ),
        (json!(13.0), json!([]), json!([]))
    );
    assert_eq!(report["warningsTotal"], 0);
    assert_eq!(
        report["ignored"],
        json!([
            "easingCurve",
            "mediaMode",
            "motionBlurSamples",
            "motionBlurShutter",
            "resamplePts",
            "tweenStep"
        ])
    );
}

#[test]
fn composition_order_lists_every_layer_and_each_keyframe_scope_back_to_front() {
    let order = file(&import(ORIGINAL))["drawing"]["compositionOrder"].clone();
    let context = &order["contexts"][0];
    assert_eq!(
        context["layers"],
        json!([{"layerUid": "ly_ink", "naturalPosition": 0, "order": {}}, {"layerUid": "ly_paint", "naturalPosition": 1, "order": {}}])
    );
    let frames: Vec<u64> = context["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["frameScope"]["frame"].as_u64().unwrap())
        .collect();
    assert_eq!(frames, [0, 8, 12]);
    assert_eq!(
        context["scopes"][0]["elements"],
        json!([
        {"layerUid": "ly_ink", "strokeId": "st_rect", "naturalPosition": 0, "order": {}},
        {"layerUid": "ly_ink", "strokeId": "st_line", "naturalPosition": 1, "order": {}},
        {"layerUid": "ly_paint", "strokeId": "st_dot", "naturalPosition": 0, "order": {}}])
    );
}

#[test]
fn frames_only_older_file_gets_the_baseline_defaults_and_generated_identities() {
    let converted = import(OLDER);
    let wire = file(&converted);
    assert_eq!(
        (
            wire["project"]["canvasW"].clone(),
            wire["project"]["canvasH"].clone(),
            wire["project"]["fps"].clone(),
            wire["drawing"]["totalFrames"].clone()
        ),
        (json!(1920), json!(1080), json!(12), json!(3))
    );
    assert_eq!(
        (
            wire["project"]["waIn"].clone(),
            wire["project"]["waOut"].clone()
        ),
        (json!(0), json!(2))
    );
    assert_eq!(converted.report.migrations, ["framesOnly:1", "layerUid:1"]);
    let layer = &wire["drawing"]["layers"][0];
    assert_eq!(
        (
            layer["layerUid"].clone(),
            layer["name"].clone(),
            layer["color"].clone()
        ),
        (json!("ly_legacy_0"), json!("Layer 1"), json!("#5FA875"))
    );
    assert_eq!(layer["timing"]["keys"], json!([{"frame": 0}, {"frame": 2}]));
    let first = &wire["drawing"]["objects"][0];
    assert_eq!(first["target"]["strokeId"], "legacy-stroke:ly_legacy_0:0:0");
    // desP: no hasRealStroke -> the stored colour (`#fff` expands), `strokeWidth || 3`, round caps.
    assert_eq!(
        first["vector"],
        json!({"stroke": {"kind": "solid", "r": 1, "g": 1, "b": 1, "a": 1}, "hasRealStroke": true, "strokeWidth": 3, "strokeCap": "round", "strokeJoin": "round", "opacity": 1})
    );
    assert_eq!(
        first["fill"],
        json!({"kind": "solid", "r": 0, "g": 1, "b": 0, "a": 1})
    );
    assert_eq!(
        warnings(&converted),
        [
            (
                "layers[0].frames[0].strokes[0].strokeId".to_owned(),
                "stroke_id_generated"
            ),
            (
                "layers[0].frames[2].strokes[0].strokeId".to_owned(),
                "stroke_id_generated"
            )
        ]
    );
}

#[test]
fn single_frame_project_takes_the_frame_count_from_its_first_layer() {
    let converted = import(SINGLE);
    let wire = file(&converted);
    assert_eq!(
        (
            wire["drawing"]["totalFrames"].clone(),
            wire["project"]["waOut"].clone(),
            wire["project"]["canvasW"].clone()
        ),
        (json!(1), json!(0), json!(100))
    );
    assert_eq!(
        converted.report.to_json()["counts"],
        json!({"layers": 1, "keyframes": 1, "strokes": 1})
    );
}

#[test]
fn mixed_model_reports_what_the_baseline_never_showed_and_repairs_identity() {
    let converted = import(MIXED);
    let wire = file(&converted);
    assert_eq!(wire["drawing"]["layers"][0]["parentLayerUid"], Value::Null);
    assert_eq!(wire["drawing"]["layers"][0]["color"], "#5FA875");
    assert_eq!(wire["drawing"]["layers"][1]["layerUid"], "ly_legacy_1");
    assert_eq!(wire["drawing"]["layers"][1]["name"], "Layer 2");
    assert_eq!(wire["drawing"]["layers"][1]["color"], "#ABCDEF");
    let ids: Vec<&str> = wire["drawing"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["target"]["strokeId"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["st_rect", "st_rect~1", "legacy-stroke:ly_legacy_1:1:0"]
    );
    assert_eq!(
        warnings(&converted),
        [
            ("layers[0].parentLayerUid".to_owned(), "dangling_parent"),
            (
                "layers[0].frames[0].strokes[1].strokeId".to_owned(),
                "stroke_id_repeated"
            ),
            ("layers[0].frames[2]".to_owned(), "hidden_strokes"),
            ("layers[0].frames[6]".to_owned(), "beyond_total_frames"),
            ("layers[0].color".to_owned(), "unreadable_color"),
            (
                "layers[1].frames[1].strokes[0].strokeId".to_owned(),
                "stroke_id_generated"
            )
        ]
    );
    assert_eq!(converted.report.migrations, ["layerUid:1"]);
}

#[test]
fn future_version_and_unknown_members_load_with_named_warnings() {
    let future = import(FUTURE);
    assert_eq!(
        warnings(&future),
        [
            ("version".to_owned(), "newer_format_version"),
            ("newFeature".to_owned(), "unknown_field")
        ]
    );
    assert_eq!(future.report.source_version, Some(14.0));
    let unknown = import(UNKNOWN);
    assert_eq!(
        warnings(&unknown),
        [
            ("layers[0].badge".to_owned(), "unknown_field"),
            ("layers[0].frames[0].flair".to_owned(), "unknown_field"),
            (
                "layers[0].frames[0].strokes[0].sparkle".to_owned(),
                "unknown_field"
            ),
            ("mystery".to_owned(), "unknown_field")
        ]
    );
    assert!(
        !unknown.report.warnings.iter().any(|w| w.path == "empty"),
        "an empty member carries nothing"
    );
}
