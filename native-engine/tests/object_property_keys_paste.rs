//! #2524 / #2377: pasting copied Motion keys is one owner transaction with one exact history entry.
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
      {"target":target("A"),"properties":[
        {"property":"rotation","track":{"keys":[
          {"frame":10,"v":[5],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}],"hIn":[1,2],"hOut":[3,4],"lockTo":"out","color":"#ff0000"},
          {"frame":20,"v":[90],"hold":true}]}}]}
    ]})
}
fn from(value: Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "key-paste-owner",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    let doc = o.acquire_snapshot(o.content_revision()).unwrap();
    serde_json::from_slice(&encode_storage_project(doc.document()).unwrap()).unwrap()
}
fn keys(w: &Value, layer: &str, property: &str) -> Vec<Value> {
    w["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == layer)
        .and_then(|h| {
            h["properties"]
                .as_array()?
                .iter()
                .find(|r| r["property"] == property)
        })
        .map(|r| r["track"]["keys"].as_array().unwrap().clone())
        .unwrap_or_default()
}
fn paste_at(frame: i64, tracks: Value) -> Value {
    json!({"command":"object.property.keys.paste","frame":frame,"tracks":tracks})
}
fn apply(o: &mut NativeObjectHistory, p: Value) -> Result<u64, DispatchErrorCode> {
    let candidate = o
        .owner
        .prepare_property_keys_paste_json(p.to_string().as_bytes())?;
    o.commit_property_edit(candidate)
}
#[test]
fn paste_lands_at_the_playhead_offsets_in_one_history_entry_and_undo_is_exact() {
    let initial = fixture();
    let mut o = from(initial.clone());
    let tracks = json!([
      {"target":target("B"),"property":"position","keys":[
        {"dt":0,"v":[10,20],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]},
        {"dt":6,"v":[300,40],"hold":true}]},
      {"target":target("A"),"property":"rotation","keys":[{"dt":3,"v":[45]}]}
    ]);
    assert_eq!(apply(&mut o, paste_at(4, tracks)).unwrap(), 1);
    assert_eq!(o.history_depths(), (1, 0), "one entry for two layers");
    let w = wire(&o);
    let pos = keys(&w, "B", "position");
    assert_eq!(pos.len(), 2);
    assert_eq!(pos[0]["frame"], json!(4));
    assert_eq!(pos[0]["v"], json!([10, 20]));
    assert_eq!(pos[0]["hOut"], json!([0, 0]), "a new key gets zero handles");
    assert!(pos[0].get("hold").is_none());
    assert_eq!(pos[1]["frame"], json!(10));
    assert_eq!(pos[1]["hold"], json!(true));
    assert_eq!(
        pos[1]["curvePoints"].as_array().unwrap().len(),
        5,
        "a key without curve points gets the default curve"
    );
    let rot: Vec<_> = keys(&w, "A", "rotation")
        .iter()
        .map(|k| k["frame"].clone())
        .collect();
    assert_eq!(
        rot,
        vec![json!(7), json!(10), json!(20)],
        "the track stays sorted"
    );
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(wire(&o), initial, "undo restores the exact document");
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o), w, "redo restores the exact edit");
}
#[test]
fn a_key_already_on_the_landing_frame_keeps_its_handles_lock_and_tint() {
    let mut o = from(fixture());
    let tracks = json!([{"target":target("A"),"property":"rotation","keys":[
        {"dt":0,"v":[77],"hold":true,"curvePoints":[{"x":0,"y":0},{"x":0.5,"y":0.9},{"x":1,"y":1}]},
        {"dt":10,"v":[12]}]}]);
    assert_eq!(apply(&mut o, paste_at(10, tracks)).unwrap(), 1);
    let rot = keys(&wire(&o), "A", "rotation");
    assert_eq!(rot.len(), 2, "overwritten, not duplicated");
    assert_eq!(rot[0]["v"], json!([77]));
    assert_eq!(rot[0]["hold"], json!(true));
    assert_eq!(rot[0]["curvePoints"].as_array().unwrap().len(), 3);
    assert_eq!(rot[0]["hIn"], json!([1, 2]));
    assert_eq!(rot[0]["hOut"], json!([3, 4]));
    assert_eq!(rot[0]["lockTo"], json!("out"));
    assert_eq!(rot[0]["color"], json!("#ff0000"));
    assert_eq!(rot[1]["v"], json!([12]));
    assert!(
        rot[1].get("hold").is_none() || rot[1]["hold"] == json!(false),
        "the pasted key is not a hold"
    );
}
#[test]
fn keys_outside_the_composition_are_skipped_and_an_all_outside_paste_adds_nothing() {
    let mut o = from(fixture());
    let tracks = json!([{"target":target("B"),"property":"rotation","keys":[
        {"dt":0,"v":[1]},{"dt":30,"v":[2]},{"dt":50,"v":[3]}]}]);
    assert_eq!(apply(&mut o, paste_at(20, tracks)).unwrap(), 1);
    let rot: Vec<_> = keys(&wire(&o), "B", "rotation")
        .iter()
        .map(|k| k["frame"].clone())
        .collect();
    assert_eq!(rot, vec![json!(20)], "only the key inside 0..39 landed");
    let again = json!([{"target":target("B"),"property":"scale","keys":[{"dt":0,"v":[1,1]}]}]);
    let before = o.content_revision();
    assert_eq!(
        apply(&mut o, paste_at(40, again)).unwrap(),
        before,
        "every key outside: no revision"
    );
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn pasting_the_same_packet_twice_adds_no_second_history_entry() {
    let mut o = from(fixture());
    let tracks = json!([{"target":target("B"),"property":"rotation","keys":[{"dt":0,"v":[1]},{"dt":4,"v":[2]}]}]);
    assert_eq!(apply(&mut o, paste_at(8, tracks.clone())).unwrap(), 1);
    assert_eq!(
        apply(&mut o, paste_at(8, tracks)).unwrap(),
        1,
        "identical: no new revision"
    );
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn a_refused_paste_publishes_nothing() {
    let mut o = from(fixture());
    let good = json!({"target":target("B"),"property":"rotation","keys":[{"dt":0,"v":[1]}]});
    let cases: Vec<(Value, DispatchErrorCode)> = vec![
        (
            paste_at(
                4,
                json!([good.clone(), {"target":target("ghost"),"property":"rotation","keys":[{"dt":0,"v":[1]}]}]),
            ),
            DispatchErrorCode::NotFound,
        ),
        (
            paste_at(
                4,
                json!([good.clone(), {"target":target("B"),"property":"rotation","keys":[{"dt":0,"v":[1,2]}]}]),
            ),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            paste_at(
                4,
                json!([good.clone(), {"target":target("B"),"property":"nonsense","keys":[{"dt":0,"v":[1]}]}]),
            ),
            DispatchErrorCode::Unavailable,
        ),
        (
            paste_at(
                4,
                json!([good.clone(), {"target":target("B"),"property":"order","keys":[{"dt":0,"v":[1]}]}]),
            ),
            DispatchErrorCode::Unavailable,
        ),
        (
            paste_at(-1, json!([good.clone()])),
            DispatchErrorCode::InvalidRequest,
        ),
        (paste_at(4, json!([])), DispatchErrorCode::InvalidRequest),
        (
            paste_at(
                4,
                json!([{"target":target("B"),"property":"rotation","keys":[]}]),
            ),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.property.keys.paste","frame":4,"tracks":[good.clone()],"extra":1}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            json!({"command":"object.property.preset.apply","frame":4,"tracks":[good]}),
            DispatchErrorCode::InvalidRequest,
        ),
    ];
    let start = wire(&o);
    for (payload, expected) in cases {
        assert_eq!(
            apply(&mut o, payload.clone()).unwrap_err(),
            expected,
            "{payload}"
        );
        assert_eq!(o.history_depths(), (0, 0));
        assert_eq!(wire(&o), start, "nothing partially written");
    }
}
