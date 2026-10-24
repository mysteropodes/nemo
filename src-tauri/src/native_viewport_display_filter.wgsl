// Display transforms operate in the already encoded sRGB byte domain, like CSS filters.
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    var out: VertexOutput;
    out.uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    out.position = vec4<f32>(out.uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv.y = 1.0 - out.uv.y;
    return out;
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@fragment
fn fs_values(v: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source, source_sampler, v.uv);
    let value = dot(color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec4<f32>(vec3<f32>(value), color.a);
}
@fragment
fn fs_contrast(v: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source, source_sampler, v.uv);
    // CSS applies contrast first, then the saturate(1.1) color matrix.
    let c = clamp((color.rgb - vec3<f32>(0.5)) * 1.6 + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
    let saturated = vec3<f32>(
        dot(c, vec3<f32>(1.0787, -0.0715, -0.0072)),
        dot(c, vec3<f32>(-0.0213, 1.0285, -0.0072)),
        dot(c, vec3<f32>(-0.0213, -0.0715, 1.0928))
    );
    return vec4<f32>(clamp(saturated, vec3<f32>(0.0), vec3<f32>(1.0)), color.a);
}
@fragment
fn fs_dim(v: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source, source_sampler, v.uv);
    return vec4<f32>(color.rgb * 0.55, color.a);
}
