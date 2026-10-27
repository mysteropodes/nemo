//! Component evaluated-shape producer join (L07-K; D32, I16a).
//!
//! Resolves the component instances placed on the layers of ONE pinned
//! `ObjectFramePacket` into detached, transformed, typed scene shapes and reports
//! them as a layer-keyed production for the pin registry (`ComponentProduction::join`,
//! `ComponentScene::merge` in `object_scene_producer_component_scene.rs`). Per visible
//! placing layer, in baseline order (`getEffectiveStrokes`, `symbolId` branch):
//!
//! 1. the instance record and its component are looked up by the stable layer uid and
//!    symbol id (an unknown symbol is a typed unavailable slot, not a vanished layer);
//! 2. the held outer key (L04-G `FrameKey`s on the layer's authored timing keys) and
//!    the `sym*` fields feed the L04-A playback kernel: a held blank is `Blank`, an
//!    inner frame that names no frame of the component is an empty `Shapes`;
//! 3. the visible inner layers expose their records at the inner frame
//!    (`object_scene_producer_component_select`), then the declared exposed properties
//!    override fill and visibility through the L04-E kernel (applyOverrides runs on
//!    the selection; fill and visibility never change geometry, so the order against
//!    the matrices is unobservable);
//! 4. the L07-H kernel applies the component camera and then the instance `symMatrix`.
//!
//! The providers that do not exist natively yet are ports, never defaults: the Time
//! Remap sampler and the component camera keys (`ComponentPorts`) and the property
//! runtime of the exposed properties (the L04-E `OverridePort`). An unavailable port
//! makes the slot of the instance a typed unavailable entry. Element and layer Motion,
//! path effects and the inner duplicator are not evaluated: an inner layer with an
//! active path effect is refused, and Motion/duplicator state has no native record
//! in the component table to read.

use crate::evaluation::object_component_kernels::object_component_exposed::ExposedEntry;
use crate::evaluation::object_component_kernels::object_component_exposed_apply::{
    apply_overrides, OverridePort,
};
use crate::evaluation::object_component_kernels::object_component_playback::{
    evaluate_component_playback, nearest_outer_key, PlaybackOutcome,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component::ResolvedComponentPlacement;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentLayerEntry, ComponentOutcome, ComponentProduction, PlacedComponent,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::{
    evaluate_owned_records, playback_input, select_records, sym_matrix, ComponentUnavailable,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use crate::evaluation::property_camera_eval::CameraKey;
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentInstance, ComponentRecord, ComponentRecordError, ComponentSet, SCENE_CONTEXT,
};
use crate::object_document::RangeEvaluator;
use crate::object_document::{ObjectLayer, ObjectRecord, ObjectTarget};
use crate::object_frame_packet::ObjectFramePacket;
use std::borrow::Cow;
use std::collections::HashSet;

/// Providers of values that are not component record data.
pub(crate) trait ComponentPorts {
    /// The sampled Time Remap of an instance that has one (`SMMotion.timeRemapValue`);
    /// `Ok(None)` is the sampler's `null` (the layer plays by its mode).
    fn time_remap(&mut self, layer_uid: &str, frame: u32) -> Result<Option<f64>, &'static str>;
    /// The camera keys of a component (`sym.cameraKeys`); `Ok(None)` or no keys is no camera.
    fn camera_keys(&mut self, symbol_id: &str) -> Result<Option<&[CameraKey]>, &'static str>;
}

/// No Time Remap sampler is wired; the component record has no camera member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoPorts;

impl ComponentPorts for NoPorts {
    fn time_remap(&mut self, _layer_uid: &str, _frame: u32) -> Result<Option<f64>, &'static str> {
        Err("time remap sampler is not wired")
    }
    fn camera_keys(&mut self, _symbol_id: &str) -> Result<Option<&[CameraKey]>, &'static str> {
        Ok(None)
    }
}

/// The component table the instances are read from and the canvas the camera needs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ComponentInputs<'a> {
    pub(crate) components: &'a ComponentSet,
    pub(crate) canvas: (f64, f64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ComponentProduceError {
    /// The packet is not a scene-root packet; instances are placed on scene layers.
    WrongContext,
    /// An instance is not placed on a scene-root layer of this packet.
    UnresolvedInstance {
        context_id: String,
        layer_uid: String,
    },
    /// Two instance records name one layer.
    DuplicateInstance { layer_uid: String },
    /// The table is not admissible against the scene layers of this packet
    /// (`ComponentSet::validate`: formats, component shape, play modes, matrices, frame
    /// keys on timing keys, Time Remap tracks). Instances naming an unknown component
    /// are not part of this check: they stay typed unavailable slots.
    InvalidTable(ComponentRecordError),
}

/// `ComponentSet::validate` against the packet layers, once per production. The checked
/// view leaves out the instances whose component is unknown (typed per-instance slots);
/// the clone is only made on that rare path.
fn check_table(
    components: &ComponentSet,
    layers: &[ObjectLayer],
) -> Result<(), ComponentProduceError> {
    let known = |instance: &ComponentInstance| components.component(instance.symbol_id()).is_some();
    let verdict = if components.instances.iter().all(known) {
        components.validate(layers)
    } else {
        let mut view = components.clone();
        view.instances.retain(known);
        view.validate(layers)
    };
    verdict.map_err(ComponentProduceError::InvalidTable)
}

fn exposed_entries(component: &ComponentRecord) -> Vec<ExposedEntry> {
    component
        .exposed_props()
        .iter()
        .map(|prop| ExposedEntry {
            key: prop.key.clone(),
            label: prop.label.clone(),
            target_stroke_id: prop.target_stroke_id.clone(),
            target_field: prop.target_field.clone(),
            default_value: prop.default_value.clone(),
        })
        .collect()
}

/// `applyOverrides`: nothing declared leaves the selection as is and never asks the
/// port; declared properties need the port, which hands back detached records.
fn apply_exposed_overrides(
    component: &ComponentRecord,
    selected: Vec<ObjectRecord>,
    host: &ObjectLayer,
    frame: u32,
    port: &mut dyn OverridePort,
) -> Result<(Vec<ObjectRecord>, Vec<ObjectTarget>), ComponentUnavailable> {
    let props = exposed_entries(component);
    if props.is_empty() {
        return Ok((selected, Vec::new()));
    }
    match apply_overrides(&props, &selected, host.layer_uid(), frame, port) {
        Err(error) => Err(ComponentUnavailable::Overrides(error)),
        Ok(Cow::Owned(records)) => {
            // `apply_overrides` only ever drops a record for the sticky `__visible` hide.
            let hidden = selected
                .iter()
                .filter(|before| {
                    records
                        .iter()
                        .all(|after| after.target() != before.target())
                })
                .map(|before| before.target().clone())
                .collect();
            Ok((records, hidden))
        }
        Ok(Cow::Borrowed(_)) => Err(ComponentUnavailable::OverridesUnavailable),
    }
}

fn evaluate_instance(
    packet: &ObjectFramePacket,
    inputs: &ComponentInputs<'_>,
    ports: &mut dyn ComponentPorts,
    overrides: &mut dyn OverridePort,
    instance: &ComponentInstance,
    host: &ObjectLayer,
) -> Result<ComponentOutcome, ComponentUnavailable> {
    let frame = packet.frame();
    let unavailable = |_| ComponentUnavailable::RangeProvider {
        layer_uid: host.layer_uid().to_owned(),
    };
    let ranges =
        RangeEvaluator::new(packet.snapshot(), packet.context_id(), frame).map_err(unavailable)?;
    if ranges.gates(host.layer_uid()).map_err(unavailable)?
        && !ranges
            .resolve(host.layer_uid(), true)
            .map_err(unavailable)?
            .contains(frame)
    {
        return Ok(ComponentOutcome::Blank);
    }
    let Some(component) = inputs.components.component(instance.symbol_id()) else {
        return Err(ComponentUnavailable::UnresolvedComponent {
            symbol_id: instance.symbol_id().to_owned(),
        });
    };
    let input = playback_input(instance, host, frame)?;
    // The baseline reads Time Remap only after the blank hold and a held componentFrame
    // have had their say, so a layer they decide never asks the sampler.
    let decided = nearest_outer_key(&input.outer_keys, i64::from(frame))
        .is_some_and(|key| key.blank_override == Some(true) || key.component_frame.is_some());
    let remap = if instance.time_remap().is_some() && !decided {
        ports
            .time_remap(host.layer_uid(), frame)
            .map_err(ComponentUnavailable::TimeRemap)?
    } else {
        None
    };
    let total = component.total_frames();
    let resolved =
        match evaluate_component_playback(&input, f64::from(total), i64::from(frame), remap) {
            PlaybackOutcome::Blank => return Ok(ComponentOutcome::Blank),
            PlaybackOutcome::Frame(resolved) => resolved,
        };
    let Some(inner) = resolved.index(total as usize) else {
        return Ok(ComponentOutcome::Shapes(PlacedComponent {
            inner_frame: resolved,
            shapes: Vec::new(),
            hidden: Vec::new(),
            placement: None,
        }));
    };
    // `index` guarantees inner < total, which is a u32.
    let selected = select_records(component, inner as u32)?;
    let (records, hidden) = apply_exposed_overrides(component, selected, host, frame, overrides)?;
    let sym = sym_matrix(instance)?;
    let keys = ports
        .camera_keys(instance.symbol_id())
        .map_err(ComponentUnavailable::Camera)?;
    let placement = ResolvedComponentPlacement::from_camera_keys(
        resolved,
        sym,
        keys,
        inputs.canvas.0,
        inputs.canvas.1,
    );
    let shapes = evaluate_owned_records(records, &placement)?;
    Ok(ComponentOutcome::Shapes(PlacedComponent {
        inner_frame: resolved,
        shapes,
        hidden,
        placement: Some(placement),
    }))
}

/// One claim per visible placing layer of the packet, in source layer order (a
/// provenance order; the plan order is applied by `ComponentProduction::join`).
pub(crate) fn produce(
    packet: &ObjectFramePacket,
    inputs: &ComponentInputs<'_>,
    ports: &mut dyn ComponentPorts,
    overrides: &mut dyn OverridePort,
) -> Result<ComponentProduction, ComponentProduceError> {
    if packet.context_id() != SCENE_CONTEXT {
        return Err(ComponentProduceError::WrongContext);
    }
    let layers = packet.source_layers();
    let uids: HashSet<&str> = layers.iter().map(ObjectLayer::layer_uid).collect();
    let mut placed = HashSet::new();
    for instance in &inputs.components.instances {
        if instance.context_id() != SCENE_CONTEXT || !uids.contains(instance.layer_uid()) {
            return Err(ComponentProduceError::UnresolvedInstance {
                context_id: instance.context_id().to_owned(),
                layer_uid: instance.layer_uid().to_owned(),
            });
        }
        if !placed.insert(instance.layer_uid()) {
            return Err(ComponentProduceError::DuplicateInstance {
                layer_uid: instance.layer_uid().to_owned(),
            });
        }
    }
    check_table(inputs.components, layers)?;
    let mut entries = Vec::new();
    for (index, host) in layers.iter().enumerate() {
        let Some(instance) = inputs.components.instance(SCENE_CONTEXT, host.layer_uid()) else {
            continue;
        };
        if !host.visible() {
            continue;
        }
        let outcome = evaluate_instance(packet, inputs, ports, overrides, instance, host)
            .unwrap_or_else(ComponentOutcome::Unavailable);
        entries.push(ComponentLayerEntry {
            source_layer_index: index,
            layer_uid: host.layer_uid().to_owned(),
            symbol_id: instance.symbol_id().to_owned(),
            outcome,
        });
    }
    Ok(ComponentProduction {
        pin: ScenePin::of_packet(packet),
        entries,
    })
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_component_join_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component_join_edges.rs"]
mod edges;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component_join_inputs.rs"]
mod inputs;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component_join_overrides.rs"]
mod overrides;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component_join_pins.rs"]
mod pins;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component_join_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component_join_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/object_scene_producer_component_join.rs"]
mod tests;
