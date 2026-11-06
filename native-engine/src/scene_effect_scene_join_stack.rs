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
//! Every layered feature G cannot render (a custom shader, an item stack no supported entry owns) comes back as the typed
//! `GpuBlendError::Unavailable` it already is; nothing is dropped. Layer, folder, effect-layer and item stacks are the
//! join's planned `EffectStackPlan`s, admitted against each layer's or item's box when the stack is composed.
//! No texture, device or scheduler state is touched: the compositor entries run the returned stack.
use crate::compositor::scene_effect_gpu_blend::{
    layer_runs_from_plan, GpuBlendError, GpuFill, GpuLayerInput, GpuLayerPaint, GpuLayerStack,
    GpuMask, GpuMaskMode, GpuUnavailable,
};
use crate::compositor::object_compositor::layered::claimed_object;
use crate::object_render_scene::layered_entries;
use crate::object_render_scene::mask_inputs::ObjectMaskMode;
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    CompositionPlan, LayerStep, PaintSource, SkipReason,
};
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

/// True for a layered scene whose ONLY layered feature is folders holding plain Normal layers (no blend, matte, mask,
/// effect, effect layer or canvas background). Such a scene is the flat sequence with each folder's children moved to
/// the folder's place (Normal compositing is associative: `folder_ranks`, applied by `compose_object`), so every family
/// the flat path paints keeps working inside a folder: Text, images, video, components, duplicators, brushes, the
/// camera and the outline. Any other layered scene goes to the layer stack.
pub(crate) fn folds_only(join: &SceneEffectPlan) -> bool {
    let CompositionPlan::Layered { background: None, steps } = join.plan() else {
        return false;
    };
    let mut folders = 0;
    for step in steps {
        match step {
            LayerStep::Skipped {
                reason: SkipReason::FolderChild,
                ..
            } => {}
            LayerStep::Paint(paint)
                if paint.mask_count == 0
                    && paint.mattes.is_empty()
                    && !paint.layer_effects
                    && paint.blend == 0 =>
            {
                match &paint.source {
                    PaintSource::Plain => {}
                    PaintSource::Folder(children) => {
                        folders += 1;
                        if children
                            .iter()
                            .any(|c| c.item_effects || c.layer_effects || c.blend != 0)
                        {
                            return false;
                        }
                    }
                    PaintSource::ElementEffects => return false,
                }
            }
            _ => return false,
        }
    }
    folders > 0
}

/// Select the compositor path for `plan`, the paint plan of the packet `join` was built from.
pub(crate) fn select_composition_path<'a>(
    join: &'a SceneEffectPlan,
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
    // Folders only: the flat sequence, folded (see `folds_only`); `compose_object` applies the same ranks.
    if plan.mask_inputs().layers().is_empty() && folds_only(join) {
        return Ok(PathSelection::Flat);
    }
    let runs = layer_runs_from_plan(plan).map_err(PathSelectionError::Stack)?;
    let mut inputs: Vec<GpuLayerInput<'a>> = (0..join.layers().len())
        .map(|_| GpuLayerInput::default())
        .collect();
    for (slot, layer) in join.layers().iter().enumerate() {
        inputs[slot].source_layer = Some(layer.source_layer_index);
        inputs[slot].effects = layer.stack.as_ref();
        inputs[slot].item_effects = layer
            .item_stacks
            .iter()
            .map(|(object, stack)| (*object, stack))
            .collect();
    }
    // Every item stack is drawn by exactly the entry that owns its record; a stack no entry claims (a record
    // nothing draws, a video slot) is refused typed, never dropped. The one exception is a duplicator host: it paints
    // its copies in place of its original records, so the stack of an original no copy claims (the output is empty)
    // has nothing to grade and stays unused, as the baseline draws nothing for that layer. The layer stack looks a
    // stack up only by the record an entry draws, so the unclaimed one is never run.
    if join.layers().iter().any(|layer| !layer.item_stacks.is_empty()) {
        let claimed: std::collections::BTreeSet<usize> = layered_entries(plan)
            .filter_map(|(_, entry)| claimed_object(plan, &entry))
            .collect();
        for (slot, layer) in join.layers().iter().enumerate() {
            if !plan.is_duplicator_host(layer.source_layer_index)
                && layer.item_stacks.iter().any(|(object, _)| !claimed.contains(object))
            {
                return Err(PathSelectionError::Stack(GpuBlendError::Unavailable(
                    GpuUnavailable::ElementEffects { layer: slot },
                )));
            }
        }
    }
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
#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/scene_effect_layered_view.rs"]
mod layered_view_tests;
