//! Motion-track repeat joined to Repeat Selection (L05-J1) through the real history owner and dispatcher: the cells, the
//! property tracks and the timeline length of one cycle land in ONE candidate, ONE history entry and ONE Undo; a track that
//! cannot be admitted refuses the whole cycle; tracks alone make a cycle a change. Needs L05-J1 (hard order dependency).
use super::{build, cell};
use crate::commands::{clamp_times, repeat_payload, RepeatRequest};
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use build::{render_layer, Build};
use serde_json::{json, Value};

fn track(keys: Value) -> Value {
    json!({"keys":keys})
}
fn holder(layer: &str, keys: Value) -> Value {
    json!({"target":{"contextId":"scene-root","layerUid":layer},"properties":[{"property":"rotation","track":track(keys)}]})
}
/// L0: keys a (frame 0) and b (frame 2) over four frames; L1: blank keys at 0 and 3. `holders` become the property holders.
fn document_with(holders: Option<Value>) -> ObjectDocument {
    let layers = vec![
        vec![cell('K', "a"), cell('.', ""), cell('K', "b"), cell('.', "")],
        vec![cell('K', ""), cell('.', ""), cell('.', ""), cell('K', "")],
    ];
    let mut value = build::document_json(&layers, Build::V1);
    if let Some(holders) = holders {
        value["propertyHolders"] = holders;
    }
    build::decode(&value)
}
/// L0: a key at frame 0 and holds after it; L1 as in `document_with`.
fn sparse_with(holders: Option<Value>) -> ObjectDocument {
    let layers = vec![
        vec![cell('K', "a"), cell('.', ""), cell('.', ""), cell('.', "")],
        vec![cell('K', ""), cell('.', ""), cell('.', ""), cell('K', "")],
    ];
    let mut value = build::document_json(&layers, Build::V1);
    if let Some(holders) = holders {
        value["propertyHolders"] = holders;
    }
    build::decode(&value)
}
fn owner(document: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("tracks-join", document).unwrap()
}
fn head(o: &NativeObjectHistory) -> crate::revision::ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    serde_json::to_value(head(o).document()).unwrap()
}
fn cycle(layer: &str, first: u32, last: u32, times: i64) -> RepeatRequest {
    RepeatRequest {
        from_layer: layer.to_owned(),
        to_layer: layer.to_owned(),
        from_frame: first,
        to_frame: last,
        times,
    }
}
fn frames_and_values(keys: &Value) -> Vec<(i64, i64)> {
    keys.as_array()
        .unwrap()
        .iter()
        .map(|key| {
            (
                key["frame"].as_i64().unwrap(),
                key["v"][0].as_i64().unwrap(),
            )
        })
        .collect()
}
fn holders() -> Value {
    json!([
        holder("L0", json!([{"frame":0,"v":[10]},{"frame":1,"v":[20]}])),
        holder("L1", json!([{"frame":0,"v":[5]}]))
    ])
}

#[test]
fn a_cycle_repeats_cells_and_tracks_in_one_history_entry_and_one_undo_restores_the_bytes() {
    let mut o = owner(document_with(Some(holders())));
    let before = bytes(&o);
    let before_wire = wire(&o);
    // L0 frames 0..=1 once: span 2, so frame 2 becomes a copy of frame 0 and frame 3 of frame 1; keys 0 and 1 are
    // cloned at 2 and 3. The timeline needs 1 + 1 + 2 = 4 frames and does not grow.
    assert_eq!(o.repeat_frames(&cycle("L0", 0, 1, 1)).unwrap(), 1);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        render_layer(head(&o).document(), 0),
        "K=a.1.10;.;K=a.1.10;."
    );
    let after = wire(&o);
    assert_eq!(
        frames_and_values(&after["propertyHolders"][0]["properties"][0]["track"]["keys"]),
        vec![(0, 10), (1, 20), (2, 10), (3, 20)]
    );
    assert_eq!(
        after["propertyHolders"][1], before_wire["propertyHolders"][1],
        "L1 is outside the bounds"
    );
    assert_eq!(after["totalFrames"], json!(4));
    let after_document = head(&o).document().clone();
    let undone = o.undo(o.content_revision()).unwrap();
    assert_eq!(
        bytes(&o),
        before,
        "one Undo restores the complete bytes, tracks included"
    );
    o.redo(undone).unwrap();
    assert_eq!(
        *head(&o).document(),
        after_document,
        "Redo reproduces the cycle"
    );
    assert_eq!(
        decode_storage_project(&bytes(&o)).unwrap(),
        after_document,
        "save and reopen"
    );
}

#[test]
fn a_track_collision_refuses_the_whole_cycle_cells_included() {
    // key 0 would be cloned onto frame 2, where L0 already has a key
    let colliding = json!([holder(
        "L0",
        json!([{"frame":0,"v":[1]},{"frame":2,"v":[2]}])
    )]);
    let mut o = owner(document_with(Some(colliding)));
    let before = (o.content_revision(), o.history_depths(), bytes(&o));
    assert_eq!(
        o.repeat_frames(&cycle("L0", 0, 1, 1)).err(),
        Some(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        (o.content_revision(), o.history_depths(), bytes(&o)),
        before,
        "the drawing half did not publish either"
    );
    assert_eq!(
        render_layer(head(&o).document(), 0),
        "K=a.1.10;.;K=b.2.20;."
    );
}

#[test]
fn tracks_alone_make_a_cycle_that_rewrites_no_cell_a_change() {
    // a hold over a hold inside the timeline: the drawing half writes nothing...
    let mut still = owner(sparse_with(None));
    let revision = still.content_revision();
    assert_eq!(
        still.repeat_frames(&cycle("L0", 1, 1, 1)).unwrap(),
        revision
    );
    assert_eq!(still.history_depths(), (0, 0), "nothing to publish");
    // ...but a key at frame 1 is cloned onto frame 2, so the same cycle now publishes one entry
    let keyed = json!([holder("L0", json!([{"frame":1,"v":[7]}]))]);
    let mut o = owner(sparse_with(Some(keyed)));
    let before = bytes(&o);
    assert_eq!(o.repeat_frames(&cycle("L0", 1, 1, 1)).unwrap(), 1);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        frames_and_values(&wire(&o)["propertyHolders"][0]["properties"][0]["track"]["keys"]),
        vec![(1, 7), (2, 7)]
    );
    assert_eq!(render_layer(head(&o).document(), 0), "K=a.1.10;.;.;.");
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), before);
}

#[test]
fn the_rounds_are_the_clamp_of_the_drawing_half() {
    assert_eq!(
        clamp_times(i64::MAX),
        50,
        "the kernel's MAX_ROUNDS is the same 50"
    );
    // one key at frame 0, one-frame span: a raw 0 runs once, a raw 77 runs 50 times and grows the timeline to 51 frames
    let one = json!([holder("L0", json!([{"frame":0,"v":[3]}]))]);
    let mut o = owner(document_with(Some(one.clone())));
    o.repeat_frames(&cycle("L0", 0, 0, 0)).unwrap();
    assert_eq!(
        frames_and_values(&wire(&o)["propertyHolders"][0]["properties"][0]["track"]["keys"]),
        vec![(0, 3), (1, 3)]
    );
    let mut o = owner(document_with(Some(one)));
    o.repeat_frames(&cycle("L0", 0, 0, 77)).unwrap();
    let after = wire(&o);
    let keys = frames_and_values(&after["propertyHolders"][0]["properties"][0]["track"]["keys"]);
    assert_eq!(keys.len(), 51);
    assert_eq!((keys[1], keys[50]), ((1, 3), (50, 3)));
    assert_eq!(after["totalFrames"], json!(51), "max(4, 0 + 1 + 1 * 50)");
}

#[test]
fn the_dispatched_cycle_carries_the_tracks_and_replay_never_executes_twice() {
    let mut o = owner(document_with(Some(holders())));
    let request = json!({"apiVersion":2,"requestId":"cycle","instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":"command.document.object.frames.repeat","payload":repeat_payload(&cycle("L0", 0, 1, 1))});
    let dispatch = |o: &mut NativeObjectHistory| {
        let typed = serde_json::from_value(request.clone()).unwrap();
        serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
    };
    let first = dispatch(&mut o);
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(first["result"]["applied"], true);
    let state = (o.content_revision(), o.history_depths(), bytes(&o));
    assert_eq!(
        frames_and_values(&wire(&o)["propertyHolders"][0]["properties"][0]["track"]["keys"]),
        vec![(0, 10), (1, 20), (2, 10), (3, 20)]
    );
    assert_eq!(
        dispatch(&mut o),
        first,
        "the exact replay returns the receipt"
    );
    assert_eq!(
        (o.content_revision(), o.history_depths(), bytes(&o)),
        state,
        "and executes nothing"
    );
}
