/* L02-J closed declarations of the native object edit operations (layer lifecycle, groups, selection edit,
 * clipboard copy/cut/paste, boolean/erase/flatten). Pure validation with the interface of the font contract (admitted,
 * requiresRevision, payload, result, correlate, paths); `compose(base)` joins another contract of that interface.
 * native-object-edit-seam.js performs the single composition the common v2 SDK injects.
 * No owner, no writer, no host. */
(function (root, factory) {
  const subselect = typeof module === 'object' && module.exports ? require('./native-subselect-contract.js') : root.NemoNativeSubselectContract;
  const api = factory(subselect);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectEditContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Subselect) {
  'use strict';

  const PREFIX = 'command.document.object.';
  const MAX_ID = 128, MAX_NAME = 256, MAX_LABEL = 64, MAX_ITEMS = 1024, MAX_FRAME = 4294967295;
  const COMBINE_MODES = ['none', 'unite', 'subtract', 'intersect', 'exclude'];
  const BOOLEAN_MODES = ['unite', 'subtract', 'intersect', 'exclude'];
  const EDIT_MODES = ['animation2d', 'motion'];
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const fail = (message) => { throw new TypeError(message); };
  const bytes = (text) => (typeof TextEncoder === 'function' ? new TextEncoder().encode(text).length : unescape(encodeURIComponent(text)).length);

  function plain(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value) &&
      (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  }
  function exact(value, required, optional, label) {
    if (!plain(value)) fail(`${label} must be a plain object`);
    const allowed = new Set([...required, ...(optional || [])]);
    if (!required.every((key) => has(value, key)) || Object.keys(value).some((key) => !allowed.has(key))) fail(`${label} has missing or unknown fields`);
    return value;
  }
  // Rust `text_ok`: non-empty, bounded in UTF-8 bytes, no control characters (Cc).
  function text(value, limit, label) {
    if (typeof value !== 'string' || !value.length || bytes(value) > limit || /[\u0000-\u001f\u007f-\u009f]/.test(value)) fail(`${label} is not a bounded identity`);
    return value;
  }
  function opaque(value, label) { if (typeof value !== 'string' || !value.length) fail(`${label} must be a non-empty string`); return value; }
  function frame(value, label) { if (!Number.isSafeInteger(value) || value < 0 || value > MAX_FRAME) fail(`${label} must be a native u32 frame`); return value; }
  function oneOf(value, list, label) { if (!list.includes(value)) fail(`${label} is not admitted`); return value; }
  function list(value, label, min, max, each) {
    if (!Array.isArray(value) || value.length < min || value.length > max) fail(`${label} has an invalid length`);
    value.forEach((item, index) => each(item, `${label}[${index}]`));
    return value;
  }
  function selection(value, label) {
    exact(value, ['active', 'selected'], ['anchor'], label);
    opaque(value.active, `${label}.active`);
    list(value.selected, `${label}.selected`, 0, MAX_ITEMS, opaque);
    if (has(value, 'anchor') && value.anchor !== null) opaque(value.anchor, `${label}.anchor`);
  }
  function labels(value, label) {
    exact(value, ['shape', 'stroke'], [], label);
    text(value.shape, MAX_LABEL, `${label}.shape`); text(value.stroke, MAX_LABEL, `${label}.stroke`);
  }
  function footprint(value, label) {
    list(value, label, 3, MAX_ITEMS, (point, at) => {
      if (!Array.isArray(point) || point.length !== 2 || !point.every((n) => typeof n === 'number' && Number.isFinite(n))) fail(`${at} must be a finite [x, y] pair`);
    });
  }
  function clipRecord(value, label) {
    exact(value, ['schemaVersion', 'family', 'target', 'geometry'], ['fill', 'vector', 'vectorExt', 'effects'], label);
    exact(value.target, ['contextId', 'frameScope', 'layerUid', 'strokeId'], [], `${label}.target`);
    exact(value.target.frameScope, ['kind', 'frame'], [], `${label}.target.frameScope`);
    if (!plain(value.geometry)) fail(`${label}.geometry must be a plain object`);
  }
  // Bounded structural pre-check only; the native owner re-validates every record (deny_unknown_fields).
  function clip(value, label) {
    exact(value, ['contextId', 'layerUid', 'frame', 'records'], ['groups', 'rasters', 'source'], label);
    text(value.contextId, MAX_ID, `${label}.contextId`); text(value.layerUid, MAX_ID, `${label}.layerUid`); frame(value.frame, `${label}.frame`);
    list(value.records, `${label}.records`, 0, MAX_ITEMS, clipRecord);
    if (has(value, 'groups')) exact(value.groups, ['contextId', 'entries', 'members'], [], `${label}.groups`);
    if (has(value, 'rasters') !== has(value, 'source')) fail(`${label} requires raster provenance`);
    if (has(value, 'rasters')) {
      list(value.rasters, `${label}.rasters`, 1, MAX_ITEMS, (entry, at) => {
        exact(entry, ['target', 'item'], [], at);
        exact(entry.target, ['contextId', 'frameScope', 'layerUid', 'strokeId'], [], `${at}.target`);
        exact(entry.target.frameScope, ['kind', 'frame'], [], `${at}.target.frameScope`);
        exact(entry.item, ['assetId', 'x', 'y', 'width', 'height'],
          ['opacity', 'rotation', 'isBitmapBrush', 'brushGroupId', 'isBrushTextureCopy', 'meshId'], `${at}.item`);
      });
      exact(value.source, ['instanceId', 'documentId', 'contentRevision', 'strokeIds'], [], `${label}.source`);
      ['instanceId', 'documentId'].forEach((key) => text(value.source[key], MAX_ID, `${label}.source.${key}`));
      if (!Number.isSafeInteger(value.source.contentRevision) || value.source.contentRevision < 0) fail(`${label}.source.contentRevision invalid`);
      list(value.source.strokeIds, `${label}.source.strokeIds`, 1, MAX_ITEMS, (id, at) => text(id, MAX_ID, at));
    }
  }

  const uid = (v, at) => text(v, MAX_ID, at), layerUid = opaque;
  const uids = (min) => (v, at) => list(v, at, min, MAX_ITEMS, (item, i) => text(item, MAX_ID, i));
  const FIELDS = {
    subselectTarget: v=>{if(!Subselect.target(v))fail('Invalid subselect target');},
    subselectIntent: v=>{if(!Subselect.intent(v))fail('Invalid subselect intent');},
    point: (v,at)=>{if(!Array.isArray(v)||v.length!==2||v.some((n)=>typeof n!=='number'||!Number.isFinite(n)||Math.abs(n)>1e12))fail(at+' invalid fill point');},
    color: (v,at)=>{if(typeof v!=='string'||!/^#[0-9a-fA-F]{8}$/.test(v))fail(at+' requires hex8');},
    opacity: (v,at)=>{if(typeof v!=='number'||!Number.isFinite(v)||v<0||v>1)fail(at+' invalid opacity');},
    flag: (v,at)=>{if(typeof v!=='boolean')fail(at+' requires boolean');},
    // A Fill Brush outline: 3 to 256 closed-path segments, each point and relative handle a finite pair.
    outline: (v,at)=>{if(!Array.isArray(v)||v.length<3||v.length>256)fail(at+' requires 3 to 256 segments');
      v.forEach((s,i)=>{if(!s||typeof s!=='object'||Array.isArray(s)||Object.keys(s).sort().join()!=='handleIn,handleOut,point')fail(at+'['+i+'] invalid segment');
        ['point','handleIn','handleOut'].forEach((k)=>FIELDS.point(s[k],at+'['+i+'].'+k));});},
    placement: (v,at)=>{if(!['above','below','merge'].includes(v))fail(at+' requires above, below or merge');},
    mask: (v,at)=>{if(v!==false&&!(typeof v==='string'&&v.length>0&&bytes(v)<=64&&!/[\u0000-\u001f\u007f-\u009f]/.test(v)))fail(at+' requires false or a mask mode');},
    uid: uid, layerUid: layerUid, layerUids: (v, at) => { list(v, at, 1, MAX_ITEMS, opaque); if (new Set(v).size !== v.length) fail(`${at} repeats a layer`); },
    gap: (v, at) => { if (typeof v !== 'number' || !Number.isFinite(v)) fail(`${at} must be a finite number`); },
    selection: selection, name: (v, at) => { if (typeof v !== 'string') fail(`${at} must be a string`); },
    contextId: uid, groupId: uid, strokeId: uid, frame: frame,
    groupName: (v, at) => text(v, MAX_NAME, at), strokeIds1: uids(1), strokeIds0: uids(0), strokeIds2: uids(2),
    combineMode: (v, at) => oneOf(v, COMBINE_MODES, at), booleanMode: (v, at) => oneOf(v, BOOLEAN_MODES, at),
    editMode: (v, at) => oneOf(v, EDIT_MODES, at), labels: labels, clip: clip, footprint: footprint,
  };
  // operation suffix -> [command, result kind, [wire member, validator]...]. Member order is the wire order.
  const SPEC = {
    'subselect.edit': ['object.subselect.edit', 'command', [['stableTarget','subselectTarget'],['intent','subselectIntent']]],
    'layer.delete': ['object.layer.delete', 'command', [['selection', 'selection']]],
    'layer.duplicate': ['object.layer.duplicate', 'created', [['layerUid', 'layerUid']]],
    'layer.rename': ['object.layer.rename', 'command', [['layerUid', 'layerUid'], ['name', 'name']]],
    'layer.reorder': ['object.layer.reorder', 'command', [['layerUids', 'layerUids'], ['gap', 'gap'], ['selection', 'selection']]],
    'group.create': ['object.group.create', 'command', [['layerUid', 'uid'], ['contextId', 'contextId'], ['groupId', 'groupId'], ['name', 'groupName'], ['strokeIds', 'strokeIds1']]],
    'group.ungroup': ['object.group.ungroup', 'command', [['layerUid', 'uid'], ['contextId', 'contextId'], ['strokeIds', 'strokeIds1']]],
    'group.combine': ['object.group.combine', 'command', [['layerUid', 'uid'], ['contextId', 'contextId'], ['groupId', 'groupId'], ['mode', 'combineMode'], ['strokeIds', 'strokeIds1']]],
    'group.mode.set': ['object.group.mode.set', 'command', [['layerUid', 'uid'], ['contextId', 'contextId'], ['groupId', 'groupId'], ['mode', 'combineMode']]],
    'group.rename': ['object.group.rename', 'command', [['layerUid', 'uid'], ['contextId', 'contextId'], ['groupId', 'groupId'], ['name', 'groupName']]],
    'group.member.remove': ['object.group.member.remove', 'command', [['layerUid', 'uid'], ['contextId', 'contextId'], ['strokeId', 'strokeId']]],
    'selection.delete': ['object.selection.delete', 'command', [['contextId', 'contextId'], ['layerUid', 'uid'], ['frame', 'frame'], ['mode', 'editMode'], ['strokeIds', 'strokeIds0']]],
    'selection.duplicate': ['object.selection.duplicate', 'command', [['contextId', 'contextId'], ['layerUid', 'uid'], ['frame', 'frame'], ['mode', 'editMode'], ['strokeIds', 'strokeIds0']]],
    'layer.split': ['object.layer.split', 'command', [['contextId', 'contextId'], ['layerUid', 'uid'], ['frame', 'frame'], ['labels', 'labels']]],
    'clipboard.paste': ['object.clipboard.paste', 'command', [['contextId', 'contextId'], ['layerUid', 'uid'], ['frame', 'frame'], ['mode', 'editMode'], ['clip', 'clip']]],
    'boolean.apply': ['object.boolean.apply', 'command', [['mode', 'booleanMode'], ['layerUid', 'uid'], ['contextId', 'contextId'], ['frame', 'frame'], ['strokeIds', 'strokeIds2']]],
    'boolean.erase': ['object.boolean.erase', 'command', [['layerUid', 'uid'], ['contextId', 'contextId'], ['frame', 'frame'], ['strokeId', 'strokeId'], ['footprint', 'footprint']]],
    'boolean.flatten': ['object.boolean.flatten', 'command', [['layerUid', 'uid'], ['contextId', 'contextId'], ['frame', 'frame'], ['groupId', 'groupId']]],
    'fill.trace':['object.fill.trace','command',[['layerUid','uid'],['contextId','contextId'],['frame','frame'],['point','point'],['color','color'],['opacity','opacity'],['remove','flag']]],
    'fill.brush':['object.fill.brush','command',[['layerUid','uid'],['contextId','contextId'],['frame','frame'],['outline','outline'],['color','color'],['opacity','opacity'],['placement','placement'],['mask','mask']]],
    'clipboard.copy': ['object.clipboard.copy', 'clipboard-copy', [['contextId', 'contextId'], ['layerUid', 'uid'], ['frame', 'frame'], ['mode', 'editMode'], ['strokeIds', 'strokeIds0']]],
    'clipboard.cut': ['object.clipboard.cut', 'clipboard-cut', [['contextId', 'contextId'], ['layerUid', 'uid'], ['frame', 'frame'], ['mode', 'editMode'], ['strokeIds', 'strokeIds0']]],
  };
  const OPERATIONS = Object.freeze(Object.keys(SPEC).map((suffix) => PREFIX + suffix));
  const spec = (operation) => (typeof operation === 'string' && operation.startsWith(PREFIX) && has(SPEC, operation.slice(PREFIX.length)) ? SPEC[operation.slice(PREFIX.length)] : null);
  const admitted = (operation) => spec(operation) !== null;

  function payload(operation, value) {
    const entry = spec(operation);
    if (!entry) fail('native object edit operation is not declared');
    exact(value, ['command', ...entry[2].map((member) => member[0])], [], operation);
    if (value.command !== entry[0]) fail(`${operation} command is invalid`);
    for (const [member, kind] of entry[2]) FIELDS[kind](value[member], `${operation}.${member}`);
  }
  function result(operation, value) {
    const entry = spec(operation);
    if (!entry) fail('native object edit operation is not declared');
    if (entry[1] === 'clipboard-copy' || entry[1] === 'clipboard-cut') {
      exact(value, ['applied', 'historyEntriesAdded', 'clipboard'], [], 'clipboard result');
      if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail('invalid clipboard disposition');
      if (entry[1] === 'clipboard-copy' && value.applied !== false) fail('copy cannot change document history');
      if (entry[1] === 'clipboard-cut' && value.applied !== (value.clipboard !== null)) fail('cut disposition must match clipboard presence');
      if (value.clipboard !== null) clip(value.clipboard, 'clipboard result.clipboard');
      return;
    }
    if (entry[1] === 'created') {
      exact(value, ['applied', 'historyEntriesAdded', 'layerUid'], [], 'layer duplicate result');
      if (value.applied !== true || value.historyEntriesAdded !== 1) fail('invalid layer duplicate disposition');
      opaque(value.layerUid, 'layerUid');
      return;
    }
    exact(value, ['applied', 'historyEntriesAdded'], [], 'object edit result');
    if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail('invalid object edit disposition');
  }
  function correlate(request, response) {
    if (response.instanceId !== request.instanceId) fail('object edit response owner mismatch');
    const replaced = response.ok === false && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('object edit response document mismatch');
    if (!response.ok) return;
    if (response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) fail('object edit receipt revision mismatch');
    if (/\.clipboard\.(?:copy|cut)$/.test(request.operation) && response.result.clipboard !== null &&
        ['contextId', 'layerUid', 'frame'].some((key) => response.result.clipboard[key] !== request.payload[key])) fail('clipboard source selector mismatch');
  }
  // Every opaque authored string may legitimately start with data: or blob:; the SDK only inspects it as text.
  const OPAQUE = Object.freeze({ has: (at) => /^payload\./.test(at) || at === 'result.layerUid' });
  const CLIP_OPAQUE = Object.freeze({ has: (at) => OPAQUE.has(at) || /^result\.clipboard(?:\.|$)/.test(at) });
  const paths = (operation) => admitted(operation) ? (/\.clipboard\.(?:copy|cut)$/.test(operation) ? CLIP_OPAQUE : OPAQUE) : null;
  const own = Object.freeze({ admitted, requiresRevision: admitted, payload, result, correlate, paths });

  // Same interface as the font contract, so one injected seam serves both.
  function compose(base) {
    if (!base) return own;
    return Object.freeze({
      admitted: (op) => admitted(op) || base.admitted(op),
      requiresRevision: (op) => admitted(op) || base.requiresRevision(op),
      payload: (op, value, envelope) => (admitted(op) ? payload(op, value) : base.payload(op, value, envelope)),
      result: (op, value) => (admitted(op) ? result(op, value) : base.result(op, value)),
      correlate: (request, response) => (admitted(request.operation) ? correlate(request, response) : base.correlate(request, response)),
      paths: (op) => (admitted(op) ? paths(op) : base.paths(op)),
    });
  }
  function commandOf(operation) { const entry = spec(operation); return entry ? entry[0] : null; }
  function members(operation) { const entry = spec(operation); return entry ? entry[2].map((m) => m[0]) : null; }
  function kinds(operation) { const entry = spec(operation); return entry ? entry[2].map((m) => m.slice()) : null; }
  // The command runner's bound for an edit request: a Fill Brush outline (up to 256 segments) gets the vector-geometry bound,
  // as the SDK does (native-object-vector-contract.js); every other edit stays at 4096.
  const requestByteLimit = (op) => (op === PREFIX + 'fill.brush' ? 65536 : 4096);
  return Object.freeze({ OPERATIONS, admitted, requiresRevision: admitted, payload, result, correlate, paths: own.paths, compose, commandOf, members, kinds, bytes, requestByteLimit });
}));
