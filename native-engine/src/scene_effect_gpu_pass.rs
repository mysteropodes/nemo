//! GPU executor of the L08-C effect-stack plan: colorAdjust, vignette, blur
//! (including the unknown-type fallback) and glow (L08-H).
//!
//! The planner (`scene_effect_params`) already decided which entries run, in
//! what order, with which defaults and zoom scaling, and where each result
//! lands. This module only executes that plan on the existing native
//! compositor, following the baseline `run_one_effect` /
//! `apply_effect_stack` / `apply_effect_stack_into`
//! (`geometry-wasm/src/engine.rs`) pass for pass:
//! - colorAdjust and vignette are one fullscreen pass each, blur is the
//!   separable pair (horizontal into the scratch, vertical into the target);
//! - glow blurs a copy of its source into the shared blur result and composites
//!   that result over the UNBLURRED source in Screen mode with the blend
//!   pipeline of the layered path (`scene_effect_gpu_blend`), never a new one;
//! - entries ping-pong between two private textures, the caller's source is
//!   never written, and the `_into` variant writes the last entry straight into
//!   the caller's target (the adjustment-layer shape).
//! Every private texture is an `ObjectTarget` of the charged allocator, reserved
//! before it exists and retired behind the queue's completion fence on success,
//! refusal, cancel, stale and unwind. A `live` check runs before any texture is
//! allocated and after every pass; its refusal frees everything and publishes
//! nothing. `custom:` effects, and the simple_fx family and hqBloom of a stack
//! admitted without an `FxFrame`, are typed refusals at admission, never
//! silently dropped; with a frame (`GpuEffectStack::from_plan_with_frame`,
//! `scene_effect_gpu_fx`, L08-I) a `simple_fx` entry is one pass of the retained
//! `simple_fx.wgsl` and hqBloom is extract (id 14) -> blur -> Screen composite.
//! No renderer, scheduler or allocator is added.

#[path = "scene_effect_gpu_pass_pipes.rs"]
mod pipes;
#[path = "scene_effect_gpu_pass_pool.rs"]
mod pool;
#[path = "scene_effect_gpu_pass_stack.rs"]
mod stack;

#[allow(unused_imports)]
pub(crate) use crate::compositor::scene_effect_gpu_pass::pipes::EffectKernels;
#[allow(unused_imports)]
pub(crate) use crate::compositor::scene_effect_gpu_pass::stack::{
    GpuEffectError, GpuEffectNeeds, GpuEffectStack, GpuEffectUnavailable,
};

use crate::compositor::object_target::{reserve_target_bytes, ObjectTarget, TargetReservation};
use crate::compositor::scene_effect_gpu_blend::BlendKernels;
use crate::compositor::scene_effect_gpu_fx::fx_payload;
use crate::compositor::scene_effect_gpu_pass::pool::{Pool, Role};
use crate::compositor::{Compositor, CompositorError};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    EffectPass, Landing, PassSource, PassTarget,
};
use std::sync::Arc;

/// The stack result: pixels owned by the charged target allocator. Dropping it
/// retires the texture behind the queue's completion fence and returns the
/// payload quota.
pub(crate) struct EffectedPixels {
    target: ObjectTarget,
    width: u32,
    height: u32,
    passes: u32,
}

impl EffectedPixels {
    pub(crate) fn texture(&self) -> &wgpu::Texture {
        self.target.texture()
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        self.target.view()
    }

    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Shader dispatches that actually ran.
    pub(crate) fn passes(&self) -> u32 {
        self.passes
    }
}

/// What the final-target variant reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EffectRun {
    pub(crate) passes: u32,
}

fn lost_landing() -> CompositorError {
    CompositorError::new("effect stack lost its landing texture")
}

impl Compositor {
    /// Run a `plan_effect_stack` plan on `source` and return the landing
    /// texture. `size` is the size of `source`; `live` is the liveness check.
    pub(super) fn apply_effect_stack(
        &mut self,
        stack: &GpuEffectStack<'_>,
        source: &wgpu::TextureView,
        size: (u32, u32),
        live: &dyn Fn() -> Result<(), CompositorError>,
    ) -> Result<EffectedPixels, CompositorError> {
        self.apply_effect_stack_with(stack, source, size, &reserve_target_bytes, live)
    }

    /// Same path with an injected payload reservation (tests use an isolated
    /// budget; production passes the shared image-resource budget).
    pub(super) fn apply_effect_stack_with(
        &mut self,
        stack: &GpuEffectStack<'_>,
        source: &wgpu::TextureView,
        size: (u32, u32),
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
        live: &dyn Fn() -> Result<(), CompositorError>,
    ) -> Result<EffectedPixels, CompositorError> {
        if stack.into_final() {
            return Err(CompositorError::new(
                "effect stack writes the caller's target; use the final-target entry",
            ));
        }
        let (mut pool, passes) = self.run_effect_steps(stack, source, None, size, reserve, live)?;
        let role = match stack.landing() {
            Landing::StackA => Role::StackA,
            Landing::StackB => Role::StackB,
            _ => return Err(lost_landing()),
        };
        let target = pool.take(role).ok_or_else(lost_landing)?;
        Ok(EffectedPixels {
            target,
            width: size.0,
            height: size.1,
            passes,
        })
    }

    /// Run a `plan_effect_stack_into` plan: intermediates stay private, the
    /// last entry writes `final_target`, which must differ from `source` and
    /// carry `RENDER_ATTACHMENT`. A refusal means the caller must not publish
    /// `final_target`.
    pub(super) fn apply_effect_stack_into(
        &mut self,
        stack: &GpuEffectStack<'_>,
        source: &wgpu::TextureView,
        final_target: &wgpu::TextureView,
        size: (u32, u32),
        live: &dyn Fn() -> Result<(), CompositorError>,
    ) -> Result<EffectRun, CompositorError> {
        self.apply_effect_stack_into_with(
            stack,
            source,
            final_target,
            size,
            &reserve_target_bytes,
            live,
        )
    }

    pub(super) fn apply_effect_stack_into_with(
        &mut self,
        stack: &GpuEffectStack<'_>,
        source: &wgpu::TextureView,
        final_target: &wgpu::TextureView,
        size: (u32, u32),
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
        live: &dyn Fn() -> Result<(), CompositorError>,
    ) -> Result<EffectRun, CompositorError> {
        if !stack.into_final() {
            return Err(CompositorError::new(
                "effect stack lands in a private texture; use the landing entry",
            ));
        }
        let (pool, passes) =
            self.run_effect_steps(stack, source, Some(final_target), size, reserve, live)?;
        drop(pool);
        Ok(EffectRun { passes })
    }

    /// The shared Gaussian/grade pipelines. `pub(super)` so a later consumer in
    /// the compositor family (e.g. layer mask feather) reuses this blur instead
    /// of building a second one.
    pub(super) fn effect_pipelines(&mut self) -> Arc<EffectKernels> {
        let (device, queue) = (self.device.clone(), self.queue.clone());
        Arc::clone(
            self.effect_kernels
                .get_or_insert_with(|| Arc::new(EffectKernels::new(&device, &queue))),
        )
    }

    /// Glow's Screen composite reuses the blend pipeline of the layered path.
    fn glow_blend_kernels(&mut self) -> Arc<BlendKernels> {
        let (device, queue) = (self.device.clone(), self.queue.clone());
        Arc::clone(
            self.blend_kernels
                .get_or_insert_with(|| Arc::new(BlendKernels::new(&device, &queue))),
        )
    }

    /// Execute every step in plan order. Returns the pool (which still owns the
    /// landing texture) and the dispatch count. An error drops the pool.
    fn run_effect_steps(
        &mut self,
        stack: &GpuEffectStack<'_>,
        source: &wgpu::TextureView,
        final_target: Option<&wgpu::TextureView>,
        size: (u32, u32),
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
        live: &dyn Fn() -> Result<(), CompositorError>,
    ) -> Result<(Pool, u32), CompositorError> {
        // Stale or cancelled work is refused before any texture exists.
        live()?;
        let needs = stack.needs();
        let fx_needs = stack.fx_needs();
        let frame = stack.frame();
        let kernels = self.effect_pipelines();
        let blend = if needs.glow_blend {
            Some(self.glow_blend_kernels())
        } else {
            None
        };
        let mut pool = Pool::new(size.0, size.1);
        for (wanted, role) in [
            (needs.stack_a, Role::StackA),
            (needs.stack_b, Role::StackB),
            (needs.blur_scratch, Role::BlurScratch),
            (needs.blur_result, Role::BlurResult),
            (fx_needs.bloom_extract, Role::BloomExtract),
        ] {
            if wanted {
                pool.ensure(self, role, reserve)?;
            }
        }
        let mut passes = 0u32;
        for step in stack.steps() {
            {
                let from = match step.source {
                    PassSource::Initial => source,
                    PassSource::StackA => pool.view(Role::StackA),
                    PassSource::StackB => pool.view(Role::StackB),
                };
                let to = match step.target {
                    PassTarget::StackA => pool.view(Role::StackA),
                    PassTarget::StackB => pool.view(Role::StackB),
                    PassTarget::Final => final_target
                        .ok_or_else(|| CompositorError::new("effect stack has no final target"))?,
                };
                match &step.pass {
                    EffectPass::ColorAdjust { p1, p2 } => {
                        kernels.color_adjust(from, *p1, *p2, to);
                        passes += 1;
                    }
                    EffectPass::Vignette { strength, radius } => {
                        kernels.vignette(from, *strength, *radius, to);
                        passes += 1;
                    }
                    EffectPass::Blur { radius_px } => {
                        kernels.blur(from, *radius_px, size, pool.view(Role::BlurScratch), to);
                        passes += 2;
                    }
                    EffectPass::Glow {
                        radius_px,
                        composite_mode,
                    } => {
                        let blurred = pool.view(Role::BlurResult);
                        kernels.blur(
                            from,
                            *radius_px,
                            size,
                            pool.view(Role::BlurScratch),
                            blurred,
                        );
                        blend
                            .as_ref()
                            .ok_or_else(|| CompositorError::new("glow has no blend pipeline"))?
                            .blend(from, blurred, *composite_mode, to);
                        passes += 3;
                    }
                    EffectPass::SimpleFx(params) => {
                        let frame = frame
                            .ok_or_else(|| CompositorError::new("simple_fx effect has no frame"))?;
                        kernels.simple_fx(from, &fx_payload(params, size, &frame), to);
                        passes += 1;
                    }
                    EffectPass::HqBloom {
                        extract,
                        radius_px,
                        composite_mode,
                    } => {
                        let frame =
                            frame.ok_or_else(|| CompositorError::new("hqBloom has no frame"))?;
                        let bloom = pool.view(Role::BloomExtract);
                        let blurred = pool.view(Role::BlurResult);
                        kernels.simple_fx(from, &fx_payload(extract, size, &frame), bloom);
                        kernels.blur(
                            bloom,
                            *radius_px,
                            size,
                            pool.view(Role::BlurScratch),
                            blurred,
                        );
                        blend
                            .as_ref()
                            .ok_or_else(|| CompositorError::new("hqBloom has no blend pipeline"))?
                            .blend(from, blurred, *composite_mode, to);
                        passes += 4;
                    }
                    _ => {
                        return Err(CompositorError::new(
                            "effect family is not rendered by this pass",
                        ))
                    }
                }
            }
            // A newer frame, a cancel or a replaced document is observed after
            // every submitted pass; the pool drops and nothing is published.
            live()?;
        }
        Ok((pool, passes))
    }
}

#[cfg(test)]
#[path = "../tests/scene_effect_gpu_pass_cases.rs"]
mod cases;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_pass_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_pass_lifetime.rs"]
mod lifetime;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_pass_reference.rs"]
mod reference;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_pass_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_pass.rs"]
mod tests;
