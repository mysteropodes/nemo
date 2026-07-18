// Vignette adjustment pass (2026-07, Motion effect layers) — darkens
// toward the frame edges, applied to EVERYTHING BELOW an effect layer in
// the stack (composite_scene passes the running accumulator as
// source_view and writes the result back as the new accumulator state),
// same "full-frame grade" contract as color_adjust.wgsl. Same fullscreen-
// triangle shape as blur.wgsl/blend.wgsl/matte.wgsl.

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
    strength: f32, // 0..1 — how dark the corners get at full falloff
    radius: f32,   // 0..1 — normalized distance from center where the darkening starts (smaller = starts sooner, larger vignette)
    _pad0: f32,
    _pad1: f32,
};
@group(0) @binding(2) var<uniform> params: Params;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let s = textureSample(src_tex, tex_sampler, in.uv);
    // Distance from center, normalized so the corners are at ~1.0
    // regardless of aspect ratio (uv is already 0..1 per axis).
    let d = distance(in.uv, vec2<f32>(0.5, 0.5)) / 0.7071;
    let start = clamp(params.radius, 0.0, 0.99);
    let falloff = clamp((d - start) / max(0.001, 1.0 - start), 0.0, 1.0);
    let darken = 1.0 - falloff * clamp(params.strength, 0.0, 1.0);
    // Premultiplied in/out, same discipline as blur.wgsl/color_adjust.wgsl —
    // darkening the color channel only (never alpha) keeps a translucent
    // backdrop's own transparency untouched by the vignette.
    return vec4<f32>(s.rgb * darken, s.a);
}
