// Phase C1 — engine skeleton: a minimal scene graph + GPU rasterizer (vello,
// via WebGPU/wgpu), built and verified in isolation (own test harness, own
// throwaway canvas) before any live-app wiring. This is the seed of what
// eventually replaces Paper.js entirely (see the project's migration plan).
//
// vello needs an owned WebGPU device+surface bound to a real <canvas>
// element, so unlike boolean.rs/fill.rs (stateless JSON-in/JSON-out
// functions) this is a stateful #[wasm_bindgen] object: JS calls
// `create_engine(canvas, w, h)` once (async — WebGPU device negotiation is
// inherently async) and keeps the returned handle to call `.render(json)`
// on every frame.
use serde::{Deserialize, Serialize};
use vello::kurbo::{Affine, BezPath, Rect, Shape, Stroke};
use vello::peniko::Color;
use vello::{wgpu, AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ItemIn {
    #[serde(default)]
    pub(crate) segments: Vec<SegIn>,
    #[serde(default)]
    pub(crate) closed: bool,
    #[serde(default)]
    pub(crate) fill_color: Option<[u8; 4]>,
    #[serde(default)]
    pub(crate) stroke_color: Option<[u8; 4]>,
    #[serde(default = "default_stroke_width")]
    pub(crate) stroke_width: f64,
    // Stroke style detail (Properties panel Cap/Join/Miter Limit/Dash/Paint
    // Order, wired to actually take effect on the live-rendered stroke —
    // previously these were UI-only decoration since ItemIn/render() only
    // ever built a bare `Stroke::new(width)` with kurbo's defaults,
    // regardless of what the panel showed. All optional/defaulted so older
    // scene JSON (or items that don't care) keeps working unchanged.
    #[serde(default)]
    pub(crate) stroke_cap: Option<String>,
    #[serde(default)]
    pub(crate) stroke_join: Option<String>,
    #[serde(default)]
    pub(crate) miter_limit: Option<f64>,
    #[serde(default)]
    pub(crate) dash_pattern: Option<Vec<f64>>,
    #[serde(default)]
    pub(crate) dash_offset: Option<f64>,
    // "fillFirst" (default, matches every prior render) or "strokeFirst" —
    // Graphite's "Paint Order" toggle, which for a single fill+stroke path
    // is really just whether the stroke's inner half is visible over the
    // fill or hidden under it.
    #[serde(default)]
    pub(crate) paint_order: Option<String>,
    // Pressure-brush strokes (Draw tool, Phase C2): an ordered centerline
    // with a per-sample width, ported from the same approach as
    // buildVariableWidthPath in tools.js — a filled ribbon, not a stroked
    // line, exactly matching how the JS/Paper.js side already treats these
    // (fillColor set, strokeColor null). Takes priority over `segments`
    // when present.
    #[serde(default)]
    pub(crate) centerline: Option<Vec<[f64; 3]>>,
    // Bitmap items (imported images / Paper.js Raster): takes priority over
    // both `segments` and `centerline` when present. Only a reference
    // (`imageId`) to a previously-`register_image()`-uploaded texture is
    // sent per frame, not the pixel bytes themselves — re-sending a whole
    // image's raw RGBA in the scene JSON on every render call (which rebuilds
    // the full scene from scratch each frame) would be enormously wasteful
    // for anything beyond a tiny thumbnail.
    #[serde(default)]
    pub(crate) image: Option<ImageRef>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageRef {
    pub(crate) image_id: String,
    // top-left placement + DISPLAY size (world units) — may differ from the
    // image's own natural pixel dimensions, requiring a scale in the draw
    // transform (Paper.js's Raster is center-positioned; JS converts to
    // top-left before sending, so this side stays a simple rect).
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    #[serde(default = "default_opacity")]
    pub(crate) opacity: f32,
}
fn default_opacity() -> f32 {
    1.0
}
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SegIn {
    pub(crate) point: [f64; 2],
    #[serde(default)]
    pub(crate) handle_in: [f64; 2],
    #[serde(default)]
    pub(crate) handle_out: [f64; 2],
}
fn default_stroke_width() -> f64 {
    1.0
}

#[derive(Deserialize, Serialize)]
pub(crate) struct LayerIn {
    pub(crate) items: Vec<ItemIn>,
    // Photoshop/Illustrator-style layer blend mode — composites this
    // whole layer's painted pixels against everything already drawn
    // beneath it. Applied via a push_layer/pop_layer bracket around the
    // WHOLE layer's items (not per-item), since blend mode is conventionally
    // a layer-level property, matching the Properties panel UI it's wired
    // from (click a layer -> blend mode dropdown). None/"normal" skips the
    // bracket entirely — cheap default path, no vello layer-stack overhead
    // for the (overwhelmingly common) unblended case.
    #[serde(default)]
    pub(crate) blend_mode: Option<String>,
}

// KNOWN BROKEN (v17 investigation, not fixed): layer blend modes render
// with zero visual effect. Root-caused via direct experimentation (NOT a
// JS wiring bug — confirmed the JSON payload correctly carries the right
// blendMode string on the right layer every time): a push_layer(Mix::X)
// group only sees a correct backdrop to blend against when EVERY layer
// beneath it was ALSO wrapped in its own push_layer/pop_layer using a
// non-Normal Mix. Wrapping every layer in Mix::Normal (Vello's own
// documented "no blend" default) for the unblended ones does NOT work —
// the blended layer still renders as if the backdrop were empty (plain
// source color, un-blended). Compose::Copy for the unblended layers was
// also tried and made it worse (erased the canvas background entirely)
// without fixing the blend. This smells like a Vello 0.9 optimization
// that fast-paths Mix::Normal groups by skipping backdrop capture
// entirely, which a later real-Mix group then reads as blank. No safe
// fix found without either a different Vello version or a CPU-side
// pre-blend (rendering each blended layer against a manually composited
// backdrop texture) — a real rendering-pipeline change, not attempted
// here to avoid destabilizing this render path (see CLAUDE.md's warning
// on the perf-critical, already-regression-prone rendering pipeline).
fn blend_from(name: Option<&str>) -> Option<vello::peniko::BlendMode> {
    use vello::peniko::{BlendMode, Compose, Mix};
    let mix = match name? {
        "multiply" => Mix::Multiply,
        "screen" => Mix::Screen,
        "overlay" => Mix::Overlay,
        "darken" => Mix::Darken,
        "lighten" => Mix::Lighten,
        "colorDodge" => Mix::ColorDodge,
        "colorBurn" => Mix::ColorBurn,
        "hardLight" => Mix::HardLight,
        "softLight" => Mix::SoftLight,
        "difference" => Mix::Difference,
        "exclusion" => Mix::Exclusion,
        "hue" => Mix::Hue,
        "saturation" => Mix::Saturation,
        "color" => Mix::Color,
        "luminosity" => Mix::Luminosity,
        _ => return None, // "normal" (or anything unrecognized) — no bracket needed
    };
    Some(BlendMode::new(mix, Compose::SrcOver))
}

#[derive(Deserialize, Serialize)]
pub(crate) struct SceneIn {
    pub(crate) layers: Vec<LayerIn>,
}

// Builds the filled ribbon outline for a pressure-brush stroke from its
// centerline + per-sample width — left-offset points forward, flat end
// cap, right-offset points backward, flat start cap, closed. Flat (butt)
// caps chosen over round for this first port to avoid an arc-sweep-
// direction edge case; round caps are a later polish pass, not a
// correctness requirement.
fn build_variable_width_outline(samples: &[[f64; 3]]) -> Option<BezPath> {
    let n = samples.len();
    if n < 2 {
        return None;
    }
    let mut normals = Vec::with_capacity(n);
    for i in 0..n {
        let prev = if i > 0 { i - 1 } else { i };
        let next = if i < n - 1 { i + 1 } else { i };
        let dx = samples[next][0] - samples[prev][0];
        let dy = samples[next][1] - samples[prev][1];
        let len = (dx * dx + dy * dy).sqrt().max(1e-6);
        normals.push((-dy / len, dx / len));
    }
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y, w) = (samples[i][0], samples[i][1], samples[i][2]);
        let (nx, ny) = normals[i];
        let hw = w / 2.0;
        left.push((x + nx * hw, y + ny * hw));
        right.push((x - nx * hw, y - ny * hw));
    }
    let mut path = BezPath::new();
    path.move_to(left[0]);
    for p in &left[1..] {
        path.line_to(*p);
    }
    // Round end cap: a semicircle around the last centerline sample, from
    // its left-offset point to its right-offset point. Both caps sweep by
    // a consistent -180° turn from "wherever the path cursor currently is"
    // — worked out by tracking that the normal is the forward tangent
    // rotated +90°, so a -180° sweep always bulges the cap outward (through
    // the forward tangent at the end, through the backward tangent at the
    // start) rather than folding back into the ribbon body.
    let (lcx, lcy, lr) = (samples[n - 1][0], samples[n - 1][1], samples[n - 1][2] / 2.0);
    add_semicircle_cap(&mut path, lcx, lcy, lr, left[n - 1]);
    for p in right[..n - 1].iter().rev() {
        path.line_to(*p);
    }
    // Round start cap, symmetric with the end cap above.
    let (rcx, rcy, rr) = (samples[0][0], samples[0][1], samples[0][2] / 2.0);
    add_semicircle_cap(&mut path, rcx, rcy, rr, right[0]);
    path.close_path();
    Some(path)
}
fn add_semicircle_cap(path: &mut BezPath, cx: f64, cy: f64, r: f64, from: (f64, f64)) {
    if r < 1e-6 {
        return;
    }
    let from_angle = (from.1 - cy).atan2(from.0 - cx);
    const STEPS: usize = 10;
    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        let a = from_angle - std::f64::consts::PI * t;
        path.line_to((cx + r * a.cos(), cy + r * a.sin()));
    }
}

// Shared with fill.rs: the exact same Paper.js-segment-to-BezPath
// reconstruction the renderer uses, so the fill engine's final result path
// is built with the identical curve semantics vello itself renders — moving
// fill.rs off its own hand-rolled polyline-only math and onto this single
// source of truth (see fill.rs's own doc comment for the fuller rationale).
pub(crate) fn build_bezpath_from_segments(segments: &[SegIn], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    if segments.is_empty() {
        return path;
    }
    path.move_to((segments[0].point[0], segments[0].point[1]));
    let n = segments.len();
    let last_idx = if closed { n } else { n - 1 };
    for i in 0..last_idx {
        let a = &segments[i];
        let b = &segments[(i + 1) % n];
        let c1 = (a.point[0] + a.handle_out[0], a.point[1] + a.handle_out[1]);
        let c2 = (b.point[0] + b.handle_in[0], b.point[1] + b.handle_in[1]);
        path.curve_to(c1, c2, (b.point[0], b.point[1]));
    }
    if closed {
        path.close_path();
    }
    path
}
pub(crate) fn build_bezpath(item: &ItemIn) -> Option<BezPath> {
    if let Some(centerline) = &item.centerline {
        return build_variable_width_outline(centerline);
    }
    if item.segments.is_empty() {
        return None;
    }
    Some(build_bezpath_from_segments(&item.segments, item.closed))
}
fn color_from(c: [u8; 4]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], c[3])
}
fn cap_from(name: &str) -> vello::kurbo::Cap {
    match name {
        "butt" => vello::kurbo::Cap::Butt,
        "square" => vello::kurbo::Cap::Square,
        _ => vello::kurbo::Cap::Round,
    }
}
fn join_from(name: &str) -> vello::kurbo::Join {
    match name {
        "bevel" => vello::kurbo::Join::Bevel,
        "miter" => vello::kurbo::Join::Miter,
        _ => vello::kurbo::Join::Round,
    }
}
/// Builds the kurbo `Stroke` for an item's stroke_cap/stroke_join/
/// miter_limit/dash_pattern/dash_offset, all optional with kurbo's own
/// defaults (Round cap/join, miter limit 4, no dashes) when absent.
fn stroke_from(item: &ItemIn) -> Stroke {
    let mut stroke = Stroke::new(item.stroke_width);
    if let Some(cap) = &item.stroke_cap {
        stroke = stroke.with_caps(cap_from(cap));
    }
    if let Some(join) = &item.stroke_join {
        stroke = stroke.with_join(join_from(join));
    }
    if let Some(limit) = item.miter_limit {
        stroke = stroke.with_miter_limit(limit);
    }
    if let Some(pattern) = &item.dash_pattern {
        if !pattern.is_empty() {
            stroke = stroke.with_dashes(item.dash_offset.unwrap_or(0.0), pattern.clone());
        }
    }
    stroke
}

// ---- Viewport: pan/zoom/rotate (Phase C2) ----
// World (0,0) is the artboard's own top-left corner, same convention the
// rest of the app already uses (state.canvasW/canvasH in the JS side) —
// NOT the canvas-center-as-origin convention. Rotation/zoom pivot around an
// explicit `pivot` point (JS passes the artboard center, since only the JS
// side knows canvasW/canvasH) rather than being silently hardcoded to
// width/2,height/2 — an earlier version baked that offset in unconditionally,
// which shifted every plain top-left-origin scene by half the canvas size
// even at pan=(0,0)/zoom=1/rotation=0 (caught via a plain rectangle test
// rendering near the bottom-right instead of where it was drawn).
#[derive(Clone, Copy)]
struct Viewport {
    pan_x: f64,
    pan_y: f64,
    zoom: f64,
    rotation: f64,
    pivot_x: f64,
    pivot_y: f64,
}
impl Default for Viewport {
    fn default() -> Self {
        Viewport { pan_x: 0.0, pan_y: 0.0, zoom: 1.0, rotation: 0.0, pivot_x: 0.0, pivot_y: 0.0 }
    }
}
impl Viewport {
    fn transform(&self) -> Affine {
        let pivot = (self.pivot_x, self.pivot_y);
        Affine::translate((self.pan_x, self.pan_y))
            * Affine::translate(pivot)
            * Affine::rotate(self.rotation)
            * Affine::scale(self.zoom)
            * Affine::translate((-pivot.0, -pivot.1))
    }
}

#[wasm_bindgen]
pub struct VelloEngine {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    // vello 0.9 dropped the old render_to_surface convenience (it now warns
    // that rendering vello's compute pipeline directly onto a swapchain
    // texture defeats some GPU optimizations) — render into this offscreen
    // Rgba8Unorm target instead, then blit it onto the surface each frame.
    offscreen: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
    blitter: wgpu::util::TextureBlitter,
    viewport: Viewport,
    // Phase C3: which (layerIndex,itemIndex) pairs are selected, persisted
    // across calls (unlike hit_test in hit.rs, which is a stateless one-shot
    // query) — lives here because VelloEngine is already the one stateful
    // object callers hold onto across frames.
    selected: Vec<(usize, usize)>,
    width: u32,
    height: u32,
    // kept so `resize()` can re-`configure()` the surface with the exact
    // same format the blitter was built for — the blitter itself doesn't
    // need rebuilding on a resize, only width/height change.
    surface_format: wgpu::TextureFormat,
    // Resolved once in create_engine from the surface's actual capabilities
    // and reused verbatim by resize() — see the comment at its computation
    // site for why this can't just be CompositeAlphaMode::Auto.
    surface_alpha_mode: wgpu::CompositeAlphaMode,
    // Uploaded-once image cache, keyed by a caller-chosen stable id (JS uses
    // the Raster's own data URL / a per-raster id — see engine-bridge.js).
    // peniko::ImageData's `data` field is a Blob<u8> (Arc-backed, cheap to
    // clone), so re-using the SAME ImageData instance across many `render()`
    // calls (instead of rebuilding one from raw bytes every frame) is what
    // lets vello's own internal resource cache recognize "this is the same
    // image as last frame" and skip re-uploading pixels to the GPU.
    images: std::collections::HashMap<String, vello::peniko::ImageData>,
}

/// Async because WebGPU adapter/device negotiation is inherently async —
/// JS must `await` this once, then reuse the returned handle every frame.
/// Sets up wgpu manually (rather than via vello::util::RenderContext) so we
/// control the backend flag: RenderContext's own default is
/// `wgpu::Backends::PRIMARY`, which on wasm32 means the *native* backends
/// bundled in the wgpu crate — NOT the browser's own WebGPU implementation
/// — and produced a `NoCompatibleDevice` error until this was pinned to
/// `Backends::BROWSER_WEBGPU` explicitly.
#[wasm_bindgen]
pub async fn create_engine(
    canvas: HtmlCanvasElement,
    width: u32,
    height: u32,
) -> Result<VelloEngine, JsValue> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|e| JsValue::from_str(&format!("create_surface failed: {e:?}")))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .map_err(|e| JsValue::from_str(&format!("no compatible WebGPU adapter: {e:?}")))?;
    // wgpu::Limits::default() lists every limit the *wgpu crate* knows
    // about; if the browser's WebGPU implementation is on a slightly
    // different spec revision than this wgpu version targets, some limit
    // name comes back unrecognized and requestDevice rejects the whole
    // descriptor outright. Asking the adapter for what it actually supports
    // and requesting exactly that avoids the mismatch.
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() })
        .await
        .map_err(|e| JsValue::from_str(&format!("request_device failed: {e:?}")))?;
    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|f| matches!(f, wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm))
        .ok_or_else(|| JsValue::from_str("no compatible surface format"))?;
    // render()'s offscreen pass clears to Color::TRANSPARENT and the blit to
    // the visible surface is a straight copy (no blending) — so whatever the
    // browser's canvas compositor does with alpha=0 pixels is what the user
    // sees behind the drawing. CompositeAlphaMode::Auto resolves to Opaque on
    // every browser tested, which paints alpha=0 as solid black, hiding the
    // CSS panel color set on the canvas's parent element. PreMultiplied (when
    // the surface actually supports it) blends against that CSS background
    // instead — falls back to Auto/Opaque on a surface that doesn't offer it
    // rather than erroring, since Opaque-with-black was already the
    // long-standing behavior.
    let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
        wgpu::CompositeAlphaMode::PreMultiplied
    } else {
        wgpu::CompositeAlphaMode::Auto
    };
    surface.configure(
        &device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        },
    );
    let renderer = Renderer::new(
        &device,
        RendererOptions { antialiasing_support: AaSupport::area_only(), ..Default::default() },
    )
    .map_err(|e| JsValue::from_str(&format!("renderer creation failed: {e:?}")))?;

    let offscreen = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vello-offscreen"),
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
    let offscreen_view = offscreen.create_view(&wgpu::TextureViewDescriptor::default());
    let blitter = wgpu::util::TextureBlitter::new(&device, format);

    Ok(VelloEngine {
        surface,
        device,
        queue,
        renderer,
        offscreen,
        offscreen_view,
        blitter,
        viewport: Viewport::default(),
        selected: Vec::new(),
        width,
        height,
        surface_format: format,
        surface_alpha_mode: alpha_mode,
        images: std::collections::HashMap::new(),
    })
}

#[wasm_bindgen]
impl VelloEngine {
    /// `rotation` in radians, pivoting around `(pivot_x, pivot_y)` — pass
    /// the artboard center (e.g. canvasW/2, canvasH/2) to match Animate's
    /// Rotate Stage tool; pass (0,0) for a plain top-left-anchored zoom/pan.
    pub fn set_viewport(&mut self, pan_x: f64, pan_y: f64, zoom: f64, rotation: f64, pivot_x: f64, pivot_y: f64) {
        // A zero/negative zoom (a stray or buggy JS-side value — this is
        // caller-controlled, not otherwise validated) makes Affine::scale
        // singular; screen_to_world's inverse() then yields inf/NaN that
        // silently poisons all hit-testing/coordinate math afterward
        // instead of failing loudly. Same floor already used by
        // gizmo_handles for the same reason (see its own zoom.max call).
        self.viewport = Viewport { pan_x, pan_y, zoom: zoom.max(0.0001), rotation, pivot_x, pivot_y };
    }

    /// Screen (canvas pixel) coordinates -> world coordinates, accounting
    /// for the current pan/zoom/rotation — the wasm-side equivalent of
    /// Paper.js's `view.viewToProject`, needed since raw pointer events now
    /// arrive in plain screen space with nothing translating them for free.
    pub fn screen_to_world(&self, sx: f64, sy: f64) -> Vec<f64> {
        let inv = self.viewport.transform().inverse();
        let p = inv * vello::kurbo::Point::new(sx, sy);
        vec![p.x, p.y]
    }

    /// Hit-tests at (x,y) (world space — caller passes the result of
    /// `screen_to_world`) and updates the persisted selection: `additive`
    /// (shift-click) toggles the hit item in/out of the existing selection;
    /// otherwise the selection is replaced by just the hit item, or cleared
    /// entirely on a miss. Returns the updated selection as JSON (see
    /// `get_selection`).
    pub fn select_at(&mut self, scene_json: &str, x: f64, y: f64, tolerance: f64, additive: bool) -> Result<String, JsValue> {
        let scene: SceneIn = serde_json::from_str(scene_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        match crate::hit::hit_test_scene(&scene, x, y, tolerance) {
            Some((li, ii, _kind)) => {
                let key = (li, ii);
                if additive {
                    if let Some(pos) = self.selected.iter().position(|k| *k == key) {
                        self.selected.remove(pos);
                    } else {
                        self.selected.push(key);
                    }
                } else {
                    self.selected = vec![key];
                }
            }
            None => {
                if !additive {
                    self.selected.clear();
                }
            }
        }
        self.get_selection()
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    pub fn get_selection(&self) -> Result<String, JsValue> {
        #[derive(Serialize)]
        struct Sel {
            #[serde(rename = "layerIndex")]
            layer_index: usize,
            #[serde(rename = "itemIndex")]
            item_index: usize,
        }
        let list: Vec<Sel> = self
            .selected
            .iter()
            .map(|(li, ii)| Sel { layer_index: *li, item_index: *ii })
            .collect();
        serde_json::to_string(&list).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Union bounding box (world space) of every currently-selected item,
    /// or `null` if nothing is selected — the box the transform gizmo's
    /// handles are built from.
    pub fn selection_bounds(&self, scene_json: &str) -> Result<String, JsValue> {
        let scene: SceneIn = serde_json::from_str(scene_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut bounds: Option<vello::kurbo::Rect> = None;
        for (li, ii) in &self.selected {
            let item = match scene.layers.get(*li).and_then(|l| l.items.get(*ii)) {
                Some(it) => it,
                None => continue,
            };
            if let Some(bez) = build_bezpath(item) {
                let bb = bez.bounding_box();
                bounds = Some(match bounds {
                    Some(b) => b.union(bb),
                    None => bb,
                });
            }
        }
        match bounds {
            Some(b) => Ok(format!("[{},{},{},{}]", b.x0, b.y0, b.x1 - b.x0, b.y1 - b.y0)),
            None => Ok("null".to_string()),
        }
    }

    /// Named handle positions (world space) for the free-transform gizmo —
    /// 8 box handles (corners + edge midpoints) plus a rotate handle offset
    /// above the top edge. The rotate-handle offset is divided by the
    /// current zoom so it looks like a constant screen-space distance
    /// regardless of how zoomed in/out the view is (mirrors how
    /// Paper.js-based tools.js already does this: handle sizes/offsets
    /// divided by view.zoom). Returns `null` if nothing is selected.
    pub fn gizmo_handles(&self, scene_json: &str) -> Result<String, JsValue> {
        let bounds_json = self.selection_bounds(scene_json)?;
        if bounds_json == "null" {
            return Ok("null".to_string());
        }
        let b: [f64; 4] = serde_json::from_str(&bounds_json).unwrap();
        let (x, y, w, h) = (b[0], b[1], b[2], b[3]);
        let rotate_offset = 24.0 / self.viewport.zoom.max(0.0001);
        #[derive(Serialize)]
        struct Handles {
            nw: [f64; 2],
            n: [f64; 2],
            ne: [f64; 2],
            e: [f64; 2],
            se: [f64; 2],
            s: [f64; 2],
            sw: [f64; 2],
            w: [f64; 2],
            rotate: [f64; 2],
        }
        let handles = Handles {
            nw: [x, y],
            n: [x + w / 2.0, y],
            ne: [x + w, y],
            e: [x + w, y + h / 2.0],
            se: [x + w, y + h],
            s: [x + w / 2.0, y + h],
            sw: [x, y + h],
            w: [x, y + h / 2.0],
            rotate: [x + w / 2.0, y - rotate_offset],
        };
        serde_json::to_string(&handles).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Scales every selected item in-place around `(anchor_x, anchor_y)` —
    /// the anchor is the OPPOSITE corner/edge from whichever handle is being
    /// dragged (mirrors selPropsApplyScale in tools.js: dragging the SE
    /// handle anchors on NW, etc — the caller picks the anchor, this just
    /// applies the transform). Returns the updated scene JSON; the caller
    /// (JS) is expected to keep using this returned scene for subsequent
    /// render()/hit-test calls, replacing its own copy.
    pub fn scale_selection(&self, scene_json: &str, anchor_x: f64, anchor_y: f64, sx: f64, sy: f64) -> Result<String, JsValue> {
        let linear = Affine::IDENTITY.then_scale_non_uniform(sx, sy);
        let tf = Affine::translate((anchor_x, anchor_y)) * linear * Affine::translate((-anchor_x, -anchor_y));
        let scale_factor = (sx.abs() * sy.abs()).sqrt();
        self.apply_transform(scene_json, tf, linear, scale_factor)
    }

    /// Rotates every selected item in-place around `(pivot_x, pivot_y)` by
    /// `angle` radians — mirrors selPropsApplyRotate in tools.js.
    pub fn rotate_selection(&self, scene_json: &str, pivot_x: f64, pivot_y: f64, angle: f64) -> Result<String, JsValue> {
        let linear = Affine::rotate(angle);
        let tf = Affine::translate((pivot_x, pivot_y)) * linear * Affine::translate((-pivot_x, -pivot_y));
        self.apply_transform(scene_json, tf, linear, 1.0)
    }

    // `linear` is the transform's rotate/scale part with no translation —
    // passed in directly (built alongside `tf` by the two callers above)
    // rather than extracted from `tf` after the fact, since kurbo::Affine
    // doesn't expose its raw coefficients publicly. Handles (handleIn/
    // handleOut) are relative offset vectors, not absolute positions —
    // translating them along with the point would shear the curve shape,
    // so they only ever get `linear`, never `tf`'s translation.
    fn apply_transform(&self, scene_json: &str, tf: Affine, linear: Affine, width_scale: f64) -> Result<String, JsValue> {
        let mut scene: SceneIn = serde_json::from_str(scene_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        for (li, ii) in &self.selected {
            if let Some(item) = scene.layers.get_mut(*li).and_then(|l| l.items.get_mut(*ii)) {
                for seg in &mut item.segments {
                    let p = tf * vello::kurbo::Point::new(seg.point[0], seg.point[1]);
                    seg.point = [p.x, p.y];
                    let hi = linear * vello::kurbo::Point::new(seg.handle_in[0], seg.handle_in[1]);
                    seg.handle_in = [hi.x, hi.y];
                    let ho = linear * vello::kurbo::Point::new(seg.handle_out[0], seg.handle_out[1]);
                    seg.handle_out = [ho.x, ho.y];
                }
                if let Some(centerline) = &mut item.centerline {
                    for sample in centerline.iter_mut() {
                        let p = tf * vello::kurbo::Point::new(sample[0], sample[1]);
                        sample[0] = p.x;
                        sample[1] = p.y;
                        sample[2] *= width_scale;
                    }
                }
                item.stroke_width *= width_scale;
            }
        }
        serde_json::to_string(&scene).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Re-configures the surface AND the offscreen render target to a new
    /// device-pixel size — needed because the canvas backing this engine is
    /// created once at a fixed size (`create_engine`'s width/height); if the
    /// app window is resized afterwards without this, the surface keeps
    /// presenting at the stale size while the canvas's CSS box (and
    /// `screen_to_world`'s caller-supplied scale) grows/shrinks, producing a
    /// stretched/blurry canvas AND wrong world-space coordinates for any
    /// interception relying on the canvas's current on-screen size (this is
    /// what made an intercepted tool's live preview land off-screen after a
    /// resize). No-ops if the size hasn't actually changed, since JS calls
    /// this on every resize-observer tick regardless.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height || width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.surface_format,
                width,
                height,
                present_mode: wgpu::PresentMode::AutoVsync,
                desired_maximum_frame_latency: 2,
                alpha_mode: self.surface_alpha_mode,
                view_formats: vec![],
            },
        );
        self.offscreen = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vello-offscreen"),
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
        self.offscreen_view = self.offscreen.create_view(&wgpu::TextureViewDescriptor::default());
    }

    /// Uploads (or re-uploads, if already cached under this id) an image's
    /// raw RGBA8 pixels, keyed by a caller-chosen stable `id` — JS calls this
    /// ONCE per distinct image (e.g. keyed by the Raster's own data URL) and
    /// then just references `id` in every subsequent `render()` scene JSON,
    /// rather than re-sending pixel bytes on every frame (see the `images`
    /// field's own comment for why re-using the same `ImageData` instance
    /// matters for vello's internal upload caching).
    pub fn register_image(&mut self, id: String, rgba: &[u8], width: u32, height: u32) -> Result<(), JsValue> {
        let expected = (width as usize) * (height as usize) * 4;
        if rgba.len() != expected {
            return Err(JsValue::from_str(&format!(
                "register_image: expected {expected} bytes for {width}x{height} RGBA8, got {}",
                rgba.len()
            )));
        }
        let data = vello::peniko::ImageData {
            data: rgba.to_vec().into(),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width,
            height,
        };
        self.images.insert(id, data);
        Ok(())
    }

    /// Lets JS skip a redundant `register_image` upload for an image it's
    /// already registered this session (images are cached for the engine's
    /// whole lifetime, not per-scene, so this is a simple presence check).
    pub fn has_image(&self, id: &str) -> bool {
        self.images.contains_key(id)
    }

    pub fn render(&mut self, scene_json: &str) -> Result<(), JsValue> {
        let scene_in: SceneIn = serde_json::from_str(scene_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let view_tf = self.viewport.transform();
        let mut scene = Scene::new();
        let full_canvas = Rect::new(0.0, 0.0, self.width as f64, self.height as f64);
        for layer in &scene_in.layers {
            let blend = blend_from(layer.blend_mode.as_deref());
            if let Some(b) = blend {
                scene.push_layer(vello::peniko::Fill::NonZero, b, 1.0, Affine::IDENTITY, &full_canvas);
            }
            for item in &layer.items {
                if let Some(img_ref) = &item.image {
                    if let Some(image_data) = self.images.get(&img_ref.image_id) {
                        // draw_image() draws at the image's OWN natural pixel
                        // size, so the display width/height (which can differ,
                        // e.g. an imported photo scaled to fit the canvas) is
                        // baked in as a scale, composed with the placement
                        // translate and the overall view transform.
                        let sx = img_ref.width / image_data.width as f64;
                        let sy = img_ref.height / image_data.height as f64;
                        let place = Affine::translate((img_ref.x, img_ref.y)) * Affine::scale_non_uniform(sx, sy);
                        let brush = vello::peniko::ImageBrush::new(image_data.clone()).multiply_alpha(img_ref.opacity);
                        scene.draw_image(&brush, view_tf * place);
                    }
                    continue;
                }
                let bez = match build_bezpath(item) {
                    Some(b) => b,
                    None => continue,
                };
                let paint_fill = |scene: &mut Scene| {
                    if let Some(fc) = item.fill_color {
                        scene.fill(vello::peniko::Fill::NonZero, view_tf, color_from(fc), None, &bez);
                    }
                };
                let paint_stroke = |scene: &mut Scene| {
                    if let Some(sc) = item.stroke_color {
                        scene.stroke(&stroke_from(item), view_tf, color_from(sc), None, &bez);
                    }
                };
                if item.paint_order.as_deref() == Some("strokeFirst") {
                    paint_stroke(&mut scene);
                    paint_fill(&mut scene);
                } else {
                    paint_fill(&mut scene);
                    paint_stroke(&mut scene);
                }
            }
            if blend.is_some() {
                scene.pop_layer();
            }
        }

        let params = RenderParams {
            // Confirmed in-browser (see removed [wasm-debug] probe) that this
            // canvas's WebGPU surface only advertises CompositeAlphaMode::
            // Opaque — Premultiplied isn't available to blend transparent
            // pixels against the canvas's CSS background, so the pasteboard
            // (area outside the document bounds, wherever nothing gets
            // drawn) must be baked in as an opaque fill here instead of
            // relying on browser compositing. Matches the app's --panel CSS
            // var (#201f25) so the canvas reads as one continuous surface
            // with the surrounding UI chrome rather than a separate black
            // box — keep this in sync with :root{--panel} in style.css if
            // that ever changes.
            base_color: Color::from_rgba8(0x20, 0x1f, 0x25, 0xff),
            width: self.width,
            height: self.height,
            antialiasing_method: AaConfig::Area,
        };
        self.renderer
            .render_to_texture(&self.device, &self.queue, &scene, &self.offscreen_view, &params)
            .map_err(|e| JsValue::from_str(&format!("render_to_texture failed: {e:?}")))?;

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => return Err(JsValue::from_str(&format!("get_current_texture failed: {other:?}"))),
        };
        let surface_view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vello-blit-encoder"),
        });
        self.blitter.copy(&self.device, &mut encoder, &self.offscreen_view, &surface_view);
        self.queue.submit(Some(encoder.finish()));
        surface_texture.present();
        Ok(())
    }

    /// Phase C6 (export pipeline): renders the scene offscreen and reads the
    /// pixels back to the CPU as straight-alpha RGBA8 bytes (width*height*4),
    /// ready for `new ImageData(...)` -> canvas -> PNG, or for feeding the
    /// existing ffmpeg sidecar frame-by-frame. Doesn't touch the visible
    /// surface at all — exporting doesn't flash frames on screen.
    /// Async because GPU->CPU buffer mapping is callback-based in WebGPU;
    /// bridged to a Rust future with a oneshot channel.
    pub async fn render_to_pixels(&mut self, scene_json: &str) -> Result<Vec<u8>, JsValue> {
        let scene_in: SceneIn = serde_json::from_str(scene_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let view_tf = self.viewport.transform();
        let mut scene = Scene::new();
        let full_canvas = Rect::new(0.0, 0.0, self.width as f64, self.height as f64);
        for layer in &scene_in.layers {
            let blend = blend_from(layer.blend_mode.as_deref());
            if let Some(b) = blend {
                scene.push_layer(vello::peniko::Fill::NonZero, b, 1.0, Affine::IDENTITY, &full_canvas);
            }
            for item in &layer.items {
                if let Some(img_ref) = &item.image {
                    if let Some(image_data) = self.images.get(&img_ref.image_id) {
                        // draw_image() draws at the image's OWN natural pixel
                        // size, so the display width/height (which can differ,
                        // e.g. an imported photo scaled to fit the canvas) is
                        // baked in as a scale, composed with the placement
                        // translate and the overall view transform.
                        let sx = img_ref.width / image_data.width as f64;
                        let sy = img_ref.height / image_data.height as f64;
                        let place = Affine::translate((img_ref.x, img_ref.y)) * Affine::scale_non_uniform(sx, sy);
                        let brush = vello::peniko::ImageBrush::new(image_data.clone()).multiply_alpha(img_ref.opacity);
                        scene.draw_image(&brush, view_tf * place);
                    }
                    continue;
                }
                let bez = match build_bezpath(item) {
                    Some(b) => b,
                    None => continue,
                };
                let paint_fill = |scene: &mut Scene| {
                    if let Some(fc) = item.fill_color {
                        scene.fill(vello::peniko::Fill::NonZero, view_tf, color_from(fc), None, &bez);
                    }
                };
                let paint_stroke = |scene: &mut Scene| {
                    if let Some(sc) = item.stroke_color {
                        scene.stroke(&stroke_from(item), view_tf, color_from(sc), None, &bez);
                    }
                };
                if item.paint_order.as_deref() == Some("strokeFirst") {
                    paint_stroke(&mut scene);
                    paint_fill(&mut scene);
                } else {
                    paint_fill(&mut scene);
                    paint_stroke(&mut scene);
                }
            }
            if blend.is_some() {
                scene.pop_layer();
            }
        }
        let params = RenderParams {
            base_color: Color::TRANSPARENT,
            width: self.width,
            height: self.height,
            antialiasing_method: AaConfig::Area,
        };
        self.renderer
            .render_to_texture(&self.device, &self.queue, &scene, &self.offscreen_view, &params)
            .map_err(|e| JsValue::from_str(&format!("render_to_texture failed: {e:?}")))?;

        // WebGPU requires bytes_per_row aligned to 256 for texture->buffer
        // copies; rows get padded on copy and stripped after mapping.
        let unpadded = self.width as usize * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buf_size = (padded * self.height as usize) as u64;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("export-readback"),
            size: buf_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("export-copy-encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.offscreen,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));

        let (tx, rx) = futures_channel::oneshot::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        rx.await
            .map_err(|_| JsValue::from_str("readback channel dropped"))?
            .map_err(|e| JsValue::from_str(&format!("buffer map failed: {e:?}")))?;

        let mapped = readback.slice(..).get_mapped_range();
        let mut out = Vec::with_capacity(unpadded * self.height as usize);
        for row in 0..self.height as usize {
            let start = row * padded;
            out.extend_from_slice(&mapped[start..start + unpadded]);
        }
        drop(mapped);
        readback.unmap();
        Ok(out)
    }
}
