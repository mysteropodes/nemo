//! L08-H CPU cases with hand-derived literals: the admission of an L08-C plan,
//! the texture needs it implies, the typed refusals and the blur kernel
//! weights. No GPU is touched here.
use super::reference::kernel;
use crate::compositor::scene_effect_gpu_pass::{
    GpuEffectError, GpuEffectNeeds, GpuEffectStack, GpuEffectUnavailable,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    plan_effect_stack, plan_effect_stack_into, EffectPass, EffectSpec, EffectStackPlan, Landing,
    PassSource, PassTarget,
};

fn spec(kind: &str, enabled: bool, values: &[Option<f32>]) -> EffectSpec {
    let mut params = [None; 8];
    for (slot, value) in params.iter_mut().zip(values) {
        *slot = *value;
    }
    EffectSpec::new(kind, enabled).with_params(params)
}

fn plan(specs: &[EffectSpec]) -> EffectStackPlan {
    plan_effect_stack(specs, 1.0, &|_| false)
}

fn plan_into(specs: &[EffectSpec]) -> EffectStackPlan {
    plan_effect_stack_into(specs, 1.0, &|_| false)
}

fn needs(a: bool, b: bool, scratch: bool, result: bool, blend: bool) -> GpuEffectNeeds {
    GpuEffectNeeds {
        stack_a: a,
        stack_b: b,
        blur_scratch: scratch,
        blur_result: result,
        glow_blend: blend,
    }
}

fn admitted(plan: &EffectStackPlan) -> GpuEffectStack<'_> {
    GpuEffectStack::from_plan(plan).expect("plan is admitted")
}

fn refused(plan: &EffectStackPlan) -> GpuEffectError {
    GpuEffectStack::from_plan(plan)
        .err()
        .expect("plan is refused")
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-6, "{actual} vs {expected}");
}

#[test]
fn blur_kernel_weights_match_the_gaussian_derived_from_the_shader_formula() {
    // sigma = radius / 3, weight(i) = exp(-i^2 / (2 sigma^2)); literals from the
    // closed form evaluated in double precision, tail sums ~ sigma * sqrt(2 pi).
    for (radius, head, total) in [
        (
            3.0,
            [1.0, 0.606_530_66, 0.135_335_28, 0.011_109, 0.000_335_46],
            2.506_628,
        ),
        (
            6.0,
            [1.0, 0.882_496_9, 0.606_530_66, 0.324_652_47, 0.135_335_28],
            5.013_257,
        ),
        (
            12.0,
            [1.0, 0.969_233_23, 0.882_496_9, 0.754_839_6, 0.606_530_66],
            10.026_513,
        ),
    ] {
        let (step, weights) = kernel(radius);
        assert_eq!(step, 1.0);
        assert_eq!(weights.len(), 49);
        for (k, expected) in head.iter().enumerate() {
            close(weights[24 + k], *expected);
            close(weights[24 - k], *expected);
        }
        let sum: f32 = weights.iter().sum();
        assert!(
            (sum - total).abs() < 1e-3,
            "radius {radius}: {sum} vs {total}"
        );
    }
}

#[test]
fn large_radii_widen_the_sample_step_and_keep_the_taps_gaussian() {
    // radius 48 -> step 2, sigma = 48 / 3 / 2 = 8: weight(8) = exp(-64 / 128).
    let (step, weights) = kernel(48.0);
    assert_eq!(step, 2.0);
    close(weights[24 + 8], 0.606_530_66);
    // radius 36 -> step 1.5 (the half-texel case of the oracle), sigma = 8.
    let (step, weights) = kernel(36.0);
    assert_eq!(step, 1.5);
    close(weights[24 + 8], 0.606_530_66);
    // radius 0 -> all weight on the centre tap: sigma floors at 0.0001.
    let (step, weights) = kernel(0.0);
    assert_eq!(step, 1.0);
    assert_eq!(weights[24], 1.0);
    assert_eq!(weights[25], 0.0);
}

#[test]
fn a_stack_with_nothing_enabled_is_refused_before_any_pass() {
    let all_off = plan(&[
        spec("blur", false, &[Some(3.0)]),
        spec("vignette", false, &[]),
    ]);
    assert_eq!(all_off.landing, Landing::Empty);
    assert_eq!(refused(&all_off), GpuEffectError::NothingToRun);
    assert_eq!(refused(&plan(&[])), GpuEffectError::NothingToRun);
    assert_eq!(refused(&plan_into(&[])), GpuEffectError::NothingToRun);
}

#[test]
fn texture_needs_follow_the_families_and_the_entry_variant() {
    let blur = plan(&[spec("blur", true, &[Some(4.0)])]);
    assert_eq!(
        admitted(&blur).needs(),
        needs(true, false, true, false, false)
    );
    let grades = plan(&[spec("colorAdjust", true, &[]), spec("vignette", true, &[])]);
    assert_eq!(
        admitted(&grades).needs(),
        needs(true, true, false, false, false)
    );
    let glow = plan(&[spec("glow", true, &[])]);
    assert_eq!(
        admitted(&glow).needs(),
        needs(true, false, true, true, true)
    );
    // The adjustment-layer shape writes straight into the caller's target.
    let single = plan_into(&[spec("colorAdjust", true, &[Some(0.2)])]);
    assert_eq!(admitted(&single).needs(), GpuEffectNeeds::default());
    let pair = plan_into(&[spec("colorAdjust", true, &[]), spec("vignette", true, &[])]);
    assert_eq!(
        admitted(&pair).needs(),
        needs(true, false, false, false, false)
    );
    let blur_into = plan_into(&[spec("unheard-of", true, &[Some(2.0)])]);
    assert_eq!(
        admitted(&blur_into).needs(),
        needs(false, false, true, false, false)
    );
}

#[test]
fn the_entry_variant_and_the_landing_follow_the_enabled_count() {
    let counts = [
        (1, Landing::StackA),
        (2, Landing::StackB),
        (3, Landing::StackA),
        (4, Landing::StackB),
    ];
    for (count, landing) in counts {
        let specs: Vec<EffectSpec> = (0..count).map(|_| spec("colorAdjust", true, &[])).collect();
        let stack_plan = plan(&specs);
        let stack = admitted(&stack_plan);
        assert_eq!(stack.landing(), landing, "{count} entries");
        assert!(!stack.into_final());
        let into_plan = plan_into(&specs);
        let into = admitted(&into_plan);
        assert_eq!(into.landing(), Landing::Final, "{count} entries");
        assert!(into.into_final());
        assert_eq!(into.steps().len(), count);
    }
}

#[test]
fn disabled_entries_leave_index_gaps_cost_no_pass_and_keep_authored_order() {
    let stack_plan = plan(&[
        spec("colorAdjust", true, &[Some(0.3), Some(0.2)]),
        spec("blur", false, &[Some(4.0)]),
        spec("vignette", true, &[Some(0.8), Some(0.3)]),
    ]);
    let stack = admitted(&stack_plan);
    let indices: Vec<usize> = stack.steps().iter().map(|step| step.effect_index).collect();
    assert_eq!(indices, [0, 2]);
    assert_eq!(stack.dispatch_count(), 2);
    assert_eq!(stack.needs(), needs(true, true, false, false, false));
    let dense = plan(&[
        spec("colorAdjust", true, &[]),
        spec("blur", true, &[Some(1.0)]),
        spec("glow", true, &[Some(1.0)]),
        spec("vignette", true, &[]),
    ]);
    assert_eq!(admitted(&dense).dispatch_count(), 1 + 2 + 3 + 1);
}

#[test]
fn the_planner_hand_off_scales_radii_by_the_zoom_and_keeps_defaults() {
    let zoomed = plan_effect_stack(
        &[
            spec("blur", true, &[Some(3.0)]),
            spec("glow", true, &[]),
            spec("vignette", true, &[]),
            spec("colorAdjust", true, &[None, Some(0.0)]),
        ],
        2.0,
        &|_| false,
    );
    let passes: Vec<EffectPass> = admitted(&zoomed)
        .steps()
        .iter()
        .map(|step| step.pass.clone())
        .collect();
    assert_eq!(
        passes,
        [
            EffectPass::Blur { radius_px: 6.0 },
            EffectPass::Glow {
                radius_px: 32.0,
                composite_mode: 2
            },
            EffectPass::Vignette {
                strength: 0.5,
                radius: 0.4
            },
            EffectPass::ColorAdjust { p1: 0.0, p2: 0.0 },
        ]
    );
}

#[test]
fn unrendered_families_are_typed_refusals_naming_the_authored_index() {
    use GpuEffectUnavailable::{CustomEffect, HqBloom, SimpleFx};
    let cases = [
        (
            plan(&[spec("sepia", true, &[])]),
            SimpleFx { effect_index: 0 },
        ),
        (
            plan(&[spec("colorAdjust", true, &[]), spec("halftone", true, &[])]),
            SimpleFx { effect_index: 1 },
        ),
        (
            plan(&[spec("colorAdjust", true, &[]), spec("hqBloom", true, &[])]),
            HqBloom { effect_index: 1 },
        ),
        (
            plan(&[spec("blur", false, &[]), spec("custom:swirl", true, &[])]),
            CustomEffect { effect_index: 1 },
        ),
    ];
    for (stack_plan, family) in cases {
        assert_eq!(refused(&stack_plan), GpuEffectError::Unavailable(family));
    }
    // A registered custom effect is still refused: it has no native owner.
    let registered = plan_effect_stack(&[spec("custom:swirl", true, &[])], 1.0, &|_| true);
    assert_eq!(
        refused(&registered),
        GpuEffectError::Unavailable(CustomEffect { effect_index: 0 })
    );
    // The first refused entry wins; an earlier renderable entry does not hide it.
    let mixed = plan(&[
        spec("vignette", true, &[]),
        spec("invert", true, &[]),
        spec("hqBloom", true, &[]),
    ]);
    assert_eq!(
        refused(&mixed),
        GpuEffectError::Unavailable(SimpleFx { effect_index: 1 })
    );
}

#[test]
fn non_finite_parameters_are_refused_with_the_authored_index() {
    for (kind, values) in [
        ("blur", vec![Some(f32::NAN)]),
        ("glow", vec![Some(f32::INFINITY)]),
        ("colorAdjust", vec![Some(0.0), Some(f32::NEG_INFINITY)]),
        ("vignette", vec![Some(f32::NAN), Some(0.1)]),
    ] {
        let stack_plan = plan(&[spec("blur", false, &[]), spec(kind, true, &values)]);
        assert_eq!(
            refused(&stack_plan),
            GpuEffectError::NonFinite { effect_index: 1 },
            "{kind}"
        );
    }
    // An explicit finite zero is a value, not an absence and not a refusal.
    assert!(GpuEffectStack::from_plan(&plan(&[spec("blur", true, &[Some(0.0)])])).is_ok());
}

#[test]
fn forged_plans_are_refused_with_the_offending_step_and_reason() {
    let good = plan(&[
        spec("colorAdjust", true, &[]),
        spec("vignette", true, &[]),
        spec("blur", true, &[Some(2.0)]),
    ]);
    assert_eq!(good.landing, Landing::StackA);
    let bad = |edit: &dyn Fn(&mut EffectStackPlan)| {
        let mut forged = good.clone();
        edit(&mut forged);
        refused(&forged)
    };
    let inconsistent = |step, reason| GpuEffectError::Inconsistent { step, reason };
    assert_eq!(
        bad(&|p| p.landing = Landing::StackB),
        inconsistent(2, "the landing is the target of the last step")
    );
    assert_eq!(
        bad(&|p| p.landing = Landing::Final),
        inconsistent(2, "the landing is the target of the last step")
    );
    assert_eq!(
        bad(&|p| p.landing = Landing::Empty),
        inconsistent(0, "a plan with steps cannot land empty")
    );
    assert_eq!(
        bad(&|p| p.steps[0].source = PassSource::StackA),
        inconsistent(0, "the first step reads the initial source")
    );
    assert_eq!(
        bad(&|p| p.steps[1].source = PassSource::StackB),
        inconsistent(1, "a step reads the previous step's target")
    );
    assert_eq!(
        bad(&|p| p.steps[1].target = PassTarget::StackA),
        inconsistent(
            1,
            "targets alternate A, B, A, ... with the final target on the last step only"
        )
    );
    assert_eq!(
        bad(&|p| p.steps[1].target = PassTarget::Final),
        inconsistent(
            1,
            "targets alternate A, B, A, ... with the final target on the last step only"
        )
    );
    assert_eq!(
        bad(&|p| p.steps[1].effect_index = 0),
        inconsistent(1, "authored indices must strictly increase")
    );
    assert_eq!(
        bad(&|p| p.steps[2].pass = EffectPass::Glow {
            radius_px: 4.0,
            composite_mode: 3
        }),
        inconsistent(2, "glow composites in Screen mode")
    );
    let mut empty = good.clone();
    empty.steps.clear();
    assert_eq!(refused(&empty), GpuEffectError::NothingToRun);
}

#[test]
fn errors_render_messages_naming_the_cause() {
    let text = |error: GpuEffectError| error.to_string();
    assert_eq!(
        text(GpuEffectError::NothingToRun),
        "effect stack has no enabled entry; no pass may run"
    );
    assert!(text(GpuEffectError::Unavailable(
        GpuEffectUnavailable::SimpleFx { effect_index: 4 }
    ))
    .contains("effect 4 is a simple_fx effect"));
    assert!(
        text(GpuEffectError::Unavailable(GpuEffectUnavailable::HqBloom {
            effect_index: 2
        }))
        .contains("effect 2 is hqBloom")
    );
    assert!(text(GpuEffectError::Unavailable(
        GpuEffectUnavailable::CustomEffect { effect_index: 7 }
    ))
    .contains("effect 7 is a custom WGSL effect"));
    assert_eq!(
        text(GpuEffectError::NonFinite { effect_index: 3 }),
        "effect 3 has a non-finite parameter"
    );
    assert_eq!(
        text(GpuEffectError::Inconsistent {
            step: 1,
            reason: "x"
        }),
        "effect plan step 1 is inconsistent: x"
    );
}
