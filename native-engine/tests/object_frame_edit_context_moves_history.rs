//! #2256 A-comp-edit-timeline part 2: key move, whole-cell Move, Repeat Selection, the frame clipboard, layer policy and visibility,
//! layer lifecycle and the playhead cut INSIDE a declared component, through the real history owner. The authored world is the one of
//! `object_frame_edit_context_history.rs` (component `sym_a`, 4 frames; `ly_in_a` keys 0 and 2 with a square each, `ly_in_b` key 0,
//! locked `ly_in_lock` key 0; scene of 6 frames). Expected values are hand-computed, never read back from the code under test; each
//! edit is undone to the original bytes, redone, and saved and reopened, and the scene and the placement table stay byte-identical.
use super::context_history::{
    bytes, component, frames_of, keys, open, open_as, outside, run, scope_frames, value,
};
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

const KEY_MOVE: &str = "command.document.object.layer.key.move";
const MOVE: &str = "command.document.object.frames.move";
const REPEAT: &str = "command.document.object.frames.repeat";
const COPY: &str = "command.document.object.frames.clipboard.copy";
const CUT: &str = "command.document.object.frames.clipboard.cut";
const PASTE: &str = "command.document.object.frames.clipboard.paste";
const DUPLICATE_SEL: &str = "command.document.object.frames.clipboard.duplicate";
const POLICY: &str = "command.document.object.layer.policy.edit";
const VISIBILITY: &str = "command.document.object.layer.visibility.set";
const CREATE: &str = "command.document.object.layer.create";
const DELETE: &str = "command.document.object.layer.delete";
const DUPLICATE: &str = "command.document.object.layer.duplicate";
const REORDER: &str = "command.document.object.layer.reorder";
const CUT_LAYER: &str = "command.document.object.layer.cut";

fn selection(active: &str) -> Value {
    json!({"active":active,"selected":[active],"anchor":null})
}
fn layer_uids(doc: &Value) -> Vec<String> {
    component(doc)["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap().to_owned())
        .collect()
}
fn layer_field(doc: &Value, uid: &str, field: &str) -> Value {
    component(doc)["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layerUid"] == uid)
        .unwrap()[field]
        .clone()
}
/// One edit, then undo to the original bytes, redo to the same edit, and save-and-reopen. Returns (reply, document after).
fn cycle(mut o: NativeObjectHistory, operation: &str, payload: Value) -> (Value, Value) {
    let (before_bytes, before) = (bytes(&o), value(&o));
    let reply = run(&mut o, operation, payload.clone());
    assert_eq!(reply["ok"], true, "{payload} -> {reply}");
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert_eq!(reply["result"]["historyEntriesAdded"], 1, "{reply}");
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
    assert_eq!(&reopened, super::head(&o).document(), "save and reopen");
    (reply, after)
}
/// A second request on the same owner needs its own request id (a repeated id is a replay, not a new request).
fn step(o: &mut NativeObjectHistory, id: &str, operation: &str, payload: Value) -> Value {
    let submitted = super::request(o, id, operation, payload);
    super::dispatch(o, &submitted)
}
fn refused(operation: &str, payload: Value) {
    let mut o = open();
    let before = (bytes(&o), o.content_revision(), o.history_depths());
    let reply = run(&mut o, operation, payload.clone());
    assert_eq!(reply["ok"], false, "{payload} -> {reply}");
    assert_eq!(
        (bytes(&o), o.content_revision(), o.history_depths()),
        before,
        "{payload}"
    );
}

#[test]
fn key_move_retimes_the_component_key_and_its_drawing() {
    // The key at 2 of ly_in_a dragged to 3 (the last frame of the 4-frame component).
    let (_, after) = cycle(
        open(),
        KEY_MOVE,
        json!({"command":"object.layer.key.move","contextId":"sym_a","layerUid":"ly_in_a","activeLayerUid":"ly_in_a","from":2,"to":3}),
    );
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 3]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 3]);
    assert_eq!(component(&after)["totalFrames"], 4);
    assert_eq!(
        after["totalFrames"], 6,
        "the scene length is not the component's"
    );
}

#[test]
fn key_move_refusals_are_typed_and_publish_nothing() {
    // A locked component layer refuses; a scene layer is not a component layer; the target is judged against the component (4).
    refused(
        KEY_MOVE,
        json!({"command":"object.layer.key.move","contextId":"sym_a","layerUid":"ly_in_lock","from":0,"to":1}),
    );
    refused(
        KEY_MOVE,
        json!({"command":"object.layer.key.move","contextId":"sym_a","layerUid":"ly_scene","from":0,"to":1}),
    );
    refused(
        KEY_MOVE,
        json!({"command":"object.layer.key.move","contextId":"sym_a","layerUid":"ly_in_a","from":2,"to":5}),
    );
    refused(
        KEY_MOVE,
        json!({"command":"object.layer.key.move","contextId":"sym_missing","layerUid":"ly_in_a","from":2,"to":3}),
    );
}

#[test]
fn whole_cell_move_carries_the_cell_to_another_component_layer() {
    // The key cell (ly_in_a, 2) dropped on (ly_in_b, 3): ly_in_a loses the key (its first key holds on), ly_in_b gains it.
    let (reply, after) = cycle(
        open(),
        MOVE,
        json!({"command":"object.frames.move","contextId":"sym_a","selection":[{"layerUid":"ly_in_a","frame":2}],"layerUid":"ly_in_b","frame":3}),
    );
    assert_eq!(
        reply["result"]["selection"],
        json!([{"layerUid":"ly_in_b","frame":3}]),
        "the moved selection is named with the component's own layers"
    );
    assert_eq!(keys(&after, "ly_in_a"), vec![0]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0]);
    assert_eq!(keys(&after, "ly_in_b"), vec![0, 3]);
    assert_eq!(frames_of(&after, "ly_in_b"), vec![0, 3]);
}

#[test]
fn whole_cell_move_refuses_a_scene_layer_and_a_locked_destination_row() {
    refused(
        MOVE,
        json!({"command":"object.frames.move","contextId":"sym_a","selection":[{"layerUid":"ly_scene","frame":0}],"layerUid":"ly_in_b","frame":3}),
    );
    refused(
        MOVE,
        json!({"command":"object.frames.move","contextId":"sym_a","selection":[{"layerUid":"ly_in_a","frame":2}],"layerUid":"ly_scene","frame":3}),
    );
}

#[test]
fn repeat_selection_cycles_the_range_and_grows_only_the_component() {
    // Frames 0..2 of ly_in_a (key, hold, key) repeated twice: span 3, so frames 3..8, and the component grows 4 -> 9.
    let (reply, after) = cycle(
        open(),
        REPEAT,
        json!({"command":"object.frames.repeat","contextId":"sym_a","fromLayerUid":"ly_in_a","toLayerUid":"ly_in_a","fromFrame":0,"toFrame":2,"times":2}),
    );
    assert_eq!(reply["result"]["cycledLayers"], 1, "{reply}");
    assert_eq!(reply["result"]["skippedComponentLayers"], 0, "{reply}");
    assert_eq!(component(&after)["totalFrames"], 9);
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 2, 3, 5, 6, 8]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 2, 3, 5, 6, 8]);
    assert_eq!(after["totalFrames"], 6, "the scene keeps its own length");
}

fn ids(o: &NativeObjectHistory) -> (String, String) {
    (o.instance_id().to_owned(), o.document_id().to_owned())
}

#[test]
fn duplicate_selection_pastes_the_copy_after_the_selection_inside_the_component() {
    let (_, after) = cycle(
        open(),
        DUPLICATE_SEL,
        json!({"command":"object.frames.clipboard.duplicate","contextId":"sym_a","layerUid":"ly_in_a","selection":[{"layerUid":"ly_in_a","frame":0}]}),
    );
    assert_eq!(keys(&after, "ly_in_a"), vec![0, 1, 2]);
    assert_eq!(frames_of(&after, "ly_in_a"), vec![0, 1, 2]);
    assert_eq!(
        component(&after)["totalFrames"],
        4,
        "a selection duplicate never grows the timeline"
    );
}

#[test]
fn cut_blanks_the_unlocked_cell_and_returns_the_packet_then_paste_lands_it_elsewhere() {
    let mut o = open();
    let before = bytes(&o);
    let reply = run(
        &mut o,
        CUT,
        json!({"command":"object.frames.clipboard.cut","contextId":"sym_a","selection":[{"layerUid":"ly_in_a","frame":2}]}),
    );
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["result"]["applied"], true);
    let packet = reply["result"]["clipboard"].clone();
    assert_eq!(packet["cells"][0]["keyframe"], true, "{packet}");
    assert_eq!(packet["cells"][0]["records"].as_array().unwrap().len(), 1);
    let cut = value(&o);
    assert_eq!(keys(&cut, "ly_in_a"), vec![0]);
    assert_eq!(frames_of(&cut, "ly_in_a"), vec![0]);
    // Paste the packet on (ly_in_b, 2).
    let pasted = step(
        &mut o,
        "paste",
        PASTE,
        json!({"command":"object.frames.clipboard.paste","contextId":"sym_a","instanceId":packet["instanceId"],"documentId":packet["documentId"],
            "layerUid":"ly_in_b","frame":2,"cells":packet["cells"]}),
    );
    assert_eq!(pasted["ok"], true, "{pasted}");
    let after = value(&o);
    assert_eq!(keys(&after, "ly_in_b"), vec![0, 2]);
    assert_eq!(frames_of(&after, "ly_in_b"), vec![0, 2]);
    assert_eq!(
        outside(&after),
        outside(&value(&open())),
        "the scene is untouched by both edits"
    );
    // Two edits, two entries; undo twice restores the original bytes.
    assert_eq!(o.history_depths(), (2, 0));
    let current = o.content_revision();
    let r = o.undo(current).unwrap();
    o.undo(r).unwrap();
    assert_eq!(bytes(&o), before);
}

#[test]
fn copy_reads_the_component_cells_and_changes_nothing() {
    let mut o = open();
    let before = (bytes(&o), o.content_revision(), o.history_depths());
    let reply = run(
        &mut o,
        COPY,
        json!({"command":"object.frames.clipboard.copy","contextId":"sym_a","selection":[{"layerUid":"ly_in_a","frame":0},{"layerUid":"ly_in_b","frame":0}]}),
    );
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["result"]["applied"], false);
    let cells = reply["result"]["clipboard"]["cells"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(cells.len(), 2, "{reply}");
    assert_eq!(
        cells
            .iter()
            .map(|c| (
                c["relLayer"].clone(),
                c["relFrame"].clone(),
                c["keyframe"].clone()
            ))
            .collect::<Vec<_>>(),
        vec![
            (json!(0), json!(0), json!(true)),
            (json!(1), json!(0), json!(true))
        ]
    );
    assert_eq!(
        (bytes(&o), o.content_revision(), o.history_depths()),
        before
    );
    // A scene layer is not a layer of the component.
    refused(
        COPY,
        json!({"command":"object.frames.clipboard.copy","contextId":"sym_a","selection":[{"layerUid":"ly_scene","frame":0}]}),
    );
}

#[test]
fn a_scene_clipboard_pastes_into_the_component_by_relative_cell() {
    // The baseline pastes by relative layer index into whichever context is open: copy (ly_scene, 0) in the scene, paste at (ly_in_b, 3).
    let mut o = open();
    let copied = step(
        &mut o,
        "copy",
        COPY,
        json!({"command":"object.frames.clipboard.copy","selection":[{"layerUid":"ly_scene","frame":0}]}),
    );
    assert_eq!(copied["ok"], true, "{copied}");
    let packet = copied["result"]["clipboard"].clone();
    let (instance, document) = ids(&o);
    assert_eq!(
        (packet["instanceId"].clone(), packet["documentId"].clone()),
        (json!(instance), json!(document))
    );
    let pasted = step(
        &mut o,
        "paste",
        PASTE,
        json!({"command":"object.frames.clipboard.paste","contextId":"sym_a","instanceId":packet["instanceId"],"documentId":packet["documentId"],
            "layerUid":"ly_in_b","frame":3,"cells":packet["cells"]}),
    );
    assert_eq!(pasted["ok"], true, "{pasted}");
    let after = value(&o);
    assert_eq!(keys(&after, "ly_in_b"), vec![0, 3]);
    assert_eq!(outside(&after), outside(&value(&open())));
}

#[test]
fn policy_and_visibility_change_the_component_layer_only() {
    let (_, locked) = cycle(
        open(),
        POLICY,
        json!({"command":"object.layer.policy.edit","contextId":"sym_a","layerUid":"ly_in_a","flag":"locked","value":true}),
    );
    assert_eq!(layer_field(&locked, "ly_in_a", "locked"), json!(true));
    assert_eq!(layer_field(&locked, "ly_in_b", "locked"), json!(false));
    let (_, shy) = cycle(
        open(),
        POLICY,
        json!({"command":"object.layer.policy.edit","contextId":"sym_a","layerUid":"ly_in_b","flag":"shy","value":true}),
    );
    assert_eq!(layer_field(&shy, "ly_in_b", "shy"), json!(true));
    let (_, hidden) = cycle(
        open(),
        VISIBILITY,
        json!({"command":"object.layer.visibility.set","contextId":"sym_a","layerUid":"ly_in_b","visible":false}),
    );
    assert_eq!(layer_field(&hidden, "ly_in_b", "visible"), json!(false));
    // Unlocking the locked layer is the same one-entry edit; a flag already at its value is a no-op.
    let (_, unlocked) = cycle(
        open(),
        POLICY,
        json!({"command":"object.layer.policy.edit","contextId":"sym_a","layerUid":"ly_in_lock","flag":"locked","value":false}),
    );
    assert_eq!(layer_field(&unlocked, "ly_in_lock", "locked"), json!(false));
    let mut o = open();
    let reply = run(
        &mut o,
        POLICY,
        json!({"command":"object.layer.policy.edit","contextId":"sym_a","layerUid":"ly_in_a","flag":"locked","value":false}),
    );
    assert_eq!(reply["result"]["applied"], false, "{reply}");
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    refused(
        POLICY,
        json!({"command":"object.layer.policy.edit","contextId":"sym_a","layerUid":"ly_scene","flag":"locked","value":true}),
    );
}

#[test]
fn create_adds_a_blank_layer_on_top_of_the_component_stack() {
    let (reply, after) = cycle(
        open(),
        CREATE,
        json!({"command":"object.layer.create","contextId":"sym_a","name":"Fresh"}),
    );
    assert_eq!(reply["result"]["layerUid"], "native-layer:1:0", "{reply}");
    assert_eq!(
        layer_uids(&after),
        vec!["ly_in_a", "ly_in_b", "ly_in_lock", "native-layer:1:0"]
    );
    assert_eq!(
        layer_field(&after, "native-layer:1:0", "name"),
        json!("Fresh")
    );
    assert_eq!(
        after["layers"].as_array().unwrap().len(),
        2,
        "the scene stack is untouched"
    );
}

#[test]
fn create_in_a_component_with_an_authored_order_plan_extends_that_plan_only() {
    let (_, after) = cycle(
        open_as(true),
        CREATE,
        json!({"command":"object.layer.create","contextId":"sym_a","name":"Fresh"}),
    );
    let inner: Vec<String> = after["compositionOrder"]["contexts"][1]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        inner,
        vec!["ly_in_a", "ly_in_b", "ly_in_lock", "native-layer:1:0"]
    );
    assert_eq!(
        scope_frames(&after),
        vec![0, 2],
        "the existing scopes are kept"
    );
}

#[test]
fn delete_duplicate_and_reorder_work_on_component_layers() {
    let (_, deleted) = cycle(
        open(),
        DELETE,
        json!({"command":"object.layer.delete","contextId":"sym_a","selection":selection("ly_in_b")}),
    );
    assert_eq!(layer_uids(&deleted), vec!["ly_in_a", "ly_in_lock"]);
    assert_eq!(
        frames_of(&deleted, "ly_in_b"),
        Vec::<u64>::new(),
        "its drawing goes with it"
    );

    let (reply, duplicated) = cycle(
        open(),
        DUPLICATE,
        json!({"command":"object.layer.duplicate","contextId":"sym_a","layerUid":"ly_in_a"}),
    );
    let copy = reply["result"]["layerUid"].as_str().unwrap().to_owned();
    assert_eq!(layer_uids(&duplicated).len(), 4, "{reply}");
    assert!(layer_uids(&duplicated).contains(&copy));
    assert_eq!(keys(&duplicated, &copy), vec![0, 2]);
    assert_eq!(frames_of(&duplicated, &copy), vec![0, 2]);

    // Move ly_in_lock to the very top of the stack (gap 0 is above the first layer).
    let (_, reordered) = cycle(
        open(),
        REORDER,
        json!({"command":"object.layer.reorder","contextId":"sym_a","layerUids":["ly_in_lock"],"gap":0,"selection":selection("ly_in_lock")}),
    );
    assert_eq!(
        layer_uids(&reordered),
        vec!["ly_in_lock", "ly_in_a", "ly_in_b"]
    );
}

#[test]
fn lifecycle_refusals_are_typed() {
    // A component keeps at least one layer: deleting them one by one refuses the last, with the document, revision and history alone.
    let mut o = open();
    for (id, uid) in [("d1", "ly_in_a"), ("d2", "ly_in_b")] {
        let reply = step(
            &mut o,
            id,
            DELETE,
            json!({"command":"object.layer.delete","contextId":"sym_a","selection":selection(uid)}),
        );
        assert_eq!(reply["ok"], true, "{reply}");
    }
    let before = (bytes(&o), o.content_revision(), o.history_depths());
    let last = step(
        &mut o,
        "d3",
        DELETE,
        json!({"command":"object.layer.delete","contextId":"sym_a","selection":selection("ly_in_lock")}),
    );
    assert_eq!(last["ok"], false, "{last}");
    assert_eq!(last["error"]["code"], "invalid_request", "{last}");
    assert_eq!(
        (bytes(&o), o.content_revision(), o.history_depths()),
        before
    );
    assert_eq!(layer_uids(&value(&o)), vec!["ly_in_lock"]);
    // A scene layer is not a component layer, an undeclared component is unknown.
    refused(
        DUPLICATE,
        json!({"command":"object.layer.duplicate","contextId":"sym_a","layerUid":"ly_scene"}),
    );
    refused(
        CREATE,
        json!({"command":"object.layer.create","contextId":"sym_missing","name":"x"}),
    );
    refused(
        REORDER,
        json!({"command":"object.layer.reorder","contextId":"sym_a","layerUids":["ly_scene"],"gap":0,"selection":selection("ly_in_a")}),
    );
}

#[test]
fn the_playhead_cut_splits_a_component_layer_at_the_frame() {
    // ly_in_a: keys 0 and 2, drawing at both. Cut at frame 2.
    let (_, after) = cycle(
        open(),
        CUT_LAYER,
        json!({"command":"object.layer.cut","contextId":"sym_a","layerUid":"ly_in_a","frame":2}),
    );
    let layers = layer_uids(&after);
    assert_eq!(
        layers[1], "native-layer:1:0",
        "the new layer sits above its source"
    );
    // Baseline splitLayerAtPlayhead: the full authored content is cloned and the ranges are hard-cut at the frame, against the
    // component's own length (4), not the scene's (6).
    assert_eq!(
        layer_field(&after, "ly_in_a", "range"),
        json!({"outPoint":1})
    );
    assert_eq!(
        layer_field(&after, "native-layer:1:0", "range"),
        json!({"inPoint":2,"outPoint":3})
    );
    assert_eq!(keys(&after, "native-layer:1:0"), vec![0, 2]);
    assert_eq!(frames_of(&after, "native-layer:1:0"), vec![0, 2]);
    assert_eq!(layers.len(), 4, "{layers:?}");
    assert_eq!(
        after["layers"].as_array().unwrap().len(),
        2,
        "the scene stack is untouched"
    );
}
