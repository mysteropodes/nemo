//! Snapshot-bound Motion values, shared by frame geometry and destructive Flatten.
//! This reads the existing admitted property tracks; it never samples a UI cache.
use crate::object_frame_packet::{sample_property_raw, PropertySampleError};
use crate::evaluation::motion_parent_chain::{motion_mat_from_values, MotionMat};
use crate::object_document::PropertyHolderTarget;
use crate::revision::ObjectSnapshot;

pub(crate) const MOTION_2D_KEYS: [&str; 5] = ["position", "anchor", "rotation", "scale", "opacity"];

pub(crate) struct MotionValues {
    pub(crate) position: [f64; 2],
    pub(crate) anchor: [f64; 2],
    pub(crate) rotation: f64,
    pub(crate) scale: [f64; 2],
    pub(crate) opacity: f64,
}

impl ObjectSnapshot {
    /// Raw tracks are the actual producer only when no consumed Motion property
    /// has an enabled expression. Expression owners must supply their evaluated
    /// values rather than silently falling back to the raw track.
    pub(crate) fn sampled_motion_values(
        &self,
        target: &PropertyHolderTarget,
        frame: f64,
    ) -> Result<MotionValues, PropertySampleError> {
        if self.document().property_holders().is_some_and(|holders| {
            holders.iter().any(|holder| {
                holder.target() == target
                    && holder.properties().iter().any(|property| {
                        MOTION_2D_KEYS.contains(&property.property())
                            && property.expression.value().is_some_and(|code| code.enabled)
                    })
            })
        }) {
            return Err(PropertySampleError::Unavailable);
        }
        let sample = |key| sample_property_raw(self, target, key, frame);
        let pair = |key| -> Result<[f64; 2], PropertySampleError> {
            let values = sample(key)?;
            match values.as_slice() {
                [x, y] => Ok([*x, *y]),
                _ => Err(PropertySampleError::Unavailable),
            }
        };
        let scalar = |key| -> Result<f64, PropertySampleError> {
            match sample(key)?.as_slice() {
                [value] => Ok(*value),
                _ => Err(PropertySampleError::Unavailable),
            }
        };
        Ok(MotionValues {
            position: pair("position")?,
            anchor: pair("anchor")?,
            rotation: scalar("rotation")?,
            scale: pair("scale")?,
            opacity: scalar("opacity")?,
        })
    }
    pub(crate) fn sampled_motion_2d(
        &self,
        target: &PropertyHolderTarget,
        frame: f64,
    ) -> Result<Option<MotionMat>, PropertySampleError> {
        let v = self.sampled_motion_values(target, frame)?;
        Ok(motion_mat_from_values(
            v.position, v.anchor, v.rotation, v.scale, v.opacity,
        ))
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_motion_values.rs"]
mod tests;
