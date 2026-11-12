//! Mesh binds of the rig edit transaction (L03-O, #2251): `bindMesh`, the mesh passes of `autoAssign`,
//! replacement, the weight actions on a mesh bind, history and save/reopen, against the executed-baseline
//! rows (`rigBindMesh` / `rigAutoAssignLayer` cut verbatim, see `fixtures/object-rig-edit-mesh/gen.cjs`) and
//! independent literal fixtures. Compiled with the `evaluation` feature, like the kernels it reaches.
use super::object_rig_edit_mesh_rows::*;
use super::object_rig_edit_rows::*;
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

fn rig_json(owner: &NativeObjectHistory) -> Value {
    serde_json::from_str(&rig_text(owner)).unwrap()
}

/// `(bound, skipped_unsupported)` of the prepared edit.
fn summary_of(owner: &NativeObjectHistory, action: &Value) -> (u32, u32) {
    let body = serde_json::to_vec(&payload(action.clone())).unwrap();
    let prepared = owner.prepare_rig_edit_json(&body).unwrap();
    let summary = prepared.summary();
    (summary.bound, summary.skipped_unsupported)
}

/// What the baseline counted: `rigBindMesh` returns `true` for one bind, the pass returns `{n, ...}`.
fn baseline_bound(row: &MeshRow) -> u32 {
    match row.ret.get("n") {
        Some(n) => n.as_u64().unwrap() as u32,
        None => u32::from(row.ret == json!(true)),
    }
}

#[test]
fn mesh_oracle_rows_match_the_executed_baseline_and_are_one_reversible_entry() {
    let rows = mesh_rows();
    assert_eq!(rows.len(), 13);
    let mut changed = 0;
    for row in &rows {
        let mut owner = owner_for_mesh_row(row);
        assert!(
            json_close(
                &rig_json(&owner),
                &serde_json::from_str::<Value>(row.before).unwrap()
            ),
            "{}: the rig before the edit",
            row.name
        );
        let initial = state(&owner);
        let geometry = rest_geometry(&owner);
        assert_eq!(
            summary_of(&owner, &row.action).0,
            baseline_bound(row),
            "{}: binds made",
            row.name
        );
        let reply = run_ok(&mut owner, row.name, payload(row.action.clone()));
        let want: Value = serde_json::from_str(row.after).unwrap();
        assert!(
            json_close(&rig_json(&owner), &want),
            "{}: rig after the edit\n{}\n{}",
            row.name,
            rig_text(&owner),
            row.after
        );
        assert_eq!(reply["result"]["applied"], row.changes(), "{}", row.name);
        assert_eq!(
            rest_geometry(&owner),
            geometry,
            "{}: a rig edit never mutates rest geometry",
            row.name
        );
        if !row.changes() {
            assert_eq!(state(&owner), initial, "{}: a no-op changes nothing", row.name);
            continue;
        }
        changed += 1;
        let edited = state(&owner);
        assert_eq!(edited.2, (1, 0), "{}: one history entry", row.name);
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), initial.0, "{}: undo restores the bytes", row.name);
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), edited.0, "{}: redo reapplies", row.name);
    }
    assert_eq!(changed, 12, "every row but the dangling mesh changes the rig");
}

#[test]
fn a_mesh_bind_has_the_assigned_literal_shape() {
    // One bone (0,0)-(100,0); a 2x2 mesh on a 100x50 raster at the origin; reach 100.
    let rig = r#"{"bones":{"bone1":{"segments":[{"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]}],"restSegments":[{"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]}],"closed":false}},"ikChains":{},"nextId":2,"binds":[]}"#;
    let meshes = json!({"im_1":{"outline":[[0,0],[1,0],[1,1],[0,1]],"verts":[[0,0],[1,0],[1,1],[0,1]],"tris":[0,1,2],
        "offsets":[[0.5,0.5],[0,0],[0,0],[0,0]]}});
    let rect = json!({"x":0,"y":0,"w":100,"h":50});
    let raster = raster_entry(LAYER, "raster:0", 0, raster_item(Some("im_1"), &rect));
    let mut owner = owner_of_text(&mesh_doc_text(rig, &[], &[raster], &meshes));
    let reply = run_ok(
        &mut owner,
        "bind",
        payload(json!({"kind":"bindMesh","meshId":"im_1","radius":100,"rotate":true})),
    );
    assert_eq!(reply["result"]["applied"], true);
    let want = r#"{"meshId":"im_1","rest":[[0,0],[100,0],[100,50],[0,50]],"weights":[[{"boneId":"bone1","ptIndex":0,"w":1}],[{"boneId":"bone1","ptIndex":1,"w":1}],[{"boneId":"bone1","ptIndex":1,"w":0.5}],[{"boneId":"bone1","ptIndex":0,"w":0.5}]],"rotate":true}"#;
    assert!(
        rig_text(&owner).contains(&format!("\"binds\":[{want}]")),
        "the rest pose ignores the stored sculpt offsets: {}",
        rig_text(&owner)
    );
    // Without a radius the stroke bind's `|| 200` applies (the baseline would store empty weights).
    let mut owner = owner_of_text(&mesh_doc_text(
        rig,
        &[],
        &[raster_entry(LAYER, "raster:0", 0, raster_item(Some("im_1"), &rect))],
        &meshes,
    ));
    run_ok(&mut owner, "default", payload(json!({"kind":"bindMesh","meshId":"im_1"})));
    let weights = &rig_json(&owner)["binds"][0]["weights"][3];
    assert_eq!(weights[0]["w"], 0.75, "1 - 50/200 against the default reach");
}

#[test]
fn auto_assign_binds_meshes_before_paths_and_counts_both() {
    let row = mesh_row("auto_mesh_and_path_meshes_first");
    let mut owner = owner_for_mesh_row(&row);
    assert_eq!(summary_of(&owner, &row.action), (2, 0));
    run_ok(&mut owner, "auto", payload(row.action.clone()));
    let rig = rig_json(&owner);
    let binds = rig["binds"].as_array().unwrap();
    assert_eq!(binds.len(), 2);
    assert_eq!(binds[0]["meshId"], "im_1", "the mesh pass runs first");
    assert_eq!(binds[1]["strokeId"], "s1");
    // `meshes: false` binds only the path (the mesh still counts in the radius scan, which the oracle row
    // pins); `paths: false` binds only the mesh.
    let off = mesh_row("auto_meshes_off_still_scans_them");
    assert_eq!(summary_of(&owner_for_mesh_row(&off), &off.action).0, 1);
    let only = mesh_row("auto_paths_off_binds_only_meshes");
    let mut owner = owner_for_mesh_row(&only);
    run_ok(&mut owner, "only", payload(only.action.clone()));
    let rig = rig_json(&owner);
    assert_eq!(rig["binds"].as_array().unwrap().len(), 1);
    assert_eq!(rig["binds"][0]["meshId"], "im_1");
}

#[test]
fn a_rebind_of_the_same_mesh_replaces_and_never_stacks() {
    let row = mesh_row("auto_mesh_and_path_meshes_first");
    let mut owner = owner_for_mesh_row(&row);
    run_ok(&mut owner, "auto", payload(row.action.clone()));
    let before = rig_json(&owner);
    let again = json!({"kind":"bindMesh","meshId":"im_1","radius":60,"rotate":false});
    run_ok(&mut owner, "again", payload(again.clone()));
    let after = rig_json(&owner);
    let binds = after["binds"].as_array().unwrap();
    assert_eq!(binds.len(), 2, "replaced, not stacked");
    assert_eq!(binds[0]["strokeId"], "s1", "the old mesh bind was removed and the new one appended");
    assert_eq!(binds[0], before["binds"][1], "the stroke bind is untouched");
    assert_eq!(binds[1]["meshId"], "im_1");
    assert_ne!(binds[1], before["binds"][0]);
    // The identical request again changes nothing: bytes, revision, history and redo stay.
    let settled = state(&owner);
    let reply = run_ok(&mut owner, "same", payload(again));
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(state(&owner), settled);
    // A stacking variant would keep two binds of `im_1`.
    assert_eq!(
        binds.iter().filter(|bind| bind["meshId"] == "im_1").count(),
        1
    );
}

#[test]
fn weight_actions_edit_one_vertex_of_a_mesh_bind_and_each_is_one_entry() {
    let row = mesh_row("bind_mesh_two_bones");
    let mut owner = owner_for_mesh_row(&row);
    run_ok(&mut owner, "bind", payload(row.action.clone()));
    let target = json!({"meshId":"im_1"});
    let steps = [
        (
            "set",
            json!({"kind":"setWeight","target":target,"vertex":0,"entry":0,"percent":40}),
        ),
        (
            "add",
            json!({"kind":"addWeight","target":target,"vertex":1,"boneId":"bone1"}),
        ),
        (
            "remove",
            json!({"kind":"removeWeight","target":target,"vertex":0,"entry":1}),
        ),
        (
            "reset",
            json!({"kind":"resetWeights","target":target,"vertex":4,"radius":150,"softness":0.5}),
        ),
    ];
    let mut vertex_changed = [0usize, 1, 0, 4].into_iter();
    for (id, action) in steps {
        let before = rig_json(&owner);
        let start = state(&owner);
        let geometry = rest_geometry(&owner);
        run_ok(&mut owner, id, payload(action));
        let after = rig_json(&owner);
        let vertex = vertex_changed.next().unwrap();
        assert_eq!(state(&owner).2 .0, start.2 .0 + 1, "{id}: one history entry");
        assert_eq!(rest_geometry(&owner), geometry, "{id}: rest geometry");
        let mut restored = after.clone();
        restored["binds"][0]["weights"][vertex] = before["binds"][0]["weights"][vertex].clone();
        assert_eq!(restored, before, "{id}: only vertex {vertex} was written");
        assert_ne!(
            after["binds"][0]["weights"][vertex], before["binds"][0]["weights"][vertex],
            "{id}"
        );
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), start.0, "{id}: undo restores the exact bytes");
        owner.redo(owner.content_revision()).unwrap();
    }
    let rig = rig_json(&owner);
    let set = &rig["binds"][0]["weights"][0][0];
    assert_eq!(set["w"], 0.4, "the percent input is a fraction of one");
    let reset = rig["binds"][0]["weights"][4].as_array().unwrap();
    assert!(!reset.is_empty());
    assert!(reset
        .iter()
        .all(|entry| entry.get("offset").is_some() && entry.get("ptIndex").is_none()));
}

#[test]
fn a_mesh_rig_survives_save_and_reopen_byte_for_byte() {
    for name in [
        "bind_mesh_soft_rotate",
        "auto_mesh_and_path_meshes_first",
        "rebind_mesh_replaces",
    ] {
        let row = mesh_row(name);
        let mut owner = owner_for_mesh_row(&row);
        run_ok(&mut owner, name, payload(row.action.clone()));
        let saved = bytes(&owner);
        let reloaded = decode_storage_project(&saved).unwrap();
        assert_eq!(&reloaded, current(&owner).document(), "{name}");
        assert_eq!(encode_storage_project(&reloaded).unwrap(), saved, "{name}");
        let reopened = NativeObjectHistory::new("reload", reloaded).unwrap();
        assert_eq!(rig_text(&reopened), rig_text(&owner), "{name}");
        let wire = wire(&reopened);
        assert!(wire["imageMeshes"]["im_1"].is_object(), "{name}: the mesh store is intact");
        assert!(wire["items"]["raster"][0]["item"]["meshId"] == "im_1", "{name}: the raster tag is intact");
    }
}
