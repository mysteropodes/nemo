//! Guard tests for the effect-stack pass planner (L08-C): a corrupted oracle
//! literal must be detected on every row, and every stack of up to four
//! entries over four effect kinds and both enabled states keeps the
//! ping-pong, landing, final-target and dispatch invariants of
//! `apply_effect_stack` / `apply_effect_stack_into` (engine.rs:2837, :2855).

use super::tests::{describe, registry, rows};
use super::*;

fn honest(r: &super::tests::Row) -> (EffectStackPlan, EffectStackPlan) {
    let reg = registry(&r.registered);
    (
        plan_effect_stack(&r.effects, r.zoom, &reg),
        plan_effect_stack_into(&r.effects, r.zoom, &reg),
    )
}

#[test]
fn a_corrupted_oracle_literal_is_detected_on_every_row() {
    let mut checked = 0;
    for r in rows() {
        let (stack, into) = honest(&r);
        let mut bad_landing = r.stack.clone();
        bad_landing.landing.push('x');
        assert_ne!(describe(&stack), bad_landing, "{}", r.id);
        if let Some(first) = r.into.steps.first() {
            let mut bad = r.into.clone();
            let flipped = if first.ends_with('0') { '1' } else { '0' };
            bad.steps[0] = format!("{}{}", &first[..first.len() - 1], flipped);
            assert_ne!(describe(&into), bad, "{}", r.id);
            checked += 1;
        }
    }
    assert_eq!(checked, 206);
}

#[test]
fn every_small_stack_keeps_the_ping_pong_and_dispatch_invariants() {
    let kinds = ["sepia", "custom:u", "glow", "hqBloom"];
    let never = |_: &str| false;
    let mut checked = 0usize;
    for len in 0..=4usize {
        for code in 0..8usize.pow(len as u32) {
            let mut rest = code;
            let mut effects = Vec::new();
            for _ in 0..len {
                let slot = rest % 8;
                rest /= 8;
                effects.push(EffectSpec::new(kinds[slot % 4], slot / 4 == 1));
            }
            let on: Vec<usize> = (0..len).filter(|i| effects[*i].enabled).collect();
            for into in [false, true] {
                let plan = if into {
                    plan_effect_stack_into(&effects, 1.0, &never)
                } else {
                    plan_effect_stack(&effects, 1.0, &never)
                };
                let indices: Vec<usize> = plan.steps.iter().map(|s| s.effect_index).collect();
                assert_eq!(indices, on);
                let n = on.len();
                let expected_landing = match (n, into) {
                    (0, _) => Landing::Empty,
                    (_, true) => Landing::Final,
                    (_, false) if n % 2 == 1 => Landing::StackA,
                    _ => Landing::StackB,
                };
                assert_eq!(plan.landing, expected_landing);
                for (k, step) in plan.steps.iter().enumerate() {
                    let ping = if k % 2 == 0 {
                        PassTarget::StackA
                    } else {
                        PassTarget::StackB
                    };
                    let target = if into && k + 1 == n {
                        PassTarget::Final
                    } else {
                        ping
                    };
                    assert_eq!(step.target, target);
                    let source = match (k, plan.steps.get(k.wrapping_sub(1)).map(|s| s.target)) {
                        (0, _) => PassSource::Initial,
                        (_, Some(PassTarget::StackA)) => PassSource::StackA,
                        _ => PassSource::StackB,
                    };
                    assert_eq!(step.source, source);
                    let kind_ok = match (&effects[step.effect_index].effect_type[..], &step.pass) {
                        ("sepia", EffectPass::SimpleFx(p)) => p.effect_id == 0.0,
                        ("custom:u", EffectPass::Custom { registered, .. }) => !*registered,
                        ("glow", EffectPass::Glow { .. }) => true,
                        ("hqBloom", EffectPass::HqBloom { .. }) => true,
                        _ => false,
                    };
                    assert!(
                        kind_ok,
                        "dispatch of {}",
                        effects[step.effect_index].effect_type
                    );
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 2 * (1 + 8 + 64 + 512 + 4096));
}
