//! Real public palette envelopes on the existing revision/history and snapshot owners.
//! Source-only until a separately booked compile/runtime and profile registration.
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const EDIT: &str = "command.document.object.project.palette.edit";
const READ: &str = "query.document.object.project.palette.read";

fn owner(extra: Value) -> NativeObjectHistory {
    let mut project = json!({"canvasW":64,"canvasH":48,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}});
    NativeObjectHistory::from_project_file("public-palette", wire.to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn command(o: &NativeObjectHistory, id: &str, action: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),"operation":EDIT,
        "payload":{"command":"object.project.palette.edit","action":action}}),
    )
    .unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, q: OpacityRequest) -> Value {
    serde_json::to_value(
        o.dispatch_object_command(&q)
            .unwrap()
            .expect("registered palette edit"),
    )
    .unwrap()
}
fn edit(o: &mut NativeObjectHistory, id: &str, action: Value) -> Value {
    let q = command(o, id, action);
    dispatch(o, q)
}
fn request(p: &ObjectSnapshot, id: &str, selector: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":p.instance_id(),
        "documentId":p.document_id(),"operation":READ,"payload":{"atRevision":p.content_revision(),
        "documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":selector}}),
    )
    .unwrap()
}
fn read(p: &ObjectSnapshot, q: OpacityRequest) -> Value {
    let result = serde_json::to_value(p.dispatch(q).unwrap()).unwrap();
    assert!(serde_json::to_vec(&result).unwrap().len() <= 4096);
    result
}
fn query(p: &ObjectSnapshot, selector: Value) -> Value {
    read(p, request(p, "palette-read", selector))
}
fn one_palette() -> Value {
    json!({"palettes":[{"id":"p","name":"Original","colors":["#AbC","#abc","#AbC"]}]})
}

#[test]
fn public_create_replays_one_commit_and_retains_old_snapshot_and_unrelated_fields() {
    let extra = json!({"workspace":{"panelOrder":null},"guides":{"h":[-0.25]},
        "comments":[{"text":"Keep","authorName":"Writer"}],"bpm":137.5,
        "customBrushPresets":{"brush":{"label":"Preset","nibSize":0.5}},
        "cameraKeys":[{"frame":0.0,"x":12.0,"y":24.0,"w":64.0,"rot":0.0,"hIn":[0.0,0.0]}],
        "cameraLayerOn":false});
    let mut o = owner(extra.clone());
    let old = pin(&o);
    let before = saved(&o);
    let q = command(
        &o,
        "create",
        json!({"kind":"create","id":"p","name":"Palette","color":"#AbC"}),
    );
    let applied = dispatch(&mut o, q.clone());
    assert_eq!(
        applied["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(dispatch(&mut o, q), applied);
    assert_eq!(o.content_revision(), 1);
    assert_eq!(o.history_depths(), (1, 0));
    let current = pin(&o);
    let result = query(&current, json!({"kind":"settings"}));
    assert_eq!(result["contentRevision"], 1);
    assert_eq!(
        result["result"]["documentSnapshotId"],
        current.snapshot_id()
    );
    assert_eq!(result["result"]["result"]["paletteCount"], 1);
    assert_eq!(
        result["result"]["result"]["effectiveActivePaletteIdx"].as_f64(),
        Some(0.)
    );
    let colors = query(
        &current,
        json!({"kind":"colorPage","ordinal":1,"cursor":0,"limit":16}),
    );
    assert_eq!(
        colors["result"]["result"]["items"],
        json!([{"index":1,"color":"#AbC"}])
    );
    assert_eq!(
        query(&old, json!({"kind":"settings"}))["result"]["result"]["palettesPresent"],
        false
    );
    let after = saved(&o);
    let value: Value = serde_json::from_slice(&after).unwrap();
    for (key, expected) in extra.as_object().unwrap() {
        assert_eq!(&value["project"][key], expected);
    }
    let fresh = NativeObjectHistory::from_project_file("palette-reopen", &after).unwrap();
    assert_eq!(saved(&fresh), after);
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    o.redo(2).unwrap();
    assert_eq!(saved(&o), after);
}

#[test]
fn public_palette_operations_use_ordinals_and_preserve_duplicate_ids_and_color_spelling() {
    let mut o = owner(one_palette());
    for (index, action) in [
        json!({"kind":"duplicate","ordinal":1,"id":"p","name":"Copy"}),
        json!({"kind":"rename","ordinal":2,"name":"Renamed"}),
        json!({"kind":"appendColor","ordinal":2,"color":"#ABC"}),
        json!({"kind":"moveColor","ordinal":2,"from":4,"to":1}),
        json!({"kind":"removeColor","ordinal":2,"index":3}),
        json!({"kind":"select","ordinal":1}),
    ]
    .into_iter()
    .enumerate()
    {
        let result = edit(&mut o, &format!("palette-{index}"), action);
        assert_eq!(
            result["result"],
            json!({"applied":true,"historyEntriesAdded":1})
        );
    }
    let p = pin(&o);
    let summaries = query(&p, json!({"kind":"palettePage","cursor":0,"limit":16}));
    assert_eq!(
        summaries["result"]["result"]["items"],
        json!([
        {"ordinal":1,"id":"p","name":"Original","colorCount":3},
        {"ordinal":2,"id":"p","name":"Renamed","colorCount":3}])
    );
    let colors = query(
        &p,
        json!({"kind":"colorPage","ordinal":2,"cursor":0,"limit":16}),
    );
    assert_eq!(
        colors["result"]["result"]["items"],
        json!([
        {"index":1,"color":"#ABC"},{"index":2,"color":"#AbC"},{"index":3,"color":"#AbC"}])
    );
    assert_eq!(
        edit(&mut o, "remove-copy", json!({"kind":"remove","ordinal":2}))["ok"],
        true
    );
    let before = saved(&o);
    let revision = o.content_revision();
    assert_eq!(
        edit(&mut o, "remove-last", json!({"kind":"remove","ordinal":1}))["error"]["code"],
        "invalid_request"
    );
    assert_eq!(o.content_revision(), revision);
    assert_eq!(saved(&o), before);
}

#[test]
fn public_noop_keeps_redo_and_old_settings_namespace_does_not_accept_palette_actions() {
    let mut o = owner(one_palette());
    edit(
        &mut o,
        "rename",
        json!({"kind":"rename","ordinal":1,"name":"Changed"}),
    );
    o.undo(1).unwrap();
    let before = saved(&o);
    let result = edit(
        &mut o,
        "duplicate-color",
        json!({"kind":"appendColor","ordinal":1,"color":"#AbC"}),
    );
    assert_eq!(
        result["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.history_depths(), (0, 1));
    let mut wrong = command(
        &o,
        "old-namespace",
        json!({"kind":"rename","ordinal":1,"name":"Wrong"}),
    );
    wrong.operation = "command.document.object.project.settings.edit".into();
    wrong.payload["command"] = json!("object.project.settings.edit");
    assert_eq!(dispatch(&mut o, wrong)["error"]["code"], "invalid_request");
    assert_eq!(
        edit(&mut o, "foreign-action", json!({"kind":"setBpm","bpm":90}))["error"]["code"],
        "invalid_request"
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 1));
    o.redo(2).unwrap();
    let p = pin(&o);
    assert_eq!(
        query(&p, json!({"kind":"palettePage","cursor":0,"limit":16}))["result"]["result"]["items"]
            [0]["name"],
        "Changed"
    );
}

#[test]
fn public_summary_and_color_pages_make_bounded_forward_progress_without_dropping_values() {
    let colors: Vec<String> = (0..18)
        .map(|i| format!("{i}-{}", "x".repeat(900)))
        .collect();
    let palettes: Vec<Value> = (0..17)
        .map(|i| json!({"id":format!("p{i}"),"name":"Palette","colors":colors}))
        .collect();
    let o = owner(json!({"palettes":palettes}));
    let p = pin(&o);
    let first = query(&p, json!({"kind":"palettePage","cursor":0,"limit":16}));
    assert_eq!(
        first["result"]["result"]["items"].as_array().unwrap().len(),
        16
    );
    assert_eq!(first["result"]["result"]["nextCursor"], 16);
    let last = query(&p, json!({"kind":"palettePage","cursor":16,"limit":16}));
    assert_eq!(last["result"]["result"]["items"][0]["ordinal"], 17);
    assert!(last["result"]["result"]["nextCursor"].is_null());
    let mut cursor = 0;
    let mut collected = Vec::new();
    loop {
        let page = query(
            &p,
            json!({"kind":"colorPage","ordinal":1,"cursor":cursor,"limit":16}),
        );
        assert_eq!(page["ok"], true);
        let body = &page["result"]["result"];
        let items = body["items"].as_array().unwrap();
        assert!(!items.is_empty());
        for item in items {
            collected.push(item["color"].as_str().unwrap().to_owned());
        }
        if body["nextCursor"].is_null() {
            break;
        }
        let next = body["nextCursor"].as_u64().unwrap();
        assert!(next > cursor);
        cursor = next;
    }
    assert_eq!(collected, colors);
}

#[test]
fn public_palette_reads_and_edits_keep_owner_revision_and_cancellation_guards() {
    let mut o = owner(one_palette());
    let before = saved(&o);
    for (field, value, code) in [
        ("instanceId", json!("foreign"), "wrong_instance"),
        ("documentId", json!("foreign"), "wrong_document"),
        ("expectedRevision", json!(1), "stale_revision"),
    ] {
        let q = command(
            &o,
            field,
            json!({"kind":"rename","ordinal":1,"name":"Wrong"}),
        );
        let mut raw = serde_json::to_value(q).unwrap();
        raw[field] = value;
        assert_eq!(
            dispatch(&mut o, serde_json::from_value(raw).unwrap())["error"]["code"],
            code
        );
    }
    let mut cancelled = command(&o, "cancelled", json!({"kind":"remove","ordinal":1}));
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    let old = pin(&o);
    edit(
        &mut o,
        "new-revision",
        json!({"kind":"rename","ordinal":1,"name":"New"}),
    );
    let current = pin(&o);
    assert_eq!(
        read(&current, request(&old, "stale", json!({"kind":"settings"})))["error"]["code"],
        "stale_revision"
    );
    let mut wrong_context = request(&current, "nested", json!({"kind":"settings"}));
    wrong_context.payload["contextId"] = json!("nested");
    assert_eq!(
        read(&current, wrong_context)["error"]["code"],
        "unavailable"
    );
    let mut revision_on_read = request(&current, "read-revision", json!({"kind":"settings"}));
    revision_on_read.expected_revision = Some(current.content_revision());
    assert_eq!(
        read(&current, revision_on_read)["error"]["code"],
        "invalid_request"
    );
}

#[test]
fn public_palette_enforces_complete_request_and_response_budgets_without_partial_mutation() {
    let mut o = owner(one_palette());
    let before = saved(&o);
    let escaped = "\n".repeat(1024);
    let result = edit(
        &mut o,
        "oversize-request",
        json!({"kind":"create","id":escaped,"name":escaped,"color":escaped}),
    );
    assert_eq!(result["error"]["code"], "invalid_request");
    assert_eq!(saved(&o), before);
    let o = owner(json!({"palettes":[{"id":escaped,"name":escaped,"colors":[]}]}));
    let p = pin(&o);
    assert_eq!(query(&p, json!({"kind":"settings"}))["ok"], true);
    assert_eq!(
        query(&p, json!({"kind":"palettePage","cursor":0,"limit":16}))["error"]["code"],
        "unavailable"
    );
    for selector in [
        json!({"kind":"palettePage","cursor":0,"limit":0}),
        json!({"kind":"palettePage","cursor":2,"limit":16}),
        json!({"kind":"colorPage","ordinal":0,"cursor":0,"limit":16}),
        json!({"kind":"settings","extra":true}),
    ] {
        assert_eq!(query(&p, selector)["error"]["code"], "invalid_request");
    }
}
