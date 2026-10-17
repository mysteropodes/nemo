//! L02-F controls: one history entry per operation, byte-exact undo/redo,
//! retained request replay, no-op preservation, owner pins on sealed candidates,
//! the shared identity rule and the baseline "spare the last layer" delete.
use super::object_layer_lifecycle_tests::{
    apply, base, bytes, delete, dispatch, duplicate, owner_of, rename, reorder, request, selection,
    send, value,
};
use super::*;
use serde_json::{json, Value};

fn operations() -> Vec<(&'static str, Value)> {
    let pick = || selection("L0", &["L0", "L2"], Some("L1"));
    vec![
        ("delete", delete("L1", &["L1"])),
        ("duplicate", duplicate("L1")),
        ("rename", rename("L2", "renamed")),
        ("reorder", reorder(&["L2"], 0.0, pick())),
    ]
}

#[test]
fn each_operation_is_one_revision_and_one_entry_and_undo_redo_restore_exact_bytes() {
    for version in [1, 2] {
        for (op, payload) in operations() {
            let mut owner = owner_of(&base(version));
            let before = bytes(&owner);
            apply(&mut owner, op, &payload);
            let after = bytes(&owner);
            assert_ne!(after, before, "{op} v{version}");
            assert_eq!(
                (owner.content_revision(), owner.history_depths()),
                (1, (1, 0))
            );
            assert_eq!(owner.undo(1).unwrap(), 2);
            assert_eq!(bytes(&owner), before, "{op} v{version} undo");
            assert_eq!(owner.history_depths(), (0, 1));
            assert_eq!(owner.redo(2).unwrap(), 3);
            assert_eq!(bytes(&owner), after, "{op} v{version} redo");
            assert_eq!(owner.history_depths(), (1, 0));
        }
    }
}

#[test]
fn dispatch_reports_compact_results_and_the_minted_identity_exactly() {
    let want = [
        ("delete", json!({"applied":true,"historyEntriesAdded":1})),
        (
            "duplicate",
            json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:1:0"}),
        ),
        ("rename", json!({"applied":true,"historyEntriesAdded":1})),
        ("reorder", json!({"applied":true,"historyEntriesAdded":1})),
    ];
    for ((op, payload), (name, result)) in operations().into_iter().zip(want) {
        assert_eq!(op, name);
        let mut owner = owner_of(&base(2));
        let req = request(&owner, "go", op, &payload);
        let reply = dispatch(&mut owner, &req);
        assert_eq!(reply["ok"], true, "{op}");
        assert_eq!(reply["result"], result, "{op}");
        assert_eq!(owner.history_depths(), (1, 0));
    }
    let mut owner = owner_of(&base(2));
    let first = send(&mut owner, "dup", "duplicate", &duplicate("L0"));
    assert_eq!(
        first["result"]["layerUid"],
        value(&owner)["layers"][3]["layerUid"]
    );
    let second = send(&mut owner, "dup-2", "duplicate", &duplicate("L0"));
    assert_eq!(second["result"]["layerUid"], "native-layer:2:0");
    assert_eq!(value(&owner)["layers"][4]["name"], "A copy 2");
}

#[test]
fn exact_replay_returns_the_retained_result_and_never_executes_twice() {
    let mut owner = owner_of(&base(2));
    let req = request(&owner, "once", "duplicate", &duplicate("L0"));
    let first = dispatch(&mut owner, &req);
    let after = bytes(&owner);
    assert_eq!(dispatch(&mut owner, &req), first);
    assert_eq!((owner.content_revision(), bytes(&owner)), (1, after));
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(1).unwrap();
    let undone = bytes(&owner);
    assert_eq!(dispatch(&mut owner, &req), first, "replay after undo");
    assert_eq!((owner.content_revision(), bytes(&owner)), (2, undone));
    assert_eq!(owner.history_depths(), (0, 1));
    let mut altered = req.clone();
    altered["payload"]["layerUid"] = json!("L1");
    let refused = dispatch(&mut owner, &altered);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn a_noop_preserves_presence_bytes_history_and_redo() {
    let mut owner = owner_of(&base(2));
    apply(&mut owner, "delete", &delete("L1", &["L1"]));
    owner.undo(1).unwrap();
    let (before, depths, revision) = (
        bytes(&owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    assert_eq!(depths, (0, 1));
    for (n, (op, payload)) in [
        ("rename", rename("L0", "A")),
        (
            "reorder",
            reorder(&["L1"], 1.0, selection("L2", &["L2"], Some("L0"))),
        ),
        (
            "reorder",
            reorder(&["L0", "L1"], 0.4, selection("L0", &[], None)),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let raw = payload.to_string();
        let candidate = if op == "rename" {
            owner.prepare_layer_rename_json(raw.as_bytes())
        } else {
            owner.prepare_layer_reorder_json(raw.as_bytes())
        }
        .unwrap();
        assert!(!candidate.changed());
        let reply = send(&mut owner, &format!("noop-{n}"), op, &payload);
        assert_eq!(
            reply["result"],
            json!({"applied":false,"historyEntriesAdded":0}),
            "{op}"
        );
        assert_eq!(
            (
                bytes(&owner),
                owner.history_depths(),
                owner.content_revision()
            ),
            (before.clone(), depths, revision)
        );
    }
    // The reorder no-op still reports where the selection sits.
    let stay = owner
        .prepare_layer_reorder_json(
            reorder(&["L1"], 1.0, selection("L2", &["L2"], Some("L0")))
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let slots = stay
        .selection()
        .map(|s| (s.active, s.selected.clone(), s.anchor));
    assert_eq!(slots, Some((2, vec![2], Some(0))));
    apply(&mut owner, "rename", &rename("L0", "changed"));
    assert_eq!(owner.history_depths(), (1, 0), "a real edit clears redo");
}

#[test]
fn sealed_candidates_pin_owner_document_and_revision() {
    let mut source = owner_of(&base(2));
    let raw = delete("L1", &["L1"]).to_string();
    let candidate = source.prepare_layer_delete_json(raw.as_bytes()).unwrap();
    let mut other = owner_of(&base(2));
    let untouched = bytes(&other);
    assert_eq!(
        other.commit_layer_lifecycle(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        crate::object_codec::decode_storage_project(base(2).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        foreign
            .commit_layer_lifecycle(candidate.clone())
            .unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(
        (
            bytes(&other),
            other.history_depths(),
            other.content_revision()
        ),
        (untouched, (0, 0), 0)
    );
    assert_eq!(foreign.history_depths(), (0, 0));
    source.commit_layer_lifecycle(candidate.clone()).unwrap();
    let after = bytes(&source);
    assert_eq!(
        source.commit_layer_lifecycle(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!((bytes(&source), source.history_depths()), (after, (1, 0)));
}

#[test]
fn duplicate_and_layer_create_mint_the_same_identity_on_a_collision() {
    let create = json!({"command":"object.layer.create","name":"N"}).to_string();
    let free = owner_of(&base(2));
    let made = free.prepare_layer_create_json(create.as_bytes()).unwrap();
    let copy = free
        .prepare_layer_duplicate_json(duplicate("L1").to_string().as_bytes())
        .unwrap();
    assert_eq!(
        (made.layer_uid(), copy.created()),
        ("native-layer:1:0", Some("native-layer:1:0"))
    );
    let taken = base(2)
        .to_string()
        .replace("\"L2\"", "\"native-layer:1:0\"");
    let owner = owner_of(&serde_json::from_str(&taken).unwrap());
    let made = owner.prepare_layer_create_json(create.as_bytes()).unwrap();
    let copy = owner
        .prepare_layer_duplicate_json(duplicate("L1").to_string().as_bytes())
        .unwrap();
    assert_eq!(
        (made.layer_uid(), copy.created()),
        ("native-layer:1:1", Some("native-layer:1:1"))
    );
}

#[test]
fn selecting_every_layer_deletes_down_to_the_lowest_slot_like_the_baseline() {
    let mut owner = owner_of(&base(2));
    let raw = delete("L1", &["L0", "L1", "L2"]).to_string();
    let candidate = owner.prepare_layer_delete_json(raw.as_bytes()).unwrap();
    assert_eq!(format!("{:?}", candidate.kind()), "Delete");
    assert_eq!(candidate.removed(), ["L1".to_string(), "L2".to_string()]);
    assert_eq!(candidate.spared(), Some("L0"));
    owner.commit_layer_lifecycle(candidate).unwrap();
    let after = value(&owner);
    assert_eq!(after["layers"].as_array().unwrap().len(), 1);
    assert_eq!(after["layers"][0]["layerUid"], "L0");
    assert_eq!(after["objects"].as_array().unwrap().len(), 1);
    assert_eq!(after["propertyHolders"].as_array().unwrap().len(), 2);
}

#[cfg(feature = "project_media_codec")]
#[test]
fn project_reference_and_metadata_survive_each_operation_save_reopen_and_undo() {
    use crate::object_document::{ProjectMetadata, ProjectReferenceState};
    let hex="89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";
    let png: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    for version in [1, 2] {
        for (op, payload) in operations() {
            let document =
                crate::object_codec::decode_storage_project(base(version).to_string().as_bytes())
                    .unwrap();
            let project =
                ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 29.97).unwrap(), document)
                    .with_reference(
                        ProjectReferenceState::admitted_still(
                            "media-1".into(),
                            "plate.png".into(),
                            false,
                            0.5,
                            -7,
                            png.clone(),
                            1024,
                        )
                        .unwrap(),
                    );
            let mut owner =
                NativeObjectHistory::from_admitted_project("project-lifecycle", project).unwrap();
            let zero = owner.acquire_snapshot(0).unwrap();
            let before: Value =
                serde_json::from_slice(&zero.serialize_project_file().unwrap()).unwrap();
            apply(&mut owner, op, &payload);
            let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
            let saved = owner
                .serialize_project_file(1, snapshot.snapshot_id())
                .unwrap();
            let after: Value = serde_json::from_slice(&saved).unwrap();
            assert_eq!(after["drawing"], value(&owner), "{op} v{version}");
            let mut kept = after.clone();
            kept["drawing"] = before["drawing"].clone();
            assert_eq!(kept, before, "{op}: metadata and reference are untouched");
            let reopened = NativeObjectHistory::from_project_file("reopened", &saved).unwrap();
            assert_eq!(value(&reopened), value(&owner));
            owner.undo(1).unwrap();
            let back = owner.acquire_snapshot(owner.content_revision()).unwrap();
            let restored: Value =
                serde_json::from_slice(&back.serialize_project_file().unwrap()).unwrap();
            assert_eq!(restored, before, "{op} v{version} undo");
        }
    }
}
