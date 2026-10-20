//! The production Flatten port reads the same retained snapshot the edit seals.
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    ElementMotionSource, MotionUnavailable,
};
use crate::object_document::object_group_kernels::object_boolean_ops::ElementMat;
use crate::object_document::{ObjectTarget, Presence, PropertyHolderTarget};
use crate::revision::ObjectSnapshot;

pub(super) struct SnapshotMotion<'a> {
    pub(super) snapshot: &'a ObjectSnapshot,
    pub(super) frame: u32,
}

impl ElementMotionSource for SnapshotMotion<'_> {
    fn consumes_authored_motion(&self) -> bool { true }
    fn element_mat(&self, target: &ObjectTarget) -> Result<Option<ElementMat>, MotionUnavailable> {
        let holder_target = PropertyHolderTarget {
            context_id: target.context_id.clone(),
            layer_uid: target.layer_uid.clone(),
            stroke_id: Presence::Present(target.stroke_id.clone()),
        };
        // Flatten removes the original holder. Refuse any authored input whose
        // consumer has not been joined, rather than losing its visible effect.
        if self.snapshot.document().property_holders().is_some_and(|holders| {
            holders.iter().any(|holder| {
                holder.target() == &holder_target
                    && (holder.controls.value().is_some_and(|controls| !controls.is_empty())
                    || holder.properties().iter().any(|property| {
                        !matches!(property.property(), "position" | "anchor" | "rotation" | "scale" | "opacity")
                    }))
            })
        }) {
            return Err(MotionUnavailable);
        }
        self.snapshot
            .sampled_motion_2d(&holder_target, f64::from(self.frame))
            .map(|motion| motion.map(|m| ElementMat {
                dx: m.dx, dy: m.dy, rot: m.rot, sx: m.sx, sy: m.sy, ax: m.ax, ay: m.ay,
            }))
            .map_err(|_| MotionUnavailable)
    }
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_boolean_frame_motion.rs"]
mod tests;
