//! The retained single-input effect passes: `color_adjust.wgsl`, `vignette.wgsl`,
//! the separable Gaussian `blur.wgsl` and (L08-I, built on first use) the
//! generic `simple_fx.wgsl`.
//!
//! The shader text is the baseline's own, taken by `include_str!` from
//! `geometry-wasm/src` (no copy). The bind-group layout (source texture,
//! filtering sampler, one uniform), the linear clamp-to-edge sampler, the
//! uniform sizes (16 bytes colour/vignette, 32 bytes blur) and the pass shape
//! (clear to transparent, one fullscreen triangle, one submit per pass) are the
//! baseline `create_*_pipeline` / `*_pass` functions of
//! `geometry-wasm/src/engine.rs` and `engine/color_adjust.rs`, kept identical
//! so the two engines cannot drift. Every texture is straight-alpha Rgba8Unorm.

use crate::compositor::scene_effect_gpu_fx::FX_PAYLOAD_BYTES;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const COLOR_ADJUST_WGSL: &str = include_str!("../../geometry-wasm/src/color_adjust.wgsl");
const VIGNETTE_WGSL: &str = include_str!("../../geometry-wasm/src/vignette.wgsl");
const BLUR_WGSL: &str = include_str!("../../geometry-wasm/src/blur.wgsl");
const SIMPLE_FX_WGSL: &str = include_str!("../../geometry-wasm/src/simple_fx.wgsl");

/// `Params` of color_adjust.wgsl and vignette.wgsl: two floats plus padding.
const SMALL_UNIFORM_BYTES: u64 = 16;
/// `Params` of blur.wgsl: radius, texture size, direction and padding.
const BLUR_UNIFORM_BYTES: u64 = 32;
/// `Params` of simple_fx.wgsl: sixteen floats (effect id, p1..p8, size, time, box).
const FX_UNIFORM_BYTES: u64 = FX_PAYLOAD_BYTES as u64;

struct Pass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
}

/// Device-bound pipelines, built once per compositor and shared by `Arc`.
pub(crate) struct EffectKernels {
    device: wgpu::Device,
    queue: wgpu::Queue,
    sampler: wgpu::Sampler,
    color: Pass,
    vignette: Pass,
    blur: Pass,
    /// `simple_fx.wgsl`, compiled on the first fx pass.
    fx: std::sync::OnceLock<Pass>,
}

fn pass(
    device: &wgpu::Device,
    label: &'static str,
    wgsl: &'static str,
    uniform_bytes: u64,
) -> Pass {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: uniform_bytes,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    Pass {
        pipeline,
        layout,
        uniform,
    }
}

impl EffectKernels {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        // Linear filtering: the blur WANTS bilinear interpolation between its
        // taps when the sample step is fractional (blur.wgsl header).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene-effect-gpu-pass-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            device: device.clone(),
            queue: queue.clone(),
            sampler,
            color: pass(
                device,
                "scene-effect-color-adjust",
                COLOR_ADJUST_WGSL,
                SMALL_UNIFORM_BYTES,
            ),
            vignette: pass(
                device,
                "scene-effect-vignette",
                VIGNETTE_WGSL,
                SMALL_UNIFORM_BYTES,
            ),
            blur: pass(device, "scene-effect-blur", BLUR_WGSL, BLUR_UNIFORM_BYTES),
            fx: std::sync::OnceLock::new(),
        }
    }

    /// `brightness` (additive) and `contrast` (-1..1) into `target`
    /// (baseline `color_adjust_pass`).
    pub(crate) fn color_adjust(
        &self,
        source: &wgpu::TextureView,
        brightness: f32,
        contrast: f32,
        target: &wgpu::TextureView,
    ) {
        let mut payload = [0u8; 16];
        payload[0..4].copy_from_slice(&brightness.to_le_bytes());
        payload[4..8].copy_from_slice(&contrast.to_le_bytes());
        self.run(&self.color, source, &payload, target);
    }

    /// Edge darkening with `strength` and start `radius` (baseline
    /// `vignette_pass`).
    pub(crate) fn vignette(
        &self,
        source: &wgpu::TextureView,
        strength: f32,
        radius: f32,
        target: &wgpu::TextureView,
    ) {
        let mut payload = [0u8; 16];
        payload[0..4].copy_from_slice(&strength.to_le_bytes());
        payload[4..8].copy_from_slice(&radius.to_le_bytes());
        self.run(&self.vignette, source, &payload, target);
    }

    /// One `simple_fx.wgsl` pass: `payload` is the 64-byte `Params` built by
    /// `scene_effect_gpu_fx::fx_payload` (baseline `simple_fx_pass`); the pipeline
    /// is created on first use.
    pub(crate) fn simple_fx(
        &self,
        source: &wgpu::TextureView,
        payload: &[u8; FX_PAYLOAD_BYTES],
        target: &wgpu::TextureView,
    ) {
        let fx_pass = self.fx.get_or_init(|| {
            pass(
                &self.device,
                "scene-effect-simple-fx",
                SIMPLE_FX_WGSL,
                FX_UNIFORM_BYTES,
            )
        });
        self.run(fx_pass, source, payload, target);
    }

    /// Separable Gaussian: a horizontal pass `source` -> `scratch`, then a
    /// vertical pass `scratch` -> `target` (baseline `blur_pass`). `scratch`
    /// must differ from `source` and `target`.
    pub(crate) fn blur(
        &self,
        source: &wgpu::TextureView,
        radius_px: f32,
        size: (u32, u32),
        scratch: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        self.blur_axis(source, radius_px, size, (1.0, 0.0), scratch);
        self.blur_axis(scratch, radius_px, size, (0.0, 1.0), target);
    }

    fn blur_axis(
        &self,
        source: &wgpu::TextureView,
        radius_px: f32,
        (width, height): (u32, u32),
        (dir_x, dir_y): (f32, f32),
        target: &wgpu::TextureView,
    ) {
        let mut payload = [0u8; 32];
        payload[0..4].copy_from_slice(&radius_px.to_le_bytes());
        payload[4..8].copy_from_slice(&(width as f32).to_le_bytes());
        payload[8..12].copy_from_slice(&(height as f32).to_le_bytes());
        payload[12..16].copy_from_slice(&dir_x.to_le_bytes());
        payload[16..20].copy_from_slice(&dir_y.to_le_bytes());
        self.run(&self.blur, source, &payload, target);
    }

    /// One fullscreen pass. The uniform is written and the pass submitted
    /// before the next pass writes it again (queue order), as the baseline does.
    fn run(
        &self,
        pass: &Pass,
        source: &wgpu::TextureView,
        payload: &[u8],
        target: &wgpu::TextureView,
    ) {
        self.queue.write_buffer(&pass.uniform, 0, payload);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-effect-gpu-pass-bind-group"),
            layout: &pass.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pass.uniform.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene-effect-gpu-pass"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene-effect-gpu-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            render_pass.set_pipeline(&pass.pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }
}
