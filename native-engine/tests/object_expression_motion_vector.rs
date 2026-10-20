//! A1's actual editable-vector post-placement regression. ALL runtime UNRUN.
use super::{fixture, owner, pins};
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_vector, VectorEvalError,
};
use crate::object_codec::encode_storage_project;
use crate::object_expression_runtime::prepare_object_motion_exposure_frame_for_owner;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFramePacket};
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, PaintEntry,
};
use serde_json::{json, Value};
use vello::kurbo::PathEl;

fn drawing(enabled: bool) -> Value {
    let mut doc = fixture("[frame,0]");
    // This is the actual editable-vector schema, not the legacy contour path.
    for record in doc["objects"].as_array_mut().unwrap() {
        record["family"] = json!("editable-vector-path");
        record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
            "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":1});
    }
    doc["propertyHolders"][0]["properties"][0]["static"] = json!([2, 0]);
    doc["propertyHolders"][0]["properties"][0]["expression"]["enabled"] = json!(enabled);
    doc["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"position","static":[7,-3]}));
    doc
}
fn starts(packet: &ObjectFramePacket) -> Vec<[f64; 2]> {
    let pin = pins(packet.snapshot(), packet.frame());
    let scene = prepare_object_render_scene(packet, &pin).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pin).unwrap();
    let result = ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Duplicator(copy) => match copy.path.elements()[0] {
                PathEl::MoveTo(p) => [p.x, p.y],
                _ => panic!("expected native cubic start"),
            },
            _ => panic!("duplicator seed must not also become ordinary paint"),
        })
        .collect();
    result
}

#[test]
fn evaluated_dup_offset_and_raw_position_reach_real_editable_vector_scene_in_original_order() {
    let o = owner(drawing(true));
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let mut effects = None;
    let p =
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects).unwrap();
    assert!(p.has_evaluated_duplicator_motion(0));
    assert!(!p.has_evaluated_duplicator_motion(1));
    // Placement already ran at the requested frame: [20,22,34] at y20.
    // The post-placement RAW layer Position [7,-3] must run exactly once.
    let placed = p.duplicator_outputs()[0]
        .output
        .copies
        .iter()
        .map(|copy| {
            [
                copy.record.geometry.segments[0].point.x.as_f64().unwrap(),
                copy.record.geometry.segments[0].point.y.as_f64().unwrap(),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(placed, [[20., 20.], [22., 20.], [34., 20.]]);
    assert_eq!(starts(&p), [[27., 17.], [29., 17.], [41., 17.]]);
    assert_eq!(
        starts(&p),
        [[27., 17.], [29., 17.], [41., 17.]],
        "no persistent transform or second Position pass"
    );
    assert!(effects.is_none());
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
}

#[test]
fn the_old_snapshot_and_ordinary_vector_entry_points_keep_their_strict_expression_refusal() {
    let o = owner(drawing(true));
    let s = o.acquire_snapshot(0).unwrap();
    assert!(prepare_object_exposure_frame(&s, &pins(&s, 2)).is_err());
    assert_eq!(
        evaluate_vector(&s, 1, 2).unwrap_err(),
        VectorEvalError::LayerMotionUnavailable
    );
    // Preparing an owner-bound packet must not toggle any global coverage bit.
    let mut effects = None;
    let p =
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects).unwrap();
    assert_eq!(starts(&p), [[27., 17.], [29., 17.], [41., 17.]]);
    drop(p);
    assert!(prepare_object_exposure_frame(&s, &pins(&s, 2)).is_err());
    assert_eq!(
        evaluate_vector(&s, 1, 2).unwrap_err(),
        VectorEvalError::LayerMotionUnavailable
    );
}

#[test]
fn a_separate_raw_packet_of_matching_uid_and_layer_index_never_inherits_the_owner_fact() {
    let evaluated_owner = owner(drawing(true));
    let evaluated = evaluated_owner.acquire_snapshot(0).unwrap();
    let mut effects = None;
    let p = prepare_object_motion_exposure_frame_for_owner(
        &evaluated_owner,
        &evaluated,
        &pins(&evaluated, 2),
        &mut effects,
    )
    .unwrap();
    assert!(p.has_evaluated_duplicator_motion(0));
    let raw_owner = owner(drawing(false));
    let raw = raw_owner.acquire_snapshot(0).unwrap();
    let plain = prepare_object_exposure_frame(&raw, &pins(&raw, 2)).unwrap();
    assert!(!plain.has_evaluated_duplicator_motion(0));
    assert_eq!(starts(&plain), [[27., 17.], [29., 17.], [41., 17.]]);
    let ordinary = evaluate_vector(&raw, 1, 2).unwrap();
    assert_eq!(ordinary.segments[0].point, [27., 17.]);
    assert_eq!(raw_owner.history_depths(), (0, 0));
}

#[test]
fn genuinely_unjoined_layer_and_element_geometry_expressions_do_not_gain_admission() {
    for fault in ["layer-position", "layer-rotation", "element-position"] {
        let mut doc = drawing(true);
        match fault {
            "layer-position" => doc["propertyHolders"][0]["properties"][1]["expression"] =
                json!({"enabled":true,"code":"[1000,1000]"}),
            "layer-rotation" => doc["propertyHolders"][0]["properties"].as_array_mut().unwrap().push(
                json!({"property":"rotation","expression":{"enabled":true,"code":"90"}})),
            _ => doc["propertyHolders"].as_array_mut().unwrap().push(json!({
                "target":{"contextId":"scene-root","layerUid":"L0","strokeId":"stroke-2"},
                "properties":[{"property":"position","expression":{"enabled":true,"code":"[1000,1000]"}}]})),
        }
        let o = owner(doc);
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        let mut effects = None;
        assert!(
            prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects)
                .is_err(),
            "{fault}"
        );
        assert!(
            effects.unwrap().effects.is_empty(),
            "{fault}: refuse before running any guest"
        );
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
}
