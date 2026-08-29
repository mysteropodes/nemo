// Feedback #104 ("le smooth sur une stroke... ça casse la courbe") root-
// cause repro. The user-visible symptom traces back to a Rust/vello render
// bug, NOT the smoothing/geometry code — see the linked GitHub issue and
// CLAUDE.md §3. This test isolates the failure at the vello layer: does
// `Renderer::render_to_texture` (the EXACT call `composite_scene`/
// `paint_layer_items` in src/engine.rs use — same params, same AA config,
// no push_layer/clip groups) silently drop part of a scene made of many
// small overlapping semi-transparent filled paths — the shape a densely-
// dabbed textured brush stroke takes (hundreds of small companion Path
// items stamped along an invisible anchor, see CLAUDE.md §1)?
//
// Two checks, cheapest first:
//   1. CPU-only, no GPU: vello's own `Scene::bump_estimate` (feature
//      "bump_estimate", dev-dependency-only override in Cargo.toml — see
//      the comment there) against the FIXED buffer sizes
//      `vello_encoding::config::BufferSizes::new` hard-codes. That file's
//      own comment: "these buffer sizes have been hand picked to
//      accommodate the vello test scenes... should instead get derived
//      from the scene layout using reasonable heuristics" — i.e. vello
//      admits these are NOT adaptive. `Renderer::render_to_texture` (via
//      `render_encoding_full`) always passes `robust: false`, so there is
//      no readback/detection/retry when a scene's actual demand exceeds
//      these fixed sizes — an overflow is silently truncated, and nothing
//      gets logged (the ONLY warning vello emits, `render.rs`'s "Trying to
//      paint too large image", fires on canvas width×height, not path/tile
//      count — confirmed by reading render.rs directly).
//   2. GPU pixel readback: render the same scene for real, read back
//      pixels, and check whether dabs past some point in paint order are
//      visibly missing while earlier ones render fine — the actual visual
//      symptom from the issue ("only the top portion of the curve
//      renders").
//
// Run: cd geometry-wasm && cargo test --test dab_dense_overlap_repro -- --nocapture

use vello::kurbo::{Affine, Circle};
use vello::peniko::{Color, Fill};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};

/// Mirrors `paint_layer_items`'s per-item fill call in src/engine.rs — one
/// filled circle per "dab", semi-transparent (~0.45 alpha, matching the
/// opacity range the user measured on real dab companions), packed far
/// closer together than their radius so they heavily overlap along a
/// vertical S-curve — exactly the shape a densely-dabbed brush stroke
/// takes. No push_layer/clip groups (geometry-wasm never uses vello's own
/// layer mechanism — see the blend.wgsl comment in engine.rs), so this
/// isolates the plain fill path, nothing else.
fn build_dab_scene(count: usize, canvas_w: f64, canvas_h: f64, radius: f64) -> (Scene, Vec<(f64, f64)>) {
    let mut scene = Scene::new();
    let mut centers = Vec::with_capacity(count);
    let margin = radius * 2.0;
    let y0 = margin;
    let y1 = canvas_h - margin;
    let cx = canvas_w / 2.0;
    let amp = (canvas_w / 2.0 - margin).max(0.0).min(80.0);
    let color = Color::from_rgba8(60, 40, 30, 115);
    for i in 0..count {
        let t = if count > 1 { i as f64 / (count - 1) as f64 } else { 0.0 };
        let y = y0 + t * (y1 - y0);
        let x = cx + amp * (t * std::f64::consts::PI * 3.0).sin();
        let circle = Circle::new((x, y), radius);
        scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &circle);
        centers.push((x, y));
    }
    (scene, centers)
}

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
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: adapter.limits(),
                ..Default::default()
            })
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

/// Renders `scene` via the SAME entry point engine.rs uses
/// (`render_to_texture`, robust=false internally, `AaConfig::Area`) and
/// reads back straight-alpha RGBA8 pixels — same padded-row-copy technique
/// as `render_to_pixels` in src/engine.rs.
fn render_and_readback(ctx: &mut GpuCtx, scene: &Scene, width: u32, height: u32) -> Vec<u8> {
    let tex = ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("dab-repro-offscreen"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let params = RenderParams { base_color: Color::TRANSPARENT, width, height, antialiasing_method: AaConfig::Area };
    ctx.renderer
        .render_to_texture(&ctx.device, &ctx.queue, scene, &view, &params)
        .expect("render_to_texture failed");

    let unpadded = width as usize * 4;
    let padded = unpadded.div_ceil(256) * 256;
    let buf_size = (padded * height as usize) as u64;
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dab-repro-readback"),
        size: buf_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: Some(height) },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    ctx.queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    ctx.device.poll(wgpu::PollType::wait_indefinitely()).expect("device poll failed");
    rx.recv().expect("map channel dropped").expect("buffer map failed");
    let mapped = slice.get_mapped_range();
    let mut out = Vec::with_capacity(unpadded * height as usize);
    for row in 0..height as usize {
        let start = row * padded;
        out.extend_from_slice(&mapped[start..start + unpadded]);
    }
    drop(mapped);
    readback.unmap();
    out
}

fn alpha_at(pixels: &[u8], width: u32, x: f64, y: f64) -> u8 {
    let xi = (x.round() as i64).clamp(0, width as i64 - 1) as usize;
    let yi = (y.round() as i64).clamp(0, i64::MAX) as usize;
    let idx = (yi * width as usize + xi) * 4 + 3;
    pixels[idx]
}

/// Check 1 — CPU-only. Prints vello's own bump-allocator estimate for a
/// 756-dab scene (the exact count from the issue's repro) against the
/// fixed sizes `BufferSizes::new` hard-codes, so there's no ambiguity
/// about which (if any) buffer the scene's real demand exceeds.
#[test]
fn bump_estimate_vs_fixed_vello_buffers() {
    // vello_encoding::config::BufferSizes::new (vello 0.9.0, not derived
    // from the scene — literal constants, see that file):
    const BIN_DATA: u32 = 1 << 18; // 262_144 u32 elements
    const TILES: u32 = 1 << 21; // 2_097_152 Tile elements
    const LINES: u32 = 1 << 21;
    const SEG_COUNTS: u32 = 1 << 21;
    const SEGMENTS: u32 = 1 << 21;
    const BLEND_SPILL: u32 = 1 << 20;
    const PTCL: u32 = 1 << 23; // 8_388_608 u32 elements

    let (canvas_w, canvas_h) = (1200.0, 1200.0);
    for &count in &[756usize, 2000, 5000] {
        let (scene, _) = build_dab_scene(count, canvas_w, canvas_h, 15.0);
        let est = scene.bump_estimate(None);
        println!(
            "dabs={count:>5}  est.binning={:>10} (cap {BIN_DATA})  est.tile={:>10} (cap {TILES})  \
             est.seg_counts={:>10} (cap {SEG_COUNTS})  est.segments={:>10} (cap {SEGMENTS})  \
             est.lines={:>10} (cap {LINES})  est.ptcl={:>10} (cap {PTCL})",
            est.binning.len(),
            est.tile.len(),
            est.seg_counts.len(),
            est.segments.len(),
            est.lines.len(),
            est.ptcl.len(),
        );
        let over = est.binning.len() > BIN_DATA
            || est.tile.len() > TILES
            || est.seg_counts.len() > SEG_COUNTS
            || est.segments.len() > SEGMENTS
            || est.lines.len() > LINES
            || est.ptcl.len() > PTCL;
        println!("  -> exceeds a fixed buffer: {over}");
        let _ = BLEND_SPILL; // not populated by bump_estimate (clip-group-only, unused here)
    }
}

/// Check 2 — real GPU render + pixel readback, same call engine.rs makes.
/// Renders the 756-dab scene and samples the pixel at every dab's own
/// center, reporting the first missing dab (if any) and whether the
/// missing set is a clean "everything past index K" suffix — which is
/// exactly the "top of the curve renders, bottom doesn't" symptom from the
/// issue.
#[test]
fn dense_dab_stroke_render_gap_repro() {
    let mut ctx = setup_gpu();
    // Sweep count/radius/canvas size well past the issue's own numbers
    // (756 dabs, y 225..1045) to find whether ANY plain-overlapping-fill
    // scene at plausible-or-generous scale trips a render gap — including
    // radii large enough to simulate a heavily zoomed-in view (a bigger
    // on-screen dab bbox spans far more GPU tiles per item than at 100%).
    let scenarios: &[(usize, f64, f64, f64)] = &[
        // (count, radius, canvas_w, canvas_h)
        (756, 15.0, 1200.0, 1200.0),
        (756, 60.0, 1200.0, 1200.0),
        (756, 150.0, 2400.0, 2400.0),
        (5_000, 60.0, 2400.0, 2400.0),
        (20_000, 30.0, 2400.0, 2400.0),
        (50_000, 15.0, 2400.0, 2400.0),
        (100_000, 15.0, 2400.0, 2400.0),
    ];

    for &(count, radius, canvas_w, canvas_h) in scenarios {
        let (scene, centers) = build_dab_scene(count, canvas_w, canvas_h, radius);
        let pixels = render_and_readback(&mut ctx, &scene, canvas_w as u32, canvas_h as u32);

        let mut missing = Vec::new();
        let mut present = 0usize;
        for (i, &(x, y)) in centers.iter().enumerate() {
            let a = alpha_at(&pixels, canvas_w as u32, x, y);
            if a == 0 {
                missing.push(i);
            } else {
                present += 1;
            }
        }
        print!(
            "count={count:>7} radius={radius:>5} canvas={canvas_w}x{canvas_h}  present={present:>7} missing={:>7}",
            missing.len()
        );
        if let Some(&first_missing) = missing.first() {
            let suffix_from_first = missing.iter().enumerate().all(|(k, &idx)| idx == first_missing + k);
            println!(
                "  <-- GAP first_missing={first_missing} (y={:.1}) clean_suffix={suffix_from_first}",
                centers[first_missing].1
            );
        } else {
            println!();
        }
    }

    // Not an assert(!) — this test's job is to report ground truth for the
    // investigation, not to gate CI on a still-being-diagnosed vello
    // behavior. See the accompanying report for the conclusion drawn from
    // this output.
}
