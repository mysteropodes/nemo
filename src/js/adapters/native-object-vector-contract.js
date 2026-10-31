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
  const PARAMETRIC = 'command.document.object.parametric.edit', EFFECTS = 'command.document.object.path.effects.edit';
  const OPS = new Set([VECTOR, PARAMETRIC, EFFECTS]);
  const FAMILY = 'editable-vector-path';
  // Full encoded request envelopes: vector edits may carry a complete 256-anchor record, and the Fill Brush commit
  // (an edit-contract operation) a 256-segment outline; every other operation keeps the default bound. Replies remain bounded separately.
  const MAX_REQUEST_BYTES = 4096, ENGINE_VECTOR_REQUEST_BYTES = 65536, FILL_BRUSH = 'command.document.object.fill.brush';
  // #2361: a bitmap-brush Draw stroke carries a whole anchor record (its PNG is a staged file, never request bytes).
  const BITMAP_STROKE = 'command.document.object.brush.bitmap.stroke';
  const requestByteLimit = (operation) => operation === VECTOR || operation === FILL_BRUSH || operation === BITMAP_STROKE ? ENGINE_VECTOR_REQUEST_BYTES : MAX_REQUEST_BYTES;
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
  // #2372 restyle members (SEL-STYLE2: switches, stroke details, preset, feather): a paint switched on names its colour; a preset or a feather stands alone.
  const HEX = (v, l) => { if (!/^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(v)) fail(`${l} must be a hex colour`); }, FLAG = (v, l) => { if (typeof v !== 'boolean') fail(`${l} must be a boolean`); }, ENUM = (list) => (v, l) => oneOf(v, list, l), RANGE = (lo, hi) => (v, l) => { if (num(v, l) < lo || v > hi) fail(`${l} is out of range`); };
  const STYLE = { fill: HEX, stroke: HEX, width: RANGE(1e-9, 10000), opacity: unit, fillEnabled: FLAG, strokeEnabled: FLAG, cap: ENUM(CAPS), join: ENUM(JOINS), miter: RANGE(1, 1000), dashOffset: RANGE(-1e6, 1e6), paintOrder: ENUM(ORDERS), strokeStyle: ENUM(['solid', 'dashed', 'dotted']), maskFeather: RANGE(0, 500), maskMode: ENUM(['add', 'subtract', 'intersect']), unmask: (v, l) => { if (v !== true) fail(`${l} must be true`); }, brushPreset: (v, l) => { if (typeof v !== 'string' || !v.length || v.length > 96 || /[\u0000-\u001f\u007f]/.test(v)) fail(`${l} is invalid`); },
    // #2372 native-fill-gradient: a stored two-stop gradient with #rrggbb[aa] stop colours, or null for the flat fill.
    fillGradient: (v, l) => { if (v === null) return; gradient(v, l); if (v.stops.some((stop) => !/^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(stop.color))) fail(`${l} stop colours are #rrggbb[aa]`); } };
  const restyleStyle = (st, label) => { const keys = Object.keys(st); if (!keys.length || st.fillEnabled === true && !has(st, 'fill') || st.strokeEnabled === true && !has(st, 'stroke') || keys.length > 1 && (has(st, 'brushPreset') || has(st, 'maskFeather') || has(st, 'maskMode') || has(st, 'unmask') || has(st, 'fillGradient'))) fail(`${label} is invalid`); keys.forEach((k) => STYLE[k](st[k], `${label}.${k}`)); };
  function opaque(value, label) { if (typeof value !== 'string' || !value.length) fail(`${label} must be a non-empty string`); return value; }
  function frame(value, label) { if (!Number.isSafeInteger(value) || value < 0 || value > MAX_FRAME) fail(`${label} must be a native u32 frame`); return value; }
  function pair(value, label) {
    if (!Array.isArray(value) || value.length !== 2) fail(`${label} must be an [x, y] pair`);
    value.forEach((part, at) => num(part, `${label}[${at}]`));
  }

  function heldContext(id) { try { const c = typeof globalThis === 'object' ? globalThis.NemoNativeComponentContext : undefined; return typeof id === 'string' && !!id && !!c && typeof c.contextId === 'function' && c.contextId(globalThis) === id; } catch (_) { return false; } } // #2256: only the component the workspace holds
  function target(value, label, create) {
    exact(value, ['contextId', 'frameScope', 'layerUid', 'strokeId'], [], label);
    if (value.contextId !== 'scene-root' && !heldContext(value.contextId)) fail(`${label}.contextId is not admitted`); // #2256: the scene root or the component the workspace holds
    exact(value.frameScope, ['kind', 'frame'], [], `${label}.frameScope`); oneOf(value.frameScope.kind, ['authored', 'reference'], `${label}.frameScope.kind`);
    frame(value.frameScope.frame, `${label}.frameScope.frame`); opaque(value.layerUid, `${label}.layerUid`);
    // A created record names no stroke id: the owner allocates it and the caller cannot choose it.
    if (create === true) { if (value.strokeId !== '') fail(`${label}.strokeId must be empty for a create`); } else opaque(value.strokeId, `${label}.strokeId`);
  }
  function point(value, label) { exact(value, ['x', 'y'], [], label); num(value.x, `${label}.x`); num(value.y, `${label}.y`); }
  function solid(value, label) {
    exact(value, ['kind', 'r', 'g', 'b', 'a'], [], label); if (value.kind !== 'solid') fail(`${label}.kind must be solid`);
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
    if (has(value, 'dashOffset')) num(value.dashOffset, `${label}.dashOffset`); if (has(value, 'paintOrder')) oneOf(value.paintOrder, ORDERS, `${label}.paintOrder`);
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
  // G16: serP's brush-texture members as stored (an anchor's preset and hidden paint, or a dab's copy flag).
  function brushTexture(value, label) {
    exact(value, [], ['brushGroupId', 'isBrushTextureCopy', 'brushTexturePreset', 'preTextureOpacity', 'preTextureStroke', 'strokeSeed'], label);
    const text = (v) => typeof v === 'string' && v.length > 0 && bytes(v) <= 255 && !v.includes('\0');   // UTF-8 bytes, as native counts
    if (!Object.keys(value).length) fail(`${label} must not be empty`);
    if (has(value, 'brushGroupId') && !text(value.brushGroupId)) fail(`${label}.brushGroupId is invalid`);
    if (has(value, 'brushTexturePreset') && !text(value.brushTexturePreset)) fail(`${label}.brushTexturePreset is invalid`);
    if (has(value, 'isBrushTextureCopy') && value.isBrushTextureCopy !== true) fail(`${label}.isBrushTextureCopy must be true`);
    if (has(value, 'preTextureOpacity') && !(typeof value.preTextureOpacity === 'number' && Number.isFinite(value.preTextureOpacity))) fail(`${label}.preTextureOpacity must be finite`);
    if (has(value, 'preTextureStroke') && value.preTextureStroke !== null && !/^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(value.preTextureStroke)) fail(`${label}.preTextureStroke is invalid`);
    if (has(value, 'isBrushTextureCopy') && has(value, 'brushTexturePreset')) fail(`${label} is not both an anchor and a dab`);
    // G16 option C: an anchor's stamping seed, a uint32.
    if (has(value, 'strokeSeed') && !(Number.isInteger(value.strokeSeed) && value.strokeSeed >= 0 && value.strokeSeed <= 0xFFFFFFFF && has(value, 'brushTexturePreset'))) fail(`${label}.strokeSeed is invalid`);
  }
  function ext(value, label, hasStroke) {
    exact(value, [], ['pressure', 'paramShape', 'boxAngle', 'fillGradient', 'fillTrace', 'isMask', 'maskMode', 'maskFeather', 'linkedFillId', 'isLinkedFillCompanion', 'brushTexture', 'shadow', 'isFillShape'], label);
    if (has(value, 'isFillShape') && value.isFillShape !== true) fail(`${label}.isFillShape must be true`);
    if (has(value, 'brushTexture')) brushTexture(value.brushTexture, `${label}.brushTexture`);
    // Shadow Brush (applyShadowBrushTag): channelTag is only ever 'shadow'; the swatch id is non-empty.
    if (has(value, 'shadow')) { exact(value.shadow, ['channelTag'], ['shadowSwatchId'], `${label}.shadow`);
      if (value.shadow.channelTag !== 'shadow' || has(value.shadow, 'shadowSwatchId') && !(typeof value.shadow.shadowSwatchId === 'string' && value.shadow.shadowSwatchId.length > 0 && bytes(value.shadow.shadowSwatchId) <= 256)) fail(`${label}.shadow is invalid`); }
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
    'object.vector.batch': [VECTOR, ['ops']],
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
    else if (key === 'ops') {
      // #2361/#2362: 1..64 ordered create/replace/delete steps on the layers of one context (#2372: a Select may span layers; a trimEnds batch is one layer), one revision and one undo entry.
      if (!Array.isArray(payloadValue) || payloadValue.length < 1 || payloadValue.length > 64) fail(`${label} must hold 1 to 64 steps`);
      const targets = payloadValue.map((op, at) => {
        const where = `${label}[${at}]`;
        if (!plain(op)) fail(`${where} must be an object`);
        if (op.op === 'create' || op.op === 'copy') {
          const copy = op.op === 'copy'; exact(op, copy ? ['op', 'fromStep', 'frame'] : ['op', 'record'], ['placement', 'keyframe'], where); if (!copy) record(op.record, true);
          // #2362 multiframe: a copy is an earlier create's (or copy's) record again on `frame`, without resending it.
          if (copy && !(Number.isSafeInteger(op.fromStep) && op.fromStep >= 0 && op.fromStep < at && ['create', 'copy'].includes(payloadValue[op.fromStep].op) && Number.isSafeInteger(op.frame) && op.frame >= 0 && op.frame <= 4294967295)) fail(`${where} must copy an earlier create to a frame`);
          if (has(op, 'placement')) oneOf(op.placement, ['front', 'behind'], `${where}.placement`);
          // A create's own keyframe: 'copy' keys its held cell with a copy even when the batch's does not (multiframe draw).
          if (has(op, 'keyframe')) oneOf(op.keyframe, ['copy'], `${where}.keyframe`);
          return copy ? null : op.record.target;
        }
        if (op.op === 'replace') { exact(op, ['op', 'record'], [], where); record(op.record, false); if (op.record.target.frameScope.kind !== 'authored') fail(`${where} replace targets an authored frame`); return op.record.target; }
        // #2362 knife: reshape gives a named stroke new geometry, clone creates one from it on `frame`; the style stays native.
        // A geometry-less clone copies any stroke shifted by `offset` (a pose stamp); by `center` rebuilds a ribbon (sculpt); by `matrix` maps any record by a record-space affine (#2372 Select).
        if (op.op === 'reshape' || op.op === 'clone') { const clone = op.op === 'clone'; exact(op, ['op', 'stableTarget'].concat(clone ? ['frame'] : []), clone ? ['geometry', 'offset', 'placement', 'keyframe'] : ['geometry', 'center', 'matrix', 'frame'], where); target(op.stableTarget, `${where}.stableTarget`, false); if (has(op, 'geometry')) geometry(op.geometry, `${where}.geometry`); if (!clone && [has(op, 'geometry'), has(op, 'center'), has(op, 'matrix')].filter(Boolean).length !== 1) fail(`${where} needs geometry, a center or a matrix`); if (has(op, 'matrix') && (!Array.isArray(op.matrix) || op.matrix.length !== 6 || !op.matrix.every(Number.isFinite))) fail(`${where}.matrix must be six finite numbers`); if (has(op, 'center')) { if (!Array.isArray(op.center) || op.center.length < 2 || op.center.length > 256) fail(`${where}.center must have 2..256 anchors`); pressure({ centerSegments: op.center }, where); } if (has(op, 'offset') && (has(op, 'geometry') || !Array.isArray(op.offset) || op.offset.length !== 2 || !op.offset.every(Number.isFinite))) fail(`${where}.offset is invalid`);
          if (has(op, 'frame') && !(Number.isSafeInteger(op.frame) && op.frame >= 0 && op.frame <= 4294967295)) fail(`${where}.frame must be a frame`); if (has(op, 'placement')) oneOf(op.placement, ['front', 'behind'], `${where}.placement`); if (has(op, 'keyframe')) oneOf(op.keyframe, ['copy'], `${where}.keyframe`); return op.stableTarget; }
        if (op.op === 'restyle') { exact(op, ['op', 'stableTarget', 'style'], ['frame'], where); target(op.stableTarget, `${where}.stableTarget`, false); restyleStyle(exact(op.style, [], Object.keys(STYLE), `${where}.style`), `${where}.style`); if (has(op, 'frame')) frame(op.frame, `${where}.frame`); return op.stableTarget; }
        if (op.op === 'delete') { exact(op, ['op', 'stableTarget'], [], where); target(op.stableTarget, `${where}.stableTarget`, false); return op.stableTarget; }
        return fail(`${where}.op must be create, copy, replace, reshape, clone, restyle or delete`);
      });
      targets.forEach((t, at) => { if (!t) targets[at] = targets[payloadValue[at].fromStep]; });
      if (targets.some((t) => t.contextId !== targets[0].contextId)) fail(`${label} must stay on one context`);
    }
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
    // G15: the two creates take an optional placement (absent = front; behind = under the layer's artwork)
    // and an optional trimEnds limit (the baseline trimStrokeEndsMax; finite, above zero).
    const creates = value.command === 'object.vector.create' || value.command === 'object.vector.createMany';
    exact(value, ['command', ...entry[1]], creates ? ['placement', 'trimEnds', 'keyframe'] : value.command === 'object.vector.batch' ? ['keyframe', 'trimEnds', 'skipUnavailable'] : [], operation);
    if (has(value, 'placement')) oneOf(value.placement, ['front', 'behind'], `${value.command}.placement`);
    if (has(value, 'trimEnds') && !(typeof value.trimEnds === 'number' && Number.isFinite(value.trimEnds) && value.trimEnds > 0)) fail(`${value.command}.trimEnds is invalid`);
    // A batch trims only when every step creates (the drawn stroke, its copies and its multi-frame stamps share one cut).
    if (has(value, 'trimEnds') && value.command === 'object.vector.batch' && Array.isArray(value.ops) && value.ops.some((op) => !plain(op) || (op.op !== 'create' && op.op !== 'copy'))) fail('object.vector.batch.trimEnds needs create steps only'); if (has(value, 'trimEnds') && value.command === 'object.vector.batch' && Array.isArray(value.ops) && new Set(value.ops.filter((op) => op.record && op.record.target).map((op) => op.record.target.layerUid)).size > 1) fail('object.vector.batch.trimEnds stays on one layer');
    // keyframe: 'copy' keys a held cell with what it showed (the Labs creators' ensureKeyframe()); absent is Draw's empty key.
    if (has(value, 'keyframe') && value.keyframe !== 'copy') fail(`${value.command} keyframe must be 'copy'`); if (has(value, 'skipUnavailable') && typeof value.skipUnavailable !== 'boolean') fail(`${value.command} skipUnavailable must be a boolean`); // #2372: a batch is atomic unless this opts in
    entry[1].forEach((key) => member(value.command, key, value[key]));
    // A replace names an existing record; a record of a reference scope is readable storage, never a drawing edit target.
    if (value.command === 'object.vector.replace' && value.record.target.frameScope.kind !== 'authored') fail('a replace targets an authored frame');
  }
  function result(operation, value) {
    if (!OPS.has(operation)) fail('native vector operation is not declared');
    const edit = operation === VECTOR;
    const many = edit && plain(value) && (value.operation === 'createMany' || value.operation === 'batch');
    exact(value, edit ? ['applied', 'historyEntriesAdded', 'operation', 'strokeId'].concat(many ? ['strokeIds'] : []) : ['applied', 'historyEntriesAdded'], value.operation === 'batch' ? ['skippedSteps'] : [], 'vector edit result');
    if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail('invalid vector edit disposition');
    if (edit) { oneOf(value.operation, ['create', 'createMany', 'replace', 'delete', 'mask.edit', 'batch'], 'result.operation'); opaque(value.strokeId, 'result.strokeId'); }
    if (many) {
      if (!Array.isArray(value.strokeIds) || value.strokeIds.length < (value.operation === 'batch' ? 1 : 2)) fail('result.strokeIds must list every created stroke');
      value.strokeIds.forEach((id, at) => opaque(id, `result.strokeIds[${at}]`)); if (has(value, 'skippedSteps') && !(Number.isSafeInteger(value.skippedSteps) && value.skippedSteps >= 1 && value.skippedSteps < value.strokeIds.length)) fail('result.skippedSteps must count some, not all, steps');
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
    if (command === 'object.vector.batch') {
      // One id per step, in order: allocated for a create, the named target for a replace or delete.
      const ids = response.result.strokeIds, ops = request.payload.ops; if (has(response.result, 'skippedSteps') && request.payload.skipUnavailable !== true) fail('vector receipt skipped steps of an atomic batch');
      if (!(ids.length === ops.length && ids[0] === stroke && ops.every((op, at) => (['create', 'copy', 'clone'].includes(op.op) ? allocated(ids[at])
        : ids[at] === (op.op === 'replace' ? op.record.target.strokeId : op.stableTarget.strokeId))))) fail('vector receipt stroke mismatch');
      return;
    }
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
  const OPAQUE = Object.freeze({ has: (at) => /^payload\.(?:record\.target|records\.[0-9]+\.target|stableTarget|ops\.[0-9]+\.(?:record\.target|stableTarget))\.(?:layerUid|strokeId)$/.test(at) || at === 'payload.layerUid' || at === 'result.strokeId' || /^result\.strokeIds\.[0-9]+$/.test(at) });
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
