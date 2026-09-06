'use strict';
// The R03 fixture corpus: twelve small deterministic Nemo documents, each
// with expectations computed by tests/fixtures/lib/reference.cjs (never by the
// application). Coverage required by #899: static / keyed / expression
// properties, held frames, components, masks & alpha, text, mesh, media,
// migration, interaction, export.
//
// Every builder returns { meta, project, expected, assets } where `assets`
// maps a relative file name to bytes. Documents follow the exportJSON
// (timeline.js) shape at format version 13; stroke dicts follow serP/serR
// (app.js). Ids are deterministic (ly_<fixture>_<n>, s_<fixture>_<n>).
const ref = require('./reference.cjs');
const png = require('./png.cjs');
const { mulberry32, seedFrom } = require('./rng.cjs');

const FORMAT_VERSION = 13;
const DEFAULT_CURVE = [{ x: 0, y: 0 }, { x: 0.25, y: 0.156 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.844 }, { x: 1, y: 1 }];
const LEGACY_STEP_CURVE = [{ x: 0, y: 0 }, { x: 0.42, y: 0 }, { x: 0.58, y: 1 }, { x: 1, y: 1 }];
const PROP_DEFAULT = { position: [0, 0], anchor: [0, 0], rotation: [0], scale: [100, 100], opacity: [100], order: [0] };
const BASE_PROPS = ['position', 'anchor', 'rotation', 'scale', 'opacity', 'order'];

// --- document building blocks -------------------------------------------------

function seg(x, y) { return { point: [x, y], handleIn: [0, 0], handleOut: [0, 0] }; }

// A stroke dict as serP writes it. Fill-only paths carry serP's historical
// '#ffffff' strokeColor fallback with hasRealStroke:false (the authoritative
// "no stroke" signal desP honours).
function pathStroke(id, points, o = {}) {
  const hasStroke = !!o.stroke;
  const d = {
    segments: points.map(([x, y]) => seg(x, y)),
    closed: o.closed !== false,
    strokeColor: hasStroke ? o.stroke : '#ffffff',
    hasRealStroke: hasStroke,
    strokeWidth: o.width || 3,
    strokeCap: 'round', strokeJoin: 'round',
    fillColor: o.fill || null,
    opacity: o.opacity === undefined ? 1 : o.opacity,
    strokeId: id,
  };
  return Object.assign(d, o.extra || {});
}

function rasterStroke(o) {
  const d = { isRaster: true, x: o.x, y: o.y, width: o.width, height: o.height, opacity: o.opacity === undefined ? 1 : o.opacity };
  if (o.src) d.src = o.src;
  if (o.rotation) d.rotation = o.rotation;
  return Object.assign(d, o.extra || {});
}

function rect(x, y, w, h) { return [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]; }

// frames(24, { 0: [...], 6: [...] }) — keyframes at the listed indices, held
// elsewhere. Frame 0 is always a keyframe (createUserLayer's convention).
function frames(total, keyed = {}) {
  const out = [];
  for (let i = 0; i < total; i++) {
    const isKey = i === 0 || Object.prototype.hasOwnProperty.call(keyed, i);
    out.push({ strokes: keyed[i] ? keyed[i] : [], isKeyframe: isKey, isInterpolated: false });
  }
  return out;
}

function layer(uid, name, fr, extra = {}) {
  return Object.assign({ name, visible: true, locked: false, frames: fr, color: '#4a9eff', effects: [], layerUid: uid, parentLayerUid: null }, extra);
}

function keys(list) { return { keys: list.map((k) => Object.assign({}, k)) }; }

function project(o) {
  const total = o.totalFrames || 24;
  return Object.assign({
    version: FORMAT_VERSION, totalFrames: total, fps: o.fps || 24,
    canvasW: o.canvasW || 640, canvasH: o.canvasH || 360, canvasBg: '#ffffff', waIn: 0, waOut: total - 1,
    mediaMode: 'embedded', layers: o.layers, layerFolders: {}, layerLinkGroups: {}, storyboard: null,
    symbols: o.symbols || {}, customEffects: [], audioTracks: [], refMedia: null, mediaLibrary: o.mediaLibrary || [],
    motionArcs: {}, easingCurve: { points: [{ x: 0, y: 0 }, { x: 0.3, y: 0.05 }, { x: 0.7, y: 0.95 }, { x: 1, y: 1 }] },
    resamplePts: 50, tweenStep: 1, tweenOverrides: {}, tweenEasing: {}, comments: [], cameraKeys: [], cameraLayerOn: false,
    markers: [], shyEnabled: false, trackRoles: {}, imageMeshes: o.imageMeshes || {}, bpm: 120, bpmOffset: 0, bpmShow: false,
    motionBlurOn: false, motionBlurSamples: 6, motionBlurShutter: 0.5, guides: { h: [], v: [] }, exprGlobals: o.exprGlobals || '',
  }, o.extra || {});
}

// --- expectation helpers ------------------------------------------------------

const TOL = { exact: 1e-9, pixel: 2 };
function motionCheck(layerUid, prop, frame, expect, note) {
  return { kind: 'motion-value', layerUid, prop, frame, expect, tolerance: TOL.exact, verify: 'node', note };
}
function pixelCheck(x, y, rgb, note, frame = 0) {
  return { kind: 'pixel', frame, x, y, rgb, tolerance: TOL.pixel, verify: 'gate', gate: 'R13 render/export harness', note };
}
function docCheck(path, expect, note) { return { kind: 'document', path, expect, verify: 'node', note }; }

// Two-colour test image: left half red, right half blue (exact, unaliased).
function halfImage(w, h) {
  const img = png.canvas(w, h, [220, 30, 30, 255]);
  png.fillRect(img, w / 2, 0, w / 2, h, [30, 60, 220, 255]);
  return png.encodeRGBA(w, h, img.rgba);
}

function motionValueChecks(uid, tracks, framesToCheck, easeAt, extraNote) {
  const out = [];
  for (const [prop, ks] of Object.entries(tracks)) for (const f of framesToCheck) {
    out.push(motionCheck(uid, prop, f, ref.trackValueAt(ks, f, easeAt), extraNote));
  }
  return out;
}

// --- 1. static properties -----------------------------------------------------

function staticProps() {
  const id = 'static-props';
  const L0 = layer('ly_static_0', 'Static shapes', frames(24, {
    0: [
      pathStroke('s_static_0', rect(200, 100, 100, 100), { fill: '#ff3b30' }),
      pathStroke('s_static_1', [[350, 120], [420, 240], [500, 140]], { closed: false, stroke: '#1c7ed6', width: 6 }),
      pathStroke('s_static_2', [[60, 300], [160, 300], [110, 220]], { fill: '#34c759', stroke: '#000000', width: 2 }),
    ],
  }), { motionStatic: { position: [10, 20], anchor: [5, -5], rotation: [15], scale: [150, 100], opacity: [50] } });
  const L1 = layer('ly_static_1', 'Unanimated defaults', frames(24, { 0: [pathStroke('s_static_3', rect(20, 20, 40, 40), { fill: '#8e8e93' })] }));
  const checks = [];
  for (const prop of BASE_PROPS) for (const f of [0, 7, 23]) {
    checks.push(motionCheck('ly_static_0', prop, f, L0.motionStatic[prop] || PROP_DEFAULT[prop], 'static value is frame-independent'));
    checks.push(motionCheck('ly_static_1', prop, f, PROP_DEFAULT[prop], 'no track and no static value → documented default'));
  }
  checks.push(docCheck('layers[0].frames[0].strokes[0].hasRealStroke', false, 'fill-only path: hasRealStroke is authoritative'));
  checks.push(docCheck('layers[0].frames[0].strokes[1].closed', false));
  checks.push({ kind: 'bounds', layerUid: 'ly_static_0', strokeId: 's_static_1', expect: ref.pointsBounds([[350, 120], [420, 240], [500, 140]]), verify: 'node' });
  return {
    meta: {
      id, title: 'Static transform properties on a plain layer', coverage: ['static'],
      areas: ['animation-rigs-expressions', 'drawing-selection'], backend: 'document', requiredCapabilities: [],
      tolerance: TOL.exact, invariants: ['motionStatic values are returned unchanged at every frame', 'a property with neither track nor static value returns its documented default', 'fill-only paths keep hasRealStroke=false'],
      verification: { node: 'SMMotion.valueAtFrame (src/js/motion.js, unmodified) against motionStatic and PROP_DEFAULT', gate: null },
    },
    project: project({ layers: [L0, L1] }), expected: checks, assets: {},
  };
}

// --- 2. keyed properties ------------------------------------------------------

function keyedProps() {
  const id = 'keyed-props';
  const easeDefault = (t) => ref.defaultEaseAt(t);
  const A = { position: [{ frame: 0, v: [100, 100] }, { frame: 20, v: [300, 200] }], rotation: [{ frame: 0, v: [0] }, { frame: 20, v: [90] }],
    scale: [{ frame: 0, v: [100, 100] }, { frame: 20, v: [200, 50] }], opacity: [{ frame: 0, v: [100] }, { frame: 8, v: [0] }, { frame: 16, v: [100] }] };
  const LA = layer('ly_keyed_a', 'Default ease', frames(24, { 0: [pathStroke('s_keyed_a', rect(0, 0, 60, 60), { fill: '#ff9500' })] }), {
    motion: { position: keys(A.position), rotation: keys(A.rotation), scale: keys(A.scale), opacity: keys(A.opacity) } });
  const B = { rotation: [{ frame: 0, v: [0], curvePoints: ref.LINEAR_CURVE.map((p) => ({ x: p.x, y: p.y })) }, { frame: 20, v: [100] }] };
  const LB = layer('ly_keyed_b', 'Linear curve', frames(24, { 0: [pathStroke('s_keyed_b', rect(100, 0, 60, 60), { fill: '#5856d6' })] }), { motion: { rotation: keys(B.rotation) } });
  const C = { position: [{ frame: 0, v: [0, 0], hOut: [200, 0] }, { frame: 20, v: [400, 300], hIn: [0, -200] }] };
  const LC = layer('ly_keyed_c', 'Spatial bezier', frames(24, { 0: [pathStroke('s_keyed_c', rect(200, 0, 60, 60), { fill: '#af52de' })] }), { motion: { position: keys(C.position) } });
  const D = { position: [{ frame: 0, v: [0, 0] }, { frame: 8, v: [80, 0] }, { frame: 16, v: [80, 80] }] };
  const LD = layer('ly_keyed_d', 'Multi-segment', frames(24, { 0: [pathStroke('s_keyed_d', rect(300, 0, 60, 60), { fill: '#ff2d55' })] }), { motion: { position: keys(D.position) } });
  const checks = [];
  checks.push(...motionValueChecks('ly_keyed_a', { position: A.position, rotation: A.rotation, scale: A.scale }, [0, 5, 10, 15, 20, 23], easeDefault, 'default ease at its on-curve waypoints (t = 0, ¼, ½, ¾, 1); 23 clamps to the last key'));
  checks.push(...motionValueChecks('ly_keyed_a', { opacity: A.opacity }, [0, 2, 4, 6, 8, 10, 12, 14, 16, 20], easeDefault, 'two segments, waypoints of each'));
  for (let f = 0; f < 24; f++) checks.push(motionCheck('ly_keyed_b', 'rotation', f, ref.trackValueAt(B.rotation, f, ref.linearEaseAt), 'two-point curve is the identity ease'));
  for (const f of [0, 5, 10, 15, 20]) checks.push(motionCheck('ly_keyed_c', 'position', f, f >= 20 ? [400, 300] : ref.spatialPositionAt(C.position[0], C.position[1], f, easeDefault), 'cubic through hOut/hIn at the eased fraction'));
  checks.push(...motionValueChecks('ly_keyed_d', { position: D.position }, [0, 2, 4, 6, 8, 10, 12, 14, 16, 23], easeDefault, 'each segment eases independently'));
  return {
    meta: {
      id, title: 'Keyframed transform properties: default ease, linear curve, spatial bezier, multi-segment', coverage: ['keyed'],
      areas: ['animation-rigs-expressions', 'timeline-layers-frames'], backend: 'document', requiredCapabilities: [],
      tolerance: TOL.exact,
      invariants: ['values at keyframes equal the key values', 'the default ease passes through its documented waypoints', 'a two-point curve interpolates linearly', 'position handles form a cubic bezier', 'values clamp outside the key range'],
      verification: { node: 'SMMotion.valueAtFrame against reference.trackValueAt / spatialPositionAt', gate: null },
    },
    project: project({ layers: [LA, LB, LC, LD] }), expected: checks, assets: {},
  };
}

// --- 3. expression properties -------------------------------------------------

function expressionProps() {
  const id = 'expression-props';
  const fps = 24;
  const scaleKeys = [{ frame: 0, v: [100, 100] }, { frame: 20, v: [60, 100] }];
  const LE = layer('ly_expr_e', 'Expr', frames(24, { 0: [pathStroke('s_expr_0', rect(50, 50, 80, 80), { fill: '#ff9500' })] }), {
    motionStatic: { position: [50, 60] }, motion: { scale: keys(scaleKeys) },
    expressions: {
      rotation: { enabled: true, code: 'frame * 3' },
      position: { enabled: true, code: '[value[0] + frame * 2, value[1]]' },
      opacity: { enabled: true, code: 'clamp(time * 100, 0, 100)' },
      scale: { enabled: true, code: '[value[0] * 2, value[1]]' },
    } });
  const LX = layer('ly_expr_x', 'ExprCross', frames(24, { 0: [pathStroke('s_expr_1', rect(200, 50, 80, 80), { fill: '#5ac8fa' })] }), {
    motionStatic: { rotation: [1] }, expressions: { rotation: { enabled: true, code: 'layer("Expr").rotation * 2' } } });
  const LU = layer('ly_expr_u', 'ExprByUid', frames(24, { 0: [pathStroke('s_expr_2', rect(350, 50, 80, 80), { fill: '#4cd964' })] }), {
    expressions: { rotation: { enabled: true, code: 'layer("ly_expr_e").rotation + 1' } } });
  const LC = layer('ly_expr_c', 'ExprControl', frames(24, { 0: [pathStroke('s_expr_3', rect(500, 50, 80, 80), { fill: '#ffcc00' })] }), {
    exprControls: [{ key: 'xc_amount', name: 'Amount', type: 'number', default: 5 }], motionStatic: { xc_amount: [9] },
    expressions: { rotation: { enabled: true, code: 'control("Amount") * 2' } } });
  const LB = layer('ly_expr_b', 'ExprError', frames(24, { 0: [pathStroke('s_expr_4', rect(50, 200, 80, 80), { fill: '#8e8e93' })] }), {
    motionStatic: { rotation: [7] }, expressions: { rotation: { enabled: true, code: 'frame +' } } });
  const LD = layer('ly_expr_d', 'ExprDisabled', frames(24, { 0: [pathStroke('s_expr_5', rect(200, 200, 80, 80), { fill: '#8e8e93' })] }), {
    motionStatic: { rotation: [11] }, expressions: { rotation: { enabled: false, code: '999' } } });
  const checks = [];
  for (const f of [0, 1, 5, 12, 20, 23]) {
    checks.push(motionCheck('ly_expr_e', 'rotation', f, [f * 3], 'frame * 3'));
    checks.push(motionCheck('ly_expr_e', 'position', f, [50 + f * 2, 60], 'value is the static position'));
    checks.push(motionCheck('ly_expr_e', 'opacity', f, [Math.min(100, (f / fps) * 100)], 'time = frame / fps'));
    checks.push(motionCheck('ly_expr_x', 'rotation', f, [f * 6], 'cross-layer read by name'));
    checks.push(motionCheck('ly_expr_u', 'rotation', f, [f * 3 + 1], 'cross-layer read by layerUid'));
    checks.push(motionCheck('ly_expr_c', 'rotation', f, [18], 'control("Amount") reads the xc_ track'));
    checks.push(motionCheck('ly_expr_b', 'rotation', f, [7], 'syntax error → raw value, error recorded'));
    checks.push(motionCheck('ly_expr_d', 'rotation', f, [11], 'disabled expression → raw value'));
  }
  for (const f of [0, 5, 10, 15, 20]) {
    const raw = ref.trackValueAt(scaleKeys, f, (t) => ref.defaultEaseAt(t));
    checks.push(motionCheck('ly_expr_e', 'scale', f, [raw[0] * 2, raw[1]], '`value` is the keyed raw value at the frame'));
  }
  checks.push({ kind: 'expression-error', layerUid: 'ly_expr_b', prop: 'rotation', expectError: true, verify: 'node', note: 'lastError is set after evaluation' });
  checks.push({ kind: 'expression-error', layerUid: 'ly_expr_e', prop: 'rotation', expectError: false, verify: 'node' });
  return {
    meta: {
      id, title: 'Expressions: arithmetic, time, keyed value, cross-layer, controls, error fallback', coverage: ['expression'],
      areas: ['animation-rigs-expressions'], backend: 'document', requiredCapabilities: [], tolerance: TOL.exact,
      invariants: ['an expression result replaces the raw value', 'time equals frame / fps', 'a syntax error leaves the raw value and records lastError', 'a disabled expression is ignored', 'random vocabulary (wiggle/random) is deliberately absent: its seed is session state, not document state'],
      verification: { node: 'SMMotion.valueAtFrame with the shipped expression compiler', gate: null },
    },
    project: project({ layers: [LE, LX, LU, LC, LB, LD], fps }), expected: checks, assets: {},
  };
}

// --- 4. held frames -----------------------------------------------------------

function heldFrames() {
  const id = 'held-frames';
  const keyed = {};
  const keyFrames = [0, 6, 12, 18];
  keyFrames.forEach((k) => { keyed[k] = [pathStroke(`s_held_${k}`, rect(100 + 10 * k, 100, 80, 80), { fill: '#ff3b30' })]; });
  const L0 = layer('ly_held_0', 'On sixes', frames(24, keyed));
  const L1 = layer('ly_held_1', 'Blank start', frames(24, { 8: [pathStroke('s_held_b8', rect(300, 100, 80, 80), { fill: '#34c759' })], 16: [] }));
  L1.frames[16].isKeyframe = true;
  const L2 = layer('ly_held_2', 'In/out gate', frames(24, { 0: [pathStroke('s_held_g0', rect(500, 100, 80, 80), { fill: '#007aff' })] }), { inPoint: 4, outPoint: 19 });
  const holdPos = [{ frame: 0, v: [0, 0], hold: true }, { frame: 6, v: [50, 0] }, { frame: 12, v: [100, 0] }];
  const holdOp = [{ frame: 0, v: [100], hold: true }, { frame: 12, v: [0], hold: true }, { frame: 23, v: [100] }];
  const L3 = layer('ly_held_3', 'Hold keys', frames(24, { 0: [pathStroke('s_held_h', rect(100, 250, 80, 80), { fill: '#af52de' })] }), { motion: { position: keys(holdPos), opacity: keys(holdOp) } });
  const checks = [];
  for (let f = 0; f < 24; f++) {
    checks.push({ kind: 'effective-keyframe', layerUid: 'ly_held_0', frame: f, expectKeyframe: ref.heldKeyframeAt(keyFrames, f), verify: 'node', note: 'last keyframe at or before the frame' });
    const b = ref.heldKeyframeAt([0, 8, 16], f);
    checks.push({ kind: 'effective-keyframe', layerUid: 'ly_held_1', frame: f, expectKeyframe: b, verify: 'node', note: 'blank keyframes hold emptiness' });
    checks.push({ kind: 'effective-keyframe', layerUid: 'ly_held_2', frame: f, expectKeyframe: ref.visibleInRange(4, 19, f, 24) ? 0 : null, verify: 'node', note: 'outside [inPoint,outPoint] a layer shows nothing' });
    checks.push(motionCheck('ly_held_3', 'opacity', f, ref.trackValueAt(holdOp, f, (t) => ref.defaultEaseAt(t)), 'hold keys step'));
  }
  for (const f of [0, 3, 5, 6, 9, 12, 20]) checks.push(motionCheck('ly_held_3', 'position', f, ref.trackValueAt(holdPos, f, (t) => ref.defaultEaseAt(t)), 'hold then eased segment'));
  return {
    meta: {
      id, title: 'Held drawing keyframes, blank holds, in/out gating, hold keys', coverage: ['held-frames'],
      areas: ['timeline-layers-frames', 'animation-rigs-expressions'], backend: 'document', requiredCapabilities: [], tolerance: TOL.exact,
      invariants: ['a non-keyframe shows the nearest earlier keyframe', 'an empty keyframe holds emptiness', 'frames outside in/out show nothing', 'a hold key pins its value until the next key'],
      verification: { node: 'getEffectiveStrokes (src/js/app.js, lifted) and SMMotion.valueAtFrame', gate: null },
    },
    project: project({ layers: [L0, L1, L2, L3] }), expected: checks, assets: {},
  };
}

// --- 5. components ------------------------------------------------------------

function components() {
  const id = 'components';
  const innerA = { name: 'Inner A', visible: true, locked: false, frames: [], color: '#4a9eff', effects: [], layerUid: 'ly_comp_ia' };
  for (let i = 0; i < 12; i++) innerA.frames.push({ strokes: [pathStroke(`s_comp_a_${i}`, rect(40 + 20 * i, 40, 60, 60), { fill: '#ff9500' })], isKeyframe: true, isInterpolated: false });
  const innerB = { name: 'Inner B', visible: true, locked: false, frames: frames(12, { 0: [pathStroke('s_comp_b_0', rect(40, 160, 60, 60), { fill: '#5856d6' })] }), color: '#4a9eff', effects: [], layerUid: 'ly_comp_ib' };
  const symbols = { sym_comp: { name: 'Comp', totalFrames: 12, fps: 24, layers: [innerA, innerB] } };
  const inst = (uid, name, extra) => layer(uid, name, frames(24), Object.assign({ symbolId: 'sym_comp', symSpeed: 1, symPlacedAt: 0, symSingleFrame: 0, locked: true }, extra));
  const L = [
    inst('ly_comp_loop', 'Loop @4', { symPlayMode: 'loop', symPlacedAt: 4 }),
    inst('ly_comp_once', 'Once', { symPlayMode: 'once' }),
    inst('ly_comp_ping', 'Pingpong', { symPlayMode: 'pingpong' }),
    inst('ly_comp_single', 'Single 5', { symPlayMode: 'single', symSingleFrame: 5 }),
    inst('ly_comp_speed', 'Loop speed 2', { symPlayMode: 'loop', symSpeed: 2 }),
    inst('ly_comp_hold', 'Held internal 3', { symPlayMode: 'loop' }),
    inst('ly_comp_blank', 'Blank override', { symPlayMode: 'loop' }),
  ];
  L[5].frames[0].componentFrame = 3;
  L[6].frames[12] = { strokes: [], isKeyframe: true, isInterpolated: false, blankOverride: true };
  const modes = { ly_comp_loop: ['loop', { placedAt: 4 }], ly_comp_once: ['once', {}], ly_comp_ping: ['pingpong', {}], ly_comp_single: ['single', { singleFrame: 5 }], ly_comp_speed: ['loop', { speed: 2 }] };
  const checks = [];
  for (let f = 0; f < 24; f++) {
    for (const [uid, [mode, o]] of Object.entries(modes)) {
      const ii = ref.symbolFrameAt(mode, 12, f, o);
      checks.push({ kind: 'symbol-frame', layerUid: uid, frame: f, expectInternalFrame: ii, expectStrokeIds: [`s_comp_a_${ii}`, 's_comp_b_0'], verify: 'node', note: `${mode} play mode` });
    }
    checks.push({ kind: 'symbol-frame', layerUid: 'ly_comp_hold', frame: f, expectInternalFrame: 3, expectStrokeIds: ['s_comp_a_3', 's_comp_b_0'], verify: 'node', note: 'componentFrame on the outer keyframe pins the internal frame' });
    checks.push({ kind: 'symbol-frame', layerUid: 'ly_comp_blank', frame: f, expectInternalFrame: f < 12 ? f % 12 : null, expectStrokeIds: f < 12 ? [`s_comp_a_${f % 12}`, 's_comp_b_0'] : [], verify: 'node', note: 'blankOverride keyframe hides the instance for its span' });
  }
  return {
    meta: {
      id, title: 'Component instances: loop/once/pingpong/single, placement, speed, held internal frame, blank override', coverage: ['components'],
      areas: ['timeline-layers-frames', 'storyboard'], backend: 'document', requiredCapabilities: [], tolerance: TOL.exact,
      invariants: ['every visible sub-layer of the component composites', 'play mode formulas match resolveSymbolFrameIdx\'s documented behaviour', 'componentFrame and blankOverride on outer keyframes are honoured'],
      verification: { node: 'getEffectiveStrokes + resolveSymbolFrameIdx (src/js/app.js, lifted) against reference.symbolFrameAt', gate: null },
    },
    project: project({ layers: L, symbols }), expected: checks, assets: {},
  };
}

// --- 6. masks and alpha -------------------------------------------------------

function masksAlpha() {
  const id = 'masks-alpha';
  const white = [255, 255, 255];
  const L0 = layer('ly_mask_content', 'Green fill (alpha matte)', frames(24, { 0: [pathStroke('s_mask_0', rect(0, 0, 640, 360), { fill: '#00c853' })] }), { matteMode: 'alpha', matteSourceLayerUid: 'ly_mask_source' });
  const L1 = layer('ly_mask_source', 'Matte shape', frames(24, { 0: [pathStroke('s_mask_1', rect(220, 80, 200, 200), { fill: '#000000' })] }));
  const L2 = layer('ly_mask_alpha', 'Alpha fill', frames(24, { 0: [pathStroke('s_mask_2', rect(40, 40, 100, 100), { fill: '#ff000080' })] }));
  const L3 = layer('ly_mask_opacity', 'Layer opacity 50%', frames(24, { 0: [pathStroke('s_mask_3', rect(500, 40, 100, 100), { fill: '#0000ff' })] }), { motionStatic: { opacity: [50] } });
  const L4 = layer('ly_mask_inlayer', 'In-layer mask', frames(24, { 0: [
    pathStroke('s_mask_4', rect(40, 220, 200, 100), { fill: '#ff9500' }),
    pathStroke('s_mask_5', rect(140, 220, 100, 100), { extra: { isMask: true, maskMode: 'add' } }),
  ] }));
  const checks = [
    pixelCheck(320, 180, [0, 200, 83], 'inside the alpha matte: content shows'),
    pixelCheck(100, 180, white, 'outside the matte: content clipped, matte source not drawn'),
    pixelCheck(90, 90, ref.over(ref.hexToRgba('#ff000080'), white), '50% alpha red over white'),
    pixelCheck(550, 90, ref.over(ref.hexToRgba('#0000ff'), white, 0.5), 'layer opacity 50% over white'),
    pixelCheck(190, 270, [255, 149, 0], 'inside the add mask: orange visible'),
    pixelCheck(90, 270, white, 'outside the add mask: clipped'),
    docCheck('layers[0].matteMode', 'alpha'),
    docCheck('layers[0].matteSourceLayerUid', 'ly_mask_source', 'matte references its source by uid'),
    docCheck('layers[2].frames[0].strokes[0].fillColor', '#ff000080', 'hex8 alpha survives in the document'),
    docCheck('layers[4].frames[0].strokes[1].isMask', true),
    { kind: 'alpha', hex: '#ff000080', expect: ref.hexToRgba('#ff000080'), verify: 'node', note: 'reference hex8 parse' },
  ];
  return {
    meta: {
      id, title: 'Alpha track matte, hex8 fill alpha, layer opacity, in-layer add mask', coverage: ['masks-alpha'],
      areas: ['effects-masks', 'drawing-selection'], backend: 'engine', requiredCapabilities: ['webgpu'], tolerance: TOL.pixel,
      invariants: ['a matte clips its content to the source alpha and the source itself is not drawn', 'an 8-digit hex fill keeps its alpha through save/load', 'layer opacity multiplies item alpha', 'a path tagged isMask never renders as content'],
      verification: { node: 'document fields and reference colour maths', gate: 'R13: render each pixel check on the engine and compare within tolerance' },
    },
    project: project({ layers: [L0, L1, L2, L3, L4] }), expected: checks, assets: {},
  };
}

// --- 7. text ------------------------------------------------------------------

function text() {
  const id = 'text';
  // The block is what buildVectorTextBlock (src/js/vector-text-bridge.js) writes:
  // every glyph carries isVectorText/vectorChar/charIndex and the block's
  // `groupId` (the key serP persists and desP restores; vectorTextGroupMembers
  // filters siblings on it), and the FIRST glyph is the root that carries the
  // text-level fields. A separate root item or a differently named group key
  // would not be recognised by production (review finding on 93d6d2a).
  const glyph = (i, ch) => pathStroke(`s_text_${i}`, rect(100 + 40 * i, 100, 30, 48), { fill: '#222222', extra: Object.assign({ isVectorText: true, vectorChar: ch, isText: true, charIndex: i, wordIndex: 0, lineIndex: 0, groupId: 'tg_text_0' },
    i === 0 ? { isTextRoot: true, text: 'NEM', vectorFont: 'sans-serif', textSize: 48, textColor: '#222222', textAlign: 'left', anchorTopLeft: { x: 100, y: 100 } } : {}) });
  const L0 = layer('ly_text_vec', 'Vector text', frames(24, { 0: [glyph(0, 'N'), glyph(1, 'E'), glyph(2, 'M')] }), {
    isTextLayer: true,
    textAnimators: [{ id: 'ta_text_0', enabled: true, selector: { selectorType: 'range', start: 0, end: 100, offset: 0, units: 'percent', basedOn: 'chars', shape: 'square', amount: 100, easeHigh: 0, easeLow: 0, smooth: 100 }, props: { position: [0, -20], scale: [100, 100], rotation: 0, opacity: 100 } }],
  });
  const one = png.encodeRGBA(1, 1, Buffer.from([0, 0, 0, 0]));
  const L1 = layer('ly_text_raster', 'Raster text', frames(24, { 0: [rasterStroke({ x: 320, y: 260, width: 200, height: 60, src: png.toDataUrl(one), extra: { isText: true, text: 'Nemo', font: 'sans-serif', size: 40, color: '#1c7ed6', align: 'center' } })] }));
  const checks = [
    docCheck('layers[0].isTextLayer', true),
    docCheck('layers[0].frames[0].strokes[0].isTextRoot', true),
    docCheck('layers[0].frames[0].strokes[0].text', 'NEM'),
    docCheck('layers[0].frames[0].strokes[2].vectorChar', 'M'),
    docCheck('layers[0].frames[0].strokes[2].charIndex', 2),
    docCheck('layers[0].textAnimators[0].props.position', [0, -20]),
    docCheck('layers[1].frames[0].strokes[0].text', 'Nemo'),
    docCheck('layers[1].frames[0].strokes[0].size', 40),
    { kind: 'bounds', layerUid: 'ly_text_vec', strokeId: 's_text_2', expect: ref.pointsBounds(rect(180, 100, 30, 48)), verify: 'node' },
    { kind: 'text-units', layerUid: 'ly_text_vec', expectCharIndices: [0, 1, 2], verify: 'node', note: 'unitIndexOf (src/js/text-selector.js) returns the stamped char index' },
    { kind: 'text-group', layerUid: 'ly_text_vec', rootStrokeId: 's_text_0', expectMembers: ['s_text_0', 's_text_1', 's_text_2'], verify: 'node', note: 'after the desP field mapping, vectorTextGroupMembers(root) (src/js/vector-text-bridge.js, lifted) returns the whole block and the root is its first glyph' },
    pixelCheck(115, 124, [34, 34, 34], 'glyph N box is filled #222222 before animators move it', 0),
  ];
  return {
    meta: {
      id, title: 'Vector text run with a range animator and a raster text item', coverage: ['text'],
      areas: ['drawing-selection', 'animation-rigs-expressions'], backend: 'document', requiredCapabilities: [], tolerance: TOL.exact,
      invariants: ['text root fields round-trip', 'per-glyph char/word/line indices round-trip', 'every glyph shares the persisted groupId and the first glyph is the root, so production regroups the block', 'animator declarations persist on the layer'],
      verification: { node: 'document fields, bounds, text-selector unit indices', gate: 'R13: animator weights and rendered glyph placement (glyph outlines here are boxes standing in for font geometry; no visual oracle is invented for real fonts)' },
    },
    project: project({ layers: [L0, L1] }), expected: checks, assets: {},
  };
}

// --- 8. image mesh ------------------------------------------------------------

function mesh() {
  const id = 'mesh';
  const image = halfImage(64, 64);
  const rectDisplay = { x: 320, y: 180, width: 200, height: 200 };
  // 2×2 mesh over the whole image: 8 outline points (rect densified to two
  // steps per side, clockwise from the top-left, the order createMesh uses)
  // plus one interior point; a fan of 8 triangles around the centre is a
  // valid Delaunay triangulation of that set.
  const outline = [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5]];
  const verts = outline.concat([[0.5, 0.5]]);
  const tris = [];
  for (let i = 0; i < 8; i++) tris.push(8, i, (i + 1) % 8);
  const offsets = verts.map(() => [0, 0]); offsets[8] = [0.1, 0];
  const imageMeshes = { im_1: { outline, verts, tris, offsets, cols: 2, rows: 2 } };
  const fr = [];
  for (let i = 0; i < 24; i++) fr.push({ strokes: [rasterStroke(Object.assign({ src: png.toDataUrl(image), extra: { meshId: 'im_1' } }, rectDisplay))], isKeyframe: i === 0, isInterpolated: false });
  const L0 = layer('ly_mesh_0', 'Meshed image', fr);
  const checks = [
    { kind: 'mesh', meshId: 'im_1', outlineCount: 8, vertexCount: 9, triangleCount: 8, verify: 'node', note: 'outline prefix, valid indices, centroids inside the outline, offsets aligned with verts' },
    { kind: 'mesh-vertex-world', meshId: 'im_1', index: 8, rect: rectDisplay, expectRest: ref.meshVertexWorld(rectDisplay, 0.5, 0.5), expectDeformed: ref.meshVertexWorld(rectDisplay, 0.6, 0.5), verify: 'node' },
    { kind: 'mesh-propagated', layerUid: 'ly_mesh_0', meshId: 'im_1', verify: 'node', note: 'meshId must be on the raster of EVERY frame (CLAUDE.md §12)' },
    pixelCheck(240, 180, [220, 30, 30], 'far left of the image: red half, unaffected by the centre offset'),
    pixelCheck(400, 180, [30, 60, 220], 'far right: blue half'),
    pixelCheck(150, 180, [255, 255, 255], 'outside the outline: masked'),
  ];
  return {
    meta: {
      id, title: 'Image mesh: rectangular outline, 2×2 grid, static centre offset', coverage: ['mesh', 'media'],
      areas: ['media-import-export', 'effects-masks'], backend: 'engine', requiredCapabilities: ['webgpu'], tolerance: TOL.pixel,
      invariants: ['verts[0..outline.length) are the outline points in order', 'triangle indices are in range and centroids lie inside the outline', 'the mesh is normalised over the display rect'],
      verification: { node: 'topology invariants with reference.pointInPolygon and meshVertexWorld; SMImageMesh.load (src/js/image-mesh.js) accepts the entry', gate: 'R13: pixel checks on the engine\'s draw_image_mesh path' },
      assets: ['assets/image.png'],
    },
    project: project({ layers: [L0], imageMeshes }), expected: checks, assets: { 'assets/image.png': image },
  };
}

// --- 9. media -----------------------------------------------------------------

function media() {
  const id = 'media';
  const embedded = halfImage(64, 48);
  const linked = halfImage(32, 24);
  const L0 = layer('ly_media_embedded', 'Embedded image', frames(24, { 0: [rasterStroke({ x: 200, y: 150, width: 128, height: 96, src: png.toDataUrl(embedded) })] }), { footage: { kind: 'still' } });
  const L1 = layer('ly_media_linked', 'Linked image', frames(24, { 0: [rasterStroke({ x: 450, y: 150, width: 64, height: 48, extra: { linked: true, linkedPath: 'tests/fixtures/media/assets/linked.png' } })] }), { footage: { kind: 'still' } });
  const mediaLibrary = [
    { id: 'ml_media_0', name: 'embedded.png', kind: 'image', layerName: 'Embedded image', layerUid: 'ly_media_embedded', linked: false, naturalW: 64, naturalH: 48, sizeBytes: embedded.length, importedAt: 0 },
    { id: 'ml_media_1', name: 'linked.png', kind: 'image', layerName: 'Linked image', layerUid: 'ly_media_linked', linked: true, path: 'tests/fixtures/media/assets/linked.png', naturalW: 32, naturalH: 24, sizeBytes: linked.length, importedAt: 0 },
  ];
  const checks = [
    docCheck('layers[0].frames[0].strokes[0].isRaster', true),
    docCheck('layers[1].frames[0].strokes[0].linked', true),
    docCheck('layers[1].frames[0].strokes[0].src', undefined, 'a linked raster never embeds bytes'),
    { kind: 'data-url-asset', layerUid: 'ly_media_embedded', asset: 'assets/embedded.png', verify: 'node', note: 'embedded data URL bytes equal the committed asset' },
    { kind: 'image-size', asset: 'assets/embedded.png', expect: { width: 64, height: 48 }, verify: 'node' },
    { kind: 'image-size', asset: 'assets/linked.png', expect: { width: 32, height: 24 }, verify: 'node' },
    pixelCheck(170, 150, [220, 30, 30], 'embedded image left half (2× display scale)'),
    pixelCheck(230, 150, [30, 60, 220], 'embedded image right half'),
    pixelCheck(450, 150, [255, 255, 255], 'linked image unresolved in a harness without the file → placeholder draws nothing'),
  ];
  return {
    meta: {
      id, title: 'Embedded raster and linked raster with media library entries', coverage: ['media'],
      areas: ['media-import-export', 'project-lifecycle-integrations'], backend: 'engine', requiredCapabilities: ['webgpu'], tolerance: TOL.pixel,
      invariants: ['an embedded raster carries its bytes as a data URL', 'a linked raster carries a reference and no bytes', 'the media library mirrors both'],
      verification: { node: 'document fields, asset hashes, decoded image sizes', gate: 'R13: engine pixel checks; R20: missing-asset behaviour for the linked entry' },
      assets: ['assets/embedded.png', 'assets/linked.png'],
    },
    project: project({ layers: [L0, L1], mediaLibrary }), expected: checks, assets: { 'assets/embedded.png': embedded, 'assets/linked.png': linked },
  };
}

// --- 10. migration ------------------------------------------------------------

function migration() {
  const id = 'migration';
  const legacyKeys = [{ frame: 0, v: [0, 0], curvePoints: LEGACY_STEP_CURVE.map((p) => ({ x: p.x, y: p.y })) }, { frame: 10, v: [100, 0], curvePoints: [{ x: 0, y: 0 }, { x: 0.4, y: 0.1, tx: 0.2 }, { x: 1, y: 1 }] }, { frame: 20, v: [0, 0] }];
  const L0 = layer('ly_mig_curve', 'Legacy curve', frames(24, { 0: [pathStroke('s_mig_0', rect(40, 40, 60, 60), { fill: '#ff9500' })] }), { motion: { position: keys(legacyKeys) } });
  const L1 = layer('ly_mig_parent', 'Time parent', frames(24, { 0: [pathStroke('s_mig_1', rect(140, 40, 60, 60), { fill: '#5856d6' })] }));
  const L2 = layer('ly_mig_timelink', 'Time link', frames(24, { 0: [pathStroke('s_mig_2', rect(240, 40, 60, 60), { fill: '#34c759' })] }), { timeLink: { parentUid: 'ly_mig_parent', inOffset: 3, outOffset: -2 } });
  const L3 = layer('ly_mig_dup', 'Duplicator with legacy effector', frames(24, { 0: [pathStroke('s_mig_3', rect(340, 40, 60, 60), { fill: '#af52de' })] }), {
    locked: true,
    duplicator: { mode: 'grid', rows: 1, cols: 3, spacingX: 150, spacingY: 150, count: 3, radius: 200, startAngle: 0, endAngle: null, radialOrient: false, pathLayerUid: null, pathAlignTangent: true, seed: 7,
      staggerRandom: { position: false, rotation: false, scale: false, opacity: false, hue: false }, timeOffset: { enabled: false, offsetFrames: 1, direction: 'forward' }, sourceMode: 'shape', sourceLayerUids: [],
      effectors: [{ offsetPos: [10, 0], offsetRot: 5 }] } });
  const legacyFill = pathStroke('s_mig_4', rect(40, 200, 60, 60), { fill: '#ffcc00' });
  delete legacyFill.hasRealStroke; legacyFill.strokeColor = null; // pre-hasRealStroke file
  const L4 = layer('ly_mig_matte', 'Matte without uid', frames(24, { 0: [legacyFill] }), { matteMode: 'alpha' });
  const L5 = layer('ly_mig_above', 'Matte source (above)', frames(24, { 0: [pathStroke('s_mig_5', rect(40, 200, 30, 60), { fill: '#000000' })] }));
  const L6 = layer('ly_mig_short', 'Short frames', frames(12, { 0: [pathStroke('s_mig_6', rect(140, 200, 60, 60), { fill: '#8e8e93' })] }));
  L6.frames.forEach((f) => { delete f.isInterpolated; });
  const p = project({ layers: [L0, L1, L2, L3, L4, L5, L6], extra: { version: 12, easingCurve: { points: [{ x: 0, y: 0 }, { x: 0.42, y: 0 }, { x: 0.58, y: 1 }, { x: 1, y: 1 }] }, tweenOverrides: { '0:0-6': { mode: 'auto' } } } });
  const checks = [
    { kind: 'migration', op: 'legacyCurve', layerUid: 'ly_mig_curve', expect: { key0: DEFAULT_CURVE, key1: legacyKeys[1].curvePoints, migratedCount: 1 }, verify: 'node', note: 'only the untouched legacy default is replaced' },
    { kind: 'migration', op: 'timeLinkOffsets', layerUid: 'ly_mig_timelink', expect: { timeLinkInOffset: [3], timeLinkOutOffset: [-2] }, verify: 'node' },
    { kind: 'migration', op: 'effectorChannels', layerUid: 'ly_mig_dup', expect: [{ prop: 'position', value: [10, 0] }, { prop: 'rotation', value: [5] }], verify: 'node' },
    { kind: 'migration', op: 'matteSourceUid', layerUid: 'ly_mig_matte', expect: 'ly_mig_above', verify: 'gate', gate: 'R12 document contract (importJSON)', note: 'matte without a source uid binds to the layer directly above' },
    { kind: 'migration', op: 'easingCurveDefault', expect: [{ x: 0, y: 0 }, { x: 0.3, y: 0.05 }, { x: 0.7, y: 0.95 }, { x: 1, y: 1 }], verify: 'gate', gate: 'R12 document contract (importJSON)' },
    { kind: 'migration', op: 'legacyStrokeFallback', layerUid: 'ly_mig_matte', strokeId: 's_mig_4', expect: { strokeColor: '#fff' }, verify: 'gate', gate: 'R12 document contract (desP)', note: 'a fill without hasRealStroke keeps the historical white fallback' },
    { kind: 'migration', op: 'tweenSpanKey', expect: 'ly_mig_curve:0-6', verify: 'gate', gate: 'R12 document contract (importJSON)' },
    { kind: 'migration', op: 'framePadding', layerUid: 'ly_mig_short', expect: { frames: 24, isInterpolated: false }, verify: 'gate', gate: 'R12 document contract (importJSON)' },
    docCheck('version', 12, 'older format version loads with defaults'),
  ];
  return {
    meta: {
      id, title: 'Older-schema document: legacy curve, time link offsets, legacy effector, matte without uid, old easing default, frame padding', coverage: ['migration'],
      areas: ['project-lifecycle-integrations', 'animation-rigs-expressions'], backend: 'document', requiredCapabilities: [], tolerance: TOL.exact,
      invariants: ['a hand-edited curve is never rewritten', 'legacy fields fold into their current representation exactly once', 'a shorter frame list is padded to totalFrames'],
      verification: { node: 'SMMotion.migrateLegacyCurves, migrateTimeLinkOffsets, effectorChannels (unmodified production code)', gate: 'R12: full importJSON contract for the gated checks' },
    },
    project: p, expected: checks, assets: {},
  };
}

// --- 11. interaction ----------------------------------------------------------

function interaction() {
  const id = 'interaction';
  const rng = mulberry32(seedFrom(id));
  const events = [];
  const n = 120, dt = 1000 / 240;
  const pts = [];
  for (let i = 0; i <= n; i++) {
    const t = i / n;
    const x = 120 + 400 * t, y = 180 + 80 * Math.sin(t * Math.PI * 2) + rng.float(-0.5, 0.5, 2);
    pts.push([Math.round(x * 100) / 100, Math.round(y * 100) / 100]);
  }
  events.push({ t: 0, type: 'pointerdown', x: pts[0][0], y: pts[0][1], pressure: 0.3, pointerType: 'pen', buttons: 1 });
  for (let i = 1; i < n; i++) events.push({ t: Math.round(i * dt * 100) / 100, type: 'pointermove', x: pts[i][0], y: pts[i][1], pressure: Math.round((0.3 + 0.6 * Math.sin((i / n) * Math.PI)) * 100) / 100, pointerType: 'pen', buttons: 1 });
  events.push({ t: Math.round(n * dt * 100) / 100, type: 'pointerup', x: pts[n][0], y: pts[n][1], pressure: 0, pointerType: 'pen', buttons: 0 });
  const bounds = ref.pointsBounds(pts);
  const gesture = {
    schema: 'nemo.interaction/1', fixture: id, canvas: { width: 640, height: 360, zoom: 1, pan: [0, 0] }, seed: rng.seed,
    steps: [
      { step: 'draw', tool: 'pen', settings: { strokeWidth: 6, strokeColor: '#1c7ed6', smoothing: 'default' }, layerUid: 'ly_int_0', frame: 0, events },
      { step: 'command', name: 'undo' },
    ],
  };
  const L0 = layer('ly_int_0', 'Drawing', frames(24));
  const checks = [
    { kind: 'interaction-schema', file: 'gesture.json', expectEvents: events.length, verify: 'node', note: 'timestamps monotonic, coordinates inside the canvas' },
    { kind: 'interaction', after: 'draw', layerUid: 'ly_int_0', frame: 0, expect: { strokesAdded: 1, minSegments: 8, boundsWithin: { x: bounds.x - 4, y: bounds.y - 4, width: bounds.width + 8, height: bounds.height + 8 } }, verify: 'gate', gate: 'R12 record/replay harness', note: 'bounds derive from the recorded events, not from the smoother' },
    { kind: 'interaction', after: 'undo', layerUid: 'ly_int_0', frame: 0, expect: { strokes: 0 }, verify: 'gate', gate: 'R12 record/replay harness' },
  ];
  return {
    meta: {
      id, title: 'Recorded pen gesture (240 Hz, pressure ramp) followed by undo', coverage: ['interaction'],
      areas: ['drawing-selection'], backend: 'browser', requiredCapabilities: ['browser-harness'], tolerance: 4,
      invariants: ['one gesture creates exactly one stroke', 'the stroke stays within the gesture bounds plus half the stroke width', 'undo removes it'],
      verification: { node: 'gesture schema and consistency with the recorded bounds', gate: 'R12: replay through the real pointer handlers (test:browser)' },
      assets: ['gesture.json'],
    },
    project: project({ layers: [L0] }), expected: checks, assets: { 'gesture.json': Buffer.from(JSON.stringify(gesture, null, 1) + '\n') },
  };
}

// --- 12. export ---------------------------------------------------------------

// export.js argument lists, verbatim (exportMP4ToPath, exportProResToPath,
// exportTIFSequence). `{frames}` and `{out}` are substituted by the harness.
// Probes are what ffprobe reports for the ENCODED stream. `-pix_fmt` selects the
// encoder's input format; the decoder reports the container's native format, so
// ProRes 4444 comes back as its 12-bit 4444 format whatever the 10-bit input —
// the invariants for the alpha export are the 4444 profile and an alpha plane
// (`yuva…`), which is why that probe accepts either 4444 format. Found by running
// the fixture against the committed sidecar (the 10-bit expectation was wrong).
const EXPORT_ARGS = {
  'mp4-h264-videotoolbox': { args: ['-y', '-framerate', '{fps}', '-i', '{frames}/frame_%04d.png', '-c:v', 'h264_videotoolbox', '-pix_fmt', 'yuv420p', '-q:v', '65', '-profile:v', 'high', '{out}.mp4'], probe: { codec_name: 'h264', profile: 'High', pix_fmt: 'yuv420p', width: 320, height: 240, nb_frames: 12 }, requires: ['videotoolbox'], source: 'src/js/export.js exportMP4ToPath (quality high → -q:v 65)' },
  'mov-prores-4444': { args: ['-y', '-framerate', '{fps}', '-i', '{frames}/frame_%04d.png', '-c:v', 'prores_ks', '-profile:v', '4', '-pix_fmt', 'yuva444p10le', '{out}.mov'], probe: { codec_name: 'prores', profile: '4444', pix_fmt: { oneOf: ['yuva444p12le', 'yuva444p10le'] }, width: 320, height: 240, nb_frames: 12 }, requires: [], source: 'src/js/export.js exportProResToPath (alpha)' },
  'mov-prores-422': { args: ['-y', '-framerate', '{fps}', '-i', '{frames}/frame_%04d.png', '-c:v', 'prores_ks', '-profile:v', '3', '-pix_fmt', 'yuv422p10le', '{out}.mov'], probe: { codec_name: 'prores', profile: 'HQ', pix_fmt: 'yuv422p10le', width: 320, height: 240, nb_frames: 12 }, requires: [], source: 'src/js/export.js exportProResToPath (no alpha)' },
  'tif-sequence': { args: ['-y', '-start_number', '1', '-i', '{frames}/frame_%04d.png', '-start_number', '1', '{out}/frame_%04d.tif'], files: 12, requires: [], source: 'src/js/export.js exportTIFSequence' },
};

function exportFixture() {
  const id = 'export';
  const W = 320, H = 240, N = 12, fps = 24;
  const assets = {};
  const strokesAt = (i) => [pathStroke(`s_export_${i}`, rect(20 + 20 * i, 100, 40, 40), { fill: '#dc1e1e' }), pathStroke(`s_export_bar_${i}`, rect(0, H - 8, 20 * (i + 1), 8), { fill: '#000000' })];
  const keyed = {};
  for (let i = 0; i < N; i++) {
    const img = png.canvas(W, H);
    png.fillRect(img, 20 + 20 * i, 100, 40, 40, [220, 30, 30]);
    png.fillRect(img, 0, H - 8, 20 * (i + 1), 8, [0, 0, 0]);
    assets[`assets/frames/frame_${String(i + 1).padStart(4, '0')}.png`] = png.encodeRGBA(W, H, img.rgba);
    keyed[i] = strokesAt(i);
  }
  const L0 = layer('ly_export_0', 'Moving square', frames(N, keyed));
  const checks = [
    { kind: 'frame-pixels', frame: 1, asset: 'assets/frames/frame_0001.png', samples: [{ x: 40, y: 120, rgb: [220, 30, 30] }, { x: 100, y: 120, rgb: [255, 255, 255] }, { x: 10, y: 236, rgb: [0, 0, 0] }], verify: 'node', note: 'committed frames decode to the painted pixels' },
  ];
  for (const [name, spec] of Object.entries(EXPORT_ARGS)) {
    checks.push({ kind: 'encode', encoder: name, args: spec.args, fps, frames: N, probe: spec.probe || null, files: spec.files || null, requires: spec.requires, source: spec.source, verify: 'node-when-available', requiredCapabilities: ['ffmpeg-sidecar'].concat(spec.requires), note: 'exact export.js argument list; skipped with reason when no runnable sidecar or ffprobe is present' });
  }
  checks.push({ kind: 'decode-sample', encoder: 'mp4-h264-videotoolbox', frame: 1, samples: [{ x: 40, y: 120, rgb: [220, 30, 30], tolerance: 24 }, { x: 160, y: 60, rgb: [255, 255, 255], tolerance: 8 }], verify: 'node-when-available', note: 'lossy 4:2:0 encode: wide tolerance on the coloured sample' });
  return {
    meta: {
      id, title: 'Twelve-frame PNG sequence encoded with the exact export.js argument lists', coverage: ['export'],
      areas: ['media-import-export'], backend: 'sidecar', requiredCapabilities: ['ffmpeg-sidecar', 'videotoolbox'], tolerance: 24,
      invariants: ['frame count, size, codec and pixel format of each output follow from the input and the argument list', 'the encoded first frame still shows the painted square'],
      verification: { node: 'frame decode now; encode + ffprobe + decode sample when a runnable ffmpeg is available (committed sidecar, or NEMO_FFMPEG for an explicit binary whose identity is recorded)', gate: 'R13/R21: the same checks driven through the packaged UI export' },
      assets: Object.keys(assets),
    },
    project: project({ layers: [L0], totalFrames: N, fps, canvasW: W, canvasH: H }), expected: checks, assets,
  };
}

// --- synthetic workload documents (tests/bench, not committed) -------------------

// A large project in the same format: `layers` layers × `total` frames,
// keyframes every `keyEvery` frames with `strokesPerKey` bezier polylines,
// Motion keys on every layer and an expression on every fifth. Deterministic
// for a seed; the manifest records its SHA-256 so a workload is bound to the
// exact document it measured.
function benchDocument({ layers = 40, total = 24, keyEvery = 6, strokesPerKey = 50, seed = seedFrom('bench-vectors'), withImages = 0 } = {}) {
  const rng = mulberry32(seed);
  const image = withImages ? png.toDataUrl(halfImage(256, 256)) : null;
  const L = [];
  for (let li = 0; li < layers; li++) {
    const keyed = {};
    for (let k = 0; k < total; k += keyEvery) {
      const strokes = [];
      for (let s = 0; s < strokesPerKey; s++) {
        const n = rng.int(6, 12), pts = [];
        let x = rng.float(0, 1920), y = rng.float(0, 1080);
        for (let i = 0; i < n; i++) { x += rng.float(-60, 60); y += rng.float(-60, 60); pts.push([x, y]); }
        const d = pathStroke(`s_b_${li}_${k}_${s}`, pts, { closed: rng.next() < 0.3, stroke: rng.pick(['#111111', '#1c7ed6', '#dc1e1e']), width: rng.float(1, 12, 1), fill: rng.next() < 0.3 ? '#ffcc00' : null });
        d.segments.forEach((sg) => { sg.handleIn = [rng.float(-20, 20), rng.float(-20, 20)]; sg.handleOut = [-sg.handleIn[0], -sg.handleIn[1]]; });
        strokes.push(d);
      }
      if (image && li < withImages) strokes.push(rasterStroke({ x: rng.float(100, 1800), y: rng.float(100, 900), width: 256, height: 256, src: image }));
      keyed[k] = strokes;
    }
    const ld = layer(`ly_b_${li}`, `Bench ${li}`, frames(total, keyed), {
      motion: {
        position: keys([{ frame: 0, v: [0, 0] }, { frame: 8, v: [rng.float(-200, 200), rng.float(-100, 100)] }, { frame: 16, v: [rng.float(-200, 200), 0] }, { frame: 23, v: [0, 0] }]),
        rotation: keys([{ frame: 0, v: [0] }, { frame: 23, v: [rng.float(-90, 90)] }]),
        scale: keys([{ frame: 0, v: [100, 100] }, { frame: 12, v: [rng.float(50, 150), rng.float(50, 150)] }, { frame: 23, v: [100, 100] }]),
        opacity: keys([{ frame: 0, v: [100] }, { frame: 23, v: [rng.float(0, 100)] }]),
      },
    });
    if (li % 5 === 0) ld.expressions = { rotation: { enabled: true, code: 'value + frame * 0.5' }, position: { enabled: true, code: '[value[0] + 10 * Math.sin(time), value[1]]' } };
    L.push(ld);
  }
  return project({ layers: L, totalFrames: total, canvasW: 1920, canvasH: 1080 });
}

const FIXTURES = [staticProps, keyedProps, expressionProps, heldFrames, components, masksAlpha, text, mesh, media, migration, interaction, exportFixture];
const REQUIRED_COVERAGE = ['static', 'keyed', 'expression', 'held-frames', 'components', 'masks-alpha', 'text', 'mesh', 'media', 'migration', 'interaction', 'export'];

module.exports = { FIXTURES, REQUIRED_COVERAGE, benchDocument, EXPORT_ARGS, DEFAULT_CURVE, LEGACY_STEP_CURVE, FORMAT_VERSION, BASE_PROPS, PROP_DEFAULT, helpers: { pathStroke, rasterStroke, rect, frames, layer, keys, project, halfImage } };
