//! Shared pieces of the lane-4 setup / convert transactions (L04-M, I10/I14/I35):
//! the typed refusal, the detached plan, identity minting and the table helpers
//! every plan uses. Declared as a private child of `object_fill_owner.rs`.
//!
//! A plan is a pure function of one immutable document: it returns a complete
//! detached candidate (the input is never touched) or one typed refusal, never
//! a partial result. Storage admission of the candidate, the pin against the
//! origin snapshot, history and replay belong to the sealed owner
//! (`object_component_convert_owner.rs`) and `NativeObjectHistory`.
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentInstance, ComponentSet, COMPONENT_SET_FORMAT, COMPONENT_SET_FORMAT_VERSION,
    SCENE_CONTEXT,
};
use crate::object_document::object_component_record_kernels::object_duplicator_records::{
    DuplicatorSet, DUPLICATOR_SET_FORMAT, DUPLICATOR_SET_FORMAT_VERSION,
};
use crate::object_document::object_component_record_kernels::object_storyboard_records::{
    StoryboardModule, StoryboardPan, StoryboardRecord, STORYBOARD_FORMAT, STORYBOARD_FORMAT_VERSION,
};
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    LayerEditPolicy, LayerOp, LifecycleError, PolicyRefusal, MAX_REVISION,
};
use crate::object_document::{
    AuthoredOrder, ComponentTables, ObjectDocument, ObjectLayer, OrderLayer, Presence,
};
use serde_json::Number;
use std::collections::HashSet;

/// `badComponentSourceReason(l)` of `app.js`: the layer kinds whose "content" is
/// not the plain frames a component clones. Variant order is the baseline test order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceRefusal {
    Component,
    LfsGroup,
    Montage,
    NativeVideo,
    Null,
    Effect,
    Guide,
    Widget,
}

impl SourceRefusal {
    /// The baseline reason text, byte for byte.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Component => "un composant",
            Self::LfsGroup => "un groupe Ligne/Plein/Ombre",
            Self::Montage => "un calque de montage StoryBoard",
            Self::NativeVideo => "un calque vidéo",
            Self::Null => "un calque Null",
            Self::Effect => "un calque d'effet",
            Self::Guide => "un calque Guide",
            Self::Widget => "un calque Widget",
        }
    }
    /// `'Impossible de convertir : ' + reason`, the single-layer toast.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Component => "Impossible de convertir : un composant",
            Self::LfsGroup => "Impossible de convertir : un groupe Ligne/Plein/Ombre",
            Self::Montage => "Impossible de convertir : un calque de montage StoryBoard",
            Self::NativeVideo => "Impossible de convertir : un calque vidéo",
            Self::Null => "Impossible de convertir : un calque Null",
            Self::Effect => "Impossible de convertir : un calque d'effet",
            Self::Guide => "Impossible de convertir : un calque Guide",
            Self::Widget => "Impossible de convertir : un calque Widget",
        }
    }
}

/// Typed refusal of a plan. Nothing here falls back to a default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SetupError {
    /// A v1 document cannot name a component context.
    V1Document,
    /// The request targets a component context (`toastCloseComponentFirst`).
    NestedContext(String),
    UnknownLayer(String),
    UnknownComponent(String),
    /// The layer carries no duplicator descriptor.
    NotADuplicator(String),
    NoLayers,
    /// An object duplicator needs a pool of at least two layers.
    TooFewLayers,
    BadSource {
        layer_uid: String,
        refusal: SourceRefusal,
    },
    /// The baseline names the component `ld.name + ' (Comp)'`; a never-named layer has no name.
    Unnamed(String),
    /// Motion holders of a converted layer cannot land in a component yet (no native owner).
    HoldersUnavailable {
        layer_uid: String,
    },
    /// Per-element Motion would point at strokes that moved into the component.
    ElementHoldersUnavailable {
        layer_uid: String,
    },
    /// Only authored-frame records can enter a component.
    ObjectScope {
        layer_uid: String,
    },
    DuplicateKey(String),
    PathSelf(String),
    PathChained(String),
    PathUnknown(String),
    SeedRange,
    IdUnavailable,
    RevisionOutOfRange,
    TooMany,
    Lifecycle(LifecycleError),
    Order(crate::commands::OrderEditError),
}

/// A detached candidate plus what the caller needs to report it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SetupPlan {
    pub(crate) document: ObjectDocument,
    /// `false`: the candidate equals the input (no revision is warranted).
    pub(crate) changed: bool,
    pub(crate) symbol_id: Option<String>,
    /// The instance / host layer the request is about.
    pub(crate) layer_uid: Option<String>,
    /// A layer the plan created (merge, place, object duplicator).
    pub(crate) created: Option<String>,
}

impl SetupPlan {
    pub(crate) fn unchanged(document: &ObjectDocument, symbol_id: Option<&str>) -> Self {
        Self {
            document: document.clone(),
            changed: false,
            symbol_id: symbol_id.map(str::to_owned),
            layer_uid: None,
            created: None,
        }
    }
}

/// The baseline never gates conversion on a lock, so the lifecycle kernels
/// (reorder / delete) are handed a policy that permits every decision.
pub(crate) struct PermitAll;

impl LayerEditPolicy for PermitAll {
    fn permit(&self, _op: LayerOp, _layer: &ObjectLayer) -> Result<(), PolicyRefusal> {
        Ok(())
    }
}

pub(crate) fn require_v2(document: &ObjectDocument) -> Result<(), SetupError> {
    if document.format_version() == 2 {
        Ok(())
    } else {
        Err(SetupError::V1Document)
    }
}

/// Take the slot by value, edit it (creating it with `make` when it was never
/// authored) and put it back: absent stays absent until a plan really authors it.
pub(crate) fn update<T, R>(
    slot: &mut Presence<T>,
    make: impl FnOnce() -> T,
    edit: impl FnOnce(&mut T) -> R,
) -> R {
    let mut value = match std::mem::take(slot) {
        Presence::Present(value) => value,
        Presence::Missing => make(),
    };
    let result = edit(&mut value);
    *slot = Presence::Present(value);
    result
}

fn empty_tables() -> ComponentTables {
    ComponentTables {
        components: Presence::Missing,
        storyboard: Presence::Missing,
        duplicators: Presence::Missing,
    }
}
pub(crate) fn empty_component_set() -> ComponentSet {
    ComponentSet {
        format: COMPONENT_SET_FORMAT.to_string(),
        format_version: COMPONENT_SET_FORMAT_VERSION,
        components: Vec::new(),
        instances: Vec::new(),
    }
}
pub(crate) fn empty_duplicator_set() -> DuplicatorSet {
    DuplicatorSet {
        format: DUPLICATOR_SET_FORMAT.to_string(),
        format_version: DUPLICATOR_SET_FORMAT_VERSION,
        duplicators: Vec::new(),
        effector_layers: Vec::new(),
    }
}
/// `sb()` of `storyboard.js`: the lazily created StoryBoard state.
fn default_storyboard() -> StoryboardRecord {
    StoryboardRecord {
        format: STORYBOARD_FORMAT.to_string(),
        format_version: STORYBOARD_FORMAT_VERSION,
        pan: Presence::Present(StoryboardPan {
            x: Number::from(40_u32),
            y: Number::from(40_u32),
        }),
        zoom: Presence::Present(Number::from(1_u32)),
        next_id: 1,
        active_montage_id: Presence::Present(None),
        modules: Vec::new(),
    }
}

/// Edit the `componentTables` member, creating it (with every table still
/// absent) only when the plan authors something into it.
pub(crate) fn with_tables<R>(
    document: &mut ObjectDocument,
    edit: impl FnOnce(&mut ComponentTables) -> Result<R, SetupError>,
) -> Result<R, SetupError> {
    update(&mut document.component_tables, empty_tables, edit)
}

/// `addInstanceAuto(symbolId)`: an `instance` module at the staggered default
/// grid slot, id `'sbm' + nextId++`.
pub(crate) fn add_instance_module(
    tables: &mut ComponentTables,
    symbol_id: &str,
) -> Result<(), SetupError> {
    update(
        &mut tables.storyboard,
        default_storyboard,
        |record| -> Result<(), SetupError> {
            let count = record
                .modules
                .iter()
                .filter(|module| module.kind == "instance")
                .count() as u64;
            let id = format!("sbm{}", record.next_id);
            record.next_id = record.next_id.checked_add(1).ok_or(SetupError::TooMany)?;
            record.modules.push(StoryboardModule {
                id,
                kind: "instance".to_string(),
                symbol_id: Presence::Present(symbol_id.to_string()),
                name: Presence::Missing,
                x: Number::from(40 + (count % 6) * 140),
                y: Number::from(40 + (count / 6) * 160),
                trim_in: Presence::Missing,
                trim_out: Presence::Missing,
                duration: Presence::Missing,
                playhead: Presence::Missing,
                chain: Presence::Missing,
                audio: Presence::Missing,
                asset_id: Presence::Missing,
                in_sec: Presence::Missing,
                out_sec: Presence::Missing,
                cursor_sec: Presence::Missing,
            });
            Ok(())
        },
    )
}

/// The instance stamp `convertLayerToComponent` / `placeComponentInstance` write:
/// `symPlayMode 'once'`, speed 1, placed at 0, single frame 0, nothing else.
pub(crate) fn new_instance(layer_uid: &str, symbol_id: &str) -> ComponentInstance {
    ComponentInstance {
        context_id: SCENE_CONTEXT.to_string(),
        layer_uid: layer_uid.to_string(),
        symbol_id: symbol_id.to_string(),
        sym_play_mode: Presence::Present(Some("once".to_string())),
        sym_speed: Presence::Present(Some(Number::from(1_u32))),
        sym_placed_at: Presence::Present(Some(Number::from(0_u32))),
        sym_single_frame: Presence::Present(Some(Number::from(0_u32))),
        sym_trim_in: Presence::Missing,
        sym_trim_out: Presence::Missing,
        sym_matrix: Presence::Missing,
        time_remap: Presence::Missing,
        frame_keys: Presence::Missing,
    }
}

/// `native-layer:{revision}:{n}`, the first free suffix, as layer-create picks it.
pub(crate) fn mint_layer_uid(document: &ObjectDocument, next: u64) -> Result<String, SetupError> {
    if next > MAX_REVISION {
        return Err(SetupError::RevisionOutOfRange);
    }
    let taken: HashSet<&str> = document.layers.iter().map(ObjectLayer::layer_uid).collect();
    (0..=taken.len())
        .map(|suffix| format!("native-layer:{next}:{suffix}"))
        .find(|id| !taken.contains(id.as_str()))
        .ok_or(SetupError::IdUnavailable)
}

/// `native-component:{revision}:{n}`, the first free suffix among the symbols.
pub(crate) fn mint_symbol_id(document: &ObjectDocument, next: u64) -> Result<String, SetupError> {
    if next > MAX_REVISION {
        return Err(SetupError::RevisionOutOfRange);
    }
    let taken: HashSet<&str> = document
        .component_set()
        .map(|set| {
            set.components
                .iter()
                .map(|c| c.symbol_id.as_str())
                .collect()
        })
        .unwrap_or_default();
    (0..=taken.len())
        .map(|suffix| format!("native-component:{next}:{suffix}"))
        .find(|id| !taken.contains(id.as_str()))
        .ok_or(SetupError::IdUnavailable)
}

/// Append a layer on top of the stack and give it the next natural slot in every
/// order-plan context, exactly as layer-create does.
pub(crate) fn append_layer(
    document: &mut ObjectDocument,
    layer: ObjectLayer,
) -> Result<(), SetupError> {
    let slot = u32::try_from(document.layers.len()).map_err(|_| SetupError::TooMany)?;
    if let Presence::Present(plan) = &mut document.composition_order {
        for context in &mut plan.contexts {
            context.layers.push(OrderLayer {
                layer_uid: layer.layer_uid.clone(),
                natural_position: slot,
                order: AuthoredOrder {
                    static_value: Presence::Missing,
                    track: Presence::Missing,
                    expression: Presence::Missing,
                },
            });
        }
    }
    document.layers.push(layer);
    Ok(())
}

pub(crate) fn layer_index(document: &ObjectDocument, uid: &str) -> Result<usize, SetupError> {
    document
        .layers
        .iter()
        .position(|layer| layer.layer_uid == uid)
        .ok_or_else(|| SetupError::UnknownLayer(uid.to_string()))
}

/// Motion holders of a scene layer: `(layer level, per element)`.
pub(crate) fn holders_on(document: &ObjectDocument, uid: &str) -> (bool, bool) {
    let mut layer_level = false;
    let mut element_level = false;
    for holder in document.property_holders().unwrap_or(&[]) {
        if holder.target.layer_uid != uid || holder.target.context_id != SCENE_CONTEXT {
            continue;
        }
        if holder.target.stroke_id.value().is_some() {
            element_level = true;
        } else {
            layer_level = true;
        }
    }
    (layer_level, element_level)
}
