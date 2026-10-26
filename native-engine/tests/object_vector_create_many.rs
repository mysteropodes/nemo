//! G23 (#2361): `object.vector.createMany` lands every symmetry copy in ONE revision and ONE undo entry,
//! with owner-allocated ids in the given order, and refuses as a whole.
use super::support::*;
use serde_json::{json, Value};

fn many(records: Vec<Value>) -> Value {
    json!({"command":"object.vector.createMany","records":records})
}

#[test]
fn several_creates_are_one_revision_and_one_undo_entry_with_ordered_ids() {
    let mut owner = owner_of(&document(1, seeds(), None));
    let before = state(&owner);
    let records = vec![record_at("A", "L1", 0, ""), record_at("B", "L1", 0, ""), record_at("C", "L1", 0, "")];
    let response = send(&mut owner, "m", many(records));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1,"operation":"createMany","strokeId":"native-stroke:1:0",
            "strokeIds":["native-stroke:1:0","native-stroke:1:1","native-stroke:1:2"]})
    );
    let after = state(&owner);
    assert_eq!(objects(&after).len(), 6);
    for (i, label) in ["A", "B", "C"].iter().enumerate() {
        assert_eq!(objects(&after)[3 + i], record_at(label, "L1", 0, &format!("native-stroke:1:{i}")));
    }
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before, "one undo removes every copy");
    owner.redo(2).unwrap();
    assert_eq!(state(&owner), after, "one redo restores every copy");
    assert_eq!(state(&owner_of(&after)), after, "the saved document reopens unchanged");
}

#[test]
fn a_bad_record_count_mixed_targets_or_a_preset_id_refuse_the_whole_request() {
    let base = document(1, seeds(), None);
    let mut mixed_frame = record_at("B", "L1", 0, "");
    mixed_frame["target"]["frameScope"]["frame"] = json!(5);
    let cases = vec![
        ("one", many(vec![record_at("A", "L1", 0, "")])),
        ("too-many", many((0..33).map(|_| record_at("A", "L1", 0, "")).collect())),
        ("mixed-layer", many(vec![record_at("A", "L1", 0, ""), record_at("B", "L2", 0, "")])),
        ("mixed-frame", many(vec![record_at("A", "L1", 0, ""), mixed_frame])),
        ("preset-id", many(vec![record_at("A", "L1", 0, ""), record_at("B", "L1", 0, "chosen")])),
        ("unknown-layer", many(vec![record_at("A", "L9", 0, ""), record_at("B", "L9", 0, "")])),
        ("both-members", json!({"command":"object.vector.createMany","records":[record_at("A","L1",0,""),record_at("B","L1",0,"")],"record":record_at("A","L1",0,"")})),
        ("records-on-create", json!({"command":"object.vector.create","records":[record_at("A","L1",0,""),record_at("B","L1",0,"")]})),
        ("records-on-replace", json!({"command":"object.vector.replace","record":record_at("A","L1",0,"s-a"),"records":[record_at("A","L1",0,""),record_at("B","L1",0,"")]})),
        ("records-on-delete", json!({"command":"object.vector.delete","stableTarget":target("L1",0,"s-a"),"records":[record_at("A","L1",0,""),record_at("B","L1",0,"")]})),
    ];
    for (name, payload) in cases {
        let mut owner = owner_of(&base);
        let refused = send(&mut owner, name, payload);
        assert_eq!(refused["ok"], false, "{name}: {refused}");
        assert_eq!(owner.content_revision(), 0, "{name}");
        assert_eq!(owner.history_depths(), (0, 0), "{name}");
        assert_eq!(state(&owner), state(&owner_of(&base)), "{name}");
    }
}

fn behind(command: &str, body: Value) -> Value {
    let mut payload = body;
    payload["command"] = json!(command);
    payload["placement"] = json!("behind");
    payload
}
fn scene_elements(state: &Value, frame: u64) -> Vec<(String, u64)> {
    state["compositionOrder"]["contexts"][0]["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|scope| scope["frameScope"]["frame"] == json!(frame))
        .map(|scope| {
            scope["elements"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["layerUid"] == "L1")
                .map(|e| (e["strokeId"].as_str().unwrap().to_string(), e["naturalPosition"].as_u64().unwrap()))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn behind_places_new_strokes_under_the_layer_artwork_of_that_frame() {
    // v2 with an order plan: the new element takes position 0 and the layer's others move up.
    let mut owner = owner_of(&document(2, seeds(), None));
    let response = send(&mut owner, "b", behind("object.vector.create", json!({"record":record_at("B", "L1", 0, "")})));
    assert_eq!(response["ok"], true, "{response}");
    let mut stack = scene_elements(&state(&owner), 0);
    stack.sort_by_key(|(_, p)| *p);
    assert_eq!(stack, vec![("native-stroke:1:0".into(), 0), ("s-a".into(), 1)]);
    // Symmetry behind: the original at 0 and its copies above it, all under the existing artwork.
    let records = vec![record_at("A", "L1", 0, ""), record_at("B", "L1", 0, "")];
    assert_eq!(send(&mut owner, "bm", behind("object.vector.createMany", json!({"records":records})))["ok"], true);
    let mut stack = scene_elements(&state(&owner), 0);
    stack.sort_by_key(|(_, p)| *p);
    let ids: Vec<&str> = stack.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["native-stroke:2:0", "native-stroke:2:1", "native-stroke:1:0", "s-a"]);
    owner.undo(2).unwrap();
    assert_eq!(scene_elements(&state(&owner), 0).len(), 2, "one undo removes the behind batch");
    // v1 without a plan: the object array is the stack, so the record goes before the layer's first record there.
    let mut plain = owner_of(&document(1, seeds(), None));
    assert_eq!(send(&mut plain, "b1", behind("object.vector.create", json!({"record":record_at("B", "L1", 0, "")})))["ok"], true);
    let order: Vec<String> = objects(&state(&plain)).iter().map(|o| o["target"]["strokeId"].as_str().unwrap().to_string()).collect();
    assert_eq!(order, ["native-stroke:1:0", "s-a", "s-b", "s-c"]);
}

#[test]
fn placement_is_an_optional_create_member_only() {
    let base = document(2, seeds(), None);
    for (name, payload) in [
        ("unknown", json!({"command":"object.vector.create","record":record_at("A","L1",0,""),"placement":"inside"})),
        ("on-delete", json!({"command":"object.vector.delete","stableTarget":target("L1",0,"s-a"),"placement":"behind"})),
        ("on-replace", json!({"command":"object.vector.replace","record":record_at("A","L1",0,"s-a"),"placement":"front"})),
        ("null", json!({"command":"object.vector.create","record":record_at("A","L1",0,""),"placement":null})),
    ] {
        let mut owner = owner_of(&base);
        assert_eq!(send(&mut owner, name, payload)["ok"], false, "{name}");
        assert_eq!(owner.content_revision(), 0, "{name}");
    }
    let mut owner = owner_of(&base);
    let front = json!({"command":"object.vector.create","record":record_at("A","L1",0,""),"placement":"front"});
    assert_eq!(send(&mut owner, "f", front)["ok"], true, "explicit front is the default placement");
}

#[test]
fn behind_moves_only_the_target_layer_in_the_target_frame() {
    let mut seeded = seeds();
    seeded.push(record_at("B", "L1", 0, "s-d"));
    seeded.push(record_at("C", "L2", 0, "s-e"));
    let mut owner = owner_of(&document(2, seeded, None));
    let before = state(&owner);
    let keep = |state: &Value| -> Vec<Value> {
        state["compositionOrder"]["contexts"][0]["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|scope| scope["elements"].as_array().unwrap().iter().map(move |e| json!([scope["frameScope"]["frame"], e])))
            .filter(|pair| !(pair[0] == json!(0) && pair[1]["layerUid"] == "L1"))
            .collect()
    };
    assert_eq!(send(&mut owner, "b", behind("object.vector.create", json!({"record":record_at("A", "L1", 0, "")})))["ok"], true);
    assert_eq!(keep(&state(&owner)), keep(&before), "other layers and frames keep their positions");
    let mut stack = scene_elements(&state(&owner), 0);
    stack.sort_by_key(|(_, p)| *p);
    assert_eq!(stack, vec![("native-stroke:1:0".into(), 0), ("s-a".into(), 1), ("s-d".into(), 2)]);
}
