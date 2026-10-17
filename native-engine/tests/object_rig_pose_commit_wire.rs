//! Envelope, sealing and strict-wire guards of the pose transaction (L03-Q, I13): cancel, stale, foreign
//! owner and wrong api publish nothing; a replay never executes a second time, also after undo; a prepared
//! pose commits once and only on its origin owner and revision; every single-flaw payload is refused as
//! `malformed_request` with bytes, revision and history unchanged.
use super::object_rig_pose_commit_oracle as oracle;
use super::object_rig_pose_commit_rows::*;
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn posed() -> oracle::Row {
    oracle::row("arm_rotate_soft_bone_turned_40_degrees")
}

#[test]
fn every_action_refuses_cancel_stale_foreign_and_replays_without_a_second_execution() {
    let row = posed();
    let mut cases: Vec<(&str, Value)> = vec![("reset", reset_payload())];
    if cfg!(feature = "evaluation") {
        cases.push(("commit", commit_payload(3)));
    }
    for (op, body) in cases {
        let mut owner = owner_for(&row, HELD);
        let original = state(&owner);
        for (id, patch) in [
            ("cancel", json!({"cancelledBeforeDispatch":true})),
            ("stale", json!({"expectedRevision":99})),
            ("foreign-instance", json!({"instanceId":"foreign"})),
            ("foreign-document", json!({"documentId":"foreign"})),
            ("wrong-api", json!({"apiVersion":1})),
        ] {
            let mut req = request(&owner, id, body.clone());
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
        let mut req = request(&owner, "missing", body.clone());
        req.as_object_mut().unwrap().remove("expectedRevision");
        assert_eq!(dispatch(&mut owner, &req)["ok"], false, "{op}");
        assert_eq!(state(&owner), original, "{op}");
        let req = request(&owner, "replay", body.clone());
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
fn a_prepared_pose_is_sealed_to_its_origin_owner_and_revision() {
    let row = posed();
    let body = serde_json::to_vec(&reset_payload()).unwrap();
    let mut source = owner_for(&row, PLAIN);
    let candidate = source.prepare_rig_pose_json(&body).unwrap();
    let mut other = owner_for(&row, PLAIN);
    let other_before = state(&other);
    assert_eq!(
        other.commit_rig_pose(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        decode_storage_project(doc_for(&row, PLAIN).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        foreign.commit_rig_pose(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(state(&other), other_before);
    assert_eq!(foreign.history_depths(), (0, 0));
    let before = source.content_revision();
    assert_eq!(
        source.commit_rig_pose(candidate.clone()).unwrap(),
        before + 1
    );
    let applied = state(&source);
    assert_eq!(
        source.commit_rig_pose(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision,
        "a candidate commits once"
    );
    assert_eq!(state(&source), applied);
    // A candidate prepared before another edit lands is stale, even when it would have changed nothing.
    let waiting = source.prepare_rig_pose_json(&body).unwrap();
    owner_advance(&mut source);
    let advanced = state(&source);
    assert_eq!(
        source.commit_rig_pose(waiting).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&source), advanced);
}

/// Lands an unrelated revision on the owner (undo then redo of the reset moves the revision twice).
fn owner_advance(owner: &mut NativeObjectHistory) {
    owner.undo(owner.content_revision()).unwrap();
    owner.redo(owner.content_revision()).unwrap();
}

#[test]
fn a_no_op_candidate_commits_to_the_same_revision_and_adds_no_history() {
    let row = oracle::row("ik_reset_pose_gives_the_stored_geometry_back");
    let mut owner = owner_for(&row, PLAIN);
    let before = state(&owner);
    let body = serde_json::to_vec(&reset_payload()).unwrap();
    let candidate = owner.prepare_rig_pose_json(&body).unwrap();
    assert_eq!(owner.commit_rig_pose(candidate).unwrap(), before.1);
    assert_eq!(state(&owner), before);
}

#[test]
fn single_flaw_payloads_are_refused_as_malformed_and_publish_nothing() {
    let reset = reset_payload;
    let commit = || commit_payload(3);
    let flaws: Vec<(&str, Value)> = vec![
        (
            "command_wrong",
            with(reset(), |p| p["command"] = json!("object.rig.edit")),
        ),
        (
            "command_missing",
            with(reset(), |p| drop_member(p, "command")),
        ),
        (
            "layer_missing",
            with(reset(), |p| drop_member(p, "layerUid")),
        ),
        ("layer_empty", with(reset(), |p| p["layerUid"] = json!(""))),
        (
            "layer_long",
            with(reset(), |p| p["layerUid"] = json!("l".repeat(129))),
        ),
        ("layer_number", with(reset(), |p| p["layerUid"] = json!(3))),
        (
            "action_missing",
            with(reset(), |p| drop_member(p, "action")),
        ),
        ("action_null", with(reset(), |p| p["action"] = json!(null))),
        (
            "action_array",
            with(reset(), |p| p["action"] = json!([{"kind":"reset"}])),
        ),
        (
            "action_string",
            with(reset(), |p| p["action"] = json!("reset")),
        ),
        (
            "kind_missing",
            with(reset(), |p| drop_member(&mut p["action"], "kind")),
        ),
        (
            "kind_unknown",
            with(reset(), |p| p["action"]["kind"] = json!("bake")),
        ),
        (
            "kind_number",
            with(reset(), |p| p["action"]["kind"] = json!(1)),
        ),
        (
            "kind_case",
            with(reset(), |p| p["action"]["kind"] = json!("Reset")),
        ),
        (
            "reset_with_frame",
            with(reset(), |p| p["action"]["frame"] = json!(3)),
        ),
        (
            "reset_with_null_frame",
            with(reset(), |p| p["action"]["frame"] = json!(null)),
        ),
        (
            "commit_without_frame",
            with(commit(), |p| drop_member(&mut p["action"], "frame")),
        ),
        (
            "commit_frame_null",
            with(commit(), |p| p["action"]["frame"] = json!(null)),
        ),
        (
            "commit_frame_fraction",
            with(commit(), |p| p["action"]["frame"] = json!(1.5)),
        ),
        (
            "commit_frame_negative",
            with(commit(), |p| p["action"]["frame"] = json!(-1)),
        ),
        (
            "commit_frame_huge",
            with(commit(), |p| p["action"]["frame"] = json!(4294967296u64)),
        ),
        (
            "commit_frame_string",
            with(commit(), |p| p["action"]["frame"] = json!("3")),
        ),
        (
            "action_unknown_member",
            with(commit(), |p| p["action"]["force"] = json!(true)),
        ),
        (
            "top_unknown_member",
            with(commit(), |p| p["frame"] = json!(3)),
        ),
    ];
    let row = posed();
    for (name, body) in flaws {
        let mut owner = owner_for(&row, HELD);
        assert_refused(
            &mut owner,
            name,
            body,
            ("invalid_request", "malformed_request"),
        );
    }
}

#[test]
fn raw_bytes_refuse_duplicates_oversize_and_malformed_input() {
    let row = posed();
    let owner = owner_for(&row, HELD);
    let before = state(&owner);
    let twin = reset_payload().to_string();
    let duplicate_kind = r#"{"command":"object.rig.pose","layerUid":"L1","action":{"kind":"reset","kind":"commit","frame":3}}"#;
    let escaped = "{\"command\":\"object.rig.pose\",\"layerUid\":\"L1\",\"action\":{\"kind\":\"commit\",\"frame\":3,\"fr\\u0061me\":4}}";
    let duplicate_layer = r#"{"command":"object.rig.pose","layerUid":"L1","layerUid":"L2","action":{"kind":"reset"}}"#;
    for raw in [
        duplicate_kind,
        escaped,
        duplicate_layer,
        "{}",
        "[]",
        "null",
        "",
        "{\"command\":",
        "not json",
    ] {
        let refusal = owner.prepare_rig_pose_json(raw.as_bytes()).unwrap_err();
        assert_eq!(refusal.0, DispatchErrorCode::InvalidRequest, "{raw}");
        assert!(refusal.1.starts_with("malformed_request"), "{raw}");
    }
    let refusal = owner
        .prepare_rig_pose_json(&[0xff, 0xfe, 0x7b])
        .unwrap_err();
    assert_eq!(
        refusal.0,
        DispatchErrorCode::InvalidRequest,
        "bytes that are not UTF-8"
    );
    let mut padded = twin;
    padded.push_str(&" ".repeat(1024 - padded.len()));
    assert!(owner.prepare_rig_pose_json(padded.as_bytes()).is_ok());
    padded.push(' ');
    assert_eq!(
        owner
            .prepare_rig_pose_json(padded.as_bytes())
            .unwrap_err()
            .0,
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(state(&owner), before, "preparation changes nothing");
}

#[test]
fn a_whole_number_frame_spelled_with_a_fraction_is_the_same_frame() {
    // `3.0` is the JavaScript number 3: the payload is accepted and refers to frame 3.
    let row = oracle::row("empty_rig_has_no_binds");
    let owner = owner_for(&row, HELD);
    let raw =
        r#"{"command":"object.rig.pose","layerUid":"L1","action":{"kind":"commit","frame":3.0}}"#;
    // The empty rig is a typed no_rigged_stroke, which is only reachable after the payload was accepted.
    let refusal = owner.prepare_rig_pose_json(raw.as_bytes()).unwrap_err();
    assert_eq!(refusal.0, DispatchErrorCode::NotFound);
    assert!(refusal.1.starts_with("no_rigged_stroke"));
}
