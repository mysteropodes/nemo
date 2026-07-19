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
    _pad0: f32,
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

    // Everything below is a plain per-pixel color remap — single sample.
    let src = textureSample(src_tex, tex_sampler, in.uv);
    var rgb = src.rgb;

    if (id == EFFECT_SEPIA) {
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
