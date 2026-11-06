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
  const DUP_MODES = ['grid', 'radial', 'path'], DUP_DIRECTIONS = ['forward', 'backward', 'centerOut', 'random'];
  const DUP_CHANNELS = ['position', 'rotation', 'scale', 'opacity', 'hue'];
  const INSTANCE_MODES = ['loop', 'once', 'pingpong', 'single'], MAX_FRAME = 4294967295, MAX_PLACED = 2147483647;
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
  const STATE_KINDS = ['setCameraKey', 'removeCameraKey', 'setCameraEase', 'clearCamera', 'addMarker', 'updateMarker', 'removeMarker', 'clearMarkers'];
  const MAX_LABEL = 256;
  const numbers = (value, count, at) => {
    if (!Array.isArray(value) || value.length !== count || !value.every((n) => typeof n === 'number' && Number.isFinite(n))) fail(`${at} must be ${count} finite numbers`);
  };
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
    // #2256 Q4: the duplicator inspector's closed members (mode label, per-channel random flags, temporal stagger).
    dupMode: (value, at) => { if (!DUP_MODES.includes(value)) fail(`${at} is not a duplicator mode`); },
    dupStagger: (value, at) => {
      exact(value, [], DUP_CHANNELS, at);
      DUP_CHANNELS.forEach((key) => { if (has(value, key) && typeof value[key] !== 'boolean') fail(`${at}.${key} must be a boolean`); });
    },
    // #2256 Q5: the Component Instance panel's closed members (playback label, native u32 frame, native i32 offset, positive speed).
    instMode: (value, at) => { if (!INSTANCE_MODES.includes(value)) fail(`${at} is not a playback mode`); },
    frame: (value, at) => { if (!Number.isSafeInteger(value) || value < 0 || value > MAX_FRAME) fail(`${at} must be a native u32 frame`); },
    placedAt: (value, at) => { if (!Number.isSafeInteger(value) || value < -MAX_PLACED - 1 || value > MAX_PLACED) fail(`${at} must be a native i32 offset`); },
    speed: (value, at) => { if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) fail(`${at} must be a finite number above zero`); },
    // C-comp-owned-state: the closed members of the component camera / marker edit.
    stateKind: (value, at) => { if (!STATE_KINDS.includes(value)) fail(`${at} is not a component state edit kind`); },
    pair: (value, at) => numbers(value, 2, at),
    quad: (value, at) => { numbers(value, 4, at); if (!(value[0] >= 0 && value[0] <= 1 && value[2] >= 0 && value[2] <= 1)) fail(`${at} x values must lie in 0..1`); },
    label: (value, at) => { if (typeof value !== 'string' || bytes(value) > MAX_LABEL || value.includes('\u0000')) fail(`${at} must be a string of at most 256 bytes`); },
    colour: (value, at) => { KINDS.label(value, at); if (!value.length) fail(`${at} must not be empty`); },
    // The Motion canvas effector handle: the inline effector at a native u32 index takes a finite world anchor.
    effectorPos: (value, at) => {
      exact(value, ['index', 'x', 'y'], [], at);
      if (!Number.isSafeInteger(value.index) || value.index < 0 || value.index > MAX_FRAME) fail(`${at}.index must be a native u32 index`);
      KINDS.number(value.x, `${at}.x`); KINDS.number(value.y, `${at}.y`);
    },
    dupTime: (value, at) => {
      exact(value, [], ['enabled', 'offsetFrames', 'direction'], at);
      if (has(value, 'enabled') && typeof value.enabled !== 'boolean') fail(`${at}.enabled must be a boolean`);
      if (has(value, 'offsetFrames')) KINDS.number(value.offsetFrames, `${at}.offsetFrames`);
      if (has(value, 'direction') && !DUP_DIRECTIONS.includes(value.direction)) fail(`${at}.direction is not a stagger direction`);
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
    // #2256 Q4: the inspector's closed partial edit of the stored descriptor (the path layer keeps duplicator.path).
    'duplicator.config': ['object.duplicator.config', 'command', [['layerUid', 'id'], ['mode', 'dupMode', true], ['rows', 'number', true], ['cols', 'number', true],
      ['spacingX', 'number', true], ['spacingY', 'number', true], ['count', 'number', true], ['radius', 'number', true], ['startAngle', 'number', true],
      ['radialOrient', 'boolean', true], ['pathAlignTangent', 'boolean', true], ['seed', 'seed', true], ['staggerRandom', 'dupStagger', true], ['timeOffset', 'dupTime', true], ['effectorPos', 'effectorPos', true]]],
    // #2256 Q5: the Component Instance panel's closed partial edit of the stored playback record.
    'component.instance.set': ['object.component.instance.set', 'command', [['layerUid', 'id'], ['mode', 'instMode', true], ['singleFrame', 'frame', true],
      ['placedAt', 'placedAt', true], ['speed', 'speed', true], ['atFrame', 'frame', true]]],
    // D-comp-break-apart: the inverse of convert (the baseline convertComponentToLayer), one instance layer baked back into an ordinary layer.
    'component.breakApart': ['object.component.breakApart', 'command', [['contextId', 'text'], ['layerUid', 'id']]],
    // C-comp-owned-state: the camera keys and markers a declared component owns, one closed edit per request (contextId = the component).
    'component.state.edit': ['object.component.state.edit', 'command', [['contextId', 'id'], ['kind', 'stateKind'], ['frame', 'frame', true], ['x', 'number', true],
      ['y', 'number', true], ['w', 'speed', true], ['rot', 'number', true], ['hOut', 'pair', true], ['hIn', 'pair', true], ['ease', 'quad', true],
      ['name', 'label', true], ['color', 'colour', true], ['newFrame', 'frame', true]]],
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
    if (request.operation === PREFIX + 'component.convert') {
      payload(request.operation, request.payload);
      const merged = new Set(request.payload.layerUids).size > 1;
      if (response.result.applied !== true || has(response.result, 'layerUid') !== merged) fail('convert receipt does not match selected layer count');
    }
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
