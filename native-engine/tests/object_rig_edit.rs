//! Behavior of the rig edit transaction that needs no kernel (L03-O, I12): bones, poses, radii, weight
//! entries, removals, history and save/reload, against the executed-baseline rows and independent literal
//! fixtures. Binds, auto-assign, reset/add weight and IK need the evaluation kernels and live in
//! `object_rig_edit_kernels.rs`.
use super::object_rig_edit_oracle as oracle;
use super::object_rig_edit_rows::*;
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

const SEGMENTS_0_100: &str = r#"[{"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]}]"#;
const SEGMENTS_100_200: &str = r#"[{"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[200,0],"handleIn":[0,0],"handleOut":[0,0]}]"#;

fn draw(segments: Value, bone_id: Option<&str>) -> Value {
    let mut action = json!({"kind":"drawBone","segments":segments,"closed":false});
    if let Some(id) = bone_id {
        action["boneId"] = json!(id);
    }
    action
}

fn flat_segments(from: i64, to: i64) -> Value {
    json!([
        {"point":[from,0],"handleIn":[0,0],"handleOut":[0,0]},
        {"point":[to,0],"handleIn":[0,0],"handleOut":[0,0]}
    ])
}

#[test]
fn oracle_row_counts_are_pinned() {
    let rows = oracle::act_rows();
    assert_eq!(rows.len(), 44);
    for (kind, count) in [
        ("drawBone", 6),
        ("moveSegment", 5),
        ("setBoneRadius", 3),
        ("bindStroke", 8),
        ("autoAssign", 8),
        ("setWeight", 4),
        ("removeWeight", 1),
        ("addWeight", 1),
        ("resetWeights", 2),
        ("setIk", 6),
    ] {
        assert_eq!(oracle::of_kind(kind).len(), count, "{kind}");
    }
    let mut names: Vec<&str> = rows.iter().map(|row| row.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 44, "row names are unique");
}

#[test]
fn bone_pose_radius_and_weight_rows_match_the_executed_baseline_and_are_one_reversible_entry() {
    let mut checked = 0;
    for kind in [
        "drawBone",
        "moveSegment",
        "setBoneRadius",
        "setWeight",
        "removeWeight",
    ] {
        for row in oracle::of_kind(kind) {
            check_row(&row);
            checked += 1;
        }
    }
    assert_eq!(checked, 19);
}

#[test]
fn a_drawn_bone_is_the_baseline_id_and_the_counter_is_the_baseline_counter() {
    for row in oracle::of_kind("drawBone") {
        let (owner, _) = apply_row(&row);
        let after: Value = serde_json::from_str(&rig_text(&owner)).unwrap();
        let expected_id = match row.action["boneId"].as_str() {
            Some(id) => id.to_owned(),
            None => row
                .ret
                .as_str()
                .expect("the baseline returned the id")
                .to_owned(),
        };
        assert!(after["bones"][&expected_id].is_object(), "{}", row.name);
        assert_eq!(
            after["bones"][&expected_id]["segments"], after["bones"][&expected_id]["restSegments"],
            "{}: restSegments is a copy of segments",
            row.name
        );
    }
}

#[test]
fn drawn_bones_have_the_assigned_literal_shape() {
    let mut owner = owner_of_text(&doc_text("null", &[]));
    let initial = state(&owner);
    run_ok(&mut owner, "d1", payload(draw(flat_segments(0, 100), None)));
    let one = format!(
        r#"{{"bones":{{"bone1":{{"segments":{SEGMENTS_0_100},"restSegments":{SEGMENTS_0_100},"closed":false}}}},"ikChains":{{}},"nextId":2,"binds":[]}}"#
    );
    assert_eq!(rig_text(&owner), one);
    run_ok(
        &mut owner,
        "d2",
        payload(draw(flat_segments(100, 200), None)),
    );
    let two = format!(
        r#"{{"bones":{{"bone1":{{"segments":{SEGMENTS_0_100},"restSegments":{SEGMENTS_0_100},"closed":false}},"bone2":{{"segments":{SEGMENTS_100_200},"restSegments":{SEGMENTS_100_200},"closed":false}}}},"ikChains":{{}},"nextId":3,"binds":[]}}"#
    );
    assert_eq!(rig_text(&owner), two);
    // Redrawing bone1 keeps its table position and the counter.
    let longer = json!([
        {"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},
        {"point":[50,5],"handleIn":[0,0],"handleOut":[0,0]},
        {"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]}
    ]);
    run_ok(&mut owner, "d3", payload(draw(longer, Some("bone1"))));
    let after: Value = serde_json::from_str(&rig_text(&owner)).unwrap();
    let ids: Vec<&String> = after["bones"].as_object().unwrap().keys().collect();
    assert_eq!(ids, ["bone1", "bone2"]);
    assert_eq!(
        after["bones"]["bone1"]["segments"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        after["bones"]["bone1"]["restSegments"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(after["nextId"], 3);
    // Undo walks back to the layer that never had a rig: the member is absent again.
    for _ in 0..3 {
        owner.undo(owner.content_revision()).unwrap();
    }
    assert_eq!(state(&owner).0, initial.0);
    assert_eq!(rig_text(&owner), "null");
    assert!(!String::from_utf8(bytes(&owner))
        .unwrap()
        .contains("\"rig\""));
}

#[test]
fn the_id_counter_comes_back_with_undo_and_never_repeats_while_the_bone_exists() {
    let mut owner = owner_of_text(&doc_text("null", &[]));
    run_ok(&mut owner, "d1", payload(draw(flat_segments(0, 100), None)));
    run_ok(
        &mut owner,
        "d2",
        payload(draw(flat_segments(100, 200), None)),
    );
    owner.undo(owner.content_revision()).unwrap();
    run_ok(
        &mut owner,
        "d3",
        payload(draw(flat_segments(100, 150), None)),
    );
    let after: Value = serde_json::from_str(&rig_text(&owner)).unwrap();
    assert_eq!(after["nextId"], 3);
    assert_eq!(
        after["bones"]["bone2"]["segments"][1]["point"],
        json!([150, 0])
    );
}

#[test]
fn a_pose_move_writes_the_live_pose_and_never_the_rest_pose() {
    let row = oracle::act("move_a_handle_out_mirrors_the_handle_in");
    let (owner, _) = apply_row(&row);
    let after: Value = serde_json::from_str(&rig_text(&owner)).unwrap();
    let before: Value = serde_json::from_str(row.before).unwrap();
    // Independent derivation: the anchor is (100,0), the pointer (130,10), so the dragged handle is the
    // vector (30,10) and the mirrored one (-30,-10).
    let live = &after["bones"]["bone1"]["segments"][1];
    assert_eq!(live["point"], json!([100, 0]));
    assert_eq!(live["handleOut"], json!([30, 10]));
    assert_eq!(live["handleIn"], json!([-30, -10]));
    assert_eq!(
        after["bones"]["bone1"]["restSegments"], before["bones"]["bone1"]["restSegments"],
        "the rest pose is the bind pose and never moves"
    );
    // A broken tangent leaves the other handle alone.
    let broken = oracle::act("move_a_handle_in_with_the_tangent_broken");
    let (owner, _) = apply_row(&broken);
    let after: Value = serde_json::from_str(&rig_text(&owner)).unwrap();
    let live = &after["bones"]["bone1"]["segments"][2];
    assert_eq!(live["handleIn"], json!([-15, -10]), "(165,30) - (180,40)");
    assert_eq!(live["handleOut"], json!([0, 0]), "untouched");
}

#[test]
fn no_op_edits_keep_presence_bytes_history_and_redo() {
    let row = oracle::act("move_an_anchor");
    let mut owner = owner_for(&row);
    run_ok(&mut owner, "edit", payload(row.action.clone()));
    owner.undo(owner.content_revision()).unwrap();
    let kept = state(&owner);
    assert_eq!(kept.2, (0, 1));
    let noop = json!({"kind":"moveSegment","boneId":"bone1","vi":0,"part":"point","to":[0,0]});
    let reply = run_ok(&mut owner, "noop", payload(noop));
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(reply["result"]["historyEntriesAdded"], 0);
    assert_eq!(state(&owner), kept, "the redo entry survives a no-op");
    // A real edit after the undo clears the redo stack.
    run_ok(
        &mut owner,
        "real",
        payload(json!({"kind":"moveSegment","boneId":"bone1","vi":0,"part":"point","to":[1,1]})),
    );
    assert_eq!(owner.history_depths(), (1, 0));
    // The radius and the weight input repeat as no-ops too.
    for (name, repeat) in [("set_radius_50", 1), ("set_weight_40", 1)] {
        let row = oracle::act(name);
        let mut owner = owner_for(&row);
        run_ok(&mut owner, "first", payload(row.action.clone()));
        let applied = state(&owner);
        for round in 0..repeat {
            let again = run_ok(
                &mut owner,
                &format!("again{round}"),
                payload(row.action.clone()),
            );
            assert_eq!(again["result"]["applied"], false, "{name}");
        }
        assert_eq!(state(&owner), applied, "{name}");
    }
}

#[test]
fn unknown_targets_and_out_of_range_indices_are_refused_and_publish_nothing() {
    let bones = oracle::act("move_an_anchor");
    let weights = oracle::act("set_weight_40");
    let no_rig = oracle::act("draw_first_bone_creates_the_rig");
    let move_body = |bone: &str, vi: i64| json!({"kind":"moveSegment","boneId":bone,"vi":vi,"part":"point","to":[1,2]});
    let weight_body = |stroke: &str, vertex: i64, entry: i64| json!({"kind":"setWeight","target":{"strokeId":stroke},"vertex":vertex,"entry":entry,"percent":10});
    let cases: Vec<(&str, &oracle::ActRow, Value, &str)> = vec![
        ("unknown bone", &bones, move_body("bone9", 0), "not_found"),
        (
            "vertex beyond the bone",
            &bones,
            move_body("bone1", 9),
            "invalid_request",
        ),
        (
            "radius of an unknown bone",
            &bones,
            json!({"kind":"setBoneRadius","boneId":"bone9","radius":5}),
            "not_found",
        ),
        (
            "redraw of an unknown bone",
            &bones,
            draw(flat_segments(0, 5), Some("bone9")),
            "not_found",
        ),
        ("no rig at all", &no_rig, move_body("bone1", 0), "not_found"),
        (
            "unknown bind",
            &weights,
            weight_body("nope", 0, 0),
            "not_found",
        ),
        (
            "vertex beyond the bind",
            &weights,
            weight_body("s1", 9, 0),
            "invalid_request",
        ),
        (
            "entry beyond the list",
            &weights,
            weight_body("s1", 0, 9),
            "invalid_request",
        ),
    ];
    for (name, row, action, want) in cases {
        let mut owner = owner_for(row);
        let initial = state(&owner);
        // The case name is a label; the request id must be an identifier (no spaces).
        let reply = run(&mut owner, &name.replace(' ', "-"), payload(action));
        assert_eq!(reply["ok"], false, "{name}");
        assert_eq!(code(&reply), want, "{name}");
        assert_eq!(state(&owner), initial, "{name}");
    }
    let mut owner = owner_for(&bones);
    let initial = state(&owner);
    let reply = run(
        &mut owner,
        "layer",
        with(payload(move_body("bone1", 0)), |p| {
            p["layerUid"] = json!("L9")
        }),
    );
    assert_eq!(code(&reply), "not_found");
    assert_eq!(state(&owner), initial);
}

#[test]
fn removals_are_reference_checked_and_reversible() {
    // A rig with one IK chain naming both bones (the oracle row after `rigSetIK`).
    let chain = oracle::act("ik_chain_replaces_the_one_with_the_same_end");
    let mut owner = owner_of_text(&doc_text(chain.after, &[]));
    let start = state(&owner);
    for bone in ["bone1", "bone2"] {
        let reply = run(
            &mut owner,
            bone,
            payload(json!({"kind":"removeBone","boneId":bone})),
        );
        assert_eq!(
            code(&reply),
            "invalid_request",
            "{bone} is named by the chain"
        );
        assert_eq!(state(&owner), start);
    }
    let unknown = run(
        &mut owner,
        "ik9",
        payload(json!({"kind":"removeIk","endKey":"bone9:1"})),
    );
    assert_eq!(code(&unknown), "not_found");
    let gone = run(
        &mut owner,
        "nobone",
        payload(json!({"kind":"removeBone","boneId":"bone9"})),
    );
    assert_eq!(code(&gone), "not_found");
    run_ok(
        &mut owner,
        "ik",
        payload(json!({"kind":"removeIk","endKey":"bone2:1"})),
    );
    let without_chain: Value = serde_json::from_str(&rig_text(&owner)).unwrap();
    assert_eq!(without_chain["ikChains"], json!({}));
    assert_eq!(owner.history_depths(), (1, 0));
    run_ok(
        &mut owner,
        "bone",
        payload(json!({"kind":"removeBone","boneId":"bone2"})),
    );
    let one_bone: Value = serde_json::from_str(&rig_text(&owner)).unwrap();
    let ids: Vec<&String> = one_bone["bones"].as_object().unwrap().keys().collect();
    assert_eq!(ids, ["bone1"]);
    assert_eq!(one_bone["nextId"], 3, "the counter never goes back");
    owner.undo(owner.content_revision()).unwrap();
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        state(&owner).0,
        start.0,
        "two undos restore the exact bytes"
    );

    // A bone named by a weight entry is refused until the bind is gone.
    let bound = oracle::act("set_weight_40");
    let mut owner = owner_for(&bound);
    let start = state(&owner);
    let reply = run(
        &mut owner,
        "weighted",
        payload(json!({"kind":"removeBone","boneId":"bone1"})),
    );
    assert_eq!(code(&reply), "invalid_request");
    assert_eq!(state(&owner), start);
    run_ok(
        &mut owner,
        "bind",
        payload(json!({"kind":"removeBind","target":{"strokeId":"s1"}})),
    );
    let unbound: Value = serde_json::from_str(&rig_text(&owner)).unwrap();
    assert_eq!(unbound["binds"], json!([]));
    let again = run(
        &mut owner,
        "bind2",
        payload(json!({"kind":"removeBind","target":{"strokeId":"s1"}})),
    );
    assert_eq!(code(&again), "not_found");
    run_ok(
        &mut owner,
        "bone",
        payload(json!({"kind":"removeBone","boneId":"bone1"})),
    );
    owner.undo(owner.content_revision()).unwrap();
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(state(&owner).0, start.0);
}

#[test]
fn a_mesh_bind_of_a_mesh_no_raster_carries_is_not_found_and_publishes_nothing() {
    // The refusal used to be a typed `unavailable` for every mesh bind (the raster record did not exist);
    // the executed mesh rows live in `object_rig_edit_meshes.rs`. A document without any raster or mesh
    // store has nothing to bind, and says so with `not_found`, never by guessing a placement.
    let row = oracle::act("move_an_anchor");
    let mut owner = owner_for(&row);
    let initial = state(&owner);
    let reply = run(
        &mut owner,
        "mesh",
        payload(json!({"kind":"bindMesh","meshId":"im_1"})),
    );
    assert_eq!(reply["ok"], false);
    assert_eq!(code(&reply), "not_found");
    assert_eq!(state(&owner), initial);
}

#[test]
fn edits_survive_save_and_reload() {
    for name in [
        "move_a_handle_out_mirrors_the_handle_in",
        "draw_closed_bone",
        "set_radius_with_a_fraction",
        "remove_a_weight_entry",
    ] {
        let row = oracle::act(name);
        let (owner, _) = apply_row(&row);
        let saved = bytes(&owner);
        let reloaded = decode_storage_project(&saved).unwrap();
        assert_eq!(&reloaded, current(&owner).document(), "{name}");
        assert_eq!(encode_storage_project(&reloaded).unwrap(), saved, "{name}");
        let reopened = NativeObjectHistory::new("reload", reloaded).unwrap();
        assert_eq!(rig_text(&reopened), row.after, "{name}");
    }
}

#[test]
fn a_redraw_that_orphans_an_ik_vertex_is_refused_by_the_record_admission_and_publishes_nothing() {
    let three = r#"[{"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[50,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]}]"#;
    let rig = format!(
        r#"{{"bones":{{"bone1":{{"segments":{three},"restSegments":{three},"closed":false}}}},"ikChains":{{"bone1:2":{{"root":{{"boneId":"bone1","vi":0}},"joint":{{"boneId":"bone1","vi":1}},"end":{{"boneId":"bone1","vi":2}},"l1":50,"l2":50,"flip":false}}}},"nextId":2,"binds":[]}}"#
    );
    let mut owner = owner_of_text(&doc_text(&rig, &[]));
    let start = state(&owner);
    let shrink = payload(draw(flat_segments(0, 100), Some("bone1")));
    let reply = run(&mut owner, "shrink", shrink);
    assert_eq!(reply["ok"], false);
    assert_eq!(
        code(&reply),
        "invalid_request",
        "vertex 2 would no longer exist"
    );
    assert_eq!(state(&owner), start);
    // Redrawing it with the same three vertices keeps the chain valid.
    let kept = json!([
        {"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},
        {"point":[40,10],"handleIn":[0,0],"handleOut":[0,0]},
        {"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]}
    ]);
    run_ok(&mut owner, "keep", payload(draw(kept, Some("bone1"))));
    assert_eq!(owner.history_depths(), (1, 0));
}
