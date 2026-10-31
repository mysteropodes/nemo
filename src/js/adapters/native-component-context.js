/* #2256 leaf 3: the session view of an entered native component context (the AE-precomp style "double-click a component").
 * Baseline: app.js enterSymbol / exitToScene / closeSymbolTab and timeline.js renderSymbolTabs (1065e9d8). On a native object project the
 * component is a CONTEXT of the one document, never a second document and never a JS copy: this module only asks the native owner
 * (NemoNativeObjectComponent.workspaceTransition, the pure query.document.object.workspace.context) which context is entered, reads
 * its layers through the pinned component read, projects the read-only view fields the ordinary UI reads (playhead, frame count,
 * work area, fps, tab strip) and asks the viewport to present the entered context. It writes no document and no history entry.
 * Editing INSIDE an entered component is not offered: every action that needs the presented scene lease declines (the presented
 * context is not scene-root) and the scene editors keep `state.activeSymbolId` set. Exit restores the scene projection from the
 * native truth (projectView), never from a saved copy of the rows. Legacy documents never reach this module. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentContext = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var SCENE = 'scene-root';
  var entered = new WeakMap(); // host -> the entered context's read-side view; absent while the scene is presented
  var names = new WeakMap();   // host -> {key, map}: the component names the tab strip shows
  var busy = new WeakSet();

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
  function integer(value, fallback) { return Number.isSafeInteger(value) && value >= 0 ? value : fallback; }
  // The live playhead and work area of the presented context, clamped into it (the owner refuses an out-of-range view).
  function viewNow(host) {
    var s = host.state, total = Math.max(1, integer(s.totalFrames, 1)), last = total - 1;
    var frame = Math.min(integer(s.currentFrame, 0), last), lo = Math.min(integer(s.waIn, 0), last), hi = Math.min(integer(s.waOut, last), last);
    return { frame: frame, workIn: Math.min(lo, hi), workOut: Math.max(lo, hi) };
  }
  async function readNames(host, c, info) {
    var held = names.get(host), key = keyOf(info);
    if (!held || held.key !== key) { held = { key: key, map: {} }; names.set(host, held); }
    var all = await c.listContexts();
    if (all.ok) all.contexts.forEach(function (ctx) { if (ctx.contextKind === 'component') held.map[ctx.contextId] = typeof ctx.name === 'string' && ctx.name ? ctx.name : ctx.contextId; });
    return held.map;
  }
  async function readRows(c, contextId) {
    var rows = [], cursor = 0;
    for (var guard = 0; guard < 64; guard += 1) {
      var page = await c.read({ kind: 'layers', contextId: contextId, cursor: cursor, limit: 16 });
      if (!page.ok) return null;
      page.result.items.forEach(function (layer) {
        rows.push({ layerUid: layer.layerUid, name: layer.name === null ? 'Layer ' + (rows.length + 1) : layer.name, layerKind: layer.layerKind,
          visible: true, locked: true, color: '#5FA875', parentLayerUid: null, frames: [], motion: {} });
      });
      if (page.result.nextCursor === null) return rows;
      cursor = page.result.nextCursor;
    }
    return null;
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
  async function applyExited(host, res, saved) {
    var st = entered.get(host);
    entered.delete(host);
    var s = session(host), info = s && s.info(), file = host.NemoNativeObjectProjectFile;
    if (!info || !file) throw new Error('native scene projection unavailable');
    var frame = Math.min(res.workspace.view.frame, info.totalFrames - 1), keep = saved || (st ? st.saved : null);
    try { file.projectView(host, info, { frame: frame, layerUids: keep ? keep.layerUids : [], activeLayerUid: keep ? keep.activeLayerUid : null, anchorLayerUid: keep ? keep.anchorLayerUid : null }); }
    catch (_) { file.projectView(host, info, null); frame = 0; }
    await present(host, frame);
    repaint(host);
  }

  function guarded(host, work) {
    if (busy.has(host)) return Promise.resolve(false);
    busy.add(host);
    return Promise.resolve().then(work).catch(function (error) {
      toast(host, 'Native component context: ' + (error && error.message || 'unavailable'));
      return false;
    }).then(function (done) { busy.delete(host); return done; });
  }
  // enterSymbol(symId)
  function enter(host, contextId, entryFrame) {
    return guarded(host, async function () {
      var c = intent(host), s = session(host);
      if (!c || !s || !s.info()) { toast(host, 'Native component context is unavailable.'); return false; }
      var saved = selectedUids(host);
      var res = await c.workspaceTransition({ kind: 'enter', contextId: contextId, entryFrame: Number.isSafeInteger(entryFrame) ? entryFrame : null }, viewNow(host));
      if (!res.ok) { if (res.code !== 'not_found') toast(host, 'Native component context: ' + (res.reason || res.code)); return false; }
      if (res.outcome === 'refused_nested') { toast(host, t(host, 'toastNestedComponentsUnsupported', 'Close the component first.')); return false; }
      if (res.outcome !== 'entered') return false;
      try { await applyEntered(host, res, saved); } catch (error) {
        // The owner entered the context but it could not be presented: leave it again rather than show a half-entered view.
        entered.delete(host);
        var back = await c.workspaceTransition({ kind: 'exit' }, res.workspace.view);
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
      var res = await c.workspaceTransition({ kind: 'exit' }, viewNow(host));
      if (!res.ok) { toast(host, 'Native component context: ' + (res.reason || res.code)); return false; }
      if (res.outcome !== 'exited') return false;
      await applyExited(host, res);
      return true;
    });
  }
  // closeSymbolTab(symId): the active tab exits first.
  function close(host, contextId) {
    return guarded(host, async function () {
      var c = intent(host), was = live(host);
      if (!c) return false;
      var res = await c.workspaceTransition({ kind: 'close', contextId: contextId }, viewNow(host));
      if (!res.ok) { toast(host, 'Native component context: ' + (res.reason || res.code)); return false; }
      if (was && res.workspace.activeContextId === null) await applyExited(host, res);
      else if (res.outcome === 'closed') repaint(host);
      return res.outcome === 'closed';
    });
  }
  // A revision moved on (Undo, an agent edit): a context that vanished is left, the others re-read their layers.
  function revision(host, info) {
    var st = live(host);
    if (!st || st.revision === info.contentRevision) return;
    st.revision = info.contentRevision;
    guarded(host, async function () {
      var c = intent(host), was = entered.get(host);
      if (!c || !was) return false;
      var res = await c.workspaceTransition({ kind: 'refresh' }, viewNow(host));
      if (!res.ok) return false;
      if (res.outcome === 'context_gone') { await applyExited(host, res); toast(host, 'The component no longer exists.'); return true; }
      var rows = await readRows(c, was.contextId);
      if (rows) { was.rows = rows; host.state.layers = rows; }
      repaint(host);
      return true;
    });
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
      for (var cursor = 0, guard = 0; guard < 64; guard += 1) {
        var page = await c.read({ kind: 'instances', contextId: ctx.contextId, cursor: cursor, limit: 16 });
        if (!page.ok) break;
        if (page.result.items.some(function (item) { return item.layerUid === layerUid; })) return enter(host, ctx.contextId, null);
        if (page.result.nextCursor === null) break;
        cursor = page.result.nextCursor;
      }
    }
    return false;
  }

  function node(host, tag, text, cls) { var el = host.document.createElement(tag); el.textContent = text; if (cls) el.className = cls; return el; }
  // The read-side paint of an entered component (the scene paint is skipped: its pins and rows are the scene's).
  function paint(host) {
    var st = live(host);
    if (!st) return false;
    var doc = host.document, list = doc.getElementById('layer-list'), header = doc.getElementById('frame-hdr'), grid = doc.getElementById('frame-grid');
    if (!list || !header || !grid) return true;
    var rows = st.rows.map(function (layer) {
      var row = node(host, 'div', '', 'native-object-layer native-component-layer');
      row.dataset.layerUid = layer.layerUid; row.style.display = 'flex';
      var pick = node(host, 'button', layer.name + ' — ' + layer.layerKind); pick.type = 'button'; pick.disabled = true; pick.style.flex = '1'; pick.style.textAlign = 'left';
      pick.setAttribute('aria-label', layer.name + ' (' + layer.layerUid + ')');
      row.appendChild(pick); return row;
    });
    list.replaceChildren.apply(list, rows);
    var frame = host.state.currentFrame;
    header.replaceChildren(node(host, 'div', st.name + ' — Frame ' + (frame + 1) + ' / ' + st.totalFrames));
    var note = node(host, 'div', 'Component layers are read-only here: editing inside a component is not available yet.', 'native-object-timing-unavailable');
    note.setAttribute('role', 'status'); grid.replaceChildren(note);
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
  return Object.freeze({ SCENE: SCENE, presented: presented, enter: enter, exit: exit, close: close, revision: revision, openLayer: openLayer, paint: paint, renderTabs: renderTabs });
}));
