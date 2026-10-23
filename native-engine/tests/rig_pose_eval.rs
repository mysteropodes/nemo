//! Rig pose evaluation join (L03-P, I12): the executed baseline oracle, an independent hand fixture and the
//! typed refusal / unlinked / unavailable classes. Guards live in `rig_pose_eval_guards.rs`, negative controls in
//! `rig_pose_eval_controls.rs`.
use super::rows::*;
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::MeshRect;
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::DeformError;
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
    evaluate_rig_pose, BindOutcome, MeshSnapshot, PoseUnavailable, RigPosePin, RigPoseRefusal,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::AnchorError;
use crate::object_document::object_rig_record::BindLink;
use serde_json::{json, Value};
use std::collections::HashMap;

#[test]
fn oracle_row_counts_and_outcome_classes_are_pinned() {
    let all = rows();
    assert_eq!(all.len(), 17);
    let names: std::collections::HashSet<&str> = all.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names.len(), 17, "row names are unique");
    let throwing = all.iter().filter(|r| r.expect["throws"] == true).count();
    assert_eq!(throwing, 1);
    let dropped: usize = all
        .iter()
        .map(|r| {
            r.expect["kept"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|k| **k == false)
                .count()
        })
        .sum();
    assert_eq!(dropped, 3, "binds relinkRigBinds drops");
    let with_mesh = all
        .iter()
        .filter(|r| !r.meshes.as_object().unwrap().is_empty())
        .count();
    assert_eq!(with_mesh, 5);
    let mesh_binds: usize = all
        .iter()
        .map(|r| {
            r.rig["binds"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b.get("meshId").is_some())
                .count()
        })
        .sum();
    assert_eq!(mesh_binds, 5);
    let open = all
        .iter()
        .flat_map(|r| r.strokes.iter())
        .filter(|s| s["closed"] == false)
        .count();
    assert_eq!(open, 1, "one open (vector family) stroke");
    let legacy_entries: usize = all
        .iter()
        .flat_map(|r| r.rig["binds"].as_array().unwrap().iter())
        .flat_map(|b| b["weights"].as_array().unwrap().iter())
        .flat_map(|w| w.as_array().unwrap().iter())
        .filter(|e| e.get("ptIndex").is_none())
        .count();
    assert_eq!(legacy_entries, 8, "offset-model weight entries");
    let with_ik = all
        .iter()
        .filter(|r| !r.rig["ikChains"].as_object().unwrap().is_empty())
        .count();
    assert_eq!(with_ik, 2);
}

#[test]
fn every_oracle_row_matches_the_executed_baseline() {
    for row in rows() {
        let provider = row_provider(&row);
        let result = evaluate_with(&row, Some(&provider));
        if row.expect["throws"] == true {
            assert!(
                matches!(result, Err(RigPoseRefusal::Deform { .. })),
                "{}: the baseline throws, got {result:?}",
                row.name
            );
            continue;
        }
        let eval = result.unwrap_or_else(|e| panic!("{}: refused {e:?}", row.name));
        if let Err(why) = compare(&row, &provider, &eval) {
            panic!("{}: {why}", row.name);
        }
    }
}

#[test]
fn the_baseline_type_error_is_a_typed_refusal_with_the_record_bind_index_and_no_geometry() {
    let row = row("offset_entry_naming_a_missing_bone_throws");
    assert_eq!(
        evaluate_row(&row),
        Err(RigPoseRefusal::Deform {
            bind_index: 0,
            error: DeformError::Anchor(AnchorError::OffsetBoneMissing),
        })
    );
}

#[test]
fn an_empty_rig_evaluates_to_nothing_and_a_layer_without_a_rig_is_refused() {
    let eval = evaluate_row(&row("empty_rig_has_no_binds")).unwrap();
    assert!(eval.binds.is_empty() && eval.strokes.is_empty() && eval.meshes.is_empty());
    let doc = document(vec![json!({"layerUid": "L1"})], vec![]);
    let snapshot = snapshot_of(&doc);
    assert_eq!(
        evaluate_rig_pose(&snapshot, &RigPosePin::of(&snapshot), "L1", 0, None),
        Err(RigPoseRefusal::NoRig)
    );
}

// ---- independent hand fixture -----------------------------------------------------------------------------

fn translated_rig(binds: Value) -> Value {
    json!({
        "bones": {"bone1": {
            "segments": [{"point": [0, 10]}, {"point": [100, 10]}],
            "restSegments": [{"point": [0, 0]}, {"point": [100, 0]}],
            "closed": false
        }},
        "ikChains": {},
        "nextId": 2,
        "binds": binds,
    })
}

fn entry(point: u32, w: f64) -> Value {
    json!({"boneId": "bone1", "ptIndex": point, "w": w})
}

#[test]
fn a_translated_bone_moves_every_bound_vertex_and_keeps_the_stored_handles() {
    // Hand derivation: the bone moved by (0, +10) with no turn. Vertex 0 hangs on control point 0 (w = 1),
    // vertex 1 on both points (0.5 + 0.5 = 1), vertex 2 on control point 1 (w = 1); sumW = 1 so nothing is
    // normalised; the displacement is w * (cur - rest) = (0, 10) in every case. No rest handles are stored,
    // so every handle keeps the value of the stored record.
    let bind = json!({
        "strokeId": "s1",
        "rest": [[0, 0], [50, 0], [100, 0]],
        "weights": [[entry(0, 1.0)], [entry(0, 0.5), entry(1, 0.5)], [entry(1, 1.0)]],
        "rotate": false
    });
    let stored = [
        [0.0, 0.0, 0.0, 0.0, 5.0, 0.0],
        [50.0, 0.0, -5.0, 1.0, 3.0, 4.0],
        [100.0, 0.0, -5.0, 0.0, 0.0, 0.0],
    ];
    let doc = document(
        vec![json!({"layerUid": "L1", "rig": translated_rig(json!([bind]))})],
        vec![stroke_record("L1", "s1", false, &stored, 0)],
    );
    let snapshot = snapshot_of(&doc);
    let eval = evaluate_rig_pose(&snapshot, &RigPosePin::of(&snapshot), "L1", 0, None).unwrap();
    assert_eq!(eval.binds, vec![BindOutcome::Posed]);
    let stroke = &eval.strokes[0];
    assert_eq!((stroke.stroke_id.as_str(), stroke.closed), ("s1", false));
    let got: Vec<[f64; 6]> = stroke
        .segments
        .iter()
        .map(|s| {
            [
                s.point[0],
                s.point[1],
                s.handle_in[0],
                s.handle_in[1],
                s.handle_out[0],
                s.handle_out[1],
            ]
        })
        .collect();
    assert_eq!(
        got,
        vec![
            [0.0, 10.0, 0.0, 0.0, 5.0, 0.0],
            [50.0, 10.0, -5.0, 1.0, 3.0, 4.0],
            [100.0, 10.0, -5.0, 0.0, 0.0, 0.0],
        ]
    );
    assert_eq!((eval.frame, eval.source_frame), (0, 0));
}

fn unit_mesh(rect: Option<MeshRect>) -> MeshSnapshot {
    MeshSnapshot {
        outline: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        verts: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        offsets: vec![[0.5, 0.5], [0.5, 0.5], [0.5, 0.5]],
        tris: vec![0, 1, 2],
        rect,
    }
}

const SQUARE: MeshRect = MeshRect {
    x: 0.0,
    y: 0.0,
    width: 100.0,
    height: 100.0,
    rotation: 0.0,
};

fn mesh_document() -> Value {
    // The bone slides +10 in x; every vertex hangs on control point 0 with weight 1, so each world point
    // moves by (10, 0): the normalized offset is 10 / 100 = 0.1 in u and 0 in v, replacing the stored 0.5.
    let rig = json!({
        "bones": {"bone1": {
            "segments": [{"point": [10, 0]}, {"point": [110, 0]}],
            "restSegments": [{"point": [0, 0]}, {"point": [100, 0]}],
            "closed": false
        }},
        "ikChains": {},
        "nextId": 2,
        "binds": [{
            "meshId": "im_1",
            "rest": [[0, 0], [100, 0], [0, 100]],
            "weights": [[entry(0, 1.0)], [entry(0, 1.0)], [entry(0, 1.0)]],
            "rotate": false
        }],
    });
    document(vec![json!({"layerUid": "L1", "rig": rig})], vec![])
}

fn provider_of(mesh: MeshSnapshot) -> Provider {
    Provider {
        meshes: HashMap::from([("im_1".to_owned(), mesh)]),
    }
}

#[test]
fn a_mesh_bind_writes_normalized_offsets_that_replace_the_stored_ones() {
    let snapshot = snapshot_of(&mesh_document());
    let provider = provider_of(unit_mesh(Some(SQUARE)));
    let eval = evaluate_rig_pose(
        &snapshot,
        &RigPosePin::of(&snapshot),
        "L1",
        0,
        Some(&provider),
    )
    .unwrap();
    assert_eq!(eval.binds, vec![BindOutcome::Posed]);
    assert!(eval.strokes.is_empty());
    let offsets = &eval.meshes[0].offsets;
    assert_eq!(offsets.len(), 3);
    for o in offsets {
        assert!(close(o[0], 0.1) && close(o[1], 0.0), "{o:?}");
    }
    // the provider's stored sculpt is untouched: the join returns copies
    assert_eq!(provider.meshes["im_1"].offsets, vec![[0.5, 0.5]; 3]);
    // the deformed corner lands where the slid bone drags it: world x = 0 + 100 * (0 + 0.1) = 10
    let payload = eval.meshes[0]
        .scene_payload(&provider.meshes["im_1"])
        .unwrap();
    assert_eq!(
        payload.verts,
        vec![[10.0, 0.0], [110.0, 0.0], [10.0, 100.0]]
    );
    assert_eq!(payload.uvs, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    assert_eq!(payload.tris, vec![0, 1, 2]);
}

// ---- links, providers and refusals ------------------------------------------------------------------------

#[test]
fn dropped_binds_report_why_and_the_surviving_ones_still_pose() {
    let row = row("mixed_path_mesh_and_binds_relink_drops");
    let eval = evaluate_row(&row).unwrap();
    assert_eq!(
        eval.binds,
        vec![
            BindOutcome::Posed,
            BindOutcome::Unlinked(BindLink::StrokeVertexCount { stroke: 4, rest: 3 }),
            BindOutcome::Unlinked(BindLink::StrokeMissing),
            BindOutcome::Posed,
        ]
    );
    assert_eq!(
        eval.strokes
            .iter()
            .map(|s| s.stroke_id.as_str())
            .collect::<Vec<_>>(),
        vec!["s1"]
    );
    assert_eq!(eval.meshes.len(), 1);
}

#[test]
fn a_mesh_rebuilt_at_another_density_is_unlinked_with_both_counts() {
    let eval = evaluate_row(&row("mesh_rebuilt_at_another_density_is_dropped")).unwrap();
    assert_eq!(
        eval.binds,
        vec![BindOutcome::Unlinked(BindLink::MeshVertexCount {
            mesh: 16,
            rest: 9
        })]
    );
    assert!(eval.meshes.is_empty());
}

#[test]
fn missing_providers_are_typed_unavailability_and_never_a_pose() {
    let row = row("mixed_path_mesh_and_binds_relink_drops");
    // no store wired at all
    let none = evaluate_with(&row, None).unwrap();
    assert_eq!(
        none.binds[3],
        BindOutcome::Unavailable(PoseUnavailable::MeshStore)
    );
    assert_eq!(
        none.binds[0],
        BindOutcome::Posed,
        "path binds do not need the store"
    );
    assert!(none.meshes.is_empty());
    // a store without the id
    let empty = Provider {
        meshes: HashMap::new(),
    };
    let missing = evaluate_with(&row, Some(&empty)).unwrap();
    assert_eq!(
        missing.binds[3],
        BindOutcome::Unlinked(BindLink::MeshMissing)
    );
    // a mesh whose raster rect is not wired: the rect is never defaulted
    let mut provider = row_provider(&row);
    provider.meshes.get_mut("im_1").unwrap().rect = None;
    let no_rect = evaluate_with(&row, Some(&provider)).unwrap();
    assert_eq!(
        no_rect.binds[3],
        BindOutcome::Unavailable(PoseUnavailable::RasterRect)
    );
    assert!(no_rect.meshes.is_empty());
}

#[test]
fn pins_frame_layer_and_rig_are_checked_before_anything_is_evaluated() {
    let row = row("arm_rotate_soft_bone_turned_40_degrees");
    let snapshot = snapshot_of(&row_document(&row));
    let pin = RigPosePin::of(&snapshot);
    let eval = |pin: &RigPosePin<'_>, layer: &str, frame: u32| {
        evaluate_rig_pose(&snapshot, pin, layer, frame, None)
    };
    assert!(eval(&pin, "L1", 0).is_ok());
    assert_eq!(
        eval(
            &RigPosePin {
                instance_id: "other",
                ..pin
            },
            "L1",
            0
        ),
        Err(RigPoseRefusal::WrongInstance)
    );
    assert_eq!(
        eval(
            &RigPosePin {
                document_id: "other",
                ..pin
            },
            "L1",
            0
        ),
        Err(RigPoseRefusal::WrongDocument)
    );
    assert_eq!(
        eval(
            &RigPosePin {
                content_revision: 7,
                ..pin
            },
            "L1",
            0
        ),
        Err(RigPoseRefusal::WrongRevision)
    );
    assert_eq!(
        eval(
            &RigPosePin {
                snapshot_id: "other",
                ..pin
            },
            "L1",
            0
        ),
        Err(RigPoseRefusal::WrongSnapshot)
    );
    assert_eq!(eval(&pin, "L1", 4), Err(RigPoseRefusal::FrameOutOfRange));
    assert_eq!(eval(&pin, "missing", 0), Err(RigPoseRefusal::LayerAbsent));
    // another document's pin is refused even though the snapshot would otherwise evaluate
    let other = snapshot_of(&row_document(&row));
    assert_eq!(
        evaluate_rig_pose(&snapshot, &RigPosePin::of(&other), "L1", 0, None),
        Err(RigPoseRefusal::WrongDocument)
    );
}
