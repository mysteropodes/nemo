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

/// The old witness is only a one-entry read cache. Every selector and setting is
/// derived again from the new plan's original immutable snapshot.
pub fn prepare_object_viewport_composition(
    plan: &ObjectPaintPlan,
    expected: &ObjectFrameSelector<'_>,
    previous: Option<&ObjectCompositionWitness>,
) -> Result<ObjectCompositionWitness, ObjectSceneError> {
    validate_pins(plan.packet(), expected)?;
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
        resources: None,
        scene_effects: None,
        reference,
        rasters: None,
    };
    crate::object_render_scene::raster_scene::attach(witness, prior_images)
}

impl ObjectCompositionWitness {
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
