/* #2256 A-comp-edit-timeline part 3: the in/out bar of an ENTERED native component is editable (drag, selection, context menu).
 * Baseline: enterSymbol swaps state.layers for the component's layers, so layer-inout.js drags, selects and offers its bar menu on the
 * component's layers unchanged (1065e9d8:src/js/layer-inout.js onDown, installBar, the bar contextmenu; app.js:4130 enterSymbol).
 * Native: the gesture is the scene's own (`object.layer.range.drag`: ordered samples, bar selection, part, plain click, Alt) stamped with
 * the held component id and sent through the shared frame-edit route (NemoNativeComponentFrameEdit, kind rangeDrag); the menu is the
 * scene's own (NemoNativeLayerRangeMenu) whose batch writes go through `object.layer.range.arrange` with the same contextId. One gesture
 * is one revision and one history entry. This module keeps only the UI bar selection; no document, no Paper object, no fallback writer.
 * Parent in Time is not offered inside a component. */
(function (root, factory) {
  var api = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentRangeEdit = api;
}(typeof globalThis === 'object' ? globalThis : this, function (root) {
  'use strict';
  var PARTS = ['in', 'out', 'both'], MOVE_MIN = 3, MAX_SAMPLES = 4096;
  var states = new WeakMap();
  var REFUSALS = {
    not_found: 'The layer is no longer in the component.',
    unavailable: 'Layer bar edit refused: an edge cannot be moved exactly (an expression-driven or fractional offset).',
    invalid_request: 'Layer bar edit refused.',
  };

  function context(host) { var c = host && host.NemoNativeComponentContext; return c && typeof c.presented === 'function' ? c : null; }
  function held(host) { var c = context(host); return c ? c.presented(host) : null; }
  function session(host) { try { var project = host.NemoNativeObjectProject; return project ? project.desktop(host) : null; } catch (_) { return null; } }
  function toast(host, text) { try { if (typeof host.showToast === 'function') host.showToast(text); } catch (_) { /* presentation only */ } }
  function stateOf(host) { var st = states.get(host); if (!st) { st = { sel: [], anchor: null, drag: null }; states.set(host, st); } return st; }
  function rowsOf(host) { var c = context(host); return (c && c.presented(host) && c.layers(host)) || []; }
  function uids(host) { return rowsOf(host).map(function (row) { return row.layerUid; }); }
  // The selected bars still listed by the entered rows, in row order.
  function selection(host) {
    var st = stateOf(host), order = uids(host);
    st.sel = st.sel.filter(function (e) { return order.indexOf(e.layerUid) >= 0; }).sort(function (a, b) { return order.indexOf(a.layerUid) - order.indexOf(b.layerUid); });
    return st.sel;
  }
  function bars(host) {
    var out = {}, grid = host.document && host.document.getElementById('frame-grid');
    if (grid) Array.prototype.forEach.call(grid.querySelectorAll('.frow[data-layer-uid]'), function (row) {
      var bar = row.querySelector('.layer-inout-bar'); if (bar) out[row.dataset.layerUid] = bar;
    });
    return out;
  }
  function partOf(host, uid) { var e = selection(host).find(function (v) { return v.layerUid === uid; }); return e ? e.part : null; }
  function paintSelection(host) {
    var all = bars(host);
    Object.keys(all).forEach(function (uid) {
      var part = partOf(host, uid), bar = all[uid];
      bar.classList.toggle('sel', part === 'both');
      var left = bar.querySelector('.layer-inout-handle.left'), right = bar.querySelector('.layer-inout-handle.right');
      if (left) left.classList.toggle('sel', part === 'in' || part === 'both');
      if (right) right.classList.toggle('sel', part === 'out' || part === 'both');
    });
  }
  function setSelection(host, list) { stateOf(host).sel = list.map(function (e) { return { layerUid: e.layerUid, part: e.part }; }); paintSelection(host); }

  // The frame the gesture is judged at: the playhead, which the owner requires inside the component.
  function activeUid(host) { var rows = rowsOf(host), at = host.state && host.state.activeLayerIdx; return rows[at] ? rows[at].layerUid : null; }

  function preview(host, members, dx) {
    var fc = Number.isFinite(host.FC) && host.FC > 0 ? host.FC : 14, all = bars(host);
    members.forEach(function (m) {
      var bar = all[m.layerUid]; if (!bar) return;
      var left = m.left, width = m.width, d = dx * fc;
      if (m.part === 'in') { left = m.left + d; width = Math.max(fc, m.width - d); } else if (m.part === 'out') width = Math.max(fc, m.width + d); else left = m.left + d;
      bar.style.left = left + 'px'; bar.style.width = width + 'px';
    });
  }
  function restore(host, members) {
    var all = bars(host);
    members.forEach(function (m) { var bar = all[m.layerUid]; if (bar) { bar.style.left = m.left + 'px'; bar.style.width = m.width + 'px'; } });
  }

  function down(host, bar, uid, part, event) {
    if (!event || event.button > 0) return;
    event.stopPropagation(); event.preventDefault();
    var here = held(host), st = stateOf(host), order = uids(host), at = order.indexOf(uid);
    if (!here || at < 0) return;
    if (event.metaKey || event.ctrlKey) { // toggle one bar, no drag
      var sel = selection(host).slice(), found = sel.findIndex(function (e) { return e.layerUid === uid; });
      if (found >= 0) sel.splice(found, 1); else sel.push({ layerUid: uid, part: 'both' });
      st.anchor = uid; setSelection(host, sel); return;
    }
    if (event.shiftKey) { // contiguous range from the anchor, no drag
      var from = st.anchor && order.indexOf(st.anchor) >= 0 ? order.indexOf(st.anchor) : at, lo = Math.min(from, at), hi = Math.max(from, at);
      setSelection(host, order.slice(lo, hi + 1).map(function (u) { return { layerUid: u, part: 'both' }; })); return;
    }
    var current = selection(host), grouped = current.length > 1 && current.some(function (e) { return e.layerUid === uid; });
    var picked = grouped ? current.slice() : [{ layerUid: uid, part: part }];
    if (!grouped) setSelection(host, picked);
    st.anchor = uid;
    var all = bars(host);
    var members = picked.map(function (e) {
      var bar2 = all[e.layerUid];
      return { layerUid: e.layerUid, part: e.part, left: bar2 ? parseFloat(bar2.style.left) || 0 : 0, width: bar2 ? parseFloat(bar2.style.width) || 0 : 0 };
    });
    var fc = Number.isFinite(host.FC) && host.FC > 0 ? host.FC : 14;
    var drag = { uid: uid, part: part, startX: event.clientX, samples: [], members: members, fc: fc, group: grouped, picked: picked, alt: !!event.altKey, here: here.contextId };
    st.drag = drag;
    var doc = host.document;
    var onMove = function (e) {
      if (st.drag !== drag || !Number.isFinite(e.clientX)) return;
      var dx = Math.round((e.clientX - drag.startX) / drag.fc);
      if (!Number.isSafeInteger(dx) || drag.samples.length >= MAX_SAMPLES) return;
      drag.samples.push(dx); // ordered, never coalesced: the owner replays them
      preview(host, drag.members, dx);
    };
    var onUp = function (e) {
      doc.removeEventListener('mousemove', onMove, true); doc.removeEventListener('mouseup', onUp, true);
      if (st.drag !== drag) return;
      st.drag = null; restore(host, drag.members);
      finish(host, drag, e);
    };
    doc.addEventListener('mousemove', onMove, true); doc.addEventListener('mouseup', onUp, true);
  }

  // The drop: one rangeDrag request when the bar moved (or, in Motion, was clicked); a bare click only narrows the UI selection.
  function finish(host, drag, event) {
    var edit = host.NemoNativeComponentFrameEdit, here = held(host);
    if (!edit || !here || here.contextId !== drag.here) return Promise.resolve(null);
    var moved = Math.abs((event && Number.isFinite(event.clientX) ? event.clientX : drag.startX) - drag.startX) >= MOVE_MIN;
    var motion = host.state && host.state.appMode === 'motion', plain = motion === true && !moved;
    if (!drag.samples.length && !plain) {
      if (drag.group) setSelection(host, [{ layerUid: drag.uid, part: drag.part }]);
      return Promise.resolve(null);
    }
    var active = activeUid(host);
    if (!active) { toast(host, 'Native layer range editing is unavailable.'); return Promise.resolve(null); }
    var gesture = { command: 'object.layer.range.drag', contextId: here.contextId, layerUid: drag.uid, activeLayerUid: active, part: drag.part,
      barSelection: drag.picked.map(function (e) { return { layerUid: e.layerUid, part: e.part }; }), samples: drag.samples.slice(), plainClick: plain,
      alt: drag.alt || !!(event && event.altKey), frame: host.state.currentFrame, selectedKeys: [] };
    return Promise.resolve(edit.run({ kind: 'rangeDrag', gesture: gesture, capture: {} })).then(function (result) {
      if (result && (result.status === 'applied-presented' || result.status === 'unchanged')) {
        if (plain) setSelection(host, [{ layerUid: drag.uid, part: drag.part }]);
        else paintSelection(host);
      } else toast(host, 'Native layer range editing is unavailable.');
      return result;
    }).catch(function () { toast(host, 'Native layer range editing is unavailable.'); return null; });
  }

  // The bar menu: the scene's menu over the entered bars; each batch write is one `layer.range.arrange` with the held contextId.
  function arrange(host, here, action, list, step) {
    var s = session(host), Command = host.NemoNativeObjectRigCommand, Contract = host.NemoNativeObjectRigContract;
    if (!s || typeof s.runComponentCommand !== 'function' || !Command || !Contract) { toast(host, 'Native layer range editing is unavailable.'); return Promise.resolve(false); }
    var origin = s.info(), c = context(host);
    var tx = Command.create(Object.freeze({ info: s.info, isActive: s.isActive, requestId: s.requestId, runResourceCommand: s.runComponentCommand.bind(s) }), 'component-range', Contract, 4096);
    return tx.execute(Contract.RANGE_ARRANGE, function (pin) {
      if (!origin || pin.contentRevision !== origin.contentRevision || !c || c.contextId(host) !== here.contextId || c.projectionReady(host, pin) !== true) {
        throw Command.fault('stale_origin', 'The presented component is not the one the menu was built against', { reason: 'origin_changed' });
      }
      var payload = { command: 'object.layer.range.arrange', contextId: here.contextId, layerUids: list.slice(), action: action,
        frame: Math.max(0, Math.floor(Number(host.state && host.state.currentFrame) || 0)) };
      if (action === 'stagger') payload.step = step;
      return payload;
    }).then(function () { return true; }, function (error) {
      toast(host, error && error.code === 'stale_origin' ? 'Native layer range editing is unavailable or stale.' : REFUSALS[error && error.code] || 'Native layer bar command was refused.');
      return false;
    });
  }
  function menu(host, uid, event) {
    var menus = host.NemoNativeLayerRangeMenu, here = held(host);
    if (!menus || !here || uids(host).indexOf(uid) < 0) return false;
    event.preventDefault(); event.stopPropagation();
    if (!partOf(host, uid)) setSelection(host, [{ layerUid: uid, part: 'both' }]);
    var all = bars(host), rows = uids(host).map(function (u) {
      return { layerUid: u, hidden: false, rawIn: all[u] ? Number(all[u].dataset.rawIn) || 0 : 0 };
    });
    var view = { info: null, li: uids(host).indexOf(uid), uid: uid, selection: selection(host).slice(), rows: rows,
      setSelection: function (list) { setSelection(host, list); },
      send: function (action, list, step) { return arrange(host, here, action, list, step); } };
    return menus.open(host, event, view);
  }

  // Makes one painted bar of the entered grid interactive: edge handles, body drag, selection and the context menu. Idempotent.
  function bind(host, bar, uid, range) {
    if (!bar || !uid) return false;
    if (range && Number.isFinite(range.rawInPoint)) bar.dataset.rawIn = String(range.rawInPoint);
    bar.setAttribute('aria-disabled', 'false');
    if (bar.nativeRangeBound === true) { paintSelection(host); return true; }
    bar.nativeRangeBound = true;
    var doc = host.document;
    [['left', 'in'], ['right', 'out']].forEach(function (pair) {
      var handle = bar.querySelector('.layer-inout-handle.' + pair[0]);
      if (!handle) { handle = doc.createElement('div'); handle.className = 'layer-inout-handle ' + pair[0]; bar.appendChild(handle); }
      handle.addEventListener('mousedown', function (e) { down(host, bar, uid, pair[1], e); });
    });
    bar.addEventListener('mousedown', function (e) { if (e.target === bar) down(host, bar, uid, 'both', e); });
    bar.addEventListener('contextmenu', function (e) { menu(host, uid, e); });
    paintSelection(host);
    return true;
  }
  return Object.freeze({ bind: bind, selection: selection, setSelection: setSelection, finish: finish, PARTS: PARTS });
}));
