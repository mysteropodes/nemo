//! Oracle parser, scene builder and pixel helpers shared by the L08-G tests.
//!
//! `fixtures/scene-effect-gpu-blend/oracle.txt` is BOTH the scenario table and
//! the expected pixels: the Rust side builds its GPU input from the very lines
//! the Node generator simulated, so scene and expectation cannot drift apart.
use crate::compositor::scene_effect_gpu_blend::{
    GpuBlendError, GpuFill, GpuLayerInput, GpuLayerPaint, GpuLayerStack, GpuMask, GpuMaskMode,
};
use crate::compositor::Compositor;
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    plan_composition, LayerComposition, MatteExtra,
};
use vello::kurbo::{BezPath, Rect, Shape};

pub(super) const ORACLE: &str = include_str!("fixtures/scene-effect-gpu-blend/oracle.txt");
pub(super) const SIDE: usize = 8;
pub(super) const SIZE: (u32, u32) = (8, 8);

pub(super) struct CaseLayer {
    pub(super) blend: Option<String>,
    pub(super) matte: Option<(String, Option<usize>)>,
    pub(super) more: Vec<(String, usize)>,
    pub(super) background: bool,
    pub(super) effect: bool,
    pub(super) fills: Vec<([i32; 4], [f32; 4])>,
    pub(super) masks: Vec<(GpuMaskMode, [i32; 4])>,
}

pub(super) struct Case {
    pub(super) name: String,
    pub(super) tol: u8,
    pub(super) base: [u8; 4],
    pub(super) layers: Vec<CaseLayer>,
    pub(super) expect: Vec<u8>,
}

fn rect(tokens: &[&str]) -> [i32; 4] {
    std::array::from_fn(|k| tokens[k].parse().expect("rect coordinate"))
}

fn rgba(tokens: &[&str]) -> [f32; 4] {
    std::array::from_fn(|k| tokens[k].parse().expect("straight colour channel"))
}

fn parse_layer(tokens: &[&str]) -> CaseLayer {
    let mut layer = CaseLayer {
        blend: None,
        matte: None,
        more: Vec::new(),
        background: false,
        effect: false,
        fills: Vec::new(),
        masks: Vec::new(),
    };
    let mut at = 0;
    while at < tokens.len() {
        match tokens[at] {
            "blend" => {
                layer.blend = Some(tokens[at + 1].to_owned());
                at += 2;
            }
            "matte" => {
                let explicit = tokens.get(at + 2) == Some(&"src");
                let source = explicit.then(|| tokens[at + 3].parse().expect("matte source"));
                layer.matte = Some((tokens[at + 1].to_owned(), source));
                at += if explicit { 4 } else { 2 };
            }
            "more" => {
                let source = tokens[at + 2].parse().expect("extra matte source");
                layer.more.push((tokens[at + 1].to_owned(), source));
                at += 3;
            }
            "bg" => {
                layer.background = true;
                at += 1;
            }
            "effect" => {
                layer.effect = true;
                at += 1;
            }
            other => panic!("unknown layer token {other}"),
        }
    }
    layer
}

fn decode_hex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|k| u8::from_str_radix(&text[k * 2..k * 2 + 2], 16).expect("hex byte"))
        .collect()
}

pub(super) fn parse(text: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens[0] == "case" {
            cases.push(Case {
                name: tokens[1].to_owned(),
                tol: tokens[3].parse().expect("tolerance"),
                base: std::array::from_fn(|k| tokens[5 + k].parse().expect("base byte")),
                layers: Vec::new(),
                expect: Vec::new(),
            });
            continue;
        }
        let case = cases.last_mut().expect("line before the first case");
        match tokens[0] {
            "layer" => case.layers.push(parse_layer(&tokens[1..])),
            "fill" => {
                let layer = case.layers.last_mut().expect("fill before a layer");
                layer.fills.push((rect(&tokens[1..5]), rgba(&tokens[5..9])));
            }
            "mask" => {
                let layer = case.layers.last_mut().expect("mask before a layer");
                let mode = match tokens[1] {
                    "add" => GpuMaskMode::Add,
                    "subtract" => GpuMaskMode::Subtract,
                    "intersect" => GpuMaskMode::Intersect,
                    other => panic!("unknown mask mode {other}"),
                };
                layer.masks.push((mode, rect(&tokens[2..6])));
            }
            "row" => {
                let y: usize = tokens[1].parse().expect("row index");
                assert_eq!(y * SIDE * 4, case.expect.len(), "rows arrive in order");
                case.expect.extend(decode_hex(tokens[2]));
            }
            "end" => assert_eq!(case.expect.len(), SIDE * SIDE * 4, "{}", case.name),
            other => panic!("unknown oracle line {other}"),
        }
    }
    cases
}

pub(super) fn oracle() -> Vec<Case> {
    parse(ORACLE)
}

pub(super) fn find(name: &str) -> Case {
    oracle()
        .into_iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("oracle case {name}"))
}

/// Closed pixel-aligned rectangles, owned so the stack can borrow them.
pub(super) struct Geometry {
    fills: Vec<Vec<BezPath>>,
    masks: Vec<Vec<BezPath>>,
}

fn rect_path(r: &[i32; 4]) -> BezPath {
    Rect::new(
        f64::from(r[0]),
        f64::from(r[1]),
        f64::from(r[2]),
        f64::from(r[3]),
    )
    .to_path(0.1)
}

impl Geometry {
    pub(super) fn of(case: &Case) -> Self {
        Self {
            fills: case
                .layers
                .iter()
                .map(|l| l.fills.iter().map(|(r, _)| rect_path(r)).collect())
                .collect(),
            masks: case
                .layers
                .iter()
                .map(|l| l.masks.iter().map(|(_, r)| rect_path(r)).collect())
                .collect(),
        }
    }
}

/// The planner's input, built from the wire fields the oracle lines carry.
pub(super) fn compositions(case: &Case) -> Vec<LayerComposition> {
    case.layers
        .iter()
        .map(|l| LayerComposition {
            blend_mode: l.blend.clone(),
            matte_mode: l.matte.as_ref().map(|m| m.0.clone()),
            matte_source_index: l.matte.as_ref().and_then(|m| m.1),
            mattes_more: (!l.more.is_empty()).then(|| {
                l.more
                    .iter()
                    .map(|(mode, source)| MatteExtra {
                        mode: Some(mode.clone()),
                        source_index: Some(*source),
                    })
                    .collect()
            }),
            is_effect_layer: l.effect.then_some(true),
            is_canvas_background: l.background.then_some(true),
            mask_count: l.masks.len(),
            ..Default::default()
        })
        .collect()
}

pub(super) fn inputs<'a>(case: &'a Case, geometry: &'a Geometry) -> Vec<GpuLayerInput<'a>> {
    case.layers
        .iter()
        .enumerate()
        .map(|(i, layer)| GpuLayerInput {
            paint: GpuLayerPaint {
                fills: layer
                    .fills
                    .iter()
                    .zip(&geometry.fills[i])
                    .map(|((_, rgba), path)| GpuFill::solid(path, *rgba))
                    .collect(),
            },
            masks: layer
                .masks
                .iter()
                .zip(&geometry.masks[i])
                .map(|((mode, _), path)| GpuMask {
                    mode: *mode,
                    paint: GpuLayerPaint {
                        fills: vec![GpuFill {
                            path,
                            rgba: [1.0, 1.0, 1.0, 1.0],
                            prepared: None,
                        }],
                    },
                })
                .collect(),
            mask_feather: 0.0,
        })
        .collect()
}

pub(super) fn stack<'a>(
    case: &'a Case,
    geometry: &'a Geometry,
) -> Result<GpuLayerStack<'a>, GpuBlendError> {
    GpuLayerStack::from_plan(
        &plan_composition(&compositions(case)),
        inputs(case, geometry),
    )
}

pub(super) fn headless() -> Compositor {
    // A missing real adapter is an explicit test FAILURE, never a skip.
    Compositor::new().expect("L08-G needs a real headless Metal adapter")
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

/// Compose one oracle case on the GPU and read the RGBA8 bytes back.
pub(super) fn render(gpu: &mut Compositor, case: &Case) -> Vec<u8> {
    let geometry = Geometry::of(case);
    let layered = stack(case, &geometry).expect("oracle case plans a layered stack");
    let pixels = gpu
        .compose_layered_stack(&layered, SIZE, case.base)
        .expect("layered GPU composition");
    assert_eq!(pixels.dimensions(), SIZE);
    let bytes = gpu
        .readback_texture(pixels.texture(), SIZE.0, SIZE.1)
        .expect("RGBA8 readback");
    let raw: &[u8] = &bytes;
    raw.to_vec()
}

/// First channel that differs by more than `tol`, as (byte index, actual, expected).
pub(super) fn worst_mismatch(actual: &[u8], expected: &[u8], tol: u8) -> Option<(usize, u8, u8)> {
    assert_eq!(actual.len(), expected.len());
    actual
        .iter()
        .zip(expected)
        .enumerate()
        .find(|(_, (a, e))| a.abs_diff(**e) > tol)
        .map(|(k, (a, e))| (k, *a, *e))
}

pub(super) fn check_case(case: &Case, actual: &[u8]) -> Result<(), String> {
    match worst_mismatch(actual, &case.expect, case.tol) {
        None => Ok(()),
        Some((k, a, e)) => Err(format!(
            "{}: pixel ({}, {}) channel {} is {a}, oracle {e}, tolerance {}",
            case.name,
            (k / 4) % SIDE,
            (k / 4) / SIDE,
            k % 4,
            case.tol
        )),
    }
}

/// Run every oracle case whose name starts with `prefix`; returns the count.
pub(super) fn run_group(prefix: &str) -> usize {
    let mut gpu = headless();
    let mut count = 0;
    for case in oracle().iter().filter(|c| c.name.starts_with(prefix)) {
        let actual = render(&mut gpu, case);
        if let Err(message) = check_case(case, &actual) {
            panic!("{message}");
        }
        count += 1;
    }
    count
}
