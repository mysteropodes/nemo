/* #2256 A-comp-edit-timeline: the dense editor's frame and key actions INSIDE an entered native component.
 * Baseline: enterSymbol swaps state.layers for the component's layers, so insertFrame / insertKeyframe / convertToKeyframes /
 * insertBlankKeyframe / duplicateKeyframe / extendExposure / clearKeyframe / deleteSelectedFrames / removeTweenSpan / removeFrame(Span) /
 * moveKeyframe / moveFrames / repeatSelection / copyFrames / cutFrames / pasteFrames / duplicateSelectedFrames
 * all edit the component unchanged (1065e9d8:src/js/app.js:4130 enterSymbol; the editors read state.layers / state.totalFrames).
 * Native: the intent planner of native-frame-edit-intent.js (the same action grammar, selection rules and payloads as the scene) is run
 * against the component's own layer rows and frame count, the held context id is stamped on the payload, and ONE closed request goes
 * through the component command route (session.runComponentCommand) to the declared native operation: one revision and one history
 * entry per gesture. No document, no Paper object, no fallback writer. The layer-bar drag has no component route yet: it answers a typed
 * unavailable code and never reaches a scene editor. The frame clipboard is the one of the cell selection (`_sel.clipboard`), so a packet
 * copied in the scene pastes into the component by relative cell, and back, as the baseline does. */
(function (root, factory) {
  var api = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentFrameEdit = api;
}(typeof globalThis === 'object' ? globalThis : this, function (root) {
  'use strict';
  // The planner, the contract and the command round trip are read from the page at call time (adapter modules do not import each other).
  var Intent = { get CODES() { return root.NemoNativeFrameEditIntent.CODES; }, action: function (v) { return root.NemoNativeFrameEditIntent.action(v); },
    readState: function (p, i) { return root.NemoNativeFrameEditIntent.readState(p, i); }, plan: function (a, b, c, d) { return root.NemoNativeFrameEditIntent.plan(a, b, c, d); } };
  var Contract = { payload: function (o, v) { return root.NemoNativeFrameEditContract.payload(o, v); }, valid: function (o, v) { return root.NemoNativeFrameEditContract.valid(o, v); },
    result: function (o, v) { return root.NemoNativeFrameEditContract.result(o, v); }, correlate: function (a, b) { return root.NemoNativeFrameEditContract.correlate(a, b); },
    requestByteLimit: function (o) { return root.NemoNativeFrameEditContract.requestByteLimit(o); } };
  var Command = { create: function (s, l, c) { return root.NemoNativeObjectVectorCommand.create(s, l, c); }, fault: function (a, b, c) { return root.NemoNativeObjectVectorCommand.fault(a, b, c); } };
  var LABEL = 'component-frame';
  // The kinds that have a component route: the scene kinds whose native operation accepts a contextId.
  var ROUTED = Object.freeze(['insertFrame', 'insertKey', 'insertKeyAt', 'convertToKeyframes', 'insertBlankKey', 'duplicateKey', 'extendExposure',
    'clearKey', 'deleteFrames', 'removeTween', 'removeFrame', 'removeFrameSpan', 'moveKey', 'moveFrames', 'repeatSelection', 'duplicateSelection',
    'copyFrames', 'cutFrames', 'pasteFrames']);
  var CODES = Object.freeze({ unavailable: 'frame_edit_component_unavailable', session: 'frame_edit_session_unavailable', invalid: 'frame_edit_invalid_intent', stale: 'frame_edit_stale_intent' });
  var P = 'command.document.object.';
  var viewContract = null;

  function unavailable(code, reason) { return Object.freeze({ status: 'unavailable', code: code, reason: reason || null }); }
  function context(host) { var c = host && host.NemoNativeComponentContext; return c && typeof c.presented === 'function' ? c : null; }
  // The component the workspace holds, or null at the scene root.
  function held(host) { var c = context(host); return c ? c.presented(host) : null; }
  function session(host) { try { var project = host.NemoNativeObjectProject; return project ? project.desktop(host) : null; } catch (_) { return null; } }
  // The shared command round trip over the component route, with the frame-edit contract (its byte bound is the 4096 of the public transport).
  function contract() {
    if (!viewContract) {
      viewContract = Object.freeze({
        payload: Contract.payload, result: Contract.result, correlate: Contract.correlate,
        bytes: function (text) { return typeof TextEncoder === 'function' ? new TextEncoder().encode(text).length : text.length; },
        requestByteLimit: Contract.requestByteLimit,
      });
    }
    return viewContract;
  }
  function commands(s) {
    if (typeof s.runComponentCommand !== 'function') return null;
    return Command.create(Object.freeze({ info: s.info, isActive: s.isActive, requestId: s.requestId, runResourceCommand: s.runComponentCommand.bind(s) }), LABEL, contract());
  }
  function failed(error) {
    var code = error && error.code, reason = (error && (error.reason || error.message)) || null;
    if (typeof code !== 'string') return unavailable(CODES.session, reason);
    if (code === 'stale_origin') return unavailable(CODES.stale, reason);
    if (code === 'invalid_request') return unavailable('frame_edit_request_invalid', reason);
    if (code === 'unavailable' || code === 'request_too_large') return unavailable(CODES.session, reason);
    if (code === 'uncorrelated' || code.endsWith('_command_indeterminate')) return Object.freeze({ status: 'unconfirmed', requestId: error.requestId });
    return Object.freeze({ status: 'refused', code: code, reason: error.reason || null });
  }
  // The editor state, read against the component: rows are its layers, the length is its own frame count.
  function readState(host, rows, total) {
    var s = host.state || {};
    return Intent.readState({
      frame: function () { return s.currentFrame; }, activeLayer: function () { return s.activeLayerIdx; },
      layerSelection: function () { return host._layerSel; }, cellSelection: function () { return host._sel && host._sel.frames ? host._sel.frames : []; },
    }, { layers: rows, totalFrames: total });
  }
  // Where the baseline left the playhead: F5 and Duplicate Key step to the new frame; a removal only clamps (the context refresh already did).
  function hint(operation, frame) { return operation === P + 'frames.insert' || operation === P + 'frames.duplicate' ? frame + 1 : null; }

  // The cell selection of the grid (the legacy selection helpers paint the .sel class on the entered cells).
  function selectCells(host, cells) {
    if (typeof host.selClear === 'function') host.selClear(); else if (host._sel) host._sel.frames = [];
    cells.forEach(function (cell) { if (typeof host.selAdd === 'function') host.selAdd(cell.layer, cell.frame); else host._sel.frames.push(cell); });
    if (typeof host.selApplyCSS === 'function') host.selApplyCSS();
  }
  // The clipboard of the cell selection: a packet {instanceId, documentId, cells} the owner returned (Copy / Cut) and Paste consumes.
  function readClipboard(host) {
    var value = host._sel && host._sel.clipboard;
    return value && !Array.isArray(value) && Array.isArray(value.cells) ? { instanceId: value.instanceId, documentId: value.documentId, cells: JSON.parse(JSON.stringify(value.cells)) } : null;
  }
  function acceptClipboard(host, packet, kind) {
    if (!host._sel || !packet) return;
    var value = JSON.parse(JSON.stringify(packet));
    Object.defineProperty(value, 'length', { value: value.cells.length, enumerable: false });
    host._sel.clipboard = value; host._sel.clipOp = kind === 'copyFrames' ? 'copy' : 'cut'; host._lastClipKind = 'frames';
    if (kind === 'cutFrames') selectCells(host, []);
  }
  // What the baseline leaves selected after a Move (the cells where they landed) and a Paste (the pasted block, inside the component).
  function selectMoved(host, rows, total, cells) {
    var out = cells.map(function (cell) { return { layer: rows.findIndex(function (row) { return row.layerUid === cell.layerUid; }), frame: cell.frame }; });
    if (out.every(function (cell) { return cell.layer >= 0 && cell.frame < total; })) selectCells(host, out);
  }
  function selectPasted(host, rows, total, payload) {
    var anchor = rows.findIndex(function (row) { return row.layerUid === payload.layerUid; });
    if (anchor < 0) return;
    selectCells(host, payload.cells.map(function (cell) { return { layer: anchor + cell.relLayer, frame: payload.frame + cell.relFrame }; })
      .filter(function (cell) { return cell.layer < rows.length && cell.frame < total; }));
  }

  async function run(host, value) {
    var c = context(host), here = c ? c.presented(host) : null;
    if (!here) return null; // not inside a component: the scene route decides
    var intent = Intent.action(value);
    if (!intent) return unavailable(CODES.invalid);
    if (ROUTED.indexOf(intent.kind) < 0) return unavailable(CODES.unavailable, intent.kind);
    var s = session(host), rows = c.layers(host), send = s && commands(s);
    if (!s || !send || !rows) return unavailable(CODES.session);
    var state = readState(host, rows, here.totalFrames);
    if (!state) return unavailable(CODES.invalid);
    var origin = s.info();
    if (!origin) return unavailable(CODES.session);
    // The planner judges a paste against the owner's identity (a packet is the document's, whichever context it was copied in).
    var info = { layers: rows, totalFrames: here.totalFrames, instanceId: origin.instanceId, documentId: origin.documentId };
    var clipboard = intent.kind === 'pasteFrames' ? readClipboard(host) : null, before = host._sel && host._sel.clipboard;
    var step = Intent.plan(intent, state, info, clipboard);
    if (step.code) return unavailable(step.code);
    var payload = Object.assign({}, step.payload, { contextId: here.contextId });
    if (!Contract.valid(step.op, payload)) return unavailable('frame_edit_request_invalid');
    var frame = state.frame;
    try {
      var receipt = await send.execute(step.op, function (pin) {
        // The component, its revision and its rows are the ones the intent was planned against, or nothing is sent.
        if (pin.contentRevision !== origin.contentRevision || c.contextId(host) !== here.contextId || c.projectionReady(host, pin) !== true ||
            (intent.kind === 'pasteFrames' && (!host._sel || host._sel.clipboard !== before))) {
          throw Command.fault('stale_origin', 'The presented component is not the one the edit was prepared against', { reason: 'origin_changed' });
        }
        return payload;
      });
      var out = { status: receipt.applied ? 'applied-presented' : 'unchanged', applied: receipt.applied === true };
      var after = held(host), now = after ? c.layers(host) : null;
      if (after && now) {
        if (intent.kind === 'copyFrames' || intent.kind === 'cutFrames') { if (receipt.clipboard) acceptClipboard(host, receipt.clipboard, intent.kind); }
        else if (intent.kind === 'pasteFrames') selectPasted(host, now, after.totalFrames, payload);
        else if (intent.kind === 'moveFrames' && receipt.selection) selectMoved(host, now, after.totalFrames, receipt.selection);
        else if (intent.kind === 'repeatSelection') { out.cycledLayers = receipt.cycledLayers; out.skippedComponentLayers = receipt.skippedComponentLayers; }
      }
      var next = receipt.applied ? hint(step.op, frame) : null;
      if (next !== null && after && next < after.totalFrames && typeof host.goToFrame === 'function') {
        try { await host.goToFrame(next); out.playhead = next; } catch (_) { /* the edit is published; only the playhead move failed */ }
      }
      return Object.freeze(out);
    } catch (error) { return failed(error); }
  }
  return Object.freeze({ run: run, held: held, ROUTED: ROUTED, CODES: CODES });
}));
