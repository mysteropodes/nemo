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
//! effects, folders and mask feather are typed `Unavailable` refusals (see
//! the stack module), never silently dropped.

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
    GpuLayerStack, GpuMask, GpuMaskMode, GpuStep, GpuUnavailable,
};

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
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            // The solid renders below replace Vello's image atlas; the next
            // reference-image composition must restore it.
            self.last_reference_image = None;
        }
        let kernels = self.layered_kernels();
        let mut pool = Pool::new(size.0, size.1);
        pool.ensure(self, Role::AccumA, reserve)?;
        pool.ensure(self, Role::AccumB, reserve)?;
        kernels.clear(pool.view(Role::AccumA), wgpu::Color::TRANSPARENT);
        if let Some(index) = stack.background() {
            pool.ensure(self, Role::Background, reserve)?;
            let paint = &stack.inputs()[index].paint;
            self.render_paints(&[paint], pool.view(Role::Background), size)?;
        }
        let (mut current, mut spare) = (Role::AccumA, Role::AccumB);
        for step in stack.steps() {
            let input = &stack.inputs()[step.layer];
            pool.ensure(self, Role::Layer, reserve)?;
            self.render_paints(&[&input.paint], pool.view(Role::Layer), size)?;
            let mut source = Role::Layer;
            if !input.masks.is_empty() {
                self.apply_masks(&kernels, &mut pool, input, reserve)?;
                source = Role::Masked;
            }
            // Each matte narrows what the previous one left (ping-pong A/B).
            let mut into_b = false;
            for matte in &step.mattes {
                pool.ensure(self, Role::Aux, reserve)?;
                let paint = &stack.inputs()[matte.source].paint;
                self.render_paints(&[paint], pool.view(Role::Aux), size)?;
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
            kernels.blend(
                pool.view(current),
                pool.view(source),
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
        for role in [
            Role::Layer,
            Role::Aux,
            Role::Masked,
            Role::MaskA,
            Role::MaskB,
            Role::MatteA,
            Role::MatteB,
            Role::Background,
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
    fn render_paints(
        &mut self,
        paints: &[&GpuLayerPaint<'_>],
        view: &wgpu::TextureView,
        (width, height): (u32, u32),
    ) -> Result<(), CompositorError> {
        let swatches = encode_straight_paints(&mut self.scene, paints);
        let rendered = self.render_target(view, width, height, Color::TRANSPARENT);
        // Release our own references on both success and Vello error. The existing
        // renderer atlas can retain a swatch until pressure eviction or renderer drop.
        self.scene.reset();
        drop(swatches);
        rendered
    }

    /// Baseline mask combine: Add masks union (one Scene), none -> fully
    /// visible, Subtract/Intersect narrow one at a time through the alpha
    /// matte math, then the silhouette multiplies the layer into `Masked`.
    /// Feather above the baseline threshold was refused when the stack was
    /// built, so the silhouette never needs the blur pass here.
    fn apply_masks(
        &mut self,
        kernels: &BlendKernels,
        pool: &mut Pool,
        input: &GpuLayerInput<'_>,
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
    ) -> Result<(), CompositorError> {
        let size = pool.size();
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
            self.render_paints(&adds, pool.view(Role::Aux), size)?;
            kernels.copy(pool.texture(Role::Aux), pool.texture(Role::MaskA), size);
        }
        let (mut current, mut spare) = (Role::MaskA, Role::MaskB);
        for mask in input
            .masks
            .iter()
            .filter(|mask| mask.mode != GpuMaskMode::Add)
        {
            self.render_paints(&[&mask.paint], pool.view(Role::Aux), size)?;
            kernels.matte(
                pool.view(current),
                pool.view(Role::Aux),
                (0, mask.mode == GpuMaskMode::Subtract),
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
#[cfg(all(test, feature = "history"))]
#[path = "../tests/scene_effect_gpu_blend_owner.rs"]
mod owner;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_blend_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_blend.rs"]
mod tests;
