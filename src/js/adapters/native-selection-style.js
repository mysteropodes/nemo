/* Native restyle of the Select selection (#2372, SEL-STYLE). With items selected on a native object project, the right panel's fill / stroke
 * colour, stroke width, brush size and opacity restyle every selected item as ONE native `object.vector.batch` of `restyle` steps (one revision,
 * one history entry), the baseline setFillColor / setStrokeColor / setBrushSize / setOpacity over `selectedPaths` (1065e9d8 timeline.js). The engine
 * decides per family (a ribbon rescales its width profile and rebuilds with its linked fill; a text block takes colour and alpha; a dab or a bitmap
 * brush is refused), so this module only names the selection and the members that changed. It holds no document state and writes nothing itself:
 * the panel's edits are coalesced (a colour picker and a scrub fire many times per gesture) and committed once, when the edits stop and no scrub
 * is live; a selection that changed meanwhile ends the pending edit with a notice. Colours keep their alpha byte (#rrggbbaa). After the commit the
 * selection is read again so the panel shows the stored style. Baseline stays: no Paper writer is reached from here.
 * SEL-STYLE2 widens the edit to the paint switches, cap / join / miter / dash / paint order / stroke style (and the swap of the two paints, which is
 * a fill and a stroke colour of one edit), a brush preset and Smooth. A preset and a smooth are edits of their own: the pending edit is committed
 * first and they are never merged with another. The steps each item takes are named by native-selection-steps.js. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectionStyle = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var QUIET = 220, MAX_STEPS = 64, MAX_BYTES = 60000; // under the 65536-byte vector request cap
  var SKIPPED = { bitmap: 'bitmap brushes', copy: 'brush dabs' }; // their image derives from more than the record
  var jobs = new WeakMap();
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  function timer(root) { return typeof root.setTimeout === 'function' ? root.setTimeout.bind(root) : setTimeout; }
  function clear(root, id) { return (typeof root.clearTimeout === 'function' ? root.clearTimeout.bind(root) : clearTimeout)(id); }
  function state(root) {
    if (!jobs.has(root)) jobs.set(root, { pending: null, chain: Promise.resolve(), commits: 0 });
    return jobs.get(root);
  }
  function route(root) { var L = root.NemoNativeObjectLabs; return L && typeof L.desktop === 'function' ? L.desktop(root) : null; }
  // The selected items the engine can address: native presented paths (stable target and layer), in selection order.
  function selected(root) {
    return (root.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget && Number.isInteger(p.data.nativeLayerIndex); });
  }
  // True when a Select / Direct Select selection on a native object project can take a style edit natively.
  function handles(root) {
    var st = root.state;
    return !!st && (st.tool === 'select' || st.tool === 'subselect') && selected(root).length > 0 && !!route(root);
  }
  // The style the panel shows for the first selected item (read from the engine's listing): { fill, stroke, width, opacity, role } or null.
  function shown(path) {
    var s = path && path.data && path.data.nativeStyle;
    return s ? Object.assign({ role: path.data.nativeRole }, s) : null;
  }
  function sameSelection(a, b) { return a.length === b.length && a.every(function (id, at) { return id === b[at]; }); }

  async function reselect(root, ids, layerIdx, before) {
    var I = root.NemoSelectCanvasIntent, fresh = null, angles = {};
    before.forEach(function (p) { if (p.data.boxAngle) angles[p.data.strokeId] = p.data.boxAngle; });
    for (var tries = 0; tries < 2 && !fresh; tries++) {
      if (tries) await new Promise(function (done) { timer(root)(done, 120); });
      try { fresh = await I.layerPaths(root, function (item) { return ids.indexOf(item.stableTarget.strokeId) >= 0; }, false, layerIdx); } catch (_) { fresh = null; }
    }
    // The user may have picked something else while the commit ran: their selection stands.
    if (!sameSelection((root.selectedPaths || []).map(function (p) { return p.data && p.data.strokeId; }), ids)) return;
    root.selectedPaths = fresh ? fresh.paths.sort(function (a, b) { return ids.indexOf(a.data.strokeId) - ids.indexOf(b.data.strokeId); })
      .map(function (p) { if (angles[p.data.strokeId] && (p.data.nativeRole === 'raster' || p.data.nativeRole === 'text')) p.data.boxAngle = angles[p.data.strokeId]; return p; }) : [];
    if (root.state) root.state.selectedStrokeIndices = [];
  }
  // ONE native batch of restyle steps for the pending edit, against the document the selection was read from.
  async function commit(root, job) {
    var I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay, labs = route(root), now = I && I.presentedState(root);
    // An edit made while an earlier one of ours was committing was read from the selection before that commit re-read it: it follows the
    // fresh selection of the same items.
    if (job.commits !== state(root).commits) {
      var again = selected(root);
      if (!sameSelection(again.map(function (p) { return p.data.strokeId; }), job.ids)) { notice(root, 'The selection changed — nothing was restyled.'); return { status: 'stale' }; }
      job.paths = again; job.ctx = I && I.pickContext(root);
    }
    var apply = job.paths.filter(function (p) { return !SKIPPED[p.data.nativeBrush]; }), skipped = job.paths.filter(function (p) { return SKIPPED[p.data.nativeBrush]; });
    if (!labs || !now) { notice(root, 'Restyling is not available right now.'); return { status: 'unavailable' }; }
    if (I.pickContext(root) !== job.ctx) { notice(root, 'The selection changed — nothing was restyled.'); return { status: 'stale' }; }
    if (skipped.length) notice(root, 'Restyling ' + skipped.map(function (p) { return SKIPPED[p.data.nativeBrush]; }).filter(function (l, i, all) { return all.indexOf(l) === i; }).join(', ') + ' is not available on native projects yet.');
    if (!apply.length) return { status: 'unavailable' };
    var S = root.NemoNativeSelectionSteps, layerUid = job.layerUid, frame = now.presentation.frame, steps;
    if (job.style.smooth !== undefined) {
      var smoothed = null;
      try { smoothed = await S.smoothSteps(root, apply.map(function (p) { return p.data.strokeId; }), job.layerIdx, job.style.smooth); } catch (_) { smoothed = null; }
      if (!smoothed) { notice(root, 'Could not read the strokes to smooth just now.'); return { status: 'unavailable' }; }
      if (I.pickContext(root) !== job.ctx) { notice(root, 'The selection changed — nothing was restyled.'); return { status: 'stale' }; }
      layerUid = smoothed.layerUid; frame = smoothed.frame; steps = smoothed.steps;
    } else steps = S.restyleSteps(apply, job.style);
    if (!steps.length) return { status: 'noop' };
    if (JSON.stringify(steps).length > MAX_BYTES) { notice(root, 'Too much to restyle at once — select fewer items.'); return { status: 'too_large' }; }
    var result;
    try { result = await labs.batchStrokes(layerUid, frame, steps); } catch (_) { result = null; }
    if (!result || (result.status !== 'applied' && result.status !== 'noop')) {
      notice(root, 'The style could not be applied' + (result && result.status === 'unavailable' ? ' to this kind of item.' : '.'));
      return { status: result ? result.status : 'unavailable' };
    }
    state(root).commits += 1;
    await reselect(root, apply.map(function (p) { return p.data.strokeId; }), job.layerIdx, apply);
    if (O && typeof O.show === 'function') O.show(root);
    if (typeof root.updateUI === 'function') root.updateUI();
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
    return { status: result.status };
  }
  function run(root, job) {
    var s = state(root);
    var next = s.chain.then(function () { return commit(root, job); }, function () { return commit(root, job); });
    s.chain = next.catch(function () {});
    return next.then(function (r) { job.done(r); return r; }, function () { var r = { status: 'unavailable' }; job.done(r); return r; });
  }
  // Commits the pending edit now, whatever the quiet timer says (a selection change, a tool change, a test).
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
  // Records a style edit of the current selection: the members named (fill / stroke as #rrggbb[aa], width, opacity 0..1) replace those of the
  // pending edit of the same selection; the commit follows when the edits stop. Resolves with { status } once committed.
  function apply(root, style) {
    var s = state(root), S = root.NemoNativeSelectionSteps, members = S && S.valid(style), paths = selected(root);
    if (!members) return Promise.resolve({ status: 'invalid' });
    if (!paths.length || !route(root)) return Promise.resolve({ status: 'unavailable' });
    var I = root.NemoSelectCanvasIntent, ids = paths.map(function (p) { return p.data.strokeId; });
    if (paths.some(function (p) { return p.data.nativeLayerIndex !== paths[0].data.nativeLayerIndex; })) {
      notice(root, 'Select items of one layer to restyle them.');
      return Promise.resolve({ status: 'unavailable' });
    }
    if (ids.length > MAX_STEPS) { notice(root, 'Too many items to restyle at once (' + MAX_STEPS + ' at most).'); return Promise.resolve({ status: 'too_large' }); }
    var pending = s.pending, own = S.alone(members);
    // A preset and a smooth are edits of their own: whatever is pending is committed first, and the next edit starts afresh.
    if (pending && (!sameSelection(pending.ids, ids) || pending.own !== own)) { flush(root); pending = null; }
    if (!pending) {
      var job = { ids: ids, paths: paths, style: {}, layerIdx: paths[0].data.nativeLayerIndex, layerUid: paths[0].data.nativeLayerUid, ctx: I && I.pickContext(root), commits: s.commits, timer: 0, waiting: [], own: own };
      job.done = function (r) { job.waiting.forEach(function (w) { w(r); }); };
      s.pending = pending = job;
    }
    Object.assign(pending.style, members);
    var waiting = new Promise(function (resolve) { pending.waiting.push(resolve); });
    arm(root, pending);
    return waiting;
  }
  return Object.freeze({ apply: apply, flush: flush, handles: handles, shown: shown });
}));
