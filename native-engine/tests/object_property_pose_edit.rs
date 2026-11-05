//! #2520/#2525: `keepPose` (the anchor edit whose Position follows in the same revision) and `setKeyHandle` (one spatial handle of an
//! existing key), through the real owner, history and save/undo. Literal oracles for the baseline anchor-drag compensation.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new(
        "pose-edit",
        decode_storage_project(
            json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":20,"layers":[{"layerUid":"L"}],"objects":[]})
                .to_string()
                .as_bytes(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&encode_storage_project(owner.acquire_snapshot(owner.content_revision()).unwrap().document()).unwrap()).unwrap()
}
fn payload(property: &str, action: Value) -> Value {
    json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L"},"property":property,"action":action})
}
fn apply(owner: &mut NativeObjectHistory, payload: Value) -> Result<u64, DispatchErrorCode> {
    let candidate = owner.prepare_property_edit_json(payload.to_string().as_bytes())?;
    owner.commit_property_edit(candidate)
}
fn set_value(frame: f64, value: f64) -> Value {
    json!({"kind":"setValue","frame":frame,"values":[value]})
}

fn property<'a>(wire: &'a Value, name: &str) -> &'a Value {
    wire["propertyHolders"][0]["properties"]
        .as_array()
        .and_then(|all| all.iter().find(|r| r["property"] == name))
        .unwrap_or(&Value::Null)
}
fn number_pair(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

// #2520/#2525: the baseline anchor drag with Rotation or Scale set moves Position in the same revision, so the artwork stays.
#[cfg(feature = "evaluation")]
#[test]
fn keep_pose_anchor_edit_moves_position_in_the_same_revision_and_one_history_entry() {
    let mut o = owner();
    apply(&mut o, payload("rotation", set_value(0., 90.))).unwrap();
    apply(&mut o, payload("scale", json!({"kind":"setValue","frame":0,"values":[200,100]}))).unwrap();
    let (revision, depths) = (o.content_revision(), o.history_depths());
    let anchor = json!({"kind":"setValue","frame":0,"values":[10,0],"keepPose":true});
    assert_eq!(apply(&mut o, payload("anchor", anchor)).unwrap(), revision + 1);
    assert_eq!(o.history_depths(), (depths.0 + 1, 0), "one history entry for the anchor and the Position that follows it");
    let after = wire(&o);
    assert_eq!(property(&after, "anchor")["static"], json!([10, 0]));
    // d = old - new = (-10, 0); R(90) S(2, 1) d = (0, -20); position = d - M d = (-10, 20)
    let position = number_pair(&property(&after, "position")["static"]);
    assert!((position[0] + 10.0).abs() < 1e-9 && (position[1] - 20.0).abs() < 1e-9, "{position:?}");
    assert_eq!(o.undo(revision + 1).unwrap(), revision + 2);
    let undone = wire(&o);
    assert!(property(&undone, "anchor").is_null() && property(&undone, "position").is_null(), "one undo restores both");
    assert_eq!(property(&undone, "rotation")["static"][0].as_f64(), Some(90.0));
}
#[cfg(feature = "evaluation")]
#[test]
fn keep_pose_is_anchor_only_and_a_zero_move_changes_nothing() {
    let mut o = owner();
    let keep = |v: Value| json!({"kind":"setValue","frame":0,"values":v,"keepPose":true});
    let before = o.history_depths();
    assert_eq!(apply(&mut o, payload("position", keep(json!([1, 2])))).unwrap_err(), DispatchErrorCode::InvalidRequest);
    assert_eq!(apply(&mut o, payload("opacity", keep(json!([50])))).unwrap_err(), DispatchErrorCode::InvalidRequest);
    apply(&mut o, payload("anchor", json!({"kind":"setValue","frame":0,"values":[0,0]}))).unwrap();
    let (revision, depths) = (o.content_revision(), o.history_depths());
    assert_ne!(depths, before);
    apply(&mut o, payload("anchor", keep(json!([0, 0])))).unwrap();
    assert_eq!((o.content_revision(), o.history_depths()), (revision, depths), "an unchanged anchor adds no revision");
    assert!(property(&wire(&o), "position").is_null(), "and writes no Position");
}
#[cfg(feature = "evaluation")]
#[test]
fn keep_pose_writes_a_key_on_a_keyed_position_at_the_edit_frame() {
    let mut o = owner();
    apply(&mut o, payload("position", json!({"kind":"setKey","frame":0,"values":[5,5]}))).unwrap();
    apply(&mut o, payload("position", json!({"kind":"setKey","frame":10,"values":[15,5]}))).unwrap();
    apply(&mut o, payload("rotation", set_value(0., 180.))).unwrap();
    apply(&mut o, payload("anchor", json!({"kind":"setValue","frame":5,"values":[4,0],"keepPose":true}))).unwrap();
    let after = wire(&o);
    let keys = property(&after, "position")["track"]["keys"].as_array().unwrap().clone();
    assert_eq!(keys.len(), 3, "the follow-up Position is keyed at frame 5 because Position is animated");
    let k = keys.iter().find(|k| k["frame"] == json!(5)).unwrap();
    // Position at the middle frame is 10 (symmetric ease); d = (-4, 0), R(180) d = (4, 0), d - M d = (-8, 0)
    let v = number_pair(&k["v"]);
    assert!((v[0] - 2.0).abs() < 1e-9 && (v[1] - 5.0).abs() < 1e-9, "{v:?}");
}

#[test]
fn key_handle_edit_sets_one_spatial_handle_of_an_existing_key() {
    let mut o = owner();
    for (frame, v) in [(0, [0, 0]), (10, [100, 0])] {
        apply(&mut o, payload("position", json!({"kind":"setKey","frame":frame,"values":v}))).unwrap();
    }
    let handle = |frame: i64, which: &str, v: Value| json!({"kind":"setKeyHandle","frame":frame,"handle":which,"values":v});
    let revision = o.content_revision();
    assert_eq!(apply(&mut o, payload("position", handle(0, "hOut", json!([30, -20])))).unwrap(), revision + 1);
    assert_eq!(apply(&mut o, payload("position", handle(10, "hIn", json!([-30, 5])))).unwrap(), revision + 2);
    let wired = wire(&o);
    let keys = &property(&wired, "position")["track"]["keys"];
    assert_eq!((keys[0]["hOut"].clone(), keys[0]["hIn"].clone()), (json!([30, -20]), json!([0, 0])));
    assert_eq!((keys[1]["hIn"].clone(), keys[1]["hOut"].clone()), (json!([-30, 5]), json!([0, 0])));
    assert_eq!(keys[0]["v"], json!([0, 0]), "a handle edit never moves the key");
    // the same values again are a no-op; a missing key, a bad handle name and a bad size are typed refusals
    let depths = o.history_depths();
    assert_eq!(apply(&mut o, payload("position", handle(0, "hOut", json!([30.0, -20.0])))).unwrap(), revision + 2);
    assert_eq!(o.history_depths(), depths);
    for (action, want) in [
        (handle(5, "hOut", json!([1, 1])), DispatchErrorCode::NotFound),
        (handle(0, "tangent", json!([1, 1])), DispatchErrorCode::InvalidRequest),
        (handle(0, "hOut", json!([1])), DispatchErrorCode::InvalidRequest),
    ] {
        assert_eq!(apply(&mut o, payload("position", action.clone())).unwrap_err(), want, "{action}");
    }
    assert_eq!(apply(&mut o, payload("opacity", handle(0, "hOut", json!([1, 1])))).unwrap_err(), DispatchErrorCode::Unavailable);
    assert_eq!(o.undo(revision + 2).unwrap(), revision + 3);
    let undone = wire(&o);
    assert_eq!(property(&undone, "position")["track"]["keys"][1]["hIn"], json!([0, 0]), "undo restores the handle");
}
