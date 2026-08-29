// GitHub feedback #50 ("un effet deform encore le canvas alors qu'il doit
// pas être déformé normalement"): confirmed-by-a-prior-session symptom is
// that a Bulge/Distort effect on an Effect (adjustment) Layer visibly warps
// the safety-zone guide overlays and the canvas edge — editor-only items
// that are pushed into `scene_in.layers` at an index AFTER the effect layer
// (buildSceneJson, engine-bridge.js), and per composite_scene's own
// documented contract (engine.rs) should therefore be painted OVER the
// already-distorted accumulator, never distorted themselves.
//
// The prior investigation (issue #50's own comments) read composite_scene
// in detail and found nothing inconsistent with that contract, but stopped
// short of actually RUNNING the GPU pipeline — this test does that: it
// reproduces composite_scene's real sequence for exactly this scenario
// (content layer -> effect layer (Bulge) -> overlay layer, the same three-
// stage shape composite_scene's is_effect_layer branch + a following
// ordinary layer produces) using the EXACT same textures/pipelines/shaders
// engine.rs itself builds (blend.wgsl verbatim via include_str!, the real
// Bulge WGSL body pulled live from shader-effects-library.js, the same
// register_custom_effect wrapper template) — not a re-derived approximation.
//
// Two probes, run independently so neither contaminates the other's pixels:
//   A. "backdrop probe" — a horizontal line painted as part of the CONTENT
//      layer, i.e. BELOW the effect layer. Expected (and correct-by-design,
//      already confirmed via live A/B in the issue): this line comes out
//      CURVED. This is the positive control — if it does NOT curve, the
//      effect isn't actually reaching this y at all and the test's
//      parameters are meaningless.
//   B. "overlay probe" — the identical line, at the identical y, painted as
//      part of the OVERLAY layer instead (i.e. AFTER the effect, standing
//      in for a safety-zone guide/canvas edge). Expected if composite_scene
//      is correct: STRAIGHT. If this comes out curved too, that's a real,
//      reproduced engine-level bug in the compositing pipeline itself.
//
// Run: cd geometry-wasm && cargo test --test effect_layer_overlay_repro -- --nocapture

use serde::Deserialize;
use std::process::Command;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};
use vello::{wgpu, AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

const BLEND_SCRATCH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct GpuCtx {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
}

fn setup_gpu() -> GpuCtx {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("no native GPU adapter available for this diagnostic test");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() })
            .await
            .expect("request_device failed");
        let renderer = Renderer::new(
            &device,
            RendererOptions { antialiasing_support: AaSupport::area_only(), ..Default::default() },
        )
        .expect("renderer creation failed");
        GpuCtx { device, queue, renderer }
    })
}

#[derive(Deserialize)]
struct ShaderDef {
    id: String,
    source: String,
}

/// Pulls the REAL "Bulge" fragment body straight from the shipped shader
/// library — same technique tests/shader_library_validation.rs already
/// uses — so this test exercises the actual shader shipped to users, not a
/// hand-copied approximation that could silently drift from it.
fn fetch_bulge_source() -> String {
    let js = r#"const fs=require('fs'),vm=require('vm'); const window={}; vm.runInNewContext(fs.readFileSync(process.argv[1],'utf8'),{window}); process.stdout.write(JSON.stringify(window.SMSHADER_EFFECTS));"#;
    let output = Command::new("node")
        .args(["-e", js, "../src/js/shader-effects-library.js"])
        .output()
        .expect("Node.js is required to inspect the shader library");
    assert!(output.status.success(), "could not load shader library: {}", String::from_utf8_lossy(&output.stderr));
    let defs: Vec<ShaderDef> = serde_json::from_slice(&output.stdout).expect("shader library must be valid JSON data");
    defs.into_iter()
        .find(|d| d.id == "shader_bulge")
        .expect("shader_bulge must exist in the shipped library")
        .source
}

/// Verbatim copy of engine.rs's `register_custom_effect` wrapper template
/// (same content tests/shader_library_validation.rs's own `wrapped` uses) —
/// vs_main / bindings / Params / local_uv setup are always the same
/// regardless of author, only fs_main's body is effect-specific.
fn wrap_custom_effect(body: &str) -> String {
    format!(
        "struct VsOut {{\n    @builtin(position) pos: vec4<f32>,\n    @location(0) uv: vec2<f32>,\n}};\n\n@vertex\nfn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {{\n    var positions = array<vec2<f32>, 3>(\n        vec2<f32>(-1.0, -1.0),\n        vec2<f32>(3.0, -1.0),\n        vec2<f32>(-1.0, 3.0),\n    );\n    let p = positions[vid];\n    var out: VsOut;\n    out.pos = vec4<f32>(p, 0.0, 1.0);\n    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 1.0 - (p.y * 0.5 + 0.5));\n    return out;\n}}\n\n@group(0) @binding(0) var src_tex: texture_2d<f32>;\n@group(0) @binding(1) var tex_sampler: sampler;\n\nstruct Params {{\n    effect_id: f32,\n    p1: f32,\n    p2: f32,\n    p3: f32,\n    tex_w: f32,\n    tex_h: f32,\n    time: f32,\n    p4: f32,\n    bbox_x: f32,\n    bbox_y: f32,\n    bbox_w: f32,\n    bbox_h: f32,\n    p5: f32,\n    p6: f32,\n    p7: f32,\n    p8: f32,\n}};\n@group(0) @binding(2) var<uniform> params: Params;\n\n@fragment\nfn fs_main(in: VsOut) -> @location(0) vec4<f32> {{\n    let uv = in.uv;\n    let texel = vec2<f32>(1.0 / max(params.tex_w, 1.0), 1.0 / max(params.tex_h, 1.0));\n    let src = textureSample(src_tex, tex_sampler, uv);\n    let bbox_o = vec2<f32>(params.bbox_x, params.bbox_y);\n    let bbox_s = vec2<f32>(max(params.bbox_w, 1.0), max(params.bbox_h, 1.0));\n    let local_uv = (uv * vec2<f32>(params.tex_w, params.tex_h) - bbox_o) / bbox_s;\n{body}\n}}\n",
        body = body
    )
}

fn make_accum_texture(device: &wgpu::Device, w: u32, h: u32, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLEND_SCRATCH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

fn make_layer_texture(device: &wgpu::Device, w: u32, h: u32, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLEND_SCRATCH_FORMAT,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

fn clear_texture(device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView, color: wgpu::Color) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("clear") });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    queue.submit(Some(encoder.finish()));
}

struct BlendPipe {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
}

fn create_blend_pipe(device: &wgpu::Device) -> BlendPipe {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blend-compositor"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../src/blend.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blend-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blend-pl"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blend-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState { format: BLEND_SCRATCH_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("blend-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor { label: Some("blend-params"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    BlendPipe { pipeline, layout, sampler, uniform }
}

fn composite_pass(device: &wgpu::Device, queue: &wgpu::Queue, bp: &BlendPipe, backdrop: &wgpu::TextureView, source: &wgpu::TextureView, mode: u32, target: &wgpu::TextureView) {
    let mut payload = [0u8; 16];
    payload[0..4].copy_from_slice(&mode.to_le_bytes());
    queue.write_buffer(&bp.uniform, 0, &payload);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blend-bg"),
        layout: &bp.layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(backdrop) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(source) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&bp.sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: bp.uniform.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("composite") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("composite-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&bp.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

struct FxPipe {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
}

fn create_fx_pipe(device: &wgpu::Device, wgsl_source: &str) -> FxPipe {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("custom-fx"), source: wgpu::ShaderSource::Wgsl(wgsl_source.to_string().into()) });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fx-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("fx-pl"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fx-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState { format: BLEND_SCRATCH_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("fx-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor { label: Some("fx-params"), size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    FxPipe { pipeline, layout, sampler, uniform }
}

#[allow(clippy::too_many_arguments)]
fn run_fx_pass(device: &wgpu::Device, queue: &wgpu::Queue, fp: &FxPipe, source: &wgpu::TextureView, p1: f32, p2: f32, tex_w: f32, tex_h: f32, bbox: (f32, f32, f32, f32), target: &wgpu::TextureView) {
    let mut payload = [0u8; 64];
    payload[0..4].copy_from_slice(&0f32.to_le_bytes()); // effect_id (unused by custom shaders)
    payload[4..8].copy_from_slice(&p1.to_le_bytes());
    payload[8..12].copy_from_slice(&p2.to_le_bytes());
    payload[12..16].copy_from_slice(&0f32.to_le_bytes()); // p3
    payload[16..20].copy_from_slice(&tex_w.to_le_bytes());
    payload[20..24].copy_from_slice(&tex_h.to_le_bytes());
    payload[24..28].copy_from_slice(&0f32.to_le_bytes()); // time
    payload[28..32].copy_from_slice(&0f32.to_le_bytes()); // p4
    payload[32..36].copy_from_slice(&bbox.0.to_le_bytes());
    payload[36..40].copy_from_slice(&bbox.1.to_le_bytes());
    payload[40..44].copy_from_slice(&bbox.2.to_le_bytes());
    payload[44..48].copy_from_slice(&bbox.3.to_le_bytes());
    queue.write_buffer(&fp.uniform, 0, &payload);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fx-bg"),
        layout: &fp.layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&fp.sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: fp.uniform.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fx-pass") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fx-render-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&fp.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

fn readback(ctx: &GpuCtx, tex: &wgpu::Texture, width: u32, height: u32) -> Vec<u8> {
    let unpadded = width as usize * 4;
    let padded = unpadded.div_ceil(256) * 256;
    let buf_size = (padded * height as usize) as u64;
    let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: buf_size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: Some(height) } },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    ctx.queue.submit(Some(encoder.finish()));
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res| { let _ = tx.send(res); });
    ctx.device.poll(wgpu::PollType::wait_indefinitely()).expect("device poll failed");
    rx.recv().expect("map channel dropped").expect("buffer map failed");
    let mapped = slice.get_mapped_range();
    let mut out = Vec::with_capacity(unpadded * height as usize);
    for row in 0..height as usize {
        let start = row * padded;
        out.extend_from_slice(&mapped[start..start + unpadded]);
    }
    drop(mapped);
    buf.unmap();
    out
}

/// "Blueness"-weighted centroid row (y) of the probe line in column `x` —
/// used to trace where the line "landed" after the pipeline, per column.
/// NOT alpha-weighted: the accumulator is seeded with an OPAQUE white base
/// (alpha=255 everywhere, matching composite_scene's own `clear_texture(...,
/// base_color)` seed), so alpha alone can't distinguish the line from the
/// background. The probe line is a distinct, highly saturated blue
/// (20,90,220) against that white field — (blue - red) is ~0 for white
/// background pixels and ~200 for line pixels, regardless of alpha.
fn line_y_at_x(pixels: &[u8], width: u32, height: u32, x: u32) -> Option<f32> {
    let mut sum_w = 0f32;
    let mut sum_wy = 0f32;
    for y in 0..height {
        let idx = ((y * width + x) * 4) as usize;
        let r = pixels[idx] as f32;
        let b = pixels[idx + 2] as f32;
        let w = (b - r).max(0.0);
        if w > 30.0 {
            sum_w += w;
            sum_wy += w * y as f32;
        }
    }
    if sum_w > 0.0 {
        Some(sum_wy / sum_w)
    } else {
        None
    }
}

fn std_dev(vals: &[f32]) -> f32 {
    let mean = vals.iter().sum::<f32>() / vals.len() as f32;
    let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32;
    var.sqrt()
}

/// Reproduces composite_scene's real sequence for "content layer -> effect
/// layer (Bulge) -> overlay layer": if `line_in_backdrop` the horizontal
/// probe line is part of the CONTENT layer (painted before/below the
/// effect); if `line_in_overlay` it's part of the OVERLAY layer (painted
/// after, standing in for a safety-zone guide). Returns the per-column
/// line-y trace across the canvas width.
fn run_scenario(ctx: &mut GpuCtx, fp: &FxPipe, bp: &BlendPipe, w: u32, h: u32, line_y: f64, line_in_backdrop: bool, line_in_overlay: bool) -> Vec<f32> {
    let (accum_a_tex, accum_a_view) = make_accum_texture(&ctx.device, w, h, "accum-a");
    let (accum_b_tex, accum_b_view) = make_accum_texture(&ctx.device, w, h, "accum-b");
    let (_layer_tex, layer_view) = make_layer_texture(&ctx.device, w, h, "layer-scratch");

    // Seed accumulator with an opaque white base, exactly like
    // composite_scene's `clear_texture(blend_accum_a_view, base_color)`.
    clear_texture(&ctx.device, &ctx.queue, &accum_a_view, wgpu::Color::WHITE);

    let layer_params = RenderParams { base_color: Color::TRANSPARENT, width: w, height: h, antialiasing_method: AaConfig::Area };
    let line_color = Color::from_rgba8(20, 90, 220, 255);
    let line_thickness = 6.0;

    // ---- Layer 0: content (below the effect) ----
    let mut content_scene = Scene::new();
    if line_in_backdrop {
        let rect = Rect::new(10.0, line_y - line_thickness / 2.0, w as f64 - 10.0, line_y + line_thickness / 2.0);
        content_scene.fill(Fill::NonZero, Affine::IDENTITY, line_color, None, &rect);
    }
    ctx.renderer.render_to_texture(&ctx.device, &ctx.queue, &content_scene, &layer_view, &layer_params).expect("render_to_texture failed (content)");
    composite_pass(&ctx.device, &ctx.queue, bp, &accum_a_view, &layer_view, 0, &accum_b_view);
    // accum now lives in accum_b (mirrors composite_scene's accum_is_a flip)

    // ---- Layer 1: Effect (adjustment) layer — Bulge, default-ish params ----
    // bbox = full canvas, matching document_bbox_px for an identity
    // viewport (pan=0, zoom=1) — the common case, and the one the issue's
    // own repro used (no unusual zoom/pan in its action trail).
    let bbox = (0.0f32, 0.0f32, w as f32, h as f32);
    // Shipped defaults (shader-effects-library.js: Amount=0.4, Radius=0.65)
    // — the issue's own repro trail shows no custom effect-parameter tweak,
    // so this is the realistic case, not an artificially extreme one.
    run_fx_pass(&ctx.device, &ctx.queue, fp, &accum_b_view, 0.4, 0.65, w as f32, h as f32, bbox, &accum_a_view);
    // accum now lives in accum_a

    // ---- Layer 2: overlay (safety-zone-guide stand-in, after the effect) ----
    let mut overlay_scene = Scene::new();
    if line_in_overlay {
        let rect = Rect::new(10.0, line_y - line_thickness / 2.0, w as f64 - 10.0, line_y + line_thickness / 2.0);
        overlay_scene.fill(Fill::NonZero, Affine::IDENTITY, line_color, None, &rect);
    }
    ctx.renderer.render_to_texture(&ctx.device, &ctx.queue, &overlay_scene, &layer_view, &layer_params).expect("render_to_texture failed (overlay)");
    composite_pass(&ctx.device, &ctx.queue, bp, &accum_a_view, &layer_view, 0, &accum_b_view);
    // final result lives in accum_b, exactly like composite_scene's final_tex

    let pixels = readback(ctx, &accum_b_tex, w, h);
    let _ = &accum_a_tex; // keep alive until readback completes

    let xs: Vec<u32> = (0..8).map(|i| 20 + i * (w - 40) / 7).collect();
    xs.iter().filter_map(|&x| line_y_at_x(&pixels, w, h, x)).collect()
}

#[test]
fn overlay_layer_after_effect_layer_stays_straight() {
    let mut ctx = setup_gpu();
    let body = fetch_bulge_source();
    let wgsl = wrap_custom_effect(&body);
    // Fail loudly (not silently) if the shipped Bulge shader doesn't even
    // compile against this test's harness — a stale wrapper here would
    // otherwise make every downstream measurement meaningless.
    let fp = create_fx_pipe(&ctx.device, &wgsl);
    let bp = create_blend_pipe(&ctx.device);

    let (w, h) = (400u32, 400u32);
    // Well within Bulge's default radius (0.65, i.e. reaches ~260px from
    // center) without being pushed past the canvas edge by the warp itself
    // (which a probe placed too close to the boundary would be, clamped
    // there and hard to trace) — r_local = 100/400 = 0.25 from center.
    let line_y = 300.0;

    let backdrop_trace = run_scenario(&mut ctx, &fp, &bp, w, h, line_y, true, false);
    let overlay_trace = run_scenario(&mut ctx, &fp, &bp, w, h, line_y, false, true);

    let backdrop_found = backdrop_trace.len();
    let overlay_found = overlay_trace.len();
    println!("backdrop probe (line BELOW effect): {backdrop_found}/8 columns found, y-trace = {backdrop_trace:?}");
    println!("overlay probe  (line AFTER effect):  {overlay_found}/8 columns found, y-trace = {overlay_trace:?}");

    assert!(backdrop_found >= 6, "positive control failed: the backdrop line barely rendered at all ({backdrop_found}/8) — effect parameters or harness are wrong, results below are meaningless");
    let backdrop_stdev = std_dev(&backdrop_trace);
    println!("backdrop probe y-stdev across columns: {backdrop_stdev:.3}px (expected LARGE — this line is BELOW the effect, by-design curving)");
    assert!(
        backdrop_stdev > 1.0,
        "positive control failed: the backdrop line came out essentially straight (stdev={backdrop_stdev:.3}px) — \
         the Bulge effect isn't actually warping content at this position/radius in this harness, so the overlay \
         probe below proves nothing either way"
    );

    assert!(overlay_found >= 6, "overlay line barely rendered at all ({overlay_found}/8) — harness issue, not a distortion finding");
    let overlay_stdev = std_dev(&overlay_trace);
    println!("overlay probe y-stdev across columns: {overlay_stdev:.3}px (expected NEAR ZERO if composite_scene's ordering contract holds)");
    assert!(
        overlay_stdev < 1.0,
        "REPRODUCED at the engine level: a layer painted AFTER an Effect Layer (Bulge) in composite_scene's own \
         sequence still comes out curved (stdev={overlay_stdev:.3}px across columns, trace={overlay_trace:?}) — \
         this is a real bug in the GPU compositing pipeline, not just JS-side layer ordering."
    );
}
