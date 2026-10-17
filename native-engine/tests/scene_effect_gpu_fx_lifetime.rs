//! L08-I texture lifetimes, charged allocation and stale/cancel refusal for the
//! fx passes, on the real headless Metal compositor.
//!
//! hqBloom adds one texture to the executor's charged pool (the extract); a
//! `simple_fx` entry adds none. Everything is allocated from the same charged
//! allocator before it exists, observed by a liveness check before any texture
//! and after every entry, and retired behind the queue's completion fence on
//! every exit path.
use super::support::{
    always_live, complete, headless, oracle, plan_specs, read, spec, textures, upload, Image,
};
use crate::compositor::scene_effect_gpu_fx::FxFrame;
use crate::compositor::scene_effect_gpu_pass::GpuEffectStack;
use crate::compositor::CompositorError;
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::EffectStackPlan;
use std::cell::Cell;

fn frame() -> FxFrame {
    FxFrame::new((0.0, 0.0, 20.0, 12.0), 0.5).unwrap()
}

fn bright() -> Image {
    oracle().image("bright20x12").clone()
}

fn bloom(into: bool) -> EffectStackPlan {
    plan_specs(&[spec("hqBloom", &[0.3, 2.0])], 1.0, into)
}

/// sepia, hqBloom, vignette: three entries, so A and B ping-pong, and the bloom
/// brings the scratch, the result and the extract.
fn three(into: bool) -> EffectStackPlan {
    plan_specs(
        &[
            spec("sepia", &[]),
            spec("hqBloom", &[0.3, 2.0]),
            spec("vignette", &[0.6, 0.3]),
        ],
        1.0,
        into,
    )
}

fn fail_at(call: u32, calls: &Cell<u32>) -> Result<(), CompositorError> {
    calls.set(calls.get() + 1);
    if calls.get() == call {
        Err(CompositorError::new("stale frame refused"))
    } else {
        Ok(())
    }
}

#[test]
fn gpu_a_bloom_landing_run_keeps_only_the_landing_texture() {
    let mut gpu = headless();
    let source = upload(&gpu, &bright());
    let plan = bloom(false);
    let stack = GpuEffectStack::from_plan_with_frame(&plan, frame()).unwrap();
    let run = |gpu: &mut crate::compositor::Compositor| {
        gpu.apply_effect_stack(&stack, source.view(), (20, 12), &always_live)
            .unwrap()
    };
    drop(run(&mut gpu)); // warm-up: both pipelines exist from here on
    complete(&gpu);
    let baseline = textures(&gpu);
    let pixels = run(&mut gpu);
    assert_eq!(pixels.passes(), 4);
    complete(&gpu);
    assert_eq!(
        textures(&gpu),
        baseline + 1,
        "A, the scratch, the result and the extract were private; only A, the landing, remains"
    );
    drop(pixels);
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
}

#[test]
fn gpu_a_bloom_final_target_run_allocates_three_textures_and_returns_all() {
    let mut gpu = headless();
    let source = upload(&gpu, &bright());
    let target = upload(&gpu, &Image::solid(20, 12, [9, 9, 9, 9]));
    let plan = bloom(true);
    let stack = GpuEffectStack::from_plan_with_frame(&plan, frame()).unwrap();
    let go = |gpu: &mut crate::compositor::Compositor| {
        gpu.apply_effect_stack_into(&stack, source.view(), target.view(), (20, 12), &always_live)
            .unwrap()
    };
    go(&mut gpu); // warm-up
    complete(&gpu);
    let baseline = textures(&gpu);
    let run = go(&mut gpu);
    assert_eq!(run.passes, 4);
    complete(&gpu);
    assert_eq!(
        textures(&gpu),
        baseline,
        "scratch, result and extract all retired"
    );
    assert_ne!(read(&gpu, target.texture(), 20, 12), vec![9; 20 * 12 * 4]);
}

#[test]
fn gpu_a_single_simple_fx_entry_into_the_target_allocates_no_texture() {
    let mut gpu = headless();
    let source = upload(&gpu, &bright());
    let target = upload(&gpu, &Image::solid(20, 12, [9, 9, 9, 9]));
    let plan = plan_specs(&[spec("sepia", &[])], 1.0, true);
    let stack = GpuEffectStack::from_plan_with_frame(&plan, frame()).unwrap();
    complete(&gpu);
    let baseline = textures(&gpu);
    let run = gpu
        .apply_effect_stack_into(&stack, source.view(), target.view(), (20, 12), &always_live)
        .unwrap();
    assert_eq!(run.passes, 1);
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
}

#[test]
fn gpu_liveness_is_consulted_once_before_allocation_and_after_every_fx_entry() {
    let mut gpu = headless();
    let source = upload(&gpu, &bright());
    let target = upload(&gpu, &Image::solid(20, 12, [9, 9, 9, 9]));
    for into in [false, true] {
        let plan = three(into);
        let stack = GpuEffectStack::from_plan_with_frame(&plan, frame()).unwrap();
        let calls = Cell::new(0);
        let counted = || fail_at(0, &calls);
        if into {
            let run = gpu
                .apply_effect_stack_into(&stack, source.view(), target.view(), (20, 12), &counted)
                .unwrap();
            assert_eq!(run.passes, 1 + 4 + 1);
        } else {
            let pixels = gpu
                .apply_effect_stack(&stack, source.view(), (20, 12), &counted)
                .unwrap();
            assert_eq!(pixels.passes(), 1 + 4 + 1);
        }
        assert_eq!(
            calls.get(),
            1 + 3,
            "into = {into}: once before, once after each entry"
        );
    }
}

#[test]
fn gpu_a_stale_check_after_the_bloom_entry_frees_everything_and_publishes_nothing() {
    let mut gpu = headless();
    let source = upload(&gpu, &bright());
    let plan = three(false);
    let stack = GpuEffectStack::from_plan_with_frame(&plan, frame()).unwrap();
    complete(&gpu);
    let baseline = textures(&gpu);
    // Call 1 precedes the run, 2 follows sepia, 3 follows hqBloom: refuse there.
    let calls = Cell::new(0);
    let stale = || fail_at(3, &calls);
    assert!(gpu
        .apply_effect_stack(&stack, source.view(), (20, 12), &stale)
        .is_err());
    assert_eq!(calls.get(), 3, "the vignette never ran");
    complete(&gpu);
    assert_eq!(
        textures(&gpu),
        baseline,
        "A, B, scratch, result and extract all retired"
    );
}

#[test]
fn gpu_a_stale_check_before_the_final_bloom_leaves_the_final_target_untouched() {
    let mut gpu = headless();
    let source = upload(&gpu, &bright());
    let sentinel = Image::solid(20, 12, [10, 20, 30, 40]);
    let target = upload(&gpu, &sentinel);
    let plan = plan_specs(
        &[spec("sepia", &[]), spec("hqBloom", &[0.3, 2.0])],
        1.0,
        true,
    );
    let stack = GpuEffectStack::from_plan_with_frame(&plan, frame()).unwrap();
    // Call 2 follows the sepia entry; the bloom that writes the target never runs.
    let calls = Cell::new(0);
    let stale = || fail_at(2, &calls);
    assert!(gpu
        .apply_effect_stack_into(&stack, source.view(), target.view(), (20, 12), &stale)
        .is_err());
    complete(&gpu);
    assert_eq!(read(&gpu, target.texture(), 20, 12), sentinel.bytes);
    gpu.apply_effect_stack_into(&stack, source.view(), target.view(), (20, 12), &always_live)
        .unwrap();
    assert_ne!(read(&gpu, target.texture(), 20, 12), sentinel.bytes);
}

#[test]
fn gpu_a_stack_admitted_without_a_frame_is_refused_before_any_texture() {
    let gpu = headless();
    let plan = bloom(false);
    complete(&gpu);
    let baseline = textures(&gpu);
    assert!(GpuEffectStack::from_plan(&plan).is_err());
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
mod budget {
    use super::*;
    use crate::render_scene::ReadbackBudget;

    const TEXTURE_BYTES: usize = 20 * 12 * 4;

    #[test]
    fn gpu_hqbloom_charges_four_textures_in_the_landing_variant_and_keeps_one() {
        let mut gpu = headless();
        let source = upload(&gpu, &bright());
        let plan = bloom(false);
        let stack = GpuEffectStack::from_plan_with_frame(&plan, frame()).unwrap();
        // A, scratch, result and extract: exactly four fit...
        let budget = ReadbackBudget::new(4 * TEXTURE_BYTES);
        let pixels = gpu
            .apply_effect_stack_with(
                &stack,
                source.view(),
                (20, 12),
                &|n| budget.reserve(n),
                &always_live,
            )
            .unwrap();
        complete(&gpu);
        assert_eq!(
            budget.used(),
            TEXTURE_BYTES,
            "only the landing stays charged"
        );
        drop(pixels);
        complete(&gpu);
        assert_eq!(budget.used(), 0);
        // ...and three do not.
        let tight = ReadbackBudget::new(3 * TEXTURE_BYTES);
        let error = gpu
            .apply_effect_stack_with(
                &stack,
                source.view(),
                (20, 12),
                &|n| tight.reserve(n),
                &always_live,
            )
            .err()
            .expect("the fourth texture must be refused");
        assert!(error.to_string().contains("payload budget refused"));
        complete(&gpu);
        assert_eq!(
            tight.used(),
            0,
            "a refusal in the middle of allocation releases everything"
        );
    }

    #[test]
    fn gpu_hqbloom_into_the_target_needs_three_charged_textures_and_returns_them() {
        let mut gpu = headless();
        let source = upload(&gpu, &bright());
        let target = upload(&gpu, &Image::solid(20, 12, [9, 9, 9, 9]));
        let plan = bloom(true);
        let stack = GpuEffectStack::from_plan_with_frame(&plan, frame()).unwrap();
        let tight = ReadbackBudget::new(2 * TEXTURE_BYTES);
        assert!(gpu
            .apply_effect_stack_into_with(
                &stack,
                source.view(),
                target.view(),
                (20, 12),
                &|n| tight.reserve(n),
                &always_live,
            )
            .is_err());
        complete(&gpu);
        assert_eq!(tight.used(), 0);
        let exact = ReadbackBudget::new(3 * TEXTURE_BYTES);
        gpu.apply_effect_stack_into_with(
            &stack,
            source.view(),
            target.view(),
            (20, 12),
            &|n| exact.reserve(n),
            &always_live,
        )
        .unwrap();
        complete(&gpu);
        assert_eq!(
            exact.used(),
            0,
            "the scratch, the result and the extract were private to the run"
        );
    }
}
