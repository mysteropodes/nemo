//! #2256 A-comp-edit-timeline: frame-structure and key edits INSIDE a declared component, through the real history owner.
//! Expected values are hand-computed from the authored world below (a component `sym_a`, 4 frames, two layers), never read back
//! from the code under test: insert / extend / duplicate / remove frames, key promote / clear / blank, each with its undo to the
//! original bytes, redo, and save-and-reopen; the scene root and the placement table must be byte-identical around every edit.
use super::{dispatch, head, owner, request};
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

const INSERT: &str = "command.document.object.frames.insert";
const EXTEND: &str = "command.document.object.frames.extend";
const DUPLICATE: &str = "command.document.object.frames.duplicate";
const REMOVE: &str = "command.document.object.frames.remove";
const PROMOTE: &str = "command.document.object.layer.key.promote";
const CLEAR: &str = "command.document.object.layer.key.clear";
const BLANK: &str = "command.document.object.layer.key.blank";

fn timing(keys: &[u32]) -> Value {
    let keys: Vec<Value> = keys.iter().map(|frame| json!({ "frame": frame })).collect();
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":keys,"samples":[]})
}
fn layer(uid: &str, keys: &[u32], locked: bool) -> Value {
    json!({"layerUid":uid,"name":uid,"visible":true,"locked":locked,"color":"#4a9eff","parentLayerUid":null,"timing":timing(keys)})
}
fn square(context: &str, layer: &str, frame: u32, id: &str) -> Value {
    let corner = |x: u32, y: u32| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":context,"frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(0,0),corner(10,0),corner(10,10),corner(0,10)]},
        "fill":{"kind":"solid","r":1,"g":0.5,"b":0,"a":1}})
}
fn order(layers: &[&str], context: &str, scopes: Value) -> Value {
    let rows: Vec<Value> = layers
        .iter()
        .enumerate()
        .map(|(at, uid)| json!({"layerUid":uid,"naturalPosition":at,"order":{}}))
        .collect();
    json!({"contextId":context,"layers":rows,"scopes":scopes})
}
fn element(layer: &str, id: &str) -> Value {
    json!({"layerUid":layer,"strokeId":id,"naturalPosition":0,"order":{}})
}
/// The component's authored order scope: natural (none declared) or explicit, covering every record.
fn inner_order(explicit: bool) -> Vec<Value> {
    if !explicit {
        return vec![];
    }
    let scopes = json!([
        {"frameScope":{"kind":"authored","frame":0},"elements":[element("ly_in_a","s_a0"),element("ly_in_b","s_b0"),element("ly_in_lock","s_l0")]},
        {"frameScope":{"kind":"authored","frame":2},"elements":[element("ly_in_a","s_a2")]}]);
    vec![order(
        &["ly_in_a", "ly_in_b", "ly_in_lock"],
        "sym_a",
        scopes,
    )]
}
/// Scene: 6 frames, one drawing layer `ly_scene` (key 0, no record) and the instance layer `ly_inst` of `sym_a`.
/// Component `sym_a`: 4 frames; `ly_in_a` keys 0 and 2 (a square at each), `ly_in_b` key 0 (a square), `ly_in_lock` locked key 0.
fn world(explicit: bool) -> Value {
    let mut contexts = vec![order(&["ly_scene", "ly_inst"], "scene-root", json!([]))];
    contexts.extend(inner_order(explicit));
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":6,
        "layers":[layer("ly_scene",&[0],false),layer("ly_inst",&[0],false)],
        "objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":contexts,
            "migration":"explicit-plan-required","referencePolicy":"unresolved"},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"sym_a","name":"A","totalFrames":4,"fps":24,
                "layers":[layer("ly_in_a",&[0,2],false),layer("ly_in_b",&[0],false),layer("ly_in_lock",&[0],true)],
                "objects":[square("sym_a","ly_in_a",0,"s_a0"),square("sym_a","ly_in_a",2,"s_a2"),
                    square("sym_a","ly_in_b",0,"s_b0"),square("sym_a","ly_in_lock",0,"s_l0")]}],
            "instances":[{"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_a","symPlayMode":"loop","symSpeed":1,"symPlacedAt":0,"symSingleFrame":0}]}}})
}
pub(super) fn open_as(explicit: bool) -> NativeObjectHistory {
    owner(
        decode_storage_project(world(explicit).to_string().as_bytes())
            .expect("the authored world is admitted"),
    )
}
pub(super) fn open() -> NativeObjectHistory {
    open_as(false)
}
pub(super) fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
pub(super) fn value(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(o)).unwrap()
}
pub(super) fn run(o: &mut NativeObjectHistory, operation: &str, payload: Value) -> Value {
    let submitted = request(o, "ctx", operation, payload);
    dispatch(o, &submitted)
}
pub(super) fn component(doc: &Value) -> &Value {
    &doc["componentTables"]["components"]["components"][0]
}
pub(super) fn keys(doc: &Value, uid: &str) -> Vec<u64> {
    component(doc)["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layerUid"] == uid)
        .unwrap()["timing"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_u64().unwrap())
        .collect()
}
pub(super) fn frames_of(doc: &Value, uid: &str) -> Vec<u64> {
    let mut out: Vec<u64> = component(doc)["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["target"]["layerUid"] == uid)
        .map(|r| r["target"]["frameScope"]["frame"].as_u64().unwrap())
        .collect();
    out.sort_unstable();
    out
}
/// Everything the edit must not touch: the scene layers/objects/length, the scene order context and the placement table.
pub(super) fn outside(doc: &Value) -> Value {
    json!([
        doc["totalFrames"],
        doc["layers"],
        doc["objects"],
        doc["compositionOrder"]["contexts"][0],
        doc["componentTables"]["components"]["instances"]
    ])
}
pub(super) fn scope_frames(doc: &Value) -> Vec<u64> {
    doc["compositionOrder"]["contexts"][1]["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scope| scope["frameScope"]["frame"].as_u64().unwrap())
        .collect()
}
/// One edit, then undo to the original bytes, redo to the same edit, and save-and-reopen. Returns the document after the edit.
fn edit_cycle(operation: &str, payload: Value) -> Value {
    edit_cycle_in(open(), operation, payload)
}
pub(super) fn edit_cycle_in(mut o: NativeObjectHistory, operation: &str, payload: Value) -> Value {
    let (before_bytes, before) = (bytes(&o), value(&o));
    let reply = run(&mut o, operation, payload);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
    let after = value(&o);
    assert_eq!(
        outside(&after),
        outside(&before),
        "the scene and the placement table are untouched"
    );
    let revision = o.undo(1).unwrap();
    assert_eq!(
        bytes(&o),
        before_bytes,
        "one Undo restores the complete bytes"
    );
    o.redo(revision).unwrap();
    assert_eq!(value(&o), after, "Redo reproduces the edit");
    let reopened = decode_storage_project(&bytes(&o)).unwrap();
    assert_eq!(&reopened, head(&o).document(), "save and reopen");
    after
}

#[test]
fn insert_frame_splices_every_unlocked_component_layer_and_grows_the_component_only() {
    // F5 at frame 1: ly_in_a's key 2 moves to 3 (record too), ly_in_b is padded, the locked layer is padded as the baseline does.
    let after = edit_cycle(
        INSERT,
        json!({"command":"object.frames.insert","contextId":"sym_a","cell":1}),
    );
    assert_eq!(component(&after)["totalFrames"], 5);
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 3]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 3]);
    assert_eq!(keys(&after, "ly_in_b"), vec![0]);
    assert_eq!(frames_of(&after, "ly_in_lock"), vec![0]);
    assert_eq!(
        after["totalFrames"], 6,
        "the scene length is not the component's"
    );
    for object in component(&after)["objects"].as_array().unwrap() {
        assert_eq!(
            object["target"]["contextId"], "sym_a",
            "every component record keeps its context"
        );
    }
}

#[test]
fn insert_frame_with_a_named_layer_touches_only_that_layer() {
    let after = edit_cycle(
        INSERT,
        json!({"command":"object.frames.insert","contextId":"sym_a","cell":0,"layerUids":["ly_in_a"]}),
    );
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 3]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 3]);
    assert_eq!(component(&after)["totalFrames"], 5);
}

#[test]
fn extend_exposure_adds_the_count_to_the_component() {
    let after = edit_cycle(
        EXTEND,
        json!({"command":"object.frames.extend","contextId":"sym_a","cell":0,"count":2}),
    );
    assert_eq!(component(&after)["totalFrames"], 6);
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 4]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 4]);
}

#[test]
fn duplicate_key_copies_the_shown_cell_into_a_new_key() {
    let after = edit_cycle(
        DUPLICATE,
        json!({"command":"object.frames.duplicate","contextId":"sym_a","layerUid":"ly_in_a","cell":0}),
    );
    assert_eq!(component(&after)["totalFrames"], 5);
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 1, 3]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 1, 3]);
}

#[test]
fn remove_frame_closes_the_gap_and_shortens_the_component() {
    let after = edit_cycle(
        REMOVE,
        json!({"command":"object.frames.remove","contextId":"sym_a","from":1,"to":1}),
    );
    assert_eq!(component(&after)["totalFrames"], 3);
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 1]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 1]);
}

#[test]
fn key_promote_clear_and_blank_work_on_component_cells() {
    let promoted = edit_cycle(
        PROMOTE,
        json!({"command":"object.layer.key.promote","contextId":"sym_a","layerUids":["ly_in_b"],"cells":[2]}),
    );
    assert_eq!(keys(&promoted, "ly_in_b"), vec![0, 2]);
    let cleared = edit_cycle(
        CLEAR,
        json!({"command":"object.layer.key.clear","contextId":"sym_a","layerUid":"ly_in_a","cell":2}),
    );
    assert_eq!(keys(&cleared, "ly_in_a"), vec![0]);
    assert_eq!(frames_of(&cleared, "ly_in_a"), vec![0]);
    let blank = edit_cycle(
        BLANK,
        json!({"command":"object.layer.key.blank","contextId":"sym_a","layerUid":"ly_in_a","cell":2}),
    );
    assert_eq!(keys(&blank, "ly_in_a"), vec![0, 2]);
    assert_eq!(
        frames_of(&blank, "ly_in_a"),
        vec![0],
        "a blank key authors no record"
    );
}

#[test]
fn the_scene_root_spelled_out_is_the_same_edit_as_the_absent_context() {
    let (mut a, mut b) = (open(), open());
    let plain = run(
        &mut a,
        INSERT,
        json!({"command":"object.frames.insert","cell":0}),
    );
    let named = run(
        &mut b,
        INSERT,
        json!({"command":"object.frames.insert","contextId":"scene-root","cell":0}),
    );
    assert_eq!(
        (plain["ok"].clone(), named["ok"].clone()),
        (json!(true), json!(true)),
        "{plain} {named}"
    );
    assert_eq!(bytes(&a), bytes(&b));
    assert_eq!(value(&a)["totalFrames"], 7);
    assert_eq!(
        component(&value(&a))["totalFrames"],
        4,
        "the component is not touched by a scene edit"
    );
}

#[test]
fn refusals_are_typed_and_leave_the_document_alone() {
    let mut o = open();
    let before = (bytes(&o), o.content_revision(), o.history_depths());
    let cases = [
        // Not a declared context.
        (
            INSERT,
            json!({"command":"object.frames.insert","contextId":"sym_missing","cell":0}),
        ),
        // A scene layer is not a layer of the component.
        (
            DUPLICATE,
            json!({"command":"object.frames.duplicate","contextId":"sym_a","layerUid":"ly_scene","cell":0}),
        ),
        // The cell is judged against the component's own length (4), not the scene's (6).
        (
            INSERT,
            json!({"command":"object.frames.insert","contextId":"sym_a","cell":5}),
        ),
        (
            REMOVE,
            json!({"command":"object.frames.remove","contextId":"sym_a","from":0,"to":9}),
        ),
        // A locked component layer refuses Clear Keyframe as in the scene.
        (
            CLEAR,
            json!({"command":"object.layer.key.clear","contextId":"sym_a","layerUid":"ly_in_lock","cell":0}),
        ),
        // An empty context id is malformed.
        (
            INSERT,
            json!({"command":"object.frames.insert","contextId":"","cell":0}),
        ),
    ];
    for (operation, payload) in cases {
        let reply = run(&mut o, operation, payload.clone());
        assert_eq!(reply["ok"], false, "{payload} -> {reply}");
        assert_eq!(
            (bytes(&o), o.content_revision(), o.history_depths()),
            before,
            "{payload}"
        );
    }
}

#[test]
fn a_semantic_no_op_inside_a_component_publishes_nothing() {
    let mut o = open();
    let reply = run(
        &mut o,
        PROMOTE,
        json!({"command":"object.layer.key.promote","contextId":"sym_a","layerUids":["ly_in_a"],"cells":[0]}),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0}),
        "{reply}"
    );
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
}

#[test]
fn an_explicit_component_order_plan_follows_the_edit_and_an_implicit_one_stays_implicit() {
    let explicit = edit_cycle_in(
        open_as(true),
        INSERT,
        json!({"command":"object.frames.insert","contextId":"sym_a","cell":1}),
    );
    assert_eq!(
        scope_frames(&explicit),
        vec![0, 3],
        "the scope of the moved key follows it"
    );
    assert_eq!(frames_of(&explicit, "ly_in_a"), vec![0, 3]);
    let implicit = edit_cycle(
        INSERT,
        json!({"command":"object.frames.insert","contextId":"sym_a","cell":1}),
    );
    assert_eq!(
        implicit["compositionOrder"]["contexts"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "no inner plan is invented"
    );
}
