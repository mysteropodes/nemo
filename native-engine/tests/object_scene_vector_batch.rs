//! Actual retained owner -> common Frame/Scene/Plan -> original SVG family consumer.
//! No test geometry producer, packet mutation or execution outside the real APIs.
use crate::export_job::native_vector_export::native_vector_export_svg::{
    serialize_svg_frame, SvgFrameSpec,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::encode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_expression_runtime::prepare_object_motion_exposure_frame_for_owner;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, ObjectFramePacket, ObjectFrameSelector,
};
use crate::object_render_scene::{
    prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn drawing(legacy: bool) -> Value {
    let row = include_str!("fixtures/render-vector-eval/oracle.txt")
        .lines()
        .find(|line| line.starts_with("vj1\tplain-absent-stack@0\t"))
        .unwrap();
    let columns: Vec<_> = row.split('\t').collect();
    let original: Value = serde_json::from_str(columns[10]).unwrap();
    let mut record = if legacy {
        let examples: Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap();
        // Actual admitted baseline legacy family/paint, with the unchanged original rectangle geometry.
        let mut record = examples["records"][0].clone();
        record["geometry"] = original["objects"][0]["geometry"].clone();
        record
    } else {
        original["objects"][0].clone()
    };
    record["target"] = json!({"contextId":"scene-root","layerUid":"L1","strokeId":"first",
        "frameScope":{"kind":"authored","frame":0}});
    let mut second = record.clone();
    second["target"]["strokeId"] = json!("second");
    for s in second["geometry"]["segments"].as_array_mut().unwrap() {
        s["point"]["x"] = json!(s["point"]["x"].as_f64().unwrap() + 200.);
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[{"layerUid":"L1","timing":{"format":"nemo.native-layer-timing","formatVersion":1,
            "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
        "objects":[record,second],"propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L1"},
            "properties":[{"property":"position","static":[-999,-999],"expression":{"enabled":true,"code":"[frame+5,7]"}},
                {"property":"anchor","static":[10,0]},{"property":"scale","static":[200,100]},
                {"property":"opacity","static":[50]}]}],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L1","naturalPosition":0,"order":{}}],"scopes":[{
                    "frameScope":{"kind":"authored","frame":0},"elements":[
                        {"layerUid":"L1","strokeId":"first","naturalPosition":0,"order":{}},
                        {"layerUid":"L1","strokeId":"second","naturalPosition":1,"order":{}}]}]}]}})
}
fn owner(drawing: &Value) -> NativeObjectHistory {
    let project = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":400,"canvasH":200,"fps":24},"drawing":drawing,
        "viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("ordinary-batch", project.to_string().as_bytes())
        .unwrap()
}
fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn packet(o: &NativeObjectHistory, s: &ObjectSnapshot, frame: u32) -> ObjectFramePacket {
    let mut effects = None;
    let packet =
        prepare_object_motion_exposure_frame_for_owner(o, s, &pins(s, frame), &mut effects)
            .unwrap();
    assert!(effects.is_none());
    packet
}
fn plan(packet: &ObjectFramePacket) -> ObjectPaintPlan {
    let selector = pins(packet.snapshot(), packet.frame());
    let scene = prepare_object_render_scene(packet, &selector).unwrap();
    prepare_object_paint_plan(&scene, &selector).unwrap()
}
fn starts(plan: &ObjectPaintPlan) -> Vec<(String, [f64; 2])> {
    let mut values = plan
        .commands()
        .map(|c| {
            (
                c.source().record().target.stroke_id.clone(),
                c.contour().segments()[0].point,
            )
        })
        .collect::<Vec<_>>();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    values
}
fn svg(plan: &ObjectPaintPlan) -> Result<String, String> {
    serialize_svg_frame(plan, SvgFrameSpec::new(400, 200, 1_000_000).unwrap())
        .map(|frame| String::from_utf8(frame.bytes).unwrap())
        .map_err(|e| format!("{e:?}"))
}

#[test]
fn real_common_batch_uses_evaluated_frame_values_and_one_whole_layer_pivot_in_both_families() {
    for legacy in [false, true] {
        let o = owner(&drawing(legacy));
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        assert!(
            prepare_object_exposure_frame(&s, &pins(&s, 0)).is_err(),
            "raw snapshot cannot borrow evaluated values"
        );
        for (frame, x) in [(0, -155.), (2, -153.), (0, -155.)] {
            let packet = packet(&o, &s, frame);
            let plan = plan(&packet);
            // Union [0,0]-[300,60], anchor [10,0] => pivot[160,30]. Position [frame+5,7].
            assert_eq!(
                starts(&plan),
                [("first".into(), [x, 7.]), ("second".into(), [x + 400., 7.])]
            );
            assert_eq!(plan.commands().len(), 2);
            assert_eq!(packet.records().len(), 2);
            for command in plan.commands() {
                assert_eq!(command.contour().segments().len(), 4);
                assert_eq!(
                    command.paint()[3].as_f64(),
                    Some(if legacy { 0.375 } else { 0.5 })
                );
                assert_eq!(
                    command.source().record().geometry.segments[0]
                        .point
                        .y
                        .as_f64(),
                    Some(0.)
                );
            }
        }
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
}

#[test]
fn actual_legacy_svg_serializes_transformed_contours_while_editable_svg_gate_stays_explicit() {
    let o = owner(&drawing(true));
    let s = o.acquire_snapshot(0).unwrap();
    let p = packet(&o, &s, 0);
    let legacy_plan = plan(&p);
    let text = svg(&legacy_plan).unwrap();
    assert!(text.contains("<path d=\"M-155,7 C-155,7 45,7 45,7"));
    assert!(text.contains("<path d=\"M245,7 C245,7 445,7 445,7"));
    assert_eq!(text.matches("fill-opacity=\"0.375\"").count(), 2);
    assert!(!text.contains("M0,0"));
    let o = owner(&drawing(false));
    let s = o.acquire_snapshot(0).unwrap();
    let p = packet(&o, &s, 0);
    let plan = plan(&p);
    assert!(svg(&plan)
        .unwrap_err()
        .contains("record family has no SVG representation"));
}

fn three_d(legacy: bool) -> Value {
    let mut source = drawing(legacy);
    source["layers"][0]["threeD"] = json!(true);
    source["propertyHolders"][0]["properties"] = json!([
        {"property":"position","static":[10,0]},{"property":"anchor","static":[10,0]},
        {"property":"scale","static":[200,100]},{"property":"opacity","static":[20],
            "expression":{"enabled":true,"code":"frame+20"}},
        {"property":"positionZ","static":[-999],"expression":{"enabled":true,"code":"frame*1000"}},
        {"property":"rotationX","static":[0]},{"property":"rotationY","static":[0]}]);
    source
}
#[test]
fn real_common_3d_replaces_2d_and_suppresses_original_layer_opacity_and_stroke_scale() {
    for legacy in [false, true] {
        let mut source = three_d(legacy);
        if !legacy {
            for record in source["objects"].as_array_mut().unwrap() {
                record["vector"]["stroke"] = json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
                record["vector"]["hasRealStroke"] = json!(true);
                record["vector"]["strokeWidth"] = json!(2);
            }
        }
        let o = owner(&source);
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        assert!(prepare_object_exposure_frame(&s, &pins(&s, 2)).is_err());
        let p = packet(&o, &s, 2);
        let plan = plan(&p);
        let target = crate::object_document::PropertyHolderTarget {
            context_id: "scene-root".into(),
            layer_uid: "L1".into(),
            stroke_id: crate::object_document::Presence::Missing,
        };
        // Original outer2D Motion still evaluates opacity before3D suppresses its paint factor.
        assert_eq!(p.sampled_motion_values(&target).unwrap().opacity, 22.);
        // Position is consumed once by3D: global x becomes25/225 at depth2, not a second2D transform.
        assert_eq!(
            starts(&plan),
            [("first".into(), [25., 50.]), ("second".into(), [225., 50.])]
        );
        for command in plan.commands() {
            assert_eq!(
                command.solid().channels()[3].as_f64(),
                Some(if legacy { 0.75 } else { 1. })
            );
            if !legacy {
                assert_eq!(command.style().unwrap().stroke.unwrap().stroke.width, 2.);
            }
        }
        if legacy {
            let text = svg(&plan).unwrap();
            assert!(text.contains("<path d=\"M25,50 C25,50 125,50 125,50"));
            assert!(text.contains("fill-opacity=\"0.75\""));
        }
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    }
}

#[test]
fn actual_scene_refuses_unjoined_scaled_stroke_gradient_and_legacy_pathfx_combinations() {
    for (kind, reason) in [
        ("stroke", "scaled vector stroke awaits"),
        ("gradient", "transformed vector gradient awaits"),
        ("legacy-pathfx", "layer path effects on a legacy record"),
    ] {
        let mut source = drawing(kind == "legacy-pathfx");
        match kind {
            "stroke" => {
                source["objects"][0]["vector"]["stroke"] =
                    json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
                source["objects"][0]["vector"]["hasRealStroke"] = json!(true);
            }
            "gradient" => {
                source["objects"][0]["vectorExt"] = json!({"fillGradient":{
                "kind":"linear","from":[0,0],"to":[100,60],"stops":[
                    {"offset":0,"color":"#000000"},{"offset":1,"color":"#ffffff"}]}})
            }
            _ => source["layers"][0]["pathFx"] = json!([{"type":"zigzag","enabled":true,"p1":2}]),
        }
        let o = owner(&source);
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        let p = packet(&o, &s, 0);
        let message = format!(
            "{:?}",
            prepare_object_render_scene(&p, &pins(&s, 0)).unwrap_err()
        );
        assert!(message.contains(reason), "{kind}: {message}");
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
}

#[test]
fn stale_common_plan_pin_does_not_publish_a_cached_projection_on_a_different_frame() {
    let o = owner(&drawing(true));
    let s = o.acquire_snapshot(0).unwrap();
    let packet = packet(&o, &s, 0);
    assert!(prepare_object_render_scene(&packet, &pins(&s, 2)).is_err());
    let fresh = packet.clone();
    let plan = plan(&fresh);
    assert_eq!(starts(&plan)[0], ("first".into(), [-155., 7.]));
}

#[test]
fn unjoined_second_record_paint_preserves_its_actual_source_index_and_atomic_scene_refusal() {
    let mut source = drawing(false);
    source["objects"][1]["vector"]["stroke"] = json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
    source["objects"][1]["vector"]["hasRealStroke"] = json!(true);
    let o = owner(&source);
    let s = o.acquire_snapshot(0).unwrap();
    let p = packet(&o, &s, 0);
    assert!(matches!(
        prepare_object_render_scene(&p, &pins(&s, 0)),
        Err(
            crate::object_render_scene::ObjectSceneError::UnsupportedGeometry {
                source_object_index: 1,
                reason: "scaled vector stroke awaits its actual paint width and dash consumer"
            }
        )
    ));
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn shared_motion_admission_refuses_mask_and_multiple_parent_controls_without_publishing() {
    use crate::evaluation::object_vector_kernels::render_vector_eval::common_layer_motion_target;
    use crate::object_document::{Presence, PropertyHolderTarget};
    let target = PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: "L1".into(),
        stroke_id: Presence::Missing,
    };
    for legacy in [false, true] {
        let o = owner(&drawing(legacy));
        let s = o.acquire_snapshot(0).unwrap();
        assert!(common_layer_motion_target(s.document(), &target));
    }
    for kind in ["mask", "parentBlend"] {
        let mut source = three_d(false);
        if kind == "mask" {
            source["objects"][0]["vectorExt"] = json!({"isMask":true,"maskMode":"add"});
        } else {
            source["propertyHolders"][0]["properties"]
                .as_array_mut()
                .unwrap()
                .push(json!({"property":kind,"static":[50]}));
        }
        let o = owner(&source);
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        assert!(!common_layer_motion_target(s.document(), &target), "{kind}");
        let mut effects = None;
        assert!(
            prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects)
                .is_err(),
            "{kind}"
        );
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
    // Unsupported descriptor bytes cannot become an apparently single-parent native layer.
    let mut source = three_d(false);
    source["layers"][0]["parentLayerUid2"] = json!("L2");
    let project = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":400,"canvasH":200,"fps":24},"drawing":source,
        "viewportReference":null,"assets":[]});
    assert!(NativeObjectHistory::from_project_file(
        "ordinary-batch",
        project.to_string().as_bytes()
    )
    .is_err());
}

#[path = "object_scene_vector_parent.rs"]
mod parent_tests;
