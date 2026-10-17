/* Public v2 contract of the lane-3 object operations: image mesh edit, widget create / write / settings, rig edit and
 * rig pose. Pure validation with the interface of the font contract (admitted, requiresRevision, payload, result,
 * correlate, paths); `compose(base)` joins another contract of that interface. The shapes below are GENERATED from
 * one table (tools/spec.js) that the Rust validator (nemo-mcp native_object_rig_contract.rs), the transport schema
 * and the capability descriptors are generated from too. No owner, no writer, no host: the native owner stays the
 * only authority and repeats every rule on the original bytes. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectRigContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const PREFIX = 'command.document.object.';
  const SPEC = {
    "image.mesh.edit": {"command":"object.image.mesh.edit","result":"command","node":{"t":"obj","req":{"meshId":{"t":"str","min":1,"max":128},"action":{"t":"union","by":"kind","cases":{"setOffset":{"req":{"index":{"t":"int","min":0,"max":4294967295},"du":{"t":"num","abs":1000000000},"dv":{"t":"num","abs":1000000000}}},"resetOffsets":{"req":{}},"setOutline":{"req":{"outline":{"t":"arr","of":{"t":"pair","abs":4},"min":3,"max":512,"perimeter":16}},"opt":{"cols":{"t":"int","min":1,"max":64},"rows":{"t":"int","min":1,"max":64}}},"setDensity":{"req":{"cols":{"t":"int","min":1,"max":64},"rows":{"t":"int","min":1,"max":64}}}}}}}},
    "widget.create": {"command":"object.widget.create","result":"created","node":{"t":"obj","req":{"kind":{"t":"enum","v":["joystick","slider"]},"name":{"t":"str","min":0},"pos":{"t":"pair","abs":1000000000}}}},
    "widget.write": {"command":"object.widget.write","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1},"frame":{"t":"num"},"to":{"t":"union","by":"kind","cases":{"local":{"req":{"point":{"t":"pair","abs":1000000000}}},"world":{"req":{"point":{"t":"pair","abs":1000000000},"anchor":{"t":"pair","abs":1000000000},"angleDegrees":{"t":"num","abs":1000000000}}},"rest":{"req":{}}}}}}},
    "widget.settings": {"command":"object.widget.settings","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1},"set":{"t":"obj","opt":{"pos":{"t":"pair","abs":1000000000},"size":{"t":"pair","abs":1000000000},"x":{"t":"obj","opt":{"min":{"t":"num","abs":1000000000},"max":{"t":"num","abs":1000000000},"rest":{"t":"num","abs":1000000000},"label":{"t":"str","min":0}},"minMembers":1},"y":{"t":"obj","opt":{"min":{"t":"num","abs":1000000000},"max":{"t":"num","abs":1000000000},"rest":{"t":"num","abs":1000000000},"label":{"t":"str","min":0}},"minMembers":1}},"minMembers":1}}}},
    "rig.edit": {"command":"object.rig.edit","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1,"max":128},"action":{"t":"union","by":"kind","cases":{"drawBone":{"req":{"segments":{"t":"arr","of":{"t":"obj","req":{"point":{"t":"pair","abs":1000000000},"handleIn":{"t":"pair","abs":1000000000},"handleOut":{"t":"pair","abs":1000000000}}},"min":2,"max":256},"closed":{"t":"bool"}},"opt":{"boneId":{"t":"str","min":1,"max":128}}},"moveSegment":{"req":{"boneId":{"t":"str","min":1,"max":128},"vi":{"t":"int","min":0,"max":4294967295},"part":{"t":"enum","v":["point","handleIn","handleOut"]},"to":{"t":"pair","abs":1000000000}},"opt":{"breakTangent":{"t":"bool"}}},"setBoneRadius":{"req":{"boneId":{"t":"str","min":1,"max":128},"radius":{"t":"num","abs":1000000000}}},"bindStroke":{"req":{"strokeId":{"t":"str","min":1,"max":128},"frame":{"t":"int","min":0,"max":4294967295}},"opt":{"boneIds":{"t":"arr","of":{"t":"str","min":1,"max":128},"min":0},"radius":{"t":"num","min":0,"abs":1000000000},"rotate":{"t":"bool"},"softness":{"t":"num","abs":1000000000}}},"bindMesh":{"req":{"meshId":{"t":"str","min":1,"max":128}}},"autoAssign":{"req":{"frame":{"t":"int","min":0,"max":4294967295}},"opt":{"defaultRadius":{"t":"num","min":0,"abs":1000000000},"rotate":{"t":"bool"},"softness":{"t":"num","abs":1000000000},"paths":{"t":"bool"},"meshes":{"t":"bool"}}},"setWeight":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]},"vertex":{"t":"int","min":0,"max":4294967295},"entry":{"t":"int","min":0,"max":4294967295},"percent":{"t":"num","abs":1000000000}}},"addWeight":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]},"vertex":{"t":"int","min":0,"max":4294967295},"boneId":{"t":"str","min":1,"max":128}}},"removeWeight":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]},"vertex":{"t":"int","min":0,"max":4294967295},"entry":{"t":"int","min":0,"max":4294967295}}},"resetWeights":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]},"vertex":{"t":"int","min":0,"max":4294967295}},"opt":{"radius":{"t":"num","min":0,"abs":1000000000},"softness":{"t":"num","min":0,"max":1}}},"setIk":{"req":{"root":{"t":"obj","req":{"boneId":{"t":"str","min":1,"max":128},"vi":{"t":"int","min":0,"max":4294967295}}},"joint":{"t":"obj","req":{"boneId":{"t":"str","min":1,"max":128},"vi":{"t":"int","min":0,"max":4294967295}}},"end":{"t":"obj","req":{"boneId":{"t":"str","min":1,"max":128},"vi":{"t":"int","min":0,"max":4294967295}}}},"opt":{"flip":{"t":"bool"}}},"removeIk":{"req":{"endKey":{"t":"str","min":1,"max":160}}},"removeBone":{"req":{"boneId":{"t":"str","min":1,"max":128}}},"removeBind":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]}}}}}}}},
    "rig.pose": {"command":"object.rig.pose","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1,"max":128},"action":{"t":"union","by":"kind","cases":{"reset":{"req":{}},"commit":{"req":{"frame":{"t":"int","min":0,"max":4294967295}}}}}}}}
  };
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const fail = (message) => { throw new TypeError(message); };
  const bytes = (text) => (typeof TextEncoder === 'function' ? new TextEncoder().encode(text).length : unescape(encodeURIComponent(text)).length);
  const plain = (v) => v !== null && typeof v === 'object' && !Array.isArray(v) && (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null);

  function number(n, v, at) {
    if (typeof v !== 'number' || !Number.isFinite(v)) fail(`${at} must be a finite number`);
    if (n.abs !== undefined && Math.abs(v) > n.abs) fail(`${at} exceeds the magnitude bound`);
    if ((n.min !== undefined && v < n.min) || (n.max !== undefined && v > n.max)) fail(`${at} is out of range`);
  }
  // Closed object: `tag` is the [member, value] pair a union case adds as a required constant member.
  function object(n, v, at, tag) {
    if (!plain(v)) fail(`${at} must be a plain object`);
    const req = n.req || {}, opt = n.opt || {}, keys = Object.keys(v);
    for (const key of keys) if (!(has(req, key) || has(opt, key) || (tag && key === tag[0]))) fail(`${at}.${key} is not declared`);
    for (const key of Object.keys(req)) if (!has(v, key)) fail(`${at}.${key} is missing`);
    if (tag && v[tag[0]] !== tag[1]) fail(`${at}.${tag[0]} is missing`);
    if (n.minMembers && keys.length < n.minMembers) fail(`${at} carries no member`);
    if (n.xor && keys.filter((key) => n.xor.includes(key)).length !== 1) fail(`${at} must name exactly one of ${n.xor.join(', ')}`);
    for (const key of keys) if (!(tag && key === tag[0])) check(has(req, key) ? req[key] : opt[key], v[key], `${at}.${key}`);
  }
  function perimeter(points) {
    let sum = 0;
    points.forEach((p, i) => { const q = points[(i + 1) % points.length]; sum += Math.hypot(q[0] - p[0], q[1] - p[1]); });
    return sum;
  }
  function check(n, v, at) {
    switch (n.t) {
      case 'obj': return object(n, v, at, null);
      case 'union': {
        if (!plain(v) || typeof v[n.by] !== 'string' || !has(n.cases, v[n.by])) fail(`${at}.${n.by} is not admitted`);
        return object(n.cases[v[n.by]], v, at, [n.by, v[n.by]]);
      }
      case 'str':
        if (typeof v !== 'string' || bytes(v) < (n.min || 0) || (n.max !== undefined && bytes(v) > n.max)) fail(`${at} is not a bounded string`);
        return undefined;
      case 'int':
        if (!Number.isSafeInteger(v) || v < n.min || v > n.max) fail(`${at} must be an integer in ${n.min}..${n.max}`);
        return undefined;
      case 'num': return number(n, v, at);
      case 'bool':
        if (typeof v !== 'boolean') fail(`${at} must be a boolean`);
        return undefined;
      case 'enum':
        if (typeof v !== 'string' || !n.v.includes(v)) fail(`${at} is not admitted`);
        return undefined;
      case 'arr': {
        if (!Array.isArray(v) || v.length < (n.min || 0) || (n.max !== undefined && v.length > n.max)) fail(`${at} has an invalid length`);
        v.forEach((item, i) => check(n.of, item, `${at}[${i}]`));
        if (n.perimeter !== undefined && perimeter(v) > n.perimeter) fail(`${at} perimeter exceeds ${n.perimeter}`);
        return undefined;
      }
      case 'pair':
        if (!Array.isArray(v) || v.length !== 2) fail(`${at} must be a pair`);
        v.forEach((item, i) => number(n, item, `${at}[${i}]`));
        return undefined;
      default: return fail('unknown shape node');
    }
  }

  const OPERATIONS = Object.freeze(Object.keys(SPEC).map((suffix) => PREFIX + suffix));
  const spec = (operation) => (typeof operation === 'string' && operation.startsWith(PREFIX) && has(SPEC, operation.slice(PREFIX.length)) ? SPEC[operation.slice(PREFIX.length)] : null);
  const admitted = (operation) => spec(operation) !== null;

  function payload(operation, value) {
    const entry = spec(operation);
    if (!entry) fail('native object operation is not declared');
    if (!plain(value) || value.command !== entry.command) fail(`${operation} command is invalid`);
    const body = Object.assign({}, value);
    delete body.command;
    check(entry.node, body, operation);
  }
  function result(operation, value) {
    const entry = spec(operation);
    if (!entry) fail('native object operation is not declared');
    if (!plain(value) || typeof value.applied !== 'boolean') fail('invalid object edit result');
    const keys = Object.keys(value).sort().join(',');
    if (entry.result === 'created') {
      if (keys !== 'applied,historyEntriesAdded,layerUid' || value.applied !== true || value.historyEntriesAdded !== 1) fail('invalid widget creation disposition');
      if (typeof value.layerUid !== 'string' || !value.layerUid.length) fail('layerUid must be a non-empty string');
      return;
    }
    if (keys !== 'applied,historyEntriesAdded' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail('invalid object edit disposition');
  }
  function correlate(request, response) {
    if (response.instanceId !== request.instanceId) fail('object edit response owner mismatch');
    const replaced = response.ok === false && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('object edit response document mismatch');
    if (!response.ok) return;
    if (response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) fail('object edit receipt revision mismatch');
  }
  // Authored identities may legitimately start with data: or blob:; the SDK only inspects them as text.
  const OPAQUE = Object.freeze({ has: (at) => /^payload\./.test(at) || at === 'result.layerUid' });
  const own = Object.freeze({ admitted, requiresRevision: admitted, payload, result, correlate, paths: (operation) => (admitted(operation) ? OPAQUE : null) });

  function compose(base) {
    if (!base) return own;
    return Object.freeze({
      admitted: (op) => admitted(op) || base.admitted(op),
      requiresRevision: (op) => admitted(op) || base.requiresRevision(op),
      payload: (op, value, envelope) => (admitted(op) ? payload(op, value) : base.payload(op, value, envelope)),
      result: (op, value) => (admitted(op) ? result(op, value) : base.result(op, value)),
      correlate: (request, response) => (admitted(request.operation) ? correlate(request, response) : base.correlate(request, response)),
      paths: (op) => (admitted(op) ? OPAQUE : base.paths(op)),
    });
  }
  const commandOf = (operation) => { const entry = spec(operation); return entry ? entry.command : null; };
  return Object.freeze({ OPERATIONS, SPEC, admitted, requiresRevision: admitted, payload, result, correlate, paths: own.paths, compose, commandOf, bytes,
    MESH_EDIT: PREFIX + 'image.mesh.edit', WIDGET_CREATE: PREFIX + 'widget.create', WIDGET_WRITE: PREFIX + 'widget.write',
    WIDGET_SETTINGS: PREFIX + 'widget.settings', RIG_EDIT: PREFIX + 'rig.edit', RIG_POSE: PREFIX + 'rig.pose' });
}));
