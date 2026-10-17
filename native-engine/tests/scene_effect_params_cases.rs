//! Hand-derived literal cases for the effect-stack pass planner (L08-C).
//!
//! Every expected value below was read off `geometry-wasm/src/engine.rs`
//! (`run_one_effect` :2664, `apply_effect_stack` :2837, `apply_effect_stack_into`
//! :2855) at fbe4b8a3 and typed by hand, independent of the Node generator that
//! produced the oracle. Zoom values are chosen so the f32 products are exact
//! (2.0, 2.5, 4.0).

use super::*;
use std::cell::RefCell;

fn never(_: &str) -> bool {
    false
}

fn ps(values: &[f32]) -> [Option<f32>; 8] {
    let mut out: [Option<f32>; 8] = [None; 8];
    for (slot, v) in out.iter_mut().zip(values) {
        *slot = Some(*v);
    }
    out
}

fn pass_of(effect_type: &str, params: [Option<f32>; 8], zoom: f64) -> EffectPass {
    let effects = [EffectSpec::new(effect_type, true).with_params(params)];
    let plan = plan_effect_stack(&effects, zoom, &never);
    assert_eq!(plan.steps.len(), 1);
    plan.steps[0].pass.clone()
}

fn simple(effect_id: f32, p: [f32; 4]) -> EffectPass {
    EffectPass::SimpleFx(FxParams {
        effect_id,
        params: [p[0], p[1], p[2], p[3], 0.0, 0.0, 0.0, 0.0],
        bounds: FxBounds::Attached,
    })
}

fn stack_of(types: &[(&str, bool)]) -> Vec<EffectSpec> {
    types
        .iter()
        .map(|(t, on)| EffectSpec::new(t, *on))
        .collect()
}

#[test]
fn simple_fx_ids_and_defaults_match_the_baseline_table() {
    let table: [(&str, f32, [f32; 4]); 14] = [
        ("sepia", 0.0, [0.0, 0.0, 0.0, 0.0]),
        ("invert", 1.0, [0.0, 0.0, 0.0, 0.0]),
        ("grayscale", 2.0, [0.0, 0.0, 0.0, 0.0]),
        ("posterize", 3.0, [6.0, 0.0, 0.0, 0.0]),
        ("pixelate", 4.0, [16.0, 0.0, 0.0, 0.0]),
        ("chromaticAberration", 5.0, [4.0, 0.0, 0.0, 0.0]),
        ("scanlines", 6.0, [240.0, 0.5, 0.0, 0.0]),
        ("grain", 7.0, [0.08, 0.0, 0.0, 0.0]),
        ("sharpen", 8.0, [0.5, 0.0, 0.0, 0.0]),
        ("edgeDetect", 9.0, [4.0, 0.0, 0.0, 0.0]),
        ("groundShadow", 10.0, [0.0, 0.62, 0.6, 0.65]),
        ("contourBrut", 11.0, [3.0, 0.4, 0.0, 0.9]),
        ("threshold", 12.0, [0.5, 0.08, 0.0, 0.0]),
        ("halftone", 13.0, [10.0, 0.9, 0.0, 0.0]),
    ];
    for (name, id, defaults) in table {
        assert_eq!(
            pass_of(name, [None; 8], 1.0),
            simple(id, defaults),
            "{name}"
        );
    }
}

#[test]
fn only_pixelate_chromatic_contour_and_halftone_scale_p1_by_zoom() {
    let z = 2.5;
    assert_eq!(
        pass_of("pixelate", [None; 8], z),
        simple(4.0, [40.0, 0.0, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("chromaticAberration", [None; 8], z),
        simple(5.0, [10.0, 0.0, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("contourBrut", [None; 8], z),
        simple(11.0, [7.5, 0.4, 0.0, 0.9])
    );
    assert_eq!(
        pass_of("halftone", [None; 8], z),
        simple(13.0, [25.0, 0.9, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("pixelate", ps(&[8.0]), z),
        simple(4.0, [20.0, 0.0, 0.0, 0.0])
    );
    // p2..p4 of a scaled effect stay unscaled.
    assert_eq!(
        pass_of("halftone", ps(&[10.0, 3.0, 5.0, 7.0]), z),
        simple(13.0, [25.0, 3.0, 5.0, 7.0])
    );
    // Every other simple_fx effect keeps its p1 at the same zoom, default or explicit.
    assert_eq!(
        pass_of("posterize", [None; 8], z),
        simple(3.0, [6.0, 0.0, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("scanlines", [None; 8], z),
        simple(6.0, [240.0, 0.5, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("edgeDetect", [None; 8], z),
        simple(9.0, [4.0, 0.0, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("sepia", ps(&[7.0]), z),
        simple(0.0, [7.0, 0.0, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("threshold", ps(&[0.25, 0.5]), z),
        simple(12.0, [0.25, 0.5, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("groundShadow", ps(&[1.0, 2.0, 3.0, 4.0]), z),
        simple(10.0, [1.0, 2.0, 3.0, 4.0])
    );
}

#[test]
fn explicit_zero_absent_and_the_default_value_are_distinct_inputs() {
    let glow_at = |p1: Option<f32>| {
        let mut p: [Option<f32>; 8] = [None; 8];
        p[0] = p1;
        pass_of("glow", p, 2.0)
    };
    let glow = |radius_px: f32| EffectPass::Glow {
        radius_px,
        composite_mode: 2,
    };
    assert_eq!(glow_at(None), glow(32.0));
    assert_eq!(glow_at(Some(0.0)), glow(0.0));
    assert_eq!(glow_at(Some(16.0)), glow(32.0));
    assert_ne!(glow_at(None), glow_at(Some(0.0)));
    let mut zero_posterize: [Option<f32>; 8] = [None; 8];
    zero_posterize[0] = Some(0.0);
    assert_eq!(
        pass_of("posterize", zero_posterize, 1.0),
        simple(3.0, [0.0, 0.0, 0.0, 0.0])
    );
    assert_eq!(
        pass_of("posterize", [None; 8], 1.0),
        simple(3.0, [6.0, 0.0, 0.0, 0.0])
    );
}

#[test]
fn glow_hqbloom_and_blur_radii_scale_and_hqbloom_extract_is_full_canvas() {
    let z = 2.5;
    assert_eq!(
        pass_of("glow", ps(&[4.0]), z),
        EffectPass::Glow {
            radius_px: 10.0,
            composite_mode: 2
        }
    );
    assert_eq!(
        pass_of("blur", [None; 8], z),
        EffectPass::Blur { radius_px: 0.0 }
    );
    assert_eq!(
        pass_of("blur", ps(&[6.0]), z),
        EffectPass::Blur { radius_px: 15.0 }
    );
    let extract = |p1: f32, p3: f32, p4: f32| FxParams {
        effect_id: 14.0,
        params: [p1, 0.0, p3, p4, 0.0, 0.0, 0.0, 0.0],
        bounds: FxBounds::FullCanvas,
    };
    assert_eq!(
        pass_of("hqBloom", [None; 8], z),
        EffectPass::HqBloom {
            extract: extract(0.55, 1.4, 0.25),
            radius_px: 70.0,
            composite_mode: 2
        }
    );
    // p2 is the blur radius (scaled), never the extract pass's p2, which stays 0.0.
    assert_eq!(
        pass_of("hqBloom", ps(&[0.9, 10.0, 2.0, 0.5]), z),
        EffectPass::HqBloom {
            extract: extract(0.9, 2.0, 0.5),
            radius_px: 25.0,
            composite_mode: 2
        }
    );
}

#[test]
fn colour_adjust_and_vignette_take_their_defaults_and_never_scale() {
    assert_eq!(
        pass_of("colorAdjust", [None; 8], 4.0),
        EffectPass::ColorAdjust { p1: 0.0, p2: 0.0 }
    );
    assert_eq!(
        pass_of("colorAdjust", ps(&[0.25, -0.5]), 4.0),
        EffectPass::ColorAdjust { p1: 0.25, p2: -0.5 }
    );
    assert_eq!(
        pass_of("vignette", [None; 8], 4.0),
        EffectPass::Vignette {
            strength: 0.5,
            radius: 0.4
        }
    );
    assert_eq!(
        pass_of("vignette", ps(&[0.75, 0.125]), 4.0),
        EffectPass::Vignette {
            strength: 0.75,
            radius: 0.125
        }
    );
}

#[test]
fn unrecognised_types_fall_back_to_blur() {
    let blur = EffectPass::Blur { radius_px: 12.0 };
    assert_eq!(pass_of("blur", ps(&[6.0]), 2.0), blur);
    for name in [
        "bogus",
        "",
        "Blur",
        "GLOW",
        "custom",
        "customs:a",
        "Custom:a",
        " glow",
        "glow ",
        "sepia2",
    ] {
        assert_eq!(pass_of(name, ps(&[6.0]), 2.0), blur, "{name:?}");
    }
}

#[test]
fn custom_effects_carry_p1_to_p8_unscaled_and_report_registration() {
    let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let expected = |name: &str, registered: bool, params: [f32; 8]| EffectPass::Custom {
        name: name.to_string(),
        registered,
        params: FxParams {
            effect_id: 0.0,
            params,
            bounds: FxBounds::Attached,
        },
    };
    assert_eq!(
        pass_of("custom:x", ps(&values), 3.0),
        expected("custom:x", false, values)
    );
    assert_eq!(
        pass_of("custom:x", [None; 8], 3.0),
        expected("custom:x", false, [0.0; 8])
    );
    assert_eq!(
        pass_of("custom:", ps(&[9.0]), 3.0),
        expected("custom:", false, [9.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
    );
    let registered = |name: &str| name == "custom:x";
    let plan = plan_effect_stack(
        &stack_of(&[("custom:x", true), ("custom:y", true)]),
        3.0,
        &registered,
    );
    assert_eq!(plan.steps[0].pass, expected("custom:x", true, [0.0; 8]));
    assert_eq!(plan.steps[1].pass, expected("custom:y", false, [0.0; 8]));
}

#[test]
fn only_enabled_custom_entries_query_the_registry_with_the_full_name() {
    let log: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let registry = |name: &str| {
        log.borrow_mut().push(name.to_string());
        true
    };
    let effects = stack_of(&[
        ("sepia", true),
        ("custom:off", false),
        ("custom:on", true),
        ("glow", true),
    ]);
    let _ = plan_effect_stack(&effects, 1.0, &registry);
    assert_eq!(*log.borrow(), vec!["custom:on".to_string()]);
}

#[test]
fn effect_zoom_floors_at_one_ten_thousandth_and_nan_takes_the_floor() {
    let floor = 0.0001f64 as f32;
    assert_eq!(floor.to_bits(), 0x38d1b717);
    for zoom in [0.0, -3.0, 0.00005, 0.0001, f64::NAN, f64::NEG_INFINITY] {
        assert_eq!(effect_zoom_scale(zoom).to_bits(), floor.to_bits(), "{zoom}");
    }
    assert_eq!(effect_zoom_scale(2.0), 2.0);
    assert_eq!(effect_zoom_scale(0.00011), 0.00011f64 as f32);
    assert_eq!(
        pass_of("blur", ps(&[10.0]), 0.0),
        EffectPass::Blur {
            radius_px: 10.0f32 * floor
        }
    );
}

#[test]
fn landing_and_targets_follow_the_ping_pong_parity() {
    let names = ["sepia", "invert", "grayscale", "grain", "sharpen"];
    let landings = [
        Landing::Empty,
        Landing::StackA,
        Landing::StackB,
        Landing::StackA,
        Landing::StackB,
        Landing::StackA,
    ];
    for n in 0..=5usize {
        let effects: Vec<EffectSpec> = names[..n]
            .iter()
            .map(|t| EffectSpec::new(t, true))
            .collect();
        let plan = plan_effect_stack(&effects, 1.0, &never);
        assert_eq!(plan.landing, landings[n], "n={n}");
        assert_eq!(plan.steps.len(), n);
        let targets: Vec<PassTarget> = plan.steps.iter().map(|s| s.target).collect();
        let expected = [
            PassTarget::StackA,
            PassTarget::StackB,
            PassTarget::StackA,
            PassTarget::StackB,
            PassTarget::StackA,
        ];
        assert_eq!(targets, expected[..n].to_vec(), "n={n}");
        let sources: Vec<PassSource> = plan.steps.iter().map(|s| s.source).collect();
        let expected = [
            PassSource::Initial,
            PassSource::StackA,
            PassSource::StackB,
            PassSource::StackA,
            PassSource::StackB,
        ];
        assert_eq!(sources, expected[..n].to_vec(), "n={n}");
    }
}

#[test]
fn the_into_variant_writes_only_the_last_entry_to_the_final_target() {
    let names = ["sepia", "invert", "grayscale", "grain"];
    for n in 1..=4usize {
        let effects: Vec<EffectSpec> = names[..n]
            .iter()
            .map(|t| EffectSpec::new(t, true))
            .collect();
        let plan = plan_effect_stack_into(&effects, 1.0, &never);
        assert_eq!(plan.landing, Landing::Final);
        let mut expected = vec![PassTarget::StackA, PassTarget::StackB, PassTarget::StackA];
        expected.truncate(n - 1);
        expected.push(PassTarget::Final);
        let targets: Vec<PassTarget> = plan.steps.iter().map(|s| s.target).collect();
        assert_eq!(targets, expected, "n={n}");
        assert_eq!(plan.steps[0].source, PassSource::Initial);
    }
    let three = plan_effect_stack_into(
        &stack_of(&[("sepia", true), ("invert", true), ("grain", true)]),
        1.0,
        &never,
    );
    assert_eq!(three.steps[1].source, PassSource::StackA);
    assert_eq!(three.steps[2].source, PassSource::StackB);
}

#[test]
fn disabled_entries_cost_no_pass_keep_their_params_and_leave_index_gaps() {
    let mut effects = stack_of(&[
        ("sepia", false),
        ("pixelate", true),
        ("invert", false),
        ("glow", true),
        ("grain", false),
    ]);
    effects[2] = effects[2].clone().with_params(ps(&[9.0]));
    let plan = plan_effect_stack(&effects, 2.0, &never);
    let indices: Vec<usize> = plan.steps.iter().map(|s| s.effect_index).collect();
    assert_eq!(indices, vec![1, 3]);
    assert_eq!(plan.steps[0].pass, simple(4.0, [32.0, 0.0, 0.0, 0.0]));
    assert_eq!(
        plan.steps[1].pass,
        EffectPass::Glow {
            radius_px: 32.0,
            composite_mode: 2
        }
    );
    assert_eq!(plan.landing, Landing::StackB);
    // Re-enabling the entry that kept its params runs it with exactly those params.
    effects[2].enabled = true;
    let again = plan_effect_stack(&effects, 2.0, &never);
    assert_eq!(again.steps.len(), 3);
    assert_eq!(again.steps[1].effect_index, 2);
    assert_eq!(again.steps[1].pass, simple(1.0, [9.0, 0.0, 0.0, 0.0]));
}

#[test]
fn a_stack_with_nothing_enabled_plans_no_pass_in_both_variants() {
    let mut effects = stack_of(&[("glow", false), ("pixelate", false)]);
    effects[0] = effects[0].clone().with_params(ps(&[9.0]));
    assert!(!any_enabled(&effects));
    for plan in [
        plan_effect_stack(&effects, 2.0, &never),
        plan_effect_stack_into(&effects, 2.0, &never),
    ] {
        assert!(plan.steps.is_empty());
        assert_eq!(plan.landing, Landing::Empty);
    }
    let none: Vec<EffectSpec> = Vec::new();
    assert!(!any_enabled(&none));
    assert_eq!(
        plan_effect_stack(&none, 1.0, &never).landing,
        Landing::Empty
    );
    effects[1].enabled = true;
    assert!(any_enabled(&effects));
}

#[test]
fn order_is_authored_order_and_an_unregistered_custom_keeps_its_slot() {
    let ab = plan_effect_stack(
        &stack_of(&[("pixelate", true), ("blur", true)]),
        1.0,
        &never,
    );
    let ba = plan_effect_stack(
        &stack_of(&[("blur", true), ("pixelate", true)]),
        1.0,
        &never,
    );
    assert_eq!(ab.steps[0].pass, simple(4.0, [16.0, 0.0, 0.0, 0.0]));
    assert_eq!(ba.steps[1].pass, simple(4.0, [16.0, 0.0, 0.0, 0.0]));
    assert_ne!(ab, ba);
    let gap = plan_effect_stack(
        &stack_of(&[("custom:u", true), ("sepia", true)]),
        1.0,
        &never,
    );
    assert!(matches!(
        gap.steps[0].pass,
        EffectPass::Custom {
            registered: false,
            ..
        }
    ));
    assert_eq!(
        (
            gap.steps[0].target,
            gap.steps[1].source,
            gap.steps[1].target
        ),
        (PassTarget::StackA, PassSource::StackA, PassTarget::StackB)
    );
    assert_eq!(gap.landing, Landing::StackB);
}

#[test]
fn constants_match_the_baseline_and_screen_blend_index() {
    assert_eq!(SCREEN_COMPOSITE_MODE, 2);
    assert_eq!(
        SCREEN_COMPOSITE_MODE,
        super::super::scene_effect_order::BlendMode::Screen.index()
    );
    assert_eq!(BLOOM_EXTRACT_EFFECT_ID, 14.0);
    assert_eq!(CUSTOM_EFFECT_PREFIX, "custom:");
    assert_eq!(MIN_EFFECT_ZOOM, 0.0001);
}
