/* L01-J public v2 contract of the native vector operations (G14 vector create/replace/delete, G22 parameter edit,
 * G13 path-effect stack) and of the `editable-vector-path` object record the read path returns. Pure validation: no
 * state, no transport, no document writer. Joined by the common SDK through compose(); the native owner stays the
 * only authority and repeats every rule (strict original-byte admission, codec, lock and path-effect guards).
 * Mirrors native-engine object_vector_record/codec/ext_codec and the L01-F/L01-G payload tables. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectVectorContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const VECTOR = 'command.document.object.vector.edit';
  const PARAMETRIC = 'command.document.object.parametric.edit';
  const EFFECTS = 'command.document.object.path.effects.edit';
  const OPS = new Set([VECTOR, PARAMETRIC, EFFECTS]);
  const FAMILY = 'editable-vector-path';
  // Full encoded request envelopes: vector edits may carry a complete 256-anchor record;
  // every other operation keeps the default bound. Replies remain bounded separately.
  const MAX_REQUEST_BYTES = 4096, ENGINE_VECTOR_REQUEST_BYTES = 65536;
  const requestByteLimit = (operation) => operation === VECTOR ? ENGINE_VECTOR_REQUEST_BYTES : MAX_REQUEST_BYTES;
  const MAX_ANCHORS = 256, MAX_FRAME = 4294967295;
  const CAPS = ['butt', 'round', 'square'], JOINS = ['miter', 'round', 'bevel'], ORDERS = ['fillFirst', 'strokeFirst'];
  const PROFILES = ['taper-in', 'taper-out', 'taper-both', 'bulge', 'even'];
  const FX_TYPES = ['zigzag', 'roughen'];
  // paramShape members by kind (object_parametric_record.rs); `box` belongs to every kind.
  const SHAPE_MEMBERS = { rect: ['tl', 'tr', 'br', 'bl'], ellipse: ['startAngle', 'sweep', 'innerRadius'], star: ['pointCount', 'innerRatio', 'cornerRadius'] };
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const fail = (message) => { throw new TypeError(message); };

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
  function num(value, label) { if (typeof value !== 'number' || !Number.isFinite(value)) fail(`${label} must be a finite number`); return value; }
  function nonNegative(value, label) { if (num(value, label) < 0) fail(`${label} must not be negative`); return value; }
  function unit(value, label) { const v = num(value, label); if (v < 0 || v > 1) fail(`${label} must be within 0..1`); return v; }
  function oneOf(value, list, label) { if (!list.includes(value)) fail(`${label} is not admitted`); return value; }
  function opaque(value, label) { if (typeof value !== 'string' || !value.length) fail(`${label} must be a non-empty string`); return value; }
  function frame(value, label) { if (!Number.isSafeInteger(value) || value < 0 || value > MAX_FRAME) fail(`${label} must be a native u32 frame`); return value; }
  function pair(value, label) {
    if (!Array.isArray(value) || value.length !== 2) fail(`${label} must be an [x, y] pair`);
    value.forEach((part, at) => num(part, `${label}[${at}]`));
  }

  function target(value, label, create) {
    exact(value, ['contextId', 'frameScope', 'layerUid', 'strokeId'], [], label);
    if (value.contextId !== 'scene-root') fail(`${label}.contextId is not admitted`);
    exact(value.frameScope, ['kind', 'frame'], [], `${label}.frameScope`);
    oneOf(value.frameScope.kind, ['authored', 'reference'], `${label}.frameScope.kind`);
    frame(value.frameScope.frame, `${label}.frameScope.frame`);
    opaque(value.layerUid, `${label}.layerUid`);
    // A created record names no stroke id: the owner allocates it and the caller cannot choose it.
    if (create === true) { if (value.strokeId !== '') fail(`${label}.strokeId must be empty for a create`); } else opaque(value.strokeId, `${label}.strokeId`);
  }
  function point(value, label) { exact(value, ['x', 'y'], [], label); num(value.x, `${label}.x`); num(value.y, `${label}.y`); }
  function solid(value, label) {
    exact(value, ['kind', 'r', 'g', 'b', 'a'], [], label);
    if (value.kind !== 'solid') fail(`${label}.kind must be solid`);
    ['r', 'g', 'b', 'a'].forEach((key) => unit(value[key], `${label}.${key}`));
  }
  function geometry(value, label) {
    exact(value, ['kind', 'closed', 'coordinateSpace', 'handleSpace', 'segments'], [], label);
    if (value.kind !== 'cubic-path' || value.coordinateSpace !== 'document-world' || value.handleSpace !== 'relative') fail(`${label} is not admitted`);
    if (typeof value.closed !== 'boolean') fail(`${label}.closed must be a boolean`);
    if (!Array.isArray(value.segments) || value.segments.length < 2 || value.segments.length > MAX_ANCHORS) fail(`${label}.segments needs 2..${MAX_ANCHORS} anchors`);
    value.segments.forEach((segment, at) => {
      exact(segment, ['point', 'handleIn', 'handleOut'], [], `${label}.segments[${at}]`);
      ['point', 'handleIn', 'handleOut'].forEach((key) => point(segment[key], `${label}.segments[${at}].${key}`));
    });
  }
  function body(value, label) {
    exact(value, ['stroke', 'hasRealStroke', 'strokeWidth', 'strokeCap', 'strokeJoin', 'opacity'], ['miterLimit', 'dashArray', 'dashOffset', 'paintOrder'], label);
    if (value.stroke !== null) solid(value.stroke, `${label}.stroke`);
    if (typeof value.hasRealStroke !== 'boolean') fail(`${label}.hasRealStroke must be a boolean`);
    // hasRealStroke is authoritative and can never disagree with the stroke slot (a fallback colour is not a stroke).
    if ((value.stroke !== null) !== value.hasRealStroke) fail(`${label}.hasRealStroke must be true exactly when a stroke is present`);
    nonNegative(value.strokeWidth, `${label}.strokeWidth`);
    oneOf(value.strokeCap, CAPS, `${label}.strokeCap`); oneOf(value.strokeJoin, JOINS, `${label}.strokeJoin`);
    if (has(value, 'miterLimit')) nonNegative(value.miterLimit, `${label}.miterLimit`);
    if (has(value, 'dashArray')) {
      const dashes = value.dashArray;
      if (!Array.isArray(dashes) || !dashes.length) fail(`${label}.dashArray must be a non-empty array`);
      let total = 0;
      dashes.forEach((dash, at) => { total += nonNegative(dash, `${label}.dashArray[${at}]`); });
      if (!(total > 0 && Number.isFinite(total))) fail(`${label}.dashArray needs a positive finite length`);
    }
    if (has(value, 'dashOffset')) num(value.dashOffset, `${label}.dashOffset`);
    if (has(value, 'paintOrder')) oneOf(value.paintOrder, ORDERS, `${label}.paintOrder`);
    unit(value.opacity, `${label}.opacity`);
  }
  function pressure(value, label) {
    exact(value, [], ['centerSegments', 'widthProfile', 'strokeProfile', 'profileBase'], label);
    if (has(value, 'centerSegments')) {
      if (!Array.isArray(value.centerSegments)) fail(`${label}.centerSegments must be an array`);
      value.centerSegments.forEach((anchor, at) => {
        const where = `${label}.centerSegments[${at}]`;
        exact(anchor, ['point', 'handleIn', 'handleOut', 'width'], [], where);
        ['point', 'handleIn', 'handleOut'].forEach((key) => point(anchor[key], `${where}.${key}`));
        num(anchor.width, `${where}.width`);
      });
    }
    if (has(value, 'widthProfile')) {
      if (!Array.isArray(value.widthProfile)) fail(`${label}.widthProfile must be an array`);
      let previous = 0;
      value.widthProfile.forEach((stop, at) => {
        const where = `${label}.widthProfile[${at}]`;
        exact(stop, ['t', 'width'], [], where);
        const t = unit(stop.t, `${where}.t`);
        if (t < previous) fail(`${where}.t must not decrease`);
        previous = t;
        num(stop.width, `${where}.width`);
      });
    }
    if (has(value, 'strokeProfile')) oneOf(value.strokeProfile, PROFILES, `${label}.strokeProfile`);
    if (has(value, 'profileBase')) nonNegative(value.profileBase, `${label}.profileBase`);
  }
  function shapeBody(value, label, forSet) {
    if (!plain(value) || !has(value, 'kind')) fail(`${label} must be an object with a kind`);
    const members = SHAPE_MEMBERS[value.kind];
    if (!members) fail(`${label}.kind must be rect, ellipse or star`);
    exact(value, ['kind'], forSet ? members : members.concat(['box']), label);
    members.forEach((key) => { if (has(value, key)) num(value[key], `${label}.${key}`); });
    if (has(value, 'box')) { exact(value.box, ['x1', 'y1', 'x2', 'y2'], [], `${label}.box`); ['x1', 'y1', 'x2', 'y2'].forEach((key) => num(value.box[key], `${label}.box.${key}`)); }
  }
  function gradient(value, label) {
    exact(value, ['kind','from','to','stops'], [], label);
    oneOf(value.kind, ['linear','radial'], label + '.kind');
    for (const key of ['from','to']) {
      if (!Array.isArray(value[key]) || value[key].length !== 2) fail(label + '.' + key + ' must be a pair');
      for (let i=0;i<2;i++) { num(value[key][i], label); if (Math.abs(value[key][i]) > 3.4028234663852886e38) fail(label + ' is not representable'); }
    }
    if (!Number.isFinite(Math.fround(Math.hypot(value.to[0]-value.from[0],value.to[1]-value.from[1])))) fail(label + ' extent is not representable');
    if (!Array.isArray(value.stops) || value.stops.length !== 2) fail(label + ' requires two stops');
    for (let i=0;i<2;i++) {
      const stop=value.stops[i]; exact(stop, ['offset','color'], [], label);
      num(stop.offset, label); if (Math.abs(stop.offset) > 3.4028234663852886e38 || typeof stop.color !== 'string' || !/^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(stop.color)) fail(label + ' invalid stop');
    }
  }
  function ext(value, label, hasStroke) {
    exact(value, [], ['pressure', 'paramShape', 'boxAngle', 'fillGradient', 'fillTrace', 'isMask', 'maskMode', 'maskFeather', 'linkedFillId', 'isLinkedFillCompanion'], label);
    if (!Object.keys(value).length) fail(`${label} must be omitted when empty`);
    if (has(value, 'pressure')) {
      pressure(value.pressure, `${label}.pressure`);
      if (hasStroke) fail(`${label}.pressure: a pressure stroke is painted through its fill and has no real stroke`);
    }
    if (has(value, 'paramShape')) shapeBody(value.paramShape, `${label}.paramShape`, false);
    if (has(value, 'boxAngle')) num(value.boxAngle, `${label}.boxAngle`);
    if (has(value, 'linkedFillId') && (typeof value.linkedFillId !== 'string' || !value.linkedFillId.length || bytes(value.linkedFillId) > 128 || /[\u0000-\u001f\u007f-\u009f]/.test(value.linkedFillId))) fail(`${label}.linkedFillId must be a bounded identity`);
    if (has(value, 'isLinkedFillCompanion') && typeof value.isLinkedFillCompanion !== 'boolean') fail(`${label}.isLinkedFillCompanion must be a boolean`);
    if (value.isLinkedFillCompanion === true && (!has(value, 'linkedFillId') || has(value, 'pressure'))) fail(`${label} has an invalid linked-fill companion`);
    if (has(value, 'linkedFillId') && value.isLinkedFillCompanion !== true && (!value.pressure || !Array.isArray(value.pressure.centerSegments) || value.pressure.centerSegments.length < 2)) fail(`${label} linked-fill primary requires its centreline`);
    if (has(value, 'isMask') && value.isMask !== null && typeof value.isMask !== 'boolean') fail(`${label}.isMask must be a boolean or null`);
    if (has(value, 'maskMode') && value.maskMode !== null && typeof value.maskMode !== 'string') fail(`${label}.maskMode must be a string or null`);
    if (has(value, 'maskFeather') && value.maskFeather !== null) num(value.maskFeather, `${label}.maskFeather`);
    if (has(value, 'fillTrace')) {
      const t=value.fillTrace; exact(t, [], ['seed','seeds','walls','gapPx'], label+'.fillTrace');
      if (!has(t,'seed')&&!has(t,'seeds')) fail(label+' fillTrace requires seed or seeds');
      const seed=(p)=>{if(!Array.isArray(p)||p.length!==2)fail(label+' fill seed requires two coordinates');p.forEach((n)=>num(n,label));};
      if(has(t,'seed'))seed(t.seed);
      if(has(t,'seeds')){if(!Array.isArray(t.seeds))fail(label+' fill seeds requires an array');t.seeds.forEach(seed);}
      if(has(t,'walls')&&(!Array.isArray(t.walls)||t.walls.some((id)=>typeof id!=='string'||!id.length)))fail(label+' invalid fill walls');
      if(has(t,'gapPx')){num(t.gapPx,label);if(t.gapPx<0)fail(label+' negative fill gap');}
    }
    if (has(value, 'fillGradient')) { gradient(value.fillGradient, label + '.fillGradient'); if (has(value, 'pressure')) fail(label + ' pressure-gradient consumer is unavailable'); }
  }
  // The complete `editable-vector-path` record (create and replace carry it; the read path returns it).
  function record(value, create) {
    exact(value, ['schemaVersion', 'family', 'target', 'geometry', 'fill', 'vector'], ['vectorExt'], 'vector record');
    if (value.schemaVersion !== 1 || value.family !== FAMILY) fail('unsupported vector record family');
    target(value.target, 'vector record target', create);
    geometry(value.geometry, 'vector record geometry');
    if (value.fill !== null) solid(value.fill, 'vector record fill');
    body(value.vector, 'vector record vector');
    if (has(value, 'vectorExt')) ext(value.vectorExt, 'vector record vectorExt', value.vector.hasRealStroke);
    if (value.vectorExt && has(value.vectorExt, 'linkedFillId') && (value.target.frameScope.kind !== 'authored' || (value.vectorExt.isLinkedFillCompanion === true && value.fill === null))) fail('linked-fill record requires an authored scope and a filled companion');
  }

  const readPaths = Object.freeze({ has: (at) =>
    /^(?:payload\.stableTarget|result\.object\.target)\.(?:layerUid|strokeId)$/.test(at) ||
    at === 'result.object.vectorExt.bitmapBrush.bitmapBrushSpec.tip' ||
    at === 'result.object.vectorExt.bitmapBrush.brushGroupId' });

  // Stored bitmap anchors are readable, but generic create/replace cannot write them.
  // Validate a detached projection with the ordinary record guard; retain the original
  // complete response, including optional-field presence and opaque authored identities.
  function readRecord(value) {
    if (!plain(value) || !plain(value.vectorExt) || !has(value.vectorExt, 'bitmapBrush')) return record(value, false);
    const fields = value.vectorExt.bitmapBrush, label = 'vector record bitmapBrush';
    exact(fields, ['bitmapBrushSpec', 'brushGroupId'], ['preTextureStroke', 'preTextureOpacity', 'bitmapPressureProfile'], label);
    const spec = fields.bitmapBrushSpec;
    exact(spec, ['tip', 'size', 'spacing', 'scatter', 'opacity', 'color', 'pressure', 'seed'], [], label + '.bitmapBrushSpec');
    const identity = (text) => typeof text === 'string' && text.length > 0 && bytes(text) <= 255 && !text.includes('\0');
    const color = (text) => typeof text === 'string' && /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(text);
    if (!identity(spec.tip) || !identity(fields.brushGroupId)) fail(label + ' has an invalid identity');
    ['size', 'spacing', 'scatter', 'opacity', 'seed'].forEach((key) => num(spec[key], label + '.' + key));
    if (!color(spec.color) || typeof spec.pressure !== 'boolean') fail(label + ' has an invalid colour or pressure flag');
    if (has(fields, 'preTextureStroke') && fields.preTextureStroke !== null && !color(fields.preTextureStroke)) fail(label + ' has an invalid remembered stroke');
    if (has(fields, 'preTextureOpacity')) num(fields.preTextureOpacity, label + '.preTextureOpacity');
    if (has(fields, 'bitmapPressureProfile')) {
      const profile = fields.bitmapPressureProfile;
      if (!Array.isArray(profile) || profile.length > 4096) fail(label + ' has an invalid pressure profile');
      profile.forEach((point) => {
        if (!Array.isArray(point) || point.length !== 3) fail(label + ' pressure points require three numbers');
        point.forEach((n) => num(n, label + '.bitmapPressureProfile'));
      });
    }
    if (has(value.vectorExt, 'linkedFillId') || value.vectorExt.isLinkedFillCompanion === true) fail(label + ' cannot also be a linked-fill pair');
    const projected = Object.assign({}, value, { vectorExt: Object.assign({}, value.vectorExt) });
    delete projected.vectorExt.bitmapBrush;
    if (!Object.keys(projected.vectorExt).length) delete projected.vectorExt;
    record(projected, false);
    if (value.fill === null ? value.vector.opacity !== 0 || !has(fields, 'preTextureOpacity')
      : value.vector.stroke !== null || !has(fields, 'preTextureStroke')) fail(label + ' camouflage is inconsistent');
  }

  function effect(value, label) {
    exact(value, ['type'], ['enabled', 'p1', 'p2', 'p3'], label);
    oneOf(value.type, FX_TYPES, `${label}.type`);
    if (has(value, 'enabled') && typeof value.enabled !== 'boolean') fail(`${label}.enabled must be a boolean`);
    ['p1', 'p2', 'p3'].forEach((key) => { if (has(value, key) && value[key] !== null) num(value[key], `${label}.${key}`); });
  }

  // command -> [operation, wire members]. Member order is the wire order.
  const COMMANDS = {
    'object.vector.create': [VECTOR, ['record']],
    'object.vector.createMany': [VECTOR, ['records']],
    'object.vector.replace': [VECTOR, ['record']],
    'object.vector.delete': [VECTOR, ['stableTarget']],
    'object.vector.mask.edit': [VECTOR, ['stableTarget', 'maskEdit']],
    'object.parametric.set': [PARAMETRIC, ['stableTarget', 'paramShape']],
    'object.parametric.translate': [PARAMETRIC, ['stableTarget', 'delta']],
    'object.parametric.scale': [PARAMETRIC, ['stableTarget', 'scale', 'anchor']],
    'object.path.effects.set': [EFFECTS, ['layerUid', 'stack']],
  };
  function member(command, key, payloadValue) {
    const label = `${command}.${key}`;
    if (key === 'record') record(payloadValue, command === 'object.vector.create');
    else if (key === 'records') {
      // G23: 2..32 creates on one layer and frame, one revision and one undo entry.
      if (!Array.isArray(payloadValue) || payloadValue.length < 2 || payloadValue.length > 32) fail(`${label} must hold 2 to 32 records`);
      payloadValue.forEach((entry) => record(entry, true));
      const first = payloadValue[0].target;
      if (payloadValue.some((entry) => entry.target.layerUid !== first.layerUid || entry.target.contextId !== first.contextId ||
        entry.target.frameScope.kind !== first.frameScope.kind || entry.target.frameScope.frame !== first.frameScope.frame)) fail(`${label} must share one layer and frame`);
    }
    else if (key === 'stableTarget') target(payloadValue, label, false);
    else if (key === 'maskEdit') {
      if (!plain(payloadValue)) fail(`${label} must be an object`);
      if (payloadValue.kind === 'mode') {
        exact(payloadValue, ['kind', 'mode'], [], label);
        oneOf(payloadValue.mode, ['add', 'subtract', 'intersect'], `${label}.mode`);
      } else if (payloadValue.kind === 'feather') {
        exact(payloadValue, ['kind', 'value'], [], label); num(payloadValue.value, `${label}.value`);
      } else {
        exact(payloadValue, ['kind'], [], label); oneOf(payloadValue.kind, ['unset'], `${label}.kind`);
      }
    }
    else if (key === 'paramShape') shapeBody(payloadValue, label, true);
    else if (key === 'delta' || key === 'scale' || key === 'anchor') pair(payloadValue, label);
    else if (key === 'layerUid') opaque(payloadValue, label);
    else if (key === 'stack') {
      if (!Array.isArray(payloadValue)) fail(`${label} must be an array`);
      payloadValue.forEach((entry, at) => effect(entry, `${label}[${at}]`));
    }
  }
  function payload(operation, value) {
    if (!OPS.has(operation)) fail('native vector operation is not declared');
    if (!plain(value) || !has(COMMANDS, value.command) || COMMANDS[value.command][0] !== operation) fail(`${operation} command is invalid`);
    const entry = COMMANDS[value.command];
    exact(value, ['command', ...entry[1]], [], operation);
    entry[1].forEach((key) => member(value.command, key, value[key]));
    // A replace names an existing record; a record of a reference scope is readable storage, never a drawing edit target.
    if (value.command === 'object.vector.replace' && value.record.target.frameScope.kind !== 'authored') fail('a replace targets an authored frame');
  }
  function result(operation, value) {
    if (!OPS.has(operation)) fail('native vector operation is not declared');
    const edit = operation === VECTOR;
    const many = edit && plain(value) && value.operation === 'createMany';
    exact(value, edit ? ['applied', 'historyEntriesAdded', 'operation', 'strokeId'].concat(many ? ['strokeIds'] : []) : ['applied', 'historyEntriesAdded'], [], 'vector edit result');
    if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail('invalid vector edit disposition');
    if (edit) { oneOf(value.operation, ['create', 'createMany', 'replace', 'delete', 'mask.edit'], 'result.operation'); opaque(value.strokeId, 'result.strokeId'); }
    if (many) {
      if (!Array.isArray(value.strokeIds) || value.strokeIds.length < 2) fail('result.strokeIds must list every created stroke');
      value.strokeIds.forEach((id, at) => opaque(id, `result.strokeIds[${at}]`));
    }
  }
  function correlate(request, response) {
    // Native refusals report the actual owner; no successful or unrelated reply gains this exception.
    if (OPS.has(request.operation) && response.ok === false && response.error && response.error.code === 'wrong_instance') return;
    if (response.instanceId !== request.instanceId) fail('vector response owner mismatch');
    const replaced = response.ok === false && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('vector response document mismatch');
    if (!response.ok) return;
    if (response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) fail('vector receipt revision mismatch');
    if (request.operation !== VECTOR) return;
    const command = request.payload.command, ran = command.slice('object.vector.'.length);
    if (response.result.operation !== ran) fail('vector receipt operation mismatch');
    const stroke = response.result.strokeId;
    // A create reports the id the owner allocated; replace and delete report the stable target they named.
    const allocation = 'native-stroke:' + (request.expectedRevision + 1) + ':';
    const allocated = (id) => typeof id === 'string' && id.startsWith(allocation) && /^(0|[1-9][0-9]*)$/.test(id.slice(allocation.length));
    if (command === 'object.vector.createMany') {
      const ids = response.result.strokeIds;
      if (!(response.result.applied === true && ids.length === request.payload.records.length && ids[0] === stroke &&
        ids.every(allocated) && new Set(ids).size === ids.length)) fail('vector receipt stroke mismatch');
      return;
    }
    if (command === 'object.vector.create' ? !(response.result.applied === true && allocated(stroke))
      : stroke !== (command === 'object.vector.replace' ? request.payload.record.target.strokeId : request.payload.stableTarget.strokeId)) fail('vector receipt stroke mismatch');
  }
  // Authored identities may legitimately start with data: or blob:; the SDK only inspects them as text.
  const OPAQUE = Object.freeze({ has: (at) => /^payload\.(?:record\.target|records\.[0-9]+\.target|stableTarget)\.(?:layerUid|strokeId)$/.test(at) || at === 'payload.layerUid' || at === 'result.strokeId' || /^result\.strokeIds\.[0-9]+$/.test(at) });
  const paths = (operation) => (OPS.has(operation) ? OPAQUE : null);
  const own = Object.freeze({ admitted: (op) => OPS.has(op), requiresRevision: (op) => OPS.has(op), payload, result, correlate, paths });

  // Same interface as the font contract: the common SDK consumes one contract object.
  function compose(base) {
    if (!base) return own;
    return Object.freeze({
      admitted: (op) => own.admitted(op) || base.admitted(op),
      requiresRevision: (op) => (own.admitted(op) ? true : base.requiresRevision(op)),
      payload: (op, value, envelope) => (own.admitted(op) ? own.payload(op, value) : base.payload(op, value, envelope)),
      result: (op, value) => (own.admitted(op) ? own.result(op, value) : base.result(op, value)),
      correlate: (request, response) => (own.admitted(request.operation) ? own.correlate(request, response) : base.correlate(request, response)),
      paths: (op) => (own.admitted(op) ? own.paths(op) : base.paths(op)),
    });
  }
  const operationOf = (command) => (has(COMMANDS, command) ? COMMANDS[command][0] : null);
  const members = (command) => (has(COMMANDS, command) ? COMMANDS[command][1].slice() : null);
  const bytes = (text) => (typeof TextEncoder === 'function' ? new TextEncoder().encode(text).length : unescape(encodeURIComponent(text)).length);
  return Object.freeze(Object.assign({
    compose, record, readRecord, readPaths, target, operationOf, members, bytes, FAMILY, VECTOR, PARAMETRIC, EFFECTS,
    MAX_REQUEST_BYTES, ENGINE_VECTOR_REQUEST_BYTES, requestByteLimit, FX_TYPES, SHAPE_MEMBERS,
    OPERATIONS: Object.freeze([VECTOR, PARAMETRIC, EFFECTS]),
  }, own));
}));
