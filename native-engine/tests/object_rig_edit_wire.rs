//! Strict wire decoding of the rig edit transaction, part two (L03-O): the kernel-backed actions' payload
//! flaws (refused before any kernel runs), the envelope members, raw-byte refusals and number spelling.
use super::object_rig_edit_oracle as oracle;
use super::object_rig_edit_rows::*;
use super::*;
use serde_json::{json, Value};

#[test]
fn single_flaw_bind_assign_reset_and_ik_payloads_are_refused_before_any_kernel_runs() {
    // These never reach a kernel: the refusal is the same with and without the evaluation feature.
    let bind = || json!({"kind":"bindStroke","strokeId":"s1","frame":0,"radius":90,"rotate":true});
    let auto = || json!({"kind":"autoAssign","frame":0,"defaultRadius":120});
    let reset = || json!({"kind":"resetWeights","target":{"strokeId":"s1"},"vertex":0});
    let chain = |end: i64| {
        json!({"kind":"setIk","root":{"boneId":"bone1","vi":0},"joint":{"boneId":"bone1","vi":1},
            "end":{"boneId":"bone2","vi":end},"flip":false})
    };
    let flaws = vec![
        (
            "bind_stroke_missing",
            with(bind(), |a| drop_member(a, "strokeId")),
        ),
        (
            "bind_stroke_empty",
            with(bind(), |a| a["strokeId"] = json!("")),
        ),
        (
            "bind_frame_missing",
            with(bind(), |a| drop_member(a, "frame")),
        ),
        (
            "bind_frame_negative",
            with(bind(), |a| a["frame"] = json!(-1)),
        ),
        (
            "bind_frame_fraction",
            with(bind(), |a| a["frame"] = json!(1.5)),
        ),
        (
            "bind_bone_ids_object",
            with(bind(), |a| a["boneIds"] = json!({})),
        ),
        (
            "bind_bone_ids_blank",
            with(bind(), |a| a["boneIds"] = json!(["bone1", ""])),
        ),
        (
            "bind_bone_ids_number",
            with(bind(), |a| a["boneIds"] = json!([1])),
        ),
        (
            "bind_radius_negative",
            with(bind(), |a| a["radius"] = json!(-1)),
        ),
        (
            "bind_radius_huge",
            with(bind(), |a| a["radius"] = json!(1e10)),
        ),
        (
            "bind_rotate_string",
            with(bind(), |a| a["rotate"] = json!("yes")),
        ),
        (
            "bind_softness_string",
            with(bind(), |a| a["softness"] = json!("0.5")),
        ),
        (
            "bind_unknown_member",
            with(bind(), |a| a["weights"] = json!([])),
        ),
        (
            "auto_frame_missing",
            with(auto(), |a| drop_member(a, "frame")),
        ),
        (
            "auto_default_radius_negative",
            with(auto(), |a| a["defaultRadius"] = json!(-3)),
        ),
        ("auto_paths_number", with(auto(), |a| a["paths"] = json!(1))),
        (
            "auto_meshes_string",
            with(auto(), |a| a["meshes"] = json!("no")),
        ),
        (
            "auto_unknown_member",
            with(auto(), |a| a["bones"] = json!(["bone1"])),
        ),
        (
            "reset_softness_above_one",
            with(reset(), |a| a["softness"] = json!(1.5)),
        ),
        (
            "reset_softness_negative",
            with(reset(), |a| a["softness"] = json!(-0.1)),
        ),
        (
            "reset_radius_negative",
            with(reset(), |a| a["radius"] = json!(-5)),
        ),
        (
            "reset_vertex_missing",
            with(reset(), |a| drop_member(a, "vertex")),
        ),
        (
            "ik_root_missing",
            with(chain(1), |a| drop_member(a, "root")),
        ),
        ("ik_vi_negative", with(chain(-1), |_| ())),
        (
            "ik_vi_fraction",
            with(chain(1), |a| a["end"]["vi"] = json!(0.5)),
        ),
        (
            "ik_ref_extra",
            with(chain(1), |a| a["joint"]["radius"] = json!(1)),
        ),
        (
            "ik_ref_blank_bone",
            with(chain(1), |a| a["end"]["boneId"] = json!("")),
        ),
        ("ik_flip_number", with(chain(1), |a| a["flip"] = json!(1))),
        ("bind_mesh_without_id", json!({"kind":"bindMesh"})),
        ("bind_mesh_blank_id", json!({"kind":"bindMesh","meshId":""})),
    ];
    assert_refused("bind_rotate_and_softness", flaws);
}

#[test]
fn envelope_members_of_the_payload_are_strict() {
    let row = oracle::act("move_an_anchor");
    let good = payload(move_twin());
    let flaws: Vec<(&str, Value)> = vec![
        (
            "command_wrong",
            with(good.clone(), |p| p["command"] = json!("object.rig.draw")),
        ),
        (
            "command_missing",
            with(good.clone(), |p| drop_member(p, "command")),
        ),
        (
            "layer_missing",
            with(good.clone(), |p| drop_member(p, "layerUid")),
        ),
        (
            "layer_empty",
            with(good.clone(), |p| p["layerUid"] = json!("")),
        ),
        (
            "layer_long",
            with(good.clone(), |p| p["layerUid"] = json!("l".repeat(129))),
        ),
        (
            "layer_number",
            with(good.clone(), |p| p["layerUid"] = json!(3)),
        ),
        (
            "action_missing",
            with(good.clone(), |p| drop_member(p, "action")),
        ),
        (
            "action_array",
            with(good.clone(), |p| p["action"] = json!([move_twin()])),
        ),
        (
            "action_null",
            with(good.clone(), |p| p["action"] = json!(null)),
        ),
        (
            "unknown_member",
            with(good.clone(), |p| p["frame"] = json!(0)),
        ),
    ];
    for (name, body) in flaws {
        let mut owner = owner_for(&row);
        let before = state(&owner);
        let req = request(&owner, name, body);
        let reply = dispatch(&mut owner, &req);
        assert_eq!(reply["ok"], false, "{name}");
        assert_eq!(code(&reply), "invalid_request", "{name}");
        assert_eq!(state(&owner), before, "{name}");
    }
}

#[test]
fn raw_bytes_refuse_duplicates_oversize_and_malformed_input_and_accept_the_padded_twin() {
    let row = oracle::act("move_an_anchor");
    let owner = owner_for(&row);
    let before = state(&owner);
    let twin = payload(move_twin()).to_string();
    let duplicate_kind = r#"{"command":"object.rig.edit","layerUid":"L0","action":{"kind":"moveSegment","kind":"setBoneRadius","boneId":"bone1","vi":1,"part":"point","to":[1,2]}}"#;
    let escaped = "{\"command\":\"object.rig.edit\",\"layerUid\":\"L0\",\"action\":{\"kind\":\"moveSegment\",\"boneId\":\"bone1\",\"vi\":1,\"part\":\"point\",\"to\":[1,2],\"b\\u006fneId\":\"bone2\"}}";
    let duplicate_layer = r#"{"command":"object.rig.edit","layerUid":"L0","layerUid":"L1","action":{"kind":"removeBone","boneId":"bone1"}}"#;
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
        assert_eq!(
            owner.prepare_rig_edit_json(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{raw}"
        );
    }
    assert_eq!(
        owner
            .prepare_rig_edit_json(&[0xff, 0xfe, 0x7b])
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest,
        "bytes that are not UTF-8"
    );
    let mut padded = twin;
    padded.push_str(&" ".repeat(4096 - padded.len()));
    assert!(owner.prepare_rig_edit_json(padded.as_bytes()).is_ok());
    padded.push(' ');
    assert_eq!(
        owner.prepare_rig_edit_json(padded.as_bytes()).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(state(&owner), before, "preparation changes nothing");
}

#[test]
fn drawn_numbers_are_spelled_like_the_baseline_export_spells_them() {
    // `1.0` and `-0.0` are the JavaScript numbers 1 and -0; JSON.stringify writes `1` and `0`.
    let raw = r#"{"command":"object.rig.edit","layerUid":"L0","action":{"kind":"drawBone","segments":[{"point":[0.0,-0.0],"handleIn":[0,0],"handleOut":[1.0,0]},{"point":[100,2.50],"handleIn":[-1.0,0],"handleOut":[0,0]}],"closed":false}}"#;
    let mut owner = owner_of_text(&doc_text("null", &[]));
    let candidate = owner.prepare_rig_edit_json(raw.as_bytes()).unwrap();
    owner.commit_rig_edit(candidate).unwrap();
    let segments = r#"[{"point":[0,0],"handleIn":[0,0],"handleOut":[1,0]},{"point":[100,2.5],"handleIn":[-1,0],"handleOut":[0,0]}]"#;
    assert_eq!(
        rig_text(&owner),
        format!(
            r#"{{"bones":{{"bone1":{{"segments":{segments},"restSegments":{segments},"closed":false}}}},"ikChains":{{}},"nextId":2,"binds":[]}}"#
        )
    );
}
