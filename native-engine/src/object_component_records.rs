//! Native component records (L04-G, I14/I35): the symbol table and the exposed
//! properties of baseline `state.symbols` at fbe4b8a3, plus the instance
//! placement records of the child `object_component_instance`.
//!
//! These are storage DTOs and their admission rules only. They own no live
//! document, evaluator or painter state: playback is `object_component_playback`,
//! the override application is the exposed-property kernel, and admission into
//! `ObjectDocument` (the optional document field, codec, history image and
//! snapshot pin) is leaf L04-I. Inner layers and strokes reuse the existing
//! `ObjectLayer` / `ObjectRecord` shapes with `contextId == symbolId`; their
//! paint, geometry and timing are judged by `object_codec::validate_content`
//! (`complete_contexts = true`) at document admission, not re-implemented here.
//!
//! Nested components are refused. Baseline `enterSymbol` refuses to enter a
//! second symbol and `getEffectiveStrokes` composites `sym.layers` as plain
//! drawing layers (it never recurses into an inner `symbolId`), so an instance
//! inside a component context has no baseline meaning and is not representable.
use crate::object_document::{
    object_deserialize, FrameScopeKind, ObjectLayer, ObjectRecord, Presence,
};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::HashSet;

#[path = "object_component_instance.rs"]
mod instance;
use instance::{bounded_entries, bounded_presence, invalid, valid_id, MAX_ENTRIES};
#[allow(unused_imports)]
pub(crate) use instance::{ComponentInstance, ComponentRecordError, FrameKey, SCENE_CONTEXT};
// T12: a video placement remap is admitted exactly like an instance remap.
pub(crate) use instance::validate_remap;

pub(crate) const COMPONENT_SET_FORMAT: &str = "nemo.native-component-set";
pub(crate) const COMPONENT_SET_FORMAT_VERSION: u32 = 1;
/// Every exposed-property key is `'ep_' + clock.toString(36) + '_' + ...` (expose()).
const EXPOSED_KEY_PREFIX: &str = "ep_";

/// Components and the instance layers that place them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentSet {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    pub(crate) components: Vec<ComponentRecord>,
    pub(crate) instances: Vec<ComponentInstance>,
}
object_deserialize!(ComponentSet {
    format: String, format_version: u32,
    #[serde(deserialize_with = "bounded_entries")] components: Vec<ComponentRecord>,
    #[serde(deserialize_with = "bounded_entries")] instances: Vec<ComponentInstance>
});

/// One `state.symbols[symbolId]`. `cameraKeys`, `markers`, `motionArcs`,
/// `tweenOverrides` and `tweenEasing` of the baseline symbol belong to owners
/// that do not exist natively yet; they are not fields here, so a record that
/// carries one is refused as an unknown member instead of silently dropped.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentRecord {
    pub(crate) symbol_id: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) name: Presence<String>,
    pub(crate) total_frames: u32,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) fps: Presence<Number>,
    pub(crate) layers: Vec<ObjectLayer>,
    pub(crate) objects: Vec<ObjectRecord>,
    /// Absent until the first `expose()` (`if (!sym.exposedProps) sym.exposedProps = []`).
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) exposed_props: Presence<Vec<ExposedProp>>,
}
object_deserialize!(ComponentRecord {
    symbol_id: String, #[serde(default)] name: Presence<String>, total_frames: u32,
    #[serde(default)] fps: Presence<Number>,
    #[serde(deserialize_with = "bounded_entries")] layers: Vec<ObjectLayer>,
    objects: Vec<ObjectRecord>,
    #[serde(default, deserialize_with = "bounded_presence")] exposed_props: Presence<Vec<ExposedProp>>
});

/// `{ key, label, targetStrokeId, targetField, default }` as `expose()` pushes it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExposedProp {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) target_stroke_id: String,
    pub(crate) target_field: String,
    #[serde(rename = "default", skip_serializing_if = "Presence::missing")]
    pub(crate) default_value: Presence<Number>,
}
object_deserialize!(ExposedProp {
    key: String, label: String, target_stroke_id: String, target_field: String,
    #[serde(default, rename = "default")] default_value: Presence<Number>
});

impl ComponentSet {
    /// Everything this table can decide alone plus the scene layers it places
    /// instances on. A failure leaves nothing to publish: the caller holds the
    /// table by value until this returns `Ok`.
    pub(crate) fn validate(
        &self,
        scene_layers: &[ObjectLayer],
    ) -> Result<(), ComponentRecordError> {
        if self.format != COMPONENT_SET_FORMAT
            || self.format_version != COMPONENT_SET_FORMAT_VERSION
        {
            return Err(ComponentRecordError::Unsupported(
                "unsupported component set format or formatVersion",
            ));
        }
        if self.components.len() > MAX_ENTRIES || self.instances.len() > MAX_ENTRIES {
            return Err(invalid("too many component records"));
        }
        let mut symbols = HashSet::new();
        for component in &self.components {
            component.validate()?;
            if !symbols.insert(component.symbol_id.as_str()) {
                return Err(ComponentRecordError::DuplicateId(
                    "duplicate component symbolId",
                ));
            }
        }
        let mut placed = HashSet::new();
        for instance in &self.instances {
            instance.validate(&symbols, scene_layers)?;
            if !placed.insert((instance.context_id.as_str(), instance.layer_uid.as_str())) {
                return Err(ComponentRecordError::DuplicateId(
                    "duplicate instance layer",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn component(&self, symbol_id: &str) -> Option<&ComponentRecord> {
        self.components.iter().find(|c| c.symbol_id == symbol_id)
    }
    pub(crate) fn instance(&self, context_id: &str, layer_uid: &str) -> Option<&ComponentInstance> {
        self.instances
            .iter()
            .find(|i| i.context_id == context_id && i.layer_uid == layer_uid)
    }
}

impl ComponentRecord {
    fn validate(&self) -> Result<(), ComponentRecordError> {
        if !valid_id(&self.symbol_id) {
            return Err(invalid("component symbolId must be a non-empty identifier"));
        }
        if self.symbol_id == SCENE_CONTEXT {
            return Err(invalid(
                "component symbolId collides with the scene context",
            ));
        }
        if self.total_frames == 0 {
            return Err(invalid("component totalFrames must be positive"));
        }
        if let Some(fps) = self.fps.value() {
            if !fps.as_f64().is_some_and(|v| v.is_finite() && v > 0.0) {
                return Err(invalid("component fps must be a positive finite number"));
            }
        }
        if self.layers.is_empty() || self.layers.len() > MAX_ENTRIES {
            return Err(invalid("a component needs 1..4096 inner layers"));
        }
        let mut layer_ids = HashSet::new();
        for layer in &self.layers {
            if layer.layer_uid().is_empty() {
                return Err(invalid("empty inner layerUid"));
            }
            if !layer_ids.insert(layer.layer_uid()) {
                return Err(ComponentRecordError::DuplicateId(
                    "duplicate inner layerUid",
                ));
            }
        }
        let mut strokes = HashSet::new();
        for object in &self.objects {
            let target = &object.target;
            if target.context_id != self.symbol_id {
                return Err(invalid("object context differs from its component"));
            }
            if !layer_ids.contains(target.layer_uid.as_str()) {
                return Err(ComponentRecordError::Unresolved(
                    "object layer is not a layer of its component",
                ));
            }
            if target.frame_scope.kind != FrameScopeKind::Authored {
                return Err(ComponentRecordError::Unsupported(
                    "component objects are authored-frame records",
                ));
            }
            if target.frame_scope.frame >= self.total_frames || target.stroke_id.is_empty() {
                return Err(invalid("object frame or strokeId is outside the component"));
            }
            strokes.insert(target.stroke_id.as_str());
        }
        let mut keys = HashSet::new();
        for prop in self.exposed_props.value().into_iter().flatten() {
            if prop.key.len() <= EXPOSED_KEY_PREFIX.len()
                || !prop.key.starts_with(EXPOSED_KEY_PREFIX)
                || prop.target_field.is_empty()
            {
                return Err(invalid("exposed property key or targetField is malformed"));
            }
            if !keys.insert(prop.key.as_str()) {
                return Err(ComponentRecordError::DuplicateId(
                    "duplicate exposed property key",
                ));
            }
            if !strokes.contains(prop.target_stroke_id.as_str()) {
                return Err(ComponentRecordError::Unresolved(
                    "exposed property targets a stroke outside its component",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn symbol_id(&self) -> &str {
        &self.symbol_id
    }
    pub(crate) fn total_frames(&self) -> u32 {
        self.total_frames
    }
    pub(crate) fn layers(&self) -> &[ObjectLayer] {
        &self.layers
    }
    pub(crate) fn objects(&self) -> &[ObjectRecord] {
        &self.objects
    }
    /// Empty for both an absent and an empty authored table (use the field to tell them apart).
    pub(crate) fn exposed_props(&self) -> &[ExposedProp] {
        match self.exposed_props.value() {
            Some(props) => props.as_slice(),
            None => &[],
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_component_records_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_component_records.rs"]
mod tests;
