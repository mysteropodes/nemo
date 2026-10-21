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

#[path = "object_duplicator_follow_path.rs"]
mod follow_path;

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

    /// Shared by strict stored Motion and the application-owned evaluated
    /// local pose. FollowPath never calls back into application evaluation.
    pub(crate) fn motion_delta_from_local(
        &self,
        layer_uid: &str,
        local: Option<crate::evaluation::motion_parent_chain::MotionMat>,
    ) -> Result<Option<MotionDelta>, Unavailable> {
        let mat =
            follow_path::compose(self.snapshot, self.context_id, self.frame, layer_uid, local)?;
        Ok(mat.map(|mat| MotionDelta {
            dx: JsNumber::Value(mat.dx),
            dy: JsNumber::Value(mat.dy),
        }))
    }

    fn target(&self, layer_uid: &str) -> PropertyHolderTarget {
        PropertyHolderTarget {
            context_id: self.context_id.to_owned(),
            layer_uid: layer_uid.to_owned(),
            stroke_id: Presence::Missing,
        }
    }
}

impl SnapshotMotion<'_> {
    fn target_value(
        &self,
        target: &PropertyHolderTarget,
        key: &'static str,
    ) -> Result<Sampled, Unavailable> {
        if self
            .snapshot
            .document()
            .property_holders()
            .is_some_and(|holders| {
                holders.iter().any(|holder| {
                    holder.target() == target
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
        let values = sample_property_raw(self.snapshot, target, key, self.frame)
            .map_err(|_| Unavailable::Property(key))?;
        Ok(Some(values.into_iter().map(JsNumber::Value).collect()))
    }
}

impl MotionPort for SnapshotMotion<'_> {
    fn value(&self, layer_uid: &str, key: &'static str) -> Result<Sampled, Unavailable> {
        let target = self.target(layer_uid);
        self.target_value(&target, key)
    }

    fn element_value(
        &self,
        layer_uid: &str,
        stroke_id: &str,
        key: &'static str,
    ) -> Result<Sampled, Unavailable> {
        let mut target = self.target(layer_uid);
        target.stroke_id = Presence::Present(stroke_id.to_owned());
        self.target_value(&target, key)
    }

    fn motion_delta(&self, layer_uid: &str) -> Result<Option<MotionDelta>, Unavailable> {
        let context = crate::evaluation::object_component_kernels::object_context_resolver::ContextSource::of_document(self.snapshot.document())
            .context(self.context_id)
            .map_err(|_| Unavailable::LayerMotion)?;
        context.layer(layer_uid).ok_or(Unavailable::LayerMotion)?;
        // layerMotionAt is local computeMotionMat plus its optional FollowPath;
        // the effector's own ancestor pose is deliberately not composed here.
        let local = self
            .snapshot
            .sampled_motion_2d(&self.target(layer_uid), self.frame)
            .map_err(|_| Unavailable::LayerMotion)?;
        self.motion_delta_from_local(layer_uid, local)
    }
}

#[cfg(test)]
#[path = "../tests/object_duplicator_snapshot.rs"]
mod tests;
