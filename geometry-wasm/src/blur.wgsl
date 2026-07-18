// Layer-level feather/blur pass (2026-07) — same fullscreen-triangle shape
// as blend.wgsl/matte.wgsl (see blend.wgsl's own doc comment for why a
// custom pass exists at all: vello/kurbo have no generic "blur this layer"
// brush or primitive). Reads one layer's own isolated render (already
// composited/matted by the time composite_scene calls this) and writes a
// blurred copy, which is what actually gets fed into blend.wgsl's
// composite_pass afterward.
//
// Fixed 9x9 sample grid (NOT a true unbounded-radius Gaussian) — WGSL wants
// statically-bounded loops, and this is meant as a soft feather effect, not
// a large-radius depth-of-field look. Larger `radius_px` spreads the same 81
// samples further apart rather than adding more of them, so quality degrades
// gracefully (softer but slightly less smooth) as radius grows instead of
// costing more GPU time — an acceptable v1 tradeoff over a true separable
// Gaussian (which would need two passes + an intermediate texture).
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
    _pad0: f32,
};
@group(0) @binding(2) var<uniform> params: Params;

const KERNEL_HALF: i32 = 4; // 9x9 grid

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = vec2<f32>(1.0 / params.tex_w, 1.0 / params.tex_h);
    let step = max(params.radius_px, 0.0) / f32(KERNEL_HALF);
    var color_sum = vec3<f32>(0.0);
    var alpha_sum = 0.0;
    var wsum = 0.0;
    let sigma2 = f32(KERNEL_HALF) * f32(KERNEL_HALF) * 0.5 + 0.001;
    for (var y = -KERNEL_HALF; y <= KERNEL_HALF; y = y + 1) {
        for (var x = -KERNEL_HALF; x <= KERNEL_HALF; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * step * texel;
            let d2 = f32(x * x + y * y);
            let w = exp(-d2 / sigma2);
            let s = textureSample(src_tex, tex_sampler, in.uv + offset);
            color_sum = color_sum + s.rgb * s.a * w;
            alpha_sum = alpha_sum + s.a * w;
            wsum = wsum + w;
        }
    }
    let out_alpha = alpha_sum / max(wsum, 0.0001);
    let out_color = color_sum / max(alpha_sum, 0.0001);
    return vec4<f32>(out_color, out_alpha);
}
