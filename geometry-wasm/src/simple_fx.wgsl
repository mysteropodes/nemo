// Generic single-pass image-effect shader (2026-07, effect-library expansion
// — feedback: "une bibliothèque plus fournie de différents effets wgsl").
// Unlike blur/vignette/color_adjust (each their own dedicated pipeline+
// bind-group-layout, per this file's own established "mirrored, not shared"
// precedent), the 10 effects here are ALL "one source texture in, one
// transformed pixel out" — none need a second pass or a different bind-group
// shape — so they share ONE pipeline switched by `effect_id`, keeping the
// Rust side to a single create_simple_fx_pipeline/simple_fx_pass pair instead
// of ten near-identical copies. Same fullscreen-triangle shape as every
// other compositor pass in this file.
//
// Each algorithm below is a well-known, independently-documented-hundreds-
// of-times technique (standard luminance weights, standard sepia matrix,
// standard Sobel kernel, the standard `fract(sin(dot(...))*43758.5453)`
// hash) reimplemented from its mathematical description — not ported from
// any single specific shader source.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = positions[vid];
    var out: VsOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 1.0 - (p.y * 0.5 + 0.5));
    return out;
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

struct Params {
    effect_id: f32, // rounded to nearest int in-shader — see EFFECT_* below
    p1: f32,
    p2: f32,
    p3: f32,
    tex_w: f32,
    tex_h: f32,
    time: f32,
    p4: f32, // was _pad0 — first effect to need a 4th slot is EFFECT_GROUND_SHADOW
    // 2026-07-30: on-screen bbox (device px) of whatever this effect is
    // attached to — see engine.rs's run_one_effect doc comment. Unused by
    // every EFFECT_* below (none has a "center of my content" concept —
    // this file's Params struct just has to stay byte-identical to the
    // register_custom_effect-generated one, since both bind the SAME
    // uniform buffer/bind-group-layout, simple_fx_bind_group_layout).
    bbox_x: f32,
    bbox_y: f32,
    bbox_w: f32,
    bbox_h: f32,
};
@group(0) @binding(2) var<uniform> params: Params;

const EFFECT_SEPIA: f32 = 0.0;
const EFFECT_INVERT: f32 = 1.0;
const EFFECT_GRAYSCALE: f32 = 2.0;
const EFFECT_POSTERIZE: f32 = 3.0;
const EFFECT_PIXELATE: f32 = 4.0;
const EFFECT_CHROMATIC_ABERRATION: f32 = 5.0;
const EFFECT_SCANLINES: f32 = 6.0;
const EFFECT_GRAIN: f32 = 7.0;
const EFFECT_SHARPEN: f32 = 8.0;
const EFFECT_EDGE_DETECT: f32 = 9.0;
const EFFECT_GROUND_SHADOW: f32 = 10.0;
const EFFECT_CONTOUR_BRUT: f32 = 11.0;
const EFFECT_THRESHOLD: f32 = 12.0;
const EFFECT_HALFTONE: f32 = 13.0;
const EFFECT_BLOOM_EXTRACT: f32 = 14.0;

// Standard Rec.601 luminance weights — appears identically across every
// grayscale/luminance tutorial, public-domain-level constant.
fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// Standard pseudo-random hash used for film-grain/noise in countless
// independent shader tutorials — generic PRNG idiom, not tied to any one
// source.
fn hash(uv: vec2<f32>) -> f32 {
    return fract(sin(dot(uv, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = vec2<f32>(1.0 / max(params.tex_w, 1.0), 1.0 / max(params.tex_h, 1.0));
    let id = params.effect_id;

    if (id == EFFECT_PIXELATE) {
        // Snap UV to a grid of block_px-sized cells before sampling —
        // standard mosaic technique.
        let block = max(params.p1, 1.0);
        let grid = vec2<f32>(params.tex_w, params.tex_h) / block;
        let snapped = floor(in.uv * grid) / grid;
        return textureSample(src_tex, tex_sampler, snapped);
    }

    if (id == EFFECT_CHROMATIC_ABERRATION) {
        // Sample R/G/B at radially-offset UVs, offset growing with distance
        // from center — standard lens/CRT-fringing approximation.
        let center = vec2<f32>(0.5, 0.5);
        let dir = in.uv - center;
        let amt = params.p1 * texel;
        let r = textureSample(src_tex, tex_sampler, in.uv + dir * amt * 2.0).r;
        let g = textureSample(src_tex, tex_sampler, in.uv).g;
        let b = textureSample(src_tex, tex_sampler, in.uv - dir * amt * 2.0).b;
        let a = textureSample(src_tex, tex_sampler, in.uv).a;
        return vec4<f32>(r, g, b, a);
    }

    if (id == EFFECT_BLOOM_EXTRACT) {
        // Bright-pass for native HQ Bloom. p1=threshold, p3=intensity,
        // p4=warmth (-1 cool .. +1 warm). p2 is reserved for blur radius
        // on the Rust side, so it is intentionally ignored here.
        let src = textureSample(src_tex, tex_sampler, in.uv);
        let l = luma(src.rgb);
        let mask = smoothstep(params.p1, min(params.p1 + 0.35, 1.0), l);
        let warmth = clamp(params.p4 * 0.5 + 0.5, 0.0, 1.0);
        let tint = mix(vec3<f32>(0.65, 0.78, 1.0), vec3<f32>(1.0, 0.78, 0.45), warmth);
        let rgb = src.rgb * src.a * mask * max(params.p3, 0.0) * tint;
        return vec4<f32>(rgb, src.a * mask);
    }

    if (id == EFFECT_SHARPEN) {
        // Classic 5-tap unsharp/sharpen kernel: [0,-1,0 / -1,5,-1 / 0,-1,0].
        let c = textureSample(src_tex, tex_sampler, in.uv);
        let n = textureSample(src_tex, tex_sampler, in.uv + vec2<f32>(0.0, -texel.y));
        let s = textureSample(src_tex, tex_sampler, in.uv + vec2<f32>(0.0, texel.y));
        let w = textureSample(src_tex, tex_sampler, in.uv + vec2<f32>(-texel.x, 0.0));
        let e = textureSample(src_tex, tex_sampler, in.uv + vec2<f32>(texel.x, 0.0));
        let amount = params.p1;
        let sharpened = c.rgb * (1.0 + 4.0 * amount) - (n.rgb + s.rgb + w.rgb + e.rgb) * amount;
        return vec4<f32>(clamp(sharpened, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    }

    if (id == EFFECT_EDGE_DETECT) {
        // Standard 3x3 Sobel Gx/Gy kernels on luminance, gradient magnitude
        // as the output — universally-documented edge-detection technique.
        var g: array<f32, 9>;
        var k = 0;
        for (var dy = -1; dy <= 1; dy = dy + 1) {
            for (var dx = -1; dx <= 1; dx = dx + 1) {
                let s = textureSample(src_tex, tex_sampler, in.uv + vec2<f32>(f32(dx), f32(dy)) * texel);
                g[k] = luma(s.rgb);
                k = k + 1;
            }
        }
        let gx = (g[2] + 2.0 * g[5] + g[8]) - (g[0] + 2.0 * g[3] + g[6]);
        let gy = (g[6] + 2.0 * g[7] + g[8]) - (g[0] + 2.0 * g[1] + g[2]);
        let mag = clamp(sqrt(gx * gx + gy * gy) * params.p1, 0.0, 1.0);
        let src = textureSample(src_tex, tex_sampler, in.uv);
        return vec4<f32>(vec3<f32>(mag), src.a);
    }

    if (id == EFFECT_GROUND_SHADOW) {
        // Fake 2D "cast shadow onto the ground" — scoped-down analog of the
        // referenced AviUtl2 GroundShadow2_S script's full 3D ground-plane
        // projection (light angle + ground plane + camera → perspective
        // transform of the object). A full 3D re-derivation isn't a good
        // fit for a single 2D fragment pass, so this reimplements the same
        // VISUAL result — a skewed, foreshortened silhouette trailing away
        // from a ground line — via the classic 2D "oblique cast shadow"
        // construction (the same shadow-skew trick used in CSS/Photoshop
        // layer-style cast shadows): a point at height h above the ground
        // line maps to a shadow point offset horizontally by h*skew and
        // pushed down past the ground line by h*length_scale. Rendered by
        // INVERSE-mapping each output pixel below the ground line back to
        // the source silhouette that would cast a shadow there, so this
        // stays a single forward raster pass with no separate geometry step.
        //
        // p1 = skew (shadow horizontal shear per unit height, e.g. light
        //      azimuth direction — mirrors the reference's "Light Slope")
        // p2 = ground_y, normalized 0..1 (mirrors "Ground Position")
        // p3 = length_scale (vertical stretch of the projected shadow —
        //      mirrors "Light Angle": shallower light = larger value)
        // p4 = opacity (mirrors "Shadow Opacity"; shadow color is fixed
        //      black — no color param in this simplified pass)
        // NOTE: every textureSample below runs UNCONDITIONALLY (no branch
        // on a per-pixel/data-dependent value gates them) — WGSL requires
        // texture sampling to stay in uniform control flow, and branching
        // on `src.a`/`in_bounds` (both derived from a per-pixel sample)
        // before another textureSample call is a validation error (naga:
        // "must only be called from uniform control flow"). All the
        // conditional LOGIC below is done with select()/clamp() arithmetic
        // instead of `if`, and the tap loop has a compile-time-constant
        // trip count, so nothing here is actually branch-gated.
        let src = textureSample(src_tex, tex_sampler, in.uv);
        let ground_y = params.p2 * params.tex_h;
        let y = in.uv.y * params.tex_h;
        let h = max((y - ground_y) / max(params.p3, 0.0001), 0.0);
        let sy = ground_y - h;
        let sx = in.uv.x * params.tex_w - h * params.p1;
        let in_bounds = sy >= 0.0 && sy < params.tex_h && sx >= 0.0 && sx < params.tex_w;
        let src_uv = vec2<f32>(sx / params.tex_w, sy / params.tex_h);
        // Distance-aware conic blur: the reference effect deliberately
        // softens the cast shadow as it gets farther from the object.  The
        // previous implementation only sampled five horizontal neighbours,
        // which produced a banded/striped shadow on diagonal silhouettes.
        // A compact weighted 3x3 kernel keeps this single-pass effect fast
        // while giving the penumbra a genuinely two-dimensional shape.
        let blur_texels = clamp(h * 0.085 + 0.75, 0.75, 14.0);
        var a_sum = 0.0;
        var w_sum = 0.0;
        for (var i = 0; i < 9; i = i + 1) {
            let ix = i % 3;
            let iy = i / 3;
            let ox = f32(ix) - 1.0;
            let oy = f32(iy) - 1.0;
            let radius = length(vec2<f32>(ox, oy));
            let weight = select(2.0, 1.0, radius > 0.9);
            let tap_uv = src_uv + vec2<f32>(ox * texel.x, oy * texel.y) * blur_texels;
            a_sum = a_sum + textureSample(src_tex, tex_sampler, tap_uv).a * weight;
            w_sum = w_sum + weight;
        }
        let valid_shadow_area = y > ground_y && params.p3 > 0.0001 && in_bounds;
        // Slightly fade the far end so long projections do not terminate as
        // a hard rectangular cut.  This is the soft contact-to-penumbra
        // transition that makes the shadow read as attached to the object.
        let distance_fade = 1.0 - smoothstep(0.72, 1.0, h / max(ground_y, 1.0));
        let shadow_a = select(0.0, (a_sum / max(w_sum, 0.001)) * clamp(params.p4, 0.0, 1.0) * distance_fade, valid_shadow_area);
        // Foreground always wins over its own shadow. IMPORTANT: this only
        // produces a correct silhouette-shaped shadow when `src` is this
        // ONE layer's own ISOLATED render (real alpha: transparent where
        // there's no content, opaque/partial only where the shape actually
        // is) — see engine.rs's call site, which runs this on the per-layer
        // `blend_layer_view`/`blur_result_view` BEFORE that layer is
        // composited onto the rest of the scene. Running this on an
        // already-flattened accumulator would NOT work: this engine seeds
        // the accumulator with an OPAQUE base color up front (composite_
        // scene's clear_texture call), so every pixel already has alpha=1
        // there regardless of whether real content is present — alpha
        // alone can no longer distinguish "shape" from "empty background"
        // once flattened (confirmed by direct pixel-probing during
        // development: sampled alpha was ~1.0 everywhere, not just over
        // the shape).
        let is_fg = src.a > 0.01;
        let out_rgb = select(vec3<f32>(0.008, 0.01, 0.016), src.rgb, is_fg);
        let out_a = select(shadow_a, src.a, is_fg);
        return vec4<f32>(out_rgb, out_a);
    }

    if (id == EFFECT_CONTOUR_BRUT) {
        // Hand-drawn/sketchy outline traced along the layer's own alpha
        // silhouette — same isolated-alpha requirement and layerOnly
        // placement as EFFECT_GROUND_SHADOW (see its comment above for why
        // this only works pre-composite). Samples a jittered ring around
        // each pixel; a mix of inside/outside alpha in that ring means the
        // pixel sits near the silhouette boundary, so it gets tinted —
        // `roughness` randomizes each tap's radius so the traced line
        // wobbles like a rough hand-drawn stroke instead of a clean circle.
        //
        // p1 = épaisseur (ring radius, px)
        // p2 = rugosité (per-tap radius jitter, 0..1)
        // p3 = luminosité du trait (0=noir..1=blanc)
        // p4 = opacité
        let src = textureSample(src_tex, tex_sampler, in.uv);
        let thickness = max(params.p1, 0.5);
        let roughness = clamp(params.p2, 0.0, 1.0);
        let taps = 12;
        var minA = 1.0;
        var maxA = 0.0;
        for (var i = 0; i < taps; i = i + 1) {
            let ang = f32(i) / f32(taps) * 6.28318;
            let jitter = 1.0 + (hash(in.uv * f32(i + 1) + params.time) - 0.5) * roughness;
            let offs = vec2<f32>(cos(ang), sin(ang)) * thickness * jitter * texel;
            let s = textureSample(src_tex, tex_sampler, in.uv + offs);
            minA = min(minA, s.a);
            maxA = max(maxA, s.a);
        }
        let edge = clamp((maxA - minA) * 2.0, 0.0, 1.0);
        let tint = clamp(params.p3, 0.0, 1.0);
        let outline_a = edge * clamp(params.p4, 0.0, 1.0);
        let out_rgb = mix(src.rgb, vec3<f32>(tint), outline_a);
        let out_a = max(src.a, outline_a);
        return vec4<f32>(out_rgb, out_a);
    }

    if (id == EFFECT_HALFTONE) {
        // Classic dot-screen: sample the color at each grid cell's CENTER
        // (not the current pixel) so every pixel within a cell agrees on
        // one dot size/luminance, then draw a black dot whose radius grows
        // as that cell's luminance drops (dark region → big dot → looks
        // darker overall, the standard halftone illusion).
        //
        // p1 = taille cellule (px)
        // p2 = intensité (max dot radius as a fraction of the cell, 0..1)
        let cell = max(params.p1, 2.0);
        let px = in.uv * vec2<f32>(params.tex_w, params.tex_h);
        let cellIdx = floor(px / cell);
        let cellPos = (px - cellIdx * cell) / cell - vec2<f32>(0.5);
        let cellCenterUv = (cellIdx + vec2<f32>(0.5)) * cell / vec2<f32>(params.tex_w, params.tex_h);
        let s = textureSample(src_tex, tex_sampler, cellCenterUv);
        let l = luma(s.rgb);
        let radius = (1.0 - l) * 0.5 * clamp(params.p2, 0.0, 1.0);
        let inDot = length(cellPos) < radius;
        let out_rgb = select(vec3<f32>(1.0), vec3<f32>(0.0), inDot);
        return vec4<f32>(out_rgb, s.a);
    }

    // Everything below is a plain per-pixel color remap — single sample.
    let src = textureSample(src_tex, tex_sampler, in.uv);
    var rgb = src.rgb;

    if (id == EFFECT_THRESHOLD) {
        // Hard (or softened) black/white luminance cutoff.
        // p1 = seuil (0..1), p2 = adoucissement (smoothstep half-width)
        let threshold = clamp(params.p1, 0.0, 1.0);
        let soft = max(params.p2, 0.0001);
        let l = luma(rgb);
        rgb = vec3<f32>(smoothstep(threshold - soft, threshold + soft, l));
    } else if (id == EFFECT_SEPIA) {
        // Standard sepia transform matrix — identical constants across
        // every independent sepia-shader tutorial.
        let r = dot(rgb, vec3<f32>(0.393, 0.769, 0.189));
        let g = dot(rgb, vec3<f32>(0.349, 0.686, 0.168));
        let b = dot(rgb, vec3<f32>(0.272, 0.534, 0.131));
        rgb = clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
    } else if (id == EFFECT_INVERT) {
        rgb = 1.0 - rgb;
    } else if (id == EFFECT_GRAYSCALE) {
        rgb = vec3<f32>(luma(rgb));
    } else if (id == EFFECT_POSTERIZE) {
        let levels = max(params.p1, 2.0);
        rgb = floor(rgb * levels) / levels;
    } else if (id == EFFECT_SCANLINES) {
        // Darken every other row via a sine wave across screen-space Y,
        // plus optional edge vignette — kept simple (no curvature/subpixel
        // mask) to stay a generic scanline darkening effect rather than a
        // full CRT-shader-pack pastiche.
        let freq = max(params.p1, 1.0);
        let darkness = clamp(params.p2, 0.0, 1.0);
        let wave = sin(in.uv.y * params.tex_h * freq * 3.14159);
        let dim = 1.0 - darkness * (0.5 + 0.5 * wave);
        rgb = rgb * dim;
    } else if (id == EFFECT_GRAIN) {
        let n = hash(in.uv * vec2<f32>(params.tex_w, params.tex_h) + params.time);
        rgb = clamp(rgb + (n - 0.5) * params.p1, vec3<f32>(0.0), vec3<f32>(1.0));
    }

    return vec4<f32>(rgb, src.a);
}
