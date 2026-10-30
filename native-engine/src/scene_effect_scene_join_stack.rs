//! Path selection of the compositor for one joined scene (L08-K; G03b, G48, G49).
//!
//! The viewport, the PNG readback and the export job all hold the same `ObjectPaintPlan` (the existing contour
//! commands) and, with L08-K, the same `SceneEffectPlan` of the packet behind it. This module is the one place that
//! decides which compositor entry paints them:
//! * a scene with an entry no producer drew is refused before anything is allocated (never a partial scene);
//! * a `Flat` composition plan keeps the accepted fast path (`Compositor::compose_object`), unchanged;
//! * a `Layered` plan becomes a `GpuLayerStack` (L08-G) whose per-wire-layer fills are the paint plan's commands
//!   grouped by SOURCE layer and placed at that layer's paint-order wire index, validated as a whole, and checked to
//!   cover the plan exactly once (`covers_plan`), so a layer can neither lose nor duplicate a command.
//! Every layered feature G cannot render yet (layer and element effect stacks, effect-layer grading, folder passes,
//! mask feather) comes back as the typed `GpuBlendError::Unavailable` it already is; nothing is dropped.
//! No texture, device or scheduler state is touched: the compositor entries run the returned stack.
use crate::compositor::scene_effect_gpu_blend::{
    layer_runs_from_plan, GpuBlendError, GpuFill, GpuLayerInput, GpuLayerPaint, GpuLayerStack,
    GpuMask, GpuMaskMode,
};
use crate::object_render_scene::mask_inputs::ObjectMaskMode;
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::CompositionPlan;
use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::SceneEffectPlan;
use crate::object_render_scene::ObjectPaintPlan;

/// Which compositor entry paints the scene.
#[derive(Clone, Debug)]
pub(crate) enum PathSelection<'a> {
    /// `compose_object`: every layer's commands in authored order, one pass.
    Flat,
    /// `compose_object_layered` / `compose_layered_scheduled_object` with this validated stack.
    Layered(GpuLayerStack<'a>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PathSelectionError {
    /// The join was built from another packet than the one behind the paint plan.
    WrongPlan,
    /// A selected record no producer drew (or a producer reported unavailable).
    Incomplete {
        source_layer_index: usize,
        source_object_index: usize,
    },
    /// The paint plan paints a source layer the join has no slot for.
    UnknownLayer { source_layer_index: usize },
    /// The layered stack refused the plan (typed, including every `Unavailable` feature).
    Stack(GpuBlendError),
    /// The layers' fills are not, as a multiset, exactly the plan's commands.
    PlanNotCovered,
}

/// Select the compositor path for `plan`, the paint plan of the packet `join` was built from.
pub(crate) fn select_composition_path<'a>(
    join: &SceneEffectPlan,
    plan: &'a ObjectPaintPlan,
) -> Result<PathSelection<'a>, PathSelectionError> {
    if !join.is_for_packet(plan.packet()) || !plan.mask_inputs().is_for_packet(plan.packet()) {
        return Err(PathSelectionError::WrongPlan);
    }
    if let Some((source_layer_index, source_object_index)) = join.first_unavailable() {
        return Err(PathSelectionError::Incomplete {
            source_layer_index,
            source_object_index,
        });
    }
    if matches!(join.plan(), CompositionPlan::Flat { .. }) {
        if plan
            .mask_inputs()
            .layers()
            .iter()
            .any(|layer| !layer.masks().is_empty())
        {
            return Err(PathSelectionError::WrongPlan);
        }
        return Ok(PathSelection::Flat);
    }
    let runs = layer_runs_from_plan(plan).map_err(PathSelectionError::Stack)?;
    let mut inputs: Vec<GpuLayerInput<'a>> = (0..join.layers().len())
        .map(|_| GpuLayerInput::default())
        .collect();
    for run in runs {
        let slot = join.wire_index_of_source(run.source_layer_index).ok_or(
            PathSelectionError::UnknownLayer {
                source_layer_index: run.source_layer_index,
            },
        )?;
        inputs[slot].paint = run.paint;
    }
    for layer in plan.mask_inputs().layers() {
        let source_layer_index = layer.source_layer_index();
        let slot = join
            .wire_index_of_source(source_layer_index)
            .ok_or(PathSelectionError::UnknownLayer { source_layer_index })?;
        let invalid = || PathSelectionError::Stack(GpuBlendError::InvalidPaint { layer: slot });
        let feather = layer.feather() as f32;
        if !feather.is_finite() {
            return Err(invalid());
        }
        inputs[slot].mask_feather = feather;
        for mask in layer.masks() {
            let path = plan.mask_path(mask).ok_or(PathSelectionError::WrongPlan)?;
            let mode = match mask.mode() {
                ObjectMaskMode::Add => GpuMaskMode::Add,
                ObjectMaskMode::Subtract => GpuMaskMode::Subtract,
                ObjectMaskMode::Intersect => GpuMaskMode::Intersect,
            };
            inputs[slot].masks.push(GpuMask {
                mode,
                paint: GpuLayerPaint {
                    fills: vec![GpuFill::solid(path, mask.rgba())],
                },
            });
        }
    }
    let stack = GpuLayerStack::from_plan(join.plan(), inputs).map_err(PathSelectionError::Stack)?;
    if !stack.covers_plan(plan) {
        return Err(PathSelectionError::PlanNotCovered);
    }
    Ok(PathSelection::Layered(stack))
}

#[cfg(test)]
#[path = "../tests/scene_effect_scene_join_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/scene_effect_scene_join_stack.rs"]
mod tests;
#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/scene_effect_scene_join_families.rs"]
mod families_tests;
