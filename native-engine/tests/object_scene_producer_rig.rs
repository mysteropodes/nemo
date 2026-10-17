//! Rig / image-mesh producer against the real baseline (L07-L; I16a, G03a).
//!
//! Every expected number comes from `fixtures/object-scene-producer-rig/oracle.txt`
//! (verbatim `applyRigDeform` + `scenePayloadFor` at fbe4b8a3, see `rows`). The producer
//! runs the real L03-E deform kernel and the real L03-A payload; scene and export read
//! one `Arc<EvaluatedMesh>` through the L03-H draw plan; the registry orders the output.

use super::packet_rows::{index_of, packet, snapshot};
use super::rows::{mesh_case, mesh_cases, stroke_case, stroke_cases, ORACLE};
use super::{
    produce_pose_output, EvaluatedMesh, PoseItem, RigAvailability, PRODUCER, REASON_NO_TRIANGLES,
    REASON_STROKE_UNPOSED,
};
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_plan::PlanSkip;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerEntry, ProducerOutput, SceneOutcome, ScenePin, UnavailableCause,
};
use std::sync::Arc;

pub(super) fn only(output: &ProducerOutput<PoseItem>) -> &ProducerEntry<PoseItem> {
    assert_eq!(output.entries.len(), 1, "one entry per target");
    &output.entries[0]
}

pub(super) fn item_of(entry: &ProducerEntry<PoseItem>) -> &PoseItem {
    match entry {
        ProducerEntry::Item { payload, .. } => payload,
        other => panic!("expected an item, got {other:?}"),
    }
}

pub(super) fn reason_of(entry: &ProducerEntry<PoseItem>) -> &str {
    match entry {
        ProducerEntry::Unavailable { reason, .. } => reason,
        other => panic!("expected an unavailable entry, got {other:?}"),
    }
}

pub(super) fn mesh_of(item: &PoseItem) -> &Arc<EvaluatedMesh> {
    match item {
        PoseItem::Mesh(mesh) => mesh,
        other => panic!("expected a mesh, got {other:?}"),
    }
}

#[test]
fn oracle_header_and_row_counts_are_pinned() {
    assert!(ORACLE.contains("at fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.contains(r#"# counts {"MESH":12,"STROKE":5,"rotatedRectCases":1}"#));
    assert!(ORACLE.contains(
        r#"# mutantMismatches {"rigIgnored":8,"rigSummedWithStored":3,"poseIgnored":1}"#
    ));
    let meshes = mesh_cases();
    let strokes = stroke_cases();
    assert_eq!((meshes.len(), strokes.len()), (12, 5));
    assert_eq!(meshes.iter().filter(|c| c.expected.is_none()).count(), 1);
    assert_eq!(meshes.iter().filter(|c| !c.bones.is_empty()).count(), 8);
    assert_eq!(meshes.iter().filter(|c| c.pose.is_some()).count(), 1);
    assert_eq!(strokes.iter().filter(|c| c.expected.is_none()).count(), 1);
}

#[test]
fn every_mesh_row_reproduces_the_baseline_payload_through_the_producer() {
    let snap = snapshot(&["a"], "rig-mesh");
    let pk = packet(&snap);
    let index = index_of(&pk, "a");
    let mut compared = 0;
    for case in mesh_cases() {
        let bones = case.bones.clone();
        let output =
            produce_pose_output(&pk, RigAvailability::Ready(&bones), &[case.target(index)]);
        let entry = only(&output);
        match &case.expected {
            None => {
                assert_eq!(reason_of(entry), REASON_NO_TRIANGLES, "{}", case.name);
                assert!(matches!(
                    entry,
                    ProducerEntry::Unavailable { source_object_index, .. } if *source_object_index == index
                ));
            }
            Some(want) => {
                let mesh = mesh_of(item_of(entry));
                let got = mesh.payload();
                assert_eq!(got.verts, want.verts, "{}", case.name);
                assert_eq!(got.uvs, want.uvs, "{}", case.name);
                assert_eq!(got.tris, want.tris, "{}", case.name);
                assert_eq!(got.outline.len(), want.outline, "{}", case.name);
                assert_eq!(mesh.image_size(), case.img, "{}", case.name);
                assert_eq!(mesh.opacity(), case.op, "{}", case.name);
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 11);
}

#[test]
fn every_stroke_row_reproduces_the_baseline_pose_through_the_producer() {
    let snap = snapshot(&["a"], "rig-stroke");
    let pk = packet(&snap);
    let index = index_of(&pk, "a");
    let mut compared = 0;
    for case in stroke_cases() {
        let output = produce_pose_output(
            &pk,
            RigAvailability::Ready(&case.bones),
            &[case.target(index)],
        );
        let entry = only(&output);
        match &case.expected {
            None => assert_eq!(reason_of(entry), REASON_STROKE_UNPOSED, "{}", case.name),
            Some(want) => {
                let PoseItem::Stroke(pose) = item_of(entry) else {
                    panic!("{}: not a stroke", case.name);
                };
                assert_eq!(pose.segments.len(), want.len(), "{}", case.name);
                assert!(pose.center.is_none(), "{}", case.name);
                for (got, exp) in pose.segments.iter().zip(want) {
                    assert_eq!(got.point, exp.point, "{}", case.name);
                    assert_eq!(got.handle_in, exp.hi, "{}", case.name);
                    assert_eq!(got.handle_out, exp.ho, "{}", case.name);
                }
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 4);
}

#[test]
fn scene_and_export_read_one_evaluated_mesh_and_one_plan_geometry() {
    let case = mesh_case("rig_centre_plus_20");
    let snap = snapshot(&["a", "b", "c", "d"], "rig-share");
    let pk = packet(&snap);
    let index = index_of(&pk, "b");
    let output = produce_pose_output(
        &pk,
        RigAvailability::Ready(&case.bones),
        &[case.target(index)],
    );
    let scene = join_scene(&pk, &ScenePin::of_packet(&pk), vec![output]).expect("join");
    let entry = scene
        .entries()
        .iter()
        .find(|e| e.source_object_index() == index)
        .expect("the claimed record is in the scene");
    let SceneOutcome::Item { payload, producer } = entry.outcome() else {
        panic!("the claimed record must be an item");
    };
    assert_eq!(producer, PRODUCER);
    let for_scene = Arc::clone(mesh_of(payload));
    let for_export = Arc::clone(mesh_of(payload));
    assert!(Arc::ptr_eq(&for_scene, &for_export));
    // The centre vertex moved by exactly the rig's 20 px (baseline world 440,280).
    assert_eq!(for_scene.payload().verts[4], [440.0, 280.0]);
    let screen = for_scene
        .draw_plan(&[2.0, 0.0, 0.0, 2.0, 10.0, 20.0])
        .expect("plan");
    let export = for_export
        .draw_plan(&[1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
        .expect("plan");
    assert_eq!(export.triangles.len(), 8);
    assert_eq!(
        export.triangles[0].dest,
        [[320.0, 180.0], [420.0, 180.0], [440.0, 280.0]]
    );
    assert_eq!(
        export.triangles[0].source,
        [[0.0, 0.0], [500.0, 0.0], [500.0, 250.0]]
    );
    assert_eq!(screen.opacity, 0.5f32);
    assert_eq!(export.opacity, 0.5f32);
    assert_eq!(screen.outline, export.outline);
    assert_eq!(screen.triangles.len(), export.triangles.len());
    for (a, b) in screen.triangles.iter().zip(&export.triangles) {
        assert_eq!(
            (a.triangle, a.source, a.dest),
            (b.triangle, b.source, b.dest)
        );
        assert_eq!(a.place, b.place);
    }
    // Only the half-pixel seam dilation follows the view.
    assert!(screen
        .triangles
        .iter()
        .zip(&export.triangles)
        .any(|(a, b)| a.clip != b.clip));
}

#[test]
fn the_registry_orders_rig_and_mesh_items_by_the_packet_and_keeps_unavailable_entries() {
    let mesh = mesh_case("rig_centre_plus_20");
    let moved = stroke_case("translate_with_rest_handles");
    let skipped = stroke_case("skipped_without_live_stroke");
    assert_eq!(mesh.bones_tok, moved.bones_tok);
    assert_eq!(mesh.bones_tok, skipped.bones_tok);
    let snap = snapshot(&["a", "b", "c", "d"], "rig-order");
    let pk = packet(&snap);
    let (a, b, c, d) = (
        index_of(&pk, "a"),
        index_of(&pk, "b"),
        index_of(&pk, "c"),
        index_of(&pk, "d"),
    );
    // Targets are handed over out of paint order; "b" is claimed by nobody.
    let output = produce_pose_output(
        &pk,
        RigAvailability::Ready(&mesh.bones),
        &[skipped.target(d), moved.target(c), mesh.target(a)],
    );
    assert_eq!(output.entries.len(), 3);
    let joined = join_scene(&pk, &ScenePin::of_packet(&pk), vec![output]).expect("join");
    assert_eq!(joined.pin(), &ScenePin::of_packet(&pk));
    assert!(joined.declared_resource_versions().is_empty());
    let order: Vec<usize> = joined
        .entries()
        .iter()
        .map(|e| e.source_object_index())
        .collect();
    assert_eq!(order, vec![a, b, c, d]);
    let outcomes: Vec<&SceneOutcome<PoseItem>> =
        joined.entries().iter().map(|e| e.outcome()).collect();
    let SceneOutcome::Item { payload, .. } = outcomes[0] else {
        panic!("a must be an item");
    };
    let want = mesh.expected.as_ref().expect("payload row");
    assert_eq!(mesh_of(payload).payload().verts, want.verts);
    assert_eq!(
        outcomes[1],
        &SceneOutcome::Unavailable(UnavailableCause::Unclaimed)
    );
    let SceneOutcome::Item { payload, .. } = outcomes[2] else {
        panic!("c must be an item");
    };
    let PoseItem::Stroke(pose) = payload else {
        panic!("c must be a stroke");
    };
    let want = moved.expected.as_ref().expect("stroke row");
    assert_eq!(pose.segments[0].point, want[0].point);
    assert_eq!(pose.segments[1].handle_in, want[1].hi);
    assert_eq!(
        outcomes[3],
        &SceneOutcome::Unavailable(UnavailableCause::Reported {
            producer: PRODUCER.to_owned(),
            reason: REASON_STROKE_UNPOSED.to_owned(),
        })
    );
    assert!(!joined.is_complete());
}

#[test]
fn an_image_that_draws_nothing_is_a_typed_plan_skip_not_a_silent_item() {
    let mut case = mesh_case("rest_plain_stored_offsets");
    case.img = (0, 0);
    let snap = snapshot(&["a"], "rig-empty");
    let pk = packet(&snap);
    let output = produce_pose_output(
        &pk,
        RigAvailability::Unavailable("no rig"),
        &[case.target(index_of(&pk, "a"))],
    );
    let mesh = mesh_of(item_of(only(&output)));
    assert_eq!(
        mesh.draw_plan(&[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        Err(PlanSkip::EmptyImage)
    );
}
