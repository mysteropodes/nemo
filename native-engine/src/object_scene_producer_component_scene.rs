//! Pinned layer-slot join of the component scene producer (L07-K; D32, I16a).
//!
//! A component instance has no packet record of its own: the placing layer holds no
//! strokes, its picture comes from the component. So the claim of this producer is the
//! LAYER, keyed by `source_layer_index` of one `ObjectFramePacket`, and its place in
//! the scene is the slot of that layer in the packet's `NumericOrderPlan`, the same
//! plan the L07-C object join walks:
//! * `ComponentProduction::join` refuses a production whose pin differs from the packet
//!   (or from the caller's expected pin) in any of the seven `ScenePin` fields, refuses
//!   a claim on a layer that is not a visible layer of the packet, and a layer claimed
//!   twice; slots are returned in plan order, never in production order;
//! * `ComponentScene::merge` interleaves those slots with an L07-C `PinnedScene`
//!   of the same packet, layer by layer in plan order (the component slot first, then
//!   the object entries of that layer), so every producer shares one snapshot, frame
//!   and ordering. A pin mismatch between the two scenes is refused.
//!
//! A held blank, an empty inner frame and a typed unavailability are three different
//! outcomes and all three keep their slot. Invisible layers are the only omission,
//! the rule of `prepare_object_paint_plan`.

use crate::evaluation::object_component_kernels::object_component_playback::ResolvedFrame;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component::ResolvedComponentPlacement;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::{
    ComponentShape, ComponentUnavailable,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    PinField, PinnedScene, SceneEntry, ScenePin,
};
use crate::object_document::ObjectTarget;
use crate::object_frame_packet::ObjectFramePacket;
use std::collections::HashMap;

/// Producer name used in `WrongPin`.
pub(crate) const PRODUCER: &str = "component";

/// The detached shapes of one instance at one inner frame (possibly none: the
/// resolved frame names no frame of the component, as `frames[ii]` undefined).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacedComponent {
    pub(crate) inner_frame: ResolvedFrame,
    pub(crate) shapes: Vec<ComponentShape>,
    /// Selected record targets the exposed `__visible` override dropped (reported by the
    /// override evaluator itself, never re-derived by a consumer).
    pub(crate) hidden: Vec<ObjectTarget>,
    /// The camera -> symMatrix chain the shapes went through; `None` when no inner frame resolved.
    pub(crate) placement: Option<ResolvedComponentPlacement>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentOutcome {
    /// A held `blankOverride` key hides the instance: reported, not a missing producer.
    Blank,
    Shapes(PlacedComponent),
    Unavailable(ComponentUnavailable),
}

/// The claim on one placing layer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComponentLayerEntry {
    pub(crate) source_layer_index: usize,
    pub(crate) layer_uid: String,
    pub(crate) symbol_id: String,
    pub(crate) outcome: ComponentOutcome,
}

/// Everything the component producer reports for one pinned frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComponentProduction {
    pub(crate) pin: ScenePin,
    pub(crate) entries: Vec<ComponentLayerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ComponentJoinError {
    /// `producer: None` is the packet disagreeing with the caller's expected pin.
    WrongPin {
        producer: Option<String>,
        field: PinField,
    },
    /// The packet has no numeric order plan: no order is invented.
    NoNumericOrder,
    /// The claim is not on a visible layer of the packet, or names another layer uid.
    UnknownLayer {
        source_layer_index: usize,
    },
    DuplicateClaim {
        source_layer_index: usize,
    },
    /// The order plan and the claimed layers disagree.
    InvalidOrder,
    /// The component scene and the object scene are pinned differently.
    ScenePinMismatch {
        field: PinField,
    },
}

/// One claimed layer at its position in the packet's order plan.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComponentSlot {
    plan_position: usize,
    entry: ComponentLayerEntry,
}

impl ComponentSlot {
    /// Index into `NumericOrderPlan::layers()`.
    pub(crate) fn plan_position(&self) -> usize {
        self.plan_position
    }
    pub(crate) fn entry(&self) -> &ComponentLayerEntry {
        &self.entry
    }
}

/// The joined component slots, back to front.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComponentScene {
    pin: ScenePin,
    slots: Vec<ComponentSlot>,
}

/// One position of the merged scene.
#[derive(Debug)]
pub(crate) enum SceneSlot<'a, T> {
    Object(&'a SceneEntry<T>),
    Component(&'a ComponentSlot),
}

impl ComponentProduction {
    /// Check the pins, order the claims by the packet plan. Consumes the production;
    /// a refusal returns no partial scene.
    pub(crate) fn join(
        self,
        packet: &ObjectFramePacket,
        expected: &ScenePin,
    ) -> Result<ComponentScene, ComponentJoinError> {
        let pin = ScenePin::of_packet(packet);
        if let Some(field) = pin.first_mismatch(expected) {
            return Err(ComponentJoinError::WrongPin {
                producer: None,
                field,
            });
        }
        if let Some(field) = pin.first_mismatch(&self.pin) {
            return Err(ComponentJoinError::WrongPin {
                producer: Some(PRODUCER.to_owned()),
                field,
            });
        }
        let order = packet
            .numeric_order()
            .ok_or(ComponentJoinError::NoNumericOrder)?;
        let mut claims: HashMap<usize, ComponentLayerEntry> = HashMap::new();
        for entry in self.entries {
            let index = entry.source_layer_index;
            let known = packet
                .source_layers()
                .get(index)
                .is_some_and(|layer| layer.visible() && layer.layer_uid() == entry.layer_uid);
            if !known {
                return Err(ComponentJoinError::UnknownLayer {
                    source_layer_index: index,
                });
            }
            if claims.insert(index, entry).is_some() {
                return Err(ComponentJoinError::DuplicateClaim {
                    source_layer_index: index,
                });
            }
        }
        let mut slots = Vec::with_capacity(claims.len());
        for (plan_position, layer) in order.layers().iter().enumerate() {
            if let Some(entry) = claims.remove(&layer.source_layer_index()) {
                slots.push(ComponentSlot {
                    plan_position,
                    entry,
                });
            }
        }
        if !claims.is_empty() {
            return Err(ComponentJoinError::InvalidOrder);
        }
        Ok(ComponentScene { pin, slots })
    }
}

impl ComponentScene {
    pub(crate) fn pin(&self) -> &ScenePin {
        &self.pin
    }

    /// Back-to-front slots of the claimed layers.
    pub(crate) fn slots(&self) -> &[ComponentSlot] {
        &self.slots
    }

    /// Layers whose instance could not be evaluated (never silently absent).
    pub(crate) fn unavailable(&self) -> impl Iterator<Item = &ComponentSlot> + '_ {
        self.slots
            .iter()
            .filter(|slot| matches!(slot.entry.outcome, ComponentOutcome::Unavailable(_)))
    }

    /// One sequence for the packet: per plan layer, the component slot of the layer
    /// first, then its object entries, with the three pins checked to be identical.
    pub(crate) fn merge<'a, T>(
        &'a self,
        packet: &ObjectFramePacket,
        objects: &'a PinnedScene<T>,
    ) -> Result<Vec<SceneSlot<'a, T>>, ComponentJoinError> {
        let pin = ScenePin::of_packet(packet);
        if let Some(field) = pin.first_mismatch(&self.pin) {
            return Err(ComponentJoinError::WrongPin {
                producer: Some(PRODUCER.to_owned()),
                field,
            });
        }
        if let Some(field) = pin.first_mismatch(objects.pin()) {
            return Err(ComponentJoinError::ScenePinMismatch { field });
        }
        let order = packet
            .numeric_order()
            .ok_or(ComponentJoinError::NoNumericOrder)?;
        let mut components = self.slots.iter().peekable();
        let mut entries = objects.entries().iter().peekable();
        let mut merged = Vec::with_capacity(self.slots.len() + objects.entries().len());
        for (position, layer) in order.layers().iter().enumerate() {
            while let Some(slot) = components.next_if(|slot| slot.plan_position == position) {
                merged.push(SceneSlot::Component(slot));
            }
            while let Some(entry) =
                entries.next_if(|entry| entry.source_layer_index() == layer.source_layer_index())
            {
                merged.push(SceneSlot::Object(entry));
            }
        }
        if components.next().is_some() || entries.next().is_some() {
            return Err(ComponentJoinError::InvalidOrder);
        }
        Ok(merged)
    }
}
