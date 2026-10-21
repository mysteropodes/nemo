//! Lane-7 scene entries joined to the layer-composition and effect-stack plans (L08-K; G03b, G48, G49).
//!
//! One pinned [`PinnedScene`] (the L07-C producer join: contour, component, text, ... entries in the packet's
//! `NumericOrderPlan` order) plus the SAME `ObjectFramePacket` it was joined onto become one immutable
//! [`SceneEffectPlan`]:
//! * every document layer gets a wire slot at its paint-order position, exactly like the baseline layer loop of
//!   `buildSceneJson` (`src/js/engine-bridge.js` at fbe4b8a3): a hidden layer or a widget layer is an EMPTY slot
//!   (no blend, matte or effects), a folder layer wins over the widget flag, an effect layer and a folder layer
//!   never carry their own blend or matte, and only an ordinary layer reads `blendMode` / `matteMode` / mattes;
//! * matte sources, extra mattes and folder children are layer UIDs resolved in the baseline's final passes
//!   (first other layer with that UID; an unresolved first source leaves the index unset so the order planner's
//!   legacy `i + 1` rule applies; an unresolved extra is dropped);
//! * the effective effect stack of a layer is the borrowed stack (`effectsFrom`, ONE hop, first other layer with
//!   that UID) followed by its own, `sceneEffectsOf`; an effect layer plans with the "into" landing, every other
//!   layer with the ping-pong landing (`apply_effect_stack_into` vs `apply_effect_stack`);
//! * the layer plan is `scene_effect_order::plan_composition` over those slots, so a scene that needs none of the
//!   layered features is `CompositionPlan::Flat` and keeps the accepted fast path;
//! * a scene entry that no producer claimed, or that a producer reported unavailable, stays on its layer as a typed
//!   [`EntryOutcome::Unavailable`]; nothing is dropped and no producer may publish a partial scene on top of it.
//!
//! Inputs no record carries yet (item-level effects, canvas-background layers, animated blend and the
//! matte-on track) are not guessed: the adapter supplies the neutral value and a later provider fills the same
//! `JoinLayerInput` field. Folder children come from `parent_layer_uid`, which the codec still refuses, so a native
//! folder has none until layer parenting is admitted. The module owns no pixel, texture or scheduler state.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    PinField, PinnedScene, SceneOutcome, ScenePin, UnavailableCause,
};
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    plan_composition, CompositionPlan, LayerComposition, MatteExtra,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    any_enabled, plan_effect_stack, plan_effect_stack_into, EffectSpec, EffectStackPlan,
};
use crate::object_render_scene::ObjectSceneError;
use std::collections::{BTreeMap, HashMap};

/// Composition-relevant projection of one document layer, in paint order. Flags and strings are the stored values:
/// an absent blend and an authored `"normal"` stay different inputs (both draw as Normal).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JoinLayerInput {
    pub(crate) layer_uid: String,
    pub(crate) source_layer_index: usize,
    pub(crate) visible: bool,
    pub(crate) is_widget: bool,
    pub(crate) is_effect: bool,
    pub(crate) is_folder: bool,
    pub(crate) blend_mode: Option<String>,
    pub(crate) matte_mode: Option<String>,
    pub(crate) matte_source_uid: Option<String>,
    pub(crate) mattes_more: Vec<(String, String)>,
    pub(crate) effects: Vec<EffectSpec>,
    pub(crate) effects_from: Option<String>,
    pub(crate) parent_uid: Option<String>,
    pub(crate) mask_count: usize,
    pub(crate) mask_feather: f64,
    pub(crate) item_effect_enabled: bool,
}

impl JoinLayerInput {
    /// A visible ordinary layer with nothing authored.
    pub(crate) fn plain(layer_uid: &str, source_layer_index: usize) -> Self {
        Self {
            layer_uid: layer_uid.to_owned(),
            source_layer_index,
            visible: true,
            is_widget: false,
            is_effect: false,
            is_folder: false,
            blend_mode: None,
            matte_mode: None,
            matte_source_uid: None,
            mattes_more: Vec::new(),
            effects: Vec::new(),
            effects_from: None,
            parent_uid: None,
            mask_count: 0,
            mask_feather: 0.,
            item_effect_enabled: false,
        }
    }
}

/// What a wire slot is. Decided in the baseline's branch order: hidden, folder, widget, effect, ordinary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotRole {
    Empty,
    Plain,
    Effect,
    Folder,
}

pub(crate) fn slot_role(layer: &JoinLayerInput) -> SlotRole {
    if !layer.visible {
        SlotRole::Empty
    } else if layer.is_folder {
        SlotRole::Folder
    } else if layer.is_widget {
        SlotRole::Empty
    } else if layer.is_effect {
        SlotRole::Effect
    } else {
        SlotRole::Plain
    }
}

/// Caller-owned evaluation parameters of the effect passes.
#[derive(Clone, Copy)]
pub(crate) struct JoinOptions<'a> {
    pub(crate) effect_zoom: f64,
    /// Whether a `custom:` effect type has a registered shader (`run_one_effect`).
    pub(crate) is_registered: &'a dyn Fn(&str) -> bool,
}

impl<'a> JoinOptions<'a> {
    pub(crate) fn new(effect_zoom: f64, is_registered: &'a dyn Fn(&str) -> bool) -> Self {
        Self {
            effect_zoom,
            is_registered,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EntryOutcome {
    Produced { producer: String },
    Unavailable(UnavailableCause),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlannedEntry {
    pub(crate) source_object_index: usize,
    pub(crate) outcome: EntryOutcome,
}

/// One wire slot: its identity, role, effective effect stack and the scene entries painted on it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlannedLayer {
    pub(crate) wire_index: usize,
    pub(crate) source_layer_index: usize,
    pub(crate) layer_uid: String,
    pub(crate) role: SlotRole,
    /// Borrowed stack first, then the own stack; empty for an empty slot.
    pub(crate) effects: Vec<EffectSpec>,
    /// `None` when no entry is enabled (the baseline runs no pass at all).
    pub(crate) stack: Option<EffectStackPlan>,
    pub(crate) entries: Vec<PlannedEntry>,
    pub(crate) mask_count: usize,
    pub(crate) mask_feather: f64,
}

/// Slot-level result of the pure join (no pin, no scene entries).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayerPlans {
    pub(crate) composition: Vec<LayerComposition>,
    pub(crate) plan: CompositionPlan,
    pub(crate) layers: Vec<PlannedLayer>,
}

fn first_other(layers: &[JoinLayerInput], uid: &str, own: usize) -> Option<usize> {
    layers
        .iter()
        .enumerate()
        .find(|(index, layer)| *index != own && layer.layer_uid == uid)
        .map(|(index, _)| index)
}

/// `sceneEffectsOf`: the borrowed stack (the source's OWN entries only, one hop), then this layer's own.
fn effective_effects(layers: &[JoinLayerInput], index: usize) -> Vec<EffectSpec> {
    let layer = &layers[index];
    let mut out = Vec::new();
    if let Some(from) = layer.effects_from.as_deref().filter(|uid| !uid.is_empty()) {
        if let Some(source) = first_other(layers, from, index) {
            out.extend(layers[source].effects.iter().cloned());
        }
    }
    out.extend(layer.effects.iter().cloned());
    out
}

fn folder_children(layers: &[JoinLayerInput], index: usize) -> Vec<usize> {
    let uid = layers[index].layer_uid.as_str();
    if uid.is_empty() {
        return Vec::new();
    }
    let is_child = |layer: &JoinLayerInput| layer.parent_uid.as_deref() == Some(uid);
    (0..layers.len())
        .filter(|&child| child != index && is_child(&layers[child]))
        .collect()
}

fn plain_composition(
    layers: &[JoinLayerInput],
    index: usize,
    effective: &[EffectSpec],
) -> LayerComposition {
    let layer = &layers[index];
    let blend_mode = layer
        .blend_mode
        .clone()
        .filter(|name| !name.is_empty() && name != "normal");
    let matte_mode = layer
        .matte_mode
        .clone()
        .filter(|name| !name.is_empty() && name != "none");
    let mut matte_source_index = None;
    let mut mattes_more = None;
    if matte_mode.is_some() {
        matte_source_index = layer
            .matte_source_uid
            .as_deref()
            .filter(|uid| !uid.is_empty())
            .and_then(|uid| first_other(layers, uid, index));
        let extras: Vec<MatteExtra> = layer
            .mattes_more
            .iter()
            .filter(|(uid, mode)| !uid.is_empty() && !mode.is_empty() && mode != "none")
            .filter_map(|(uid, mode)| {
                first_other(layers, uid, index).map(|source| MatteExtra {
                    mode: Some(mode.clone()),
                    source_index: Some(source),
                })
            })
            .collect();
        if !extras.is_empty() {
            mattes_more = Some(extras);
        }
    }
    LayerComposition {
        blend_mode,
        matte_mode,
        matte_source_index,
        mattes_more,
        layer_effect_enabled: any_enabled(effective),
        item_effect_enabled: layer.item_effect_enabled,
        mask_count: layer.mask_count,
        ..LayerComposition::default()
    }
}

/// The pure join over layers already in paint order. Position in `inputs` is the wire index.
pub(crate) fn plan_layers(inputs: &[JoinLayerInput], options: &JoinOptions<'_>) -> LayerPlans {
    let mut composition = Vec::with_capacity(inputs.len());
    let mut layers = Vec::with_capacity(inputs.len());
    for (index, layer) in inputs.iter().enumerate() {
        let role = slot_role(layer);
        let effects = match role {
            SlotRole::Empty => Vec::new(),
            _ => effective_effects(inputs, index),
        };
        composition.push(match role {
            SlotRole::Empty => LayerComposition::default(),
            SlotRole::Effect => LayerComposition {
                is_effect_layer: Some(true),
                layer_effect_enabled: any_enabled(&effects),
                ..LayerComposition::default()
            },
            SlotRole::Folder => LayerComposition {
                is_folder_layer: Some(true),
                folder_child_indices: Some(folder_children(inputs, index)),
                layer_effect_enabled: any_enabled(&effects),
                ..LayerComposition::default()
            },
            SlotRole::Plain => plain_composition(inputs, index, &effects),
        });
        let stack = if !any_enabled(&effects) {
            None
        } else if role == SlotRole::Effect {
            Some(plan_effect_stack_into(
                &effects,
                options.effect_zoom,
                options.is_registered,
            ))
        } else {
            Some(plan_effect_stack(
                &effects,
                options.effect_zoom,
                options.is_registered,
            ))
        };
        layers.push(PlannedLayer {
            wire_index: index,
            source_layer_index: layer.source_layer_index,
            layer_uid: layer.layer_uid.clone(),
            role,
            effects,
            stack,
            entries: Vec::new(),
            mask_count: if role == SlotRole::Plain {
                layer.mask_count
            } else {
                0
            },
            mask_feather: if role == SlotRole::Plain {
                layer.mask_feather
            } else {
                0.
            },
        });
    }
    let plan = plan_composition(&composition);
    LayerPlans {
        composition,
        plan,
        layers,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneEffectJoinError {
    /// The scene was joined onto a different instance/document/revision/snapshot/context/scope/frame.
    WrongPin(PinField),
    MaskInputs(ObjectSceneError),
    /// A v1 or unsupported-scope packet has no paint order; none is invented.
    NoNumericOrder,
    /// The order plan does not list every document layer exactly once, or an entry names a foreign layer.
    InvalidOrder,
    /// Stored keys need their actual parameter sampler; static substitution is forbidden.
    EffectParametersUnavailable {
        layer: usize,
    },
}

#[path = "scene_effect_scene_join_inputs.rs"]
mod inputs;
pub(crate) use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::inputs::layer_inputs;

/// The immutable join of one packet, one pinned scene and the layer records: what the viewport and the export
/// both read, so a frame has one composition plan whichever door asks for it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SceneEffectPlan {
    pin: ScenePin,
    resources: BTreeMap<String, String>,
    plans: LayerPlans,
}

impl SceneEffectPlan {
    pub(crate) fn pin(&self) -> &ScenePin {
        &self.pin
    }
    pub(crate) fn is_for_packet(&self, packet: &ObjectFramePacket) -> bool {
        self.pin == ScenePin::of_packet(packet)
    }
    pub(crate) fn composition(&self) -> &[LayerComposition] {
        &self.plans.composition
    }
    pub(crate) fn plan(&self) -> &CompositionPlan {
        &self.plans.plan
    }
    pub(crate) fn layers(&self) -> &[PlannedLayer] {
        &self.plans.layers
    }
    /// Input of `EvaluationKey::new(.., declared_resource_versions)` and of the resource-set check.
    pub(crate) fn declared_resource_versions(&self) -> &BTreeMap<String, String> {
        &self.resources
    }
    pub(crate) fn wire_index_of_source(&self, source_layer_index: usize) -> Option<usize> {
        self.plans
            .layers
            .iter()
            .position(|layer| layer.source_layer_index == source_layer_index)
    }
    /// `(source layer index, source object index)` of the first entry no producer drew, in paint order.
    pub(crate) fn first_unavailable(&self) -> Option<(usize, usize)> {
        self.plans.layers.iter().find_map(|layer| {
            layer
                .entries
                .iter()
                .find(|entry| matches!(entry.outcome, EntryOutcome::Unavailable(_)))
                .map(|entry| (layer.source_layer_index, entry.source_object_index))
        })
    }
    pub(crate) fn is_complete(&self) -> bool {
        self.first_unavailable().is_none()
    }
    /// Every producer that drew at least one entry, sorted and de-duplicated.
    pub(crate) fn producers(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .plans
            .layers
            .iter()
            .flat_map(|layer| layer.entries.iter())
            .filter_map(|entry| match &entry.outcome {
                EntryOutcome::Produced { producer } => Some(producer.clone()),
                EntryOutcome::Unavailable(_) => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

/// Join the pinned scene and the layer records of its packet. Refuses a scene joined onto another pin before
/// anything is read; the result owns clones only.
pub(crate) fn plan_scene_effects<T>(
    packet: &ObjectFramePacket,
    scene: &PinnedScene<T>,
    options: &JoinOptions<'_>,
) -> Result<SceneEffectPlan, SceneEffectJoinError> {
    let pin = ScenePin::of_packet(packet);
    if let Some(field) = pin.first_mismatch(scene.pin()) {
        return Err(SceneEffectJoinError::WrongPin(field));
    }
    let inputs = layer_inputs(packet)?;
    let mut plans = plan_layers(&inputs, options);
    let position: HashMap<usize, usize> = plans
        .layers
        .iter()
        .map(|layer| (layer.source_layer_index, layer.wire_index))
        .collect();
    for entry in scene.entries() {
        let slot = *position
            .get(&entry.source_layer_index())
            .ok_or(SceneEffectJoinError::InvalidOrder)?;
        let outcome = match entry.outcome() {
            SceneOutcome::Item { producer, .. } => EntryOutcome::Produced {
                producer: producer.clone(),
            },
            SceneOutcome::Unavailable(cause) => EntryOutcome::Unavailable(cause.clone()),
        };
        plans.layers[slot].entries.push(PlannedEntry {
            source_object_index: entry.source_object_index(),
            outcome,
        });
    }
    Ok(SceneEffectPlan {
        pin,
        resources: scene.declared_resource_versions().clone(),
        plans,
    })
}

#[path = "scene_effect_scene_join_resources.rs"]
mod resources;
#[allow(unused_imports)]
pub(crate) use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::resources::{
    bind_scene_resources, BindResourcesError,
};

#[cfg(test)]
#[path = "../tests/scene_effect_scene_join_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/scene_effect_scene_join_docs.rs"]
mod docs;
#[cfg(test)]
#[path = "../tests/scene_effect_scene_join_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/scene_effect_scene_join.rs"]
mod tests;
