//! Pose, IK, memory-only-field and older-save tests of the native rig record (L03-N, I11), checked
//! against the executed baseline rows (`rig`, `ikset` and `hand`) and hand-derived geometry.

use super::oracle_rows::{hand_row, ikset_rows, layer, rel_rows, rig_row, rig_rows, v1};
use super::{BindTarget, IkChainRecord, KeyedTable, RigPair, RigRecord};
use crate::object_codec::{decode_project, ObjectCodecErrorKind};
use crate::object_document::Presence;

fn rig(text: &str) -> RigRecord {
    serde_json::from_str(text).expect("rig decodes")
}

fn reencode(record: &RigRecord) -> String {
    serde_json::to_string(record).expect("rig encodes")
}

fn pair(text: &str) -> RigPair {
    serde_json::from_str(text).expect("pair decodes")
}

#[test]
fn ik_chains_have_the_assigned_literal_shape_and_the_end_key() {
    for (name, flip) in [("ik_chain_rest", false), ("ik_chain_flip", true)] {
        let record = rig(rig_row(name).export);
        assert_eq!(record.ik_chains.len(), 1, "{name}");
        let (key, chain) = &record.ik_chains.entries()[0];
        assert_eq!(key, "bone2:1", "{name}");
        assert_eq!(*key, chain.expected_key());
        // upper bone (0,0)-(100,0), lower bone (100,0)-(180,0): l1 = |joint-root| = 100, l2 = 80
        assert_eq!((chain.root.bone_id.as_str(), chain.root.vi), ("bone1", 0));
        assert_eq!((chain.joint.bone_id.as_str(), chain.joint.vi), ("bone1", 1));
        assert_eq!((chain.end.bone_id.as_str(), chain.end.vi), ("bone2", 1));
        assert_eq!(
            (chain.l1.as_f64(), chain.l2.as_f64(), chain.flip),
            (Some(100.0), Some(80.0), flip)
        );
    }
}

#[test]
fn ik_set_rows_pin_the_refusals_and_the_boolean_flip() {
    let rows = ikset_rows();
    let by = |name: &str| rows.iter().find(|r| r.name == name).unwrap();
    // rigSetIK stores nothing when an anchor is missing: the table is still {}
    for name in ["missing_end_bone", "vertex_beyond_rest"] {
        assert!(!by(name).returned, "{name}");
        assert_eq!(by(name).chains, "{}", "{name}");
    }
    // a truthy flip is stored as true; natively flip is a boolean on the wire, so the number 1 is refused
    let row = by("flip_is_coerced_to_boolean");
    assert!(row.returned);
    let table: KeyedTable<IkChainRecord> = serde_json::from_str(row.chains).unwrap();
    assert_eq!(table.entries().len(), 1);
    let chain = &table.entries()[0].1;
    assert!(chain.flip);
    assert_eq!(
        (chain.l1.as_f64(), chain.l2.as_f64()),
        (Some(100.0), Some(0.0))
    );
    assert!(serde_json::from_str::<KeyedTable<IkChainRecord>>(
        &row.chains.replace("\"flip\":true", "\"flip\":1")
    )
    .is_err());
}

#[test]
fn posed_bones_keep_the_live_pose_apart_from_the_rest_pose() {
    let posed = rig(rig_row("ik_posed_segments_differ_from_rest").export);
    let joint = posed.bone("bone1").unwrap().segments[1].point_value();
    // law of cosines for target (120,90) from root (0,0), l1=100, l2=80: the distance 150 is reachable,
    // cosA = (100^2 + 150^2 - 80^2) / (2*100*150) = 0.87, joint = l1 at angle atan2(90,120) + acos(cosA)
    let angle = 90f64.atan2(120.0) + (26100.0f64 / 30000.0).acos();
    assert!(
        (joint[0] - 100.0 * angle.cos()).abs() < 1e-9
            && (joint[1] - 100.0 * angle.sin()).abs() < 1e-9
    );
    assert_eq!(
        posed.bone("bone1").unwrap().rest_segments[1].point,
        pair("[100,0]")
    );
    assert_ne!(
        posed.bone("bone1").unwrap().segments,
        posed.bone("bone1").unwrap().rest_segments
    );
    // rig-bridge.js mirrors the solved joint onto the end bone's coincident near end
    assert_eq!(
        posed.bone("bone2").unwrap().segments[0].point,
        posed.bone("bone1").unwrap().segments[1].point
    );
    // the end sits where the drag put it
    assert_eq!(
        posed.bone("bone2").unwrap().segments[1].point,
        pair("[120,90]")
    );
    // rigResetPose restores every bone's segments from its restSegments
    let reset = rig(rig_row("ik_pose_reset_segments_equal_rest").export);
    assert!(reset
        .bones
        .entries()
        .iter()
        .all(|(_, b)| b.segments == b.rest_segments));
    // beyond reach the end stops 1e-6 short of l1 + l2 along the base angle
    let clamped = rig(rig_row("ik_drag_beyond_reach_is_clamped").export);
    let end = clamped.bone("bone2").unwrap().segments[1].point_value();
    assert!((end[0].hypot(end[1]) - (180.0 - 1e-6)).abs() < 1e-9 && end[1] == 0.0);
    // a hand pose of point and handles is data on the live pose only
    let hand = rig(rig_row("posed_point_and_handles").export);
    let bone = hand.bone("bone1").unwrap();
    assert_eq!(bone.segments[1].point, pair("[100,30]"));
    assert_eq!(
        bone.segments[1].handle_in,
        Presence::Present(Some(pair("[-15,-5]")))
    );
    assert_eq!(bone.rest_segments[1].point, pair("[100,0]"));
}

#[test]
fn memory_only_fields_are_refused_by_name_and_never_silently_dropped() {
    let row = rig_row("vector_brush_center_data_is_memory_only");
    assert!(row.memory.contains("\"centerRest\"") && row.memory.contains("\"centerWeights\""));
    assert!(!row.export.contains("centerRest") && !row.export.contains("centerWeights"));
    let error = serde_json::from_str::<RigRecord>(row.memory)
        .expect_err("memory-only members")
        .to_string();
    assert!(error.contains("unknown field `centerRest`"), "{error}");
    let live = hand_row("live_references_never_reach_the_whitelist");
    assert!(serde_json::from_str::<RigRecord>(live.input).is_err());
    let exported = rig(live.export);
    assert_eq!(exported.validate(), Ok(()));
    assert_eq!(reencode(&exported), live.export);
    // through the codec an unknown member is Unsupported, never a quiet drop
    let text = v1(&[layer("R", row.memory)], &[]);
    let kind = decode_project(text.as_bytes()).expect_err("refused").kind();
    assert_eq!(kind, ObjectCodecErrorKind::Unsupported);
}

#[test]
fn older_save_shapes_decode_and_the_documented_narrowings_hold() {
    let old = rig(hand_row("old_rig_without_rest_handles_or_radius").export);
    assert_eq!(old.validate(), Ok(()));
    assert!(old.binds[0].rest_handles.missing() && old.bone("bone1").unwrap().radius.missing());
    assert_eq!(
        reencode(&old),
        hand_row("old_rig_without_rest_handles_or_radius").export
    );
    let mesh_only = rig(hand_row("bind_with_undefined_optionals_is_dropped_by_json").export);
    assert!(mesh_only.binds[0].stroke_id.missing() && mesh_only.binds[0].rest_handles.missing());
    assert_eq!(mesh_only.validate(), Ok(()));
    // exportJSON writes binds:[] for a rig that lost the array; the in-memory shape without it is refused
    let lost = hand_row("rig_without_binds_array_exports_empty_list");
    assert!(serde_json::from_str::<RigRecord>(lost.input).is_err());
    assert_eq!(rig(lost.export).binds.len(), 0);
    // the baseline whitelist copies bones wholesale, so a foreign bone member survives it; native refuses
    let foreign = hand_row("bones_with_foreign_member_pass_through_the_whitelist");
    assert!(foreign.export.contains("\"label\":\"x\""));
    assert!(serde_json::from_str::<RigRecord>(foreign.export).is_err());
    // JS orders integer-like keys first; the Rig tool only makes boneN ids, and native keeps authored order
    let order = rig(hand_row("numeric_bone_key_order_is_integer_first").export);
    let ids: Vec<&str> = order
        .bones
        .entries()
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(ids, ["7", "b"]);
    assert_eq!(order.validate(), Ok(()));
}

#[test]
fn authored_states_and_number_spelling_survive_the_wire() {
    let text = r#"{"bones":{"bone1":{"segments":[{"point":[0,0]},{"point":[1.0,0.5],"handleIn":null,"handleOut":[0,0]}],"restSegments":[{"point":[0,0]},{"point":[1,0.5]}],"closed":false,"radius":0}},"ikChains":{},"nextId":2,"binds":[]}"#;
    let record = rig(text);
    assert_eq!(
        reencode(&record),
        text,
        "absent, null, explicit zero and 1.0 vs 1 are all kept"
    );
    let bone = record.bone("bone1").unwrap();
    assert!(bone.segments[0].handle_in.missing() && bone.segments[0].handle_out.missing());
    assert_eq!(bone.segments[1].handle_in, Presence::Present(None));
    assert_eq!(
        bone.segments[1].handle_out,
        Presence::Present(Some(pair("[0,0]")))
    );
    assert_eq!(bone.radius_value(), Some(0.0));
    assert_ne!(
        bone.segments[1].point, bone.rest_segments[1].point,
        "1.0 and 1 are different spellings"
    );
    let absent = rig(&text.replace(",\"radius\":0", ""));
    assert_ne!(absent, record, "radius 0 is not an absent radius");
}

#[test]
fn the_oracle_is_not_vacuous() {
    let rigs: Vec<RigRecord> = rig_rows().iter().map(|r| rig(r.export)).collect();
    let binds: Vec<_> = rigs.iter().flat_map(|r| r.binds.iter()).collect();
    assert!(binds
        .iter()
        .any(|b| matches!(b.target(), Some(BindTarget::Stroke(_)))));
    assert!(binds
        .iter()
        .any(|b| matches!(b.target(), Some(BindTarget::Mesh(_)))));
    assert!(binds.iter().any(|b| b.rotate) && binds.iter().any(|b| !b.rotate));
    let entries: Vec<_> = binds
        .iter()
        .flat_map(|b| b.weights.iter().flatten())
        .collect();
    assert!(entries.iter().any(|w| w.pt_index.value().is_some()));
    assert!(entries
        .iter()
        .any(|w| w.pt_index.missing() && w.offset.value().is_some()));
    let chains: Vec<_> = rigs
        .iter()
        .flat_map(|r| r.ik_chains.entries().iter())
        .collect();
    assert!(chains.iter().any(|(_, c)| c.flip) && chains.iter().any(|(_, c)| !c.flip));
    let bones: Vec<_> = rigs.iter().flat_map(|r| r.bones.entries().iter()).collect();
    assert!(
        bones.iter().any(|(_, b)| b.radius.value().is_some())
            && bones.iter().any(|(_, b)| b.radius.missing())
    );
    assert!(
        bones.iter().any(|(_, b)| b.closed)
            && bones.iter().any(|(_, b)| b.segments != b.rest_segments)
    );
    assert!(binds.iter().any(|b| b
        .rest_handles
        .value()
        .is_some_and(|h| h.iter().any(|s| s.o.value().is_some_and(Option::is_some)))));
    assert!(
        rel_rows().iter().any(|r| r.kept.contains(&true))
            && rel_rows().iter().any(|r| r.kept.contains(&false))
    );
}
