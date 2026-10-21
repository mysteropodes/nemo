//! L08-K compositor-layer join on real documents: the real paint plan, the real pinned scene and the real
//! `GpuLayerStack` validation of L08-G (CPU only: no device, texture or scheduler is created here).
use super::support::{
    base_document, contour_claims, packet, paint_plan, pinned, render_scene, set_layer, snapshot,
    FRAME,
};
use super::{select_composition_path, PathSelection, PathSelectionError};
use crate::compositor::scene_effect_gpu_blend::{GpuBlendError, GpuUnavailable};
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::MatteStep;
use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::{
    plan_scene_effects, JoinOptions, SceneEffectPlan,
};
use crate::object_render_scene::ObjectPaintPlan;
use serde_json::{json, Value};

fn no_custom_effects(_: &str) -> bool {
    false
}

static NO_CUSTOM: fn(&str) -> bool = no_custom_effects;

/// The paint plan and the join of one document, both from one packet; `skip` records are drawn by no producer.
fn plans(document: &Value, instance: &str, skip: &[usize]) -> (ObjectPaintPlan, SceneEffectPlan) {
    let snap = snapshot(document, instance);
    let pk = packet(&snap, FRAME);
    let scene = render_scene(&pk);
    let joined = pinned(&pk, vec![contour_claims(&scene, "contour", skip)]);
    let join = plan_scene_effects(&pk, &joined, &JoinOptions::new(1.0, &NO_CUSTOM)).unwrap();
    (paint_plan(&scene), join)
}

#[test]
fn a_flat_scene_keeps_the_fast_path() {
    let (plan, join) = plans(&base_document(), "stack-flat", &[]);
    assert!(matches!(
        select_composition_path(&join, &plan),
        Ok(PathSelection::Flat)
    ));
}

#[test]
fn blends_become_a_layered_stack_with_fills_at_their_paint_order_wire_index() {
    let mut document = base_document();
    set_layer(&mut document, "L1", json!({"blendMode": "multiply"}));
    set_layer(&mut document, "L2", json!({"blendMode": "screen"}));
    let (plan, join) = plans(&document, "stack-blend", &[]);
    let Ok(PathSelection::Layered(stack)) = select_composition_path(&join, &plan) else {
        panic!("blend modes need the layered path");
    };
    // The document stores L2, L0, L1; wire order is L0, L1, L2 with 3, 1 and 1 commands.
    let fills: Vec<usize> = stack.inputs().iter().map(|i| i.paint.fills.len()).collect();
    assert_eq!(fills, [3, 1, 1]);
    assert_eq!(stack.layer_fill_count(), 5);
    let steps: Vec<(usize, u32)> = stack.steps().iter().map(|s| (s.layer, s.blend)).collect();
    assert_eq!(steps, [(0, 0), (1, 1), (2, 2)]);
    assert!(stack.covers_plan(&plan));
}

#[test]
fn a_matte_source_paints_only_through_its_matte() {
    let mut document = base_document();
    set_layer(
        &mut document,
        "L0",
        json!({"matteMode": "alpha", "matteSourceLayerUid": "L1"}),
    );
    let (plan, join) = plans(&document, "stack-matte", &[]);
    let Ok(PathSelection::Layered(stack)) = select_composition_path(&join, &plan) else {
        panic!("a matte needs the layered path");
    };
    let steps: Vec<usize> = stack.steps().iter().map(|s| s.layer).collect();
    assert_eq!(steps, [0, 2], "the matte source is not a paint step");
    assert_eq!(
        stack.steps()[0].mattes,
        vec![MatteStep {
            source: 1,
            mode: 0,
            invert: false
        }]
    );
    assert_eq!(
        stack.inputs()[1].paint.fills.len(),
        1,
        "its fills stay available to the matte pass"
    );
    assert!(stack.covers_plan(&plan));
}

#[test]
fn an_enabled_layer_effect_is_a_typed_unavailability_not_a_missing_effect() {
    let mut document = base_document();
    set_layer(&mut document, "L1", json!({"blendMode": "multiply"}));
    set_layer(
        &mut document,
        "L2",
        json!({"effects": [{"type": "blur", "enabled": true, "p1": 4}]}),
    );
    let (plan, join) = plans(&document, "stack-effects", &[]);
    assert_eq!(
        select_composition_path(&join, &plan).unwrap_err(),
        PathSelectionError::Stack(GpuBlendError::Unavailable(GpuUnavailable::LayerEffects {
            layer: 2
        }))
    );
}

#[test]
fn a_record_no_producer_drew_refuses_the_whole_selection_even_on_the_flat_path() {
    let snap = snapshot(&base_document(), "stack-incomplete");
    let pk = packet(&snap, FRAME);
    let victim = pk.records().next().unwrap();
    let (layer, object) = (victim.source_layer_index(), victim.source_object_index());
    let (plan, join) = plans(&base_document(), "stack-incomplete", &[object]);
    assert_eq!(
        select_composition_path(&join, &plan).unwrap_err(),
        PathSelectionError::Incomplete {
            source_layer_index: layer,
            source_object_index: object
        }
    );
}

#[test]
fn a_join_from_another_packet_cannot_select_a_path_for_this_plan() {
    let document = base_document();
    let (_, join_a) = plans(&document, "stack-pin-a", &[]);
    let (plan_b, _) = plans(&document, "stack-pin-b", &[]);
    assert_eq!(
        select_composition_path(&join_a, &plan_b).unwrap_err(),
        PathSelectionError::WrongPlan
    );
}

// Original Path.data mask contract at fbe4b8a3 engine-bridge.js:2024-2035.
// Actual stored records traverse codec/snapshot/Frame/scene/paint/join above.
fn stored_mask_document(feather: f64) -> Value {
    let mut value = base_document();
    for record in value["objects"].as_array_mut().unwrap() {
        if record["target"]["layerUid"] != "L0" {
            continue;
        }
        record["family"] = json!("editable-vector-path");
        record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
            "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":0});
        record["fill"] = Value::Null;
        let mode = match record["target"]["strokeId"].as_str().unwrap() {
            "c" => "intersect",
            "b" => "subtract",
            "a" => "unknown-add-fallback",
            _ => unreachable!(),
        };
        record["vectorExt"] = json!({"isMask":true,"maskMode":mode,"maskFeather":feather});
        record["effects"] = json!([{"type":"blur","enabled":true,"p1":3}]);
    }
    value
}

#[test]
fn actual_stored_masks_become_borrowed_white_gpu_inputs_without_ordinary_paint_leak() {
    use crate::compositor::scene_effect_gpu_blend::GpuMaskMode;
    use crate::object_codec::encode_storage_project;
    use crate::object_render_scene::ordered_entries;
    let (plan, join) = plans(&stored_mask_document(0.), "stack-actual-masks", &[]);
    let before = encode_storage_project(plan.packet().snapshot().document()).unwrap();
    let PathSelection::Layered(stack) = select_composition_path(&join, &plan).unwrap() else {
        panic!("actual masks must select their existing layered consumer");
    };
    assert!(join.is_complete());
    assert_eq!(
        join.layers()
            .iter()
            .map(|layer| layer.entries.len())
            .sum::<usize>(),
        5
    );
    assert_eq!(join.mask_count(1), Some(3));
    assert_eq!(join.mask_feather(1), Some(0.));
    assert_eq!(
        stack
            .inputs()
            .iter()
            .map(|i| i.masks.len())
            .collect::<Vec<_>>(),
        [3, 0, 0]
    );
    assert_eq!(
        stack
            .inputs()
            .iter()
            .map(|i| i.paint.fills.len())
            .collect::<Vec<_>>(),
        [0, 1, 1]
    );
    assert_eq!(stack.layer_fill_count(), 2);
    assert_eq!(plan.commands().len(), 2);
    assert_eq!(ordered_entries(&plan).count(), 2);
    assert!(plan.commands().all(|c| !c.source().record().is_mask()));
    let input = &stack.inputs()[0]; // stored source L0=1 resolves to actual wire slot0.
    assert_eq!(input.mask_feather, 0.);
    assert_eq!(
        input.masks.iter().map(|m| m.mode).collect::<Vec<_>>(),
        [
            GpuMaskMode::Intersect,
            GpuMaskMode::Subtract,
            GpuMaskMode::Add
        ]
    );
    let masks = plan.mask_inputs().layer(1).unwrap().masks();
    for (actual, retained) in input.masks.iter().zip(masks) {
        assert_eq!(actual.paint.fills.len(), 1);
        let fill = &actual.paint.fills[0];
        assert!(std::ptr::eq(fill.path, plan.mask_path(retained).unwrap()));
        assert_eq!(fill.rgba, [1.; 4]);
        assert!(
            fill.prepared.is_none(),
            "no authored paint, stroke or item effect in clip"
        );
    }
    assert!(stack.covers_plan(&plan));
    assert_eq!(
        encode_storage_project(plan.packet().snapshot().document()).unwrap(),
        before
    );
}

#[test]
fn actual_stored_mask_feather_keeps_the_original_threshold_and_typed_refusal() {
    let (plan, join) = plans(&stored_mask_document(0.01), "stack-mask-threshold", &[]);
    let PathSelection::Layered(stack) = select_composition_path(&join, &plan).unwrap() else {
        panic!("baseline feather at0.01 skips blur");
    };
    assert_eq!(stack.inputs()[0].mask_feather, 0.01f32);
    assert_eq!(join.mask_feather(1), Some(0.01));
    let (plan, join) = plans(&stored_mask_document(4.), "stack-mask-feather", &[]);
    assert_eq!(plan.mask_inputs().layer(1).unwrap().feather(), 4.);
    assert_eq!(join.mask_feather(1), Some(4.));
    assert_eq!(
        select_composition_path(&join, &plan).unwrap_err(),
        PathSelectionError::Stack(GpuBlendError::Unavailable(GpuUnavailable::MaskFeather {
            layer: 0
        }))
    );
}

#[test]
fn actual_mask_join_cannot_authorize_another_retained_packet_of_the_same_source() {
    let value = stored_mask_document(0.);
    let (first, join) = plans(&value, "stack-mask-same-instance", &[]);
    let (second, _) = plans(&value, "stack-mask-same-instance", &[]);
    assert!(first.mask_inputs().is_for_packet(first.packet()));
    assert!(!first.mask_inputs().is_for_packet(second.packet()));
    assert_eq!(
        select_composition_path(&join, &second).unwrap_err(),
        PathSelectionError::WrongPlan
    );
    assert!(matches!(
        select_composition_path(&join, &first),
        Ok(PathSelection::Layered(_))
    ));
}
