//! Layer-stack composition-order planner for the native scene.
//!
//! Pure CPU decisions only, ported from the retained baseline compositor
//! (`geometry-wasm/src/engine.rs` at fbe4b8a3): the blend-mode vocabulary,
//! track-matte resolution, the consumed matte-source and folder-child sets,
//! the canvas-background index, the slow-path predicate, and the per-layer
//! dispatch order of `composite_scene`. It owns no pixels, GPU resource,
//! scheduler or document; a caller supplies one `LayerComposition` per wire
//! layer and receives an immutable `CompositionPlan`. A stack that needs none
//! of the layered features plans to the flat authored order the object
//! compositor already paints, so existing pixels are unchanged.
//!
//! Fields stay `Option` where the baseline wire type is `Option`: an absent
//! `blendMode` and an authored `"normal"` both draw as Normal but remain
//! distinct values here.

/// Blend vocabulary; the discriminant is the `blend.wgsl` mode index
/// (`mix_mode_index`, engine.rs:431).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlendMode {
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
    ColorDodge = 6,
    ColorBurn = 7,
    HardLight = 8,
    SoftLight = 9,
    Difference = 10,
    Exclusion = 11,
    Hue = 12,
    Saturation = 13,
    Color = 14,
    Luminosity = 15,
}

impl BlendMode {
    pub(crate) const ALL: [BlendMode; 16] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];

    /// Absent, `"normal"` and every unrecognized name are Normal; names are
    /// case-sensitive exactly as in the baseline.
    pub(crate) fn from_name(name: Option<&str>) -> BlendMode {
        match name.unwrap_or("normal") {
            "multiply" => BlendMode::Multiply,
            "screen" => BlendMode::Screen,
            "overlay" => BlendMode::Overlay,
            "darken" => BlendMode::Darken,
            "lighten" => BlendMode::Lighten,
            "colorDodge" => BlendMode::ColorDodge,
            "colorBurn" => BlendMode::ColorBurn,
            "hardLight" => BlendMode::HardLight,
            "softLight" => BlendMode::SoftLight,
            "difference" => BlendMode::Difference,
            "exclusion" => BlendMode::Exclusion,
            "hue" => BlendMode::Hue,
            "saturation" => BlendMode::Saturation,
            "color" => BlendMode::Color,
            "luminosity" => BlendMode::Luminosity,
            _ => BlendMode::Normal,
        }
    }

    pub(crate) fn index(self) -> u32 {
        self as u32
    }
}

pub(crate) fn mix_mode_index(name: Option<&str>) -> u32 {
    BlendMode::from_name(name).index()
}

/// `matte.wgsl` mode indices (engine.rs:1392).
pub(crate) const MATTE_ALPHA: u32 = 0;
pub(crate) const MATTE_LUMA: u32 = 1;

/// `(mode, invert)` for a recognized matte mode string; `None` for absent,
/// `"none"` and anything else (including a wrong-case spelling).
pub(crate) fn matte_mode_of(s: Option<&str>) -> Option<(u32, bool)> {
    match s {
        Some("alpha") => Some((MATTE_ALPHA, false)),
        Some("alphaInverted") => Some((MATTE_ALPHA, true)),
        Some("luma") => Some((MATTE_LUMA, false)),
        Some("lumaInverted") => Some((MATTE_LUMA, true)),
        _ => None,
    }
}

/// One extra matte under a layer's first (`MatteIn`, engine.rs:1418).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MatteExtra {
    pub(crate) mode: Option<String>,
    pub(crate) source_index: Option<usize>,
}

/// Composition-relevant projection of one wire layer (`LayerIn`). The two
/// effect flags and `mask_count` are reductions of the layer's `effects`,
/// its items' `effects` and its `masks`: only "any enabled" and "how many"
/// influence ordering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayerComposition {
    pub(crate) blend_mode: Option<String>,
    pub(crate) matte_mode: Option<String>,
    pub(crate) matte_source_index: Option<usize>,
    pub(crate) mattes_more: Option<Vec<MatteExtra>>,
    pub(crate) is_effect_layer: Option<bool>,
    pub(crate) is_canvas_background: Option<bool>,
    pub(crate) is_folder_layer: Option<bool>,
    pub(crate) folder_child_indices: Option<Vec<usize>>,
    pub(crate) layer_effect_enabled: bool,
    pub(crate) item_effect_enabled: bool,
    pub(crate) mask_count: usize,
}

impl LayerComposition {
    fn is_folder(&self) -> bool {
        self.is_folder_layer.unwrap_or(false)
    }
    fn is_effect(&self) -> bool {
        self.is_effect_layer.unwrap_or(false)
    }
}

/// One resolved matte: source layer index, matte mode, invert flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MatteStep {
    pub(crate) source: usize,
    pub(crate) mode: u32,
    pub(crate) invert: bool,
}

/// Which layer is `i`'s first-matte source (engine.rs:1464). An explicit
/// index wins and, when it is `i` itself or out of range, resolves to `None`
/// with NO legacy fallback; only an absent index falls back to `i + 1`. A
/// layer without a recognized matte mode has no source. An `i` outside the
/// stack is `None` (the baseline never asks).
pub(crate) fn resolve_matte_source(layers: &[LayerComposition], i: usize) -> Option<usize> {
    let layer = layers.get(i)?;
    matte_mode_of(layer.matte_mode.as_deref())?;
    let n = layers.len();
    match layer.matte_source_index {
        Some(s) if s < n && s != i => Some(s),
        Some(_) => None,
        None if i + 1 < n => Some(i + 1),
        None => None,
    }
}

/// Every matte on layer `i`, first one included, in application order
/// (engine.rs:1437). The extras count only once the first matte resolved and
/// never get the `i + 1` fallback; an extra whose source is `i`, out of
/// range, absent or whose mode is unrecognized is dropped.
pub(crate) fn resolve_all_mattes(layers: &[LayerComposition], i: usize) -> Vec<MatteStep> {
    let mut out = Vec::new();
    let layer = match layers.get(i) {
        Some(layer) => layer,
        None => return out,
    };
    if let (Some((mode, invert)), Some(source)) = (
        matte_mode_of(layer.matte_mode.as_deref()),
        resolve_matte_source(layers, i),
    ) {
        out.push(MatteStep {
            source,
            mode,
            invert,
        });
    }
    if out.is_empty() {
        return out;
    }
    let n = layers.len();
    if let Some(extra) = layer.mattes_more.as_ref() {
        for m in extra {
            let source = match m.source_index {
                Some(s) if s < n && s != i => s,
                _ => continue,
            };
            if let Some((mode, invert)) = matte_mode_of(m.mode.as_deref()) {
                out.push(MatteStep {
                    source,
                    mode,
                    invert,
                });
            }
        }
    }
    out
}

/// Union of every resolved matte source: those layers do not paint on their
/// own (engine.rs:3112-3121).
pub(crate) fn matte_source_set(layers: &[LayerComposition]) -> Vec<bool> {
    let mut set = vec![false; layers.len()];
    for i in 0..layers.len() {
        for step in resolve_all_mattes(layers, i) {
            set[step.source] = true;
        }
    }
    set
}

/// Layers consumed by some folder's child list (engine.rs:3137-3157). A
/// folder-shaped child is NOT consumed: it stays a top-level layer. Indices
/// out of range are ignored.
pub(crate) fn folder_child_set(layers: &[LayerComposition]) -> Vec<bool> {
    let n = layers.len();
    let mut set = vec![false; n];
    for layer in layers {
        if !layer.is_folder() {
            continue;
        }
        if let Some(kids) = &layer.folder_child_indices {
            for &ci in kids {
                if ci < n && !layers[ci].is_folder() {
                    set[ci] = true;
                }
            }
        }
    }
    set
}

/// First layer flagged `isCanvasBackground` (engine.rs:3111).
pub(crate) fn background_index(layers: &[LayerComposition]) -> Option<usize> {
    layers
        .iter()
        .position(|l| l.is_canvas_background.unwrap_or(false))
}

/// Why a stack cannot use the flat fast path (engine.rs:3121-3136, 3158).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SlowPathReasons {
    pub(crate) blend: bool,
    pub(crate) matte: bool,
    pub(crate) effect_stack: bool,
    pub(crate) effect_layer: bool,
    pub(crate) mask: bool,
    pub(crate) folder: bool,
}

impl SlowPathReasons {
    pub(crate) fn of(layers: &[LayerComposition]) -> SlowPathReasons {
        Self::with_matte_set(layers, &matte_source_set(layers))
    }

    fn with_matte_set(layers: &[LayerComposition], matte_set: &[bool]) -> SlowPathReasons {
        SlowPathReasons {
            blend: layers
                .iter()
                .any(|l| mix_mode_index(l.blend_mode.as_deref()) != 0),
            matte: matte_set.iter().any(|&b| b),
            effect_stack: layers
                .iter()
                .any(|l| l.layer_effect_enabled || l.item_effect_enabled),
            effect_layer: layers.iter().any(|l| l.is_effect()),
            mask: layers.iter().any(|l| l.mask_count > 0),
            folder: layers.iter().any(|l| l.is_folder()),
        }
    }

    /// False only when no blend, matte, enabled effect, effect layer, mask or
    /// folder exists anywhere in the stack.
    pub(crate) fn any(&self) -> bool {
        self.blend
            || self.matte
            || self.effect_stack
            || self.effect_layer
            || self.mask
            || self.folder
    }
}

/// Why a top-level layer does not paint on its own. Checked in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkipReason {
    MatteSource,
    FolderChild,
    CanvasBackground,
}

/// One child painted inside a folder's isolated pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FolderChildStep {
    pub(crate) index: usize,
    pub(crate) item_effects: bool,
    pub(crate) layer_effects: bool,
    pub(crate) blend: u32,
    /// The first in-range child seeds the accumulator; its blend is moot.
    pub(crate) seeds_accumulator: bool,
}

/// What produces a painted layer's own pixels. Folder wins over item effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PaintSource {
    Plain,
    ElementEffects,
    Folder(Vec<FolderChildStep>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaintStep {
    pub(crate) index: usize,
    pub(crate) source: PaintSource,
    pub(crate) mask_count: usize,
    pub(crate) mattes: Vec<MatteStep>,
    pub(crate) layer_effects: bool,
    pub(crate) blend: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LayerStep {
    Skipped {
        index: usize,
        reason: SkipReason,
    },
    /// An effect layer never paints items: it grades the running accumulator
    /// (`applies_stack`) or passes it through unchanged.
    EffectLayer {
        index: usize,
        applies_stack: bool,
    },
    Paint(PaintStep),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompositionPlan {
    /// Fast path: every layer's items in authored order, one pass.
    Flat { order: Vec<usize> },
    /// Slow path: `background` paints into its own texture first and (when
    /// present) re-composites after the last step; the accumulator then
    /// flattens against the base color.
    Layered {
        background: Option<usize>,
        steps: Vec<LayerStep>,
    },
}

fn folder_children(layers: &[LayerComposition], layer: &LayerComposition) -> Vec<FolderChildStep> {
    let mut out = Vec::new();
    if let Some(kids) = &layer.folder_child_indices {
        for &index in kids {
            let child = match layers.get(index) {
                Some(child) => child,
                None => continue,
            };
            let seeds_accumulator = out.is_empty();
            out.push(FolderChildStep {
                index,
                item_effects: child.item_effect_enabled,
                layer_effects: child.layer_effect_enabled,
                blend: mix_mode_index(child.blend_mode.as_deref()),
                seeds_accumulator,
            });
        }
    }
    out
}

/// Plan one scene's layer stack exactly as `composite_scene` walks it.
pub(crate) fn plan_composition(layers: &[LayerComposition]) -> CompositionPlan {
    let matte_set = matte_source_set(layers);
    if !SlowPathReasons::with_matte_set(layers, &matte_set).any() {
        return CompositionPlan::Flat {
            order: (0..layers.len()).collect(),
        };
    }
    let folder_set = folder_child_set(layers);
    let background = background_index(layers);
    let mut steps = Vec::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        let skip = if matte_set[index] {
            Some(SkipReason::MatteSource)
        } else if folder_set[index] {
            Some(SkipReason::FolderChild)
        } else if background == Some(index) {
            Some(SkipReason::CanvasBackground)
        } else {
            None
        };
        if let Some(reason) = skip {
            steps.push(LayerStep::Skipped { index, reason });
            continue;
        }
        if layer.is_effect() {
            steps.push(LayerStep::EffectLayer {
                index,
                applies_stack: layer.layer_effect_enabled,
            });
            continue;
        }
        let source = if layer.is_folder() {
            PaintSource::Folder(folder_children(layers, layer))
        } else if layer.item_effect_enabled {
            PaintSource::ElementEffects
        } else {
            PaintSource::Plain
        };
        steps.push(LayerStep::Paint(PaintStep {
            index,
            source,
            mask_count: layer.mask_count,
            mattes: resolve_all_mattes(layers, index),
            layer_effects: layer.layer_effect_enabled,
            blend: mix_mode_index(layer.blend_mode.as_deref()),
        }));
    }
    CompositionPlan::Layered { background, steps }
}

#[cfg(test)]
#[path = "../tests/scene_effect_order.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/scene_effect_order_cases.rs"]
mod cases;

#[cfg(test)]
#[path = "../tests/scene_effect_order_controls.rs"]
mod controls;
