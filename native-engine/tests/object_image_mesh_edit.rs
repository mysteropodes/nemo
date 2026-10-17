//! Behavior of the image-mesh edit transaction (L03-L, F17/F18): `setOffset` and `resetOffsets` through
//! the real dispatcher, owner, history and codec. Literal expected values come from the executed
//! baseline (`oracle.txt`) or from the fixture corpus mesh; nothing is derived from the code under test.
use super::object_image_mesh_edit_oracle as oracle;
use super::object_image_mesh_edit_rows::*;
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn literal(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn oracle_row_counts_are_pinned() {
    for (tag, count) in [("mesh", 6), ("so", 161), ("ro", 6), ("os", 68), ("dn", 25)] {
        assert_eq!(oracle::rows(tag).len(), count, "{tag}");
    }
    assert_eq!(oracle::ORACLE.lines().count(), 266);
}

#[test]
fn set_offset_rows_match_the_executed_baseline() {
    let starts = oracle::starts();
    let (mut applied, mut refused) = (0, 0);
    for row in oracle::offset_rows() {
        let start = &starts.iter().find(|(n, _)| *n == row.start).unwrap().1;
        let mut owner = owner_with(start);
        let before = state(&owner);
        let reply = run(
            &mut owner,
            "e",
            payload(ID, set_offset(row.index, row.du, row.dv)),
        );
        let what = format!("{} {} {} {}", row.start, row.index, row.du, row.dv);
        if !row.ret {
            // The baseline returned false and left the offsets alone; the native request is a typed refusal.
            assert!(oracle::same_pts(&row.offsets, &start.offsets), "{what}");
            assert_eq!(code(&reply), "invalid_request", "{what}");
            assert_eq!(state(&owner), before, "{what}");
            refused += 1;
            continue;
        }
        assert_eq!(reply["ok"], true, "{what}: {reply}");
        let mesh = stored(&owner, ID);
        assert!(oracle::same_pts(&offsets_of(&mesh), &row.offsets), "{what}");
        // Rest geometry, outline prefix, triangles and density are never touched by an offset edit.
        assert!(
            oracle::same_pts(&mesh.rest_points(), &start.verts),
            "{what}"
        );
        assert!(
            oracle::same_pts(&mesh.outline_points(), &start.outline),
            "{what}"
        );
        assert_eq!(mesh.triangles(), start.tris.as_slice(), "{what}");
        assert_eq!(
            (mesh.effective_cols(), mesh.effective_rows()),
            (start.cols, start.rows),
            "{what}"
        );
        let changed = !oracle::same_pts(&row.offsets, &start.offsets);
        assert_eq!(
            reply["result"],
            json!({"applied":changed,"historyEntriesAdded":u64::from(changed)}),
            "{what}"
        );
        assert_eq!(owner.history_depths(), (usize::from(changed), 0), "{what}");
        applied += 1;
    }
    assert_eq!((applied, refused), (107, 54));
}

#[test]
fn reset_offsets_rows_match_the_executed_baseline() {
    let starts = oracle::starts();
    for (name, expected) in oracle::reset_rows() {
        let start = &starts.iter().find(|(n, _)| *n == name).unwrap().1;
        assert!(
            expected.iter().all(|o| o[0] == 0.0 && o[1] == 0.0),
            "{name}: the baseline writes zeros"
        );
        let mut owner = owner_with(start);
        let reply = run_ok(&mut owner, "r", payload(ID, reset_offsets()));
        let mesh = stored(&owner, ID);
        assert!(oracle::same_pts(&offsets_of(&mesh), &expected), "{name}");
        assert_eq!(
            mesh.effective_offsets().len(),
            start.verts.len(),
            "{name}: one zero pair per vertex"
        );
        let changed = start.offsets.iter().any(|o| o[0] != 0.0 || o[1] != 0.0);
        assert_eq!(
            reply["result"],
            json!({"applied":changed,"historyEntriesAdded":u64::from(changed)}),
            "{name}"
        );
        assert_eq!(owner.history_depths(), (usize::from(changed), 0), "{name}");
    }
}

#[test]
fn fixture_edit_then_reset_returns_the_initial_bytes() {
    let mut owner = owner_of_text(&doc_text(CORPUS_ZERO));
    let initial = state(&owner);
    assert_eq!(wire(&owner)["imageMeshes"], literal(CORPUS_ZERO));
    let drag = run_ok(&mut owner, "drag", payload("im_1", set_offset(8, 0.1, 0.0)));
    assert_eq!(
        drag["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(wire(&owner)["imageMeshes"], literal(CORPUS_DRAGGED));
    let dragged = bytes(&owner);
    let reset = run_ok(&mut owner, "reset", payload("im_1", reset_offsets()));
    assert_eq!(
        reset["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(
        bytes(&owner),
        initial.0,
        "the reset returns the document bytes to the initial bytes"
    );
    assert_eq!(owner.history_depths(), (2, 0));
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        bytes(&owner),
        dragged,
        "undo restores the exact dragged offsets"
    );
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), initial.0);
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), dragged);
    assert_eq!(owner.history_depths(), (1, 1));
}

#[test]
fn an_equal_offset_is_a_noop_preserving_presence_bytes_history_and_redo() {
    // Stored 0.1 at slot 8; a real edit then an undo leaves one redo entry that a no-op must not erase.
    let mut owner = owner_of_text(&doc_text(CORPUS_DRAGGED));
    run_ok(
        &mut owner,
        "real",
        payload("im_1", set_offset(0, 0.02, 0.0)),
    );
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(owner.history_depths(), (0, 1));
    let before = state(&owner);
    // 0.0 is the same JavaScript number as the stored 0: not even the authored spelling changes.
    let same = json!({"kind":"setOffset","index":8,"du":0.1,"dv":0.0});
    let reply = run_ok(&mut owner, "same", payload("im_1", same));
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&owner), before);
    // A mesh that never authored offsets: a zero write keeps them absent, a real edit authors them.
    let mut entry = oracle::start("corpus").entry();
    for member in ["offsets", "cols", "rows"] {
        entry.as_object_mut().unwrap().remove(member);
    }
    let mut owner = owner_of(&doc(1, json!({ "im_1": entry })));
    let initial = state(&owner);
    assert!(wire(&owner)["imageMeshes"]["im_1"].get("offsets").is_none());
    let zero = run_ok(&mut owner, "zero", payload("im_1", set_offset(3, 0.0, 0.0)));
    assert_eq!(
        zero["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    let reset = run_ok(&mut owner, "none", payload("im_1", reset_offsets()));
    assert_eq!(
        reset["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(state(&owner), initial, "absent offsets stay absent");
    run_ok(&mut owner, "real", payload("im_1", set_offset(3, 0.2, 0.0)));
    let authored = wire(&owner)["imageMeshes"]["im_1"].clone();
    let mut expected = vec![json!([0, 0]); 9];
    expected[3] = json!([0.2, 0]);
    assert_eq!(authored["offsets"], json!(expected));
    assert!(authored.get("cols").is_none() && authored.get("rows").is_none());
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        bytes(&owner),
        initial.0,
        "undo makes the absent offsets absent again"
    );
}

#[test]
fn reset_over_zero_offsets_is_a_noop_whatever_their_spelling() {
    let mut entry = oracle::start("corpus").entry();
    entry["offsets"] = json!(vec![json!([0.0, -0.0]); 9]);
    let mut owner = owner_of(&doc(1, json!({ "im_1": entry })));
    let before = state(&owner);
    let reply = run_ok(&mut owner, "reset", payload("im_1", reset_offsets()));
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(
        state(&owner),
        before,
        "all-zero offsets are left byte for byte"
    );
    // One non-zero slot makes the reset real; the zeros it writes are spelled like JSON.stringify.
    let mut entry = oracle::start("corpus").entry();
    entry["offsets"] = json!(vec![json!([0, 0]); 9]);
    entry["offsets"][2] = json!([0.25, 0]);
    let mut owner = owner_of(&doc(1, json!({ "im_1": entry })));
    let reply = run_ok(&mut owner, "reset", payload("im_1", reset_offsets()));
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(
        wire(&owner)["imageMeshes"]["im_1"]["offsets"],
        json!(vec![json!([0, 0]); 9])
    );
}

#[test]
fn reset_replaces_every_slot_and_undo_redo_move_the_exact_offsets() {
    let start = oracle::start("g4s");
    assert!(
        start
            .offsets
            .iter()
            .filter(|o| o[0] != 0.0 || o[1] != 0.0)
            .count()
            >= 5
    );
    let mut owner = owner_with(&start);
    let before = bytes(&owner);
    run_ok(&mut owner, "reset", payload(ID, reset_offsets()));
    let after = bytes(&owner);
    assert_ne!(before, after);
    assert!(offsets_of(&stored(&owner, ID))
        .iter()
        .all(|o| o[0] == 0.0 && o[1] == 0.0));
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        bytes(&owner),
        before,
        "undo restores the exact prior offsets"
    );
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), after, "redo reapplies the reset");
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn an_outline_vertex_edit_moves_only_its_offset_slot_and_the_mask_prefix_stays() {
    let start = oracle::start("g4");
    let mut owner = owner_with(&start);
    run_ok(
        &mut owner,
        "outline-vertex",
        payload(ID, set_offset(0, 0.07, -0.02)),
    );
    let mesh = stored(&owner, ID);
    let offsets = offsets_of(&mesh);
    assert_eq!(offsets[0], [0.07, -0.02]);
    assert!(offsets.iter().skip(1).all(|o| o[0] == 0.0 && o[1] == 0.0));
    // The outline is the vertex prefix and both keep their REST positions: the mask deforms through the offset.
    assert!(oracle::same_pts(&mesh.rest_points(), &start.verts));
    assert!(oracle::same_pts(&mesh.outline_points(), &start.outline));
    assert!(oracle::same_pts(
        &mesh.rest_points()[..start.outline.len()],
        &mesh.outline_points()
    ));
}

#[test]
fn an_index_past_the_last_vertex_is_refused_and_publishes_nothing() {
    let start = oracle::start("g4");
    let len = start.verts.len() as i64;
    for index in [len, len + 1, 1000, i64::from(u32::MAX)] {
        let mut owner = owner_with(&start);
        let before = state(&owner);
        let reply = run(&mut owner, "past", payload(ID, set_offset(index, 0.1, 0.1)));
        assert_eq!(code(&reply), "invalid_request", "{index}");
        assert_eq!(state(&owner), before, "{index}");
    }
    let mut owner = owner_with(&start);
    run_ok(
        &mut owner,
        "last",
        payload(ID, set_offset(len - 1, 0.1, 0.1)),
    );
    assert_eq!(
        offsets_of(&stored(&owner, ID))[(len - 1) as usize],
        [0.1, 0.1]
    );
}

#[test]
fn unknown_mesh_and_missing_store_are_not_found() {
    let start = oracle::start("g4");
    let mut owner = owner_with(&start);
    let before = state(&owner);
    for (id, body) in [("other", set_offset(0, 0.1, 0.1)), ("M", reset_offsets())] {
        // one request id per request: reusing "x" with another payload is a fingerprint conflict
        // (`invalid_request`), which would mask the lookup this loop is about
        let reply = run(&mut owner, &format!("x-{id}"), payload(id, body));
        assert_eq!(code(&reply), "not_found", "{id}");
        assert_eq!(state(&owner), before, "{id}");
    }
    let mut owner = owner_of(&doc(1, Value::Null));
    let before = state(&owner);
    let reply = run(&mut owner, "x", payload(ID, set_offset(0, 0.1, 0.1)));
    assert_eq!(
        code(&reply),
        "not_found",
        "a document without a store has no mesh to edit"
    );
    assert_eq!(state(&owner), before);
    let reply = run(&mut owner, "y", payload("", reset_offsets()));
    assert_eq!(
        code(&reply),
        "invalid_request",
        "an empty id is never addressable"
    );
}

#[test]
fn only_the_target_entry_changes_and_authored_store_order_is_kept() {
    let (a, b) = (oracle::start("g4s"), oracle::start("corpus"));
    let table = format!(
        r#"{{"b":{},"a":{}}}"#,
        serde_json::to_string(&b.entry()).unwrap(),
        serde_json::to_string(&a.entry()).unwrap()
    );
    let mut owner = owner_of_text(&doc_text(&table));
    let before = wire(&owner)["imageMeshes"].clone();
    run_ok(&mut owner, "a", payload("a", reset_offsets()));
    let after = wire(&owner)["imageMeshes"].clone();
    assert_eq!(
        after["b"], before["b"],
        "the other mesh is byte for byte untouched"
    );
    assert_ne!(after["a"], before["a"]);
    let text = String::from_utf8(bytes(&owner)).unwrap();
    assert!(
        text.find("\"b\":{").unwrap() < text.find("\"a\":{").unwrap(),
        "entry order is authored order"
    );
}

#[test]
fn edits_survive_save_and_reload_in_v1_and_v2_documents() {
    for version in [1, 2] {
        let mut owner = owner_of(&doc(version, store_of(&[(ID, &oracle::start("g4s"))])));
        run_ok(&mut owner, "a", payload(ID, set_offset(7, 0.11, -0.12)));
        run_ok(&mut owner, "b", payload(ID, set_offset(2, -0.5, 0.25)));
        let saved = bytes(&owner);
        let reopened =
            NativeObjectHistory::new("reopened", decode_storage_project(&saved).unwrap()).unwrap();
        assert_eq!(
            bytes(&reopened),
            saved,
            "v{version}: byte-identical after reload"
        );
        let mesh = stored(&reopened, ID);
        assert_eq!(offsets_of(&mesh)[7], [0.11, -0.12]);
        assert_eq!(offsets_of(&mesh)[2], [-0.5, 0.25]);
        assert_eq!(
            reopened.history_depths(),
            (0, 0),
            "history is not part of the saved bytes"
        );
    }
}

#[test]
fn an_edit_changes_nothing_outside_the_mesh_store() {
    let mut owner = owner_of(&doc(2, store_of(&[(ID, &oracle::start("g4s"))])));
    let outside = without_meshes(&owner);
    run_ok(&mut owner, "drag", payload(ID, set_offset(5, 0.3, 0.3)));
    run_ok(&mut owner, "reset", payload(ID, reset_offsets()));
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        without_meshes(&owner),
        outside,
        "layers, objects and order plan are untouched"
    );
    assert_eq!(current(&owner).document().layers().len(), 1);
}
