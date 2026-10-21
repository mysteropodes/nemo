//! Pinned inline selector receipts; the scene consumes numbers, never a guest.
use crate::evaluation::motion_parent_chain::motion_mat_from_values;
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_layers::MotionPort;
use crate::evaluation::property_text_animator::TextAnimator;
use crate::object_document::object_text_layout_kernels::animator::SELECTOR_KEYS;
use crate::object_document::{Presence, PropertyHolderTarget};
use crate::object_frame_packet::{ObjectFrameError, ObjectFramePacket};
#[derive(Debug, Clone)]
pub(super) struct SelectorReceipt {
    layer: usize,
    animator_id: String,
    key: &'static str,
    value: f64,
}
fn needed_text_layers(
    packet: &ObjectFramePacket,
) -> Result<std::collections::BTreeSet<usize>, ObjectFrameError> {
    let mut needed = packet
        .text_records()
        .map(|r| r.layer_index())
        .collect::<std::collections::BTreeSet<_>>();
    for start in packet
        .text_records()
        .map(|r| r.layer_index())
        .chain(packet.raster_records().map(|r| r.layer_index()))
    {
        let mut next = Some(start);
        let mut seen = std::collections::BTreeSet::new();
        while let Some(index) = next {
            if !seen.insert(index)
                || seen.len() > crate::evaluation::motion_parent_chain::CHAIN_NODE_LIMIT + 1
            {
                return Err(ObjectFrameError::Unavailable);
            }
            let layer = packet
                .source_layers()
                .get(index)
                .ok_or(ObjectFrameError::InvalidSource)?;
            let target = PropertyHolderTarget {
                context_id: packet.context_id().to_owned(),
                layer_uid: layer.layer_uid().to_owned(),
                stroke_id: Presence::Missing,
            };
            let m = packet.sampled_motion_values(&target)?;
            // Exactly SceneParentChains' own-opacity exclusion and bound census.
            if motion_mat_from_values(
                m.position,
                m.anchor,
                m.rotation,
                m.scale,
                if index == start { 100. } else { m.opacity },
            )
            .is_some()
            {
                needed.insert(index);
            }
            next = layer
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
    let actual = packet
        .snapshot()
        .document()
        .item_records()
        .into_iter()
        .flat_map(|i| i.text_entries())
        .filter_map(|entry| {
            let layer = packet
                .source_layers()
                .iter()
                .position(|l| l.layer_uid() == entry.target.layer_uid())?;
            (needed.contains(&layer)
                && entry.target.context_id() == packet.context_id()
                && entry.target.scope_kind() == packet.scope_kind()
                && packet.source_frame_for_layer(layer) == Some(entry.target.frame()))
            .then_some(layer)
        })
        .collect();
    Ok(actual)
}
pub(super) fn capture(
    packet: &ObjectFramePacket,
    motion: &dyn MotionPort,
) -> Result<Vec<SelectorReceipt>, ObjectFrameError> {
    let mut receipts = Vec::new();
    for layer in needed_text_layers(packet)? {
        let source = &packet.source_layers()[layer];
        for animator in source
            .text_animators
            .value()
            .into_iter()
            .flat_map(|r| &r.0)
            .filter(|a| a.enabled.value() != Some(&false))
        {
            for property in animator
                .properties
                .value()
                .into_iter()
                .flatten()
                .filter(|p| p.expression.value().is_some_and(|e| e.enabled))
            {
                let key = *SELECTOR_KEYS
                    .iter()
                    .find(|&&key| key == property.property())
                    .ok_or(ObjectFrameError::Unavailable)?;
                let values = motion
                    .selector_value(source.layer_uid(), &animator.id, key)
                    .map_err(|_| ObjectFrameError::Unavailable)?
                    .ok_or(ObjectFrameError::Unavailable)?;
                let value = match values.as_slice() {
                    [JsNumber::Value(v)] if v.is_finite() => *v,
                    _ => return Err(ObjectFrameError::Unavailable),
                };
                receipts.push(SelectorReceipt {
                    layer,
                    animator_id: animator.id.clone(),
                    key,
                    value,
                });
            }
        }
    }
    Ok(receipts)
}
impl ObjectFramePacket {
    pub(crate) fn resolved_text_animators(
        &self,
        layer: usize,
    ) -> Result<Vec<TextAnimator>, ObjectFrameError> {
        let source = self
            .source_layers()
            .get(layer)
            .ok_or(ObjectFrameError::InvalidSource)?;
        source
            .text_animators
            .value()
            .into_iter()
            .flat_map(|r| &r.0)
            .map(|record| {
                record
                    .resolve_with(f64::from(self.frame()), &mut |key| {
                        self.selector_receipts
                            .iter()
                            .find(|r| {
                                r.layer == layer && r.animator_id == record.id && r.key == key
                            })
                            .map(|r| r.value)
                            .ok_or("text selector expression receipt unavailable")
                    })
                    .map_err(|_| ObjectFrameError::Unavailable)
            })
            .collect()
    }
}
