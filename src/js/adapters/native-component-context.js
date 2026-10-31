/* #2256 leaf 3: the session view of an entered native component context (the AE-precomp style "double-click a component").
 * Baseline: app.js enterSymbol / exitToScene / closeSymbolTab and timeline.js renderSymbolTabs (1065e9d8). On a native object project the
 * component is a CONTEXT of the one document, never a second document and never a JS copy: this module only asks the native owner
 * (NemoNativeObjectComponent.workspaceTransition, the pure query.document.object.workspace.context) which context is entered, reads
 * its layers through the pinned component read, projects the read-only view fields the ordinary UI reads (playhead, frame count,
 * work area, fps, tab strip) and asks the viewport to present the entered context. It writes no document and no history entry.
 * Editing INSIDE an entered component is offered to the vector family only (#2256 component writes: Draw, Pen and shape create, and Select's
 * pick, move / scale / rotate, restyle and Delete, through the component command route, which targets the context held here; the timeline
 * paints the component's own frames and range from the pinned timing reads of that context, native-component-grid.js); every other action that needs the presented scene lease declines (the
 * presented context is not scene-root) and the scene editors keep `state.activeSymbolId` set. Exit restores the scene projection from the
 * native truth (projectView), never from a saved copy of the rows. Legacy documents never reach this module. */
(function (root, factory) {
  var api = factory(typeof module === 'object' && module.exports ? require('./native-component-grid.js') : root.NemoNativeComponentGrid);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentContext = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Grid) {
  'use strict';
  var SCENE = 'scene-root';
  var entered = new WeakMap(); // host -> the entered context's read-side view; absent while the scene is presented
  var names = new WeakMap();   // host -> {key, map}: the component names the tab strip shows
  var busy = new WeakSet();
  var queued = new WeakSet(); // a revision that arrived while a transition was in flight: the context is re-read once that transition is done

  function session(host) {
    try {
      var project = host.NemoNativeObjectProject, s = project && project.desktop(host);
      return s && s.blocksLegacy() === true && s.isActive() === true ? s : null;
    } catch (_) { return null; }
  }
  function intent(host) { try { var c = host.NemoNativeObjectComponent; return c ? c.desktop(host) : null; } catch (_) { return null; } }
  function keyOf(info) { return info ? info.instanceId + '|' + info.documentId : null; }
  function toast(host, text) { try { if (typeof host.showToast === 'function') host.showToast(text); } catch (_) { /* presentation only */ } }
  function t(host, key, fallback) { try { var v = host.SM && host.SM.t && host.SM.t(key); return typeof v === 'string' && v ? v : fallback; } catch (_) { return fallback; } }

  // The entered context, or null while the scene is presented (or when the project was replaced since it was entered).
  function live(host) {
    var st = entered.get(host);
    if (!st) return null;
    var s = session(host), info = s && s.info();
    if (!info) return null; // transiently not active: the entered view is neither dropped nor presented
    if (keyOf(info) !== st.key) { entered.delete(host); return null; }
    return st;
  }
  // What the viewport presents instead of scene-root; a frozen copy of facts the owner answered.
  function presented(host) {
    var st = live(host);
    return st ? Object.freeze({ contextId: st.contextId, totalFrames: st.totalFrames, fps: st.fps }) : null;
  }
  // #2256 component writes. The context an edit targets is the one the workspace controller holds (the id the owner declared on enter), never a caller's
  // text: the scene root while no component is entered. layerRows is what the drawing tools judge a layer by inside it (the owner judges lock and effects).
  function contextId(host) { var st = live(host); return st ? st.contextId : SCENE; }
  function layerRows(host) {
    var st = live(host);
    return st ? st.rows.map(function (row) { return { layerUid: row.layerUid, locked: false, visible: true }; }) : null;
  }
  function frameBound(host, pin) { var st = live(host); return st ? st.totalFrames : pin.totalFrames; }
  // #2256: what a pinned timing read of the held context is judged by: the component's own id, frame count and layer uids (null at the scene root).
  function readScope(host) {
    var st = live(host);
    return st ? Object.freeze({ contextId: st.contextId, totalFrames: st.totalFrames, layerUids: Object.freeze(st.rows.map(function (row) { return row.layerUid; })) }) : null;
  }
  // The entered rows with their names (what the Select adapters walk as the layer list of the context).
  function layers(host) { var st = live(host); return st ? st.rows.map(function (row) { return Object.freeze({ layerUid: row.layerUid, name: row.name, locked: false, visible: true }); }) : null; }
  // Row click: the component's layer becomes the active (and only selected) one. The layer table of a component is read-only here; this is a view choice.
  function selectLayer(host, index) {
    var st = live(host);
    if (!st || !Number.isSafeInteger(index) || index < 0 || index >= st.rows.length) return false;
    host.state.activeLayerIdx = index; host._layerSel = [index]; host._layerSelAnchor = index; host.selectedPaths = []; host.state.selectedStrokeIndices = [];
    repaint(host);
    return true;
  }
  function integer(value, fallback) { return Number.isSafeInteger(value) && value >= 0 ? value : fallback; }
  // The live playhead and work area of the presented context, clamped into it (the owner refuses an out-of-range view).
  function viewNow(host) {
    var s = host.state, total = Math.max(1, integer(s.totalFrames, 1)), last = total - 1;
    var frame = Math.min(integer(s.currentFrame, 0), last), lo = Math.min(integer(s.waIn, 0), last), hi = Math.min(integer(s.waOut, last), last);
    return { frame: frame, workIn: Math.min(lo, hi), workOut: Math.max(lo, hi) };
  }
  // `via` (optional) is the lease of a publication that already runs inside the project queue (see revision).
  async function readNames(host, c, info, via) {
    var held = names.get(host), key = keyOf(info);
    if (!held || held.key !== key) { held = { key: key, map: {} }; names.set(host, held); }
    var all = await c.listContexts(via);
    if (all.ok) all.contexts.forEach(function (ctx) { if (ctx.contextKind === 'component') held.map[ctx.contextId] = typeof ctx.name === 'string' && ctx.name ? ctx.name : ctx.contextId; });
    return held.map;
  }
  async function readRows(c, contextId, via) {
    var all = await c.listLayers(contextId, via);
    if (!all.ok) return null;
    var rows = [];
    all.items.forEach(function (layer) {
      rows.push({ layerUid: layer.layerUid, name: layer.name === null ? 'Layer ' + (rows.length + 1) : layer.name, layerKind: layer.layerKind,
        visible: true, locked: false, color: '#5FA875', parentLayerUid: null, frames: [], motion: {} });
    });
    return rows;
  }
  function selectedUids(host) {
    var s = host.state, rows = Array.isArray(s.layers) ? s.layers : [], sel = Array.isArray(host._layerSel) ? host._layerSel : [];
    var uid = function (i) { return rows[i] && typeof rows[i].layerUid === 'string' ? rows[i].layerUid : null; };
    var anchor = Number.isInteger(host._layerSelAnchor) ? uid(host._layerSelAnchor) : null;
    return { layerUids: sel.map(uid).filter(Boolean), activeLayerUid: uid(s.activeLayerIdx), anchorLayerUid: anchor };
  }
  // The read-side view fields the ordinary UI reads. No Paper object, no document: the baseline swapped sym.layers into state.layers.
  function projectEntered(host, st, view) {
    var s = host.state;
    s.activeSymbolId = st.contextId; s.openSymbolTabs = st.tabs.slice();
    s.layers = st.rows; s.totalFrames = st.totalFrames; s.fps = st.fps;
    s.waIn = view.workIn; s.waOut = view.workOut; s.currentFrame = view.frame; s.activeLayerIdx = 0;
    host._curFrame = view.frame; host._totalF = st.totalFrames; host._waIn = view.workIn; host._waOut = view.workOut;
    host._layerSel = st.rows.length ? [0] : []; host._layerSelAnchor = 0; host.selectedPaths = []; s.selectedStrokeIndices = [];
    if (host._sel) host._sel.frames = [];
  }
  function repaint(host) {
    try { if (typeof host.renderSymbolTabs === 'function') host.renderSymbolTabs(); } catch (_) { /* presentation only */ }
    try { if (typeof host.updateUI === 'function') host.updateUI(); } catch (_) { /* the paint reports its own failure */ }
  }
  async function present(host, frame) { var s = session(host); if (s && typeof s.renderPreview === 'function') await s.renderPreview(frame); }

  async function applyEntered(host, res, saved) {
    var s = session(host), c = intent(host), info = s.info();
    var rows = await readRows(c, res.contextId);
    if (!rows) throw new Error('component layers unavailable');
    var map = await readNames(host, c, info);
    var st = { key: keyOf(info), contextId: res.contextId, totalFrames: res.totalFrames, fps: res.fps, rows: rows,
      tabs: res.workspace.openTabs, name: map[res.contextId] || res.contextId, saved: saved, revision: info.contentRevision };
    entered.set(host, st);
    projectEntered(host, st, res.workspace.view);
    await present(host, res.workspace.view.frame);
    repaint(host);
  }
  // Back to the scene projection, from the native truth. The frame is the scene playhead the owner restored.
  async function applyExited(host, res, saved, lease) {
    var st = entered.get(host);
    entered.delete(host);
    var s = session(host), info = s && s.info(), file = host.NemoNativeObjectProjectFile;
    if (!info || !file) throw new Error('native scene projection unavailable');
    var frame = Math.min(res.workspace.view.frame, info.totalFrames - 1), keep = saved || (st ? st.saved : null);
    try { file.projectView(host, info, { frame: frame, layerUids: keep ? keep.layerUids : [], activeLayerUid: keep ? keep.activeLayerUid : null, anchorLayerUid: keep ? keep.anchorLayerUid : null }); }
    catch (_) { file.projectView(host, info, null); frame = 0; }
    if (!lease) await present(host, frame); // inside a publication the revision's own frame is presented next, with the scene context
    repaint(host);
  }

  function guarded(host, work) {
    if (busy.has(host)) return Promise.resolve(false);
    busy.add(host);
    return Promise.resolve().then(work).catch(function (error) {
      toast(host, 'Native component context: ' + (error && error.message || 'unavailable'));
      return false;
    }).then(function (done) {
      busy.delete(host);
      if (queued.delete(host)) guarded(host, function () { var s = session(host), i = s && s.info(); return i ? settle(host, null, i.contentRevision) : false; });
      return done;
    });
  }
  // A transition that read a pin the project moved off is asked again (the answer was discarded, nothing was applied).
  async function transition(c, action, view, via) {
    var res;
    for (var attempt = 0; attempt < 3; attempt += 1) {
      res = await c.workspaceTransition(action, view, via);
      if (res.ok || res.code !== 'stale_revision') return res;
    }
    return res;
  }
  // enterSymbol(symId). `layerUid` (optional) is the instance layer entered: the owner starts at the component frame that instance shows at
  // the scene playhead (Motion enterComponentLayer: resolveSymbolFrameIdx), where a plain tab or SM.enterSymbol starts at the first frame.
  function enter(host, contextId, entryFrame, layerUid) {
    return guarded(host, async function () {
      var c = intent(host), s = session(host);
      if (!c || !s || !s.info()) { toast(host, 'Native component context is unavailable.'); return false; }
      var saved = selectedUids(host);
      var action = { kind: 'enter', contextId: contextId, entryFrame: Number.isSafeInteger(entryFrame) ? entryFrame : null };
      if (typeof layerUid === 'string' && layerUid && action.entryFrame === null) action.fromLayerUid = layerUid;
      var res = await transition(c, action, viewNow(host));
      if (!res.ok) { if (res.code !== 'not_found') toast(host, 'Native component context: ' + (res.reason || res.code)); return false; }
      if (res.outcome === 'refused_nested') { toast(host, t(host, 'toastNestedComponentsUnsupported', 'Close the component first.')); return false; }
      if (res.outcome !== 'entered') return false;
      try { await applyEntered(host, res, saved); } catch (error) {
        // The owner entered the context but it could not be presented: leave it again rather than show a half-entered view.
        entered.delete(host);
        var back = await transition(c, { kind: 'exit' }, res.workspace.view);
        if (back.ok) await applyExited(host, back, saved).catch(function () { return null; });
        throw error;
      }
      return true;
    });
  }
  // exitToScene()
  function exit(host) {
    return guarded(host, async function () {
      var c = intent(host);
      if (!c || !live(host)) return false;
      var res = await transition(c, { kind: 'exit' }, viewNow(host));
      if (!res.ok) { toast(host, 'Native component context: ' + (res.reason || res.code)); return false; }
      // A component that vanished under the user (deleted by Undo or an agent) answers not_in_context: the scene is shown all the same.
      if (res.outcome !== 'exited' && res.outcome !== 'not_in_context') return false;
      await applyExited(host, res);
      return true;
    });
  }
  // closeSymbolTab(symId): the active tab exits first.
  function close(host, contextId) {
    return guarded(host, async function () {
      var c = intent(host), was = live(host);
      if (!c) return false;
      var res = await transition(c, { kind: 'close', contextId: contextId }, viewNow(host));
      if (!res.ok) { toast(host, 'Native component context: ' + (res.reason || res.code)); return false; }
      if (was && res.workspace.activeContextId === null) await applyExited(host, res);
      else if (res.outcome === 'closed') repaint(host);
      return res.outcome === 'closed';
    });
  }
  // The entered context after a refresh answer: its frame count, fps, tabs, layers and the owner's clamped playhead and work area.
  function projectRefreshed(host, st, res, rows) {
    var s = host.state, view = res.workspace.view, count = rows ? rows.length : st.rows.length;
    if (rows) st.rows = rows;
    st.totalFrames = res.totalFrames; st.fps = res.fps; st.tabs = res.workspace.openTabs;
    s.openSymbolTabs = st.tabs.slice(); s.layers = st.rows; s.totalFrames = st.totalFrames; s.fps = st.fps;
    s.waIn = view.workIn; s.waOut = view.workOut; s.currentFrame = view.frame; s.activeLayerIdx = Math.min(integer(s.activeLayerIdx, 0), Math.max(0, count - 1));
    host._curFrame = view.frame; host._totalF = st.totalFrames; host._waIn = view.workIn; host._waOut = view.workOut;
    host._layerSel = (Array.isArray(host._layerSel) ? host._layerSel : []).filter(function (i) { return i < count; });
  }
  // Re-read the entered context against the current revision: a context that vanished is left, the others re-read their layers.
  // Resolves true once the context is settled (refreshed, gone, or no longer entered) and false when the owner could not answer.
  async function refresh(host, lease) {
    var c = intent(host), was = entered.get(host);
    if (!was) return true;
    if (!c) return false;
    var res = await transition(c, { kind: 'refresh' }, viewNow(host), lease);
    if (!res.ok) return false;
    if (res.outcome === 'context_gone') { await applyExited(host, res, null, lease); toast(host, 'The component no longer exists.'); return true; }
    var rows = await readRows(c, was.contextId, lease);
    if (entered.get(host) !== was) return true; // left (or replaced) while the layers were read
    await readNames(host, c, session(host).info(), lease).catch(function () { return null; });
    projectRefreshed(host, was, res, rows);
    repaint(host);
    return true;
  }
  // A failed refresh is asked again after these pauses (bounded), then the scene is shown instead of a stale entered view.
  var RETRY_MS = [100, 300, 900];
  function pause(host, ms) { return new Promise(function (resolve) { (typeof host.setTimeout === 'function' ? host.setTimeout : setTimeout)(resolve, ms); }); }
  // Retries ran out: leave the context for the scene (the owner's exit, applied from the native truth) and say so.
  async function giveUp(host, lease) {
    var c = intent(host);
    try {
      var res = c ? await transition(c, { kind: 'exit' }, viewNow(host), lease) : null;
      if (res && res.ok && (res.outcome === 'exited' || res.outcome === 'not_in_context')) {
        await applyExited(host, res, null, lease);
        toast(host, 'The component could not be refreshed; back to the Scene.');
        return true;
      }
    } catch (_) { /* reported below: the entered view stays fenced (projectionReady is false) */ }
    toast(host, 'Native component context: the component could not be refreshed.');
    return false;
  }
  // Re-read the entered context until it is settled. The revision is acknowledged (`st.revision`) ONLY once a refresh succeeded: until then
  // projectionReady() is false, so the viewport never presents a revision over rows, tabs and a frame count that are stale.
  async function settle(host, lease, target) {
    var st = live(host);
    for (var attempt = 0; attempt <= RETRY_MS.length; attempt += 1) {
      if (!st || live(host) !== st) return true; // left, closed or replaced meanwhile: nothing left to settle
      var ok = false;
      try { ok = await refresh(host, lease); } catch (_) { ok = false; }
      if (ok) { if (live(host) === st) st.revision = target; return true; }
      if (attempt < RETRY_MS.length) await pause(host, RETRY_MS[attempt]);
    }
    return st && live(host) === st ? giveUp(host, lease) : true;
  }
  // A revision moved on (Undo, an MCP edit). `lease` is the publication the project queue is running: the refresh rides it, so the
  // context, frame count and layers are settled BEFORE the frame of this revision is presented (a shrunk or deleted component is
  // never asked for a frame it no longer has). Without a lease the refresh is queued behind any transition in flight, never dropped.
  function revision(host, info, lease) {
    var st = live(host), target = info.contentRevision;
    if (!st || st.revision === target) return Promise.resolve(true);
    if (lease) return settle(host, lease, target).then(function () { return true; });
    if (busy.has(host)) { queued.add(host); return Promise.resolve(true); }
    return guarded(host, function () { return settle(host, null, target); }).then(function () { return true; });
  }
  // The final projection check of a revision synchronization while a component is presented: same owner, same revision.
  function projectionReady(host, info) {
    var st = live(host);
    return !!st && !!info && st.key === keyOf(info) && st.revision === info.contentRevision;
  }
  // Double-click on an instance layer: the component it instances (scene layers are read, never guessed).
  async function openLayer(host, layerUid) {
    var c = intent(host);
    if (!c || !session(host) || live(host)) return false;
    var all = await c.listContexts();
    if (!all.ok) return false;
    for (var i = 0; i < all.contexts.length; i += 1) {
      var ctx = all.contexts[i];
      if (ctx.contextKind !== 'component') continue;
      var found = await c.listInstances(ctx.contextId);
      if (found.ok && found.items.some(function (item) { return item.layerUid === layerUid; })) return enter(host, ctx.contextId, null, layerUid);
    }
    return false;
  }

  function node(host, tag, text, cls) { var el = host.document.createElement(tag); el.textContent = text; if (cls) el.className = cls; return el; }
  // The pinned timing read of the entered component (cells: the whole row; range: the layer's in/out at the playhead frame). The reads consult the
  // held context themselves; a read the owner refuses answers null and the row stays blank.
  function timingPort(host, st) {
    return {
      current: function () { return live(host) === st; },
      select: function (index) { selectLayer(host, index); },
      read: function (kind, layerUid) {
        var s = session(host), info = s && s.info();
        if (!info || typeof s.readLayerCells !== 'function' || typeof s.readLayerRange !== 'function') return null;
        if (kind !== 'cells') return s.readLayerRange(layerUid, host.state.currentFrame, info);
        // A row's cells are a function of the revision alone (the playhead is not an input), so a scrub reads each row once per revision.
        if (!st.cells || st.cells.revision !== info.contentRevision) st.cells = { revision: info.contentRevision, rows: new Map() };
        var held = st.cells;
        if (held.rows.has(layerUid)) return Promise.resolve(held.rows.get(layerUid));
        return Promise.resolve(s.readLayerCells(layerUid, info)).then(function (runs) { if (runs && st.cells === held) held.rows.set(layerUid, runs); return runs; });
      },
    };
  }
  // The paint of an entered component (the scene paint is skipped: its pins and rows are the scene's). The grid is the component's own timeline.
  function paint(host) {
    var st = live(host);
    if (!st) return false;
    if (Grid) Grid.paint(host, st, timingPort(host, st));
    var doc = host.document, frame = host.state.currentFrame;
    var field = doc.getElementById('tl-cf');
    if (field) { if (doc.activeElement !== field) field.value = frame + 1; field.max = st.totalFrames; }
    ['tl-tf', 'info-frame'].forEach(function (id) { var el = doc.getElementById(id); if (el) el.textContent = id === 'tl-tf' ? st.totalFrames : frame + 1; });
    var head = doc.getElementById('playhead'), flag = doc.getElementById('playhead-flag');
    if (head && typeof host.playheadLeftPx === 'function') head.style.left = host.playheadLeftPx(frame) + 'px';
    if (flag) flag.textContent = frame + 1;
    return true;
  }
  // timeline.js renderSymbolTabs(): Scene, then one tab per open component. Legacy documents keep their own strip.
  function renderTabs(host, bar) {
    if (!session(host)) return false;
    var c = intent(host), ws = c && typeof c.workspaceNow === 'function' ? c.workspaceNow() : null;
    var active = ws ? ws.activeContextId : null, tabs = ws ? ws.openTabs : [];
    var held = names.get(host), map = held ? held.map : {};
    bar.innerHTML = '';
    var scene = node(host, 'div', 'Scene', 'sym-tab' + (active ? '' : ' act'));
    scene.addEventListener('click', function () { if (live(host)) exit(host); });
    bar.appendChild(scene);
    tabs.forEach(function (id) {
      var tab = node(host, 'div', '', 'sym-tab' + (active === id ? ' act' : '')), x = node(host, 'span', '×', 'sym-tab-x');
      tab.appendChild(node(host, 'span', map[id] || id)); tab.appendChild(x);
      tab.addEventListener('click', function (e) { if (e.target !== x) enter(host, id, null); });
      x.addEventListener('click', function (e) { e.stopPropagation(); close(host, id); });
      bar.appendChild(tab);
    });
    return true;
  }
  return Object.freeze({ SCENE: SCENE, presented: presented, contextId: contextId, layerRows: layerRows, layers: layers, readScope: readScope, selectLayer: selectLayer, frameBound: frameBound, enter: enter, exit: exit, close: close, revision: revision, projectionReady: projectionReady, openLayer: openLayer, paint: paint, renderTabs: renderTabs });
}));
