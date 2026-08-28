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
#[cfg(target_arch = "wasm32")]
use web_sys::HtmlCanvasElement;

// Clone (2026-08, masks): composite_scene's Add-mask union step needs a
// contiguous `&[ItemIn]` of just the Add-mode masks pulled out of
// LayerIn::masks (a Vec<MaskIn>, not a Vec<ItemIn>) — cloning that handful
// of mask items into a fresh Vec is the simplest way to get one without
// changing paint_layer_items' slice-based signature (used elsewhere,
// CLAUDE.md §3 territory). Cheap: masks are a handful of items, cloned
// once per frame, nowhere near the hot path.
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ItemIn {
    // Reference into the engine's retained path store (register_path) — when
    // present, `segments`/`closed` are ABSENT by contract and the stored
    // BezPath is used instead. Precedence: `image` > `pathRef` > `centerline`
    // > `segments` (mirrors the existing "richer field wins" convention).
    // NOTE: the legacy Phase-C1 selection APIs (select_at, selection_bounds,
    // apply_transform — zero JS callers today, selection lives in Paper.js)
    // do NOT resolve pathRef; if they are ever revived they must go through
    // the same store lookup paint_layer_items uses, or refs will read as
    // empty geometry there (bug family §1).
    #[serde(default)]
    pub(crate) path_ref: Option<String>,
    // Affine [a,b,c,d,e,f] (SVG/kurbo convention) composed with the view
    // transform for THIS item only — lets an animated shape reuse its
    // registered path instead of falling back to re-serializing every
    // coordinate. Element/layer/parent Motion matrices are all affine around
    // a pivot, so the whole chain collapses into one of these.
    //
    // The stroke rides the same transform, so JS must send the UNSCALED
    // strokeWidth when this is present (it pre-multiplies by strokeScale on
    // the inline path). Only emitted for uniformly-scaled chains, where
    // vello's stroking and JS's (|sx|+|sy|)/2 agree exactly.
    #[serde(default)]
    pub(crate) path_transform: Option<[f64; 6]>,
    #[serde(default)]
    pub(crate) segments: Vec<SegIn>,
    #[serde(default)]
    pub(crate) closed: bool,
    #[serde(default)]
    pub(crate) fill_color: Option<[u8; 4]>,
    #[serde(default)]
    pub(crate) stroke_color: Option<[u8; 4]>,
    // Gradient fill (2026-07) — takes priority over `fill_color` when
    // present (same "richer field wins" precedent as `centerline` over
    // `segments`, and `image` over both). World-space anchor points, same
    // convention as `segments`' own coordinates — NOT yet composed with a
    // Motion transform (motion.js's elMat/motionMat/parentChain only ever
    // touch `segments`/`centerline`/`image` today), a known v1 limitation:
    // an animated shape's gradient stays anchored to its ORIGINAL position
    // instead of riding along with the shape's Motion keyframes.
    #[serde(default)]
    pub(crate) fill_gradient: Option<GradientIn>,
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
    // Per-ELEMENT effects (2026-07, "possible de différencié les effet par
    // éléments sélectionné si j'applique un effet sur un élément select
    // alors il ne s'applique sur celui-ci") — same shape/semantics as
    // LayerIn::effects, just scoped to this one item instead of the whole
    // layer. Empty (the overwhelming common case) for every item that
    // never had a per-element effect applied — see
    // paint_layer_with_element_effects for how a non-empty one is handled.
    #[serde(default)]
    pub(crate) effects: Vec<EffectIn>,
}
#[derive(Deserialize, Serialize, Clone)]
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
    // Rotation in DEGREES around the rect's own center (2026-07 — image
    // items previously had no rotation at all, the "known v1 limitation"
    // noted in motion.js transformImageRect: a rotated imported image or
    // native-video layer silently rendered axis-aligned). Degrees, not
    // radians, matching every other rotation value in the JS scene
    // pipeline (Paper.js and motion.js are degree-based throughout).
    #[serde(default)]
    pub(crate) rotation: f64,
}
fn default_opacity() -> f32 {
    1.0
}
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GradientStopIn {
    pub(crate) offset: f32,
    pub(crate) color: [u8; 4],
}
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GradientIn {
    // "linear" (default) | "radial" — anything else falls back to linear,
    // same forgiving-default spirit as mix_mode_index/matte_mode_of.
    pub(crate) kind: String,
    pub(crate) from: [f64; 2],
    // Linear: the gradient's end point. Radial: a point on the gradient's
    // outer edge — its distance from `from` (the center) is the radius.
    // One shared shape for both kinds rather than a radius-only radial
    // variant, so the JS side's "drag a handle to set direction/size" gizmo
    // (same interaction for either kind) doesn't need a kind-specific
    // payload shape.
    pub(crate) to: [f64; 2],
    pub(crate) stops: Vec<GradientStopIn>,
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
#[serde(rename_all = "camelCase")]
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
    // Track matte (2026-07, scouted from Caddis's Layer.matteMode): this
    // layer's alpha comes from a SOURCE layer's painted pixels instead of
    // its own. "alpha"/"alphaInverted"/"luma"/"lumaInverted"; None/"none"
    // is the default (no matte). The matte SOURCE layer itself is consumed
    // — composite_scene skips painting it as its own visible layer.
    #[serde(default)]
    pub(crate) matte_mode: Option<String>,
    // Which layer is the matte source (2026-07-31, uid-based mattes):
    // resolved entirely JS-side from ld.matteSourceLayerUid (engine-bridge's
    // buildSceneJson final pass) — the engine stays stateless and purely
    // positional, it just no longer ASSUMES adjacency. None = the legacy
    // implicit "layer directly above (i+1)" AE convention (scene JSON from
    // older saves whose uid didn't resolve, or pre-migration projects) —
    // see resolve_matte_source, the single reader of both conventions.
    #[serde(default)]
    pub(crate) matte_source_index: Option<usize>,
    // Adjustment/effect layer (2026-07, Motion) — an AE-style layer with no
    // painted content of its own whose EFFECTS STACK (below) applies to
    // EVERYTHING BELOW it in the layer stack instead of just itself.
    // Unlike an ordinary layer's `effects` (which only ever transform THIS
    // layer's own isolated render), composite_scene reads the running
    // accumulator (the composite of every layer so far) as the stack's
    // source and writes the result back as the new accumulator state — see
    // composite_scene's is_effect_layer branch. `items` is ignored entirely
    // for this layer (never painted), matching AE's own "Adjustment Layer"
    // toggle hiding the layer's own shape while its effects still apply.
    #[serde(default)]
    pub(crate) is_effect_layer: Option<bool>,
    // Effects stack (2026-07 rewrite — was a handful of fixed fields:
    // blur_radius/gshadow_*/effect_type/effect_p1/effect_p2, one value each,
    // one effect per layer max). Now a proper AE-style stack: any number of
    // effects, each independently toggleable, applied in order. Ordinary
    // layers run this on their OWN isolated alpha (see composite_scene's
    // per-layer loop, right after matte); effect/adjustment layers
    // (is_effect_layer=true) run it on the flattened accumulator instead —
    // same apply_effect_stack function either way, just fed a different
    // source texture, since each pass function only cares about whatever
    // texture it's given.
    //
    // IMPORTANT asymmetry this implies: "groundShadow" AND "contourBrut"
    // both need the shape's OWN true alpha silhouette (transparent where
    // there's no content) to produce a correctly-shaped result — but this
    // engine seeds the ACCUMULATOR with an OPAQUE base color up front
    // (composite_scene's clear_texture call), so alpha can no longer tell
    // "shape" from "empty background" once flattened (confirmed by direct
    // pixel-probing during development). Adding either to an effect/
    // adjustment layer's stack is therefore a no-op-looking full-canvas
    // darken/outline, not a silhouette-aware result — the JS-side "Add
    // Effect" menu accordingly only offers them for ordinary layers,
    // mirroring AE's own real distinction between a plain Effect (works
    // anywhere) and a shape-aware Layer Style like Drop Shadow/Stroke
    // (per-layer only).
    #[serde(default)]
    pub(crate) effects: Vec<EffectIn>,
    // Vector masks (2026-08, AE-style "Mask", per-layer geometry that clips
    // this layer's own content — distinct from matte_mode/matte_source_index
    // above, which clips against ANOTHER layer). JS (engine-bridge.js)
    // pre-builds each mask's `item` as solid opaque white fill/no stroke —
    // only its GEOMETRY matters here, never its real on-canvas paint, so
    // paint_layer_items can render it completely unmodified (see
    // composite_scene's mask-combine algorithm for why this needs no new
    // WGSL: Add is a plain union (all Add items painted in one Scene —
    // overlapping opaque white stays opaque white), Subtract/Intersect
    // reuse matte_pass's own alpha-multiply math one mask at a time).
    #[serde(default)]
    pub(crate) masks: Vec<MaskIn>,
    // ONE shared feather (world px, pre-scaled by JS same as any other
    // radius-shaped effect param) applied to the FINAL combined silhouette,
    // not per-mask — a deliberate v1 simplification (see the mask-feature
    // audit): AE feathers each mask individually before combining, but the
    // overwhelmingly common case is one mask per masked layer, where the
    // two are indistinguishable, and a single shared value avoids a whole
    // extra per-mask blur+combine interleaving for a rarely-exercised case.
    #[serde(default)]
    pub(crate) mask_feather: f64,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaskIn {
    pub(crate) item: ItemIn,
    // "add" (union, default) | "subtract" | "intersect" — same vocabulary
    // as AE's own mask mode dropdown (minus Lighten/Darken/Difference,
    // dropped for v1: rare in practice and each needs its own compose
    // formula beyond matte_pass's reusable alpha-multiply).
    #[serde(default = "default_mask_mode")]
    pub(crate) mode: String,
}
fn default_mask_mode() -> String {
    "add".to_string()
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EffectIn {
    // "blur" | "colorAdjust" | "vignette" | "glow" | "groundShadow" | one of
    // the simple_fx.wgsl effects (sepia/invert/grayscale/posterize/
    // pixelate/chromaticAberration/scanlines/grain/sharpen/edgeDetect).
    // Unrecognized falls back to "blur" (see run_one_effect).
    #[serde(default)]
    pub(crate) effect_type: String,
    // Per-instance on/off toggle (the little eye icon in the Effects list)
    // — kept distinct from removing the entry so a disabled effect's
    // params aren't lost. Disabled entries are skipped entirely in
    // apply_effect_stack, same "off = no GPU cost" convention the old
    // single-field version had via 0-valued radius/opacity.
    #[serde(default)]
    pub(crate) enabled: bool,
    // Generic params, meaning depends on effect_type — see simple_fx.wgsl's
    // Params doc comment and each pass function's own doc comment for the
    // exact mapping (unchanged from the old effect_p1..p4/gshadow_* fields,
    // just renamed to a flat p1..p4 per stack entry instead of one set of
    // fields per layer).
    #[serde(default)]
    pub(crate) p1: Option<f32>,
    #[serde(default)]
    pub(crate) p2: Option<f32>,
    #[serde(default)]
    pub(crate) p3: Option<f32>,
    #[serde(default)]
    pub(crate) p4: Option<f32>,
    // p5..p8 (2026-08, "possibilité de sortir plus de paramètres
    // d'effets") — same #[serde(default)] contract as p1..p4: an older
    // saved project's effect entries simply have none of these fields at
    // all, which deserializes to None/0.0 here, never an error. Every
    // BUILT-IN effect (run_one_effect) still only ever reads p1..p4 — only
    // "custom:" shader-library effects (simple_fx_pass's other caller) can
    // define param() entries using p5..p8 today.
    #[serde(default)]
    pub(crate) p5: Option<f32>,
    #[serde(default)]
    pub(crate) p6: Option<f32>,
    #[serde(default)]
    pub(crate) p7: Option<f32>,
    #[serde(default)]
    pub(crate) p8: Option<f32>,
}

// FIXED (was "KNOWN BROKEN, v17 investigation" — see git history for the
// original writeup): layer blend modes used to render with zero visual
// effect. Root-caused via direct experimentation as a real vello 0.9 issue,
// not a JS wiring bug: a push_layer(Mix::X) group never saw a correct
// backdrop to blend against, no matter how the layers beneath it were
// wrapped. No fix found within vello's own API (0.9 is the current release
// — nothing to bump to), and CPU-side readback isn't viable either: WebGPU's
// mapAsync is async-only on wasm32 with no blocking-poll escape hatch, and
// render() is called synchronously on every frame from engine-bridge.js's
// tick() — making it async would ripple through the whole render loop for
// a rarely-used feature. Fixed instead with our OWN compositor (blend.wgsl
// + composite_scene below): every blended scene renders each layer to its
// own isolated texture via vello (unmodified, GPU-only, synchronous) and
// composites them ourselves with a small fullscreen-shader pass implementing
// the standard W3C Compositing-and-Blending Mix formulas — entirely
// GPU-side, so it slots into render()'s existing synchronous signature.
// This index ordering must stay in sync with blend.wgsl's `switch params.mode`.
fn mix_mode_index(name: Option<&str>) -> u32 {
    match name.unwrap_or("normal") {
        "multiply" => 1,
        "screen" => 2,
        "overlay" => 3,
        "darken" => 4,
        "lighten" => 5,
        "colorDodge" => 6,
        "colorBurn" => 7,
        "hardLight" => 8,
        "softLight" => 9,
        "difference" => 10,
        "exclusion" => 11,
        "hue" => 12,
        "saturation" => 13,
        "color" => 14,
        "luminosity" => 15,
        _ => 0, // "normal" (or anything unrecognized) — plain SrcOver, no mix
    }
}

#[derive(Deserialize, Serialize)]
pub(crate) struct SceneIn {
    pub(crate) layers: Vec<LayerIn>,
    #[serde(default)]
    pub(crate) time: f32,
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
/// Builds a peniko `Gradient` brush from a `GradientIn` — `None` only when
/// there are fewer than 2 stops (a gradient with 0-1 stops has no ramp to
/// draw; caller falls back to `fill_color`/no-fill, same as any other
/// malformed-input case in this file).
fn gradient_brush(g: &GradientIn) -> Option<vello::peniko::Gradient> {
    if g.stops.len() < 2 {
        return None;
    }
    let stops: Vec<(f32, Color)> = g.stops.iter().map(|s| (s.offset, color_from(s.color))).collect();
    let gradient = if g.kind == "radial" {
        let radius = ((g.to[0] - g.from[0]).powi(2) + (g.to[1] - g.from[1]).powi(2)).sqrt() as f32;
        vello::peniko::Gradient::new_radial((g.from[0], g.from[1]), radius.max(0.01))
    } else {
        vello::peniko::Gradient::new_linear((g.from[0], g.from[1]), (g.to[0], g.to[1]))
    };
    Some(gradient.with_stops(stops.as_slice()))
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
/// Rotates a kurbo dash pattern ([dash,gap,dash,gap,...], index 0 = dash)
/// so that starting to draw it from index 0 with offset 0 looks the same as
/// drawing the ORIGINAL pattern starting `offset` units in. Used to work
/// around vello 0.9's GPU stroke path ignoring `with_dashes`' offset
/// argument entirely (see the call site's comment) — bakes the phase shift
/// into the array instead.
fn dash_pattern_with_offset(pattern: &[f64], offset: f64) -> Vec<f64> {
    let total: f64 = pattern.iter().sum();
    if total <= 0.0 || pattern.is_empty() {
        return pattern.to_vec();
    }
    let mut remaining = offset.rem_euclid(total);
    let n = pattern.len();
    let mut start_idx = 0usize;
    while remaining >= pattern[start_idx] {
        remaining -= pattern[start_idx];
        start_idx = (start_idx + 1) % n;
    }
    let first_seg_left = pattern[start_idx] - remaining;
    let mut result = Vec::with_capacity(n + 1);
    if start_idx % 2 == 0 {
        // Offset lands inside a DASH segment — start already drawing, for
        // whatever length of that dash remains.
        result.push(first_seg_left);
    } else {
        // Offset lands inside a GAP segment — start NOT drawing. A dash
        // pattern's convention always starts with an "on" (dash) entry, so
        // prepend a zero-length dash to preserve that parity before the
        // remaining gap length.
        result.push(0.0);
        result.push(first_seg_left);
    }
    let mut i = (start_idx + 1) % n;
    while i != start_idx {
        result.push(pattern[i]);
        i = (i + 1) % n;
    }
    result
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
            let off = item.dash_offset.unwrap_or(0.0);
            // vello 0.9's GPU stroke path (stroke_gpu_inner -> kurbo::dash())
            // silently ignores the `dash_offset` argument to with_dashes —
            // confirmed by hardcoding a large offset here directly and
            // rendering: zero visual difference regardless of value, on a
            // freshly rebuilt wasm on a never-cached preview port. Rather
            // than wait on an upstream fix (or risk a vello bump re-hitting
            // the wgpu/WebGPU version-drift issue this crate is deliberately
            // pinned against, see Cargo.toml's own comment), bake the phase
            // shift into the pattern array itself and always pass offset
            // 0.0, which does render correctly.
            let shifted = if off != 0.0 { dash_pattern_with_offset(pattern, off) } else { pattern.clone() };
            stroke = stroke.with_dashes(0.0, shifted);
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
    // Logical editor zoom, excluding the device-pixel ratio baked into
    // `zoom` for geometry rasterization. Pixel-space effects use this so
    // Retina displays do not accidentally double their radius/block size.
    effect_zoom: f64,
    rotation: f64,
    pivot_x: f64,
    pivot_y: f64,
}
impl Default for Viewport {
    fn default() -> Self {
        Viewport { pan_x: 0.0, pan_y: 0.0, zoom: 1.0, effect_zoom: 1.0, rotation: 0.0, pivot_x: 0.0, pivot_y: 0.0 }
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

/// Device-pixel bounding box of the document under the current viewport.
///
/// JS deliberately passes the artboard centre as the viewport pivot (see
/// engine-bridge.js::syncViewport and resizeEngineOffscreen), so twice that
/// pivot is the document's stable world-space size. Adjustment/effect layers
/// need this box as their procedural reference frame: the render target is the
/// editor viewport and therefore stays a fixed screen size while zoom/pan
/// changes. Using the render-target rectangle made custom shader patterns
/// regenerate in viewport space whenever the user zoomed.
///
/// Keep the full-target fallback for callers that intentionally use the
/// documented `(0,0)` pivot convention or render before set_viewport().
fn document_bbox_px(
    viewport: &Viewport,
    target_width: u32,
    target_height: u32,
) -> (f32, f32, f32, f32) {
    if viewport.pivot_x <= 0.0 || viewport.pivot_y <= 0.0 {
        return (0.0, 0.0, target_width as f32, target_height as f32);
    }
    let document = Rect::new(0.0, 0.0, viewport.pivot_x * 2.0, viewport.pivot_y * 2.0);
    let rect = viewport.transform().transform_rect_bbox(document);
    (
        rect.x0 as f32,
        rect.y0 as f32,
        rect.width() as f32,
        rect.height() as f32,
    )
}

#[cfg(test)]
mod viewport_document_bbox_tests {
    use super::*;

    fn viewport_for_zoom(zoom: f64, pan_x: f64, pan_y: f64) -> Viewport {
        let pivot_x = 960.0;
        let pivot_y = 540.0;
        Viewport {
            // Same adjusted-pan convention as engine-bridge.js::syncViewport.
            pan_x: pan_x + pivot_x * (zoom - 1.0),
            pan_y: pan_y + pivot_y * (zoom - 1.0),
            zoom,
            effect_zoom: zoom,
            rotation: 0.0,
            pivot_x,
            pivot_y,
        }
    }

    #[test]
    fn document_local_coordinates_are_invariant_under_zoom_and_pan() {
        let world = vello::kurbo::Point::new(480.0, 270.0);
        for viewport in [
            viewport_for_zoom(0.48, 37.0, -21.0),
            viewport_for_zoom(1.01, -83.0, 46.0),
        ] {
            let bbox = document_bbox_px(&viewport, 1400, 900);
            let screen = viewport.transform() * world;
            let local_x = (screen.x - bbox.0 as f64) / bbox.2 as f64;
            let local_y = (screen.y - bbox.1 as f64) / bbox.3 as f64;
            assert!(
                (local_x - 0.25).abs() < 1e-6,
                "x changed with viewport: {local_x}"
            );
            assert!(
                (local_y - 0.25).abs() < 1e-6,
                "y changed with viewport: {local_y}"
            );
        }
    }

    #[test]
    fn missing_document_pivot_preserves_the_legacy_full_target_box() {
        let viewport = Viewport::default();
        assert_eq!(
            document_bbox_px(&viewport, 1400, 900),
            (0.0, 0.0, 1400.0, 900.0)
        );
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
    // Retained path store (2026-07-28) — the `images` pattern above, applied
    // to vector geometry. A stroke's segments used to be re-serialized into
    // the scene JSON, re-parsed by serde, and rebuilt into a BezPath on EVERY
    // render even though the geometry hadn't changed since the last frame —
    // measured as the reason 2000 vector strokes scrubbed at 31fps while
    // 2000 rasters (six numbers + an id per item) reached 61fps. JS registers
    // a path once via register_path() and then sends only `pathRef` in the
    // scene item; the JS side owns invalidation (a Paper `_changed` hook
    // clears the item→dict stamp on any geometry mutation) and retirement
    // (FinalizationRegistry on the stroke dicts → retire_paths()). This side
    // is a dumb map on purpose: no LRU, no eviction heuristics — a silent
    // engine-side eviction would make a referenced path vanish from the
    // picture with no JS-visible signal (same reasoning as registeredImageIds
    // never guessing at lifetimes). A pathRef miss paints nothing and warns.
    paths: std::collections::HashMap<String, BezPath>,
    // 1×1 transparent "atlas keepalive" drawn far off-canvas into EVERY
    // scene (see composite_scene) — works around a vello 0.9 atlas bug
    // found live (2026-07-17, "trait bitmap brush disparaît après dessin
    // + scrub", atlas Debug logs): when a resolve pass contains ZERO
    // images, the image atlas reports 1×1 and render_encoding_coarse
    // FREES the 2048² GPU atlas texture; when an image reappears next
    // frame the CPU-side ImageCache still has it resident with
    // dirty=false, so the regrown atlas texture is brand-new and BLANK
    // with 0 uploads — the image samples nothing and disappears until
    // something forces a re-upload. Observed log sequence:
    //   2048x2048 Created (1 upload) → 1x1 Resized (0) →
    //   2048x2048 Resized (0 uploads!) → invisible.
    // A scrub triggers it constantly: each frame change rebuilds Rasters
    // whose async decode hasn't finished, so scenes momentarily contain
    // no images. Keeping ONE image alive in every scene pins the atlas
    // size (growth still goes through vello's bump+repack path, which
    // correctly re-marks residents dirty) so the texture is never freed.
    atlas_keepalive: vello::peniko::ImageData,
    // ---- Custom blend compositor (see blend.wgsl's own doc comment for
    // why this exists instead of vello's own push_layer(Mix::X)) ----
    // Fullscreen-triangle pipeline: samples a backdrop + a single layer's
    // isolated render, applies the CSS/W3C Mix formula, writes the SrcOver
    // composite. Built once; only ever touched when a scene actually
    // contains a blended layer (see composite_scene's fast-path check) —
    // the ordinary no-blend render path never allocates or binds any of
    // this, so it costs nothing when unused.
    blend_pipeline: wgpu::RenderPipeline,
    blend_bind_group_layout: wgpu::BindGroupLayout,
    blend_sampler: wgpu::Sampler,
    blend_uniform_buf: wgpu::Buffer,
    // Reused every layer: vello's render_to_texture target for ONE layer's
    // own content, painted alone against a transparent backdrop. Needs
    // STORAGE_BINDING (vello's compute-based renderer writes via storage,
    // not a traditional render-pass attachment — see render_to_texture's
    // own doc comment) plus TEXTURE_BINDING to be sampled as `source_tex`.
    blend_layer_tex: wgpu::Texture,
    blend_layer_view: wgpu::TextureView,
    // Ping-pong accumulator pair: composite_pass always reads one and
    // writes the other, so the roles swap after every layer instead of
    // allocating a fresh output texture per layer. RENDER_ATTACHMENT (my
    // own fragment shader writes here, not vello) + TEXTURE_BINDING
    // (sampled as `backdrop_tex` next iteration) + COPY_SRC/DST (seeding
    // the clear color, copying the final result into self.offscreen).
    blend_accum_a: wgpu::Texture,
    blend_accum_a_view: wgpu::TextureView,
    blend_accum_b: wgpu::Texture,
    blend_accum_b_view: wgpu::TextureView,
    // ---- Track matte compositor (matte.wgsl, scouted from Caddis's
    // Layer.matteMode field — see composite_scene's matte handling) ----
    // Same fullscreen-triangle shape as the blend pipeline, kept fully
    // separate (own textures, own pipeline) rather than shoehorned into
    // blend_layer_tex: a matted layer needs BOTH its own isolated render
    // AND its matte source's isolated render alive at once (two inputs to
    // one pass), which a single reused scratch texture can't hold.
    matte_pipeline: wgpu::RenderPipeline,
    matte_bind_group_layout: wgpu::BindGroupLayout,
    matte_sampler: wgpu::Sampler,
    matte_uniform_buf: wgpu::Buffer,
    matte_source_tex: wgpu::Texture,
    matte_source_view: wgpu::TextureView,
    matte_result_tex: wgpu::Texture,
    matte_result_view: wgpu::TextureView,
    // ---- Vector masks (2026-08, AE-style "Mask" — see composite_scene's
    // mask handling) — NOT a new pipeline: masks reuse matte_pipeline
    // itself (the "multiply alpha" math a Subtract/Intersect combine needs
    // IS matte_pass's own alpha-matte formula) and blur_pass for feather.
    // Only new textures are needed: mask_scratch (vello writes a single
    // mask's own white-filled geometry here, or the union of every
    // Add-mode mask in one render — STORAGE_BINDING, same shape as
    // matte_source), and an accum ping-pong pair + one result texture
    // (RENDER_ATTACHMENT, our own matte_pass writes these, same shape as
    // matte_result) for chaining Subtract/Intersect masks one at a time
    // and then applying the finished silhouette to the layer's own
    // content. See composite_scene's doc comment on the combine algorithm.
    mask_scratch_tex: wgpu::Texture,
    mask_scratch_view: wgpu::TextureView,
    mask_accum_a_tex: wgpu::Texture,
    mask_accum_a_view: wgpu::TextureView,
    mask_accum_b_tex: wgpu::Texture,
    mask_accum_b_view: wgpu::TextureView,
    mask_applied_tex: wgpu::Texture,
    mask_applied_view: wgpu::TextureView,
    // ---- Feather/blur compositor (blur.wgsl, see composite_scene's blur
    // handling and blur.wgsl's own doc comment) ----
    // blur_result reuses matte_result's exact texture shape (RENDER_ATTACHMENT
    // | TEXTURE_BINDING, our own fragment shader writes it) — same
    // create_matte_result_texture() constructor, just a second instance, not
    // worth a parallel same-shaped function.
    blur_pipeline: wgpu::RenderPipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    blur_sampler: wgpu::Sampler,
    blur_uniform_buf: wgpu::Buffer,
    blur_result_tex: wgpu::Texture,
    blur_result_view: wgpu::TextureView,
    // Intermediate texture for the horizontal pass's output before the
    // vertical pass reads it (2026-07, separable 2-pass blur rewrite) —
    // distinct from blur_result_tex/view, which every call site still uses
    // to hold the FINAL blurred result (or, for "glow", as blur_pass's own
    // scratch before the screen-blend composite — unrelated to this one).
    blur_scratch_tex: wgpu::Texture,
    blur_scratch_view: wgpu::TextureView,
    bloom_extract_tex: wgpu::Texture,
    bloom_extract_view: wgpu::TextureView,
    // ---- Effect (adjustment) layers (2026-07) — color_adjust.wgsl. No
    // result_tex/view of its own: unlike blur_result above (used for a
    // per-layer blur that still needs to flow into the ordinary composite/
    // blend step afterward), an effect layer's pass writes DIRECTLY into
    // whichever blend_accum_a/b view is the ping-pong's next target — see
    // composite_scene's is_effect_layer branch.
    color_pipeline: wgpu::RenderPipeline,
    color_bind_group_layout: wgpu::BindGroupLayout,
    color_sampler: wgpu::Sampler,
    color_uniform_buf: wgpu::Buffer,
    // "glow" effectType needs no pipeline of its own — it reuses blur_pass
    // (into blur_result_view as scratch) followed by composite_pass in
    // "screen" mode, both already wired for other purposes.
    vignette_pipeline: wgpu::RenderPipeline,
    vignette_bind_group_layout: wgpu::BindGroupLayout,
    vignette_sampler: wgpu::Sampler,
    vignette_uniform_buf: wgpu::Buffer,
    // ---- Simple single-pass effect library (2026-07) — simple_fx.wgsl.
    // ONE shared pipeline for all 10 effect_ids (see that file's own doc
    // comment for why these, unlike blur/vignette/color_adjust, don't get
    // one pipeline each) — same "writes directly into the ping-pong
    // accumulator" contract as color_pipeline/vignette_pipeline above.
    simple_fx_pipeline: wgpu::RenderPipeline,
    simple_fx_bind_group_layout: wgpu::BindGroupLayout,
    simple_fx_sampler: wgpu::Sampler,
    simple_fx_uniform_buf: wgpu::Buffer,
    // ---- Effects stack ping-pong scratch (2026-07 rewrite) — dedicated
    // pair, distinct from blur_pass's OWN internal blur_scratch_view: a
    // multi-effect stack that includes "blur" would otherwise alias reads
    // and writes on the same texture (blur_pass reads `source_view` and
    // uses blur_scratch_view as ITS OWN intermediate — if the outer stack's
    // ping-pong also happened to BE blur_scratch_view, the horizontal
    // sub-pass would read and write the same texture in one pass, which is
    // a WebGPU validation error). See apply_effect_stack's doc comment.
    effect_stack_a_tex: wgpu::Texture,
    effect_stack_a_view: wgpu::TextureView,
    effect_stack_b_tex: wgpu::Texture,
    effect_stack_b_view: wgpu::TextureView,
    // ---- Per-ELEMENT effects ping-pong (2026-07, "possible de différencié
    // les effet par éléments sélectionné") — ACCUMULATES a layer's own
    // paint order across multiple item-groups when at least one item
    // carries its own effects stack (paint_layer_with_element_effects):
    // each group (a run of plain items, or one effect-processed item) gets
    // composited onto whichever of this pair is CURRENT, ping-ponging to
    // the other, so by the time every group has been painted the final
    // view holds the whole layer exactly as if every item had painted into
    // one ordinary Scene — just with the effected item(s) isolated and
    // processed along the way. Distinct from effect_stack_a/b (that pair
    // is apply_effect_stack's OWN internal ping-pong for chaining an
    // item's/layer's multiple effects; aliasing the two would corrupt
    // whichever runs while the other still holds a pending read).
    element_build_a_tex: wgpu::Texture,
    element_build_a_view: wgpu::TextureView,
    element_build_b_tex: wgpu::Texture,
    element_build_b_view: wgpu::TextureView,
    // ---- User-authored custom WGSL effects (2026-07, feedback: "la
    // possibilité d'ajouter ses propres effets wgsl et leur paramètre
    // correspondant") — each entry is a full pipeline built at RUNTIME from
    // JS-supplied fragment-shader source, reusing simple_fx_bind_group_layout
    // (same texture+sampler+Params(32-byte) contract every built-in effect
    // already uses) so it drops into apply_effect_stack/run_one_effect
    // exactly like any built-in effect_type — just keyed by a JS-chosen
    // string id (e.g. "custom:<uuid>") instead of a hardcoded match arm.
    // See register_custom_effect's own doc comment for why compiling
    // arbitrary WGSL at runtime is safe here (no unsafe/native FFI
    // involved — worst case is a normal WebGPU validation error, reported
    // async via the browser's own uncaptured-error mechanism, same as any
    // other shader bug in this file; it can't corrupt or crash the wasm
    // instance).
    custom_effect_pipelines: std::collections::HashMap<String, wgpu::RenderPipeline>,
    // Current scene time in seconds, copied from SceneIn before each render.
    // Custom/simple WGSL effects receive it as params.time.
    fx_time: f32,
}

// ---- Blend compositor plumbing (see blend.wgsl's doc comment) ----
const BLEND_SCRATCH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn create_blend_layer_texture(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blend-layer-target"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLEND_SCRATCH_FORMAT,
        // vello's render_to_texture writes via a compute pipeline, hence
        // STORAGE_BINDING (matches self.offscreen's own usage flags) —
        // TEXTURE_BINDING lets the blend shader sample it as `source_tex`.
        // COPY_DST/COPY_SRC (2026-07, per-element effects):
        // paint_layer_with_element_effects' final step copies the
        // assembled result straight into this texture (reused as
        // blend_layer_view's own backing store, DST), and a plain
        // (non-effected) run copies straight OUT of it as that run's
        // result (SRC) when it's the very first run in the layer — without
        // both flags either copy is a WebGPU validation error.
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

fn create_blend_accum_texture(device: &wgpu::Device, width: u32, height: u32, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLEND_SCRATCH_FORMAT,
        // RENDER_ATTACHMENT: our OWN fragment shader (not vello) writes here.
        // TEXTURE_BINDING: sampled as `backdrop_tex` on the next layer.
        // COPY_SRC/DST: seeding the clear color, and copying the final
        // composited result into self.offscreen at the end.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

fn create_blend_pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler, wgpu::Buffer) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blend-compositor"),
        source: wgpu::ShaderSource::Wgsl(include_str!("blend.wgsl").into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blend-bind-group-layout"),
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
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
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
        label: Some("blend-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blend-pipeline"),
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
                format: BLEND_SCRATCH_FORMAT,
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
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("blend-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    // 16 bytes: one u32 (mode) + padding to satisfy WGSL's minimum uniform
    // buffer struct alignment (16-byte rounding) — see blend.wgsl's Params.
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blend-params"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, bind_group_layout, sampler, uniform_buf)
}

// ---- Track matte compositor plumbing (see matte.wgsl's doc comment) ----
// matte_source_tex/matte_result_tex reuse the exact same descriptor as
// blend_layer_tex (STORAGE_BINDING for matte_source — vello writes it via
// render_to_texture same as any isolated layer render; matte_result is
// written by OUR OWN fragment shader instead, so it needs
// RENDER_ATTACHMENT, not STORAGE_BINDING, mirroring blend_accum's usage
// flags rather than blend_layer's).
fn create_matte_result_texture(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("matte-result-target"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLEND_SCRATCH_FORMAT,
        // COPY_DST (2026-08, masks): the Add-mask union step needs to copy
        // mask_scratch's vello output (STORAGE_BINDING) into mask_accum_a
        // (built via this function) so the Subtract/Intersect loop always
        // reads/writes RENDER_ATTACHMENT-shaped textures — every OTHER
        // texture built here (matte_result/blur_result/blur_scratch/
        // bloom_extract) is only ever a render-pass TARGET, never a plain
        // copy destination, so this flag was never needed before; adding
        // it is a pure no-op for them.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

fn create_matte_pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler, wgpu::Buffer) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("matte-compositor"),
        source: wgpu::ShaderSource::Wgsl(include_str!("matte.wgsl").into()),
    });
    // Same bind group shape as the blend pipeline (2 textures + sampler +
    // uniform) — see create_blend_pipeline for the entry-by-entry rationale.
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("matte-bind-group-layout"),
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
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
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
        label: Some("matte-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("matte-pipeline"),
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
                format: BLEND_SCRATCH_FORMAT,
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
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("matte-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    // 16 bytes: mode (u32) + invert (u32) + padding — see matte.wgsl's Params.
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("matte-params"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, bind_group_layout, sampler, uniform_buf)
}

/// Runs matte.wgsl: multiplies `layer_view`'s alpha by `matte_view`'s alpha
/// (mode 0) or luma (mode 1), optionally inverted, writing into
/// `target_view`. Mirrors composite_pass's shape (see its own doc comment).
#[allow(clippy::too_many_arguments)]
fn matte_pass(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
    layer_view: &wgpu::TextureView,
    matte_view: &wgpu::TextureView,
    mode: u32,
    invert: bool,
    target_view: &wgpu::TextureView,
) {
    let mut payload = [0u8; 16];
    payload[0..4].copy_from_slice(&mode.to_le_bytes());
    payload[4..8].copy_from_slice(&(invert as u32).to_le_bytes());
    queue.write_buffer(uniform_buf, 0, &payload);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("matte-bind-group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(layer_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(matte_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: uniform_buf.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("matte-composite") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("matte-composite-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

/// Parses a `LayerIn.matte_mode` string into (mode, invert), or None for
/// "no matte" (absent field, "none", or any unrecognized value — same
/// forgiving-default spirit as mix_mode_index treating an unknown blend
/// mode as Normal rather than erroring).
fn matte_mode_of(s: Option<&str>) -> Option<(u32, bool)> {
    match s {
        Some("alpha") => Some((0, false)),
        Some("alphaInverted") => Some((0, true)),
        Some("luma") => Some((1, false)),
        Some("lumaInverted") => Some((1, true)),
        _ => None,
    }
}

/// The ONE place "which layer is layer `i`'s matte source" is answered
/// (2026-07-31, uid-based mattes) — used by BOTH composite_scene sites (the
/// is_matte_source precompute AND the paint-time lookup), which previously
/// each hardcoded `i + 1` independently: exactly the duplicated-readers
/// drift trap this repo's CLAUDE.md §3 documents for render/render_to_pixels.
/// Explicit matte_source_index wins (JS-resolved from matteSourceLayerUid,
/// may point anywhere in the stack, above or below); None falls back to the
/// legacy implicit "directly above (i+1)" convention so old scene JSON keeps
/// rendering unchanged. Out-of-range or self-referencing indices return None
/// — the matte degrades to a no-op for the frame instead of panicking or
/// silently masking against the wrong layer.
fn resolve_matte_source(layers: &[LayerIn], i: usize) -> Option<usize> {
    if matte_mode_of(layers[i].matte_mode.as_deref()).is_none() {
        return None;
    }
    let n = layers.len();
    match layers[i].matte_source_index {
        Some(s) if s < n && s != i => Some(s),
        Some(_) => None,
        None => {
            if i + 1 < n {
                Some(i + 1)
            } else {
                None
            }
        }
    }
}

// ---- Feather/blur compositor plumbing (see blur.wgsl's doc comment) ----
fn create_blur_pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler, wgpu::Buffer) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blur-compositor"),
        source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
    });
    // One texture + sampler + uniform (matte/blend need two textures for
    // their two-input formulas; blur only ever reads its own single source).
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blur-bind-group-layout"),
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
        label: Some("blur-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blur-pipeline"),
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
                format: BLEND_SCRATCH_FORMAT,
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
    // Linear filtering (unlike matte's Nearest — blur specifically WANTS
    // the GPU's bilinear interpolation between the 81 sample points, it's
    // part of how the softening reads smoothly rather than in visible rings).
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("blur-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // 32 bytes: radius_px, tex_w, tex_h, dir_x, dir_y, + 3 padding floats —
    // see blur.wgsl's Params (grew from 16 bytes when the pass became
    // separable and needed a blur-direction vector for the H/V passes).
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blur-params"),
        size: 32,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, bind_group_layout, sampler, uniform_buf)
}

/// Runs blur.wgsl ONCE, along a single axis (`dir_x`/`dir_y`, a unit vector —
/// (1,0) for horizontal, (0,1) for vertical): writes a blurred copy of
/// `source_view` into `target_view`. Mirrors matte_pass/composite_pass's
/// shape. Not called directly outside this file — see `blur_pass` below,
/// which chains two of these (H then V) into the real separable blur.
#[allow(clippy::too_many_arguments)]
fn blur_pass_1d(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
    source_view: &wgpu::TextureView,
    radius_px: f32,
    width: u32,
    height: u32,
    dir_x: f32,
    dir_y: f32,
    target_view: &wgpu::TextureView,
) {
    let mut payload = [0u8; 32];
    payload[0..4].copy_from_slice(&radius_px.to_le_bytes());
    payload[4..8].copy_from_slice(&(width as f32).to_le_bytes());
    payload[8..12].copy_from_slice(&(height as f32).to_le_bytes());
    payload[12..16].copy_from_slice(&dir_x.to_le_bytes());
    payload[16..20].copy_from_slice(&dir_y.to_le_bytes());
    queue.write_buffer(uniform_buf, 0, &payload);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blur-bind-group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: uniform_buf.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("blur-composite") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blur-composite-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

/// Real entry point (2026-07 rewrite — see blur.wgsl's own header comment
/// for why this became two passes): runs a horizontal blur_pass_1d from
/// `source_view` into `scratch_view`, then a vertical one from
/// `scratch_view` into `target_view`. `scratch_view` must be a texture
/// distinct from both `source_view` and `target_view` (the engine's
/// `blur_scratch_view` field exists exactly for this). Same call shape as
/// the old single-pass version plus one extra scratch-texture argument, so
/// every call site just threads that through.
#[allow(clippy::too_many_arguments)]
fn blur_pass(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
    source_view: &wgpu::TextureView,
    radius_px: f32,
    width: u32,
    height: u32,
    scratch_view: &wgpu::TextureView,
    target_view: &wgpu::TextureView,
) {
    blur_pass_1d(
        device, queue, pipeline, bind_group_layout, sampler, uniform_buf,
        source_view, radius_px, width, height, 1.0, 0.0, scratch_view,
    );
    blur_pass_1d(
        device, queue, pipeline, bind_group_layout, sampler, uniform_buf,
        scratch_view, radius_px, width, height, 0.0, 1.0, target_view,
    );
}

/// Brightness/contrast pipeline for effect (adjustment) layers — same
/// single-texture-input shape as create_blur_pipeline, just a different
/// shader/uniform payload (see color_adjust.wgsl).
fn create_color_adjust_pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler, wgpu::Buffer) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("color-adjust-compositor"),
        source: wgpu::ShaderSource::Wgsl(include_str!("color_adjust.wgsl").into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("color-adjust-bind-group-layout"),
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
        label: Some("color-adjust-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("color-adjust-pipeline"),
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
                format: BLEND_SCRATCH_FORMAT,
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
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("color-adjust-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // 16 bytes: brightness (f32) + contrast (f32) + 2 padding floats — see
    // color_adjust.wgsl's Params.
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("color-adjust-params"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, bind_group_layout, sampler, uniform_buf)
}

/// Runs color_adjust.wgsl: writes a brightness/contrast-adjusted copy of
/// `source_view` into `target_view`. Mirrors blur_pass's shape.
#[allow(clippy::too_many_arguments)]
fn color_adjust_pass(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
    source_view: &wgpu::TextureView,
    brightness: f32,
    contrast: f32,
    target_view: &wgpu::TextureView,
) {
    let mut payload = [0u8; 16];
    payload[0..4].copy_from_slice(&brightness.to_le_bytes());
    payload[4..8].copy_from_slice(&contrast.to_le_bytes());
    queue.write_buffer(uniform_buf, 0, &payload);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("color-adjust-bind-group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: uniform_buf.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("color-adjust-composite") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("color-adjust-composite-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

/// Vignette pipeline — identical single-texture-input shape as
/// create_color_adjust_pipeline, just a different shader/uniform payload
/// (see vignette.wgsl). Mirrored rather than parameterized/shared, same
/// "blur_pass and color_adjust_pass are already near-identical duplicates"
/// precedent already established in this file.
fn create_vignette_pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler, wgpu::Buffer) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("vignette-compositor"),
        source: wgpu::ShaderSource::Wgsl(include_str!("vignette.wgsl").into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vignette-bind-group-layout"),
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
        label: Some("vignette-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vignette-pipeline"),
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
                format: BLEND_SCRATCH_FORMAT,
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
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("vignette-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // 16 bytes: strength (f32) + radius (f32) + 2 padding floats — see
    // vignette.wgsl's Params.
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vignette-params"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, bind_group_layout, sampler, uniform_buf)
}

/// Simple-effect-library pipeline (simple_fx.wgsl) — one shared pipeline for
/// all 10 effect_ids; identical single-texture-input shape as
/// create_vignette_pipeline, just a wider (8-float) uniform payload so one
/// Params struct can carry every effect's own p1/p2/p3 + resolution/time.
fn create_simple_fx_pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler, wgpu::Buffer) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("simple-fx-compositor"),
        source: wgpu::ShaderSource::Wgsl(include_str!("simple_fx.wgsl").into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("simple-fx-bind-group-layout"),
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
        label: Some("simple-fx-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("simple-fx-pipeline"),
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
                format: BLEND_SCRATCH_FORMAT,
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
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("simple-fx-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // 64 bytes: effect_id, p1, p2, p3, tex_w, tex_h, time, p4, bbox_x,
    // bbox_y, bbox_w, bbox_h, p5, p6, p7, p8 — see simple_fx.wgsl's Params.
    // (2026-07-30: grew from 32 to 48 bytes to add the bbox_* fields; 2026-08:
    // grew from 48 to 64 to append p5..p8 — see run_one_effect's and
    // simple_fx_pass's own doc comments for why.)
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("simple-fx-params"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, bind_group_layout, sampler, uniform_buf)
}

/// Builds a pipeline for a user-authored custom effect (2026-07) from a
/// FULL WGSL source string — reuses the EXACT bind-group-layout
/// (texture@0, sampler@1, Params-uniform@2) `bind_group_layout` already
/// describes, so it can be driven by the same simple_fx_pass function as
/// every built-in effect, just with a different pipeline reference. The
/// caller (register_custom_effect) is responsible for wrapping the user's
/// fragment-shader BODY into this full document — see that function's own
/// doc comment for the template (vs_main/bindings/Params are always the
/// same regardless of author, only fs_main's body is user-supplied).
fn create_custom_effect_pipeline(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, wgsl_source: &str) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("custom-fx-compositor"),
        source: wgpu::ShaderSource::Wgsl(wgsl_source.to_string().into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("custom-fx-pipeline-layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("custom-fx-pipeline"),
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
                format: BLEND_SCRATCH_FORMAT,
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
    })
}

/// Runs simple_fx.wgsl: writes the selected effect_id's transform of
/// `source_view` into `target_view`.
#[allow(clippy::too_many_arguments)]
fn simple_fx_pass(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
    source_view: &wgpu::TextureView,
    effect_id: f32,
    p1: f32,
    p2: f32,
    p3: f32,
    p4: f32,
    time: f32,
    tex_w: f32,
    tex_h: f32,
    bbox_x: f32,
    bbox_y: f32,
    bbox_w: f32,
    bbox_h: f32,
    // p5..p8 (2026-08, "possibilité de sortir plus de paramètres
    // d'effets") — appended as a NEW trailing 16-byte block (bytes 48..64)
    // rather than interleaved among the existing fields, so every byte
    // offset above stays exactly where it already was — zero risk of
    // silently shifting bbox_x/y/w/h under any of the many existing
    // effect-type call sites that don't pass p5..p8 at all yet.
    p5: f32,
    p6: f32,
    p7: f32,
    p8: f32,
    target_view: &wgpu::TextureView,
) {
    let mut payload = [0u8; 64];
    payload[0..4].copy_from_slice(&effect_id.to_le_bytes());
    payload[4..8].copy_from_slice(&p1.to_le_bytes());
    payload[8..12].copy_from_slice(&p2.to_le_bytes());
    payload[12..16].copy_from_slice(&p3.to_le_bytes());
    payload[16..20].copy_from_slice(&tex_w.to_le_bytes());
    payload[20..24].copy_from_slice(&tex_h.to_le_bytes());
    payload[24..28].copy_from_slice(&time.to_le_bytes());
    payload[28..32].copy_from_slice(&p4.to_le_bytes());
    payload[32..36].copy_from_slice(&bbox_x.to_le_bytes());
    payload[36..40].copy_from_slice(&bbox_y.to_le_bytes());
    payload[40..44].copy_from_slice(&bbox_w.to_le_bytes());
    payload[44..48].copy_from_slice(&bbox_h.to_le_bytes());
    payload[48..52].copy_from_slice(&p5.to_le_bytes());
    payload[52..56].copy_from_slice(&p6.to_le_bytes());
    payload[56..60].copy_from_slice(&p7.to_le_bytes());
    payload[60..64].copy_from_slice(&p8.to_le_bytes());
    queue.write_buffer(uniform_buf, 0, &payload);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("simple-fx-bind-group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: uniform_buf.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("simple-fx-composite") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("simple-fx-composite-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

/// Runs vignette.wgsl: writes a vignetted copy of `source_view` into
/// `target_view`. Mirrors color_adjust_pass's shape (same 2-float payload).
#[allow(clippy::too_many_arguments)]
fn vignette_pass(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
    source_view: &wgpu::TextureView,
    strength: f32,
    radius: f32,
    target_view: &wgpu::TextureView,
) {
    let mut payload = [0u8; 16];
    payload[0..4].copy_from_slice(&strength.to_le_bytes());
    payload[4..8].copy_from_slice(&radius.to_le_bytes());
    queue.write_buffer(uniform_buf, 0, &payload);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vignette-bind-group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: uniform_buf.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("vignette-composite") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vignette-composite-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

fn wgpu_color_from(c: Color) -> wgpu::Color {
    let rgba = c.to_rgba8();
    wgpu::Color { r: rgba.r as f64 / 255.0, g: rgba.g as f64 / 255.0, b: rgba.b as f64 / 255.0, a: rgba.a as f64 / 255.0 }
}

/// Clears `view` to a flat color via a plain wgpu render pass — used to
/// seed the blend accumulator with `base_color` before the first layer,
/// exactly mirroring what vello's own `render_to_texture(..., base_color)`
/// does for the ordinary (no-blend) fast path.
fn clear_texture(device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView, color: wgpu::Color) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("blend-clear") });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blend-clear-pass"),
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

/// Runs the blend.wgsl fullscreen pass: composites `source_view` over
/// `backdrop_view` using `mode` (see mix_mode_index), writing into
/// `target_view`. All GPU-side, synchronous — see blend.wgsl's own doc
/// comment for why this exists instead of vello's push_layer(Mix::X).
#[allow(clippy::too_many_arguments)]
fn composite_pass(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
    backdrop_view: &wgpu::TextureView,
    source_view: &wgpu::TextureView,
    mode: u32,
    target_view: &wgpu::TextureView,
) {
    let mut payload = [0u8; 16];
    payload[0..4].copy_from_slice(&mode.to_le_bytes());
    queue.write_buffer(uniform_buf, 0, &payload);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blend-bind-group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(backdrop_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(source_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: uniform_buf.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("blend-composite") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blend-composite-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

/// Paints a slice of items into `scene` — the exact same per-item logic
/// render()/render_to_pixels() always used, factored out so the ordinary
/// (fast, no-blend) path and the per-layer isolated-texture blend path
/// both go through one implementation (CLAUDE.md §3: duplicated render
/// logic drifting out of sync is the recurring bug class here). Takes a
/// slice rather than `&LayerIn` so paint_layer_with_element_effects can
/// paint just a RUN of one layer's items (the items between/around a
/// per-element-effect item) without needing a fake LayerIn wrapper.
fn paint_layer_items(
    scene: &mut Scene,
    items: &[ItemIn],
    view_tf: Affine,
    images: &std::collections::HashMap<String, vello::peniko::ImageData>,
    paths: &std::collections::HashMap<String, BezPath>,
) {
    for item in items {
        if let Some(img_ref) = &item.image {
            if let Some(image_data) = images.get(&img_ref.image_id) {
                let sx = img_ref.width / image_data.width as f64;
                let sy = img_ref.height / image_data.height as f64;
                let mut place = Affine::translate((img_ref.x, img_ref.y)) * Affine::scale_non_uniform(sx, sy);
                if img_ref.rotation != 0.0 {
                    // Spin around the DISPLAY rect's center (x,y is its
                    // top-left) — the same pivot convention the JS side's
                    // transformImageRect/serialization uses.
                    let (cx, cy) = (img_ref.x + img_ref.width / 2.0, img_ref.y + img_ref.height / 2.0);
                    place = Affine::translate((cx, cy))
                        * Affine::rotate(img_ref.rotation.to_radians())
                        * Affine::translate((-cx, -cy))
                        * place;
                }
                let brush = vello::peniko::ImageBrush::new(image_data.clone()).multiply_alpha(img_ref.opacity);
                scene.draw_image(&brush, view_tf * place);
            }
            continue;
        }
        // Retained-ref first: a pathRef item carries no segments at all, so
        // falling through to build_bezpath would silently skip it. A missing
        // store entry paints nothing but WARNS (console via console_log) —
        // it means the JS registration/retirement contract was broken, and a
        // silent skip here is exactly how "the shape vanished" class bugs
        // stay invisible (see the CompoundPath story in engine-bridge.js).
        let built: BezPath;
        let bez: &BezPath = if let Some(r) = &item.path_ref {
            match paths.get(r) {
                Some(p) => p,
                None => {
                    log::warn!("pathRef '{}' not in retained store — item skipped", r);
                    continue;
                }
            }
        } else {
            match build_bezpath(item) {
                Some(b) => {
                    built = b;
                    &built
                }
                None => continue,
            }
        };
        // A retained path is stored in its own untransformed space; its
        // per-item Motion chain arrives as one affine folded in here.
        let item_tf = match &item.path_transform {
            Some(m) => view_tf * Affine::new(*m),
            None => view_tf,
        };
        let paint_fill = |scene: &mut Scene| {
            if let Some(grad) = item.fill_gradient.as_ref().and_then(gradient_brush) {
                scene.fill(vello::peniko::Fill::NonZero, item_tf, &grad, None, bez);
            } else if let Some(fc) = item.fill_color {
                scene.fill(vello::peniko::Fill::NonZero, item_tf, color_from(fc), None, bez);
            }
        };
        let paint_stroke = |scene: &mut Scene| {
            if let Some(sc) = item.stroke_color {
                scene.stroke(&stroke_from(item), item_tf, color_from(sc), None, bez);
            }
        };
        if item.paint_order.as_deref() == Some("strokeFirst") {
            paint_stroke(scene);
            paint_fill(scene);
        } else {
            paint_fill(scene);
            paint_stroke(scene);
        }
    }
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
#[cfg(target_arch = "wasm32")]
pub async fn create_engine(
    canvas: HtmlCanvasElement,
    width: u32,
    height: u32,
) -> Result<VelloEngine, JsValue> {
    // Route vello's internal `log` lines to the browser console — added
    // while chasing the "texture bitmap disparaît au scrub" report
    // (2026-07-17; turned out to be the async-decode race fixed in desR,
    // app.js) and KEPT at Warn level: vello only logs at Warn for
    // genuinely abnormal conditions (e.g. render.rs's "Trying to paint
    // too large image" coarse-buffer overflow), which used to vanish
    // silently — the exact kind of signal that made that bug hunt blind.
    // Bump to Level::Debug locally to also see every image-atlas action
    // (Created/Resized/Reused, upload/eviction counts). Init can only run
    // once per wasm module; ignore the error if a second engine is ever
    // created.
    let _ = console_log::init_with_level(log::Level::Warn);
    console_error_panic_hook::set_once();
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
    // wgpu validation/OOM/internal errors from calls made after this point
    // (e.g. a malformed bind group, an unsupported texture usage combo) are
    // otherwise swallowed silently on wasm32 — there's no `log`/env_logger
    // sink installed, so they'd never reach the browser console at all,
    // making a broken render pass indistinguishable from "nothing to draw".
    device.on_uncaptured_error(std::sync::Arc::new(|e: wgpu::Error| {
        web_sys::console::error_1(&format!("[wgpu] uncaptured error: {e}").into());
    }));
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
        // COPY_DST: composite_scene's blend path (blend.wgsl) copies the
        // final ping-ponged accumulator texture into this one as its last
        // step — the ordinary no-blend fast path never writes here except
        // via render_to_texture (which doesn't need COPY_DST), so this was
        // missing until the blend path actually exercised it.
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let offscreen_view = offscreen.create_view(&wgpu::TextureViewDescriptor::default());
    let blitter = wgpu::util::TextureBlitter::new(&device, format);

    let (blend_pipeline, blend_bind_group_layout, blend_sampler, blend_uniform_buf) = create_blend_pipeline(&device);
    let (blend_layer_tex, blend_layer_view) = create_blend_layer_texture(&device, width, height);
    let (blend_accum_a, blend_accum_a_view) = create_blend_accum_texture(&device, width, height, "blend-accum-a");
    let (blend_accum_b, blend_accum_b_view) = create_blend_accum_texture(&device, width, height, "blend-accum-b");
    let (matte_pipeline, matte_bind_group_layout, matte_sampler, matte_uniform_buf) = create_matte_pipeline(&device);
    let (matte_source_tex, matte_source_view) = create_blend_layer_texture(&device, width, height);
    let (matte_result_tex, matte_result_view) = create_matte_result_texture(&device, width, height);
    let (mask_scratch_tex, mask_scratch_view) = create_blend_layer_texture(&device, width, height);
    let (mask_accum_a_tex, mask_accum_a_view) = create_matte_result_texture(&device, width, height);
    let (mask_accum_b_tex, mask_accum_b_view) = create_matte_result_texture(&device, width, height);
    let (mask_applied_tex, mask_applied_view) = create_matte_result_texture(&device, width, height);
    let (blur_pipeline, blur_bind_group_layout, blur_sampler, blur_uniform_buf) = create_blur_pipeline(&device);
    let (blur_result_tex, blur_result_view) = create_matte_result_texture(&device, width, height);
    let (blur_scratch_tex, blur_scratch_view) = create_matte_result_texture(&device, width, height);
    let (bloom_extract_tex, bloom_extract_view) = create_matte_result_texture(&device, width, height);
    let (color_pipeline, color_bind_group_layout, color_sampler, color_uniform_buf) = create_color_adjust_pipeline(&device);
    let (vignette_pipeline, vignette_bind_group_layout, vignette_sampler, vignette_uniform_buf) = create_vignette_pipeline(&device);
    let (simple_fx_pipeline, simple_fx_bind_group_layout, simple_fx_sampler, simple_fx_uniform_buf) = create_simple_fx_pipeline(&device);
    let (effect_stack_a_tex, effect_stack_a_view) = create_blend_accum_texture(&device, width, height, "effect-stack-a");
    let (effect_stack_b_tex, effect_stack_b_view) = create_blend_accum_texture(&device, width, height, "effect-stack-b");
    let (element_build_a_tex, element_build_a_view) = create_blend_accum_texture(&device, width, height, "element-build-a");
    let (element_build_b_tex, element_build_b_view) = create_blend_accum_texture(&device, width, height, "element-build-b");

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
        paths: std::collections::HashMap::new(),
        atlas_keepalive: vello::peniko::ImageData {
            data: vec![0u8, 0, 0, 0].into(),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        },
        blend_pipeline,
        blend_bind_group_layout,
        blend_sampler,
        blend_uniform_buf,
        blend_layer_tex,
        blend_layer_view,
        blend_accum_a,
        blend_accum_a_view,
        blend_accum_b,
        blend_accum_b_view,
        matte_pipeline,
        matte_bind_group_layout,
        matte_sampler,
        matte_uniform_buf,
        matte_source_tex,
        matte_source_view,
        matte_result_tex,
        matte_result_view,
        mask_scratch_tex,
        mask_scratch_view,
        mask_accum_a_tex,
        mask_accum_a_view,
        mask_accum_b_tex,
        mask_accum_b_view,
        mask_applied_tex,
        mask_applied_view,
        blur_pipeline,
        blur_bind_group_layout,
        blur_sampler,
        blur_uniform_buf,
        blur_result_tex,
        blur_result_view,
        blur_scratch_tex,
        blur_scratch_view,
        bloom_extract_tex,
        bloom_extract_view,
        color_pipeline,
        color_bind_group_layout,
        color_sampler,
        color_uniform_buf,
        vignette_pipeline,
        vignette_bind_group_layout,
        vignette_sampler,
        vignette_uniform_buf,
        simple_fx_pipeline,
        simple_fx_bind_group_layout,
        simple_fx_sampler,
        simple_fx_uniform_buf,
        effect_stack_a_tex,
        effect_stack_a_view,
        effect_stack_b_tex,
        effect_stack_b_view,
        element_build_a_tex,
        element_build_a_view,
        element_build_b_tex,
        element_build_b_view,
        custom_effect_pipelines: std::collections::HashMap::new(),
        fx_time: 0.0,
    })
}

// Internal helper, not part of the JS-facing API — kept in its own plain
// `impl` block (rather than inside the `#[wasm_bindgen] impl` below) so the
// wasm_bindgen macro never has to reason about a non-exported method.
impl VelloEngine {
    /// Shared by render() and render_to_pixels() (CLAUDE.md §3: these two
    /// must stay in sync) — paints `scene_in` into `self.offscreen_view`.
    ///
    /// Fast path (no layer has a real blend_mode, the overwhelmingly common
    /// case): unchanged from the pre-fix code — one Scene, one
    /// render_to_texture call, zero extra allocation or GPU work.
    ///
    /// Slow path (>=1 blended layer OR >=1 track-matted layer): renders each
    /// layer to its own isolated, transparent-backed texture (vello,
    /// unmodified) and composites them one at a time through blend.wgsl's
    /// fullscreen pass — Normal for an unblended layer (plain SrcOver, mode
    /// 0), the real Mix formula for a blended one. See blend.wgsl's and
    /// mix_mode_index's doc comments for why this exists at all.
    ///
    /// Track matte (2026-07, scouted from Caddis's Layer.matteMode): layer
    /// i's matte_mode names how the layer immediately ABOVE it (i+1, AE
    /// convention — the source is implicit, never referenced by id) masks
    /// it. That source layer is rendered isolated too, matte.wgsl multiplies
    /// layer i's alpha by the source's alpha/luma, and the MASKED result
    /// (not the raw layer) is what composite_pass blends into the
    /// accumulator. The source layer itself is then skipped as its own
    /// visible layer (`is_matte_source`) — matching AE, where a matte
    /// source never draws on its own once consumed.
    /// Pins the vello image atlas by encoding the 1×1 keepalive image far
    /// off-canvas — see `atlas_keepalive`'s field comment for the bug this
    /// works around. Must run for EVERY scene the renderer resolves (fast
    /// path, per-layer blend scenes, matte-source scenes): each
    /// render_to_texture call is its own resolve pass, and a single
    /// imageless pass is enough to free the atlas texture.
    fn push_atlas_keepalive(&self, scene: &mut Scene) {
        scene.draw_image(
            &vello::peniko::ImageBrush::new(self.atlas_keepalive.clone()),
            Affine::translate((-1.0e7, -1.0e7)),
        );
    }
    /// Dispatches ONE effects-stack entry: `source` in, transformed pixels
    /// into `target`. `&self` only (no mutation) — every *_pass helper takes
    /// its device/queue/pipeline as already-split borrowed params, so this
    /// never needs `&mut self`, which is what lets apply_effect_stack below
    /// hold multiple `&self.effect_stack_*_view` borrows across a loop
    /// without fighting the borrow checker.
    // `bbox` = (x, y, w, h) in DEVICE PIXELS — the on-screen bounding box of
    // whatever this effect is actually attached to (one item for a per-
    // element effect, the whole layer's items for a per-layer effect, or
    // the full canvas for an adjustment/effect layer, which has no shape of
    // its own — see items_bbox_px and this fn's three call sites). Forwarded
    // into simple_fx_pass's Params uniform so shader bodies that need a
    // "center of MY content" concept (Twirl/Bulge/Spherize/etc., see
    // shader-effects-library.js's local_uv convention) don't have to assume
    // that's the center of the canvas — see local_uv's own doc comment
    // (register_custom_effect) for why that assumption was wrong: Cyril,
    // 2026-07-30, "un effet Wgsl... quand on zoom dans le canvas... même en
    // bougeant le canvas avec la main" — reproduced live, a Twirl effect's
    // pattern changed under PURE PANNING (no zoom at all), which only makes
    // sense if the effect's reference frame was the viewport, not the shape.
    fn run_one_effect(&self, eff: &EffectIn, source: &wgpu::TextureView, target: &wgpu::TextureView, bbox: (f32, f32, f32, f32)) {
        // Every PIXEL-space effect parameter below (blur/glow radius,
        // pixelate block size, chromatic-aberration offset, contour
        // thickness, halftone cell size) is applied to the raster AFTER
        // view_tf (pan/zoom/rotation) already baked the current zoom into
        // the composited scene — so a fixed "8px" radius is 8 SCREEN
        // pixels regardless of zoom, meaning the SAME setting looks
        // relatively thinner/weaker at high zoom (content grew, effect
        // didn't) and relatively thicker/stronger at low zoom (content
        // shrank, effect didn't) — feedback 2026-07: "normal que les
        // effets change de size en fonction du zoom ?" (no, it wasn't).
        // Scaling every such parameter by the current zoom makes the
        // effect a fixed size relative to the DOCUMENT instead of the
        // screen, exactly like every other zoom-invariant editor
        // convention already in this app (see gizmo_handles' own
        // `24.0 / self.viewport.zoom` for the inverse case — a handle
        // that must NOT grow with zoom). Not applied to custom: WGSL
        // effects below — p1..p4 there have no fixed meaning this
        // function could assume is a pixel size.
        let z = self.viewport.effect_zoom.max(0.0001) as f32;
        match eff.effect_type.as_str() {
            "colorAdjust" => color_adjust_pass(
                &self.device, &self.queue, &self.color_pipeline, &self.color_bind_group_layout, &self.color_sampler, &self.color_uniform_buf,
                source, eff.p1.unwrap_or(0.0), eff.p2.unwrap_or(0.0), target,
            ),
            "vignette" => vignette_pass(
                &self.device, &self.queue, &self.vignette_pipeline, &self.vignette_bind_group_layout, &self.vignette_sampler, &self.vignette_uniform_buf,
                source, eff.p1.unwrap_or(0.5), eff.p2.unwrap_or(0.4), target,
            ),
            "glow" => {
                // Bloom: blur a copy into the shared blur-scratch/result
                // pair (private to blur_pass, distinct from this stack's
                // own ping-pong — see effect_stack_a/b_view's doc comment),
                // then screen-blend it back on top of the UNBLURRED
                // `source` — reuses blur_pass + the ordinary layer
                // compositor's "screen" blend mode rather than a new shader.
                blur_pass(
                    &self.device, &self.queue, &self.blur_pipeline, &self.blur_bind_group_layout, &self.blur_sampler, &self.blur_uniform_buf,
                    source, eff.p1.unwrap_or(16.0) * z, self.width, self.height, &self.blur_scratch_view, &self.blur_result_view,
                );
                composite_pass(
                    &self.device, &self.queue, &self.blend_pipeline, &self.blend_bind_group_layout, &self.blend_sampler, &self.blend_uniform_buf,
                    source, &self.blur_result_view, 2, target,
                );
            }
            "hqBloom" => {
                // Threshold/brightness pass, no "center of my shape" concept
                // — full-canvas bbox (a no-op for this effect either way).
                simple_fx_pass(
                    &self.device, &self.queue, &self.simple_fx_pipeline, &self.simple_fx_bind_group_layout, &self.simple_fx_sampler, &self.simple_fx_uniform_buf,
                    source, 14.0,
                    eff.p1.unwrap_or(0.55), 0.0, eff.p3.unwrap_or(1.4), eff.p4.unwrap_or(0.25),
                    self.fx_time, self.width as f32, self.height as f32, 0.0, 0.0, self.width as f32, self.height as f32,
                    0.0, 0.0, 0.0, 0.0, &self.bloom_extract_view,
                );
                blur_pass(
                    &self.device, &self.queue, &self.blur_pipeline, &self.blur_bind_group_layout, &self.blur_sampler, &self.blur_uniform_buf,
                    &self.bloom_extract_view, eff.p2.unwrap_or(28.0) * z, self.width, self.height, &self.blur_scratch_view, &self.blur_result_view,
                );
                composite_pass(
                    &self.device, &self.queue, &self.blend_pipeline, &self.blend_bind_group_layout, &self.blend_sampler, &self.blend_uniform_buf,
                    source, &self.blur_result_view, 2, target,
                );
            }
            "sepia" | "invert" | "grayscale" | "posterize" | "pixelate" | "chromaticAberration" | "scanlines" | "grain" | "sharpen" | "edgeDetect" | "groundShadow" | "contourBrut" | "threshold" | "halftone" => {
                let effect_id = match eff.effect_type.as_str() {
                    "sepia" => 0.0,
                    "invert" => 1.0,
                    "grayscale" => 2.0,
                    "posterize" => 3.0,
                    "pixelate" => 4.0,
                    "chromaticAberration" => 5.0,
                    "scanlines" => 6.0,
                    "grain" => 7.0,
                    "sharpen" => 8.0,
                    "edgeDetect" => 9.0,
                    "groundShadow" => 10.0,
                    "contourBrut" => 11.0,
                    "threshold" => 12.0,
                    _ => 13.0, // "halftone"
                };
                let default_p1 = match eff.effect_type.as_str() {
                    "posterize" => 6.0,
                    "pixelate" => 16.0,
                    "chromaticAberration" => 4.0,
                    "scanlines" => 240.0,
                    "grain" => 0.08,
                    "sharpen" => 0.5,
                    "edgeDetect" => 4.0,
                    "groundShadow" => 0.0,
                    "contourBrut" => 3.0,
                    "threshold" => 0.5,
                    "halftone" => 10.0,
                    _ => 0.0,
                };
                let default_p2 = match eff.effect_type.as_str() {
                    "scanlines" => 0.5,
                    // kept in sync with EFFECT_DEFAULTS.groundShadow in
                    // effects-panel.js — ground=75%/length=1 left the
                    // shadow's reachable source band far from a typical
                    // centered shape, making it look broken by default.
                    "groundShadow" => 0.62,
                    "contourBrut" => 0.4,
                    "threshold" => 0.08,
                    "halftone" => 0.9,
                    _ => 0.0,
                };
                let default_p3 = if eff.effect_type == "groundShadow" { 0.6 } else { 0.0 };
                let default_p4 = match eff.effect_type.as_str() {
                    "groundShadow" => 0.65,
                    "contourBrut" => 0.9,
                    _ => 0.0,
                };
                {
                    // p1 is a pixel SIZE (block/offset/thickness/cell) for
                    // these four — everyone else's p1 is already a unit-less
                    // ratio/level/intensity that shouldn't scale with zoom.
                    let mut p1 = eff.p1.unwrap_or(default_p1);
                    if matches!(eff.effect_type.as_str(), "pixelate" | "chromaticAberration" | "contourBrut" | "halftone") {
                        p1 *= z;
                    }
                    simple_fx_pass(
                        &self.device, &self.queue, &self.simple_fx_pipeline, &self.simple_fx_bind_group_layout, &self.simple_fx_sampler, &self.simple_fx_uniform_buf,
                        source, effect_id,
                        p1, eff.p2.unwrap_or(default_p2), eff.p3.unwrap_or(default_p3), eff.p4.unwrap_or(default_p4),
                        self.fx_time, self.width as f32, self.height as f32, bbox.0, bbox.1, bbox.2, bbox.3,
                        0.0, 0.0, 0.0, 0.0, target,
                    );
                }
            }
            _ if eff.effect_type.starts_with("custom:") => {
                // User-authored WGSL effect (register_custom_effect) — same
                // simple_fx_pass call shape as every built-in simple_fx
                // effect, just with a dynamically-looked-up pipeline
                // instead of self.simple_fx_pipeline. If the pipeline isn't
                // registered yet (e.g. a project was just loaded and JS
                // hasn't re-called register_custom_effect for its saved
                // custom effects this session yet), this is a no-op —
                // `target` is left whatever it already held. JS is expected
                // to register every custom effect immediately on load,
                // before the first render(), so this window is effectively
                // never observed in practice.
                if let Some(pipeline) = self.custom_effect_pipelines.get(&eff.effect_type) {
                    simple_fx_pass(
                        &self.device, &self.queue, pipeline, &self.simple_fx_bind_group_layout, &self.simple_fx_sampler, &self.simple_fx_uniform_buf,
                        source, 0.0,
                        eff.p1.unwrap_or(0.0), eff.p2.unwrap_or(0.0), eff.p3.unwrap_or(0.0), eff.p4.unwrap_or(0.0),
                        self.fx_time, self.width as f32, self.height as f32, bbox.0, bbox.1, bbox.2, bbox.3,
                        eff.p5.unwrap_or(0.0), eff.p6.unwrap_or(0.0), eff.p7.unwrap_or(0.0), eff.p8.unwrap_or(0.0), target,
                    );
                }
            }
            _ => {
                // Default/"blur" — reuses blur_pass verbatim.
                blur_pass(
                    &self.device, &self.queue, &self.blur_pipeline, &self.blur_bind_group_layout, &self.blur_sampler, &self.blur_uniform_buf,
                    source, eff.p1.unwrap_or(0.0) * z, self.width, self.height, &self.blur_scratch_view, target,
                );
            }
        }
    }

    /// Runs every ENABLED entry in `effects`, in order, on `initial_source`,
    /// ping-ponging between effect_stack_a_view/effect_stack_b_view.
    /// Returns true if the final result landed in effect_stack_a_view,
    /// false if effect_stack_b_view — callers re-point their own
    /// `source_view`/`target_view` locals accordingly (mirrors the existing
    /// "source_view = &self.blur_result_view" convention elsewhere in this
    /// file). Caller must check `effects.iter().any(|e| e.enabled)` first
    /// and skip calling this entirely when false (cheap early-exit, same
    /// "0 = disabled, no GPU cost" convention the old single-field version
    /// had via 0-valued radius/opacity) — this function assumes at least
    /// one enabled entry exists.
    fn apply_effect_stack(&self, initial_source: &wgpu::TextureView, effects: &[EffectIn], bbox: (f32, f32, f32, f32)) -> bool {
        let mut current: &wgpu::TextureView = initial_source;
        let mut use_a = true;
        for eff in effects.iter().filter(|e| e.enabled) {
            let target: &wgpu::TextureView = if use_a { &self.effect_stack_a_view } else { &self.effect_stack_b_view };
            self.run_one_effect(eff, current, target, bbox);
            current = target;
            use_a = !use_a;
        }
        !use_a
    }

    /// Adjustment-layer variant of `apply_effect_stack`: intermediate
    /// effects still ping-pong through the private stack textures, but the
    /// final effect writes straight into the caller's next accumulator.
    /// This removes the old full-frame texture copy and its extra queue
    /// submission for every enabled adjustment layer without changing pass
    /// order or shader inputs.
    fn apply_effect_stack_into(
        &self,
        initial_source: &wgpu::TextureView,
        effects: &[EffectIn],
        bbox: (f32, f32, f32, f32),
        final_target: &wgpu::TextureView,
    ) {
        let enabled_count = effects.iter().filter(|e| e.enabled).count();
        debug_assert!(enabled_count > 0);
        let mut current = initial_source;
        let mut use_a = true;
        let mut rendered = 0usize;
        for eff in effects.iter().filter(|e| e.enabled) {
            rendered += 1;
            let target = if rendered == enabled_count {
                final_target
            } else if use_a {
                &self.effect_stack_a_view
            } else {
                &self.effect_stack_b_view
            };
            self.run_one_effect(eff, current, target, bbox);
            current = target;
            use_a = !use_a;
        }
    }

    /// Union on-screen (device-pixel) bounding box of `items` after
    /// `view_tf` — mirrors paint_layer_items' own per-item geometry
    /// resolution (path_ref lookup / build_bezpath, item_tf construction,
    /// placed image rect) so the box matches exactly what actually got
    /// painted. Falls back to the full canvas when no item resolves to real
    /// geometry (matches every effect's pre-existing canvas-wide behavior
    /// for that edge case — an empty/unresolvable set of items is not
    /// something a distortion effect can meaningfully center on anyway).
    fn items_bbox_px(&self, items: &[ItemIn], view_tf: Affine) -> (f32, f32, f32, f32) {
        let mut acc: Option<Rect> = None;
        for item in items {
            let item_rect = if let Some(img_ref) = &item.image {
                let rect = Rect::new(img_ref.x, img_ref.y, img_ref.x + img_ref.width, img_ref.y + img_ref.height);
                let place = if img_ref.rotation != 0.0 {
                    let (cx, cy) = (img_ref.x + img_ref.width / 2.0, img_ref.y + img_ref.height / 2.0);
                    Affine::translate((cx, cy)) * Affine::rotate(img_ref.rotation.to_radians()) * Affine::translate((-cx, -cy))
                } else {
                    Affine::IDENTITY
                };
                Some((view_tf * place).transform_rect_bbox(rect))
            } else {
                let built: BezPath;
                let bez: &BezPath = if let Some(r) = &item.path_ref {
                    match self.paths.get(r) {
                        Some(p) => p,
                        None => continue,
                    }
                } else {
                    match build_bezpath(item) {
                        Some(b) => {
                            built = b;
                            &built
                        }
                        None => continue,
                    }
                };
                let item_tf = match &item.path_transform {
                    Some(m) => view_tf * Affine::new(*m),
                    None => view_tf,
                };
                Some(item_tf.transform_rect_bbox(bez.bounding_box()))
            };
            if let Some(r) = item_rect {
                acc = Some(match acc {
                    Some(a) => a.union(r),
                    None => r,
                });
            }
        }
        match acc {
            Some(r) if r.width() > 0.5 && r.height() > 0.5 => (r.x0 as f32, r.y0 as f32, r.width() as f32, r.height() as f32),
            _ => (0.0, 0.0, self.width as f32, self.height as f32),
        }
    }

    /// Paints one layer whose items include at least one with its OWN
    /// effects stack (2026-07, "possible de différencié les effet par
    /// éléments sélectionné") — leaves the fully-composited result in
    /// self.blend_layer_view, the exact same contract the plain
    /// paint_layer_items+render_to_texture call this replaces already had,
    /// so the rest of composite_scene (matte, layer-level effects stack,
    /// blend composite) stays completely unaware anything special happened.
    ///
    /// Splits the layer's items into ordered RUNS — a run is either a
    /// stretch of consecutive plain items, or a single item that has its
    /// own effects — and paints/composites each run in turn onto
    /// element_build_a/b (ping-ponging), preserving the layer's original
    /// paint order exactly as if every item had painted into one ordinary
    /// Scene. A plain run is rendered straight into blend_layer_view
    /// (reused here as scratch — safe because apply_effect_stack below
    /// only reads it as a one-shot `source`, never after starting its own
    /// separate effect_stack_a/b ping-pong) and composited unchanged; an
    /// effected item is ALSO rendered alone into blend_layer_view first,
    /// then run through apply_effect_stack (that function's own ping-pong,
    /// effect_stack_a/b — distinct textures, no aliasing with
    /// element_build_a/b) before being composited.
    fn paint_layer_with_element_effects(&mut self, layer: &LayerIn, view_tf: Affine) -> Result<(), JsValue> {
        let layer_params = RenderParams { base_color: Color::TRANSPARENT, width: self.width, height: self.height, antialiasing_method: AaConfig::Area };
        let n = layer.items.len();
        clear_texture(&self.device, &self.queue, &self.element_build_a_view, wgpu::Color::TRANSPARENT);
        let mut accum_is_a = true;
        let mut first_run = true;
        let mut i = 0;
        while i < n {
            let has_fx = layer.items[i].effects.iter().any(|e| e.enabled);
            let end = if has_fx {
                i + 1
            } else {
                let mut j = i + 1;
                while j < n && !layer.items[j].effects.iter().any(|e| e.enabled) {
                    j += 1;
                }
                j
            };
            let mut scene = Scene::new();
            self.push_atlas_keepalive(&mut scene);
            paint_layer_items(&mut scene, &layer.items[i..end], view_tf, &self.images, &self.paths);
            self.renderer
                .render_to_texture(&self.device, &self.queue, &scene, &self.blend_layer_view, &layer_params)
                .map_err(|e| JsValue::from_str(&format!("render_to_texture failed: {e:?}")))?;
            let (run_source, run_source_tex): (&wgpu::TextureView, &wgpu::Texture) = if has_fx {
                let bbox = self.items_bbox_px(&layer.items[i..i + 1], view_tf);
                let in_a = self.apply_effect_stack(&self.blend_layer_view, &layer.items[i].effects, bbox);
                if in_a { (&self.effect_stack_a_view, &self.effect_stack_a_tex) } else { (&self.effect_stack_b_view, &self.effect_stack_b_tex) }
            } else {
                (&self.blend_layer_view, &self.blend_layer_tex)
            };
            if first_run {
                // First run: nothing to composite ONTO yet — element_build_a
                // is already cleared to transparent above, so just copy
                // this run's own result in as the starting accumulator.
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("element-fx-first-run-copy") });
                let dst_tex = if accum_is_a { &self.element_build_a_tex } else { &self.element_build_b_tex };
                encoder.copy_texture_to_texture(
                    run_source_tex.as_image_copy(),
                    dst_tex.as_image_copy(),
                    wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
                );
                self.queue.submit(Some(encoder.finish()));
                first_run = false;
            } else {
                let (backdrop_view, target_view) =
                    if accum_is_a { (&self.element_build_a_view, &self.element_build_b_view) } else { (&self.element_build_b_view, &self.element_build_a_view) };
                composite_pass(
                    &self.device, &self.queue, &self.blend_pipeline, &self.blend_bind_group_layout, &self.blend_sampler, &self.blend_uniform_buf,
                    backdrop_view, run_source, 0, target_view,
                );
                accum_is_a = !accum_is_a;
            }
            i = end;
        }
        let final_tex = if accum_is_a { &self.element_build_a_tex } else { &self.element_build_b_tex };
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("element-fx-final-copy") });
        encoder.copy_texture_to_texture(
            final_tex.as_image_copy(),
            self.blend_layer_tex.as_image_copy(),
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn composite_scene(&mut self, scene_in: &SceneIn, view_tf: Affine, base_color: Color) -> Result<(), JsValue> {
        let n = scene_in.layers.len();
        let mut is_matte_source = vec![false; n];
        for i in 0..n {
            // resolve_matte_source is the single source of truth for which
            // layer gets consumed — uid-resolved index or legacy i+1, same
            // answer the paint-time lookup below will compute.
            if let Some(s) = resolve_matte_source(&scene_in.layers, i) {
                is_matte_source[s] = true;
            }
        }
        let has_matte = is_matte_source.iter().any(|&b| b);
        let has_blend = scene_in.layers.iter().any(|l| mix_mode_index(l.blend_mode.as_deref()) != 0);
        let has_effects_stack = scene_in.layers.iter().any(|l| {
            l.effects.iter().any(|e| e.enabled) || l.items.iter().any(|it| it.effects.iter().any(|e| e.enabled))
        });
        let has_effect = scene_in.layers.iter().any(|l| l.is_effect_layer.unwrap_or(false));
        let has_mask = scene_in.layers.iter().any(|l| !l.masks.is_empty());
        if !has_blend && !has_matte && !has_effects_stack && !has_effect && !has_mask {
            let mut scene = Scene::new();
            self.push_atlas_keepalive(&mut scene);
            for layer in &scene_in.layers {
                paint_layer_items(&mut scene, &layer.items, view_tf, &self.images, &self.paths);
            }
            let params = RenderParams { base_color, width: self.width, height: self.height, antialiasing_method: AaConfig::Area };
            self.renderer
                .render_to_texture(&self.device, &self.queue, &scene, &self.offscreen_view, &params)
                .map_err(|e| JsValue::from_str(&format!("render_to_texture failed: {e:?}")))?;
            return Ok(());
        }

        clear_texture(&self.device, &self.queue, &self.blend_accum_a_view, wgpu_color_from(base_color));
        let mut accum_is_a = true;
        let layer_params = RenderParams { base_color: Color::TRANSPARENT, width: self.width, height: self.height, antialiasing_method: AaConfig::Area };
        for (i, layer) in scene_in.layers.iter().enumerate() {
            // Consumed as a matte source by the layer below it — doesn't
            // paint as its own visible layer (AE convention).
            if is_matte_source[i] {
                continue;
            }
            // Effect (adjustment) layer — unlike every other branch here,
            // this one never paints its own items at all: it reads the
            // RUNNING ACCUMULATOR (everything composited so far, i.e.
            // "everything below" since layers are processed bottom-up) as
            // its source and writes the graded/blurred result back as the
            // new accumulator state, exactly like flipping accum_is_a for
            // an ordinary layer — just with a color/blur pass instead of
            // paint_layer_items+composite_pass in between.
            if layer.is_effect_layer.unwrap_or(false) {
                let backdrop_view = if accum_is_a { &self.blend_accum_a_view } else { &self.blend_accum_b_view };
                if layer.effects.iter().any(|e| e.enabled) {
                    // Full-document bbox, deliberately NOT items_bbox_px: an
                    // adjustment/effect layer has no shape of its own (its
                    // own `items` are ignored entirely, is_effect_layer's
                    // whole point) — it grades/distorts everything already
                    // composited below it, so "my own content" IS the whole
                    // document, same as a real adjustment layer. The render
                    // target itself is only the editor viewport and is not a
                    // stable procedural coordinate system under zoom/pan.
                    let bbox = document_bbox_px(&self.viewport, self.width, self.height);
                    let target_view = if accum_is_a { &self.blend_accum_b_view } else { &self.blend_accum_a_view };
                    self.apply_effect_stack_into(backdrop_view, &layer.effects, bbox, target_view);
                } else {
                    // No enabled effects on this adjustment layer — pass
                    // the backdrop through unchanged (matches AE: a layer
                    // with every effect disabled/deleted is a no-op).
                    let backdrop_tex = if accum_is_a { &self.blend_accum_a } else { &self.blend_accum_b };
                    let target_tex = if accum_is_a { &self.blend_accum_b } else { &self.blend_accum_a };
                    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("effect-stack-passthrough-copy") });
                    encoder.copy_texture_to_texture(
                        backdrop_tex.as_image_copy(),
                        target_tex.as_image_copy(),
                        wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
                    );
                    self.queue.submit(Some(encoder.finish()));
                }
                accum_is_a = !accum_is_a;
                continue;
            }
            if layer.items.iter().any(|it| it.effects.iter().any(|e| e.enabled)) {
                self.paint_layer_with_element_effects(layer, view_tf)?;
            } else {
                let mut scene = Scene::new();
                self.push_atlas_keepalive(&mut scene);
                paint_layer_items(&mut scene, &layer.items, view_tf, &self.images, &self.paths);
                self.renderer
                    .render_to_texture(&self.device, &self.queue, &scene, &self.blend_layer_view, &layer_params)
                    .map_err(|e| JsValue::from_str(&format!("render_to_texture failed: {e:?}")))?;
            }

            // Vector masks (2026-08) — combine this layer's own masks (if
            // any) into one silhouette and multiply it into the layer's
            // own just-rendered content, BEFORE matte (order doesn't
            // matter between the two — both are plain alpha multiplies,
            // see matte_pass — but masks need to run first regardless
            // since the matte block below reads `masked_view` as ITS
            // input instead of the layer's raw paint). See LayerIn::masks'
            // doc comment for the combine algorithm and the v1
            // shared-feather simplification.
            let masked_view: &wgpu::TextureView = if !layer.masks.is_empty() {
                let add_items: Vec<ItemIn> =
                    layer.masks.iter().filter(|m| m.mode != "subtract" && m.mode != "intersect").map(|m| m.item.clone()).collect();
                if add_items.is_empty() {
                    // No Add-mode mask at all (only Subtract/Intersect) —
                    // start from "everything visible" so a lone Intersect
                    // shows just that shape and a lone Subtract punches a
                    // hole in a fully-visible layer, both more intuitive
                    // than AE's literal top-to-bottom accumulation (see the
                    // mask-feature audit's deliberate-deviation note).
                    clear_texture(&self.device, &self.queue, &self.mask_accum_a_view, wgpu::Color::WHITE);
                } else {
                    // Union: every Add mask painted white into ONE Scene —
                    // ordinary src-over of opaque-white-on-opaque-white
                    // stays opaque white, so overlapping Add shapes union
                    // for free with zero extra compose passes.
                    let mut mscene = Scene::new();
                    self.push_atlas_keepalive(&mut mscene);
                    paint_layer_items(&mut mscene, &add_items, view_tf, &self.images, &self.paths);
                    self.renderer
                        .render_to_texture(&self.device, &self.queue, &mscene, &self.mask_scratch_view, &layer_params)
                        .map_err(|e| JsValue::from_str(&format!("render_to_texture failed: {e:?}")))?;
                    // mask_scratch is STORAGE_BINDING (vello writes it);
                    // mask_accum_a is RENDER_ATTACHMENT (matte_pass writes
                    // it) — a plain texture copy bridges the two so the
                    // Subtract/Intersect loop below always reads/writes the
                    // same RENDER_ATTACHMENT-shaped pair, matching the
                    // established blend_accum_a/b idiom.
                    let mut encoder =
                        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("mask-add-to-accum-copy") });
                    encoder.copy_texture_to_texture(
                        self.mask_scratch_tex.as_image_copy(),
                        self.mask_accum_a_tex.as_image_copy(),
                        wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
                    );
                    self.queue.submit(Some(encoder.finish()));
                }
                let mut accum_is_a = true;
                for m in layer.masks.iter().filter(|m| m.mode == "subtract" || m.mode == "intersect") {
                    let mut sscene = Scene::new();
                    self.push_atlas_keepalive(&mut sscene);
                    paint_layer_items(&mut sscene, std::slice::from_ref(&m.item), view_tf, &self.images, &self.paths);
                    self.renderer
                        .render_to_texture(&self.device, &self.queue, &sscene, &self.mask_scratch_view, &layer_params)
                        .map_err(|e| JsValue::from_str(&format!("render_to_texture failed: {e:?}")))?;
                    let (cur_view, next_view) =
                        if accum_is_a { (&self.mask_accum_a_view, &self.mask_accum_b_view) } else { (&self.mask_accum_b_view, &self.mask_accum_a_view) };
                    // Subtract = matte_pass with invert=true (alpha *=
                    // 1-this); Intersect = invert=false (alpha *= this) —
                    // matte_pass's own alpha-matte formula IS the combine
                    // math a mask compose needs, no new WGSL required.
                    matte_pass(
                        &self.device,
                        &self.queue,
                        &self.matte_pipeline,
                        &self.matte_bind_group_layout,
                        &self.matte_sampler,
                        &self.matte_uniform_buf,
                        cur_view,
                        &self.mask_scratch_view,
                        0,
                        m.mode == "subtract",
                        next_view,
                    );
                    accum_is_a = !accum_is_a;
                }
                let combined_view: &wgpu::TextureView = if accum_is_a { &self.mask_accum_a_view } else { &self.mask_accum_b_view };
                // Feather (2026-08, AE parity — the actual ask this
                // shipped for): softens the FINAL combined silhouette's
                // edge via the exact same separable Gaussian blur_pass
                // already used for the Effects panel's own Blur — a true
                // edge-normal alpha falloff on the mask itself, not a
                // whole-layer image blur (which would soften the masked
                // CONTENT instead of just the clip boundary).
                let feather = layer.mask_feather as f32;
                let silhouette_view: &wgpu::TextureView = if feather > 0.01 {
                    blur_pass(
                        &self.device,
                        &self.queue,
                        &self.blur_pipeline,
                        &self.blur_bind_group_layout,
                        &self.blur_sampler,
                        &self.blur_uniform_buf,
                        combined_view,
                        feather,
                        self.width,
                        self.height,
                        &self.blur_scratch_view,
                        &self.blur_result_view,
                    );
                    &self.blur_result_view
                } else {
                    combined_view
                };
                matte_pass(
                    &self.device,
                    &self.queue,
                    &self.matte_pipeline,
                    &self.matte_bind_group_layout,
                    &self.matte_sampler,
                    &self.matte_uniform_buf,
                    &self.blend_layer_view,
                    silhouette_view,
                    0,
                    false,
                    &self.mask_applied_view,
                );
                &self.mask_applied_view
            } else {
                &self.blend_layer_view
            };
            // Both halves resolved through the SAME helper as the precompute
            // — a matte whose source doesn't resolve (dangling uid, index
            // out of range) degrades to "no matte" instead of masking
            // against the wrong layer.
            let matte_src = resolve_matte_source(&scene_in.layers, i);
            let mut source_view: &wgpu::TextureView = if let (Some((mode, invert)), Some(ms)) = (matte_mode_of(layer.matte_mode.as_deref()), matte_src) {
                let mut matte_scene = Scene::new();
                self.push_atlas_keepalive(&mut matte_scene);
                paint_layer_items(&mut matte_scene, &scene_in.layers[ms].items, view_tf, &self.images, &self.paths);
                self.renderer
                    .render_to_texture(&self.device, &self.queue, &matte_scene, &self.matte_source_view, &layer_params)
                    .map_err(|e| JsValue::from_str(&format!("render_to_texture failed: {e:?}")))?;
                matte_pass(
                    &self.device,
                    &self.queue,
                    &self.matte_pipeline,
                    &self.matte_bind_group_layout,
                    &self.matte_sampler,
                    &self.matte_uniform_buf,
                    masked_view,
                    &self.matte_source_view,
                    mode,
                    invert,
                    &self.matte_result_view,
                );
                &self.matte_result_view
            } else {
                masked_view
            };
            // Effects stack (2026-07 rewrite) — runs on THIS layer's own
            // isolated alpha (real transparency), AFTER matte (so a matted
            // layer's edge softens/shadows too, not just its raw content)
            // and BEFORE the blend/composite pass, so an effect's output
            // participates correctly in whatever blend mode this layer has.
            // See LayerIn::effects' doc comment for why this differs from
            // the accumulator an effect/adjustment layer's stack runs on.
            if layer.effects.iter().any(|e| e.enabled) {
                let bbox = self.items_bbox_px(&layer.items, view_tf);
                let in_a = self.apply_effect_stack(source_view, &layer.effects, bbox);
                source_view = if in_a { &self.effect_stack_a_view } else { &self.effect_stack_b_view };
            }

            let mode = mix_mode_index(layer.blend_mode.as_deref());
            let (backdrop_view, target_view) =
                if accum_is_a { (&self.blend_accum_a_view, &self.blend_accum_b_view) } else { (&self.blend_accum_b_view, &self.blend_accum_a_view) };
            composite_pass(
                &self.device,
                &self.queue,
                &self.blend_pipeline,
                &self.blend_bind_group_layout,
                &self.blend_sampler,
                &self.blend_uniform_buf,
                backdrop_view,
                source_view,
                mode,
                target_view,
            );
            accum_is_a = !accum_is_a;
        }
        let final_tex = if accum_is_a { &self.blend_accum_a } else { &self.blend_accum_b };
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("blend-final-copy") });
        encoder.copy_texture_to_texture(
            final_tex.as_image_copy(),
            self.offscreen.as_image_copy(),
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }
}

#[wasm_bindgen]
impl VelloEngine {
    /// `rotation` in radians, pivoting around `(pivot_x, pivot_y)` — pass
    /// the artboard center (e.g. canvasW/2, canvasH/2) to match Animate's
    /// Rotate Stage tool; pass (0,0) for a plain top-left-anchored zoom/pan.
    pub fn set_viewport(&mut self, pan_x: f64, pan_y: f64, zoom: f64, rotation: f64, pivot_x: f64, pivot_y: f64, effect_zoom: f64) {
        // A zero/negative zoom (a stray or buggy JS-side value — this is
        // caller-controlled, not otherwise validated) makes Affine::scale
        // singular; screen_to_world's inverse() then yields inf/NaN that
        // silently poisons all hit-testing/coordinate math afterward
        // instead of failing loudly. Same floor already used by
        // gizmo_handles for the same reason (see its own zoom.max call).
        self.viewport = Viewport {
            pan_x, pan_y,
            zoom: zoom.max(0.0001),
            effect_zoom: effect_zoom.max(0.0001),
            rotation, pivot_x, pivot_y,
        };
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
            // Keep in sync with create_engine's own offscreen descriptor —
            // COPY_DST is needed by composite_scene's blend path (blend.wgsl)
            // for its final copy into this texture.
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.offscreen_view = self.offscreen.create_view(&wgpu::TextureViewDescriptor::default());
        let (blend_layer_tex, blend_layer_view) = create_blend_layer_texture(&self.device, width, height);
        self.blend_layer_tex = blend_layer_tex;
        self.blend_layer_view = blend_layer_view;
        let (blend_accum_a, blend_accum_a_view) = create_blend_accum_texture(&self.device, width, height, "blend-accum-a");
        self.blend_accum_a = blend_accum_a;
        self.blend_accum_a_view = blend_accum_a_view;
        let (blend_accum_b, blend_accum_b_view) = create_blend_accum_texture(&self.device, width, height, "blend-accum-b");
        self.blend_accum_b = blend_accum_b;
        self.blend_accum_b_view = blend_accum_b_view;
        let (matte_source_tex, matte_source_view) = create_blend_layer_texture(&self.device, width, height);
        self.matte_source_tex = matte_source_tex;
        self.matte_source_view = matte_source_view;
        let (matte_result_tex, matte_result_view) = create_matte_result_texture(&self.device, width, height);
        self.matte_result_tex = matte_result_tex;
        self.matte_result_view = matte_result_view;
        let (mask_scratch_tex, mask_scratch_view) = create_blend_layer_texture(&self.device, width, height);
        self.mask_scratch_tex = mask_scratch_tex;
        self.mask_scratch_view = mask_scratch_view;
        let (mask_accum_a_tex, mask_accum_a_view) = create_matte_result_texture(&self.device, width, height);
        self.mask_accum_a_tex = mask_accum_a_tex;
        self.mask_accum_a_view = mask_accum_a_view;
        let (mask_accum_b_tex, mask_accum_b_view) = create_matte_result_texture(&self.device, width, height);
        self.mask_accum_b_tex = mask_accum_b_tex;
        self.mask_accum_b_view = mask_accum_b_view;
        let (mask_applied_tex, mask_applied_view) = create_matte_result_texture(&self.device, width, height);
        self.mask_applied_tex = mask_applied_tex;
        self.mask_applied_view = mask_applied_view;
        let (blur_result_tex, blur_result_view) = create_matte_result_texture(&self.device, width, height);
        self.blur_result_tex = blur_result_tex;
        self.blur_result_view = blur_result_view;
        let (blur_scratch_tex, blur_scratch_view) = create_matte_result_texture(&self.device, width, height);
        self.blur_scratch_tex = blur_scratch_tex;
        self.blur_scratch_view = blur_scratch_view;
        let (bloom_extract_tex, bloom_extract_view) = create_matte_result_texture(&self.device, width, height);
        self.bloom_extract_tex = bloom_extract_tex;
        self.bloom_extract_view = bloom_extract_view;
        let (effect_stack_a_tex, effect_stack_a_view) = create_blend_accum_texture(&self.device, width, height, "effect-stack-a");
        self.effect_stack_a_tex = effect_stack_a_tex;
        self.effect_stack_a_view = effect_stack_a_view;
        let (effect_stack_b_tex, effect_stack_b_view) = create_blend_accum_texture(&self.device, width, height, "effect-stack-b");
        self.effect_stack_b_tex = effect_stack_b_tex;
        self.effect_stack_b_view = effect_stack_b_view;
        let (element_build_a_tex, element_build_a_view) = create_blend_accum_texture(&self.device, width, height, "element-build-a");
        self.element_build_a_tex = element_build_a_tex;
        self.element_build_a_view = element_build_a_view;
        let (element_build_b_tex, element_build_b_view) = create_blend_accum_texture(&self.device, width, height, "element-build-b");
        self.element_build_b_tex = element_build_b_tex;
        self.element_build_b_view = element_build_b_view;
    }

    /// Registers (or re-registers, if `key` already exists — e.g. the
    /// author just edited the source) a user-authored custom WGSL effect
    /// (2026-07, feedback: "la possibilité d'ajouter ses propres effets
    /// wgsl et leur paramètre correspondant") — `key` is a JS-chosen stable
    /// id (e.g. "custom:<uuid>") later used as an EffectIn.effect_type,
    /// `fs_body` is ONLY the body of the fragment shader (a sequence of
    /// WGSL statements ending in `return vec4<f32>(...)`), wrapped here
    /// into a full document that already declares the standard fullscreen-
    /// triangle vertex shader, the texture/sampler/Params bindings, and
    /// six convenience locals every author can use without re-deriving
    /// them: `uv` (0..1 across the FULL CANVAS), `src` (the pixel already
    /// sampled at `uv`), `texel` (1 texel in UV units, for neighbor-
    /// sampling effects), and — 2026-07-30, see run_one_effect's own doc
    /// comment for the bug this fixes — `bbox_o`/`bbox_s` (the on-screen
    /// device-pixel origin/size of whatever this effect is actually
    /// attached to) and `local_uv` (0..1 across just THAT bbox instead of
    /// the whole canvas, can go outside 0..1 near/past its edges same as
    /// `uv` already can). Any effect with a "center of my own shape"
    /// concept (a twirl/bulge pivot, a wave's phase, a particle grid)
    /// should distort in `local_uv` space and map back to real texture
    /// coordinates via `bbox_o + result * bbox_s` (in device px) before
    /// dividing by `vec2(tex_w, tex_h)` for the final textureSample — NOT
    /// `uv`/`vec2(0.5)` directly, which is the canvas center, not the
    /// shape's — confirmed live: a shipped Twirl effect's pattern visibly
    /// changed under pure panning (zero zoom change) before this existed,
    /// which only makes sense if its reference frame was the viewport.
    /// Same `Params{effect_id,p1,p2,p3,tex_w,tex_h,time,p4,bbox_x,bbox_y,
    /// bbox_w,bbox_h}` layout as simple_fx.wgsl, so an author's
    /// `params.p1`..`params.p4` map 1:1 onto the SAME p1..p4 fields the
    /// stack UI's generic param editor already writes for every other
    /// effect type — no separate wiring needed on the JS side for a custom
    /// effect's parameters.
    ///
    /// Compiling arbitrary author-supplied WGSL at runtime is safe here:
    /// this crate only ever targets the web/WebGPU wgpu backend (built via
    /// `wasm-pack build --target web`), where shader compilation is the
    /// BROWSER's own WebGPU implementation doing the work — invalid WGSL
    /// produces a normal asynchronous validation error via the browser's
    /// uncaptured-error mechanism (the SAME "wgpu uncaptured error" console
    /// messages every other shader bug in this file already produces),
    /// never a Rust panic or a corrupted wasm instance.
    pub fn register_custom_effect(&mut self, key: String, fs_body: String) -> Result<(), JsValue> {
        let source = format!(
            "struct VsOut {{\n    @builtin(position) pos: vec4<f32>,\n    @location(0) uv: vec2<f32>,\n}};\n\n@vertex\nfn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {{\n    var positions = array<vec2<f32>, 3>(\n        vec2<f32>(-1.0, -1.0),\n        vec2<f32>(3.0, -1.0),\n        vec2<f32>(-1.0, 3.0),\n    );\n    let p = positions[vid];\n    var out: VsOut;\n    out.pos = vec4<f32>(p, 0.0, 1.0);\n    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 1.0 - (p.y * 0.5 + 0.5));\n    return out;\n}}\n\n@group(0) @binding(0) var src_tex: texture_2d<f32>;\n@group(0) @binding(1) var tex_sampler: sampler;\n\nstruct Params {{\n    effect_id: f32,\n    p1: f32,\n    p2: f32,\n    p3: f32,\n    tex_w: f32,\n    tex_h: f32,\n    time: f32,\n    p4: f32,\n    bbox_x: f32,\n    bbox_y: f32,\n    bbox_w: f32,\n    bbox_h: f32,\n    p5: f32,\n    p6: f32,\n    p7: f32,\n    p8: f32,\n}};\n@group(0) @binding(2) var<uniform> params: Params;\n\n@fragment\nfn fs_main(in: VsOut) -> @location(0) vec4<f32> {{\n    let uv = in.uv;\n    let texel = vec2<f32>(1.0 / max(params.tex_w, 1.0), 1.0 / max(params.tex_h, 1.0));\n    let src = textureSample(src_tex, tex_sampler, uv);\n    let bbox_o = vec2<f32>(params.bbox_x, params.bbox_y);\n    let bbox_s = vec2<f32>(max(params.bbox_w, 1.0), max(params.bbox_h, 1.0));\n    let local_uv = (uv * vec2<f32>(params.tex_w, params.tex_h) - bbox_o) / bbox_s;\n{fs_body}\n}}\n"
        );
        let pipeline = create_custom_effect_pipeline(&self.device, &self.simple_fx_bind_group_layout, &source);
        self.custom_effect_pipelines.insert(key, pipeline);
        Ok(())
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
    /// already registered (presence check — the store is now bounded and JS
    /// may have retired this id, so a `false` here means "upload again").
    pub fn has_image(&self, id: &str) -> bool {
        self.images.contains_key(id)
    }

    /// Total decoded bytes held by the image store. This used to be unbounded
    /// by design ("cached for the engine's whole lifetime"), which is fine for
    /// a handful of imported rasters and untenable for footage: a 1000-frame
    /// 1920x1080 sequence is 8.3GB of RGBA8. JS drives eviction (it is the
    /// side that knows what the CURRENT scene references and can re-upload
    /// from the Paper Raster / video bridge on demand) — this just reports.
    pub fn image_store_bytes(&self) -> f64 {
        self.images.values().map(|d| (d.width as f64) * (d.height as f64) * 4.0).sum()
    }
    pub fn image_store_size(&self) -> u32 {
        self.images.len() as u32
    }

    /// Drops images by id. Mirrors retire_paths. Never called for an id the
    /// scene being rendered still references — the caller checks that, because
    /// dropping a live id would make the picture lose an image with no signal
    /// beyond a warning in paint_layer_items.
    pub fn retire_images(&mut self, ids_json: &str) -> Result<(), JsValue> {
        let ids: Vec<String> = serde_json::from_str(ids_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        for id in ids {
            self.images.remove(&id);
        }
        Ok(())
    }

    /// Retained path store (see the `paths` field's doc comment). `coords` is
    /// a flat [px,py, hInX,hInY, hOutX,hOutY] × n array — the same
    /// RELATIVE-handle convention as SegIn/serP, 6 slots per segment with
    /// explicit zeros where the JSON form omits a zero handle. Built through
    /// build_bezpath_from_segments so a registered path and an inline one
    /// produce byte-identical curves (single source of truth, §3).
    pub fn register_path(&mut self, id: String, coords: &[f64], closed: bool) {
        let n = coords.len() / 6;
        let mut segs = Vec::with_capacity(n);
        for i in 0..n {
            let o = i * 6;
            segs.push(SegIn {
                point: [coords[o], coords[o + 1]],
                handle_in: [coords[o + 2], coords[o + 3]],
                handle_out: [coords[o + 4], coords[o + 5]],
            });
        }
        self.paths.insert(id, build_bezpath_from_segments(&segs, closed));
    }

    /// Retirement is JS-driven (FinalizationRegistry on the stroke dicts) —
    /// this side never guesses at lifetimes. `ids_json`: JSON array of keys.
    pub fn retire_paths(&mut self, ids_json: &str) -> Result<(), JsValue> {
        let ids: Vec<String> = serde_json::from_str(ids_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        for id in ids {
            self.paths.remove(&id);
        }
        Ok(())
    }

    /// Project-load hygiene (importJSON): every stroke dict is new, so every
    /// stored path is garbage at once — cheaper than waiting for the GC.
    pub fn clear_paths(&mut self) {
        self.paths.clear();
    }

    pub fn path_store_size(&self) -> u32 {
        self.paths.len() as u32
    }

    pub fn render(&mut self, scene_json: &str) -> Result<(), JsValue> {
        let scene_in: SceneIn = serde_json::from_str(scene_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.fx_time = scene_in.time;
        let view_tf = self.viewport.transform();
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
        self.composite_scene(&scene_in, view_tf, Color::from_rgba8(0x20, 0x1f, 0x25, 0xff))?;

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
        self.fx_time = scene_in.time;
        let view_tf = self.viewport.transform();
        self.composite_scene(&scene_in, view_tf, Color::TRANSPARENT)?;

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
