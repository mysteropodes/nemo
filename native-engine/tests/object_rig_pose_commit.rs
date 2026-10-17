//! Reset pose and the refusals shared by both pose actions (L03-Q, I13), against the executed baseline
//! rows and independent literal fixtures, through the REAL `NativeObjectHistory`. Commit success rows that
//! need the evaluation kernels live in `object_rig_pose_commit_kernels.rs`.
use super::object_rig_pose_commit_oracle as oracle;
use super::object_rig_pose_commit_rows::*;
use super::*;
use serde_json::{json, Value};

/// A rig whose bone is posed away from its bind pose, with the three handle states the record keeps apart:
/// absent (`handleIn` of vertex 0 in the live pose), `null` and a value.
const POSED_RIG: &str = r#"{"bones":{"bone1":{"segments":[{"point":[0,0],"handleOut":[5,0]},{"point":[40,30],"handleIn":null,"handleOut":[7,1]}],"restSegments":[{"point":[0,0],"handleIn":null},{"point":[100,0],"handleOut":[0.5,0]}],"closed":false}},"ikChains":{"bone1:1":{"root":{"boneId":"bone1","vi":0},"joint":{"boneId":"bone1","vi":1},"end":{"boneId":"bone1","vi":1},"l1":40,"l2":50,"flip":true}},"nextId":2,"binds":[]}"#;
const RESET_RIG: &str = r#"{"bones":{"bone1":{"segments":[{"point":[0,0],"handleIn":null},{"point":[100,0],"handleOut":[0.5,0]}],"restSegments":[{"point":[0,0],"handleIn":null},{"point":[100,0],"handleOut":[0.5,0]}],"closed":false}},"ikChains":{"bone1:1":{"root":{"boneId":"bone1","vi":0},"joint":{"boneId":"bone1","vi":1},"end":{"boneId":"bone1","vi":1},"l1":40,"l2":50,"flip":true}},"nextId":2,"binds":[]}"#;

fn literal_owner(rig: &str) -> NativeObjectHistory {
    let rig: Value = serde_json::from_str(rig).unwrap();
    owner_of(&document_json(
        vec![layer_json(LAYER, &rig, None, false)],
        Vec::new(),
    ))
}

#[test]
fn oracle_row_counts_and_outcome_classes_are_pinned() {
    let rows = oracle::rows();
    assert_eq!(rows.len(), 20);
    let ret = |value: Value| rows.iter().filter(|r| r.commit_ret() == &value).count();
    assert_eq!(ret(json!(true)), 17);
    assert_eq!(ret(json!(false)), 2);
    assert_eq!(ret(json!("throws")), 1);
    assert_eq!(rows.iter().filter(|r| r.has_meshes()).count(), 5);
    assert_eq!(rows.iter().filter(|r| r.commits_natively()).count(), 11);
    assert_eq!(rows.iter().filter(|r| r.rig.is_null()).count(), 1);
    // The authored member order is the export whitelist order (Value sorts keys, so read the text).
    for row in rows.iter().filter(|r| !r.rig.is_null()) {
        let at = |member: &str| row.rig_raw.find(member).expect(member);
        let order = [
            at("\"bones\":"),
            at("\"ikChains\":"),
            at("\"nextId\":"),
            at("\"binds\":"),
        ];
        assert!(
            order.windows(2).all(|pair| pair[0] < pair[1]),
            "{}",
            row.name
        );
    }
}

#[test]
fn the_native_record_round_trips_every_oracle_rig_byte_for_byte() {
    for row in oracle::rows() {
        let owner = owner_for(&row, PLAIN);
        assert_eq!(rig_text(&owner), row.rig_raw, "{}", row.name);
        assert_eq!(
            rig_text(&owner) == "null",
            row.rig.is_null(),
            "{}",
            row.name
        );
    }
}

#[test]
fn reset_rows_match_the_executed_baseline_and_are_one_reversible_entry() {
    let mut changing = 0;
    for row in oracle::rows().iter().filter(|r| !r.rig.is_null()) {
        let mut owner = owner_for(row, PLAIN);
        let initial = state(&owner);
        let objects = objects_of(&owner);
        let want = row.reset_after();
        let changes = rig_text(&owner) != want;
        let reply = run_ok(&mut owner, row.name, reset_payload());
        assert_eq!(rig_text(&owner), want, "{}: rig after the reset", row.name);
        assert_eq!(reply["result"]["applied"], changes, "{}", row.name);
        assert_eq!(
            reply["result"]["historyEntriesAdded"],
            u64::from(changes),
            "{}",
            row.name
        );
        assert_eq!(
            objects_of(&owner),
            objects,
            "{}: strokes untouched",
            row.name
        );
        if !changes {
            assert_eq!(
                state(&owner),
                initial,
                "{}: a no-op changes nothing",
                row.name
            );
            continue;
        }
        changing += 1;
        let edited = state(&owner);
        assert_eq!(edited.1, initial.1 + 1, "{}", row.name);
        assert_eq!(edited.2, (1, 0), "{}: one history entry", row.name);
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(
            bytes(&owner),
            initial.0,
            "{}: undo restores the bytes",
            row.name
        );
        assert_eq!(rig_text(&owner), row.rig_raw, "{}", row.name);
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), edited.0, "{}: redo reapplies", row.name);
    }
    // The rows that start away from the rest pose really are posed (the discriminating half of the table).
    assert_eq!(changing, 15);
}

#[test]
fn every_bone_of_every_changing_row_is_back_on_its_bind_pose() {
    for row in oracle::rows().iter().filter(|r| !r.rig.is_null()) {
        let mut owner = owner_for(row, PLAIN);
        run_ok(&mut owner, row.name, reset_payload());
        let rig = rig_value(&owner);
        for (id, bone) in rig["bones"].as_object().unwrap() {
            assert_eq!(bone["segments"], bone["restSegments"], "{}: {id}", row.name);
        }
        // Rest rig data is the bind's, never the pose's.
        let before: Value = row.rig.clone();
        for member in ["ikChains", "nextId", "binds"] {
            assert_eq!(rig[member], before[member], "{}: {member}", row.name);
        }
        for (id, bone) in before["bones"].as_object().unwrap() {
            assert_eq!(
                rig["bones"][id]["restSegments"], bone["restSegments"],
                "{}: {id} rest pose",
                row.name
            );
        }
    }
}

#[test]
fn the_rest_pose_the_baseline_reset_leaves_is_the_one_a_later_pose_edit_cannot_reach() {
    // `restAfterEdit` is the baseline's restSegments after the live pose was nudged post-reset: equal to
    // the bind pose, i.e. the reset copy is deep. The native record owns its numbers; the same equality
    // holds for the record the transaction published.
    for row in oracle::rows().iter().filter(|r| !r.rig.is_null()) {
        let expected = &row.expect["reset"]["restAfterEdit"];
        if expected.is_null() {
            assert!(
                row.rig["bones"].as_object().unwrap().is_empty(),
                "{}",
                row.name
            );
            continue;
        }
        let mut owner = owner_for(row, PLAIN);
        run_ok(&mut owner, row.name, reset_payload());
        let rig = rig_value(&owner);
        let first = row.rig["bones"].as_object().unwrap().keys().next().unwrap();
        assert_eq!(
            &rig["bones"][first.as_str()]["restSegments"],
            expected,
            "{}",
            row.name
        );
    }
}

#[test]
fn reset_copies_the_bind_pose_with_the_authored_handle_states() {
    let mut owner = literal_owner(POSED_RIG);
    assert_eq!(rig_text(&owner), POSED_RIG);
    let reply = run_ok(&mut owner, "reset", reset_payload());
    assert_eq!(reply["result"]["applied"], true);
    // Absent handle -> `null` (the bind pose said null); explicit value kept; the IK chain, the counter
    // and the empty bind list are untouched.
    assert_eq!(rig_text(&owner), RESET_RIG);
    let again = run_ok(&mut owner, "again", reset_payload());
    assert_eq!(again["result"]["applied"], false, "already at rest");
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn a_pose_equal_to_the_rest_pose_in_value_is_left_exactly_as_authored() {
    // `100.0` and `100` are the same JavaScript number: nothing to reset, so not one byte is rewritten.
    let spelled = POSED_RIG
        .replace(
            r#""segments":[{"point":[0,0],"handleOut":[5,0]},{"point":[40,30],"handleIn":null,"handleOut":[7,1]}]"#,
            r#""segments":[{"point":[0.0,0],"handleIn":null},{"point":[100.0,0.0],"handleOut":[0.50,0]}]"#,
        );
    let mut owner = literal_owner(&spelled);
    let before = state(&owner);
    let reply = run_ok(&mut owner, "equal", reset_payload());
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(reply["result"]["historyEntriesAdded"], 0);
    assert_eq!(state(&owner), before);
}

#[test]
fn a_handle_state_that_differs_from_the_bind_pose_is_a_pose() {
    // Same numbers, but the live handleIn is absent where the bind pose says null: that is not the rest pose.
    let mut owner = literal_owner(
        &POSED_RIG
            .replace(
                r#"{"point":[40,30],"handleIn":null,"handleOut":[7,1]}"#,
                r#"{"point":[100,0],"handleOut":[0.5,0]}"#,
            )
            .replace(r#"{"point":[0,0],"handleOut":[5,0]}"#, r#"{"point":[0,0]}"#),
    );
    let reply = run_ok(&mut owner, "handle-state", reset_payload());
    assert_eq!(reply["result"]["applied"], true);
    assert_eq!(rig_text(&owner), RESET_RIG);
}

#[test]
fn a_reset_that_changes_nothing_keeps_the_redo_stack_of_the_document() {
    // L1 is posed, L2 has no rig. Reset L1 (one entry), undo it (one redo), then reset L2: the baseline
    // `if(!ld||!ld.rig)return` is a silent no-op, so presence, bytes, history and redo stay as they were.
    let row = oracle::row("arm_rotate_soft_bone_turned_40_degrees");
    let layers = vec![
        layer_json(LAYER, &row.rig, None, false),
        layer_json(OTHER, &Value::Null, None, false),
    ];
    let mut owner = owner_of(&document_json(layers, row_records(&row)));
    run_ok(&mut owner, "reset", reset_payload());
    owner.undo(owner.content_revision()).unwrap();
    let before = state(&owner);
    assert_eq!(before.2, (0, 1));
    let body = with(reset_payload(), |p| p["layerUid"] = json!(OTHER));
    let reply = run_ok(&mut owner, "rigless", body);
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(reply["result"]["historyEntriesAdded"], 0);
    assert_eq!(
        state(&owner),
        before,
        "a no-op keeps bytes, revision, history and redo"
    );
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(rig_text(&owner), row.reset_after());
}

#[test]
fn a_rigless_layer_resets_to_nothing_even_when_it_is_locked() {
    // The baseline returns before it asks `canEditActiveLayer`.
    let layers = vec![layer_json(LAYER, &Value::Null, None, true)];
    let mut owner = owner_of(&document_json(layers, Vec::new()));
    let before = state(&owner);
    let reply = run_ok(&mut owner, "locked-rigless", reset_payload());
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(state(&owner), before);
}

#[test]
fn a_locked_layer_refuses_both_actions_after_the_rig_checks() {
    let row = oracle::row("arm_rotate_soft_bone_turned_40_degrees");
    let locked = Build {
        locked: true,
        ..HELD
    };
    let mut owner = owner_for(&row, locked);
    assert_refused(
        &mut owner,
        "reset",
        reset_payload(),
        ("unavailable", "layer_locked"),
    );
    assert_refused(
        &mut owner,
        "commit",
        commit_payload(3),
        ("unavailable", "layer_locked"),
    );
    // A locked layer with no binds reports the missing binds first, as the baseline does.
    let empty = oracle::row("empty_rig_has_no_binds");
    let mut owner = owner_for(&empty, locked);
    assert_refused(
        &mut owner,
        "empty",
        commit_payload(3),
        ("not_found", "no_rigged_stroke"),
    );
}

#[test]
fn commit_without_a_rig_or_without_binds_is_the_typed_no_rigged_stroke_refusal() {
    for name in ["empty_rig_has_no_binds", "layer_that_never_had_a_rig"] {
        let row = oracle::row(name);
        // The baseline: `toastNoRiggedStroke` and a `false` return, no undo checkpoint.
        assert_eq!(row.commit_ret(), &json!(false), "{name}");
        assert_eq!(
            row.expect["commit"]["toasts"],
            json!(["toastNoRiggedStroke"]),
            "{name}"
        );
        assert_eq!(row.expect["commit"]["undo"], json!(0), "{name}");
        for build in [PLAIN, HELD] {
            let mut owner = owner_for(&row, build);
            for frame in [0, 3] {
                // One id per request: a refusal is receipted, so a second body under `name` is a changed-body reuse.
                assert_refused(
                    &mut owner,
                    &format!("{name}-{frame}"),
                    commit_payload(frame),
                    ("not_found", "no_rigged_stroke"),
                );
            }
        }
    }
}

#[test]
fn unknown_layers_and_frames_outside_the_document_are_refused_and_publish_nothing() {
    let row = oracle::row("arm_rotate_soft_bone_turned_40_degrees");
    let mut owner = owner_for(&row, HELD);
    let unknown = |body: Value| with(body, |p| p["layerUid"] = json!("nope"));
    assert_refused(
        &mut owner,
        "reset",
        unknown(reset_payload()),
        ("not_found", "layer_absent"),
    );
    assert_refused(
        &mut owner,
        "commit",
        unknown(commit_payload(3)),
        ("not_found", "layer_absent"),
    );
    assert_refused(
        &mut owner,
        "past-the-end",
        commit_payload(TOTAL),
        ("invalid_request", "frame_out_of_range"),
    );
}

#[cfg(not(feature = "evaluation"))]
#[test]
fn without_the_evaluation_kernels_a_rigged_commit_is_a_typed_unavailable() {
    for row in oracle::rows().iter().filter(|r| r.commits_natively()) {
        let mut owner = owner_for(row, HELD);
        assert_refused(
            &mut owner,
            row.name,
            commit_payload(3),
            ("unavailable", "pose_unavailable"),
        );
    }
}
