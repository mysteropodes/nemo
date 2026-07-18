// Adjustment-layer color pass (2026-07, Motion effect layers) — brightness/
// contrast applied to EVERYTHING BELOW an effect layer in the stack
// (composite_scene passes the running accumulator as source_view and
// writes the adjusted result back as the new accumulator state), not just
// one layer's own content — the AE "Adjustment Layer" convention. Same
// fullscreen-triangle shape as blur.wgsl/blend.wgsl/matte.wgsl (see
// blend.wgsl's own doc comment for why a custom pass exists at all: vello/
// kurbo have no generic "grade this layer" primitive).

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
    brightness: f32, // -1..1, additive
    contrast: f32,   // -1..1, 0 = no change, pivots around mid-gray
    _pad0: f32,
    _pad1: f32,
};
@group(0) @binding(2) var<uniform> params: Params;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let s = textureSample(src_tex, tex_sampler, in.uv);
    // Un-premultiply, adjust straight color, re-premultiply — same
    // premultiplied-in/out discipline as blur.wgsl, so a translucent
    // backdrop never bleeds color from behind its own transparent pixels
    // into the brightness/contrast math.
    let a = max(s.a, 0.0001);
    var color = s.rgb / a;
    let contrast_factor = 1.0 + clamp(params.contrast, -1.0, 1.0);
    color = (color - vec3<f32>(0.5)) * contrast_factor + vec3<f32>(0.5) + params.brightness;
    color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(color * s.a, s.a);
}
