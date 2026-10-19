//! The raster branch of the existing object compositor, preserving pinned paint order and cache ownership.
use crate::compositor::object_compositor::prepare_command;
use crate::compositor::{Compositor, CompositorError};
use crate::object_render_scene::{ordered_entries, ObjectCompositionWitness, PaintEntry};
use vello::kurbo::Affine;
impl Compositor {
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    pub(super) fn retire_raster_cache(
        &mut self,
        prepared: &ObjectCompositionWitness,
        dimensions: (u32, u32),
    ) -> Result<(), CompositorError> {
        if self.retained_object_reference.as_ref().is_some_and(|old| {
            old.has_raster_images() && !old.reference_matches_plan(prepared.plan())
        }) {
            self.retire_object_reference_for(prepared.plan(), dimensions)?;
        }
        Ok(())
    }
    pub(super) fn emit_object_paint_sequence(
        &mut self,
        prepared: &ObjectCompositionWitness,
    ) -> Result<(), CompositorError> {
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            if prepared.has_scene_images() {
                self.retained_object_reference = Some(prepared.clone());
            }
            // Vello can replace an image atlas on a solid-only pass. Every live native asset is
            // explicitly restored before use; instances of the same asset share one ImageData ID.
            for image in prepared.raster_images() {
                self.renderer.mark_override_image_dirty(image);
            }
        }
        for entry in ordered_entries(prepared.plan()) {
            match entry {
                PaintEntry::Contour(command) => prepare_command(command)?.draw(
                    &mut self.scene,
                    Affine::IDENTITY,
                    command.path(),
                ),
                PaintEntry::Text(source) => source.draw(&mut self.scene),
                PaintEntry::Component(source) => {
                    source
                        .prepared
                        .draw(&mut self.scene, Affine::IDENTITY, &source.path)
                }
                PaintEntry::Raster(index) => {
                    #[cfg(all(feature = "application", feature = "project_media_codec"))]
                    {
                        let (image, transform, opacity) =
                            prepared.raster_image(index).ok_or_else(|| {
                                CompositorError::new("native raster resource is unavailable")
                            })?;
                        let brush =
                            vello::peniko::ImageBrush::new(image.clone()).multiply_alpha(opacity);
                        self.scene.draw_image(&brush, transform);
                    }
                    #[cfg(not(all(feature = "application", feature = "project_media_codec")))]
                    {
                        let _ = index;
                        return Err(CompositorError::new(
                            "native raster resources require project media support",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
