//! D37 key-lock writes: layer keyLock through layer policy, per-key lockTo through track edit.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};
#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;
#[path = "object_layer_shift_build.rs"]
mod shift_fixture;

fn target(layer: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer})
}
fn sel(layer: &str, property: &str, frame: i64) -> Value {
    json!({"target":target(layer),"property":property,"frame":frame})
}
fn lock_to(selections: Vec<Value>, mode: Value) -> Value {
    json!({"command":"object.property.track.edit","selections":selections,
        "action":{"kind":"lockTo","mode":mode}})
}
fn fixture() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
    "layers":[{"layerUid":"A"},{"layerUid":"B","keyLock":null}],"objects":[],
    "propertyHolders":[
      {"target":target("A"),"properties":[
        {"property":"opacity","track":{"keys":[
          {"frame":0,"v":[0],"lockTo":"in"},{"frame":4,"v":[80]},
          {"frame":12,"v":[0],"lockTo":null},{"frame":24,"v":[240],"hold":true}]}},
        {"property":"rotation","track":{"keys":[{"frame":0,"v":[0]},{"frame":12,"v":[90],"lockTo":"out"}]}}]},
      {"target":target("B"),"properties":[{"property":"opacity","track":{"keys":[{"frame":0,"v":[7]}]}}]}
    ]})
}
fn from(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "d37-owner",
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
fn policy(layer: &str, value: Value) -> Vec<u8> {
    json!({"command":"object.layer.policy.edit","layerUid":layer,"flag":"keyLock","value":value})
        .to_string()
        .into_bytes()
}
fn set_layer(
    o: &mut NativeObjectHistory,
    layer: &str,
    value: Value,
) -> Result<u64, DispatchErrorCode> {
    let candidate = o.prepare_layer_policy_json(&policy(layer, value))?;
    o.commit_layer_policy(candidate)
}
fn set_keys(o: &mut NativeObjectHistory, payload: &Value) -> Result<u64, DispatchErrorCode> {
    let candidate = o
        .owner
        .prepare_property_track_edit_json(payload.to_string().as_bytes())?;
    o.commit_property_edit(candidate)
}

#[test]
fn layer_key_lock_sets_switches_and_clears_in_one_history_entry_each() {
    let source = fixture();
    let mut o = from(&source);
    let zero = bytes(&o);
    let candidate = o
        .prepare_layer_policy_json(&policy("A", json!("in")))
        .unwrap();
    assert_eq!(bytes(&o), zero, "preparation is detached");
    assert_eq!(o.commit_layer_policy(candidate), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    let mut expected = source.clone();
    expected["layers"][0]["keyLock"] = json!("in");
    assert_eq!(wire(&o), expected);
    for (mode, revision) in [("out", 2), ("layer", 3)] {
        assert_eq!(set_layer(&mut o, "A", json!(mode)), Ok(revision));
        assert_eq!(wire(&o)["layers"][0]["keyLock"], mode);
    }
    // Clearing authors the baseline explicit null, never a stale mode.
    assert_eq!(set_layer(&mut o, "A", Value::Null), Ok(4));
    assert_eq!(wire(&o)["layers"][0]["keyLock"], Value::Null);
    assert_eq!(o.history_depths(), (4, 0));
    for _ in 0..4 {
        o.undo(o.content_revision()).unwrap();
    }
    assert_eq!(bytes(&o), zero);
    for _ in 0..4 {
        o.redo(o.content_revision()).unwrap();
    }
    assert_eq!(wire(&o)["layers"][0]["keyLock"], Value::Null);
    assert_eq!(
        decode_storage_project(&bytes(&o)).unwrap(),
        *o.acquire_snapshot(o.content_revision()).unwrap().document()
    );
}

#[test]
fn layer_key_lock_noops_preserve_presence_revision_and_redo() {
    let mut o = from(&fixture());
    assert_eq!(set_layer(&mut o, "A", json!("in")), Ok(1));
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    // A (absent) and B (explicit null) both already read as no lock.
    for layer in ["A", "B"] {
        let before = state(&o);
        assert_eq!(set_layer(&mut o, layer, Value::Null), Ok(before.0));
        assert_eq!(state(&o), before);
    }
    assert_eq!(set_layer(&mut o, "A", json!("layer")), Ok(3));
    assert_eq!(o.history_depths(), (1, 0), "a change clears redo");
    let before = state(&o);
    assert_eq!(set_layer(&mut o, "A", json!("layer")), Ok(before.0));
    assert_eq!(state(&o), before);
}

#[test]
fn layer_key_lock_refusals_and_flag_value_mismatches_publish_nothing() {
    let mut o = from(&fixture());
    let before = state(&o);
    for value in [
        json!(true),
        json!(false),
        json!(1),
        json!("both"),
        json!(""),
        json!(["in"]),
        json!({"in":1}),
    ] {
        assert_eq!(
            set_layer(&mut o, "A", value),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
    for (flag, value) in [
        ("locked", json!("in")),
        ("locked", Value::Null),
        ("shy", json!("layer")),
    ] {
        let bytes =
            json!({"command":"object.layer.policy.edit","layerUid":"A","flag":flag,"value":value});
        assert_eq!(
            o.prepare_layer_policy_json(bytes.to_string().as_bytes())
                .err(),
            Some(DispatchErrorCode::InvalidRequest)
        );
    }
    assert_eq!(
        set_layer(&mut o, "missing", json!("in")),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(state(&o), before);
    // A locked flag edit still works and does not touch keyLock.
    let locked =
        json!({"command":"object.layer.policy.edit","layerUid":"A","flag":"locked","value":true});
    let c = o
        .prepare_layer_policy_json(locked.to_string().as_bytes())
        .unwrap();
    assert_eq!(o.commit_layer_policy(c), Ok(1));
    assert!(wire(&o)["layers"][0].get("keyLock").is_none());
}

#[test]
fn stale_layer_key_lock_candidate_cannot_commit() {
    let mut o = from(&fixture());
    let stale = o
        .prepare_layer_policy_json(&policy("A", json!("in")))
        .unwrap();
    assert_eq!(set_layer(&mut o, "A", json!("out")), Ok(1));
    let before = state(&o);
    assert_eq!(
        o.commit_layer_policy(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(state(&o), before);
}

#[test]
fn key_lock_to_sets_a_multi_track_selection_atomically_and_round_trips_history() {
    let source = fixture();
    let mut o = from(&source);
    let zero = bytes(&o);
    let p = lock_to(
        vec![
            sel("A", "rotation", 0),
            sel("A", "opacity", 4),
            sel("A", "opacity", 12),
        ],
        json!("layer"),
    );
    let candidate = o
        .owner
        .prepare_property_track_edit_json(p.to_string().as_bytes())
        .unwrap();
    assert_eq!(bytes(&o), zero);
    assert_eq!(o.commit_property_edit(candidate), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    let mut expected = source.clone();
    expected["propertyHolders"][0]["properties"][0]["track"]["keys"][1]["lockTo"] = json!("layer");
    expected["propertyHolders"][0]["properties"][0]["track"]["keys"][2]["lockTo"] = json!("layer");
    expected["propertyHolders"][0]["properties"][1]["track"]["keys"][0]["lockTo"] = json!("layer");
    assert_eq!(wire(&o), expected);
    o.undo(1).unwrap();
    assert_eq!(bytes(&o), zero);
    o.redo(2).unwrap();
    assert_eq!(wire(&o), expected);
}

#[test]
fn key_lock_to_clear_writes_null_only_where_a_lock_existed() {
    let mut o = from(&fixture());
    let p = lock_to(
        vec![
            sel("A", "opacity", 0),
            sel("A", "opacity", 4),
            sel("A", "opacity", 12),
            sel("A", "rotation", 12),
        ],
        Value::Null,
    );
    assert_eq!(set_keys(&mut o, &p), Ok(1));
    let opacity = keys(&o, 0, 0);
    assert_eq!(opacity[0]["lockTo"], Value::Null, "in -> explicit null");
    assert!(opacity[1].get("lockTo").is_none(), "absent stays absent");
    assert_eq!(opacity[2]["lockTo"], Value::Null, "explicit null untouched");
    assert_eq!(opacity[3]["hold"], true);
    assert_eq!(keys(&o, 0, 1)[1]["lockTo"], Value::Null);
}

#[test]
fn key_lock_to_noops_publish_nothing_and_keep_redo() {
    let mut o = from(&fixture());
    assert_eq!(
        set_keys(&mut o, &lock_to(vec![sel("A", "opacity", 4)], json!("out"))),
        Ok(1)
    );
    o.undo(1).unwrap();
    let before = state(&o);
    for p in [
        lock_to(vec![sel("A", "opacity", 0)], json!("in")),
        lock_to(
            vec![sel("A", "opacity", 4), sel("A", "opacity", 24)],
            Value::Null,
        ),
        lock_to(vec![sel("A", "opacity", 12)], Value::Null),
    ] {
        assert_eq!(set_keys(&mut o, &p), Ok(before.0));
        assert_eq!(state(&o), before);
    }
    assert_eq!(o.history_depths(), (0, 1));
}

#[test]
fn key_lock_to_failures_never_publish_a_partial_selection() {
    let mut o = from(&fixture());
    let before = state(&o);
    let cases = [
        (
            lock_to(
                vec![sel("A", "opacity", 4), sel("A", "opacity", 5)],
                json!("in"),
            ),
            DispatchErrorCode::NotFound,
        ),
        (
            lock_to(
                vec![sel("A", "opacity", 4), sel("Z", "opacity", 4)],
                json!("in"),
            ),
            DispatchErrorCode::NotFound,
        ),
        (
            lock_to(
                vec![sel("A", "opacity", 4), sel("A", "opacity", 4)],
                json!("in"),
            ),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            lock_to(
                vec![sel("A", "opacity", 4), sel("A", "timeRemap", 4)],
                json!("in"),
            ),
            DispatchErrorCode::Unavailable,
        ),
        (
            lock_to(vec![sel("A", "opacity", 4)], json!("both")),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            lock_to(vec![sel("A", "opacity", 4)], json!(true)),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            lock_to(vec![sel("A", "opacity", 4)], json!("")),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            lock_to(vec![], json!("in")),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.property.track.edit","selections":[sel("A","opacity",4)],
            "action":{"kind":"lockTo"}}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.property.track.edit","selections":[sel("A","opacity",4)],
            "action":{"kind":"lockTo","mode":"in","extra":1}}),
            DispatchErrorCode::InvalidRequest,
        ),
    ];
    for (payload, error) in cases {
        assert_eq!(set_keys(&mut o, &payload), Err(error), "{payload}");
        assert_eq!(state(&o), before);
    }
}

#[test]
fn stale_key_lock_to_candidate_cannot_commit() {
    let mut o = from(&fixture());
    let stale = o
        .owner
        .prepare_property_track_edit_json(
            lock_to(vec![sel("A", "opacity", 4)], json!("in"))
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    assert_eq!(
        set_keys(&mut o, &lock_to(vec![sel("A", "opacity", 4)], json!("out"))),
        Ok(1)
    );
    let before = state(&o);
    assert_eq!(
        o.commit_property_edit(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(state(&o), before);
    assert_eq!(keys(&o, 0, 0)[1]["lockTo"], "out");
}

fn shift_through_history(input: &Value, edit: impl FnOnce(&mut NativeObjectHistory)) -> Value {
    let mut h = NativeObjectHistory::new("d37-shift", shift_fixture::admit(input.clone())).unwrap();
    edit(&mut h);
    let p = h
        .prepare_layer_range_drag_json(shift_fixture::gesture("both", &[2]).to_string().as_bytes())
        .unwrap();
    h.commit_layer_range_drag(p).unwrap();
    wire(&h)
}

#[test]
fn written_layer_and_key_locks_drive_the_layer_shift_exactly_like_authored_ones() {
    let base = shift_fixture::value(&shift_fixture::document());
    let plain = shift_through_history(&base, |_| {});
    let mut authored = base.clone();
    authored["layers"][0]["keyLock"] = json!("in");
    authored["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["lockTo"] = json!("in");
    let locked = shift_through_history(&authored, |_| {});
    assert_ne!(locked, plain, "the fixture must exercise the locks");
    let written = shift_through_history(&base, |h| {
        assert_eq!(set_layer(h, "L0", json!("in")), Ok(1));
        let p = lock_to(vec![sel("L0", "rotation", 1)], json!("in"));
        assert_eq!(set_keys(h, &p), Ok(2));
    });
    assert_eq!(written, locked);
    let cleared = shift_through_history(&base, |h| {
        set_layer(h, "L0", json!("in")).unwrap();
        set_keys(h, &lock_to(vec![sel("L0", "rotation", 1)], json!("in"))).unwrap();
        set_layer(h, "L0", Value::Null).unwrap();
        set_keys(h, &lock_to(vec![sel("L0", "rotation", 1)], Value::Null)).unwrap();
    });
    assert_eq!(cleared["layers"][0]["keyLock"], Value::Null);
    let mut normalized = cleared.clone();
    normalized["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("keyLock");
    normalized["propertyHolders"][0]["properties"][0]["track"]["keys"][0]
        .as_object_mut()
        .unwrap()
        .remove("lockTo");
    assert_eq!(normalized, plain);
}
