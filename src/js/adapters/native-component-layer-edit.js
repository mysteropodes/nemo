/* #2256 A-comp-edit-timeline part 2: the layer-level actions of the dense editor INSIDE an entered native component.
 * Baseline: enterSymbol swaps state.layers for the component's layers, so the layer list and its toolbar work unchanged on them
 * (1065e9d8:src/js/app.js:4130; timeline.js renderLayerList rows with the eye and padlock, the New / Delete / Duplicate buttons, the row
 * menu and splitLayerAtPlayhead). Native: each action is ONE closed request with the held component's id as `contextId`, sent through the
 * component command route (session.runComponentCommand), so one gesture is one revision and one history entry; the rows then refresh
 * from the owner (NemoNativeComponentContext). No document, no Paper object, no fallback writer. A layer the entered view no longer lists
 * is refused as stale before anything is sent. Part 3 adds the drag-to-reorder of the rows (one layer.reorder), Split into layers (layer.split)
 * and the shy mark (layer.policy.edit flag shy) with the baseline's hint toast. Rename is the layer list's inline rename (lane C), not offered here. */
(function (root, factory) {
  var api = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentLayerEdit = api;
}(typeof globalThis === 'object' ? globalThis : this, function (root) {
  'use strict';
  var P = 'command.document.object.';
  var CODES = Object.freeze({ session: 'layer_edit_session_unavailable', stale: 'layer_edit_stale_intent', invalid: 'layer_edit_invalid_intent', last: 'last_layer' });
  var LOCAL = Object.freeze({
    'layer.create': { command: 'object.layer.create', members: ['name'], result: ['layerUid'] },
    'layer.visibility.set': { command: 'object.layer.visibility.set', members: ['layerUid', 'visible'], result: [] },
    'layer.policy.edit': { command: 'object.layer.policy.edit', members: ['layerUid', 'flag', 'value'], result: [] },
  });
  var EDIT = Object.freeze(['layer.delete', 'layer.duplicate', 'layer.reorder', 'layer.split']);
  var Command = { create: function (s, l, c) { return root.NemoNativeObjectVectorCommand.create(s, l, c); }, fault: function (a, b, c) { return root.NemoNativeObjectVectorCommand.fault(a, b, c); } };
  function has(o, k) { return Object.prototype.hasOwnProperty.call(o, k); }
  function suffix(op) { return typeof op === 'string' && op.indexOf(P) === 0 ? op.slice(P.length) : null; }
  function fail(message) { throw new TypeError(message); }
  function plain(v, keys) { if (!v || typeof v !== 'object' || Array.isArray(v) || Object.keys(v).length !== keys.length || keys.some(function (k) { return !has(v, k); })) fail('invalid layer edit value'); }

  // One contract over the operations of this adapter: lifecycle ops and the cut keep their own contracts, the other three are closed here.
  function route(op) {
    var name = suffix(op);
    if (EDIT.indexOf(name) >= 0) return root.NemoNativeObjectEditContract;
    if (name === 'layer.cut') return root.NemoNativeLayerPlayheadCutContract;
    return null;
  }
  var contract = Object.freeze({
    payload: function (op, body) {
      var other = route(op), entry = LOCAL[suffix(op)];
      if (other) return other.payload(op, body);
      if (!entry) fail('layer edit operation is not declared');
      var keys = ['command', 'contextId'].concat(entry.members);
      plain(body, keys);
      if (body.command !== entry.command || typeof body.contextId !== 'string' || !body.contextId) fail('invalid layer edit command');
      if (has(body, 'name') && typeof body.name !== 'string') fail('invalid layer name');
      if (has(body, 'layerUid') && (typeof body.layerUid !== 'string' || !body.layerUid)) fail('invalid layer');
      if (has(body, 'visible') && typeof body.visible !== 'boolean') fail('invalid visibility');
      if (has(body, 'flag') && (['locked', 'shy'].indexOf(body.flag) < 0 || typeof body.value !== 'boolean')) fail('invalid policy flag');
    },
    result: function (op, value) {
      var other = route(op), entry = LOCAL[suffix(op)];
      if (other) return other.result(op, value);
      if (!entry) fail('layer edit operation is not declared');
      plain(value, ['applied', 'historyEntriesAdded'].concat(entry.result));
      if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail('invalid layer edit disposition');
      if (entry.result.length && (typeof value.layerUid !== 'string' || !value.layerUid)) fail('invalid created layer');
    },
    correlate: function (request, response) {
      var other = route(request.operation);
      if (other) return other.correlate(request, response);
      if (response.instanceId !== request.instanceId || response.documentId !== request.documentId) fail('layer edit response owner mismatch');
      if (response.ok && response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) fail('layer edit receipt revision mismatch');
    },
    bytes: function (text) { return typeof TextEncoder === 'function' ? new TextEncoder().encode(text).length : text.length; },
    requestByteLimit: function () { return 4096; },
  });

  function context(host) { var c = host && host.NemoNativeComponentContext; return c && typeof c.presented === 'function' ? c : null; }
  function held(host) { var c = context(host); return c ? c.presented(host) : null; }
  function session(host) { try { var project = host.NemoNativeObjectProject; return project ? project.desktop(host) : null; } catch (_) { return null; } }
  function toast(host, text) { try { if (typeof host.showToast === 'function') host.showToast(text); } catch (_) { /* presentation only */ } }
  function word(host, key, fallback) { try { var v = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return typeof v === 'string' && v && v !== key ? v : fallback; } catch (_) { return fallback; } }
  function commands(s) {
    if (typeof s.runComponentCommand !== 'function') return null;
    return Command.create(Object.freeze({ info: s.info, isActive: s.isActive, requestId: s.requestId, runResourceCommand: s.runComponentCommand.bind(s) }), 'component-layer', contract);
  }
  function refusal(error) {
    var code = error && error.code;
    if (code === 'stale_origin') return { ok: false, code: CODES.stale };
    if (typeof code === 'string' && !code.endsWith('_command_indeterminate') && code !== 'uncorrelated') return { ok: false, code: code };
    return { ok: false, code: CODES.session };
  }

  // One request over the component route. `build(rows)` returns the payload members for the rows read now; nothing is sent when the held
  // component, its revision or its rows are not the ones the gesture started against.
  async function send(host, name, build) {
    var c = context(host), here = c && c.presented(host), s = session(host), rows = here && c.layers(host), tx = s && commands(s), origin = s && s.info();
    if (!here || !s || !tx || !rows || !origin) return { ok: false, code: CODES.session };
    var members = build(rows);
    if (!members) return { ok: false, code: CODES.invalid };
    try {
      var receipt = await tx.execute(P + name, function (pin) {
        if (pin.contentRevision !== origin.contentRevision || c.contextId(host) !== here.contextId || c.projectionReady(host, pin) !== true) {
          throw Command.fault('stale_origin', 'The presented component is not the one the edit was prepared against', { reason: 'origin_changed' });
        }
        return Object.assign({ command: members.command, contextId: here.contextId }, members.body);
      });
      return { ok: true, applied: receipt.applied === true, layerUid: receipt.layerUid };
    } catch (error) { return refusal(error); }
  }
  function indexOf(host, uid) { var rows = (context(host) && context(host).layers(host)) || []; return rows.findIndex(function (row) { return row.layerUid === uid; }); }
  // The new or duplicated layer becomes the active (and only selected) one, as the baseline's setActiveLayer does.
  function select(host, uid) { var at = indexOf(host, uid), c = context(host); if (at >= 0 && c) c.selectLayer(host, at); }

  function nextName(rows) {
    var names = new Set(rows.map(function (row) { return row.name; })), n = 1;
    while (names.has('Layer ' + n)) n += 1;
    return 'Layer ' + n;
  }
  function create(host) {
    return send(host, 'layer.create', function (rows) { return { command: 'object.layer.create', body: { name: nextName(rows) } }; })
      .then(function (r) { if (r.ok && r.layerUid) select(host, r.layerUid); return r; });
  }
  function duplicate(host, uid) {
    return send(host, 'layer.duplicate', function (rows) { return rows.some(function (row) { return row.layerUid === uid; }) ? { command: 'object.layer.duplicate', body: { layerUid: uid } } : null; })
      .then(function (r) { if (r.ok && r.layerUid) select(host, r.layerUid); return r; });
  }
  // The layers the toolbar and the row menu act on: the marked rows (or the active one), by uid.
  function picked(host, rows) {
    var marked = (Array.isArray(host._layerSel) ? host._layerSel : []).filter(function (i, k, all) { return Number.isSafeInteger(i) && i >= 0 && i < rows.length && all.indexOf(i) === k; });
    var act = host.state && Number.isSafeInteger(host.state.activeLayerIdx) && host.state.activeLayerIdx < rows.length ? host.state.activeLayerIdx : -1;
    var at = marked.length ? marked : act >= 0 ? [act] : [];
    return at.length ? { active: rows[act >= 0 ? act : at[0]].layerUid, selected: at.slice().sort(function (a, b) { return a - b; }).map(function (i) { return rows[i].layerUid; }) } : null;
  }
  function remove(host) {
    var c = context(host), all = (c && c.layers(host)) || [];
    if (all.length <= 1) return Promise.resolve({ ok: false, code: CODES.last }); // a component keeps at least one layer
    return send(host, 'layer.delete', function (rows) {
      var sel = picked(host, rows);
      return sel ? { command: 'object.layer.delete', body: { selection: { active: sel.active, selected: sel.selected } } } : null;
    });
  }
  function flag(host, uid, name, value) {
    if (name === 'visible') return send(host, 'layer.visibility.set', function (rows) { return rows.some(function (row) { return row.layerUid === uid; }) ? { command: 'object.layer.visibility.set', body: { layerUid: uid, visible: value } } : null; });
    return send(host, 'layer.policy.edit', function (rows) { return rows.some(function (row) { return row.layerUid === uid; }) ? { command: 'object.layer.policy.edit', body: { layerUid: uid, flag: name, value: value } } : null; });
  }
  // splitLayerAtPlayhead(index): the active layer when no index is given; a playhead outside the layer is the baseline notice.
  function cut(host, index) {
    var frame = host.state && host.state.currentFrame, at = index == null ? host.state && host.state.activeLayerIdx : index;
    return send(host, 'layer.cut', function (rows) {
      return Number.isSafeInteger(at) && rows[at] && Number.isSafeInteger(frame) && frame >= 0 ? { command: 'object.layer.cut', body: { layerUid: rows[at].layerUid, frame: frame } } : null;
    }).then(function (r) {
      if (r.ok && r.applied && r.layerUid) { select(host, r.layerUid); return true; }
      if (r.ok) toast(host, word(host, 'toastPlayheadInsideLayerToCut', 'The playhead must be inside the layer to cut it'));
      return false;
    });
  }

  // "Split into layers": one layer per shape of the playhead frame, named with the locale's shape and stroke words (baseline splitLayerIntoElements).
  function split(host, uid) {
    var frame = host.state && host.state.currentFrame;
    return send(host, 'layer.split', function (rows) {
      return Number.isSafeInteger(frame) && frame >= 0 && rows.some(function (row) { return row.layerUid === uid; })
        ? { command: 'object.layer.split', body: { layerUid: uid, frame: frame, labels: { shape: word(host, 'autoNameShape', 'Shape'), stroke: word(host, 'autoNameStroke', 'Stroke') } } } : null;
    }).then(function (r) { if (r.ok && r.applied) select(host, uid); return r; });
  }
  // The shy mark (baseline toggleLayerShy): marking a layer while the shy switch is off turns the switch on and says so.
  function shy(host, uid) {
    var rows = (context(host) && context(host).layers(host)) || [], row = rows.find(function (r) { return r.layerUid === uid; });
    if (!row) return Promise.resolve({ ok: false, code: CODES.stale });
    var marking = row.shy !== true, before = host.state && host.state.shyEnabled === true;
    return flag(host, uid, 'shy', marking).then(function (r) {
      if (r.ok && marking && !before && host.state) {
        host.state.shyEnabled = true;
        try { var button = host.document.getElementById('btn-shy'); if (button) button.classList.add('active'); } catch (_) { /* presentation only */ }
        toast(host, word(host, 'toastLayerMarkedShyHint', 'Layer marked « shy » — turn on the switch to hide it'));
      }
      return r;
    });
  }
  // The drop of a reorder gesture: ONE layer.reorder with the held contextId (the dragged layers in list order, the gap, the selection after).
  function reorder(host, order, gap, selection) {
    return send(host, 'layer.reorder', function (rows) {
      var ids = order.concat([selection.active], selection.selected);
      if (!Number.isSafeInteger(gap) || gap < 0 || gap > rows.length || ids.some(function (id) { return !rows.some(function (row) { return row.layerUid === id; }); })) return null;
      return { command: 'object.layer.reorder', body: { layerUids: order.slice(), gap: gap, selection: { active: selection.active, selected: selection.selected.slice() } } };
    }).then(function (r) { if (r.ok && r.applied) select(host, selection.active); return r; });
  }
  // Arms the shared drag of the layer list (NemoNativeLayerRowDrag) on one entered row: the gesture is bound to the pin and the rows it started on.
  function armReorder(host, uid, event) {
    var drag = host.NemoNativeLayerRowDrag, c = context(host), s = session(host), list = host.document.getElementById('layer-list');
    if (!drag || !c || !s || !list || !held(host) || !s.isActive()) return false;
    var asLayers = function () { return (c.layers(host) || []).map(function (row) { return { layerUid: row.layerUid, name: row.name, kind: 'drawing', parentLayerUid: row.parentLayerUid || null }; }); };
    var pin = function () { return Object.assign({}, s.info(), { layers: asLayers() }); };
    var rows = c.layers(host) || [], sel = picked(host, rows), now = pin();
    return drag.arm(host, {
      writes: { reorder: function (order, gap, selection) { return reorder(host, order, gap, selection); } },
      session: { isActive: function () { return s.isActive(); }, info: pin }, info: now, uid: uid,
      selection: sel, list: list, intoFolder: null, stale: 'The layers changed - the move was cancelled.',
      toast: function (t) { toast(host, t); }, refused: function (r) { report(host, r); },
    }, event);
  }

  var WORDS = { hsCannotDeleteLastLayer: 'The last layer cannot be deleted', ctxDuplicateLayer: 'Duplicate the layer', ctxDeleteLayer: 'Delete the layer',
    ctxCutAtPlayhead: 'Cut at the playhead  (⌘⇧D)', ctxSplitIntoLayers: 'Split into layers', ctxMarkAsShy: 'Mark as « shy »', ctxRemoveShyMark: 'Remove the « shy » mark' };
  function report(host, result) {
    if (!result || result.ok) return result;
    toast(host, result.code === CODES.last ? word(host, 'hsCannotDeleteLastLayer', WORDS.hsCannotDeleteLastLayer) :
      result.code === CODES.stale ? 'The layers changed - nothing was applied.' : 'The layer change was not applied.');
    return result;
  }
  // The toolbar New / Delete / Duplicate buttons: true when this adapter owns the gesture (a component is entered).
  function toolbar(host, kind) {
    if (!held(host)) return false;
    var rows = context(host).layers(host) || [], sel = picked(host, rows);
    if (kind === 'delete') Promise.resolve(remove(host)).then(function (r) { report(host, r); });
    else if (kind === 'duplicate') { if (sel) Promise.resolve(duplicate(host, sel.active)).then(function (r) { report(host, r); }); }
    else if (kind === 'create') Promise.resolve(create(host)).then(function (r) { report(host, r); });
    return true;
  }
  function icon(host, title, html, on, run) {
    var el = host.document.createElement('div');
    el.className = 'lico' + (on ? '' : ' off'); el.title = title; el.innerHTML = html;
    el.addEventListener('click', function (e) { e.stopPropagation(); Promise.resolve(run()).then(function (r) { report(host, r); }); });
    return el;
  }
  // The eye, the padlock and the row menu of one entered row (the grid builds the row; the baseline row carries the same controls).
  function decorate(host, row, layer, index) {
    var off = host.ICO_EYE_CLOSED || '', on = host.ICO_EYE || '', lockOn = host.ICO_LOCK || '', lockOff = host.ICO_UNLOCK || '';
    row.appendChild(icon(host, 'Show / hide layer', layer.visible !== false ? on : off, layer.visible !== false, function () { return flag(host, layer.layerUid, 'visible', layer.visible === false); }));
    row.appendChild(icon(host, 'Lock / unlock layer', layer.locked === true ? lockOn : lockOff, layer.locked === true, function () { return flag(host, layer.layerUid, 'locked', layer.locked !== true); }));
    row.addEventListener('contextmenu', function (e) {
      e.preventDefault();
      if (!Array.isArray(host._layerSel) || host._layerSel.indexOf(index) < 0) { var c = context(host); if (c) c.selectLayer(host, index); }
      if (typeof host.showContextMenu !== 'function') return;
      host.showContextMenu(e.clientX, e.clientY, [
        { label: word(host, 'ctxDuplicateLayer', WORDS.ctxDuplicateLayer), action: function () { Promise.resolve(duplicate(host, layer.layerUid)).then(function (r) { report(host, r); }); } },
        { label: word(host, 'ctxDeleteLayer', WORDS.ctxDeleteLayer), action: function () { Promise.resolve(remove(host)).then(function (r) { report(host, r); }); } },
        { sep: true },
        { label: word(host, 'ctxSplitIntoLayers', WORDS.ctxSplitIntoLayers), action: function () { Promise.resolve(split(host, layer.layerUid)).then(function (r) { report(host, r); }); } },
        { label: word(host, 'ctxCutAtPlayhead', WORDS.ctxCutAtPlayhead), action: function () { cut(host, index); } },
        { label: layer.shy === true ? word(host, 'ctxRemoveShyMark', WORDS.ctxRemoveShyMark) : word(host, 'ctxMarkAsShy', WORDS.ctxMarkAsShy),
          action: function () { Promise.resolve(shy(host, layer.layerUid)).then(function (r) { report(host, r); }); } },
      ]);
    });
    // A vertical press-and-drag of the row reorders the layers (the shared gesture decides past 4px; a click stays a click).
    var begin = function (e) { if (e.button === 0 && !(e.target && e.target.closest && e.target.closest('.lico'))) armReorder(host, layer.layerUid, e); };
    row.addEventListener('mousedown', begin); row.addEventListener('pointerdown', begin);
  }
  return Object.freeze({ held: held, create: create, duplicate: duplicate, remove: remove, flag: flag, cut: cut, split: split, shy: shy, reorder: reorder, armReorder: armReorder, toolbar: toolbar, decorate: decorate, CODES: CODES });
}));
