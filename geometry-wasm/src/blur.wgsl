// Layer-level feather/blur pass (2026-07, rewritten 2026-07 for quality —
// feedback: "le flou n'est pas de bonne qualité") — same fullscreen-triangle
// shape as blend.wgsl/matte.wgsl (see blend.wgsl's own doc comment for why a
// custom pass exists at all: vello/kurbo have no generic "blur this layer"
// brush or primitive). Reads one layer's own isolated render (already
// composited/matted by the time composite_scene calls this) and writes a
// blurred copy, which is what actually gets fed into blend.wgsl's
// composite_pass afterward.
//
// SEPARABLE two-pass Gaussian (run this shader once with dir=(1,0), then
// again with dir=(0,1) on the first pass's output — see engine.rs's
// blur_pass call sites, each now issues the pair) — replaces a single-pass
// fixed 9x9=81-tap 2D kernel that looked blocky/banded at anything but small
// radii. Root cause of the old artifact: its sample SPACING scaled with
// radius_px while the tap COUNT stayed fixed at 9 per axis, so at e.g.
// radius=50px samples ended up ~12px apart with nothing in between ever
// sampled — a classic undersampled-kernel look, exactly the blockiness in
// the reported screenshot. This version keeps sample spacing fixed at 1
// texel (KERNEL_HALF=16, 33 taps) and derives a REAL Gaussian sigma from
// radius_px (sigma = radius_px/3, the standard "practical cutoff ≈ 3σ"
// convention), so the weight curve is evaluated at the sample's actual
// physical offset rather than merely its index in the loop — dense,
// gap-free coverage for any radius up to ~48px (3×16) per pass, matching
// how separable Gaussian blurs are implemented in Graphite/most real-time
// engines (two 1D passes instead of one 2D pass, which is also ~2.5x
// cheaper here: 2×33=66 taps total vs the old 81).
//
// Premultiplies before averaging, un-premultiplies after (straight alpha in,
// straight alpha out, matching every other pass here) — blurring straight
// alpha directly bleeds color out of transparent edges (a fully-transparent
// red pixel still contributes full "red" to a naive average); premultiplying
// first is the standard fix.

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
    radius_px: f32,
    tex_w: f32,
    tex_h: f32,
    dir_x: f32,
    dir_y: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
@group(0) @binding(2) var<uniform> params: Params;

const KERNEL_HALF: i32 = 24; // 49 taps; spacing adapts for very large radii

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = vec2<f32>(1.0 / params.tex_w, 1.0 / params.tex_h);
    let dir = vec2<f32>(params.dir_x, params.dir_y);
    // sigma ≈ radius/3 (practical Gaussian cutoff), floored so radius=0
    // degenerates to an all-weight-on-center passthrough instead of a 0/0.
    // Keep dense sampling for small blurs, but cover the complete requested
    // radius for large blurs instead of clipping the Gaussian at ±16 px.
    // Linear filtering between these taps preserves a smooth result without
    // the wide-radius banding of the old fixed-spacing kernel.
    let sample_step = max(params.radius_px / 24.0, 1.0);
    let sigma = max(params.radius_px / 3.0 / sample_step, 0.0001);
    let two_sigma2 = 2.0 * sigma * sigma;
    var color_sum = vec3<f32>(0.0);
    var alpha_sum = 0.0;
    var wsum = 0.0;
    for (var i = -KERNEL_HALF; i <= KERNEL_HALF; i = i + 1) {
        let offset_texels = f32(i);
        let w = exp(-(offset_texels * offset_texels) / two_sigma2);
        let offset = dir * offset_texels * sample_step * texel;
        let s = textureSample(src_tex, tex_sampler, in.uv + offset);
        color_sum = color_sum + s.rgb * s.a * w;
        alpha_sum = alpha_sum + s.a * w;
        wsum = wsum + w;
    }
    let out_alpha = alpha_sum / max(wsum, 0.0001);
    let out_color = color_sum / max(alpha_sum, 0.0001);
    return vec4<f32>(out_color, out_alpha);
}
