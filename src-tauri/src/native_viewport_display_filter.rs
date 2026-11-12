//! Closed, display-only transforms on encoded unorm compositor output.
//! No renderer/export dependency, new context, source writes or CPU readback.
use native_engine::desktop_viewport::ViewportDisplayFilter;

pub(super) struct DisplayFilterBlitter {
    unfiltered: wgpu::util::TextureBlitter,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    values: wgpu::RenderPipeline,
    contrast: wgpu::RenderPipeline,
    dim: wgpu::RenderPipeline,
    /// Copy, values, contrast and dim, each sampled through `vs_mirror`.
    mirrored: [wgpu::RenderPipeline; 4],
}

impl DisplayFilterBlitter {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        // The surface contract excludes sRGB attachments: these bytes are already encoded.
        assert!(matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
        ));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("native-display-filter-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("native-display-filter-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("native_viewport_display_filter.wgsl"));
        let stage = |vertex, entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = |entry| stage("vs_main", entry);
        let mirrored = ["fs_copy", "fs_values", "fs_contrast", "fs_dim"]
            .map(|entry| stage("vs_mirror", entry));
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        Self {
            unfiltered: wgpu::util::TextureBlitter::new(device, format),
            values: pipeline("fs_values"),
            contrast: pipeline("fs_contrast"),
            dim: pipeline("fs_dim"),
            mirrored,
            layout,
            sampler,
        }
    }

    pub(super) fn copy(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        filter: ViewportDisplayFilter,
    ) {
        let pipeline = match filter {
            ViewportDisplayFilter::None => {
                self.unfiltered.copy(device, encoder, source, target);
                return;
            }
            ViewportDisplayFilter::Values => &self.values,
            ViewportDisplayFilter::Contrast => &self.contrast,
            ViewportDisplayFilter::Dim => &self.dim,
            ViewportDisplayFilter::Mirrored => &self.mirrored[0],
            ViewportDisplayFilter::ValuesMirrored => &self.mirrored[1],
            ViewportDisplayFilter::ContrastMirrored => &self.mirrored[2],
            ViewportDisplayFilter::DimMirrored => &self.mirrored[3],
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("native-display-filter-source"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("native-display-filter-final-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
#[path = "native_viewport_display_filter_tests.rs"]
mod tests;
