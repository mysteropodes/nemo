//! L03-P (#2252) / L07-L (#2230) mesh part: a stored rig pose reaches the retained scene as the offsets
//! vector that feeds the image-mesh payload, through the ONE `layer_pose` call site.
//!
//! The oracle (`fixtures/object-rig-mesh-join/oracle.txt`, generator `gen.cjs` next to it) is the verbatim
//! baseline `applyRigDeform` over a live raster stub followed by the real `scenePayloadFor`, run in a Node
//! vm. Expected numbers come from that file, never from this crate's output.
use super::{layer_pose, mesh_offsets, packet_poses};
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::{
    prepare_mesh_geometry, MeshSceneSource,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::BindOutcome;
use crate::object_codec::decode_storage_project;
use crate::object_document::object_rig_record::BindLink;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::object_render_scene::{prepare_object_render_scene, ObjectSceneError};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "rig_pose_eval_rows.rs"]
mod rows;

const ORACLE: &str = include_str!("fixtures/object-rig-mesh-join/oracle.txt");
const STORED: &str = "stored_sculpt_is_replaced_by_the_rig";

struct Row {
    name: String,
    input: Value,
    offsets: Vec<[f64; 2]>,
    payload: Value,
}

fn pairs(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
        .collect()
}

fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 4, "oracle row shape");
            Row {
                name: f[0].to_owned(),
                input: serde_json::from_str(f[1]).unwrap(),
                offsets: pairs(&serde_json::from_str(f[2]).unwrap()),
                payload: serde_json::from_str(f[3]).unwrap(),
            }
        })
        .collect()
}

fn row(name: &str) -> Row {
    rows().into_iter().find(|r| r.name == name).expect(name)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// A one-layer document: layer `L1` carries the rig; one raster of mesh `im_1` at `rect` (the raster's own
/// rect: `x`,`y` top-left, `w`,`h`, optional `rot`).
fn document(rig: &Value, mesh: &Value, rect: &Value, extra_raster: Option<Value>) -> Value {
    let item = |id: &str, rect: &Value| {
        let w = rect["w"].as_f64().unwrap();
        let h = rect["h"].as_f64().unwrap();
        let mut item = json!({"assetId":"image:source",
            "x": rect["x"].as_f64().unwrap() + w / 2.0, "y": rect["y"].as_f64().unwrap() + h / 2.0,
            "width": w, "height": h, "meshId": "im_1"});
        if let Some(rot) = rect.get("rot") {
            item["rotation"] = rot.clone();
        }
        json!({"target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"L1","strokeId":id}, "item": item})
    };
    let mut rasters = vec![item("raster:mesh", rect)];
    let mut elements = vec![json!({"layerUid":"L1","strokeId":"raster:mesh","naturalPosition":0,"order":{}})];
    if let Some(other) = extra_raster {
        rasters.push(item("raster:other", &other));
        elements.push(json!({"layerUid":"L1","strokeId":"raster:other","naturalPosition":1,"order":{}}));
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,
        "layers":[{"layerUid":"L1","rig":rig}],"objects":[],
        "items":{"raster":rasters},"imageMeshes":{"im_1":mesh},
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L1","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]}})
}

fn snapshot_of(doc: &Value) -> ObjectSnapshot {
    let project = decode_storage_project(&serde_json::to_vec(doc).unwrap()).unwrap();
    ObjectSnapshot::new("rig-mesh-join", project).unwrap()
}

fn selector(snapshot: &ObjectSnapshot) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    }
}

fn packet_of(snapshot: &ObjectSnapshot) -> ObjectFramePacket {
    prepare_object_frame(snapshot, &selector(snapshot)).unwrap()
}

fn row_snapshot(row: &Row) -> ObjectSnapshot {
    let i = &row.input;
    snapshot_of(&document(&i["rig"], &i["mesh"], &i["rect"], None))
}

#[test]
fn baseline_rig_pose_reaches_the_scene_payload_for_every_oracle_row() {
    let mut linked = 0;
    for row in rows() {
        let snapshot = row_snapshot(&row);
        let packet = packet_of(&snapshot);
        let scene = prepare_object_render_scene(&packet, &selector(&snapshot)).unwrap();
        let source = &scene.rasters[0];
        let stored = snapshot.document().image_meshes().unwrap().get("im_1").unwrap();
        // An unlinked bind leaves the stored sculpt, which is what the baseline shows after relink.
        let offsets: Vec<[f64; 2]> = match source.rig_offsets() {
            Some(posed) => {
                linked += 1;
                posed.to_vec()
            }
            None => stored.effective_offsets(),
        };
        assert_eq!(offsets.len(), row.offsets.len(), "{}", row.name);
        for (i, (got, want)) in offsets.iter().zip(&row.offsets).enumerate() {
            assert!(
                near(got[0], want[0]) && near(got[1], want[1]),
                "{} vertex {i}: {got:?} != {want:?}",
                row.name
            );
        }
        // The payload the scene draws from those offsets equals the baseline scenePayloadFor.
        let f = &row.input["finalRect"];
        let rect = ImageRect {
            x: f["x"].as_f64().unwrap(),
            y: f["y"].as_f64().unwrap(),
            width: f["w"].as_f64().unwrap(),
            height: f["h"].as_f64().unwrap(),
            rotation: f.get("rot").and_then(Value::as_f64).unwrap_or(0.0),
        };
        let geometry = prepare_mesh_geometry(
            &rect,
            (64, 64),
            1.0,
            &MeshSceneSource {
                mesh_id: "im_1",
                view: [1., 0., 0., 1., 0., 0.],
                offsets: source.rig_offsets(),
                pose: None,
            },
            snapshot.document().image_meshes(),
        )
        .unwrap()
        .unwrap();
        let want_verts = pairs(&row.payload["verts"]);
        for (i, (got, want)) in geometry.payload.verts.iter().zip(&want_verts).enumerate() {
            assert!(
                near(got[0], want[0]) && near(got[1], want[1]),
                "{} payload vertex {i}: {got:?} != {want:?}",
                row.name
            );
        }
        assert_eq!(geometry.payload.verts.len(), want_verts.len(), "{}", row.name);
        assert_eq!(geometry.payload.uvs, pairs(&row.payload["uvs"]), "{}", row.name);
        let tris: Vec<u32> = row.payload["tris"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(geometry.payload.tris, tris, "{}", row.name);
        assert_eq!(geometry.payload.outline.len(), 8, "{}", row.name);
    }
    assert_eq!(linked, 6, "six rows pose a linked bind; the clamped one is unlinked");
}

#[test]
fn the_posed_offsets_are_not_the_stored_ones_and_the_negative_control_is_visible() {
    let row = row(STORED);
    let snapshot = row_snapshot(&row);
    let packet = packet_of(&snapshot);
    let scene = prepare_object_render_scene(&packet, &selector(&snapshot)).unwrap();
    let posed = scene.rasters[0].rig_offsets().expect("linked bind poses");
    let stored = snapshot.document().image_meshes().unwrap().get("im_1").unwrap();
    assert_ne!(posed, stored.effective_offsets().as_slice());
    assert!(posed.iter().any(|o| o[0] > 0.1), "the bone moved the vertices");
}

#[test]
fn one_evaluation_per_snapshot_is_bit_identical_detached_and_pinned() {
    let row = row("rotate_mode_turns_the_vertices");
    let snapshot = row_snapshot(&row);
    let packet = packet_of(&snapshot);
    let first = layer_pose(&packet, 0).unwrap();
    let second = layer_pose(&packet, 0).unwrap();
    assert_eq!(first, second);
    for (a, b) in first.meshes[0].offsets.iter().zip(&second.meshes[0].offsets) {
        assert_eq!((a[0].to_bits(), a[1].to_bits()), (b[0].to_bits(), b[1].to_bits()));
    }
    // Pinned to the snapshot and frame it was evaluated on.
    assert_eq!(first.snapshot_id, snapshot.snapshot_id());
    assert_eq!(first.content_revision, snapshot.content_revision());
    assert_eq!(first.document_id, snapshot.document_id());
    assert_eq!((first.frame, first.source_frame), (0, 0));
    // Detached: scribbling over the owned output cannot change the rest geometry, the stored mesh or a
    // later evaluation.
    let store_before = snapshot.document().image_meshes().unwrap().clone();
    let mut scribbled = first.clone();
    for o in &mut scribbled.meshes[0].offsets {
        *o = [9e9, -9e9];
    }
    scribbled.meshes[0].offsets.clear();
    assert_eq!(snapshot.document().image_meshes().unwrap(), &store_before);
    assert_eq!(layer_pose(&packet, 0).unwrap(), second);
    // The contour half walks contour records only: a raster-only layer is evaluated by the raster join alone,
    // through the same `layer_pose`.
    assert!(packet_poses(&packet).is_empty());
}

#[test]
fn a_mismatched_bind_reports_unlinked_and_leaves_the_stored_mesh_drawing() {
    let row = row("rest_longer_than_verts_is_clamped");
    let snapshot = row_snapshot(&row);
    let packet = packet_of(&snapshot);
    let evaluation = layer_pose(&packet, 0).unwrap();
    assert_eq!(
        evaluation.binds,
        [BindOutcome::Unlinked(BindLink::MeshVertexCount { mesh: 9, rest: 11 })]
    );
    assert!(evaluation.meshes.is_empty());
    assert_eq!(mesh_offsets(&packet, 0, "im_1", &Ok(evaluation)), Ok(None));
}

#[test]
fn a_missing_store_is_typed_unavailability_and_never_a_pose() {
    let row = row("translate_all_bound_vertices");
    let mut doc = document(&row.input["rig"], &row.input["mesh"], &row.input["rect"], None);
    doc.as_object_mut().unwrap().remove("imageMeshes");
    let snapshot = snapshot_of(&doc);
    let packet = packet_of(&snapshot);
    let evaluation = layer_pose(&packet, 0).unwrap();
    assert!(matches!(
        evaluation.binds[0],
        BindOutcome::Unavailable(_)
    ));
    assert!(evaluation.meshes.is_empty());
    assert_eq!(
        mesh_offsets(&packet, 0, "im_1", &Ok(evaluation)),
        Err(super::REASON_MESH_UNAVAILABLE)
    );
    assert!(matches!(
        prepare_object_render_scene(&packet, &selector(&snapshot)),
        Err(ObjectSceneError::UnsupportedGeometry { reason, .. })
            if reason == super::REASON_MESH_UNAVAILABLE
    ));
}

#[test]
fn an_ambiguous_raster_rect_is_typed_unavailability_not_a_guessed_pose() {
    let row = row("translate_all_bound_vertices");
    let other = json!({"x": 10.0, "y": 10.0, "w": 50.0, "h": 50.0});
    let doc = document(&row.input["rig"], &row.input["mesh"], &row.input["rect"], Some(other));
    let snapshot = snapshot_of(&doc);
    let packet = packet_of(&snapshot);
    let evaluation = layer_pose(&packet, 0).unwrap();
    assert!(matches!(evaluation.binds[0], BindOutcome::Unavailable(_)));
    assert!(matches!(
        prepare_object_render_scene(&packet, &selector(&snapshot)),
        Err(ObjectSceneError::UnsupportedGeometry { reason, .. })
            if reason == super::REASON_MESH_UNAVAILABLE
    ));
    // The same rect twice is not ambiguous: both rasters are posed by the one evaluation.
    let same = document(&row.input["rig"], &row.input["mesh"], &row.input["rect"], Some(row.input["rect"].clone()));
    let snapshot = snapshot_of(&same);
    let packet = packet_of(&snapshot);
    let scene = prepare_object_render_scene(&packet, &selector(&snapshot)).unwrap();
    assert_eq!(scene.rasters.len(), 2);
    assert_eq!(scene.rasters[0].rig_offsets(), scene.rasters[1].rig_offsets());
    assert!(scene.rasters[0].rig_offsets().is_some());
}

#[test]
fn a_wrong_pin_or_a_layer_without_a_rig_never_evaluates() {
    let row = row("translate_all_bound_vertices");
    let snapshot = row_snapshot(&row);
    let packet = packet_of(&snapshot);
    let mut wrong = selector(&snapshot);
    wrong.document_snapshot_id = "foreign";
    assert_eq!(
        prepare_object_render_scene(&packet, &wrong).err(),
        Some(ObjectSceneError::WrongSnapshot)
    );
    // A rig-less layer with a meshed raster draws the stored mesh: no evaluation, no offsets.
    let mut doc = document(&row.input["rig"], &row.input["mesh"], &row.input["rect"], None);
    doc["layers"][0].as_object_mut().unwrap().remove("rig");
    let snapshot = snapshot_of(&doc);
    let packet = packet_of(&snapshot);
    assert!(packet_poses(&packet).is_empty());
    let scene = prepare_object_render_scene(&packet, &selector(&snapshot)).unwrap();
    assert!(scene.rasters[0].rig_offsets().is_none());
}

#[test]
fn a_rigged_layer_that_also_holds_a_vector_keeps_the_typed_chain_refusal() {
    // The pivot bounds of a vector in the layer would be the unposed stored ones, so the supported case
    // stops at pure raster/mesh layers; the mixed layer stays an atomic typed refusal.
    let row = row("translate_all_bound_vertices");
    let mut doc = document(&row.input["rig"], &row.input["mesh"], &row.input["rect"], None);
    let square = [[0., 0., 0., 0., 0., 0.], [10., 0., 0., 0., 0., 0.], [10., 10., 0., 0., 0., 0.]];
    doc["objects"] = json!([rows::stroke_record("L1", "s1", true, &square, 0)]);
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"L1","strokeId":"s1","naturalPosition":1,"order":{}}));
    let snapshot = snapshot_of(&doc);
    let packet = packet_of(&snapshot);
    assert!(matches!(
        prepare_object_render_scene(&packet, &selector(&snapshot)),
        Err(ObjectSceneError::UnsupportedGeometry { reason, .. })
            if reason == "native parent group, rig or effect context unavailable"
    ));
}
