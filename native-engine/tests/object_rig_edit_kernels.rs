//! Behavior of the rig edit transaction that reaches the evaluation kernels (L03-O, I12): `bindStroke`,
//! `autoAssign`, `addWeight`, `resetWeights` and `setIk` against the executed-baseline rows and independent
//! literal fixtures. Compiled only with the `evaluation` feature, exactly like the kernels themselves.
use super::object_rig_edit_oracle as oracle;
use super::object_rig_edit_rows::*;
use super::*;
use serde_json::{json, Value};

/// `max(0, 1 - dist / reach)` of a control point, restated from `rigWeighOnePointByControl`.
fn falloff(point: [f64; 2], control: [f64; 2], reach: f64) -> f64 {
    let dist = ((point[0] - control[0]) * (point[0] - control[0])
        + (point[1] - control[1]) * (point[1] - control[1]))
        .sqrt();
    (1.0 - dist / reach).max(0.0)
}

fn after_of(row: &oracle::ActRow) -> Value {
    let (owner, _) = apply_row(row);
    serde_json::from_str(&rig_text(&owner)).unwrap()
}

/// `[(boneId, ptIndex, w)]` of one vertex of one bind.
fn entries(rig: &Value, bind: usize, vertex: usize) -> Vec<(String, u64, f64)> {
    rig["binds"][bind]["weights"][vertex]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["boneId"].as_str().unwrap().to_owned(),
                entry["ptIndex"].as_u64().unwrap(),
                entry["w"].as_f64().unwrap(),
            )
        })
        .collect()
}

fn prepared(owner: &NativeObjectHistory, row: &oracle::ActRow) -> PreparedRigEdit {
    let body = serde_json::to_vec(&payload(row.action.clone())).unwrap();
    owner.prepare_rig_edit_json(&body).unwrap()
}

#[test]
fn bind_stroke_rows_match_the_executed_baseline_and_are_one_reversible_entry() {
    let rows = oracle::of_kind("bindStroke");
    assert_eq!(rows.len(), 8);
    for row in rows {
        assert_eq!(row.ret, json!(true), "{}", row.name);
        check_row(&row);
        let owner = owner_for(&row);
        let summary = prepared(&owner, &row).summary().clone();
        assert_eq!(summary.bound, 1, "{}", row.name);
        assert_eq!(summary.skipped_unsupported, 0, "{}", row.name);
    }
}

#[test]
fn auto_assign_rows_match_the_executed_baseline_and_report_what_they_did() {
    let rows = oracle::of_kind("autoAssign");
    assert_eq!(rows.len(), 8);
    for row in rows {
        check_row(&row);
        let owner = owner_for(&row);
        let summary = prepared(&owner, &row).summary().clone();
        let baseline_bound = row.ret["n"].as_u64().unwrap_or(0) as u32;
        assert_eq!(summary.bound, baseline_bound, "{}", row.name);
        // Every stroke of these rows is representable; the CompoundPath of the baseline row has no native
        // record (the planner test counts the unsupported family on an in-memory record).
        assert_eq!(summary.skipped_unsupported, 0, "{}", row.name);
        let has_bones = row.before.contains("\"bone1\"");
        let meshes_off = row.action["meshes"] == json!(false);
        assert_eq!(
            summary.mesh_binds_unavailable,
            has_bones && !meshes_off,
            "{}: the mesh gap is reported, never silent",
            row.name
        );
    }
}

#[test]
fn add_and_reset_weight_rows_match_and_write_only_that_vertex() {
    for row in oracle::of_kind("addWeight")
        .into_iter()
        .chain(oracle::of_kind("resetWeights"))
    {
        check_row(&row);
        let before: Value = serde_json::from_str(row.before).unwrap();
        let mut after: Value = serde_json::from_str(row.after).unwrap();
        let vertex = row.action["vertex"].as_u64().unwrap() as usize;
        assert_ne!(
            after["binds"][0]["weights"][vertex], before["binds"][0]["weights"][vertex],
            "{}: the vertex changed",
            row.name
        );
        after["binds"][0]["weights"][vertex] = before["binds"][0]["weights"][vertex].clone();
        assert_eq!(after, before, "{}: nothing else changed", row.name);
    }
    // Reset swaps the control-point list of the vertex for the legacy offset model: no ptIndex survives.
    let reset = after_of(&oracle::act("reset_a_vertex_to_the_offset_model"));
    let list = reset["binds"][0]["weights"][2].as_array().unwrap();
    assert!(!list.is_empty());
    for entry in list {
        assert!(entry.get("offset").is_some() && entry.get("ptIndex").is_none());
    }
    // The add row stores the half weight the popover uses.
    let added = after_of(&oracle::act(
        "add_a_bone_to_a_vertex_stores_the_nearest_offset",
    ));
    let last = added["binds"][0]["weights"][0]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["boneId"], "bone2");
    assert_eq!(last["w"], 0.5);
}

#[test]
fn ik_rows_match_and_a_chain_with_a_missing_anchor_is_refused() {
    let rows = oracle::of_kind("setIk");
    assert_eq!(rows.len(), 6);
    for row in rows {
        if row.ret == json!(true) {
            check_row(&row);
        } else {
            check_refused_row(&row, "invalid_request");
        }
    }
}

#[test]
fn ik_lengths_are_the_assigned_literals_measured_on_the_live_pose() {
    // Rest pose: bone1 runs (0,0)-(100,0) and bone2 (100,0)-(180,60), a 3-4-5 triangle scaled by 20,
    // so both lengths are exactly 100.
    let rest = after_of(&oracle::act("ik_chain_from_the_rest_pose"));
    let chain = &rest["ikChains"]["bone2:1"];
    assert_eq!(chain["l1"], 100);
    assert_eq!(chain["l2"], 100);
    assert_eq!(chain["flip"], false);
    // The live pose wins over the rest pose: the end vertex was posed to (150,80), 94.34 from the joint.
    let posed = after_of(&oracle::act("ik_lengths_follow_the_live_pose"));
    let chain = &posed["ikChains"]["bone2:1"];
    assert_eq!(chain["l1"], 100);
    let l2 = chain["l2"].as_f64().unwrap();
    assert!((l2 - 8900f64.sqrt()).abs() < 1e-9, "{l2}");
    // The chain is filed under the end reference and an existing key is overwritten in place.
    let again = after_of(&oracle::act("ik_chain_replaces_the_one_with_the_same_end"));
    assert_eq!(again["ikChains"].as_object().unwrap().len(), 1);
    assert_eq!(again["ikChains"]["bone2:1"]["flip"], true);
}

#[test]
fn a_bound_stroke_has_the_assigned_literal_shape() {
    // Radius and softness absent: the reach is 200 for a bone without its own radius, 80 for bone2.
    let after = after_of(&oracle::act("bind_radius_and_softness_absent"));
    let bind = &after["binds"][0];
    assert_eq!(after["binds"].as_array().unwrap().len(), 1);
    assert_eq!(bind["strokeId"], "s1");
    assert_eq!(bind["rotate"], false);
    assert_eq!(
        bind["rest"],
        json!([[10, -20], [190, -20], [190, 20], [10, 20]])
    );
    assert_eq!(
        bind["restHandles"],
        json!([
            {"i":null,"o":[10,0]},
            {"i":[-10,0],"o":[0,5]},
            {"i":[0,-5],"o":null},
            {"i":null,"o":null}
        ]),
        "a zero handle is null, absent and null stay apart from a value"
    );
    assert!(bind.get("meshId").is_none());
    let v0 = [10.0, -20.0];
    assert_eq!(
        entries(&after, 0, 0),
        [
            ("bone1".to_owned(), 0, falloff(v0, [0.0, 0.0], 200.0)),
            ("bone1".to_owned(), 1, falloff(v0, [100.0, 0.0], 200.0)),
        ]
    );
    let v1 = [190.0, -20.0];
    assert_eq!(
        entries(&after, 0, 1),
        [
            ("bone1".to_owned(), 0, falloff(v1, [0.0, 0.0], 200.0)),
            ("bone1".to_owned(), 1, falloff(v1, [100.0, 0.0], 200.0)),
            ("bone2".to_owned(), 1, falloff(v1, [200.0, 0.0], 80.0)),
        ],
        "bone.radius || radius: bone2 reaches 80, bone1 the bind radius"
    );
    assert_eq!(
        bind["weights"].as_array().unwrap().len(),
        4,
        "one list per rest point"
    );
}

#[test]
fn a_rebind_replaces_the_old_bind_and_appends_the_new_one() {
    let row = oracle::act("rebind_replaces_and_moves_to_the_end");
    let before: Value = serde_json::from_str(row.before).unwrap();
    let ids = |rig: &Value| -> Vec<String> {
        rig["binds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|bind| bind["strokeId"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(ids(&before), ["s1", "s2"]);
    let after = after_of(&row);
    assert_eq!(ids(&after), ["s2", "s1"], "never two binds for one stroke");
    assert!(entries(&after, 1, 1)
        .iter()
        .all(|(bone, _, _)| bone == "bone1"));
    assert_eq!(after["binds"][1]["rotate"], false);
    assert_eq!(
        after["binds"][0], before["binds"][1],
        "the other stroke is untouched"
    );
}

#[test]
fn auto_assign_grows_the_radius_by_the_baseline_margin() {
    // bone1 has no radius, bone2 has 80 and is left out of the scan. The farthest vertex is (200,460),
    // 470.74 from bone1's nearest point (100,0); ceil(470.74 * 1.05) = 495 leaves it a sliver of pull.
    let after = after_of(&oracle::act("auto_grows_the_radius_to_the_farthest_vertex"));
    let ids: Vec<&str> = after["binds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|bind| bind["strokeId"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["f1", "a1"], "layer order");
    let far = (100.0f64 * 100.0 + 460.0 * 460.0).sqrt();
    assert!(far > 470.0 && far < 471.0);
    assert_eq!(
        entries(&after, 0, 2),
        [("bone1".to_owned(), 1, 1.0 - far / 495.0)],
        "radius 495: the far control point reaches it, the near one (501.6 away) does not"
    );
    assert_eq!(after["binds"][0]["rotate"], true);
    // A bone with its own radius is not measured: bone1 here has 500 and would have grown the reach.
    let own = after_of(&oracle::act(
        "auto_leaves_out_the_bones_with_their_own_radius",
    ));
    let corner = [190.0, 340.0];
    assert_eq!(
        entries(&own, 0, 2),
        [
            ("bone1".to_owned(), 0, falloff(corner, [0.0, 0.0], 500.0)),
            ("bone1".to_owned(), 1, falloff(corner, [100.0, 0.0], 500.0)),
            ("bone2".to_owned(), 0, falloff(corner, [100.0, 0.0], 357.0)),
            ("bone2".to_owned(), 1, falloff(corner, [200.0, 0.0], 357.0)),
        ],
        "ceil(340 * 1.05) = 357 for the bone without a radius, 500 for the bone that has one"
    );
}

#[test]
fn auto_assign_rebinds_what_was_bound_and_keeps_the_rest() {
    let row = oracle::act("auto_rebinds_what_was_bound_and_keeps_the_rest");
    let before: Value = serde_json::from_str(row.before).unwrap();
    let after = after_of(&row);
    let ids = |rig: &Value| -> Vec<String> {
        rig["binds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|bind| bind["strokeId"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(ids(&before), ["s0", "a1"]);
    assert_eq!(ids(&after), ["s0", "a1", "a2"]);
    assert_eq!(
        after["binds"][0], before["binds"][0],
        "s0 is not on the frame: untouched"
    );
    assert_ne!(after["binds"][1], before["binds"][1], "a1 was bound again");
}

#[test]
fn unknown_bones_strokes_frames_and_missing_rigs_are_refused_and_publish_nothing() {
    let base = oracle::act("bind_rotate_and_softness");
    let no_rig = oracle::act("draw_first_bone_creates_the_rig");
    let bind = |change: &dyn Fn(&mut Value)| {
        let mut action = base.action.clone();
        change(&mut action);
        action
    };
    let cases: Vec<(&str, &oracle::ActRow, Value, &str)> = vec![
        (
            "unknown bone",
            &base,
            bind(&|a| a["boneIds"] = json!(["bone9"])),
            "invalid_request",
        ),
        (
            "duplicate bone",
            &base,
            bind(&|a| a["boneIds"] = json!(["bone1", "bone1"])),
            "invalid_request",
        ),
        (
            "unknown stroke",
            &base,
            bind(&|a| a["strokeId"] = json!("nope")),
            "not_found",
        ),
        (
            "stroke on another frame",
            &base,
            bind(&|a| a["frame"] = json!(1)),
            "not_found",
        ),
        (
            "negative radius",
            &base,
            bind(&|a| a["radius"] = json!(-1)),
            "invalid_request",
        ),
        ("no rig", &no_rig, base.action.clone(), "not_found"),
        (
            "auto-assign without a rig",
            &no_rig,
            json!({"kind":"autoAssign","frame":0}),
            "not_found",
        ),
        (
            "reset of a vertex past the bind",
            &base,
            json!({"kind":"resetWeights","target":{"strokeId":"s1"},"vertex":9}),
            "invalid_request",
        ),
        (
            "add of an unknown bone",
            &base,
            json!({"kind":"addWeight","target":{"strokeId":"s1"},"vertex":0,"boneId":"bone9"}),
            "not_found",
        ),
    ];
    for (name, row, action, want) in cases {
        let mut owner = owner_for(row);
        let initial = state(&owner);
        let reply = run(&mut owner, name, payload(action));
        assert_eq!(reply["ok"], false, "{name}");
        assert_eq!(code(&reply), want, "{name}");
        assert_eq!(state(&owner), initial, "{name}");
    }
}

#[test]
fn a_bone_the_vertex_already_has_is_not_added_twice() {
    let base = oracle::act("add_a_bone_to_a_vertex_stores_the_nearest_offset");
    let mut owner = owner_for(&base);
    let initial = state(&owner);
    // Vertex 0 is already weighed against bone1.
    let reply = run(
        &mut owner,
        "twice",
        payload(json!({"kind":"addWeight","target":{"strokeId":"s1"},"vertex":0,"boneId":"bone1"})),
    );
    assert_eq!(code(&reply), "invalid_request");
    assert_eq!(state(&owner), initial);
}

#[test]
fn an_auto_assign_that_finds_nothing_to_bind_is_a_no_op() {
    let base = oracle::act("auto_two_strokes_default_radius");
    let mut owner = owner_for(&base);
    let initial = state(&owner);
    let reply = run_ok(
        &mut owner,
        "other-frame",
        payload(json!({"kind":"autoAssign","frame":2})),
    );
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(
        state(&owner),
        initial,
        "no stroke on frame 2: nothing changes"
    );
}

#[test]
fn a_full_rig_session_is_undone_and_redone_step_by_step() {
    // Draw two bones, bind a stroke, edit one weight, pose a bone, file an IK chain, auto-assign: every
    // step is its own history entry, undo walks back through the exact snapshots and redo forward again.
    let row = oracle::act("bind_rotate_and_softness");
    let records: Vec<Value> = row.strokes.as_array().unwrap().iter().map(record).collect();
    let mut owner = owner_of_text(&doc_text("null", &records));
    let start = state(&owner);
    let bone = |from: i64, to: i64| {
        json!({"kind":"drawBone","closed":false,"segments":[
            {"point":[from,0],"handleIn":[0,0],"handleOut":[0,0]},
            {"point":[to,0],"handleIn":[0,0],"handleOut":[0,0]}]})
    };
    let steps = vec![
        bone(0, 100),
        bone(100, 200),
        json!({"kind":"setBoneRadius","boneId":"bone2","radius":80}),
        row.action.clone(),
        json!({"kind":"setWeight","target":{"strokeId":"s1"},"vertex":1,"entry":0,"percent":40}),
        json!({"kind":"moveSegment","boneId":"bone2","vi":1,"part":"point","to":[150,80]}),
        json!({"kind":"setIk","root":{"boneId":"bone1","vi":0},"joint":{"boneId":"bone1","vi":1},
            "end":{"boneId":"bone2","vi":1},"flip":true}),
        json!({"kind":"autoAssign","frame":0,"rotate":false}),
    ];
    let mut snapshots = vec![state(&owner)];
    for (index, action) in steps.iter().enumerate() {
        let reply = run_ok(&mut owner, &format!("step{index}"), payload(action.clone()));
        assert_eq!(reply["result"]["applied"], true, "step {index}");
        assert_eq!(owner.history_depths(), (index + 1, 0), "step {index}");
        snapshots.push(state(&owner));
        if index == 3 {
            assert_eq!(
                rig_text(&owner),
                row.after,
                "drawn bones plus the bind are the baseline rig"
            );
        }
    }
    let rest_geometry = objects_of(&owner);
    assert_eq!(
        rest_geometry,
        objects_of(&owner_of_text(&doc_text("null", &records))),
        "binding and posing never touch the rest geometry"
    );
    for index in (0..steps.len()).rev() {
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(
            bytes(&owner),
            snapshots[index].0,
            "undo to before step {index}"
        );
    }
    assert_eq!(state(&owner).0, start.0);
    assert_eq!(rig_text(&owner), "null");
    for (index, snapshot) in snapshots.iter().enumerate().skip(1) {
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(bytes(&owner), snapshot.0, "redo of step {}", index - 1);
    }
}
