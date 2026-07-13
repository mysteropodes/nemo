// ---- MOTION MODE (v1, 2026-07) ----
// A second animation paradigm alongside Animation 2D's frame-by-frame
// drawing: After-Effects-style property keyframing (Position/Rotation/
// Scale/Opacity per layer), reusing the SAME layers ("on doit retrouver
// nos éléments dedans" — the user's own requirement) so the two modes stay
// two VIEWS of one project, not two separate documents.
//
// Engine: a generalized copy of camera.js's proven per-segment cubic-bezier
// ease + spatial-bezier-handle math (position gets real motion-path
// curvature via hOut/hIn, same as the camera's framing keys already do;
// rotation/scale/opacity are scalar lerps along the same eased t). Kept as
// a SEPARATE small copy of bezierEase rather than a shared import — same
// call this codebase already makes for other tiny stable pure-math pairs
// (CLAUDE.md §3's JS/Rust duplicates) — must stay in sync with camera.js's
// copy if the easing math itself ever changes.
//
// CRITICAL save-safety constraint (CLAUDE.md's "family of bug #1" — a new
// per-frame effect that mutates the LIVE Paper.js layer would get baked
// into saveActiveLayerFrame()/serP() on the very next save, permanently
// corrupting the original drawing with the motion offset): the motion
// transform is applied ONLY inside buildSceneJson()'s own item-serialization
// pass (engine-bridge.js hook below), never to userLayers[i] itself. The
// live Paper.js layer stays exactly as drawn; only the JSON handed to the
// Rust renderer (and, symmetrically, export.js's own frame builder) gets
// translated/rotated/scaled/faded.
(function () {
  var DEFAULT_EASE = [0.42, 0, 0.58, 1]; // easeInOut, same default as camera.js
  var PROPS = ['position', 'rotation', 'scale', 'opacity'];
  var PROP_LABEL = { position: 'Position', rotation: 'Rotation', scale: 'Scale', opacity: 'Opacity' };
  var PROP_DIM = { position: 2, rotation: 1, scale: 2, opacity: 1 };
  var PROP_UNIT = { position: 'px', rotation: '°', scale: '%', opacity: '%' };
  var PROP_DEFAULT = { position: [0, 0], rotation: [0], scale: [100, 100], opacity: [100] };
  var PROP_ICON = {
    position: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="2.5" fill="currentColor" stroke="none"/><path d="M12 3v4M12 17v4M3 12h4M17 12h4"/></svg>',
    rotation: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 12a8 8 0 1 1-2.6-5.9"/><path d="M20 4v5h-5"/></svg>',
    scale: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/></svg>',
    opacity: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"/><path d="M12 3a9 9 0 0 0 0 18Z" fill="currentColor" stroke="none"/></svg>',
  };

  // ---- easing math (see header comment: deliberate copy of camera.js) ----
  function bezierEase(t, x1, y1, x2, y2) {
    if (t <= 0) return 0;
    if (t >= 1) return 1;
    function bx(u) { var v = 1 - u; return 3 * v * v * u * x1 + 3 * v * u * u * x2 + u * u * u; }
    function by(u) { var v = 1 - u; return 3 * v * v * u * y1 + 3 * v * u * u * y2 + u * u * u; }
    function dbx(u) { var v = 1 - u; return 3 * v * v * x1 + 6 * v * u * (x2 - x1) + 3 * u * u * (1 - x2); }
    var u = t;
    for (var i = 0; i < 8; i++) {
      var d = dbx(u);
      if (Math.abs(d) < 1e-6) break;
      var err = bx(u) - t;
      if (Math.abs(err) < 1e-5) return by(u);
      u = Math.max(0, Math.min(1, u - err / d));
    }
    var lo = 0, hi = 1;
    for (var j = 0; j < 24; j++) { u = (lo + hi) / 2; if (bx(u) < t) lo = u; else hi = u; }
    return by(u);
  }

  // ---- data model: state.layers[i].motion = {position:{keys:[...]}, ...},
  // state.layers[i].motionStatic = {position:[x,y], ...} for a property
  // that has a non-default value but ISN'T keyframed (stopwatch off) ----
  function ensureTrack(ld, prop) {
    if (!ld.motion) ld.motion = {};
    if (!ld.motion[prop]) ld.motion[prop] = { keys: [] };
    return ld.motion[prop];
  }
  function hasKeys(ld, prop) { return !!(ld.motion && ld.motion[prop] && ld.motion[prop].keys.length); }
  function isAnimated(ld, prop) { return hasKeys(ld, prop); }
  function sortKeys(track) { track.keys.sort(function (a, b) { return a.frame - b.frame; }); }
  function keyAt(track, frame) { return track.keys.find(function (k) { return k.frame === frame; }) || null; }
  function staticValue(ld, prop) {
    var st = ld.motionStatic && ld.motionStatic[prop];
    return st ? st.slice() : PROP_DEFAULT[prop].slice();
  }

  // The value of `prop` on layer `ld` at `frame` — exact key, interpolated,
  // clamped outside the keyed range, the static override, or the neutral
  // default. Always returns an array (length 1 or 2, per PROP_DIM).
  function valueAtFrame(ld, prop, frame) {
    var track = ld.motion && ld.motion[prop];
    if (!track || !track.keys.length) return staticValue(ld, prop);
    var ks = track.keys;
    if (frame <= ks[0].frame) return ks[0].v.slice();
    var last = ks[ks.length - 1];
    if (frame >= last.frame) return last.v.slice();
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1];
      if (frame >= a.frame && frame < b.frame) {
        var t = (frame - a.frame) / (b.frame - a.frame);
        var e = a.ease || DEFAULT_EASE;
        var y = bezierEase(t, e[0], e[1], e[2], e[3]);
        // Position gets real spatial-bezier curvature through its
        // hOut/hIn handles (same construction as camera.js's motion
        // path) whenever either handle is non-zero — a straight [0,0]
        // handle collapses to the plain linear-in-eased-t case below.
        if (prop === 'position' && ((a.hOut && (a.hOut[0] || a.hOut[1])) || (b.hIn && (b.hIn[0] || b.hIn[1])))) {
          var ho = a.hOut || [0, 0], hi = b.hIn || [0, 0];
          var p1x = a.v[0] + ho[0], p1y = a.v[1] + ho[1], p2x = b.v[0] + hi[0], p2y = b.v[1] + hi[1];
          var v = 1 - y;
          var px = v * v * v * a.v[0] + 3 * v * v * y * p1x + 3 * v * y * y * p2x + y * y * y * b.v[0];
          var py = v * v * v * a.v[1] + 3 * v * v * y * p1y + 3 * v * y * y * p2y + y * y * y * b.v[1];
          return [px, py];
        }
        var out = [];
        for (var d = 0; d < a.v.length; d++) out.push(a.v[d] + (b.v[d] - a.v[d]) * y);
        return out;
      }
    }
    return last.v.slice();
  }
  // The segment whose ease governs `frame` (its LEFT key) — same contract
  // as camera.js's segmentLeftKey, generalized to any track.
  function segmentLeftKey(track, frame) {
    var ks = track.keys;
    if (ks.length < 2) return null;
    if (frame >= ks[ks.length - 1].frame) return ks[ks.length - 2];
    for (var i = ks.length - 2; i >= 0; i--) if (frame >= ks[i].frame) return ks[i];
    return ks[0];
  }

  function setKeyAtCurrentFrame(ld, prop, values) {
    var track = ensureTrack(ld, prop);
    var k = keyAt(track, state.currentFrame);
    if (k) { k.v = values.slice(); }
    else {
      track.keys.push({ frame: state.currentFrame, v: values.slice(), ease: DEFAULT_EASE.slice(), hOut: [0, 0], hIn: [0, 0] });
      sortKeys(track);
    }
    return keyAt(track, state.currentFrame);
  }
  function removeKeyAtCurrentFrame(ld, prop) {
    var track = ld.motion && ld.motion[prop];
    if (!track) return;
    var i = track.keys.findIndex(function (k) { return k.frame === state.currentFrame; });
    if (i >= 0) track.keys.splice(i, 1);
  }
  // Stopwatch toggle: OFF→ON starts animating from the CURRENT effective
  // value (matches AE: turning on keyframing never jumps the value); ON→OFF
  // freezes at the current interpolated value as a static override, same
  // "manual edit wins" principle CLAUDE.md documents for fill-merge —
  // switching modes must never silently snap a layer back to its neutral
  // default.
  function toggleAnimated(ld, prop) {
    if (isAnimated(ld, prop)) {
      var v = valueAtFrame(ld, prop, state.currentFrame);
      if (!ld.motion) ld.motion = {};
      ld.motion[prop] = { keys: [] };
      if (!ld.motionStatic) ld.motionStatic = {};
      ld.motionStatic[prop] = v;
    } else {
      var cur = staticValue(ld, prop);
      ensureTrack(ld, prop).keys = [{ frame: state.currentFrame, v: cur, ease: DEFAULT_EASE.slice(), hOut: [0, 0], hIn: [0, 0] }];
    }
  }
  // Editing a value field: if animated, this is a scrub at the CURRENT
  // frame — auto-adds/updates a key there (AE convention). If not
  // animated, it's just the static override.
  function setValue(ld, prop, values) {
    if (isAnimated(ld, prop)) setKeyAtCurrentFrame(ld, prop, values);
    else { if (!ld.motionStatic) ld.motionStatic = {}; ld.motionStatic[prop] = values.slice(); }
  }

  // ---- render-time transform (engine-bridge.js hook — see header
  // comment: NEVER applied to the live userLayers[i], only to the JSON
  // items handed to the renderer, so a save can never bake this in) ----
  function rotPt(px, py, cx, cy, deg) {
    if (!deg) return [px, py];
    var a = deg * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
    var dx = px - cx, dy = py - cy;
    return [cx + dx * c - dy * s, cy + dx * s + dy * c];
  }
  function rotVec(dx, dy, deg) {
    if (!deg) return [dx, dy];
    var a = deg * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
    return [dx * c - dy * s, dx * s + dy * c];
  }
  // Null when the layer has no motion at all this frame (the overwhelmingly
  // common case) — callers skip the per-item transform pass entirely then.
  function layerMotionAt(li, frameIdx) {
    var ld = state.layers[li];
    // Defense in depth: the UI already blocks expanding a component-instance
    // layer's Transform group (see renderLayerListMotion), but stale motion
    // data can still exist (set before the layer became an instance, or
    // hand-edited into a project file) — never double-apply on top of the
    // instance's own symMatrix pivot at render time either.
    if (!ld || ld.symbolId || (!ld.motion && !ld.motionStatic)) return null;
    var pos = valueAtFrame(ld, 'position', frameIdx);
    var rot = valueAtFrame(ld, 'rotation', frameIdx)[0];
    var scl = valueAtFrame(ld, 'scale', frameIdx);
    var op = valueAtFrame(ld, 'opacity', frameIdx)[0];
    if (!pos[0] && !pos[1] && !rot && scl[0] === 100 && scl[1] === 100 && op === 100) return null;
    return { dx: pos[0], dy: pos[1], rot: rot, sx: scl[0] / 100, sy: scl[1] / 100, op: Math.max(0, op / 100) };
  }
  // Transforms one item's already-built segments array (engine-bridge.js's
  // {point,handleIn,handleOut} triples, handles as RELATIVE offsets — see
  // serP/lottieShapeValue's shared convention) around `pivot`. Scale+rotate
  // happen in the pivot's local frame, translate last — matches the order
  // camera.js's own applyToExportLayer uses (scale, rotate, then reposition).
  function transformSegments(segments, pivot, m) {
    return segments.map(function (s) {
      var lx = (s.point[0] - pivot.x) * m.sx, ly = (s.point[1] - pivot.y) * m.sy;
      var r = rotPt(pivot.x + lx, pivot.y + ly, pivot.x, pivot.y, m.rot);
      var out = { point: [r[0] + m.dx, r[1] + m.dy] };
      if (s.handleIn) { var hi = rotVec(s.handleIn[0] * m.sx, s.handleIn[1] * m.sy, m.rot); out.handleIn = hi; }
      if (s.handleOut) { var ho = rotVec(s.handleOut[0] * m.sx, s.handleOut[1] * m.sy, m.rot); out.handleOut = ho; }
      return out;
    });
  }
  // Transforms a raster item's on-canvas rect. v1 scope: scale + translate
  // only (skips rotation for images — the axis-aligned {x,y,width,height}
  // the renderer expects has no rotation field; rare case since almost all
  // Nemo content is vector strokes, noted as a known v1 limitation).
  function transformImageRect(rb, pivot, m) {
    var cx = rb.x + rb.width / 2, cy = rb.y + rb.height / 2;
    var ncx = pivot.x + (cx - pivot.x) * m.sx + m.dx, ncy = pivot.y + (cy - pivot.y) * m.sy + m.dy;
    var w = rb.width * m.sx, h = rb.height * m.sy;
    return { x: ncx - w / 2, y: ncy - h / 2, width: w, height: h };
  }

  // ---- canvas overlay: the position motion path for the layer(s)
  // currently expanded in the Motion panel — same dashed-bezier + handle
  // visual language as camera.js's own trajectory overlay. ----
  function circleSegs(cx, cy, r) {
    var k = r * 0.5523;
    return [
      { point: [cx + r, cy], handleIn: [0, k], handleOut: [0, -k] },
      { point: [cx, cy - r], handleIn: [k, 0], handleOut: [-k, 0] },
      { point: [cx - r, cy], handleIn: [0, -k], handleOut: [0, k] },
      { point: [cx, cy + r], handleIn: [-k, 0], handleOut: [k, 0] },
    ];
  }
  function buildOverlayItems() {
    // `!= null` (not a truthy check): layer index 0 is a valid, common
    // expanded-layer value and must not be treated the same as "nothing
    // expanded" — a bare `!window._motionExpandedLayer` bug hid the motion
    // path overlay specifically for the first layer in the panel.
    if (state.appMode !== 'motion' || window._motionExpandedLayer == null) return [];
    var li = window._motionExpandedLayer;
    var ld = state.layers[li];
    if (!ld || !hasKeys(ld, 'position')) return [];
    var track = ld.motion.position;
    var items = [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    var pathCol = [63, 107, 245, 200]; // --accent
    var handleCol = [255, 170, 40, 220];
    var ks = track.keys;
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1];
      var ho = a.hOut || [0, 0], hi = b.hIn || [0, 0];
      var pts = [];
      var steps = 24;
      for (var s = 0; s <= steps; s++) {
        var t = s / steps, v = 1 - t;
        pts.push({
          point: [
            v * v * v * a.v[0] + 3 * v * v * t * (a.v[0] + ho[0]) + 3 * v * t * t * (b.v[0] + hi[0]) + t * t * t * b.v[0],
            v * v * v * a.v[1] + 3 * v * v * t * (a.v[1] + ho[1]) + 3 * v * t * t * (b.v[1] + hi[1]) + t * t * t * b.v[1],
          ],
        });
      }
      items.push({ segments: pts, closed: false, fillColor: null, strokeColor: pathCol, strokeWidth: 1.5 * zs, dashPattern: [5 * zs, 4 * zs] });
    }
    ks.forEach(function (k, ki) {
      var isCur = k.frame === state.currentFrame;
      items.push({ segments: circleSegs(k.v[0], k.v[1], (isCur ? 6 : 4.5) * zs), closed: true, fillColor: isCur ? [255, 170, 40, 255] : [230, 230, 230, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1.2 * zs });
      // Spatial handles, same discoverable small-dot pattern as camera.js
      var hs = [];
      if (ki < ks.length - 1) hs.push(k.hOut || [0, 0]);
      if (ki > 0) hs.push(k.hIn || [0, 0]);
      hs.forEach(function (h) {
        if (!h[0] && !h[1]) return;
        var hx = k.v[0] + h[0], hy = k.v[1] + h[1];
        items.push({ segments: [{ point: [k.v[0], k.v[1]] }, { point: [hx, hy] }], closed: false, fillColor: null, strokeColor: handleCol, strokeWidth: 1 * zs });
        items.push({ segments: circleSegs(hx, hy, 4 * zs), closed: true, fillColor: handleCol, strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
      });
    });
    return items;
  }

  // ---- Motion mode UI: layer list (Transform property rows) ----
  function fmtVal(n) { return Math.round(n * 10) / 10; }
  function scrubField(value, onCommit) {
    var inp = document.createElement('input');
    inp.type = 'number'; inp.className = 'pi scrub motion-val'; inp.value = fmtVal(value);
    inp.step = 1;
    inp.addEventListener('change', function () { onCommit(parseFloat(inp.value) || 0); });
    inp.addEventListener('click', function (e) { e.stopPropagation(); });
    inp.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    return inp;
  }
  function renderLayerListMotion(list) {
    var order = (typeof computeLayerRenderOrder === 'function') ? computeLayerRenderOrder() : state.layers.map(function (_l, i) { return { type: 'layer', idx: i }; });
    order.forEach(function (entry) {
      if (entry.type !== 'layer' || entry.hidden) return;
      var li = entry.idx, ld = state.layers[li];
      // Component instances already have their own placement transform
      // (symMatrix, dragged on canvas) plus Frame/Speed/Offset — stacking
      // Motion's keyframed Position/Rotation/Scale on top would pivot around
      // a DIFFERENT center (userLayers[i].bounds vs symMatrix's own pivot)
      // and fight the instance panel silently. Same precedent as Ghost All
      // refusing symbolId layers (timeline.js) — block expansion here rather
      // than let the two transforms produce confusing, uneditable-looking
      // results.
      var isComponent = !!ld.symbolId;
      var expanded = window._motionExpandedLayer === li;
      var row = document.createElement('div');
      row.className = 'lrow' + (li === state.activeLayerIdx ? ' act' : '') + (isComponent ? ' motion-disabled' : '');
      if (isComponent) row.title = 'Motion mode ne gère pas encore les instances de composant (utilise Frame/Speed/Offset dans le panneau du calque)';
      var arrow = document.createElement('div'); arrow.className = 'lico larrow'; arrow.textContent = isComponent ? '·' : (expanded ? '▾' : '▸');
      var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = ld.name || ('Layer ' + (li + 1));
      row.appendChild(arrow); row.appendChild(nm);
      row.addEventListener('click', function () {
        if (isComponent) { state.activeLayerIdx = li; renderLayerList(); return; }
        window._motionExpandedLayer = expanded ? null : li;
        state.activeLayerIdx = li;
        renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
      list.appendChild(row);
      if (!expanded) return;
      var grp = document.createElement('div'); grp.className = 'lrow motion-group-row'; grp.textContent = 'Transform';
      list.appendChild(grp);
      PROPS.forEach(function (prop) {
        var pr = document.createElement('div'); pr.className = 'lrow motion-prop-row';
        var sw = document.createElement('div');
        sw.className = 'lico motion-stopwatch' + (isAnimated(ld, prop) ? ' on' : '');
        sw.title = isAnimated(ld, prop) ? 'Désactiver l’animation (fige la valeur actuelle)' : 'Activer l’animation de cette propriété';
        sw.innerHTML = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="13" r="8"/><path d="M12 9v4l3 2"/><path d="M9 2h6"/></svg>';
        sw.addEventListener('click', function (e) {
          e.stopPropagation(); pushUndo(); toggleAnimated(ld, prop);
          renderLayerList(); renderTimeline();
          if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
        });
        var pnm = document.createElement('div'); pnm.className = 'lnm motion-prop-name';
        pnm.innerHTML = PROP_ICON[prop] + '<span>' + PROP_LABEL[prop] + '</span>';
        pr.appendChild(sw); pr.appendChild(pnm);
        var vals = isAnimated(ld, prop) ? valueAtFrame(ld, prop, state.currentFrame) : staticValue(ld, prop);
        var fieldWrap = document.createElement('div'); fieldWrap.className = 'motion-fields';
        for (var d = 0; d < PROP_DIM[prop]; d++) {
          (function (dim) {
            var f = scrubField(vals[dim], function (nv) {
              pushUndo();
              var nvals = isAnimated(ld, prop) ? valueAtFrame(ld, prop, state.currentFrame) : staticValue(ld, prop);
              nvals[dim] = nv;
              setValue(ld, prop, nvals);
              renderTimeline();
              if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
            });
            fieldWrap.appendChild(f);
          })(d);
        }
        var unit = document.createElement('span'); unit.className = 'motion-unit'; unit.textContent = PROP_UNIT[prop];
        fieldWrap.appendChild(unit);
        pr.appendChild(fieldWrap);
        list.appendChild(pr);
      });
    });
  }

  // ---- Motion mode UI: keyframe tracks (mirrors the layer list's rows) ----
  function trackRowHtml(ld, prop, rowEl) {
    rowEl.innerHTML = '';
    rowEl.style.position = 'relative';
    var track = ld.motion && ld.motion[prop];
    var w = state.totalFrames * FC;
    var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', w); svg.setAttribute('height', 34);
    svg.style.cssText = 'position:absolute;left:0;top:0;pointer-events:none;';
    rowEl.appendChild(svg);
    if (track && track.keys.length) {
      // Connection bar between consecutive keys whose value actually
      // changes — AE-wishlist idea (sandervandijk.tv "Connection"/"Keyframe
      // Duration"): makes it obvious at a glance WHERE movement happens
      // instead of a row of identical-looking diamonds with no context.
      for (var i = 0; i < track.keys.length - 1; i++) {
        var a = track.keys[i], b = track.keys[i + 1];
        var changed = a.v.some(function (v, d) { return v !== b.v[d]; });
        if (!changed) continue;
        var rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        rect.setAttribute('x', a.frame * FC + FC / 2); rect.setAttribute('y', 14);
        rect.setAttribute('width', (b.frame - a.frame) * FC); rect.setAttribute('height', 6);
        rect.setAttribute('fill', 'var(--accent)'); rect.setAttribute('opacity', '0.35');
        svg.appendChild(rect);
      }
    }
    for (var fi = 0; fi < state.totalFrames; fi++) {
      var c = document.createElement('div');
      c.className = 'fc motion-fc' + (fi === state.currentFrame ? ' cur' : '');
      c.dataset.frame = fi;
      var k = track ? keyAt(track, fi) : null;
      if (k) {
        var dia = document.createElement('div');
        dia.className = 'motion-key' + (fi === state.currentFrame ? ' cur' : '');
        c.appendChild(dia);
      }
      (function (frameIdx, key) {
        c.addEventListener('mousedown', function (e) {
          e.stopPropagation();
          if (key) { window._motionKeyDrag = { ld: ld, prop: prop, key: key, startX: e.clientX, startFrame: key.frame }; }
          goToFrame(frameIdx);
        });
        c.addEventListener('contextmenu', function (e) {
          e.preventDefault(); e.stopPropagation();
          goToFrame(frameIdx);
          window.showContextMenu(e.clientX, e.clientY, [
            key
              ? { label: 'Supprimer cette clé', action: function () { pushUndo(); var tr = ld.motion[prop]; tr.keys.splice(tr.keys.indexOf(key), 1); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } }
              : { label: 'Ajouter une clé ici', action: function () { pushUndo(); setKeyAtCurrentFrame(ld, prop, isAnimated(ld, prop) ? valueAtFrame(ld, prop, frameIdx) : staticValue(ld, prop)); renderLayerList(); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } },
          ]);
        });
      })(fi, k);
      rowEl.appendChild(c);
    }
  }
  function renderTimelineMotion(grid) {
    var order = (typeof computeLayerRenderOrder === 'function') ? computeLayerRenderOrder() : state.layers.map(function (_l, i) { return { type: 'layer', idx: i }; });
    order.forEach(function (entry) {
      if (entry.type !== 'layer' || entry.hidden) return;
      var li = entry.idx, ld = state.layers[li];
      var expanded = window._motionExpandedLayer === li;
      var spacer = document.createElement('div'); spacer.className = 'frow';
      grid.appendChild(spacer);
      if (!expanded) return;
      var grpSpacer = document.createElement('div'); grpSpacer.className = 'frow';
      grid.appendChild(grpSpacer);
      PROPS.forEach(function (prop) {
        var row = document.createElement('div'); row.className = 'frow motion-track-row';
        trackRowHtml(ld, prop, row);
        grid.appendChild(row);
      });
    });
  }
  // Drag-to-retime a keyframe (mousemove/up delegated from ui.js's global
  // pointer handlers via SMMotion.onDragMove/onDragUp, same pattern as the
  // span-end/keyframe drag handlers already in timeline.js).
  function onDragMove(e) {
    var d = window._motionKeyDrag; if (!d) return;
    var deltaFrames = Math.round((e.clientX - d.startX) / FC);
    var nf = Math.max(0, Math.min(state.totalFrames - 1, d.startFrame + deltaFrames));
    if (nf === d.key.frame) return;
    var track = d.ld.motion[d.prop];
    if (keyAt(track, nf)) return; // don't stomp an existing key
    d.key.frame = nf; sortKeys(track);
    renderTimeline();
  }
  function onDragUp() {
    if (!window._motionKeyDrag) return;
    window._motionKeyDrag = null;
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  document.addEventListener('mousemove', onDragMove);
  document.addEventListener('mouseup', onDragUp);

  // ---- mode switching ----
  function setAppMode(mode) {
    if (state.appMode === mode) return;
    state.appMode = mode;
    document.querySelectorAll('.app-mode-btn').forEach(function (b) { b.classList.toggle('active', b.dataset.mode === mode); });
    document.body.classList.toggle('mode-motion', mode === 'motion');
    renderLayerList(); renderTimeline();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function initModeSwitch() {
    document.querySelectorAll('.app-mode-btn').forEach(function (b) {
      if (b.disabled) return;
      b.addEventListener('click', function () { setAppMode(b.dataset.mode); });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initModeSwitch); else initModeSwitch();

  window.SMMotion = {
    valueAtFrame: valueAtFrame,
    isAnimated: isAnimated,
    layerMotionAt: layerMotionAt,
    transformSegments: transformSegments,
    transformImageRect: transformImageRect,
    buildOverlayItems: buildOverlayItems,
    renderLayerListMotion: renderLayerListMotion,
    renderTimelineMotion: renderTimelineMotion,
    setAppMode: setAppMode,
  };
})();
