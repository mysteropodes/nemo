//! Envelope, sealing and discrimination guards of the rig edit transaction (L03-O): dispatch refusals and
//! replay, sealed candidates, the typed unavailability of the kernel-backed actions without the evaluation
//! feature, and oracle non-vacuity.
use super::object_rig_edit_oracle as oracle;
use super::object_rig_edit_rows::*;
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn draw_action() -> Value {
    json!({"kind":"drawBone","closed":false,"segments":[
        {"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},
        {"point":[100,0],"handleIn":[0,0],"handleOut":[0,0]}]})
}

#[test]
fn every_kind_of_edit_refuses_cancel_stale_foreign_and_replays_without_a_second_execution() {
    let moved = oracle::act("move_an_anchor");
    let weighted = oracle::act("set_weight_40");
    let bound = oracle::act("bind_rotate_and_softness");
    let mut cases: Vec<(&str, Option<&oracle::ActRow>, Value)> = vec![
        ("draw", None, draw_action()),
        ("move", Some(&moved), moved.action.clone()),
        ("weight", Some(&weighted), weighted.action.clone()),
    ];
    if cfg!(feature = "evaluation") {
        cases.push(("bind", Some(&bound), bound.action.clone()));
    }
    for (op, row, action) in cases {
        let make = || match row {
            Some(row) => owner_for(row),
            None => owner_of_text(&doc_text("null", &[])),
        };
        let mut owner = make();
        let original = state(&owner);
        for (id, patch) in [
            ("cancel", json!({"cancelledBeforeDispatch":true})),
            ("stale", json!({"expectedRevision":99})),
            ("foreign-instance", json!({"instanceId":"foreign"})),
            ("foreign-document", json!({"documentId":"foreign"})),
            ("wrong-api", json!({"apiVersion":1})),
        ] {
            let mut req = request(&owner, id, payload(action.clone()));
            for (member, value) in patch.as_object().unwrap() {
                req[member] = value.clone();
            }
            let reply = dispatch(&mut owner, &req);
            let want = match id {
                "cancel" => "cancelled_before_dispatch",
                "stale" => "stale_revision",
                "foreign-instance" => "wrong_instance",
                "foreign-document" => "wrong_document",
                _ => "invalid_request",
            };
            assert_eq!(code(&reply), want, "{op} {id}");
            assert_eq!(state(&owner), original, "{op} {id}");
        }
        let mut req = request(&owner, "missing", payload(action.clone()));
        req.as_object_mut().unwrap().remove("expectedRevision");
        assert_eq!(dispatch(&mut owner, &req)["ok"], false, "{op}");
        assert_eq!(state(&owner), original, "{op}");
        let req = request(&owner, "replay", payload(action.clone()));
        let first = dispatch(&mut owner, &req);
        assert_eq!(first["ok"], true, "{op}: {first}");
        let applied = state(&owner);
        assert_eq!(applied.2, (1, 0), "{op}: one history entry");
        assert_eq!(
            dispatch(&mut owner, &req),
            first,
            "{op}: same body, same receipt"
        );
        assert_eq!(state(&owner), applied, "{op}: the replay executed nothing");
        let altered = with(req.clone(), |r| r["payload"]["extra"] = json!(1));
        assert_eq!(
            dispatch(&mut owner, &altered)["ok"],
            false,
            "{op}: reused id, changed body"
        );
        assert_eq!(state(&owner), applied, "{op}");
        owner.undo(owner.content_revision()).unwrap();
        let undone = state(&owner);
        assert_eq!(dispatch(&mut owner, &req), first, "{op}: replay after undo");
        assert_eq!(state(&owner), undone, "{op}: replay cannot run twice");
    }
}

#[test]
fn a_prepared_edit_is_sealed_to_its_origin_owner_and_revision() {
    let row = oracle::act("move_an_anchor");
    let body = serde_json::to_vec(&payload(row.action.clone())).unwrap();
    let mut source = owner_for(&row);
    let candidate = source.prepare_rig_edit_json(&body).unwrap();
    let mut other = owner_for(&row);
    let other_before = state(&other);
    assert_eq!(
        other.commit_rig_edit(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        decode_storage_project(text_for(&row).as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        foreign.commit_rig_edit(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(state(&other), other_before);
    assert_eq!(foreign.history_depths(), (0, 0));
    let before = source.content_revision();
    assert_eq!(
        source.commit_rig_edit(candidate.clone()).unwrap(),
        before + 1
    );
    let applied = state(&source);
    assert_eq!(
        source.commit_rig_edit(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision,
        "a candidate commits once"
    );
    assert_eq!(state(&source), applied);
    // A candidate prepared before another edit lands is stale, even when it would have changed nothing.
    let noop = serde_json::to_vec(&payload(
        json!({"kind":"moveSegment","boneId":"bone1","vi":0,"part":"point","to":[0,0]}),
    ))
    .unwrap();
    let waiting = source.prepare_rig_edit_json(&noop).unwrap();
    let advance =
        json!({"kind":"moveSegment","boneId":"bone1","vi":1,"part":"point","to":[120,40]});
    run_ok(&mut source, "advance", payload(advance));
    let advanced = state(&source);
    assert_eq!(
        source.commit_rig_edit(waiting).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&source), advanced);
}

#[test]
fn a_no_op_candidate_commits_to_the_same_revision_and_adds_no_history() {
    let row = oracle::act("move_an_anchor_to_where_it_is_changes_nothing");
    let mut owner = owner_for(&row);
    let before = state(&owner);
    let body = serde_json::to_vec(&payload(row.action.clone())).unwrap();
    let candidate = owner.prepare_rig_edit_json(&body).unwrap();
    assert_eq!(owner.commit_rig_edit(candidate).unwrap(), before.1);
    assert_eq!(state(&owner), before);
}

#[cfg(not(feature = "evaluation"))]
#[test]
fn without_the_evaluation_kernels_bind_assign_reset_add_and_ik_are_a_typed_unavailable() {
    for name in [
        "bind_rotate_and_softness",
        "auto_two_strokes_default_radius",
        "add_a_bone_to_a_vertex_stores_the_nearest_offset",
        "reset_a_vertex_to_the_offset_model",
        "ik_chain_from_the_rest_pose",
    ] {
        let row = oracle::act(name);
        let mut owner = owner_for(&row);
        let before = state(&owner);
        let reply = run(&mut owner, name, payload(row.action.clone()));
        assert_eq!(reply["ok"], false, "{name}");
        assert_eq!(code(&reply), "unavailable", "{name}");
        assert_eq!(
            state(&owner),
            before,
            "{name}: nothing is guessed or approximated"
        );
    }
}

#[test]
fn the_oracle_discriminates_the_flaws_the_rows_exist_for() {
    let rows = oracle::act_rows();
    let binds: Vec<&oracle::ActRow> = rows.iter().filter(|r| r.kind() == "bindStroke").collect();
    let autos: Vec<&oracle::ActRow> = rows.iter().filter(|r| r.kind() == "autoAssign").collect();
    // Rotate, softness and radius are each exercised with and without a value.
    for (member, name) in [
        ("rotate", "rotate"),
        ("softness", "softness"),
        ("radius", "radius"),
    ] {
        let with_it = binds
            .iter()
            .filter(|r| r.action.get(member).is_some())
            .count();
        assert!(
            with_it >= 3 && with_it < binds.len(),
            "bind rows must vary {name}: {with_it} of {}",
            binds.len()
        );
    }
    let softness: Vec<f64> = binds
        .iter()
        .chain(autos.iter())
        .filter_map(|r| r.action["softness"].as_f64())
        .collect();
    assert!(
        softness.iter().any(|s| *s > 1.0),
        "an out-of-range softness is clamped"
    );
    assert!(softness.iter().any(|s| *s > 0.0 && *s < 1.0));
    // Replace, not stack: a re-bind starts from a rig that already binds the stroke.
    assert!(rows
        .iter()
        .any(|r| r.name.starts_with("rebind") && r.before.matches("\"strokeId\"").count() == 2));
    assert!(autos
        .iter()
        .any(|r| r.name.starts_with("auto_rebinds") && r.before.contains("\"strokeId\":\"a1\"")));
    // Both flip values, a refused chain and a live-pose chain.
    let flips: Vec<bool> = oracle::of_kind("setIk")
        .iter()
        .filter_map(|r| r.action["flip"].as_bool())
        .collect();
    assert!(flips.contains(&true) && flips.contains(&false));
    assert_eq!(rows.iter().filter(|r| r.ret == json!(false)).count(), 2);
    // No-op rows exist for the edits whose no-op must keep the bytes.
    assert_eq!(rows.iter().filter(|r| !r.changes()).count(), 5);
    // Every changing row differs from its start by more than its own spelling.
    for row in rows.iter().filter(|r| r.changes()) {
        let before: Value = serde_json::from_str(row.before).unwrap_or(Value::Null);
        let after: Value = serde_json::from_str(row.after).unwrap();
        assert_ne!(before, after, "{}", row.name);
    }
    // The rest pose of a bone only ever changes through a redraw.
    for row in rows
        .iter()
        .filter(|r| r.kind() != "drawBone" && r.before != "null")
    {
        let before: Value = serde_json::from_str(row.before).unwrap();
        let after: Value = serde_json::from_str(row.after).unwrap();
        for (id, bone) in before["bones"].as_object().unwrap() {
            assert_eq!(
                bone["restSegments"], after["bones"][id]["restSegments"],
                "{}: {id} rest pose",
                row.name
            );
        }
    }
}

fn outcome(reply: &Value) -> String {
    if reply["ok"] == true {
        "ok".to_owned()
    } else {
        code(reply).to_owned()
    }
}

/// The rig edit kinds whose weights come from the evaluation kernels: without that feature they are
/// `unavailable` whatever the layer is, so only the other kinds can show a lock refusal by themselves.
const KERNEL_BACKED: [&str; 5] = [
    "bindStroke",
    "autoAssign",
    "resetWeights",
    "addWeight",
    "setIk",
];

#[test]
fn a_locked_layer_refuses_every_kind_of_rig_edit_and_publishes_nothing() {
    let kinds = [
        "drawBone",
        "moveSegment",
        "setBoneRadius",
        "bindStroke",
        "autoAssign",
        "setWeight",
        "addWeight",
        "removeWeight",
        "resetWeights",
        "setIk",
    ];
    for kind in kinds {
        let row = oracle::of_kind(kind)
            .into_iter()
            .find(|row| row.ret != json!(false))
            .unwrap_or_else(|| panic!("no accepted {kind} row"));
        let id = format!("{kind}-{}", row.name);
        // Control: the same request on the unlocked document is accepted (kernel-backed kinds need the
        // evaluation feature to be accepted at all).
        if cfg!(feature = "evaluation") || !KERNEL_BACKED.contains(&kind) {
            let mut open = owner_for(&row);
            let reply = run(&mut open, &id, payload(row.action.clone()));
            assert_eq!(outcome(&reply), "ok", "control {id}: {reply}");
        }
        let mut locked = owner_with_layer_members(&row, "\"locked\":true");
        let before = state(&locked);
        let reply = run(&mut locked, &id, payload(row.action.clone()));
        assert_eq!(outcome(&reply), "unavailable", "{id}: {reply}");
        assert_eq!(state(&locked), before, "{id}: nothing was published");
    }
    // The kinds the baseline has no button for, on a rigged and bound layer.
    let row = oracle::act("set_weight_40");
    for (name, action) in [
        ("bindMesh", json!({"kind":"bindMesh","meshId":"im_1"})),
        ("removeIk", json!({"kind":"removeIk","endKey":"bone2:1"})),
        ("removeBone", json!({"kind":"removeBone","boneId":"bone2"})),
        (
            "removeBind",
            json!({"kind":"removeBind","target":{"strokeId":"s1"}}),
        ),
    ] {
        // Control: unlocked, none of them is `unavailable` (a missing mesh, chain or a still-weighted bone is
        // a different refusal; removing the bind succeeds).
        let mut open = owner_for(&row);
        let reply = run(&mut open, name, payload(action.clone()));
        assert_ne!(outcome(&reply), "unavailable", "control {name}: {reply}");
        let mut locked = owner_with_layer_members(&row, "\"locked\":true");
        let before = state(&locked);
        let reply = run(&mut locked, name, payload(action));
        assert_eq!(outcome(&reply), "unavailable", "{name}: {reply}");
        assert_eq!(state(&locked), before, "{name}");
    }
}

#[cfg(feature = "evaluation")]
#[test]
fn auto_assign_refuses_an_active_path_effect_and_no_other_rig_edit_does() {
    let auto = oracle::act("auto_two_strokes_default_radius");
    let mv = oracle::act("move_an_anchor");
    let active = [
        json!([{"type": "zigzag"}]),
        json!([{"type": "roughen", "enabled": true}]),
        json!([{"type": "zigzag", "enabled": false}, {"type": "roughen"}]),
    ];
    for (index, stack) in active.into_iter().enumerate() {
        let members = format!("\"pathFx\":{stack}");
        let mut owner = owner_with_layer_members(&auto, &members);
        let before = state(&owner);
        let reply = run(
            &mut owner,
            &format!("auto{index}"),
            payload(auto.action.clone()),
        );
        assert_eq!(outcome(&reply), "unavailable", "{stack}: {reply}");
        assert_eq!(state(&owner), before, "{stack}");
        // The baseline gates only the auto-assign button: moving a bone is not a drawing edit.
        let mut moving = owner_with_layer_members(&mv, &members);
        let reply = run(
            &mut moving,
            &format!("move{index}"),
            payload(mv.action.clone()),
        );
        assert_eq!(outcome(&reply), "ok", "{stack}: {reply}");
    }
    // A stack with nothing active (empty, or every entry disabled) never blocks.
    for (index, stack) in [json!([]), json!([{"type": "zigzag", "enabled": false}])]
        .into_iter()
        .enumerate()
    {
        let mut owner = owner_with_layer_members(&auto, &format!("\"pathFx\":{stack}"));
        let reply = run(
            &mut owner,
            &format!("quiet{index}"),
            payload(auto.action.clone()),
        );
        assert_eq!(outcome(&reply), "ok", "{stack}: {reply}");
    }
}

#[cfg(feature = "evaluation")]
#[test]
fn a_pressure_ribbon_is_never_bound_and_the_refusal_publishes_nothing() {
    // `s3` of this row is an open path, i.e. an `editable-vector-path` record that can be a ribbon.
    let bind = oracle::act("bind_an_open_path");
    let auto = json!({"kind": "autoAssign", "frame": 0});
    let radius = json!({"kind": "setBoneRadius", "boneId": "bone1", "radius": 50});
    // Control: the plain stroke binds, by hand and by auto-assign.
    for (id, action) in [("bind", bind.action.clone()), ("auto", auto.clone())] {
        let mut plain = owner_for(&bind);
        let reply = run(&mut plain, id, payload(action));
        assert_eq!(outcome(&reply), "ok", "control {id}: {reply}");
    }
    for (id, action) in [("bind", bind.action.clone()), ("auto", auto)] {
        let mut owner = owner_with_ribbon(&bind, "s3");
        let before = state(&owner);
        let reply = run(&mut owner, id, payload(action));
        assert_eq!(outcome(&reply), "unavailable", "{id}: {reply}");
        assert_eq!(state(&owner), before, "{id}: nothing was published");
    }
    // The ribbon does not stop edits that do not bind it.
    let mut owner = owner_with_ribbon(&bind, "s3");
    let reply = run(&mut owner, "radius", payload(radius));
    assert_eq!(outcome(&reply), "ok", "{reply}");
}
