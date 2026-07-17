// ---- LABS PROTOTYPE — 3D reference viewer (Umoupen OBJ reference, scoped) ----
// feature-scouting.md #4 flagged three.js as too heavy a dependency for
// one reference-viewing feature. This is the honestly-scoped build
// instead: a from-scratch, dependency-free WebGL viewer — a minimal OBJ
// parser (v/vn/f only, no materials/textures/groups) + ~60 lines of raw
// GLSL (one directional light, flat vertex-color-free Lambert shading)
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

  var VS = 'attribute vec3 aPos; attribute vec3 aNorm; uniform mat4 uModel; uniform mat4 uView; uniform mat4 uProj; varying vec3 vNorm;' +
    'void main(){ vNorm = mat3(uModel) * aNorm; gl_Position = uProj * uView * uModel * vec4(aPos,1.0); }';
  var FS = 'precision mediump float; varying vec3 vNorm; uniform vec3 uLightDir; uniform vec3 uColor;' +
    'void main(){ float diff = max(dot(normalize(vNorm), normalize(uLightDir)), 0.15); gl_FragColor = vec4(uColor * diff, 1.0); }';

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
    raf = null; glState = null;
    if (panel) { panel.remove(); panel = null; }
  }

  window.SMLabs.open3DReference = function (objText) {
    var mesh = parseOBJ(objText);
    if (!mesh) { if (typeof showToast === 'function') showToast('OBJ invalide ou vide (v/f manquants)'); return false; }
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
    panel.appendChild(cv); panel.appendChild(closeBtn);
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
    if (!gl) { if (typeof showToast === 'function') showToast('WebGL indisponible ici'); close(); return false; }
    var prog = compileProgram(gl);
    var posBuf = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, posBuf); gl.bufferData(gl.ARRAY_BUFFER, mesh.positions, gl.STATIC_DRAW);
    var normBuf = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, normBuf); gl.bufferData(gl.ARRAY_BUFFER, mesh.normals, gl.STATIC_DRAW);
    var aPos = gl.getAttribLocation(prog, 'aPos'), aNorm = gl.getAttribLocation(prog, 'aNorm');
    var uModel = gl.getUniformLocation(prog, 'uModel'), uView = gl.getUniformLocation(prog, 'uView'), uProj = gl.getUniformLocation(prog, 'uProj');
    var uLight = gl.getUniformLocation(prog, 'uLightDir'), uColor = gl.getUniformLocation(prog, 'uColor');
    gl.enable(gl.DEPTH_TEST);
    gl.clearColor(0.078, 0.075, 0.094, 1);

    glState = { yaw: 0.6, pitch: -0.3, zoom: 3, triCount: mesh.triCount };
    function draw() {
      if (!glState) return;
      gl.viewport(0, 0, cv.width, cv.height);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.useProgram(prog);
      gl.bindBuffer(gl.ARRAY_BUFFER, posBuf); gl.enableVertexAttribArray(aPos); gl.vertexAttribPointer(aPos, 3, gl.FLOAT, false, 0, 0);
      gl.bindBuffer(gl.ARRAY_BUFFER, normBuf); gl.enableVertexAttribArray(aNorm); gl.vertexAttribPointer(aNorm, 3, gl.FLOAT, false, 0, 0);
      var model = mat4Multiply(mat4RotY(glState.yaw), mat4RotX(glState.pitch));
      var view = mat4Translate(0, 0, -glState.zoom);
      var proj = mat4Perspective(Math.PI / 4, cv.width / cv.height, 0.1, 100);
      gl.uniformMatrix4fv(uModel, false, model);
      gl.uniformMatrix4fv(uView, false, view);
      gl.uniformMatrix4fv(uProj, false, proj);
      gl.uniform3f(uLight, 0.4, 0.6, 1.0);
      gl.uniform3f(uColor, 0.55, 0.62, 0.95);
      gl.drawArrays(gl.TRIANGLES, 0, mesh.triCount * 3);
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
  };
  window.SMLabs.set3DRotation = function (yaw, pitch) { if (glState) { glState.yaw = yaw; glState.pitch = pitch; if (glState.draw) glState.draw(); } };
  window.SMLabs.redraw3D = function () { if (glState && glState.draw) glState.draw(); };
  window.SMLabs.close3DReference = close;
  window.SMLabs.get3DState = function () { return glState ? { yaw: glState.yaw, pitch: glState.pitch, zoom: glState.zoom, triCount: glState.triCount } : null; };

  window.SMLabs.register('reference-3d', {
    flag: 'nemo-labs-3dref',
    describe: '3D reference viewer OBJ (Umoupen, scope réaliste sans three.js — voir feature-scouting #4) : SMLabs.open3DReference(objText) — WebGL brut, orbite au drag, jamais exporté/baké',
    onDisable: close,
  });
})();
