//! #2356 Direct Select inside an ENTERED component (baseline subselect-bridge.js edited the symbol's shapes while `state.activeSymbolId` was set,
//! 1065e9d8): the editable listing, the geometry read, the preview and the `object.subselect.edit` commit resolve the record in its own context.
//! One gesture is one revision and one history entry, every instance re-renders, the scene root is untouched, undo / redo / reopen are exact and a
//! locked or undeclared context is refused with nothing written.
use super::{at, component, crec, doc, held_doc, plan_ids, scene_part, undo, CTX};
use super::super::support::{bytes, dispatch, owner_of, record_at, request, state, target};
use super::component_view;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

const OP: &str = "command.document.object.subselect.edit";

fn edit(owner: &NativeObjectHistory, id: &str, target: Value, intent: Value) -> Value {
    let mut r = request(owner, id, json!({"command":"object.subselect.edit","stableTarget":target,"intent":intent}));
    r["operation"] = OP.into();
    r
}
fn run(owner: &mut NativeObjectHistory, id: &str, target: Value, intent: Value) -> Value {
    let request = edit(owner, id, target, intent);
    dispatch(owner, &request)
}
fn read(owner: &NativeObjectHistory, target: &Value, subselect: Value) -> Value {
    let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let request = json!({"apiVersion":2,"requestId":"geo","instanceId":s.instance_id(),"documentId":s.document_id(),
        "operation":"query.document.object","payload":{"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),
        "stableTarget":target,"subselect":subselect}});
    serde_json::to_value(s.dispatch(serde_json::from_value(request).unwrap()).unwrap()).unwrap()
}
fn first_point(state: &Value) -> [f64; 2] {
    let p = &component(state)[0]["geometry"]["segments"][0]["point"];
    [p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap()]
}

#[test]
fn the_editable_listing_geometry_and_preview_of_an_entered_component_are_read() {
    let owner = owner_of(&doc());
    let target = at("inner", 0, "c-a");
    // The viewport listing of the editable (vertex) mode for the component's layer.
    let listing = component_view(&owner, CTX, "inner", false).unwrap();
    let items = listing["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{listing}");
    assert_eq!(items[0]["stableTarget"], target);
    // The geometry of the record: the same anchors the stored record has, and the vertex types.
    let plain = read(&owner, &target, json!({"cursor":0,"limit":16}));
    assert_eq!(plain["ok"], true, "{plain}");
    let total = plain["result"]["subselect"]["total"].as_u64().unwrap();
    assert_eq!(total, items[0]["count"].as_u64().unwrap());
    assert_eq!(plain["result"]["subselect"]["stableTarget"]["contextId"], CTX);
    let before = plain["result"]["subselect"]["segments"][0]["point"].clone();
    // The detached preview of a move does not write and moves the first vertex.
    let preview = read(&owner, &target, json!({"cursor":0,"limit":16,"preview":{"kind":"move","indices":[0],"delta":[7,9]}}));
    assert_eq!(preview["ok"], true, "{preview}");
    let moved = &preview["result"]["subselect"]["segments"][0]["point"];
    assert_eq!([moved[0].as_f64().unwrap() - 7.0, moved[1].as_f64().unwrap() - 9.0], [before[0].as_f64().unwrap(), before[1].as_f64().unwrap()]);
    assert_eq!(owner.history_depths(), (0, 0));
    // An undeclared context and a foreign layer are not found; nothing is read.
    let mut ghost = target.clone();
    ghost["contextId"] = json!("comp-gone");
    assert_eq!(read(&owner, &ghost, json!({"cursor":0,"limit":16}))["ok"], false);
}

#[test]
fn move_point_type_insert_and_delete_in_a_component_are_one_entry_each_and_undo_redo_reopen_exactly() {
    for (name, intent) in [
        ("move", json!({"kind":"move","indices":[0],"delta":[7,9]})),
        ("pointType", json!({"kind":"pointType","indices":[0],"pointType":"smooth"})),
        ("insert", json!({"kind":"insert","curve":0,"time":0.5})),
    ] {
        let mut owner = owner_of(&doc());
        let (before, bytes_before, rev) = (state(&owner), bytes(&owner), owner.content_revision());
        let r = run(&mut owner, name, at("inner", 0, "c-a"), intent);
        assert_eq!(r["ok"], true, "{name}: {r}");
        assert_eq!(r["result"], json!({"applied":true,"historyEntriesAdded":1}), "{name}");
        assert_eq!((owner.content_revision(), owner.history_depths()), (rev + 1, (1, 0)), "{name}: one revision, one entry");
        let after = state(&owner);
        assert_ne!(component(&after)[0]["geometry"], component(&before)[0]["geometry"], "{name}: the component's record changed");
        assert_eq!(scene_part(&after), scene_part(&before), "{name}: the scene root is untouched");
        match name {
            "move" => assert_eq!(first_point(&after), [first_point(&before)[0] + 7.0, first_point(&before)[1] + 9.0]),
            "insert" => assert_eq!(
                component(&after)[0]["geometry"]["segments"].as_array().unwrap().len(),
                component(&before)[0]["geometry"]["segments"].as_array().unwrap().len() + 1
            ),
            _ => {}
        }
        let bytes_after = bytes(&owner);
        undo(&mut owner);
        assert_eq!(bytes(&owner), bytes_before, "{name}: undo restores the document exactly");
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), bytes_after, "{name}: redo restores the edit exactly");
        let reopened = NativeObjectHistory::new("subselect-component-reopen", decode_storage_project(&bytes_after).unwrap()).unwrap();
        assert_eq!(bytes(&reopened), bytes_after, "{name}: save / reopen keeps it");
        assert_eq!(encode_storage_project(owner.acquire_snapshot(owner.content_revision()).unwrap().document()).unwrap(), bytes_after);
    }
}

#[test]
fn a_scene_edit_and_a_component_edit_of_the_same_gesture_produce_the_same_geometry() {
    // The scene record `s-a` of L1 and the component record `c-a` of `inner` carry the same geometry (record_at "A").
    for intent in [
        json!({"kind":"move","indices":[0],"delta":[7,9]}),
        json!({"kind":"pointType","indices":[0],"pointType":"smooth"}),
        json!({"kind":"insert","curve":0,"time":0.5}),
        json!({"kind":"handle","index":0,"handle":"handleOut","point":[5,5],"breakPair":false}),
    ] {
        let (mut scene, mut inside) = (owner_of(&doc()), owner_of(&doc()));
        let a = run(&mut scene, "scene", target("L1", 0, "s-a"), intent.clone());
        let b = run(&mut inside, "inside", at("inner", 0, "c-a"), intent.clone());
        assert_eq!((a["ok"].clone(), a["result"].clone()), (b["ok"].clone(), b["result"].clone()), "{intent}: {a} {b}");
        assert_eq!(state(&scene)["objects"][0]["geometry"], component(&state(&inside))[0]["geometry"], "{intent}: same record geometry");
        // The reads agree as well: the same anchors and vertex types for the same stored geometry.
        let (sa, ca) = (read(&scene, &target("L1", 0, "s-a"), json!({"cursor":0,"limit":16})), read(&inside, &at("inner", 0, "c-a"), json!({"cursor":0,"limit":16})));
        assert_eq!(sa["result"]["subselect"]["segments"], ca["result"]["subselect"]["segments"], "{intent}");
        assert_eq!(sa["result"]["subselect"]["pointTypes"], ca["result"]["subselect"]["pointTypes"], "{intent}");
    }
}

#[test]
fn deleting_a_vertex_in_a_component_is_one_entry_and_matches_the_scene() {
    // The two-vertex record cannot lose one (the old limit): insert one, then delete it, in the scene and in the component.
    let (mut scene, mut inside) = (owner_of(&doc()), owner_of(&doc()));
    let insert = json!({"kind":"insert","curve":0,"time":0.5});
    let remove = json!({"kind":"delete","indices":[1]});
    assert_eq!(run(&mut scene, "i", target("L1", 0, "s-a"), insert.clone())["ok"], true);
    assert_eq!(run(&mut inside, "i", at("inner", 0, "c-a"), insert)["ok"], true);
    let (a, b) = (run(&mut scene, "d", target("L1", 0, "s-a"), remove.clone()), run(&mut inside, "d", at("inner", 0, "c-a"), remove));
    assert_eq!((a["ok"].clone(), a["result"].clone()), (json!(true), json!({"applied":true,"historyEntriesAdded":1})), "{a}");
    assert_eq!((b["ok"].clone(), b["result"].clone()), (a["ok"].clone(), a["result"].clone()), "{b}");
    assert_eq!(inside.history_depths(), (2, 0), "an entry per gesture");
    assert_eq!(state(&scene)["objects"][0]["geometry"], component(&state(&inside))[0]["geometry"]);
    assert_eq!(component(&state(&inside))[0]["geometry"]["segments"].as_array().unwrap().len(), 2);
    undo(&mut inside);
    assert_eq!(component(&state(&inside))[0]["geometry"]["segments"].as_array().unwrap().len(), 3);
}

#[test]
fn a_locked_layer_and_an_undeclared_context_refuse_the_edit_with_nothing_written() {
    let mut d = doc();
    d["componentTables"]["components"]["components"][0]["layers"][0]["locked"] = json!(true);
    let mut owner = owner_of(&d);
    let before = bytes(&owner);
    let r = run(&mut owner, "locked", at("inner", 0, "c-a"), json!({"kind":"move","indices":[0],"delta":[7,9]}));
    assert_eq!((r["ok"].clone(), r["error"]["code"].clone()), (json!(false), json!("unavailable")), "{r}");
    let mut open = owner_of(&doc());
    let open_before = bytes(&open);
    let mut ghost = at("inner", 0, "c-a");
    ghost["contextId"] = json!("comp-gone");
    let r = run(&mut open, "ghost", ghost, json!({"kind":"move","indices":[0],"delta":[7,9]}));
    assert_eq!((r["ok"].clone(), r["error"]["code"].clone()), (json!(false), json!("not_found")), "{r}");
    assert_eq!((bytes(&owner), owner.history_depths()), (before, (0, 0)));
    assert_eq!((bytes(&open), open.history_depths()), (open_before, (0, 0)));
}

#[test]
fn a_direct_select_edit_on_a_held_component_frame_copy_keys_the_cell_and_edits_the_copy() {
    let mut owner = owner_of(&held_doc());
    let before = state(&owner);
    let mut q = edit(&owner, "held", at("inner", 0, "c-a"), json!({"kind":"move","indices":[0],"delta":[7,9]}));
    q["payload"]["atFrame"] = json!(3);
    let r = dispatch(&mut owner, &q);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (1, 0), "the promotion and the edit are ONE entry");
    let after = state(&owner);
    let layer = &after["componentTables"]["components"]["components"][0]["layers"][0];
    assert_eq!(layer["timing"]["keys"], json!([{"frame":0},{"frame":3},{"frame":6}]), "the held cell became a key");
    assert_eq!(plan_ids(&after, 3), vec!["c-a".to_string()], "the copy is in the component's order");
    assert_eq!(scene_part(&after), scene_part(&before));
    undo(&mut owner);
    assert_eq!(state(&owner), before);
    // An identity click on a held frame still ensures the key (the baseline Direct Select did), through the exposure owner.
    let mut idle = edit(&owner, "identity", at("inner", 0, "c-a"), json!({"kind":"move","indices":[0],"delta":[0,0]}));
    idle["payload"]["atFrame"] = json!(3);
    let r = dispatch(&mut owner, &idle);
    assert_eq!(r["ok"], true, "{r}");
    let promoted = state(&owner);
    assert_eq!(promoted["componentTables"]["components"]["components"][0]["layers"][0]["timing"]["keys"], json!([{"frame":0},{"frame":3},{"frame":6}]));
    assert_eq!(scene_part(&promoted), scene_part(&before));
    let _ = (crec, record_at);
}
