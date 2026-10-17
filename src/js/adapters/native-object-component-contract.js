/* L04-N closed declarations of the native component and duplicator setup operations (convert, expose, place;
 * duplicator set, object pool, path). Pure validation with the interface of the font contract (admitted,
 * requiresRevision, payload, result, correlate, paths); `compose(base)` joins another contract of that interface.
 * The common v2 SDK injects it through that one seam. No owner, no writer, no host: the native object history
 * stays the only authority and repeats every rule on the original bytes. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectComponentContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const PREFIX = 'command.document.object.';
  // Owner bounds of the wire payloads (object_component_convert_wire.rs): identities are non-empty, at most 128
  // UTF-8 bytes and carry no NUL; layer lists hold 1..256 identities; a seed is a native u32.
  const MAX_ID = 128, MAX_LAYERS = 256, MAX_SEED = 4294967295;
  const EXPOSED_KEY_PREFIX = 'ep_';
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const fail = (message) => { throw new TypeError(message); };
  const bytes = (text) => (typeof TextEncoder === 'function' ? new TextEncoder().encode(text).length : unescape(encodeURIComponent(text)).length);

  function plain(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value) &&
      (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  }
  function exact(value, required, optional, label) {
    if (!plain(value)) fail(`${label} must be a plain object`);
    const allowed = new Set([...required, ...optional]);
    if (!required.every((key) => has(value, key)) || Object.keys(value).some((key) => !allowed.has(key))) fail(`${label} has missing or unknown fields`);
    return value;
  }
  // Rust `id_ok`: non-empty, at most 128 UTF-8 bytes, no NUL.
  function id(value, label) {
    if (typeof value !== 'string' || !value.length || bytes(value) > MAX_ID || value.includes('\u0000')) fail(`${label} is not a bounded identity`);
    return value;
  }
  const KINDS = {
    // Any string: a name or label the owner stores verbatim (the 4096-byte envelope bounds it).
    text: (value, at) => { if (typeof value !== 'string') fail(`${at} must be a string`); },
    id: id,
    ids: (value, at) => {
      if (!Array.isArray(value) || value.length < 1 || value.length > MAX_LAYERS) fail(`${at} has an invalid length`);
      value.forEach((item, index) => id(item, `${at}[${index}]`));
    },
    // The record admission refuses a key that is not "ep_" plus at least one more byte (object_component_records.rs).
    exposedKey: (value, at) => {
      id(value, at);
      if (!value.startsWith(EXPOSED_KEY_PREFIX) || bytes(value) <= EXPOSED_KEY_PREFIX.length) fail(`${at} is not an exposed property key`);
    },
    number: (value, at) => { if (typeof value !== 'number' || !Number.isFinite(value)) fail(`${at} must be a finite number`); },
    boolean: (value, at) => { if (typeof value !== 'boolean') fail(`${at} must be a boolean`); },
    // Wire shape is a native u32; the owner additionally refuses a seed above 999999 when it enables a duplicator.
    seed: (value, at) => { if (!Number.isSafeInteger(value) || value < 0 || value > MAX_SEED) fail(`${at} must be a native u32 seed`); },
    // null clears the path layer; a string names one. Same bound as the owner (at most 128 bytes, no other rule).
    pathUid: (value, at) => {
      if (value === null) return;
      if (typeof value !== 'string' || bytes(value) > MAX_ID) fail(`${at} must be null or a bounded identity`);
    },
  };
  // operation suffix -> [command, receipt kind, [[wire member, kind, optional]...]]. Member order is the wire order.
  // Receipt kinds: 'command' = {applied, historyEntriesAdded}; 'created' = a new layer was minted (applied, one history entry,
  // layerUid); 'either' = a merge convert reports its new instance layer, a single-layer convert reports the compact result.
  const SPEC = {
    'component.convert': ['object.component.convert', 'either', [['contextId', 'text'], ['layerUids', 'ids']]],
    'component.expose': ['object.component.expose', 'command', [['symbolId', 'id'], ['key', 'exposedKey'], ['targetStrokeId', 'id'],
      ['targetField', 'id'], ['label', 'text'], ['default', 'number', true]]],
    'component.place': ['object.component.place', 'created', [['symbolId', 'id'], ['name', 'text']]],
    'duplicator.set': ['object.duplicator.set', 'command', [['layerUid', 'id'], ['enabled', 'boolean'], ['seed', 'seed', true]]],
    'duplicator.objects': ['object.duplicator.objects', 'created', [['layerUids', 'ids'], ['seed', 'seed']]],
    'duplicator.path': ['object.duplicator.path', 'command', [['layerUid', 'id'], ['pathLayerUid', 'pathUid']]],
  };
  const OPERATIONS = Object.freeze(Object.keys(SPEC).map((suffix) => PREFIX + suffix));
  const spec = (operation) => (typeof operation === 'string' && operation.startsWith(PREFIX) && has(SPEC, operation.slice(PREFIX.length)) ? SPEC[operation.slice(PREFIX.length)] : null);
  const admitted = (operation) => spec(operation) !== null;

  function payload(operation, value) {
    const entry = spec(operation);
    if (!entry) fail('native component operation is not declared');
    const required = ['command'].concat(entry[2].filter((member) => !member[2]).map((member) => member[0]));
    const optional = entry[2].filter((member) => member[2]).map((member) => member[0]);
    exact(value, required, optional, operation);
    if (value.command !== entry[0]) fail(`${operation} command is invalid`);
    for (const [member, kind, isOptional] of entry[2]) {
      if (isOptional && !has(value, member)) continue;
      KINDS[kind](value[member], `${operation}.${member}`);
    }
  }
  function compact(value, label) {
    exact(value, ['applied', 'historyEntriesAdded'], [], label);
    if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail(`invalid ${label} disposition`);
  }
  function created(value, label) {
    exact(value, ['applied', 'historyEntriesAdded', 'layerUid'], [], label);
    if (value.applied !== true || value.historyEntriesAdded !== 1) fail(`invalid ${label} disposition`);
    if (typeof value.layerUid !== 'string' || !value.layerUid.length) fail(`${label} names no layer`);
  }
  function result(operation, value) {
    const entry = spec(operation);
    if (!entry) fail('native component operation is not declared');
    if (entry[1] === 'created') return created(value, 'component setup result');
    if (entry[1] === 'command') return compact(value, 'component setup result');
    if (plain(value) && has(value, 'layerUid')) return created(value, 'component convert result');
    return compact(value, 'component convert result');
  }
  function correlate(request, response) {
    if (response.instanceId !== request.instanceId) fail('component response owner mismatch');
    const replaced = response.ok === false && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('component response document mismatch');
    if (!response.ok) return;
    if (response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) fail('component receipt revision mismatch');
  }
  // Every opaque authored string may legitimately start with data: or blob:; the SDK only inspects it as text.
  const OPAQUE = Object.freeze({ has: (at) => /^payload\./.test(at) || at === 'result.layerUid' });
  const own = Object.freeze({ admitted, requiresRevision: admitted, payload, result, correlate, paths: (operation) => (admitted(operation) ? OPAQUE : null) });

  // Same interface as the font contract, so one injected seam serves both.
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
  function commandOf(operation) { const entry = spec(operation); return entry ? entry[0] : null; }
  function receiptOf(operation) { const entry = spec(operation); return entry ? entry[1] : null; }
  function members(operation) { const entry = spec(operation); return entry ? entry[2].map((m) => m[0]) : null; }
  function optionalMembers(operation) { const entry = spec(operation); return entry ? entry[2].filter((m) => m[2]).map((m) => m[0]) : null; }
  function kinds(operation) { const entry = spec(operation); return entry ? entry[2].map((m) => [m[0], m[1], m[2] === true]) : null; }
  return Object.freeze({
    OPERATIONS, PREFIX, MAX_ID, MAX_LAYERS, MAX_SEED, EXPOSED_KEY_PREFIX,
    admitted, requiresRevision: admitted, payload, result, correlate, paths: own.paths, compose,
    commandOf, receiptOf, members, optionalMembers, kinds, bytes,
  });
}));
