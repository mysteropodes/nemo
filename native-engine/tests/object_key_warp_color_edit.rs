//! Key selection box warps (top/bottom skew, left/right space, liquify) and key tints through the actual owner, history and codec.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn target(layer: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer})
}
fn sel(layer: &str, property: &str, frame: i64) -> Value {
    json!({"target":target(layer),"property":property,"frame":frame})
}
fn edit(selections: Vec<Value>, action: Value) -> Value {
    json!({"command":"object.property.track.edit","selections":selections,"action":action})
}
fn warp(mode: &str, total: f64) -> Value {
    json!({"kind":"warp","mode":mode,"total":total})
}
fn fixture() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":60,
    "layers":[{"layerUid":"A"}],"objects":[],
    "propertyHolders":[{"target":target("A"),"properties":[
      {"property":"opacity","track":{"keys":[{"frame":0,"v":[0]},{"frame":10,"v":[50],"hold":true},{"frame":20,"v":[100]},{"frame":40,"v":[60]}]}},
      {"property":"rotation","track":{"keys":[{"frame":0,"v":[0]},{"frame":10,"v":[90]},{"frame":30,"v":[10]}]}},
      {"property":"scale","track":{"keys":[{"frame":0,"v":[1,1]},{"frame":20,"v":[2,2]}]}}]}]})
}
fn from(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "key-warp-owner",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(o)).unwrap()
}
fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), Vec<u8>) {
    (o.content_revision(), o.history_depths(), bytes(o))
}
fn keys(o: &NativeObjectHistory, record: usize) -> Value {
    wire(o)["propertyHolders"][0]["properties"][record]["track"]["keys"].clone()
}
fn frames(o: &NativeObjectHistory, record: usize) -> Vec<i64> {
    keys(o, record)
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_i64().unwrap())
        .collect()
}
fn run(o: &mut NativeObjectHistory, payload: &Value) -> Result<u64, DispatchErrorCode> {
    let candidate = o
        .owner
        .prepare_property_track_edit_json(payload.to_string().as_bytes())?;
    o.commit_property_edit(candidate)
}
fn refused(o: &mut NativeObjectHistory, payload: &Value, code: DispatchErrorCode) {
    let before = state(o);
    assert_eq!(run(o, payload), Err(code), "{payload}");
    assert_eq!(state(o), before, "a refusal publishes nothing: {payload}");
}

/// Three rows (opacity, rotation, scale), one selected key each at frames 10, 10 and 20.
fn three_rows() -> Vec<Value> {
    vec![
        sel("A", "opacity", 10),
        sel("A", "rotation", 10),
        sel("A", "scale", 20),
    ]
}

#[test]
fn skew_top_moves_the_first_row_fully_and_the_last_not_at_all() {
    let mut o = from(&fixture());
    let zero = bytes(&o);
    // n = 3 rows: factors (2/2, 1/2, 0/2) of a 6-frame drag are 6, 3, 0.
    assert_eq!(run(&mut o, &edit(three_rows(), warp("top", 6.0))), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(frames(&o, 0), vec![0, 16, 20, 40]);
    assert_eq!(frames(&o, 1), vec![0, 13, 30]);
    assert_eq!(frames(&o, 2), vec![0, 20]);
    assert_eq!(keys(&o, 0)[1], json!({"frame":16,"v":[50],"hold":true}), "the key keeps its own data");
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), zero);
    o.redo(o.content_revision()).unwrap();
    assert_eq!(frames(&o, 0), vec![0, 16, 20, 40]);
    assert_eq!(
        decode_storage_project(&bytes(&o)).unwrap(),
        *o.acquire_snapshot(o.content_revision()).unwrap().document()
    );
}

#[test]
fn skew_bottom_is_the_mirror_and_negative_drags_round_halves_up() {
    let mut o = from(&fixture());
    // factors (0, 1/2, 1): a -3 drag moves 0, round(-1.5) = -1, -3.
    assert_eq!(run(&mut o, &edit(three_rows(), warp("bottom", -3.0))), Ok(1));
    assert_eq!(frames(&o, 0), vec![0, 10, 20, 40]);
    assert_eq!(frames(&o, 1), vec![0, 9, 30]);
    assert_eq!(frames(&o, 2), vec![0, 17]);
}

#[test]
fn space_anchors_the_opposite_edge_of_the_selected_frames() {
    // Selected frames 10, 10 and 20: min 10, max 20, span 10.
    let mut o = from(&fixture());
    assert_eq!(run(&mut o, &edit(three_rows(), warp("right", 10.0))), Ok(1));
    assert_eq!(frames(&o, 0), vec![0, 10, 20, 40], "the key on the left edge stays");
    assert_eq!(frames(&o, 2), vec![0, 30], "the key on the right edge moves the whole drag");
    let mut o = from(&fixture());
    assert_eq!(run(&mut o, &edit(three_rows(), warp("left", -5.0))), Ok(1));
    assert_eq!(frames(&o, 0), vec![0, 5, 20, 40], "the left edge moves the whole drag");
    assert_eq!(frames(&o, 2), vec![0, 20], "the key on the right edge stays");
    // Every selected key on one frame: nothing to spread, nothing changes.
    let mut o = from(&fixture());
    let same = edit(vec![sel("A", "opacity", 10), sel("A", "rotation", 10)], warp("right", 7.0));
    assert_eq!(run(&mut o, &same), Ok(0), "no revision for a no-op");
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn liquify_falls_off_around_the_grab_frame() {
    // Selected frames 0 and 20 on one row: span 20, radius 7. Grabbed at 0: the key at 0 moves the whole drag, the key at 20
    // is (20/7)^2 = 8.2 e-folds away, which rounds to no move.
    let mut o = from(&fixture());
    let payload = edit(
        vec![sel("A", "opacity", 10), sel("A", "opacity", 20)],
        json!({"kind":"warp","mode":"liquify","total":4.0,"grab":10.0}),
    );
    // grab = 10 on keys at 10 and 20, span 10, radius 3.5: factors 1 and exp(-(10/3.5)^2) = 0.0003 -> 4 and 0.
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(frames(&o, 0), vec![0, 14, 20, 40]);
}

#[test]
fn warp_refusals_are_atomic() {
    let mut o = from(&fixture());
    // Out of range: the first row would land before frame 0 (-> 10-12) .
    refused(&mut o, &edit(three_rows(), warp("top", -12.0)), DispatchErrorCode::Unavailable);
    refused(&mut o, &edit(three_rows(), warp("top", 51.0)), DispatchErrorCode::Unavailable);
    // Onto a key that stays: 10 + 10 lands on the key at 20.
    refused(&mut o, &edit(vec![sel("A", "opacity", 10), sel("A", "rotation", 10)], warp("top", 10.0)), DispatchErrorCode::Unavailable);
    // Malformed gestures.
    refused(&mut o, &edit(three_rows(), json!({"kind":"warp","mode":"diagonal","total":1})), DispatchErrorCode::InvalidRequest);
    refused(&mut o, &edit(three_rows(), json!({"kind":"warp","mode":"liquify","total":1})), DispatchErrorCode::InvalidRequest);
    refused(&mut o, &edit(three_rows(), json!({"kind":"warp","mode":"top","total":1,"grab":3})), DispatchErrorCode::InvalidRequest);
    refused(&mut o, &edit(three_rows(), json!({"kind":"warp","mode":"top","total":2000000})), DispatchErrorCode::InvalidRequest);
    refused(&mut o, &edit(three_rows(), json!({"kind":"warp","mode":"top"})), DispatchErrorCode::InvalidRequest);
    // A stale or foreign selection.
    refused(&mut o, &edit(vec![sel("A", "opacity", 11)], warp("top", 1.0)), DispatchErrorCode::NotFound);
}

#[test]
fn warp_names_runs_of_keys_per_track_like_the_other_batches() {
    let mut o = from(&fixture());
    let ranges = json!([
        {"target":target("A"),"property":"opacity","ranges":[[10,20]]},
        {"target":target("A"),"property":"scale","ranges":[[0,20]]}
    ]);
    // Two rows: factors (1, 0) for "top": opacity keys 10 and 20 move 4, scale stays.
    let payload = json!({"command":"object.property.track.edit","selections":ranges,"action":warp("top", 4.0)});
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(frames(&o, 0), vec![0, 14, 24, 40]);
    assert_eq!(frames(&o, 2), vec![0, 20]);
}

#[test]
fn colour_tints_the_selected_keys_and_clears_them() {
    let mut o = from(&fixture());
    let zero = bytes(&o);
    let tint = |c: Value| edit(vec![sel("A", "opacity", 10), sel("A", "scale", 20)], json!({"kind":"color","color":c}));
    assert_eq!(run(&mut o, &tint(json!("#E8B64C"))), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(keys(&o, 0)[1], json!({"frame":10,"v":[50],"hold":true,"color":"#e8b64c"}));
    assert_eq!(keys(&o, 2)[1]["color"], json!("#e8b64c"));
    assert!(keys(&o, 0)[0].get("color").is_none(), "unselected keys are untouched");
    // The same tint again publishes nothing.
    assert_eq!(run(&mut o, &tint(json!("#e8b64c"))), Ok(1), "an identical write is not a new revision");
    assert_eq!(o.history_depths(), (1, 0));
    // The tint survives the codec round trip.
    let decoded = decode_storage_project(&bytes(&o)).unwrap();
    assert_eq!(decoded, *o.acquire_snapshot(o.content_revision()).unwrap().document());
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), zero);
    o.redo(o.content_revision()).unwrap();
    // Null removes the tint and the field with it.
    let revision = o.content_revision();
    assert_eq!(run(&mut o, &tint(Value::Null)), Ok(revision + 1));
    assert!(keys(&o, 0)[1].get("color").is_none());
    assert_eq!(keys(&o, 0)[1], fixture()["propertyHolders"][0]["properties"][0]["track"]["keys"][1]);
}

#[test]
fn colour_refusals_and_hostile_documents_are_rejected() {
    let mut o = from(&fixture());
    for bad in [json!("red"), json!("#12345"), json!("#1234567"), json!("e8b64c"), json!("#gggggg"), json!(5)] {
        refused(&mut o, &edit(vec![sel("A", "opacity", 10)], json!({"kind":"color","color":bad})), DispatchErrorCode::InvalidRequest);
    }
    refused(&mut o, &edit(vec![sel("A", "opacity", 10)], json!({"kind":"color"})), DispatchErrorCode::InvalidRequest);
    let mut hostile = fixture();
    hostile["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["color"] = json!("not-a-colour");
    assert!(decode_storage_project(hostile.to_string().as_bytes()).is_err());
    let mut ok = fixture();
    ok["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["color"] = json!("#4ea9ffcc");
    assert!(decode_storage_project(ok.to_string().as_bytes()).is_ok());
}

fn values(o: &NativeObjectHistory, record: usize) -> Vec<i64> {
    keys(o, record).as_array().unwrap().iter().map(|k| k["v"][0].as_i64().unwrap()).collect()
}
fn pair(property: &str, a: i64, b: i64) -> Vec<Value> {
    vec![sel("A", property, a), sel("A", property, b)]
}

#[test]
fn space_and_liquify_may_pile_selected_keys_onto_one_frame_keeping_the_key_the_baseline_evaluation_answers_with() {
    // Baseline `onDragMove` let dragged keys share a frame (only a key that stays blocks) and `rawValueAtFrame` answered with the FIRST key of a
    // pile at or before the first key of the track, and with the LAST key of a pile anywhere else. A track holds one key per frame: keep that key.
    // Interior pile: keys 10 (v 50, hold) and 20 (v 100) meet on frame 10; the later key answers.
    let mut o = from(&fixture());
    assert_eq!(run(&mut o, &edit(pair("opacity", 10, 20), warp("right", -10.0))), Ok(1));
    assert_eq!((frames(&o, 0), values(&o, 0)), (vec![0, 10, 40], vec![0, 100, 60]));
    assert_eq!(o.history_depths(), (1, 0), "one history entry for the whole gesture");
    // Pile on the first frame of the track: keys 0 (v 0) and 10 meet on frame 0; the first key answers.
    let mut o = from(&fixture());
    assert_eq!(run(&mut o, &edit(pair("opacity", 0, 10), warp("right", -10.0))), Ok(1));
    assert_eq!((frames(&o, 0), values(&o, 0)), (vec![0, 20, 40], vec![0, 100, 60]));
    // Pile on the last frame: keys 20 and 40 meet on frame 40; the later key answers.
    let mut o = from(&fixture());
    assert_eq!(run(&mut o, &edit(pair("opacity", 20, 40), warp("left", 20.0))), Ok(1));
    assert_eq!((frames(&o, 0), values(&o, 0)), (vec![0, 10, 40], vec![0, 50, 60]));
    // Liquify: the grabbed key at 10 moves the whole drag onto the key at 20 that barely moves.
    let mut o = from(&fixture());
    let liquify = edit(pair("opacity", 10, 20), json!({"kind":"warp","mode":"liquify","total":10.0,"grab":10.0}));
    assert_eq!(run(&mut o, &liquify), Ok(1));
    assert_eq!((frames(&o, 0), values(&o, 0)), (vec![0, 20, 40], vec![0, 100, 60]));
    // The collapsed track survives save/reload byte for byte, and undo/redo walk it exactly.
    let saved = bytes(&o);
    let reopened = from(&serde_json::from_slice::<Value>(&saved).unwrap());
    assert_eq!(bytes(&reopened), saved);
    let after = bytes(&o);
    assert_eq!(o.undo(1), Ok(2));
    assert_eq!(frames(&o, 0), vec![0, 10, 20, 40]);
    assert_eq!(o.redo(2), Ok(3));
    assert_eq!(bytes(&o), after);
}

#[test]
fn a_pile_is_per_row_and_a_key_that_stays_still_blocks_the_whole_gesture() {
    // Two rows pile independently: opacity keys 10 and 20 meet on 10, rotation keys 10 and 30 meet on 10.
    let mut o = from(&fixture());
    let rows = vec![sel("A", "opacity", 10), sel("A", "opacity", 20), sel("A", "rotation", 10), sel("A", "rotation", 30)];
    assert_eq!(run(&mut o, &edit(rows, warp("right", -20.0))), Ok(1));
    assert_eq!((frames(&o, 0), values(&o, 0)), (vec![0, 10, 40], vec![0, 100, 60]));
    assert_eq!((frames(&o, 1), values(&o, 1)), (vec![0, 10], vec![0, 10]));
    // Landing on a key that stays (frame 20, not selected) is still refused with nothing published.
    let mut o = from(&fixture());
    refused(&mut o, &edit(pair("opacity", 10, 40), warp("right", -20.0)), DispatchErrorCode::Unavailable);
}
