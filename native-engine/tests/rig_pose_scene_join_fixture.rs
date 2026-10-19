//! Original fixture/oracle construction and real immutable packet builders.
//! The eleven consumer test bodies remain in rig_pose_scene_join.rs.
use super::rows::{close, num, row_document, Row};
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, prepare_object_frame, ObjectFramePacket, ObjectFrameSelector,
};
use crate::object_render_scene::{
    prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use vello::kurbo::{BezPath, PathEl};

pub(super) const ARM: &str = "arm_rotate_soft_bone_turned_40_degrees";

pub(super) fn document(row: &Row) -> Value {
    let mut doc = row_document(row);
    doc["formatVersion"] = json!(2);
    let elements: Vec<_> = doc["objects"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, record)| {
            json!({"layerUid":"L1",
            "strokeId":record["target"]["strokeId"],"naturalPosition":index,"order":{}})
        })
        .collect();
    doc["compositionOrder"] = json!({
        "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root",
            "layers":[{"layerUid":"L1","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"
    });
    doc
}
pub(super) fn snapshot_of(source: &Value) -> ObjectSnapshot {
    let document = decode_storage_project(&serde_json::to_vec(source).unwrap()).unwrap();
    ObjectSnapshot::new("rig-scene-production", document).unwrap()
}
pub(super) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
pub(super) fn packet(snapshot: &ObjectSnapshot, frame: u32, exposure: bool) -> ObjectFramePacket {
    let pins = selector(snapshot, frame);
    if exposure {
        prepare_object_exposure_frame(snapshot, &pins).unwrap()
    } else {
        prepare_object_frame(snapshot, &pins).unwrap()
    }
}
pub(super) fn plan(snapshot: &ObjectSnapshot, frame: u32, exposure: bool) -> ObjectPaintPlan {
    let packet = packet(snapshot, frame, exposure);
    let pins = selector(snapshot, frame);
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins).unwrap()
}
pub(super) fn expected_segments(row: &Row, id: &str) -> Vec<[f64; 6]> {
    row.expect["strokes"][id]
        .as_array()
        .unwrap()
        .iter()
        .map(|segment| std::array::from_fn(|index| num(&segment[index])))
        .collect()
}
pub(super) fn expected_path(segments: &[[f64; 6]], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    path.move_to((segments[0][0], segments[0][1]));
    let edges = if closed {
        segments.len()
    } else {
        segments.len() - 1
    };
    for index in 0..edges {
        let a = segments[index];
        let b = segments[(index + 1) % segments.len()];
        path.curve_to(
            (a[0] + a[4], a[1] + a[5]),
            (b[0] + b[2], b[1] + b[3]),
            (b[0], b[1]),
        );
    }
    if closed {
        path.close_path();
    }
    path
}
pub(super) fn coordinates(path: &BezPath) -> Vec<f64> {
    path.elements()
        .iter()
        .flat_map(|element| match element {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p.x, p.y],
            PathEl::QuadTo(a, b) => vec![a.x, a.y, b.x, b.y],
            PathEl::CurveTo(a, b, c) => vec![a.x, a.y, b.x, b.y, c.x, c.y],
            PathEl::ClosePath => vec![],
        })
        .collect()
}
pub(super) fn assert_path(actual: &BezPath, expected: &BezPath, label: &str) {
    let a = coordinates(actual);
    let b = coordinates(expected);
    assert_eq!(a.len(), b.len(), "{label}");
    for (index, (a, b)) in a.iter().zip(b).enumerate() {
        assert!(close(*a, b), "{label} scalar {index}: {a} != {b}");
    }
    assert_eq!(
        actual.elements().last() == Some(&PathEl::ClosePath),
        expected.elements().last() == Some(&PathEl::ClosePath)
    );
}
