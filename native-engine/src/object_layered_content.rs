//! #2257: what one wire layer of the layered compositor paints.
//!
//! The layered path renders every layer into its own transparent texture. A layer that holds only plain contours keeps
//! the straight-RGB fill encoder of `scene_effect_gpu_blend` byte for byte. Every other family (text, images and image
//! meshes, components, duplicators, video slots, brush ink and dabs), and every viewport display mode that changes how
//! an item is drawn (the current-frame outline, hidden shadow guides), is drawn from the SAME paint entries and the
//! SAME `emit_entry` the flat sequence uses, one source layer at a time, so a family looks the same whichever
//! compositor entry reaches it (CLAUDE.md section 3). The stored camera wraps each layer, mask and matte exactly once
//! (baseline `view_tf` is applied to every item of every pass), and what sits UNDER the artwork (the transparency
//! checker, the viewport reference image, the onion frames) is one more texture composited beneath the accumulator, so
//! no blend, matte or effect ever reads it.
use crate::compositor::object_compositor::raster::emit_entry;
use crate::compositor::scene_effect_gpu_blend::{GpuLayerPaint, GpuLayerStack};
use crate::compositor::scene_effect_gpu_fx::{items_bbox_px, FxItemBounds};
use crate::compositor::{Compositor, CompositorError};
use crate::object_render_scene::viewport_checker::shadow::is_shadow_guide;
use crate::object_render_scene::{layered_entries, ObjectCompositionWitness, PaintEntry};
use std::collections::{BTreeMap, BTreeSet};
use vello::kurbo::{Affine, BezPath, Rect, Shape};
use vello::peniko::{Color, Fill};

/// The stored camera (or session navigation) matrix and the canvas rectangle that clips it.
pub(in crate::compositor) type CameraWrap = ([f64; 6], (u32, u32));

/// The witness behind a layered frame, with its paint entries grouped by source layer.
pub(in crate::compositor) struct LayerContext<'w> {
    witness: &'w ObjectCompositionWitness,
    camera: Option<CameraWrap>,
    entries: BTreeMap<usize, Vec<PaintEntry<'w>>>,
    non_contour: BTreeSet<usize>,
    every_layer: bool,
}

impl<'w> LayerContext<'w> {
    pub(in crate::compositor) fn new(
        witness: &'w ObjectCompositionWitness,
    ) -> Result<Self, CompositorError> {
        let camera = match witness.camera_matrix() {
            Some(matrix) => Some((
                matrix,
                witness
                    .camera_canvas_dimensions()
                    .ok_or_else(|| CompositorError::new("stored camera clip is unavailable"))?,
            )),
            None => None,
        };
        let mut entries: BTreeMap<usize, Vec<PaintEntry<'w>>> = BTreeMap::new();
        let mut non_contour = BTreeSet::new();
        for (layer, entry) in layered_entries(witness.plan()) {
            if !matches!(entry, PaintEntry::Contour(_)) {
                non_contour.insert(layer);
            }
            entries.entry(layer).or_default().push(entry);
        }
        Ok(Self {
            witness,
            camera,
            entries,
            non_contour,
            // Outline and hidden guides change how a CONTOUR is drawn, so every layer takes the entry path.
            every_layer: witness.viewport_outline().is_some()
                || witness.viewport_hides_shadow_guides(),
        })
    }

    pub(in crate::compositor) fn camera(&self) -> Option<CameraWrap> {
        self.camera
    }

    /// Scene time in seconds of the effect shaders (baseline `frameForFx / fpsForFx`).
    pub(in crate::compositor) fn time(&self) -> f32 {
        let packet = self.witness.plan().packet();
        let fps = packet
            .snapshot()
            .project()
            .map_or(24., |project| project.project().fps());
        (f64::from(packet.frame()) / if fps > 0. { fps } else { 24. }) as f32
    }

    fn view(&self) -> [f64; 6] {
        self.camera
            .map_or([1., 0., 0., 1., 0., 0.], |(matrix, _)| matrix)
    }

    /// Baseline `items_bbox_px` of one source layer's items under the stored camera; the full target when none resolves.
    pub(in crate::compositor) fn layer_bbox(
        &self,
        layer: Option<usize>,
        canvas: (u32, u32),
    ) -> (f32, f32, f32, f32) {
        let hide_shadow = self.witness.viewport_hides_shadow_guides();
        let items: Vec<FxItemBounds> = layer
            .and_then(|layer| self.entries.get(&layer))
            .into_iter()
            .flatten()
            .filter(|entry| !(hide_shadow && is_shadow_guide(entry)))
            .flat_map(|entry| entry_bounds(self.witness, entry))
            .collect();
        items_bbox_px(&items, self.view(), canvas)
    }

    /// Baseline `document_bbox_px`: the whole document (the project canvas) under the stored camera.
    pub(in crate::compositor) fn document_bbox(&self, canvas: (u32, u32)) -> (f32, f32, f32, f32) {
        let (width, height) = self.witness.camera_canvas_dimensions().unwrap_or(canvas);
        let document = FxItemBounds::Path {
            bounds: [0., 0., f64::from(width), f64::from(height)],
            transform: None,
        };
        items_bbox_px(&[document], self.view(), canvas)
    }

    fn through_entries(&self, layer: usize) -> bool {
        self.every_layer || self.non_contour.contains(&layer)
    }

    /// True when anything is painted beneath the artwork (viewport witnesses only).
    pub(in crate::compositor) fn has_under(&self) -> bool {
        let w = self.witness;
        let mut under = w.has_viewport_checker();
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            under |= w.reference_image().is_some() || w.ghost_underlay().is_some();
        }
        under
    }
}

impl Compositor {
    /// Wrap `artwork` in the camera: clip to the canvas, then append under the matrix (the flat sequence's wrapper).
    pub(in crate::compositor) fn append_camera(
        &mut self,
        artwork: &vello::Scene,
        camera: CameraWrap,
    ) {
        let (matrix, (width, height)) = camera;
        self.scene.push_clip_layer(
            Fill::NonZero,
            Affine::IDENTITY,
            &Rect::new(0., 0., f64::from(width), f64::from(height)),
        );
        self.scene.append(artwork, Some(Affine::new(matrix)));
        self.scene.pop_layer();
    }

    /// Paint the content of wire layer `wire` into `view` (transparent behind it).
    pub(in crate::compositor) fn render_wire_layer(
        &mut self,
        ctx: Option<&LayerContext<'_>>,
        stack: &GpuLayerStack<'_>,
        wire: usize,
        view: &wgpu::TextureView,
        size: (u32, u32),
    ) -> Result<(), CompositorError> {
        let paint: &GpuLayerPaint<'_> = &stack.inputs()[wire].paint;
        let layer = stack.inputs()[wire].source_layer;
        match (ctx, layer) {
            (Some(ctx), Some(layer)) if ctx.through_entries(layer) => {
                self.render_layer_entries(ctx, layer, view, size)
            }
            _ => self.render_paints(&[paint], ctx.and_then(LayerContext::camera), view, size),
        }
    }

    fn render_layer_entries(
        &mut self,
        ctx: &LayerContext<'_>,
        layer: usize,
        view: &wgpu::TextureView,
        (width, height): (u32, u32),
    ) -> Result<(), CompositorError> {
        let witness = ctx.witness;
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        self.restore_object_images(witness);
        let outline = witness.viewport_outline();
        let hide_shadow = witness.viewport_hides_shadow_guides();
        self.scene.reset();
        let mut artwork = vello::Scene::new();
        let scene = if ctx.camera.is_some() {
            &mut artwork
        } else {
            &mut self.scene
        };
        let mut drawn = Ok(());
        for entry in ctx.entries.get(&layer).into_iter().flatten().copied() {
            if hide_shadow && is_shadow_guide(&entry) {
                continue;
            }
            drawn = emit_entry(witness, entry, scene, outline);
            if drawn.is_err() {
                break;
            }
        }
        if drawn.is_ok() {
            if let Some(camera) = ctx.camera {
                self.append_camera(&artwork, camera);
            }
        }
        let rendered = match drawn {
            Ok(()) => self.render_target(view, width, height, Color::TRANSPARENT),
            Err(error) => Err(error),
        };
        self.scene.reset();
        rendered
    }

    /// The transparency checker, the viewport reference image and the onion frames, in the flat sequence's order, into
    /// one transparent texture that the final composite places beneath the accumulator.
    pub(in crate::compositor) fn render_under(
        &mut self,
        ctx: &LayerContext<'_>,
        view: &wgpu::TextureView,
        (width, height): (u32, u32),
    ) -> Result<(), CompositorError> {
        let witness = ctx.witness;
        self.scene.reset();
        witness.paint_viewport_checker(&mut self.scene);
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            self.restore_object_images(witness);
            if let Some((image, transform, opacity)) = witness.reference_image() {
                self.retained_object_reference = Some(witness.without_media());
                self.renderer.mark_override_image_dirty(image);
                let brush = vello::peniko::ImageBrush::new(image.clone()).multiply_alpha(opacity);
                self.scene.draw_image(&brush, transform);
            }
            if let Some(ghosts) = witness.ghost_underlay() {
                let mut artwork = vello::Scene::new();
                ghosts.draw(&mut artwork);
                match ctx.camera {
                    Some(camera) => self.append_camera(&artwork, camera),
                    None => self.scene.append(&artwork, None),
                }
            }
        }
        let rendered = self.render_target(view, width, height, Color::TRANSPARENT);
        self.scene.reset();
        rendered
    }
}

fn path_bounds(path: &BezPath) -> FxItemBounds {
    let b = path.bounding_box();
    FxItemBounds::Path {
        bounds: [b.x0, b.y0, b.x1, b.y1],
        transform: None,
    }
}

/// An image placed by `transform`: its pixel rectangle under that affine.
fn image_bounds(image: &vello::peniko::ImageData, transform: Affine) -> FxItemBounds {
    FxItemBounds::Path {
        bounds: [0., 0., f64::from(image.width), f64::from(image.height)],
        transform: Some(transform.as_coeffs()),
    }
}

/// The boxes one entry contributes to `items_bbox_px` (paths by their geometry, images by their placed rectangle).
fn entry_bounds(witness: &ObjectCompositionWitness, entry: &PaintEntry<'_>) -> Vec<FxItemBounds> {
    let _ = witness;
    match entry {
        PaintEntry::Contour(command) => vec![path_bounds(command.path())],
        PaintEntry::Component(source) => vec![path_bounds(&source.path)],
        PaintEntry::Duplicator(source) => vec![path_bounds(&source.path)],
        PaintEntry::Dab(dab) => vec![path_bounds(&dab.path)],
        PaintEntry::Text(source) => source
            .item
            .painted()
            .map(|(contour, _)| path_bounds(contour.contour.path()))
            .collect(),
        PaintEntry::Video(index) => witness
            .video_image(*index)
            .map(|(image, transform, _)| image_bounds(image, transform))
            .into_iter()
            .collect(),
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        PaintEntry::Brush(index) => witness
            .brush_image(*index)
            .ok()
            .flatten()
            .map(|(image, transform, _)| image_bounds(image, transform))
            .into_iter()
            .collect(),
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        PaintEntry::Raster(index) => witness
            .raster_image(*index)
            .map(|(image, transform, _)| image_bounds(image, transform))
            .into_iter()
            .collect(),
        #[cfg(not(all(feature = "application", feature = "project_media_codec")))]
        PaintEntry::Raster(_) => Vec::new(),
    }
}
