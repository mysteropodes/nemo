//! L08-G CPU-only cases: plan -> stack admission, typed refusals and the flat
//! fast-path gate. No adapter is needed; every literal is derived by hand from
//! `composite_scene` and the L08-A planner contract, independent of the oracle.
use super::support::{find, stack, Geometry};
use crate::compositor::scene_effect_gpu_blend::{
    GpuBlendError, GpuFill, GpuLayerInput, GpuLayerPaint, GpuLayerStack, GpuMask, GpuMaskMode,
    GpuStep, GpuUnavailable,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    plan_composition, CompositionPlan, LayerComposition, LayerStep, MatteStep, PaintSource,
    PaintStep,
};
use vello::kurbo::{BezPath, Rect, Shape};

fn square() -> BezPath {
    Rect::new(0.0, 0.0, 4.0, 4.0).to_path(0.1)
}

fn fill(path: &BezPath) -> GpuFill<'_> {
    GpuFill {
        path,
        rgba: [0.2, 0.4, 0.6, 1.0],
    }
}

fn input(path: &BezPath) -> GpuLayerInput<'_> {
    GpuLayerInput {
        paint: GpuLayerPaint {
            fills: vec![fill(path)],
        },
        ..Default::default()
    }
}

fn plain(index: usize) -> PaintStep {
    PaintStep {
        index,
        source: PaintSource::Plain,
        mask_count: 0,
        mattes: Vec::new(),
        layer_effects: false,
        blend: 0,
    }
}

fn layered(steps: Vec<LayerStep>) -> CompositionPlan {
    CompositionPlan::Layered {
        background: None,
        steps,
    }
}

#[test]
fn a_flat_plan_is_refused_so_the_object_compositor_fast_path_keeps_painting_it() {
    let path = square();
    // No blend, matte, mask, effect or folder anywhere: the baseline predicate is false.
    let flat = plan_composition(&[LayerComposition::default(), LayerComposition::default()]);
    assert_eq!(flat, CompositionPlan::Flat { order: vec![0, 1] });
    let result = GpuLayerStack::from_plan(&flat, vec![input(&path), input(&path)]);
    assert_eq!(result.unwrap_err(), GpuBlendError::FlatPlan);
    // An authored "normal" blend is not a blend for the baseline predicate either.
    let normal = LayerComposition {
        blend_mode: Some("normal".to_owned()),
        ..Default::default()
    };
    assert!(matches!(
        plan_composition(&[normal]),
        CompositionPlan::Flat { .. }
    ));
}

#[test]
fn matte_scene_plans_to_literal_steps_and_skips_the_consumed_source() {
    let case = find("matte-alpha");
    let geometry = Geometry::of(&case);
    let layered = stack(&case, &geometry).unwrap();
    assert_eq!(layered.background(), None);
    assert_eq!(
        layered.steps(),
        &[
            GpuStep {
                layer: 0,
                mattes: Vec::new(),
                blend: 0
            },
            GpuStep {
                layer: 1,
                mattes: vec![MatteStep {
                    source: 2,
                    mode: 0,
                    invert: false
                }],
                blend: 0
            },
        ]
    );
    // base layer 1 fill + orange 1 fill + matte source 2 fills.
    assert_eq!(layered.layer_fill_count(), 4);
}

#[test]
fn background_blend_and_mask_scenes_plan_to_literal_steps() {
    let case = find("canvas-background-is-not-the-blend-backdrop");
    let geometry = Geometry::of(&case);
    let layered = stack(&case, &geometry).unwrap();
    assert_eq!(layered.background(), Some(0));
    assert_eq!(
        layered.steps(),
        &[GpuStep {
            layer: 1,
            mattes: Vec::new(),
            blend: 1
        }]
    );
    let case = find("blend-hue");
    let geometry = Geometry::of(&case);
    let layered = stack(&case, &geometry).unwrap();
    assert_eq!(layered.steps()[1].blend, 12);
    assert_eq!(layered.layer_fill_count(), 6);
    let case = find("mask-add-then-subtract");
    let geometry = Geometry::of(&case);
    let layered = stack(&case, &geometry).unwrap();
    assert_eq!(layered.inputs()[1].masks.len(), 2);
    assert_eq!(layered.inputs()[1].masks[1].mode, GpuMaskMode::Subtract);
}

#[test]
fn an_inert_effect_layer_adds_no_step_and_an_enabled_one_is_refused() {
    let case = find("effect-layer-without-effects-passes-through");
    let geometry = Geometry::of(&case);
    let layered = stack(&case, &geometry).unwrap();
    let painted: Vec<usize> = layered.steps().iter().map(|s| s.layer).collect();
    assert_eq!(painted, vec![0, 2]);
    let path = square();
    let plan = layered_plan_with_effect(true);
    let error = GpuLayerStack::from_plan(&plan, vec![input(&path), input(&path)]).unwrap_err();
    assert_eq!(
        error,
        GpuBlendError::Unavailable(GpuUnavailable::EffectLayerStack { layer: 1 })
    );
}

fn layered_plan_with_effect(applies_stack: bool) -> CompositionPlan {
    layered(vec![
        LayerStep::Paint(plain(0)),
        LayerStep::EffectLayer {
            index: 1,
            applies_stack,
        },
    ])
}

#[test]
fn every_unrendered_baseline_feature_is_a_typed_refusal_before_any_allocation() {
    let path = square();
    let step = |source: PaintSource, layer_effects: bool| {
        layered(vec![LayerStep::Paint(PaintStep {
            source,
            layer_effects,
            ..plain(0)
        })])
    };
    let cases = [
        (
            step(PaintSource::ElementEffects, false),
            GpuUnavailable::ElementEffects { layer: 0 },
        ),
        (
            step(PaintSource::Folder(Vec::new()), false),
            GpuUnavailable::FolderLayer { layer: 0 },
        ),
        (
            step(PaintSource::Plain, true),
            GpuUnavailable::LayerEffects { layer: 0 },
        ),
    ];
    for (plan, expected) in cases {
        let error = GpuLayerStack::from_plan(&plan, vec![input(&path)]).unwrap_err();
        assert_eq!(error, GpuBlendError::Unavailable(expected));
        assert!(error.message().contains("unavailable"));
    }
}

#[test]
fn mask_feather_above_the_baseline_threshold_is_refused_at_or_below_it_is_accepted() {
    let path = square();
    let plan = layered(vec![LayerStep::Paint(PaintStep {
        mask_count: 1,
        ..plain(0)
    })]);
    let with_feather = |feather: f32| {
        let mut layer = input(&path);
        layer.masks.push(GpuMask {
            mode: GpuMaskMode::Add,
            paint: GpuLayerPaint {
                fills: vec![fill(&path)],
            },
        });
        layer.mask_feather = feather;
        GpuLayerStack::from_plan(&plan, vec![layer])
    };
    for accepted in [0.0, 0.01, -3.0, f32::NAN] {
        assert!(with_feather(accepted).is_ok(), "{accepted}");
    }
    for refused in [0.011, 2.0, 30.0, f32::INFINITY] {
        assert_eq!(
            with_feather(refused).unwrap_err(),
            GpuBlendError::Unavailable(GpuUnavailable::MaskFeather { layer: 0 }),
            "{refused}"
        );
    }
}

#[test]
fn inconsistent_inputs_are_refused_with_the_offending_index() {
    let path = square();
    let two = layered(vec![LayerStep::Paint(plain(0)), LayerStep::Paint(plain(1))]);
    assert_eq!(
        GpuLayerStack::from_plan(&two, vec![input(&path)]).unwrap_err(),
        GpuBlendError::LayerCountMismatch {
            planned: 2,
            supplied: 1
        }
    );
    let masked = layered(vec![LayerStep::Paint(PaintStep {
        mask_count: 2,
        ..plain(0)
    })]);
    assert_eq!(
        GpuLayerStack::from_plan(&masked, vec![input(&path)]).unwrap_err(),
        GpuBlendError::MaskCountMismatch {
            layer: 0,
            planned: 2,
            supplied: 0
        }
    );
    let dangling = layered(vec![LayerStep::Paint(PaintStep {
        mattes: vec![MatteStep {
            source: 9,
            mode: 0,
            invert: false,
        }],
        ..plain(0)
    })]);
    assert_eq!(
        GpuLayerStack::from_plan(&dangling, vec![input(&path)]).unwrap_err(),
        GpuBlendError::IndexOutOfRange { index: 9 }
    );
    let background = CompositionPlan::Layered {
        background: Some(5),
        steps: vec![LayerStep::Paint(plain(0))],
    };
    assert_eq!(
        GpuLayerStack::from_plan(&background, vec![input(&path)]).unwrap_err(),
        GpuBlendError::IndexOutOfRange { index: 5 }
    );
}

#[test]
fn non_finite_or_out_of_range_paint_is_refused_in_layers_and_masks() {
    let path = square();
    let plan = layered(vec![LayerStep::Paint(PaintStep {
        mask_count: 1,
        ..plain(0)
    })]);
    for bad in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        let mut layer = input(&path);
        layer.paint.fills[0].rgba[3] = bad;
        assert_eq!(
            GpuLayerStack::from_plan(&plan_without_mask(), vec![layer]).unwrap_err(),
            GpuBlendError::InvalidPaint { layer: 0 },
            "layer alpha {bad}"
        );
        let mut layer = input(&path);
        let mut mask_fill = fill(&path);
        mask_fill.rgba[0] = bad;
        layer.masks.push(GpuMask {
            mode: GpuMaskMode::Intersect,
            paint: GpuLayerPaint {
                fills: vec![mask_fill],
            },
        });
        assert_eq!(
            GpuLayerStack::from_plan(&plan, vec![layer]).unwrap_err(),
            GpuBlendError::InvalidPaint { layer: 0 },
            "mask red {bad}"
        );
    }
}

fn plan_without_mask() -> CompositionPlan {
    layered(vec![LayerStep::Paint(plain(0))])
}

#[test]
fn errors_render_a_message_naming_the_cause() {
    assert!(GpuBlendError::FlatPlan.message().contains("fast path"));
    assert!(GpuBlendError::IndexOutOfRange { index: 7 }
        .message()
        .contains('7'));
    assert!(GpuBlendError::NonContiguousLayer { source_layer: 3 }
        .message()
        .contains("two separate runs"));
}
