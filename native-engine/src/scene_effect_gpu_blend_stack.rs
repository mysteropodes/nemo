//! Typed, immutable input of the layered GPU composition path (L08-G).
//!
//! A `GpuLayerStack` is the L08-A `CompositionPlan` joined to per-layer paint:
//! it holds no texture, device or scheduler state and is validated completely
//! BEFORE any GPU allocation. A flat plan is refused (`FlatPlan`): the object
//! compositor's fast path keeps painting it unchanged. Every baseline feature
//! this leaf does not render (effect stacks, element effects, folders, mask
//! feather) is a typed `Unavailable` refusal, never a silent drop.

use crate::object_render_scene::object_scene_style_emit::{prepare_item, PreparedItem};
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    CompositionPlan, LayerStep, MatteStep, PaintSource,
};
use crate::object_render_scene::{ObjectPaintCommand, ObjectPaintPlan};
use std::collections::BTreeMap;
use vello::kurbo::BezPath;

/// Baseline `if feather > 0.01` threshold of `composite_scene`: a feather above
/// it needs the shared blur pass, which belongs to the effect-pass leaf.
const FEATHER_PASS_THRESHOLD: f32 = 0.01;

/// One closed contour with straight (non-premultiplied) RGBA in `0..=1`.
#[derive(Clone, Debug)]
pub(crate) struct GpuFill<'a> {
    pub(crate) path: &'a BezPath,
    pub(crate) rgba: [f32; 4],
    // A prevalidated full style for gradient, stroke and empty vector paint.
    // None preserves the original straight-image solid precision path.
    pub(crate) prepared: Option<PreparedItem>,
}

impl<'a> GpuFill<'a> {
    pub(crate) fn solid(path: &'a BezPath, rgba: [f32; 4]) -> Self {
        Self {
            path,
            rgba,
            prepared: None,
        }
    }

    fn from_command(command: ObjectPaintCommand<'a>) -> Result<Self, GpuBlendError> {
        let layer = command.source().source_layer_index();
        let invalid = || GpuBlendError::InvalidPaint { layer };
        let style = command.style().map_err(|_| invalid())?;
        // Always prepare first: Fallback must refuse rather than retaining
        // its flat paint merely because FillResolution::solid returns it.
        let full = prepare_item(&style).map_err(|_| invalid())?;
        let prepared = if style.stroke.is_none() && style.fill.solid().is_some() {
            None
        } else {
            Some(full)
        };
        Ok(Self {
            path: command.path(),
            rgba: command.solid().rgba_f32().map_err(|_| invalid())?,
            prepared,
        })
    }
}

/// Fills painted in order into one transparent-backed texture.
#[derive(Clone, Debug, Default)]
pub(crate) struct GpuLayerPaint<'a> {
    pub(crate) fills: Vec<GpuFill<'a>>,
}

/// `MaskIn::mode`: anything not subtract/intersect is a union member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuMaskMode {
    Add,
    Subtract,
    Intersect,
}

/// One vector mask; only the coverage (alpha) of its paint matters.
#[derive(Clone, Debug)]
pub(crate) struct GpuMask<'a> {
    pub(crate) mode: GpuMaskMode,
    pub(crate) paint: GpuLayerPaint<'a>,
}

/// Everything the executor needs from one wire layer, indexed like the plan.
#[derive(Clone, Debug, Default)]
pub(crate) struct GpuLayerInput<'a> {
    pub(crate) paint: GpuLayerPaint<'a>,
    pub(crate) masks: Vec<GpuMask<'a>>,
    pub(crate) mask_feather: f32,
}

/// Baseline features this leaf does not render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuUnavailable {
    ElementEffects { layer: usize },
    FolderLayer { layer: usize },
    LayerEffects { layer: usize },
    EffectLayerStack { layer: usize },
    MaskFeather { layer: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuBlendError {
    /// The plan is the flat fast path; paint it with `compose_object`.
    FlatPlan,
    LayerCountMismatch {
        planned: usize,
        supplied: usize,
    },
    IndexOutOfRange {
        index: usize,
    },
    MaskCountMismatch {
        layer: usize,
        planned: usize,
        supplied: usize,
    },
    InvalidPaint {
        layer: usize,
    },
    /// A paint plan visited one source layer in two separate runs.
    NonContiguousLayer {
        source_layer: usize,
    },
    Unavailable(GpuUnavailable),
}

impl GpuBlendError {
    pub(crate) fn message(&self) -> String {
        match self {
            GpuBlendError::FlatPlan => "flat plan uses the object compositor fast path".to_owned(),
            GpuBlendError::LayerCountMismatch { planned, supplied } => {
                format!("plan covers {planned} layers but {supplied} inputs were supplied")
            }
            GpuBlendError::IndexOutOfRange { index } => {
                format!("layer index {index} is outside the stack")
            }
            GpuBlendError::MaskCountMismatch {
                layer,
                planned,
                supplied,
            } => format!("layer {layer} plans {planned} masks but supplies {supplied}"),
            GpuBlendError::InvalidPaint { layer } => {
                format!("layer {layer} holds a non-finite or out-of-range paint")
            }
            GpuBlendError::NonContiguousLayer { source_layer } => {
                format!("source layer {source_layer} is painted in two separate runs")
            }
            GpuBlendError::Unavailable(what) => {
                format!("unavailable on the layered path: {what:?}")
            }
        }
    }
}

/// One planned paint step: the layer, its resolved mattes, its blend index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuStep {
    pub(crate) layer: usize,
    pub(crate) mattes: Vec<MatteStep>,
    pub(crate) blend: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuLayerStack<'a> {
    inputs: Vec<GpuLayerInput<'a>>,
    background: Option<usize>,
    steps: Vec<GpuStep>,
}

fn paint_is_valid(paint: &GpuLayerPaint<'_>) -> bool {
    paint.fills.iter().all(|fill| {
        fill.rgba
            .iter()
            .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
    })
}

impl<'a> GpuLayerStack<'a> {
    /// Join a layered plan to its per-layer inputs. Nothing is allocated.
    pub(crate) fn from_plan(
        plan: &CompositionPlan,
        inputs: Vec<GpuLayerInput<'a>>,
    ) -> Result<Self, GpuBlendError> {
        let (background, plan_steps) = match plan {
            CompositionPlan::Flat { .. } => return Err(GpuBlendError::FlatPlan),
            CompositionPlan::Layered { background, steps } => (*background, steps),
        };
        if plan_steps.len() != inputs.len() {
            return Err(GpuBlendError::LayerCountMismatch {
                planned: plan_steps.len(),
                supplied: inputs.len(),
            });
        }
        for (layer, input) in inputs.iter().enumerate() {
            let masks_ok = input.masks.iter().all(|m| paint_is_valid(&m.paint));
            if !paint_is_valid(&input.paint) || !masks_ok {
                return Err(GpuBlendError::InvalidPaint { layer });
            }
        }
        if let Some(index) = background.filter(|index| *index >= inputs.len()) {
            return Err(GpuBlendError::IndexOutOfRange { index });
        }
        let mut steps = Vec::new();
        for step in plan_steps {
            match step {
                LayerStep::Skipped { .. } => {}
                LayerStep::EffectLayer {
                    index,
                    applies_stack,
                } => {
                    // A disabled stack copies the backdrop through unchanged
                    // (identical pixels); an enabled one needs the effect pass.
                    if *applies_stack {
                        return Err(GpuBlendError::Unavailable(
                            GpuUnavailable::EffectLayerStack { layer: *index },
                        ));
                    }
                }
                LayerStep::Paint(paint) => {
                    let layer = paint.index;
                    let unavailable = match &paint.source {
                        PaintSource::Plain if paint.layer_effects => {
                            Some(GpuUnavailable::LayerEffects { layer })
                        }
                        PaintSource::Plain => None,
                        PaintSource::ElementEffects => {
                            Some(GpuUnavailable::ElementEffects { layer })
                        }
                        PaintSource::Folder(_) => Some(GpuUnavailable::FolderLayer { layer }),
                    };
                    if let Some(what) = unavailable {
                        return Err(GpuBlendError::Unavailable(what));
                    }
                    let input = inputs
                        .get(layer)
                        .ok_or(GpuBlendError::IndexOutOfRange { index: layer })?;
                    if paint.mask_count != input.masks.len() {
                        return Err(GpuBlendError::MaskCountMismatch {
                            layer,
                            planned: paint.mask_count,
                            supplied: input.masks.len(),
                        });
                    }
                    if !input.masks.is_empty() && input.mask_feather > FEATHER_PASS_THRESHOLD {
                        return Err(GpuBlendError::Unavailable(GpuUnavailable::MaskFeather {
                            layer,
                        }));
                    }
                    if let Some(matte) = paint.mattes.iter().find(|m| m.source >= inputs.len()) {
                        return Err(GpuBlendError::IndexOutOfRange {
                            index: matte.source,
                        });
                    }
                    steps.push(GpuStep {
                        layer,
                        mattes: paint.mattes.clone(),
                        blend: paint.blend,
                    });
                }
            }
        }
        Ok(Self {
            inputs,
            background,
            steps,
        })
    }

    pub(crate) fn inputs(&self) -> &[GpuLayerInput<'a>] {
        &self.inputs
    }

    pub(crate) fn background(&self) -> Option<usize> {
        self.background
    }

    pub(crate) fn steps(&self) -> &[GpuStep] {
        &self.steps
    }

    /// Fills of every layer's own paint (mask shapes excluded): the number a
    /// witness plan must account for, one fill per paint command.
    pub(crate) fn layer_fill_count(&self) -> usize {
        self.inputs
            .iter()
            .map(|input| input.paint.fills.len())
            .sum()
    }

    /// True when the layers' own fills are, as a multiset, exactly the witness
    /// plan's paint commands: the same contour (by address, the plan owns the
    /// path) and the same straight-RGBA bits and full prepared gradient/stroke style. A duplicated run paired with a
    /// dropped one, a foreign path or a changed colour all keep the fill COUNT
    /// and are caught here. Layer order is a plan matter and is not compared.
    pub(crate) fn covers_plan(&self, plan: &ObjectPaintPlan) -> bool {
        // Group by exact path and scalar bits first. This keeps the usual
        // one-command-per-path case O(n log n), while matching duplicate
        // commands as a multiset, including their complete prepared style.
        let mut wanted: BTreeMap<FillKey, Vec<Option<PreparedItem>>> = BTreeMap::new();
        for command in plan.commands() {
            let Ok(fill) = GpuFill::from_command(command) else {
                return false;
            };
            wanted
                .entry(fill_key(fill.path, fill.rgba))
                .or_default()
                .push(fill.prepared);
        }
        for fill in self
            .inputs
            .iter()
            .flat_map(|input| input.paint.fills.iter())
        {
            let Some(group) = wanted.get_mut(&fill_key(fill.path, fill.rgba)) else {
                return false;
            };
            let Some(index) = group.iter().position(|style| style == &fill.prepared) else {
                return false;
            };
            group.swap_remove(index);
        }
        wanted.values().all(Vec::is_empty)
    }
}

/// Contour address plus exact colour bits of one fill.
type FillKey = (usize, [u32; 4]);

fn fill_key(path: &BezPath, rgba: [f32; 4]) -> FillKey {
    (path as *const BezPath as usize, rgba.map(f32::to_bits))
}

/// One source layer's contiguous commands, in painting order.
pub(crate) struct GpuLayerRun<'a> {
    pub(crate) source_layer_index: usize,
    pub(crate) paint: GpuLayerPaint<'a>,
}

/// Group the real paint plan's commands by source layer. The plan's join
/// guarantees each layer is one contiguous run; a violation is refused.
pub(crate) fn layer_runs_from_plan(
    plan: &ObjectPaintPlan,
) -> Result<Vec<GpuLayerRun<'_>>, GpuBlendError> {
    let mut runs: Vec<GpuLayerRun<'_>> = Vec::new();
    for command in plan.commands() {
        let layer = command.source().source_layer_index();
        let fill = GpuFill::from_command(command)?;
        if runs.last().map(|run| run.source_layer_index) != Some(layer) {
            if runs.iter().any(|run| run.source_layer_index == layer) {
                return Err(GpuBlendError::NonContiguousLayer {
                    source_layer: layer,
                });
            }
            runs.push(GpuLayerRun {
                source_layer_index: layer,
                paint: GpuLayerPaint::default(),
            });
        }
        if let Some(run) = runs.last_mut() {
            run.paint.fills.push(fill);
        }
    }
    Ok(runs)
}
