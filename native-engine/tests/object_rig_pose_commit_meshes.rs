//! The mesh half of reset pose (L03-Q, I13): the offsets `rigResetPose`'s own `applyRigDeform` leaves in the
//! image-mesh store (L03-J), against the executed baseline rows with meshes. A commit that meets a mesh bind
//! is the typed `pose_unavailable` (see the kernels file); a reset needs only the store, because it writes zeros.
use super::object_rig_pose_commit_oracle as oracle;
use super::object_rig_pose_commit_rows::*;
use super::*;
use serde_json::Value;

const MESHED: Build<'static> = Build {
    keys: None,
    locked: false,
    meshes: true,
};

fn offsets_of(owner: &NativeObjectHistory, id: &str) -> Vec<[f64; 2]> {
    wire(owner)["imageMeshes"][id]["offsets"]
        .as_array()
        .expect("offsets are authored in these documents")
        .iter()
        .map(|pair| [oracle::num(&pair[0]), oracle::num(&pair[1])])
        .collect()
}

fn expected_offsets(row: &oracle::Row, id: &str) -> Vec<[f64; 2]> {
    row.expect["reset"]["meshOffsets"][id]
        .as_array()
        .expect("baseline offsets")
        .iter()
        .map(|pair| [oracle::num(&pair[0]), oracle::num(&pair[1])])
        .collect()
}

fn pre_offsets(row: &oracle::Row, id: &str) -> Vec<[f64; 2]> {
    row.meshes[id]["offsets"]
        .as_array()
        .expect("stored offsets")
        .iter()
        .map(|pair| [oracle::num(&pair[0]), oracle::num(&pair[1])])
        .collect()
}

/// `(meshes_zeroed, mesh_binds_unlinked)` of the prepared reset.
fn report_of(owner: &NativeObjectHistory) -> (u32, u32) {
    let bytes = serde_json::to_vec(&reset_payload()).unwrap();
    let prepared = owner.prepare_rig_pose_json(&bytes).unwrap();
    (
        prepared.report().meshes_zeroed,
        prepared.report().mesh_binds_unlinked,
    )
}

#[test]
fn reset_leaves_in_the_store_exactly_the_offsets_the_baseline_reset_wrote() {
    let rows: Vec<oracle::Row> = oracle::rows()
        .into_iter()
        .filter(|r| r.has_meshes())
        .collect();
    assert_eq!(rows.len(), 5);
    let mut sculpts_zeroed = 0;
    for row in &rows {
        let mut owner = owner_for(row, MESHED);
        let initial = state(&owner);
        let want = expected_offsets(row, "im_1");
        let mesh_changes = want != pre_offsets(row, "im_1");
        let changes = rig_text(&owner) != row.reset_after() || mesh_changes;
        let reply = run_ok(&mut owner, row.name, reset_payload());
        let got = offsets_of(&owner, "im_1");
        assert_eq!(got.len(), want.len(), "{}", row.name);
        for (g, w) in got.iter().zip(want.iter()) {
            assert!(
                oracle::close(g[0], w[0]) && oracle::close(g[1], w[1]),
                "{}: {g:?} {w:?}",
                row.name
            );
        }
        assert_eq!(reply["result"]["applied"], changes, "{}", row.name);
        assert_eq!(rig_text(&owner), row.reset_after(), "{}", row.name);
        if mesh_changes {
            sculpts_zeroed += 1;
            assert!(
                got.iter().all(|o| o == &[0.0, 0.0]),
                "{}: exact zeros",
                row.name
            );
        }
        if changes {
            let edited = state(&owner);
            assert_eq!(
                edited.2,
                (1, 0),
                "{}: store and rig are ONE entry",
                row.name
            );
            owner.undo(owner.content_revision()).unwrap();
            assert_eq!(bytes(&owner), initial.0, "{}: undo restores both", row.name);
            owner.redo(owner.content_revision()).unwrap();
            assert_eq!(bytes(&owner), edited.0, "{}", row.name);
        }
    }
    // The two rows that start with a stored sculpt: the plain rect and the rest pose.
    assert_eq!(sculpts_zeroed, 2);
}

#[test]
fn a_mesh_that_already_has_zero_offsets_keeps_its_authored_bytes() {
    let row = oracle::row("mesh_rotated_rect_no_rotate_mode");
    assert!(pre_offsets(&row, "im_1").iter().all(|o| o == &[0.0, 0.0]));
    let mut owner = owner_for(&row, MESHED);
    let store_before = wire(&owner)["imageMeshes"].clone();
    run_ok(&mut owner, "zero", reset_payload());
    assert_eq!(
        wire(&owner)["imageMeshes"],
        store_before,
        "only the rig changed"
    );
    assert_eq!(report_of(&owner), (0, 0));
    // Absent offsets read as zeros (`load()`): they stay absent.
    let mut doc = doc_for(&row, MESHED);
    drop_member(&mut doc["imageMeshes"]["im_1"], "offsets");
    let mut owner = owner_of(&doc);
    run_ok(&mut owner, "absent", reset_payload());
    assert!(wire(&owner)["imageMeshes"]["im_1"].get("offsets").is_none());
}

#[test]
fn mesh_binds_the_store_cannot_reach_are_counted_and_never_dropped_or_rewritten() {
    // Plain rect row: with the store the sculpt is zeroed (one mesh); without the store the bind is counted.
    let plain = oracle::row("mesh_plain_rect_stored_offsets_overwritten");
    assert_eq!(report_of(&owner_for(&plain, MESHED)), (1, 0));
    let storeless = owner_for(&plain, PLAIN);
    assert_eq!(report_of(&storeless), (0, 1));
    // Another vertex count than the bind was made against: the baseline relink dropped it.
    let rebuilt = oracle::row("mesh_rebuilt_at_another_density_is_dropped");
    assert_eq!(report_of(&owner_for(&rebuilt, MESHED)), (0, 1));
    // The unreachable bind stays in the record, byte for byte.
    let mut owner = owner_for(&rebuilt, MESHED);
    run_ok(&mut owner, "keep", reset_payload());
    let rig = rig_value(&owner);
    let before: Value = rebuilt.rig.clone();
    assert_eq!(rig["binds"], before["binds"]);
    assert_eq!(rig["binds"].as_array().unwrap().len(), 1);
    assert_eq!(rig["binds"][0]["meshId"], "im_1");
}
