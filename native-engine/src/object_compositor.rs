//! D2 headless object pixels on the existing compositor; never presentation.
use crate::compositor::object_target::ObjectTarget;
use crate::compositor::scene_effect_gpu_blend::GpuLayerStack;
use crate::compositor::scene_effect_scene_join_stack::{select_composition_path, PathSelection};
use crate::compositor::{align_to, Compositor, CompositorError};
use crate::object_render_scene::object_scene_style_emit::{prepare_item, PreparedItem};
use crate::object_render_scene::{
    LiveObjectComposition, ObjectCompositionWitness, ObjectPaintCommand, ScheduledObjectComposition,
};
use crate::render_scene::ScheduledFrameIdentity;
use vello::kurbo::Affine;
use vello::peniko::Color;

#[path = "object_raster_compositor.rs"]
mod raster;

const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 16_777_216;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Explicit white compatibility or background from the immutable project witness.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ObjectOutputSpec {
    width: u32,
    height: u32,
    project_background: bool,
}

impl ObjectOutputSpec {
    pub(crate) fn new(width: u32, height: u32, mode: &str) -> Result<Self, CompositorError> {
        let project_background = match mode {
            "white-srgb-normal-rgba8-area" => false,
            "project-srgb-normal-rgba8-area" => true,
            _ => return Err(CompositorError::new("unsupported object output mode")),
        };
        output_budget(width, height, MAX_DIMENSION)?;
        Ok(Self {
            width,
            height,
            project_background,
        })
    }
}

/// Check both the GPU target and padded readback BEFORE either allocation.
fn output_budget(width: u32, height: u32, device_max: u32) -> Result<(), CompositorError> {
    let dimension = MAX_DIMENSION.min(device_max);
    if width == 0 || height == 0 || width > dimension || height > dimension {
        return Err(CompositorError::new(
            "object output dimensions exceed budget",
        ));
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| CompositorError::new("object output pixel count overflow"))?;
    let row = width
        .checked_mul(4)
        .ok_or_else(|| CompositorError::new("object output row overflow"))?;
    let padded = align_to(row, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let raw = pixels.checked_mul(4);
    let aligned = u64::from(padded).checked_mul(u64::from(height));
    if pixels > MAX_PIXELS
        || raw.is_none_or(|n| n > MAX_BYTES)
        || aligned.is_none_or(|n| n > MAX_BYTES)
    {
        return Err(CompositorError::new(
            "object RGBA allocation exceeds budget",
        ));
    }
    Ok(())
}

/// Typed style of one command, converted to Vello values but not drawn: paint channels, stroke style
/// and gradient preparation all fail here, before any target allocation or scene replacement.
fn prepare_command(command: ObjectPaintCommand<'_>) -> Result<PreparedItem, CompositorError> {
    let style = command
        .style()
        .map_err(|error| CompositorError::new(error.message()))?;
    prepare_item(&style).map_err(|error| CompositorError::new(error.message()))
}

fn check_flat_masks(witness: &ObjectCompositionWitness) -> Result<(), CompositorError> {
    if !witness.plan().mask_inputs().layers().is_empty() {
        return Err(CompositorError::new(
            "native vector masks require joined layer composition",
        ));
    }
    Ok(())
}

impl Compositor {
    /// Retire the old image's CPU Blob in Vello's private resolver before the
    /// caller decodes its replacement. The compositor/device/queue stay fixed.
    /// Call only after selector, output and paint admission. Construct first so
    /// a failed reset preserves the old cache and remains safe to retry.
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    pub fn retire_object_reference_for(
        &mut self,
        plan: &crate::object_render_scene::ObjectPaintPlan,
        dimensions: (u32, u32),
    ) -> Result<bool, CompositorError> {
        output_budget(
            dimensions.0,
            dimensions.1,
            self.device.limits().max_texture_dimension_2d,
        )?;
        for command in plan.commands() {
            prepare_command(command)?;
        }
        if self
            .retained_object_reference
            .as_ref()
            .is_none_or(|previous| previous.reference_matches_plan(plan))
        {
            return Ok(false);
        }
        self.reset_object_image_cache()?;
        Ok(true)
    }

    /// Borrow the same texture and GPU under a live scheduler permit. No raw
    /// scheduled texture can escape for later consumption without that borrow.
    pub(crate) fn scheduled_object_surface<'a>(
        &'a self,
        result: &'a ScheduledObjectCompositionResult,
        live: &'a LiveObjectComposition<'a>,
    ) -> Result<ScheduledObjectSurface<'a>, CompositorError> {
        check_live(live, &result.prepared)?;
        self.check_object_device(&result.pixels)?;
        Ok(ScheduledObjectSurface {
            instance: &self.instance,
            adapter: &self.adapter,
            device: &self.device,
            queue: &self.queue,
            texture: result.pixels.target.texture(),
            texture_view: result.pixels.target.view(),
            dimensions: result.prepared.dimensions(),
            identity: result.identity(),
            _live: live,
        })
    }

    pub(crate) fn compose_scheduled_object(
        &mut self,
        live: &LiveObjectComposition<'_>,
    ) -> Result<ScheduledObjectCompositionResult, CompositorError> {
        let prepared = live.prepared();
        check_live(live, prepared)?;
        let (width, height) = prepared.dimensions();
        let output = ObjectOutputSpec::new(width, height, "project-srgb-normal-rgba8-area")?;
        // L08-K: a witness that carries its joined scene-effect plan paints through the layered entry when that
        // plan is layered; an incomplete plan is refused here, a flat plan and a witness with no plan keep the
        // accepted fast path unchanged.
        let pixels = match prepared.composition().scene_effects() {
            Some(effects) => {
                let selected = select_composition_path(effects, prepared.composition().plan())
                    .map_err(|error| {
                        CompositorError::new(format!(
                            "scene composition path unavailable: {error:?}"
                        ))
                    })?;
                match selected {
                    PathSelection::Layered(stack) => {
                        self.compose_object_layered(prepared.composition(), output, &stack)?
                    }
                    PathSelection::Flat => self.compose_object(prepared.composition(), output)?,
                }
            }
            None => self.compose_object(prepared.composition(), output)?,
        };
        Ok(ScheduledObjectCompositionResult {
            pixels,
            prepared: prepared.clone(),
        })
    }

    /// Layered counterpart of [`Self::compose_scheduled_object`] (L08-G): the same
    /// liveness check and output spec, pixels from a GPU layer stack built from the
    /// same plan. Staleness, cancellation and replacement are refused by `check_live`
    /// before any texture exists, exactly as on the fast path.
    #[allow(dead_code)]
    pub(crate) fn compose_layered_scheduled_object(
        &mut self,
        live: &LiveObjectComposition<'_>,
        stack: &GpuLayerStack<'_>,
    ) -> Result<ScheduledObjectCompositionResult, CompositorError> {
        let prepared = live.prepared();
        check_live(live, prepared)?;
        let (width, height) = prepared.dimensions();
        let output = ObjectOutputSpec::new(width, height, "project-srgb-normal-rgba8-area")?;
        Ok(ScheduledObjectCompositionResult {
            pixels: self.compose_object_layered(prepared.composition(), output, stack)?,
            prepared: prepared.clone(),
        })
    }

    /// Layered counterpart of [`Self::compose_object`] (L08-G). Same admission (output
    /// budget, every paint validated, same background rule); pixels come from the GPU
    /// layer stack, which must account for every paint command of this witness' plan
    /// exactly once (`GpuLayerStack::covers_plan`: same contour address and colour bits,
    /// compared as a multiset, so a count-preserving mis-mapping is refused). A reference image is refused: the layered path draws solid layers.
    #[allow(dead_code)]
    pub(crate) fn compose_object_layered(
        &mut self,
        prepared: &ObjectCompositionWitness,
        output: ObjectOutputSpec,
        stack: &GpuLayerStack<'_>,
    ) -> Result<ObjectCompositionResult, CompositorError> {
        raster::check_layered_camera(prepared)?;
        let plan = prepared.plan();
        output_budget(
            output.width,
            output.height,
            self.device.limits().max_texture_dimension_2d,
        )?;
        for command in plan.commands() {
            prepare_command(command)?;
        }
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        if prepared.has_raster_images() {
            return Err(CompositorError::new(
                "native raster layered effects are unavailable",
            ));
        }
        if plan.has_text() {
            return Err(CompositorError::new(
                "native Text layered effects are unavailable",
            ));
        }
        if plan.has_components() {
            return Err(CompositorError::new(
                "native component layered effects are unavailable",
            ));
        }
        if plan.has_duplicators() {
            return Err(CompositorError::new(
                "native duplicator layered effects await their shared paint join",
            ));
        }
        raster::refuse_layered_video(plan)?;
        if !stack.covers_plan(plan) {
            return Err(CompositorError::new(
                "layer stack does not cover the composition plan",
            ));
        }
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        if prepared.reference_image().is_some() {
            return Err(CompositorError::new(
                "reference image is unavailable on the layered path",
            ));
        }
        // All layered/style/reference admission above must precede retiring
        // a prior native image that could fill Vello's private atlas.
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        self.retire_object_reference_for(plan, (output.width, output.height))?;
        let background = if output.project_background {
            prepared.canvas_background_rgba8()
        } else {
            [255; 4]
        };
        let target = self
            .compose_layered_stack(stack, (output.width, output.height), background)?
            .into_target();
        Ok(ObjectCompositionResult {
            instance: self.instance.clone(),
            device: self.device.clone(),
            target,
            output,
            prepared: prepared.clone(),
        })
    }

    pub(crate) fn readback_scheduled_object_rgba8(
        &self,
        result: &ScheduledObjectCompositionResult,
        live: &LiveObjectComposition<'_>,
    ) -> Result<ScheduledObjectRgba8Readback, CompositorError> {
        check_live(live, &result.prepared)?;
        Ok(ScheduledObjectRgba8Readback {
            pixels: self.readback_object_rgba8(&result.pixels)?,
            prepared: result.prepared.clone(),
        })
    }

    pub(crate) fn compose_object(
        &mut self,
        prepared: &ObjectCompositionWitness,
        output: ObjectOutputSpec,
    ) -> Result<ObjectCompositionResult, CompositorError> {
        // Only the render-scene boundary can create this validated witness.
        let plan = prepared.plan();
        output_budget(
            output.width,
            output.height,
            self.device.limits().max_texture_dimension_2d,
        )?;
        check_flat_masks(prepared)?;
        // Validate all styles before target allocation or replacing retained scene data.
        for command in plan.commands() {
            prepare_command(command)?;
        }
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        self.retire_raster_cache(prepared, (output.width, output.height))?;
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        let reference_image = prepared
            .reference_image()
            .map(|(image, _, _)| image.data.id());
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        let restore_image_atlas = self.scene.encoding().resources.patches.is_empty()
            || self.last_reference_image != reference_image;
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            self.last_reference_image = None;
        }
        self.scene.reset();
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        if let Some((image, transform, opacity)) = prepared.reference_image() {
            // Retain identity before fallible rendering, including error/unwind.
            self.retained_object_reference = Some(prepared.without_media());
            // Vello 0.9's solid-only resolve replaces the atlas with a 1x1
            // texture but retains image residency. Restore each image when it
            // becomes active after export, hiding or another image; undo may
            // revive an older cache entry whose atlas pixels were lost too.
            if restore_image_atlas {
                self.renderer.mark_override_image_dirty(image);
            }
            let brush = vello::peniko::ImageBrush::new(image.clone()).multiply_alpha(opacity);
            self.scene.draw_image(&brush, transform);
        }
        self.emit_object_paint_sequence(prepared)?;
        let background = if output.project_background {
            Color::new(
                prepared
                    .canvas_background_rgba8()
                    .map(|channel| f32::from(channel) / 255.),
            )
        } else {
            Color::WHITE
        };
        let mut target = self.make_object_target(output.width, output.height)?;
        // Pinned Vello0.9 fine shader writes straight RGBA (rgba_sep). Keep
        // readback unchanged: a second unpremultiplication would corrupt alpha.
        target.render(|view| self.render_target(view, output.width, output.height, background))?;
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            self.last_reference_image = reference_image;
            self.release_ghost_images(prepared)?;
        }
        Ok(ObjectCompositionResult {
            instance: self.instance.clone(),
            device: self.device.clone(),
            target,
            output,
            prepared: prepared.clone(),
        })
    }

    fn emit_object_commands<'a>(
        &mut self,
        commands: impl Iterator<Item = ObjectPaintCommand<'a>>,
    ) -> Result<(), CompositorError> {
        for command in commands {
            prepare_command(command)?.draw(&mut self.scene, Affine::IDENTITY, command.path());
        }
        Ok(())
    }

    pub(crate) fn readback_object_rgba8(
        &self,
        result: &ObjectCompositionResult,
    ) -> Result<ObjectRgba8Readback, CompositorError> {
        self.check_object_device(result)?;
        output_budget(
            result.output.width,
            result.output.height,
            self.device.limits().max_texture_dimension_2d,
        )?;
        Ok(ObjectRgba8Readback {
            bytes: self.readback_texture(
                result.target.texture(),
                result.output.width,
                result.output.height,
            )?,
            output: result.output,
            prepared: result.prepared.clone(),
        })
    }

    fn check_object_device(&self, result: &ObjectCompositionResult) -> Result<(), CompositorError> {
        // Pinned WGPU CoreDevice equality uses an instance-local ID. Instance
        // equality also pins the retained Global context's Arc address, so an
        // equal numeric device ID in another context never reaches this copy.
        if result.instance != self.instance || result.device != self.device {
            return Err(CompositorError::new(
                "object pixels belong to a different GPU device",
            ));
        }
        Ok(())
    }
}

pub(crate) struct ScheduledObjectSurface<'a> {
    pub(crate) instance: &'a wgpu::Instance,
    pub(crate) adapter: &'a wgpu::Adapter,
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) texture: &'a wgpu::Texture,
    pub(crate) texture_view: &'a wgpu::TextureView,
    pub(crate) dimensions: (u32, u32),
    pub(crate) identity: &'a ScheduledFrameIdentity,
    _live: &'a LiveObjectComposition<'a>,
}

fn check_live(
    live: &LiveObjectComposition<'_>,
    prepared: &ScheduledObjectComposition,
) -> Result<(), CompositorError> {
    live.validate_result(prepared)
        .map_err(|error| CompositorError::new(format!("object work is not consumable: {error:?}")))
}

/// Raw texture/readback handles remain private; identity is diagnostic only.
pub(crate) struct ScheduledObjectCompositionResult {
    pixels: ObjectCompositionResult,
    prepared: ScheduledObjectComposition,
}
impl ScheduledObjectCompositionResult {
    pub(crate) fn identity(&self) -> &ScheduledFrameIdentity {
        self.prepared.identity()
    }
}

pub(crate) struct ScheduledObjectRgba8Readback {
    pixels: ObjectRgba8Readback,
    prepared: ScheduledObjectComposition,
}
impl ScheduledObjectRgba8Readback {
    pub(crate) fn identity(&self) -> &ScheduledFrameIdentity {
        self.prepared.identity()
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        self.pixels.dimensions()
    }
    pub(crate) fn bytes<'a>(
        &'a self,
        live: &'a LiveObjectComposition<'_>,
    ) -> Result<&'a [u8], CompositorError> {
        check_live(live, &self.prepared)?;
        Ok(self.pixels.bytes())
    }
}

pub(crate) struct ObjectCompositionResult {
    instance: wgpu::Instance,
    device: wgpu::Device,
    target: ObjectTarget,
    output: ObjectOutputSpec,
    prepared: ObjectCompositionWitness,
}
impl ObjectCompositionResult {
    pub(crate) fn identity(&self) -> &ObjectCompositionWitness {
        &self.prepared
    }
    pub(crate) fn texture_view(&self) -> &wgpu::TextureView {
        self.target.view()
    }
}

pub(crate) struct ObjectRgba8Readback {
    bytes: crate::compositor::readback::ReadbackBytes,
    output: ObjectOutputSpec,
    prepared: ObjectCompositionWitness,
}
impl ObjectRgba8Readback {
    pub(crate) fn identity(&self) -> &ObjectCompositionWitness {
        &self.prepared
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.output.width, self.output.height)
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

// L01-I: Metal readback and export parity of the editable vector family through this module's private scene and target.
#[cfg(all(test, feature = "export_job"))]
#[path = "../tests/render_vector_export_parity.rs"]
mod render_vector_export_parity;
#[cfg(test)]
#[path = "../tests/render_vector_pixels.rs"]
mod render_vector_pixels;
// #2361: Metal readback of a textured pressure ribbon and its dabs (inside the compositor tree for the private target).
#[cfg(test)]
#[path = "../tests/object_brush_dab_pixels.rs"]
mod object_brush_dab_pixels;
#[cfg(test)]
#[path = "../tests/object_compositor.rs"]
mod tests;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_scheduled_compositor.rs"]
pub(super) mod scheduled_tests;

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_project_background.rs"]
mod project_background_tests;

// L07-J (#2113): Metal readback of the brush producer's pinned scene (inside the compositor tree for the private scene/target).
#[cfg(test)]
#[path = "../tests/object_scene_producer_brush_ink.rs"]
mod object_scene_producer_brush_ink_tests;

// L07-I: Metal readback of the typed scene style through this module's private scene and target.
#[cfg(test)]
#[path = "../tests/object_scene_style_emit_gpu.rs"]
mod object_scene_style_emit_gpu;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_scene_style_emit_refusal.rs"]
mod object_scene_style_emit_refusal;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_compositor_masks.rs"]
mod mask_tests;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_subselect_pixels.rs"]
mod subselect_pixels;
