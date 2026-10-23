//! StoryBoard chain selection kernel (L04-F, I35).
//!
//! The selection half of `montageStrokesAt` and `symbolStrokesAt`
//! (`src/js/storyboard.js`, baseline `fbe4b8a3`): which layer frames of which
//! Component a StoryBoard montage shows at a frame. The time model (retime
//! defaults, `montageTotal`, the `trimIn + min(srcLen - 1, floor(local * srcLen /
//! duration))` source-frame formula, the looping montage-layer frame) already
//! has one owner, `object_montage_timing` (L10-A); this module imports it and
//! adds only what that kernel left out: the previous-keyframe backfill of
//! `symbolStrokesAt`, the storyboard module/Component view that answers its
//! `MontageModules` port, and the Components-only admission.
//!
//! The result is selection data only: `(symbol id, source frame)` plus, per
//! symbol layer, the index of the frame whose strokes the layer shows. The
//! strokes themselves, the layer path effects, combine groups, the duplicator
//! and the Component camera that `symbolStrokesAt` applies afterwards belong to
//! their own kernels (L04-D, the camera owners) and are not repeated here.
//!
//! Baseline facts kept: a dangling chain id and a non-`instance` module are
//! skipped; `moduleById` is the FIRST module with the id; a missing symbol
//! shows nothing and lasts 24 frames; a layer index past its frames reads its
//! last frame; a keyframe or interpolated frame shows its own strokes, any other
//! frame shows the nearest PRECEDING keyframe (an interpolated frame is not a
//! backfill source). A negative montage frame is a typed error of
//! `object_montage_timing`, as there.
//!
//! StoryBoard uses Components only. [`StoryboardChain::baseline`] reproduces the
//! tolerant baseline (an instance naming no symbol lasts 24 frames and shows
//! nothing); [`StoryboardChain::components_only`] refuses such a module with
//! [`StoryboardChainError::NotAComponent`], and nothing here converts a flat
//! layer into a Component. Frame values are integers, as in `object_montage_timing`.

use crate::evaluation::object_montage_timing::{
    montage_layer_frame, montage_source_frame_with, montage_total, resolve_chain,
    storyboard_source_frame, ChainMember, ChainSlot, LayerFrame, MontageMember, MontageModules,
    MontageTimingError, SourceFrame, SourcePick, SymbolView,
};
use std::collections::BTreeMap;

/// One entry of a symbol layer's `frames[]` that is not `null`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SymbolFrame {
    pub is_keyframe: bool,
    pub is_interpolated: bool,
}

/// `sym.layers[i]`: its `frames[]`, a `None` entry being a JavaScript `null`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SymbolLayer {
    pub frames: Vec<Option<SymbolFrame>>,
}

/// A Component (`state.symbols[id]`) as far as selection needs it. `total_frames`
/// is `sym.totalFrames` (`None` when absent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComponentSymbol {
    pub name: Option<String>,
    pub total_frames: Option<i64>,
    pub layers: Vec<SymbolLayer>,
}

/// `state.symbols`, keyed by Component id.
pub(crate) type SymbolTable = BTreeMap<String, ComponentSymbol>;

/// `module.type`; any label but the three StoryBoard types is `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModuleKind {
    Instance,
    Montage,
    Sound,
    Other,
}

impl ModuleKind {
    /// The baseline's `===` comparisons: exact, case-sensitive labels.
    pub(crate) fn from_label(label: &str) -> ModuleKind {
        match label {
            "instance" => ModuleKind::Instance,
            "montage" => ModuleKind::Montage,
            "sound" => ModuleKind::Sound,
            _ => ModuleKind::Other,
        }
    }
}

/// A StoryBoard module as the chain reads it. `symbol_id` and the retime fields
/// matter for an `Instance` only; a `None` retime field is `null`/`undefined`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoryModule {
    pub id: String,
    pub kind: ModuleKind,
    pub symbol_id: String,
    pub trim_in: Option<i64>,
    pub trim_out: Option<i64>,
    pub duration: Option<i64>,
}

/// Why a StoryBoard cannot be admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StoryboardChainError {
    /// An `instance` module names an id that is not a Component (a flat layer
    /// or a deleted Component). Nothing is converted or created for it.
    NotAComponent {
        module_id: String,
        symbol_id: String,
    },
}

/// The frame a symbol layer shows: `layer_index` into `sym.layers`,
/// `frame_index` into that layer's `frames`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayerSource {
    pub layer_index: usize,
    pub frame_index: usize,
}

/// What a montage frame shows: the member's Component, the source frame asked
/// of it and the per-layer frame selection (empty for a missing symbol).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MontageSelection {
    pub symbol_id: String,
    pub src_frame: i64,
    pub layers: Vec<LayerSource>,
}

/// `symbolStrokesAt`'s frame choice for one layer at frame `fi`: the index is
/// clamped to the last frame (`Math.min(fi, frames.length - 1)`); a keyframe or
/// interpolated frame shows itself; otherwise the nearest preceding KEYFRAME.
/// `None`: nothing (index below zero, no frames, no preceding keyframe).
pub(crate) fn layer_source_frame(frames: &[Option<SymbolFrame>], fi: i64) -> Option<usize> {
    let idx = fi.min(frames.len() as i64 - 1);
    if idx < 0 {
        return None;
    }
    let idx = idx as usize;
    if frames[idx].is_some_and(|f| f.is_keyframe || f.is_interpolated) {
        return Some(idx);
    }
    (0..idx)
        .rev()
        .find(|&k| frames[k].is_some_and(|f| f.is_keyframe))
}

/// `symbolStrokesAt(symbolId, fi)` for a present symbol: the frame each layer
/// shows, layers that show nothing left out, in layer order.
pub(crate) fn symbol_layer_sources(symbol: &ComponentSymbol, fi: i64) -> Vec<LayerSource> {
    let mut out = Vec::new();
    for (layer_index, layer) in symbol.layers.iter().enumerate() {
        if let Some(frame_index) = layer_source_frame(&layer.frames, fi) {
            out.push(LayerSource {
                layer_index,
                frame_index,
            });
        }
    }
    out
}

/// What `symbolDuration` reads of `state.symbols[id]`: `None` is a missing symbol.
pub(crate) fn symbol_view(symbol: Option<&ComponentSymbol>) -> SymbolView {
    match symbol {
        None => SymbolView::Missing,
        Some(s) => SymbolView::Present {
            name: s.name.clone(),
            total_frames: s.total_frames.unwrap_or(0),
            first_layer_frame_count: s.layers.first().map(|l| l.frames.len() as i64),
        },
    }
}

fn selection_for(symbols: &SymbolTable, source: SourceFrame) -> MontageSelection {
    let layers = match symbols.get(&source.symbol_id) {
        Some(symbol) => symbol_layer_sources(symbol, source.src_frame),
        None => Vec::new(),
    };
    MontageSelection {
        symbol_id: source.symbol_id,
        src_frame: source.src_frame,
        layers,
    }
}

/// `montageStrokesAt(m, f)` as selection data, with the source-frame formula and
/// the module lookup injected (the controls run wrong ones through this walk).
/// `Ok(None)` is the baseline's `[]` for a frame at or beyond the total.
pub(crate) fn montage_selection_with<M: MontageModules>(
    modules: &M,
    symbols: &SymbolTable,
    chain: &[String],
    frame: i64,
    pick: SourcePick,
) -> Result<Option<MontageSelection>, MontageTimingError> {
    let members = resolve_chain(chain, modules)?;
    let source = montage_source_frame_with(&members, frame, pick)?;
    Ok(source.map(|s| selection_for(symbols, s)))
}

/// The montage-layer path of `getEffectiveStrokes` (`ld.montageId`): a zero
/// total or a frame before `symPlacedAt` shows nothing, otherwise the looping
/// montage-local frame is resolved as above.
pub(crate) fn layer_selection_with<M: MontageModules>(
    modules: &M,
    symbols: &SymbolTable,
    chain: &[String],
    frame: i64,
    placed_at: Option<i64>,
    pick: SourcePick,
) -> Result<Option<MontageSelection>, MontageTimingError> {
    let members = resolve_chain(chain, modules)?;
    let total = montage_total(&members)?;
    match montage_layer_frame(frame, placed_at, total)? {
        LayerFrame::None(_) => Ok(None),
        LayerFrame::Local(local) => {
            let source = montage_source_frame_with(&members, local, pick)?;
            Ok(source.map(|s| selection_for(symbols, s)))
        }
    }
}

/// The StoryBoard modules and the Component table, seen through the
/// `MontageModules` port of `object_montage_timing`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StoryboardChain<'a> {
    modules: &'a [StoryModule],
    symbols: &'a SymbolTable,
}

impl<'a> StoryboardChain<'a> {
    /// The tolerant baseline: an instance naming no symbol is a 24-frame member
    /// that shows nothing. For oracle and parity code ONLY: it silently accepts an
    /// instance naming a flat layer or a deleted Component. StoryBoard callers
    /// (strip, commit/convert transactions, L04-K, L10-I) must construct through
    /// `components_only` so the typed refusal cannot be bypassed.
    pub(crate) fn baseline(modules: &'a [StoryModule], symbols: &'a SymbolTable) -> Self {
        StoryboardChain { modules, symbols }
    }

    /// StoryBoard uses Components only: every `instance` module must name a
    /// Component of `symbols`; the first one (in module order) that does not is
    /// refused. Modules of other kinds are not checked. Nothing is created.
    pub(crate) fn components_only(
        modules: &'a [StoryModule],
        symbols: &'a SymbolTable,
    ) -> Result<Self, StoryboardChainError> {
        for module in modules.iter().filter(|m| m.kind == ModuleKind::Instance) {
            if !symbols.contains_key(&module.symbol_id) {
                return Err(StoryboardChainError::NotAComponent {
                    module_id: module.id.clone(),
                    symbol_id: module.symbol_id.clone(),
                });
            }
        }
        Ok(StoryboardChain { modules, symbols })
    }

    /// `chainMods` with `ensureRetime` applied (nothing is written back).
    pub(crate) fn members(&self, chain: &[String]) -> Result<Vec<ChainMember>, MontageTimingError> {
        resolve_chain(chain, self)
    }

    /// `montageTotal(m)`.
    pub(crate) fn total(&self, chain: &[String]) -> Result<i64, MontageTimingError> {
        montage_total(&self.members(chain)?)
    }

    /// `symbolStrokesAt(symbolId, fi)`; a missing symbol shows nothing.
    pub(crate) fn symbol_selection(&self, symbol_id: &str, fi: i64) -> Vec<LayerSource> {
        match self.symbols.get(symbol_id) {
            Some(symbol) => symbol_layer_sources(symbol, fi),
            None => Vec::new(),
        }
    }

    /// `montageStrokesAt(m, f)`.
    pub(crate) fn selection_at(
        &self,
        chain: &[String],
        frame: i64,
    ) -> Result<Option<MontageSelection>, MontageTimingError> {
        montage_selection_with(self, self.symbols, chain, frame, storyboard_source_frame)
    }

    /// The montage layer at `frame` with its `symPlacedAt`.
    pub(crate) fn layer_selection_at(
        &self,
        chain: &[String],
        frame: i64,
        placed_at: Option<i64>,
    ) -> Result<Option<MontageSelection>, MontageTimingError> {
        layer_selection_with(
            self,
            self.symbols,
            chain,
            frame,
            placed_at,
            storyboard_source_frame,
        )
    }
}

impl MontageModules for StoryboardChain<'_> {
    /// `moduleById` (the first module with the id) plus the `instance` filter.
    fn slot(&self, module_id: &str) -> ChainSlot {
        match self.modules.iter().find(|m| m.id == module_id) {
            None => ChainSlot::Dangling,
            Some(m) if m.kind != ModuleKind::Instance => ChainSlot::NotInstance,
            Some(m) => ChainSlot::Instance(MontageMember {
                symbol_id: m.symbol_id.clone(),
                trim_in: m.trim_in,
                trim_out: m.trim_out,
                duration: m.duration,
                symbol: symbol_view(self.symbols.get(&m.symbol_id)),
            }),
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_storyboard_chain_admission.rs"]
mod admission_tests;
#[cfg(test)]
#[path = "../tests/object_storyboard_chain_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_storyboard_chain_playback.rs"]
mod playback_tests;
#[cfg(test)]
#[path = "../tests/object_storyboard_chain.rs"]
mod tests;
