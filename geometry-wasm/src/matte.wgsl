// Track matte pass (2026-07, scouted from Caddis's Layer.matteLayerId/
// matteMode fields — the AE "track matte" feature: one layer's alpha or
// luminance masks the layer directly below it, instead of the mask layer
// drawing its own visible content).
//
// Fullscreen-triangle pass, same shape as blend.wgsl (see its own doc
// comment for why a custom GPU pass exists instead of vello's
// push_layer — same root cause applies to any vello-native clip/mask
// primitive: not usable here). Reads `layer_tex` (the masked layer's own
// isolated render) and `matte_tex` (the matte SOURCE layer's isolated
// render, i.e. the layer immediately above it in the stack), multiplies
// layer_tex's alpha by the matte's alpha or luma, writes the masked
// result — which the caller (engine.rs composite_scene) then feeds into
// the EXISTING blend.wgsl composite_pass exactly like any other layer.
//
// STRAIGHT (non-premultiplied) alpha throughout — same convention as
// blend.wgsl (vello's render_to_texture writes straight-alpha Rgba8Unorm).

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

@group(0) @binding(0) var layer_tex: texture_2d<f32>;
@group(0) @binding(1) var matte_tex: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

struct Params {
    mode: u32,   // 0 = alpha matte, 1 = luma matte
    invert: u32, // 0 = normal, 1 = inverted (AE's "…Inverted" matte variants)
    _pad0: u32,
    _pad1: u32,
};
@group(0) @binding(3) var<uniform> params: Params;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let layer = textureSample(layer_tex, tex_sampler, in.uv);
    let matte = textureSample(matte_tex, tex_sampler, in.uv);

    var m: f32;
    if (params.mode == 0u) {
        m = matte.a;
    } else {
        // Rec. 709 luma, weighted by the matte source's own alpha — a
        // transparent pixel in the matte source contributes zero mask
        // regardless of what color happened to be under it.
        let luma = dot(matte.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        m = luma * matte.a;
    }
    if (params.invert != 0u) {
        m = 1.0 - m;
    }
    return vec4<f32>(layer.rgb, layer.a * m);
}
