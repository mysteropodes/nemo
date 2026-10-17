//! The two retained fullscreen passes of the layered path: `blend.wgsl`
//! (W3C mix + Porter-Duff SrcOver) and `matte.wgsl` (alpha/luma multiply),
//! plus the clear and copy helpers the baseline compositor used between them.
//!
//! The shader text is the baseline's own, taken by `include_str!` from
//! `geometry-wasm/src` (no copy). The pipeline layout, sampler, uniform size
//! and pass shape are the baseline `create_blend_pipeline` / `matte_pass`
//! (`geometry-wasm/src/engine.rs`), kept identical so the two engines cannot
//! drift: every texture is straight-alpha Rgba8Unorm, sampled nearest.

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BLEND_WGSL: &str = include_str!("../../geometry-wasm/src/blend.wgsl");
const MATTE_WGSL: &str = include_str!("../../geometry-wasm/src/matte.wgsl");

struct PassPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
}

/// Device-bound pipelines, built once per compositor and shared by `Arc`.
pub(crate) struct BlendKernels {
    device: wgpu::Device,
    queue: wgpu::Queue,
    sampler: wgpu::Sampler,
    blend: PassPipeline,
    matte: PassPipeline,
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// Two textures + sampler + one 16-byte uniform, as `blend.wgsl`/`matte.wgsl`
/// both declare (`@group(0) @binding(0..3)`).
fn pass_pipeline(device: &wgpu::Device, label: &'static str, wgsl: &'static str) -> PassPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            texture_entry(0),
            texture_entry(1),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
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
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    PassPipeline {
        pipeline,
        layout,
        uniform,
    }
}

impl BlendKernels {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene-effect-gpu-blend-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            device: device.clone(),
            queue: queue.clone(),
            sampler,
            blend: pass_pipeline(device, "scene-effect-blend", BLEND_WGSL),
            matte: pass_pipeline(device, "scene-effect-matte", MATTE_WGSL),
        }
    }

    /// `source` over `backdrop` with mix `mode` into `target` (blend.wgsl).
    pub(crate) fn blend(
        &self,
        backdrop: &wgpu::TextureView,
        source: &wgpu::TextureView,
        mode: u32,
        target: &wgpu::TextureView,
    ) {
        let mut payload = [0u8; 16];
        payload[0..4].copy_from_slice(&mode.to_le_bytes());
        self.run(&self.blend, backdrop, source, payload, target);
    }

    /// `layer` alpha multiplied by the matte's alpha (`mode` 0) or luma
    /// (`mode` 1), optionally inverted, into `target` (matte.wgsl).
    pub(crate) fn matte(
        &self,
        layer: &wgpu::TextureView,
        matte: &wgpu::TextureView,
        (mode, invert): (u32, bool),
        target: &wgpu::TextureView,
    ) {
        let mut payload = [0u8; 16];
        payload[0..4].copy_from_slice(&mode.to_le_bytes());
        payload[4..8].copy_from_slice(&u32::from(invert).to_le_bytes());
        self.run(&self.matte, layer, matte, payload, target);
    }

    fn run(
        &self,
        pass: &PassPipeline,
        first: &wgpu::TextureView,
        second: &wgpu::TextureView,
        payload: [u8; 16],
        target: &wgpu::TextureView,
    ) {
        self.queue.write_buffer(&pass.uniform, 0, &payload);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-effect-pass-bind-group"),
            layout: &pass.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(first),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(second),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: pass.uniform.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene-effect-pass"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene-effect-pass"),
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

    /// Flat colour into `view` through a plain render pass (baseline
    /// `clear_texture`); `view` needs `RENDER_ATTACHMENT`.
    pub(crate) fn clear(&self, view: &wgpu::TextureView, color: wgpu::Color) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene-effect-clear"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene-effect-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Whole-texture copy (`source` COPY_SRC, `target` COPY_DST).
    pub(crate) fn copy(
        &self,
        source: &wgpu::Texture,
        target: &wgpu::Texture,
        (width, height): (u32, u32),
    ) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene-effect-copy"),
            });
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            target.as_image_copy(),
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
    }
}
