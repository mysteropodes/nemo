//! `object.vector.batch` (#2361, #2362): ordered creates, replaces and deletes on one layer as one revision and one undo
//! entry, through the single-command kernels; a refused step refuses the whole batch.
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn op_create(record: Value) -> Value {
    json!({"op":"create","record":record})
}
fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
fn ids_at(owner: &NativeObjectHistory, layer: &str, frame: u64) -> Vec<String> {
    objects(&state(owner))
        .iter()
        .filter(|r| r["target"]["layerUid"] == layer && r["target"]["frameScope"]["frame"].as_u64() == Some(frame))
        .map(|r| r["target"]["strokeId"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_cut_is_one_revision_and_one_undo_entry() {
    for version in [1, 2] {
        let mut owner = owner_of(&document(version, seeds(), None));
        let before = state(&owner);
        let response = send(&mut owner, "cut", batch(vec![
            json!({"op":"delete","stableTarget":target("L1", 0, "s-a")}),
            op_create(record_at("B", "L1", 0, "")),
            op_create(record_at("C", "L1", 0, "")),
        ]));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["contentRevision"], 1);
        assert_eq!(response["result"], json!({"applied":true,"historyEntriesAdded":1,"operation":"batch",
            "strokeId":"s-a","strokeIds":["s-a","native-stroke:1:0","native-stroke:1:1"]}));
        assert_eq!(ids_at(&owner, "L1", 0), ["native-stroke:1:0", "native-stroke:1:1"].map(String::from).to_vec());
        let after = state(&owner);
        assert_eq!(owner.history_depths(), (1, 0));
        owner.undo(1).unwrap();
        assert_eq!(state(&owner), before, "v{version}: one undo restores every step");
        owner.redo(2).unwrap();
        assert_eq!(state(&owner), after);
        assert_eq!(state(&owner_of(&after)), after, "the saved document reopens unchanged");
    }
}

#[test]
fn replaces_and_copy_keyed_creates_across_frames_are_one_entry() {
    let mut doc = document(2, vec![record_at("A", "L1", 0, "s-a"), record_at("C", "L1", 0, "s-c")], None);
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = owner_of(&doc);
    let mut moved = record_at("A", "L1", 0, "s-a");
    moved["geometry"]["segments"][0]["point"]["x"] = json!(-7);
    let response = send(&mut owner, "sculpt", batch(vec![json!({"op":"replace","record":moved})]));
    assert_eq!(response["result"]["operation"], "batch", "{response}");
    assert_eq!(objects(&state(&owner)).iter().find(|r| r["target"]["strokeId"] == "s-a").unwrap()["geometry"]["segments"][0]["point"]["x"], -7);
    let mut payload = batch(vec![op_create(record_at("B", "L1", 2, "")), op_create(record_at("B", "L1", 5, ""))]);
    payload["keyframe"] = json!("copy");
    let response = send(&mut owner, "multi", payload);
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(state(&owner)["layers"][0]["timing"]["keys"], json!([{"frame":0},{"frame":2},{"frame":5}]));
    for frame in [2, 5] {
        let cell = ids_at(&owner, "L1", frame);
        assert_eq!(cell.len(), 3, "frame {frame}: the two shown strokes copied, then the new one");
        assert_eq!(&cell[..2], &["s-a".to_string(), "s-c".to_string()]);
    }
    assert_eq!(owner.history_depths(), (2, 0));
}

#[test]
fn any_refused_step_refuses_the_whole_batch() {
    let many: Vec<Value> = (0..65).map(|_| op_create(record_at("B", "L1", 0, ""))).collect();
    for payload in [
        batch(vec![]),
        batch(many),
        batch(vec![op_create(record_at("B", "L1", 0, "")), op_create(record_at("C", "L2", 0, ""))]),
        batch(vec![json!({"op":"move","record":record_at("B", "L1", 0, "")})]),
        batch(vec![op_create(record_at("B", "L1", 0, "")), json!({"op":"delete","stableTarget":target("L1", 0, "missing")})]),
        batch(vec![json!({"op":"delete","stableTarget":target("L1", 0, "s-a"),"placement":"behind"})]),
        { let mut p = batch(vec![op_create(record_at("B", "L1", 0, ""))]); p["trimEnds"] = json!(4); p },
        { let mut p = batch(vec![op_create(record_at("B", "L1", 0, ""))]); p["placement"] = json!("behind"); p },
        { let mut p = create(record_at("B", "L1", 0, "")); p["ops"] = json!([]); p },
    ] {
        let mut owner = owner_of(&document(1, seeds(), None));
        let before = state(&owner);
        let response = send(&mut owner, "bad", payload.clone());
        assert_eq!(response["ok"], false, "{payload}");
        assert_eq!((state(&owner), owner.history_depths()), (before, (0, 0)));
    }
}
