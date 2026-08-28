// ---- LABS PROTOTYPE — 3D reference viewer (Umoupen OBJ reference, scoped) ----
// feature-scouting.md #4 flagged three.js as too heavy a dependency for
// one reference-viewing feature. This is the honestly-scoped build
// instead: a from-scratch, dependency-free WebGL viewer — a minimal OBJ
// parser plus a compact static VRM/glTF material reader + raw GLSL
// (directional light and embedded base-colour textures)
// in a floating panel, same "reference overlay, never exported, never
// touches the render pipeline" role as the existing roto reference
// (SMReference) already fills for video/image references — this is that
// same idea for a rotatable 3D model instead of a flat image.
//
//   SMLabs.open3DReference(objText)   — parses + opens the viewer
//   SMLabs.close3DReference()
//   SMLabs.set3DRotation(yaw, pitch)  — radians, also settable by dragging
// Orbit: drag rotates (yaw/pitch), wheel zooms. Pure reference — never
// baked into a frame, never exported; the existing roto-PNG workaround
// (screenshot a pose, import as a normal reference layer) remains the
// zero-new-code path for "I need this exact pose in my drawing."
(function () {
  // ---- minimal OBJ parser (v / vn / f only) ----
  function parseOBJ(text) {
    var positions = [], normals = [], faces = [];
    var lines = text.split('\n');
    for (var i = 0; i < lines.length; i++) {
      var line = lines[i].trim();
      if (line[0] === '#' || !line) continue;
      var parts = line.split(/\s+/);
      var tag = parts[0];
      if (tag === 'v') positions.push([+parts[1], +parts[2], +parts[3]]);
      else if (tag === 'vn') normals.push([+parts[1], +parts[2], +parts[3]]);
      else if (tag === 'f') {
        var idx = [];
        for (var k = 1; k < parts.length; k++) {
          var comp = parts[k].split('/');
          idx.push({ v: parseInt(comp[0], 10) - 1, n: comp[2] ? parseInt(comp[2], 10) - 1 : -1 });
        }
        // fan-triangulate n-gons (OBJ faces are convex polygons in practice)
        for (var t = 1; t < idx.length - 1; t++) faces.push([idx[0], idx[t], idx[t + 1]]);
      }
    }
    if (!positions.length || !faces.length) return null;
    // Flatten to a plain vertex buffer (position+normal per triangle
    // corner) — simplest correct approach for a viewer, no index reuse.
    var verts = [], norms = [];
    var haveNormals = normals.length > 0;
    faces.forEach(function (tri) {
      var pA = positions[tri[0].v], pB = positions[tri[1].v], pC = positions[tri[2].v];
      var faceNormal = null;
      if (!haveNormals || tri[0].n < 0) {
        var ux = pB[0] - pA[0], uy = pB[1] - pA[1], uz = pB[2] - pA[2];
        var vx = pC[0] - pA[0], vy = pC[1] - pA[1], vz = pC[2] - pA[2];
        var nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx;
        var len = Math.hypot(nx, ny, nz) || 1;
        faceNormal = [nx / len, ny / len, nz / len];
      }
      tri.forEach(function (c) {
        var p = positions[c.v];
        verts.push(p[0], p[1], p[2]);
        var n = (haveNormals && c.n >= 0) ? normals[c.n] : faceNormal;
        norms.push(n[0], n[1], n[2]);
      });
    });
    // Center + normalize scale so any model fills the viewport consistently.
    var minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity, minZ = Infinity, maxZ = -Infinity;
    for (var i2 = 0; i2 < verts.length; i2 += 3) {
      minX = Math.min(minX, verts[i2]); maxX = Math.max(maxX, verts[i2]);
      minY = Math.min(minY, verts[i2 + 1]); maxY = Math.max(maxY, verts[i2 + 1]);
      minZ = Math.min(minZ, verts[i2 + 2]); maxZ = Math.max(maxZ, verts[i2 + 2]);
    }
    var cx = (minX + maxX) / 2, cy = (minY + maxY) / 2, cz = (minZ + maxZ) / 2;
    var radius = Math.max(maxX - minX, maxY - minY, maxZ - minZ) / 2 || 1;
    for (var i3 = 0; i3 < verts.length; i3 += 3) {
      verts[i3] = (verts[i3] - cx) / radius;
      verts[i3 + 1] = (verts[i3 + 1] - cy) / radius;
      verts[i3 + 2] = (verts[i3 + 2] - cz) / radius;
    }
    return { positions: new Float32Array(verts), normals: new Float32Array(norms), triCount: faces.length };
  }

  // Minimal static glTF/VRM reader.  VRM 0.x is a GLB-compatible container;
  // for the drawing viewer we read its rest-pose geometry, normals, UVs and
  // embedded base-colour textures without importing a game-engine dependency.
  // Skeleton posing is intentionally outside this small offline viewer.
  function parseGLB(buf) {
    var dv = new DataView(buf);
    if (dv.getUint32(0, true) !== 0x46546c67) throw new Error('GLB invalide');
    var jsonLen = dv.getUint32(12, true), jsonType = dv.getUint32(16, true);
    if (jsonType !== 0x4e4f534a) throw new Error('GLB sans JSON');
    var gltf = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, 20, jsonLen)));
    var binAt = 20 + jsonLen, binLen = dv.getUint32(binAt, true), binType = dv.getUint32(binAt + 4, true);
    if (binType !== 0x004e4942) throw new Error('GLB sans buffer binaire');
    var bin = buf.slice(binAt + 8, binAt + 8 + binLen);
    function accessor(id) {
      var a = gltf.accessors[id], v = gltf.bufferViews[a.bufferView], comp = { 5126: Float32Array, 5125: Uint32Array, 5123: Uint16Array, 5121: Uint8Array }[a.componentType];
      var width = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 }[a.type];
      if (!comp || !width || v.byteStride) throw new Error('Attribut glTF non pris en charge');
      return { data: new comp(bin, (v.byteOffset || 0) + (a.byteOffset || 0), a.count * width), width: width, count: a.count };
    }
    var batches = [], blobUrls = [];
    (gltf.meshes || []).forEach(function (mesh) {
      (mesh.primitives || []).forEach(function (p) {
        if (!p.attributes || p.attributes.POSITION === undefined || (p.mode !== undefined && p.mode !== 4)) return;
        var pos = accessor(p.attributes.POSITION), nor = p.attributes.NORMAL === undefined ? null : accessor(p.attributes.NORMAL);
        var uv = p.attributes.TEXCOORD_0 === undefined ? null : accessor(p.attributes.TEXCOORD_0);
        var idx = p.indices === undefined ? null : accessor(p.indices);
        var material = (gltf.materials || [])[p.material || 0] || {};
        var pbr = material.pbrMetallicRoughness || {};
        var factor = pbr.baseColorFactor || [0.72, 0.76, 0.9, 1];
        var textureUrl = null;
        if (pbr.baseColorTexture) {
          var tex = (gltf.textures || [])[pbr.baseColorTexture.index];
          var image = tex && (gltf.images || [])[tex.source];
          var imageView = image && gltf.bufferViews[image.bufferView];
          if (imageView) {
            var start = imageView.byteOffset || 0;
            textureUrl = URL.createObjectURL(new Blob([bin.slice(start, start + imageView.byteLength)], { type: image.mimeType || 'image/png' }));
            blobUrls.push(textureUrl);
          }
        }
        var count = idx ? idx.count : pos.count;
        var out = [], norm = [], texCoords = [], col = [];
        for (var i = 0; i < count; i++) {
          var k = idx ? idx.data[i] : i, po = k * pos.width, no = nor ? k * nor.width : 0;
          out.push(pos.data[po], pos.data[po + 1], pos.data[po + 2]);
          norm.push(nor ? nor.data[no] : 0, nor ? nor.data[no + 1] : 1, nor ? nor.data[no + 2] : 0);
          var uo = uv ? k * uv.width : 0;
          texCoords.push(uv ? uv.data[uo] : 0, uv ? uv.data[uo + 1] : 0);
          col.push(factor[0], factor[1], factor[2]);
        }
        batches.push({ positions: new Float32Array(out), normals: new Float32Array(norm), uvs: new Float32Array(texCoords), colors: new Float32Array(col), textureUrl: textureUrl, unlit: !!(material.extensions && material.extensions.KHR_materials_unlit), triCount: count / 3 });
      });
    });
    if (!batches.length) throw new Error('Aucune géométrie triangle');
    // VRM files use their own units/origin.  Normalize the whole avatar,
    // rather than every piece separately, so its proportions remain intact.
    var min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
    batches.forEach(function (batch) { for (var i = 0; i < batch.positions.length; i += 3) for (var axis = 0; axis < 3; axis++) { min[axis] = Math.min(min[axis], batch.positions[i + axis]); max[axis] = Math.max(max[axis], batch.positions[i + axis]); } });
    var center = [(min[0] + max[0]) / 2, (min[1] + max[1]) / 2, (min[2] + max[2]) / 2];
    var radius = Math.max(max[0] - min[0], max[1] - min[1], max[2] - min[2]) / 2 || 1;
    batches.forEach(function (batch) { for (var i = 0; i < batch.positions.length; i += 3) for (var axis = 0; axis < 3; axis++) batch.positions[i + axis] = (batch.positions[i + axis] - center[axis]) / radius; });
    return { batches: batches, blobUrls: blobUrls, triCount: batches.reduce(function (n, batch) { return n + batch.triCount; }, 0) };
  }

  function loadMeshTextures(mesh) {
    return Promise.all((mesh.batches || []).map(function (batch) {
      if (!batch.textureUrl) return null;
      return new Promise(function (resolve) {
        var image = new Image();
        image.onload = function () { batch.image = image; resolve(); };
        image.onerror = function () { resolve(); };
        image.src = batch.textureUrl;
      });
    }));
  }

  // ---- Built-in drawing mannequins --------------------------------------
  // Kept procedural rather than bundled as a third-party character asset:
  // they load instantly/offline, are neutral drawing references, and can be
  // rendered by this small dependency-free viewer.  The character is a
  // deliberately stylised animation mannequin (large head, clear torso and
  // limb masses); the hand is proportioned for construction drawing.
  function meshBuilder() { return { p: [], n: [] }; }
  function tri(m, a, b, c, na, nb, nc) {
    m.p.push(a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]);
    m.n.push(na[0], na[1], na[2], nb[0], nb[1], nb[2], nc[0], nc[1], nc[2]);
  }
  function sphere(m, c, r, sy) {
    var lat = 9, lon = 12, sx = r, sz = r, ry = r * (sy || 1);
    for (var y = 0; y < lat; y++) for (var x = 0; x < lon; x++) {
      function pt(yy, xx) {
        var v = yy / lat, u = xx / lon, th = u * Math.PI * 2, ph = v * Math.PI;
        var nx = Math.sin(ph) * Math.cos(th), ny = Math.cos(ph), nz = Math.sin(ph) * Math.sin(th);
        return { p: [c[0] + nx * sx, c[1] + ny * ry, c[2] + nz * sz], n: [nx, ny, nz] };
      }
      var a = pt(y, x), b = pt(y + 1, x), d = pt(y, x + 1), e = pt(y + 1, x + 1);
      tri(m, a.p, b.p, d.p, a.n, b.n, d.n); tri(m, d.p, b.p, e.p, d.n, b.n, e.n);
    }
  }
  function limb(m, a, b, r) {
    var dx = b[0] - a[0], dy = b[1] - a[1], dz = b[2] - a[2], len = Math.hypot(dx, dy, dz) || 1;
    var w = [dx / len, dy / len, dz / len];
    var up = Math.abs(w[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0];
    var u = [up[1] * w[2] - up[2] * w[1], up[2] * w[0] - up[0] * w[2], up[0] * w[1] - up[1] * w[0]];
    var ul = Math.hypot(u[0], u[1], u[2]) || 1; u = [u[0] / ul, u[1] / ul, u[2] / ul];
    var v = [w[1] * u[2] - w[2] * u[1], w[2] * u[0] - w[0] * u[2], w[0] * u[1] - w[1] * u[0]];
    for (var i = 0; i < 10; i++) {
      var j = (i + 1) % 10, t0 = i * Math.PI * 2 / 10, t1 = j * Math.PI * 2 / 10;
      function ring(c, t) { var nx = u[0] * Math.cos(t) + v[0] * Math.sin(t), ny = u[1] * Math.cos(t) + v[1] * Math.sin(t), nz = u[2] * Math.cos(t) + v[2] * Math.sin(t); return { p: [c[0] + nx * r, c[1] + ny * r, c[2] + nz * r], n: [nx, ny, nz] }; }
      var p0 = ring(a, t0), p1 = ring(a, t1), p2 = ring(b, t0), p3 = ring(b, t1);
      tri(m, p0.p, p2.p, p1.p, p0.n, p2.n, p1.n); tri(m, p1.p, p2.p, p3.p, p1.n, p2.n, p3.n);
    }
    sphere(m, a, r * 1.05); sphere(m, b, r * 1.05);
  }
  function finish(m) { return { positions: new Float32Array(m.p), normals: new Float32Array(m.n), triCount: m.p.length / 9 }; }
  function characterMesh(pose) {
    var m = meshBuilder();
    // head / neck / chest / hips
    sphere(m, [0, 1.23, 0], 0.34, 1.08); limb(m, [0, 0.96, 0], [0, 0.78, 0], 0.105);
    sphere(m, [0, 0.48, 0], 0.32, 1.55); sphere(m, [0, -0.16, 0], 0.28, 0.72);
    var arm = pose === 'action' ? [[-0.26, 0.72, 0], [-0.7, 1.04, -0.12], [-1.02, 1.28, -0.18], [-1.16, 1.16, -0.18], [0.26, 0.72, 0], [0.66, 0.35, 0.16], [0.92, 0.12, 0.24], [1.07, 0.06, 0.25]] : [[-0.26, 0.72, 0], [-0.62, 0.4, 0], [-0.82, 0.05, 0], [-0.91, -0.08, 0], [0.26, 0.72, 0], [0.62, 0.4, 0], [0.82, 0.05, 0], [0.91, -0.08, 0]];
    limb(m, arm[0], arm[1], .115); limb(m, arm[1], arm[2], .09); sphere(m, arm[3], .12, .78); limb(m, arm[4], arm[5], .115); limb(m, arm[5], arm[6], .09); sphere(m, arm[7], .12, .78);
    var leg = pose === 'action' ? [[-0.16, -0.36, 0], [-0.5, -0.84, 0.14], [-0.28, -1.32, 0.28], [-0.22, -1.44, 0.05], [0.16, -0.36, 0], [0.42, -0.72, -0.2], [0.72, -1.08, -0.4], [0.83, -1.17, -0.24]] : [[-0.16, -0.36, 0], [-0.22, -0.84, 0], [-0.25, -1.34, 0], [-0.23, -1.47, 0.12], [0.16, -0.36, 0], [0.22, -0.84, 0], [0.25, -1.34, 0], [0.23, -1.47, 0.12]];
    limb(m, leg[0], leg[1], .15); limb(m, leg[1], leg[2], .115); sphere(m, leg[3], .16, .42); limb(m, leg[4], leg[5], .15); limb(m, leg[5], leg[6], .115); sphere(m, leg[7], .16, .42);
    return finish(m);
  }
  function handMesh(pose) {
    var m = meshBuilder(); sphere(m, [0, 0, 0], .42, 1.18); limb(m, [0, -.46, 0], [0, -.78, .02], .18);
    var spread = pose === 'fist' ? .07 : .19, curl = pose === 'fist' ? -.32 : pose === 'point' ? 0 : .12;
    for (var i = 0; i < 4; i++) {
      var x = (i - 1.5) * spread, base = [x, .28, 0], tipY = pose === 'point' && i === 1 ? 1.1 : .82 - Math.abs(i - 1.5) * .08;
      var mid = [x * 1.15, .57, curl * (i === 1 ? .2 : 1)], tip = [x * 1.24, tipY, curl * 1.8];
      limb(m, base, mid, .095 - i * .006); limb(m, mid, tip, .075 - i * .006);
    }
    limb(m, [-.31, .02, 0], [-.66, .32, .04], .12); limb(m, [-.66, .32, .04], [-.78, .57, .02], .095);
    return finish(m);
  }

  // ---- tiny mat4 helpers (row-major-free, column-major like GL expects) ----
  function mat4Identity() { return new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]); }
  function mat4Multiply(a, b) {
    var o = new Float32Array(16);
    for (var c = 0; c < 4; c++) for (var r = 0; r < 4; r++) {
      o[c * 4 + r] = a[0 * 4 + r] * b[c * 4 + 0] + a[1 * 4 + r] * b[c * 4 + 1] + a[2 * 4 + r] * b[c * 4 + 2] + a[3 * 4 + r] * b[c * 4 + 3];
    }
    return o;
  }
  function mat4RotY(a) { var c = Math.cos(a), s = Math.sin(a); return new Float32Array([c, 0, -s, 0, 0, 1, 0, 0, s, 0, c, 0, 0, 0, 0, 1]); }
  function mat4RotX(a) { var c = Math.cos(a), s = Math.sin(a); return new Float32Array([1, 0, 0, 0, 0, c, s, 0, 0, -s, c, 0, 0, 0, 0, 1]); }
  function mat4Perspective(fovy, aspect, near, far) {
    var f = 1 / Math.tan(fovy / 2), nf = 1 / (near - far);
    return new Float32Array([f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) * nf, -1, 0, 0, 2 * far * near * nf, 0]);
  }
  function mat4Translate(x, y, z) { var m = mat4Identity(); m[12] = x; m[13] = y; m[14] = z; return m; }

  var VS = 'attribute vec3 aPos; attribute vec3 aNorm; attribute vec3 aColor; attribute vec2 aUV; uniform mat4 uModel; uniform mat4 uView; uniform mat4 uProj; varying vec3 vNorm; varying vec3 vColor; varying vec2 vUV;' +
    'void main(){ vNorm = mat3(uModel) * aNorm; vColor = aColor; vUV = aUV; gl_Position = uProj * uView * uModel * vec4(aPos,1.0); }';
  var FS = 'precision mediump float; varying vec3 vNorm; varying vec3 vColor; varying vec2 vUV; uniform vec3 uLightDir; uniform vec3 uColor; uniform sampler2D uTex; uniform float uHasTex; uniform float uUnlit;' +
    'void main(){ vec4 sample = texture2D(uTex, vUV); vec4 albedo = mix(vec4(vColor,1.0), sample * vec4(vColor,1.0), uHasTex); float diffuse = max(dot(normalize(vNorm), normalize(uLightDir)), 0.15); float light = mix(diffuse, 1.0, uUnlit); gl_FragColor = vec4(uColor * albedo.rgb * light, albedo.a); }';

  function compileProgram(gl) {
    function sh(type, src) { var s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s); if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; }
    var prog = gl.createProgram();
    gl.attachShader(prog, sh(gl.VERTEX_SHADER, VS));
    gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FS));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog));
    return prog;
  }

  var panel = null, raf = null, glState = null;
  function close() {
    if (raf) cancelAnimationFrame(raf);
    if (glState && glState.blobUrls) glState.blobUrls.forEach(function (url) { URL.revokeObjectURL(url); });
    raf = null; glState = null;
    if (panel) { panel.remove(); panel = null; }
  }

  function openMeshReference(mesh, title, type) {
    close();
    panel = document.createElement('div');
    panel.id = 'labs-3dref';
    panel.style.cssText = 'position:fixed;top:70px;right:16px;width:320px;height:320px;z-index:9999;background:#141318;border:1px solid rgba(255,255,255,.14);border-radius:10px;overflow:hidden;box-shadow:0 12px 40px rgba(0,0,0,.5);cursor:grab;';
    var cv = document.createElement('canvas');
    cv.width = 320; cv.height = 320; cv.style.cssText = 'width:100%;height:100%;display:block;';
    var closeBtn = document.createElement('button');
    closeBtn.textContent = '×';
    closeBtn.style.cssText = 'position:absolute;top:4px;right:6px;background:rgba(0,0,0,.4);color:#fff;border:none;border-radius:4px;width:20px;height:20px;cursor:pointer;';
    closeBtn.addEventListener('click', function (e) { e.stopPropagation(); close(); });
    var label = document.createElement('div');
    label.textContent = title || SM.t('labsRef3dDefaultTitle');
    label.style.cssText = 'position:absolute;top:7px;left:9px;color:rgba(255,255,255,.8);font:600 10px system-ui;pointer-events:none;text-shadow:0 1px 2px #000;';
    var views = document.createElement('div');
    views.style.cssText = 'position:absolute;left:8px;bottom:8px;display:flex;gap:4px;cursor:default;';
    [[SM.t('labsRef3dViewFront'), 0, 0], [SM.t('labsRef3dViewProfile'), Math.PI / 2, 0], [SM.t('labsRef3dViewBack'), Math.PI, 0], [SM.t('labsRef3dViewReset'), .6, -.3]].forEach(function (v) { var b = document.createElement('button'); b.textContent = v[0]; b.style.cssText = 'font:10px system-ui;padding:3px 5px;border:0;border-radius:4px;background:rgba(12,13,20,.7);color:#eee;cursor:pointer;'; b.addEventListener('pointerdown', function (e) { e.stopPropagation(); if (glState) { glState.yaw = v[1]; glState.pitch = v[2]; } }); views.appendChild(b); });
    panel.appendChild(cv); panel.appendChild(label); panel.appendChild(closeBtn); panel.appendChild(views);
    document.body.appendChild(panel);

    // preserveDrawingBuffer:true — WITHOUT it (the WebGL default), the
    // browser auto-clears the drawing buffer to transparent black right
    // after each compositor present, on its own schedule, independent of
    // our draw() calls. Live-found: reading the canvas back (readPixels,
    // and a plain screenshot) immediately after open3DReference()
    // sometimes showed real content and sometimes came back fully
    // transparent (0,0,0,0), non-deterministically, depending purely on
    // whether a compositor auto-clear had run since the last draw() — the
    // classic symptom of this flag being off in a floating/overlay canvas
    // that isn't guaranteed to redraw every single compositor frame.
    var gl = cv.getContext('webgl', { preserveDrawingBuffer: true }) || cv.getContext('experimental-webgl', { preserveDrawingBuffer: true });
    if (!gl) { if (typeof showToast === 'function') showToast(SM.t('labsToastWebglUnavailable')); close(); return false; }
    var prog = compileProgram(gl);
    var aPos = gl.getAttribLocation(prog, 'aPos'), aNorm = gl.getAttribLocation(prog, 'aNorm'), aColor = gl.getAttribLocation(prog, 'aColor'), aUV = gl.getAttribLocation(prog, 'aUV');
    var uModel = gl.getUniformLocation(prog, 'uModel'), uView = gl.getUniformLocation(prog, 'uView'), uProj = gl.getUniformLocation(prog, 'uProj');
    var uLight = gl.getUniformLocation(prog, 'uLightDir'), uColor = gl.getUniformLocation(prog, 'uColor'), uTex = gl.getUniformLocation(prog, 'uTex'), uHasTex = gl.getUniformLocation(prog, 'uHasTex'), uUnlit = gl.getUniformLocation(prog, 'uUnlit');
    var sourceBatches = mesh.batches || [{ positions: mesh.positions, normals: mesh.normals, colors: mesh.colors, triCount: mesh.triCount }];
    function makeBuffer(data) { var buffer = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buffer); gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW); return buffer; }
    var whiteTex = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, whiteTex); gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
    var batches = sourceBatches.map(function (batch) {
      var texture = whiteTex;
      if (batch.image) { texture = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, texture); gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true); gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, batch.image); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE); }
      return { pos: makeBuffer(batch.positions), norm: makeBuffer(batch.normals), color: makeBuffer(batch.colors || new Float32Array(batch.positions.length).fill(1)), uv: makeBuffer(batch.uvs || new Float32Array(batch.positions.length / 3 * 2)), texture: texture, hasTexture: !!batch.image, unlit: !!batch.unlit, count: batch.triCount * 3 };
    });
    gl.enable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.clearColor(0.078, 0.075, 0.094, 1);

    glState = { yaw: 0.6, pitch: -0.3, zoom: type === 'hand' ? 2.7 : 3.4, triCount: mesh.triCount, type: type || 'obj', blobUrls: mesh.blobUrls || [] };
    function draw() {
      if (!glState) return;
      gl.viewport(0, 0, cv.width, cv.height);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.useProgram(prog);
      var model = mat4Multiply(mat4RotY(glState.yaw), mat4RotX(glState.pitch));
      var view = mat4Translate(0, 0, -glState.zoom);
      var proj = mat4Perspective(Math.PI / 4, cv.width / cv.height, 0.1, 100);
      gl.uniformMatrix4fv(uModel, false, model);
      gl.uniformMatrix4fv(uView, false, view);
      gl.uniformMatrix4fv(uProj, false, proj);
      gl.uniform3f(uLight, 0.4, 0.6, 1.0);
      if (glState.type === 'hand') gl.uniform3f(uColor, 0.88, 0.52, 0.42);
      else if (glState.type === 'avatar') gl.uniform3f(uColor, 1, 1, 1);
      else gl.uniform3f(uColor, 0.48, 0.66, 0.94);
      gl.uniform1i(uTex, 0);
      batches.forEach(function (batch) {
        gl.bindBuffer(gl.ARRAY_BUFFER, batch.pos); gl.enableVertexAttribArray(aPos); gl.vertexAttribPointer(aPos, 3, gl.FLOAT, false, 0, 0);
        gl.bindBuffer(gl.ARRAY_BUFFER, batch.norm); gl.enableVertexAttribArray(aNorm); gl.vertexAttribPointer(aNorm, 3, gl.FLOAT, false, 0, 0);
        gl.bindBuffer(gl.ARRAY_BUFFER, batch.color); gl.enableVertexAttribArray(aColor); gl.vertexAttribPointer(aColor, 3, gl.FLOAT, false, 0, 0);
        gl.bindBuffer(gl.ARRAY_BUFFER, batch.uv); gl.enableVertexAttribArray(aUV); gl.vertexAttribPointer(aUV, 2, gl.FLOAT, false, 0, 0);
        gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, batch.texture);
        gl.uniform1f(uHasTex, batch.hasTexture ? 1 : 0); gl.uniform1f(uUnlit, batch.unlit ? 1 : 0);
        gl.drawArrays(gl.TRIANGLES, 0, batch.count);
      });
    }
    glState.draw = draw; // exposed for redraw3D() — see its own comment
    (function loop() { draw(); raf = requestAnimationFrame(loop); })();
    draw(); // paint the first frame synchronously — don't wait on rAF,
            // which browsers throttle/pause for backgrounded or
            // not-yet-composited tabs (found live: readPixels() right
            // after open3DReference() returned all-transparent (0,0,0,0),
            // not even the clear color, because no rAF tick had run yet)

    var dragging = false, lastX = 0, lastY = 0;
    panel.addEventListener('pointerdown', function (e) { if (e.target === closeBtn) return; dragging = true; lastX = e.clientX; lastY = e.clientY; panel.style.cursor = 'grabbing'; });
    window.addEventListener('pointermove', function (e) {
      if (!dragging || !glState) return;
      glState.yaw += (e.clientX - lastX) * 0.01;
      glState.pitch += (e.clientY - lastY) * 0.01;
      lastX = e.clientX; lastY = e.clientY;
    });
    window.addEventListener('pointerup', function () { dragging = false; if (panel) panel.style.cursor = 'grab'; });
    panel.addEventListener('wheel', function (e) { e.preventDefault(); if (glState) glState.zoom = Math.max(1.2, Math.min(15, glState.zoom + e.deltaY * 0.01)); }, { passive: false });

    return { triCount: mesh.triCount };
  }
  window.SMLabs.open3DReference = function (objText) {
    var mesh = parseOBJ(objText);
    if (!mesh) { if (typeof showToast === 'function') showToast(SM.t('labsToastObjInvalidOrEmpty')); return false; }
    return openMeshReference(mesh, SM.t('labsRef3dTitleObj'), 'obj');
  };
  window.SMLabs.openCC0AvatarReference = async function () {
    try {
      var res = await fetch('assets/reference-3d/AvatarSample_D.vrm');
      if (!res.ok) throw new Error('asset absent (' + res.status + ')');
      var mesh = parseGLB(await res.arrayBuffer());
      await loadMeshTextures(mesh);
      return openMeshReference(mesh, SM.t('labsRef3dTitleAvatar'), 'avatar');
    } catch (e) {
      console.error('[reference-3d] avatar CC0', e);
      if (typeof showToast === 'function') showToast(SM.t('labsToastAvatarCC0UnavailablePrefix') + e.message);
      return false;
    }
  };
  window.SMLabs.openCC0HandReference = async function () {
    try {
      var res = await fetch('assets/reference-3d/CC0_Rigged_Arms.obj');
      if (!res.ok) throw new Error('asset absent (' + res.status + ')');
      var mesh = parseOBJ(await res.text());
      if (!mesh) throw new Error('OBJ invalide');
      return openMeshReference(mesh, SM.t('labsRef3dTitleHands'), 'hand');
    } catch (e) {
      console.error('[reference-3d] hands CC0', e);
      if (typeof showToast === 'function') showToast(SM.t('labsToastHandsCC0UnavailablePrefix') + e.message);
      return false;
    }
  };
  window.SMLabs.openCharacterReference = function (pose) { return openMeshReference(characterMesh(pose === 'action' ? 'action' : 'neutral'), pose === 'action' ? SM.t('labsRef3dTitleCharacterAction') : SM.t('labsRef3dTitleCharacterNeutral'), 'character'); };
  window.SMLabs.openHandReference = function (pose) { pose = pose === 'fist' || pose === 'point' ? pose : 'open'; return openMeshReference(handMesh(pose), SM.t('labsRef3dTitleHandPrefix') + (pose === 'fist' ? SM.t('labsRef3dHandFist') : pose === 'point' ? SM.t('labsRef3dHandPoint') : SM.t('labsRef3dHandOpen')), 'hand'); };
  window.SMLabs.set3DRotation = function (yaw, pitch) { if (glState) { glState.yaw = yaw; glState.pitch = pitch; if (glState.draw) glState.draw(); } };
  window.SMLabs.redraw3D = function () { if (glState && glState.draw) glState.draw(); };
  window.SMLabs.close3DReference = close;
  window.SMLabs.get3DState = function () { return glState ? { yaw: glState.yaw, pitch: glState.pitch, zoom: glState.zoom, triCount: glState.triCount } : null; };

  window.SMLabs.register('reference-3d', {
    flag: 'nemo-labs-3dref',
    describe: 'labsDescribeReference3d',
    onEnable: function () { window.SMLabs.openCC0AvatarReference(); },
    onDisable: close,
  });
})();
