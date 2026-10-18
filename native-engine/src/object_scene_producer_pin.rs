//! Pinned multi-producer scene join (L07-C; D32, G03a).
//!
//! Every scene producer (contour, component, text, rig, mesh, brush, ...) reports
//! per source record, keyed by `source_object_index` of ONE admitted
//! `ObjectFramePacket`. This join is the only place where those reports become a
//! scene sequence:
//! * all producers must carry the packet's pin (`ScenePin`: instance, document,
//!   content revision, snapshot, context, scope, frame) or the whole join is refused;
//! * the sequence is the packet `NumericOrderPlan` (layers, then elements), never
//!   producer encounter order and never array position;
//! * a selected record that no producer claimed, or that a producer reported as
//!   unavailable, stays in the sequence as a typed unavailable entry;
//! * every selected record is claimed at most once; indices outside the packet's
//!   selected records are refused;
//! * declared `(resource id, version)` pairs merge into one map that is exactly the
//!   `declared_resource_versions` input of the scheduler's `EvaluationKey::new`.
//!
//! The payload `T` is opaque to the join: it holds no renderer, document or host
//! state. Invisible layers are the only intentional omission, the same rule as
//! `prepare_object_paint_plan` (their records are not in the packet's selection).
//! Nothing here mutates the packet or a payload; the join consumes its outputs by
//! value, so a refusal returns no partial scene.

use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::ObjectFramePacket;
use std::collections::{BTreeMap, HashMap, HashSet};

/// The pin every producer output and the joined scene share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScenePin {
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) document_snapshot_id: String,
    pub(crate) context_id: String,
    pub(crate) scope_kind: FrameScopeKind,
    pub(crate) frame: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinField {
    Instance,
    Document,
    Revision,
    Snapshot,
    Context,
    Scope,
    Frame,
}

impl ScenePin {
    pub(crate) fn of_packet(packet: &ObjectFramePacket) -> Self {
        Self {
            instance_id: packet.instance_id().to_owned(),
            document_id: packet.document_id().to_owned(),
            content_revision: packet.content_revision(),
            document_snapshot_id: packet.document_snapshot_id().to_owned(),
            context_id: packet.context_id().to_owned(),
            scope_kind: packet.scope_kind().clone(),
            frame: packet.frame(),
        }
    }

    /// First differing field in the fixed order instance, document, revision,
    /// snapshot, context, scope, frame; `None` when the pins are identical.
    pub(crate) fn first_mismatch(&self, other: &Self) -> Option<PinField> {
        if self.instance_id != other.instance_id {
            Some(PinField::Instance)
        } else if self.document_id != other.document_id {
            Some(PinField::Document)
        } else if self.content_revision != other.content_revision {
            Some(PinField::Revision)
        } else if self.document_snapshot_id != other.document_snapshot_id {
            Some(PinField::Snapshot)
        } else if self.context_id != other.context_id {
            Some(PinField::Context)
        } else if self.scope_kind != other.scope_kind {
            Some(PinField::Scope)
        } else if self.frame != other.frame {
            Some(PinField::Frame)
        } else {
            None
        }
    }
}

/// One producer claim on one selected source record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProducerEntry<T> {
    Item {
        source_object_index: usize,
        payload: T,
    },
    Unavailable {
        source_object_index: usize,
        reason: String,
    },
}

/// Everything one producer reports for one pinned frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProducerOutput<T> {
    pub(crate) producer: String,
    pub(crate) pin: ScenePin,
    pub(crate) entries: Vec<ProducerEntry<T>>,
    pub(crate) resources: Vec<(String, String)>,
}

impl<T> ProducerOutput<T> {
    pub(crate) fn new(producer: impl Into<String>, pin: ScenePin) -> Self {
        Self {
            producer: producer.into(),
            pin,
            entries: Vec::new(),
            resources: Vec::new(),
        }
    }

    pub(crate) fn with_item(mut self, source_object_index: usize, payload: T) -> Self {
        self.entries.push(ProducerEntry::Item {
            source_object_index,
            payload,
        });
        self
    }

    pub(crate) fn with_unavailable(
        mut self,
        source_object_index: usize,
        reason: impl Into<String>,
    ) -> Self {
        self.entries.push(ProducerEntry::Unavailable {
            source_object_index,
            reason: reason.into(),
        });
        self
    }

    pub(crate) fn with_resource(
        mut self,
        resource_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        self.resources.push((resource_id.into(), version.into()));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnavailableCause {
    /// A producer claimed the record and reported it unavailable.
    Reported { producer: String, reason: String },
    /// No producer claimed the record: it is kept, never dropped.
    Unclaimed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SceneOutcome<T> {
    Item { producer: String, payload: T },
    Unavailable(UnavailableCause),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SceneEntry<T> {
    source_layer_index: usize,
    source_object_index: usize,
    outcome: SceneOutcome<T>,
}

impl<T> SceneEntry<T> {
    pub(crate) fn source_layer_index(&self) -> usize {
        self.source_layer_index
    }
    pub(crate) fn source_object_index(&self) -> usize {
        self.source_object_index
    }
    pub(crate) fn outcome(&self) -> &SceneOutcome<T> {
        &self.outcome
    }
}

/// The joined scene: one entry per selected record, back to front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PinnedScene<T> {
    pin: ScenePin,
    entries: Vec<SceneEntry<T>>,
    resources: BTreeMap<String, String>,
}

impl<T> PinnedScene<T> {
    pub(crate) fn pin(&self) -> &ScenePin {
        &self.pin
    }
    /// Back-to-front sequence from the packet's `NumericOrderPlan`.
    pub(crate) fn entries(&self) -> &[SceneEntry<T>] {
        &self.entries
    }
    /// Input of `EvaluationKey::new(.., declared_resource_versions)` (an
    /// `IntoIterator<Item = (String, String)>`); ids unique, ids/versions non-empty.
    pub(crate) fn declared_resource_versions(&self) -> &BTreeMap<String, String> {
        &self.resources
    }
    pub(crate) fn unavailable(&self) -> impl Iterator<Item = &SceneEntry<T>> + '_ {
        self.entries
            .iter()
            .filter(|entry| matches!(entry.outcome, SceneOutcome::Unavailable(_)))
    }
    /// True when every selected record has a producer item.
    pub(crate) fn is_complete(&self) -> bool {
        self.unavailable().next().is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SceneJoinError {
    /// `producer: None` is the packet disagreeing with the caller's expected pin.
    WrongPin {
        producer: Option<String>,
        field: PinField,
    },
    /// The packet has no numeric order plan (v1 or unsupported scope): no order
    /// is invented.
    NoNumericOrder,
    EmptyProducerName,
    DuplicateProducer {
        producer: String,
    },
    /// The index is not a selected record of the packet (outside it, or in an
    /// invisible layer).
    UnknownSource {
        producer: String,
        source_object_index: usize,
    },
    DuplicateClaim {
        source_object_index: usize,
    },
    /// A declared resource id or version is empty.
    InvalidResource {
        producer: String,
    },
    /// Two declarations of one resource id carry different versions.
    ResourceConflict {
        resource_id: String,
    },
    /// The order plan and the packet's selected records disagree.
    InvalidOrder,
}

/// Join producer outputs onto one packet. Checks run output by output in the
/// given order (name, duplicate name, pin, resources, entries); the first failure
/// is returned and nothing is published.
pub(crate) fn join_scene<T>(
    packet: &ObjectFramePacket,
    expected: &ScenePin,
    outputs: Vec<ProducerOutput<T>>,
) -> Result<PinnedScene<T>, SceneJoinError> {
    let pin = ScenePin::of_packet(packet);
    if let Some(field) = pin.first_mismatch(expected) {
        return Err(SceneJoinError::WrongPin {
            producer: None,
            field,
        });
    }
    let order = packet
        .numeric_order()
        .ok_or(SceneJoinError::NoNumericOrder)?;
    // Selected provenance: source object index -> source layer index.
    let selected: HashMap<usize, usize> = packet.selected_source_indices().collect();
    let mut claims: HashMap<usize, SceneOutcome<T>> = HashMap::new();
    let mut names = HashSet::new();
    let mut resources: BTreeMap<String, String> = BTreeMap::new();
    for output in outputs {
        let ProducerOutput {
            producer,
            pin: output_pin,
            entries,
            resources: declared,
        } = output;
        if producer.is_empty() {
            return Err(SceneJoinError::EmptyProducerName);
        }
        if !names.insert(producer.clone()) {
            return Err(SceneJoinError::DuplicateProducer { producer });
        }
        if let Some(field) = pin.first_mismatch(&output_pin) {
            return Err(SceneJoinError::WrongPin {
                producer: Some(producer),
                field,
            });
        }
        for (resource_id, version) in declared {
            if resource_id.is_empty() || version.is_empty() {
                return Err(SceneJoinError::InvalidResource { producer });
            }
            let conflict = resources
                .get(&resource_id)
                .is_some_and(|existing| *existing != version);
            if conflict {
                return Err(SceneJoinError::ResourceConflict { resource_id });
            }
            resources.insert(resource_id, version);
        }
        for entry in entries {
            let (index, outcome) = match entry {
                ProducerEntry::Item {
                    source_object_index,
                    payload,
                } => (
                    source_object_index,
                    SceneOutcome::Item {
                        producer: producer.clone(),
                        payload,
                    },
                ),
                ProducerEntry::Unavailable {
                    source_object_index,
                    reason,
                } => (
                    source_object_index,
                    SceneOutcome::Unavailable(UnavailableCause::Reported {
                        producer: producer.clone(),
                        reason,
                    }),
                ),
            };
            if !selected.contains_key(&index) {
                return Err(SceneJoinError::UnknownSource {
                    producer,
                    source_object_index: index,
                });
            }
            if claims.insert(index, outcome).is_some() {
                return Err(SceneJoinError::DuplicateClaim {
                    source_object_index: index,
                });
            }
        }
    }
    let mut entries = Vec::with_capacity(selected.len());
    let mut seen = HashSet::new();
    let mut layers = HashSet::new();
    for layer in order.layers() {
        let layer_index = layer.source_layer_index();
        let Some(source) = packet.source_layers().get(layer_index) else {
            return Err(SceneJoinError::InvalidOrder);
        };
        if !layers.insert(layer_index) {
            return Err(SceneJoinError::InvalidOrder);
        }
        if !source.visible() {
            continue;
        }
        for element in layer.elements() {
            let object_index = element.source_object_index();
            if selected.get(&object_index) != Some(&layer_index) || !seen.insert(object_index) {
                return Err(SceneJoinError::InvalidOrder);
            }
            let outcome = claims
                .remove(&object_index)
                .unwrap_or(SceneOutcome::Unavailable(UnavailableCause::Unclaimed));
            entries.push(SceneEntry {
                source_layer_index: layer_index,
                source_object_index: object_index,
                outcome,
            });
        }
    }
    if seen.len() != selected.len() || !claims.is_empty() {
        return Err(SceneJoinError::InvalidOrder);
    }
    Ok(PinnedScene {
        pin,
        entries,
        resources,
    })
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_pin_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_pin_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_pin.rs"]
mod tests;
