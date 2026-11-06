//! Effect passes of the layered path (#2257): a layer's, folder child's or adjustment layer's planned effect stack and
//! the mask silhouette feather, executed by the L08-H/I effect executor on the textures the layered pool already owns.
//!
//! Baseline `composite_scene` order per ordinary layer: paint, masks (the silhouette is blurred by the feather when it
//! is above 0.01), mattes, the layer's own effect stack on that isolated alpha, then the blend into the accumulator. An
//! adjustment layer grades the running accumulator (`apply_effect_stack_into`) on the document box, not on any item box.
use crate::compositor::scene_effect_gpu_fx::FxFrame;
use crate::compositor::scene_effect_gpu_pass::{EffectedPixels, GpuEffectStack};
use crate::compositor::{Compositor, CompositorError};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::EffectStackPlan;

type Bbox = (f32, f32, f32, f32);

fn admitted(
    plan: &EffectStackPlan,
    bbox: Bbox,
    time: f32,
) -> Result<GpuEffectStack<'_>, CompositorError> {
    let frame =
        FxFrame::new(bbox, time).map_err(|error| CompositorError::new(error.to_string()))?;
    GpuEffectStack::from_plan_with_frame(plan, frame)
        .map_err(|error| CompositorError::new(format!("layer effect stack unavailable: {error}")))
}

impl Compositor {
    /// Admit `plan` against `bbox` BEFORE any texture of the composition exists, so an unavailable effect (a custom
    /// shader) refuses the whole frame typed, never a partial one.
    pub(super) fn admit_layer_effects(
        plan: &EffectStackPlan,
        bbox: Bbox,
        time: f32,
    ) -> Result<(), CompositorError> {
        admitted(plan, bbox, time).map(|_| ())
    }

    /// The layer's own stack on its isolated result; the landing texture is the new source of the blend.
    pub(super) fn apply_layer_effects(
        &mut self,
        plan: &EffectStackPlan,
        (bbox, time): (Bbox, f32),
        source: &wgpu::TextureView,
        size: (u32, u32),
    ) -> Result<EffectedPixels, CompositorError> {
        let stack = admitted(plan, bbox, time)?;
        self.apply_effect_stack(&stack, source, size, &|| Ok(()))
    }

    /// An adjustment layer: grade `backdrop` (the accumulator) straight into `target` (the other accumulator).
    pub(super) fn grade_backdrop(
        &mut self,
        plan: &EffectStackPlan,
        (bbox, time): (Bbox, f32),
        backdrop: &wgpu::TextureView,
        target: &wgpu::TextureView,
        size: (u32, u32),
    ) -> Result<(), CompositorError> {
        let stack = admitted(plan, bbox, time)?;
        self.apply_effect_stack_into(&stack, backdrop, target, size, &|| Ok(()))
            .map(|_| ())
    }
}
