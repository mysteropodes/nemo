/* L01-J baseline stroke dictionary <-> native `editable-vector-path` record (G14, G22, G13). Pure functions, no state, no Paper
 * and no transport. `normalize` reproduces the record-relevant members of serP(desP(dict)) (src/js/app.js 609/846, executed
 * over the shipped Paper.js by tools/gen-oracle.cjs); `toRecord` maps that to the native record; `toDict` is the inverse.
 * The importer rules live HERE, not in storage: `strokeWidth || 3`, the white `#fff` fallback, Paper's miterLimit 10 and
 * dashOffset 0, 3-decimal outline rounding (_r3) and hex8 <-> {r,g,b,a}. A stroke the record cannot carry without changing
 * its look (unsupported gradients, texture/bitmap brushes, text, per-element effects) is a typed refusal, never a silent drop. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectVectorStroke = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const FAMILY = 'editable-vector-path';
  // Members the record owns; everything else a dictionary carries is reported (UNMAPPED) or refuses the mapping (REFUSED).
  const CARRIED = ['segments', 'closed', 'strokeColor', 'hasRealStroke', 'strokeWidth', 'strokeCap', 'strokeJoin', 'miterLimit', 'fillColor',
    'opacity', 'dashArray', 'dashOffset', 'paintOrder', 'isVectorBrush', 'centerSegments', 'widthProfile', 'strokeProfile', 'profileBase',
    'strokeId', 'boxAngle', 'paramShape', 'fillGradient', 'fillSeed', 'fillSeeds', 'fillWalls', 'fillGapPx', 'isMask', 'maskMode', 'maskFeather', 'linkedFillId', 'isLinkedFillCompanion'];
  // Look-changing members with no record slot: mapping them as a plain path would lose the look without a word.
  const REFUSED = ['strokeGradientAlongPath', 'bitmapBrushSpec', 'brushTexturePreset', 'isBrushTextureCopy', 'isText', 'isTextRoot',
    'isVectorText', 'effects'];
  const SHAPE_MEMBERS = { rect: ['tl', 'tr', 'br', 'bl'], ellipse: ['startAngle', 'sweep', 'innerRadius'], star: ['pointCount', 'innerRatio', 'cornerRadius'] };
  const FX_TYPES = [
    { id: 'zigzag', params: [{ key: 'p1', def: 12 }, { key: 'p2', def: 4 }, { key: 'p3', def: 0 }] },
    { id: 'roughen', params: [{ key: 'p1', def: 8 }, { key: 'p2', def: 6 }, { key: 'p3', def: 0 }] },
  ];
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const plain = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
  // The baseline serializer drops falsy members and empty arrays; a zero (fillGapPx 0) is a value.
  const present = (value) => value !== undefined && value !== null && value !== false && !(Array.isArray(value) && !value.length);
  function problem(code, reason, extra) { return Object.freeze(Object.assign({ ok: false, code, reason }, extra)); }
  class Refusal extends Error { constructor(code, reason, extra) { super(reason); this.refusal = problem(code, reason, extra); } }
  const refuse = (code, reason, extra) => { throw new Refusal(code, reason, extra); };

  // ---- colours -------------------------------------------------------------------------------------------------
  // #rgb, #rrggbb, #rrggbbaa (what colorHex8 writes and Paper reads); channels are byte / 255.
  function parseColor(text) {
    if (typeof text !== 'string') refuse('unsupported_color', 'a colour is a hex string');
    const m = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.exec(text);
    if (!m) refuse('unsupported_color', `colour ${JSON.stringify(text)} is not #rgb, #rrggbb or #rrggbbaa`);
    let hex = m[1].toLowerCase();
    if (hex.length === 3) hex = hex.replace(/./g, (c) => c + c);
    const byte = (at) => parseInt(hex.slice(at, at + 2), 16) / 255;
    return { kind: 'solid', r: byte(0), g: byte(2), b: byte(4), a: hex.length === 8 ? byte(6) : 1 };
  }
  // colorHex8: opaque -> #rrggbb, otherwise #rrggbbaa.
  function colorHex8(color) {
    const h = (n) => Math.max(0, Math.min(255, Math.round(n * 255))).toString(16).padStart(2, '0');
    return '#' + h(color.r) + h(color.g) + h(color.b) + (color.a >= 1 ? '' : h(color.a));
  }
  // app.js _r3 verbatim: persisted outline coordinates are rounded to 3 decimals, non-finite becomes 0, -0 survives.
  function r3(v) { return isFinite(v) ? Math.round(v * 1000) / 1000 : 0; }

  // ---- serP(desP(dict)) for the record-relevant members -------------------------------------------------------
  function xy(value, label) {
    if (!Array.isArray(value) || value.length < 2) refuse('invalid_dict', `${label} must be an [x, y] pair`);
    return [value[0], value[1]];
  }
  function gradient(value) {
    if (!plain(value) || Object.keys(value).sort().join(',') !== 'from,kind,stops,to' ||
        !['linear','radial'].includes(value.kind) || !Array.isArray(value.stops) || value.stops.length !== 2) refuse('unavailable', 'only two-stop linear/radial fillGradient is admitted');
    const coordinate = (p) => {
      if (!Array.isArray(p) || p.length !== 2 || ![p[0],p[1]].every((n) => Number.isFinite(n) && Math.abs(n) <= 3.4028234663852886e38)) refuse('invalid_gradient', 'gradient points must be finite pairs');
      return p.slice();
    };
    const stops = [0,1].map((i) => {
      const s = value.stops[i];
      if (!plain(s) || Object.keys(s).sort().join(',') !== 'color,offset' || !Number.isFinite(s.offset) || Math.abs(s.offset) > 3.4028234663852886e38 ||
          typeof s.color !== 'string' || !/^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(s.color)) refuse('invalid_gradient', 'invalid gradient stop');
      return {offset:s.offset,color:s.color};
    });
    const from=coordinate(value.from), to=coordinate(value.to);
    if (!Number.isFinite(Math.fround(Math.hypot(to[0]-from[0],to[1]-from[1])))) refuse('invalid_gradient', 'gradient extent is not representable');
    return {kind:value.kind,from,to,stops};
  }
  function normalize(dict, op) {
    if (!plain(dict) || !Array.isArray(dict.segments)) refuse('invalid_dict', 'a stroke dictionary needs a segments array');
    const vb = !!dict.isVectorBrush;
    const segments = dict.segments.map((s, at) => {
      if (!plain(s)) refuse('invalid_dict', `segments[${at}] must be an object`);
      const out = {};
      ['point', 'handleIn', 'handleOut'].forEach((key) => { const p = xy(s[key], `segments[${at}].${key}`); out[key] = [r3(p[0]), r3(p[1])]; });
      return out;
    });
    // desP: hasRealStroke === false is authoritative; a missing colour falls back to #fff unless the stroke is a ribbon or brush anchor.
    let stroke = dict.hasRealStroke === false ? null
      : (dict.strokeColor || ((vb || dict.brushTexturePreset || dict.bitmapBrushSpec || dict.isBrushTextureCopy) ? null : '#fff'));
    if (vb && !dict.isBrushTextureCopy && !dict.isFillShape) stroke = null;   // applyBrushKeyline
    const strokeColor = stroke ? parseColor(stroke) : null;
    const out = {
      segments, closed: !!dict.closed,
      // serP: a ribbon and a brush-texture anchor without a stroke write null; every other stroke-less path writes the white fallback.
      strokeColor: vb ? null : (strokeColor ? colorHex8(strokeColor) : ((dict.brushTexturePreset || dict.isBrushTextureCopy || dict.bitmapBrushSpec) ? null : '#ffffff')),
      hasRealStroke: vb ? false : !!strokeColor,
      strokeWidth: dict.strokeWidth || 3,
      strokeCap: (typeof dict.strokeCap === 'string' ? dict.strokeCap : 'round') || 'round',
      strokeJoin: (typeof dict.strokeJoin === 'string' ? dict.strokeJoin : 'round') || 'round',
    };
    if (dict.miterLimit !== undefined && typeof dict.miterLimit !== 'number') refuse('invalid_dict', 'miterLimit must be a number');
    out.miterLimit = dict.miterLimit !== undefined ? dict.miterLimit : 10;   // Paper's default
    out.fillColor = dict.fillColor ? colorHex8(parseColor(dict.fillColor)) : null;
    out.opacity = op !== undefined ? op : (dict.opacity !== undefined ? dict.opacity : 1);
    if (dict.dashArray && dict.dashArray.length) out.dashArray = dict.dashArray.slice();
    out.dashOffset = dict.dashOffset !== undefined ? dict.dashOffset : 0;
    if (dict.paintOrder) out.paintOrder = dict.paintOrder;
    if (dict.fillGradient) out.fillGradient = gradient(dict.fillGradient);
    if (vb) {
      out.isVectorBrush = true;
      if (dict.centerSegments) out.centerSegments = dict.centerSegments;
      if (dict.widthProfile) out.widthProfile = dict.widthProfile;
      if (dict.strokeProfile) out.strokeProfile = dict.strokeProfile;
      if (dict.profileBase) out.profileBase = dict.profileBase;
    }
    if (dict.strokeId) out.strokeId = dict.strokeId;
    if (dict.boxAngle) out.boxAngle = dict.boxAngle;
    for (const key of ['isMask', 'maskMode', 'maskFeather']) {
      if (!has(dict, key)) continue;
      const v = dict[key];
      if (v !== null && (key === 'isMask' ? typeof v !== 'boolean' : key === 'maskMode' ? typeof v !== 'string' : typeof v !== 'number' || !Number.isFinite(v))) {
        refuse('invalid_dict', `invalid ${key}`);
      }
      out[key] = v;
    }
    if (dict.paramShape) out.paramShape = dict.paramShape;
    const seed=(p)=>Array.isArray(p)&&p.length===2&&p.every((n)=>typeof n==='number'&&Number.isFinite(n));
    ['fillSeed','fillSeeds','fillWalls','fillGapPx'].forEach((key)=>{
      if(!has(dict,key)||dict[key]===undefined)return;
      const v=dict[key],ok=key==='fillSeed'?seed(v):key==='fillSeeds'?Array.isArray(v)&&v.every(seed):
        key==='fillWalls'?Array.isArray(v)&&v.every((id)=>typeof id==='string'&&id.length>0):
        typeof v==='number'&&Number.isFinite(v)&&v>=0;
      if(!ok)refuse('invalid_dict','invalid '+key);
      out[key]=JSON.parse(JSON.stringify(v));
    });
    for (const key of ['linkedFillId', 'isLinkedFillCompanion']) {
      if (!has(dict,key)) continue;
      if (key === 'linkedFillId' ? typeof dict[key] !== 'string' || !dict[key].length : typeof dict[key] !== 'boolean') refuse('invalid_dict','invalid '+key);
      out[key]=dict[key];
    }
    return out;
  }

  // ---- dictionary -> record ------------------------------------------------------------------------------------
  const pt = (pair) => ({ x: pair[0], y: pair[1] });
  function ribbonAnchor(anchor, at) {
    if (!plain(anchor)) refuse('invalid_dict', `centerSegments[${at}] must be an object`);
    return { point: pt(xy(anchor.point, `centerSegments[${at}].point`)), handleIn: pt(xy(anchor.handleIn, `centerSegments[${at}].handleIn`)),
      handleOut: pt(xy(anchor.handleOut, `centerSegments[${at}].handleOut`)), width: anchor.width };
  }
  function shape(value) {
    if (!plain(value) || !SHAPE_MEMBERS[value.kind]) refuse('unsupported_member', 'paramShape.kind must be rect, ellipse or star', { members: ['paramShape'] });
    const allowed = ['kind', 'box'].concat(SHAPE_MEMBERS[value.kind]);
    const stray = Object.keys(value).filter((key) => !allowed.includes(key) && value[key] !== undefined);
    if (stray.length) refuse('unsupported_member', 'paramShape carries members its kind does not own', { members: stray.map((key) => `paramShape.${key}`) });
    const out = { kind: value.kind };
    SHAPE_MEMBERS[value.kind].forEach((key) => { if (value[key] !== undefined) out[key] = value[key]; });
    if (value.box !== undefined) out.box = { x1: value.box.x1, y1: value.box.y1, x2: value.box.x2, y2: value.box.y2 };
    return out;
  }
  // target: { layerUid, frame, strokeId?, contextId?, scope? }. The record keeps the authored strokeId (or '' when there is none,
  // which is what a create needs: the owner allocates the id).
  function toRecord(dict, where, options) {
    try {
      if (!plain(where) || typeof where.layerUid !== 'string' || !where.layerUid || !Number.isSafeInteger(where.frame) || where.frame < 0) {
        refuse('invalid_target', 'target needs layerUid and a non-negative integer frame');
      }
      const refused = plain(dict) ? REFUSED.filter((key) => present(dict[key])) : [];
      if (refused.length) refuse('unavailable', 'the stroke carries members the native vector record does not own', { members: refused });
      const n = normalize(dict, options && options.opacity);
      const count = n.segments.length;
      if (count < 2 || count > 256) refuse('unsupported_geometry', `a vector record needs 2..256 anchors, the stroke has ${count}`, { count });
      const vector = {
        stroke: n.hasRealStroke ? parseColor(n.strokeColor) : null, hasRealStroke: n.hasRealStroke,
        strokeWidth: n.strokeWidth, strokeCap: n.strokeCap, strokeJoin: n.strokeJoin,
      };
      ['miterLimit', 'dashArray', 'dashOffset', 'paintOrder'].forEach((key) => { if (n[key] !== undefined) vector[key] = n[key]; });
      vector.opacity = n.opacity;
      const ext = {};
      if (has(dict, 'linkedFillId')) {
        if (typeof dict.linkedFillId !== 'string' || !dict.linkedFillId.length || (typeof TextEncoder === 'function' ? new TextEncoder().encode(dict.linkedFillId).length : unescape(encodeURIComponent(dict.linkedFillId)).length) > 128 || /[\u0000-\u001f\u007f-\u009f]/.test(dict.linkedFillId)) refuse('unavailable', 'invalid linked fill identity');
        ext.linkedFillId = dict.linkedFillId;
      }
      if (has(dict, 'isLinkedFillCompanion')) {
        if (typeof dict.isLinkedFillCompanion !== 'boolean') refuse('unavailable', 'invalid linked fill role');
        ext.isLinkedFillCompanion = dict.isLinkedFillCompanion;
      }
      if (dict.isLinkedFillCompanion === true && (!has(ext, 'linkedFillId') || n.isVectorBrush || !n.fillColor)) refuse('unavailable', 'invalid separate linked fill companion');
      if (has(ext, 'linkedFillId') && dict.isLinkedFillCompanion !== true && (!n.isVectorBrush || !Array.isArray(n.centerSegments) || n.centerSegments.length < 2)) refuse('unavailable', 'linked fill primary needs its actual pressure centerline');
      if (n.isVectorBrush) {
        const pressure = {};
        if (n.centerSegments !== undefined) pressure.centerSegments = n.centerSegments.map(ribbonAnchor);
        if (n.widthProfile !== undefined) pressure.widthProfile = n.widthProfile.map((stop) => ({ t: stop.t, width: stop.width }));
        if (n.strokeProfile !== undefined) pressure.strokeProfile = n.strokeProfile;
        if (n.profileBase !== undefined) pressure.profileBase = n.profileBase;
        ext.pressure = pressure;
      }
      if (n.fillGradient !== undefined) {
        if (n.isVectorBrush) refuse('unavailable', 'pressure-gradient consumer is unavailable');
        ext.fillGradient = gradient(n.fillGradient);
      }
      if (n.paramShape !== undefined) ext.paramShape = shape(n.paramShape);
      if (n.boxAngle !== undefined) ext.boxAngle = n.boxAngle;
      for (const key of ['isMask', 'maskMode', 'maskFeather']) if (has(n, key)) ext[key] = n[key];
      const trace={};
      [['fillSeed','seed'],['fillSeeds','seeds'],['fillWalls','walls'],['fillGapPx','gapPx']].forEach(([key,stored])=>{
        if(has(n,key))trace[stored]=JSON.parse(JSON.stringify(n[key]));
      });
      if(Object.keys(trace).length){
        if(!has(trace,'seed')&&!has(trace,'seeds'))refuse('unavailable','fill provenance requires its authored seed');
        ext.fillTrace=trace;
      }
      const strokeId = where.strokeId !== undefined ? where.strokeId : (n.strokeId || '');
      const record = {
        schemaVersion: 1, family: FAMILY,
        target: { contextId: where.contextId || 'scene-root', frameScope: { kind: where.scope || 'authored', frame: where.frame }, layerUid: where.layerUid, strokeId },
        geometry: {
          kind: 'cubic-path', closed: n.closed, coordinateSpace: 'document-world', handleSpace: 'relative',
          segments: n.segments.map((s) => ({ point: pt(s.point), handleIn: pt(s.handleIn), handleOut: pt(s.handleOut) })),
        },
        fill: n.fillColor ? parseColor(n.fillColor) : null,
        vector,
      };
      if (Object.keys(ext).length) record.vectorExt = ext;
      const unmapped = Object.keys(dict).filter((key) => present(dict[key]) && !CARRIED.includes(key));
      return Object.freeze({ ok: true, record, unmapped, authoredStrokeId: n.strokeId || null });
    } catch (error) {
      if (error instanceof Refusal) return error.refusal;
      throw error;
    }
  }

  // ---- record -> dictionary (the serP shape for the members the record owns) -----------------------------------
  const pair = (p) => [p.x, p.y];
  function toDict(record) {
    if (!plain(record) || record.family !== FAMILY) return problem('invalid_record', 'not an editable-vector-path record');
    const v = record.vector, x = record.vectorExt || {}, ribbon = x.pressure;
    const out = {
      segments: record.geometry.segments.map((s) => ({ point: pair(s.point), handleIn: pair(s.handleIn), handleOut: pair(s.handleOut) })),
      closed: record.geometry.closed,
      strokeColor: ribbon ? null : (v.stroke ? colorHex8(v.stroke) : '#ffffff'),   // serP writes the white fallback beside hasRealStroke:false
      hasRealStroke: ribbon ? false : v.hasRealStroke,
      strokeWidth: v.strokeWidth, strokeCap: v.strokeCap, strokeJoin: v.strokeJoin,
    };
    if (has(v, 'miterLimit')) out.miterLimit = v.miterLimit;
    out.fillColor = record.fill ? colorHex8(record.fill) : null;
    out.opacity = v.opacity;
    if (has(v, 'dashArray')) out.dashArray = v.dashArray.slice();
    if (has(v, 'dashOffset')) out.dashOffset = v.dashOffset;
    if (has(v, 'paintOrder')) out.paintOrder = v.paintOrder;
    if (ribbon) {
      out.isVectorBrush = true;
      if (has(ribbon, 'centerSegments')) out.centerSegments = ribbon.centerSegments.map((a) => ({ point: pair(a.point), handleIn: pair(a.handleIn), handleOut: pair(a.handleOut), width: a.width }));
      if (has(ribbon, 'widthProfile')) out.widthProfile = ribbon.widthProfile.map((s) => ({ t: s.t, width: s.width }));
      if (ribbon.strokeProfile) out.strokeProfile = ribbon.strokeProfile;
      if (ribbon.profileBase) out.profileBase = ribbon.profileBase;
    }
    if (record.target.strokeId) out.strokeId = record.target.strokeId;
    if (x.boxAngle) out.boxAngle = x.boxAngle;
    for (const key of ['isMask', 'maskMode', 'maskFeather', 'linkedFillId', 'isLinkedFillCompanion']) if (has(x, key)) out[key] = x[key];
    if (x.paramShape) out.paramShape = JSON.parse(JSON.stringify(x.paramShape));
    if (x.fillGradient) out.fillGradient = gradient(x.fillGradient);
    if(x.fillTrace)[['seed','fillSeed'],['seeds','fillSeeds'],['walls','fillWalls'],['gapPx','fillGapPx']].forEach(([stored,key])=>{
      if(has(x.fillTrace,stored))out[key]=JSON.parse(JSON.stringify(x.fillTrace[stored]));
    });
    return out;
  }

  // ---- layer path-effect stack (path-fx.js) --------------------------------------------------------------------
  function defaultsFor(type) {
    const def = FX_TYPES.find((t) => t.id === type);
    const out = { type, enabled: true };
    if (def) def.params.forEach((p) => { out[p.key] = p.def; });
    return out;
  }
  // ld.pathFx -> the native stack: the entries exactly as stored (enabled/p1..p3 kept absent, null or explicit). An entry the
  // baseline would skip silently (unknown type, non-object) cannot be stored by the owner, so it is a typed refusal, not a drop.
  function stackToNative(stack) {
    if (stack === undefined || stack === null) return problem('invalid_stack', 'pathFx is absent');
    if (!Array.isArray(stack)) return problem('invalid_stack', 'pathFx must be an array');
    const out = [];
    for (let at = 0; at < stack.length; at += 1) {
      const e = stack[at];
      if (!plain(e) || !FX_TYPES.some((t) => t.id === e.type)) return problem('unsupported_effect', `pathFx[${at}] is not a known effect`, { index: at });
      const entry = { type: e.type };
      ['enabled', 'p1', 'p2', 'p3'].forEach((key) => { if (e[key] !== undefined) entry[key] = e[key]; });
      out.push(entry);
    }
    return Object.freeze({ ok: true, stack: out });
  }
  // SMPathFx.hasAny: at least one entry that is enabled (!== false) and of a known type.
  function hasActive(stack) {
    return Array.isArray(stack) && stack.some((e) => plain(e) && e.enabled !== false && FX_TYPES.some((t) => t.id === e.type));
  }

  return Object.freeze({
    FAMILY, CARRIED: Object.freeze(CARRIED.slice()), REFUSED: Object.freeze(REFUSED.slice()), FX_TYPES: Object.freeze(FX_TYPES.map((t) => t.id)),
    parseColor, colorHex8, r3, normalize: (dict, op) => { try { return normalize(dict, op); } catch (e) { if (e instanceof Refusal) return e.refusal; throw e; } },
    toRecord, toDict, defaultsFor, stackToNative, hasActive,
  });
}));
