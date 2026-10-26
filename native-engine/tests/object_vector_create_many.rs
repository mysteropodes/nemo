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
