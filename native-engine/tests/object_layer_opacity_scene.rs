//! Actual immutable owner/frame/scene/paint controls. Authored UNRUN: no compiler/GPU credit.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, ObjectFrameError, ObjectFrameSelector,
};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn source(properties: Value) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut a = cases["records"][0].clone();
    a["target"] = json!({"contextId":"scene-root","layerUid":"L0","strokeId":"a","frameScope":{"kind":"authored","frame":0}});
    a["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":0.75});
    let mut b = a.clone();
    b["target"]["strokeId"] = json!("b");
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":16,
        "layers":[{"layerUid":"L0","timing":timing}],"objects":[a,b],
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},"properties":properties}],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
          "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
            "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
              {"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}},
              {"layerUid":"L0","strokeId":"b","naturalPosition":1,"order":{}}]}]}]}})
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "opacity-scene",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
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
fn plan(s: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let selector = pins(s, frame);
    let packet = prepare_object_exposure_frame(s, &selector).unwrap();
    let scene = prepare_object_render_scene(&packet, &selector).unwrap();
    prepare_object_paint_plan(&scene, &selector).unwrap()
}
fn alpha(plan: &ObjectPaintPlan) -> Vec<f64> {
    plan.commands()
        .map(|command| command.paint()[3].as_f64().unwrap())
        .collect()
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}
fn edit(o: &mut NativeObjectHistory, value: f64) -> u64 {
    let bytes = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L0"},
        "property":"opacity","action":{"kind":"setValue","frame":0,"values":[value]}}).to_string();
    let candidate = o.prepare_property_edit_json(bytes.as_bytes()).unwrap();
    o.commit_property_edit(candidate).unwrap()
}

#[test]
fn overlapping_children_receive_the_factor_before_composition_and_source_numbers_stay_immutable() {
    let o = owner(&source(json!([{"property":"opacity","static":[50]}])));
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let p = plan(&s, 7);
    assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
    assert_eq!(p.packet().frame(), 7);
    assert_eq!(alpha(&p), [0.375, 0.375]);
    // Two overlapping paints: baseline per-item alpha, deliberately distinct from group opacity.
    close(1.0 - (1.0 - alpha(&p)[0]) * (1.0 - alpha(&p)[1]), 0.609375);
    assert_ne!(0.609375, (1.0 - (1.0 - 0.75) * (1.0 - 0.75)) * 0.5);
    for command in p.commands() {
        assert_eq!(
            command.source().record().solid_fill().unwrap().a.as_f64(),
            Some(0.75)
        );
        close(
            command.style().unwrap().fill.solid().unwrap().channels()[3]
                .as_f64()
                .unwrap(),
            0.375,
        );
        assert_eq!(command.solid().rgba_f32().unwrap()[3], 0.375);
    }
    let witness = prepare_object_composition(&p, &pins(&s, 7)).unwrap();
    assert_eq!(alpha(witness.plan()), [0.375, 0.375]);
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn held_drawing_uses_true_evaluation_time_for_linear_hold_default_and_custom_easing() {
    let cases = [
        (
            json!({"frame":0,"v":[0],"curvePoints":[]}),
            vec![(0, 0.0), (2, 0.2), (5, 0.5), (10, 1.0), (15, 1.0)],
        ),
        (
            json!({"frame":0,"v":[0],"hold":true}),
            vec![(0, 0.0), (5, 0.0), (9, 0.0), (10, 1.0)],
        ),
        (
            json!({"frame":0,"v":[0]}),
            vec![(0, 0.0), (2, 0.11321757498376982), (5, 0.5), (10, 1.0)],
        ),
        (
            json!({"frame":0,"v":[0],"curvePoints":[{"x":0,"y":0,"tx":1,"ty":0},{"x":1,"y":1,"tx":1,"ty":0}]}),
            vec![(0, 0.0), (2, 0.104), (5, 0.5), (10, 1.0)],
        ),
        (
            json!({"frame":-2,"v":[0],"curvePoints":[]}),
            vec![(0, 1.0 / 6.0), (4, 0.5), (10, 1.0)],
        ),
    ];
    for (first, expected) in cases {
        let o = owner(&source(
            json!([{"property":"opacity","track":{"keys":[first,{"frame":10,"v":[100]}]}}]),
        ));
        let s = o.acquire_snapshot(0).unwrap();
        let original = encode_storage_project(s.document()).unwrap();
        for (frame, factor) in expected {
            let p = plan(&s, frame);
            assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
            close(p.packet().layer_opacity(0).unwrap(), factor);
            for a in alpha(&p) {
                close(a, 0.75 * factor);
            }
        }
        assert_eq!(encode_storage_project(s.document()).unwrap(), original);
        assert_eq!(o.history_depths(), (0, 0));
    }
}

#[test]
fn absent_default_empty_track_negative_and_positive_overshoot_keep_baseline_alpha_rules() {
    for (property, factor, expected) in [
        (json!({"property":"opacity"}), 1.0, 0.75),
        (json!({"property":"opacity","track":{"keys":[]}}), 1.0, 0.75),
        (
            json!({"property":"opacity","static":[25],"track":{"keys":[]}}),
            0.25,
            0.1875,
        ),
        (json!({"property":"opacity","static":[-50]}), 0.0, 0.0),
        (json!({"property":"opacity","static":[200]}), 2.0, 1.0),
    ] {
        let o = owner(&source(json!([property])));
        let s = o.acquire_snapshot(0).unwrap();
        let p = plan(&s, 5);
        close(p.packet().layer_opacity(0).unwrap(), factor);
        assert_eq!(alpha(&p), [expected, expected]);
        if factor == 1.0 {
            let c = p.commands().next().unwrap();
            assert!(std::ptr::eq(
                c.paint()[3],
                &c.source().record().solid_fill().unwrap().a
            ));
        }
    }
    let mut no_holder = source(json!([]));
    no_holder.as_object_mut().unwrap().remove("propertyHolders");
    let s = owner(&no_holder).acquire_snapshot(0).unwrap();
    assert_eq!(alpha(&plan(&s, 7)), [0.75, 0.75]);
}

#[test]
fn vector_item_fill_and_stroke_opacity_multiply_layer_factor_once() {
    let mut value = source(json!([{"property":"opacity","static":[25]}]));
    for record in value["objects"].as_array_mut().unwrap() {
        record["family"] = json!("editable-vector-path");
        record["vector"] = json!({"stroke":{"kind":"solid","r":0,"g":0,"b":1,"a":0.8},"hasRealStroke":true,
            "strokeWidth":2,"strokeCap":"round","strokeJoin":"miter","opacity":0.5,"paintOrder":"strokeFirst"});
    }
    let s = owner(&value).acquire_snapshot(0).unwrap();
    let p = plan(&s, 5);
    for c in p.commands() {
        let style = c.style().unwrap();
        close(
            style.fill.solid().unwrap().channels()[3].as_f64().unwrap(),
            0.09375,
        );
        close(
            style.stroke.unwrap().paint.channels()[3].as_f64().unwrap(),
            0.1,
        );
    }
    assert_eq!(alpha(&p), [0.09375, 0.09375]);
}

#[test]
fn unsupported_properties_controls_enabled_expressions_and_nonordinary_layers_stay_closed() {
    let base = source(json!([{"property":"opacity","static":[50]}]));
    let mut position = base.clone();
    position["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"position","static":[0,0]}));
    let mut element = base.clone();
    element["propertyHolders"][0]["target"]["strokeId"] = json!("a");
    let mut active = base.clone();
    active["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled":true,"code":"50"});
    let mut controls = base.clone();
    controls["propertyHolders"][0]["controls"] =
        json!([{"key":"xc_a","name":"a","type":"number","default":[1]}]);
    let mut folder = base.clone();
    folder["layers"][0]["isFolderLayer"] = json!(true);
    let mut effect = base.clone();
    effect["layers"][0]["isEffectLayer"] = json!(true);
    // Kind layers cannot own objects at codec admission. Keep these negative twins valid.
    for value in [&mut folder, &mut effect] {
        value["objects"] = json!([]);
        value["compositionOrder"]["contexts"][0]["scopes"] = json!([]);
    }
    let mut empty = base.clone();
    empty["propertyHolders"][0]["properties"] = json!([]);
    for value in [position, element, active, controls, folder, effect, empty] {
        let o = owner(&value);
        let s = o.acquire_snapshot(0).unwrap();
        let bytes = encode_storage_project(s.document()).unwrap();
        assert_eq!(
            prepare_object_exposure_frame(&s, &pins(&s, 7)).unwrap_err(),
            ObjectFrameError::Unavailable
        );
        assert_eq!(encode_storage_project(s.document()).unwrap(), bytes);
        assert_eq!(o.history_depths(), (0, 0));
    }
    let mut disabled = base;
    disabled["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled":false,"code":"throw 1"});
    let s = owner(&disabled).acquire_snapshot(0).unwrap();
    assert_eq!(alpha(&plan(&s, 7)), [0.375, 0.375]);
}

#[test]
fn owner_edits_undo_redo_reopen_and_stale_pins_do_not_mutate_retained_plans() {
    let mut o = owner(&source(json!([{"property":"opacity","static":[100]}])));
    let original = o.acquire_snapshot(0).unwrap();
    let original_bytes = encode_storage_project(original.document()).unwrap();
    let old = plan(&original, 7);
    assert_eq!(edit(&mut o, 50.0), 1);
    let s = o.acquire_snapshot(1).unwrap();
    let changed = plan(&s, 7);
    assert_eq!(alpha(&changed), [0.375, 0.375]);
    assert_eq!(alpha(&old), [0.75, 0.75]);
    let stale = pins(&original, 7);
    assert!(prepare_object_composition(&changed, &stale).is_err());
    for bad in [0, 1, 2, 3, 4] {
        let mut wrong = pins(&s, 7);
        match bad {
            0 => wrong.instance_id = "foreign",
            1 => wrong.document_id = "foreign",
            2 => wrong.content_revision = 0,
            3 => wrong.document_snapshot_id = "foreign",
            _ => wrong.frame = 6,
        };
        assert!(prepare_object_composition(&changed, &wrong).is_err());
    }
    let bytes = encode_storage_project(s.document()).unwrap();
    let reopened =
        NativeObjectHistory::new("reopened", decode_storage_project(&bytes).unwrap()).unwrap();
    assert_eq!(
        alpha(&plan(&reopened.acquire_snapshot(0).unwrap(), 7)),
        [0.375, 0.375]
    );
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(
        alpha(&plan(&o.acquire_snapshot(2).unwrap(), 7)),
        [0.75, 0.75]
    );
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(
        alpha(&plan(&o.acquire_snapshot(3).unwrap(), 7)),
        [0.375, 0.375]
    );
    assert_eq!(edit(&mut o, 50.0), 3);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        encode_storage_project(original.document()).unwrap(),
        original_bytes
    );
    assert_eq!(alpha(&old), [0.75, 0.75]);
}

#[cfg(feature = "export_job")]
#[test]
fn svg_reads_the_same_evaluated_alpha_and_keeps_pins_without_an_extra_opacity_group() {
    use crate::export_job::native_vector_export::native_vector_export_svg::{
        serialize_svg_frame, SvgFrameSpec,
    };
    let s = owner(&source(json!([{"property":"opacity","static":[50]}])))
        .acquire_snapshot(0)
        .unwrap();
    let p = plan(&s, 7);
    let svg = serialize_svg_frame(&p, SvgFrameSpec::new(64, 64, 8192).unwrap()).unwrap();
    let text = std::str::from_utf8(&svg.bytes).unwrap();
    assert_eq!(text.matches("fill-opacity=\"0.375\"").count(), 2);
    assert!(!text.contains("<g"));
    assert_eq!(svg.pins.frame, 7);
    assert_eq!(svg.pins.document_snapshot_id, s.snapshot_id());
    assert_eq!(svg.path_count, 2);
}

#[test]
fn fractional_authored_key_times_survive_the_integer_presentation_clock() {
    let o = owner(&source(json!([{"property":"opacity","track":{"keys":[
        {"frame":-0.5,"v":[0],"curvePoints":[]},{"frame":9.5,"v":[100]}]}}])));
    let s = o.acquire_snapshot(0).unwrap();
    assert_eq!(alpha(&plan(&s, 2)), [0.1875, 0.1875]);
    assert_eq!(plan(&s, 2).packet().source_frame_for_layer(0), Some(0));
}

#[cfg(all(
    feature = "viewport",
    feature = "export_job",
    feature = "application",
    feature = "project_media_codec"
))]
#[path = "object_layer_opacity_gpu.rs"]
mod gpu;

#[test]
fn admitted_native_range_selects_exposure_and_samples_opacity_at_the_immutable_query_frame() {
    let mut value = source(json!([{"property":"opacity","track":{"keys":[
        {"frame":0,"v":[20],"curvePoints":[]},{"frame":10,"v":[80]}]}}]));
    value["layers"][0]["range"] = json!({"inPoint":3,"outPoint":7});
    let o = owner(&value);
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let pin = s.snapshot_id().to_owned();
    for (frame, visible, factor) in [
        (2, false, 0.32),
        (3, true, 0.38),
        (5, true, 0.5),
        (7, true, 0.62),
        (8, false, 0.68),
        (3, true, 0.38),
    ] {
        let packet = prepare_object_exposure_frame(&s, &pins(&s, frame)).unwrap();
        assert_eq!(packet.frame(), frame);
        assert_eq!(packet.source_frame_for_layer(0), visible.then_some(0));
        assert_eq!(packet.records().len(), if visible { 2 } else { 0 });
        close(packet.layer_opacity(0).unwrap(), factor);
        let painted = alpha(&plan(&s, frame));
        assert_eq!(painted.len(), if visible { 2 } else { 0 });
        for alpha in painted {
            close(alpha, 0.75 * factor);
        }
        assert_eq!(packet.document_snapshot_id(), pin);
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
}
