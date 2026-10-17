//! L08-H texture lifetimes, charged allocation and stale/cancel refusal on the
//! real headless Metal compositor.
//!
//! The executor allocates its ping-pong and blur textures from the charged
//! target allocator before they exist, checks liveness before any texture is
//! allocated and after every pass, and retires everything behind the queue's
//! completion fence on every exit path. The liveness check is the seam the
//! scheduled entry plugs `check_live` into (L08-K); here it is driven directly
//! so every terminal path is observable.
use super::support::{always_live, complete, headless, oracle, read, textures, upload, Image};
use crate::compositor::scene_effect_gpu_pass::GpuEffectStack;
use crate::compositor::CompositorError;
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    plan_effect_stack, plan_effect_stack_into, EffectSpec, EffectStackPlan,
};
use std::cell::Cell;

fn spec(kind: &str, values: &[f32]) -> EffectSpec {
    let mut params = [None; 8];
    for (slot, value) in params.iter_mut().zip(values) {
        *slot = Some(*value);
    }
    EffectSpec::new(kind, true).with_params(params)
}

/// blur (scratch), vignette, colorAdjust: three entries, so A, B and the scratch
/// are private; the landing entry lands in A, the final-target entry in the target.
fn three(into: bool) -> EffectStackPlan {
    let specs = [
        spec("blur", &[2.0]),
        spec("vignette", &[0.6, 0.3]),
        spec("colorAdjust", &[0.1, 0.0]),
    ];
    if into {
        plan_effect_stack_into(&specs, 1.0, &|_| false)
    } else {
        plan_effect_stack(&specs, 1.0, &|_| false)
    }
}

fn fail_at(call: u32, calls: &Cell<u32>) -> Result<(), CompositorError> {
    calls.set(calls.get() + 1);
    if calls.get() == call {
        Err(CompositorError::new("stale frame refused"))
    } else {
        Ok(())
    }
}

fn grad8() -> Image {
    oracle().image("grad8").clone()
}

#[test]
fn gpu_only_the_landing_texture_survives_and_it_retires_with_the_result() {
    let mut gpu = headless();
    let image = grad8();
    let source = upload(&gpu, &image);
    let plan = three(false);
    let stack = GpuEffectStack::from_plan(&plan).unwrap();
    let run = |gpu: &mut crate::compositor::Compositor| {
        gpu.apply_effect_stack(&stack, source.view(), (8, 8), &always_live)
            .unwrap()
    };
    drop(run(&mut gpu)); // warm-up: pipelines exist from here on
    complete(&gpu);
    let baseline = textures(&gpu);
    let pixels = run(&mut gpu);
    complete(&gpu);
    assert_eq!(
        textures(&gpu),
        baseline + 1,
        "A, B and the scratch retired; the landing remains"
    );
    drop(pixels);
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
}

#[test]
fn gpu_a_refused_size_allocates_nothing() {
    let mut gpu = headless();
    let source = upload(&gpu, &grad8());
    let plan = three(false);
    let stack = GpuEffectStack::from_plan(&plan).unwrap();
    complete(&gpu);
    let baseline = textures(&gpu);
    for size in [(0, 8), (8, 0)] {
        assert!(gpu
            .apply_effect_stack(&stack, source.view(), size, &always_live)
            .is_err());
    }
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
}

#[test]
fn gpu_the_final_target_variant_allocates_only_what_the_chain_needs() {
    let mut gpu = headless();
    let image = grad8();
    let source = upload(&gpu, &image);
    let target = upload(&gpu, &Image::solid(8, 8, [9, 9, 9, 9]));
    // One entry writes the caller's target directly: no private texture at all.
    let single = plan_effect_stack_into(&[spec("colorAdjust", &[0.2, 0.0])], 1.0, &|_| false);
    let stack = GpuEffectStack::from_plan(&single).unwrap();
    complete(&gpu);
    let baseline = textures(&gpu);
    let run = gpu
        .apply_effect_stack_into(&stack, source.view(), target.view(), (8, 8), &always_live)
        .unwrap();
    assert_eq!(run.passes, 1);
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
    // Three entries need A, B and the blur scratch, all gone after completion.
    let plan = three(true);
    let stack = GpuEffectStack::from_plan(&plan).unwrap();
    let run = gpu
        .apply_effect_stack_into(&stack, source.view(), target.view(), (8, 8), &always_live)
        .unwrap();
    assert_eq!(run.passes, 2 + 1 + 1);
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
}

#[test]
fn gpu_the_wrong_entry_variant_is_refused_before_any_texture_or_liveness_check() {
    let mut gpu = headless();
    let source = upload(&gpu, &grad8());
    let target = upload(&gpu, &Image::solid(8, 8, [9, 9, 9, 9]));
    let landing_plan = three(false);
    let into_plan = three(true);
    let landing = GpuEffectStack::from_plan(&landing_plan).unwrap();
    let into = GpuEffectStack::from_plan(&into_plan).unwrap();
    complete(&gpu);
    let baseline = textures(&gpu);
    let calls = Cell::new(0);
    let counted = || fail_at(0, &calls);
    let error = gpu
        .apply_effect_stack(&into, source.view(), (8, 8), &counted)
        .err()
        .expect("a final-target plan cannot land privately");
    assert!(error.to_string().contains("use the final-target entry"));
    let error = gpu
        .apply_effect_stack_into(&landing, source.view(), target.view(), (8, 8), &counted)
        .err()
        .expect("a landing plan has no caller target");
    assert!(error.to_string().contains("use the landing entry"));
    assert_eq!(calls.get(), 0, "no liveness check, no pass");
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
    assert_eq!(
        read(&gpu, target.texture(), 8, 8),
        vec![9; 8 * 8 * 4],
        "the target was not written"
    );
}

#[test]
fn gpu_liveness_is_consulted_once_before_allocation_and_after_every_pass() {
    let mut gpu = headless();
    let source = upload(&gpu, &grad8());
    let target = upload(&gpu, &Image::solid(8, 8, [9, 9, 9, 9]));
    for into in [false, true] {
        let plan = three(into);
        let stack = GpuEffectStack::from_plan(&plan).unwrap();
        let calls = Cell::new(0);
        let counted = || fail_at(0, &calls);
        if into {
            gpu.apply_effect_stack_into(&stack, source.view(), target.view(), (8, 8), &counted)
                .unwrap();
        } else {
            gpu.apply_effect_stack(&stack, source.view(), (8, 8), &counted)
                .unwrap();
        }
        assert_eq!(
            calls.get(),
            1 + 3,
            "into = {into}: once before, once after each of 3 entries"
        );
    }
}

#[test]
fn gpu_a_stale_check_before_the_first_pass_allocates_nothing() {
    let mut gpu = headless();
    let source = upload(&gpu, &grad8());
    let plan = three(false);
    let stack = GpuEffectStack::from_plan(&plan).unwrap();
    complete(&gpu);
    let baseline = textures(&gpu);
    let calls = Cell::new(0);
    let stale = || fail_at(1, &calls);
    let error = gpu
        .apply_effect_stack(&stack, source.view(), (8, 8), &stale)
        .err()
        .expect("stale before the first pass");
    assert_eq!(error.to_string(), "stale frame refused");
    assert_eq!(calls.get(), 1);
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline, "no texture was ever created");
}

#[test]
fn gpu_a_stale_check_after_a_pass_frees_everything_and_publishes_nothing() {
    let mut gpu = headless();
    let source = upload(&gpu, &grad8());
    let plan = three(false);
    let stack = GpuEffectStack::from_plan(&plan).unwrap();
    complete(&gpu);
    let baseline = textures(&gpu);
    // Calls: 1 before, 2 after entry 1, 3 after entry 2 -> refuse there.
    let calls = Cell::new(0);
    let stale = || fail_at(3, &calls);
    let outcome = gpu.apply_effect_stack(&stack, source.view(), (8, 8), &stale);
    assert!(
        outcome.is_err(),
        "no pixels are published after a stale check"
    );
    assert_eq!(calls.get(), 3, "the third entry never ran");
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline, "A, B and the scratch all retired");
}

#[test]
fn gpu_a_late_stale_check_after_the_last_pass_still_refuses_the_result() {
    let mut gpu = headless();
    let source = upload(&gpu, &grad8());
    let plan = three(false);
    let stack = GpuEffectStack::from_plan(&plan).unwrap();
    complete(&gpu);
    let baseline = textures(&gpu);
    let calls = Cell::new(0);
    let stale = || fail_at(4, &calls);
    assert!(gpu
        .apply_effect_stack(&stack, source.view(), (8, 8), &stale)
        .is_err());
    assert_eq!(calls.get(), 4);
    complete(&gpu);
    assert_eq!(
        textures(&gpu),
        baseline,
        "the landing texture retired with the refusal"
    );
}

#[test]
fn gpu_a_stale_check_before_the_final_pass_leaves_the_final_target_untouched() {
    let mut gpu = headless();
    let image = grad8();
    let source = upload(&gpu, &image);
    let sentinel = Image::solid(8, 8, [10, 20, 30, 40]);
    let target = upload(&gpu, &sentinel);
    let plan = plan_effect_stack_into(
        &[
            spec("colorAdjust", &[0.3, 0.2]),
            spec("vignette", &[0.8, 0.3]),
        ],
        1.0,
        &|_| false,
    );
    let stack = GpuEffectStack::from_plan(&plan).unwrap();
    // Call 2 follows the first entry (into A); the vignette that writes the target never runs.
    let calls = Cell::new(0);
    let stale = || fail_at(2, &calls);
    assert!(gpu
        .apply_effect_stack_into(&stack, source.view(), target.view(), (8, 8), &stale)
        .is_err());
    complete(&gpu);
    assert_eq!(
        read(&gpu, target.texture(), 8, 8),
        sentinel.bytes,
        "nothing was published"
    );
    // The same stack with a live frame does publish.
    gpu.apply_effect_stack_into(&stack, source.view(), target.view(), (8, 8), &always_live)
        .unwrap();
    assert_ne!(read(&gpu, target.texture(), 8, 8), sentinel.bytes);
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
mod budget {
    use super::*;
    use crate::render_scene::ReadbackBudget;

    const TEXTURE_BYTES: usize = 8 * 8 * 4;

    fn glow_then_vignette() -> EffectStackPlan {
        plan_effect_stack(
            &[spec("glow", &[2.0]), spec("vignette", &[0.6, 0.3])],
            1.0,
            &|_| false,
        )
    }

    #[test]
    fn gpu_charges_each_private_texture_and_returns_the_quota_after_completion() {
        let mut gpu = headless();
        let source = upload(&gpu, &grad8());
        let plan = plan_effect_stack(
            &[spec("blur", &[2.0]), spec("vignette", &[0.6, 0.3])],
            1.0,
            &|_| false,
        );
        let stack = GpuEffectStack::from_plan(&plan).unwrap();
        complete(&gpu);
        let baseline = textures(&gpu);
        let budget = ReadbackBudget::new(64 * TEXTURE_BYTES);
        let pixels = gpu
            .apply_effect_stack_with(
                &stack,
                source.view(),
                (8, 8),
                &|n| budget.reserve(n),
                &always_live,
            )
            .unwrap();
        complete(&gpu);
        assert_eq!(
            budget.used(),
            TEXTURE_BYTES,
            "A, B and the scratch were charged; only the landing stays"
        );
        assert_eq!(textures(&gpu), baseline + 1);
        drop(pixels);
        complete(&gpu);
        assert_eq!(budget.used(), 0);
        assert_eq!(textures(&gpu), baseline);
    }

    #[test]
    fn gpu_a_refusal_in_the_middle_of_allocation_releases_everything() {
        let mut gpu = headless();
        let source = upload(&gpu, &grad8());
        let plan = three(false);
        let stack = GpuEffectStack::from_plan(&plan).unwrap();
        complete(&gpu);
        let baseline = textures(&gpu);
        // A and B fit; the blur scratch is refused.
        let budget = ReadbackBudget::new(2 * TEXTURE_BYTES);
        let error = gpu
            .apply_effect_stack_with(
                &stack,
                source.view(),
                (8, 8),
                &|n| budget.reserve(n),
                &always_live,
            )
            .err()
            .expect("the third texture must be refused");
        assert!(error.to_string().contains("payload budget refused"));
        complete(&gpu);
        assert_eq!(budget.used(), 0);
        assert_eq!(textures(&gpu), baseline);
    }

    #[test]
    fn gpu_glow_charges_the_scratch_and_the_result_besides_the_ping_pong_pair() {
        let mut gpu = headless();
        let source = upload(&gpu, &grad8());
        let plan = glow_then_vignette();
        let stack = GpuEffectStack::from_plan(&plan).unwrap();
        // A, B, scratch and result: exactly four textures fit...
        let budget = ReadbackBudget::new(4 * TEXTURE_BYTES);
        let pixels = gpu
            .apply_effect_stack_with(
                &stack,
                source.view(),
                (8, 8),
                &|n| budget.reserve(n),
                &always_live,
            )
            .unwrap();
        assert_eq!(pixels.passes(), 3 + 1);
        drop(pixels);
        complete(&gpu);
        assert_eq!(budget.used(), 0);
        // ...and three do not.
        let tight = ReadbackBudget::new(3 * TEXTURE_BYTES);
        let error = gpu
            .apply_effect_stack_with(
                &stack,
                source.view(),
                (8, 8),
                &|n| tight.reserve(n),
                &always_live,
            )
            .err()
            .expect("the fourth texture must be refused");
        assert!(error.to_string().contains("payload budget refused"));
        complete(&gpu);
        assert_eq!(tight.used(), 0);
    }

    #[test]
    fn gpu_the_final_target_variant_and_a_stale_check_return_the_whole_quota() {
        let mut gpu = headless();
        let source = upload(&gpu, &grad8());
        let target = upload(&gpu, &Image::solid(8, 8, [9, 9, 9, 9]));
        let plan = three(true);
        let stack = GpuEffectStack::from_plan(&plan).unwrap();
        let budget = ReadbackBudget::new(64 * TEXTURE_BYTES);
        gpu.apply_effect_stack_into_with(
            &stack,
            source.view(),
            target.view(),
            (8, 8),
            &|n| budget.reserve(n),
            &always_live,
        )
        .unwrap();
        complete(&gpu);
        assert_eq!(
            budget.used(),
            0,
            "A, B and the scratch were private to the run"
        );
        let calls = Cell::new(0);
        let stale = || fail_at(2, &calls);
        assert!(gpu
            .apply_effect_stack_into_with(
                &stack,
                source.view(),
                target.view(),
                (8, 8),
                &|n| budget.reserve(n),
                &stale
            )
            .is_err());
        complete(&gpu);
        assert_eq!(budget.used(), 0, "a stale refusal returns the quota too");
    }
}
