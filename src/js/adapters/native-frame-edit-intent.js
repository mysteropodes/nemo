/* L05-K UI intent -> native frame-edit command. The dense editor's F5/F6/F7, Clear Keyframe, Delete Frames, Duplicate Key,
 * Extend Exposure, Remove Frame(s), Remove Tween, key move, Duplicate/Repeat Selection and the clipboard actions are mapped to the
 * public native operations declared by native-frame-edit-contract.js. Native revision, history, receipts and presentation stay
 * authoritative: this adapter owns no document, no history and no fallback writer. An action whose operation the shared
 * application client does not declare, or that native has no operation for yet, reports a typed unavailable code and never
 * reaches insertFrame/insertKeyframe or any other legacy writer. */
(function (root, factory) {
  var contract = typeof module === 'object' && module.exports ? require('./native-frame-edit-contract.js') : root.NemoNativeFrameEditContract;
  var api = factory(contract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeFrameEditIntent = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Contract) {
  'use strict';
  if (!Contract || typeof Contract.example !== 'function') throw new Error('Native frame edit contract is unavailable');
  var P = 'command.document.object.';
  var PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var MAX_EXTEND = 999;
  // Stable unavailable codes. `declaration_missing` is the only one that clears when the shared declaration lands.
  var CODES = Object.freeze({
    retired: 'frame_edit_retired', invalid: 'frame_edit_invalid_intent', session: 'frame_edit_session_unavailable',
    selection: 'frame_edit_selection_required', declaration: 'frame_edit_declaration_missing',
    request: 'frame_edit_request_invalid', blankKey: 'frame_edit_blank_key_unavailable',
    packet: 'frame_edit_packet_unavailable', stale: 'frame_edit_stale_intent'
  });
  var PACKET = { copyFrames: 1, cutFrames: 1, pasteFrames: 1 };
  var FIELDS = {
    insertFrame: [], insertKey: [], insertKeyAt: ['layer', 'frame'], convertToKeyframes: [], insertBlankKey: [],
    duplicateKey: [], extendExposure: ['count'], clearKey: [], deleteFrames: [], removeTween: ['layer', 'from', 'to'],
    removeFrame: [], removeFrameSpan: [], moveKey: ['layer', 'from', 'to'], duplicateSelection: [], repeatSelection: ['times'],
    copyFrames: [], cutFrames: [], pasteFrames: []
  };
  var OPTIONAL = { extendExposure: ['count'] };

  function same(a, b) { return !!a && !!b && PIN.every(function (key) { return a[key] === b[key]; }); }
  function sameList(a, b) { return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every(function (v, i) { return v === b[i]; }); }
  function whole(value, min, max) { return Number.isSafeInteger(value) && value >= min && value <= max; }
  function unavailable(code) { return Object.freeze({ status: 'unavailable', code: code }); }
  function presented(session, info, frame) {
    var value = session.readPresentedObjectSession();
    return !!value && value.available === true && same(info, value) && same(info, value.presentation) &&
      value.presentation.owner === 'native' && value.presentation.status === 'presented' && value.presentation.frame === frame;
  }
  function action(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value) || !Object.prototype.hasOwnProperty.call(FIELDS, value.kind)) return null;
    var fields = FIELDS[value.kind], optional = OPTIONAL[value.kind] || [];
    var keys = Object.keys(value).filter(function (key) { return key !== 'kind'; });
    if (keys.some(function (key) { return fields.indexOf(key) < 0; }) ||
        fields.some(function (key) { return optional.indexOf(key) < 0 && !Object.prototype.hasOwnProperty.call(value, key); })) return null;
    return Object.freeze(Object.assign({}, value));
  }

  // The editor state read once per run, validated against the pinned info: indices into info.layers, frames inside the timeline.
  function readState(ports, info) {
    var frame = ports.frame(), active = ports.activeLayer(), layers = ports.layerSelection(), cells = ports.cellSelection();
    var count = info.layers.length;
    if (!whole(frame, 0, info.totalFrames - 1) || !whole(active, 0, count - 1) || !Array.isArray(layers) || !Array.isArray(cells)) return null;
    var seen = new Set();
    if (!layers.every(function (index) { var ok = whole(index, 0, count - 1) && !seen.has(index); seen.add(index); return ok; })) return null;
    seen = new Set();
    var ok = cells.every(function (cell) {
      var token = cell && cell.layer + ':' + cell.frame;
      var valid = !!cell && whole(cell.layer, 0, count - 1) && whole(cell.frame, 0, info.totalFrames - 1) && !seen.has(token);
      seen.add(token);
      return valid;
    });
    if (!ok) return null;
    return { frame: frame, active: active, layers: layers.slice(), cells: cells.map(function (cell) { return { layer: cell.layer, frame: cell.frame }; }) };
  }
  function bounds(cells) {
    var out = { minL: Infinity, maxL: -1, minF: Infinity, maxF: -1 };
    cells.forEach(function (cell) {
      out.minL = Math.min(out.minL, cell.layer); out.maxL = Math.max(out.maxL, cell.layer);
      out.minF = Math.min(out.minF, cell.frame); out.maxF = Math.max(out.maxF, cell.frame);
    });
    return out;
  }

  // intent + editor state -> { op, payload } | { code }. Pure: no port, no session.
  function plan(intent, state, info, clipboard) {
    function uid(index) { return info.layers[index].layerUid; }
    function selected(command, extra) {
      var payload = Object.assign({ command: command }, extra);
      if (state.layers.length) payload.layerUids = state.layers.map(uid);
      return payload;
    }
    function cellRefs() { return state.cells.map(function (cell) { return { layerUid: uid(cell.layer), frame: cell.frame }; }); }
    if (intent.kind === 'copyFrames' || intent.kind === 'cutFrames') {
      if (!state.cells.length) return { code: CODES.selection };
      var command = intent.kind === 'copyFrames' ? 'object.frames.clipboard.copy' : 'object.frames.clipboard.cut';
      return { op: 'command.document.' + command, payload: { command: command, selection: cellRefs() } };
    }
    if (intent.kind === 'pasteFrames') {
      if (!clipboard) return { code: CODES.packet };
      if (clipboard.instanceId !== info.instanceId || clipboard.documentId !== info.documentId) return { code: CODES.stale };
      return { op: P + 'frames.clipboard.paste', payload: { command: 'object.frames.clipboard.paste',
        instanceId: clipboard.instanceId, documentId: clipboard.documentId, layerUid: uid(state.active), frame: state.frame, cells: clipboard.cells } };
    }
    switch (intent.kind) {
      case 'insertBlankKey': return { op: P + 'layer.key.blank', payload: { command: 'object.layer.key.blank', layerUid: uid(state.active), cell: state.frame } };
      case 'insertFrame': return { op: P + 'frames.insert', payload: selected('object.frames.insert', { cell: state.frame }) };
      case 'insertKey': return { op: P + 'layer.key.promote', payload: selected('object.layer.key.promote', { cells: [state.frame] }) };
      case 'insertKeyAt':
        if (!whole(intent.layer, 0, info.layers.length - 1) || !whole(intent.frame, 0, info.totalFrames - 1)) return { code: CODES.invalid };
        return { op: P + 'layer.key.promote', payload: { command: 'object.layer.key.promote', layerUids: [uid(intent.layer)], cells: [intent.frame] } };
      case 'convertToKeyframes':
        var frames = state.cells.length ? state.cells.filter(function (cell) { return cell.layer === state.active; }).map(function (cell) { return cell.frame; }) : [state.frame];
        if (!frames.length) return { code: CODES.selection };
        return { op: P + 'layer.key.promote', payload: { command: 'object.layer.key.promote', layerUids: [uid(state.active)], cells: frames } };
      case 'duplicateKey': return { op: P + 'frames.duplicate', payload: { command: 'object.frames.duplicate', layerUid: uid(state.active), cell: state.frame } };
      case 'extendExposure':
        // The baseline reads the count as `n || 1`.
        var count = intent.count === undefined ? 1 : intent.count || 1;
        if (!whole(count, 1, MAX_EXTEND)) return { code: CODES.invalid };
        return { op: P + 'frames.extend', payload: { command: 'object.frames.extend', cell: state.frame, count: count } };
      case 'clearKey': return { op: P + 'layer.key.clear', payload: { command: 'object.layer.key.clear', layerUid: uid(state.active), cell: state.frame } };
      case 'deleteFrames':
        if (!state.cells.length) return { code: CODES.selection };
        return { op: P + 'layer.key.clear_cells', payload: { command: 'object.layer.key.clear_cells', targets: cellRefs() } };
      case 'removeTween':
        if (!whole(intent.layer, 0, info.layers.length - 1) || !whole(intent.from, 0, info.totalFrames - 1) || !whole(intent.to, 0, info.totalFrames - 1)) return { code: CODES.invalid };
        return { op: P + 'layer.tween.remove', payload: { command: 'object.layer.tween.remove', layerUid: uid(intent.layer), from: intent.from, to: intent.to } };
      case 'removeFrame': return { op: P + 'frames.remove', payload: { command: 'object.frames.remove', from: state.frame, to: state.frame } };
      case 'removeFrameSpan':
        // No selection falls back to the single current frame, as the baseline does.
        var span = state.cells.length ? bounds(state.cells) : { minF: state.frame, maxF: state.frame };
        return { op: P + 'frames.remove', payload: { command: 'object.frames.remove', from: span.minF, to: span.maxF } };
      case 'moveKey':
        if (!whole(intent.layer, 0, info.layers.length - 1) || !whole(intent.from, 0, info.totalFrames - 1) || !whole(intent.to, 0, info.totalFrames - 1)) return { code: CODES.invalid };
        return { op: P + 'layer.key.move', payload: { command: 'object.layer.key.move', layerUid: uid(intent.layer), from: intent.from, to: intent.to } };
      case 'duplicateSelection':
        if (!state.cells.length) return { code: CODES.selection };
        return { op: P + 'frames.clipboard.duplicate', payload: { command: 'object.frames.clipboard.duplicate', layerUid: uid(state.active), selection: cellRefs() } };
      case 'repeatSelection':
        if (!Number.isSafeInteger(intent.times)) return { code: CODES.invalid };
        if (!state.cells.length) return { code: CODES.selection };
        var box = bounds(state.cells);
        return { op: P + 'frames.repeat', payload: { command: 'object.frames.repeat', fromLayerUid: uid(box.minL), toLayerUid: uid(box.maxL),
          fromFrame: box.minF, toFrame: box.maxF, times: intent.times } };
      default: return { code: CODES.invalid };
    }
  }
  // What the published timeline length must be, or null when the operation does not fix it (repeat and selection duplicate may grow it).
  function expectedLength(op, payload, before, applied) {
    if (!applied) return before;
    switch (op) {
      case P + 'frames.insert': case P + 'frames.duplicate': return before + 1;
      case P + 'frames.extend': return before + payload.count;
      case P + 'frames.remove': return before - (payload.to - payload.from + 1);
      case P + 'frames.repeat': case P + 'frames.clipboard.duplicate': return null;
      default: return before;
    }
  }
  // Where the baseline left the playhead after an applied edit that moves it: F5 and Duplicate Key step to the new frame (goToFrame(cf+1)),
  // a removal only clamps it into the shorter timeline. The editor performs the move; this is the target it should use.
  function playheadHint(op, frame, total) {
    if (op === P + 'frames.insert' || op === P + 'frames.duplicate') return frame + 1;
    if (op === P + 'frames.remove') return Math.min(frame, total - 1);
    return null;
  }
  function refusalCode(error) {
    var match = error && typeof error.message === 'string' && /^Native command refused: ([a-z_]+)$/.exec(error.message);
    return match ? match[1] : null;
  }

  function create(ports) {
    ['currentSession', 'application', 'frame', 'activeLayer', 'layerSelection', 'cellSelection', 'available'].forEach(function (name) {
      if (!ports || (name === 'application' ? !ports.application || typeof ports.application.validateRequest !== 'function' : typeof ports[name] !== 'function')) {
        throw new TypeError('Native frame edit intent requires port ' + name);
      }
    });
    var pending = false, retired = false;

    function declared(op, info) {
      try {
        ports.application.validateRequest({ apiVersion: 2, requestId: 'frame-edit-probe', instanceId: info.instanceId, documentId: info.documentId,
          operation: op, expectedRevision: info.contentRevision, payload: Contract.example(op) });
        return true;
      } catch (_) { return false; }
    }
    async function run(value) {
      if (retired) return unavailable(CODES.retired);
      if (pending) return Object.freeze({ status: 'busy' });
      var intent, session, before, state, write, step, clipboardToken;
      try {
        intent = action(value);
        if (!intent) return unavailable(CODES.invalid);
        // Kinds native has no operation for answer before any session or selection is consulted.
        if (PACKET[intent.kind] && (typeof ports.captureClipboard !== 'function' || typeof ports.clipboardUnchanged !== 'function')) return unavailable(CODES.packet);
        if (PACKET[intent.kind] && (intent.kind === 'pasteFrames' ? typeof ports.readClipboard !== 'function' || typeof ports.acceptPaste !== 'function' :
            typeof ports.acceptClipboard !== 'function')) return unavailable(CODES.packet);
        session = ports.currentSession(); before = session && session.info();
        if (!session || !before || !Array.isArray(before.layers) || !whole(before.totalFrames, 1, 4294967295) ||
            ports.available(session, before) !== true || typeof session.readPresentedObjectSession !== 'function') return unavailable(CODES.session);
        state = readState(ports, before);
        if (!state) return unavailable(CODES.invalid);
        clipboardToken = PACKET[intent.kind] ? ports.captureClipboard() : null;
        step = plan(intent, state, before, intent.kind === 'pasteFrames' ? ports.readClipboard() : null);
        if (step.code) return unavailable(step.code);
        write = session.editFrames;
        if (typeof write !== 'function' || !presented(session, before, state.frame)) return unavailable(CODES.session);
        if (!declared(step.op, before)) return unavailable(CODES.declaration);
        try {
          ports.application.validateRequest({ apiVersion: 2, requestId: 'frame-edit-probe', instanceId: before.instanceId, documentId: before.documentId,
            operation: step.op, expectedRevision: before.contentRevision, payload: step.payload });
        } catch (_) { return unavailable(CODES.request); }
      } catch (_) { return unavailable(CODES.session); }
      pending = true;
      var applied = null, frame = state.frame, layers = before.layers.map(function (row) { return row.layerUid; });
      try {
        var receipt = await write.call(session, step.op, step.payload, before), result = receipt && receipt.result;
        try { Contract.result(step.op, result); } catch (_) { return Object.freeze({ status: 'unconfirmed' }); }
        if (!receipt || receipt.apiVersion !== 2 || receipt.ok !== true || receipt.instanceId !== before.instanceId || receipt.documentId !== before.documentId ||
            !result || typeof result.applied !== 'boolean' || result.historyEntriesAdded !== (result.applied ? 1 : 0) ||
            (step.op === P + 'layer.key.blank' && result.applied !== true) ||
            receipt.contentRevision !== before.contentRevision + (result.applied ? 1 : 0)) return Object.freeze({ status: 'unconfirmed' });
        applied = result.applied;
        if (retired || ports.currentSession() !== session) return Object.freeze({ status: 'applied-unconfirmed', applied: applied });
        var after = session.info(), length = expectedLength(step.op, step.payload, before.totalFrames, applied);
        // The published length and the layer order are checked, not assumed. The playhead is the editor's: it may have been
        // clamped by a shorter timeline, so the presentation must match wherever it is now, inside the new length.
        var playhead = after && whole(after.totalFrames, 1, 4294967295) ? ports.frame() : -1;
        if (!after || after.instanceId !== before.instanceId || after.documentId !== before.documentId || after.lifecycleGeneration !== before.lifecycleGeneration ||
            after.contentRevision !== receipt.contentRevision || !Array.isArray(after.layers) || !sameList(layers, after.layers.map(function (row) { return row.layerUid; })) ||
            (length === null ? !(after.totalFrames >= before.totalFrames) : after.totalFrames !== length) || ports.available(session, after) !== true ||
            !whole(playhead, 0, after.totalFrames - 1) || !presented(session, after, playhead)) return Object.freeze({ status: 'applied-unconfirmed', applied: applied });
        var out = { status: applied ? 'applied-presented' : 'unchanged', applied: applied }, hint = applied ? playheadHint(step.op, frame, after.totalFrames) : null;
        if (PACKET[intent.kind]) {
          var current = readState(ports, after);
          if (ports.clipboardUnchanged(clipboardToken) !== true || !current || current.frame !== state.frame || current.active !== state.active || !sameList(current.layers, state.layers) ||
              JSON.stringify(current.cells) !== JSON.stringify(state.cells)) return Object.freeze({ status: 'applied-unconfirmed', applied: applied });
          if (intent.kind === 'pasteFrames') {
            if (ports.acceptPaste(step.payload, after) !== true) return Object.freeze({ status: 'applied-unconfirmed', applied: applied });
          } else if (result.clipboard !== null) {
            if (result.clipboard.instanceId !== before.instanceId || result.clipboard.documentId !== before.documentId ||
                ports.acceptClipboard(result.clipboard, intent.kind, after) !== true) return Object.freeze({ status: 'applied-unconfirmed', applied: applied });
          }
        }
        if (step.op === P + 'frames.repeat') {
          out.cycledLayers = result.cycledLayers; out.skippedComponentLayers = result.skippedComponentLayers;
        }
        if (hint !== null) out.playhead = hint;
        return Object.freeze(out);
      } catch (error) {
        // The session throws these two before anything is sent; a native refusal can only come before a receipt exists.
        var message = applied === null && error ? error.message : null;
        if (message === 'Native frame edit intent is stale') return unavailable(CODES.stale);
        if (message === 'Native frame edit intent is invalid') return unavailable(CODES.request);
        var code = applied === null ? refusalCode(error) : null;
        if (code) return Object.freeze({ status: 'refused', code: code });
        return applied === null ? Object.freeze({ status: 'unconfirmed' }) : Object.freeze({ status: 'applied-unconfirmed', applied: applied });
      } finally { pending = false; }
    }
    return Object.freeze({ run: run, isPending: function () { return pending; }, dispose: function () { retired = true; } });
  }
  return Object.freeze({ create: create, CODES: CODES, KINDS: Object.freeze(Object.keys(FIELDS)) });
}));
