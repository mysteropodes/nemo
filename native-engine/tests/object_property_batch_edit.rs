//! #2520 / #2525: a batch of Motion value edits is one owner transaction with one exact history entry.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};
fn target(layer: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer})
}
fn fixture() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
    "layers":[{"layerUid":"A","name":"first"},{"layerUid":"B","name":"second"}],"objects":[],
    "propertyHolders":[
      {"target":target("A"),"properties":[{"property":"position","static":[10,20]}]},
      {"target":target("B"),"properties":[{"property":"rotation","track":{"keys":[
          {"frame":10,"v":[5],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]},{"frame":20,"v":[90]}]}}]}
    ]})
}
fn from(value: Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "property-batch-owner",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    let doc = o.acquire_snapshot(o.content_revision()).unwrap();
    serde_json::from_slice(&encode_storage_project(doc.document()).unwrap()).unwrap()
}
fn record(w: &Value, layer: &str, property: &str) -> Value {
    w["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == layer)
        .and_then(|h| h["properties"].as_array()?.iter().find(|r| r["property"] == property))
        .cloned()
        .unwrap_or(Value::Null)
}
fn edit(layer: &str, property: &str, kind: &str, frame: i64, values: Value) -> Value {
    json!({"target":target(layer),"property":property,"action":{"kind":kind,"frame":frame,"values":values}})
}
fn apply(o: &mut NativeObjectHistory, edits: Value) -> Result<u64, DispatchErrorCode> {
    let payload = json!({"command":"object.property.batch","edits":edits});
    let candidate = o
        .owner
        .prepare_property_batch_edit_json(payload.to_string().as_bytes())?;
    o.commit_property_edit(candidate)
}
#[test]
fn two_layers_move_as_one_revision_and_one_history_entry_and_undo_is_exact() {
    let initial = fixture();
    let mut o = from(initial.clone());
    let before = wire(&o);
    let edits = json!([
        edit("A", "position", "setValue", 0, json!([110, 70])),
        edit("B", "position", "setValue", 0, json!([-5, 8.5]))
    ]);
    assert_eq!(apply(&mut o, edits).unwrap(), 1);
    assert_eq!(o.history_depths(), (1, 0), "one entry for two layers");
    let after = wire(&o);
    assert_eq!(record(&after, "A", "position")["static"], json!([110, 70]));
    assert_eq!(record(&after, "B", "position")["static"], json!([-5, 8.5]));
    assert_eq!(record(&after, "B", "rotation"), record(&before, "B", "rotation"), "other properties are untouched");
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(wire(&o), before, "undo restores the exact document");
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o), after, "redo restores the exact edit");
}
#[test]
fn a_key_edit_and_a_value_edit_mix_in_one_candidate() {
    let mut o = from(fixture());
    let edits = json!([
        edit("B", "rotation", "setKey", 30, json!([180])),
        edit("A", "scale", "setValue", 0, json!([150, 150]))
    ]);
    assert_eq!(apply(&mut o, edits).unwrap(), 1);
    let w = wire(&o);
    let frames: Vec<Value> = record(&w, "B", "rotation")["track"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].clone())
        .collect();
    assert_eq!(frames, vec![json!(10), json!(20), json!(30)]);
    assert_eq!(record(&w, "A", "scale")["static"], json!([150, 150]));
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn a_refused_entry_publishes_nothing() {
    let mut o = from(fixture());
    let before = wire(&o);
    let revision = o.content_revision();
    let unowned = json!([
        edit("A", "position", "setValue", 0, json!([1, 2])),
        edit("B", "nosuchproperty", "setValue", 0, json!([3, 4]))
    ]);
    assert_eq!(apply(&mut o, unowned), Err(DispatchErrorCode::Unavailable), "an unowned property refuses the whole batch");
    let wrong_size = json!([
        edit("A", "position", "setValue", 0, json!([1, 2])),
        edit("B", "position", "setValue", 0, json!([3, 4, 5]))
    ]);
    assert!(apply(&mut o, wrong_size).is_err(), "a value of the wrong dimension refuses the whole batch");
    let unknown = json!([
        edit("A", "position", "setValue", 0, json!([1, 2])),
        edit("ZZ", "position", "setValue", 0, json!([3, 4]))
    ]);
    assert_eq!(apply(&mut o, unknown), Err(DispatchErrorCode::NotFound));
    assert_eq!(o.content_revision(), revision);
    assert_eq!(wire(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
}
#[test]
fn the_shape_of_a_batch_is_closed() {
    let mut o = from(fixture());
    let one = || edit("A", "position", "setValue", 0, json!([1, 2]));
    for bad in [
        json!([]),
        json!([one(), one()]),
        json!([{"target":target("A"),"property":"position","action":{"kind":"removeKey","frame":0}}]),
        json!([{"target":target("A"),"property":"anchor","action":{"kind":"setValue","frame":0,"values":[1,2],"keepPose":true}}]),
        json!([{"target":target("A"),"property":"position","action":{"kind":"setValue","frame":0,"values":[1,2]},"extra":1}]),
    ] {
        assert_eq!(apply(&mut o, bad.clone()), Err(DispatchErrorCode::InvalidRequest), "{bad}");
    }
    let too_many = Value::Array((0..65).map(|i| edit("A", &format!("p{i}"), "setValue", 0, json!([1]))).collect());
    assert_eq!(apply(&mut o, too_many), Err(DispatchErrorCode::InvalidRequest));
    let wrong = json!({"command":"object.property.edit","edits":[one()]});
    assert!(o.owner.prepare_property_batch_edit_json(wrong.to_string().as_bytes()).is_err());
    assert_eq!(o.history_depths(), (0, 0));
}
#[test]
fn a_batch_that_changes_nothing_adds_no_history_entry() {
    let mut o = from(fixture());
    let revision = o.content_revision();
    let same = json!([edit("A", "position", "setValue", 0, json!([10, 20]))]);
    assert_eq!(apply(&mut o, same).unwrap(), revision);
    assert_eq!(o.history_depths(), (0, 0));
}
