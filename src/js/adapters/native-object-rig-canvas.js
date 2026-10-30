/* The Rig canvas tool on a native object project (#2255): the ordinary Tracer / Assigner / Déplacer of rig-bridge.js without `ld.rig`.
 * Facts come from ONE bounded native read per presented frame (`readRig`, the rig view of query.document.object: the live bones, their
 * handles and influence centres, the IK chains, the binds and, in Assigner, the posed bound vertices), pinned to the actual presentation.
 * Each gesture is a local preview and ONE `object.rig.edit` on release through NemoNativeObjectRig, so it is one history entry:
 * a finished bone is `drawBone`, an anchor or handle drag is `moveSegment` (Alt on a handle breaks the tangent), an influence-rim drag is
 * `setBoneRadius`, and a weight edit is one `setWeight` / `addWeight` / `removeWeight` / `resetWeights` from the popover. What has no
 * native operation (the Alt-drag IK pose, the live deforming pose drag) is the typed `rig_pose_drag_unavailable`; a layer with motion or an
 * expression is the typed `rig_presentation_unavailable`. The pointer listeners stay on `document` in the capture phase (CLAUDE.md
 * section 13) and, with the Rig tool armed on a native project, take every canvas gesture so nothing else draws. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectRigCanvas = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const SVG = 'http://www.w3.org/2000/svg';
  const BONE = 'rgba(255,200,60,.92)', HANDLE = 'rgba(255,200,60,.7)', INFLUENCE = 'rgba(255,120,220,.78)', WHITE = 'rgba(255,255,255,1)', RUBBER = 'rgba(255,220,130,.6)';
  const PRESENTATION_REASON = 'rig_presentation_unavailable';
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);

  function toast(win, message) { try { if (win.showToast) win.showToast(message, 'warn'); } catch (_) { /* presentation only */ } }
  function failure(win, error) {
    toast(win, (error && error.message || 'Native rig is unavailable.') + (error && error.code ? ' (' + error.code + (error.reason ? ': ' + error.reason : '') + ')' : ''));
  }
  const widgetUi = (win) => win.NemoNativeObjectWidgetUI;
  const session = (win) => { const w = widgetUi(win); return w ? w.session(win) : null; };
  const active = (win) => !!session(win);
  const rigTool = (win) => !!win.state && win.state.tool === 'rig';
  const fieldNumber = (win, id, fallback) => { const el = win.document.getElementById(id), v = el ? parseFloat(el.value) : NaN; return Number.isFinite(v) && v ? v : fallback; };
  const defaultRadius = (win) => fieldNumber(win, 'rig-weight-radius', 200);
  const softness = (win) => fieldNumber(win, 'rig-falloff-softness', 0) / 100;
  const stepOf = (a, b, shift) => { // Shift constrains a new anchor to a multiple of 45 degrees from the last one (constrainAngle45).
    if (!shift) return b;
    const dx = b[0] - a[0], dy = b[1] - a[1], r = Math.hypot(dx, dy), snap = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
    return [a[0] + r * Math.cos(snap), a[1] + r * Math.sin(snap)];
  };

  function create(win) {
    let cached = null, serial = 0, overlay = null, paintQueued = false, watching = false, drag = null, draft = null, busy = false, pointer = null;
    const keyOf = (proof, layerUid, assign) => JSON.stringify([proof.info.instanceId, proof.info.documentId, proof.info.lifecycleGeneration,
      proof.info.contentRevision, proof.info.documentSnapshotId, proof.selector.frame, layerUid, assign]);
    const usable = () => (cached && cached.read && cached.proof.current() ? cached : null);
    const adapter = () => { const m = win.NemoNativeObjectRig; return m && typeof m.desktop === 'function' ? m.desktop(win) : null; };
    const mode = () => win.state.rigSubMode || 'draw';
    const activeLayer = (info) => (info && info.layers ? info.layers[win.state.activeLayerIdx] : null) || null;

    function watch() {
      if (watching || !win.requestAnimationFrame) return;
      watching = true;
      win.requestAnimationFrame(() => { watching = false; if (!active(win) || !rigTool(win)) { paint(); return; } refresh(); watch(); });
    }
    function refresh() {
      const W = widgetUi(win), proof = rigTool(win) && W ? W.proofNow(win, 'readRig') : null, layer = proof && activeLayer(proof.info), prior = cached;
      if (!proof || !layer) { if (cached) { serial++; cached = null; } paint(); return Promise.resolve(null); }
      const assign = mode() === 'assign', key = keyOf(proof, layer.layerUid, assign);
      if (prior && prior.key === key && prior.proof.current()) { prior.proof = proof; paint(); return prior.promise || Promise.resolve(prior.read); }
      serial++;
      const token = serial, entry = { proof, key, layerUid: layer.layerUid, read: null, promise: null };
      cached = entry;
      const parts = ['bones', 'vertices', 'chains', 'binds'].concat(assign ? ['bound'] : []);
      const selector = { contextId: 'scene-root', frame: proof.selector.frame, layerUid: layer.layerUid };
      entry.promise = Promise.resolve().then(() => (token === serial && proof.current() ? proof.session.readRig(selector, proof.info, parts) : null))
        .then((read) => {
          if (token !== serial || cached !== entry || !read || !proof.current()) return null;
          entry.read = read; paint(); return read;
        }).catch((error) => { if (token === serial && cached === entry) { entry.error = error; paint(); } return null; })
        .finally(() => { if (cached === entry) entry.promise = null; });
      watch();
      return entry.promise;
    }
    const parts = () => { const entry = usable(); return entry && !entry.read.unavailable ? entry.read.parts : null; };
    const vertsOf = (boneId) => ((parts() || {}).vertices || []).filter((v) => v.boneId === boneId).sort((a, b) => a.vi - b.vi);
    const boneOf = (boneId) => ((parts() || {}).bones || []).find((b) => b.boneId === boneId) || null;
    const radiusOf = (bone) => (bone.radius || defaultRadius(win));

    // ---- geometry shown (the read, with the gesture's local preview applied) ----
    function shown(boneId) {
      const list = vertsOf(boneId).map((v) => ({ vi: v.vi, point: v.point.slice(), hi: v.handleIn.slice(), ho: v.handleOut.slice() }));
      if (drag && drag.mode === 'move' && drag.hit.boneId === boneId) {
        const v = list.find((one) => one.vi === drag.hit.vi);
        if (v && drag.hit.kind === 'anchor') v.point = drag.to.slice();
        else if (v) {
          const mine = drag.hit.kind === 'handleIn' ? 'hi' : 'ho', other = mine === 'hi' ? 'ho' : 'hi', vec = [drag.to[0] - v.point[0], drag.to[1] - v.point[1]];
          v[mine] = vec; if (!drag.breakTangent) v[other] = [-vec[0], -vec[1]];
        }
      }
      return list;
    }
    const centreOf = (bone) => bone.center;

    // ---- overlay (presentation only) ----
    const node = (name, attrs) => { const n = win.document.createElementNS(SVG, name); Object.keys(attrs).forEach((k) => n.setAttribute(k, attrs[k])); return n; };
    function ensureOverlay() {
      if (overlay && overlay.isConnected) return overlay;
      overlay = node('svg', { 'aria-hidden': 'true' });
      Object.assign(overlay.style, { position: 'fixed', left: '0', top: '0', width: '100vw', height: '100vh', overflow: 'visible', pointerEvents: 'none', zIndex: '900' });
      (win.document.body || win.document.documentElement).appendChild(overlay);
      return overlay;
    }
    function paint() {
      if (paintQueued) return;
      paintQueued = true;
      (win.requestAnimationFrame || ((fn) => fn()))(() => { paintQueued = false; draw(); });
    }
    function mapping(entry) { const W = widgetUi(win); return entry && W ? W.mappingFor(win, entry) : null; }
    const unit = (map) => { const a = map.api.screen({ x: 0, y: 0 }, map), b = map.api.screen({ x: 1, y: 0 }, map), d = Math.hypot(b.x - a.x, b.y - a.y); return d > 0 && Number.isFinite(d) ? d : 1; };
    function pathOf(S, verts, closed) {
      if (!verts.length) return '';
      const at = (p) => { const s = S(p); return s.x + ' ' + s.y; }, abs = (v, h) => [v.point[0] + h[0], v.point[1] + h[1]];
      let d = 'M ' + at(verts[0].point);
      const last = closed ? verts.length : verts.length - 1;
      for (let n = 0; n < last; n++) { const a = verts[n], b = verts[(n + 1) % verts.length]; d += ' C ' + at(abs(a, a.ho)) + ' ' + at(abs(b, b.hi)) + ' ' + at(b.point); }
      return d + (closed ? ' Z' : '');
    }
    function draw() {
      const svg = ensureOverlay();
      while (svg.firstChild) svg.removeChild(svg.firstChild);
      const entry = usable(), map = rigTool(win) && entry && !entry.read.unavailable && mapping(entry);
      if (!map) return;
      const S = (p) => map.api.screen({ x: p[0], y: p[1] }, map), scale = unit(map);
      const dot = (p, r, fill, stroke) => { const s = S(p); svg.appendChild(node('circle', { cx: s.x, cy: s.y, r, fill, stroke, 'stroke-width': 1.2 })); };
      const handleMark = (p) => { const s = S(p); svg.appendChild(node('rect', { x: s.x - 3, y: s.y - 3, width: 6, height: 6, fill: WHITE, stroke: BONE, 'stroke-width': 1 })); };
      const handles = (v) => [['hi', v.hi], ['ho', v.ho]].forEach((pair) => {
        if (Math.hypot(pair[1][0], pair[1][1]) <= 0.5) return;
        const tip = [v.point[0] + pair[1][0], v.point[1] + pair[1][1]], a = S(v.point), b = S(tip);
        svg.appendChild(node('line', { x1: a.x, y1: a.y, x2: b.x, y2: b.y, stroke: HANDLE, 'stroke-width': 1 })); handleMark(tip);
      });
      (parts().bones || []).forEach((bone) => {
        const verts = shown(bone.boneId);
        if (verts.length < 2) return;
        svg.appendChild(node('path', { d: pathOf(S, verts, bone.closed), fill: 'none', stroke: BONE, 'stroke-width': 2 }));
        verts.forEach((v) => { handles(v); dot(v.point, 3.5, WHITE, BONE); });
        if (mode() === 'assign') {
          const c = S(centreOf(bone)), r = (drag && drag.mode === 'radius' && drag.boneId === bone.boneId ? drag.radius : radiusOf(bone)) * scale;
          svg.appendChild(node('line', { x1: c.x - 4, y1: c.y, x2: c.x + 4, y2: c.y, stroke: INFLUENCE, 'stroke-width': 1.4 }));
          svg.appendChild(node('line', { x1: c.x, y1: c.y - 4, x2: c.x, y2: c.y + 4, stroke: INFLUENCE, 'stroke-width': 1.4 }));
          svg.appendChild(node('circle', { cx: c.x, cy: c.y, r, fill: 'none', stroke: INFLUENCE, 'stroke-width': 1.6 }));
        }
      });
      if (draft) {
        const verts = draft.segs.map((s, vi) => ({ vi, point: s.point, hi: s.handleIn, ho: s.handleOut }));
        if (verts.length > 1) svg.appendChild(node('path', { d: pathOf(S, verts, false), fill: 'none', stroke: BONE, 'stroke-width': 2 }));
        if (pointer && verts.length) { const a = S(verts[verts.length - 1].point), b = S(pointer); svg.appendChild(node('line', { x1: a.x, y1: a.y, x2: b.x, y2: b.y, stroke: RUBBER, 'stroke-width': 1 })); }
        verts.forEach((v) => { handles(v); dot(v.point, 3.5, WHITE, BONE); });
      }
    }

    // ---- hit tests, in world units ----
    function hitAnchorOrHandle(pt, tol) {
      let best = null, bestD = tol;
      ((parts() || {}).vertices || []).forEach((v) => {
        let d = dist(pt, v.point);
        if (d < bestD) { bestD = d; best = { boneId: v.boneId, vi: v.vi, kind: 'anchor', at: v.point }; }
        [['handleIn', v.handleIn], ['handleOut', v.handleOut]].forEach((pair) => {
          if (Math.abs(pair[1][0]) < 0.5 && Math.abs(pair[1][1]) < 0.5) return;
          const tip = [v.point[0] + pair[1][0], v.point[1] + pair[1][1]];
          d = dist(pt, tip);
          if (d < bestD) { bestD = d; best = { boneId: v.boneId, vi: v.vi, kind: pair[0], at: tip }; }
        });
      });
      return best;
    }
    function hitRadius(pt, tol) {
      let best = null, bestD = tol;
      ((parts() || {}).bones || []).forEach((bone) => { const d = Math.abs(dist(pt, centreOf(bone)) - radiusOf(bone)); if (d < bestD) { bestD = d; best = bone; } });
      return best;
    }
    function hitBound(pt, tol) {
      let best = null, bestD = tol;
      ((parts() || {}).bound || []).forEach((item) => { const d = dist(pt, [item[2], item[3]]); if (d < bestD) { bestD = d; best = { bind: item[0], vi: item[1] }; } });
      return best;
    }
    function tipHit(pt, tol) { // an open bone's end close to the pointer, for branching (plain) or extending (Alt) in Tracer
      let best = null, bestD = tol;
      ((parts() || {}).bones || []).forEach((bone) => {
        const list = vertsOf(bone.boneId);
        if (bone.closed || list.length < 2) return;
        [['first', list[0]], ['last', list[list.length - 1]]].forEach((pair) => { const d = dist(pt, pair[1].point); if (d < bestD) { bestD = d; best = { bone, end: pair[0], point: pair[1].point.slice() }; } });
      });
      return best;
    }

    // ---- commands ----
    function send(run) {
      const api = adapter(), entry = usable();
      if (!api || !entry) { toast(win, 'Native rig is unavailable.'); return Promise.resolve(false); }
      busy = true;
      return Promise.resolve().then(() => run(api, entry)).then(() => true, (error) => { failure(win, error); return false; })
        .then((ok) => { busy = false; return refresh().then(() => ok); });
    }
    function finishDraft(closed) {
      const done = draft; draft = null; paint();
      if (!done || done.segs.length < 2) return Promise.resolve(false);
      const segments = done.segs.map((s) => ({ point: s.point, handleIn: s.handleIn, handleOut: s.handleOut }));
      return send((api, entry) => api.drawBone(entry.layerUid, segments, !!closed, done.boneId));
    }
    function openWeights(hit, e) {
      const entry = usable(), P = parts(), W = win.NemoNativeObjectRigWeightsUI, api = adapter(), bind = P && P.binds.find((b) => b.index === hit.bind);
      if (!entry || !bind || !W || !api) return;
      const target = bind.strokeId !== null ? { strokeId: bind.strokeId } : { meshId: bind.meshId };
      const label = (boneId) => { const i = (P.bones || []).findIndex((b) => b.boneId === boneId); return 'Os ' + (i >= 0 ? i + 1 : '?'); };
      const load = () => {
        const live = usable();
        if (!live) return Promise.resolve(null);
        return live.proof.session.readRigWeights({ contextId: 'scene-root', frame: live.proof.selector.frame, layerUid: live.layerUid }, live.proof.info, hit.bind, hit.vi)
          .then((read) => read.parts.weights, (error) => { failure(win, error); return null; });
      };
      const commit = (run) => send((a, en) => run(a, en)).then(() => load()).then(render);
      function render(entries) {
        if (!entries) { W.close(win); return; }
        const used = new Set(entries.map((w) => w.boneId));
        W.show(win, { x: e.clientX, y: e.clientY, vertex: hit.vi,
          entries: entries.map((w) => ({ entry: w.entry, boneId: w.boneId, label: label(w.boneId), percent: Math.round(w.w * 100) })),
          addable: (P.bones || []).filter((b) => !used.has(b.boneId)).map((b) => ({ boneId: b.boneId, label: label(b.boneId) })),
          on: {
            set: (index, percent) => commit((a, en) => a.setWeight(en.layerUid, target, hit.vi, index, percent)),
            remove: (index) => commit((a, en) => a.removeWeight(en.layerUid, target, hit.vi, index)),
            add: (boneId) => commit((a, en) => a.addWeight(en.layerUid, target, hit.vi, boneId)),
            reset: () => commit((a, en) => a.resetWeights(en.layerUid, target, hit.vi, { radius: defaultRadius(win), softness: softness(win) })),
          } });
      }
      load().then(render);
    }

    // ---- gestures ----
    const areaOf = (e) => { const area = win.document.getElementById('canvas-area'); return area && e.target && area.contains(e.target) ? area : null; };
    const consume = (e) => { e.stopImmediatePropagation(); e.preventDefault(); };
    function world(e, map) { try { const w = map.api.point(e, map); return [w.x, w.y]; } catch (_) { return null; } }
    function onDown(e) {
      if (!active(win) || !rigTool(win) || win.state.playing || busy || (e.button !== undefined && e.button !== 0) || !areaOf(e)) return;
      consume(e);
      const entry = usable(), map = entry && mapping(entry), P = parts();
      if (!entry || !map || !P) {
        const reason = entry && entry.read && entry.read.unavailable;
        toast(win, 'Rig editing on the canvas is unavailable (' + PRESENTATION_REASON + (reason ? ': ' + reason : '') + ').');
        return;
      }
      const pt = world(e, map);
      if (!pt) return;
      const tol = 9 / unit(map), m = mode();
      if (m === 'move') {
        const hit = hitAnchorOrHandle(pt, tol);
        if (!hit) return;
        if (hit.kind === 'anchor' && e.altKey) { const api = adapter(); if (api) api.dragIkEnd().catch((error) => failure(win, error)); return; }
        drag = { mode: 'move', hit, to: hit.at.slice(), pointerId: e.pointerId, map, entry, moved: false, breakTangent: !!e.altKey };
      } else if (m === 'assign') {
        const bone = hitRadius(pt, 10 / unit(map));
        if (bone) { drag = { mode: 'radius', boneId: bone.boneId, centre: centreOf(bone), radius: radiusOf(bone), pointerId: e.pointerId, map, entry, moved: false }; paint(); return; }
        const bound = hitBound(pt, 10 / unit(map));
        if (bound) openWeights(bound, e);
        return;
      } else {
        const now = Date.now(), tipTol = 10 / unit(map);
        const twice = draft && draft.last && now - draft.last.time < 350 && dist(pt, draft.last.at) < tipTol;
        if (twice) { finishDraft(false); return; }
        if (!draft) {
          const tip = tipHit(pt, tipTol);
          if (tip && e.altKey) { // reopen and extend the same bone (the rarer case), its first end being the open end when reversed
            const list = vertsOf(tip.bone.boneId).map((v) => ({ point: v.point.slice(), handleIn: v.handleIn.slice(), handleOut: v.handleOut.slice() }));
            if (tip.end === 'first') list.reverse().forEach((s) => { const t = s.handleIn; s.handleIn = s.handleOut; s.handleOut = t; });
            draft = { segs: list, boneId: tip.bone.boneId, last: null };
          } else draft = { segs: [{ point: tip ? tip.point : pt, handleIn: [0, 0], handleOut: [0, 0] }], boneId: undefined, last: null };
        } else if (draft.segs.length > 1 && dist(pt, draft.segs[0].point) < tipTol) { finishDraft(true); return; }
        else draft.segs.push({ point: stepOf(draft.segs[draft.segs.length - 1].point, pt, e.shiftKey), handleIn: [0, 0], handleOut: [0, 0] });
        draft.last = { time: now, at: pt };
        draft.dragging = true; draft.pointerId = e.pointerId;
      }
      paint();
    }
    function onMove(e) {
      if (!active(win) || !rigTool(win)) return;
      if (!watching) refresh(); // the first pointer over the canvas with the tool armed starts the per-frame check
      const entry = draft || drag ? usable() : null, map = entry && mapping(entry);
      if (!map) { if (drag) { drag = null; paint(); } return; }
      const pt = world(e, map);
      if (!pt) return;
      pointer = pt;
      if (drag && (e.pointerId === undefined || drag.pointerId === undefined || e.pointerId === drag.pointerId)) {
        consume(e);
        if (drag.mode === 'radius') drag.radius = Math.max(1, dist(pt, drag.centre)); else { drag.to = pt; drag.breakTangent = drag.hit.kind !== 'anchor' && !!e.altKey; }
        drag.moved = true;
      } else if (draft) {
        if (draft.dragging) {
          const seg = draft.segs[draft.segs.length - 1], v = [pt[0] - seg.point[0], pt[1] - seg.point[1]];
          seg.handleOut = v; seg.handleIn = e.altKey ? [0, 0] : [-v[0], -v[1]];
        }
        consume(e);
      }
      paint();
    }
    function onUp(e) {
      if (draft && draft.dragging) { draft.dragging = false; consume(e); paint(); return; }
      if (!drag) return;
      consume(e);
      const done = drag; drag = null; paint();
      if (!done.moved || !done.entry.proof.current()) return;
      if (done.mode === 'radius') send((api, en) => api.setBoneRadius(en.layerUid, done.boneId, done.radius));
      else send((api, en) => api.moveSegment(en.layerUid, done.hit.boneId, done.hit.vi, done.hit.kind === 'anchor' ? 'point' : done.hit.kind, done.to, done.hit.kind === 'anchor' ? undefined : done.breakTangent));
    }
    function onKey(e) { if (draft && e.key === 'Enter' && rigTool(win) && active(win)) { e.preventDefault(); finishDraft(false); } }
    function listeners() {
      const doc = win.document, capture = { capture: true };
      doc.addEventListener('pointerdown', onDown, capture);
      doc.addEventListener('pointermove', onMove, capture);
      doc.addEventListener('pointerup', onUp, capture);
      doc.addEventListener('pointercancel', () => { if (drag || draft) { drag = null; draft = null; paint(); } }, capture);
      doc.addEventListener('keydown', onKey, capture);
    }
    const api = Object.freeze({ install: listeners, refresh, paint, active: () => active(win), data: () => parts() });
    return api;
  }
  const controllers = new WeakMap();
  function desktop(win) {
    if (!controllers.has(win)) controllers.set(win, create(win));
    return controllers.get(win);
  }
  // The listeners sit on document from the first load; each handler reads the window state only when a pointer arrives.
  if (typeof window === 'object' && window && typeof document === 'object' && document && typeof document.addEventListener === 'function') {
    try { desktop(window).install(); } catch (_) { /* a window without a document: nothing to listen on */ }
  }
  return Object.freeze({ create, desktop, active, PRESENTATION_REASON });
}));
