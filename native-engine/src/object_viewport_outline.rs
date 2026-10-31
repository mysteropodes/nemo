//! #2372: the viewport-only "Current frame outline" display mode, painted by the engine for the presented frame.
//! Baseline 1065e9d8 (engine-bridge `includeEditorOverlays && state.currentFrameOutline`, oracle `mode currentFrameOutline on`):
//! each vector item drops its fill (and gradient); an item without a stroke gets a 235,235,240 stroke at its own opacity and
//! `max(1 / zoom, 0.75)` world units wide, an authored stroke is kept as it is. Only a viewport witness can carry the mode, so an
//! export (a different witness) never contains it, and the plain viewport's cached frame can never be mistaken for it.
use crate::object_render_scene::{ObjectCompositionWitness, ObjectSceneError};

/// Narrowest outline in artwork units (baseline `Math.max(1 / view.zoom, 0.75)`).
const MIN_STROKE: f64 = 0.75;

/// The part of the viewport's canonical cache identity that belongs to the outline mode: another display scale is another stroke.
pub fn viewport_outline_quality_suffix(display_scale: f64) -> String {
    format!("+outline:{:016x}", display_scale.to_bits())
}

impl ObjectCompositionWitness {
    /// Draw the items of this VIEWPORT witness as outlines. `display_scale` is screen px per composition px (the canvas element
    /// width over the output width); the witness' own camera / navigation scale completes the baseline `view.zoom`. An export
    /// witness refuses it, and the viewport quality gains the outline identity.
    pub fn with_viewport_outline(mut self, display_scale: f64) -> Result<Self, ObjectSceneError> {
        let zoom = display_scale * self.camera.map_or(1., |m| m[0].hypot(m[1]));
        if !(zoom.is_finite() && zoom > 0.) {
            return Err(ObjectSceneError::UnsupportedGeometry {
                source_object_index: 0,
                reason: "viewport outline zoom unavailable",
            });
        }
        if !self.viewport {
            return Err(ObjectSceneError::WrongSnapshot);
        }
        let base = self
            .viewport_quality
            .take()
            .ok_or(ObjectSceneError::WrongSnapshot)?;
        self.viewport_quality = Some(format!(
            "{base}{}",
            viewport_outline_quality_suffix(display_scale)
        ));
        self.viewport_outline = Some((1. / zoom).max(MIN_STROKE));
        Ok(self)
    }

    /// The outline stroke width in artwork units; `None` for every witness that is not drawn as an outline (every export).
    pub(crate) fn viewport_outline(&self) -> Option<f64> {
        self.viewport_outline
    }
}
