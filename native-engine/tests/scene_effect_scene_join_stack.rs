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
