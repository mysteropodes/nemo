//! The production Flatten port reads the same retained snapshot the edit seals.
use crate::evaluation::motion_parent_chain::{transform_segments, MotionMat, Segment};
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    ElementMotionSource, MotionUnavailable,
};
use crate::object_document::object_group_kernels::object_boolean_ops::{
    ElementMat, MultiPolygon, RingSeg, MAX_ABS_COORD,
};
use crate::object_document::object_group_kernels::object_boolean_topology::Polygon;
use crate::object_document::object_group_kernels::object_fill_trace_ops::native::{
    path, polygon, ring,
};
use crate::object_document::{ObjectRecord, ObjectTarget, Presence, PropertyHolderTarget};
use crate::revision::ObjectSnapshot;
use kurbo::Shape;

pub(super) struct SnapshotMotion<'a> {
    pub(super) snapshot: &'a ObjectSnapshot,
    pub(super) frame: u32,
}

impl SnapshotMotion<'_> {
    fn motion(&self, target: &ObjectTarget) -> Result<Option<MotionMat>, MotionUnavailable> {
        let holder_target = PropertyHolderTarget {
            context_id: target.context_id.clone(),
            layer_uid: target.layer_uid.clone(),
            stroke_id: Presence::Present(target.stroke_id.clone()),
        };
        // Flatten removes the original holder. Refuse any authored input whose
        // consumer has not been joined, rather than losing its visible effect.
        if self
            .snapshot
            .document()
            .property_holders()
            .is_some_and(|holders| {
                holders.iter().any(|holder| {
                    holder.target() == &holder_target
                        && (holder
                            .controls
                            .value()
                            .is_some_and(|controls| !controls.is_empty())
                            || holder.properties().iter().any(|property| {
                                !matches!(
                                    property.property(),
                                    "position" | "anchor" | "rotation" | "scale" | "opacity"
                                )
                            }))
                })
            })
        {
            return Err(MotionUnavailable);
        }
        let motion = self
            .snapshot
            .sampled_motion_2d(&holder_target, f64::from(self.frame))
            .map_err(|_| MotionUnavailable)?;
        // Original elementMotionBakedClone never consumes elMat.op. Do not
        // erase non-neutral alpha as if geometry materialization had baked it.
        if motion.is_some_and(|m| m.op != 1.) {
            return Err(MotionUnavailable);
        }
        Ok(motion)
    }
}
impl ElementMotionSource for SnapshotMotion<'_> {
    fn consumes_authored_motion(&self) -> bool {
        true
    }
    fn element_mat(&self, target: &ObjectTarget) -> Result<Option<ElementMat>, MotionUnavailable> {
        Ok(self.motion(target)?.map(|m| ElementMat {
            dx: m.dx,
            dy: m.dy,
            rot: m.rot,
            sx: m.sx,
            sy: m.sy,
            ax: m.ax,
            ay: m.ay,
        }))
    }
    fn materialized_polygons(
        &self,
        record: &ObjectRecord,
    ) -> Result<MultiPolygon, MotionUnavailable> {
        if record.solid_fill().is_none() {
            return Err(MotionUnavailable);
        }
        let mut input = ring(record);
        if let Some(m) = self.motion(&record.target)? {
            let b = path(&input, record.geometry.closed).bounding_box();
            let pivot = [b.center().x + m.ax, b.center().y + m.ay];
            let segments: Vec<_> = input
                .iter()
                .map(|s| Segment {
                    point: s.point,
                    handle_in: Some(s.handle_in),
                    handle_out: Some(s.handle_out),
                })
                .collect();
            input = transform_segments(&segments, pivot, &m)
                .into_iter()
                .map(|s| RingSeg {
                    point: s.point,
                    handle_in: s.handle_in.unwrap_or([0., 0.]),
                    handle_out: s.handle_out.unwrap_or([0., 0.]),
                })
                .collect();
        }
        // Refuse overflow before asking the adaptive provider to subdivide it.
        if input
            .iter()
            .flat_map(|s| s.point.into_iter().chain(s.handle_in).chain(s.handle_out))
            .any(|v| !v.is_finite() || v.abs() > MAX_ABS_COORD)
        {
            return Err(MotionUnavailable);
        }
        // Open fills close with a straight chord, never the unused closing
        // cubic handles. polygon(false) retains precisely that open contour;
        // the polygon consumer supplies the implicit straight closing edge.
        Ok(vec![Polygon {
            exterior: polygon(&input, record.geometry.closed),
            holes: Vec::new(),
        }])
    }
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_boolean_frame_motion.rs"]
mod tests;
