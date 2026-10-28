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
    var pending = null, captureTarget = null;
    function valid() {
      try { return pending.context === ports.context(); }
      catch (_) { return false; }
    }
    function cancel(e) {
      if (!pending || e && e.pointerId !== undefined && e.pointerId !== pending.id) return false;
      var id = pending.id, target = captureTarget;
      pending = null; captureTarget = null;
      if (target && target.releasePointerCapture &&
          (!target.hasPointerCapture || target.hasPointerCapture(id))) {
        try { target.releasePointerCapture(id); } catch (_) {}
      }
      return true;
    }
    function down(e) {
      consume(e);
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
      } catch (_) { cancel(); }
    }
    function move(e) {
      consume(e);
      if (!pending) return;
      if (pending.blocked || e.pointerId !== pending.id) return;
      if (!valid() || e.buttons !== 1 || !Number.isFinite(e.clientX) || !Number.isFinite(e.clientY) ||
          Math.hypot(e.clientX - pending.x, e.clientY - pending.y) > 3) pending.blocked = true;
    }
    function up(e) {
      consume(e);
      if (!pending) return;
      if (e.pointerId !== pending.id) return;
      var accept = !pending.blocked && valid() && Number.isFinite(e.clientX) && Number.isFinite(e.clientY) &&
        Math.hypot(e.clientX - pending.x, e.clientY - pending.y) <= 3;
      var decision = pending.decision, shift = pending.shift;
      cancel();
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
  async function presented(root, layerIndex, attempt) {
    var s = presentedSession(root), layer = s && s.pin.layers[layerIndex], data;
    if (!layer) return null;
    try { data = await s.session.readSubselect({ contextId: 'scene-root', layerUid: layer.layerUid, frame: s.presentation.frame }, s.pin); }
    catch (error) {
      if (!error || error.code !== 'stale_revision' || (attempt || 0) >= 3) throw error;
      await new Promise(function (done) { (root.setTimeout || setTimeout)(done, 30); });
      var next = presentedSession(root);
      return next && samePin(next.pin, s.pin) && next.presentation.frame === s.presentation.frame ? presented(root, layerIndex, (attempt || 0) + 1) : null;
    }
    var now = presentedSession(root);
    if (!now || !samePin(now.pin, s.pin) || now.presentation.frame !== s.presentation.frame) return null;
    var Path = root.Path || root.paper.Path, Matrix = root.Matrix || root.paper.Matrix;
    return { pin: s.pin, presentation: s.presentation, frame: s.presentation.frame, layerUid: layer.layerUid, paths: data.items.map(function (item) {
      var geometry = item.paint || item;   // a pressure ribbon is hit on its painted outline
      var path = new Path({ insert: false, closed: geometry.closed, segments: geometry.segments.map(function (a) { return [a.point, a.handleIn, a.handleOut]; }) });
      path.transform(new Matrix(item.matrix[0], item.matrix[1], item.matrix[2], item.matrix[3], item.matrix[4], item.matrix[5]));
      path.strokeColor = [0, 0, 0]; path.strokeWidth = 1; if (item.paint) path.fillColor = [0, 0, 0];
      path.data = { strokeId: item.stableTarget.strokeId, nativeStableTarget: item.stableTarget, nativeRibbon: !!item.paint, nativeMatrix: item.matrix };
      return path;
    }) };
  }
  // World-space Paper segments back to a record geometry in the stroke's own space (inverse of its presented matrix).
  function recordGeometry(path, m) {
    var d = m[0] * m[3] - m[1] * m[2];
    if (!Number.isFinite(d) || Math.abs(d) < 1e-12) throw new Error('Presented transform is singular');
    var lin = function (x, y) { return { x: (m[3] * x - m[2] * y) / d, y: (-m[1] * x + m[0] * y) / d }; };
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
  // The top-most presented stroke of the active layer within 8 screen pixels of the click, or null.
  async function pick(root, clientX, clientY) {
    var s = presentedSession(root), layers = s && await presented(root, root.state.activeLayerIdx);
    if (!layers) return undefined;
    var Point = root.Point || root.paper.Point, w = worldPoint(root, clientX, clientY, s.pin, s.presentation), at = new Point(w.x, w.y);
    for (var i = layers.paths.length - 1; i >= 0; i--)
      if (layers.paths[i].hitTest(at, { fill: true, stroke: true, tolerance: 8 * w.perPixel })) return layers.paths[i];
    return null;
  }

  // What a pick depends on: a repaint may re-project the layer rows (the pointer context counts those) but a pick still
  // stands while the tool, mode, frame, layer and document revision are unchanged.
  function pickContext(root) {
    var st = root.state, s = objectProject(root), pin = s && s.info();
    return JSON.stringify([st.tool, st.appMode, st.currentFrame, st.activeLayerIdx, st.activeSymbolId || null,
      pin ? PINS.map(function (k) { return pin[k]; }) : null]);
  }
  // Select's own rule on a picked stroke: a click selects it, Shift toggles it, an empty click clears unless Shift is held.
  // Picks are fresh objects, so identity is the native stroke id.
  function applyPick(root, item, shift) {
    var selected = root.selectedPaths || [], id = item && item.data.strokeId;
    var held = item ? selected.filter(function (p) { return p && p.data && p.data.strokeId === id; }) : [];
    root.selectedPaths = item ? (shift ? (held.length ? selected.filter(function (p) { return held.indexOf(p) < 0; }) : selected.concat([item])) : [item])
      : (shift ? selected : []);
    root.state.selectedStrokeIndices = [];
    if (typeof root.updateUI === 'function') root.updateUI();
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
  }

  return Object.freeze({ create: create, hitHandles: hitHandles, objectProject: objectProject, presented: presented,
    worldPoint: worldPoint, recordGeometry: recordGeometry, pick: pick, pickContext: pickContext, applyPick: applyPick });
}));
