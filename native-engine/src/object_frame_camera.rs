//! Stored camera/3D inputs joined to the existing numerical kernels at one snapshot.
use super::{sample_property_raw, PropertySampleError};
use crate::evaluation::property_camera_eval::{camera_matrix_at_frame, CameraKey};
use crate::evaluation::property_projection_3d::Pose3D;
use crate::object_document::PropertyHolderTarget;
use crate::revision::ObjectSnapshot;

impl ObjectSnapshot {
    /// Until the common scene producer applies the returned pose, do not admit
    /// a frame that would silently ignore authored camera or 3D projection.
    pub(crate) fn frame_needs_camera_or_3d_join(&self) -> bool {
        self.project().is_some_and(|project| {
            project.project().camera_layer_on() == Some(true)
                && project.project().camera_keys().is_some_and(|keys| !keys.is_empty())
        }) || self.document().layers().iter().any(|layer| layer.visible() && layer.three_d.value() == Some(&true))
    }
    /// Camera pose independent of the viewport's session-only cameraView switch.
    /// Export and camera-locked preview select this same stored-camera provider.
    pub(crate) fn sampled_camera_matrix(&self, frame: f64) -> Result<Option<[f64;6]>, PropertySampleError> {
        if !frame.is_finite() { return Err(PropertySampleError::InvalidFrame) }
        let Some(project) = self.project().map(|p| p.project()) else { return Ok(None) };
        if project.camera_layer_on() != Some(true) { return Ok(None) }
        let keys = project.camera_keys().map(|keys| keys.iter().map(|key| CameraKey {
            frame:key.frame, x:key.x, y:key.y, w:key.w,
            rot:key.rot.value().copied(), h_out:key.h_out.value().copied(),
            h_in:key.h_in.value().copied(), ease:key.ease.value().copied(),
        }).collect::<Vec<_>>());
        let (width,height) = project.canvas_dimensions();
        let matrix = camera_matrix_at_frame(keys.as_deref(), frame, f64::from(width), f64::from(height));
        if matrix.is_some_and(|matrix| matrix.iter().any(|n| !n.is_finite())) {
            return Err(PropertySampleError::Unavailable);
        }
        Ok(matrix)
    }

    /// The persisted threeD flag gates the stored Z/X/Y tracks. Merely having
    /// an old 3D key does not activate projection on an ordinary 2D layer.
    pub(crate) fn sampled_pose_3d(&self, target:&PropertyHolderTarget, frame:f64) -> Result<Option<Pose3D>,PropertySampleError> {
        if !frame.is_finite() { return Err(PropertySampleError::InvalidFrame) }
        if target.stroke_id().is_some() { return Err(PropertySampleError::UnsupportedProperty) }
        let layer = self.document().layers().iter().find(|layer| layer.layer_uid()==target.layer_uid())
            .ok_or(PropertySampleError::MissingTarget)?;
        // Validate the full context even when the flag is off.
        sample_property_raw(self,target,"positionZ",frame)?;
        if layer.three_d.value() != Some(&true) { return Ok(None) }
        let keys = ["positionZ","rotationX","rotationY"];
        if self.document().property_holders().is_some_and(|holders| holders.iter().any(|holder|
            holder.target()==target && holder.properties().iter().any(|property|
                keys.contains(&property.property()) && property.expression.value().is_some_and(|e|e.enabled)))) {
            return Err(PropertySampleError::Unavailable);
        }
        let m = self.sampled_motion_values(target,frame)?;
        let scalar = |key| match sample_property_raw(self,target,key,frame)?.as_slice() {
            [value] => Ok(*value), _ => Err(PropertySampleError::Unavailable)
        };
        Ok(Some(Pose3D {
            position:m.position,
            anchor:m.anchor,
            rotation:m.rotation,
            scale:m.scale,
            position_z:scalar("positionZ")?, rotation_x:scalar("rotationX")?, rotation_y:scalar("rotationY")?,
        }))
    }
}

#[cfg(all(test,feature="history",feature="project_media_codec"))]
#[path = "../tests/object_frame_camera.rs"]
mod tests;
