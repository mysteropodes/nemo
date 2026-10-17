//! Negative and mutation controls of the pose transaction (L03-Q, I13): the pure rules and the strict
//! decoder with private access, flawed twins that must differ from the real rules, and the refusal table.
use super::parse::{parse_pose, COMMAND};
use super::rules::{js_number, replace_segments, rest_rig, same_pose};
use super::*;
use crate::object_codec::decode_storage_project;
use crate::object_document::object_rig_record::RigRecord;
use crate::object_snapshot::object_rig_pose_read::{PoseStroke, PoseVertex};
use serde_json::{json, Value};

fn rig(text: &str) -> RigRecord {
    serde_json::from_str(text).expect("an admitted rig record")
}

fn bone_rig(segments: &str, rest: &str) -> RigRecord {
    rig(&format!(
        r#"{{"bones":{{"bone1":{{"segments":{segments},"restSegments":{rest},"closed":false}}}},"ikChains":{{}},"nextId":2,"binds":[]}}"#
    ))
}

const REST: &str = r#"[{"point":[0,0],"handleIn":null},{"point":[100,0],"handleOut":[0.5,0]}]"#;

#[test]
fn a_pose_that_is_the_rest_pose_is_not_reset_and_any_difference_is() {
    // Identical, and equal in value but spelled differently (`100.0`, `0.50`, `-0`): nothing to reset.
    assert!(rest_rig(&bone_rig(REST, REST)).is_none());
    let spelled =
        r#"[{"point":[-0,0.0],"handleIn":null},{"point":[100.0,0],"handleOut":[0.50,0]}]"#;
    assert!(rest_rig(&bone_rig(spelled, REST)).is_none());
    // One number, one handle value, one handle state, one vertex more or fewer: each IS a pose.
    let flaws = [
        r#"[{"point":[0,0],"handleIn":null},{"point":[100,0.5],"handleOut":[0.5,0]}]"#,
        r#"[{"point":[0,0],"handleIn":null},{"point":[100,0],"handleOut":[0.5,0.25]}]"#,
        r#"[{"point":[0,0]},{"point":[100,0],"handleOut":[0.5,0]}]"#,
        r#"[{"point":[0,0],"handleIn":[0,0]},{"point":[100,0],"handleOut":[0.5,0]}]"#,
        r#"[{"point":[0,0],"handleIn":null},{"point":[100,0]}]"#,
        r#"[{"point":[0,0],"handleIn":null},{"point":[100,0],"handleOut":[0.5,0]},{"point":[1,1]}]"#,
    ];
    for flaw in flaws {
        let posed = bone_rig(flaw, REST);
        let next = rest_rig(&posed).expect(flaw);
        assert_eq!(
            serde_json::to_string(&next.bones.entries()[0].1.segments).unwrap(),
            REST,
            "{flaw}"
        );
        assert!(!same_pose(
            &posed.bones.entries()[0].1.segments,
            &posed.bones.entries()[0].1.rest_segments
        ));
    }
}

/// A flawed twin of the equality: it reads an absent handle and a `null` handle as the same state.
fn same_pose_blind_to_handle_state(a: &RigRecord, b: &RigRecord) -> bool {
    let strip = |text: &str| text.replace(r#","handleIn":null"#, "");
    strip(&serde_json::to_string(&a.bones.entries()[0].1.segments).unwrap())
        == strip(&serde_json::to_string(&b.bones.entries()[0].1.segments).unwrap())
}

#[test]
fn the_handle_state_flawed_twin_is_caught_where_the_real_rule_sees_a_pose() {
    let absent = bone_rig(
        r#"[{"point":[0,0]},{"point":[100,0],"handleOut":[0.5,0]}]"#,
        REST,
    );
    let rest = bone_rig(REST, REST);
    assert!(
        same_pose_blind_to_handle_state(&absent, &rest),
        "the twin is blind to the state"
    );
    assert!(rest_rig(&absent).is_some(), "the real rule is not");
}

#[test]
fn a_reset_changes_only_the_segments_of_posed_bones() {
    let two = rig(&format!(
        r#"{{"bones":{{"bone1":{{"segments":{REST},"restSegments":{REST},"closed":false}},"bone2":{{"segments":[{{"point":[1,2]}},{{"point":[3,4]}}],"restSegments":[{{"point":[5,6]}},{{"point":[7,8]}}],"closed":true,"radius":30}}}},"ikChains":{{}},"nextId":3,"binds":[]}}"#
    ));
    let next = rest_rig(&two).expect("bone2 is posed");
    assert_eq!(
        next.bones.entries()[0],
        two.bones.entries()[0],
        "bone1 is untouched, in place"
    );
    let bone2 = &next.bones.entries()[1].1;
    assert_eq!(bone2.segments, two.bones.entries()[1].1.rest_segments);
    assert!(bone2.closed);
    assert_eq!(
        bone2.radius_value(),
        Some(30.0),
        "the influence radius is not a pose"
    );
    assert_eq!(next.next_id, 3);
    assert_eq!(next.ik_chains, two.ik_chains);
}

#[test]
fn computed_numbers_are_spelled_like_json_stringify_and_equal_values_keep_their_spelling() {
    for (value, want) in [
        (1.0, "1"),
        (-0.0, "0"),
        (0.0, "0"),
        (2.5, "2.5"),
        (-3.0, "-3"),
        (0.1 + 0.2, "0.30000000000000004"),
    ] {
        assert_eq!(js_number(value).unwrap().to_string(), want, "{value}");
    }
    assert!(js_number(f64::NAN).is_none() && js_number(f64::INFINITY).is_none());
    // A stored `100.0` posed to 100 stays `100.0`; a stored `1` posed to 2.5 becomes 2.5.
    let doc = |x: &str| {
        let record = json!({"schemaVersion":1,"family":"editable-vector-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"s1"},
            "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[{"point":{"x":1,"y":2},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
                    {"point":{"x":7,"y":8},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
            "fill":null,"vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1}});
        let text = serde_json::to_string(&record)
            .unwrap()
            .replace(r#""x":1,"#, &format!(r#""x":{x},"#));
        let document = format!(
            r#"{{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":2,"layers":[{{"layerUid":"L1"}}],"objects":[{text}]}}"#
        );
        decode_storage_project(document.as_bytes()).unwrap()
    };
    // The record admission wants 2..256 cubic segments: the second vertex is posed to its stored value.
    let pose = |x: f64| PoseStroke {
        stroke_id: "s1".to_owned(),
        closed: false,
        vertices: vec![
            PoseVertex {
                point: [x, 2.0],
                handle_in: [0.0, 0.0],
                handle_out: [0.0, 0.0],
            },
            PoseVertex {
                point: [7.0, 8.0],
                handle_in: [0.0, 0.0],
                handle_out: [0.0, 0.0],
            },
        ],
    };
    let mut same = doc("1.0");
    let before = same.clone();
    replace_segments(&mut same, "L1", 0, &[pose(1.0)]).unwrap();
    assert_eq!(
        same, before,
        "an equal value keeps the stored spelling: not one byte moves"
    );
    let mut moved = doc("1");
    replace_segments(&mut moved, "L1", 0, &[pose(2.5)]).unwrap();
    let wire = serde_json::to_value(&moved).unwrap();
    assert_eq!(
        wire["objects"][0]["geometry"]["segments"][0]["point"]["x"],
        json!(2.5)
    );
    assert_eq!(
        wire["objects"][0]["geometry"]["segments"][0]["point"]["y"],
        json!(2)
    );
}

#[test]
fn replacing_segments_refuses_a_missing_stroke_or_another_vertex_count() {
    let layer = json!({"layerUid":"L1"});
    let record = |id: &str, frame: u32| {
        json!({"schemaVersion":1,"family":"editable-vector-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":"L1","strokeId":id},
            "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[{"point":{"x":1,"y":2},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
                    {"point":{"x":5,"y":6},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
            "fill":null,"vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1}})
    };
    let text = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[layer],"objects":[record("s1", 0), record("s1", 1)]});
    let mut document = decode_storage_project(text.to_string().as_bytes()).unwrap();
    let pose = |id: &str, vertices: usize| PoseStroke {
        stroke_id: id.to_owned(),
        closed: false,
        vertices: vec![
            PoseVertex {
                point: [9.0, 9.0],
                handle_in: [0.0, 0.0],
                handle_out: [0.0, 0.0],
            };
            vertices
        ],
    };
    // Each stored record has two vertices: one more or one fewer is another vertex count.
    assert_eq!(
        replace_segments(&mut document, "L1", 0, &[pose("nope", 2)]),
        Err(PoseRefusal::Internal)
    );
    assert_eq!(
        replace_segments(&mut document, "L1", 2, &[pose("s1", 2)]),
        Err(PoseRefusal::Internal)
    );
    assert_eq!(
        replace_segments(&mut document, "L2", 0, &[pose("s1", 2)]),
        Err(PoseRefusal::Internal)
    );
    assert_eq!(
        replace_segments(&mut document, "L1", 0, &[pose("s1", 3)]),
        Err(PoseRefusal::Internal)
    );
    assert_eq!(
        replace_segments(&mut document, "L1", 0, &[pose("s1", 1)]),
        Err(PoseRefusal::Internal)
    );
    // The frame selects the record: frame 1 is written, frame 0 is not.
    replace_segments(&mut document, "L1", 1, &[pose("s1", 2)]).unwrap();
    let wire = serde_json::to_value(&document).unwrap();
    assert_eq!(
        wire["objects"][0]["geometry"]["segments"][0]["point"]["x"],
        json!(1)
    );
    assert_eq!(
        wire["objects"][1]["geometry"]["segments"][0]["point"]["x"],
        json!(9)
    );
}

#[test]
fn the_decoder_accepts_the_two_actions_and_spellings_of_a_whole_frame() {
    let ok = |text: &str| parse_pose(text.as_bytes()).unwrap();
    let head =
        |action: &str| format!(r#"{{"command":"{COMMAND}","layerUid":"L1","action":{action}}}"#);
    assert_eq!(
        ok(&head(r#"{"kind":"reset"}"#)),
        ("L1".to_owned(), PoseAction::Reset)
    );
    for (frame, want) in [
        ("0", 0),
        ("-0", 0),
        ("3", 3),
        ("3.0", 3),
        ("4294967295", u32::MAX),
    ] {
        let action = format!(r#"{{"kind":"commit","frame":{frame}}}"#);
        assert_eq!(
            ok(&head(&action)).1,
            PoseAction::Commit { frame: want },
            "{frame}"
        );
    }
    for frame in ["3.5", "-1", "4294967296", "1e10", "\"3\"", "null", "true"] {
        let action = format!(r#"{{"kind":"commit","frame":{frame}}}"#);
        assert_eq!(
            parse_pose(head(&action).as_bytes()),
            Err(PoseRefusal::Malformed),
            "{frame}"
        );
    }
    // The member order of the payload is free, the member set is not.
    let shuffled =
        r#"{"action":{"frame":2,"kind":"commit"},"layerUid":"L1","command":"object.rig.pose"}"#;
    assert_eq!(
        parse_pose(shuffled.as_bytes()).unwrap().1,
        PoseAction::Commit { frame: 2 }
    );
}

#[test]
fn every_refusal_has_a_distinct_stable_reason_a_matching_message_and_the_documented_code() {
    let table = [
        (
            PoseRefusal::Malformed,
            "malformed_request",
            DispatchErrorCode::InvalidRequest,
        ),
        (
            PoseRefusal::LayerAbsent,
            "layer_absent",
            DispatchErrorCode::NotFound,
        ),
        (
            PoseRefusal::Locked,
            "layer_locked",
            DispatchErrorCode::Unavailable,
        ),
        (
            PoseRefusal::PathEffects,
            "path_effects_active",
            DispatchErrorCode::Unavailable,
        ),
        (
            PoseRefusal::PressureRibbon,
            "pressure_ribbon_unavailable",
            DispatchErrorCode::Unavailable,
        ),
        (
            PoseRefusal::NoRiggedStroke,
            "no_rigged_stroke",
            DispatchErrorCode::NotFound,
        ),
        (
            PoseRefusal::FrameOutOfRange,
            "frame_out_of_range",
            DispatchErrorCode::InvalidRequest,
        ),
        (
            PoseRefusal::KeyframeUnavailable,
            "keyframe_unavailable",
            DispatchErrorCode::Unavailable,
        ),
        (
            PoseRefusal::PoseUnavailable,
            "pose_unavailable",
            DispatchErrorCode::Unavailable,
        ),
        (
            PoseRefusal::NotExposed,
            "frame_not_exposed",
            DispatchErrorCode::NotFound,
        ),
        (
            PoseRefusal::DeformFailed,
            "deform_failed",
            DispatchErrorCode::InvalidRequest,
        ),
        (
            PoseRefusal::Internal,
            "internal",
            DispatchErrorCode::Internal,
        ),
    ];
    for (refusal, reason, code) in table {
        assert_eq!(refusal.reason(), reason);
        assert_eq!(refusal.code(), code, "{reason}");
        assert!(
            refusal.message().starts_with(&format!("{reason}: ")),
            "{reason}"
        );
    }
    let mut reasons: Vec<&str> = table.iter().map(|row| row.1).collect();
    reasons.sort_unstable();
    reasons.dedup();
    assert_eq!(reasons.len(), table.len());
}

#[test]
fn the_lock_check_is_the_layer_policy_not_a_second_reading_of_the_flag() {
    // `locked` absent reads as the baseline's false default; only an authored `true` locks.
    let free: Value = json!({"layerUid":"L1"});
    let open: Value = json!({"layerUid":"L1","locked":false});
    let shut: Value = json!({"layerUid":"L1","locked":true});
    for (layer, locked) in [(free, false), (open, false), (shut, true)] {
        let text = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":2,
            "layers":[layer],"objects":[]});
        let document = decode_storage_project(text.to_string().as_bytes()).unwrap();
        assert_eq!(document.layers()[0].locked(), locked);
    }
}
