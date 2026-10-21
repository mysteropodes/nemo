//! Sealed viewport-only reference input from the charged native read boundary.
use crate::object_frame_packet::ObjectFrameSelector;
use crate::object_render_scene::{
    validate_pins, ObjectCompositionWitness, ObjectPaintPlan, ObjectSceneError,
};
use crate::read_queries::{visible_reference, ReferencePixels};
use std::sync::Arc;
use vello::{
    kurbo::Affine,
    peniko::{Blob, ImageAlphaType, ImageData, ImageFormat},
};

#[derive(Debug, Clone)]
pub(super) struct ReferenceScene {
    pixels: ReferencePixels,
    image: ImageData,
    transform: Affine,
    opacity: f32,
}

/// One canonical quality identity, sealed by this actual viewport producer.
pub fn object_viewport_quality(camera_view: bool, zoom: f64, pan: [f64; 2]) -> String {
    let base = if camera_view { "final-camera-view" } else { "final" };
    if zoom == 1. && pan == [0.,0.] { return base.into(); }
    let [x,y] = pan.map(|n| if n == 0. { 0. } else { n });
    format!("{}:{:016x}:{:016x}:{:016x}", base, zoom.to_bits(), x.to_bits(), y.to_bits())
}

/// The old witness is only a one-entry read cache. Every selector and setting is
/// derived again from the new plan's original immutable snapshot.
pub fn prepare_object_viewport_composition(
    plan: &ObjectPaintPlan,
    expected: &ObjectFrameSelector<'_>,
    previous: Option<&ObjectCompositionWitness>,
) -> Result<ObjectCompositionWitness, ObjectSceneError> {
    prepare_object_navigated_viewport_composition(plan, expected, previous, false, 1., [0.,0.])
}

/// Explicit session-only Camera View. Exports retain their separate stored-project route.
pub fn prepare_object_camera_viewport_composition(
    plan: &ObjectPaintPlan,
    expected: &ObjectFrameSelector<'_>,
    previous: Option<&ObjectCompositionWitness>,
) -> Result<ObjectCompositionWitness, ObjectSceneError> {
    prepare_object_navigated_viewport_composition(plan, expected, previous, true, 1., [0.,0.])
}

pub fn prepare_object_navigated_viewport_composition(
    plan: &ObjectPaintPlan,
    expected: &ObjectFrameSelector<'_>,
    previous: Option<&ObjectCompositionWitness>,
    camera_view: bool,
    zoom: f64,
    pan: [f64; 2],
) -> Result<ObjectCompositionWitness, ObjectSceneError> {
    validate_pins(plan.packet(), expected)?;
    let camera = if camera_view {
        plan.packet().stored_camera_matrix().map_err(|_| ObjectSceneError::UnsupportedGeometry {
            source_object_index: 0, reason: "stored camera transform unavailable",
        })?
    } else { None };
    // Session navigation changes no source geometry, project camera or export.
    let camera = if zoom == 1. && pan == [0.,0.] { camera } else {
        if !zoom.is_finite() || zoom <= 0. || pan.iter().any(|n| !n.is_finite()) {
            return Err(ObjectSceneError::UnsupportedGeometry { source_object_index: 0, reason: "viewport navigation unavailable" });
        }
        let (w,h) = plan.packet().snapshot().project().ok_or(ObjectSceneError::ReferenceUnavailable)?.project().canvas_dimensions();
        let centre = (f64::from(w) * 0.5, f64::from(h) * 0.5);
        let matrix = Affine::translate(centre) * Affine::scale(zoom)
            * Affine::translate((-centre.0,-centre.1))
            * Affine::new(camera.unwrap_or([1.,0.,0.,1.,0.,0.])) * Affine::translate((-pan[0],-pan[1]));
        let values = matrix.as_coeffs(); let determinant = values[0]*values[3]-values[1]*values[2];
        if values.iter().any(|n| !n.is_finite()) || !determinant.is_finite() || determinant == 0. {
            return Err(ObjectSceneError::UnsupportedGeometry { source_object_index: 0, reason: "viewport navigation unavailable" });
        }
        Some(values)
    };
    let prior_images = previous;
    let previous = previous.and_then(|witness| witness.reference.as_ref());
    let read = visible_reference(
        plan.packet().snapshot(),
        previous.map(|value| &value.pixels),
    )
    .map_err(|_| ObjectSceneError::ReferenceUnavailable)?;
    let reference = read.map(|read| {
        let (width, height) = read.pixels.dimensions();
        let scale = (f64::from(read.canvas.0) / f64::from(width))
            .min(f64::from(read.canvas.1) / f64::from(height));
        let transform = Affine::new([
            scale,
            0.0,
            0.0,
            scale,
            (f64::from(read.canvas.0) - f64::from(width) * scale) * 0.5,
            (f64::from(read.canvas.1) - f64::from(height) * scale) * 0.5,
        ]);
        // The reference is viewport-only world content. The artwork consumer
        // applies camera separately, so compose it into this image fit exactly once.
        let transform = camera.map_or(transform, |matrix| Affine::new(matrix) * transform);
        let image = match previous.filter(|value| value.pixels.same_allocation_as(&read.pixels)) {
            Some(value) => value.image.clone(),
            None => ImageData {
                // Vello may retain the Blob: retain its charge too, with no RGBA copy.
                data: Blob::new(Arc::new(read.pixels.clone())),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width,
                height,
            },
        };
        ReferenceScene {
            pixels: read.pixels,
            image,
            transform,
            opacity: read.opacity as f32,
        }
    });
    let witness = ObjectCompositionWitness {
        plan: plan.clone(),
        viewport: true,
        viewport_quality: Some(object_viewport_quality(camera_view, zoom, pan)),
        camera,
        resources: None,
        scene_effects: None,
        reference,
        rasters: None,
    };
    crate::object_render_scene::text_scene::attach(
        crate::object_render_scene::raster_scene::attach(witness, prior_images)?,
    )
}

impl ObjectCompositionWitness {
    /// Exact affine consumed by this viewport witness, before surface stretching.
    /// Artwork/export witnesses cannot issue a viewport coordinate proof.
    pub fn viewport_document_transform(&self) -> Option<[f64; 6]> {
        self.viewport.then(|| self.camera.unwrap_or([1., 0., 0., 1., 0., 0.]))
    }

    /// Availability is sampled from this exact retained packet, never legacy UI state.
    pub fn viewport_stored_camera_available(&self) -> bool {
        self.viewport && self.plan.packet().stored_camera_matrix().is_ok_and(|matrix| matrix.is_some())
    }
    /// Compares only the original encoded allocation. Visibility, opacity and
    /// frame changes can reuse it; replacement/removal cannot retain its raster.
    pub fn reference_matches_plan(&self, plan: &ObjectPaintPlan) -> bool {
        let reference = self
            .reference
            .as_ref()
            .is_none_or(|reference| reference.pixels.matches_image(plan.packet().snapshot()));
        let rasters = self
            .rasters
            .as_ref()
            .is_none_or(|rasters| rasters.matches(plan));
        self.has_scene_images() && reference && rasters
    }

    pub(crate) fn reference_image(&self) -> Option<(&ImageData, Affine, f32)> {
        self.reference
            .as_ref()
            .map(|value| (&value.image, value.transform, value.opacity))
    }
    /// A hidden/removed reference need not discard the sole cached raster;
    /// a later undo can reuse it, but only after native allocation comparison.
    pub fn has_viewport_reference(&self) -> bool {
        self.reference.is_some()
    }
}

#[cfg(test)]
#[path = "../tests/object_reference_viewport.rs"]
mod tests;
