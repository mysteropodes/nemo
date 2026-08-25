use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
struct ShaderDef {
    id: String,
    source: String,
}

// The shipped library is JavaScript data, whereas the renderer validates WGSL
// only once WebGPU creates its pipeline.  This test brings that validation
// into `cargo test`: a malformed built-in shader can no longer silently ship.
fn wrapped(body: &str) -> String {
    format!(
        r#"struct VsOut {{ @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, }};
@vertex fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {{
  var positions = array<vec2<f32>, 3>(vec2<f32>(-1.0,-1.0), vec2<f32>(3.0,-1.0), vec2<f32>(-1.0,3.0));
  let p = positions[vid]; var out: VsOut; out.pos = vec4<f32>(p,0.0,1.0); out.uv = vec2<f32>(p.x * 0.5 + 0.5, 1.0 - (p.y * 0.5 + 0.5)); return out;
}}
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;
struct Params {{ effect_id:f32, p1:f32, p2:f32, p3:f32, tex_w:f32, tex_h:f32, time:f32, p4:f32, bbox_x:f32, bbox_y:f32, bbox_w:f32, bbox_h:f32, p5:f32, p6:f32, p7:f32, p8:f32, }};
@group(0) @binding(2) var<uniform> params: Params;
@fragment fn fs_main(in: VsOut) -> @location(0) vec4<f32> {{
  let uv = in.uv; let texel = vec2<f32>(1.0 / max(params.tex_w, 1.0), 1.0 / max(params.tex_h, 1.0));
  let src = textureSample(src_tex, tex_sampler, uv);
  let bbox_o = vec2<f32>(params.bbox_x, params.bbox_y);
  let bbox_s = vec2<f32>(max(params.bbox_w, 1.0), max(params.bbox_h, 1.0));
  let local_uv = (uv * vec2<f32>(params.tex_w, params.tex_h) - bbox_o) / bbox_s;
  {body}
}}"#
    )
}

#[test]
fn every_shipped_shader_parses_and_validates() {
    let js = r#"const fs=require('fs'),vm=require('vm'); const window={}; vm.runInNewContext(fs.readFileSync(process.argv[1],'utf8'),{window}); process.stdout.write(JSON.stringify(window.SMSHADER_EFFECTS));"#;
    let output = Command::new("node")
        .args(["-e", js, "../src/js/shader-effects-library.js"])
        .output()
        .expect("Node.js is required to inspect the shader library");
    assert!(output.status.success(), "could not load shader library: {}", String::from_utf8_lossy(&output.stderr));
    let defs: Vec<ShaderDef> = serde_json::from_slice(&output.stdout).expect("shader library must be valid JSON data");
    assert!(!defs.is_empty());
    for effect in defs {
        let module = naga::front::wgsl::parse_str(&wrapped(&effect.source))
            .unwrap_or_else(|error| panic!("{} has invalid WGSL: {error}", effect.id));
        let mut validator = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        validator.validate(&module).unwrap_or_else(|error| panic!("{} failed WGSL validation: {error}", effect.id));
    }
}
