//! Real admitted snapshots, literal Motion values and preservation controls.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::PropertyHolderTarget;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

// Reuse the original real stored-rig fixture builder; this file contains no tests.
#[allow(dead_code)]
#[path = "rig_pose_eval_rows.rs"]
mod rig_input;

fn target() -> PropertyHolderTarget {
    serde_json::from_value(json!({"contextId":"scene-root","layerUid":"L0"})).unwrap()
}
fn snapshot(properties: Value) -> ObjectSnapshot {
    let value = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
        "layers":[{"layerUid":"L0"}],"objects":[],
        "propertyHolders":[{"target":target(),"properties":properties}]});
    ObjectSnapshot::new(
        "frame-motion",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}

#[test]
fn neutral_defaults_and_fractional_tracks_use_the_retained_source() {
    assert!(snapshot(json!([]))
        .sampled_motion_2d(&target(), 0.)
        .unwrap()
        .is_none());
    let s = snapshot(json!([
        {"property":"position","track":{"keys":[{"frame":0,"v":[0,10]}, {"frame":4,"v":[40,50]}]}},
        {"property":"anchor","static":[3,-4]},
        {"property":"rotation","static":[90]},
        {"property":"scale","static":[0,-200]},
        {"property":"opacity","static":[25]}
    ]));
    let before = encode_storage_project(s.document()).unwrap();
    let m = s.sampled_motion_2d(&target(), 2.).unwrap().unwrap();
    assert_eq!(
        [m.dx, m.dy, m.ax, m.ay, m.rot, m.sx, m.sy, m.op],
        [20., 30., 3., -4., 90., 0., -2., 0.25]
    );
    assert_eq!(before, encode_storage_project(s.document()).unwrap());
    assert_eq!(s.content_revision(), 0);
}

#[test]
fn active_motion_expression_is_not_silently_sampled_raw() {
    let s = snapshot(json!([{"property":"position","static":[5,6],
        "expression":{"enabled":true,"code":"[99,99]"}}]));
    assert!(s.sampled_motion_2d(&target(), 0.).is_err());
}

#[test]
fn invalid_frames_and_unknown_targets_never_return_neutral_motion() {
    let s = snapshot(json!([]));
    assert!(s.sampled_motion_2d(&target(), f64::NAN).is_err());
    let missing =
        serde_json::from_value(json!({"contextId":"scene-root","layerUid":"absent"})).unwrap();
    assert!(s.sampled_motion_2d(&missing, 0.).is_err());
}

#[test]
fn actual_vector_evaluation_consumes_element_motion_about_its_native_bounds() {
    use crate::evaluation::object_vector_kernels::render_vector_eval::evaluate_vector;
    let segments:Vec<_> = [[0,0],[10,0],[10,10],[0,10]].into_iter().map(|p|
        json!({"point":{"x":p[0],"y":p[1]},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})).collect();
    let doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":[{"layerUid":"L0"}],"objects":[{"schemaVersion":1,"family":"editable-vector-path",
            "target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a","frameScope":{"kind":"authored","frame":0}},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":segments},
            "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1},
            "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1}}],
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},"properties":[
            {"property":"position","static":[5,0]}, {"property":"rotation","static":[90]}, {"property":"scale","static":[200,100]}]}]});
    let s = ObjectSnapshot::new(
        "vector-motion",
        decode_storage_project(doc.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let evaluated = evaluate_vector(&s, 0, 0).unwrap();
    for (segment, point) in
        evaluated
            .segments
            .iter()
            .zip([[15., -5.], [15., 15.], [5., 15.], [5., -5.]])
    {
        for i in 0..2 {
            assert!((segment.point[i] - point[i]).abs() < 1e-10);
        }
    }
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
}

#[test]
fn the_actual_stored_rig_pose_scene_route_also_applies_element_motion() {
    use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
        evaluate_rig_pose, RigPosePin,
    };
    use crate::evaluation::object_vector_kernels::render_vector_eval::evaluate_record_with_pose;
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    let mut doc =
        rig_input::row_document(&rig_input::row("arm_rotate_soft_bone_turned_40_degrees"));
    for record in doc["objects"].as_array_mut().unwrap() {
        record["family"] = json!("editable-vector-path");
        record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1});
    }
    let id = doc["objects"][0]["target"]["strokeId"].clone();
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L1","strokeId":id},
        "properties":[{"property":"position","static":[7,-3]}]}]);
    let s = ObjectSnapshot::new(
        "rig-element-motion",
        decode_storage_project(doc.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let pose = evaluate_rig_pose(&s, &RigPosePin::of(&s), "L1", 0, None).unwrap();
    let selected = pose
        .strokes
        .iter()
        .find(|p| p.stroke_id == id.as_str().unwrap())
        .unwrap();
    let selector = ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&s, &selector).unwrap();
    let record = packet
        .records()
        .find(|r| r.record().target().stroke_id() == id.as_str().unwrap())
        .unwrap();
    let actual = evaluate_record_with_pose(&packet, record, Some(selected)).unwrap();
    assert_eq!(actual.segments.len(), selected.segments.len());
    for (actual, posed) in actual.segments.iter().zip(&selected.segments) {
        assert_eq!(actual.point, [posed.point[0] + 7., posed.point[1] - 3.]);
        assert_eq!(actual.handle_in, posed.handle_in);
        assert_eq!(actual.handle_out, posed.handle_out);
    }
    assert_eq!(s.content_revision(), 0);
}
