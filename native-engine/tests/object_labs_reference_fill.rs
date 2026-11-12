//! U36 (#2362) reference fill through the real dispatcher: the ordinary native bucket fill on the target cell with every
//! other effectively visible layer's shown strokes as walls (`src/js/labs/reference-fill.js` @1065e9d8). The fill keeps
//! its seed and gap but no wall ids; a miss changes nothing; hidden layers and hidden folders are not walls.
use crate::history::object_history::{object_boolean_ops_tests as f, NativeObjectHistory};
use serde_json::{json, Value};

fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|k| json!({"frame":k})).collect::<Vec<_>>(),"samples":[]})
}
/// L1 holds a stroked 40x40 square at frame 0; L0, the target, is empty.
fn owner(edit: impl Fn(&mut Value)) -> NativeObjectHistory {
    let wall = f::vector(
        "L1",
        "wall",
        0,
        &f::rect(0.0, 0.0, 40.0, 40.0),
        &f::VectorSpec {
            fill: None,
            stroke: Some(f::BLUE),
            ..Default::default()
        },
    );
    let mut d = f::document(2, vec![wall], None, None);
    for layer in d["layers"].as_array_mut().unwrap() {
        layer["timing"] = timing(&[0]);
    }
    edit(&mut d);
    f::owner_of(&d)
}
fn run(o: &mut NativeObjectHistory, operation: &str, payload: Value) -> Value {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let request = json!({"apiVersion":2,"requestId":format!("rf-{id}"),"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),"operation":operation,"payload":payload});
    let typed = serde_json::from_value(request).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
const OP: &str = "command.document.object.labs.referenceFill";
fn payload(frame: u32) -> Value {
    json!({"command":"object.labs.referenceFill","layerUid":"L0","frame":frame,"point":[20,20],"color":"#336699ff","opacity":0.5})
}
fn fills(o: &NativeObjectHistory) -> Vec<Value> {
    f::state(o)["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["vectorExt"]["fillTrace"].is_object())
        .cloned()
        .collect()
}

#[test]
fn another_layers_stroke_bounds_the_fill_which_keeps_its_seed_but_no_wall_ids() {
    let mut o = owner(|_| {});
    let before = f::bytes(&o);
    let ordinary = run(
        &mut o,
        "command.document.object.fill.trace",
        json!({"command":"object.fill.trace","layerUid":"L0",
        "contextId":"scene-root","frame":0,"point":[20,20],"color":"#336699ff","opacity":0.5,"remove":false}),
    );
    assert_eq!(
        ordinary["result"]["applied"], false,
        "the ordinary fill sees no wall on its own layer"
    );
    assert_eq!(f::bytes(&o), before);
    let reply = run(&mut o, OP, payload(0));
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    let fill = &fills(&o)[0];
    assert_eq!(fill["target"]["layerUid"], "L0");
    assert_eq!(
        fill["vectorExt"]["fillTrace"],
        json!({"seed":[20.0,20.0],"gapPx":0})
    );
    assert_eq!(fill["vector"]["opacity"], 0.5);
    assert!(fill["vector"]["stroke"].is_null());
    assert_eq!(fill["geometry"]["segments"].as_array().unwrap().len(), 4);
    let wall = f::state(&o)["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["target"]["strokeId"] == "wall")
        .cloned()
        .unwrap();
    assert_eq!(
        wall["target"]["layerUid"], "L1",
        "the wall stays where it was, unchanged"
    );
    let edited = f::bytes(&o);
    let undone = o.undo(o.content_revision()).unwrap();
    assert_eq!(f::bytes(&o), before);
    o.redo(undone).unwrap();
    assert_eq!(f::bytes(&o), edited);
}

#[test]
fn hidden_layers_and_hidden_folders_are_not_walls_and_a_miss_changes_nothing() {
    for hide in [
        (|d: &mut Value| d["layers"][1]["visible"] = json!(false)) as fn(&mut Value),
        |d: &mut Value| {
            d["layers"][1]["parentLayerUid"] = json!("F");
            d["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":"F","name":"Folder","visible":false,"isFolderLayer":true}));
            d["compositionOrder"]["contexts"][0]["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":"F","naturalPosition":2,"order":{}}));
        },
    ] {
        let mut o = owner(hide);
        let before = f::bytes(&o);
        let reply = run(&mut o, OP, payload(0));
        assert_eq!(
            reply["result"],
            json!({"applied":false,"historyEntriesAdded":0}),
            "{reply}"
        );
        assert_eq!((f::bytes(&o), o.history_depths()), (before, (0, 0)));
    }
}

#[test]
fn a_held_wall_cell_bounds_the_fill_and_the_held_target_is_promoted_in_the_same_entry() {
    let mut o = owner(|_| {});
    let reply = run(&mut o, OP, payload(2));
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert_eq!(o.history_depths(), (1, 0));
    let fill = &fills(&o)[0];
    assert_eq!(fill["target"]["frameScope"]["frame"], 2);
    let state = f::state(&o);
    assert!(
        state["layers"][0]["timing"]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["frame"] == 2),
        "ensureKeyframe on the target"
    );
}

#[test]
fn refusals_publish_nothing() {
    for (edit, payload_edit, code) in [
        (
            (|_: &mut Value| {}) as fn(&mut Value),
            json!({"layerUid":"L9"}),
            "not_found",
        ),
        (
            |d: &mut Value| d["layers"][0]["locked"] = json!(true),
            json!({}),
            "unavailable",
        ),
        (
            |_: &mut Value| {},
            json!({"color":"#336699"}),
            "invalid_request",
        ),
        (|_: &mut Value| {}, json!({"frame":4}), "invalid_request"),
        (|_: &mut Value| {}, json!({"extra":1}), "invalid_request"),
    ] {
        let mut o = owner(edit);
        let before = f::bytes(&o);
        let mut p = payload(0);
        for (k, v) in payload_edit.as_object().unwrap() {
            p[k] = v.clone();
        }
        let reply = run(&mut o, OP, p);
        assert_eq!(reply["error"]["code"], code, "{reply}");
        assert_eq!((f::bytes(&o), o.history_depths()), (before, (0, 0)));
    }
}
