//! #2256 A-comp-edit-timeline part 3: the layer bar (`layer.range.drag`), its context menu (`layer.range.arrange`) and the shift form
//! of the bulk move INSIDE a declared component, through the real history owner. The authored world is the one of
//! `object_frame_edit_context_history.rs` (component `sym_a`, 4 frames; `ly_in_a` keys 0 and 2, `ly_in_b` key 0, locked `ly_in_lock`).
//! Expected values are hand-computed: an unauthored layer spans 0..total-1 (here 0..3); each edit is undone to the original bytes,
//! redone, and saved and reopened, and the scene and the placement table stay byte-identical.
use super::context_history::{bytes, component, frames_of, keys, open, outside, run, value};
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

const DRAG: &str = "command.document.object.layer.range.drag";
const ARRANGE: &str = "command.document.object.layer.range.arrange";

fn drag(layer: &str, part: &str, samples: &[i64]) -> Value {
    json!({"command":"object.layer.range.drag","contextId":"sym_a","layerUid":layer,"activeLayerUid":layer,
        "part":part,"barSelection":[],"samples":samples,"plainClick":false,"alt":false,"frame":0,"selectedKeys":[]})
}
fn arrange(action: &str, layers: &[&str]) -> Value {
    json!({"command":"object.layer.range.arrange","contextId":"sym_a","layerUids":layers,"action":action,"frame":0})
}
fn range_of(doc: &Value, uid: &str) -> Value {
    component(doc)["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layerUid"] == uid)
        .unwrap()["range"]
        .clone()
}
/// One edit, then undo to the original bytes, redo to the same edit, and save-and-reopen. Returns the document after the edit.
fn cycle(mut o: NativeObjectHistory, operation: &str, payload: Value) -> Value {
    let (before_bytes, before) = (bytes(&o), value(&o));
    let reply = run(&mut o, operation, payload.clone());
    assert_eq!(reply["ok"], true, "{payload} -> {reply}");
    assert_eq!(reply["result"]["historyEntriesAdded"], 1, "{reply}");
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
    let after = value(&o);
    assert_eq!(outside(&after), outside(&before), "scene and placements untouched");
    let revision = o.undo(1).unwrap();
    assert_eq!(bytes(&o), before_bytes, "one Undo restores the complete bytes");
    o.redo(revision).unwrap();
    assert_eq!(value(&o), after, "Redo reproduces the edit");
    let reopened = decode_storage_project(&bytes(&o)).unwrap();
    assert_eq!(&reopened, super::head(&o).document(), "save and reopen");
    after
}
fn refused(operation: &str, payload: Value) {
    let mut o = open();
    let before = (bytes(&o), o.content_revision(), o.history_depths());
    let reply = run(&mut o, operation, payload.clone());
    assert_eq!(reply["ok"], false, "{payload} -> {reply}");
    assert_eq!((bytes(&o), o.content_revision(), o.history_depths()), before);
}

#[test]
fn body_drag_moves_the_component_bar_and_retimes_its_drawn_keys() {
    // ly_in_a spans 0..3 and draws at 0 and 2; the whole bar dragged by +1 spans 1..4 and its keys retime to 1 and 3.
    let after = cycle(open(), DRAG, drag("ly_in_a", "both", &[1]));
    assert_eq!(range_of(&after, "ly_in_a"), json!({"inPoint":1,"outPoint":4}));
    assert_eq!(keys(&after, "ly_in_a"), vec![1, 3]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![1, 3]);
    assert_eq!(keys(&after, "ly_in_b"), vec![0], "the other layers stay");
    assert_eq!(component(&after)["totalFrames"], 4);
    assert_eq!(after["totalFrames"], 6, "the scene length is not the component's");
}

#[test]
fn edge_drag_moves_only_the_in_point_and_the_content_stays() {
    let after = cycle(open(), DRAG, drag("ly_in_a", "in", &[2]));
    assert_eq!(range_of(&after, "ly_in_a")["inPoint"], 2);
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 2]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 2]);
}

#[test]
fn a_group_drag_moves_every_selected_component_bar() {
    let mut payload = drag("ly_in_a", "both", &[1]);
    payload["barSelection"] = json!([{"layerUid":"ly_in_a","part":"both"},{"layerUid":"ly_in_b","part":"both"}]);
    let after = cycle(open(), DRAG, payload);
    assert_eq!(range_of(&after, "ly_in_a")["inPoint"], 1);
    assert_eq!(range_of(&after, "ly_in_b")["inPoint"], 1);
    assert_eq!(keys(&after, "ly_in_b"), vec![1]);
}

#[test]
fn bar_drag_refusals_are_typed_and_publish_nothing() {
    // A scene layer is not a component layer; an undeclared context; a frame beyond the component (4 frames).
    refused(DRAG, drag("ly_scene", "both", &[1]));
    let mut missing = drag("ly_in_a", "both", &[1]);
    missing["contextId"] = json!("sym_missing");
    refused(DRAG, missing);
    let mut late = drag("ly_in_a", "both", &[1]);
    late["frame"] = json!(4);
    refused(DRAG, late);
}

#[test]
fn arrange_menu_runs_inside_the_component() {
    // Reset needs only one bar: first author a range through the drag, then clear it through the menu in a second step.
    let mut o = open();
    assert_eq!(run(&mut o, DRAG, drag("ly_in_a", "in", &[1]))["ok"], true);
    let authored = value(&o);
    assert_eq!(range_of(&authored, "ly_in_a")["inPoint"], 1);
    let payload = arrange("reset", &["ly_in_a"]);
    let submitted = super::request(&o, "ctx2", ARRANGE, payload);
    let reply = super::dispatch(&mut o, &submitted);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(range_of(&value(&o), "ly_in_a"), Value::Null, "the range record is gone");
    assert_eq!(o.history_depths(), (2, 0));
}

#[test]
fn arrange_align_left_and_stagger_place_component_bars() {
    // ly_in_a authored at 1..3 (in dragged +1); align-left puts ly_in_b on the same in point (width preserved: 0..3 -> 1..4).
    let mut o = open();
    assert_eq!(run(&mut o, DRAG, drag("ly_in_a", "in", &[1]))["ok"], true);
    let submitted = super::request(&o, "ctx2", ARRANGE, arrange("alignRight", &["ly_in_a", "ly_in_b"]));
    let reply = super::dispatch(&mut o, &submitted);
    assert_eq!(reply["ok"], true, "{reply}");
    let after = value(&o);
    assert_eq!(range_of(&after, "ly_in_a")["outPoint"], 3);
    assert_eq!(range_of(&after, "ly_in_b"), json!({"inPoint":0,"outPoint":3}));
    let before = bytes(&o);
    let revision = o.undo(o.content_revision()).unwrap();
    o.redo(revision).unwrap();
    assert_eq!(bytes(&o), before);
    let mut stagger = arrange("stagger", &["ly_in_a", "ly_in_b"]);
    stagger["step"] = json!(1);
    let mut p = open();
    assert_eq!(run(&mut p, ARRANGE, stagger)["ok"], true);
    assert_eq!(range_of(&value(&p), "ly_in_b")["inPoint"], 1);
}

#[test]
fn arrange_refusals_are_typed_and_publish_nothing() {
    refused(ARRANGE, arrange("alignLeft", &["ly_in_a", "ly_scene"]));
    let mut missing = arrange("reset", &["ly_in_a"]);
    missing["contextId"] = json!("sym_missing");
    refused(ARRANGE, missing);
}

#[test]
fn the_shift_form_of_the_bulk_move_runs_on_the_component() {
    // The content-only shift of the layer bar: ly_in_a draws at 0 and 2, shifted by +1 it draws at 1 and 3 (the component has 4 frames).
    let mut o = open();
    let before = bytes(&o);
    let payload = json!({"command":"object.layer.frames.shift","contextId":"sym_a","layerUid":"ly_in_a","delta":1});
    let candidate = o.owner.prepare_bulk_frame_edit_json(payload.to_string().as_bytes()).unwrap();
    assert!(candidate.document_changed());
    o.owner.commit_bulk_frame_edit(candidate).unwrap();
    let after = value(&o);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![1, 3]);
    assert_eq!(frames_of(&after, "ly_in_b"), vec![0]);
    assert_eq!(outside(&after), outside(&serde_json::from_slice(&before).unwrap()));
    let refusal = json!({"command":"object.layer.frames.shift","contextId":"sym_missing","layerUid":"ly_in_a","delta":1});
    assert!(o.owner.prepare_bulk_frame_edit_json(refusal.to_string().as_bytes()).is_err());
}

#[test]
fn split_into_layers_runs_inside_the_component() {
    // ly_in_a draws two shapes at its key 0 (s_a0, s_a0b): the split makes one layer per shape; the last piece keeps the source uid.
    let mut doc = value(&open());
    let objects = doc["componentTables"]["components"]["components"][0]["objects"].as_array_mut().unwrap();
    let mut twin = objects[0].clone();
    twin["target"]["strokeId"] = json!("s_a0b");
    objects.push(twin);
    let owner = super::owner(decode_storage_project(doc.to_string().as_bytes()).unwrap());
    let payload = json!({"command":"object.layer.split","contextId":"sym_a","layerUid":"ly_in_a","frame":0,
        "labels":{"shape":"Shape","stroke":"Stroke"}});
    let after = cycle(owner, "command.document.object.layer.split", payload);
    let layers = component(&after)["layers"].as_array().unwrap();
    assert_eq!(layers.len(), 4, "one new layer beside ly_in_a, ly_in_b, ly_in_lock");
    let layer_of = |stroke: &str| -> String {
        component(&after)["objects"].as_array().unwrap().iter().find(|r| r["target"]["strokeId"] == stroke).unwrap()["target"]["layerUid"].as_str().unwrap().to_owned()
    };
    assert_eq!(layer_of("s_a0b"), "ly_in_a", "the last piece keeps the source uid");
    assert_ne!(layer_of("s_a0"), "ly_in_a", "the first shape moved to its own new layer");
    assert_eq!(after["layers"].as_array().unwrap().len(), 2, "the scene layers are untouched");
}

#[test]
fn split_refuses_an_undeclared_context_and_a_scene_layer() {
    for (context, layer) in [("sym_missing", "ly_in_a"), ("sym_a", "ly_scene")] {
        refused(
            "command.document.object.layer.split",
            json!({"command":"object.layer.split","contextId":context,"layerUid":layer,"frame":0,"labels":{"shape":"S","stroke":"T"}}),
        );
    }
}

#[test]
fn a_component_layer_without_timing_records_is_refused_like_the_scene_layer() {
    // The owner has authored no timing for ly_in_b: the bar gesture needs it, so it is the same typed refusal as for a scene layer.
    let mut doc = value(&open());
    doc["componentTables"]["components"]["components"][0]["layers"][1].as_object_mut().unwrap().remove("timing");
    let mut o = super::owner(decode_storage_project(doc.to_string().as_bytes()).unwrap());
    let before = (bytes(&o), o.content_revision(), o.history_depths());
    let reply = run(&mut o, DRAG, drag("ly_in_b", "both", &[1]));
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["error"]["code"], "unavailable");
    assert_eq!((bytes(&o), o.content_revision(), o.history_depths()), before);
    let mut scene = value(&open());
    scene["layers"][0].as_object_mut().unwrap().remove("timing");
    let mut o = super::owner(decode_storage_project(scene.to_string().as_bytes()).unwrap());
    let mut payload = drag("ly_scene", "both", &[1]);
    payload["contextId"] = json!("scene-root");
    assert_eq!(run(&mut o, DRAG, payload)["error"]["code"], "unavailable", "the scene answers the same");
}

#[test]
fn a_nested_component_instance_is_not_admitted_so_no_bar_edit_can_meet_one() {
    // The storage codec refuses a component that hosts an instance of another component, so the bar edits never meet one.
    let mut doc = value(&open());
    let tables = &mut doc["componentTables"]["components"];
    let inner = json!({"layerUid":"ly_nested","name":"Nested","visible":true,"locked":false,"color":"#4a9eff","parentLayerUid":null,
        "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}});
    tables["components"][0]["layers"].as_array_mut().unwrap().push(inner.clone());
    let mut leaf = inner.clone();
    leaf["layerUid"] = json!("ly_leaf");
    tables["components"].as_array_mut().unwrap().push(json!({"symbolId":"sym_b","name":"B","totalFrames":2,"fps":24,"layers":[leaf],"objects":[]}));
    tables["instances"].as_array_mut().unwrap().push(json!({"contextId":"sym_a","layerUid":"ly_nested","symbolId":"sym_b","symPlayMode":"loop","symSpeed":1,"symPlacedAt":0,"symSingleFrame":0}));
    let error = decode_storage_project(doc.to_string().as_bytes()).unwrap_err();
    assert_eq!(error.to_string().contains("nested components are unavailable"), true, "{error:?}");
}
