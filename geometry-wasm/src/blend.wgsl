// Fullscreen-triangle blend/composite pass: composites `source_tex` (one
// scene layer's own isolated, transparent-backed render) over `backdrop_tex`
// (everything composited so far) using the CSS/W3C Compositing-and-Blending
// Mix formula selected by `params.mode`, then standard Porter-Duff SrcOver.
//
// This is StrokeMotion's own blend compositor, added because vello 0.9's
// own push_layer(Mix::X) silently produces no visible blend for a
// whole-layer group (root-caused in engine.rs's mix_mode_index doc comment
// — not a wiring bug), and there is no way to work around it with a CPU
// readback from render()'s synchronous, every-frame call site: WebGPU's
// mapAsync is fundamentally async-only on wasm32, with no blocking-poll
// escape hatch. Keeping the whole composite operation GPU-side and
// synchronous is what lets it slot into the existing render()/
// render_to_pixels() call pattern unchanged.
//
// Both input textures and the output are treated as STRAIGHT (non-
// premultiplied) alpha, matching how vello's own render_to_texture writes
// an Rgba8Unorm target (confirmed by render_to_pixels()'s own doc comment:
// the CPU readback is documented as "straight-alpha RGBA8 bytes").

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

@group(0) @binding(0) var backdrop_tex: texture_2d<f32>;
@group(0) @binding(1) var source_tex: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

struct Params {
    mode: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};
@group(0) @binding(3) var<uniform> params: Params;

fn blend_multiply(cb: f32, cs: f32) -> f32 { return cb * cs; }
fn blend_screen(cb: f32, cs: f32) -> f32 { return cb + cs - cb * cs; }
fn blend_hardlight(cb: f32, cs: f32) -> f32 {
    if (cs <= 0.5) { return blend_multiply(cb, 2.0 * cs); }
    return blend_screen(cb, 2.0 * cs - 1.0);
}
// Overlay(Cb,Cs) = HardLight(Cs,Cb) per the W3C spec's own definition.
fn blend_overlay(cb: f32, cs: f32) -> f32 { return blend_hardlight(cs, cb); }
fn blend_darken(cb: f32, cs: f32) -> f32 { return min(cb, cs); }
fn blend_lighten(cb: f32, cs: f32) -> f32 { return max(cb, cs); }
fn blend_colordodge(cb: f32, cs: f32) -> f32 {
    if (cb <= 0.0) { return 0.0; }
    if (cs >= 1.0) { return 1.0; }
    return min(1.0, cb / (1.0 - cs));
}
fn blend_colorburn(cb: f32, cs: f32) -> f32 {
    if (cb >= 1.0) { return 1.0; }
    if (cs <= 0.0) { return 0.0; }
    return 1.0 - min(1.0, (1.0 - cb) / cs);
}
fn blend_softlight(cb: f32, cs: f32) -> f32 {
    if (cs <= 0.5) {
        return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
    }
    var d: f32;
    if (cb <= 0.25) { d = ((16.0 * cb - 12.0) * cb + 4.0) * cb; } else { d = sqrt(cb); }
    return cb + (2.0 * cs - 1.0) * (d - cb);
}
fn blend_difference(cb: f32, cs: f32) -> f32 { return abs(cb - cs); }
fn blend_exclusion(cb: f32, cs: f32) -> f32 { return cb + cs - 2.0 * cb * cs; }

fn lum(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.3, 0.59, 0.11)); }

fn clip_color(c_in: vec3<f32>) -> vec3<f32> {
    var c = c_in;
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    if (n < 0.0) {
        c = l + (c - l) * (l / max(l - n, 1e-6));
    }
    if (x > 1.0) {
        c = l + (c - l) * ((1.0 - l) / max(x - l, 1e-6));
    }
    return c;
}
fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    let d = l - lum(c);
    return clip_color(c + vec3<f32>(d, d, d));
}
fn sat(c: vec3<f32>) -> f32 { return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b)); }

// W3C SetSat: rescale so the max/mid/min channels land on s/(rescaled)/0,
// preserving hue+luminosity for the later SetLum pass. WGSL has no
// convenient dynamic-index sort for a 3-vector, so this walks all 6
// orderings of (r,g,b) explicitly rather than sorting indices.
fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    var r = c.r; var g = c.g; var b = c.b;
    if (r <= g) {
        if (g <= b) {
            if (b > r) { g = (g - r) * s / (b - r); b = s; } else { g = 0.0; b = 0.0; }
            r = 0.0;
        } else if (r <= b) {
            if (g > r) { b = (b - r) * s / (g - r); g = s; } else { b = 0.0; g = 0.0; }
            r = 0.0;
        } else {
            if (g > b) { r = (r - b) * s / (g - b); g = s; } else { r = 0.0; g = 0.0; }
            b = 0.0;
        }
    } else {
        if (r <= b) {
            if (b > g) { r = (r - g) * s / (b - g); b = s; } else { r = 0.0; b = 0.0; }
            g = 0.0;
        } else if (g <= b) {
            if (r > g) { b = (b - g) * s / (r - g); r = s; } else { b = 0.0; r = 0.0; }
            g = 0.0;
        } else {
            if (r > b) { g = (g - b) * s / (r - b); r = s; } else { g = 0.0; r = 0.0; }
            b = 0.0;
        }
    }
    return vec3<f32>(r, g, b);
}

fn blend_hue(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> { return set_lum(set_sat(cs, sat(cb)), lum(cb)); }
fn blend_saturation(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> { return set_lum(set_sat(cb, sat(cs)), lum(cb)); }
fn blend_color(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> { return set_lum(cs, lum(cb)); }
fn blend_luminosity(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> { return set_lum(cb, lum(cs)); }

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let backdrop = textureSample(backdrop_tex, tex_sampler, in.uv);
    let source = textureSample(source_tex, tex_sampler, in.uv);
    let ab = backdrop.a;
    let as_ = source.a;
    let cb = backdrop.rgb;
    let cs = source.rgb;

    var blended: vec3<f32>;
    switch params.mode {
        case 1u: { blended = vec3<f32>(blend_multiply(cb.r, cs.r), blend_multiply(cb.g, cs.g), blend_multiply(cb.b, cs.b)); }
        case 2u: { blended = vec3<f32>(blend_screen(cb.r, cs.r), blend_screen(cb.g, cs.g), blend_screen(cb.b, cs.b)); }
        case 3u: { blended = vec3<f32>(blend_overlay(cb.r, cs.r), blend_overlay(cb.g, cs.g), blend_overlay(cb.b, cs.b)); }
        case 4u: { blended = vec3<f32>(blend_darken(cb.r, cs.r), blend_darken(cb.g, cs.g), blend_darken(cb.b, cs.b)); }
        case 5u: { blended = vec3<f32>(blend_lighten(cb.r, cs.r), blend_lighten(cb.g, cs.g), blend_lighten(cb.b, cs.b)); }
        case 6u: { blended = vec3<f32>(blend_colordodge(cb.r, cs.r), blend_colordodge(cb.g, cs.g), blend_colordodge(cb.b, cs.b)); }
        case 7u: { blended = vec3<f32>(blend_colorburn(cb.r, cs.r), blend_colorburn(cb.g, cs.g), blend_colorburn(cb.b, cs.b)); }
        case 8u: { blended = vec3<f32>(blend_hardlight(cb.r, cs.r), blend_hardlight(cb.g, cs.g), blend_hardlight(cb.b, cs.b)); }
        case 9u: { blended = vec3<f32>(blend_softlight(cb.r, cs.r), blend_softlight(cb.g, cs.g), blend_softlight(cb.b, cs.b)); }
        case 10u: { blended = vec3<f32>(blend_difference(cb.r, cs.r), blend_difference(cb.g, cs.g), blend_difference(cb.b, cs.b)); }
        case 11u: { blended = vec3<f32>(blend_exclusion(cb.r, cs.r), blend_exclusion(cb.g, cs.g), blend_exclusion(cb.b, cs.b)); }
        case 12u: { blended = blend_hue(cb, cs); }
        case 13u: { blended = blend_saturation(cb, cs); }
        case 14u: { blended = blend_color(cb, cs); }
        case 15u: { blended = blend_luminosity(cb, cs); }
        default: { blended = cs; } // Normal / unrecognized — plain source color, straight SrcOver below
    }

    // W3C generalized compositing formula (https://www.w3.org/TR/compositing-1/#generalformula),
    // Porter-Duff SrcOver (Fa=1, Fb=1-as) applied to the blend-mixed source color:
    //   Cs_mix = (1-ab)*Cs + ab*B(Cb,Cs)
    //   Co     = as*Cs_mix + ab*(1-as)*Cb      (premultiplied numerator)
    //   ao     = as + ab*(1-as)
    // then divide back down to straight color for storage in this straight-
    // alpha target.
    let cs_mix = (1.0 - ab) * cs + ab * blended;
    let ao = as_ + ab * (1.0 - as_);
    let co = as_ * cs_mix + ab * (1.0 - as_) * cb;
    let co_straight = select(vec3<f32>(0.0), co / max(ao, 1e-6), ao > 1e-6);
    return vec4<f32>(co_straight, ao);
}
