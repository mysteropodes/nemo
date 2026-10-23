//! L08-H negative and mutation controls, analytic invariants and the
//! independent Rust CPU cross-checks (`scene_effect_gpu_pass_reference.rs`).
//!
//! Nothing here reads the Node oracle's pixels except the controls that must
//! prove the oracle itself cannot pass a wrong render.
use super::reference;
use super::support::{
    headless, oracle, read, run_case, upload, worst_mismatch, Case, Image, Outcome,
};
use crate::compositor::scene_effect_gpu_pass::GpuEffectStack;
use crate::compositor::Compositor;
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    plan_effect_stack, EffectSpec,
};

const MUTATIONS: [&str; 7] = [
    "swap-order",
    "enable-the-disabled-entry",
    "ignore-the-zoom",
    "glow-as-plain-blur",
    "negate-brightness",
    "explicit-zero-read-as-default",
    "halve-the-blur-radius",
];

/// The same edit the generator proved moves the image (tools/gen_..._oracle.js).
fn mutate(name: &str, base: &Case) -> Case {
    let mut case = base.clone();
    match name {
        "swap-order" => case.effects.reverse(),
        "enable-the-disabled-entry" => case.effects.iter_mut().for_each(|e| e.enabled = true),
        "ignore-the-zoom" => case.zoom = 1.0,
        "glow-as-plain-blur" => case
            .effects
            .iter_mut()
            .for_each(|e| e.effect_type = "blur".to_owned()),
        "negate-brightness" => case
            .effects
            .iter_mut()
            .for_each(|e| e.params[0] = e.params[0].map(|v| -v)),
        "explicit-zero-read-as-default" => case.effects.iter_mut().for_each(|e| e.params[0] = None),
        "halve-the-blur-radius" => case
            .effects
            .iter_mut()
            .for_each(|e| e.params[0] = e.params[0].map(|v| v / 2.0)),
        other => panic!("unknown mutation {other}"),
    }
    case
}

fn spec(kind: &str, values: &[f32]) -> EffectSpec {
    let mut params = [None; 8];
    for (slot, value) in params.iter_mut().zip(values) {
        *slot = Some(*value);
    }
    EffectSpec::new(kind, true).with_params(params)
}

fn ramp() -> Image {
    Image {
        w: 8,
        h: 8,
        bytes: (0..64u32)
            .flat_map(|k| [((k % 8) * 36) as u8, ((k / 8) * 36) as u8, 128, 255])
            .collect(),
    }
}

/// Run `effects` as a one-off scenario through the landing entry.
fn adhoc(gpu: &mut Compositor, image: &Image, effects: Vec<EffectSpec>, zoom: f64) -> Outcome {
    let case = Case {
        name: "adhoc".to_owned(),
        tol: 0,
        image: String::new(),
        zoom,
        into: false,
        passes: 0,
        effects,
        expect: Vec::new(),
    };
    run_case(gpu, &case, image)
}

fn assert_close(actual: &[u8], expected: &[u8], tol: u8, what: &str) {
    if let Some((pixel, channel, delta)) = worst_mismatch(actual, expected, tol) {
        panic!("{what}: pixel {pixel} channel {channel} off by {delta}, tolerance {tol}");
    }
}

#[test]
fn a_corrupted_oracle_byte_is_detected_and_the_tolerance_edge_is_exact() {
    let oracle = oracle();
    let case = oracle.case("color-adjust-brightness");
    assert_eq!(case.tol, 1);
    let exact = case.expect.clone();
    assert!(worst_mismatch(&exact, &case.expect, 1).is_none());
    for (index, label) in [(0, "first byte"), (exact.len() - 1, "last byte")] {
        let mut broken = exact.clone();
        broken[index] ^= 0x80;
        assert!(
            worst_mismatch(&broken, &case.expect, 1).is_some(),
            "{label}"
        );
    }
    // Opaque colour: +1 passes, +2 fails.
    let mut edge = exact.clone();
    edge[0] += 1;
    assert!(worst_mismatch(&edge, &case.expect, 1).is_none());
    edge[0] += 1;
    assert!(worst_mismatch(&edge, &case.expect, 1).is_some());
    // Alpha: -1 passes, -2 fails.
    let mut alpha = exact.clone();
    alpha[3] -= 1;
    assert!(worst_mismatch(&alpha, &case.expect, 1).is_none());
    alpha[3] -= 1;
    assert!(worst_mismatch(&alpha, &case.expect, 1).is_some());
    // Straight colour under zero alpha is noise: the impulse blur has such texels.
    let blur = oracle.case("blur-impulse-radius-6");
    let noisy = (0..blur.expect.len() / 4)
        .find(|p| blur.expect[p * 4 + 3] == 0 && blur.expect[p * 4] != 0)
        .expect("a texel with colour under zero alpha");
    let mut cleared = blur.expect.clone();
    cleared[noisy * 4..noisy * 4 + 3].fill(0);
    assert!(worst_mismatch(&cleared, &blur.expect, blur.tol).is_none());
    cleared[noisy * 4 + 3] = 3;
    assert!(worst_mismatch(&cleared, &blur.expect, blur.tol).is_some());
}

#[test]
fn every_mutation_the_generator_proved_moves_the_image_beyond_its_tolerance() {
    let oracle = oracle();
    let mut names: Vec<&str> = oracle.mutations.iter().map(|m| m.name.as_str()).collect();
    names.sort_unstable();
    let mut expected = MUTATIONS.to_vec();
    expected.sort_unstable();
    assert_eq!(
        names, expected,
        "the Rust mutations are the generator's mutations"
    );
    for mutation in &oracle.mutations {
        let base = oracle.case(&mutation.base);
        assert_eq!(mutation.tol, base.tol, "{}", mutation.name);
        assert!(
            mutation.max_delta > f64::from(base.tol) + 1.0,
            "{} moves the image by only {}",
            mutation.name,
            mutation.max_delta
        );
        // The Rust edit really changes the scenario it is supposed to change.
        let mutated = mutate(&mutation.name, base);
        assert!(
            mutated.effects != base.effects || mutated.zoom != base.zoom,
            "{} edits nothing",
            mutation.name
        );
    }
}

#[test]
fn gpu_mutating_the_stack_is_detected_against_the_unmutated_oracle() {
    let mut gpu = headless();
    let oracle = oracle();
    for mutation in &oracle.mutations {
        let base = oracle.case(&mutation.base);
        let image = oracle.image(&base.image);
        let honest = run_case(&mut gpu, base, image).bytes;
        assert!(worst_mismatch(&honest, &base.expect, base.tol).is_none());
        let mutated = run_case(&mut gpu, &mutate(&mutation.name, base), image).bytes;
        assert!(
            worst_mismatch(&mutated, &base.expect, base.tol).is_some(),
            "mutation {} was not detected",
            mutation.name
        );
    }
}

#[test]
fn gpu_analytic_invariants_hold_without_the_oracle() {
    let mut gpu = headless();
    let ramp = ramp();
    let white = Image::solid(8, 8, [255, 255, 255, 255]);
    let black = Image::solid(8, 8, [0, 0, 0, 255]);
    let flat = Image::solid(8, 8, [200, 100, 50, 255]);
    let run = |gpu: &mut Compositor, image: &Image, kind: &str, values: &[f32]| {
        adhoc(gpu, image, vec![spec(kind, values)], 1.0).bytes
    };
    // Neutral parameters are the identity.
    for (kind, values) in [
        ("colorAdjust", vec![0.0, 0.0]),
        ("vignette", vec![0.0]),
        ("blur", vec![0.0]),
    ] {
        assert_close(&run(&mut gpu, &ramp, kind, &values), &ramp.bytes, 1, kind);
    }
    // Brightness +1 saturates every channel; contrast -1 flattens to mid grey.
    let bright = run(&mut gpu, &ramp, "colorAdjust", &[1.0, 0.0]);
    assert!(bright.chunks_exact(4).all(|px| px == [255, 255, 255, 255]));
    let flattened = run(&mut gpu, &ramp, "colorAdjust", &[0.0, -1.0]);
    assert!(flattened
        .chunks_exact(4)
        .all(|px| (127..=128).contains(&px[0])
            && px[0] == px[1]
            && px[1] == px[2]
            && px[3] == 255));
    // Vignette strength 1, start 0.2 on white: the 8x8 centre is untouched, the corner
    // texel has d = 0.4375 * sqrt(2) / 0.7071 = 0.875, falloff = 0.84375, 255 * 0.15625 ~ 40.
    let vignetted = run(&mut gpu, &white, "vignette", &[1.0, 0.2]);
    for centre in [3 * 8 + 3, 3 * 8 + 4, 4 * 8 + 3, 4 * 8 + 4] {
        assert_eq!(&vignetted[centre * 4..centre * 4 + 4], [255, 255, 255, 255]);
    }
    for corner in [0, 7, 56, 63] {
        let px = &vignetted[corner * 4..corner * 4 + 4];
        assert!((39..=41).contains(&px[0]) && px[0] == px[1] && px[1] == px[2] && px[3] == 255);
    }
    // A uniform image is a fixed point of the Gaussian, edges included (clamp to edge).
    assert_close(
        &run(&mut gpu, &flat, "blur", &[12.0]),
        &flat.bytes,
        1,
        "uniform blur",
    );
    // Screen(0, blur(0)) = 0 and Screen(1, blur(1)) = 1: glow cannot create or lose light there.
    assert_close(
        &run(&mut gpu, &black, "glow", &[8.0]),
        &black.bytes,
        1,
        "glow black",
    );
    assert_close(
        &run(&mut gpu, &white, "glow", &[8.0]),
        &white.bytes,
        1,
        "glow white",
    );
}

#[test]
fn gpu_colour_adjust_and_vignette_match_the_independent_rust_reference() {
    let mut gpu = headless();
    let oracle = oracle();
    let ramp = ramp();
    let translucent = oracle.image("trans8");
    for image in [&ramp, translucent] {
        for (brightness, contrast) in [(0.1f32, -0.3f32), (-0.2, 0.6), (0.0, 0.0)] {
            let gpu_bytes = adhoc(
                &mut gpu,
                image,
                vec![spec("colorAdjust", &[brightness, contrast])],
                1.0,
            )
            .bytes;
            let cpu = reference::color_adjust(&image.bytes, brightness, contrast);
            assert_close(&gpu_bytes, &cpu, 1, "colorAdjust vs reference");
        }
        for (strength, radius) in [(0.7f32, 0.25f32), (1.0, 0.0), (0.3, 0.9)] {
            let gpu_bytes = adhoc(
                &mut gpu,
                image,
                vec![spec("vignette", &[strength, radius])],
                1.0,
            )
            .bytes;
            let cpu = reference::vignette(&image.bytes, (image.w, image.h), strength, radius);
            assert_close(&gpu_bytes, &cpu, 1, "vignette vs reference");
        }
    }
}

#[test]
fn gpu_blur_matches_the_independent_rust_reference() {
    let mut gpu = headless();
    let oracle = oracle();
    for (name, radius) in [
        ("edge16", 3.0f32),
        ("edge16", 12.0),
        ("impulse16", 6.0),
        ("trans8", 2.0),
        ("grad8", 24.0),
    ] {
        let image = oracle.image(name);
        let gpu_bytes = adhoc(&mut gpu, image, vec![spec("blur", &[radius])], 1.0).bytes;
        let cpu = reference::blur(&image.bytes, (image.w, image.h), radius);
        assert_close(&gpu_bytes, &cpu, 2, &format!("blur {name} {radius}"));
    }
}

#[test]
fn gpu_zoom_two_with_radius_three_is_bit_identical_to_zoom_one_with_radius_six() {
    let mut gpu = headless();
    let oracle = oracle();
    let image = oracle.image("edge16");
    let zoomed = adhoc(&mut gpu, image, vec![spec("blur", &[3.0])], 2.0).bytes;
    let direct = adhoc(&mut gpu, image, vec![spec("blur", &[6.0])], 1.0).bytes;
    assert_eq!(zoomed, direct);
    let unscaled = adhoc(&mut gpu, image, vec![spec("blur", &[3.0])], 1.0).bytes;
    assert_ne!(zoomed, unscaled, "the zoom really widened the kernel");
}

#[test]
fn gpu_a_disabled_entry_costs_no_pass_and_leaves_pixels_identical() {
    let mut gpu = headless();
    let ramp = ramp();
    let mut disabled = spec("blur", &[4.0]);
    disabled.enabled = false;
    let with_gap = adhoc(
        &mut gpu,
        &ramp,
        vec![
            spec("colorAdjust", &[0.3, 0.2]),
            disabled,
            spec("vignette", &[0.8, 0.3]),
        ],
        1.0,
    );
    let without = adhoc(
        &mut gpu,
        &ramp,
        vec![
            spec("colorAdjust", &[0.3, 0.2]),
            spec("vignette", &[0.8, 0.3]),
        ],
        1.0,
    );
    assert_eq!(with_gap.bytes, without.bytes);
    assert_eq!((with_gap.passes, without.passes), (2, 2));
}

#[test]
fn gpu_the_landing_buffer_alternates_with_the_enabled_count() {
    // k brightness steps of 0.1 move an unsaturated texel by ~25.5 each, so a landing
    // taken from the wrong ping-pong texture is off by a whole step.
    let mut gpu = headless();
    let ramp = ramp();
    let mut expected = ramp.bytes.clone();
    let mut previous: Option<Vec<u8>> = None;
    for count in 1..=4usize {
        expected = reference::color_adjust(&expected, 0.1, 0.0);
        let effects: Vec<EffectSpec> = (0..count)
            .map(|_| spec("colorAdjust", &[0.1, 0.0]))
            .collect();
        let landing = adhoc(&mut gpu, &ramp, effects.clone(), 1.0).bytes;
        assert_close(
            &landing,
            &expected,
            2,
            &format!("{count} entries, landing entry"),
        );
        let into_case = Case {
            name: "adhoc-into".to_owned(),
            tol: 0,
            image: String::new(),
            zoom: 1.0,
            into: true,
            passes: 0,
            effects,
            expect: Vec::new(),
        };
        let into = run_case(&mut gpu, &into_case, &ramp).bytes;
        assert_close(
            &into,
            &expected,
            2,
            &format!("{count} entries, final-target entry"),
        );
        if let Some(before) = previous {
            assert!(
                worst_mismatch(&landing, &before, 8).is_some(),
                "step {count} moved nothing"
            );
        }
        previous = Some(landing);
    }
}

#[test]
fn gpu_the_callers_source_is_never_written() {
    let mut gpu = headless();
    let oracle = oracle();
    let image = oracle.image("edge16");
    let source = upload(&gpu, image);
    let plan = plan_effect_stack(
        &[
            spec("blur", &[3.0]),
            spec("colorAdjust", &[0.2, 0.0]),
            spec("glow", &[2.0]),
        ],
        1.0,
        &|_| false,
    );
    let stack = GpuEffectStack::from_plan(&plan).unwrap();
    let pixels = gpu
        .apply_effect_stack(
            &stack,
            source.view(),
            (image.w, image.h),
            &super::support::always_live,
        )
        .unwrap();
    assert_eq!(pixels.passes(), 6);
    assert_eq!(read(&gpu, source.texture(), image.w, image.h), image.bytes);
    assert_ne!(read(&gpu, pixels.texture(), image.w, image.h), image.bytes);
}
