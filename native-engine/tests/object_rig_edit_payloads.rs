//! Strict payload decoding of the rig edit transaction (L03-O): single-flaw payloads against accepted twins,
//! raw-byte refusals (duplicates, oversize, malformed) and number spelling. Every refusal is
//! `invalid_request` and leaves bytes, revision and history untouched.
use super::object_rig_edit_rows::*;
use super::*;
use serde_json::{json, Value};

fn segment(point: [i64; 2]) -> Value {
    json!({"point":point,"handleIn":[0,0],"handleOut":[0,0]})
}

fn draw_twin() -> Value {
    json!({"kind":"drawBone","segments":[segment([0,0]),segment([100,0])],"closed":false})
}

fn weight_twin() -> Value {
    json!({"kind":"setWeight","target":{"strokeId":"s1"},"vertex":0,"entry":0,"percent":40})
}

fn remove_weight_twin() -> Value {
    json!({"kind":"removeWeight","target":{"strokeId":"s1"},"vertex":0,"entry":0})
}

#[test]
fn single_flaw_draw_payloads_are_refused_and_their_twin_is_accepted() {
    assert_accepted(
        "draw_first_bone_creates_the_rig",
        vec![
            draw_twin(),
            with(draw_twin(), |a| a["closed"] = json!(true)),
        ],
    );
    let twin = draw_twin;
    let flaws = vec![
        (
            "one_vertex",
            with(twin(), |a| a["segments"] = json!([segment([0, 0])])),
        ),
        ("no_vertex", with(twin(), |a| a["segments"] = json!([]))),
        (
            "segments_missing",
            with(twin(), |a| drop_member(a, "segments")),
        ),
        (
            "segments_object",
            with(twin(), |a| a["segments"] = json!({})),
        ),
        ("closed_missing", with(twin(), |a| drop_member(a, "closed"))),
        (
            "closed_string",
            with(twin(), |a| a["closed"] = json!("false")),
        ),
        ("closed_number", with(twin(), |a| a["closed"] = json!(0))),
        ("kind_missing", with(twin(), |a| drop_member(a, "kind"))),
        (
            "kind_unknown",
            with(twin(), |a| a["kind"] = json!("teleport")),
        ),
        ("kind_case", with(twin(), |a| a["kind"] = json!("DrawBone"))),
        ("kind_number", with(twin(), |a| a["kind"] = json!(3))),
        ("unknown_member", with(twin(), |a| a["radius"] = json!(5))),
        ("bone_id_empty", with(twin(), |a| a["boneId"] = json!(""))),
        (
            "bone_id_long",
            with(twin(), |a| a["boneId"] = json!("b".repeat(129))),
        ),
        ("bone_id_number", with(twin(), |a| a["boneId"] = json!(7))),
        (
            "handle_in_missing",
            with(twin(), |a| drop_member(&mut a["segments"][0], "handleIn")),
        ),
        (
            "handle_out_missing",
            with(twin(), |a| drop_member(&mut a["segments"][1], "handleOut")),
        ),
        (
            "point_three_numbers",
            with(twin(), |a| a["segments"][0]["point"] = json!([0, 0, 0])),
        ),
        (
            "point_one_number",
            with(twin(), |a| a["segments"][0]["point"] = json!([0])),
        ),
        (
            "point_strings",
            with(twin(), |a| a["segments"][0]["point"] = json!(["a", "b"])),
        ),
        (
            "point_null",
            with(twin(), |a| a["segments"][0]["point"] = json!(null)),
        ),
        (
            "point_huge",
            with(twin(), |a| a["segments"][0]["point"] = json!([1e10, 0])),
        ),
        (
            "handle_negative_huge",
            with(twin(), |a| {
                a["segments"][1]["handleOut"] = json!([0, -1e10])
            }),
        ),
        (
            "segment_unknown_member",
            with(twin(), |a| a["segments"][0]["tension"] = json!(1)),
        ),
        (
            "segment_array",
            with(twin(), |a| a["segments"][0] = json!([0, 0])),
        ),
    ];
    assert_refused("draw_first_bone_creates_the_rig", flaws);
}

#[test]
fn single_flaw_move_radius_and_removal_payloads_are_refused_and_their_twins_accepted() {
    assert_accepted(
        "move_an_anchor",
        vec![
            move_twin(),
            with(move_twin(), |a| a["part"] = json!("handleIn")),
            with(move_twin(), |a| a["breakTangent"] = json!(true)),
            json!({"kind":"setBoneRadius","boneId":"bone1","radius":50}),
        ],
    );
    let twin = move_twin;
    let radius = || json!({"kind":"setBoneRadius","boneId":"bone1","radius":50});
    let flaws = vec![
        ("vi_negative", with(twin(), |a| a["vi"] = json!(-1))),
        ("vi_fraction", with(twin(), |a| a["vi"] = json!(1.5))),
        ("vi_string", with(twin(), |a| a["vi"] = json!("1"))),
        ("vi_huge", with(twin(), |a| a["vi"] = json!(4294967296u64))),
        ("vi_missing", with(twin(), |a| drop_member(a, "vi"))),
        ("part_unknown", with(twin(), |a| a["part"] = json!("tip"))),
        ("part_case", with(twin(), |a| a["part"] = json!("Point"))),
        ("part_missing", with(twin(), |a| drop_member(a, "part"))),
        ("to_one", with(twin(), |a| a["to"] = json!([1]))),
        (
            "to_null_member",
            with(twin(), |a| a["to"] = json!([1, null])),
        ),
        ("to_huge", with(twin(), |a| a["to"] = json!([1e10, 1]))),
        ("to_missing", with(twin(), |a| drop_member(a, "to"))),
        (
            "break_tangent_number",
            with(twin(), |a| a["breakTangent"] = json!(1)),
        ),
        ("bone_missing", with(twin(), |a| drop_member(a, "boneId"))),
        ("bone_empty", with(twin(), |a| a["boneId"] = json!(""))),
        (
            "move_unknown_member",
            with(twin(), |a| a["pressure"] = json!(1)),
        ),
        (
            "radius_missing",
            with(radius(), |a| drop_member(a, "radius")),
        ),
        (
            "radius_string",
            with(radius(), |a| a["radius"] = json!("50")),
        ),
        ("radius_huge", with(radius(), |a| a["radius"] = json!(1e10))),
        ("radius_null", with(radius(), |a| a["radius"] = json!(null))),
        (
            "remove_bone_extra",
            json!({"kind":"removeBone","boneId":"bone1","force":true}),
        ),
        ("remove_bone_missing_id", json!({"kind":"removeBone"})),
        (
            "remove_ik_empty_key",
            json!({"kind":"removeIk","endKey":""}),
        ),
        (
            "remove_ik_long_key",
            json!({"kind":"removeIk","endKey":"k".repeat(161)}),
        ),
        ("remove_ik_number", json!({"kind":"removeIk","endKey":7})),
    ];
    assert_refused("move_an_anchor", flaws);
}

#[test]
fn single_flaw_weight_and_target_payloads_are_refused_and_their_twins_accepted() {
    assert_accepted(
        "set_weight_40",
        vec![
            weight_twin(),
            with(weight_twin(), |a| a["percent"] = json!(250)),
            with(weight_twin(), |a| a["percent"] = json!(-5)),
            remove_weight_twin(),
            json!({"kind":"removeBind","target":{"strokeId":"s1"}}),
        ],
    );
    let twin = weight_twin;
    let flaws = vec![
        (
            "percent_missing",
            with(twin(), |a| drop_member(a, "percent")),
        ),
        (
            "percent_string",
            with(twin(), |a| a["percent"] = json!("40")),
        ),
        ("percent_huge", with(twin(), |a| a["percent"] = json!(1e10))),
        ("vertex_negative", with(twin(), |a| a["vertex"] = json!(-1))),
        (
            "vertex_fraction",
            with(twin(), |a| a["vertex"] = json!(0.5)),
        ),
        ("entry_negative", with(twin(), |a| a["entry"] = json!(-1))),
        ("entry_missing", with(twin(), |a| drop_member(a, "entry"))),
        ("target_missing", with(twin(), |a| drop_member(a, "target"))),
        ("target_empty", with(twin(), |a| a["target"] = json!({}))),
        (
            "target_both",
            with(twin(), |a| {
                a["target"] = json!({"strokeId":"s1","meshId":"im_1"})
            }),
        ),
        (
            "target_blank_id",
            with(twin(), |a| a["target"] = json!({"strokeId":""})),
        ),
        (
            "target_unknown_member",
            with(twin(), |a| a["target"] = json!({"layerUid":"L0"})),
        ),
        ("target_string", with(twin(), |a| a["target"] = json!("s1"))),
        (
            "target_null_id",
            with(twin(), |a| a["target"] = json!({"strokeId":null})),
        ),
        (
            "remove_weight_extra",
            with(remove_weight_twin(), |a| a["percent"] = json!(1)),
        ),
        (
            "add_weight_bone_missing",
            json!({"kind":"addWeight","target":{"strokeId":"s1"},"vertex":0}),
        ),
        ("remove_bind_without_target", json!({"kind":"removeBind"})),
    ];
    assert_refused("set_weight_40", flaws);
}
