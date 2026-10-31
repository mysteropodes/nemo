//! The raster branch of the existing object compositor, preserving pinned paint order and cache ownership.
use crate::compositor::object_compositor::prepare_command;
use crate::compositor::{Compositor, CompositorError};
use crate::object_render_scene::{ordered_entries, ObjectCompositionWitness, PaintEntry};
use vello::kurbo::Affine;
/// T12: the layered path draws solid layers only, so a video is refused, never omitted.
pub(super) fn refuse_layered_video(
    plan: &crate::object_render_scene::ObjectPaintPlan,
) -> Result<(), CompositorError> {
    if plan.video_slots().is_empty() {
        return Ok(());
    }
    Err(CompositorError::new(
        "native video layered effects are unavailable",
    ))
}
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
    /// Ghost images the live frame does not draw would stay resident (and charged) in the renderer's atlas for as long
    /// as it lives: start a fresh one rather than accumulating a decoded image per ghost frame. A ghost set already
    /// resident (pan, zoom and repeat presents draw the same memoised images) costs no reset and no re-upload.
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    pub(super) fn release_ghost_images(
        &mut self,
        prepared: &ObjectCompositionWitness,
    ) -> Result<(), CompositorError> {
        let Some(ghosts) = prepared.ghost_underlay() else {
            return Ok(());
        };
        let drawn = ghosts.foreign_image_ids(prepared);
        if drawn.iter().all(|id| self.resident_ghost_images.contains(id)) {
            return Ok(());
        }
        self.reset_object_image_cache()?;
        self.ghost_cache_resets += 1;
        self.resident_ghost_images = drawn;
        Ok(())
    }
    /// Times a ghost image set forced the object image cache to be rebuilt (a probe for "unchanged set, no reset").
    #[cfg(all(test, feature = "application", feature = "project_media_codec"))]
    pub(crate) fn ghost_cache_resets(&self) -> u64 {
        self.ghost_cache_resets
    }
    pub(super) fn emit_object_paint_sequence(
        &mut self,
        prepared: &ObjectCompositionWitness,
    ) -> Result<(), CompositorError> {
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            if prepared.has_scene_images() {
                self.retained_object_reference = Some(prepared.without_media());
            }
            // Vello can replace an image atlas on a solid-only pass. Every live native asset is
            // explicitly restored before use; instances of the same asset share one ImageData ID.
            for image in prepared.raster_images().chain(prepared.brush_images()) {
                self.renderer.mark_override_image_dirty(image);
            }
            // #2358 GHOST-UNDER: the ghost frames' images are restored like any live native asset.
            for image in prepared.ghost_underlay().into_iter().flat_map(|g| g.images()) {
                self.renderer.mark_override_image_dirty(image);
            }
        }
        // Append the whole artwork after all per-item paint, preserving stroke,
        // gradient, glyph and image transforms. No affine is applied twice.
        let camera = prepared.camera_matrix();
        let clip = if camera.is_some() {
            Some(
                prepared
                    .camera_canvas_dimensions()
                    .ok_or_else(|| CompositorError::new("stored camera clip is unavailable"))?,
            )
        } else {
            None
        };
        let mut artwork = vello::Scene::new();
        let scene = if camera.is_some() {
            &mut artwork
        } else {
            &mut self.scene
        };
        // Onion / Ghost All sit UNDER the live frame (baseline stack order): drawn first, in the same artwork scene,
        // so the camera / viewport navigation applies to them exactly once and live pixels win where they overlap.
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        if let Some(ghosts) = prepared.ghost_underlay() {
            ghosts.draw(scene);
        }
        for entry in ordered_entries(prepared.plan()) {
            match entry {
                PaintEntry::Contour(command) => {
                    prepare_command(command)?.draw(scene, Affine::IDENTITY, command.path())
                }
                PaintEntry::Text(source) => source.draw(scene),
                PaintEntry::Component(source) => {
                    source.prepared.draw(scene, Affine::IDENTITY, &source.path)
                }
                // T12: a video slot draws only from its decoded frame, never blank.
                PaintEntry::Video(index) => {
                    let (image, transform, opacity) = prepared
                        .video_image(index)
                        .ok_or_else(|| CompositorError::new("native video frame is unavailable"))?;
                    let brush =
                        vello::peniko::ImageBrush::new(image.clone()).multiply_alpha(opacity);
                    scene.draw_image(&brush, transform);
                }
                PaintEntry::Duplicator(source) => {
                    source.prepared.draw(scene, Affine::IDENTITY, &source.path)
                }
                PaintEntry::Dab(dab) => dab.prepared.draw(scene, Affine::IDENTITY, &dab.path),
                #[cfg(all(feature = "application", feature = "project_media_codec"))]
                PaintEntry::Brush(index) => {
                    if let Some((image, transform, opacity)) = prepared
                        .brush_image(index)
                        .map_err(|_| CompositorError::new("admitted brush image unavailable"))?
                    {
                        let brush =
                            vello::peniko::ImageBrush::new(image.clone()).multiply_alpha(opacity);
                        scene.draw_image(&brush, transform);
                    }
                }
                PaintEntry::Raster(index) => {
                    #[cfg(all(feature = "application", feature = "project_media_codec"))]
                    {
                        if !prepared
                            .replay_raster_mesh(index, scene)
                            .map_err(CompositorError::new)?
                        {
                            let (image, transform, opacity) =
                                prepared.raster_image(index).ok_or_else(|| {
                                    CompositorError::new("native raster resource is unavailable")
                                })?;
                            let brush = vello::peniko::ImageBrush::new(image.clone())
                                .multiply_alpha(opacity);
                            scene.draw_image(&brush, transform);
                        }
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
        if let Some(matrix) = camera {
            let (width, height) = clip.expect("camera clip preflighted");
            self.scene.push_clip_layer(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                &vello::kurbo::Rect::new(0., 0., f64::from(width), f64::from(height)),
            );
            self.scene.append(&artwork, Some(Affine::new(matrix)));
            self.scene.pop_layer();
        }
        Ok(())
    }
}

/// A layered GPU result needs its own final whole-image camera pass; do not
/// pretend that transforming only the flat source paints covers that path.
pub(super) fn check_layered_camera(
    prepared: &ObjectCompositionWitness,
) -> Result<(), CompositorError> {
    if prepared.camera_matrix().is_some() {
        return Err(CompositorError::new(
            "stored camera with layered effects is unavailable",
        ));
    }
    Ok(())
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
impl Compositor {
    /// Existing renderer/device/cache authority. Construct first: failure retains
    /// the previous renderer, scene, witness and charged immutable resources.
    pub(super) fn reset_object_image_cache(&mut self) -> Result<(), CompositorError> {
        let renderer = vello::Renderer::new(
            &self.device,
            vello::RendererOptions {
                antialiasing_support: vello::AaSupport::area_only(),
                use_cpu: true,
                ..Default::default()
            },
        )
        .map_err(|error| CompositorError::new(format!("reset Vello image cache: {error}")))?;
        self.renderer = renderer;
        self.scene.reset();
        self.retained_object_reference = None;
        self.last_reference_image = None;
        self.resident_ghost_images.clear();
        Ok(())
    }
    /// Called only on an admitted brush cache miss, before charged replacement
    /// decode/bake. An absent retained brush witness proves no brush mirrors:
    /// every normal image draw retains its complete witness until this reset.
    pub fn retire_object_brush_resources(
        &mut self,
        ids: &[String],
        dimensions: (u32, u32),
    ) -> Result<(), CompositorError> {
        crate::compositor::object_compositor::output_budget(
            dimensions.0,
            dimensions.1,
            self.device.limits().max_texture_dimension_2d,
        )?;
        if ids.is_empty() || ids.iter().any(|id| id.is_empty()) {
            return Err(CompositorError::new(
                "brush retirement requires actual resource identities",
            ));
        }
        if self
            .retained_object_reference
            .as_ref()
            .is_some_and(|old| old.plan().has_brushes())
        {
            self.reset_object_image_cache()?;
        }
        Ok(())
    }
}
