//! L08-I negative and mutation controls.
//!
//! CPU: a corrupted oracle byte is detected and the tolerance edge is exact; the
//! generator-proved mutation deltas exceed their tolerances. GPU (shared
//! appointment): every mutation of the scene is rendered and must FAIL the
//! unmutated oracle; grain and contourBrut, whose per-pixel noise a CPU oracle
//! cannot pin, are held to analytic invariants; a built-in effect ignores the
//! box; the caller's source is never written.
use super::support::{
    always_live, complete, headless, oracle, plan_specs, read, run_plan, spec, upload,
    worst_mismatch, Case, Image,
};
use crate::compositor::scene_effect_gpu_fx::FxFrame;
use crate::compositor::scene_effect_gpu_pass::GpuEffectStack;
use crate::compositor::Compositor;
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::EffectSpec;

const MUTATIONS: [&str; 8] = [
    "swap-order",
    "enable-the-disabled-entry",
    "ignore-the-zoom",
    "bloom-as-glow",
    "explicit-zero-read-as-default",
    "sepia-as-grayscale",
    "bloom-warmth-ignored",
    "zero-intensity-read-as-default",
];

/// The Rust edit of each mutation; mirrors the generator's `MUTATIONS` table.
fn mutate(name: &str, base: &Case) -> Case {
    let mut case = base.clone();
    match name {
        "swap-order" => case.effects.reverse(),
        "enable-the-disabled-entry" => case.effects.iter_mut().for_each(|e| e.enabled = true),
        "ignore-the-zoom" => case.zoom = 1.0,
        "bloom-as-glow" => {
            let radius = case.effects[0].params[1];
            let mut params = [None; 8];
            params[0] = radius;
            case.effects = vec![EffectSpec::new("glow", true).with_params(params)];
        }
        "explicit-zero-read-as-default" => case.effects.iter_mut().for_each(|e| e.params[0] = None),
        "sepia-as-grayscale" => case
            .effects
            .iter_mut()
            .for_each(|e| e.effect_type = "grayscale".to_owned()),
        "bloom-warmth-ignored" => case
            .effects
            .iter_mut()
            .for_each(|e| e.params[3] = Some(-1.0)),
        "zero-intensity-read-as-default" => {
            case.effects.iter_mut().for_each(|e| e.params[2] = None)
        }
        other => panic!("unknown mutation {other}"),
    }
    case
}

#[test]
fn a_corrupted_oracle_byte_is_detected_and_the_tolerance_edge_is_exact() {
    let oracle = oracle();
    let case = oracle.case("fx-sepia-ramp");
    assert_eq!(case.tol, 1);
    assert!(worst_mismatch(&case.expect, &case.expect, case.tol).is_none());
    // First and last byte of an opaque colour channel: +1 passes, +2 fails.
    for index in [0usize, case.expect.len() - 2] {
        let mut actual = case.expect.clone();
        assert!(actual[index] < 250, "room to move the byte up");
        actual[index] += 1;
        assert!(worst_mismatch(&actual, &case.expect, case.tol).is_none());
        actual[index] += 1;
        let (_, channel, delta) = worst_mismatch(&actual, &case.expect, case.tol).expect("+2");
        assert_eq!(channel, index % 4);
        assert_eq!(delta, 2.0);
    }
    // Alpha: -1 passes, -2 fails (tolerance 1).
    let mut actual = case.expect.clone();
    actual[3] -= 1;
    assert!(worst_mismatch(&actual, &case.expect, case.tol).is_none());
    actual[3] -= 1;
    assert_eq!(
        worst_mismatch(&actual, &case.expect, case.tol),
        Some((0, 3, 2.0))
    );
}

#[test]
fn straight_colour_under_zero_alpha_is_ignored_but_alpha_error_is_not() {
    let expected = [10u8, 20, 30, 0];
    assert!(worst_mismatch(&[200, 1, 99, 0], &expected, 2).is_none());
    assert!(worst_mismatch(&[10, 20, 30, 2], &expected, 2).is_none());
    assert_eq!(
        worst_mismatch(&[10, 20, 30, 3], &expected, 2),
        Some((0, 3, 3.0))
    );
}

#[test]
fn every_mutation_the_generator_proved_moves_the_image_beyond_its_tolerance() {
    let oracle = oracle();
    let names: Vec<&str> = oracle.mutations.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, MUTATIONS);
    for mutation in &oracle.mutations {
        let base = oracle.case(&mutation.base);
        assert_eq!(mutation.tol, base.tol, "{}", mutation.name);
        assert!(
            mutation.max_delta > f64::from(mutation.tol) + 1.0,
            "{} moves the oracle image by only {}",
            mutation.name,
            mutation.max_delta
        );
        // The Rust edit really changes the scenario the GPU is asked to render.
        let mutated = mutate(&mutation.name, base);
        let changed = mutated.effects != base.effects || mutated.zoom != base.zoom;
        assert!(changed, "{} leaves the scene untouched", mutation.name);
        assert_eq!(mutated.into, base.into);
    }
}

#[test]
fn gpu_mutating_the_stack_is_detected_against_the_unmutated_oracle() {
    let mut gpu = headless();
    let oracle = oracle();
    for mutation in &oracle.mutations {
        let base = oracle.case(&mutation.base);
        let image = oracle.image(&base.image);
        let mutated = mutate(&mutation.name, base);
        let outcome = run_plan(
            &mut gpu,
            &mutated.plan(),
            mutated.frame(),
            image,
            mutated.into,
        );
        let verdict = worst_mismatch(&outcome.bytes, &base.expect, base.tol);
        assert!(
            verdict.is_some(),
            "mutation {} was not detected",
            mutation.name
        );
    }
}

fn mid_grey() -> Image {
    Image::solid(20, 12, [128, 128, 128, 255])
}

fn frame_at(time: f32) -> FxFrame {
    FxFrame::new((0.0, 0.0, 20.0, 12.0), time).unwrap()
}

fn run_one(gpu: &mut Compositor, kind: &str, values: &[f32], image: &Image, time: f32) -> Vec<u8> {
    let plan = plan_specs(&[spec(kind, values)], 1.0, false);
    run_plan(gpu, &plan, frame_at(time), image, false).bytes
}

#[test]
fn gpu_grain_is_bounded_noise_shared_by_the_channels_that_depends_on_time() {
    // The hash is a chaotic sin(), so its pixels cannot be pinned against a CPU model; its
    // contract can: out = clamp(in + (n - 0.5) * p1) with ONE scalar n per pixel.
    let mut gpu = headless();
    let image = mid_grey();
    let first = run_one(&mut gpu, "grain", &[0.5], &image, 1.0);
    let again = run_one(&mut gpu, "grain", &[0.5], &image, 1.0);
    let later = run_one(&mut gpu, "grain", &[0.5], &image, 2.5);
    assert_eq!(
        first, again,
        "the noise is a pure function of the pixel and the time"
    );
    let mut values = Vec::new();
    for texel in first.chunks_exact(4) {
        assert_eq!(texel[0], texel[1], "one scalar noise for all channels");
        assert_eq!(texel[1], texel[2]);
        assert_eq!(texel[3], 255, "alpha is untouched");
        // (n - 0.5) * 0.5 spans -0.25..0.25 of 255 = +-63.75 around 128, plus rounding.
        assert!(
            (64..=192).contains(&texel[0]),
            "{} is outside the amplitude",
            texel[0]
        );
        values.push(f32::from(texel[0]));
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let variance =
        values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / values.len() as f32;
    assert!(
        variance.sqrt() > 15.0,
        "noise is not flat: deviation {}",
        variance.sqrt()
    );
    let moved = first
        .chunks_exact(4)
        .zip(later.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(moved > 100, "a new time changes {moved} of 240 pixels");
    // Amplitude zero is the identity whatever the time.
    assert_eq!(run_one(&mut gpu, "grain", &[0.0], &image, 7.0), image.bytes);
}

#[test]
fn gpu_contour_roughness_jitters_the_ring_but_roughness_zero_is_deterministic() {
    let oracle = oracle();
    let shape = oracle.image("shape20x12");
    let mut gpu = headless();
    let mut ring = |roughness: f32, time: f32| {
        run_one(
            &mut gpu,
            "contourBrut",
            &[1.5, roughness, 0.9, 0.8],
            shape,
            time,
        )
    };
    let smooth = ring(0.0, 0.0);
    let rough = ring(1.0, 0.0);
    let rough_again = ring(1.0, 0.0);
    let rough_later = ring(1.0, 3.0);
    assert_eq!(rough, rough_again, "repeatable for one time");
    assert_ne!(smooth, rough, "roughness moves the traced ring");
    assert_ne!(rough, rough_later, "the jitter follows the time");
    // Far from the silhouette the ring sees one alpha everywhere: no outline, whatever the jitter.
    for (corner, label) in [
        (0usize, "top left"),
        (19, "top right"),
        (11 * 20, "bottom left"),
        (11 * 20 + 19, "bottom right"),
    ] {
        assert_eq!(
            &rough[corner * 4..][..4],
            &shape.bytes[corner * 4..][..4],
            "{label}"
        );
    }
}

#[test]
fn gpu_a_built_in_effect_ignores_the_box_and_the_time() {
    let oracle = oracle();
    let case = oracle.case("mix-three-lands-in-a");
    assert_ne!(case.bbox, (0.0, 0.0, 20.0, 12.0));
    let image = oracle.image(&case.image);
    let mut gpu = headless();
    let plan = case.plan();
    let with_box = run_plan(&mut gpu, &plan, case.frame(), image, case.into);
    let other = FxFrame::new((5.0, 6.0, 3.0, 2.0), 99.0).unwrap();
    let without = run_plan(&mut gpu, &plan, other, image, case.into);
    assert_eq!(
        with_box.bytes, without.bytes,
        "invert, pixelate and sepia read neither"
    );
}

#[test]
fn gpu_the_callers_source_is_never_written() {
    let mut gpu = headless();
    let oracle = oracle();
    let case = oracle.case("mix-blur-bloom-threshold");
    let image = oracle.image(&case.image);
    let source = upload(&gpu, image);
    let plan = case.plan();
    let stack = GpuEffectStack::from_plan_with_frame(&plan, case.frame()).unwrap();
    let pixels = gpu
        .apply_effect_stack(&stack, source.view(), (image.w, image.h), &always_live)
        .unwrap();
    assert_eq!(pixels.passes(), case.passes);
    complete(&gpu);
    assert_eq!(read(&gpu, source.texture(), image.w, image.h), image.bytes);
}
