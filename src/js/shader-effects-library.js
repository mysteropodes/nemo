// ---- Shader WGSL effect library (2026-07) ----
// Shipped as built-in custom shaders. Each source is a fragment BODY only:
// no top-level WGSL declarations, because engine.rs wraps it inside fs_main.
(function () {
  // `spatial` (2026-07-29 fix, "un effet twirl qui bouge en fonction du
  // zoom du canvas"): engine.rs's run_one_effect deliberately does NOT
  // zoom-compensate any "custom:" effect's p1..p4 (its own comment: "p1..p4
  // there have no fixed meaning this function could assume is a pixel
  // size") — correct for a truly arbitrary user-authored shader, but this
  // SHIPPED library already knows exactly what each parameter means (e.g.
  // Twirl's Radius below is a distance from center, normalized 0-1 against
  // the FIXED-size render target/uv space — the exact same "effects change
  // size in the wrong direction as zoom changes" bug already fixed for the
  // built-in effects, just via a normalized UV radius here instead of a raw
  // pixel count). Flag any param here whose value is a spatial size/radius/
  // offset (angles, colors, ratios, counts, opacities are NOT spatial) —
  // sceneEffectsOf (engine-bridge.js) reads this flag and multiplies the
  // value by view.zoom before it ever reaches the engine, mirroring
  // run_one_effect's own `* z` compensation for the built-in effects.
  function param(key, label, min, max, step, unit, defaultValue, spatial) {
    var out = { key: key, label: label, min: min, max: max, step: step, scale: 1, unit: unit || '' };
    if (defaultValue !== undefined) out.defaultValue = defaultValue;
    if (spatial) out.spatial = true;
    return out;
  }
  function fx(id, name, category, params, lines) {
    return { id: id, name: name, category: category, params: params, source: lines.join('\n') };
  }

  var effects = [
    fx('shader_exposure', 'Exposure', 'Color', [
      param('p1', 'Exposure', -4, 4, 0.05, 'EV', 0),
      param('p2', 'Offset', -1, 1, 0.01, '', 0),
      param('p3', 'Gamma', 0.1, 4, 0.05, '', 1),
    ], [
      'let rgb = pow(max(src.rgb * pow(2.0, params.p1) + vec3<f32>(params.p2), vec3<f32>(0.0)), vec3<f32>(1.0 / max(params.p3, 0.001)));',
      'return vec4<f32>(rgb, src.a);',
    ]),
    fx('shader_hue_saturation', 'Hue / Saturation', 'Color', [
      param('p1', 'Hue', -180, 180, 1, 'deg', 0),
      param('p2', 'Saturation', -1, 2, 0.01, '', 0),
      param('p3', 'Lightness', -1, 1, 0.01, '', 0),
    ], [
      'let a = params.p1 * 0.01745329252;',
      'let yiq = vec3<f32>(dot(src.rgb, vec3<f32>(0.299, 0.587, 0.114)), dot(src.rgb, vec3<f32>(0.596, -0.274, -0.322)), dot(src.rgb, vec3<f32>(0.211, -0.523, 0.312)));',
      'let chroma = vec2<f32>(yiq.y * cos(a) - yiq.z * sin(a), yiq.y * sin(a) + yiq.z * cos(a)) * (1.0 + params.p2);',
      'let y = clamp(yiq.x + params.p3, 0.0, 1.0);',
      'let rgb = vec3<f32>(y + 0.956 * chroma.x + 0.621 * chroma.y, y - 0.272 * chroma.x - 0.647 * chroma.y, y - 1.106 * chroma.x + 1.703 * chroma.y);',
      'return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);',
    ]),
    fx('shader_tint', 'Tint', 'Color', [
      param('p1', 'Amount', 0, 1, 0.01, '', 1),
      param('p2', 'Dark tint', 0, 1, 0.01, '', 0.1),
      param('p3', 'Light tint', 0, 1, 0.01, '', 0.9),
    ], [
      'let l = dot(src.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'let dark = vec3<f32>(params.p2, 0.02, 1.0 - params.p2);',
      'let light = vec3<f32>(1.0, 0.82 + params.p3 * 0.18, params.p3);',
      'return vec4<f32>(mix(src.rgb, mix(dark, light, l), clamp(params.p1, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_tritone', 'Tritone', 'Color', [
      param('p1', 'Midpoint', 0, 1, 0.01, '', 0.5),
      param('p2', 'Blend', 0, 1, 0.01, '', 1),
    ], [
      'let l = dot(src.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'let sh = vec3<f32>(0.02, 0.04, 0.18); let mid = vec3<f32>(0.95, 0.28, 0.1); let hi = vec3<f32>(1.0, 0.95, 0.55);',
      'let m = clamp(params.p1, 0.05, 0.95);',
      'let tri = select(mix(mid, hi, smoothstep(m, 1.0, l)), mix(sh, mid, smoothstep(0.0, m, l)), l < m);',
      'return vec4<f32>(mix(src.rgb, tri, clamp(params.p2, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_channel_mixer', 'Channel Mixer', 'Color', [
      param('p1', 'Red mix', -1, 1, 0.01, '', 0),
      param('p2', 'Green mix', -1, 1, 0.01, '', 0),
      param('p3', 'Blue mix', -1, 1, 0.01, '', 0),
    ], [
      'let c = src.rgb;',
      'return vec4<f32>(vec3<f32>(c.r + (c.g - c.r) * params.p1, c.g + (c.b - c.g) * params.p2, c.b + (c.r - c.b) * params.p3), src.a);',
    ]),
    fx('shader_levels', 'Levels', 'Color', [
      param('p1', 'Black', 0, 1, 0.01, '', 0),
      param('p2', 'White', 0, 1, 0.01, '', 1),
      param('p3', 'Gamma', 0.1, 4, 0.05, '', 1),
    ], [
      'let black = min(params.p1, params.p2 - 0.01); let white = max(params.p2, black + 0.01);',
      'let rgb = pow(clamp((src.rgb - vec3<f32>(black)) / (white - black), vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / max(params.p3, 0.001)));',
      'return vec4<f32>(rgb, src.a);',
    ]),
    fx('shader_emboss', 'Emboss', 'Stylize', [
      param('p1', 'Strength', 0, 4, 0.05, '', 1),
      param('p2', 'Angle', 0, 360, 1, 'deg', 135),
    ], [
      'let a = params.p2 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)) * texel * 2.0;',
      'let l1 = dot(textureSample(src_tex, tex_sampler, uv + d).rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'let l2 = dot(textureSample(src_tex, tex_sampler, uv - d).rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'return vec4<f32>(vec3<f32>(0.5 + (l1 - l2) * params.p1), src.a);',
    ]),
    fx('shader_find_edges', 'Find Edges', 'Stylize', [
      param('p1', 'Intensity', 0, 8, 0.1, '', 3),
      param('p2', 'Invert', 0, 1, 1, '', 0),
    ], [
      'let gx = dot(textureSample(src_tex, tex_sampler, uv + vec2<f32>(texel.x, 0.0)).rgb - textureSample(src_tex, tex_sampler, uv - vec2<f32>(texel.x, 0.0)).rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'let gy = dot(textureSample(src_tex, tex_sampler, uv + vec2<f32>(0.0, texel.y)).rgb - textureSample(src_tex, tex_sampler, uv - vec2<f32>(0.0, texel.y)).rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'var e = clamp(length(vec2<f32>(gx, gy)) * params.p1, 0.0, 1.0); e = select(e, 1.0 - e, params.p2 > 0.5);',
      'return vec4<f32>(vec3<f32>(e), src.a);',
    ]),
    fx('shader_threshold_rgb', 'Threshold RGB', 'Stylize', [
      param('p1', 'Red', 0, 1, 0.01, '', 0.5),
      param('p2', 'Green', 0, 1, 0.01, '', 0.5),
      param('p3', 'Blue', 0, 1, 0.01, '', 0.5),
    ], [
      'return vec4<f32>(vec3<f32>(select(0.0, 1.0, src.r >= params.p1), select(0.0, 1.0, src.g >= params.p2), select(0.0, 1.0, src.b >= params.p3)), src.a);',
    ]),
    fx('shader_roughen_alpha', 'Roughen Alpha', 'Stylize', [
      param('p1', 'Amount', 0, 1, 0.01, '', 0.5),
      // Missing the `spatial` flag every other 'px'-unit param in this file
      // carries (found 2026-07-30 auditing every effect against the Twirl
      // fix's own precedent, Cyril: "les effets Wgsl qui ne sont pas stable
      // appliqué à un objet calques quand on zoom") — without it, the noise
      // cell size stays a fixed SCREEN-pixel count instead of scaling with
      // view.zoom like shader_grid's near-identical 'Size' param does, so
      // the roughened edge's grain visibly changes density every time you
      // zoom instead of staying constant relative to the artwork.
      param('p2', 'Scale', 1, 80, 1, 'px', 18, true),
      param('p3', 'Softness', 0, 1, 0.01, '', 0.15),
    ], [
      'let cell = floor(uv * max(params.p2, 1.0) * 80.0);',
      'let n = fract(sin(dot(cell, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let edge = smoothstep(n - params.p3, n + params.p3 + 0.001, src.a);',
      'let a = mix(src.a, edge * src.a, clamp(params.p1, 0.0, 1.0));',
      'return vec4<f32>(src.rgb * select(0.0, a / max(src.a, 0.001), src.a > 0.001), a);',
    ]),
    fx('shader_minimax', 'Minimax', 'Stylize', [
      param('p1', 'Radius', 1, 12, 1, 'px', 2, true),
      param('p2', 'Mode', 0, 1, 1, '', 1),
    ], [
      'let r = max(1.0, params.p1); var mn = src.rgb; var mx = src.rgb;',
      'for (var i = -2; i <= 2; i = i + 1) { for (var j = -2; j <= 2; j = j + 1) { let c = textureSample(src_tex, tex_sampler, uv + vec2<f32>(f32(i), f32(j)) * texel * r).rgb; mn = min(mn, c); mx = max(mx, c); } }',
      'return vec4<f32>(select(mn, mx, params.p2 > 0.5), src.a);',
    ]),
    fx('shader_grid', 'Grid', 'Generate', [
      param('p1', 'Size', 4, 160, 1, 'px', 40, true),
      param('p2', 'Width', 0.5, 12, 0.5, 'px', 1, true),
      param('p3', 'Opacity', 0, 1, 0.01, '', 0.7),
    ], [
      'let cell = max(params.p1, 1.0); let pos = uv * vec2<f32>(params.tex_w, params.tex_h); let g = min(fract(pos.x / cell), fract(pos.y / cell));',
      'let line = 1.0 - smoothstep(0.0, params.p2 / cell, g);',
      'return vec4<f32>(mix(src.rgb, vec3<f32>(1.0), line * clamp(params.p3, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_checkerboard', 'Checkerboard', 'Generate', [
      param('p1', 'Size', 4, 160, 1, 'px', 32, true),
      param('p2', 'Opacity', 0, 1, 0.01, '', 0.8),
    ], [
      'let pos = floor(uv * vec2<f32>(params.tex_w, params.tex_h) / max(params.p1, 1.0)); let chk = fract((pos.x + pos.y) * 0.5) * 2.0;',
      'return vec4<f32>(mix(src.rgb, mix(vec3<f32>(0.08), vec3<f32>(0.9), chk), clamp(params.p2, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_gradient_ramp', 'Gradient Ramp', 'Generate', [
      param('p1', 'Angle', 0, 360, 1, 'deg', 0),
      param('p2', 'Blend', 0, 1, 0.01, '', 1),
    ], [
      'let a = params.p1 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)); let t = clamp(dot(uv - vec2<f32>(0.5), d) + 0.5, 0.0, 1.0);',
      'let ramp = mix(vec3<f32>(0.02, 0.05, 0.18), vec3<f32>(1.0, 0.45, 0.12), t);',
      'return vec4<f32>(mix(src.rgb, ramp, clamp(params.p2, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_fractal_noise', 'Fractal Noise', 'Generate', [
      param('p1', 'Contrast', 0, 4, 0.05, '', 1.5),
      param('p2', 'Scale', 1, 80, 1, '', 16),
      param('p3', 'Blend', 0, 1, 0.01, '', 1),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let evo = vec2<f32>(cos(params.p4 * 0.01745329252), sin(params.p4 * 0.01745329252)) * 3.0;',
      'var n = 0.0; var amp = 0.52; var freq = max(params.p2, 1.0); var norm = 0.0;',
      'for (var i = 0; i < 5; i = i + 1) {',
      '  let p = (uv + evo * 0.015) * freq;',
      '  let ip = floor(p); let fp = fract(p); let u = fp * fp * (vec2<f32>(3.0) - 2.0 * fp);',
      '  let a = fract(sin(dot(ip, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      '  let b = fract(sin(dot(ip + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      '  let c = fract(sin(dot(ip + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      '  let d = fract(sin(dot(ip + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      '  n = n + mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * amp; norm = norm + amp; amp = amp * 0.52; freq = freq * 2.03;',
      '}',
      'let v = clamp((n / max(norm, 0.001) - 0.5) * params.p1 + 0.5, 0.0, 1.0);',
      'return vec4<f32>(mix(src.rgb, vec3<f32>(v), clamp(params.p3, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_lens_flare', 'Lens Flare', 'Generate', [
      param('p1', 'Brightness', 0, 3, 0.05, '', 1),
      param('p2', 'X', 0, 1, 0.01, '', 0.5),
      param('p3', 'Y', 0, 1, 0.01, '', 0.5),
    ], [
      'let c = vec2<f32>(params.p2, params.p3); let d = distance(uv, c); let core = 1.0 / (1.0 + d * d * 180.0);',
      'let ray = pow(abs(sin((uv.x - c.x) * 80.0) * sin((uv.y - c.y) * 80.0)), 18.0);',
      'return vec4<f32>(src.rgb + vec3<f32>(1.0, 0.78, 0.42) * (core + ray * 0.25) * params.p1, src.a);',
    ]),
    fx('shader_twirl', 'Twirl', 'Distort', [
      param('p1', 'Angle', -720, 720, 1, 'deg', 180),
      param('p2', 'Radius', 0, 1.5, 0.01, '', 0.5, true),
    ], [
      'let c = vec2<f32>(0.5); let v = uv - c; let r = length(v); let radius = max(params.p2, 0.001);',
      'let amt = (1.0 - smoothstep(0.0, radius, r)) * params.p1 * 0.01745329252; let s = sin(amt); let co = cos(amt);',
      'let suv = c + vec2<f32>(v.x * co - v.y * s, v.x * s + v.y * co);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_bulge', 'Bulge', 'Distort', [
      param('p1', 'Amount', -1, 1, 0.01, '', 0.4),
      param('p2', 'Radius', 0, 1.5, 0.01, '', 0.65, true),
    ], [
      'let c = vec2<f32>(0.5); let v = uv - c; let r = length(v); let fall = 1.0 - smoothstep(0.0, max(params.p2, 0.001), r);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(c + v * (1.0 - params.p1 * fall * 0.5), vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_wave_warp', 'Wave Warp', 'Distort', [
      param('p1', 'Height', -80, 80, 1, 'px', 16, true),
      param('p2', 'Width', 4, 300, 1, 'px', 80, true),
      param('p3', 'Direction', 0, 360, 1, 'deg', 0),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let a = params.p3 * 0.01745329252; let dir = vec2<f32>(cos(a), sin(a)); let nrm = vec2<f32>(-dir.y, dir.x);',
      'let phase = dot(uv * vec2<f32>(params.tex_w, params.tex_h), dir) / max(params.p2, 1.0) * 6.2831853 + params.p4 * 0.01745329252;',
      'let outc = textureSample(src_tex, tex_sampler, clamp(uv + nrm * sin(phase) * params.p1 * texel, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_turbulent_displace', 'Turbulent Displace', 'Distort', [
      param('p1', 'Amount', 0, 120, 1, 'px', 20, true),
      param('p2', 'Size', 1, 80, 1, '', 12),
      param('p3', 'Complexity', 1, 4, 1, '', 2),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let evo = vec2<f32>(cos(params.p4 * 0.01745329252), sin(params.p4 * 0.01745329252)) * 13.0;',
      'let scale = max(params.p2, 1.0) * 10.0;',
      'let p = (uv + evo * 0.002) * scale; let ip = floor(p); let fp = fract(p); let u = fp * fp * (vec2<f32>(3.0) - 2.0 * fp);',
      'let a = fract(sin(dot(ip, vec2<f32>(127.1, 311.7))) * 43758.5453); let b = fract(sin(dot(ip + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let c = fract(sin(dot(ip + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453); let d = fract(sin(dot(ip + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'var n1 = mix(mix(a, b, u.x), mix(c, d, u.x), u.y);',
      'let q = (uv + vec2<f32>(9.2, 4.7) - evo * 0.0015) * scale; let iq = floor(q); let fq = fract(q); let uq = fq * fq * (vec2<f32>(3.0) - 2.0 * fq);',
      'let e = fract(sin(dot(iq, vec2<f32>(269.5, 183.3))) * 43758.5453); let f = fract(sin(dot(iq + vec2<f32>(1.0, 0.0), vec2<f32>(269.5, 183.3))) * 43758.5453);',
      'let g = fract(sin(dot(iq + vec2<f32>(0.0, 1.0), vec2<f32>(269.5, 183.3))) * 43758.5453); let h = fract(sin(dot(iq + vec2<f32>(1.0, 1.0), vec2<f32>(269.5, 183.3))) * 43758.5453);',
      'var n2 = mix(mix(e, f, uq.x), mix(g, h, uq.x), uq.y);',
      'if (params.p3 > 1.5) { let detail = sin((uv.x + uv.y) * scale * 3.7 + params.p4 * 0.01745329252) * 0.12; n1 = clamp(n1 + detail, 0.0, 1.0); }',
      'let outc = textureSample(src_tex, tex_sampler, clamp(uv + (vec2<f32>(n1, n2) - vec2<f32>(0.5)) * params.p1 * texel, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_optics_compensation', 'Optics Compensation', 'Distort', [
      param('p1', 'FOV', -1, 1, 0.01, '', 0.25),
    ], [
      'let c = vec2<f32>(0.5); let v = uv - c; let suv = c + v * (1.0 + params.p1 * dot(v, v) * 1.8);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_luma_key', 'Luma Key', 'Keying', [
      param('p1', 'Threshold', 0, 1, 0.01, '', 0.5),
      param('p2', 'Softness', 0, 1, 0.01, '', 0.1),
      param('p3', 'Invert', 0, 1, 1, '', 0),
    ], [
      'let l = dot(src.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'var m = smoothstep(params.p1 - params.p2, params.p1 + params.p2, l); m = select(m, 1.0 - m, params.p3 > 0.5);',
      'let a = src.a * m;',
      'return vec4<f32>(src.rgb * select(0.0, a / max(src.a, 0.001), src.a > 0.001), a);',
    ]),
    fx('shader_color_key', 'Color Key', 'Keying', [
      param('p1', 'Hue', 0, 360, 1, 'deg', 120),
      param('p2', 'Tolerance', 0, 1, 0.01, '', 0.08),
      param('p3', 'Softness', 0, 1, 0.01, '', 0.08),
    ], [
      'let h = params.p1 * 0.01745329252;',
      'let key_color = clamp(vec3<f32>(0.5 + 0.5 * cos(h), 0.5 + 0.5 * cos(h - 2.0943951), 0.5 + 0.5 * cos(h + 2.0943951)), vec3<f32>(0.0), vec3<f32>(1.0));',
      'let d = distance(normalize(src.rgb + vec3<f32>(0.001)), normalize(key_color + vec3<f32>(0.001)));',
      'let keep = smoothstep(params.p2, params.p2 + params.p3 + 0.001, d);',
      'let a = src.a * keep;',
      'return vec4<f32>(src.rgb * select(0.0, a / max(src.a, 0.001), src.a > 0.001), a);',
    ]),
    fx('shader_photo_filter', 'Photo Filter', 'Color', [
      param('p1', 'Density', 0, 1, 0.01, '', 0.35),
      param('p2', 'Warmth', -1, 1, 0.01, '', 0.4),
    ], [
      'let warm = vec3<f32>(1.0, 0.68, 0.34); let cool = vec3<f32>(0.35, 0.62, 1.0);',
      'let tint = mix(cool, warm, clamp(params.p2 * 0.5 + 0.5, 0.0, 1.0));',
      'return vec4<f32>(mix(src.rgb, src.rgb * tint * 1.25, clamp(params.p1, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_black_white_mix', 'Black & White Mix', 'Color', [
      param('p1', 'Red weight', -1, 2, 0.01, '', 0.3),
      param('p2', 'Green weight', -1, 2, 0.01, '', 0.59),
      param('p3', 'Blue weight', -1, 2, 0.01, '', 0.11),
      param('p4', 'Blend', 0, 1, 0.01, '', 1),
    ], [
      'let w = vec3<f32>(params.p1, params.p2, params.p3);',
      'let l = clamp(dot(src.rgb, w) / max(dot(abs(w), vec3<f32>(1.0)), 0.001), 0.0, 1.0);',
      'return vec4<f32>(mix(src.rgb, vec3<f32>(l), clamp(params.p4, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_vibrance', 'Vibrance', 'Color', [
      param('p1', 'Amount', -1, 2, 0.01, '', 0.5),
    ], [
      'let mx = max(max(src.r, src.g), src.b); let mn = min(min(src.r, src.g), src.b);',
      'let sat = mx - mn; let l = dot(src.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'let boosted = mix(vec3<f32>(l), src.rgb, 1.0 + params.p1 * (1.0 - sat));',
      'return vec4<f32>(clamp(boosted, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);',
    ]),
    fx('shader_colorama', 'Color Map', 'Color', [
      param('p1', 'Phase', -360, 360, 1, 'deg', 0),
      param('p2', 'Intensity', 0, 1, 0.01, '', 1),
    ], [
      'let l = dot(src.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      'let h = l * 6.2831853 + params.p1 * 0.01745329252;',
      'let mapped = 0.5 + 0.5 * cos(vec3<f32>(h, h - 2.0943951, h + 2.0943951));',
      'return vec4<f32>(mix(src.rgb, mapped, clamp(params.p2, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_directional_blur', 'Directional Blur', 'Blur', [
      param('p1', 'Length', 0, 160, 1, 'px', 24, true),
      param('p2', 'Direction', 0, 360, 1, 'deg', 0),
    ], [
      'let a = params.p2 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)) * texel * params.p1;',
      'var acc = vec4<f32>(0.0); var wsum = 0.0;',
      'for (var i = -16; i <= 16; i = i + 1) {',
      '  let x = f32(i) / 16.0;',
      '  let w = exp(-x * x * 2.35);',
      '  let smp = textureSample(src_tex, tex_sampler, clamp(uv + d * x, vec2<f32>(0.0), vec2<f32>(1.0)));',
      '  acc = acc + vec4<f32>(smp.rgb * smp.a, smp.a) * w;',
      '  wsum = wsum + w;',
      '}',
      'let outc = acc / max(wsum, 0.001);',
      'return vec4<f32>(outc.rgb / max(outc.a, 0.001), outc.a);',
    ]),
    fx('shader_radial_blur', 'Radial Blur', 'Blur', [
      param('p1', 'Amount', 0, 1, 0.01, '', 0.35),
      param('p2', 'Center X', 0, 1, 0.01, '', 0.5),
      param('p3', 'Center Y', 0, 1, 0.01, '', 0.5),
    ], [
      'let c = vec2<f32>(params.p2, params.p3); let v = uv - c;',
      'var acc = vec4<f32>(0.0); var wsum = 0.0;',
      'for (var i = 0; i < 17; i = i + 1) {',
      '  let t = f32(i) / 16.0;',
      '  let w = exp(-t * t * 2.0);',
      '  let smp = textureSample(src_tex, tex_sampler, clamp(c + v * (1.0 - params.p1 * t), vec2<f32>(0.0), vec2<f32>(1.0)));',
      '  acc = acc + vec4<f32>(smp.rgb * smp.a, smp.a) * w;',
      '  wsum = wsum + w;',
      '}',
      'let outc = acc / max(wsum, 0.001);',
      'return vec4<f32>(outc.rgb / max(outc.a, 0.001), outc.a);',
    ]),
    fx('shader_soft_directional_blur', 'Soft Directional Blur', 'Blur', [
      param('p1', 'Length', 0, 240, 1, 'px', 48, true),
      param('p2', 'Direction', 0, 360, 1, 'deg', 0),
      param('p3', 'Softness', 0.2, 3, 0.05, '', 1.4),
    ], [
      'let a = params.p2 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)) * texel * params.p1;',
      'var acc = vec4<f32>(0.0); var wsum = 0.0;',
      'for (var i = -16; i <= 16; i = i + 1) {',
      '  let x = f32(i) / 16.0;',
      '  let w = exp(-x * x * (1.35 / max(params.p3, 0.2)));',
      '  let smp = textureSample(src_tex, tex_sampler, clamp(uv + d * x, vec2<f32>(0.0), vec2<f32>(1.0)));',
      '  acc = acc + vec4<f32>(smp.rgb * smp.a, smp.a) * w;',
      '  wsum = wsum + w;',
      '}',
      'let outc = acc / max(wsum, 0.001);',
      'return vec4<f32>(outc.rgb / max(outc.a, 0.001), outc.a);',
    ]),
    fx('shader_four_color_gradient', '4-Color Gradient', 'Generate', [
      param('p1', 'Hue offset', -360, 360, 1, 'deg', 0),
      param('p2', 'Blend', 0, 1, 0.01, '', 1),
    ], [
      'let h = params.p1 * 0.01745329252;',
      'let c1 = 0.5 + 0.5 * cos(vec3<f32>(h, h - 2.0943951, h + 2.0943951));',
      'let c2 = 0.5 + 0.5 * cos(vec3<f32>(h + 1.7, h - 0.4, h + 3.4));',
      'let c3 = 0.5 + 0.5 * cos(vec3<f32>(h + 3.1, h + 1.0, h - 1.2));',
      'let c4 = 0.5 + 0.5 * cos(vec3<f32>(h + 4.6, h + 2.5, h + 0.4));',
      'let top = mix(c1, c2, uv.x); let bot = mix(c3, c4, uv.x);',
      'return vec4<f32>(mix(src.rgb, mix(top, bot, uv.y), clamp(params.p2, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_lightning', 'Lightning', 'Generate', [
      param('p1', 'Intensity', 0, 4, 0.05, '', 1.5),
      param('p2', 'Jaggedness', 0, 1, 0.01, '', 0.45),
      param('p3', 'Width', 0.001, 0.08, 0.001, '', 0.015, true),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let y = uv.y; let evo = params.p4 * 0.01745329252;',
      'let trunk = sin(y * 24.0 + evo) * 0.09 + sin(y * 73.0 - evo * 1.7) * 0.035 + sin(y * 151.0 + evo * 0.6) * 0.018;',
      'let x = 0.5 + trunk * params.p2;',
      'let dist = abs(uv.x - x);',
      'let core = exp(-dist * dist / max(params.p3 * params.p3, 0.000001));',
      'let glow = exp(-dist * dist / max(params.p3 * params.p3 * 24.0, 0.000001));',
      'let branch1 = exp(-abs((uv.x - x) - (uv.y - 0.35) * 0.55) * 70.0) * smoothstep(0.18, 0.38, uv.y) * (1.0 - smoothstep(0.38, 0.58, uv.y));',
      'let branch2 = exp(-abs((uv.x - x) + (uv.y - 0.62) * 0.42) * 75.0) * smoothstep(0.48, 0.65, uv.y) * (1.0 - smoothstep(0.65, 0.82, uv.y));',
      'let bolt = vec3<f32>(0.35, 0.62, 1.0) * glow * 0.45 + vec3<f32>(1.0) * core + vec3<f32>(0.55, 0.78, 1.0) * (branch1 + branch2) * params.p2;',
      'return vec4<f32>(src.rgb + bolt * params.p1, src.a);',
    ]),
    fx('shader_radio_waves', 'Radio Waves', 'Generate', [
      param('p1', 'Frequency', 1, 40, 1, '', 12),
      param('p2', 'Width', 0.001, 0.08, 0.001, '', 0.015),
      param('p3', 'Opacity', 0, 1, 0.01, '', 0.8),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let r = distance(uv, vec2<f32>(0.5));',
      'let phase = fract(r * params.p1 - params.p4 / 360.0);',
      'let ring = exp(-pow(abs(phase - 0.5) / max(params.p2, 0.001), 2.0));',
      'let fade = 1.0 - smoothstep(0.25, 0.75, r);',
      'return vec4<f32>(src.rgb + vec3<f32>(0.55, 0.8, 1.0) * ring * fade * params.p3, src.a);',
    ]),
    fx('shader_particle_field', 'Particle Field', 'Particles', [
      param('p1', 'Count', 4, 80, 1, '', 32),
      param('p2', 'Size', 0.001, 0.05, 0.001, '', 0.012, true),
      param('p3', 'Brightness', 0, 4, 0.05, '', 1.5),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let grid = max(params.p1, 1.0); let cell = floor(uv * grid); let f = fract(uv * grid);',
      'let seed = fract(sin(dot(cell, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let drift = vec2<f32>(sin(seed * 40.0 + params.p4 * 0.01745329252), cos(seed * 31.0 + params.p4 * 0.013)) * 0.26;',
      'let p = fract(vec2<f32>(seed, fract(seed * 17.13)) + drift);',
      'let d = distance(f, p);',
      'let core = exp(-d * d / max(params.p2 * params.p2 * grid * grid, 0.00001));',
      'let halo = exp(-d * d / max(params.p2 * params.p2 * grid * grid * 9.0, 0.00001));',
      'let sparkle = pow(max(0.0, sin(seed * 90.0 + params.p4 * 0.029)), 4.0);',
      'return vec4<f32>(src.rgb + (vec3<f32>(1.0, 0.86, 0.55) * core + vec3<f32>(0.45, 0.7, 1.0) * halo * 0.35) * params.p3 * (0.65 + sparkle), src.a);',
    ]),
    fx('shader_particle_streaks', 'Particle Streaks', 'Particles', [
      param('p1', 'Density', 4, 80, 1, '', 28),
      param('p2', 'Length', 0.01, 0.4, 0.01, '', 0.14),
      param('p3', 'Brightness', 0, 4, 0.05, '', 1.2),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let grid = max(params.p1, 1.0); let pos = uv * grid; let cell = floor(pos); let f = fract(pos);',
      'let seed = fract(sin(dot(cell, vec2<f32>(91.7, 271.3))) * 43758.5453);',
      'let y = fract(seed + params.p4 / 360.0);',
      'let dx = abs(f.x - seed); let dy = abs(f.y - y);',
      'let head = exp(-(dx * dx + dy * dy) / 0.004);',
      'let tail = exp(-dx * dx / 0.003) * exp(-max(f.y - y, 0.0) / max(params.p2, 0.001)) * select(0.0, 1.0, f.y > y);',
      'return vec4<f32>(src.rgb + (vec3<f32>(1.0) * head + vec3<f32>(0.55, 0.78, 1.0) * tail * 0.8) * params.p3, src.a);',
    ]),
    fx('shader_particle_vortex', 'Particle Vortex', 'Particles', [
      param('p1', 'Count', 8, 120, 1, '', 48),
      param('p2', 'Radius', 0.05, 0.8, 0.01, '', 0.45, true),
      param('p3', 'Brightness', 0, 4, 0.05, '', 1.4),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let c = vec2<f32>(0.5); let v = uv - c; let r = length(v); let ang = atan2(v.y, v.x) + params.p4 * 0.01745329252;',
      'let arms = sin(ang * 5.0 + r * params.p1);',
      'let ring = 1.0 - smoothstep(0.0, 0.025, abs(r - params.p2 * (0.55 + 0.35 * arms)));',
      'let fade = 1.0 - smoothstep(params.p2, params.p2 + 0.2, r);',
      'return vec4<f32>(src.rgb + vec3<f32>(1.0, 0.55, 0.22) * ring * fade * params.p3, src.a);',
    ]),
    fx('shader_particle_burst', 'Particle Burst', 'Particles', [
      param('p1', 'Count', 8, 120, 1, '', 56),
      param('p2', 'Spread', 0.05, 1, 0.01, '', 0.55, true),
      param('p3', 'Brightness', 0, 4, 0.05, '', 1.6),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let c = vec2<f32>(0.5); let v = uv - c; let r = length(v); let a = atan2(v.y, v.x);',
      'let spokes = abs(sin(a * params.p1 * 0.5 + params.p4 * 0.01745329252));',
      'let front = fract(params.p4 / 360.0);',
      'let shell = 1.0 - smoothstep(0.0, 0.04, abs(r - front * params.p2));',
      'let spark = pow(spokes, 24.0) * shell * (1.0 - smoothstep(params.p2, params.p2 + 0.2, r));',
      'return vec4<f32>(src.rgb + vec3<f32>(1.0, 0.78, 0.35) * spark * params.p3, src.a);',
    ]),
    fx('shader_cinematic_glow', 'Cinematic Glow', 'Stylize', [
      param('p1', 'Threshold', 0, 1, 0.01, '', 0.55),
      param('p2', 'Radius', 1, 80, 1, 'px', 28, true),
      param('p3', 'Intensity', 0, 4, 0.05, '', 1.4),
      param('p4', 'Warmth', -1, 1, 0.01, '', 0.25),
    ], [
      'var acc = vec3<f32>(0.0); var wsum = 0.0;',
      'for (var i = -5; i <= 5; i = i + 1) { for (var j = -5; j <= 5; j = j + 1) {',
      '  let off = vec2<f32>(f32(i), f32(j)) / 5.0 * params.p2 * texel;',
      '  let smp = textureSample(src_tex, tex_sampler, clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0)));',
      '  let l = dot(smp.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));',
      '  let b = smoothstep(params.p1, 1.0, l);',
      '  let w = exp(-dot(vec2<f32>(f32(i), f32(j)), vec2<f32>(f32(i), f32(j))) / 16.0);',
      '  acc = acc + smp.rgb * smp.a * b * w; wsum = wsum + w;',
      '} }',
      'let warm = mix(vec3<f32>(0.65, 0.78, 1.0), vec3<f32>(1.0, 0.76, 0.42), clamp(params.p4 * 0.5 + 0.5, 0.0, 1.0));',
      'let bloom = acc / max(wsum, 0.001) * warm * params.p3;',
      'return vec4<f32>(src.rgb + bloom, src.a);',
    ]),
    fx('shader_heat_haze', 'Heat Haze', 'Distort', [
      param('p1', 'Amount', 0, 80, 1, 'px', 18, true),
      param('p2', 'Scale', 1, 80, 1, '', 22),
      param('p3', 'Vertical Bias', -1, 1, 0.01, '', 0.35),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let evo = params.p4 * 0.01745329252;',
      'let n = sin(uv.y * params.p2 * 9.0 + evo) * 0.55 + sin((uv.y + uv.x * 0.3) * params.p2 * 17.0 - evo * 1.37) * 0.28 + sin(uv.x * params.p2 * 5.0 + evo * 0.7) * 0.17;',
      'let lift = smoothstep(1.0, 0.0, uv.y) * (0.6 + params.p3 * 0.4);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(uv + vec2<f32>(n, n * 0.22) * params.p1 * texel * lift, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_caustics', 'Caustics', 'Generate', [
      param('p1', 'Scale', 4, 80, 1, '', 22),
      param('p2', 'Intensity', 0, 4, 0.05, '', 1.2),
      param('p3', 'Softness', 0, 1, 0.01, '', 0.4),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let t = params.p4 * 0.01745329252;',
      'let p = (uv - vec2<f32>(0.5)) * params.p1;',
      'let v = sin(p.x * 1.7 + sin(p.y * 1.3 + t) + t) + sin(p.y * 2.1 + cos(p.x * 1.1 - t) - t * 0.7);',
      'let lines = pow(smoothstep(1.25 - params.p3, 1.85, v), 2.0);',
      'let water = vec3<f32>(0.25, 0.78, 1.0) * lines * params.p2;',
      'return vec4<f32>(src.rgb + water, src.a);',
    ]),
    fx('shader_plexus_field', 'Plexus Field', 'Particles', [
      param('p1', 'Density', 4, 48, 1, '', 18),
      param('p2', 'Point Size', 0.001, 0.06, 0.001, '', 0.012, true),
      param('p3', 'Lines', 0, 1, 0.01, '', 0.65),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let grid = max(params.p1, 1.0); let pos = uv * grid; let cell = floor(pos); let f = fract(pos);',
      'var glow = 0.0; var line = 0.0;',
      'for (var ox = -1; ox <= 1; ox = ox + 1) { for (var oy = -1; oy <= 1; oy = oy + 1) {',
      '  let nc = cell + vec2<f32>(f32(ox), f32(oy)); let seed = fract(sin(dot(nc, vec2<f32>(137.7, 247.3))) * 43758.5453);',
      '  let p = vec2<f32>(seed, fract(seed * 19.19)) + vec2<f32>(sin(seed * 33.0 + params.p4 * 0.017), cos(seed * 21.0 - params.p4 * 0.013)) * 0.22;',
      '  let lp = p + vec2<f32>(f32(ox), f32(oy)); let d = distance(f, lp);',
      '  glow = glow + exp(-d * d / max(params.p2 * params.p2 * grid * grid, 0.00001));',
      '  line = line + exp(-abs(d - 0.34) * 24.0) * smoothstep(0.55, 0.0, d);',
      '} }',
      'let col = vec3<f32>(0.42, 0.75, 1.0) * glow + vec3<f32>(0.32, 0.55, 1.0) * line * params.p3;',
      'return vec4<f32>(src.rgb + col, src.a);',
    ]),
    fx('shader_depth_starfield', 'Depth Starfield', 'Particles', [
      param('p1', 'Density', 8, 120, 1, '', 54),
      param('p2', 'Speed', -4, 4, 0.05, '', 1),
      param('p3', 'Brightness', 0, 4, 0.05, '', 1.5),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let c = vec2<f32>(0.5); let v = uv - c; var col = vec3<f32>(0.0);',
      'for (var layer = 1; layer <= 4; layer = layer + 1) {',
      '  let z = f32(layer); let scale = params.p1 / z;',
      '  let tuv = c + v * (1.0 + fract(params.p4 / 360.0 * params.p2 + z * 0.23) * 1.8);',
      '  let cell = floor(tuv * scale); let f = fract(tuv * scale); let seed = fract(sin(dot(cell, vec2<f32>(91.1 + z, 311.7))) * 43758.5453);',
      '  let p = vec2<f32>(seed, fract(seed * 13.7)); let d = distance(f, p); let star = exp(-d * d / 0.0025) * step(0.82, seed);',
      '  col = col + vec3<f32>(0.7 + seed * 0.3, 0.82, 1.0) * star / z;',
      '}',
      'return vec4<f32>(src.rgb + col * params.p3, src.a);',
    ]),
    fx('shader_chroma_smear', 'Chroma Smear', 'Stylize', [
      param('p1', 'Amount', 0, 80, 1, 'px', 20, true),
      param('p2', 'Direction', 0, 360, 1, 'deg', 0),
      param('p3', 'Color Split', 0, 2, 0.01, '', 0.8),
    ], [
      'let a = params.p2 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)) * texel * params.p1;',
      'let base = textureSample(src_tex, tex_sampler, uv);',
      'let r = textureSample(src_tex, tex_sampler, clamp(uv + d * params.p3, vec2<f32>(0.0), vec2<f32>(1.0))).r;',
      'let b = textureSample(src_tex, tex_sampler, clamp(uv - d * params.p3, vec2<f32>(0.0), vec2<f32>(1.0))).b;',
      'let smear = textureSample(src_tex, tex_sampler, clamp(uv - d * 0.45, vec2<f32>(0.0), vec2<f32>(1.0))).rgb * 0.35 + textureSample(src_tex, tex_sampler, clamp(uv - d * 0.9, vec2<f32>(0.0), vec2<f32>(1.0))).rgb * 0.2;',
      'return vec4<f32>(vec3<f32>(r, base.g, b) + smear, base.a);',
    ]),
    fx('shader_mirror', 'Mirror', 'Distort', [
      param('p1', 'Axis', 0, 360, 1, 'deg', 0),
      param('p2', 'Position', 0, 1, 0.01, '', 0.5),
    ], [
      'let a = params.p1 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)); let q = uv - vec2<f32>(0.5);',
      'let side = dot(q, d); let muv = uv - d * side * 2.0 * select(0.0, 1.0, side > params.p2 - 0.5);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(muv, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_mosaic', 'Mosaic', 'Stylize', [
      param('p1', 'Horizontal Tiles', 2, 200, 1, '', 32),
      param('p2', 'Vertical Tiles', 2, 200, 1, '', 32),
      param('p3', 'Feather', 0, 1, 0.01, '', 0),
    ], [
      'let tiles = vec2<f32>(max(params.p1, 2.0), max(params.p2, 2.0)); let cell = floor(uv * tiles) / tiles + 0.5 / tiles;',
      'let outc = textureSample(src_tex, tex_sampler, cell); return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_ripple', 'Ripple', 'Distort', [
      param('p1', 'Amplitude', 0, 80, 1, 'px', 12, true),
      param('p2', 'Frequency', 1, 80, 1, '', 24),
      param('p3', 'Phase', -3600, 3600, 1, 'deg', 0),
      param('p4', 'Direction', 0, 360, 1, 'deg', 90),
    ], [
      'let a = params.p4 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)); let phase = params.p3 * 0.01745329252;',
      'let wave = sin(dot(uv - vec2<f32>(0.5), d) * params.p2 * 6.28318 + phase) * params.p1 * texel;',
      'let outc = textureSample(src_tex, tex_sampler, clamp(uv + vec2<f32>(-d.y, d.x) * wave, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_spherize', 'Spherize', 'Distort', [
      param('p1', 'Amount', -1, 1, 0.01, '', 0.65),
      param('p2', 'Radius', 0.05, 1, 0.01, '', 0.5, true),
    ], [
      'let c = vec2<f32>(0.5); let v = uv - c; let r = length(v); let radius = max(params.p2, 0.01);',
      'let inside = smoothstep(radius, radius - 0.01, r); let nr = r * (1.0 - params.p1 * (1.0 - smoothstep(0.0, radius, r)) * 0.42);',
      'let suv = c + normalize(v + vec2<f32>(0.00001)) * nr * inside + v * (1.0 - inside); let outc = textureSample(src_tex, tex_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_displacement', 'Displacement', 'Distort', [
      param('p1', 'Amount', 0, 120, 1, 'px', 18, true),
      param('p2', 'Scale', 2, 80, 1, '', 18),
      param('p3', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let t = params.p3 * 0.01745329252; let n1 = sin(uv.y * params.p2 + t) * 0.6 + sin(uv.y * params.p2 * 2.7 - t * 1.4) * 0.3;',
      'let n2 = sin(uv.x * params.p2 * 1.3 - t * 0.8) * 0.6 + sin(uv.x * params.p2 * 3.1 + t) * 0.3;',
      'let outc = textureSample(src_tex, tex_sampler, clamp(uv + vec2<f32>(n1, n2) * params.p1 * texel, vec2<f32>(0.0), vec2<f32>(1.0))); return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_fill', 'Fill Color', 'Color', [
      param('p1', 'Amount', 0, 1, 0.01, '', 1),
      param('p2', 'Hue', 0, 360, 1, 'deg', 210),
      param('p3', 'Brightness', 0, 1, 0.01, '', 0.75),
    ], [
      'let h = params.p2 * 0.01745329252; let fill = vec3<f32>(0.5 + 0.5 * cos(h), 0.5 + 0.5 * cos(h - 2.094), 0.5 + 0.5 * cos(h + 2.094)) * params.p3;',
      'return vec4<f32>(mix(src.rgb, fill, clamp(params.p1, 0.0, 1.0)), src.a);',
    ]),
    fx('shader_color_balance', 'Color Balance', 'Color', [
      param('p1', 'Shadows', -1, 1, 0.01, '', 0),
      param('p2', 'Midtones', -1, 1, 0.01, '', 0),
      param('p3', 'Highlights', -1, 1, 0.01, '', 0),
    ], [
      'let l = dot(src.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)); let sh = smoothstep(0.7, 0.05, l); let hi = smoothstep(0.3, 0.95, l);',
      'let warm = vec3<f32>(1.0, 0.38, -0.28); let rgb = src.rgb + warm * (params.p1 * sh + params.p2 * (1.0 - sh) * (1.0 - hi) + params.p3 * hi) * 0.22;',
      'return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);',
    ]),
    fx('shader_radial_wipe', 'Radial Wipe', 'Keying', [
      param('p1', 'Completion', 0, 1, 0.01, '', 0.5),
      param('p2', 'Angle', -360, 360, 1, 'deg', -90),
      param('p3', 'Softness', 0, 0.5, 0.01, '', 0.03),
    ], [
      'let a = atan2(uv.y - 0.5, uv.x - 0.5) - params.p2 * 0.01745329252; let turn = fract(a / 6.28318 + 1.0);',
      'let mask = smoothstep(params.p1, params.p1 - max(params.p3, 0.001), turn); return vec4<f32>(src.rgb, src.a * (1.0 - mask));',
    ]),
    fx('shader_gradient_wipe', 'Gradient Wipe', 'Keying', [
      param('p1', 'Completion', 0, 1, 0.01, '', 0.5),
      param('p2', 'Angle', 0, 360, 1, 'deg', 0),
      param('p3', 'Softness', 0, 1, 0.01, '', 0.12, true),
    ], [
      'let a = params.p2 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)); let g = dot(uv - vec2<f32>(0.5), d) + 0.5;',
      'let mask = smoothstep(params.p1, params.p1 - max(params.p3, 0.001), g); return vec4<f32>(src.rgb, src.a * (1.0 - mask));',
    ]),
    fx('shader_drop_shadow', 'Drop Shadow', 'Stylize', [
      param('p1', 'Distance', 0, 160, 1, 'px', 18, true),
      param('p2', 'Direction', 0, 360, 1, 'deg', 135),
      param('p3', 'Softness', 0, 80, 1, 'px', 16, true),
      param('p4', 'Opacity', 0, 100, 1, '%', 55),
    ], [
      'let a = params.p2 * 0.01745329252; let dir = vec2<f32>(cos(a), sin(a)); let off = dir * params.p1 * texel;',
      'let spread = max(params.p3, 0.5) * texel; var sa = 0.0; var sw = 0.0;',
      'for (var i = 0; i < 9; i = i + 1) { let ox = f32(i % 3) - 1.0; let oy = f32(i / 3) - 1.0; let w = select(1.0, 2.0, i == 4); sa = sa + textureSample(src_tex, tex_sampler, clamp(uv - off + vec2<f32>(ox, oy) * spread, vec2<f32>(0.0), vec2<f32>(1.0))).a * w; sw = sw + w; }',
      'let shadow = sa / max(sw, 0.001) * clamp(params.p4 * 0.01, 0.0, 1.0) * (1.0 - src.a); let oa = src.a + shadow * (1.0 - src.a); let rgb = (src.rgb * src.a + vec3<f32>(0.01, 0.012, 0.02) * shadow) / max(oa, 0.001);',
      'return vec4<f32>(rgb, oa);',
    ]),
    fx('shader_inner_shadow', 'Inner Shadow', 'Stylize', [
      param('p1', 'Distance', 0, 100, 1, 'px', 12, true),
      param('p2', 'Direction', 0, 360, 1, 'deg', 135),
      param('p3', 'Softness', 0, 60, 1, 'px', 12, true),
      param('p4', 'Opacity', 0, 100, 1, '%', 60),
    ], [
      'let a = params.p2 * 0.01745329252; let dir = vec2<f32>(cos(a), sin(a)); let off = dir * params.p1 * texel; let edge = textureSample(src_tex, tex_sampler, clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0))).a;',
      'let softness = max(params.p3, 0.5) * texel; var outside = 0.0;',
      'for (var i = 0; i < 5; i = i + 1) { let t = (f32(i) - 2.0) * 0.5; outside = outside + textureSample(src_tex, tex_sampler, clamp(uv + off + vec2<f32>(t) * softness, vec2<f32>(0.0), vec2<f32>(1.0))).a; }',
      'let shadow = clamp(src.a - outside / 5.0, 0.0, 1.0) * clamp(params.p4 * 0.01, 0.0, 1.0); return vec4<f32>(src.rgb * (1.0 - shadow * 0.82), src.a);',
    ]),
    fx('shader_bevel', 'Bevel & Relief', 'Stylize', [
      param('p1', 'Size', 1, 40, 1, 'px', 8, true),
      param('p2', 'Angle', 0, 360, 1, 'deg', 135),
      param('p3', 'Strength', 0, 200, 1, '%', 85),
      param('p4', 'Softness', 0, 1, 0.01, '', 0.35),
    ], [
      'let a = params.p2 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)) * params.p1 * texel; let hi = textureSample(src_tex, tex_sampler, uv - d).a; let lo = textureSample(src_tex, tex_sampler, uv + d).a;',
      'let edge = (hi - lo) * clamp(params.p3 * 0.01, 0.0, 2.0); let soft = 1.0 - clamp(params.p4, 0.0, 1.0) * 0.55; let rgb = src.rgb + vec3<f32>(edge * 0.55 * soft) + vec3<f32>(0.12, 0.16, 0.22) * max(edge, 0.0) * soft;',
      'return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);',
    ]),
    fx('shader_soft_outline', 'Soft Outline', 'Stylize', [
      param('p1', 'Thickness', 1, 40, 1, 'px', 8, true),
      param('p2', 'Brightness', 0, 200, 1, '%', 100),
      param('p3', 'Opacity', 0, 100, 1, '%', 75),
    ], [
      'let r = max(params.p1, 1.0) * texel; var ring = 0.0;',
      'for (var i = 0; i < 12; i = i + 1) { let a = f32(i) * 0.5235988; ring = max(ring, textureSample(src_tex, tex_sampler, clamp(uv + vec2<f32>(cos(a), sin(a)) * r, vec2<f32>(0.0), vec2<f32>(1.0))).a); }',
      'let outline = clamp(ring - src.a, 0.0, 1.0) * clamp(params.p3 * 0.01, 0.0, 1.0); let oa = src.a + outline * (1.0 - src.a); let rgb = (src.rgb * src.a + vec3<f32>(1.0) * outline * (1.0 - src.a) * params.p2 * 0.01) / max(oa, 0.001);',
      'return vec4<f32>(rgb, oa);',
    ]),
    fx('shader_soft_light', 'Soft Light', 'Color', [
      param('p1', 'Amount', 0, 100, 1, '%', 45),
      param('p2', 'Warmth', -100, 100, 1, '%', 0),
    ], [
      'let l = dot(src.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)); let glow = smoothstep(0.25, 0.85, l) * 0.22; let warm = vec3<f32>(params.p2 * 0.0018, 0.0, -params.p2 * 0.0018);',
      'let lit = src.rgb + (vec3<f32>(l) - src.rgb) * (-glow) + vec3<f32>(glow) + warm; return vec4<f32>(mix(src.rgb, clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0)), params.p1 * 0.01), src.a);',
    ]),
    fx('shader_kaleidoscope', 'Kaleidoscope', 'Distort', [
      param('p1', 'Segments', 2, 24, 1, '', 6),
      param('p2', 'Rotation', -360, 360, 1, 'deg', 0),
      param('p3', 'Center X', 0, 1, 0.01, '', 0.5),
      param('p4', 'Center Y', 0, 1, 0.01, '', 0.5),
    ], [
      'let c = vec2<f32>(params.p3, params.p4); let v = uv - c; let r = length(v); let ang = atan2(v.y, v.x) + params.p2 * 0.01745329252; let seg = 6.2831853 / max(params.p1, 2.0); let folded = abs(fract(ang / seg + 0.5) - 0.5) * seg;',
      'let suv = c + vec2<f32>(cos(folded), sin(folded)) * r; let outc = textureSample(src_tex, tex_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0))); return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_polar_coordinates', 'Polar Coordinates', 'Distort', [
      param('p1', 'Mix', 0, 1, 0.01, '', 1),
      param('p2', 'Rotation', -360, 360, 1, 'deg', 0),
      param('p3', 'Radius', 0.1, 2, 0.01, '', 1),
    ], [
      'let q = uv - vec2<f32>(0.5); let r = length(q) * params.p3; let a = atan2(q.y, q.x) / 6.2831853 + 0.5 + params.p2 * 0.0027777778; let polar = vec2<f32>(fract(a), clamp(r, 0.0, 1.0));',
      'let outc = textureSample(src_tex, tex_sampler, polar); let rgb = mix(src.rgb, outc.rgb, clamp(params.p1, 0.0, 1.0)); return vec4<f32>(rgb, mix(src.a, outc.a, clamp(params.p1, 0.0, 1.0)));',
    ]),
    fx('shader_motion_tile', 'Motion Tile', 'Distort', [
      param('p1', 'Offset X', -2, 2, 0.01, '', 0),
      param('p2', 'Offset Y', -2, 2, 0.01, '', 0),
      param('p3', 'Scale', 0.25, 4, 0.01, '', 1),
      param('p4', 'Mirror', 0, 1, 1, '', 1),
    ], [
      'let q = (uv - vec2<f32>(0.5)) * params.p3 + vec2<f32>(0.5 + params.p1, 0.5 + params.p2); let cell = floor(q); let f = fract(q); let parity = fract((cell.x + cell.y) * 0.5) * 2.0; let mirrored = select(f, 1.0 - f, parity > 0.5 && params.p4 > 0.5);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(mirrored, vec2<f32>(0.0), vec2<f32>(1.0))); return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_light_sweep', 'Light Sweep', 'Stylize', [
      param('p1', 'Position', -1, 2, 0.01, '', 0.5),
      param('p2', 'Width', 0.01, 1, 0.01, '', 0.18, true),
      param('p3', 'Angle', -180, 180, 1, 'deg', 25),
      param('p4', 'Intensity', 0, 3, 0.05, '', 1.2),
    ], [
      'let a = params.p3 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)); let t = dot(uv - vec2<f32>(0.5), d) + 0.5 - params.p1; let band = exp(-(t * t) / max(params.p2 * params.p2, 0.0001));',
      'let tint = vec3<f32>(1.0, 0.82, 0.48) * band * params.p4; return vec4<f32>(clamp(src.rgb + tint, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);',
    ]),
    fx('shader_color_dodge', 'Color Dodge', 'Color', [
      param('p1', 'Amount', 0, 100, 1, '%', 45),
      param('p2', 'Exposure', 0, 3, 0.05, '', 0.7),
    ], [
      'let amt = clamp(params.p1 * 0.01, 0.0, 1.0); let denom = max(vec3<f32>(1.0) - src.rgb, vec3<f32>(0.02)); let dodge = clamp(src.rgb / denom * (1.0 + params.p2 * 0.18), vec3<f32>(0.0), vec3<f32>(1.0));',
      'return vec4<f32>(mix(src.rgb, dodge, amt), src.a);',
    ]),
    fx('shader_difference', 'Difference', 'Color', [
      param('p1', 'Amount', 0, 100, 1, '%', 65),
      param('p2', 'Hue Shift', -180, 180, 1, 'deg', 0),
    ], [
      'let h = params.p2 * 0.01745329252; let tint = 0.5 + 0.5 * cos(vec3<f32>(h, h - 2.094, h + 2.094)); let diff = abs(src.rgb - tint);',
      'return vec4<f32>(mix(src.rgb, diff, clamp(params.p1 * 0.01, 0.0, 1.0)), src.a);',
    ]),
  ];

  window.SMSHADER_EFFECT_CATEGORIES = ['Color', 'Blur', 'Stylize', 'Generate', 'Distort', 'Keying', 'Particles'];
  window.SMSHADER_EFFECTS = effects;
  window.SMShaderEffectDef = function (id) {
    for (var i = 0; i < effects.length; i++) if (effects[i].id === id) return effects[i];
    return null;
  };
})();
