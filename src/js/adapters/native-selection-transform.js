/* Native Transform fields and arrow-key nudge of the Select selection (#2372, insp-transform-fields). With items selected on a native object
 * project, the inspector's X / Y / W / H / rotation fields and the arrow keys move, scale and turn the selection as ONE native `object.vector.batch`
 * of record-space reshapes (one revision, one history entry) - the same route as the canvas drag (native-select-gesture.js commitMatrix) - instead
 * of the legacy selPropsApplyMove / Scale / Rotate, whose history checkpoint a native project refuses. The matrices follow the baseline
 * (1065e9d8 timeline.js selPropsApply*): X / Y translate by the difference to the selection's bounds; W / H scale about the bounds' top-left
 * corner (both axes by the same factor while the proportion lock is on); rotation turns by the difference to the accumulated angle about the
 * anchor point. A selection may span layers: it is still ONE batch. It holds no document state and writes nothing itself.
 * A typed value fires many edits: they are coalesced per field and committed once when they stop (a scrub commits at its release); an edit of
 * another field, or of another selection, commits the pending one first. A nudge is one batch per key press (the baseline pushed one undo per
 * press); the auto-repeat presses that arrive while an earlier one is still being written are summed into the next batch. A refusal is a
 * notice, never a throw. Baseline stays: no Paper writer is reached from here. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectionTransform = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var QUIET = 220, FIELDS = ['x', 'y', 'w', 'h', 'rot'], REPEAT_NOTICE_MS = 2000;
  var jobs = new WeakMap();
  function now() { return Date.now(); }
  function notice(root, text) {
    var s = state(root);
    if (s.said && s.said.text === text && now() - s.said.at < REPEAT_NOTICE_MS) return; // a scrub or a held key must not stack the same toast
    s.said = { text: text, at: now() };
    if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {}
  }
  function timer(root) { return typeof root.setTimeout === 'function' ? root.setTimeout.bind(root) : setTimeout; }
  function clear(root, id) { return (typeof root.clearTimeout === 'function' ? root.clearTimeout.bind(root) : clearTimeout)(id); }
  function state(root) {
    if (!jobs.has(root)) jobs.set(root, { pending: null, chain: Promise.resolve(), commits: 0, inflight: 0, held: null, said: null });
    return jobs.get(root);
  }
  // The native object project the app is showing, or null (a legacy project keeps the legacy writers).
  function session(root) {
    try { var s = root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); return s && s.blocksLegacy() === true ? s : null; } catch (_) { return null; }
  }
  function native(root) { return !!session(root); }
  function selected(root) {
    return (root.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget && Number.isInteger(p.data.nativeLayerIndex); });
  }
  function idsOf(paths) { return paths.map(function (p) { return p.data.strokeId; }); }
  function same(a, b) { return a.length === b.length && a.every(function (id, at) { return id === b[at]; }); }
  function route(root) { var L = root.NemoNativeObjectLabs; return L && typeof L.desktop === 'function' ? L.desktop(root) : null; }
  // True while the overlay draws the selection for the context the app is in now.
  function owns(root) {
    var I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay;
    return !!(I && O && typeof O.selectionContext === 'function' && O.selectionContext(root) === I.pickContext(root));
  }
  function dropSelection(root) {
    var O = root.NemoNativeSelectOverlay;
    root.selectedPaths = []; if (root.state) root.state.selectedStrokeIndices = [];
    if (O && typeof O.show === 'function') O.show(root);
    if (typeof root.updateUI === 'function') root.updateUI();
  }
  // True when the Select selection can take a native transform now (the panel marks its numeric fields to scrub statically then).
  function handles(root) {
    var st = root.state;
    return !!st && st.tool === 'select' && st.appMode !== 'motion' && selected(root).length > 0 && !!route(root);
  }
  // Why the selection cannot be transformed here, as a notice; 'none' when there is nothing selected (the baseline did nothing then); null when it can.
  function refusal(root, vertex) {
    var st = root.state;
    if (!(root.selectedPaths || []).length) return 'none';
    if (st && st.appMode === 'motion') return 'Transforming the selection is not available in Motion on native projects yet.';
    if (vertex) return 'Editing the selected points from here is not available on native projects yet.';
    if (!st || st.tool !== 'select') return 'Switch to the Select tool (V) to transform the selection.';
    if (!selected(root).length) return 'Select items to transform them.';
    if (!route(root) || !root.NemoSelectCanvasIntent || !root.NemoNativeSelectGesture || typeof root.NemoNativeSelectGesture.commitMatrix !== 'function') return 'Transforming is not available right now.';
    return null;
  }
  // The selection must be the one the overlay draws for this frame, layer and tool: one that outlived a context change is dropped, never moved.
  function current(root, pending) {
    var I = root.NemoSelectCanvasIntent, paths = selected(root), ids = idsOf(paths), ctx = I.pickContext(root);
    if (!(owns(root) || (pending && pending.ctx === ctx && same(pending.ids, ids)))) {
      dropSelection(root);
      notice(root, 'The selection no longer applies here — select the items again.');
      return null;
    }
    return { paths: paths, ids: ids, ctx: ctx };
  }
  function scaleAbout(sx, sy, ax, ay) { return [sx, 0, 0, sy, ax - sx * ax, ay - sy * ay]; }
  function turnAbout(deg, cx, cy) {
    var r = deg * Math.PI / 180, cos = Math.cos(r), sin = Math.sin(r);
    return [cos, sin, -sin, cos, cx - cos * cx + sin * cy, cy - sin * cx - cos * cy];
  }
  // The world affine a field edit means against the selection as it is now (baseline wireLiveXformField rules), or null when it changes nothing.
  // `rot` (the target angle) is set for a turn so the caller can store the accumulated angle once the batch has been applied.
  function fieldMatrix(job) {
    var r = job.read && job.read(), b = r && r.bounds, v = job.value, a = r && r.anchor;
    if (!b) return null;
    var out = null;
    if (job.field === 'x') out = { m: [1, 0, 0, 1, (Number.isFinite(v) ? v : 0) - b.x, 0] };
    else if (job.field === 'y') out = { m: [1, 0, 0, 1, 0, (Number.isFinite(v) ? v : 0) - b.y] };
    else if (job.field === 'w' || job.field === 'h') {
      var wide = job.field === 'w', size = wide ? b.width : b.height, other = wide ? b.height : b.width;
      if (size < 0.01) return null;
      var f = Math.max(0.01, v || size) / size;
      var both = job.lock && other >= 0.01;
      out = { m: wide ? scaleAbout(f, both ? f : 1, b.x, b.y) : scaleAbout(both ? f : 1, f, b.x, b.y) };
    } else if (job.field === 'rot') {
      var nv = Number.isFinite(v) ? v : 0, delta = nv - (r.rotAccum || 0);
      if (!delta || !a) return null;
      out = { m: turnAbout(delta, a.x, a.y), angle: delta, rot: nv };
    }
    return out && out.m.every(Number.isFinite) && !(Math.abs(out.m[0] - 1) + Math.abs(out.m[1]) + Math.abs(out.m[2]) + Math.abs(out.m[3] - 1) + Math.abs(out.m[4]) + Math.abs(out.m[5]) < 1e-9) ? out : null;
  }
  // ONE native batch for the job, against the document the selection was read from.
  async function execute(root, job) {
    var s = state(root), I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay;
    var paths = selected(root), ids = idsOf(paths), at = I && await I.presentedSettled(root);
    // An edit made while an earlier one of ours was committing was read before that commit re-read the selection: it follows the fresh one.
    if (job.commits !== s.commits) { if (same(ids, job.ids)) job.ctx = I.pickContext(root); }
    if (!at || !paths.length || !same(ids, job.ids) || I.pickContext(root) !== job.ctx) { notice(root, 'The selection changed — nothing was transformed.'); return { status: 'stale' }; }
    var spec = job.kind === 'nudge' ? { m: [1, 0, 0, 1, job.dx, job.dy] } : fieldMatrix(job);
    if (!spec) return { status: 'noop' };
    var status = await root.NemoNativeSelectGesture.commitMatrix(root, paths, spec.m, {
      pin: at.pin, gone: 'The selection changed — nothing was transformed.', angle: spec.angle || 0,
      stale: function () { return I.pickContext(root) !== job.ctx; } });
    var kept = root.state ? (root.state.selRotAccum || 0) : 0;
    if (status !== 'applied') return { status: status };
    s.commits += 1; // after the re-read: an edit that follows sees the fresh selection, never the one before this commit
    if (O && typeof O.show === 'function') O.show(root);
    if (typeof root.updateUI === 'function') root.updateUI(); // the re-read items are new objects, so the panel resets its accumulated angle...
    // ...but it is the same selection: a move, resize or nudge keeps the angle (baseline never re-read), a turn keeps the one it just made.
    var angle = spec.rot !== undefined ? spec.rot : kept;
    if (root.state && root.state.selRotAccum !== angle) { root.state.selRotAccum = angle; if (typeof root.updateUI === 'function') root.updateUI(); }
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
    return { status: 'applied' };
  }
  function run(root, job) {
    var s = state(root), settle = function () { s.inflight -= 1; };
    var next = s.chain.then(function () { return execute(root, job); }, function () { return execute(root, job); });
    s.inflight += 1;
    next.then(settle, settle);
    s.chain = next.then(function () {}, function () {});
    return next.then(function (r) { if (job.done) job.done(r); return r; }, function () { var r = { status: 'unavailable' }; if (job.done) job.done(r); return r; });
  }
  // Commits the pending field edit now, whatever the quiet timer says (an edit of another field, a test).
  function flush(root) {
    var s = state(root), job = s.pending;
    if (!job) return s.chain.then(function () { return { status: 'noop' }; });
    s.pending = null;
    if (job.timer) clear(root, job.timer);
    return run(root, job);
  }
  function arm(root, job) {
    var s = state(root);
    if (job.timer) clear(root, job.timer);
    job.timer = timer(root)(function () {
      job.timer = 0;
      if (s.pending !== job) return;
      if (root._scrubLiveActive) return arm(root, job); // a scrub in flight commits when it is released
      s.pending = null;
      run(root, job);
    }, QUIET);
  }
  // A field edit of the current selection: { field: 'x'|'y'|'w'|'h'|'rot', value (number, NaN when the box is empty), lock (proportion lock), vertex
  // (the panel is driving picked points), read() -> { bounds: {x, y, width, height}, anchor: {x, y}, rotAccum } of the selection as it is when called }.
  // Later edits of the same field replace the pending value; the commit follows when they stop. Resolves with { status } once committed.
  function edit(root, spec) {
    var s = state(root), why = refusal(root, spec.vertex);
    if (why === 'none') return Promise.resolve({ status: 'noop' });
    if (why) { notice(root, why); return Promise.resolve({ status: 'unavailable' }); }
    if (FIELDS.indexOf(spec.field) < 0 || typeof spec.read !== 'function') return Promise.resolve({ status: 'invalid' });
    // A commit in flight re-reads the selection when it ends: this edit follows it instead of reading the one before.
    if (s.inflight) return s.chain.then(function () { return edit(root, spec); });
    var pending = s.pending, cur = current(root, pending);
    if (!cur) return Promise.resolve({ status: 'unavailable' });
    if (pending && (pending.field !== spec.field || !same(pending.ids, cur.ids) || pending.ctx !== cur.ctx)) return flush(root).then(function () { return edit(root, spec); });
    if (!pending) {
      var job = { kind: 'field', field: spec.field, ids: cur.ids, ctx: cur.ctx, commits: s.commits, timer: 0, waiting: [] };
      job.done = function (r) { job.waiting.forEach(function (w) { w(r); }); };
      s.pending = pending = job;
    }
    pending.value = spec.value; pending.lock = !!spec.lock; pending.read = spec.read;
    var waiting = new Promise(function (resolve) { pending.waiting.push(resolve); });
    arm(root, pending);
    return waiting;
  }
  // One arrow-key press: the selection moves by (dx, dy) world units as ONE batch. `o.repeat` marks an auto-repeat press, which is summed into the
  // next batch while an earlier press is still being written (a held key does not queue a batch per repeat).
  function nudge(root, dx, dy, o) {
    var s = state(root), why = refusal(root, false);
    if (why === 'none') return Promise.resolve({ status: 'noop' });
    if (why) { notice(root, why); return Promise.resolve({ status: 'unavailable' }); }
    if (!dx && !dy) return Promise.resolve({ status: 'noop' });
    if (s.pending) return flush(root).then(function () { return nudge(root, dx, dy, o); }); // a typed value first, then the key
    if (s.inflight) {
      if (s.held && o && o.repeat) { s.held.dx += dx; s.held.dy += dy; return s.held.promise; }
      var held = { dx: dx, dy: dy };
      held.promise = s.chain.then(function () { if (s.held === held) s.held = null; return nudge(root, held.dx, held.dy, { repeat: false }); });
      s.held = held;
      return held.promise;
    }
    var cur = current(root, null);
    if (!cur) return Promise.resolve({ status: 'unavailable' });
    return run(root, { kind: 'nudge', dx: dx, dy: dy, ids: cur.ids, ctx: cur.ctx, commits: s.commits });
  }
  // Tells the user why an edit cannot be made (a legacy-only path that still ends here on a native project).
  function refuse(root, text) { notice(root, text); }
  return Object.freeze({ edit: edit, nudge: nudge, flush: flush, handles: handles, native: native, refuse: refuse });
}));
