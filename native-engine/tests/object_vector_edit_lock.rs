//! PRIVATE UNRUN: Drawing-only vector lock policy through the existing owner.
//! Generic Motion properties are deliberately outside this command's policy.
use super::support::*;
use serde_json::{json, Value};

fn operations() -> Vec<(&'static str, Value)> {
    vec![
        ("lock-create", create(record_at("D", "L1", 0, ""))),
        ("lock-replace", replace(record_at("D2", "L1", 0, "s-a"))),
        ("lock-delete", delete(target("L1", 0, "s-a"))),
    ]
}

#[test]
fn locked_vector_edits_refuse_with_source_snapshot_revision_and_redo_unchanged() {
    for version in [1, 2] {
        let mut source = document(version, seeds(), None);
        source["layers"][0]["locked"] = json!(true);
        let mut owner = owner_of(&source);
        assert_eq!(state(&owner), source); // Persisted true is admitted in both versions.
        let owed = seed_redo(&mut owner); // This targets unlocked L2, not locked L1.
        let pinned = owner.acquire_snapshot(owner.content_revision()).unwrap();
        for (id, payload) in operations() {
            assert_unavailable(&mut owner, id, payload);
        }
        assert_eq!(serde_json::to_value(pinned.document()).unwrap(), source);
        let revision = owner.content_revision();
        assert_eq!(owner.redo(revision), Ok(revision + 1));
        assert_eq!(
            bytes(&owner),
            owed,
            "refusals preserve the actual redo content"
        );
        assert_eq!(owner.history_depths(), (1, 0));
        assert_eq!(serde_json::to_value(pinned.document()).unwrap(), source);
    }
}

#[test]
fn absent_false_shy_and_hidden_layers_accept_each_vector_operation() {
    for version in [1, 2] {
        for policy in [
            json!({}),
            json!({"locked":false}),
            json!({"shy":true}),
            json!({"visible":false}),
            json!({"locked":false,"shy":true,"visible":false}),
        ] {
            for (id, payload) in operations() {
                let mut source = document(version, seeds(), None);
                for (key, value) in policy.as_object().unwrap() {
                    source["layers"][0][key] = value.clone();
                }
                let mut owner = owner_of(&source);
                assert_eq!(state(&owner), source);
                let old = owner.acquire_snapshot(0).unwrap();
                let response = send(&mut owner, id, payload);
                assert_eq!(response["ok"], true, "v{version} {policy} {id}: {response}");
                assert_eq!(response["contentRevision"], 1);
                assert_eq!(response["result"]["applied"], true);
                assert_eq!(response["result"]["historyEntriesAdded"], 1);
                assert_eq!(owner.content_revision(), 1);
                assert_eq!(owner.history_depths(), (1, 0));
                let after = state(&owner);
                assert_eq!(
                    after["layers"], source["layers"],
                    "policy presence is not normalized"
                );
                assert_eq!(serde_json::to_value(old.document()).unwrap(), source);
                assert_eq!(owner.undo(1), Ok(2));
                assert_eq!(
                    bytes(&owner),
                    crate::object_codec::encode_storage_project(old.document()).unwrap()
                );
            }
        }
    }
}
