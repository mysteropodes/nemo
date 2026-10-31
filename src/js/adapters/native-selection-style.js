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
 * first and they are never merged with another. The steps each item takes are named by native-selection-steps.js.
 * RV6-STYLE (review of #2630 / #2632): an edit is taken only while the selection is the one the overlay draws for the CURRENT context (frame, layer,
 * tool, revision) - nothing else clears root.selectedPaths on a native project, so a stale selection is dropped, not restyled into another cell.
 * The pending edit commits to the cell it was made on (its layer, frame and document pin), so flush() on a context change (the overlay drops the
 * selection) or a new pick keeps an edit made inside the quiet window. An edit made while a commit runs waits for it and its re-read selection.
 * MULTI-LAYER: a selection may span layers (a marquee or a Shift pick across them); every restyle step names its own layer, so the edit is still ONE
 * batch, one revision and one history entry. */
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
    if (!jobs.has(root)) jobs.set(root, { pending: null, chain: Promise.resolve(), commits: 0, inflight: 0 });
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
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function livePin(root) { var I = root.NemoSelectCanvasIntent, session = I && I.objectProject(root); try { return session ? session.info() : null; } catch (_) { return null; } }
  // True while the overlay draws the selection for the context the app is in now (the same rule as the Select gesture's).
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

  async function reselect(root, ids, before) {
    var I = root.NemoSelectCanvasIntent, fresh = null, angles = {};
    before.forEach(function (p) { if (p.data.boxAngle) angles[p.data.strokeId] = p.data.boxAngle; });
    for (var tries = 0; tries < 2 && !fresh; tries++) {
      if (tries) await new Promise(function (done) { timer(root)(done, 120); });
      try { fresh = await I.reread(root, before, false); } catch (_) { fresh = null; }
    }
    // The user may have picked something else while the commit ran: their selection stands.
    if (!sameSelection((root.selectedPaths || []).map(function (p) { return p.data && p.data.strokeId; }), ids)) return;
    root.selectedPaths = fresh ? fresh.paths
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
      if (!sameSelection(again.map(function (p) { return p.data.strokeId; }), job.ids)) { notice(root, 'The selection changed \u2014 nothing was restyled.'); return { status: 'stale' }; }
      job.paths = again; job.ctx = I && I.pickContext(root); job.pin = now && now.pin; job.frame = now && now.presentation.frame; job.commits = state(root).commits;
    }
    // The edit lands in the cell it was made on: the document must be the one it was read from, whatever frame, layer or tool the app moved to since.
    if (!labs || !I || !job.pin || job.frame === undefined) { notice(root, 'Restyling is not available right now.'); return { status: 'unavailable' }; }
    if (!samePin(livePin(root), job.pin) || (root.state && (root.state.activeSymbolId || null) !== job.symbol)) { notice(root, 'The selection changed \u2014 nothing was restyled.'); return { status: 'stale' }; }
    var here = owns(root); // false when the app has left the context the edit was made in (a flushed edit): the stale selection is dropped after the commit
    var apply = job.paths.filter(function (p) { return !SKIPPED[p.data.nativeBrush]; }), skipped = job.paths.filter(function (p) { return SKIPPED[p.data.nativeBrush]; });
    if (skipped.length) notice(root, 'Restyling ' + skipped.map(function (p) { return SKIPPED[p.data.nativeBrush]; }).filter(function (l, i, all) { return all.indexOf(l) === i; }).join(', ') + ' is not available on native projects yet.');
    if (!apply.length) return { status: 'unavailable' };
    var S = root.NemoNativeSelectionSteps, layerUid = job.layerUid, frame = job.frame, steps;
    if (job.style.smooth !== undefined) {
      // Smooth reads the geometry of the presented cell, so it needs the app still on the context it was made in.
      if (!now || I.pickContext(root) !== job.ctx) { notice(root, 'The selection changed \u2014 nothing was restyled.'); return { status: 'stale' }; }
      var smoothed = null, layerIdxs = [];
      apply.forEach(function (p) { if (layerIdxs.indexOf(p.data.nativeLayerIndex) < 0) layerIdxs.push(p.data.nativeLayerIndex); });
      try {
        // One read per layer the selection sits on; their steps go in the one batch.
        for (var n = 0; n < layerIdxs.length; n++) {
          var part = await S.smoothSteps(root, apply.filter(function (p) { return p.data.nativeLayerIndex === layerIdxs[n]; }).map(function (p) { return p.data.strokeId; }), layerIdxs[n], job.style.smooth);
          if (!part) { smoothed = null; break; }
          smoothed = smoothed ? { layerUid: smoothed.layerUid, frame: smoothed.frame, steps: smoothed.steps.concat(part.steps) } : part;
        }
      } catch (_) { smoothed = null; }
      if (!smoothed) { notice(root, 'Could not read the strokes to smooth just now.'); return { status: 'unavailable' }; }
      if (I.pickContext(root) !== job.ctx) { notice(root, 'The selection changed \u2014 nothing was restyled.'); return { status: 'stale' }; }
      layerUid = smoothed.layerUid; frame = smoothed.frame; steps = smoothed.steps;
    } else steps = job.recolor ? S.recolourSteps(apply, job.recolor.from, job.recolor.to) : S.restyleSteps(apply, job.style, job.last);
    if (!steps.length) return { status: 'noop' };
    if (JSON.stringify(steps).length > MAX_BYTES) { notice(root, 'Too much to restyle at once — select fewer items.'); return { status: 'too_large' }; }
    var result;
    try { result = await labs.batchStrokes(layerUid, frame, steps, { skipUnavailable: true, select: true }); } catch (_) { result = null; }
    if (!result || (result.status !== 'applied' && result.status !== 'noop')) {
      notice(root, 'The style could not be applied' + (result && result.status === 'unavailable' ? ' to this kind of item.' : '.'));
      return { status: result ? result.status : 'unavailable' };
    }
    var applied = apply.map(function (p) { return p.data.strokeId; });
    if (here) await reselect(root, applied, apply);
    else if (sameSelection((root.selectedPaths || []).map(function (p) { return p.data && p.data.strokeId; }), job.ids)) dropSelection(root);
    state(root).commits += 1; // after the re-read: an edit that follows sees the fresh selection, never the one before this commit
    if (O && typeof O.show === 'function') O.show(root);
    if (typeof root.updateUI === 'function') root.updateUI();
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
    return { status: result.status };
  }
  function run(root, job) {
    var s = state(root), settle = function () { s.inflight -= 1; };
    var next = s.chain.then(function () { return commit(root, job); }, function () { return commit(root, job); });
    s.inflight += 1;
    next.then(settle, settle);
    s.chain = next.then(function () {}, function () {});
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
    var S = root.NemoNativeSelectionSteps, members = S && S.valid(style);
    return members ? edit(root, members, null, S.alone(members)) : Promise.resolve({ status: 'invalid' });
  }
  // The Selected Colors panel's recolour (#2372 insp-recolour): every fill or stroke of the selection equal to `from` becomes `to`, as ONE restyle batch.
  // Coalesced like any panel edit (a picker drag or an opacity scrub fires many times); a recolour of another colour is an edit of its own.
  // The panel's row moves its from-colour to the colour it just sent (baseline: the synchronous recolour re-rendered the row), so a drag arrives as a
  // CHAIN, each event starting from the previous event's `to`: while the edit is pending the chain extends it (the colours the items still show are the
  // chain's first `from`), and once it has committed the items show the chain's last colour, which the next event starts from.
  function applyRecolor(root, from, to) {
    var S = root.NemoNativeSelectionSteps, a = S && S.key(from), b = S && S.key(to), ok = /^#[0-9a-f]{6}([0-9a-f]{2})?$/;
    if (!(ok.test(a) && ok.test(b))) return Promise.resolve({ status: 'invalid' });
    var s = state(root);
    // The edit follows a commit in flight, then decides against the pending edit it finds.
    if (s.inflight) return s.chain.then(function () { return applyRecolor(root, from, to); });
    var p = s.pending;
    if (p && p.recolor) {
      if (a === p.recolor.from) return edit(root, {}, { from: a, to: b }, p.own); // the same colour edited again: the later target wins
      // A continuation extends the pending edit unless the intermediate colour is also worn by another listed item: that one is a different group the
      // baseline would have recoloured together with this one only AFTER merging, so the pending edit commits first and this one follows it.
      if (a === p.recolor.to && !p.paths.some(function (path) { return S.coloursOf(path).indexOf(a) >= 0; })) return edit(root, {}, { from: p.recolor.from, to: b }, p.own);
    }
    return edit(root, {}, { from: a, to: b }, 'recolor:' + a);
  }
  function edit(root, members, recolor, own) {
    var s = state(root), paths = selected(root);
    if (!paths.length || !route(root)) return Promise.resolve({ status: 'unavailable' });
    // A commit in flight re-reads the selection when it ends: this edit follows it instead of reading the one before.
    if (s.inflight) return s.chain.then(function () { return edit(root, members, recolor, own); });
    var I = root.NemoSelectCanvasIntent, ids = paths.map(function (p) { return p.data.strokeId; }), ctx = I && I.pickContext(root), pending = s.pending;
    // The selection must be the one drawn for this frame, layer and tool: a selection that outlived a context change is dropped, never restyled.
    if (!(owns(root) || (pending && pending.ctx === ctx && sameSelection(pending.ids, ids)))) {
      dropSelection(root);
      notice(root, 'The selection no longer applies here \u2014 select the items again.');
      return Promise.resolve({ status: 'unavailable' });
    }
    if (ids.length > MAX_STEPS) { notice(root, 'Too many items to restyle at once (' + MAX_STEPS + ' at most).'); return Promise.resolve({ status: 'too_large' }); }
    // A preset and a smooth are edits of their own: whatever is pending is committed first, and the next edit starts afresh.
    if (pending && (!sameSelection(pending.ids, ids) || pending.own !== own)) { flush(root); pending = null; }
    if (!pending) {
      var shown = I.presentedState(root);
      if (!shown) { notice(root, 'Restyling is not available right now.'); return Promise.resolve({ status: 'unavailable' }); }
      var job = { ids: ids, paths: paths, style: {}, layerIdx: paths[0].data.nativeLayerIndex, layerUid: paths[0].data.nativeLayerUid, ctx: ctx, pin: shown.pin, frame: shown.presentation.frame,
        symbol: (root.state && root.state.activeSymbolId) || null, commits: s.commits, timer: 0, waiting: [], own: own, last: null, recolor: null };
      job.done = function (r) { job.waiting.forEach(function (w) { w(r); }); };
      s.pending = pending = job;
    }
    Object.assign(pending.style, members);
    if (recolor) pending.recolor = recolor;
    // A ribbon is inked by one colour: when the fill and the stroke colour differ in one window the later write wins, as the baseline's two setters did.
    if (members.stroke !== undefined) pending.last = 'stroke'; else if (members.fill !== undefined) pending.last = 'fill';
    var waiting = new Promise(function (resolve) { pending.waiting.push(resolve); });
    arm(root, pending);
    return waiting;
  }
  // True on a native object project (the panel's writes then go through the engine, never a Paper item).
  function native(root) { return !!route(root); }
  // The colours of the selected native items, most used first, as the Selected Colors panel lists them; null when no native item is selected.
  function colours(root) {
    var S = root.NemoNativeSelectionSteps, paths = selected(root), map = {};
    if (!S || !paths.length) return null;
    paths.forEach(function (p) { S.coloursOf(p).forEach(function (c) { (map[c] = map[c] || { hex: c, count: 0 }).count++; }); });
    return Object.keys(map).map(function (c) { return map[c]; }).sort(function (a, b) { return b.count - a.count; });
  }
  return Object.freeze({ apply: apply, applyRecolor: applyRecolor, colours: colours, native: native, flush: flush, handles: handles, shown: shown });
}));
