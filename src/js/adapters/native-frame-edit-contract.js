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
  var DATA_BYTES = 4 * 1024 * 1024, MAX_DATA_ITEMS = DATA_BYTES / 32, MESSAGE_BYTES = DATA_BYTES + MAX_LIST * 256 + 4096;
  function bytes(value) {
    var text = JSON.stringify(value), count = 0;
    // UTF-8 of the same USVString TextEncoder would consume, without requiring
    // browser globals in classic/bare VM loaders. Unpaired surrogates become U+FFFD.
    for (var i = 0; i < text.length; i++) {
      var code = text.charCodeAt(i);
      if (code < 0x80) count += 1;
      else if (code < 0x800) count += 2;
      else if (code >= 0xD800 && code <= 0xDBFF && i + 1 < text.length &&
          text.charCodeAt(i + 1) >= 0xDC00 && text.charCodeAt(i + 1) <= 0xDFFF) {
        count += 4;
        i += 1;
      } else count += 3;
    }
    return count;
  }
  function clipboardOperation(op) { return [PREFIX + 'frames.clipboard.copy', PREFIX + 'frames.clipboard.cut', PREFIX + 'frames.clipboard.paste'].indexOf(op) >= 0; }
  function requestByteLimit(op) { return clipboardOperation(op) ? MESSAGE_BYTES : 4096; }
  function responseByteLimit(op) { return packetResult(op) ? MESSAGE_BYTES : 4096; }
  var PREFIX = 'command.document.object.';
  // Operation -> native payload command. Clipboard records remain detached data; Rust alone validates/publishes them.
  var COMMANDS = Object.freeze({
    'command.document.object.frames.insert': 'object.frames.insert',
    'command.document.object.frames.extend': 'object.frames.extend',
    'command.document.object.frames.duplicate': 'object.frames.duplicate',
    'command.document.object.frames.remove': 'object.frames.remove',
    'command.document.object.layer.key.promote': 'object.layer.key.promote',
    'command.document.object.layer.key.blank': 'object.layer.key.blank',
    'command.document.object.layer.key.clear': 'object.layer.key.clear',
    'command.document.object.layer.key.clear_cells': 'object.layer.key.clear_cells',
    'command.document.object.layer.tween.remove': 'object.layer.tween.remove',
    'command.document.object.layer.key.move': 'object.layer.key.move',
    'command.document.object.frames.clipboard.duplicate': 'object.frames.clipboard.duplicate',
    'command.document.object.frames.clipboard.copy': 'object.frames.clipboard.copy',
    'command.document.object.frames.clipboard.cut': 'object.frames.clipboard.cut',
    'command.document.object.frames.clipboard.paste': 'object.frames.clipboard.paste',
    'command.document.object.frames.move': 'object.frames.move',
    'command.document.object.layer.range.drag': 'object.layer.range.drag',
    'command.document.object.frames.repeat': 'object.frames.repeat'
  });
  var OPERATIONS = Object.freeze(Object.keys(COMMANDS));
  var OPAQUE_PATH = /^payload\.(?:layerUid|activeLayerUid|fromLayerUid|toLayerUid)$|^payload\.layerUids\.\d+$|^payload\.(?:targets|selection)\.\d+\.layerUid$/;

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

  // Bounded structural shell, matching the existing native clipboard DTO. Rust validates
  // every record and authored-order value before atomic publication; this is not a JS document owner.
  function selectedCells(value) {
    if (!Array.isArray(value) || value.length > MAX_LIST) fail('clipboard selection must be bounded');
    var seen = new Set();
    value.forEach(function (v) { cell(v); var k = cellKey(v); if (seen.has(k)) fail('duplicate clipboard cell'); seen.add(k); });
  }
  function packetCells(value) {
    if (!Array.isArray(value) || value.length > MAX_LIST) fail('clipboard cells must be bounded');
    var dataBytes = 0;
    value.forEach(function (v) {
      exact(v, ['relLayer', 'relFrame', 'keyframe', 'records'], ['orderElements', 'rasterItems', 'textItems'], 'clipboard cell');
      u32(v.relLayer, 'relLayer'); u32(v.relFrame, 'relFrame');
      if (typeof v.keyframe !== 'boolean' || !Array.isArray(v.records) || v.records.length > MAX_DATA_ITEMS ||
          !v.records.every(plain)) fail('invalid clipboard records');
      if (has(v, 'orderElements') && (!Array.isArray(v.orderElements) || v.orderElements.length > MAX_DATA_ITEMS ||
          !v.orderElements.every(plain))) fail('invalid clipboard order');
      ['rasterItems', 'textItems'].forEach(function (key) {
        if (has(v, key) && (!Array.isArray(v[key]) || v[key].length > MAX_DATA_ITEMS || !v[key].every(plain))) fail('invalid clipboard items');
        if (has(v, key)) v[key].forEach(function (item) { dataBytes += bytes(item); });
      });
      v.records.forEach(function (record) { dataBytes += bytes(record); });
      if (has(v, 'orderElements')) dataBytes += bytes(v.orderElements);
      if (dataBytes > DATA_BYTES) fail('clipboard data exceeds native 4MiB bound');
    });
  }
  function clipboard(value) {
    exact(value, ['instanceId', 'documentId', 'cells'], [], 'clipboard packet');
    uid(value.instanceId, 'clipboard instanceId'); uid(value.documentId, 'clipboard documentId'); packetCells(value.cells);
  }
  function packetResult(operation) { return operation === PREFIX + 'frames.clipboard.copy' || operation === PREFIX + 'frames.clipboard.cut'; }
  function motionKey(value) {
    exact(value, ['target', 'frame'], [], 'selected Motion key');
    if (!Number.isFinite(value.frame)) fail('invalid selected Motion frame');
    var t=value.target;
    if (!plain(t)) fail('invalid selected Motion target');
    if (t.kind === 'property') {
      exact(t, ['kind', 'target', 'property'], [], 'Motion property target');
      exact(t.target, ['contextId', 'layerUid'], ['strokeId'], 'Motion holder');
      ['contextId','layerUid'].forEach(function(k){uid(t.target[k],k);});
      if(has(t.target,'strokeId'))uid(t.target.strokeId,'strokeId');
      uid(t.property,'property');
    } else {
      exact(t, ['kind','contextId','layerUid'].concat(t.kind==='elementOrder'?['frameScope','strokeId']:[]), [], 'Motion track target');
      if(!['layerOrder','elementOrder','timeRemap'].includes(t.kind))fail('invalid Motion track kind');
      uid(t.contextId,'contextId');uid(t.layerUid,'layerUid');
      if(t.kind==='elementOrder'){
        exact(t.frameScope,['kind','frame'],[],'Motion frame scope');
        if(!['authored','reference'].includes(t.frameScope.kind))fail('invalid Motion scope');
        u32(t.frameScope.frame,'Motion frame');uid(t.strokeId,'strokeId');
      }
    }
  }
  function rangeDrag(p) {
    exact(p,['command','contextId','layerUid','activeLayerUid','part','barSelection','samples','plainClick','alt','frame','selectedKeys'],[],'layer range gesture');
    ['contextId','layerUid','activeLayerUid'].forEach(function(k){uid(p[k],k);});
    if(!['in','out','both'].includes(p.part)||typeof p.plainClick!=='boolean'||typeof p.alt!=='boolean')fail('invalid bar gesture');
    u32(p.frame,'frame');
    if(!Array.isArray(p.barSelection)||p.barSelection.length>MAX_LIST||!Array.isArray(p.samples)||p.samples.length>MAX_LIST||
       !Array.isArray(p.selectedKeys)||p.selectedKeys.length>MAX_LIST)fail('unbounded bar gesture');
    var seen=new Set();p.barSelection.forEach(function(v){exact(v,['layerUid','part'],[],'bar selection');uid(v.layerUid,'bar UID');
      if(!['in','out','both'].includes(v.part)||seen.has(v.layerUid))fail('invalid duplicate bar selection');seen.add(v.layerUid);});
    p.samples.forEach(function(v){if(!Number.isSafeInteger(v))fail('invalid ordered bar sample');});
    seen=new Set();p.selectedKeys.forEach(function(v){motionKey(v);var t=v.target,a=t.target||t,key=JSON.stringify([t.kind,a.contextId,a.layerUid,Object.prototype.hasOwnProperty.call(a,'strokeId'),a.strokeId,t.property,t.frameScope&&t.frameScope.kind,t.frameScope&&t.frameScope.frame,v.frame]);
      if(seen.has(key))fail('duplicate selected key');seen.add(key);});
  }
  var SHAPES = {
    'object.layer.range.drag': rangeDrag,
    'object.frames.clipboard.copy': function (p) { exact(p, ['command', 'selection'], [], 'clipboard copy'); selectedCells(p.selection); },
    'object.frames.clipboard.cut': function (p) { exact(p, ['command', 'selection'], [], 'clipboard cut'); selectedCells(p.selection); },
    'object.frames.clipboard.paste': function (p) {
      exact(p, ['command', 'instanceId', 'documentId', 'layerUid', 'frame', 'cells'], [], 'clipboard paste');
      uid(p.instanceId, 'instanceId'); uid(p.documentId, 'documentId'); uid(p.layerUid, 'layerUid'); u32(p.frame, 'frame'); packetCells(p.cells);
    },
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
    'object.layer.key.blank': function (p) {
      exact(p, ['command', 'layerUid', 'cell'], [], 'key blank'); uid(p.layerUid, 'layerUid'); u32(p.cell, 'cell');
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
      exact(p, ['command', 'layerUid', 'from', 'to'], ['activeLayerUid'], 'key move');
      if (has(p, 'activeLayerUid')) uid(p.activeLayerUid, 'activeLayerUid');
      uid(p.layerUid, 'layerUid'); u32(p.from, 'from'); u32(p.to, 'to');
    },
    'object.frames.clipboard.duplicate': function (p) {
      exact(p, ['command', 'layerUid', 'selection'], [], 'selection duplicate');
      uid(p.layerUid, 'layerUid'); list(p.selection, 'selection', cell, cellKey);
    },
    'object.frames.move': function (p) {
      exact(p, ['command', 'selection', 'layerUid', 'frame'], [], 'frames move');
      selectedCells(p.selection); uid(p.layerUid, 'layerUid'); u32(p.frame, 'frame');
    },
    'object.frames.repeat': function (p) {
      exact(p, ['command', 'fromLayerUid', 'toLayerUid', 'fromFrame', 'toFrame', 'times'], [], 'frames repeat');
      uid(p.fromLayerUid, 'fromLayerUid'); uid(p.toLayerUid, 'toLayerUid'); u32(p.fromFrame, 'fromFrame'); u32(p.toFrame, 'toFrame');
      if (!Number.isSafeInteger(p.times)) fail('times must be an integer');
    }
  };
  var EXAMPLES = {
    'object.layer.range.drag': function () { return {command:'object.layer.range.drag',contextId:'scene-root',layerUid:'layer-a',activeLayerUid:'layer-b',part:'both',barSelection:[],samples:[0,2],plainClick:false,alt:false,frame:1,selectedKeys:[]}; },
    'object.frames.clipboard.copy': function () { return {"command":"object.frames.clipboard.copy","selection":[{"layerUid":"layer-a","frame":2}]}; },
    'object.frames.clipboard.cut': function () { return {"command":"object.frames.clipboard.cut","selection":[{"layerUid":"layer-a","frame":2}]}; },
    'object.frames.clipboard.paste': function () { return {"command":"object.frames.clipboard.paste","instanceId":"instance-A","documentId":"document-A","cells":[{"relLayer":0,"relFrame":0,"keyframe":true,"records":[],"orderElements":[]}],"layerUid":"layer-a","frame":4}; },
    'object.frames.insert': function () { return { command: 'object.frames.insert', cell: 3 }; },
    'object.frames.extend': function () { return { command: 'object.frames.extend', cell: 3, count: 1 }; },
    'object.frames.duplicate': function () { return { command: 'object.frames.duplicate', layerUid: 'layer-a', cell: 3 }; },
    'object.frames.remove': function () { return { command: 'object.frames.remove', from: 3, to: 3 }; },
    'object.layer.key.promote': function () { return { command: 'object.layer.key.promote', cells: [3] }; },
    'object.layer.key.blank': function () { return { command: 'object.layer.key.blank', layerUid: 'layer-a', cell: 3 }; },
    'object.layer.key.clear': function () { return { command: 'object.layer.key.clear', layerUid: 'layer-a', cell: 3 }; },
    'object.layer.key.clear_cells': function () {
      return { command: 'object.layer.key.clear_cells', targets: [{ layerUid: 'layer-a', frame: 3 }] };
    },
    'object.layer.tween.remove': function () { return { command: 'object.layer.tween.remove', layerUid: 'layer-a', from: 2, to: 6 }; },
    'object.layer.key.move': function () { return { command: 'object.layer.key.move', layerUid: 'layer-a', from: 2, to: 4 }; },
    'object.frames.clipboard.duplicate': function () {
      return { command: 'object.frames.clipboard.duplicate', layerUid: 'layer-a', selection: [{ layerUid: 'layer-a', frame: 2 }] };
    },
    'object.frames.move': function () {
      return { command: 'object.frames.move', selection: [{ layerUid: 'layer-a', frame: 3 }], layerUid: 'layer-b', frame: 4 };
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
    var repeat = operation === PREFIX + 'frames.repeat', move = operation === PREFIX + 'frames.move';
    exact(value, ['applied', 'historyEntriesAdded'].concat(repeat ? ['cycledLayers', 'skippedComponentLayers'] : move ? ['selection'] : packetResult(operation) ? ['clipboard'] : []), [], 'frame edit result');
    if (packetResult(operation) && value.clipboard !== null) clipboard(value.clipboard);
    if (operation === PREFIX + 'frames.clipboard.cut' && value.applied === true && value.clipboard === null) fail('applied cut must carry its packet');
    if (operation === PREFIX + 'frames.clipboard.copy' && (value.applied !== false || value.historyEntriesAdded !== 0)) fail('copy cannot mutate history');
    if (move) {
      if (value.applied === true) selectedCells(value.selection);
      else if (value.selection !== null) fail('unchanged move must retain selection');
    }
    if (repeat) { u32(value.cycledLayers, 'cycledLayers'); u32(value.skippedComponentLayers, 'skippedComponentLayers'); }
    // F7 preserves the unconditional baseline checkpoint, including an already blank key.
    if (['layer.key.blank','layer.range.drag'].some(function(k){return operation===PREFIX+k;}) && value.applied !== true) fail('blank key must add one history entry');
    if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) {
      fail('invalid frame edit disposition');
    }
  }
  function correlate(request, response) {
    if (response.instanceId !== request.instanceId) fail('frame edit response owner mismatch');
    var replaced = !response.ok && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('frame edit response owner mismatch');
    if (response.ok && packetResult(request.operation) && response.result.clipboard !== null &&
        (response.result.clipboard.instanceId !== request.instanceId || response.result.clipboard.documentId !== request.documentId)) fail('clipboard response owner mismatch');
    if (response.ok && response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) {
      fail('frame edit receipt revision mismatch');
    }
  }
  // Layer UIDs are opaque authored bytes: a `data:`-prefixed UID is a name, not a resource payload.
  function paths(operation) {
    return admitted(operation) ? { has: function (path) {
      return OPAQUE_PATH.test(path) || (operation === PREFIX + 'layer.range.drag' && (/^payload\.(?:contextId|activeLayerUid)$/.test(path) || /^payload\.barSelection\.\d+\.layerUid$/.test(path) || /^payload\.selectedKeys\.\d+\.target\.(?:target\.)?(?:contextId|layerUid|strokeId)$/.test(path))) || (operation === PREFIX + 'frames.move' && /^result\.selection\.\d+\.layerUid$/.test(path)) || /^payload\.cells\.\d+\.(?:records\.\d+\.target\.(?:contextId|layerUid|strokeId)|orderElements\.\d+\.(?:layerUid|strokeId)|(?:rasterItems|textItems)\.\d+\.target\.(?:contextId|layerUid|strokeId))$/.test(path) ||
        /^result\.clipboard\.cells\.\d+\.(?:records\.\d+\.target\.(?:contextId|layerUid|strokeId)|orderElements\.\d+\.(?:layerUid|strokeId)|(?:rasterItems|textItems)\.\d+\.target\.(?:contextId|layerUid|strokeId))$/.test(path);
    } } : null;
  }
  // The SDK holds one contract object. join() adds these operations to whichever contract it already has (fonts).
  function join(base) {
    function pick(operation) {
      if (admitted(operation)) return api;
      if (!base) fail('frame edit join has no contract for ' + operation);
      return base;
    }
    return Object.freeze({
      requestByteLimit: requestByteLimit, responseByteLimit: responseByteLimit,
      admitted: function (operation) { return admitted(operation) || !!base && base.admitted(operation); },
      requiresRevision: function (operation) { return admitted(operation) ? true : !!base && base.requiresRevision(operation); },
      payload: function (operation, value, envelope) { return pick(operation).payload(operation, value, envelope); },
      result: function (operation, value) { return pick(operation).result(operation, value); },
      correlate: function (request, response) { return pick(request.operation).correlate(request, response); },
      paths: function (operation) { return pick(operation).paths(operation); }
    });
  }
  var api = Object.freeze({
    DATA_BYTES: DATA_BYTES, MAX_DATA_ITEMS: MAX_DATA_ITEMS, MESSAGE_BYTES: MESSAGE_BYTES, requestByteLimit: requestByteLimit, responseByteLimit: responseByteLimit,
    PREFIX: PREFIX, OPERATIONS: OPERATIONS, admitted: admitted, requiresRevision: requiresRevision, commandOf: commandOf,
    payload: payload, valid: valid, result: result, correlate: correlate, paths: paths, example: example, join: join
  });
  return api;
}));
