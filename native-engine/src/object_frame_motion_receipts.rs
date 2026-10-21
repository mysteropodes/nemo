//! Owned numeric Motion values from the exact frame producer; no evaluator escapes.
use super::{MotionValues, ObjectFrameError, ObjectFramePacket, Presence, PropertyHolderTarget};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_layers::MotionPort;
use crate::object_document::{ObjectDocument, PropertyHolder};

pub(crate) const KEYS: [&str; 5] = ["position", "anchor", "rotation", "scale", "opacity"];
#[derive(Debug, Clone)]
pub(super) struct MotionReceipt {
    target: PropertyHolderTarget,
    values: MotionValues,
}

pub(super) fn capture(
    packet: &ObjectFramePacket,
    motion: &dyn MotionPort,
) -> Result<Vec<MotionReceipt>, ObjectFrameError> {
    let mut needed = std::collections::BTreeSet::new();
    for layer in packet
        .raster_records()
        .map(|r| r.layer_index())
        .chain(packet.text_records().map(|r| r.layer_index()))
    {
        let mut next = Some(layer);
        let mut walk = std::collections::BTreeSet::new();
        while let Some(index) = next {
            if !walk.insert(index)
                || walk.len() > crate::evaluation::motion_parent_chain::CHAIN_NODE_LIMIT + 1
            {
                return Err(ObjectFrameError::Unavailable);
            }
            needed.insert(index);
            let source = packet
                .source_layers()
                .get(index)
                .ok_or(ObjectFrameError::InvalidSource)?;
            next = source
                .parent_layer_uid
                .value()
                .and_then(Option::as_deref)
                .map(|uid| {
                    packet
                        .source_layers()
                        .iter()
                        .position(|l| l.layer_uid() == uid)
                        .ok_or(ObjectFrameError::InvalidSource)
                })
                .transpose()?;
        }
    }
    let mut targets = needed
        .into_iter()
        .map(|layer| PropertyHolderTarget {
            context_id: packet.context_id().to_owned(),
            layer_uid: packet.source_layers()[layer].layer_uid().to_owned(),
            stroke_id: Presence::Missing,
        })
        .collect::<Vec<_>>();
    // Hidden ancestor Text used for a real pivot must use the same numeric source.
    if let Some(items) = packet.snapshot().document().item_records() {
        for entry in items.text_entries() {
            let target = &entry.target;
            let Some(layer) = packet
                .source_layers()
                .iter()
                .position(|l| l.layer_uid() == target.layer_uid())
            else {
                continue;
            };
            if target.context_id() == packet.context_id()
                && target.scope_kind() == packet.scope_kind()
                && packet.source_frame_for_layer(layer) == Some(target.frame())
                && targets.iter().any(|t| t.layer_uid() == target.layer_uid())
            {
                targets.push(PropertyHolderTarget {
                    context_id: packet.context_id().to_owned(),
                    layer_uid: target.layer_uid().to_owned(),
                    stroke_id: Presence::Present(target.stroke_id().to_owned()),
                });
            }
        }
    }
    targets
        .into_iter()
        .map(|target| {
            let read = |key: &'static str| -> Result<Vec<f64>, ObjectFrameError> {
                let values = match target.stroke_id() {
                    Some(stroke) => motion.element_value(target.layer_uid(), stroke, key),
                    None => motion.value(target.layer_uid(), key),
                }
                .map_err(|_| ObjectFrameError::Unavailable)?
                .ok_or(ObjectFrameError::Unavailable)?;
                values
                    .into_iter()
                    .map(|v| match v {
                        JsNumber::Value(v) if v.is_finite() => Ok(v),
                        _ => Err(ObjectFrameError::Unavailable),
                    })
                    .collect()
            };
            let pair = |key| match read(key)?.as_slice() {
                [x, y] => Ok([*x, *y]),
                _ => Err(ObjectFrameError::Unavailable),
            };
            let scalar = |key| match read(key)?.as_slice() {
                [v] => Ok(*v),
                _ => Err(ObjectFrameError::Unavailable),
            };
            let values = MotionValues {
                position: pair("position")?,
                anchor: pair("anchor")?,
                rotation: scalar("rotation")?,
                scale: pair("scale")?,
                opacity: scalar("opacity")?,
            };
            Ok(MotionReceipt { target, values })
        })
        .collect()
}

impl ObjectFramePacket {
    /// The receipt is private and inseparable from this packet's snapshot/frame.
    /// Strict raw packets continue to refuse enabled expressions.
    pub(crate) fn sampled_motion_values(
        &self,
        target: &PropertyHolderTarget,
    ) -> Result<MotionValues, ObjectFrameError> {
        if target.context_id() != self.context_id() {
            return Err(ObjectFrameError::InvalidContext);
        }
        self.motion_receipts
            .iter()
            .find(|r| &r.target == target)
            .map(|r| Ok(r.values))
            .unwrap_or_else(|| {
                self.snapshot()
                    .sampled_motion_values(target, f64::from(self.frame()))
                    .map_err(|_| ObjectFrameError::Unavailable)
            })
    }
}

pub(crate) fn text_element(document: &ObjectDocument, target: &PropertyHolderTarget) -> bool {
    target.stroke_id().is_some_and(|stroke| {
        document.item_records().is_some_and(|items| {
            items.text_entries().iter().any(|entry| {
                entry.target.context_id() == target.context_id()
                    && entry.target.layer_uid() == target.layer_uid()
                    && entry.target.stroke_id() == stroke
            })
        })
    })
}

/// Remove only values the actual owner port captures for real Scene consumers.
pub(crate) fn remaining_holder(
    document: &ObjectDocument,
    holder: &PropertyHolder,
) -> Option<PropertyHolder> {
    let target = holder.target();
    let layer = target.stroke_id().is_none()
        && crate::evaluation::object_vector_kernels::render_vector_eval::motion::scene_consumer_uses_layer(document, target);
    if !(layer || text_element(document, target)) {
        return None;
    }
    // Until the vector batch consumer is joined, a mixed visible contour layer
    // may consume only its shared opacity here. Other pose fields must not be
    // admitted on the strength of a Text/raster sibling alone.
    let mixed = layer
        && document.layers().iter().any(|l| {
            l.layer_uid() == target.layer_uid()
                && l.visible()
                && document.objects().iter().any(|record| {
                    record.target().context_id() == target.context_id()
                        && record.target().layer_uid() == target.layer_uid()
                })
        });
    let mut remaining = holder.clone();
    remaining
        .properties
        .retain(|p| !KEYS.contains(&p.property()) || (mixed && p.property() != "opacity"));
    if remaining.properties.len() == holder.properties().len() {
        return None;
    }
    if remaining.properties.is_empty() {
        remaining.controls = Presence::Missing;
    }
    Some(remaining)
}
