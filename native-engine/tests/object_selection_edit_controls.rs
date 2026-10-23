//! L02-H controls: one revision and one history entry per edit, byte-exact
//! undo/redo, retained request replay that never executes twice, no-op
//! preservation of presence, bytes and redo, envelope refusals, owner pins on
//! sealed candidates, and project save/reopen of every edit.
use super::object_selection_edit_clipboard::copy;
use super::object_selection_edit_tests::{
    apply, base, bytes, delete, dispatch, duplicate, frozen, owner_of, prepare, request, send,
    split, strokes_at, value,
};
use super::*;
use serde_json::{json, Value};

fn operations() -> Vec<(&'static str, Value)> {
    let clip = copy(&owner_of(&base(2)), "L0", 0, &["b"]).unwrap().unwrap();
    let mut operations = vec![
        ("delete", delete("L0", 0, "motion", &["b"])),
        ("duplicate", duplicate("L0", 0, "motion", &["b"])),
        ("split", split("L0", 0)),
        (
            "paste",
            json!({"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"L1",
                "frame":0,"mode":"motion","clip":serde_json::to_value(&clip).unwrap()}),
        ),
    ];
    operations.extend(held_operations());
    operations
}

fn held_operations() -> Vec<(&'static str, Value)> {
    let clip = copy(&owner_of(&base(2)), "L0", 0, &["b"]).unwrap().unwrap();
    vec![
        ("delete", delete("L0", 5, "animation2d", &["d"])),
        ("duplicate", duplicate("L0", 5, "animation2d", &["d"])),
        (
            "paste",
            json!({"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"L0",
                "frame":5,"mode":"animation2d","clip":serde_json::to_value(&clip).unwrap()}),
        ),
    ]
}

#[test]
fn the_real_owner_promotes_the_held_cell_with_the_edit_and_preserves_its_source() {
    for version in [1, 2] {
        for (op, payload) in held_operations() {
            let mut owner = owner_of(&base(version));
            let before = frozen(&owner);
            let candidate = prepare(&owner, op, &payload).unwrap();
            assert!(
                candidate.promoted() && candidate.changed(),
                "{op} v{version}"
            );
            assert_eq!(candidate.content_frame(), 5);
            assert_eq!(frozen(&owner), before, "preparation publishes nothing");
            owner.commit_selection_edit(candidate).unwrap();
            let after = value(&owner);
            let expected = if op == "delete" {
                vec!["a"]
            } else {
                vec!["a", "d", "native-stroke:1:0"]
            };
            assert_eq!(strokes_at(&after, "L0", 5), expected, "{op} v{version}");
            assert_eq!(strokes_at(&after, "L0", 3), ["a", "d"]);
            assert_eq!(strokes_at(&after, "L0", 0), ["a", "b", "c"]);
            assert_eq!(strokes_at(&after, "L1", 0), ["x"]);
            assert_eq!(
                after["layers"][0]["timing"]["keys"],
                json!([{"frame":0},{"frame":3},{"frame":5}])
            );
            if version == 2 {
                let elements = super::object_selection_edit_tests::elements_at(&after, "L0", 5);
                let expected: Vec<_> = expected
                    .iter()
                    .enumerate()
                    .map(|(slot, stroke)| (stroke.to_string(), slot as u64))
                    .collect();
                assert_eq!(elements, expected);
            }
            assert_eq!(
                (owner.content_revision(), owner.history_depths()),
                (1, (1, 0))
            );
        }
    }
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
fn identities_are_named_after_the_published_revision_so_undo_never_recycles_them() {
    let mut owner = owner_of(&base(1));
    let payload = duplicate("L0", 0, "motion", &["b"]);
    apply(&mut owner, "duplicate", &payload).unwrap();
    owner.undo(1).unwrap();
    apply(&mut owner, "duplicate", &payload).unwrap();
    assert_eq!(
        strokes_at(&value(&owner), "L0", 0),
        ["a", "b", "c", "native-stroke:3:0"],
        "revision 2 was the undo; the next publication is revision 3"
    );
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
    for (op, payload) in operations() {
        let mut owner = owner_of(&base(2));
        let req = request(&owner, "once", op, &payload);
        let first = dispatch(&mut owner, &req);
        let after = bytes(&owner);
        assert_eq!(dispatch(&mut owner, &req), first, "{op}");
        assert_eq!(
            (owner.content_revision(), bytes(&owner)),
            (1, after),
            "{op}"
        );
        assert_eq!(owner.history_depths(), (1, 0), "{op}");
        owner.undo(1).unwrap();
        let undone = bytes(&owner);
        assert_eq!(dispatch(&mut owner, &req), first, "{op}: replay after undo");
        assert_eq!(
            (owner.content_revision(), bytes(&owner)),
            (2, undone),
            "{op}"
        );
        assert_eq!(owner.history_depths(), (0, 1), "{op}");
        let mut altered = req.clone();
        altered["payload"]["frame"] = json!(1);
        let refused = dispatch(&mut owner, &altered);
        assert_eq!(refused["ok"], false, "{op}");
        assert_eq!(refused["error"]["code"], "invalid_request", "{op}");
        assert_eq!(owner.history_depths(), (0, 1), "{op}");
    }
}

#[test]
fn envelope_refusals_leave_state_and_redo_alone_and_can_be_retried() {
    for (op, payload) in operations() {
        let mut owner = owner_of(&base(2));
        apply(&mut owner, "delete", &delete("L0", 0, "motion", &["c"])).unwrap();
        owner.undo(1).unwrap();
        let before = frozen(&owner);
        let good = request(&owner, "env", op, &payload);
        let mut cancelled = good.clone();
        cancelled["cancelledBeforeDispatch"] = json!(true);
        let mut stale = good.clone();
        stale["expectedRevision"] = json!(before.2 + 1);
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove("expectedRevision");
        let mut instance = good.clone();
        instance["instanceId"] = json!("other-instance");
        let mut document = good.clone();
        document["documentId"] = json!("other-document");
        let mut api = good.clone();
        api["apiVersion"] = json!(1);
        for (label, mut req, code) in [
            ("cancelled", cancelled, "cancelled_before_dispatch"),
            ("stale", stale, "stale_revision"),
            ("missing", missing, "invalid_request"),
            ("instance", instance, "wrong_instance"),
            ("document", document, "wrong_document"),
            ("api", api, "invalid_request"),
        ] {
            // Each refusal owns its requestId: a retained refusal answers only its own body.
            req["requestId"] = json!(format!("env-{label}"));
            let reply = dispatch(&mut owner, &req);
            assert_eq!(reply["ok"], false, "{op} {label}");
            assert_eq!(reply["error"]["code"], code, "{op} {label}");
            assert_eq!(frozen(&owner), before, "{op} {label}");
        }
        // The refusals changed nothing, so the valid request still applies.
        let reply = dispatch(&mut owner, &good);
        assert_eq!(reply["ok"], true, "{op}");
        assert_eq!(
            owner.history_depths(),
            (1, 0),
            "{op}: the edit cleared redo"
        );
    }
}

#[test]
fn a_noop_preserves_presence_bytes_history_and_redo() {
    let mut owner = owner_of(&base(2));
    apply(&mut owner, "delete", &delete("L0", 0, "motion", &["c"])).unwrap();
    owner.undo(1).unwrap();
    let before = frozen(&owner);
    assert_eq!(before.1, (0, 1));
    let noops = [
        ("delete", delete("L0", 0, "motion", &[])),
        ("duplicate", duplicate("L0", 0, "animation2d", &[])),
        ("delete", delete("L0", 5, "animation2d", &[])),
        ("duplicate", duplicate("L0", 5, "animation2d", &[])),
    ];
    for (n, (op, payload)) in noops.iter().enumerate() {
        let candidate = prepare(&owner, op, payload).unwrap();
        assert!(!candidate.changed(), "{op}");
        assert!(candidate.removed().is_empty() && candidate.created().is_empty());
        assert_eq!(apply(&mut owner, op, payload), Ok(false), "{op}");
        let reply = send(&mut owner, &format!("noop-{n}"), op, payload);
        assert_eq!(
            reply["result"],
            json!({"applied":false,"historyEntriesAdded":0}),
            "{op}"
        );
        assert_eq!(frozen(&owner), before, "{op}");
    }
    apply(
        &mut owner,
        "duplicate",
        &duplicate("L0", 0, "motion", &["a"]),
    )
    .unwrap();
    assert_eq!(owner.history_depths(), (1, 0), "a real edit clears redo");
}

#[test]
fn sealed_candidates_pin_owner_document_and_revision() {
    for (op, payload) in operations() {
        let mut source = owner_of(&base(2));
        let candidate = prepare(&source, op, &payload).unwrap();
        let mut other = owner_of(&base(2));
        let untouched = bytes(&other);
        assert_eq!(
            other.commit_selection_edit(candidate.clone()).unwrap_err(),
            DispatchErrorCode::WrongDocument,
            "{op}"
        );
        let mut foreign = NativeObjectHistory::new(
            "foreign",
            crate::object_codec::decode_storage_project(base(2).to_string().as_bytes()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            foreign
                .commit_selection_edit(candidate.clone())
                .unwrap_err(),
            DispatchErrorCode::WrongInstance,
            "{op}"
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
        source.commit_selection_edit(candidate.clone()).unwrap();
        let after = bytes(&source);
        assert_eq!(
            source.commit_selection_edit(candidate).unwrap_err(),
            DispatchErrorCode::StaleRevision,
            "{op}"
        );
        assert_eq!((bytes(&source), source.history_depths()), (after, (1, 0)));
    }
}

#[test]
fn the_edited_document_survives_save_and_reopen_byte_for_byte() {
    for (op, payload) in operations() {
        let mut owner = owner_of(&base(2));
        apply(&mut owner, op, &payload).unwrap();
        let saved = value(&owner);
        let reopened = owner_of(&saved);
        assert_eq!(bytes(&reopened), bytes(&owner), "{op}");
    }
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
                NativeObjectHistory::from_admitted_project("project-edits", project).unwrap();
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
