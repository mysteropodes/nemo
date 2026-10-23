//! L08-G negative and mutation controls, the independent L08-B cross-check
//! and the GPU texture / payload lifetime proofs.
//!
//! A control is only worth having if it can fail: every mutation below is also
//! simulated by the oracle generator, which refuses to write the fixture unless
//! the mutated image moves by more than the largest tolerance (3).
use super::support::{
    check_case, complete, find, headless, oracle, render, stack, textures, worst_mismatch, Case,
    Geometry, SIDE, SIZE,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_blend_ref::blend_pixel;

const MODES: [&str; 15] = [
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "colorDodge",
    "colorBurn",
    "hardLight",
    "softLight",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
];

#[test]
fn a_corrupted_oracle_byte_is_detected_and_the_tolerance_edge_is_exact() {
    let case = find("blend-multiply");
    assert_eq!(case.tol, 1);
    assert!(check_case(&case, &case.expect).is_ok());
    let mut inside = case.expect.clone();
    inside[0] += 1;
    assert!(check_case(&case, &inside).is_ok(), "one step is inside");
    let mut outside = case.expect.clone();
    outside[0] += 2;
    let message = check_case(&case, &outside).unwrap_err();
    assert!(message.contains("blend-multiply") && message.contains("pixel (0, 0) channel 0"));
    assert_eq!(worst_mismatch(&outside, &case.expect, 1), Some((0, 84, 82)));
    // The last byte of the image is checked as well.
    let mut last = case.expect.clone();
    let end = last.len() - 1;
    last[end] ^= 0x40;
    assert!(check_case(&case, &last).is_err());
}

#[test]
fn the_fifteen_blend_mode_images_are_pairwise_far_apart() {
    let images: Vec<Vec<u8>> = MODES
        .iter()
        .map(|mode| find(&format!("blend-{mode}")).expect)
        .collect();
    for (i, first) in images.iter().enumerate() {
        for (j, second) in images.iter().enumerate().skip(i + 1) {
            assert!(
                worst_mismatch(first, second, 3).is_some(),
                "{} and {} would be confusable within the tolerance",
                MODES[i],
                MODES[j]
            );
        }
    }
}

#[test]
fn no_oracle_case_renders_the_bare_base_colour() {
    for case in oracle() {
        let bare: Vec<u8> = case
            .base
            .iter()
            .copied()
            .cycle()
            .take(case.expect.len())
            .collect();
        assert!(
            worst_mismatch(&case.expect, &bare, case.tol).is_some(),
            "{} would pass with no layers at all",
            case.name
        );
    }
}

fn assert_mutation_is_detected(
    gpu: &mut crate::compositor::Compositor,
    name: &str,
    mutate: impl FnOnce(&mut Case),
) {
    let original = find(name);
    let mut mutated = find(name);
    mutate(&mut mutated);
    let actual = render(gpu, &mutated);
    assert!(
        check_case(&original, &actual).is_err(),
        "{name}: the mutation went unnoticed by the oracle comparison"
    );
}

#[test]
fn gpu_mutating_the_scene_is_detected_against_the_unmutated_oracle() {
    let mut gpu = headless();
    assert_mutation_is_detected(&mut gpu, "blend-multiply", |c| {
        c.layers[1].blend = Some("screen".to_owned());
    });
    assert_mutation_is_detected(&mut gpu, "matte-alpha", |c| {
        c.layers[1].matte.as_mut().unwrap().0 = "alphaInverted".to_owned();
    });
    assert_mutation_is_detected(&mut gpu, "mask-add-then-subtract", |c| {
        c.layers[1].masks[1].0 = crate::compositor::scene_effect_gpu_blend::GpuMaskMode::Intersect;
    });
    assert_mutation_is_detected(&mut gpu, "mask-add-union", |c| {
        c.layers[1].masks.pop();
    });
    assert_mutation_is_detected(
        &mut gpu,
        "canvas-background-is-not-the-blend-backdrop",
        |c| c.layers[0].background = false,
    );
    assert_mutation_is_detected(&mut gpu, "matte-two-mattes-narrow-in-order", |c| {
        c.layers[1].more.clear();
    });
    assert_mutation_is_detected(&mut gpu, "base-colour-is-the-final-backdrop", |c| {
        c.base = [255, 255, 255, 255];
    });
}

#[test]
fn gpu_opaque_blend_pixels_equal_the_independent_cpu_reference_within_one_step() {
    // L08-B's f32 reference never quantises between passes; for opaque layers
    // the GPU result is one shader evaluation, so the two must agree to 1/255.
    let backdrops = [[0.8f32, 0.4, 0.2, 1.0], [0.2, 0.6, 0.4, 1.0]];
    let sources = [
        [0.4f32, 0.6, 0.8, 1.0],
        [0.6, 0.2, 0.4, 1.0],
        [1.0, 0.0, 0.6, 1.0],
        [0.2, 0.8, 0.0, 1.0],
    ];
    let mut gpu = headless();
    for (index, mode) in MODES.iter().enumerate() {
        let case = find(&format!("blend-{mode}"));
        let actual = render(&mut gpu, &case);
        for (band, backdrop) in backdrops.iter().enumerate() {
            for (column, source) in sources.iter().enumerate() {
                let want = blend_pixel(index as u32 + 1, *backdrop, *source);
                let at = (band * 4 * SIDE + column * 2) * 4;
                for (channel, value) in want.iter().enumerate() {
                    let expected = (value * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
                    assert!(
                        actual[at + channel].abs_diff(expected) <= 1,
                        "{mode} band {band} column {column} channel {channel}: gpu {} cpu {expected}",
                        actual[at + channel]
                    );
                }
            }
        }
    }
}

#[test]
fn gpu_only_the_result_texture_survives_and_it_retires_with_the_result() {
    let mut gpu = headless();
    let case = find("mask-then-matte");
    let geometry = Geometry::of(&case);
    let layered = stack(&case, &geometry).unwrap();
    // Warm-up fixes Vello's own steady-state textures; they are not ours.
    drop(
        gpu.compose_layered_stack(&layered, SIZE, case.base)
            .unwrap(),
    );
    complete(&gpu);
    let baseline = textures(&gpu);
    let pixels = gpu
        .compose_layered_stack(&layered, SIZE, case.base)
        .unwrap();
    complete(&gpu);
    assert_eq!(
        textures(&gpu),
        baseline + 1,
        "layer, matte, mask and accumulator textures are retired; the result remains"
    );
    drop(pixels);
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
    // A refused size allocates nothing.
    for size in [(0, 8), (8, 0)] {
        assert!(gpu
            .compose_layered_stack(&layered, size, case.base)
            .is_err());
    }
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
mod budget {
    use super::*;
    use crate::render_scene::ReadbackBudget;

    const TEXTURE_BYTES: usize = 8 * 8 * 4;

    #[test]
    fn gpu_charges_each_pooled_texture_and_returns_the_quota_after_completion() {
        let mut gpu = headless();
        let case = find("mask-then-matte");
        let geometry = Geometry::of(&case);
        let layered = stack(&case, &geometry).unwrap();
        drop(
            gpu.compose_layered_stack(&layered, SIZE, case.base)
                .unwrap(),
        );
        complete(&gpu);
        let baseline = textures(&gpu);
        let budget = ReadbackBudget::new(64 * TEXTURE_BYTES);
        let pixels = gpu
            .compose_layered_stack_with(&layered, SIZE, case.base, &|n| budget.reserve(n))
            .unwrap();
        complete(&gpu);
        assert_eq!(
            budget.used(),
            TEXTURE_BYTES,
            "only the result stays charged"
        );
        assert_eq!(textures(&gpu), baseline + 1);
        drop(pixels);
        complete(&gpu);
        assert_eq!(budget.used(), 0);
        assert_eq!(textures(&gpu), baseline);
    }

    #[test]
    fn gpu_a_refusal_in_the_middle_of_a_composition_releases_everything() {
        let mut gpu = headless();
        let case = find("mask-then-matte");
        let geometry = Geometry::of(&case);
        let layered = stack(&case, &geometry).unwrap();
        drop(
            gpu.compose_layered_stack(&layered, SIZE, case.base)
                .unwrap(),
        );
        complete(&gpu);
        let baseline = textures(&gpu);
        // Two accumulators fit; the first layer texture is refused.
        let budget = ReadbackBudget::new(2 * TEXTURE_BYTES);
        let error = gpu
            .compose_layered_stack_with(&layered, SIZE, case.base, &|n| budget.reserve(n))
            .err()
            .expect("the third texture must be refused");
        assert!(error.to_string().contains("payload budget refused"));
        complete(&gpu);
        assert_eq!(budget.used(), 0);
        assert_eq!(textures(&gpu), baseline);
    }
}
