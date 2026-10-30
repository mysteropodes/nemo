//! Owned numeric Motion values from the exact frame producer; no evaluator escapes.
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_layers::MotionPort;
use crate::evaluation::property_projection_3d::Pose3D;
use crate::object_document::{ObjectDocument, PropertyHolder};
use crate::object_frame_packet::{
    MotionValues, ObjectFrameError, ObjectFramePacket, Presence, PropertyHolderTarget,
};

pub(crate) const KEYS: [&str; 5] = ["position", "anchor", "rotation", "scale", "opacity"];
#[derive(Debug, Clone)]
pub(super) struct MotionReceipt {
    target: PropertyHolderTarget,
    values: MotionValues,
    pose: Option<Pose3D>,
}

pub(super) fn capture(
    packet: &ObjectFramePacket,
    motion: &dyn MotionPort,
) -> Result<Vec<MotionReceipt>, ObjectFrameError> {
    let mut needed = std::collections::BTreeSet::new();
    for layer in packet.records().filter(|r| {
        let target = PropertyHolderTarget {context_id: packet.context_id().to_owned(),
            layer_uid: packet.source_layers()[r.source_layer_index()].layer_uid().to_owned(), stroke_id: Presence::Missing};
        crate::evaluation::object_vector_kernels::render_vector_eval::common_layer_motion_target(packet.snapshot().document(), &target)
    }).map(|r| r.source_layer_index()).chain(packet
        .raster_records()
        .map(|r| r.layer_index()))
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
            // The original producer evaluates ordinary2D Motion first, even
            // when3D then suppresses its final affine/opacity paint. Reuse those
            // actual values; never run those guests a second time forPose3D.
            let pose = if target.stroke_id().is_none()
                && packet.source_layers().iter().any(|l| {
                    l.layer_uid() == target.layer_uid() && l.three_d.value() == Some(&true)
                }) {
                Some(Pose3D {
                    position: values.position,
                    anchor: values.anchor,
                    rotation: values.rotation,
                    scale: values.scale,
                    position_z: scalar("positionZ")?,
                    rotation_x: scalar("rotationX")?,
                    rotation_y: scalar("rotationY")?,
                })
            } else {
                None
            };
            Ok(MotionReceipt {
                target,
                values,
                pose,
            })
        })
        .collect()
}

impl ObjectFramePacket {
    pub(crate) fn sampled_pose_3d(
        &self,
        target: &PropertyHolderTarget,
    ) -> Result<Option<Pose3D>, ObjectFrameError> {
        if target.context_id() != self.context_id()
            || target.context_id() != "scene-root"
            || target.stroke_id().is_some()
        {
            return Err(ObjectFrameError::InvalidContext);
        }
        let layer = self
            .source_layers()
            .iter()
            .find(|l| l.layer_uid() == target.layer_uid())
            .ok_or(ObjectFrameError::InvalidSource)?;
        if layer.three_d.value() != Some(&true) {
            return Ok(None);
        }
        match self.motion_receipts.iter().find(|r| &r.target == target) {
            Some(receipt) => receipt.pose.map(Some).ok_or(ObjectFrameError::Unavailable),
            None => self
                .snapshot()
                .sampled_pose_3d(target, f64::from(self.frame()))
                .map_err(|_| ObjectFrameError::Unavailable),
        }
    }
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
    evaluated: bool,
) -> Option<PropertyHolder> {
    let target = holder.target();
    let layer = target.stroke_id().is_none()
        && crate::evaluation::object_vector_kernels::render_vector_eval::motion::scene_consumer_uses_layer(document, target);
    let common =
        crate::evaluation::object_vector_kernels::render_vector_eval::common_layer_motion_target(
            document, target,
        );
    let three_d = common
        && document
            .layers()
            .iter()
            .any(|l| l.layer_uid() == target.layer_uid() && l.three_d.value() == Some(&true));
    if !(layer || common || text_element(document, target)) {
        return None;
    }
    // A mixed visible contour and Text/raster layer
    // may consume only its shared opacity here. Other pose fields must not be
    // admitted on the strength of a Text/raster sibling alone.
    let mixed = layer
        && !crate::evaluation::object_vector_kernels::render_vector_eval::motion::layer_vectors_batchable(document, target)
        && document.layers().iter().any(|l| {
            l.layer_uid() == target.layer_uid()
                && l.visible()
                && document.objects().iter().any(|record| {
                    record.target().context_id() == target.context_id()
                        && record.target().layer_uid() == target.layer_uid()
                })
        });
    let mut remaining = holder.clone();
    remaining.properties.retain(|p| {
        let key = p.property();
        let consumed = KEYS.contains(&key)
            || (three_d && ["positionZ", "rotationX", "rotationY"].contains(&key));
        let owned = consumed && (common || !mixed || key == "opacity");
        !owned || (!evaluated && p.expression.value().is_some_and(|e| e.enabled))
    });
    if remaining.properties.len() == holder.properties().len() {
        return None;
    }
    if evaluated && remaining.properties.is_empty() {
        remaining.controls = Presence::Missing;
    }
    Some(remaining)
}
