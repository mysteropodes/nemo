//! L08-I CPU cases: the 64-byte `simple_fx.wgsl` uniform, the effect frame,
//! admission of the fx passes of a real L08-C plan (accepted, refused without a
//! frame, forged) and the texture and dispatch facts. No GPU. The bounding
//! boxes are in `scene_effect_gpu_fx_bounds_cases.rs`.
use super::support::{plan_specs, spec};
use crate::compositor::scene_effect_gpu_fx::{fx_payload, FxFrame, FxFrameError, GpuFxNeeds};
use crate::compositor::scene_effect_gpu_pass::{
    GpuEffectError, GpuEffectNeeds, GpuEffectStack, GpuEffectUnavailable,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    plan_effect_stack, EffectPass, EffectSpec, EffectStackPlan, FxBounds, FxParams, Landing,
    PassTarget,
};

const FX_NAMES: [&str; 14] = [
    "sepia",
    "invert",
    "grayscale",
    "posterize",
    "pixelate",
    "chromaticAberration",
    "scanlines",
    "grain",
    "sharpen",
    "edgeDetect",
    "groundShadow",
    "contourBrut",
    "threshold",
    "halftone",
];

fn frame() -> FxFrame {
    FxFrame::new((3.0, 2.0, 10.0, 7.0), 1.25).unwrap()
}

fn plan_of(kind: &str, values: &[f32]) -> EffectStackPlan {
    plan_specs(&[spec(kind, values)], 1.0, false)
}

fn admit_with(plan: &EffectStackPlan) -> Result<GpuEffectStack<'_>, GpuEffectError> {
    GpuEffectStack::from_plan_with_frame(plan, frame())
}

fn words(payload: &[u8; 64]) -> Vec<f32> {
    payload
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn fx_params(plan: &EffectStackPlan) -> FxParams {
    match &plan.steps[0].pass {
        EffectPass::SimpleFx(params) => params.clone(),
        EffectPass::HqBloom { extract, .. } => extract.clone(),
        other => panic!("not an fx pass: {other:?}"),
    }
}

#[test]
fn the_simple_fx_uniform_is_the_baseline_sixteen_float_layout() {
    // pixelate p1 = 4 at zoom 2 -> block 8; baseline simple_fx_pass offsets:
    // 0 effect_id, 4 p1, 8 p2, 12 p3, 16 tex_w, 20 tex_h, 24 time, 28 p4,
    // 32..48 bbox x y w h, 48..64 p5..p8.
    let plan = plan_specs(&[spec("pixelate", &[4.0])], 2.0, false);
    let payload = fx_payload(&fx_params(&plan), (20, 12), &frame());
    #[rustfmt::skip]
    let expected: [u8; 64] = [
        0x00, 0x00, 0x80, 0x40, 0x00, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xa0, 0x41, 0x00, 0x00, 0x40, 0x41, 0x00, 0x00, 0xa0, 0x3f, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x20, 0x41, 0x00, 0x00, 0xe0, 0x40,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(payload, expected);
    assert_eq!(
        words(&payload),
        [4.0, 8.0, 0.0, 0.0, 20.0, 12.0, 1.25, 0.0, 3.0, 2.0, 10.0, 7.0, 0.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn the_bloom_extract_covers_the_full_canvas_whatever_the_frame_says() {
    let plan = plan_of("hqBloom", &[0.3, 2.0, 2.0, 1.0]);
    let params = fx_params(&plan);
    assert_eq!(params.bounds, FxBounds::FullCanvas);
    let payload = fx_payload(&params, (20, 12), &frame());
    #[rustfmt::skip]
    let expected: [u8; 64] = [
        0x00, 0x00, 0x60, 0x41, 0x9a, 0x99, 0x99, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40,
        0x00, 0x00, 0xa0, 0x41, 0x00, 0x00, 0x40, 0x41, 0x00, 0x00, 0xa0, 0x3f, 0x00, 0x00, 0x80, 0x3f,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa0, 0x41, 0x00, 0x00, 0x40, 0x41,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(
        payload, expected,
        "id 14, p2 stays 0, box (0, 0, 20, 12), time kept"
    );
}

#[test]
fn every_payload_slot_carries_its_own_value() {
    let plan = plan_of("groundShadow", &[0.3, 0.62, 1.0, 0.7]);
    let slots = words(&fx_payload(&fx_params(&plan), (640, 360), &frame()));
    assert_eq!(slots[0], 10.0, "groundShadow is effect id 10");
    assert_eq!(slots[1..4], [0.3, 0.62, 1.0]);
    assert_eq!(slots[4..7], [640.0, 360.0, 1.25], "width, height, time");
    assert_eq!(slots[7], 0.7, "p4 sits after the time, not after p3");
    assert_eq!(slots[8..12], [3.0, 2.0, 10.0, 7.0], "attached box");
    assert_eq!(slots[12..], [0.0; 4]);
}

#[test]
fn frames_refuse_non_finite_values_and_the_canvas_box_is_the_target() {
    let nan = f32::NAN;
    for bbox in [
        (nan, 0.0, 1.0, 1.0),
        (0.0, f32::INFINITY, 1.0, 1.0),
        (0.0, 0.0, f32::NEG_INFINITY, 1.0),
        (0.0, 0.0, 1.0, nan),
    ] {
        assert_eq!(FxFrame::new(bbox, 0.0), Err(FxFrameError::NonFinite));
    }
    assert_eq!(
        FxFrame::new((0.0, 0.0, 1.0, 1.0), nan),
        Err(FxFrameError::NonFinite)
    );
    let canvas = FxFrame::canvas((1920, 1080), 2.5).unwrap();
    assert_eq!(canvas.bbox(), (0.0, 0.0, 1920.0, 1080.0));
    assert_eq!(canvas.time(), 2.5);
    assert!(FxFrameError::NonFinite.to_string().contains("non-finite"));
}

#[test]
fn all_fourteen_built_ins_are_admitted_with_a_frame_and_unavailable_without_one() {
    for (position, name) in FX_NAMES.iter().enumerate() {
        // The final-target variant writes the caller's texture: no private texture at all.
        let plan = plan_specs(&[spec(name, &[])], 1.0, true);
        let stack = admit_with(&plan).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(stack.dispatch_count(), 1, "{name} is one pass");
        assert_eq!(stack.frame(), Some(frame()));
        assert_eq!(
            stack.fx_needs(),
            GpuFxNeeds {
                pipeline: true,
                bloom_extract: false
            }
        );
        assert_eq!(
            stack.needs(),
            GpuEffectNeeds::default(),
            "{name} shares nothing"
        );
        assert_eq!(
            GpuEffectStack::from_plan(&plan).unwrap_err(),
            GpuEffectError::Unavailable(GpuEffectUnavailable::SimpleFx { effect_index: 0 }),
            "{name} ({position}) needs a frame"
        );
    }
}

#[test]
fn hqbloom_needs_the_extract_the_blur_pair_and_the_blend_pipeline() {
    let plan = plan_of("hqBloom", &[]);
    let stack = admit_with(&plan).unwrap();
    assert_eq!(
        stack.dispatch_count(),
        4,
        "extract, two blur passes, composite"
    );
    assert_eq!(
        stack.fx_needs(),
        GpuFxNeeds {
            pipeline: true,
            bloom_extract: true
        }
    );
    let needs = stack.needs();
    assert!(needs.blur_scratch && needs.blur_result && needs.glow_blend);
    assert!(
        needs.stack_a && !needs.stack_b,
        "the landing variant lands in A"
    );
    assert_eq!(
        GpuEffectStack::from_plan(&plan).unwrap_err(),
        GpuEffectError::Unavailable(GpuEffectUnavailable::HqBloom { effect_index: 0 })
    );
}

#[test]
fn custom_and_unknown_types_keep_their_disposition_with_a_frame() {
    // A `custom:` entry has no native owner, registered or not.
    for registered in [false, true] {
        let specs = [spec("custom:Twirl", &[1.0])];
        let plan = plan_effect_stack(&specs, 1.0, &|_| registered);
        assert_eq!(
            admit_with(&plan).unwrap_err(),
            GpuEffectError::Unavailable(GpuEffectUnavailable::CustomEffect { effect_index: 0 })
        );
    }
    // Case-sensitive names: anything else is the blur fallback, fx stack or not.
    for kind in ["hqbloom", "Sepia", "frobnicate", ""] {
        let plan = plan_of(kind, &[3.0]);
        assert!(
            matches!(plan.steps[0].pass, EffectPass::Blur { .. }),
            "{kind:?}"
        );
        let stack = admit_with(&plan).unwrap();
        assert_eq!(stack.fx_needs(), GpuFxNeeds::default());
        assert_eq!(stack.dispatch_count(), 2);
    }
}

#[test]
fn a_frame_changes_nothing_for_the_classic_effects() {
    let specs = [
        spec("colorAdjust", &[0.1, 0.2]),
        spec("blur", &[2.0]),
        spec("glow", &[3.0]),
        spec("vignette", &[]),
    ];
    for into in [false, true] {
        let plan = plan_specs(&specs, 1.5, into);
        let plain = GpuEffectStack::from_plan(&plan).unwrap();
        let framed = admit_with(&plan).unwrap();
        assert_eq!(plain.steps(), framed.steps());
        assert_eq!(plain.landing(), framed.landing());
        assert_eq!(plain.needs(), framed.needs());
        assert_eq!(plain.dispatch_count(), 1 + 2 + 3 + 1);
        assert_eq!(plain.frame(), None);
        assert_eq!(framed.fx_needs(), GpuFxNeeds::default());
    }
}

#[test]
fn a_mixed_stack_counts_every_pass_keeps_authored_indices_and_lands_like_the_plan() {
    let specs = [
        spec("sepia", &[]),
        EffectSpec::new("posterize", false),
        spec("hqBloom", &[0.3, 2.0]),
        spec("blur", &[1.0]),
        spec("halftone", &[4.0]),
    ];
    let plan = plan_specs(&specs, 1.0, false);
    let stack = admit_with(&plan).unwrap();
    let indices: Vec<usize> = stack.steps().iter().map(|s| s.effect_index).collect();
    assert_eq!(indices, [0, 2, 3, 4]);
    assert_eq!(stack.dispatch_count(), 1 + 4 + 2 + 1);
    assert_eq!(stack.landing(), Landing::StackB);
    let into_plan = plan_specs(&specs, 1.0, true);
    let into = admit_with(&into_plan).unwrap();
    assert!(into.into_final());
    assert_eq!(into.steps().last().unwrap().target, PassTarget::Final);
}

fn refused(plan: &EffectStackPlan) -> GpuEffectError {
    admit_with(plan).unwrap_err()
}

fn forge(kind: &str, change: impl Fn(&mut EffectPass)) -> EffectStackPlan {
    let mut plan = plan_of(kind, &[]);
    change(&mut plan.steps[0].pass);
    plan
}

fn inconsistent(reason: &'static str) -> GpuEffectError {
    GpuEffectError::Inconsistent { step: 0, reason }
}

fn simple(pass: &mut EffectPass) -> &mut FxParams {
    match pass {
        EffectPass::SimpleFx(params) => params,
        _ => panic!("simple_fx expected"),
    }
}

#[test]
fn forged_simple_fx_passes_are_refused_with_the_step_and_the_reason() {
    let id_reason = "simple_fx effect id is one of the fourteen built-ins";
    for bad_id in [14.0f32, 15.0, -1.0, 2.5, 99.0] {
        let plan = forge("sepia", |pass| simple(pass).effect_id = bad_id);
        assert_eq!(refused(&plan), inconsistent(id_reason), "id {bad_id}");
    }
    let plan = forge("sepia", |pass| simple(pass).bounds = FxBounds::FullCanvas);
    assert_eq!(
        refused(&plan),
        inconsistent("a built-in effect is attached to the stack's box")
    );
    for slot in 4..8 {
        let plan = forge("sepia", |pass| simple(pass).params[slot] = 1.0);
        assert_eq!(
            refused(&plan),
            inconsistent("built-in effects send p5..p8 as zero"),
            "slot {slot}"
        );
    }
    for slot in 0..4 {
        let plan = forge("sepia", |pass| simple(pass).params[slot] = f32::NAN);
        assert_eq!(
            refused(&plan),
            GpuEffectError::NonFinite { effect_index: 0 }
        );
    }
    let plan = forge("sepia", |pass| simple(pass).effect_id = f32::INFINITY);
    assert_eq!(
        refused(&plan),
        GpuEffectError::NonFinite { effect_index: 0 }
    );
}

#[test]
fn forged_bloom_passes_are_refused_with_the_step_and_the_reason() {
    use EffectPass::HqBloom;
    let forge_bloom = |change: &dyn Fn(&mut EffectPass)| forge("hqBloom", |pass| change(pass));
    let plan = forge_bloom(&|pass| {
        if let HqBloom { composite_mode, .. } = pass {
            *composite_mode = 1;
        }
    });
    assert_eq!(
        refused(&plan),
        inconsistent("hqBloom composites in Screen mode")
    );
    let plan = forge_bloom(&|pass| {
        if let HqBloom { extract, .. } = pass {
            extract.effect_id = 13.0;
        }
    });
    assert_eq!(
        refused(&plan),
        inconsistent("hqBloom extracts with simple_fx id 14")
    );
    let plan = forge_bloom(&|pass| {
        if let HqBloom { extract, .. } = pass {
            extract.bounds = FxBounds::Attached;
        }
    });
    assert_eq!(
        refused(&plan),
        inconsistent("the hqBloom extract covers the full canvas")
    );
    let plan = forge_bloom(&|pass| {
        if let HqBloom { extract, .. } = pass {
            extract.params[1] = 0.5;
        }
    });
    assert_eq!(
        refused(&plan),
        inconsistent("hqBloom keeps p2 for the blur radius, the extract ignores it")
    );
    let plan = forge_bloom(&|pass| {
        if let HqBloom { extract, .. } = pass {
            extract.params[6] = 2.0;
        }
    });
    assert_eq!(
        refused(&plan),
        inconsistent("built-in effects send p5..p8 as zero")
    );
    let plan = forge_bloom(&|pass| {
        if let HqBloom { radius_px, .. } = pass {
            *radius_px = f32::NAN;
        }
    });
    assert_eq!(
        refused(&plan),
        GpuEffectError::NonFinite { effect_index: 0 }
    );
    let plan = forge_bloom(&|pass| {
        if let HqBloom { extract, .. } = pass {
            extract.params[2] = f32::INFINITY;
        }
    });
    assert_eq!(
        refused(&plan),
        GpuEffectError::NonFinite { effect_index: 0 }
    );
}

#[test]
fn a_forged_fx_pass_in_a_later_step_names_that_step_and_its_authored_index() {
    let mut plan = plan_specs(
        &[
            spec("sepia", &[]),
            EffectSpec::new("invert", false),
            spec("invert", &[]),
        ],
        1.0,
        false,
    );
    simple(&mut plan.steps[1].pass).effect_id = 20.0;
    assert_eq!(
        refused(&plan),
        GpuEffectError::Inconsistent {
            step: 1,
            reason: "simple_fx effect id is one of the fourteen built-ins"
        }
    );
    simple(&mut plan.steps[1].pass).effect_id = 1.0;
    simple(&mut plan.steps[1].pass).params[0] = f32::NAN;
    assert_eq!(
        refused(&plan),
        GpuEffectError::NonFinite { effect_index: 2 }
    );
}
