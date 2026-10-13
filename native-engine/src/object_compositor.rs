//! D2 headless object pixels on the existing compositor; never presentation.
use crate::compositor::{align_to, Compositor, CompositorError};
use crate::object_render_scene::{
    LiveObjectComposition, ObjectCompositionWitness, ObjectPaintCommand, ScheduledObjectComposition,
};
use crate::render_scene::ScheduledFrameIdentity;
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};

const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 16_777_216;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The one admitted output mode avoids any unpremultiplication ambiguity.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ObjectOutputSpec {
    width: u32,
    height: u32,
}

impl ObjectOutputSpec {
    pub(crate) fn new(width: u32, height: u32, mode: &str) -> Result<Self, CompositorError> {
        if mode != "white-srgb-normal-rgba8-area" {
            return Err(CompositorError::new("unsupported object output mode"));
        }
        output_budget(width, height, MAX_DIMENSION)?;
        Ok(Self { width, height })
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

fn color(command: ObjectPaintCommand<'_>) -> Result<Color, CompositorError> {
    let source = command.paint();
    let mut rgba = [0.0f32; 4];
    for (target, source) in rgba.iter_mut().zip(source) {
        let value = source
            .as_f64()
            .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
            .ok_or_else(|| CompositorError::new("unrepresentable object straight RGBA"))?;
        *target = value as f32;
    }
    // AlphaColor<Srgb> holds straight float channels; do not quantize to RGB8.
    Ok(Color::new(rgba))
}

impl Compositor {
    pub(crate) fn compose_scheduled_object(
        &mut self,
        live: &LiveObjectComposition<'_>,
    ) -> Result<ScheduledObjectCompositionResult, CompositorError> {
        let prepared = live.prepared();
        check_live(live, prepared)?;
        let (width, height) = prepared.dimensions();
        let output = ObjectOutputSpec::new(width, height, "white-srgb-normal-rgba8-area")?;
        Ok(ScheduledObjectCompositionResult {
            pixels: self.compose_object(prepared.composition(), output)?,
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
        // Validate all paints before target allocation or replacing retained scene data.
        for command in plan.commands() {
            color(command)?;
        }
        self.scene.reset();
        self.emit_object_commands(plan.commands())?;
        let (texture, view) = self.make_target(output.width, output.height);
        self.render_target(&view, output.width, output.height, Color::WHITE)?;
        Ok(ObjectCompositionResult {
            instance: self.instance.clone(),
            device: self.device.clone(),
            texture,
            view,
            output,
            prepared: prepared.clone(),
        })
    }

    fn emit_object_commands<'a>(
        &mut self,
        commands: impl Iterator<Item = ObjectPaintCommand<'a>>,
    ) -> Result<(), CompositorError> {
        for command in commands {
            self.scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color(command)?,
                None,
                command.contour().path(),
            );
        }
        Ok(())
    }

    pub(crate) fn readback_object_rgba8(
        &self,
        result: &ObjectCompositionResult,
    ) -> Result<ObjectRgba8Readback, CompositorError> {
        // Pinned WGPU CoreDevice equality uses an instance-local ID. Instance
        // equality also pins the retained Global context's Arc address, so an
        // equal numeric device ID in another context never reaches this copy.
        if result.instance != self.instance || result.device != self.device {
            return Err(CompositorError::new(
                "object pixels belong to a different GPU device",
            ));
        }
        output_budget(
            result.output.width,
            result.output.height,
            self.device.limits().max_texture_dimension_2d,
        )?;
        Ok(ObjectRgba8Readback {
            bytes: self.readback_texture(
                &result.texture,
                result.output.width,
                result.output.height,
            )?,
            output: result.output,
            prepared: result.prepared.clone(),
        })
    }
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
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    output: ObjectOutputSpec,
    prepared: ObjectCompositionWitness,
}
impl ObjectCompositionResult {
    pub(crate) fn identity(&self) -> &ObjectCompositionWitness {
        &self.prepared
    }
    pub(crate) fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

pub(crate) struct ObjectRgba8Readback {
    bytes: Vec<u8>,
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

#[cfg(test)]
#[path = "../tests/object_compositor.rs"]
mod tests;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_scheduled_compositor.rs"]
mod scheduled_tests;
