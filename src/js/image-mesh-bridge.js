// Native image-mesh panel and presentation-only canvas handles.
(function () {
  'use strict';

  var SVG = 'http://www.w3.org/2000/svg';
  var IDLE = 'rgba(74,158,255,1)';
  var ACCENT = 'rgba(255,184,108,1)';
  var WHITE = 'rgba(255,255,255,1)';
  var WIRE = 'rgba(120,170,255,.51)';
  var OUTLINE = 'rgba(255,184,108,.86)';
  var editing = false;
  var cached = null;
  var serial = 0;
  var readQueue = Promise.resolve();
  var overlay = null;
  var paintQueued = false;
  var drag = null;

  function el(id) { return document.getElementById(id); }
  function toast(message) { if (window.showToast) window.showToast(message, 'warn'); }
  function samePin(a, b) {
    return !!a && !!b && ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId']
      .every(function (k) { return a[k] === b[k]; });
  }
  function proofNow() {
    var ui = window.NemoNativeObjectUI;
    if (!ui || typeof ui.readImageMeshSelection !== 'function') return null;
    try {
      var proof = ui.readImageMeshSelection(window);
      if (!proof || !proof.session || typeof proof.session.readImageMesh !== 'function' ||
          !proof.selector || typeof proof.selector.contextId !== 'string' || typeof proof.selector.layerUid !== 'string' ||
          !Number.isSafeInteger(proof.selector.frame) || proof.selector.frame < 0 || !proof.info || !proof.info.instanceId ||
          !proof.info.documentId || !Number.isSafeInteger(proof.info.contentRevision) || !Number.isSafeInteger(proof.info.lifecycleGeneration) ||
          !proof.info.documentSnapshotId || typeof proof.current !== 'function' || proof.current() !== true) return null;
      return proof;
    } catch (_) { return null; }
  }
  function keyOf(proof) {
    var i = proof.info, s = proof.selector;
    return [s.contextId, s.layerUid, s.frame, i.instanceId, i.documentId, i.lifecycleGeneration,
      i.contentRevision, i.documentSnapshotId];
  }
  function sameKey(a, b) { return !!a && !!b && a.key.length === b.length && a.key.every(function (v, i) { return v === b[i]; }); }
  function currentCache() { return cached && cached.proof.current() === true ? cached : null; }
  function refresh() {
    var proof = proofNow(), key = proof && keyOf(proof), prior = currentCache();
    if (!proof) {
      if (cached) { serial++; cached = null; cancelDrag(); }
      paintPanel(); schedulePaint(); return Promise.resolve(null);
    }
    if (prior && sameKey(prior, key)) { cached.proof = proof; paintPanel(); schedulePaint(); return prior.promise || Promise.resolve(prior.read); }
    if (cached) cancelDrag();
    serial++;
    var token = serial;
    var entry = { proof: proof, key: key, read: null, promise: null };
    cached = entry;
    paintPanel(); schedulePaint();
    entry.promise = readQueue.catch(function () {}).then(function () {
      if (token !== serial || proof.current() !== true) return null;
      return proof.session.readImageMesh(proof.selector, proof.info);
    }).then(function (read) {
      if (token !== serial || cached !== entry || proof.current() !== true || !read || !samePin(read.info, proof.info)) return null;
      entry.read = read;
      paintPanel(); schedulePaint();
      return read;
    }).catch(function (error) {
      if (token === serial && cached === entry) { entry.error = error; paintPanel(); schedulePaint(); }
      return null;
    }).finally(function () { if (cached === entry) entry.promise = null; });
    readQueue = entry.promise.catch(function () {});
    return entry.promise;
  }
  function usable() {
    var entry = currentCache();
    return entry && entry.read && samePin(entry.read.info, entry.proof.info) ? entry : null;
  }
  function adapter() {
    var module = window.NemoNativeObjectImageMesh;
    return module && typeof module.desktop === 'function' ? module.desktop(window) : null;
  }
  function liveGuard(entry) {
    return function (pin, ctx) { return entry.proof.current(ctx) === true && samePin(pin, entry.read.info); };
  }
  function invoke(name, args, entry) {
    var api = adapter();
    if (!api || typeof api[name] !== 'function') return Promise.reject(Object.assign(new Error('Native image mesh is unavailable'), { code: 'unavailable' }));
    args.push(liveGuard(entry));
    return Promise.resolve(api[name].apply(api, args));
  }
  function showFailure(error) {
    var message = error && error.message ? error.message : 'Native image mesh is unavailable.';
    if (error && error.code) message += ' (' + error.code + (error.reason ? ': ' + error.reason : '') + ')';
    toast(message);
  }
  function refreshAfter(receipt) {
    if (receipt && receipt.applied === false) return refresh();
    return refresh();
  }
  function toggleMesh(on) {
    var entry = usable();
    if (!entry) { refresh(); return Promise.resolve(false); }
    var read = entry.read, target = read.target;
    if (!target || read.meshId !== null && !read.meshId) { toast('Native image mesh target is unavailable.'); return Promise.resolve(false); }
    var operation;
    if (on) {
      if (read.meshId) return Promise.resolve(true);
      if (!window.SMImageMesh || typeof SMImageMesh.createMesh !== 'function') return Promise.resolve(false);
      var initial = SMImageMesh.createMesh({ cols: 4, rows: 4 });
      operation = invoke('attach', [target, initial], entry);
    } else {
      if (!read.meshId) { editing = false; refresh(); return Promise.resolve(true); }
      operation = invoke('detach', [target], entry).then(function (receipt) { editing = false; return receipt; });
    }
    return operation.then(refreshAfter).then(function () { return true; }).catch(function (error) { showFailure(error); refresh(); return false; });
  }
  function densityOf(raw) {
    var module = window.NemoNativeObjectImageMesh;
    return module && typeof module.densityOf === 'function' ? module.densityOf(raw) : Math.max(1, Math.min(32, parseInt(raw, 10) || 4));
  }
  function setDensity() {
    var entry = usable();
    if (!entry || !entry.read.meshId) { refresh(); return Promise.resolve(false); }
    var cols = el('p-imagemesh-cols'), rows = el('p-imagemesh-rows');
    var c = densityOf(cols && cols.value), r = densityOf(rows && rows.value);
    return invoke('setDensity', [entry.read.meshId, c, r], entry).then(refreshAfter).then(function () { return true; })
      .catch(function (error) { showFailure(error); refresh(); return false; });
  }
  function resetPose() {
    var entry = usable();
    if (!entry || !entry.read.meshId) { refresh(); return Promise.resolve(false); }
    return invoke('resetOffsets', [entry.read.meshId], entry).then(refreshAfter).then(function () { return true; })
      .catch(function (error) { showFailure(error); refresh(); return false; });
  }
  function setEditing(value) {
    var entry = usable();
    editing = !!value && !!(entry && entry.read.meshId);
    paintPanel(); schedulePaint();
  }
  function paintPanel() {
    var section = el('p-imagemesh-sec');
    if (!section) return;
    var entry = currentCache();
    if (!entry) { section.style.display = 'none'; return; }
    section.hidden = false;
    section.style.display = '';
    var read = entry.read, has = !!(read && read.meshId);
    if (!has) editing = false;
    var on = el('p-imagemesh-on'); if (on) { on.checked = has; on.disabled = !read; }
    var edit = el('p-imagemesh-edit'); if (edit) { edit.checked = editing; edit.disabled = !has; }
    var body = el('p-imagemesh-body'); if (body) body.style.display = has ? '' : 'none';
    if (!has) return;
    var mesh = read.mesh;
    var cols = el('p-imagemesh-cols'), rows = el('p-imagemesh-rows');
    if (cols && document.activeElement !== cols) cols.value = mesh.cols === undefined ? 4 : mesh.cols;
    if (rows && document.activeElement !== rows) rows.value = mesh.rows === undefined ? 4 : mesh.rows;
    var info = el('p-imagemesh-info');
    if (info) info.textContent = mesh.verts.length + ' / ' + (mesh.tris.length / 3);
  }
  window.renderImageMeshPanel = refresh;

  function mappingFor(read, info) {
    var mod = window.NemoNativeViewportUnderlay, map = mod && mod.canvasMapping;
    if (!map || !map.bounds || !map.point || !map.screen || !map.matchesView || !map.matchesView(window, read.presentation)) return null;
    var bounds = map.bounds(window, info);
    if (!bounds) return null;
    return { api: map, bounds: bounds, presentation: read.presentation,
      canvasW: info.canvasW, canvasH: info.canvasH };
  }
  function effectiveMesh(read) {
    if (!drag || drag.entry.read !== read || !drag.moved) return read.mesh;
    var source = read.mesh, offsets = source.offsets ? source.offsets.map(function (p) { return p.slice(); }) :
      source.verts.map(function () { return [0, 0]; });
    offsets[drag.index] = drag.offset.slice();
    return { outline: source.outline, verts: source.verts, tris: source.tris, offsets: offsets };
  }
  function screenPoints(entry, read, map) {
    var mesh = effectiveMesh(read), world = window.SMImageMesh && SMImageMesh.worldVerts(mesh, read.rect);
    if (!world) return null;
    return world.map(function (p) { return map.api.screen({ x: p[0], y: p[1] }, map); });
  }
  function ensureOverlay() {
    if (overlay && overlay.isConnected) return overlay;
    overlay = document.createElementNS(SVG, 'svg');
    overlay.setAttribute('aria-hidden', 'true');
    overlay.style.position = 'fixed'; overlay.style.left = '0'; overlay.style.top = '0';
    overlay.style.width = '100vw'; overlay.style.height = '100vh'; overlay.style.overflow = 'visible';
    overlay.style.pointerEvents = 'none'; overlay.style.zIndex = '900';
    (document.body || document.documentElement).appendChild(overlay);
    return overlay;
  }
  function node(name, attrs) {
    var n = document.createElementNS(SVG, name);
    Object.keys(attrs).forEach(function (k) { n.setAttribute(k, attrs[k]); });
    return n;
  }
  function drawLine(root, a, b, color, width) {
    root.appendChild(node('line', { x1: a.x, y1: a.y, x2: b.x, y2: b.y, stroke: color, 'stroke-width': width, 'vector-effect': 'non-scaling-stroke' }));
  }
  function drawOverlay() {
    var svg = ensureOverlay();
    while (svg.firstChild) svg.removeChild(svg.firstChild);
    var entry = usable();
    if (!editing || !entry || !entry.read.mesh || !window.SMImageMesh) return;
    var read = entry.read, map = mappingFor(read, entry.proof.info), pts = map && screenPoints(entry, read, map), mesh = effectiveMesh(read);
    if (!pts) return;
    for (var i = 0; i + 2 < mesh.tris.length; i += 3) {
      var a = pts[mesh.tris[i]], b = pts[mesh.tris[i + 1]], c = pts[mesh.tris[i + 2]];
      if (!a || !b || !c) continue;
      drawLine(svg, a, b, WIRE, 1); drawLine(svg, b, c, WIRE, 1); drawLine(svg, c, a, WIRE, 1);
    }
    for (var o = 0; o < mesh.outline.length; o++) drawLine(svg, pts[o], pts[(o + 1) % mesh.outline.length], OUTLINE, 1.6);
    pts.forEach(function (p, index) {
      var r = drag && drag.index === index ? 5 : 3.5;
      if (SMImageMesh.isOutlineVertex(mesh, index)) svg.appendChild(node('rect', {
        x: p.x - r, y: p.y - r, width: 2 * r, height: 2 * r, rx: 0.5,
        fill: drag && drag.index === index ? WHITE : ACCENT, stroke: WHITE, 'stroke-width': 1,
      }));
      else svg.appendChild(node('circle', {
        cx: p.x, cy: p.y, r: r, fill: drag && drag.index === index ? WHITE : IDLE, stroke: WHITE, 'stroke-width': 1,
      }));
    });
  }
  function schedulePaint() {
    if (paintQueued) return;
    paintQueued = true;
    window.requestAnimationFrame(function () { paintQueued = false; drawOverlay(); });
  }
  function areaFor(e) {
    var area = document.getElementById('canvas-area');
    return area && e.target && area.contains(e.target) ? area : null;
  }
  function cancelDrag() {
    if (!drag) return;
    var old = drag; drag = null;
    try { if (old.area && old.area.hasPointerCapture && old.area.hasPointerCapture(old.pointerId)) old.area.releasePointerCapture(old.pointerId); } catch (_) {}
    schedulePaint();
  }
  function onDown(e) {
    if (drag || !editing || !areaFor(e)) return;
    var entry = usable();
    if (!entry || !entry.read.mesh || entry.proof.current() !== true || !window.SMImageMesh) return;
    var map = mappingFor(entry.read, entry.proof.info), pts = map && screenPoints(entry, entry.read, map);
    if (!pts) return;
    var best = -1, distance = 10;
    pts.forEach(function (p, index) {
      var d = Math.hypot(p.x - e.clientX, p.y - e.clientY);
      if (d < distance) { best = index; distance = d; }
    });
    if (best < 0) return;
    drag = { entry: entry, pointerId: e.pointerId, index: best, area: areaFor(e), map: map,
      startOffset: entry.read.mesh.offsets && entry.read.mesh.offsets[best] ? entry.read.mesh.offsets[best].slice() : [0, 0],
      offset: null, moved: false };
    try { if (drag.area.setPointerCapture) drag.area.setPointerCapture(e.pointerId); } catch (_) {}
    e.stopImmediatePropagation(); e.preventDefault();
  }
  function onMove(e) {
    if (!drag || e.pointerId !== drag.pointerId) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var entry = drag.entry, read = entry.read;
    if (cached !== entry || entry.proof.current() !== true || !samePin(read.info, entry.proof.info)) { cancelDrag(); return; }
    try {
      var world = drag.map.api.point(e, drag.map), uv = SMImageMesh.normalizedOf(read.rect, world.x, world.y);
      var rest = read.mesh.verts[drag.index];
      if (!uv || !rest || !uv.every(Number.isFinite)) { cancelDrag(); return; }
      drag.offset = [uv[0] - rest[0], uv[1] - rest[1]];
      drag.moved = Math.hypot(drag.offset[0] - drag.startOffset[0], drag.offset[1] - drag.startOffset[1]) > 1e-12;
      schedulePaint();
    } catch (_) { cancelDrag(); }
  }
  function onUp(e) {
    if (!drag || e.pointerId !== drag.pointerId) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var done = drag; drag = null;
    try { if (done.area.hasPointerCapture && done.area.hasPointerCapture(done.pointerId)) done.area.releasePointerCapture(done.pointerId); } catch (_) {}
    schedulePaint();
    if (!done.moved || !done.offset || done.entry.proof.current() !== true || cached !== done.entry) return;
    invoke('setOffset', [done.entry.read.meshId, done.index, done.offset[0], done.offset[1]], done.entry)
      .then(refreshAfter).catch(function (error) { showFailure(error); refresh(); });
  }
  function onCancel(e) { if (drag && e.pointerId === drag.pointerId) cancelDrag(); }

  function init() {
    var on = el('p-imagemesh-on');
    if (on) on.addEventListener('change', function () { toggleMesh(on.checked); });
    var edit = el('p-imagemesh-edit');
    if (edit) edit.addEventListener('change', function () { setEditing(edit.checked); });
    var cols = el('p-imagemesh-cols'), rows = el('p-imagemesh-rows');
    if (cols) cols.addEventListener('change', setDensity);
    if (rows) rows.addEventListener('change', setDensity);
    var reset = el('btn-imagemesh-reset'); if (reset) reset.addEventListener('click', resetPose);
    document.addEventListener('pointerdown', function (e) { if (areaFor(e)) onDown(e); }, { capture: true });
    document.addEventListener('pointermove', function (e) { if (drag) onMove(e); }, { capture: true });
    document.addEventListener('pointerup', function (e) { if (drag) onUp(e); }, { capture: true });
    document.addEventListener('pointercancel', function (e) { if (drag) onCancel(e); }, { capture: true });
    document.addEventListener('lostpointercapture', function (e) { if (drag) onCancel(e); }, { capture: true });
    window.addEventListener('resize', schedulePaint);
    refresh();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();

  // Retained only for the obsolete WASM scene hook; native presentation owns mesh rendering.
  window.buildImageMeshOverlayItems = function () { return []; };
  window.SMImageMeshUI = {
    refresh: refresh,
    canMesh: function () { return !!proofNow(); },
    hasMesh: function () { var entry = usable(); return !!(entry && entry.read.meshId); },
    toggleMesh: function () { var entry = usable(); return toggleMesh(!(entry && entry.read.meshId)); },
    isEditing: function () { return editing; },
    canMaskWithShape: function () { return false; },
    maskImageWithShape: function () {
      var e = Object.assign(new Error('Native shape selection is unavailable'), { code: 'unavailable', reason: 'native_shape_selection_unavailable' });
      showFailure(e); return Promise.resolve({ ok: false, code: e.code, reason: e.reason });
    },
    toggleEditing: function () { var entry = usable(); if (entry && entry.read.meshId) setEditing(!editing); },
  };
}());
