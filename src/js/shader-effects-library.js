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
      'let cell = floor(local_uv * max(params.p2, 1.0) * 80.0);',
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
    // Port of the public-domain "Film Grain" shader by zfedoran (MIT,
    // requested 2026-08-22: "un autre effet pour du procedural grain").
    // iTime -> p3 'Evolution' (same manual-animation convention as the
    // Musk flare above, no free-running clock in this sandbox); iMouse's
    // vertical-wipe intensity preview is dropped (editor-only debug aid in
    // the original, not a real param); iChannel0 sampling -> src (this
    // effect already receives the shape's own rendered pixel as `src`, no
    // texture fetch needed). BLEND_MODE's 5 compile-time branches become a
    // single 'Mode' param (0-4) switched at runtime with plain if/else —
    // same idea as shader_minimax's p2 'Mode', just more branches.
    fx('shader_film_grain', 'Film Grain', 'Stylize', [
      param('p1', 'Intensity', 0, 1, 0.01, '', 0.15),
      param('p2', 'Variance', 0.05, 1, 0.01, '', 0.5),
      param('p3', 'Evolution', -3600, 3600, 1, 'deg', 0),
      param('p4', 'Mode', 0, 4, 1, '', 0),
    ], [
      'let seed = dot(uv, vec2<f32>(12.9898, 78.233));',
      'let evo = params.p3 * 0.1;',
      'var n = fract(sin(seed) * 43758.5453 + evo);',
      'let variance = max(params.p2, 0.02);',
      'n = (1.0 / (variance * 2.5066282746)) * exp(-(n * n) / (2.0 * variance * variance));',
      'let grain = vec3<f32>(n) * (1.0 - src.rgb);',
      'let w = clamp(params.p1, 0.0, 1.0);',
      'let m = params.p4;',
      'var rgb = src.rgb;',
      'if (m < 0.5) {',
      '  rgb = src.rgb + grain * w;',
      '} else if (m < 1.5) {',
      '  rgb = mix(src.rgb, vec3<f32>(1.0) - (vec3<f32>(1.0) - src.rgb) * (vec3<f32>(1.0) - grain), w);',
      '} else if (m < 2.5) {',
      '  let lo = 2.0 * src.rgb * grain;',
      '  let hi = vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - src.rgb) * (vec3<f32>(1.0) - grain);',
      '  let ov = select(lo, hi, src.rgb >= vec3<f32>(0.5));',
      '  rgb = mix(src.rgb, ov, w);',
      '} else if (m < 3.5) {',
      '  let sl = pow(max(src.rgb, vec3<f32>(0.0001)), pow(vec3<f32>(2.0), 2.0 * (vec3<f32>(0.5) - grain)));',
      '  rgb = mix(src.rgb, sl, w);',
      '} else {',
      '  rgb = max(src.rgb, grain * w);',
      '}',
      'return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);',
    ]),
    // Watercolor applies the original wet/dry edge, pigment and paper
    // visualization to existing layered artwork. Its procedural circle SDF
    // is replaced by a signed distance estimate sampled from src_tex, then
    // advected through a coherent wet-flow field. It never generates shapes.
    fx('shader_watercolor', 'Watercolor', 'Stylize', [
      param('p1', 'Diffusion radius', 4, 150, 1, 'px', 45, true),
      param('p2', 'Wet diffusion', 0, 1, 0.01, '', 0.65),
      param('p3', 'Blend', 0, 1, 0.01, '', 1),
      param('p4', 'Paper', 0, 1, 0.01, '', 0.4),
    ], [
      // All procedural fields live in the affected content's coordinate
      // system, not in viewport UV. Zooming and panning therefore reveal the
      // same wash instead of generating a different one.
      'let localAspect = bbox_s.x / max(bbox_s.y, 1.0);',
      'let nuv = vec2<f32>((local_uv.x * 2.0 - 1.0) * localAspect, local_uv.y * 2.0 - 1.0);',
      'let radiusPx = max(params.p1, 1.0);',
      'let diffusion = clamp(params.p2, 0.0, 1.0);',
      'let organicTime = params.time * 0.12;',
      'let timeOffset = 133.7 + organicTime;',
      // A coherent two-component flow field bends diffusion paths like
      // pigment carried by water. It changes slowly over the canvas, so the
      // result forms broad blooms instead of per-pixel jitter.
      'let flowDrift = vec2<f32>(cos(organicTime * 0.37), sin(organicTime * 0.29));',
      'let flowP = nuv * 2.35 + flowDrift * 0.42 + vec2<f32>(timeOffset * 0.004, -timeOffset * 0.003);',
      'let flowI = floor(flowP); let flowF = fract(flowP); let flowU = flowF * flowF * (vec2<f32>(3.0) - 2.0 * flowF);',
      'let flowA = fract(sin(dot(flowI, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let flowB = fract(sin(dot(flowI + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let flowC = fract(sin(dot(flowI + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let flowD = fract(sin(dot(flowI + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let flowN1 = mix(mix(flowA, flowB, flowU.x), mix(flowC, flowD, flowU.x), flowU.y);',
      'let flowP2 = flowP + vec2<f32>(19.17, -7.43);',
      'let flowI2 = floor(flowP2); let flowF2 = fract(flowP2); let flowU2 = flowF2 * flowF2 * (vec2<f32>(3.0) - 2.0 * flowF2);',
      'let flowA2 = fract(sin(dot(flowI2, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let flowB2 = fract(sin(dot(flowI2 + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let flowC2 = fract(sin(dot(flowI2 + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let flowD2 = fract(sin(dot(flowI2 + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let flowN2 = mix(mix(flowA2, flowB2, flowU2.x), mix(flowC2, flowD2, flowU2.x), flowU2.y);',
      'let waterFlow = vec2<f32>(flowN1 - 0.5, flowN2 - 0.5);',
      'let flowDir = normalize(waterFlow + vec2<f32>(0.0001, 0.0));',
      // A second, smaller-scale field decides where water actually escapes.
      // Keeping direction and wetness at different scales avoids a global,
      // uniformly blurred halo.
      'let detailP = nuv * 8.5 + flowDrift.yx * 0.73 + vec2<f32>(timeOffset * 0.008, timeOffset * 0.005);',
      'let detailI = floor(detailP); let detailF = fract(detailP); let detailU = detailF * detailF * (vec2<f32>(3.0) - 2.0 * detailF);',
      'let detailA = fract(sin(dot(detailI, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let detailB = fract(sin(dot(detailI + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let detailC = fract(sin(dot(detailI + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let detailD = fract(sin(dot(detailI + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let detailWet = mix(mix(detailA, detailB, detailU.x), mix(detailC, detailD, detailU.x), detailU.y);',
      'let waterPulse = 0.82 + 0.18 * sin(organicTime * 0.61 + detailWet * 6.28318);',
      'let wetBloom = smoothstep(0.43, 0.72, (detailWet * 0.62 + max(flowN1, flowN2) * 0.38) * waterPulse);',
      // The already-composited paper/background colour is estimated from
      // all four corners. Both RGB and alpha distance are used, so this also
      // works when an Effect Layer receives a fully opaque scene.
      'let texSize = vec2<f32>(params.tex_w, params.tex_h);',
      'let contentMinUv = clamp((bbox_o + vec2<f32>(2.0)) / texSize, vec2<f32>(0.002), vec2<f32>(0.998));',
      'let contentMaxUv = clamp((bbox_o + bbox_s - vec2<f32>(2.0)) / texSize, vec2<f32>(0.002), vec2<f32>(0.998));',
      'let bg0 = textureSample(src_tex, tex_sampler, contentMinUv);',
      'let bg1 = textureSample(src_tex, tex_sampler, vec2<f32>(contentMaxUv.x, contentMinUv.y));',
      'let bg2 = textureSample(src_tex, tex_sampler, vec2<f32>(contentMinUv.x, contentMaxUv.y));',
      'let bg3 = textureSample(src_tex, tex_sampler, contentMaxUv);',
      // Averaging corners contaminated the paper colour whenever one corner
      // touched the canvas border. Component-wise maximum reliably selects
      // white paper while remaining unchanged for a uniformly dark ground.
      'let bg = max(max(bg0, bg1), max(bg2, bg3));',
      'let centerDelta = length(src.rgb - bg.rgb) * 0.577350269 + abs(src.a - bg.a);',
      'let centerMask = smoothstep(0.025, 0.14, centerDelta);',
      'let centerInside = centerMask > 0.5;',
      // Probe the real layer image over a Vogel disk. A golden-angle spiral
      // has no repeated rings or preferred axes, unlike a polar grid.
      'var maskSum = centerMask;',
      'var maskWeight = 1.0;',
      'var pigmentSum = src.rgb * centerMask;',
      'var pigmentSqSum = src.rgb * src.rgb * centerMask;',
      'var opticalSum = -log(max(src.rgb, vec3<f32>(0.008))) * centerMask;',
      'var pigmentWeight = centerMask;',
      'let spiralRotation = (detailWet - 0.5) * 0.9 + organicTime * 0.07;',
      // 384 taps favour final-image quality over the first uncached frame.
      // Nemo's playback cache absorbs that higher cost after rendering, while
      // the denser disk avoids sparse coverage along long edges and corners.
      'for (var tap = 0; tap < 384; tap = tap + 1) {',
      '  let sampleIndex = f32(tap) + 0.5;',
      '  let rr = sqrt(sampleIndex / 384.0);',
      '  let angle = sampleIndex * 2.39996323 + spiralRotation;',
      '  let ray = vec2<f32>(cos(angle), sin(angle));',
      '  let directionalFlow = dot(ray, flowDir);',
      '  let irregularReach = clamp(0.74 + diffusion * (directionalFlow * 0.42 + wetBloom * 0.68), 0.34, 1.38);',
      '  let mobileFlow = waterFlow * (0.78 + 0.22 * sin(organicTime * 0.43 + rr * 5.0));',
      '  let probePx = ray * rr * radiusPx * irregularReach + mobileFlow * radiusPx * rr * diffusion * 0.48;',
      '  let sampleUv = clamp(uv + probePx * texel, vec2<f32>(0.0), vec2<f32>(1.0));',
      '  let s = textureSample(src_tex, tex_sampler, sampleUv);',
      '  let sampleDelta = length(s.rgb - bg.rgb) * 0.577350269 + abs(s.a - bg.a);',
      '  let sampleMask = smoothstep(0.025, 0.14, sampleDelta);',
      // The Gaussian alone still has about 12% weight at rr == 1, then is
      // truncated abruptly by the finite sampling disk. That non-zero rim
      // reproduces the source silhouette at exactly radiusPx. A compact C1
      // window brings both weight and slope smoothly to zero at the support
      // boundary, so broad flat edges dissolve instead of ending as plates.
      '  let supportFade = 1.0 - smoothstep(0.55, 1.0, rr);',
      '  let rw = exp(-rr * rr * 1.35) * supportFade * supportFade;',
      '  maskSum = maskSum + sampleMask * rw;',
      '  maskWeight = maskWeight + rw;',
      '  pigmentSum = pigmentSum + s.rgb * sampleMask * rw;',
      '  pigmentSqSum = pigmentSqSum + s.rgb * s.rgb * sampleMask * rw;',
      '  opticalSum = opticalSum - log(max(s.rgb, vec3<f32>(0.008))) * sampleMask * rw;',
      '  pigmentWeight = pigmentWeight + sampleMask * rw;',
      '}',
      'let blurredMask = maskSum / max(maskWeight, 0.001);',
      'let pigmentRgb = select(src.rgb, pigmentSum / max(pigmentWeight, 0.001), pigmentWeight > 0.001);',
      'let pigmentMeanSq = pigmentSqSum / max(pigmentWeight, 0.001);',
      'let pigmentVariance = length(max(pigmentMeanSq - pigmentRgb * pigmentRgb, vec3<f32>(0.0)));',
      'let colourMeeting = smoothstep(0.008, 0.16, pigmentVariance) * diffusion;',
      // Optical density approximates Beer-Lambert mixing of multiple wet
      // pigments. Flat single-colour regions retain the cheaper RGB average.
      'let opticalPigment = exp(-opticalSum / max(pigmentWeight, 0.001));',
      'let mixedPigment = mix(pigmentRgb, opticalPigment, colourMeeting * 0.82);',
      'let pigmentCoverage = clamp(pigmentWeight / max(maskWeight, 0.001) * 2.4, 0.0, 1.0);',
      // A purely continuous distance proxy prevents visible diffusion
      // terraces. The previous nearest-crossing term could only return one
      // of 96 sample distances and exposed repeated silhouettes.
      'var dist = (0.5 - blurredMask) * 0.46;',
      // Wet blooms locally push the front beyond the geometric contour.
      // blurredMask gates them to actual nearby artwork, never generated
      // geometry or a full-canvas colour wash.
      'let bloomGate = smoothstep(0.004, 0.22, blurredMask);',
      'dist = dist - wetBloom * bloomGate * diffusion * 0.16;',
      // Slowly evolving cauliflower/backrun fronts break the wet boundary
      // into connected lobes rather than random static noise.
      'let backrunWave = sin(detailWet * 9.0 + flowN1 * 5.0 - organicTime * 0.55);',
      'dist = dist + backrunWave * bloomGate * diffusion * 0.026;',
      // Original one-octave small noise (freq 20), used only to roughen the
      // reconstructed boundary.
      'let pS = nuv * 20.0 + vec2<f32>(timeOffset * 0.013, -timeOffset * 0.017);',
      'let iS = floor(pS); let fS = fract(pS); let uS = fS * fS * (vec2<f32>(3.0) - 2.0 * fS);',
      'let sA = fract(sin(dot(iS, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let sB = fract(sin(dot(iS + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let sC = fract(sin(dot(iS + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let sD = fract(sin(dot(iS + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let smallNoise = mix(mix(sA, sB, uS.x), mix(sC, sD, uS.x), uS.y);',
      'dist = dist + 0.01 * (smallNoise * 2.0 - 1.0);',
      // Original one-octave large deformation.
      'let largeFreq = 1.0 + sin(timeOffset) * 0.5;',
      'let pL = nuv * largeFreq + vec2<f32>(timeOffset * 0.011, timeOffset * 0.007);',
      'let iL = floor(pL); let fL = fract(pL); let uL = fL * fL * (vec2<f32>(3.0) - 2.0 * fL);',
      'let lA = fract(sin(dot(iL, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let lB = fract(sin(dot(iL + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let lC = fract(sin(dot(iL + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let lD = fract(sin(dot(iL + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let largeNoise = mix(mix(lA, lB, uL.x), mix(lC, lD, uL.x), uL.y);',
      'dist = dist - mix(0.07, 0.18, diffusion) * bloomGate * smoothstep(0.35, 1.0, largeNoise);',
      // Original wetness field and dry/wet edge contrast. The Wet diffusion
      // control scales the physical reach while retaining the source curve.
      'let wetFreq = 2.0 + sin(timeOffset);',
      'let pW = nuv * wetFreq + vec2<f32>(-timeOffset * 0.009, timeOffset * 0.015);',
      'let iW = floor(pW); let fW = fract(pW); let uW = fW * fW * (vec2<f32>(3.0) - 2.0 * fW);',
      'let wA = fract(sin(dot(iW, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let wB = fract(sin(dot(iW + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let wC = fract(sin(dot(iW + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let wD = fract(sin(dot(iW + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let wetNoise = mix(mix(wA, wB, uW.x), mix(wC, wD, uW.x), uW.y);',
      'let wetNoiseStrength = (0.15 + sin(timeOffset) * 0.05) * mix(0.45, 1.55, diffusion);',
      'let wetness = clamp(smoothstep(0.55, 0.55 + wetNoiseStrength, wetNoise), 0.0, 1.0);',
      'let aaDelta = 0.006;',
      'let edgeDry = 0.02;',
      'let edgeWet = 0.24 * mix(0.4, 1.4, diffusion);',
      'let edge1 = mix(edgeDry, edgeWet, wetness) + aaDelta;',
      'var value = 0.0; var valueSlim = 1.0;',
      'if (dist < 0.0) { value = dist; valueSlim = 1.0; } else { value = smoothstep(edgeDry, edge1, dist); valueSlim = pow(clamp(1.0 - smoothstep(0.0, edge1, dist), 0.0, 1.0), 0.15); }',
      'value = value + 0.28 * valueSlim;',
      // Original post-noise breakup (freq 15).
      'let pP = nuv * 15.0 + vec2<f32>(timeOffset * 0.019, -timeOffset * 0.005);',
      'let iP = floor(pP); let fP = fract(pP); let uP = fP * fP * (vec2<f32>(3.0) - 2.0 * fP);',
      'let pA = fract(sin(dot(iP, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let pB = fract(sin(dot(iP + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let pC = fract(sin(dot(iP + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let pD = fract(sin(dot(iP + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let postNoise = mix(mix(pA, pB, uP.x), mix(pC, pD, uP.x), uP.y);',
      'value = value + value * (1.0 - wetNoise) * smoothstep(0.0, max(postNoise, 0.001), value);',
      // Wyatt's original paper() function, including its 3x3 fibre hash.
      // Paper is stationary while water and mobile pigment move over it.
      'let paperCoord = local_uv * vec2<f32>(1024.0 * localAspect, 1024.0);',
      'var paperV = 0.005 * (sin(0.6 * paperCoord.x + 0.1 * paperCoord.y) + sin(0.7 * paperCoord.y - 0.1 * paperCoord.x));',
      'for (var yy = -1; yy <= 1; yy = yy + 1) {',
      '  for (var xx = -1; xx <= 1; xx = xx + 1) {',
      '    let hp = paperCoord + vec2<f32>(f32(xx), f32(yy));',
      '    var ph = fract(vec3<f32>(hp.x, hp.y, hp.x) * 0.1031);',
      '    ph = ph + vec3<f32>(dot(ph, ph.yzx + vec3<f32>(33.33)));',
      '    paperV = paperV + 0.01875 * fract((ph.x + ph.y) * ph.z);',
      '  }',
      '}',
      'let paperEff = paperV * clamp(params.p4, 0.0, 1.0);',
      // Keep the actual colours of the layers. A small amount of the
      // original cosine palette supplies the subtle chromatic variation,
      // instead of replacing the artwork with a generated palette.
      'let gradient = 0.5 * (nuv.x + nuv.y);',
      'let palT = gradient + timeOffset * 0.1;',
      'let paletteCol = vec3<f32>(0.8, 0.5, 0.4) + vec3<f32>(0.2, 0.4, 0.2) * cos(6.28318 * (vec3<f32>(2.0, 1.0, 1.0) * palT + vec3<f32>(0.0, 0.25, 0.25)));',
      'let paletteAmount = (0.025 + diffusion * 0.055) * pigmentCoverage;',
      'let distCol = mix(mixedPigment, paletteCol, paletteAmount);',
      'let colors = mix(distCol, vec3<f32>(1.0), value - paperEff * 2.0);',
      'var washed = clamp(colors, vec3<f32>(0.0), vec3<f32>(1.0)) - paperEff * 0.3;',
      // Diffusion outside the source is not enough on its own: centerMask is
      // still fully opaque immediately inside a vector edge and can leave
      // one side visibly cut while the opposite wet lobe is soft. Rebuild
      // pigment coverage from the same isotropic blurred field on both sides
      // of the contour. The document background, rather than hard-coded
      // white, is revealed progressively through the thinning wash.
      'let bodyCoverageRaw = smoothstep(0.0, mix(0.70, 0.84, diffusion), blurredMask);',
      'let bodyCoverage = pow(bodyCoverageRaw, 1.35);',
      'washed = mix(bg.rgb, washed, bodyCoverage);',
      // A dilute chromatic halo precedes the denser pigment body. It uses
      // only continuous coverage ramps, so it can glow beyond the artwork
      // without revealing the finite sampling support as another plate.
      'let haloCoverage = smoothstep(0.0, 0.13, blurredMask) * (1.0 - bodyCoverage);',
      'let haloStrength = haloCoverage * mix(0.12, 0.34, diffusion) * mix(0.55, 1.0, wetBloom);',
      'let haloColor = mix(bg.rgb, mixedPigment, 0.52);',
      'washed = mix(washed, haloColor, haloStrength);',
      // Limit edge pooling to a translucent pigment rim. The previous
      // extrapolation could collapse anti-aliased edge pixels toward black.
      'let edgeProximity = 1.0 - smoothstep(0.035, 0.19, abs(dist));',
      'let safePigment = mix(washed, max(washed, mixedPigment * 0.58), edgeProximity * pigmentCoverage);',
      'washed = mix(safePigment, mixedPigment * 0.9, edgeProximity * colourMeeting * 0.16);',
      // Pigment deposits at a moving wet/dry frontier. This produces subtle
      // backruns without the hard permanent outline of the previous pass.
      'let drying = smoothstep(0.48, 0.82, 0.5 + 0.5 * sin(detailWet * 7.0 + flowN2 * 3.0 - organicTime * 0.31));',
      'let depositRim = edgeProximity * wetBloom * drying * pigmentCoverage;',
      'washed = mix(washed, mixedPigment * 0.82, depositRim * 0.14);',
      // Thin capillary deposit at the reconstructed source boundary. The two
      // opposing smoothsteps form a narrow soft band; paper/post noise breaks
      // its continuity like the fine darker rim in the reference wash.
      'let fineRim = smoothstep(0.28, 0.46, blurredMask) * (1.0 - smoothstep(0.46, 0.66, blurredMask));',
      'let rimBreakup = mix(0.58, 1.0, postNoise) * mix(0.72, 1.0, wetness);',
      'let fineRimStrength = fineRim * rimBreakup * mix(0.09, 0.18, diffusion);',
      'washed = mix(washed, mixedPigment * 0.74, fineRimStrength);',
      // Apply chromatic diffusion only to real pigment and to connected wet
      // lobes. Pixels farther away remain exactly the incoming layer image.
      // A finite diffusion kernel necessarily ends at radiusPx. If the
      // smallest non-zero coverage is promoted too quickly, its last Vogel
      // samples expose that limit as a crisp protruding plate. Two continuous
      // coverage ramps keep the useful wet body while making the final wash
      // asymptotically disappear into the paper.
      'let outerFeather = smoothstep(0.0, 0.085, blurredMask);',
      'let wetTail = outerFeather * smoothstep(0.0, 0.18, blurredMask) * mix(0.12, 1.0, wetBloom);',
      // Coverage, not wetBloom, decides where edge reconstruction is active.
      // This keeps dry sides and corners as soft as the visibly wet sides;
      // wetBloom remains free to shape only the organic protrusions.
      'let localInfluence = clamp(max(centerMask, max(bodyCoverage, max(wetTail, haloCoverage))), 0.0, 1.0);',
      'let blend = clamp(params.p3, 0.0, 1.0) * clamp(src.a * 3.0, 0.0, 1.0) * localInfluence;',
      'return vec4<f32>(mix(src.rgb, clamp(washed, vec3<f32>(0.0), vec3<f32>(1.0)), blend), src.a);',
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
    // Anamorphic-lens streak look (2026-08, "possible d'avoir un flare
    // comme ça" — reference: a horizontal light streak off a bright source
    // plus one or two chromatic ghost rings strung along the axis toward
    // frame center), distinct from the plain sun-flare above. Aspect-
    // corrected (`aspect` multiplies x before any distance/length call) so
    // the streak stays a clean horizontal line and the rings stay circular
    // on non-square canvases instead of squashing to the canvas's own w/h
    // ratio. Ghost ring positions/radii and the warm/cool tint split are
    // fixed constants tuned to the reference look, not exposed as params —
    // matches this library's existing "opinionated look, few knobs"
    // convention (e.g. Ground Shadow, Contour Brut above) rather than a
    // fully general N-ring flare system.
    fx('shader_anamorphic_flare', 'Anamorphic Flare', 'Generate', [
      param('p1', 'Brightness', 0, 3, 0.05, '', 1),
      param('p2', 'X', 0, 1, 0.01, '', 0.75),
      param('p3', 'Y', 0, 1, 0.01, '', 0.35),
      param('p4', 'Streak Length', 0.02, 1, 0.01, '', 0.35),
    ], [
      'let aspect = params.tex_w / max(params.tex_h, 1.0);',
      'let c = vec2<f32>(params.p2, params.p3);',
      'let d = uv - c; let dp = vec2<f32>(d.x * aspect, d.y);',
      'let core = 1.0 / (1.0 + dot(dp, dp) * 900.0);',
      'let len = max(params.p4, 0.02);',
      'let streak = exp(-dp.y * dp.y * 3000.0) * exp(-abs(d.x) / len);',
      'let toC = vec2<f32>(0.5, 0.5) - c;',
      'let g1 = c + toC * 0.4; let g1p = vec2<f32>((uv.x - g1.x) * aspect, uv.y - g1.y); let r1 = length(g1p);',
      'let ring1 = smoothstep(0.012, 0.0, abs(r1 - 0.045));',
      'let g2 = c + toC * 0.75; let g2p = vec2<f32>((uv.x - g2.x) * aspect, uv.y - g2.y); let r2 = length(g2p);',
      'let ring2 = smoothstep(0.02, 0.0, abs(r2 - 0.09));',
      'let warm = vec3<f32>(1.0, 0.82, 0.55); let cool = vec3<f32>(0.55, 0.75, 1.0);',
      'let flareColor = warm * (core + streak * 0.6) + cool * ring1 * 0.8 + warm * ring2 * 0.5;',
      'return vec4<f32>(src.rgb + flareColor * params.p1, src.a);',
    ]),
    // Port of "Musk's lens flare" (icecool's mod of Shadertoy 4sX3Rs),
    // requested 2026-08-22 as a richer alternative to the flare above. The
    // original samples iChannel0 (a noise texture) offset by iTime for its
    // ring wobble/shimmer — neither exists in this custom-effect sandbox
    // (no texture channels, no clock uniform), so both are replaced: the
    // noise() calls become inline hash functions (same fract(sin(dot(...)))
    // trick as shader_fractal_noise above), and iTime becomes the p4
    // 'Evolution' param — same pattern as Fractal Noise/Turbulent Displace,
    // i.e. a value the user animates by hand via keyframes rather than a
    // free-running clock.
    fx('shader_musk_flare', 'Cinematic Lens Flare', 'Generate', [
      param('p1', 'Brightness', 0, 3, 0.05, '', 1),
      param('p2', 'X', 0, 1, 0.01, '', 0.5),
      param('p3', 'Y', 0, 1, 0.01, '', 0.5),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let aspect = params.tex_w / max(params.tex_h, 1.0);',
      'let evo = params.p4 * 0.0174532925;',
      'let uvC = uv - vec2<f32>(0.5); let uvA = vec2<f32>(uvC.x * aspect, uvC.y);',
      'let posC = vec2<f32>(params.p2, params.p3) - vec2<f32>(0.5); let posA = vec2<f32>(posC.x * aspect, posC.y);',
      'let main = uvA - posA; let uvd = uvA * length(uvA);',
      'let ang = atan2(main.y, main.x);',
      'var dist = length(main); dist = pow(max(dist, 0.0001), 0.1);',
      'let nP = vec2<f32>((ang - evo * 6.0) * 16.0, dist * 32.0);',
      'let n = fract(sin(dot(nP, vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let n2 = fract(sin((abs(ang) + n * 0.5) * 127.1) * 43758.5453);',
      'var f0 = 1.0 / (length(uvA - posA) * 16.0 + 1.0);',
      'f0 = f0 + f0 * (sin((ang + evo * 3.0 + n2 * 2.0) * 12.0) * 0.1 + dist * 0.1 + 0.8);',
      'let f2 = max(1.0 / (1.0 + 32.0 * pow(length(uvd + 0.8 * posA), 2.0)), 0.0) * 0.25;',
      'let f22 = max(1.0 / (1.0 + 32.0 * pow(length(uvd + 0.85 * posA), 2.0)), 0.0) * 0.23;',
      'let f23 = max(1.0 / (1.0 + 32.0 * pow(length(uvd + 0.9 * posA), 2.0)), 0.0) * 0.21;',
      'var uvx = mix(uvA, uvd, -0.5);',
      'let f4 = max(0.01 - pow(length(uvx + 0.4 * posA), 2.4), 0.0) * 6.0;',
      'let f42 = max(0.01 - pow(length(uvx + 0.45 * posA), 2.4), 0.0) * 5.0;',
      'let f43 = max(0.01 - pow(length(uvx + 0.5 * posA), 2.4), 0.0) * 3.0;',
      'uvx = mix(uvA, uvd, -0.4);',
      'let f5 = max(0.01 - pow(length(uvx + 0.2 * posA), 5.5), 0.0) * 2.0;',
      'let f52 = max(0.01 - pow(length(uvx + 0.4 * posA), 5.5), 0.0) * 2.0;',
      'let f53 = max(0.01 - pow(length(uvx + 0.6 * posA), 5.5), 0.0) * 2.0;',
      'uvx = mix(uvA, uvd, -0.5);',
      'let f6 = max(0.01 - pow(length(uvx - 0.3 * posA), 1.6), 0.0) * 6.0;',
      'let f62 = max(0.01 - pow(length(uvx - 0.325 * posA), 1.6), 0.0) * 3.0;',
      'let f63 = max(0.01 - pow(length(uvx - 0.35 * posA), 1.6), 0.0) * 5.0;',
      'var c = vec3<f32>(f2 + f4 + f5 + f6, f22 + f42 + f52 + f62, f23 + f43 + f53 + f63);',
      'c = c + vec3<f32>(f0); c = c * vec3<f32>(1.4, 1.2, 1.0);',
      'let w = c.x + c.y + c.z; c = mix(c, vec3<f32>(w) * 0.5, w * 0.1);',
      'return vec4<f32>(src.rgb + c * params.p1, src.a);',
    ]),
    // 2026-07-30: 'Distort' effects below were rewritten to operate in
    // local_uv (0..1 across THIS shape's own on-screen bbox, see
    // engine.rs's register_custom_effect doc comment) instead of raw uv
    // (0..1 across the whole canvas) — Cyril reproduced live that a
    // shipped Twirl's pattern visibly changed under PURE PANNING (zero
    // zoom change), which only makes sense if its "center" (vec2(0.5)) was
    // actually the canvas center, not the shape's. Any Radius/Center-style
    // param that's already a UV-fraction (not a 'px' value) had its
    // `spatial` flag REMOVED here too: local_uv already normalizes to the
    // shape's current on-screen size (zoom AND any Motion/3D/Duplicator
    // scale baked in via the item's own transform), so the old
    // view.zoom-only multiplication is now redundant and would
    // double-compensate.
    fx('shader_twirl', 'Twirl', 'Distort', [
      param('p1', 'Angle', -720, 720, 1, 'deg', 180),
      param('p2', 'Radius', 0, 1.5, 0.01, '', 0.5),
    ], [
      'let c = vec2<f32>(0.5); let v = local_uv - c; let r = length(v); let radius = max(params.p2, 0.001);',
      'let amt = (1.0 - smoothstep(0.0, radius, r)) * params.p1 * 0.01745329252; let s = sin(amt); let co = cos(amt);',
      'let lsuv = c + vec2<f32>(v.x * co - v.y * s, v.x * s + v.y * co);',
      'let suv = (bbox_o + lsuv * bbox_s) / vec2<f32>(params.tex_w, params.tex_h);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_bulge', 'Bulge', 'Distort', [
      param('p1', 'Amount', -1, 1, 0.01, '', 0.4),
      param('p2', 'Radius', 0, 1.5, 0.01, '', 0.65),
    ], [
      'let c = vec2<f32>(0.5); let v = local_uv - c; let r = length(v); let fall = 1.0 - smoothstep(0.0, max(params.p2, 0.001), r);',
      'let lsuv = c + v * (1.0 - params.p1 * fall * 0.5);',
      'let suv = (bbox_o + lsuv * bbox_s) / vec2<f32>(params.tex_w, params.tex_h);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_wave_warp', 'Wave Warp', 'Distort', [
      param('p1', 'Height', -80, 80, 1, 'px', 16, true),
      param('p2', 'Width', 4, 300, 1, 'px', 80, true),
      param('p3', 'Direction', 0, 360, 1, 'deg', 0),
      param('p4', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let a = params.p3 * 0.01745329252; let dir = vec2<f32>(cos(a), sin(a)); let nrm = vec2<f32>(-dir.y, dir.x);',
      'let phase = dot(local_uv * bbox_s, dir) / max(params.p2, 1.0) * 6.2831853 + params.p4 * 0.01745329252;',
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
      'let p = (local_uv + evo * 0.002) * scale; let ip = floor(p); let fp = fract(p); let u = fp * fp * (vec2<f32>(3.0) - 2.0 * fp);',
      'let a = fract(sin(dot(ip, vec2<f32>(127.1, 311.7))) * 43758.5453); let b = fract(sin(dot(ip + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'let c = fract(sin(dot(ip + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453); let d = fract(sin(dot(ip + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);',
      'var n1 = mix(mix(a, b, u.x), mix(c, d, u.x), u.y);',
      'let q = (local_uv + vec2<f32>(9.2, 4.7) - evo * 0.0015) * scale; let iq = floor(q); let fq = fract(q); let uq = fq * fq * (vec2<f32>(3.0) - 2.0 * fq);',
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
      'let c = vec2<f32>(0.5); let v = local_uv - c; let lsuv = c + v * (1.0 + params.p1 * dot(v, v) * 1.8);',
      'let suv = (bbox_o + lsuv * bbox_s) / vec2<f32>(params.tex_w, params.tex_h);',
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
      // Center X/Y are shape-local (0..1 across this item's own bbox, see
      // local_uv's doc comment) — converted to real uv ONCE, then the tap
      // loop below stays in ordinary uv space like every other blur here.
      'let c = (bbox_o + vec2<f32>(params.p2, params.p3) * bbox_s) / vec2<f32>(params.tex_w, params.tex_h); let v = uv - c;',
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
      'let n = sin(local_uv.y * params.p2 * 9.0 + evo) * 0.55 + sin((local_uv.y + local_uv.x * 0.3) * params.p2 * 17.0 - evo * 1.37) * 0.28 + sin(local_uv.x * params.p2 * 5.0 + evo * 0.7) * 0.17;',
      'let lift = smoothstep(1.0, 0.0, local_uv.y) * (0.6 + params.p3 * 0.4);',
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
      'let a = params.p1 * 0.01745329252; let d = vec2<f32>(cos(a), sin(a)); let q = local_uv - vec2<f32>(0.5);',
      'let side = dot(q, d); let lmuv = local_uv - d * side * 2.0 * select(0.0, 1.0, side > params.p2 - 0.5);',
      'let muv = (bbox_o + lmuv * bbox_s) / vec2<f32>(params.tex_w, params.tex_h);',
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
      'let wave = sin(dot(local_uv - vec2<f32>(0.5), d) * params.p2 * 6.28318 + phase) * params.p1 * texel;',
      'let outc = textureSample(src_tex, tex_sampler, clamp(uv + vec2<f32>(-d.y, d.x) * wave, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_spherize', 'Spherize', 'Distort', [
      param('p1', 'Amount', -1, 1, 0.01, '', 0.65),
      param('p2', 'Radius', 0.05, 1, 0.01, '', 0.5),
    ], [
      'let c = vec2<f32>(0.5); let v = local_uv - c; let r = length(v); let radius = max(params.p2, 0.01);',
      'let inside = smoothstep(radius, radius - 0.01, r); let nr = r * (1.0 - params.p1 * (1.0 - smoothstep(0.0, radius, r)) * 0.42);',
      'let lsuv = c + normalize(v + vec2<f32>(0.00001)) * nr * inside + v * (1.0 - inside);',
      'let suv = (bbox_o + lsuv * bbox_s) / vec2<f32>(params.tex_w, params.tex_h);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)));',
      'return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_displacement', 'Displacement', 'Distort', [
      param('p1', 'Amount', 0, 120, 1, 'px', 18, true),
      param('p2', 'Scale', 2, 80, 1, '', 18),
      param('p3', 'Evolution', -3600, 3600, 1, 'deg', 0),
    ], [
      'let t = params.p3 * 0.01745329252; let n1 = sin(local_uv.y * params.p2 + t) * 0.6 + sin(local_uv.y * params.p2 * 2.7 - t * 1.4) * 0.3;',
      'let n2 = sin(local_uv.x * params.p2 * 1.3 - t * 0.8) * 0.6 + sin(local_uv.x * params.p2 * 3.1 + t) * 0.3;',
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
      'let c = vec2<f32>(params.p3, params.p4); let v = local_uv - c; let r = length(v); let ang = atan2(v.y, v.x) + params.p2 * 0.01745329252; let seg = 6.2831853 / max(params.p1, 2.0); let folded = abs(fract(ang / seg + 0.5) - 0.5) * seg;',
      'let lsuv = c + vec2<f32>(cos(folded), sin(folded)) * r;',
      'let suv = (bbox_o + lsuv * bbox_s) / vec2<f32>(params.tex_w, params.tex_h);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0))); return vec4<f32>(outc.rgb, outc.a);',
    ]),
    fx('shader_polar_coordinates', 'Polar Coordinates', 'Distort', [
      param('p1', 'Mix', 0, 1, 0.01, '', 1),
      param('p2', 'Rotation', -360, 360, 1, 'deg', 0),
      param('p3', 'Radius', 0.1, 2, 0.01, '', 1),
    ], [
      'let q = local_uv - vec2<f32>(0.5); let r = length(q) * params.p3; let a = atan2(q.y, q.x) / 6.2831853 + 0.5 + params.p2 * 0.0027777778; let lpolar = vec2<f32>(fract(a), clamp(r, 0.0, 1.0));',
      'let polar = (bbox_o + lpolar * bbox_s) / vec2<f32>(params.tex_w, params.tex_h);',
      'let outc = textureSample(src_tex, tex_sampler, clamp(polar, vec2<f32>(0.0), vec2<f32>(1.0))); let rgb = mix(src.rgb, outc.rgb, clamp(params.p1, 0.0, 1.0)); return vec4<f32>(rgb, mix(src.a, outc.a, clamp(params.p1, 0.0, 1.0)));',
    ]),
    fx('shader_motion_tile', 'Motion Tile', 'Distort', [
      param('p1', 'Offset X', -2, 2, 0.01, '', 0),
      param('p2', 'Offset Y', -2, 2, 0.01, '', 0),
      param('p3', 'Scale', 0.25, 4, 0.01, '', 1),
      param('p4', 'Mirror', 0, 1, 1, '', 1),
    ], [
      'let q = (local_uv - vec2<f32>(0.5)) * params.p3 + vec2<f32>(0.5 + params.p1, 0.5 + params.p2); let cell = floor(q); let f = fract(q); let parity = fract((cell.x + cell.y) * 0.5) * 2.0; let lmirrored = select(f, 1.0 - f, parity > 0.5 && params.p4 > 0.5);',
      'let mirrored = (bbox_o + lmirrored * bbox_s) / vec2<f32>(params.tex_w, params.tex_h);',
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
