//! #2256 component writes: the vector edit owner edits a DECLARED component context like the scene (baseline
//! `enterSymbol` swaps `state.layers` for the component's, app.js @1065e9d8): one revision and one undo entry per
//! edit, every instance of the component shows it, the scene root is untouched, and a context the document does not
//! declare (or no longer declares) is refused with nothing prepared.
use super::support::*;
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, PaintEntry,
};
use serde_json::{json, Value};

const CTX: &str = "comp-a";

fn at(layer: &str, frame: u32, stroke: &str) -> Value {
    let mut t = target(layer, frame, stroke);
    t["contextId"] = json!(CTX);
    t
}
fn crec(frame: u32, stroke: &str) -> Value {
    let mut r = record_at("A", "inner", frame, stroke);
    r["target"] = at("inner", frame, stroke);
    r
}
fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|k| json!({"frame":k})).collect::<Vec<_>>(),"samples":[]})
}
fn layers(uids: &[&str]) -> Value {
    json!(uids.iter().enumerate().map(|(i, l)| json!({"layerUid":l,"naturalPosition":i,"order":{}})).collect::<Vec<_>>())
}
fn scope(frame: u32, ids: &[(&str, &str)]) -> Value {
    json!({"frameScope":{"kind":"authored","frame":frame},"elements":ids.iter().enumerate()
        .map(|(i, (l, s))| json!({"layerUid":l,"strokeId":s,"naturalPosition":i,"order":{}})).collect::<Vec<_>>()})
}
/// A scene layer `L1` with one record, two host layers placing component `comp-a` (8 frames, layer `inner`, a record at 0).
fn doc() -> Value {
    let mut d = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":24,
        "layers":[{"layerUid":"L1","timing":timing(&[0])},{"layerUid":"host-a","timing":timing(&[0])},{"layerUid":"host-b","timing":timing(&[0])}],
        "objects":[record_at("A", "L1", 0, "s-a")]});
    d["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[
        {"contextId":"scene-root","layers":layers(&["L1","host-a","host-b"]),"scopes":[scope(0,&[("L1","s-a")])]},
        {"contextId":CTX,"layers":layers(&["inner"]),"scopes":[scope(0,&[("inner","c-a")])]}]});
    d["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":CTX,"totalFrames":8,"layers":[{"layerUid":"inner","timing":timing(&[0])}],
            "objects":[crec(0, "c-a")]}],
        "instances":[
            {"contextId":"scene-root","layerUid":"host-a","symbolId":CTX,"symPlayMode":"loop"},
            {"contextId":"scene-root","layerUid":"host-b","symbolId":CTX,"symPlayMode":"loop"}]}});
    d
}
fn component(state: &Value) -> &Vec<Value> {
    state["componentTables"]["components"]["components"][0]["objects"].as_array().unwrap()
}
fn ids(state: &Value) -> Vec<String> {
    component(state).iter().map(|r| r["target"]["strokeId"].as_str().unwrap().to_string()).collect()
}
/// Paint entries the scene shows for the component at scene frame 0 (one per instance and drawn stroke).
fn painted(owner: &NativeObjectHistory, stroke: &str) -> usize {
    let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let expected = ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_exposure_frame(&s, &expected).unwrap_or_else(|e| panic!("{e:?}"));
    let scene = prepare_object_render_scene(&packet, &expected).unwrap();
    let plan = prepare_object_paint_plan(&scene, &expected).unwrap();
    ordered_entries(&plan)
        .filter(|e| matches!(e, PaintEntry::Component(c) if c.target.context_id() == CTX && c.target.stroke_id() == stroke))
        .count()
}
fn undo(owner: &mut NativeObjectHistory) {
    owner.undo(owner.content_revision()).unwrap();
}
fn scene_part(state: &Value) -> Value {
    json!({"layers":state["layers"],"objects":state["objects"]})
}

#[test]
fn a_create_in_a_component_is_one_revision_one_undo_and_every_instance_shows_it() {
    let mut owner = owner_of(&doc());
    let (before, rev) = (state(&owner), owner.content_revision());
    assert_eq!((painted(&owner, "c-a"), ids(&before)), (2, vec!["c-a".to_string()]));
    let id = created_id(&send(&mut owner, "make", create(crec(0, ""))));
    let after = state(&owner);
    assert_eq!(owner.content_revision(), rev + 1);
    assert_eq!(owner.history_depths(), (1, 0), "one history entry");
    assert_eq!(ids(&after), vec!["c-a".to_string(), id.clone()]);
    assert_eq!(scene_part(&after), scene_part(&before), "the scene root is untouched");
    assert_eq!(painted(&owner, &id), 2, "both instances re-render with the new stroke");
    let kept = bytes(&owner);
    undo(&mut owner);
    assert_eq!(state(&owner), before, "undo restores exactly");
    assert_eq!(painted(&owner, &id), 0);
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), kept, "redo restores exactly");
}

#[test]
fn replace_and_delete_in_a_component_are_one_entry_each_and_undo_exactly() {
    let mut owner = owner_of(&doc());
    let before = bytes(&owner);
    let mut moved = record_at("B", "inner", 0, "c-a");
    moved["target"] = at("inner", 0, "c-a");
    assert_eq!(send(&mut owner, "swap", replace(moved.clone()))["result"]["applied"], true);
    assert_eq!(component(&state(&owner))[0]["fill"], moved["fill"]);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(send(&mut owner, "drop", delete(at("inner", 0, "c-a")))["ok"], true);
    assert!(ids(&state(&owner)).is_empty());
    assert_eq!(painted(&owner, "c-a"), 0);
    assert_eq!(owner.history_depths(), (2, 0));
    undo(&mut owner);
    undo(&mut owner);
    assert_eq!(state(&owner)["objects"], serde_json::from_slice::<Value>(&before).unwrap()["objects"]);
    assert_eq!(state(&owner), serde_json::from_slice::<Value>(&before).unwrap());
}

#[test]
fn a_batch_in_a_component_restyles_and_adds_as_one_entry() {
    let mut owner = owner_of(&doc());
    let before = bytes(&owner);
    let state_before = state(&owner);
    let payload = json!({"command":"object.vector.batch","ops":[
        {"op":"create","record":crec(0, "")},
        {"op":"restyle","stableTarget":at("inner", 0, "c-a"),"style":{"stroke":"#00ff00","width":9}}]});
    let response = send(&mut owner, "batch", payload);
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"]["strokeIds"].as_array().unwrap().len(), 2);
    assert_eq!(owner.history_depths(), (1, 0), "restyle and create are ONE entry");
    let after = state(&owner);
    assert_eq!(ids(&after).len(), 2);
    assert_ne!(component(&after)[0], component(&state_before)[0], "the stroke was restyled");
    let moved = send(&mut owner, "move", json!({"command":"object.vector.batch","ops":[
        {"op":"reshape","stableTarget":at("inner", 0, "c-a"),"matrix":[1,0,0,1,10,5]}]}));
    assert_eq!(moved["ok"], true, "{moved}");
    assert_ne!(component(&state(&owner))[0]["geometry"], component(&after)[0]["geometry"], "Select moved it");
    assert_eq!(owner.history_depths(), (2, 0));
    undo(&mut owner);
    undo(&mut owner);
    assert_eq!(state(&owner)["objects"], serde_json::from_slice::<Value>(&before).unwrap()["objects"]);
    assert_eq!(state(&owner), serde_json::from_slice::<Value>(&before).unwrap());
}

#[test]
fn an_undeclared_or_vanished_context_and_foreign_layers_are_refused_with_nothing_prepared() {
    let mut owner = owner_of(&doc());
    let before = bytes(&owner);
    let rev = owner.content_revision();
    let mut gone = crec(0, "");
    gone["target"]["contextId"] = json!("comp-gone");
    for (id, payload) in [
        ("gone", create(gone)),
        ("scene-layer", create({
            let mut r = crec(0, "");
            r["target"]["layerUid"] = json!("L1");
            r
        })),
        ("frame-past", create(crec(8, ""))),
        ("copy-key", json!({"command":"object.vector.create","record":crec(3, ""),"keyframe":"copy"})),
    ] {
        let response = send(&mut owner, id, payload);
        assert_eq!(response["ok"], false, "{id}: {response}");
        assert_eq!(owner.content_revision(), rev, "{id}");
        assert_eq!(bytes(&owner), before, "{id}");
        assert_eq!(owner.history_depths(), (0, 0), "{id}");
    }
    let missing = send(&mut owner, "ghost", delete(at("inner", 0, "nope")));
    assert_eq!((missing["ok"].clone(), missing["error"]["code"].clone()), (json!(false), json!("not_found")));
    // A scene-root target keeps editing the scene: the component's records are not reachable through it.
    let scene = send(&mut owner, "scene", create(record_at("B", "L1", 1, "")));
    assert_eq!(scene["ok"], true, "{scene}");
    assert_eq!(ids(&state(&owner)), vec!["c-a".to_string()]);
}

#[test]
fn a_held_cell_of_a_component_takes_draws_plain_key() {
    let mut d = doc();
    d["componentTables"]["components"]["components"][0]["layers"][0]["timing"] = timing(&[0, 6]);
    let mut owner = owner_of(&d);
    let id = created_id(&send(&mut owner, "held", create(crec(3, ""))));
    let layer = &state(&owner)["componentTables"]["components"]["components"][0]["layers"][0];
    assert_eq!(layer["timing"]["keys"], json!([{"frame":0},{"frame":3},{"frame":6}]), "frame 3 was a hold: keyed empty");
    assert!(ids(&state(&owner)).contains(&id));
    undo(&mut owner);
    assert_eq!(state(&owner)["componentTables"]["components"]["components"][0]["layers"][0]["timing"]["keys"],
        json!([{"frame":0},{"frame":6}]), "one undo removes the stroke and the key");
}

fn read(owner: &NativeObjectHistory, context: &str, layer: &str, frame: u32, view: Option<&str>) -> Result<Value, crate::request_receipts::DispatchErrorCode> {
    let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let mut selector = json!({"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),
        "contextId":context,"layerUid":layer,"frame":frame});
    let bytes = |v: &Value| v.to_string().into_bytes();
    match view {
        None => s.query_layer_timing_json(&bytes(&selector)).map(|r| serde_json::to_value(r).unwrap()),
        Some(kind) => {
            selector["view"] = json!(kind);
            if kind == "cells" { s.query_layer_cells_json(&bytes(&selector)) } else { s.query_layer_range_json(&bytes(&selector)) }
        }
    }
}

#[test]
fn the_entered_timeline_reads_the_components_own_frames() {
    let mut d = doc();
    d["componentTables"]["components"]["components"][0]["layers"][0]["timing"] = timing(&[0, 4]);
    let owner = owner_of(&d);
    let cell = |frame| read(&owner, CTX, "inner", frame, None).unwrap();
    assert_eq!((cell(0)["cellKind"].clone(), cell(0)["hasContent"].clone()), (json!("key"), json!(true)));
    assert_eq!((cell(2)["cellKind"].clone(), cell(2)["sourceFrame"].clone()), (json!("held"), json!(0)));
    assert_eq!(cell(4)["cellKind"], "key");
    assert_eq!(cell(7)["contextId"], CTX);
    let cells = read(&owner, CTX, "inner", 0, Some("cells")).unwrap();
    assert_eq!(cells["cells"]["runs"].as_array().unwrap().iter().map(|r| r[1].as_u64().unwrap()).sum::<u64>(), 8, "the component's 8 frames, not the scene's 24");
    let range = read(&owner_of(&doc()), CTX, "inner", 0, Some("range")).unwrap();
    assert_eq!((range["range"]["inPoint"].clone(), range["range"]["outPoint"].clone()), (json!(0.0), json!(7.0)));
    assert_eq!(read(&owner, "scene-root", "L1", 23, None).unwrap()["frame"], 23, "the scene keeps its own length");
}

#[test]
fn component_reads_refuse_an_undeclared_context_a_foreign_layer_and_a_frame_past_the_component() {
    use crate::request_receipts::DispatchErrorCode::{InvalidRequest, NotFound, Unavailable};
    let owner = owner_of(&doc());
    for view in [None, Some("cells"), Some("range")] {
        assert_eq!(read(&owner, "comp-gone", "inner", 0, view).unwrap_err(), Unavailable, "{view:?}");
        assert_eq!(read(&owner, CTX, "L1", 0, view).unwrap_err(), NotFound, "{view:?}: a scene layer is not the component's");
    }
    assert_eq!(read(&owner, CTX, "inner", 8, None).unwrap_err(), InvalidRequest);
    assert_eq!(read(&owner, CTX, "inner", 8, Some("cells")).unwrap_err(), InvalidRequest);
}

/// #2256: Select's hit-test listing of the ENTERED component, as the viewport presents it: the component's own layer and records, named by the
/// component context, with the geometry read of the same record; the editable (Subselect) listing stays scene-root.
fn component_view(owner: &NativeObjectHistory, context: &str, layer: &str, hit: bool) -> Result<Value, &'static str> {
    use crate::object_render_scene::prepare_object_viewport_composition;
    let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let expected = ObjectFrameSelector {
        instance_id: s.instance_id(), document_id: s.document_id(), content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(), context_id: context, scope_kind: FrameScopeKind::Authored, frame: 0,
    };
    let packet = prepare_object_exposure_frame(&s, &expected).unwrap_or_else(|e| panic!("{e:?}"));
    let scene = prepare_object_render_scene(&packet, &expected).unwrap();
    let plan = prepare_object_paint_plan(&scene, &expected).unwrap();
    let witness = prepare_object_viewport_composition(&plan, &expected, None).unwrap();
    assert_eq!(witness.context_id(), context);
    let projection = witness.subselect_read_projection(layer, None, hit)?.with_presentation("native-work-1".into(), 1).unwrap();
    let mut view = json!({"kind":"subselect","contextId":context,"layerUid":layer,"frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8});
    if hit { view["hitTest"] = json!(true); }
    let request = json!({"apiVersion":2,"requestId":"hit","instanceId":s.instance_id(),"documentId":s.document_id(),
        "operation":"query.document.object","payload":{"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),"view":view}});
    let response = s.dispatch_with_subselect_view(serde_json::from_value(request).unwrap(), Ok(&projection)).unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    Ok(response["result"].clone())
}

#[test]
fn select_lists_the_entered_components_records_and_reads_their_geometry() {
    let owner = owner_of(&doc());
    let listing = component_view(&owner, CTX, "inner", true).unwrap();
    assert_eq!(listing["contextId"], CTX);
    let items = listing["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{listing}");
    assert_eq!(items[0]["stableTarget"]["contextId"], CTX);
    assert_eq!(items[0]["stableTarget"]["layerUid"], "inner");
    assert_eq!(items[0]["stableTarget"]["strokeId"], "c-a");
    // The geometry read of the same record (what Select pages for the records its click reaches).
    let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let request = json!({"apiVersion":2,"requestId":"geo","instanceId":s.instance_id(),"documentId":s.document_id(),
        "operation":"query.document.object","payload":{"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),
        "stableTarget":items[0]["stableTarget"],"subselect":{"cursor":0,"limit":16,"hitTest":true}}});
    let response = serde_json::to_value(s.dispatch(serde_json::from_value(request.clone()).unwrap()).unwrap()).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"]["subselect"]["stableTarget"]["contextId"], CTX);
    assert_eq!(response["result"]["subselect"]["total"].as_u64().unwrap() as usize, items[0]["count"].as_u64().unwrap() as usize);
    // The editable read of a component record stays refused; the scene root is listed as before.
    let mut editable = request.clone();
    editable["payload"]["subselect"] = json!({"cursor":0,"limit":16});
    let refused = serde_json::to_value(s.dispatch(serde_json::from_value(editable).unwrap()).unwrap()).unwrap();
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(component_view(&owner, CTX, "inner", false).is_err(), "the editable listing is scene-root only");
    let scene = component_view(&owner, "scene-root", "L1", true).unwrap();
    assert_eq!(scene["contextId"], "scene-root");
}
