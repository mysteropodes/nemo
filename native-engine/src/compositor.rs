//! One native Vello scene and offscreen GPU composition boundary.
//!
//! N12 neither presents nor encodes. Preview consumers use the opaque native
//! texture/view and export consumers cross the explicit RGBA8 readback boundary.

use crate::render_scene::{RenderScene, ScheduledFrameIdentity};
use std::fmt::{Display, Formatter};
use std::sync::mpsc;
use std::time::Duration;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{BlendMode, Color, Fill};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositorError {
    message: String,
}

impl CompositorError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for CompositorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CompositorError {}

/// The sole N12 native GPU owner. It retains exactly one Vello `Scene`.
pub struct Compositor {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    scene: Scene,
}

/// First half of native GPU construction. A window host creates its surface
/// from this exact instance before adapter/device selection; headless callers
/// can continue to use [`Compositor::new`].
pub struct CompositorInstance {
    instance: wgpu::Instance,
}

impl CompositorInstance {
    pub fn new() -> Self {
        Self {
            instance: wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::METAL,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            }),
        }
    }

    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    pub fn create_headless(self) -> Result<Compositor, CompositorError> {
        self.create(None)
    }

    pub fn create_for_surface(
        self,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Compositor, CompositorError> {
        self.create(Some(surface))
    }

    fn create(
        self,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Compositor, CompositorError> {
        let adapter =
            pollster::block_on(self.instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface,
            }))
            .map_err(|error| {
                CompositorError::new(format!("request native GPU adapter: {error}"))
            })?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("n12-native-compositor-device"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|error| CompositorError::new(format!("request native GPU device: {error}")))?;
        let renderer = Renderer::new(
            &device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|error| CompositorError::new(format!("create Vello renderer: {error}")))?;
        Ok(Compositor {
            instance: self.instance,
            adapter,
            device,
            queue,
            renderer,
            scene: Scene::new(),
        })
    }
}

impl Default for CompositorInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl Compositor {
    pub fn new() -> Result<Self, CompositorError> {
        CompositorInstance::new().create_headless()
    }

    /// The instance which created both the retained adapter and any compatible
    /// native surface constructed through [`CompositorInstance`].
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// The adapter selected before this compositor's sole device and queue.
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// Native-only access for the later viewport host; no CPU pixel boundary.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Native-only queue access for a later presentation host.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Render a prepared immutable scene into one native offscreen result.
    pub fn compose(&mut self, input: &RenderScene) -> Result<CompositionResult, CompositorError> {
        let (width, height) = input.output_spec().dimensions();
        let (texture, view) = self.make_target(width, height);
        self.scene.reset();
        for layer in &input.layers {
            let [red, green, blue] = layer.paint.rgb();
            let rect = Rect::new(
                layer.bounds[0],
                layer.bounds[1],
                layer.bounds[2],
                layer.bounds[3],
            );
            self.scene.push_layer(
                Fill::NonZero,
                BlendMode::default(),
                (layer.opacity_percent / 100.0) as f32,
                Affine::new(layer.transform),
                &rect,
            );
            self.scene.fill(
                Fill::NonZero,
                Affine::new(layer.transform),
                Color::from_rgb8(red, green, blue),
                None,
                &rect,
            );
            self.scene.pop_layer();
        }
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &view,
                &RenderParams {
                    base_color: Color::WHITE,
                    width,
                    height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|error| CompositorError::new(format!("render Vello scene: {error}")))?;
        Ok(CompositionResult {
            texture,
            view,
            width,
            height,
            scheduled_identity: input.scheduled_identity().clone(),
            document_id: input.document_id().to_owned(),
            content_revision: input.content_revision(),
        })
    }

    /// The only CPU readback path. N13 can use `CompositionResult::texture_view` instead.
    pub fn readback_rgba8(
        &self,
        result: &CompositionResult,
    ) -> Result<NativeRgba8Readback, CompositorError> {
        let unpadded_bytes_per_row = result
            .width
            .checked_mul(4)
            .ok_or_else(|| CompositorError::new("RGBA8 readback width overflows bytes per row"))?;
        let padded_bytes_per_row =
            align_to(unpadded_bytes_per_row, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let size = u64::from(padded_bytes_per_row)
            .checked_mul(u64::from(result.height))
            .ok_or_else(|| CompositorError::new("RGBA8 readback allocation overflows"))?;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n12-native-rgba8-readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("n12-native-rgba8-copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &result.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(result.height),
                },
            },
            wgpu::Extent3d {
                width: result.width,
                height: result.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |mapped| {
            let _ = sender.send(mapped.map_err(|error| error.to_string()));
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| CompositorError::new(format!("wait for RGBA8 readback: {error}")))?;
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| CompositorError::new(format!("receive RGBA8 readback: {error}")))?
            .map_err(|error| CompositorError::new(format!("map RGBA8 readback: {error}")))?;
        let mapped = slice.get_mapped_range();
        let mut bytes = Vec::with_capacity((unpadded_bytes_per_row * result.height) as usize);
        for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
            bytes.extend_from_slice(&row[..unpadded_bytes_per_row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(NativeRgba8Readback {
            width: result.width,
            height: result.height,
            bytes,
            scheduled_identity: result.scheduled_identity.clone(),
            document_id: result.document_id.clone(),
            content_revision: result.content_revision,
        })
    }

    fn make_target(&self, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("n12-vello-offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }
}

pub struct CompositionResult {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    scheduled_identity: ScheduledFrameIdentity,
    document_id: String,
    content_revision: u64,
}

impl CompositionResult {
    /// Native-only preview access; it never exposes CPU/JS pixel bytes.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn scheduled_identity(&self) -> &ScheduledFrameIdentity {
        &self.scheduled_identity
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeRgba8Readback {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
    scheduled_identity: ScheduledFrameIdentity,
    document_id: String,
    content_revision: u64,
}

impl NativeRgba8Readback {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn document_snapshot_id(&self) -> &str {
        self.scheduled_identity
            .evaluation_key()
            .document_snapshot_id()
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub fn context_id(&self) -> &str {
        self.scheduled_identity.evaluation_key().context_id()
    }

    pub fn frame(&self) -> u32 {
        self.scheduled_identity.evaluation_key().frame()
    }

    pub fn quality(&self) -> &str {
        self.scheduled_identity.evaluation_key().quality()
    }

    pub fn scheduled_identity(&self) -> &ScheduledFrameIdentity {
        &self.scheduled_identity
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
