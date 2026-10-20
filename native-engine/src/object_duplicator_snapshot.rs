//! Native duplicator/effector inputs read from one retained document snapshot.
//! The existing placement kernels keep their original call order and defaults.
//! Enabled expressions require their evaluated provider; never substitute raw tracks.
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_layer::{MotionDelta, Sampled};
use crate::evaluation::object_component_kernels::object_effector_layers::{
    MotionPort, Unavailable,
};
use crate::object_document::{Presence, PropertyHolderTarget};
use crate::object_frame_packet::sample_property_raw;
use crate::revision::ObjectSnapshot;

pub(crate) struct SnapshotMotion<'a> {
    snapshot: &'a ObjectSnapshot,
    context_id: &'a str,
    frame: f64,
}

impl<'a> SnapshotMotion<'a> {
    pub(crate) fn new(snapshot: &'a ObjectSnapshot, context_id: &'a str, frame: f64) -> Self {
        Self {
            snapshot,
            context_id,
            frame,
        }
    }

    fn target(&self, layer_uid: &str) -> PropertyHolderTarget {
        PropertyHolderTarget {
            context_id: self.context_id.to_owned(),
            layer_uid: layer_uid.to_owned(),
            stroke_id: Presence::Missing,
        }
    }
}

impl MotionPort for SnapshotMotion<'_> {
    fn value(&self, layer_uid: &str, key: &'static str) -> Result<Sampled, Unavailable> {
        let target = self.target(layer_uid);
        if self
            .snapshot
            .document()
            .property_holders()
            .is_some_and(|holders| {
                holders.iter().any(|holder| {
                    holder.target() == &target
                        && holder.properties().iter().any(|property| {
                            property.property() == key
                                && property
                                    .expression
                                    .value()
                                    .is_some_and(|expression| expression.enabled)
                        })
                })
            })
        {
            return Err(Unavailable::Property(key));
        }
        let values = sample_property_raw(self.snapshot, &target, key, self.frame)
            .map_err(|_| Unavailable::Property(key))?;
        Ok(Some(values.into_iter().map(JsNumber::Value).collect()))
    }

    fn motion_delta(&self, layer_uid: &str) -> Result<Option<MotionDelta>, Unavailable> {
        let context = crate::evaluation::object_component_kernels::object_context_resolver::ContextSource::of_document(self.snapshot.document())
            .context(self.context_id)
            .map_err(|_| Unavailable::LayerMotion)?;
        let layer = context
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .ok_or(Unavailable::LayerMotion)?;
        if layer
            .parent_layer_uid
            .value()
            .is_some_and(|parent| parent.is_some())
        {
            // sampled_motion_2d is a LOCAL pose. Never claim the effector's
            // parent-resolved origin from that local translation alone.
            return Err(Unavailable::LayerMotion);
        }
        // This is the same stored five-property computation as the real frame
        // consumer: neutral Motion stays None, and unsupported live expressions
        // stay unavailable rather than being replaced with an identity pose.
        self.snapshot
            .sampled_motion_2d(&self.target(layer_uid), self.frame)
            .map(|mat| {
                mat.map(|mat| MotionDelta {
                    dx: JsNumber::Value(mat.dx),
                    dy: JsNumber::Value(mat.dy),
                })
            })
            .map_err(|_| Unavailable::LayerMotion)
    }
}

#[cfg(test)]
#[path = "../tests/object_duplicator_snapshot.rs"]
mod tests;
