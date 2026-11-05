//! Key batch edits (shift, delete, flip, distribute) through the actual owner, history and codec.
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
fn batch(selections: Vec<Value>, action: Value) -> Value {
    json!({"command":"object.property.track.edit","selections":selections,"action":action})
}
fn shift(delta: i64) -> Value {
    json!({"kind":"shift","delta":delta})
}
fn fixture() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
    "layers":[{"layerUid":"A"},{"layerUid":"B"}],"objects":[],
    "propertyHolders":[
      {"target":target("A"),"properties":[
        {"property":"opacity","static":[17],"track":{"keys":[
          {"frame":0,"v":[0],"hold":true},{"frame":4,"v":[80],"curvePoints":[]},
          {"frame":12,"v":[10]},{"frame":24,"v":[240],"lockTo":"in"}]}},
        {"property":"rotation","track":{"keys":[{"frame":0,"v":[0]},{"frame":12,"v":[90]}]}}]},
      {"target":target("B"),"properties":[{"property":"opacity","track":{"keys":[{"frame":0,"v":[7]},{"frame":8,"v":[9]}]}}]}
    ]})
}
fn from(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "key-batch-owner",
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
fn keys(o: &NativeObjectHistory, holder: usize, record: usize) -> Value {
    wire(o)["propertyHolders"][holder]["properties"][record]["track"]["keys"].clone()
}
fn frames(o: &NativeObjectHistory, holder: usize, record: usize) -> Vec<i64> {
    keys(o, holder, record)
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

#[test]
fn shift_moves_selected_keys_in_one_history_entry_and_keeps_their_data() {
    let source = fixture();
    let mut o = from(&source);
    let zero = bytes(&o);
    let payload = batch(
        vec![sel("A", "opacity", 4), sel("A", "opacity", 12)],
        shift(3),
    );
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(frames(&o, 0, 0), vec![0, 7, 15, 24]);
    let moved = keys(&o, 0, 0);
    assert_eq!(moved[1], json!({"frame":7,"v":[80],"curvePoints":[]}));
    assert_eq!(moved[2], json!({"frame":15,"v":[10]}));
    assert_eq!(
        moved[0],
        keys(&from(&source), 0, 0)[0],
        "unselected keys keep their bytes"
    );
    assert_eq!(frames(&o, 0, 1), vec![0, 12], "other tracks are untouched");
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), zero);
    o.redo(o.content_revision()).unwrap();
    assert_eq!(frames(&o, 0, 0), vec![0, 7, 15, 24]);
    assert_eq!(
        decode_storage_project(&bytes(&o)).unwrap(),
        *o.acquire_snapshot(o.content_revision()).unwrap().document()
    );
}

#[test]
fn shift_negative_and_group_moves_across_two_tracks_are_one_entry() {
    let mut o = from(&fixture());
    let payload = batch(
        vec![
            sel("A", "rotation", 12),
            sel("A", "opacity", 12),
            sel("B", "opacity", 8),
        ],
        shift(-2),
    );
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(frames(&o, 0, 0), vec![0, 4, 10, 24]);
    assert_eq!(frames(&o, 0, 1), vec![0, 10]);
    assert_eq!(frames(&o, 1, 0), vec![0, 6]);
    // A group may slide over the frames it vacates: 0, 4, 12 each move four frames later.
    let mut o = from(&fixture());
    let payload = batch(
        vec![
            sel("A", "opacity", 0),
            sel("A", "opacity", 4),
            sel("A", "opacity", 12),
        ],
        shift(4),
    );
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(frames(&o, 0, 0), vec![4, 8, 16, 24]);
    assert_eq!(
        keys(&o, 0, 0)[0]["hold"],
        json!(true),
        "the key keeps its own hold flag"
    );
}

#[test]
fn shift_collisions_negative_time_and_overrun_are_atomic_refusals() {
    let mut o = from(&fixture());
    for (selections, delta) in [
        (vec![sel("A", "opacity", 4)], 8), // lands on the unselected key at 12
        (vec![sel("A", "opacity", 0)], -1), // negative time
        (vec![sel("A", "opacity", 24)], 16), // frame 40 is past the last frame 39
        (
            vec![sel("A", "opacity", 12), sel("A", "rotation", 12)],
            -12 - 1,
        ), // one member runs off
        (vec![sel("A", "rotation", 0), sel("B", "opacity", 0)], -1),
    ] {
        refused(
            &mut o,
            &batch(selections, shift(delta)),
            DispatchErrorCode::Unavailable,
        );
    }
    // The rotation key would go negative, so the opacity key's valid move must not publish either.
    refused(
        &mut o,
        &batch(
            vec![sel("A", "opacity", 24), sel("A", "rotation", 0)],
            shift(-1),
        ),
        DispatchErrorCode::Unavailable,
    );
    assert_eq!(
        run(&mut o, &batch(vec![sel("A", "opacity", 24)], shift(15))),
        Ok(1)
    );
    assert_eq!(frames(&o, 0, 0), vec![0, 4, 12, 39]);
}

#[test]
fn zero_distance_shift_and_single_flip_add_no_revision_or_history() {
    let mut o = from(&fixture());
    let before = state(&o);
    let one = vec![sel("A", "opacity", 4)];
    for payload in [
        batch(one.clone(), shift(0)),
        batch(one.clone(), json!({"kind":"flip"})),
        batch(one, json!({"kind":"distribute"})),
    ] {
        assert_eq!(run(&mut o, &payload), Ok(before.0), "{payload}");
        assert_eq!(state(&o), before, "{payload}");
    }
}

#[test]
fn delete_removes_selected_keys_and_drops_a_property_left_with_nothing() {
    let mut o = from(&fixture());
    let zero = bytes(&o);
    let payload = batch(
        vec![
            sel("A", "opacity", 4),
            sel("A", "opacity", 24),
            sel("A", "rotation", 0),
            sel("A", "rotation", 12),
        ],
        json!({"kind":"delete"}),
    );
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(frames(&o, 0, 0), vec![0, 12]);
    // Rotation lost every key and carried no static value or expression: the record is gone.
    let props = wire(&o)["propertyHolders"][0]["properties"].clone();
    assert_eq!(props.as_array().unwrap().len(), 1);
    assert_eq!(props[0]["property"], "opacity");
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), zero);
    o.redo(o.content_revision()).unwrap();
    // Opacity keeps its static value once its last key goes.
    let all = batch(
        [0, 12].iter().map(|f| sel("A", "opacity", *f)).collect(),
        json!({"kind":"delete"}),
    );
    let next = o.content_revision() + 1;
    assert_eq!(run(&mut o, &all), Ok(next));
    let record = wire(&o)["propertyHolders"][0]["properties"][0].clone();
    assert_eq!(record, json!({"property":"opacity","static":[17]}));
}

#[test]
fn flip_swaps_frame_slots_and_each_key_carries_its_own_value() {
    let mut o = from(&fixture());
    let payload = batch(
        vec![
            sel("A", "opacity", 0),
            sel("A", "opacity", 4),
            sel("A", "opacity", 24),
        ],
        json!({"kind":"flip"}),
    );
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        keys(&o, 0, 0),
        json!([
            {"frame":0,"v":[240],"lockTo":"in"},
            {"frame":4,"v":[80],"curvePoints":[]},
            {"frame":12,"v":[10]},
            {"frame":24,"v":[0],"hold":true}
        ])
    );
}

#[test]
fn distribute_spaces_selected_keys_evenly_and_refuses_collisions() {
    let mut o = from(&fixture());
    let every = |f: &[i64]| {
        f.iter()
            .map(|f| sel("A", "opacity", *f))
            .collect::<Vec<_>>()
    };
    // Selected 0, 4, 24 spread to 0, 12, 24 and would land on the unselected key at 12.
    refused(
        &mut o,
        &batch(every(&[0, 4, 24]), json!({"kind":"distribute"})),
        DispatchErrorCode::Unavailable,
    );
    assert_eq!(
        run(
            &mut o,
            &batch(every(&[0, 4, 12, 24]), json!({"kind":"distribute"}))
        ),
        Ok(1)
    );
    assert_eq!(frames(&o, 0, 0), vec![0, 8, 16, 24]);
    // 0, 1 and 4 spread over 0..4 with a step of 2: 0, 2, 4 (the 4 is the last key itself).
    let mut o = from(
        &json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
        "layers":[{"layerUid":"A"}],"objects":[],"propertyHolders":[{"target":target("A"),"properties":[
        {"property":"opacity","track":{"keys":[{"frame":0,"v":[0]},{"frame":1,"v":[1]},{"frame":4,"v":[4]},{"frame":7,"v":[7]}]}}]}]}),
    );
    assert_eq!(
        run(
            &mut o,
            &batch(every(&[0, 1, 4]), json!({"kind":"distribute"}))
        ),
        Ok(1)
    );
    assert_eq!(frames(&o, 0, 0), vec![0, 2, 4, 7]);
    // Halves round up like the baseline Math.round: 0, 1, 2 over 0..3 -> 0, 1.5 -> 2, 3.
    let mut o = from(
        &json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
        "layers":[{"layerUid":"A"}],"objects":[],"propertyHolders":[{"target":target("A"),"properties":[
        {"property":"opacity","track":{"keys":[{"frame":0,"v":[0]},{"frame":1,"v":[1]},{"frame":3,"v":[3]}]}}]}]}),
    );
    assert_eq!(
        run(
            &mut o,
            &batch(every(&[0, 1, 3]), json!({"kind":"distribute"}))
        ),
        Ok(1)
    );
    assert_eq!(frames(&o, 0, 0), vec![0, 2, 3]);
    // Keys already evenly spaced stay put: no revision, no history entry.
    let mut o = from(
        &json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
        "layers":[{"layerUid":"A"}],"objects":[],"propertyHolders":[{"target":target("A"),"properties":[
        {"property":"opacity","track":{"keys":[{"frame":0,"v":[0]},{"frame":1,"v":[1]},{"frame":2,"v":[2]}]}}]}]}),
    );
    assert_eq!(
        run(
            &mut o,
            &batch(every(&[0, 1, 2]), json!({"kind":"distribute"}))
        ),
        Ok(0)
    );
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn malformed_forged_and_unsupported_batches_are_typed_refusals() {
    let mut o = from(&fixture());
    let one = || vec![sel("A", "opacity", 4)];
    for (payload, code) in [
        (
            batch(one(), json!({"kind":"shift","delta":1.5})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            batch(one(), json!({"kind":"shift","delta":"2"})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            batch(one(), json!({"kind":"shift","delta":1000001})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            batch(one(), json!({"kind":"shift"})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            batch(one(), json!({"kind":"delete","keep":true})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            batch(vec![], json!({"kind":"delete"})),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            batch(
                vec![sel("A", "opacity", 4), sel("A", "opacity", 4)],
                json!({"kind":"delete"}),
            ),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            batch(vec![sel("A", "opacity", 5)], json!({"kind":"delete"})),
            DispatchErrorCode::NotFound,
        ),
        (
            batch(vec![sel("Z", "opacity", 4)], shift(1)),
            DispatchErrorCode::NotFound,
        ),
        (
            batch(vec![sel("A", "timeRemap", 4)], json!({"kind":"delete"})),
            DispatchErrorCode::Unavailable,
        ),
    ] {
        refused(&mut o, &payload, code);
    }
}
