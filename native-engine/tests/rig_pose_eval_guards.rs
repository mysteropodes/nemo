//! Guards of the rig pose evaluation join (L03-P, I12): the output is detached from the snapshot, the rig and
//! the mesh port; evaluation is deterministic and pinned to one frame; held frames select the authored key; scene
//! and export receive the same single evaluation.
use super::rows::*;
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
    evaluate_and_deliver, evaluate_rig_pose, BindOutcome, PosedGeometrySink, RigPoseEvaluation,
    RigPosePin, RigPoseRefusal,
};
use serde_json::{json, Value};

const ARM: &str = "arm_rotate_soft_bone_turned_40_degrees";

#[test]
fn the_output_is_detached_so_mutating_it_cannot_change_the_rest_geometry() {
    let row = row(ARM);
    let snapshot = snapshot_of(&row_document(&row));
    let provider = row_provider(&row);
    let before_document = snapshot.document().clone();
    let before_mesh = provider.meshes.clone();
    let pin = RigPosePin::of(&snapshot);
    let mut first = evaluate_rig_pose(&snapshot, &pin, LAYER, 0, Some(&provider)).unwrap();
    let pristine = first.clone();
    for stroke in &mut first.strokes {
        for segment in &mut stroke.segments {
            segment.point = [f64::MAX, f64::MIN];
            segment.handle_in = [1.0e9, 1.0e9];
            segment.handle_out = [-1.0e9, -1.0e9];
        }
        stroke.segments.clear();
    }
    first.binds.clear();
    first.meshes.clear();
    let second = evaluate_rig_pose(&snapshot, &pin, LAYER, 0, Some(&provider)).unwrap();
    assert_eq!(
        second, pristine,
        "a mutated result changed what the next evaluation returns"
    );
    assert_eq!(
        snapshot.document(),
        &before_document,
        "the snapshot document was written through"
    );
    assert_eq!(provider.meshes.len(), before_mesh.len());
    assert!(
        provider.meshes.iter().all(|(id, m)| before_mesh[id] == *m),
        "the mesh port was written through"
    );
}

#[test]
fn the_mesh_result_is_a_copy_of_the_stored_offsets_not_a_view() {
    let row = row("mesh_plain_rect_stored_offsets_overwritten");
    let snapshot = snapshot_of(&row_document(&row));
    let mut provider = row_provider(&row);
    let pin = RigPosePin::of(&snapshot);
    let eval = evaluate_rig_pose(&snapshot, &pin, LAYER, 0, Some(&provider)).unwrap();
    let copy = eval.meshes[0].offsets.clone();
    // editing the store afterwards must not reach the already returned result
    for o in &mut provider.meshes.get_mut("im_1").unwrap().offsets {
        *o = [9.0, 9.0];
    }
    assert_eq!(eval.meshes[0].offsets, copy);
    assert_ne!(copy, provider.meshes["im_1"].offsets);
}

fn bits(eval: &RigPoseEvaluation) -> String {
    // Debug prints the shortest text that round-trips the bits, so equal text is equal bits
    // (NaN never occurs in an admitted rig).
    format!("{eval:?}")
}

#[test]
fn two_evaluations_of_the_same_snapshot_are_bit_identical() {
    for row in rows().into_iter().filter(|r| r.expect["throws"] == false) {
        let snapshot = snapshot_of(&row_document(&row));
        let provider = row_provider(&row);
        let pin = RigPosePin::of(&snapshot);
        let a = evaluate_rig_pose(&snapshot, &pin, LAYER, 0, Some(&provider)).unwrap();
        let b = evaluate_rig_pose(&snapshot, &pin, LAYER, 0, Some(&provider)).unwrap();
        assert_eq!(a, b, "{}", row.name);
        assert_eq!(bits(&a), bits(&b), "{}", row.name);
    }
}

#[test]
fn the_rest_pose_gives_the_stored_geometry_back() {
    let row = row("ik_reset_pose_gives_the_stored_geometry_back");
    let eval = evaluate_row(&row).unwrap();
    let stored = &row.strokes[0]["segs"];
    let got = &eval.strokes[0].segments;
    assert_eq!(got.len(), 4);
    for (i, segment) in got.iter().enumerate() {
        let want = &stored[i];
        let have = [
            segment.point[0],
            segment.point[1],
            segment.handle_in[0],
            segment.handle_in[1],
            segment.handle_out[0],
            segment.handle_out[1],
        ];
        for k in 0..6 {
            assert!(
                close(have[k], num(&want[k])),
                "segment {i} field {k}: {} vs {}",
                have[k],
                num(&want[k])
            );
        }
    }
}

// ---- frames and timing ------------------------------------------------------------------------------------

fn timed_document(row: &Row, keys: Value) -> Value {
    let mut doc = row_document(row);
    doc["layers"][0]["timing"] = json!({
        "format": "nemo.native-layer-timing",
        "formatVersion": 1,
        "family": "drawing",
        "contextId": "scene-root",
        "keys": keys,
        "samples": []
    });
    doc
}

#[test]
fn a_held_frame_reads_the_strokes_of_its_authored_key() {
    let row = row(ARM);
    let snapshot = snapshot_of(&timed_document(&row, json!([{"frame": 0}, {"frame": 2}])));
    let pin = RigPosePin::of(&snapshot);
    let at_key = evaluate_rig_pose(&snapshot, &pin, LAYER, 0, None).unwrap();
    let held = evaluate_rig_pose(&snapshot, &pin, LAYER, 1, None).unwrap();
    assert_eq!((at_key.frame, at_key.source_frame), (0, 0));
    assert_eq!((held.frame, held.source_frame), (1, 0));
    assert_eq!(held.binds, vec![BindOutcome::Posed]);
    assert_eq!(
        held.strokes, at_key.strokes,
        "a held frame shows the key's posed strokes"
    );
    // frame 2 is a key of its own with no authored stroke: the bind is reported, not silently posed
    let next = evaluate_rig_pose(&snapshot, &pin, LAYER, 2, None).unwrap();
    assert_eq!(next.source_frame, 2);
    assert!(matches!(next.binds[0], BindOutcome::Unlinked(_)));
    assert!(next.strokes.is_empty());
}

#[test]
fn a_frame_before_the_first_key_is_not_exposed() {
    let row = row(ARM);
    let snapshot = snapshot_of(&timed_document(&row, json!([{"frame": 1}])));
    let pin = RigPosePin::of(&snapshot);
    assert_eq!(
        evaluate_rig_pose(&snapshot, &pin, LAYER, 0, None),
        Err(RigPoseRefusal::NotExposed)
    );
}

#[test]
fn without_layer_timing_only_the_authored_frame_itself_is_exposed() {
    let row = row(ARM);
    let snapshot = snapshot_of(&row_document(&row));
    let pin = RigPosePin::of(&snapshot);
    let later = evaluate_rig_pose(&snapshot, &pin, LAYER, 1, None).unwrap();
    assert_eq!(later.source_frame, 1);
    assert!(
        matches!(later.binds[0], BindOutcome::Unlinked(_)),
        "no record is authored at frame 1"
    );
}

// ---- one evaluation, two consumers ------------------------------------------------------------------------

#[derive(Default)]
struct Recorder {
    seen: Vec<(usize, RigPoseEvaluation)>,
}

impl PosedGeometrySink for Recorder {
    fn accept(&mut self, evaluation: &RigPoseEvaluation) {
        self.seen.push((
            evaluation as *const RigPoseEvaluation as usize,
            evaluation.clone(),
        ));
    }
}

#[test]
fn scene_and_export_receive_the_same_single_evaluation() {
    let row = row("mixed_path_mesh_and_binds_relink_drops");
    let snapshot = snapshot_of(&row_document(&row));
    let provider = row_provider(&row);
    let pin = RigPosePin::of(&snapshot);
    let (mut scene, mut export) = (Recorder::default(), Recorder::default());
    let returned = evaluate_and_deliver(
        &snapshot,
        &pin,
        LAYER,
        0,
        Some(&provider),
        &mut scene,
        &mut export,
    )
    .unwrap();
    assert_eq!(
        (scene.seen.len(), export.seen.len()),
        (1, 1),
        "each consumer is called exactly once"
    );
    assert_eq!(
        scene.seen[0].0, export.seen[0].0,
        "both consumers saw the same value, not two evaluations"
    );
    assert_eq!(scene.seen[0].1, export.seen[0].1);
    assert_eq!(scene.seen[0].1, returned);
    // the geometry both saw is the baseline geometry
    let provider = row_provider(&row);
    compare(&row, &provider, &scene.seen[0].1).unwrap();
    compare(&row, &provider, &export.seen[0].1).unwrap();
    // and the mesh feed of both consumers is the one baseline scenePayloadFor payload
    let mesh = &export.seen[0].1.meshes[0];
    assert_eq!(
        mesh.scene_payload(&provider.meshes["im_1"]),
        scene.seen[0].1.meshes[0].scene_payload(&provider.meshes["im_1"])
    );
}

#[test]
fn a_refusal_reaches_neither_consumer() {
    let arm = row(ARM);
    let snapshot = snapshot_of(&row_document(&arm));
    let other = snapshot_of(&row_document(&arm));
    let (mut scene, mut export) = (Recorder::default(), Recorder::default());
    let refused = evaluate_and_deliver(
        &snapshot,
        &RigPosePin::of(&other),
        LAYER,
        0,
        None,
        &mut scene,
        &mut export,
    );
    assert_eq!(refused, Err(RigPoseRefusal::WrongDocument));
    assert!(scene.seen.is_empty() && export.seen.is_empty());
    let throwing = row("offset_entry_naming_a_missing_bone_throws");
    let thrower = snapshot_of(&row_document(&throwing));
    let thrown = evaluate_and_deliver(
        &thrower,
        &RigPosePin::of(&thrower),
        LAYER,
        0,
        None,
        &mut scene,
        &mut export,
    );
    assert!(matches!(thrown, Err(RigPoseRefusal::Deform { .. })));
    assert!(scene.seen.is_empty() && export.seen.is_empty());
}
