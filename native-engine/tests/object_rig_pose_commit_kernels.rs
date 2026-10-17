//! Commit pose against the executed baseline (L03-Q, I13): the rows that reach the pose evaluation of
//! L03-P. Compiled only with the `evaluation` feature, exactly like the kernels themselves.
//!
//! The baseline stores `rigCommitFrame`'s result with `saveActiveLayerFrame`; the native transaction writes
//! the same posed segments into the authored records of the promoted key. Every document is one layer with a
//! key at frame 0 (`HELD`), so frame 3 is a hold of it and committing there must promote it first.
use super::object_rig_pose_commit_oracle as oracle;
use super::object_rig_pose_commit_rows::*;
use super::*;
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{evaluate_rig_pose, RigPosePin};
use serde_json::{json, Value};

const HELD_FRAME: u32 = 3;
const MESHED_HELD: Build<'static> = Build {
    keys: Some(&[0]),
    locked: false,
    meshes: true,
};
/// Rows whose pose already equals the stored geometry: committing on an existing key changes no byte.
const AT_REST: [&str; 2] = [
    "ik_reset_pose_gives_the_stored_geometry_back",
    "rest_pose_bound_stroke_commit_replaces_with_equal_values",
];

fn commit_rows() -> Vec<oracle::Row> {
    oracle::rows()
        .into_iter()
        .filter(|r| r.commits_natively())
        .collect()
}

fn stroke_id(record: &Value) -> &str {
    record["target"]["strokeId"].as_str().unwrap()
}

/// The records at `frame` hold the baseline's saved segments (numbers within the oracle tolerance).
fn assert_saved_like_the_baseline(row: &oracle::Row, owner: &NativeObjectHistory, frame: u32) {
    let saved = row.expect["commit"]["saved"].as_object().expect("saved");
    let records = records_at(owner, frame);
    assert_eq!(records.len(), row.strokes.len(), "{}", row.name);
    for record in &records {
        let want = oracle::segs(&saved[stroke_id(record)]);
        let got = record_segs(record);
        assert_eq!(got.len(), want.len(), "{} {}", row.name, stroke_id(record));
        for (g, w) in got.iter().zip(want.iter()) {
            for k in 0..6 {
                assert!(
                    oracle::close(g[k], w[k]),
                    "{} {}: {g:?} vs {w:?}",
                    row.name,
                    stroke_id(record)
                );
            }
        }
    }
}

#[test]
fn commit_rows_match_the_executed_baseline_at_a_held_frame_and_are_one_reversible_entry() {
    let rows = commit_rows();
    assert_eq!(rows.len(), 11);
    for row in &rows {
        // The baseline: one undo checkpoint, the success toast, the rig left as it was.
        assert_eq!(row.expect["commit"]["undo"], json!(1), "{}", row.name);
        let mut owner = owner_for(row, HELD);
        let initial = state(&owner);
        let rig_before = rig_text(&owner);
        let key_records = records_at(&owner, 0);
        let reply = run_ok(&mut owner, row.name, commit_payload(HELD_FRAME));
        assert_eq!(reply["result"]["applied"], true, "{}", row.name);
        assert_eq!(reply["result"]["historyEntriesAdded"], 1, "{}", row.name);
        assert_saved_like_the_baseline(row, &owner, HELD_FRAME);
        assert_eq!(
            records_at(&owner, 0),
            key_records,
            "{}: the source key is untouched",
            row.name
        );
        assert_eq!(
            rig_text(&owner),
            rig_before,
            "{}: rest rig data is preserved",
            row.name
        );
        assert_eq!(
            wire(&owner)["layers"][0]["timing"]["keys"],
            json!([{"frame": 0}, {"frame": HELD_FRAME}]),
            "{}: the hold became a key",
            row.name
        );
        let edited = state(&owner);
        assert_eq!(edited.1, initial.1 + 1, "{}", row.name);
        assert_eq!(
            edited.2,
            (1, 0),
            "{}: promotion, deform and replace are ONE entry",
            row.name
        );
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(
            bytes(&owner),
            initial.0,
            "{}: undo restores promotion and pose together",
            row.name
        );
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), edited.0, "{}: redo reapplies", row.name);
    }
}

#[test]
fn commit_on_an_existing_key_replaces_in_place_and_promotes_nothing() {
    let mut at_rest = Vec::new();
    for row in &commit_rows() {
        let mut owner = owner_for(row, HELD);
        let initial = state(&owner);
        let saved = row.expect["commit"]["saved"].as_object().unwrap();
        let differs = row
            .strokes
            .iter()
            .any(|s| s["segs"] != saved[s["id"].as_str().unwrap()]);
        let reply = run_ok(&mut owner, row.name, commit_payload(0));
        assert_eq!(reply["result"]["applied"], differs, "{}", row.name);
        assert_saved_like_the_baseline(row, &owner, 0);
        assert_eq!(
            wire(&owner)["layers"][0]["timing"]["keys"],
            json!([{"frame": 0}]),
            "{}: no promotion on a key",
            row.name
        );
        assert_eq!(
            objects_of(&owner).as_array().unwrap().len(),
            row.strokes.len(),
            "{}",
            row.name
        );
        if !differs {
            at_rest.push(row.name);
            assert_eq!(
                state(&owner),
                initial,
                "{}: nothing posed, nothing written",
                row.name
            );
        }
    }
    assert_eq!(at_rest, AT_REST);
}

#[test]
fn the_committed_segments_are_the_pose_evaluation_output_unchanged() {
    let row = oracle::row("arm_rotate_soft_bone_turned_40_degrees");
    let mut owner = owner_for(&row, HELD);
    let snapshot = current(&owner);
    let evaluation =
        evaluate_rig_pose(&snapshot, &RigPosePin::of(&snapshot), LAYER, 0, None).unwrap();
    assert_eq!(evaluation.strokes.len(), 1);
    run_ok(&mut owner, "commit", commit_payload(0));
    let records = records_at(&owner, 0);
    let got = record_segs(&records[0]);
    let posed = &evaluation.strokes[0];
    assert_eq!(got.len(), posed.segments.len());
    for (g, p) in got.iter().zip(posed.segments.iter()) {
        let want = [
            p.point[0],
            p.point[1],
            p.handle_in[0],
            p.handle_in[1],
            p.handle_out[0],
            p.handle_out[1],
        ];
        assert_eq!(g, &want, "bit-identical to the evaluation");
    }
    // The pose really moved the geometry (the oracle row is not at rest).
    assert_ne!(got, oracle::segs(&row.strokes[0]["segs"]));
}

#[test]
fn after_a_reset_the_evaluated_pose_is_the_baseline_live_rest_geometry() {
    let mut checked = 0;
    for row in oracle::rows().iter().filter(|r| {
        !r.rig.is_null()
            && !r.has_meshes()
            && r.expect["reset"]["throws"] == json!(false)
            && r.expect["kept"]
                .as_array()
                .unwrap()
                .iter()
                .all(|k| k == &json!(true))
            && !r.expect["kept"].as_array().unwrap().is_empty()
    }) {
        let mut owner = owner_for(row, PLAIN);
        run_ok(&mut owner, row.name, reset_payload());
        let snapshot = current(&owner);
        let evaluation =
            evaluate_rig_pose(&snapshot, &RigPosePin::of(&snapshot), LAYER, 0, None).unwrap();
        assert!(!evaluation.strokes.is_empty(), "{}", row.name);
        let live = row.expect["reset"]["live"].as_object().unwrap();
        for posed in &evaluation.strokes {
            let want = oracle::segs(&live[&posed.stroke_id]);
            assert_eq!(want.len(), posed.segments.len(), "{}", row.name);
            for (p, w) in posed.segments.iter().zip(want.iter()) {
                let got = [
                    p.point[0],
                    p.point[1],
                    p.handle_in[0],
                    p.handle_in[1],
                    p.handle_out[0],
                    p.handle_out[1],
                ];
                for k in 0..6 {
                    assert!(
                        oracle::close(got[k], w[k]),
                        "{} {}: {got:?} vs {w:?}",
                        row.name,
                        posed.stroke_id
                    );
                }
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 11);
}

#[test]
fn a_commit_that_changes_nothing_keeps_presence_bytes_history_and_redo() {
    let row = oracle::row("rest_pose_bound_stroke_commit_replaces_with_equal_values");
    let mut owner = owner_for(&row, HELD);
    run_ok(&mut owner, "promote", commit_payload(HELD_FRAME));
    let committed = bytes(&owner);
    owner.undo(owner.content_revision()).unwrap();
    let before = state(&owner);
    assert_eq!(before.2, (0, 1));
    let reply = run_ok(&mut owner, "no-op", commit_payload(0));
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(reply["result"]["historyEntriesAdded"], 0);
    assert_eq!(
        state(&owner),
        before,
        "a no-op keeps bytes, revision, history and redo"
    );
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), committed);
}

#[test]
fn a_deform_failure_after_a_successful_promotion_publishes_nothing() {
    let row = oracle::row("offset_entry_naming_a_missing_bone_throws");
    // The baseline promoted, threw inside applyRigDeform and never saved: the pose was lost midway.
    assert_eq!(row.commit_ret(), &json!("throws"));
    let mut owner = owner_for(&row, HELD);
    assert_refused(
        &mut owner,
        "throws",
        commit_payload(HELD_FRAME),
        ("invalid_request", "deform_failed"),
    );
    assert_eq!(
        wire(&owner)["layers"][0]["timing"]["keys"],
        json!([{"frame": 0}])
    );
    assert!(
        records_at(&owner, HELD_FRAME).is_empty(),
        "no half-promoted key"
    );
}

#[test]
fn mesh_binds_make_the_commit_unavailable_and_publish_nothing() {
    let rows: Vec<oracle::Row> = oracle::rows()
        .into_iter()
        .filter(|r| r.has_meshes())
        .collect();
    assert_eq!(rows.len(), 5);
    for row in &rows {
        // The baseline committed these (it has the store and the raster); no native provider is wired.
        assert_eq!(row.commit_ret(), &json!(true), "{}", row.name);
        for build in [HELD, MESHED_HELD] {
            let mut owner = owner_for(row, build);
            assert_refused(
                &mut owner,
                row.name,
                commit_payload(HELD_FRAME),
                ("unavailable", "pose_unavailable"),
            );
        }
    }
}

#[test]
fn binds_that_all_fail_to_relink_are_the_typed_refusal_and_not_a_bare_promotion() {
    let row = oracle::row("stored_frame_has_another_vertex_count_so_the_bind_is_unlinked");
    assert_eq!(
        row.commit_ret(),
        &json!(true),
        "the baseline reported success"
    );
    let mut owner = owner_for(&row, HELD);
    assert_refused(
        &mut owner,
        "unlinked",
        commit_payload(HELD_FRAME),
        ("not_found", "no_rigged_stroke"),
    );
    assert_eq!(
        wire(&owner)["layers"][0]["timing"]["keys"],
        json!([{"frame": 0}])
    );
}

#[test]
fn an_unlinked_bind_beside_a_posed_one_is_counted_and_kept_in_the_record() {
    let row = oracle::row("rebind_replaces_and_second_stroke_added");
    assert_eq!(row.strokes.len(), 2);
    // Drop the second stroke from the document: its bind no longer links, the first still poses.
    let layer = layer_json(LAYER, &row.rig, Some(&[0]), false);
    let first = row_records(&row).into_iter().take(1).collect();
    let mut owner = owner_of(&document_json(vec![layer], first));
    let rig_before = rig_text(&owner);
    let body = serde_json::to_vec(&commit_payload(HELD_FRAME)).unwrap();
    let prepared = owner.prepare_rig_pose_json(&body).unwrap();
    assert_eq!(prepared.report().posed_strokes, 1);
    assert_eq!(prepared.report().unlinked_binds, 1);
    assert!(prepared.report().promoted);
    owner.commit_rig_pose(prepared).unwrap();
    assert_eq!(
        rig_text(&owner),
        rig_before,
        "the unlinked bind is neither dropped nor rewritten"
    );
    let saved = row.expect["commit"]["saved"].as_object().unwrap();
    let got = record_segs(&records_at(&owner, HELD_FRAME)[0]);
    for (g, w) in got.iter().zip(oracle::segs(&saved["s1"]).iter()) {
        for k in 0..6 {
            assert!(oracle::close(g[k], w[k]));
        }
    }
}

#[test]
fn a_layer_without_timing_is_its_own_key_so_nothing_is_promoted() {
    let row = oracle::row("arm_rotate_soft_bone_turned_40_degrees");
    let mut owner = owner_for(&row, PLAIN);
    let body = serde_json::to_vec(&commit_payload(0)).unwrap();
    let prepared = owner.prepare_rig_pose_json(&body).unwrap();
    assert!(!prepared.report().promoted);
    owner.commit_rig_pose(prepared).unwrap();
    assert_saved_like_the_baseline(&row, &owner, 0);
    assert!(wire(&owner)["layers"][0].get("timing").is_none());
    // No record exists at frame 3 and no timing says one is shown: the bind has nothing to pose.
    assert_refused(
        &mut owner,
        "empty-frame",
        commit_payload(HELD_FRAME),
        ("not_found", "no_rigged_stroke"),
    );
}

#[test]
fn the_same_commit_twice_gives_bit_identical_documents() {
    let row = oracle::row("legacy_offset_weights_mixed_with_control_points");
    let run_once = || {
        let mut owner = owner_for(&row, HELD);
        run_ok(&mut owner, "once", commit_payload(HELD_FRAME));
        bytes(&owner)
    };
    assert_eq!(run_once(), run_once());
}
