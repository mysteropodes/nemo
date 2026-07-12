// ---- LABS PROTOTYPE — Real WGSL shader effects (Figma Motion-inspired, baked) ----
// feature-scouting.md #2 explains why LIVE non-destructive per-layer
// effects need engine.rs (offscreen render per layer + WGSL pass in the
// Rust/vello pipeline itself). This is the honest baked version — same
// documented tradeoff as layer-effects-bake.js — but using REAL WGSL
// fragment shaders on the actual WebGPU API (navigator.gpu), not a CSS
// canvas filter string. CSS `filter` only covers blur/brightness/
// contrast/etc.; it structurally cannot do the effects Figma Motion
// actually ships (Config 2026: bloom/glow, chromatic aberration,
// dithering, pixelation, liquid/stretch distortion, fractal noise,
// moiré) — those need real per-pixel shader math, which is what this
// file provides for five of them.
//
// Pipeline, fully OFFSCREEN (never touches a live GPUCanvasContext, so
// none of reference-3d.js's canvas-presentation-timing bug applies
// here): rasterize the frame's layer content to a bitmap (reusing
// storyboard-mode's fixed technique — Layer scratch stays attached
// while populated, resolution:72 = 1 doc unit = 1px), upload it as a
// GPUTexture, run a fullscreen-triangle vertex shader + a per-effect
// WGSL fragment shader into a render-target texture, copy that texture
// to a buffer (respecting WebGPU's 256-byte bytesPerRow alignment —
// the classic gotcha), and paint the unpadded result into a 2D canvas
// for the final PNG — then bake it back into the layer exactly like
// layer-effects-bake.js does (desR, same serialized-Raster shape
// save/load already produces).
//
//   SMLabs.bakeShaderEffect(layerIdx, 'bloom'|'chromatic'|'pixelate'|'dither'|'wave', params)
//   SMLabs.listShaderEffects()   — names + param docs
(function () {
  var device = null, adapterInfo = null;
  function ensureDevice() {
    if (device) return Promise.resolve(device);
    if (!navigator.gpu) return Promise.reject(new Error('WebGPU indisponible (navigator.gpu absent)'));
    return navigator.gpu.requestAdapter().then(function (adapter) {
      if (!adapter) throw new Error('Pas d\'adaptateur WebGPU');
      adapterInfo = adapter;
      return adapter.requestDevice();
    }).then(function (d) { device = d; return d; });
  }

  var VERT_WGSL =
    '@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {\n' +
    '  var p = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0), vec2<f32>(3.0,-1.0), vec2<f32>(-1.0,3.0));\n' +
    '  return vec4<f32>(p[i], 0.0, 1.0);\n' +
    '}\n';

  // Each effect is a COMPLETE fragment shader (simpler/more robust than
  // composing fragments) sampling `srcTex`/`srcSampler` and one small
  // uniform struct `Params` at group0 binding2 — same group-0 texture/
  // sampler/uniform slot convention CLAUDE.md documents for Rive's own
  // WGSL post-process pattern, kept here purely as a familiar convention
  // (this file's pipeline is otherwise unrelated to that Rive/Luau path).
  // Live-found bug: per-effect WGSL structs originally declared their own
  // ad-hoc leading f32 fields before a `texel: vec2<f32>` (e.g. `{blockPx,
  // pad, pad2, texel}`). WGSL/std140 requires a vec2<f32> to start at an
  // 8-BYTE-aligned offset — for a struct with 1 or 3 leading f32 fields
  // (4 or 12 bytes in), the compiler silently inserts ADDITIONAL padding
  // to push texel to the next 8-byte boundary, which the JS-side
  // packParams() (computing layout generically from Object.keys(params))
  // did not account for. Confirmed live: an isolated hand-packed 8-float
  // (32-byte) buffer with texel manually placed at float-index 4 rendered
  // correctly (block-quantized red/green pixels, exactly as expected);
  // the SAME shader driven by the real packParams()-built 4-float buffer
  // rendered fully transparent (0,0,0,0) everywhere — no WebGPU
  // validation error surfaced (pushErrorScope('validation') came back
  // null), it just silently read garbage/out-of-bounds uniform data.
  // Fixed by standardizing on ONE unambiguous layout for every effect:
  // `struct Params { v: vec4<f32>, texel: vec4<f32> }` — a vec4 has
  // 16-byte size AND 16-byte alignment, so there is no padding ambiguity
  // to get wrong, ever. Up to 4 named scalars live in v.x/.y/.z/.w.
  var PARAM_STRUCT = 'struct Params { v: vec4<f32>, texel: vec4<f32> };\n';
  var EFFECTS = {
    bloom: {
      // v.x=threshold v.y=intensity v.z=radius
      params: { threshold: 0.55, intensity: 1.4, radius: 3 },
      wgsl: PARAM_STRUCT +
        '@group(0) @binding(0) var srcTex: texture_2d<f32>;\n' +
        '@group(0) @binding(1) var srcSampler: sampler;\n' +
        '@group(0) @binding(2) var<uniform> p: Params;\n' +
        '@fragment fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {\n' +
        '  let texel = p.texel.xy;\n' +
        '  let uv = pos.xy * texel;\n' +
        '  let base = textureSample(srcTex, srcSampler, uv);\n' +
        '  var bloom = vec3<f32>(0.0);\n' +
        '  var n = 0.0;\n' +
        '  let r = i32(p.v.z);\n' +
        '  for (var dy = -r; dy <= r; dy = dy + 1) {\n' +
        '    for (var dx = -r; dx <= r; dx = dx + 1) {\n' +
        '      let s = textureSample(srcTex, srcSampler, uv + vec2<f32>(f32(dx), f32(dy)) * texel * 2.0).rgb;\n' +
        '      let l = max(s.r, max(s.g, s.b));\n' +
        '      let bright = max(l - p.v.x, 0.0);\n' +
        '      bloom = bloom + s * bright;\n' +
        '      n = n + 1.0;\n' +
        '    }\n' +
        '  }\n' +
        '  bloom = bloom / max(n, 1.0) * p.v.y;\n' +
        '  return vec4<f32>(base.rgb + bloom, base.a);\n' +
        '}\n',
    },
    chromatic: {
      // v.x=strength
      params: { strength: 6.0 },
      wgsl: PARAM_STRUCT +
        '@group(0) @binding(0) var srcTex: texture_2d<f32>;\n' +
        '@group(0) @binding(1) var srcSampler: sampler;\n' +
        '@group(0) @binding(2) var<uniform> p: Params;\n' +
        '@fragment fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {\n' +
        '  let texel = p.texel.xy;\n' +
        '  let uv = pos.xy * texel;\n' +
        '  let center = vec2<f32>(0.5, 0.5);\n' +
        '  let d = uv - center;\n' +
        '  let off = d * (p.v.x * texel * 40.0);\n' + // radial falloff (GPUDog-style)
        '  let r = textureSample(srcTex, srcSampler, uv + off).r;\n' +
        '  let g = textureSample(srcTex, srcSampler, uv).g;\n' +
        '  let b = textureSample(srcTex, srcSampler, uv - off).b;\n' +
        '  let a = textureSample(srcTex, srcSampler, uv).a;\n' +
        '  return vec4<f32>(r, g, b, a);\n' +
        '}\n',
    },
    pixelate: {
      // v.x=blockPx
      params: { blockPx: 12.0 },
      wgsl: PARAM_STRUCT +
        '@group(0) @binding(0) var srcTex: texture_2d<f32>;\n' +
        '@group(0) @binding(1) var srcSampler: sampler;\n' +
        '@group(0) @binding(2) var<uniform> p: Params;\n' +
        '@fragment fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {\n' +
        '  let block = vec2<f32>(p.v.x, p.v.x);\n' +
        '  let snapped = floor(pos.xy / block) * block + block * 0.5;\n' +
        '  return textureSample(srcTex, srcSampler, snapped * p.texel.xy);\n' +
        '}\n',
    },
    dither: {
      // v.x=levels
      params: { levels: 4.0 },
      wgsl: PARAM_STRUCT +
        '@group(0) @binding(0) var srcTex: texture_2d<f32>;\n' +
        '@group(0) @binding(1) var srcSampler: sampler;\n' +
        '@group(0) @binding(2) var<uniform> p: Params;\n' +
        'fn bayer(x: u32, y: u32) -> f32 {\n' +
        '  var m = array<f32,16>(0.0,8.0,2.0,10.0, 12.0,4.0,14.0,6.0, 3.0,11.0,1.0,9.0, 15.0,7.0,13.0,5.0);\n' +
        '  return m[(y % 4u) * 4u + (x % 4u)] / 16.0;\n' +
        '}\n' +
        '@fragment fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {\n' +
        '  let uv = pos.xy * p.texel.xy;\n' +
        '  let c = textureSample(srcTex, srcSampler, uv);\n' +
        '  let t = bayer(u32(pos.x), u32(pos.y)) - 0.5;\n' +
        '  let lv = max(p.v.x - 1.0, 1.0);\n' +
        '  let q = floor(c.rgb * lv + t + 0.5) / lv;\n' +
        '  return vec4<f32>(clamp(q, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);\n' +
        '}\n',
    },
    wave: {
      // v.x=amplitudePx v.y=frequency v.z=phase
      params: { amplitudePx: 10.0, frequency: 3.0, phase: 0.0 },
      wgsl: PARAM_STRUCT +
        '@group(0) @binding(0) var srcTex: texture_2d<f32>;\n' +
        '@group(0) @binding(1) var srcSampler: sampler;\n' +
        '@group(0) @binding(2) var<uniform> p: Params;\n' +
        '@fragment fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {\n' +
        '  let texel = p.texel.xy;\n' +
        '  let uv = pos.xy * texel;\n' +
        '  let wobble = sin(uv.y * p.v.y * 6.2831853 + p.v.z) * p.v.x * texel.x;\n' +
        '  return textureSample(srcTex, srcSampler, uv + vec2<f32>(wobble, 0.0));\n' +
        '}\n',
    },
  };

  window.SMLabs.listShaderEffects = function () {
    return Object.keys(EFFECTS).map(function (n) { return { name: n, defaultParams: EFFECTS[n].params }; });
  };

  // ---- rasterize the frame (same fixed technique as storyboard-mode / layer-effects-bake) ----
  function rasterizeLayer(layerIdx, frame) {
    var prevActiveLayer = project.activeLayer;
    var scratch = new Layer();
    var strokes = getEffectiveStrokes(layerIdx, frame);
    strokes.forEach(function (sd) { if (sd.isRaster) desR(sd, scratch); else desP(sd, scratch); });
    var bounds = scratch.children.length ? scratch.bounds : new Rectangle(0, 0, 1, 1);
    var w = Math.max(1, Math.ceil(bounds.width));
    var h = Math.max(1, Math.ceil(bounds.height));
    var cv = document.createElement('canvas'); cv.width = w; cv.height = h;
    var ctx = cv.getContext('2d');
    var hadContent = false;
    try {
      if (scratch.children.length) {
        var raster = scratch.rasterize({ resolution: 72, insert: false }); // 1 doc unit = 1px
        if (raster && raster.canvas) { ctx.drawImage(raster.canvas, 0, 0, w, h); hadContent = true; raster.remove(); }
      }
    } catch (e) { console.warn('[labs] shader-effects rasterize a échoué', e); }
    scratch.remove();
    prevActiveLayer.activate();
    return { canvas: cv, x: bounds.x, y: bounds.y, w: w, h: h, hadContent: hadContent };
  }

  // ---- WebGPU offscreen run ----
  // Matches `struct Params { v: vec4<f32>, texel: vec4<f32> }` exactly —
  // always 8 floats / 32 bytes, always the same shape, no per-effect
  // padding math to get wrong (see the EFFECTS header comment for the
  // live-found std140-alignment bug this replaced).
  function packParams(paramsObj, order, w, h) {
    var v = order.map(function (k) { return paramsObj[k]; });
    while (v.length < 4) v.push(0);
    var arr = new Float32Array(8);
    arr.set(v.slice(0, 4), 0);
    arr[4] = 1 / w; arr[5] = 1 / h; arr[6] = 0; arr[7] = 0;
    return arr;
  }

  function runEffect(dev, srcCanvas, w, h, effectDef, params) {
    var order = Object.keys(effectDef.params);
    var uniformData = packParams(Object.assign({}, effectDef.params, params || {}), order, w, h);

    var srcBitmapPromise = createImageBitmap(srcCanvas);
    return srcBitmapPromise.then(function (bitmap) {
      var srcTex = dev.createTexture({ size: [w, h], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST | GPUTextureUsage.RENDER_ATTACHMENT });
      dev.queue.copyExternalImageToTexture({ source: bitmap }, { texture: srcTex }, [w, h]);

      var sampler = dev.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'clamp-to-edge', addressModeV: 'clamp-to-edge' });
      var uniformBuf = dev.createBuffer({ size: uniformData.byteLength, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
      dev.queue.writeBuffer(uniformBuf, 0, uniformData);

      var shaderMod = dev.createShaderModule({ code: VERT_WGSL + effectDef.wgsl });
      var pipeline = dev.createRenderPipeline({
        layout: 'auto',
        vertex: { module: shaderMod, entryPoint: 'vs' },
        fragment: { module: shaderMod, entryPoint: 'fs', targets: [{ format: 'rgba8unorm' }] },
        primitive: { topology: 'triangle-list' },
      });
      var bindGroup = dev.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: srcTex.createView() },
          { binding: 1, resource: sampler },
          { binding: 2, resource: { buffer: uniformBuf } },
        ],
      });

      var outTex = dev.createTexture({ size: [w, h], format: 'rgba8unorm', usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC });
      var encoder = dev.createCommandEncoder();
      var pass = encoder.beginRenderPass({ colorAttachments: [{ view: outTex.createView(), loadOp: 'clear', storeOp: 'store', clearValue: { r: 0, g: 0, b: 0, a: 0 } }] });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, bindGroup);
      pass.draw(3);
      pass.end();

      // WebGPU requires bytesPerRow to be a multiple of 256 — pad the
      // readback buffer's row stride, then strip the padding back out
      // when building the 2D ImageData (the classic offscreen-readback
      // gotcha this file's header comment calls out).
      var bytesPerRowUnpadded = w * 4;
      var bytesPerRow = Math.ceil(bytesPerRowUnpadded / 256) * 256;
      var readBuf = dev.createBuffer({ size: bytesPerRow * h, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
      encoder.copyTextureToBuffer({ texture: outTex }, { buffer: readBuf, bytesPerRow: bytesPerRow }, [w, h]);
      dev.queue.submit([encoder.finish()]);

      return readBuf.mapAsync(GPUMapMode.READ).then(function () {
        var padded = new Uint8Array(readBuf.getMappedRange());
        var out = new Uint8ClampedArray(w * h * 4);
        for (var row = 0; row < h; row++) out.set(padded.subarray(row * bytesPerRow, row * bytesPerRow + bytesPerRowUnpadded), row * bytesPerRowUnpadded);
        readBuf.unmap();
        var outCv = document.createElement('canvas'); outCv.width = w; outCv.height = h;
        outCv.getContext('2d').putImageData(new ImageData(out, w, h), 0, 0);
        srcTex.destroy(); outTex.destroy(); uniformBuf.destroy(); readBuf.destroy();
        return outCv;
      });
    });
  }

  function ensureKeyframeAt(layerIdx, frame) {
    var f = state.layers[layerIdx].frames[frame];
    if (f.isKeyframe || f.isInterpolated) return;
    f.strokes = JSON.parse(JSON.stringify(getEffectiveStrokes(layerIdx, frame)));
    f.isKeyframe = true; f.isInterpolated = false;
    if (typeof syncLinkedKeyframeFolder === 'function') syncLinkedKeyframeFolder(layerIdx, frame);
  }

  window.SMLabs.bakeShaderEffect = function (layerIdx, effectName, params) {
    var effectDef = EFFECTS[effectName];
    if (!effectDef) { console.warn('[labs] effet inconnu:', effectName, Object.keys(EFFECTS)); return Promise.resolve(false); }
    var ld = state.layers[layerIdx];
    if (!ld || ld.locked || ld.symbolId) { if (typeof showToast === 'function') showToast('Calque invalide/verrouillé'); return Promise.resolve(false); }
    var frame = state.currentFrame;
    var src = rasterizeLayer(layerIdx, frame);
    if (!src.hadContent) { if (typeof showToast === 'function') showToast('Calque vide sur cette frame'); return Promise.resolve(false); }
    return ensureDevice().then(function (dev) {
      return runEffect(dev, src.canvas, src.w, src.h, effectDef, params);
    }).then(function (outCanvas) {
      pushUndo();
      ensureKeyframeAt(layerIdx, frame);
      var layer = userLayers[layerIdx];
      layer.removeChildren();
      var rd = { isRaster: true, src: outCanvas.toDataURL('image/png'), x: src.x + src.w / 2, y: src.y + src.h / 2, width: src.w, height: src.h, opacity: 1 };
      desR(rd, layer);
      saveActiveLayerFrame(); updateUI();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      if (typeof showToast === 'function') showToast('Effet WGSL « ' + effectName + ' » appliqué (baké, frame ' + (frame + 1) + ')');
      return true;
    }).catch(function (e) {
      console.warn('[labs] bakeShaderEffect a échoué', e);
      if (typeof showToast === 'function') showToast('Effet WGSL indisponible : ' + e.message);
      return false;
    });
  };

  window.SMLabs.register('shader-effects-bake', {
    flag: 'nemo-labs-shaders',
    describe: 'Effets WGSL réels (Figma Motion — bloom/chromatic/pixelate/dither/wave), bakés (pas live — voir feature-scouting #2) : SMLabs.bakeShaderEffect(layerIdx, nom, params) — WebGPU offscreen, aucune dépendance',
  });
})();
