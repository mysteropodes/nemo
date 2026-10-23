//! Original custom preset metadata intents through public native dispatch/history.
//! Custom vector brush execution remains a separate provider dependency.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const NUMERIC_FIELDS: &[&str] = &[
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
];
fn all_fields() -> Value {
    let mut preset = serde_json::Map::new();
    for (index, field) in NUMERIC_FIELDS.iter().enumerate() {
        preset.insert((*field).into(), json!(-(index as f64) - 0.25));
    }
    preset.extend(
        json!({"rotationMode":"","scatterDistribution":"unknown-import",
        "tipShape":"custom","label":"","markerTip":false,
        "customStamp":{"segments":[{"point":[-0.5,0.25]},
            {"point":[0.5,-0.25],"handleIn":[0.0,0.0],"handleOut":[-0.25,0.5]}],
            "closed":false,"pointCount":-2.5}})
        .as_object()
        .unwrap()
        .clone(),
    );
    assert_eq!(preset.len(), 36);
    Value::Object(preset)
}
fn wire(catalog: Option<&str>, extra: Value) -> Vec<u8> {
    let mut project = json!({"canvasW":100,"canvasH":80,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    if catalog.is_some() {
        project["customBrushPresets"] = json!("__CUSTOM_CATALOG__");
    }
    let text = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}})
    .to_string();
    catalog
        .map_or(text.clone(), |raw| {
            text.replace("\"__CUSTOM_CATALOG__\"", raw)
        })
        .into_bytes()
}
fn owner(catalog: Option<&str>, extra: Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("custom-brush", &wire(catalog, extra)).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn payload(action: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"command":"object.project.customBrushPreset.edit","action":action}))
        .unwrap()
}
fn command(o: &NativeObjectHistory, id: &str, action: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":"command.document.object.project.customBrushPreset.edit",
        "payload":{"command":"object.project.customBrushPreset.edit","action":action}}),
    )
    .unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, request: OpacityRequest) -> Value {
    let response = o
        .dispatch_object_command(&request)
        .unwrap()
        .expect("registered custom preset edit");
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn edit(o: &mut NativeObjectHistory, id: &str, action: Value) -> Value {
    let request = command(o, id, action);
    dispatch(o, request)
}
fn request(p: &ObjectSnapshot, id: &str, selector: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":p.instance_id(),
        "documentId":p.document_id(),"operation":"query.document.object.project.customBrushPreset.read",
        "payload":{"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),
            "contextId":"scene-root","selector":selector}})).unwrap()
}
fn read(p: &ObjectSnapshot, q: OpacityRequest) -> Value {
    let response = p.dispatch(q).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn query(p: &ObjectSnapshot, selector: Value) -> Value {
    read(p, request(p, "custom-read", selector))
}
fn ids(o: &NativeObjectHistory) -> Vec<String> {
    pin(o)
        .project()
        .unwrap()
        .project()
        .custom_brush_presets()
        .unwrap()
        .entries()
        .iter()
        .map(|entry| entry.0.clone())
        .collect()
}
#[test]
fn upsert_update_delete_replay_old_pin_history_and_fresh_reopen_preserve_other_allocations() {
    let mut o = owner(
        Some(r#"{"zebra":{"label":"Z"},"alpha":{"label":"A"}}"#),
        json!({
        "workspace":{"panelOrder":null},"comments":[{"authorId":"one","authorName":"Writer",
            "authorColor":"#AbC","text":"Keep\n日本語","resolved":false}],
        "shadowPalette":[{"id":"same","color":"#AbC"},{"id":"same","color":"#abc"}],
        "perspectiveMode":"custom","perspectiveDensity":-3.5,"guides":{"h":[-0.25]}}),
    );
    let old = pin(&o);
    let before = saved(&o);
    let annotations = old.project().unwrap().project().annotations();
    let workspace_ptr = annotations.0.unwrap() as *const _;
    let comments_ptr = annotations.1.unwrap().as_ptr();
    let q = command(
        &o,
        "update",
        json!({"kind":"upsert","id":"zebra","preset":all_fields()}),
    );
    let first = dispatch(&mut o, q.clone());
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(ids(&o), vec!["zebra", "alpha"]);
    assert_eq!(
        query(&pin(&o), json!({"kind":"record","id":"zebra"}))["result"]["result"]["preset"],
        all_fields()
    );
    edit(
        &mut o,
        "append",
        json!({"kind":"upsert","id":"middle","preset":{}}),
    );
    assert_eq!(ids(&o), vec!["zebra", "alpha", "middle"]);
    let appended = saved(&o);
    edit(&mut o, "delete", json!({"kind":"delete","id":"alpha"}));
    assert_eq!(ids(&o), vec!["zebra", "middle"]);
    assert_eq!(o.history_depths(), (3, 0));
    let changed = saved(&o);
    let current = pin(&o);
    let annotations = current.project().unwrap().project().annotations();
    assert_eq!(annotations.0.unwrap() as *const _, workspace_ptr);
    assert_eq!(annotations.1.unwrap().as_ptr(), comments_ptr);
    let mut next: Value = serde_json::from_slice(&changed).unwrap();
    let original: Value = serde_json::from_slice(&before).unwrap();
    next["project"]["customBrushPresets"] = original["project"]["customBrushPresets"].clone();
    assert_eq!(next, original);
    assert_eq!(dispatch(&mut o, q), first);
    assert_eq!(o.history_depths(), (3, 0));
    assert_eq!(
        query(&old, json!({"kind":"record","id":"zebra"}))["result"]["result"]["preset"],
        json!({"label":"Z"})
    );
    o.undo(3).unwrap();
    assert_eq!(saved(&o), appended);
    let noop = edit(
        &mut o,
        "noop-update",
        json!({"kind":"upsert","id":"alpha","preset":{"label":"A"}}),
    );
    assert_eq!(
        noop["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.history_depths(), (2, 1));
    let noop = edit(
        &mut o,
        "noop-delete",
        json!({"kind":"delete","id":"missing"}),
    );
    assert_eq!(noop["result"]["applied"], false);
    assert_eq!(o.content_revision(), 4);
    o.redo(4).unwrap();
    assert_eq!(saved(&o), changed);
    let fresh = NativeObjectHistory::from_project_file("custom-reopen", &changed).unwrap();
    assert_eq!(saved(&fresh), changed);
}
#[test]
fn every_optional_field_and_empty_stamp_or_label_survives_without_merge_defaults_or_selection() {
    let mut o = owner(None, json!({}));
    let complete = all_fields();
    for field in complete.as_object().unwrap().keys() {
        let mut sparse = complete.clone();
        sparse.as_object_mut().unwrap().remove(field);
        edit(
            &mut o,
            &format!("omit-{field}"),
            json!({"kind":"upsert","id":"raw","preset":sparse}),
        );
        assert_eq!(
            query(&pin(&o), json!({"kind":"record","id":"raw"}))["result"]["result"]["preset"],
            sparse
        );
    }
    for (index, preset) in [json!({}), json!({"label":""}), json!({"markerTip":false}),
        json!({"rotationMode":"","scatterDistribution":"","tipShape":"","label":""}),
        json!({"customStamp":{"segments":[]}}),
        json!({"customStamp":{"segments":[{"point":[0.0,0.0]}]}}),
        json!({"customStamp":{"segments":[{"point":[0.0,0.0],"handleIn":[0.0,0.0],"handleOut":[0.0,0.0]}],"closed":false,"pointCount":0.0}})]
        .into_iter().enumerate()
    {
        edit(&mut o, &format!("empty-{index}"), json!({"kind":"upsert","id":"\u{0000}日本語","preset":preset}));
        assert_eq!(query(&pin(&o), json!({"kind":"record","id":"\u{0000}日本語"}))["result"]["result"]["preset"], preset);
    }
    let value: Value = serde_json::from_slice(&saved(&o)).unwrap();
    for field in ["brushPreset", "brushSize", "favorites", "toolPresets"] {
        assert!(value["project"].get(field).is_none());
    }
}
#[test]
fn missing_and_empty_catalogs_and_missing_delete_retain_presence_and_noop_redo() {
    for catalog in [None, Some("{}")] {
        let mut o = owner(catalog, json!({}));
        let before = saved(&o);
        assert_eq!(
            query(&pin(&o), json!({"kind":"settings"}))["result"]["result"],
            json!({"customBrushPresetsPresent":catalog.is_some(),"presetCount":0})
        );
        assert_eq!(
            edit(&mut o, "missing", json!({"kind":"delete","id":"none"}))["result"],
            json!({"applied":false,"historyEntriesAdded":0})
        );
        assert_eq!(saved(&o), before);
        assert_eq!(
            query(&pin(&o), json!({"kind":"record","id":"none"}))["error"]["code"],
            "not_found"
        );
        edit(
            &mut o,
            "create-empty-id",
            json!({"kind":"upsert","id":"","preset":{}}),
        );
        assert_eq!(ids(&o), vec![""]);
        edit(&mut o, "remove-last", json!({"kind":"delete","id":""}));
        assert_eq!(
            query(&pin(&o), json!({"kind":"settings"}))["result"]["result"],
            json!({"customBrushPresetsPresent":true,"presetCount":0})
        );
        o.undo(2).unwrap();
        o.undo(3).unwrap();
        assert_eq!(saved(&o), before);
        assert_eq!(
            edit(
                &mut o,
                "missing-after-undo",
                json!({"kind":"delete","id":"none"})
            )["result"]["applied"],
            false
        );
        assert_eq!(o.history_depths(), (0, 2));
        o.redo(4).unwrap();
        assert_eq!(ids(&o), vec![""]);
    }
}
#[path = "object_custom_brush_read.rs"]
mod read_tests;
