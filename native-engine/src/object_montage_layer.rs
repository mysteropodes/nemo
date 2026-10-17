//! Montage layer join (L10-I, lane 10 / #2116; IDs I18, I19, I20, I36, I42).
//!
//! The native consumer of the StoryBoard montage time model (`object_montage_timing`, L10-A) over the
//! ADMITTED records: `StoryboardRecord` and `ComponentSet` (L04-G). Baseline `fbe4b8a3`:
//! `placeMontageAsLayer` (`src/js/storyboard.js`), the `if(ld.montageId){}` branch of
//! `getEffectiveStrokes` and `symbolStrokesAt`'s missing-symbol guard (`src/js/app.js`).
//!
//! - [`MontageLayer`] / [`MontageLayerSet`]: the persisted `montageId` (+ `symPlacedAt`) of a montage
//!   layer as a storage record with its own admission rules. Absent, null and explicit `symPlacedAt`
//!   stay distinct on the wire; a montage whose module was deleted stays representable (the baseline
//!   draws nothing for it and never crashes).
//! - [`ResolvedMontage`]: the real `MontageModules` provider. Dangling ids and non-instance modules
//!   become `ChainSlot::{Dangling, NotInstance}`, a missing component becomes `SymbolView::Missing`.
//! - [`plan_place_montage`]: the gesture as a pure plan (empty refusal first, then "one layer per
//!   montage" focus of the FIRST scene layer carrying the montage, else create). Nothing is minted,
//!   written or published here: the create is the single layer-create commit of the existing owner
//!   plus the record from [`MontagePlacement::bind`].
//! - [`locate_montage_frame`] / [`resolve_montage_layer_frame`]: which source frame of which
//!   component a montage layer shows at a document frame. The component CONTENT of that frame is the
//!   lane 4 evaluation join and enters through [`SymbolFramePort`]; the shipped
//!   [`UnavailableSymbolFrames`] answers a typed `Unavailable`, never a fabricated frame.
//!
//! Documented differences from the baseline, each pinned by a test: retime values and `symPlacedAt`
//! must be exact integers (the baseline accepts fractions and yields meaningless frames); a montage
//! without a name cannot be placed (the baseline names the layer `undefined`); `ensureRetime` never
//! writes its defaults back; an instance module without `symbolId` reads as a missing component.
//!
//! Not here (remaining obligations, see HANDOFF): admission of [`MontageLayerSet`] as an
//! `ObjectDocument` member, the sealed create commit and its history entry, the render/export scene
//! join, and the Time Range gate that every layer passes before this branch.
use crate::evaluation::object_montage_timing::{
    montage_layer_frame, montage_source_frame, montage_total, resolve_chain, ChainMember,
    ChainSlot, LayerFrame, LayerNone, MontageMember, MontageModules, MontageTimingError,
    SourceFrame, SymbolView,
};
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentSet, SCENE_CONTEXT,
};
use crate::object_document::object_component_record_kernels::object_storyboard_records::StoryboardRecord;
use crate::object_document::{object_deserialize, ObjectLayer, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::{HashMap, HashSet};

pub(crate) const MONTAGE_LAYER_SET_FORMAT: &str = "nemo.native-montage-layers";
pub(crate) const MONTAGE_LAYER_SET_FORMAT_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 4096;
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
/// Message keys of the baseline (`SM.t`), printed by the JS adapter.
pub(crate) const KEY_EMPTY_HINT: &str = "toastMontageEmptySnapHint";
pub(crate) const KEY_ALREADY_PLACED: &str = "toastMontageAlreadyPlacedLayer";
pub(crate) const KEY_PLACED: &str = "toastMontagePlacedAsLayerSuffix";

/// A provider that cannot answer says so; it never invents a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderError {
    Unavailable(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MontageLayerError {
    Unsupported(&'static str),
    Invalid(&'static str),
    DuplicateLayer,
    Unresolved(&'static str),
    /// `placeMontageAsLayer` on an id that names no montage module.
    UnknownMontage,
    /// `toastMontageEmptySnapHint`: the chain has no instance member.
    EmptyMontage,
    /// A montage without a name cannot name its layer.
    MissingName,
    /// A retime or placement value that is not an exact integer.
    NonInteger(&'static str),
    Timing(MontageTimingError),
    Provider(ProviderError),
}

impl MontageLayerError {
    /// The baseline toast key of a refusal, `None` when the baseline printed none.
    pub(crate) fn toast_key(&self) -> Option<&'static str> {
        match self {
            Self::EmptyMontage => Some(KEY_EMPTY_HINT),
            _ => None,
        }
    }
}

impl From<MontageTimingError> for MontageLayerError {
    fn from(error: MontageTimingError) -> Self {
        Self::Timing(error)
    }
}

/// `ld.montageId` and `ld.symPlacedAt` of one scene layer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MontageLayer {
    pub(crate) layer_uid: String,
    pub(crate) montage_id: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sym_placed_at: Presence<Option<Number>>,
}
object_deserialize!(MontageLayer {
    layer_uid: String,
    montage_id: String,
    #[serde(default)] sym_placed_at: Presence<Option<Number>>
});

/// Every montage layer of the document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MontageLayerSet {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    pub(crate) layers: Vec<MontageLayer>,
}
object_deserialize!(MontageLayerSet {
    format: String,
    format_version: u32,
    layers: Vec<MontageLayer>
});

/// `v` as an exact integer inside the JavaScript safe range (`None` stays `None`).
fn exact_int(value: Option<f64>, field: &'static str) -> Result<Option<i64>, MontageLayerError> {
    match value {
        None => Ok(None),
        Some(v) if v.is_finite() && v.fract() == 0.0 && v.abs() <= MAX_SAFE => Ok(Some(v as i64)),
        Some(_) => Err(MontageLayerError::NonInteger(field)),
    }
}

impl MontageLayer {
    /// `ld.symPlacedAt || 0`: absent and null read as no placement.
    pub(crate) fn placed_at(&self) -> Result<Option<i64>, MontageLayerError> {
        match self.sym_placed_at.value() {
            Some(Some(number)) => exact_int(number.as_f64(), "symPlacedAt"),
            _ => Ok(None),
        }
    }
}

impl MontageLayerSet {
    pub(crate) fn new(layers: Vec<MontageLayer>) -> Self {
        Self {
            format: MONTAGE_LAYER_SET_FORMAT.to_string(),
            format_version: MONTAGE_LAYER_SET_FORMAT_VERSION,
            layers,
        }
    }
    /// A layer must be a scene layer and never also a component instance (the baseline's
    /// first-match-wins branches are mutually exclusive by construction). Several layers may carry
    /// the same montage (duplicate layer copies `montageId`); a montage id that names no montage is
    /// admitted, exactly as the baseline draws nothing for it.
    pub(crate) fn validate(
        &self,
        components: &ComponentSet,
        scene_layers: &[ObjectLayer],
    ) -> Result<(), MontageLayerError> {
        if self.format != MONTAGE_LAYER_SET_FORMAT
            || self.format_version != MONTAGE_LAYER_SET_FORMAT_VERSION
        {
            return Err(MontageLayerError::Unsupported(
                "unsupported montage layer set format or formatVersion",
            ));
        }
        if self.layers.len() > MAX_ENTRIES {
            return Err(MontageLayerError::Invalid("too many montage layers"));
        }
        let mut seen = HashSet::new();
        for layer in &self.layers {
            if layer.layer_uid.trim().is_empty() || layer.montage_id.trim().is_empty() {
                return Err(MontageLayerError::Invalid(
                    "layerUid and montageId must be non-empty",
                ));
            }
            if !seen.insert(layer.layer_uid.as_str()) {
                return Err(MontageLayerError::DuplicateLayer);
            }
            if !scene_layers
                .iter()
                .any(|l| l.layer_uid() == layer.layer_uid)
            {
                return Err(MontageLayerError::Unresolved(
                    "montage layer is not a scene layer",
                ));
            }
            if components
                .instance(SCENE_CONTEXT, &layer.layer_uid)
                .is_some()
            {
                return Err(MontageLayerError::Invalid(
                    "a layer is a montage layer or a component instance, never both",
                ));
            }
            layer.placed_at()?;
        }
        Ok(())
    }
    pub(crate) fn layer(&self, layer_uid: &str) -> Option<&MontageLayer> {
        self.layers.iter().find(|l| l.layer_uid == layer_uid)
    }
}

/// The chain slots of one montage, looked up by module id.
pub(crate) struct ChainView {
    slots: HashMap<String, ChainSlot>,
}

impl MontageModules for ChainView {
    fn slot(&self, module_id: &str) -> ChainSlot {
        self.slots
            .get(module_id)
            .cloned()
            .unwrap_or(ChainSlot::Dangling)
    }
}

/// `montageById` plus the chain of real module records, ready for the L10-A kernel.
pub(crate) struct ResolvedMontage {
    montage_id: String,
    name: Option<String>,
    chain: Vec<String>,
    view: ChainView,
}

fn symbol_view(components: &ComponentSet, symbol_id: &str) -> SymbolView {
    match components.component(symbol_id) {
        None => SymbolView::Missing,
        // A native component always has totalFrames > 0, so the baseline's layers[0] fallback never
        // applies (`first_layer_frame_count: None` leaves only the 24 default for an unvalidated 0).
        Some(component) => SymbolView::Present {
            name: component.name.value().cloned(),
            total_frames: i64::from(component.total_frames()),
            first_layer_frame_count: None,
        },
    }
}

fn slot_of(
    storyboard: &StoryboardRecord,
    components: &ComponentSet,
    module_id: &str,
) -> Result<ChainSlot, MontageLayerError> {
    match storyboard.module(module_id) {
        None => Ok(ChainSlot::Dangling),
        Some(module) if module.kind() != "instance" => Ok(ChainSlot::NotInstance),
        Some(module) => {
            let symbol_id = module.symbol_id().unwrap_or("").to_string();
            Ok(ChainSlot::Instance(MontageMember {
                trim_in: exact_int(module.trim_in(), "trimIn")?,
                trim_out: exact_int(module.trim_out(), "trimOut")?,
                duration: exact_int(module.duration(), "duration")?,
                symbol: symbol_view(components, &symbol_id),
                symbol_id,
            }))
        }
    }
}

impl ResolvedMontage {
    /// `Ok(None)` is the baseline's `montageById` null (no module, or a module of another type).
    pub(crate) fn resolve(
        storyboard: &StoryboardRecord,
        components: &ComponentSet,
        montage_id: &str,
    ) -> Result<Option<Self>, MontageLayerError> {
        let montage = match storyboard
            .modules()
            .iter()
            .find(|m| m.kind() == "montage" && m.id() == montage_id)
        {
            Some(montage) => montage,
            None => return Ok(None),
        };
        let chain: Vec<String> = montage.chain().to_vec();
        let mut slots = HashMap::new();
        for id in &chain {
            if !slots.contains_key(id) {
                slots.insert(id.clone(), slot_of(storyboard, components, id)?);
            }
        }
        Ok(Some(Self {
            montage_id: montage.id().to_string(),
            name: montage.name.value().cloned(),
            chain,
            view: ChainView { slots },
        }))
    }
    pub(crate) fn montage_id(&self) -> &str {
        &self.montage_id
    }
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// `chainMods` + `ensureRetime`: the instance members in chain order.
    pub(crate) fn members(&self) -> Result<Vec<ChainMember>, MontageLayerError> {
        resolve_chain(&self.chain, &self.view).map_err(MontageLayerError::from)
    }
}

/// What the gesture decided; nothing was published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlaceOutcome {
    /// "One layer per montage": select the existing layer (and switch to the Animation 2D view).
    Focus {
        layer_uid: String,
        layer_name: Option<String>,
    },
    Create(MontagePlacement),
}

/// The create plan: the layer is named after the montage and carries `montageId` and no placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MontagePlacement {
    pub(crate) montage_id: String,
    pub(crate) layer_name: String,
    pub(crate) total_frames: i64,
}

impl MontagePlacement {
    /// The record of the layer the owner minted for this placement.
    pub(crate) fn bind(&self, layer_uid: String) -> MontageLayer {
        MontageLayer {
            layer_uid,
            montage_id: self.montage_id.clone(),
            sym_placed_at: Presence::Missing,
        }
    }
}

impl PlaceOutcome {
    /// The toast of the baseline with its template; `t` translates a message key.
    pub(crate) fn toast(&self, t: &dyn Fn(&str) -> String) -> String {
        match self {
            Self::Focus { layer_name, .. } => format!(
                "{}{} \u{bb}",
                t(KEY_ALREADY_PLACED),
                layer_name.as_deref().unwrap_or("undefined")
            ),
            Self::Create(plan) => format!(
                "{}{} \u{bb} ({} images, en boucle)",
                t(KEY_PLACED),
                plan.layer_name,
                plan.total_frames
            ),
        }
    }
}

/// `placeMontageAsLayer`: empty chain first (refusal), then the first scene layer (in layer order)
/// already carrying this montage, else a create plan.
pub(crate) fn plan_place_montage(
    storyboard: &StoryboardRecord,
    components: &ComponentSet,
    placed: &MontageLayerSet,
    scene_layers: &[ObjectLayer],
    montage_id: &str,
) -> Result<PlaceOutcome, MontageLayerError> {
    let montage = ResolvedMontage::resolve(storyboard, components, montage_id)?
        .ok_or(MontageLayerError::UnknownMontage)?;
    let members = montage.members()?;
    if members.is_empty() {
        return Err(MontageLayerError::EmptyMontage);
    }
    for layer in scene_layers {
        let carries = placed
            .layers
            .iter()
            .any(|p| p.layer_uid == layer.layer_uid() && p.montage_id == montage_id);
        if carries {
            return Ok(PlaceOutcome::Focus {
                layer_uid: layer.layer_uid().to_string(),
                layer_name: layer.name().map(str::to_string),
            });
        }
    }
    let layer_name = montage
        .name()
        .ok_or(MontageLayerError::MissingName)?
        .to_string();
    Ok(PlaceOutcome::Create(MontagePlacement {
        montage_id: montage.montage_id().to_string(),
        layer_name,
        total_frames: montage_total(&members)?,
    }))
}

/// Why a montage layer shows nothing at a frame (every case is `[]` in the baseline).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MontageEmpty {
    /// `montageById` null: the module was deleted or is not a montage.
    MissingMontage,
    /// `montageTotal` is zero.
    ZeroTotal,
    /// `frame - (symPlacedAt || 0)` is negative.
    BeforePlacement,
    /// No member holds the looped frame (only reachable with negative durations).
    PastChain,
    /// `symbolStrokesAt`'s `if (!sym) return []`.
    MissingSymbol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Located {
    Empty(MontageEmpty),
    Source(SourceFrame),
}

/// The branch of `getEffectiveStrokes` up to (not including) `symbolStrokesAt`.
pub(crate) fn locate_montage_frame(
    layer: &MontageLayer,
    storyboard: &StoryboardRecord,
    components: &ComponentSet,
    frame: i64,
) -> Result<Located, MontageLayerError> {
    let montage = match ResolvedMontage::resolve(storyboard, components, &layer.montage_id)? {
        Some(montage) => montage,
        None => return Ok(Located::Empty(MontageEmpty::MissingMontage)),
    };
    let members = montage.members()?;
    let total = montage_total(&members)?;
    let local = match montage_layer_frame(frame, layer.placed_at()?, total)? {
        LayerFrame::None(LayerNone::ZeroTotal) => {
            return Ok(Located::Empty(MontageEmpty::ZeroTotal))
        }
        LayerFrame::None(LayerNone::BeforePlacement) => {
            return Ok(Located::Empty(MontageEmpty::BeforePlacement))
        }
        LayerFrame::Local(local) => local,
    };
    match montage_source_frame(&members, local)? {
        None => Ok(Located::Empty(MontageEmpty::PastChain)),
        Some(source) if components.component(&source.symbol_id).is_none() => {
            Ok(Located::Empty(MontageEmpty::MissingSymbol))
        }
        Some(source) => Ok(Located::Source(source)),
    }
}

/// The component content provider (lane 4 evaluation join). It receives the request the baseline
/// passes to `symbolStrokesAt` and returns an owned, detached content value.
pub(crate) trait SymbolFramePort {
    type Content;
    fn symbol_frame(&self, symbol_id: &str, src_frame: i64)
        -> Result<Self::Content, ProviderError>;
}

/// The shipped port until the component evaluation join is integrated.
pub(crate) struct UnavailableSymbolFrames;

impl SymbolFramePort for UnavailableSymbolFrames {
    type Content = ();
    fn symbol_frame(&self, _symbol_id: &str, _src_frame: i64) -> Result<(), ProviderError> {
        Err(ProviderError::Unavailable(
            "component frame content: the lane 4 evaluation join is not integrated",
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MontageFrame<C> {
    Empty(MontageEmpty),
    Content { source: SourceFrame, content: C },
}

/// A montage layer at a document frame: the located source frame plus its component content.
pub(crate) fn resolve_montage_layer_frame<P: SymbolFramePort>(
    layer: &MontageLayer,
    storyboard: &StoryboardRecord,
    components: &ComponentSet,
    frame: i64,
    port: &P,
) -> Result<MontageFrame<P::Content>, MontageLayerError> {
    match locate_montage_frame(layer, storyboard, components, frame)? {
        Located::Empty(why) => Ok(MontageFrame::Empty(why)),
        Located::Source(source) => {
            let content = port
                .symbol_frame(&source.symbol_id, source.src_frame)
                .map_err(MontageLayerError::Provider)?;
            Ok(MontageFrame::Content { source, content })
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_montage_layer_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/object_montage_layer.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_montage_layer_controls.rs"]
mod controls;
