//! Oracle parser, GPU harness and pixel helpers shared by the L08-I tests.
//!
//! `fixtures/scene-effect-gpu-fx/oracle.txt` is BOTH the scenario table (images,
//! effect stacks, zoom, entry mode, box, time) and the expected pixels: the Rust
//! side builds its plan, frame and GPU input from the very lines the Node
//! generator simulated, so scene and expectation cannot drift apart.
use crate::compositor::object_target::ObjectTarget;
use crate::compositor::scene_effect_gpu_fx::FxFrame;
use crate::compositor::scene_effect_gpu_pass::GpuEffectStack;
use crate::compositor::{Compositor, CompositorError};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    plan_effect_stack, plan_effect_stack_into, EffectSpec, EffectStackPlan,
};

pub(super) const ORACLE: &str = include_str!("fixtures/scene-effect-gpu-fx/oracle.txt");

#[derive(Clone, Debug)]
pub(super) struct Image {
    pub(super) w: u32,
    pub(super) h: u32,
    pub(super) bytes: Vec<u8>,
}

impl Image {
    pub(super) fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Self {
        Self {
            w,
            h,
            bytes: (0..w * h).flat_map(|_| rgba).collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Case {
    pub(super) name: String,
    pub(super) tol: u8,
    pub(super) image: String,
    pub(super) zoom: f64,
    pub(super) into: bool,
    pub(super) passes: u32,
    pub(super) bbox: (f32, f32, f32, f32),
    pub(super) time: f32,
    pub(super) effects: Vec<EffectSpec>,
    pub(super) expect: Vec<u8>,
}

impl Case {
    /// The planner's output for this scenario: no custom effect is registered.
    pub(super) fn plan(&self) -> EffectStackPlan {
        plan_specs(&self.effects, self.zoom, self.into)
    }

    pub(super) fn frame(&self) -> FxFrame {
        FxFrame::new(self.bbox, self.time).expect("oracle frames are finite")
    }
}

pub(super) fn plan_specs(effects: &[EffectSpec], zoom: f64, into: bool) -> EffectStackPlan {
    if into {
        plan_effect_stack_into(effects, zoom, &|_| false)
    } else {
        plan_effect_stack(effects, zoom, &|_| false)
    }
}

/// An enabled entry with every listed parameter present (`p1..`).
pub(super) fn spec(kind: &str, values: &[f32]) -> EffectSpec {
    let mut params = [None; 8];
    for (slot, value) in params.iter_mut().zip(values) {
        *slot = Some(*value);
    }
    EffectSpec::new(kind, true).with_params(params)
}

#[derive(Clone, Debug)]
pub(super) struct Mutation {
    pub(super) name: String,
    pub(super) base: String,
    pub(super) max_delta: f64,
    pub(super) tol: u8,
}

pub(super) struct Oracle {
    pub(super) images: Vec<(String, Image)>,
    pub(super) cases: Vec<Case>,
    pub(super) mutations: Vec<Mutation>,
}

impl Oracle {
    pub(super) fn image(&self, name: &str) -> &Image {
        &self
            .images
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("oracle image {name}"))
            .1
    }

    pub(super) fn case(&self, name: &str) -> &Case {
        self.cases
            .iter()
            .find(|case| case.name == name)
            .unwrap_or_else(|| panic!("oracle case {name}"))
    }
}

pub(super) fn decode_hex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|k| u8::from_str_radix(&text[k * 2..k * 2 + 2], 16).expect("hex byte"))
        .collect()
}

fn param(token: &str) -> Option<f32> {
    (token != "-").then(|| token.parse().expect("effect parameter"))
}

fn number(token: &str) -> f32 {
    token.parse().expect("oracle number")
}

pub(super) fn parse(text: &str) -> Oracle {
    let mut oracle = Oracle {
        images: Vec::new(),
        cases: Vec::new(),
        mutations: Vec::new(),
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens[0] == "#" {
            if tokens.get(1) == Some(&"mutation") {
                oracle.mutations.push(Mutation {
                    name: tokens[2].to_owned(),
                    base: tokens[4].to_owned(),
                    max_delta: tokens[6].parse().expect("mutation delta"),
                    tol: tokens[8].parse().expect("mutation tolerance"),
                });
            }
            continue;
        }
        match tokens[0] {
            "image" => oracle.images.push((
                tokens[1].to_owned(),
                Image {
                    w: tokens[2].parse().expect("image width"),
                    h: tokens[3].parse().expect("image height"),
                    bytes: Vec::new(),
                },
            )),
            "px" => {
                let image = &mut oracle.images.last_mut().expect("px before an image").1;
                let y: usize = tokens[1].parse().expect("pixel row");
                assert_eq!(y * image.w as usize * 4, image.bytes.len(), "rows in order");
                image.bytes.extend(decode_hex(tokens[2]));
            }
            "end-image" => {
                let image = &oracle.images.last().expect("end-image").1;
                assert_eq!((image.w * image.h * 4) as usize, image.bytes.len());
            }
            "case" => oracle.cases.push(Case {
                name: tokens[1].to_owned(),
                tol: tokens[3].parse().expect("tolerance"),
                image: tokens[5].to_owned(),
                zoom: tokens[7].parse().expect("zoom"),
                into: tokens[9] == "into",
                passes: tokens[11].parse().expect("pass count"),
                bbox: (
                    number(tokens[13]),
                    number(tokens[14]),
                    number(tokens[15]),
                    number(tokens[16]),
                ),
                time: number(tokens[18]),
                effects: Vec::new(),
                expect: Vec::new(),
            }),
            "effect" => {
                let case = oracle.cases.last_mut().expect("effect before a case");
                let mut params = [None; 8];
                for (slot, token) in params.iter_mut().zip(&tokens[3..11]) {
                    *slot = param(token);
                }
                case.effects
                    .push(EffectSpec::new(tokens[1], tokens[2] == "1").with_params(params));
            }
            "row" => {
                let case = oracle.cases.last_mut().expect("row before a case");
                let y: usize = tokens[1].parse().expect("row index");
                let width = oracle
                    .images
                    .iter()
                    .find(|(n, _)| *n == case.image)
                    .map(|(_, i)| i.w as usize)
                    .expect("case image");
                assert_eq!(y * width * 4, case.expect.len(), "rows arrive in order");
                case.expect.extend(decode_hex(tokens[2]));
            }
            "end" => {}
            other => panic!("unknown oracle line {other}"),
        }
    }
    oracle
}

pub(super) fn oracle() -> Oracle {
    parse(ORACLE)
}

pub(super) fn headless() -> Compositor {
    // A missing real adapter is an explicit test FAILURE, never a skip.
    Compositor::new().expect("L08-I needs a real headless Metal adapter")
}

pub(super) fn complete(gpu: &Compositor) {
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}

pub(super) fn textures(gpu: &Compositor) -> usize {
    gpu.instance
        .generate_report()
        .unwrap()
        .hub
        .textures
        .num_allocated
}

/// A charged RGBA8 target holding `image` (the caller-owned source or final
/// target of an effect run). Its completion fence is armed.
pub(super) fn upload(gpu: &Compositor, image: &Image) -> ObjectTarget {
    let mut target = gpu
        .make_object_target(image.w, image.h)
        .expect("test target");
    gpu.queue.write_texture(
        target.texture().as_image_copy(),
        &image.bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.w * 4),
            rows_per_image: Some(image.h),
        },
        wgpu::Extent3d {
            width: image.w,
            height: image.h,
            depth_or_array_layers: 1,
        },
    );
    target.render(|_| Ok(())).expect("arm the fence");
    target
}

pub(super) fn read(gpu: &Compositor, texture: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    let bytes = gpu.readback_texture(texture, w, h).expect("RGBA8 readback");
    let raw: &[u8] = &bytes;
    raw.to_vec()
}

pub(super) fn always_live() -> Result<(), CompositorError> {
    Ok(())
}

pub(super) struct Outcome {
    pub(super) bytes: Vec<u8>,
    pub(super) passes: u32,
}

/// Admit `plan` with `frame` and run it on the GPU through the matching entry.
pub(super) fn run_plan(
    gpu: &mut Compositor,
    plan: &EffectStackPlan,
    frame: FxFrame,
    image: &Image,
    into: bool,
) -> Outcome {
    let stack = GpuEffectStack::from_plan_with_frame(plan, frame).expect("plan is admitted");
    let source = upload(gpu, image);
    let size = (image.w, image.h);
    if into {
        let sentinel = Image::solid(image.w, image.h, [9, 9, 9, 9]);
        let destination = upload(gpu, &sentinel);
        let run = gpu
            .apply_effect_stack_into(
                &stack,
                source.view(),
                destination.view(),
                size,
                &always_live,
            )
            .expect("final-target effect stack");
        Outcome {
            bytes: read(gpu, destination.texture(), size.0, size.1),
            passes: run.passes,
        }
    } else {
        let pixels = gpu
            .apply_effect_stack(&stack, source.view(), size, &always_live)
            .expect("landing effect stack");
        assert_eq!(pixels.dimensions(), size);
        Outcome {
            bytes: read(gpu, pixels.texture(), size.0, size.1),
            passes: pixels.passes(),
        }
    }
}

pub(super) fn run_case(gpu: &mut Compositor, case: &Case, image: &Image) -> Outcome {
    run_plan(gpu, &case.plan(), case.frame(), image, case.into)
}

/// First pixel whose alpha differs by more than `tol` steps or whose
/// PREMULTIPLIED colour differs by more than `tol` steps (|c*a - c'*a'| / 255);
/// a straight colour channel under (near) zero alpha is quantisation noise.
/// Returns (pixel index, channel, delta). Opaque pixels reduce to |c - c'|.
pub(super) fn worst_mismatch(
    actual: &[u8],
    expected: &[u8],
    tol: u8,
) -> Option<(usize, usize, f32)> {
    assert_eq!(actual.len(), expected.len());
    let limit = f32::from(tol);
    for pixel in 0..actual.len() / 4 {
        let (a, e) = (&actual[pixel * 4..][..4], &expected[pixel * 4..][..4]);
        let alpha = f32::from(a[3].abs_diff(e[3]));
        if alpha > limit {
            return Some((pixel, 3, alpha));
        }
        for channel in 0..3 {
            let delta = (f32::from(a[channel]) * f32::from(a[3])
                - f32::from(e[channel]) * f32::from(e[3]))
            .abs()
                / 255.0;
            if delta > limit {
                return Some((pixel, channel, delta));
            }
        }
    }
    None
}

pub(super) fn check_case(case: &Case, width: u32, actual: &[u8]) -> Result<(), String> {
    match worst_mismatch(actual, &case.expect, case.tol) {
        None => Ok(()),
        Some((pixel, channel, delta)) => Err(format!(
            "{}: pixel ({}, {}) channel {channel} is off by {delta}, tolerance {}",
            case.name,
            pixel % width as usize,
            pixel / width as usize,
            case.tol
        )),
    }
}

/// Run every oracle case whose name starts with `prefix`; returns the count.
pub(super) fn run_group(prefix: &str) -> usize {
    let mut gpu = headless();
    let oracle = oracle();
    let mut count = 0;
    for case in oracle.cases.iter().filter(|c| c.name.starts_with(prefix)) {
        let image = oracle.image(&case.image);
        let outcome = run_case(&mut gpu, case, image);
        if let Err(message) = check_case(case, image.w, &outcome.bytes) {
            panic!("{message}");
        }
        assert_eq!(outcome.passes, case.passes, "{} pass count", case.name);
        count += 1;
    }
    count
}
