//! L02-G controls: one history entry per edit, byte-exact undo/redo, retained
//! request replay, no-op preservation of presence, bytes and redo, owner pins on
//! sealed candidates, and project save/reopen of the table.
use super::object_group_ops_tests::{
    apply, base, bytes, combine, dispatch, document, exit_group, group, has_table, owner_of,
    prepare, rename, request, send, set_mode, table_of, ungroup, value,
};
use super::*;
use serde_json::{json, Value};

fn operations() -> Vec<(&'static str, Value)> {
    vec![
        ("group", group(&["s5", "s6"], "gx", "X")),
        ("ungroup", ungroup(&["s1"])),
        ("combine", combine(&["s5", "s6"], "gk", "exclude")),
        ("mode", set_mode("g1", "subtract")),
        ("rename", rename("g2", "two")),
        ("exit", exit_group("s1")),
    ]
}
fn frozen(owner: &NativeObjectHistory) -> (Vec<u8>, (usize, usize), u64) {
    (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    )
}

#[test]
fn each_edit_is_one_revision_and_one_entry_and_undo_redo_restore_exact_bytes() {
    for version in [1, 2] {
        for (op, payload) in operations() {
            let mut owner = owner_of(&base(version));
            let before = bytes(&owner);
            assert_eq!(apply(&mut owner, op, &payload), Ok(true), "{op}");
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
fn dispatch_reports_the_compact_result_and_one_entry_per_edit() {
    for (op, payload) in operations() {
        let mut owner = owner_of(&base(2));
        let req = request(&owner, "go", op, &payload);
        let reply = dispatch(&mut owner, &req);
        assert_eq!(reply["ok"], true, "{op}");
        assert_eq!(
            reply["result"],
            json!({"applied":true,"historyEntriesAdded":1}),
            "{op}"
        );
        assert_eq!(owner.history_depths(), (1, 0), "{op}");
    }
}

#[test]
fn exact_replay_returns_the_retained_result_and_never_executes_twice() {
    let mut owner = owner_of(&base(2));
    let req = request(&owner, "once", "group", &group(&["s5", "s6"], "gx", "X"));
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
    altered["payload"]["groupId"] = json!("other");
    let refused = dispatch(&mut owner, &altered);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn a_noop_preserves_presence_bytes_history_and_redo() {
    let mut owner = owner_of(&base(2));
    apply(&mut owner, "rename", &rename("g2", "tmp")).unwrap();
    owner.undo(1).unwrap();
    let before = frozen(&owner);
    assert_eq!(before.1, (0, 1));
    let noops = [
        ("mode", set_mode("g1", "unite")),
        ("rename", rename("g1", "one")),
        // exactly the membership of g1, already `unite`
        ("combine", combine(&["s2", "s1"], "unused", "unite")),
    ];
    for (n, (op, payload)) in noops.iter().enumerate() {
        let candidate = prepare(&owner, op, payload).unwrap();
        assert!(!candidate.changed(), "{op}");
        assert_eq!(candidate.created(), None);
        assert_eq!(apply(&mut owner, op, payload), Ok(false), "{op}");
        let reply = send(&mut owner, &format!("noop-{n}"), op, payload);
        assert_eq!(
            reply["result"],
            json!({"applied":false,"historyEntriesAdded":0}),
            "{op}"
        );
        assert_eq!(frozen(&owner), before, "{op}");
    }
    apply(&mut owner, "mode", &set_mode("g1", "subtract")).unwrap();
    assert_eq!(owner.history_depths(), (1, 0), "a real edit clears redo");
}

#[test]
fn a_refusal_never_creates_the_table_and_leaves_state_and_redo_alone() {
    let mut owner = owner_of(&document(2, &["s1", "s2", "s3"], None));
    apply(&mut owner, "group", &group(&["s1", "s2"], "ga", "A")).unwrap();
    owner.undo(1).unwrap();
    assert!(!has_table(&owner));
    let before = frozen(&owner);
    // The baseline's ensureLayerGroups created an empty table BEFORE refusing.
    let refused = [
        (
            "ungroup",
            ungroup(&["s1"]),
            DispatchErrorCode::InvalidRequest,
        ),
        ("exit", exit_group("s1"), DispatchErrorCode::InvalidRequest),
        (
            "group",
            group(&["s1"], "gb", "B"),
            DispatchErrorCode::InvalidRequest,
        ),
        ("mode", set_mode("ga", "unite"), DispatchErrorCode::NotFound),
        ("rename", rename("ga", "x"), DispatchErrorCode::NotFound),
    ];
    for (op, payload, code) in refused {
        assert_eq!(apply(&mut owner, op, &payload), Err(code), "{op}");
        assert_eq!(frozen(&owner), before, "{op}");
        assert!(!has_table(&owner), "{op}");
    }
}

#[test]
fn sealed_candidates_pin_owner_document_and_revision() {
    let mut source = owner_of(&base(2));
    let candidate = prepare(&source, "mode", &set_mode("g1", "subtract")).unwrap();
    let mut other = owner_of(&base(2));
    let untouched = bytes(&other);
    assert_eq!(
        other.commit_group_edit(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        crate::object_codec::decode_storage_project(base(2).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        foreign.commit_group_edit(candidate.clone()).unwrap_err(),
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
    source.commit_group_edit(candidate.clone()).unwrap();
    let after = bytes(&source);
    assert_eq!(
        source.commit_group_edit(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!((bytes(&source), source.history_depths()), (after, (1, 0)));
}

#[test]
fn the_table_survives_undo_redo_save_and_reopen_with_presence_intact() {
    let mut owner = owner_of(&document(1, &["s1", "s2", "s3"], None));
    apply(
        &mut owner,
        "combine",
        &combine(&["s1", "s2"], "ga", "unite"),
    )
    .unwrap();
    let saved = value(&owner);
    assert_eq!(saved["layers"][0]["groups"]["entries"][0]["gid"], "ga");
    assert!(
        saved["layers"][1].get("groups").is_none(),
        "L1 never had a table"
    );
    let reopened = owner_of(&saved);
    assert_eq!(bytes(&reopened), bytes(&owner));
    assert_eq!(table_of(&reopened), table_of(&owner));
    owner.undo(1).unwrap();
    assert!(!has_table(&owner));
    assert_eq!(value(&owner)["layers"][0].get("groups"), None);
}

#[cfg(feature = "project_media_codec")]
#[test]
fn project_reference_and_metadata_survive_each_edit_save_reopen_and_undo() {
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
                NativeObjectHistory::from_admitted_project("project-groups", project).unwrap();
            let zero = owner.acquire_snapshot(0).unwrap();
            let before: Value =
                serde_json::from_slice(&zero.serialize_project_file().unwrap()).unwrap();
            apply(&mut owner, op, &payload).unwrap();
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
