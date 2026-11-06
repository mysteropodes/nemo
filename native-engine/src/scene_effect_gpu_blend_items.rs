//! Item-level effect passes of the layered path (#2257): the baseline `paint_layer_with_element_effects`.
//!
//! A layer whose objects carry enabled effect stacks is painted in RUNS: consecutive plain items form one run, an
//! effected item is its own run. Each run is rendered alone into a transparent texture; an effected run is then graded
//! by its own stack on ITS box (`items_bbox_px` of that one item under the stored camera); the first run seeds the
//! element accumulator by copy and every later run composites Normal onto it (ping-pong). The result stands for the
//! layer's own paint, so masks, mattes, the layer stack and the blend downstream are unchanged. Matte sources and the
//! canvas background are painted plain, exactly as the baseline does.
use crate::compositor::object_compositor::layered::{item_object, LayerContext};
use crate::compositor::object_target::TargetReservation;
use crate::compositor::scene_effect_gpu_blend::pool::{Pool, Role};
use crate::compositor::scene_effect_gpu_blend::{BlendKernels, GpuLayerInput, GpuLayerStack};
use crate::compositor::{Compositor, CompositorError};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::EffectStackPlan;
use crate::object_render_scene::PaintEntry;

type Bbox = (f32, f32, f32, f32);

/// One run of the layer: its entries and, for an effected item, the stack with the item's box.
pub(in crate::compositor) struct Run<'a, 'w> {
    pub(in crate::compositor) entries: Vec<PaintEntry<'w>>,
    pub(in crate::compositor) effect: Option<(&'a EffectStackPlan, Bbox)>,
}

pub(in crate::compositor) fn runs<'a, 'w>(
    ctx: &LayerContext<'w>,
    layer: usize,
    input: &GpuLayerInput<'a>,
    size: (u32, u32),
) -> Vec<Run<'a, 'w>> {
    let mut runs: Vec<Run<'a, 'w>> = Vec::new();
    for entry in ctx.layer_entries(layer) {
        let stack = item_object(ctx.plan(), &entry)
            .and_then(|object| input.item_effects.get(&object).copied());
        match (stack, runs.last_mut()) {
            (
                None,
                Some(Run {
                    entries,
                    effect: None,
                }),
            ) => entries.push(entry),
            (None, _) => runs.push(Run {
                entries: vec![entry],
                effect: None,
            }),
            (Some(plan), _) => runs.push(Run {
                effect: Some((plan, ctx.entry_bbox(&entry, size))),
                entries: vec![entry],
            }),
        }
    }
    runs
}

fn unavailable() -> CompositorError {
    CompositorError::new("item-level effects need the witness behind the layer stack")
}

impl Compositor {
    /// Admit every item stack of every element step (and element folder child) against its item's box BEFORE any
    /// texture exists, so a custom shader refuses the whole frame typed.
    pub(super) fn admit_item_effects(
        ctx: Option<&LayerContext<'_>>,
        stack: &GpuLayerStack<'_>,
        size: (u32, u32),
    ) -> Result<(), CompositorError> {
        let wires = stack.steps().iter().flat_map(|step| {
            let own = step.elements.then_some(step.layer);
            own.into_iter().chain(
                step.folder
                    .iter()
                    .flatten()
                    .filter(|c| c.elements)
                    .map(|c| c.layer),
            )
        });
        for wire in wires {
            let input = &stack.inputs()[wire];
            let (Some(ctx), Some(layer)) = (ctx, input.source_layer) else {
                return Err(unavailable());
            };
            for run in runs(ctx, layer, input, size) {
                if let Some((plan, bbox)) = run.effect {
                    Self::admit_layer_effects(plan, bbox, ctx.time())?;
                }
            }
        }
        Ok(())
    }

    /// Paint wire layer `wire` into the pool role `target`: through its item stacks when `elements`, plain otherwise.
    pub(super) fn paint_wire(
        &mut self,
        (kernels, pool): (&BlendKernels, &mut Pool),
        (ctx, stack): (Option<&LayerContext<'_>>, &GpuLayerStack<'_>),
        (wire, target, elements): (usize, Role, bool),
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
    ) -> Result<(), CompositorError> {
        let size = pool.size();
        let input = &stack.inputs()[wire];
        if !elements {
            return self.render_wire_layer(ctx, stack, wire, pool.view(target), size);
        }
        let (Some(ctx), Some(layer)) = (ctx, input.source_layer) else {
            return Err(unavailable());
        };
        for role in [Role::ElemA, Role::ElemB, Role::Run] {
            pool.ensure(self, role, reserve)?;
        }
        kernels.clear(pool.view(Role::ElemA), wgpu::Color::TRANSPARENT);
        let (mut current, mut spare) = (Role::ElemA, Role::ElemB);
        let mut first = true;
        for run in runs(ctx, layer, input, size) {
            self.render_entries(ctx, &run.entries, pool.view(Role::Run), size)?;
            let graded = match run.effect {
                Some((plan, bbox)) => Some(self.apply_layer_effects(
                    plan,
                    (bbox, ctx.time()),
                    pool.view(Role::Run),
                    size,
                )?),
                None => None,
            };
            let (view, texture) = match &graded {
                Some(graded) => (graded.view(), graded.texture()),
                None => (pool.view(Role::Run), pool.texture(Role::Run)),
            };
            if first {
                // Nothing to composite onto yet: the accumulator is transparent, so a copy seeds it.
                kernels.copy(texture, pool.texture(current), size);
                first = false;
            } else {
                kernels.blend(pool.view(current), view, 0, pool.view(spare));
                std::mem::swap(&mut current, &mut spare);
            }
        }
        kernels.copy(pool.texture(current), pool.texture(target), size);
        Ok(())
    }
}
