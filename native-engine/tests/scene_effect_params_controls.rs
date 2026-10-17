//! Mutation, negative and guard controls for the effect-stack pass planner
//! (L08-C).
//!
//! Each mutant is a deliberately wrong variant of one rule (a non-listed
//! effect scaled by the zoom, the zoom ignored, the enabled filter dropped,
//! the authored order reversed, the landing buffer flipped, absent read as
//! zero, explicit zero read as absent, unknown types dropped instead of
//! falling back to blur, unregistered custom effects dropped instead of
//! keeping their slot, custom parameters scaled, the final-target rule
//! replaced by the stack rule). It is replayed over the SAME oracle rows as the
//! real planner and must fail a named, literal subset of them; the unmutated
//! planner must fail none, so a failure can only come from the mutation. The
//! counts and names were evaluated by `tools/verify_controls_claims.js`
//! (the generator's planner with the same mutation) before being written here.

use super::tests::{describe, registry, rows, Row};
use super::*;

type Plans = (EffectStackPlan, EffectStackPlan);

fn plans(r: &Row, effects: &[EffectSpec], zoom: f64) -> Plans {
    let reg = registry(&r.registered);
    (
        plan_effect_stack(effects, zoom, &reg),
        plan_effect_stack_into(effects, zoom, &reg),
    )
}

fn honest(r: &Row) -> Plans {
    plans(r, &r.effects, r.zoom)
}

fn failing(mutant: impl Fn(&Row) -> Plans) -> Vec<String> {
    rows()
        .into_iter()
        .filter(|r| {
            let (stack, into) = mutant(r);
            describe(&stack) != r.stack || describe(&into) != r.into
        })
        .map(|r| r.id)
        .collect()
}

fn claim(name: &str, got: &[String], count: usize, included: &[&str], excluded: &[&str]) {
    assert_eq!(got.len(), count, "{name}: failing row count");
    for id in included {
        assert!(got.iter().any(|g| g == id), "{name} must fail {id}");
    }
    for id in excluded {
        assert!(!got.iter().any(|g| g == id), "{name} must not fail {id}");
    }
}

const EMPTY_ROWS: [&str; 3] = [
    "stack-empty",
    "stack-all-disabled-keep-params",
    "bridge:layer-without-effects-is-empty",
];

fn map_simple(mut plan: EffectStackPlan, f: impl Fn(&mut FxParams)) -> EffectStackPlan {
    for step in plan.steps.iter_mut() {
        if let EffectPass::SimpleFx(p) = &mut step.pass {
            f(p);
        }
    }
    plan
}

fn map_custom(mut plan: EffectStackPlan, f: impl Fn(&mut FxParams)) -> EffectStackPlan {
    for step in plan.steps.iter_mut() {
        if let EffectPass::Custom { params, .. } = &mut step.pass {
            f(params);
        }
    }
    plan
}

fn flip_buffers(mut plan: EffectStackPlan) -> EffectStackPlan {
    plan.landing = match plan.landing {
        Landing::StackA => Landing::StackB,
        Landing::StackB => Landing::StackA,
        other => other,
    };
    for step in plan.steps.iter_mut() {
        step.source = match step.source {
            PassSource::StackA => PassSource::StackB,
            PassSource::StackB => PassSource::StackA,
            other => other,
        };
        step.target = match step.target {
            PassTarget::StackA => PassTarget::StackB,
            PassTarget::StackB => PassTarget::StackA,
            other => other,
        };
    }
    plan
}

#[test]
fn the_unmutated_planner_fails_no_oracle_row() {
    assert!(failing(honest).is_empty());
}

#[test]
fn scaling_a_non_listed_effect_by_the_zoom_changes_literals() {
    let got = failing(|r| {
        let z = effect_zoom_scale(r.zoom);
        let scale = move |p: &mut FxParams| {
            if ![4.0f32, 5.0, 11.0, 13.0].contains(&p.effect_id) {
                p.params[0] *= z;
            }
        };
        let (stack, into) = honest(r);
        (map_simple(stack, scale), map_simple(into, scale))
    });
    claim(
        "scale-unlisted",
        &got,
        39,
        &[
            "default-z2.5:posterize",
            "explicit-z2.5:sepia",
            "p1-only-z2:groundShadow",
            "stack-three",
        ],
        &[
            "default-z1:posterize",
            "explicit-z2.5:pixelate",
            "explicit-z2.5:halftone",
            "explicit-z2.5:glow",
            "default-z2.5:sepia",
        ],
    );
}

#[test]
fn ignoring_the_zoom_changes_every_scaled_radius_and_size() {
    let got = failing(|r| plans(r, &r.effects, 1.0));
    claim(
        "zoom-ignored",
        &got,
        80,
        &[
            "default-z2.5:pixelate",
            "explicit-z2.5:glow",
            "default-z2.5:hqBloom",
            "zoom-7:blur",
            "zoom-0:glow",
        ],
        &[
            "default-z1:pixelate",
            "explicit-z2.5:posterize",
            "explicit-z2.5:custom:a",
            "default-z2.5:blur",
            "default-z2.5:vignette",
        ],
    );
}

#[test]
fn dropping_the_enabled_filter_changes_stacks_with_disabled_entries() {
    let got = failing(|r| {
        let all: Vec<EffectSpec> = r
            .effects
            .iter()
            .map(|e| EffectSpec {
                enabled: true,
                ..e.clone()
            })
            .collect();
        plans(r, &all, r.zoom)
    });
    claim(
        "enabled-filter-dropped",
        &got,
        7,
        &[
            "stack-disabled-gaps",
            "stack-all-disabled-keep-params",
            "stack-absent-enabled-defaults-to-false",
            "bridge:disabled-entry-keeps-params-and-leaves-a-gap",
        ],
        &["stack-three", "default-z1:glow"],
    );
}

#[test]
fn reversing_the_authored_order_changes_multi_entry_stacks_only() {
    let got = failing(|r| {
        let mut reversed = r.effects.clone();
        reversed.reverse();
        plans(r, &reversed, r.zoom)
    });
    claim(
        "reversed-order",
        &got,
        15,
        &[
            "stack-order-matters-ab",
            "stack-order-matters-ba",
            "stack-three",
            "bridge:instance-effect-borrowed-stack-comes-first",
        ],
        &[
            "stack-one",
            "default-z1:glow",
            "stack-duplicate-types",
            "stack-empty",
        ],
    );
}

#[test]
fn flipping_the_landing_buffer_changes_every_non_empty_plan() {
    let got = failing(|r| {
        let (stack, into) = honest(r);
        (flip_buffers(stack), flip_buffers(into))
    });
    claim(
        "landing-flipped",
        &got,
        206,
        &["default-z1:sepia", "stack-one", "stack-five-mixed"],
        &EMPTY_ROWS,
    );
}

#[test]
fn replacing_the_final_target_rule_with_the_stack_rule_changes_every_non_empty_plan() {
    let got = failing(|r| {
        let (stack, _) = honest(r);
        (stack.clone(), stack)
    });
    claim(
        "into-is-stack",
        &got,
        206,
        &["default-z1:sepia", "stack-two"],
        &EMPTY_ROWS,
    );
}

#[test]
fn reading_absent_as_zero_changes_every_effect_with_a_non_zero_default() {
    let got = failing(|r| {
        let zeroed: Vec<EffectSpec> = r
            .effects
            .iter()
            .map(|e| {
                let mut e = e.clone();
                for slot in e.params.iter_mut() {
                    *slot = Some(slot.unwrap_or(0.0));
                }
                e
            })
            .collect();
        plans(r, &zeroed, r.zoom)
    });
    claim(
        "absent-as-zero",
        &got,
        52,
        &[
            "default-z1:vignette",
            "default-z2.5:glow",
            "default-z1:scanlines",
            "p1-only-z2:hqBloom",
        ],
        &[
            "default-z1:sepia",
            "default-z1:blur",
            "default-z1:colorAdjust",
            "explicit-z2.5:vignette",
            "default-z1:custom:a",
        ],
    );
}

#[test]
fn reading_explicit_zero_as_absent_changes_the_zero_rows_with_non_zero_defaults() {
    let got = failing(|r| {
        let absent: Vec<EffectSpec> = r
            .effects
            .iter()
            .map(|e| {
                let mut e = e.clone();
                for slot in e.params.iter_mut() {
                    if *slot == Some(0.0) {
                        *slot = None;
                    }
                }
                e
            })
            .collect();
        plans(r, &absent, r.zoom)
    });
    claim(
        "zero-as-absent",
        &got,
        14,
        &[
            "zero-z3:vignette",
            "zero-z3:glow",
            "zero-z3:hqBloom",
            "zero-z3:halftone",
        ],
        &[
            "zero-z3:sepia",
            "zero-z3:blur",
            "zero-z3:colorAdjust",
            "zero-z3:custom:a",
        ],
    );
}

const KNOWN: [&str; 19] = [
    "colorAdjust",
    "vignette",
    "glow",
    "hqBloom",
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
    "blur",
];

#[test]
fn dropping_unknown_types_instead_of_falling_back_to_blur_changes_literals() {
    let got = failing(|r| {
        let kept: Vec<EffectSpec> = r
            .effects
            .iter()
            .filter(|e| {
                KNOWN.contains(&e.effect_type.as_str()) || e.effect_type.starts_with("custom:")
            })
            .cloned()
            .collect();
        plans(r, &kept, r.zoom)
    });
    claim(
        "unknown-dropped",
        &got,
        20,
        &[
            "default-z1:bogus",
            "explicit-z2.5:empty-type",
            "p1-only-z2:Blur",
            "stack-five-mixed",
            "bridge:typeless-entry-falls-back-to-blur",
        ],
        &["default-z1:blur", "default-z1:custom:b", "stack-three"],
    );
}

#[test]
fn dropping_unregistered_custom_effects_instead_of_keeping_their_slot_changes_literals() {
    let got = failing(|r| {
        let kept: Vec<EffectSpec> = r
            .effects
            .iter()
            .filter(|e| {
                !e.effect_type.starts_with("custom:") || r.registered.contains(&e.effect_type)
            })
            .cloned()
            .collect();
        plans(r, &kept, r.zoom)
    });
    claim(
        "unregistered-custom-dropped",
        &got,
        13,
        &[
            "default-z1:custom:b",
            "p1-only-z2:custom:",
            "stack-custom-unregistered-keeps-slot",
        ],
        &[
            "default-z1:custom:a",
            "stack-custom-registered",
            "default-z1:bogus",
        ],
    );
}

#[test]
fn scaling_custom_parameters_by_the_zoom_changes_literals() {
    let got = failing(|r| {
        let z = effect_zoom_scale(r.zoom);
        let scale = move |p: &mut FxParams| {
            for v in p.params.iter_mut() {
                *v *= z;
            }
        };
        let (stack, into) = honest(r);
        (map_custom(stack, scale), map_custom(into, scale))
    });
    claim(
        "custom-scaled",
        &got,
        13,
        &[
            "explicit-z2.5:custom:a",
            "p1-only-z2:custom:b",
            "stack-custom-registered",
            "bridge:user-custom-effect-is-not-scaled-by-js",
        ],
        &[
            "default-z2.5:custom:a",
            "zero-z3:custom:a",
            "explicit-z2.5:glow",
            "default-z1:custom:a",
        ],
    );
}
