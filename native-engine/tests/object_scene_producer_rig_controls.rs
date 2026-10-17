//! Negative and mutation controls of the rig / image-mesh producer (L07-L; I16a, G03a).

use super::packet_rows::{index_of, packet, snapshot};
use super::rows::{mesh_case, mesh_cases, stroke_case, MeshCase, ORACLE};
use super::tests::{item_of, mesh_of, only, reason_of};
use super::{produce_pose_output, PoseItem, PoseTarget, RigAvailability, PRODUCER};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    scene_payload, MeshView,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform::{
    apply_rig_deform, BindInput, BindPose, MeshBind, MeshTarget, PathBind,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::{
    DeformBone, DeformSegment,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{ControlWeight, WeightEntry};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinField, ProducerEntry, SceneJoinError, ScenePin,
};

fn real_verts(case: &MeshCase) -> Option<Vec<[f64; 2]>> {
    let snap = snapshot(&["a"], "rig-ctl");
    let pk = packet(&snap);
    let output = produce_pose_output(
        &pk,
        RigAvailability::Ready(&case.bones),
        &[case.target(index_of(&pk, "a"))],
    );
    match only(&output) {
        ProducerEntry::Item { payload, .. } => Some(mesh_of(payload).payload().verts.clone()),
        _ => None,
    }
}

/// The rig offsets the real L03-E kernel writes for a rigged case, `None` without a rig.
fn rig_offsets(case: &MeshCase) -> Option<Vec<[f64; 2]>> {
    if case.bones.is_empty() {
        return None;
    }
    let bind = MeshBind {
        rest: &case.rest,
        weights: &case.weights,
        rotate: false,
        target: Some(MeshTarget {
            verts: &case.verts,
            rect: case.rect,
        }),
    };
    let mut poses = apply_rig_deform(&case.bones, &[BindInput::Mesh(bind)]).expect("deform");
    match poses.pop() {
        Some(BindPose::Mesh(offsets)) => Some(offsets),
        other => panic!("not a mesh pose: {other:?}"),
    }
}

fn payload_verts(case: &MeshCase, offsets: &[[f64; 2]], pose: bool) -> Option<Vec<[f64; 2]>> {
    let view = MeshView {
        outline: &case.outline,
        verts: &case.verts,
        offsets,
        tris: &case.tris,
    };
    let pose = if pose { case.pose.as_deref() } else { None };
    scene_payload(&view, &case.rect, pose).map(|p| p.verts)
}

fn disagreements(wrong: impl Fn(&MeshCase) -> Option<Vec<[f64; 2]>>) -> usize {
    mesh_cases()
        .iter()
        .filter(|case| {
            let want = case.expected.as_ref().map(|e| e.verts.clone());
            wrong(*case) != want
        })
        .count()
}

#[test]
fn the_real_producer_agrees_with_the_oracle_where_the_wrong_ones_do_not() {
    let real = disagreements(real_verts);
    assert_eq!(real, 0);
    // The rig ignored: the stored offsets alone.
    let ignored = disagreements(|c| payload_verts(c, &c.offs, true));
    // The rig delta ADDED to the stored offsets instead of replacing the first n of them.
    let summed = disagreements(|c| {
        let mut offsets = c.offs.clone();
        offsets.resize(c.verts.len(), [0.0, 0.0]);
        if let Some(rig) = rig_offsets(c) {
            for (slot, delta) in offsets.iter_mut().zip(&rig) {
                *slot = [slot[0] + delta[0], slot[1] + delta[1]];
            }
        }
        payload_verts(c, &offsets, true)
    });
    // The animated pose dropped.
    let no_pose = disagreements(|c| {
        let mut offsets = c.offs.clone();
        offsets.resize(c.verts.len(), [0.0, 0.0]);
        if let Some(rig) = rig_offsets(c) {
            for (slot, delta) in offsets.iter_mut().zip(&rig) {
                *slot = *delta;
            }
        }
        payload_verts(c, &offsets, false)
    });
    assert_eq!((ignored, summed, no_pose), (8, 3, 1));
    assert!(ORACLE.contains(
        r#"# mutantMismatches {"rigIgnored":8,"rigSummedWithStored":3,"poseIgnored":1}"#
    ));
}

fn stroke_bind<'a>(rest: &'a [[f64; 2]], weights: &'a [Vec<WeightEntry>]) -> PathBind<'a> {
    PathBind {
        rest,
        rest_handles: None,
        weights,
        rotate: false,
        live_len: Some(1),
        center: None,
    }
}

fn broken_bone() -> Vec<DeformBone> {
    let mut bones = stroke_case("translate_with_rest_handles").bones;
    if let Some(live) = bones[0].segments.as_mut() {
        live[1] = DeformSegment {
            point: None,
            handle_in: None,
            handle_out: None,
        };
    }
    bones
}

#[test]
fn a_rig_failure_publishes_no_pose_for_any_rig_driven_target_and_spares_static_meshes() {
    let bones = broken_bone();
    let good_rest = [[320.0, 180.0]];
    let bad_rest = [[370.0, 180.0]];
    let no_weights: Vec<Vec<WeightEntry>> = vec![];
    let bad_weights = vec![vec![WeightEntry::Control(ControlWeight {
        bone_id: "b1".to_owned(),
        pt_index: 1,
        w: 1.0,
        stray_offset: None,
    })]];
    let static_mesh = mesh_case("rest_plain_stored_offsets");
    let snap = snapshot(&["a", "b", "c"], "rig-fail");
    let pk = packet(&snap);
    let (a, b, c) = (index_of(&pk, "a"), index_of(&pk, "b"), index_of(&pk, "c"));
    let output = produce_pose_output(
        &pk,
        RigAvailability::Ready(&bones),
        &[
            static_mesh.target(a),
            PoseTarget::RigStroke {
                source_object_index: b,
                bind: stroke_bind(&good_rest, &no_weights),
            },
            PoseTarget::RigStroke {
                source_object_index: c,
                bind: stroke_bind(&bad_rest, &bad_weights),
            },
        ],
    );
    assert_eq!(output.entries.len(), 3);
    assert!(matches!(item_of(&output.entries[0]), PoseItem::Mesh(_)));
    for entry in &output.entries[1..] {
        let reason = reason_of(entry);
        assert!(
            reason.starts_with("the rig deform failed at bind 1:"),
            "{reason}"
        );
        assert!(reason.contains("MissingPoint"), "{reason}");
    }
}

#[test]
fn an_unavailable_rig_is_a_typed_reason_for_rig_driven_targets_only() {
    let mesh = mesh_case("rig_centre_plus_20");
    let stat = mesh_case("rest_plain_stored_offsets");
    let snap = snapshot(&["a", "b"], "rig-none");
    let pk = packet(&snap);
    let (a, b) = (index_of(&pk, "a"), index_of(&pk, "b"));
    let output = produce_pose_output(
        &pk,
        RigAvailability::Unavailable("no rig record is available"),
        &[mesh.target(a), stat.target(b)],
    );
    assert_eq!(reason_of(&output.entries[0]), "no rig record is available");
    let kept = mesh_of(item_of(&output.entries[1]));
    let want = stat.expected.as_ref().expect("payload row");
    assert_eq!(kept.payload().verts, want.verts);
}

#[test]
fn an_output_from_another_snapshot_is_refused_by_the_registry() {
    let case = mesh_case("rest_plain_stored_offsets");
    let first = snapshot(&["a", "b"], "rig-first");
    let second = snapshot(&["a", "b"], "rig-second");
    let (pk1, pk2) = (packet(&first), packet(&second));
    let output = produce_pose_output(
        &pk1,
        RigAvailability::Unavailable("none"),
        &[case.target(index_of(&pk1, "a"))],
    );
    assert_eq!(output.pin, ScenePin::of_packet(&pk1));
    let refused = join_scene(&pk2, &ScenePin::of_packet(&pk2), vec![output]);
    assert_eq!(
        refused.err(),
        Some(SceneJoinError::WrongPin {
            producer: Some(PRODUCER.to_owned()),
            field: PinField::Instance,
        })
    );
}

#[test]
fn two_claims_on_one_record_are_refused_not_merged() {
    let case = mesh_case("rest_plain_stored_offsets");
    let snap = snapshot(&["a", "b"], "rig-dup");
    let pk = packet(&snap);
    let index = index_of(&pk, "a");
    let output = produce_pose_output(
        &pk,
        RigAvailability::Unavailable("none"),
        &[case.target(index), case.target(index)],
    );
    let refused = join_scene(&pk, &ScenePin::of_packet(&pk), vec![output]);
    assert_eq!(
        refused.err(),
        Some(SceneJoinError::DuplicateClaim {
            source_object_index: index
        })
    );
}

#[test]
fn the_output_is_detached_from_the_inputs_it_was_built_from() {
    let mut case = mesh_case("rig_centre_plus_20");
    let snap = snapshot(&["a"], "rig-detach");
    let pk = packet(&snap);
    let index = index_of(&pk, "a");
    let before = produce_pose_output(
        &pk,
        RigAvailability::Ready(&case.bones),
        &[case.target(index)],
    );
    let kept = before.entries.clone();
    case.offs[3] = [0.25, 0.25];
    case.verts[0] = [0.1, 0.1];
    let after = produce_pose_output(
        &pk,
        RigAvailability::Ready(&case.bones),
        &[case.target(index)],
    );
    assert_eq!(before.entries, kept);
    assert_ne!(after.entries, kept);
    let want = mesh_case("rig_centre_plus_20")
        .expected
        .expect("payload row");
    assert_eq!(mesh_of(item_of(&kept[0])).payload().verts, want.verts);
}

#[test]
fn the_producer_source_has_no_super_imports_and_no_document_or_js_writer() {
    let source = include_str!("../src/object_scene_producer_rig.rs");
    assert!(!source.contains("super::"));
    for forbidden in [
        "ObjectSnapshot",
        "ObjectDocument",
        "ObjectRecord",
        "paper",
        "Paper",
    ] {
        assert!(
            !source.contains(forbidden),
            "the producer must not name {forbidden}"
        );
    }
}
