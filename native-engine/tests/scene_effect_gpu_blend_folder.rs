//! Folder layers on the layered GPU path: the children are flattened in isolation, at the folder's own place in
//! the stack, then composed as one layer. Every literal is derived by hand from `paint_folder_children` of the
//! baseline engine (first child seeds a transparent accumulator, later children blend in with their own mode,
//! the children's own masks and mattes are not honoured inside a folder). Real Metal; no adapter is a FAILURE.
use super::support::{headless, SIZE};
use crate::compositor::scene_effect_gpu_blend::{
    GpuBlendError, GpuFill, GpuFolderChild, GpuLayerInput, GpuLayerPaint, GpuLayerStack,
    GpuUnavailable,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    plan_composition, LayerComposition,
};
use vello::kurbo::{BezPath, Rect, Shape};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
    Rect::new(x0, y0, x1, y1).to_path(0.1)
}

fn painted<'a>(path: &'a BezPath, rgba: [f32; 4]) -> GpuLayerInput<'a> {
    GpuLayerInput {
        paint: GpuLayerPaint {
            fills: vec![GpuFill::solid(path, rgba)],
        },
        ..Default::default()
    }
}

fn folder(children: &[usize]) -> LayerComposition {
    LayerComposition {
        is_folder_layer: Some(true),
        folder_child_indices: Some(children.to_vec()),
        ..Default::default()
    }
}

fn blended(mode: &str) -> LayerComposition {
    LayerComposition {
        blend_mode: Some(mode.to_owned()),
        ..Default::default()
    }
}

fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * SIZE.0 as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

fn render(layers: &[LayerComposition], inputs: Vec<GpuLayerInput<'_>>) -> Vec<u8> {
    let stack = GpuLayerStack::from_plan(&plan_composition(layers), inputs).unwrap();
    let mut gpu = headless();
    let pixels = gpu
        .compose_layered_stack(&stack, SIZE, [255; 4])
        .expect("layered GPU composition");
    let bytes = gpu
        .readback_texture(pixels.texture(), SIZE.0, SIZE.1)
        .expect("RGBA8 readback");
    let raw: &[u8] = &bytes;
    raw.to_vec()
}

#[test]
fn the_stack_carries_the_folder_children_in_order_and_skips_them_at_the_top_level() {
    let (c, a, b) = (
        rect(2., 2., 6., 6.),
        rect(0., 0., 5., 5.),
        rect(3., 3., 8., 8.),
    );
    let layers = [
        LayerComposition::default(),
        folder(&[2, 3]),
        LayerComposition::default(),
        blended("multiply"),
    ];
    let stack = GpuLayerStack::from_plan(
        &plan_composition(&layers),
        vec![
            painted(&c, GREEN),
            GpuLayerInput::default(),
            painted(&a, RED),
            painted(&b, BLUE),
        ],
    )
    .unwrap();
    let steps: Vec<_> = stack
        .steps()
        .iter()
        .map(|s| (s.layer, s.folder.clone()))
        .collect();
    assert_eq!(
        steps,
        vec![
            (0, None),
            (
                1,
                Some(vec![
                    GpuFolderChild {
                        layer: 2,
                        blend: 0,
                        seeds: true
                    },
                    GpuFolderChild {
                        layer: 3,
                        blend: 1,
                        seeds: false
                    },
                ])
            ),
        ],
        "the two children are consumed by the folder, only layer 0 and the folder paint"
    );
    assert_eq!(stack.layer_fill_count(), 3, "no child fill is lost");
}

#[test]
fn a_folder_flattens_its_children_in_isolation_and_lands_at_its_own_position() {
    // Bottom-up: C green x2..6 y2..6; the folder; its children A red x0..5 y0..5 and B blue x3..8 y3..8 (Multiply).
    let (c, a, b) = (
        rect(2., 2., 6., 6.),
        rect(0., 0., 5., 5.),
        rect(3., 3., 8., 8.),
    );
    let bytes = render(
        &[
            LayerComposition::default(),
            folder(&[2, 3]),
            LayerComposition::default(),
            blended("multiply"),
        ],
        vec![
            painted(&c, GREEN),
            GpuLayerInput::default(),
            painted(&a, RED),
            painted(&b, BLUE),
        ],
    );
    // Inside the folder A and B overlap: red x blue = black. The folder then covers C.
    assert_eq!(
        pixel(&bytes, 4, 4),
        [0, 0, 0, 255],
        "A x B inside the folder"
    );
    assert_eq!(pixel(&bytes, 1, 1), [255, 0, 0, 255], "A alone");
    assert_eq!(
        pixel(&bytes, 2, 2),
        [255, 0, 0, 255],
        "A over C: the folder is above C"
    );
    // B multiplies A only, never C: over the empty folder backdrop it is itself, and it then covers C.
    assert_eq!(pixel(&bytes, 5, 5), [0, 0, 255, 255], "B alone, over C");
    assert_eq!(
        pixel(&bytes, 7, 7),
        [0, 0, 255, 255],
        "B alone, over the base"
    );
    assert_eq!(
        pixel(&bytes, 5, 2),
        [0, 255, 0, 255],
        "C outside the folder's children"
    );
    assert_eq!(pixel(&bytes, 7, 0), [255; 4], "untouched base");
}

#[test]
fn a_folder_with_no_children_paints_nothing_and_the_first_child_seeds_whatever_its_blend() {
    let c = rect(2., 2., 6., 6.);
    let empty = render(
        &[LayerComposition::default(), folder(&[])],
        vec![painted(&c, GREEN), GpuLayerInput::default()],
    );
    assert_eq!(
        pixel(&empty, 3, 3),
        [0, 255, 0, 255],
        "an empty folder is transparent"
    );
    // The only child is Multiply: with nothing under it in the folder it stays itself.
    let a = rect(0., 0., 5., 5.);
    let seeded = render(
        &[folder(&[1]), blended("multiply")],
        vec![GpuLayerInput::default(), painted(&a, RED)],
    );
    assert_eq!(pixel(&seeded, 2, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&seeded, 6, 6), [255; 4]);
}

#[test]
fn a_child_effect_or_a_folder_effect_is_still_a_typed_refusal() {
    use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
        CompositionPlan, FolderChildStep, LayerStep, PaintSource, PaintStep,
    };
    let path = rect(0., 0., 4., 4.);
    let plan = |child_item: bool, child_layer: bool, own_effects: bool| {
        CompositionPlan::Layered {
        background: None,
        steps: vec![
            LayerStep::Paint(PaintStep {
                index: 0,
                source: PaintSource::Folder(vec![FolderChildStep {
                    index: 1,
                    item_effects: child_item,
                    layer_effects: child_layer,
                    blend: 0,
                    seeds_accumulator: true,
                }]),
                mask_count: 0,
                mattes: Vec::new(),
                layer_effects: own_effects,
                blend: 0,
            }),
            LayerStep::Skipped {
                index: 1,
                reason: crate::object_render_scene::scene_effect_kernels::scene_effect_order::SkipReason::FolderChild,
            },
        ],
    }
    };
    let inputs = || vec![GpuLayerInput::default(), painted(&path, RED)];
    for (item, layer, own, expected) in [
        (
            true,
            false,
            false,
            GpuUnavailable::ElementEffects { layer: 1 },
        ),
        (
            false,
            true,
            false,
            GpuUnavailable::LayerEffects { layer: 1 },
        ),
        (
            false,
            false,
            true,
            GpuUnavailable::LayerEffects { layer: 0 },
        ),
    ] {
        assert_eq!(
            GpuLayerStack::from_plan(&plan(item, layer, own), inputs()).unwrap_err(),
            GpuBlendError::Unavailable(expected)
        );
    }
    assert!(GpuLayerStack::from_plan(&plan(false, false, false), inputs()).is_ok());
    let stray = CompositionPlan::Layered {
        background: None,
        steps: vec![LayerStep::Paint(PaintStep {
            index: 0,
            source: PaintSource::Folder(vec![FolderChildStep {
                index: 9,
                item_effects: false,
                layer_effects: false,
                blend: 0,
                seeds_accumulator: true,
            }]),
            mask_count: 0,
            mattes: Vec::new(),
            layer_effects: false,
            blend: 0,
        })],
    };
    assert_eq!(
        GpuLayerStack::from_plan(&stray, vec![GpuLayerInput::default()]).unwrap_err(),
        GpuBlendError::IndexOutOfRange { index: 9 }
    );
}
