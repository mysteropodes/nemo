/* Selection input adapter: transient scalar intent, never document authority. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoSelectCanvasIntent = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function consume(e) { e.stopImmediatePropagation(); e.preventDefault(); }
  function create(ports) {
    ['context', 'probe', 'select'].forEach(function (name) {
      if (!ports || typeof ports[name] !== 'function') throw new TypeError('Missing pointer port: ' + name);
    });
    var pending = null, captureTarget = null, gesture = null;
    function valid() {
      try { return pending.context === ports.context(); }
      catch (_) { return false; }
    }
    // #2372: a drag gesture (marquee, move, scale, rotate) rides on the pending press; cancelling the press cancels it with no write.
    function drop() { var g = gesture; gesture = null; if (g) try { g.cancel(); } catch (_) {} }
    function cancel(e) {
      if (!pending || e && e.pointerId !== undefined && e.pointerId !== pending.id) return false;
      var id = pending.id, target = captureTarget;
      drop();
      pending = null; captureTarget = null;
      if (target && target.releasePointerCapture &&
          (!target.hasPointerCapture || target.hasPointerCapture(id))) {
        try { target.releasePointerCapture(id); } catch (_) {}
      }
      return true;
    }
    function hover(e) { if (typeof ports.hover === 'function') try { ports.hover(e); } catch (_) {} }
    function down(e) {
      consume(e);
      hover(null);
      cancel();
      if (e.button !== 0 || !Number.isSafeInteger(e.pointerId) ||
          !Number.isFinite(e.clientX) || !Number.isFinite(e.clientY)) return;
      try {
        var context = ports.context(), decision = ports.probe(e);
        if (!decision || !Object.isFrozen(decision)) return;
        pending = { id: e.pointerId, x: e.clientX, y: e.clientY,
          context: context, decision: decision, shift: !!e.shiftKey, blocked: false };
        captureTarget = e.currentTarget;
        if (captureTarget && captureTarget.setPointerCapture) captureTarget.setPointerCapture(e.pointerId);
        if (ports.gesture && decision.mode === 'pick') gesture = ports.gesture.begin(e, function () { cancel(); }) || null;
      } catch (_) { cancel(); }
    }
    function move(e) {
      consume(e);
      if (!pending) return hover(e);
      if (e.pointerId !== pending.id) return;
      if (gesture) try { gesture.move(e); } catch (_) {}
      if (pending.blocked) return;
      if (!valid() || e.buttons !== 1 || !Number.isFinite(e.clientX) || !Number.isFinite(e.clientY) ||
          Math.hypot(e.clientX - pending.x, e.clientY - pending.y) > 3) pending.blocked = true;
    }
    function up(e) {
      consume(e);
      if (!pending) return;
      if (e.pointerId !== pending.id) return;
      var accept = !pending.blocked && valid() && Number.isFinite(e.clientX) && Number.isFinite(e.clientY) &&
        Math.hypot(e.clientX - pending.x, e.clientY - pending.y) <= 3;
      var decision = pending.decision, shift = pending.shift, g = gesture;
      gesture = null; cancel();
      if (g) try { // a drag commits through the gesture; a click that never dragged selects, except on a handle (which selects nothing)
        if (!accept) return g.end(e);
        g.cancel();
        if (g.owns()) return;
      } catch (_) { return; }
      if (accept) try { ports.select(decision, shift); } catch (_) {}
    }
    function cancelEvent(e) { consume(e); cancel(e); }
    function snapshot() {
      return pending ? Object.freeze({ id: pending.id, x: pending.x, y: pending.y,
        context: pending.context, decision: pending.decision, blocked: pending.blocked }) : null;
    }
    return Object.freeze({ down: down, move: move, up: up, cancel: cancel,
      cancelEvent: cancelEvent, snapshot: snapshot });
  }
  function hitHandles(h, pt, altHeld, zoom) {
    if (!h) return null;
    // NOTE (2026-08-31): `pt` stays in WORLD space here on purpose.
    // computeHandles already maps its corners/ring/anchor through the
    // layer's Motion transform (its own `WP` helper), so both sides are
    // world-space and agree. Converting the point here — as
    // hitTestOrientedBoxA2D genuinely must, since orientedBoxForPath is
    // raw geometry — was tried and broke the drag outright: with a
    // selection live, the mismatched point matched a handle, onDown took
    // the handle branch and never reached the branch that sets mode
    // 'move', so a shape could be selected and then not moved at all.
    var tol = 9 / zoom;
    // Anchor crosshair — checked FIRST/exclusively, but ONLY while Alt is
    // held (live feedback 2026-07: "ça peut être confusant quand il faut
    // déplacer un petit élément" — a small object's own body can fall
    // within the anchor's hit tolerance, so an unconditional grab there
    // silently moved the PIVOT instead of the object with no way to tell
    // which one just happened). Without Alt, a click in that same spot now
    // falls through to the normal move/marquee logic below — Alt+drag is
    // otherwise free on the Select tool (viewtools-bridge.js's global
    // Alt-drag-rotate never reaches here anyway: this file's onDown always
    // stopImmediatePropagation()s first while the Select tool is active),
    // so repurposing it for "grab the anchor" doesn't collide with
    // anything. A default (center) anchor sits nowhere near a resize
    // handle so this never shadows them in the common case; when a preset
    // corner anchor DOES coincide with its own resize handle, grabbing the
    // anchor (Alt held) is what the user is more likely reaching for right
    // there, so it still wins that specific tie.
    if (h.anchorPos && altHeld) {
      var dAnchor = pt.getDistance(h.anchorPos);
      if (dAnchor < tol) return { type: 'anchor' };
    }
    // Ring band test — anywhere within ~7px of the circumference counts,
    // not just a single point, checked before the corners since the 16px
    // margin baked into ringRadius already keeps it clear of them.
    var ringTol = 7 / zoom;
    if (Math.abs(pt.getDistance(h.ringCenter) - h.ringRadius) < ringTol) return { type: 'rotate' };
    var bestD = tol, best = null;
    Object.keys(h.corners).forEach(function (k) {
      var d = pt.getDistance(h.corners[k]);
      if (d < bestD) { bestD = d; best = { type: 'scale', dir: k }; }
    });
    if (best) return best;
    // Skew zones (2026-08) — checked only once no corner/edge scale handle
    // matched, so a resize handle always wins a tie. Two small hit-zones
    // flank each edge's midpoint handle, running ALONG that edge, starting
    // just past the scale handle's own tolerance (SKEW_INNER) so the two
    // never overlap. Mirrors buildTransformBoxItems' (engine-bridge.js) own
    // tick-mark geometry exactly — the two must agree, or a mark could be
    // drawn somewhere a click here won't recognize.
    var skewHit = skewZoneHitTest(h, pt, zoom);
    if (skewHit) return skewHit;
    return null;
  }
  var SKEW_MIN_EDGE_PX = 48, SKEW_INNER_PX = 9, SKEW_OUTER_PX = 20;
  var EDGE_ENDPOINTS = { n: ['nw', 'ne'], s: ['sw', 'se'], w: ['nw', 'sw'], e: ['ne', 'se'] };
  function skewZoneHitTest(h, pt, zoom) {
    var zs = 1 / zoom;
    var found = null;
    Object.keys(EDGE_ENDPOINTS).forEach(function (k) {
      if (found) return;
      var ep = EDGE_ENDPOINTS[k];
      var a = h.corners[ep[0]], b2 = h.corners[ep[1]];
      var edgeVec = b2.subtract(a);
      var edgeLenPx = edgeVec.length * zoom;
      if (edgeLenPx < SKEW_MIN_EDGE_PX) return;
      var dir = edgeVec.normalize();
      var mid = h.corners[k];
      [-1, 1].forEach(function (side) {
        if (found) return;
        var base = mid.add(dir.multiply(side * (SKEW_INNER_PX + SKEW_OUTER_PX) / 2 * zs));
        var ext = pt.subtract(base);
        var along = ext.dot(dir);
        var perp = ext.subtract(dir.multiply(along)).length;
        if (Math.abs(along) <= (SKEW_OUTER_PX - SKEW_INNER_PX) / 2 * zs && perp <= SKEW_INNER_PX * zs) {
          found = { type: 'skew', edge: k };
        }
      });
    });
    return found;
  }

  // #2362: a native OBJECT project builds no Paper items (native-object-project-file.js projectView), so a Select click
  // and the Labs tools found nothing to hit. They read the presented layer's geometry through the native subselect view
  // instead: transient, never-inserted Paper paths in world space, top-most last, carrying their native identity only
  // (no style, no writer). A read whose presentation changed before it returned is discarded.
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function objectProject(root) {
    try { var s = root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root);
      return s && s.blocksLegacy() === true ? s : null; } catch (_) { return null; }
  }
  function presentedSession(root) {
    var session = objectProject(root);
    var pin = session && session.isActive() && session.info(), shown = pin && session.readPresentedObjectSession();
    return shown && shown.presentation && samePin(pin, shown.presentation) ? { session: session, pin: pin, presentation: shown.presentation } : null;
  }
  // A gesture's own repaint (the knife resumes rendering as it commits) can re-present the canvas while the read is in
  // flight, which the reader refuses as stale; the read is retried on the fresh presentation of the same revision.
  // A listed record's control-point bounds mapped to world space (a cubic lies inside its control polygon).
  function worldBounds(item) {
    var b = item.bounds, m = item.matrix, xs = [], ys = [];
    [[b[0], b[1]], [b[2], b[1]], [b[0], b[3]], [b[2], b[3]]].forEach(function (p) { xs.push(m[0] * p[0] + m[2] * p[1] + m[4]); ys.push(m[1] * p[0] + m[3] * p[1] + m[5]); });
    return [Math.min.apply(null, xs), Math.min.apply(null, ys), Math.max.apply(null, xs), Math.max.apply(null, ys)];
  }
  // #2372: a listed plain stroke's authored width (record space) carried to world space by the item matrix, as the baseline hit the
  // real painted width; 0 for an unstroked record, a ribbon, an image or a text block (they hit by their outline or bounds).
  function worldStrokeWidth(item) {
    var m = item.matrix, w = item.strokeWidth, k = Math.sqrt(Math.abs(m[0] * m[3] - m[1] * m[2]));
    return typeof w === 'number' && w > 0 && Number.isFinite(k) ? w * k : 0;
  }
  // The presented layer through the native hit-test listing: every painted record (role plain | ribbon | fill | shape |
  // brush), geometry paged only for the records `wanted(item, worldBounds)` keeps; with `centres` a kept ribbon also carries
  // its world-space centreline as the baseline item did (data.isVectorBrush, data.centerSegments with record widths).
  async function presented(root, layerIndex, wanted, attempt, centres) {
    var s = presentedSession(root), layer = s && s.pin.layers[layerIndex], data;
    if (!layer) return null;
    try {
      data = await s.session.readPresentedStrokes({ contextId: 'scene-root', layerUid: layer.layerUid, frame: s.presentation.frame }, s.pin,
        function (item) { return !wanted || wanted(item, worldBounds(item)); }, centres);
    } catch (error) {
      if (!error || error.code !== 'stale_revision' || (attempt || 0) >= 3) throw error;
      await new Promise(function (done) { (root.setTimeout || setTimeout)(done, 30); });
      var next = presentedSession(root);
      return next && samePin(next.pin, s.pin) && next.presentation.frame === s.presentation.frame ? presented(root, layerIndex, wanted, (attempt || 0) + 1, centres) : null;
    }
    var now = presentedSession(root);
    if (!now || !samePin(now.pin, s.pin) || now.presentation.frame !== s.presentation.frame) return null;
    var Path = root.Path || root.paper.Path, Matrix = root.Matrix || root.paper.Matrix;
    return { pin: s.pin, presentation: s.presentation, frame: s.presentation.frame, layerUid: layer.layerUid, items: data.items, paths: data.items.filter(function (item) {
      return Array.isArray(item.segments); }).map(function (item) {
      var path = new Path({ insert: false, closed: item.closed, segments: item.segments.map(function (a) { return [a.point, a.handleIn, a.handleOut]; }) });
      path.transform(new Matrix(item.matrix[0], item.matrix[1], item.matrix[2], item.matrix[3], item.matrix[4], item.matrix[5]));
      // Painted outlines (a ribbon, its fill, a shape) and any closed record that carries fill paint are hit inside (as the
      // baseline did a filled Paper item); an unfilled plain stroke only on its line.
      path.strokeColor = [0, 0, 0]; path.strokeWidth = Math.max(1, worldStrokeWidth(item)); if (item.closed && (item.role !== 'plain' || item.filled === true)) path.fillColor = [0, 0, 0];
      path.data = { strokeId: item.stableTarget.strokeId, nativeStableTarget: item.stableTarget, nativeRole: item.role, nativeRibbon: item.role === 'ribbon', nativeMatrix: item.matrix,
        nativeOwner: item.owner, nativeBrush: item.brush, nativeGroup: item.group, nativeStyle: item.style };
      // The record's stored boxAngle (baseline serP/desP data.boxAngle) turned by the presented matrix when that is a pure similarity, so the
      // oriented box is drawn in world space after a frame change, an undo or a reload.
      // #2372 SEL-MASK: a listed mask is the selection the Mask panel edits (the baseline mask Path carried data.isMask / maskMode / maskFeather).
      if (item.role === 'mask') { path.data.isMask = true; path.data.maskMode = item.style && item.style.maskMode || 'add'; path.data.maskFeather = item.style && item.style.maskFeather || 0; }
      var mx = item.matrix, sim = mx[0] * mx[3] - mx[1] * mx[2] > 0 && Math.abs(mx[0] - mx[3]) < 1e-6 * (Math.abs(mx[0]) + 1) && Math.abs(mx[1] + mx[2]) < 1e-6 * (Math.abs(mx[1]) + 1);
      if (item.boxAngle) path.data.boxAngle = (item.boxAngle + (sim ? Math.atan2(mx[1], mx[0]) * 180 / Math.PI : 0)) % 360;
      // #2267: a text block's stored turn is in its listed map (its record space is the unturned frame), so its box is turned by that angle.
      else if (item.role === 'text' && sim && Math.abs(mx[1]) > 1e-9) path.data.boxAngle = (Math.atan2(mx[1], mx[0]) * 180 / Math.PI) % 360;
      if (item.center) { var m = item.matrix, at = function (p, d) { return [m[0] * p[0] + m[2] * p[1] + d * m[4], m[1] * p[0] + m[3] * p[1] + d * m[5]]; };
        path.data.isVectorBrush = true; path.data.centerSegments = item.center.map(function (c) {
          return { point: at(c.point, 1), handleIn: at(c.handleIn, 0), handleOut: at(c.handleOut, 0), width: c.width }; }); }
      return path;
    }) };
  }
  // World-space Paper segments back to a record geometry in the stroke's own space (inverse of its presented matrix).
  function inverse(m) {
    var d = m[0] * m[3] - m[1] * m[2];
    if (!Number.isFinite(d) || Math.abs(d) < 1e-12) throw new Error('Presented transform is singular');
    return function (x, y) { return { x: (m[3] * x - m[2] * y) / d, y: (-m[1] * x + m[0] * y) / d }; };
  }
  // A world-space ribbon centreline ([x, y] anchors) back to the record's own space; widths are the record's own.
  function recordCenter(center, m) {
    var lin = inverse(m);
    return center.map(function (c) { return { point: lin(c.point[0] - m[4], c.point[1] - m[5]), handleIn: lin(c.handleIn[0], c.handleIn[1]),
      handleOut: lin(c.handleOut[0], c.handleOut[1]), width: c.width }; });
  }
  function recordGeometry(path, m) {
    var lin = inverse(m);
    return { kind: 'cubic-path', closed: path.closed, coordinateSpace: 'document-world', handleSpace: 'relative', segments: path.segments.map(function (s) {
      return { point: lin(s.point.x - m[4], s.point.y - m[5]), handleIn: lin(s.handleIn.x, s.handleIn.y), handleOut: lin(s.handleOut.x, s.handleOut.y) }; }) };
  }
  function worldPoint(root, clientX, clientY, pin, presentation) {
    var mapping = root.NemoNativeViewportUnderlay && root.NemoNativeViewportUnderlay.canvasMapping;
    var r = root.document.getElementById('drawing-canvas').getBoundingClientRect();
    var p = mapping.point({ clientX: clientX, clientY: clientY }, { bounds: { left: r.left, top: r.top, width: r.width, height: r.height },
      canvasW: pin.canvasW, canvasH: pin.canvasH, presentation: presentation });
    return { x: p.x, y: p.y, perPixel: pin.canvasW / r.width };
  }
  // A linked fill is selected as its ribbon, as the baseline's resolveBrushAnchor did (the native listing names it).
  function owned(path) {
    var id = path.data.nativeOwner;
    if (path.data.nativeRole !== 'fill' || !id) return path;
    var ribbon = path.clone({ insert: false });
    ribbon.data = Object.assign({}, path.data, { strokeId: id, nativeRole: 'ribbon', nativeRibbon: true, nativeOwner: undefined, nativeStyle: undefined,
      nativeStableTarget: Object.assign({}, path.data.nativeStableTarget, { strokeId: id }) });
    return ribbon;
  }
  // A ribbon carries its linked fill as the baseline Paper item did (data.linkedFill), so the Labs routes that move a
  // selection with its fill (path bake) name both.
  function withFill(path, items) {
    var fill = path.data.nativeRole === 'ribbon' && items.find(function (i) { return i.role === 'fill' && i.owner === path.data.strokeId; });
    if (fill) path.data.linkedFill = { data: { strokeId: fill.stableTarget.strokeId } };
    return path;
  }
  // #2372: a texture dab is selected as its brush's anchor (the baseline resolved a companion to its anchor); a dab whose anchor is not
  // listed stays itself and Select refuses to transform it.
  async function anchored(root, layerIndex, hit, layers) {
    if (hit.data.nativeBrush !== 'copy' || !hit.data.nativeGroup) return hit;
    var anchor = layers.items.find(function (i) { return i.brush === 'anchor' && i.group === hit.data.nativeGroup; });
    var again = anchor && await presented(root, layerIndex, function (item) { return item.stableTarget.strokeId === anchor.stableTarget.strokeId; });
    return again && again.paths[0] ? withFill(again.paths[0], again.items) : hit;
  }
  // The top-most presented stroke within 8 screen pixels of the click, or null, as the baseline did: the active layer is
  // tested first, then every other visible unlocked layer from the topmost down (#2372). The hit carries its layer
  // (data.nativeLayerIndex/Uid) so applyPick can activate it. Only records whose bounds reach the click have their geometry
  // read; a hidden or locked layer (the active one too) is never read. An integer `only` tests that one layer alone (the Tween
  // Reassign flow clicks the elements of its own layer, as the baseline layer.hitTest did).
  async function pick(root, clientX, clientY, only) {
    var s = presentedSession(root);
    if (!s) return undefined;
    var w = worldPoint(root, clientX, clientY, s.pin, s.presentation), t = 8 * w.perPixel, active = root.state.activeLayerIdx;
    var Point = root.Point || root.paper.Point, at = new Point(w.x, w.y), order = [active], reach = function (item, b) {
      var r = t + worldStrokeWidth(item) / 2;
      return w.x >= b[0] - r && w.x <= b[2] + r && w.y >= b[1] - r && w.y <= b[3] + r; };
    if (Number.isInteger(only)) order = [only];
    else for (var k = s.pin.layers.length - 1; k >= 0; k--) if (k !== active) order.push(k);
    for (var n = 0; n < order.length; n++) {
      var info = s.pin.layers[order[n]];
      if (!info || info.locked === true || info.visible === false) continue;
      var layers = await presented(root, order[n], reach);
      if (!layers) return undefined;
      for (var i = layers.paths.length - 1; i >= 0; i--) if (layers.paths[i].hitTest(at, { fill: true, stroke: true, tolerance: t })) {
        var hit = await anchored(root, order[n], withFill(owned(layers.paths[i]), layers.items), layers);
        hit.data.nativeLayerIndex = order[n]; hit.data.nativeLayerUid = layers.layerUid;
        return hit;
      }
    }
    return null;
  }

  // #2372: the selectable paths of one layer (default: the active one) the `wanted(item, worldBounds)` filter keeps, each carrying its
  // linked fill and layer; a locked or hidden layer is never read. `centres` adds a ribbon's world centreline (what a commit needs).
  async function layerPaths(root, wanted, centres, layerIndex) {
    var s = presentedSession(root), idx = layerIndex === undefined ? root.state.activeLayerIdx : layerIndex, info = s && s.pin.layers[idx];
    if (!info || info.locked === true || info.visible === false) return null;
    var layers = await presented(root, idx, wanted, 0, centres);
    if (layers) layers.paths.forEach(function (p) { withFill(p, layers.items); p.data.nativeLayerIndex = idx; p.data.nativeLayerUid = layers.layerUid; });
    return layers;
  }
  // The items of the active layer whose painted bounds meet the world rectangle [x0, y0, x1, y1] (the baseline marquee's intersection
  // rule); a linked fill and a texture dab are never their own selection, they follow their ribbon / anchor.
  async function marquee(root, rect) {
    var layers = await layerPaths(root, function (item, b) {
      return !(item.role === 'fill' && item.owner) && item.brush !== 'copy' && b[0] <= rect[2] && b[2] >= rect[0] && b[1] <= rect[3] && b[3] >= rect[1]; });
    return layers && layers.paths;
  }

  // #2362 Labs sculpt: a drag over the active layer's presented strokes (a ribbon with its world centreline). Moves wait
  // until the read lands; the tool's `push(layer, at, delta, R)` / `smooth(layer, at, R)` (R its `radius()` in screen pixels)
  // edit the transient paths and the ones `touched()` lists are previewed in an SVG over the canvas, as native Subselect
  // previews. `end()` resolves to the read, or null.
  function presentedDrag(root, e, tool) {
    var Point = root.Point || root.paper.Point, g = { queue: [[e.clientX, e.clientY, e.shiftKey]], shown: null, layer: null, last: null, svg: null };
    var role = function (r) { return r === 'plain' || r === 'ribbon'; };
    var ready = presented(root, root.state.activeLayerIdx, function (item) { return role(item.role); }, 0, true).then(function (shown) {
      if (!shown || shown.frame !== root.state.currentFrame) return null;
      g.shown = shown; g.layer = { children: shown.paths.filter(function (p) { return role(p.data.nativeRole); }) };
      g.queue.splice(0).forEach(function (q) { move(q[0], q[1], q[2]); });
      return shown;
    }, function () { return null; });
    function move(x, y, smooth) {
      if (!g.shown) { g.queue.push([x, y, smooth]); return; }
      var w = worldPoint(root, x, y, g.shown.pin, g.shown.presentation), cur = new Point(w.x, w.y);
      var R = tool.radius() * w.perPixel;
      if (g.last && (smooth ? tool.smooth(g.layer, cur, R) : tool.push(g.layer, cur, cur.subtract(g.last), R))) preview(root, g, tool.touched());
      g.last = cur;
    }
    return { move: move, end: function () { return ready.then(function (shown) { if (g.svg) g.svg.remove(); return shown; }); } };
  }
  function preview(root, g, paths) {
    var mapping = root.NemoNativeViewportUnderlay && root.NemoNativeViewportUnderlay.canvasMapping, el = root.document.getElementById('drawing-canvas');
    if (!mapping || !el) return;
    var b = el.getBoundingClientRect(), pin = g.shown.pin, V = mapping.transform({ presentation: g.shown.presentation }, false), NS = 'http://www.w3.org/2000/svg';
    var at = function (p) { return (V[0] * p.x + V[2] * p.y + V[4]) + ' ' + (V[1] * p.x + V[3] * p.y + V[5]); };
    if (!g.svg) { g.svg = root.document.createElementNS(NS, 'svg'); g.svg.setAttribute('aria-hidden', 'true'); root.document.body.appendChild(g.svg); }
    g.svg.setAttribute('viewBox', '0 0 ' + pin.canvasW + ' ' + pin.canvasH); g.svg.setAttribute('preserveAspectRatio', 'none');
    Object.assign(g.svg.style, { position: 'fixed', pointerEvents: 'none', zIndex: '20', left: b.left + 'px', top: b.top + 'px', width: b.width + 'px', height: b.height + 'px' });
    g.svg.textContent = '';
    paths.forEach(function (p) {
      var segs = p.segments, d = 'M ' + at(segs[0].point), node = root.document.createElementNS(NS, 'path');
      var curve = function (a, c) { d += ' C ' + at(a.point.add(a.handleOut)) + ' ' + at(c.point.add(c.handleIn)) + ' ' + at(c.point); };
      for (var i = 1; i < segs.length; i++) curve(segs[i - 1], segs[i]);
      if (p.closed) { curve(segs[segs.length - 1], segs[0]); d += ' Z'; }
      [['d', d], ['fill', p.data.isVectorBrush ? 'rgba(255,184,108,0.35)' : 'none'], ['stroke', '#ffb86c'], ['stroke-width', '1.5'], ['vector-effect', 'non-scaling-stroke']]
        .forEach(function (a) { node.setAttribute(a[0], a[1]); });
      g.svg.appendChild(node);
    });
  }
  // The deformed transient paths as batch reshapes: a ribbon by its new centreline, a plain stroke by its new geometry.
  function reshapeSteps(paths) {
    return paths.map(function (p) {
      var t = p.data.nativeStableTarget, m = p.data.nativeMatrix;
      return { reshape: p.data.isVectorBrush && p.data.centerSegments ? { stableTarget: t, center: recordCenter(p.data.centerSegments, m) } : { stableTarget: t, geometry: recordGeometry(p, m) } };
    });
  }

  // #2372 Select: a world affine `m` ([a b c d e f]) in the record's own space, P^-1 * M * P for the presented matrix P of the record.
  function inRecordSpace(p, m) {
    var d = p[0] * p[3] - p[1] * p[2];
    if (!Number.isFinite(d) || Math.abs(d) < 1e-12) throw new Error('Presented transform is singular');
    var mul = function (a, b) { return [a[0] * b[0] + a[2] * b[1], a[1] * b[0] + a[3] * b[1], a[0] * b[2] + a[2] * b[3], a[1] * b[2] + a[3] * b[3],
      a[0] * b[4] + a[2] * b[5] + a[4], a[1] * b[4] + a[3] * b[5] + a[5]]; };
    var inv = [p[3] / d, -p[1] / d, -p[2] / d, p[0] / d, (p[2] * p[5] - p[3] * p[4]) / d, (p[1] * p[4] - p[0] * p[5]) / d];
    return mul(inv, mul(m, p));
  }
  // The Select gesture's batch steps: EVERY painted record (plain stroke, ribbon, fill, shape, texture anchor) is mapped by the engine through
  // a record-space matrix (six numbers however many anchors it has), which also carries its linked fill, texture copies, gradient end
  // points and shape box. `paths` are fresh presented paths (untransformed) and `m` the world affine.
  function transformSteps(root, paths, m) {
    return paths.map(function (p) { return { reshape: { stableTarget: p.data.nativeStableTarget, matrix: inRecordSpace(p.data.nativeMatrix, m) } }; });
  }

  // What a pick depends on: a repaint may re-project the layer rows (the pointer context counts those) but a pick still
  // stands while the tool, mode, frame, layer and document revision are unchanged.
  function pickContext(root) {
    var st = root.state, s = objectProject(root), pin = s && s.info();
    return JSON.stringify([st.tool, st.appMode, st.currentFrame, st.activeLayerIdx, st.activeSymbolId || null,
      pin ? PINS.map(function (k) { return pin[k]; }) : null]);
  }
  // #2372: a pick that resolves undefined (the presentation could not be proven current) or throws is not "nothing under the
  // cursor" (null, which clears): retry once after the presentation settles, then say so instead of dropping the click.
  // `I` is the intent API in use (its pick/applyPick); `ctx` is the pickContext captured at the click.
  function resolvePick(root, I, intent, shift, ctx, retried) {
    var unavailable = function () { if (typeof root.showToast === 'function') root.showToast('Could not read the canvas just now \u2014 click again.'); };
    return I.pick(root, intent.x, intent.y).then(function (item) {
      if (I.pickContext(root) !== ctx) return;
      if (item === undefined) {
        if (retried) return unavailable();
        return new Promise(function (done) { (root.setTimeout || setTimeout)(done, 120); }).then(function () { return resolvePick(root, I, intent, shift, ctx, true); });
      }
      try { I.applyPick(root, item, shift); } catch (_) {}
    }, unavailable);
  }
  // Select's own rule on a picked stroke: a click selects it, Shift toggles it, an empty click clears unless Shift is held.
  // Picks are fresh objects, so identity is the native stroke id.
  function applyPick(root, item, shift) {
    var data = item && item.data;
    // #2372: a picked image also makes its layer the one selected layer row, which is what the Mesh panel reads
    // (readImageMeshSelection); a Shift pick keeps the layer selection it extends.
    var image = !!data && data.nativeRole === 'raster' && !shift;
    if (data && Number.isInteger(data.nativeLayerIndex) && (data.nativeLayerIndex !== root.state.activeLayerIdx || image)) {
      // #2372: a hit on another layer activates it through the native layer-selection route (_layerSel follows); a refused
      // activation selects nothing rather than a stroke of a layer that is not active.
      var project = root.NemoNativeObjectProject;
      if (!project || project.selectLayerUI(root, data.nativeLayerIndex, false, data.nativeLayerUid) !== true ||
          root.state.activeLayerIdx !== data.nativeLayerIndex) return;
    }
    var selected = root.selectedPaths || [], id = item && item.data.strokeId;
    var held = item ? selected.filter(function (p) { return p && p.data && p.data.strokeId === id; }) : [];
    root.selectedPaths = item ? (shift ? (held.length ? selected.filter(function (p) { return held.indexOf(p) < 0; }) : selected.concat([item])) : [item])
      : (shift ? selected : []);
    root.state.selectedStrokeIndices = [];
    if (root.NemoNativeSelectOverlay) root.NemoNativeSelectOverlay.show(root); // #2372: the pick's outline, box and handles
    if (typeof root.updateUI === 'function') root.updateUI();
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
  }

  return Object.freeze({ create: create, hitHandles: hitHandles, objectProject: objectProject, presented: presented,
    presentedState: presentedSession, worldPoint: worldPoint, recordGeometry: recordGeometry, recordCenter: recordCenter, presentedDrag: presentedDrag, reshapeSteps: reshapeSteps, transformSteps: transformSteps, inRecordSpace: inRecordSpace, layerPaths: layerPaths, marquee: marquee, pick: pick, pickContext: pickContext, applyPick: applyPick, resolvePick: resolvePick });
}));
