/* L05-K public contract of the native frame-edit operations (F09-F13). Joined to the common v2 SDK by
 * native-application.js only: it declares closed request/result shapes and nothing else. The native owner stays the
 * single authority (no JS/Paper writer, no optimistic mutation, no fallback). Every shape mirrors the strict payload
 * parser of the Rust owner of that operation (L05-C structure, L05-D keys, L05-E retime, L05-I clipboard, L05-J1 repeat). */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeFrameEditContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var MAX_U32 = 4294967295, MAX_LIST = 4096, MAX_EXTEND = 999;
  var PREFIX = 'command.document.object.';
  // operation -> payload `command` member. Cut/copy/paste are deliberately absent: a clipboard packet is produced by the
  // typed native API only, so there is no public declaration for them yet (see HANDOFF, remaining obligations).
  var COMMANDS = Object.freeze({
    'command.document.object.frames.insert': 'object.frames.insert',
    'command.document.object.frames.extend': 'object.frames.extend',
    'command.document.object.frames.duplicate': 'object.frames.duplicate',
    'command.document.object.frames.remove': 'object.frames.remove',
    'command.document.object.layer.key.promote': 'object.layer.key.promote',
    'command.document.object.layer.key.clear': 'object.layer.key.clear',
    'command.document.object.layer.key.clear_cells': 'object.layer.key.clear_cells',
    'command.document.object.layer.tween.remove': 'object.layer.tween.remove',
    'command.document.object.layer.key.move': 'object.layer.key.move',
    'command.document.object.frames.clipboard.duplicate': 'object.frames.clipboard.duplicate',
    'command.document.object.frames.repeat': 'object.frames.repeat'
  });
  var OPERATIONS = Object.freeze(Object.keys(COMMANDS));
  var OPAQUE_PATH = /^payload\.(?:layerUid|fromLayerUid|toLayerUid)$|^payload\.layerUids\.\d+$|^payload\.(?:targets|selection)\.\d+\.layerUid$/;

  function has(value, key) { return Object.prototype.hasOwnProperty.call(value, key); }
  function fail(message) { throw new TypeError(message); }
  function plain(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value) &&
      (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  }
  function exact(value, required, optional, label) {
    if (!plain(value)) fail(label + ' must be a plain object');
    var allowed = required.concat(optional || []);
    if (!required.every(function (key) { return has(value, key); }) ||
        Object.keys(value).some(function (key) { return allowed.indexOf(key) < 0; })) {
      fail(label + ' has missing or unknown fields');
    }
    return value;
  }
  function u32(value, label) {
    if (!Number.isSafeInteger(value) || value < 0 || value > MAX_U32) fail(label + ' must be a u32 frame number');
  }
  function uid(value, label) {
    if (typeof value !== 'string' || value.length === 0) fail(label + ' must be a nonempty opaque layer UID');
  }
  function list(value, label, each, key) {
    if (!Array.isArray(value) || value.length < 1 || value.length > MAX_LIST) fail(label + ' must be a bounded nonempty list');
    var seen = new Set();
    value.forEach(function (item) {
      each(item);
      var token = key ? key(item) : item;
      if (seen.has(token)) fail(label + ' must not repeat an entry');
      seen.add(token);
    });
  }
  function layerList(value, label) { list(value, label, function (item) { uid(item, label + ' entry'); }); }
  function cell(value) {
    exact(value, ['layerUid', 'frame'], [], 'cell reference');
    uid(value.layerUid, 'cell layerUid'); u32(value.frame, 'cell frame');
  }
  function cellKey(value) { return JSON.stringify([value.layerUid, value.frame]); }

  var SHAPES = {
    'object.frames.insert': function (p) {
      exact(p, ['command', 'cell'], ['layerUids'], 'frames insert');
      u32(p.cell, 'cell'); if (has(p, 'layerUids')) layerList(p.layerUids, 'layerUids');
    },
    'object.frames.extend': function (p) {
      exact(p, ['command', 'cell', 'count'], [], 'frames extend');
      u32(p.cell, 'cell'); u32(p.count, 'count');
      if (p.count > MAX_EXTEND) fail('count exceeds the frame-count field bound');
    },
    'object.frames.duplicate': function (p) {
      exact(p, ['command', 'layerUid', 'cell'], [], 'frames duplicate'); uid(p.layerUid, 'layerUid'); u32(p.cell, 'cell');
    },
    'object.frames.remove': function (p) {
      exact(p, ['command', 'from', 'to'], [], 'frames remove'); u32(p.from, 'from'); u32(p.to, 'to');
    },
    'object.layer.key.promote': function (p) {
      exact(p, ['command', 'cells'], ['layerUids'], 'key promote');
      list(p.cells, 'cells', function (item) { u32(item, 'cell'); });
      if (has(p, 'layerUids')) layerList(p.layerUids, 'layerUids');
    },
    'object.layer.key.clear': function (p) {
      exact(p, ['command', 'layerUid', 'cell'], [], 'key clear'); uid(p.layerUid, 'layerUid'); u32(p.cell, 'cell');
    },
    'object.layer.key.clear_cells': function (p) {
      exact(p, ['command', 'targets'], [], 'clear cells'); list(p.targets, 'targets', cell, cellKey);
    },
    'object.layer.tween.remove': function (p) {
      exact(p, ['command', 'layerUid', 'from', 'to'], [], 'tween remove');
      uid(p.layerUid, 'layerUid'); u32(p.from, 'from'); u32(p.to, 'to');
    },
    'object.layer.key.move': function (p) {
      exact(p, ['command', 'layerUid', 'from', 'to'], [], 'key move');
      uid(p.layerUid, 'layerUid'); u32(p.from, 'from'); u32(p.to, 'to');
    },
    'object.frames.clipboard.duplicate': function (p) {
      exact(p, ['command', 'layerUid', 'selection'], [], 'selection duplicate');
      uid(p.layerUid, 'layerUid'); list(p.selection, 'selection', cell, cellKey);
    },
    'object.frames.repeat': function (p) {
      exact(p, ['command', 'fromLayerUid', 'toLayerUid', 'fromFrame', 'toFrame', 'times'], [], 'frames repeat');
      uid(p.fromLayerUid, 'fromLayerUid'); uid(p.toLayerUid, 'toLayerUid'); u32(p.fromFrame, 'fromFrame'); u32(p.toFrame, 'toFrame');
      if (!Number.isSafeInteger(p.times)) fail('times must be an integer');
    }
  };
  var EXAMPLES = {
    'object.frames.insert': function () { return { command: 'object.frames.insert', cell: 3 }; },
    'object.frames.extend': function () { return { command: 'object.frames.extend', cell: 3, count: 1 }; },
    'object.frames.duplicate': function () { return { command: 'object.frames.duplicate', layerUid: 'layer-a', cell: 3 }; },
    'object.frames.remove': function () { return { command: 'object.frames.remove', from: 3, to: 3 }; },
    'object.layer.key.promote': function () { return { command: 'object.layer.key.promote', cells: [3] }; },
    'object.layer.key.clear': function () { return { command: 'object.layer.key.clear', layerUid: 'layer-a', cell: 3 }; },
    'object.layer.key.clear_cells': function () {
      return { command: 'object.layer.key.clear_cells', targets: [{ layerUid: 'layer-a', frame: 3 }] };
    },
    'object.layer.tween.remove': function () { return { command: 'object.layer.tween.remove', layerUid: 'layer-a', from: 2, to: 6 }; },
    'object.layer.key.move': function () { return { command: 'object.layer.key.move', layerUid: 'layer-a', from: 2, to: 4 }; },
    'object.frames.clipboard.duplicate': function () {
      return { command: 'object.frames.clipboard.duplicate', layerUid: 'layer-a', selection: [{ layerUid: 'layer-a', frame: 2 }] };
    },
    'object.frames.repeat': function () {
      return { command: 'object.frames.repeat', fromLayerUid: 'layer-a', toLayerUid: 'layer-a', fromFrame: 2, toFrame: 3, times: 2 };
    }
  };

  function admitted(operation) { return typeof operation === 'string' && has(COMMANDS, operation); }
  function requiresRevision(operation) { return admitted(operation); }
  function commandOf(operation) { return admitted(operation) ? COMMANDS[operation] : null; }
  function payload(operation, value) {
    if (!admitted(operation)) fail('frame edit operation is not declared');
    var command = COMMANDS[operation];
    if (!plain(value) || value.command !== command) fail('frame edit command does not match its operation');
    SHAPES[command](value);
  }
  // Non-throwing form of payload() for callers that only need a verdict (the session mutation).
  function valid(operation, value) {
    try { payload(operation, value); return true; } catch (_) { return false; }
  }
  function example(operation) {
    if (!admitted(operation)) fail('frame edit operation is not declared');
    return EXAMPLES[COMMANDS[operation]]();
  }
  function result(operation, value) {
    if (!admitted(operation)) fail('frame edit operation is not declared');
    exact(value, ['applied', 'historyEntriesAdded'], [], 'frame edit result');
    if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) {
      fail('invalid frame edit disposition');
    }
  }
  function correlate(request, response) {
    if (response.instanceId !== request.instanceId) fail('frame edit response owner mismatch');
    var replaced = !response.ok && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('frame edit response owner mismatch');
    if (response.ok && response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) {
      fail('frame edit receipt revision mismatch');
    }
  }
  // Layer UIDs are opaque authored bytes: a `data:`-prefixed UID is a name, not a resource payload.
  function paths(operation) {
    return admitted(operation) ? { has: function (path) { return OPAQUE_PATH.test(path); } } : null;
  }
  // The SDK holds one contract object. join() adds these operations to whichever contract it already has (fonts).
  function join(base) {
    function pick(operation) {
      if (admitted(operation)) return api;
      if (!base) fail('frame edit join has no contract for ' + operation);
      return base;
    }
    return Object.freeze({
      admitted: function (operation) { return admitted(operation) || !!base && base.admitted(operation); },
      requiresRevision: function (operation) { return admitted(operation) ? true : !!base && base.requiresRevision(operation); },
      payload: function (operation, value, envelope) { return pick(operation).payload(operation, value, envelope); },
      result: function (operation, value) { return pick(operation).result(operation, value); },
      correlate: function (request, response) { return pick(request.operation).correlate(request, response); },
      paths: function (operation) { return pick(operation).paths(operation); }
    });
  }
  var api = Object.freeze({
    PREFIX: PREFIX, OPERATIONS: OPERATIONS, admitted: admitted, requiresRevision: requiresRevision, commandOf: commandOf,
    payload: payload, valid: valid, result: result, correlate: correlate, paths: paths, example: example, join: join
  });
  return api;
}));
