//! Layered GPU composition: per-layer isolation, track mattes, vector masks and
//! the 16 blend modes on the existing native compositor (L08-G).
//!
//! The baseline `composite_scene` (`geometry-wasm/src/engine.rs`) cannot rely
//! on Vello's `push_layer(Mix)` (its blend defect is documented at
//! `mix_mode_index`), so every layer is rendered into its own transparent
//! texture and folded into a ping-pong accumulator by the baseline
//! `blend.wgsl` / `matte.wgsl` passes. This module reproduces exactly that
//! statement order for plain layers: transparent accumulator, canvas
//! background in its own texture and re-composited UNDER the accumulator at
//! the end, base colour flattened last. Vello stays the only rasteriser and
//! `Compositor::scene` the only Scene; there is no second scheduler.
//!
//! The flat fast path is untouched: `GpuLayerStack::from_plan` refuses a flat
//! plan, so a scene that needs none of these features is still painted by
//! `Compositor::compose_object` pixel for pixel. Effect stacks, element
//! effects and mask feather are typed `Unavailable` refusals (see the stack
//! module), never silently dropped. A folder is rendered like the baseline's
//! `paint_folder_children`: its children are flattened in isolation (first
//! child seeds a transparent accumulator, each later child blends in with its
//! own blend mode) and that result is the folder's own paint.

#[path = "scene_effect_gpu_blend_fx.rs"]
mod fx;
#[path = "scene_effect_gpu_blend_pipes.rs"]
mod pipes;
#[path = "scene_effect_gpu_blend_pool.rs"]
mod pool;
#[path = "scene_effect_gpu_blend_stack.rs"]
mod stack;

#[allow(unused_imports)]
pub(crate) use crate::compositor::scene_effect_gpu_blend::pipes::BlendKernels;
#[allow(unused_imports)]
pub(crate) use crate::compositor::scene_effect_gpu_blend::stack::{
    layer_runs_from_plan, GpuBlendError, GpuFill, GpuLayerInput, GpuLayerPaint, GpuLayerRun,
    GpuFolderChild, GpuLayerStack, GpuMask, GpuMaskMode, GpuStep, GpuUnavailable,
};

use crate::compositor::object_compositor::layered::{CameraWrap, LayerContext};
use crate::compositor::scene_effect_gpu_blend::stack::FEATHER_PASS_THRESHOLD;
use crate::compositor::object_target::{reserve_target_bytes, ObjectTarget, TargetReservation};
use crate::compositor::scene_effect_gpu_blend::pool::{Pool, Role};
use crate::compositor::{Compositor, CompositorError};
use std::collections::BTreeMap;
use std::sync::Arc;
use vello::kurbo::Affine;
use vello::peniko::{
    Blob, Color, Extend, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality,
};

/// Encode original fill order in ONE Vello scene using straight RGB swatches.
/// Vello's solid brush quantises premultiplied RGB before coverage is applied.
/// Its image brush instead loads straight RGB8 and premultiplies in the fine
/// shader. Coverage and every overlapping fill accumulate in f32 until the
/// single final isolation write. No RGBA8 coverage or per-fill output is used.
///
/// Image alpha multiplication is encoded as u8 by Vello 0.9: this deliberately
/// preserves the original solid-brush alpha8, rather than promising float alpha.
/// One four-byte opaque swatch is shared for each exact RGB8 within this scene.
/// Vello may retain those blobs in its EXISTING pressure-evicted image atlas;
/// dropping this map/scene is not a promise of immediate renderer-cache eviction.
fn encode_straight_paints(
    scene: &mut vello::Scene,
    paints: &[&GpuLayerPaint<'_>],
) -> BTreeMap<[u8; 3], ImageData> {
    scene.reset();
    let mut swatches = BTreeMap::new();
    for paint in paints {
        for fill in &paint.fills {
            if let Some(prepared) = &fill.prepared {
                prepared.draw(scene, Affine::IDENTITY, fill.path);
                continue;
            }
            if (fill.rgba[3] * 255.0).round() == 0.0 {
                continue;
            }
            let rgb = [fill.rgba[0], fill.rgba[1], fill.rgba[2]]
                .map(|channel| (channel * 255.0).round() as u8);
            let image = swatches.entry(rgb).or_insert_with(|| ImageData {
                data: Blob::new(Arc::new([rgb[0], rgb[1], rgb[2], 255])),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: 1,
                height: 1,
            });
            let brush = ImageBrush::new(image.clone())
                .with_extend(Extend::Pad)
                .with_quality(ImageQuality::Low)
                .multiply_alpha(fill.rgba[3]);
            scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, fill.path);
        }
    }
    swatches
}

/// Composited pixels owned by the charged target allocator. Dropping it (or a
/// failed composition) retires every texture behind the queue's completion
/// fence and returns the payload quota.
pub(crate) struct LayeredPixels {
    target: ObjectTarget,
    width: u32,
    height: u32,
}

impl LayeredPixels {
    pub(crate) fn texture(&self) -> &wgpu::Texture {
        self.target.texture()
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        self.target.view()
    }

    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(super) fn into_target(self) -> ObjectTarget {
        self.target
    }
}

fn lost_accumulator() -> CompositorError {
    CompositorError::new("layered composition lost its accumulator")
}

impl Compositor {
    /// Compose `stack` into one charged RGBA8 target. `base_rgba8` is the
    /// final backdrop (white compatibility output or the project background).
    pub(super) fn compose_layered_stack(
        &mut self,
        stack: &GpuLayerStack<'_>,
        size: (u32, u32),
        base_rgba8: [u8; 4],
    ) -> Result<LayeredPixels, CompositorError> {
        self.compose_layered_stack_with(stack, size, base_rgba8, &reserve_target_bytes)
    }

    /// Same path with an injected payload reservation (tests use an isolated
    /// budget; production passes the shared image-resource budget).
    pub(super) fn compose_layered_stack_with(
        &mut self,
        stack: &GpuLayerStack<'_>,
        size: (u32, u32),
        base_rgba8: [u8; 4],
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
    ) -> Result<LayeredPixels, CompositorError> {
        self.compose_layered_stack_in(None, stack, size, base_rgba8, reserve)
    }

    /// The layered path of a witness: `ctx` carries its paint entries (every family), stored camera and the content
    /// beneath the artwork; without one only the stack's contour fills exist.
    pub(super) fn compose_layered_stack_in(
        &mut self,
        ctx: Option<&LayerContext<'_>>,
        stack: &GpuLayerStack<'_>,
        size: (u32, u32),
        base_rgba8: [u8; 4],
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
    ) -> Result<LayeredPixels, CompositorError> {
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            // The solid renders below replace Vello's image atlas; the next
            // reference-image composition must restore it.
            self.last_reference_image = None;
        }
        // Every effect stack is admitted against its layer's box before any texture exists (a custom shader refuses the
        // whole frame typed). A folder and an adjustment layer have no items of their own: the target / document box.
        let time = ctx.map_or(0., LayerContext::time);
        let canvas_box = (0., 0., size.0 as f32, size.1 as f32);
        let layer_box = |wire: usize| {
            ctx.map_or(canvas_box, |c| c.layer_bbox(stack.inputs()[wire].source_layer, size))
        };
        let document_box = ctx.map_or(canvas_box, |c| c.document_bbox(size));
        for step in stack.steps().iter().filter(|step| step.effects) {
            let plan = stack.inputs()[step.layer].effects.expect("admitted by the stack");
            let bbox = match (step.effect_layer, step.folder.is_some()) {
                (true, _) => document_box,
                (_, true) => canvas_box,
                _ => layer_box(step.layer),
            };
            Self::admit_layer_effects(plan, bbox, time)?;
            for child in step.folder.iter().flatten().filter(|child| child.effects) {
                let plan = stack.inputs()[child.layer].effects.expect("admitted by the stack");
                Self::admit_layer_effects(plan, layer_box(child.layer), time)?;
            }
        }
        let kernels = self.layered_kernels();
        let mut pool = Pool::new(size.0, size.1);
        pool.ensure(self, Role::AccumA, reserve)?;
        pool.ensure(self, Role::AccumB, reserve)?;
        kernels.clear(pool.view(Role::AccumA), wgpu::Color::TRANSPARENT);
        let under = ctx.is_some_and(LayerContext::has_under);
        if let (true, Some(ctx)) = (under, ctx) {
            pool.ensure(self, Role::Under, reserve)?;
            self.render_under(ctx, pool.view(Role::Under), size)?;
        }
        if let Some(index) = stack.background() {
            pool.ensure(self, Role::Background, reserve)?;
            self.render_wire_layer(ctx, stack, index, pool.view(Role::Background), size)?;
        }
        let (mut current, mut spare) = (Role::AccumA, Role::AccumB);
        for step in stack.steps() {
            let input = &stack.inputs()[step.layer];
            if step.effect_layer {
                // Adjustment layer: no paint, the stack grades everything accumulated below it.
                let plan = input.effects.expect("admitted by the stack");
                self.grade_backdrop(
                    plan,
                    (document_box, time),
                    pool.view(current),
                    pool.view(spare),
                    size,
                )?;
                std::mem::swap(&mut current, &mut spare);
                continue;
            }
            pool.ensure(self, Role::Layer, reserve)?;
            match &step.folder {
                Some(children) => {
                    let boxes = (&layer_box as &dyn Fn(usize) -> _, time);
                    self.render_folder(&kernels, &mut pool, (ctx, stack), boxes, children, reserve)?
                }
                None => self.render_wire_layer(ctx, stack, step.layer, pool.view(Role::Layer), size)?,
            }
            let mut source = Role::Layer;
            if !input.masks.is_empty() {
                self.apply_masks(&kernels, &mut pool, ctx, input, reserve)?;
                source = Role::Masked;
            }
            // Each matte narrows what the previous one left (ping-pong A/B).
            let mut into_b = false;
            for matte in &step.mattes {
                pool.ensure(self, Role::Aux, reserve)?;
                self.render_wire_layer(ctx, stack, matte.source, pool.view(Role::Aux), size)?;
                let destination = if into_b { Role::MatteB } else { Role::MatteA };
                pool.ensure(self, destination, reserve)?;
                kernels.matte(
                    pool.view(source),
                    pool.view(Role::Aux),
                    (matte.mode, matte.invert),
                    pool.view(destination),
                );
                source = destination;
                into_b = !into_b;
            }
            // The layer's own stack runs on its masked, matted alpha, so a matted edge softens too, before the blend.
            let graded = match input.effects.filter(|_| step.effects) {
                Some(plan) => {
                    let bbox = if step.folder.is_some() { canvas_box } else { layer_box(step.layer) };
                    Some(self.apply_layer_effects(plan, (bbox, time), pool.view(source), size)?)
                }
                None => None,
            };
            kernels.blend(
                pool.view(current),
                graded.as_ref().map_or_else(|| pool.view(source), |g| g.view()),
                step.blend,
                pool.view(spare),
            );
            std::mem::swap(&mut current, &mut spare);
        }
        if stack.background().is_some() {
            // The background is the BACKDROP of the finished accumulator, so
            // no layer (and no future adjustment pass) ever saw it.
            kernels.blend(
                pool.view(Role::Background),
                pool.view(current),
                0,
                pool.view(spare),
            );
            std::mem::swap(&mut current, &mut spare);
        }
        if under {
            // Beneath the canvas background and every layer, exactly where the flat sequence paints it.
            kernels.blend(pool.view(Role::Under), pool.view(current), 0, pool.view(spare));
            std::mem::swap(&mut current, &mut spare);
        }
        for role in [
            Role::Layer,
            Role::Aux,
            Role::Masked,
            Role::MaskA,
            Role::MaskB,
            Role::MatteA,
            Role::MatteB,
            Role::Background,
            Role::FolderA,
            Role::FolderB,
            Role::Under,
        ] {
            pool.release(role);
        }
        pool.ensure(self, Role::Flat, reserve)?;
        let [red, green, blue, alpha] = base_rgba8.map(|channel| f64::from(channel) / 255.0);
        kernels.clear(
            pool.view(Role::Flat),
            wgpu::Color {
                r: red,
                g: green,
                b: blue,
                a: alpha,
            },
        );
        kernels.blend(
            pool.view(Role::Flat),
            pool.view(current),
            0,
            pool.view(spare),
        );
        std::mem::swap(&mut current, &mut spare);
        self.scene.reset();
        let target = pool.take(current).ok_or_else(lost_accumulator)?;
        Ok(LayeredPixels {
            target,
            width: size.0,
            height: size.1,
        })
    }

    fn layered_kernels(&mut self) -> Arc<BlendKernels> {
        let (device, queue) = (self.device.clone(), self.queue.clone());
        Arc::clone(
            self.blend_kernels
                .get_or_insert_with(|| Arc::new(BlendKernels::new(&device, &queue))),
        )
    }

    /// Keep the whole isolated layer in Vello's Area-AA/f32 accumulation path.
    pub(super) fn render_paints(
        &mut self,
        paints: &[&GpuLayerPaint<'_>],
        camera: Option<CameraWrap>,
        view: &wgpu::TextureView,
        (width, height): (u32, u32),
    ) -> Result<(), CompositorError> {
        let swatches = match camera {
            Some(camera) => {
                let mut artwork = vello::Scene::new();
                let swatches = encode_straight_paints(&mut artwork, paints);
                self.scene.reset();
                self.append_camera(&artwork, camera);
                swatches
            }
            None => encode_straight_paints(&mut self.scene, paints),
        };
        let rendered = self.render_target(view, width, height, Color::TRANSPARENT);
        // Release our own references on both success and Vello error. The existing
        // renderer atlas can retain a swatch until pressure eviction or renderer drop.
        self.scene.reset();
        drop(swatches);
        rendered
    }

    /// Baseline `paint_folder_children`: flatten the folder's children, bottom-up, into a fresh transparent
    /// accumulator pair and leave the result in `Role::Layer`, where an ordinary layer's own paint would be.
    /// Each child's own masks and mattes are not honoured inside the folder (the baseline ignores them too).
    fn render_folder(
        &mut self,
        kernels: &BlendKernels,
        pool: &mut Pool,
        (ctx, stack): (Option<&LayerContext<'_>>, &GpuLayerStack<'_>),
        (layer_box, time): (&dyn Fn(usize) -> (f32, f32, f32, f32), f32),
        children: &[GpuFolderChild],
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
    ) -> Result<(), CompositorError> {
        let size = pool.size();
        for role in [Role::FolderA, Role::FolderB, Role::Aux] {
            pool.ensure(self, role, reserve)?;
        }
        kernels.clear(pool.view(Role::FolderA), wgpu::Color::TRANSPARENT);
        let (mut current, mut spare) = (Role::FolderA, Role::FolderB);
        for child in children {
            self.render_wire_layer(ctx, stack, child.layer, pool.view(Role::Aux), size)?;
            // A child's own effect stack runs on its isolated paint before it folds into the folder (baseline
            // `paint_folder_children`), with its own blend.
            let graded = match stack.inputs()[child.layer].effects.filter(|_| child.effects) {
                Some(plan) => Some(self.apply_layer_effects(
                    plan,
                    (layer_box(child.layer), time),
                    pool.view(Role::Aux),
                    size,
                )?),
                None => None,
            };
            let (view, texture) = match &graded {
                Some(graded) => (graded.view(), graded.texture()),
                None => (pool.view(Role::Aux), pool.texture(Role::Aux)),
            };
            if child.seeds {
                kernels.copy(texture, pool.texture(current), size);
            } else {
                kernels.blend(pool.view(current), view, child.blend, pool.view(spare));
                std::mem::swap(&mut current, &mut spare);
            }
        }
        kernels.copy(pool.texture(current), pool.texture(Role::Layer), size);
        Ok(())
    }

    /// Baseline mask combine: Add masks union (one Scene), none -> fully
    /// visible, Subtract/Intersect narrow one at a time through the alpha
    /// matte math, then the silhouette multiplies the layer into `Masked`.
    /// Feather above the baseline threshold blurs the silhouette first.
    fn apply_masks(
        &mut self,
        kernels: &BlendKernels,
        pool: &mut Pool,
        ctx: Option<&LayerContext<'_>>,
        input: &GpuLayerInput<'_>,
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
    ) -> Result<(), CompositorError> {
        let size = pool.size();
        let camera = ctx.and_then(LayerContext::camera);
        for role in [Role::Aux, Role::MaskA, Role::MaskB, Role::Masked] {
            pool.ensure(self, role, reserve)?;
        }
        let adds: Vec<&GpuLayerPaint<'_>> = input
            .masks
            .iter()
            .filter(|mask| mask.mode == GpuMaskMode::Add)
            .map(|mask| &mask.paint)
            .collect();
        if adds.is_empty() {
            kernels.clear(pool.view(Role::MaskA), wgpu::Color::WHITE);
        } else {
            self.render_paints(&adds, camera, pool.view(Role::Aux), size)?;
            kernels.copy(pool.texture(Role::Aux), pool.texture(Role::MaskA), size);
        }
        let (mut current, mut spare) = (Role::MaskA, Role::MaskB);
        for mask in input
            .masks
            .iter()
            .filter(|mask| mask.mode != GpuMaskMode::Add)
        {
            self.render_paints(&[&mask.paint], camera, pool.view(Role::Aux), size)?;
            kernels.matte(
                pool.view(current),
                pool.view(Role::Aux),
                (0, mask.mode == GpuMaskMode::Subtract),
                pool.view(spare),
            );
            std::mem::swap(&mut current, &mut spare);
        }
        if input.mask_feather > FEATHER_PASS_THRESHOLD {
            // Baseline feather: the shared separable Gaussian softens the FINAL combined silhouette's edge, not the
            // masked content (one feather for all masks of the layer).
            let blur = self.effect_pipelines();
            blur.blur(
                pool.view(current),
                input.mask_feather,
                size,
                pool.view(Role::Aux),
                pool.view(spare),
            );
            std::mem::swap(&mut current, &mut spare);
        }
        kernels.matte(
            pool.view(Role::Layer),
            pool.view(current),
            (0, false),
            pool.view(Role::Masked),
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/scene_effect_gpu_blend_cases.rs"]
mod cases;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_blend_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_blend_folder.rs"]
mod folder;
#[cfg(all(test, feature = "history"))]
#[path = "../tests/scene_effect_gpu_blend_owner.rs"]
mod owner;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_blend_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_blend.rs"]
mod tests;
