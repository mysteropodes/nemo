'use strict';
// Loads PRODUCTION modules from src/js into a Node vm sandbox so fixtures and
// workloads exercise the shipped evaluator, never a copy of it (R03; rule
// "pure unit tests use production imports and independent expected results",
// engineering/remediation/03_TESTING_AND_DEBUGGING.md).
//
// motion.js runs whole: its IIFE only touches the DOM lazily, so a stub
// `document` whose lookups return null is enough for the evaluator, the
// expression compiler and the migrations. app.js does not (Paper.js bound at
// load), so the few pure functions the fixtures need are lifted out of it by
// brace matching — the same technique tests/performance-regressions.test.cjs
// uses — and run against a minimal `state`. Nothing here modifies src/.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..', '..', '..');
const SRC = path.join(ROOT, 'src', 'js');

function read(file) { return fs.readFileSync(path.join(SRC, file), 'utf8'); }

// Extracts `function name(...) { ... }` (top-level or IIFE-indented) by
// matching braces. Naive about braces inside strings/regex literals; every
// function lifted here is plain control flow and has been checked.
function extractFunction(source, name) {
  const start = source.indexOf(`function ${name}(`);
  if (start === -1) throw new Error(`function ${name} not found`);
  const open = source.indexOf('{', start);
  let depth = 0;
  for (let i = open; i < source.length; i++) {
    const ch = source[i];
    if (ch === '{') depth++;
    else if (ch === '}') { depth--; if (depth === 0) return source.slice(start, i + 1); }
  }
  throw new Error(`unbalanced braces in ${name}`);
}

function stubElement() {
  const noop = () => {};
  return {
    addEventListener: noop, removeEventListener: noop, appendChild: noop, removeChild: noop, insertBefore: noop, remove: noop,
    querySelector: () => null, querySelectorAll: () => [], getElementsByClassName: () => [],
    classList: { add: noop, remove: noop, toggle: noop, contains: () => false },
    style: {}, dataset: {}, children: [], childNodes: [], setAttribute: noop, getAttribute: () => null, removeAttribute: noop,
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0 }),
    focus: noop, blur: noop, click: noop, textContent: '', innerHTML: '', value: '',
  };
}

function baseSandbox(state) {
  const noop = () => {};
  const document = Object.assign(stubElement(), {
    getElementById: () => null, createElement: stubElement, createElementNS: stubElement, createTextNode: () => ({}),
    body: stubElement(), documentElement: stubElement(), head: stubElement(),
  });
  const sb = {
    console, Math, JSON, Date, Array, Object, Number, String, Boolean, Function, RegExp, Error, SyntaxError, TypeError, RangeError,
    Map, Set, WeakMap, Promise, Symbol, isFinite, isNaN, parseFloat, parseInt, Infinity, NaN, undefined,
    Float32Array, Float64Array, Int32Array, Uint8Array, Uint32Array, ArrayBuffer,
    setTimeout, clearTimeout, setInterval, clearInterval, queueMicrotask,
    requestAnimationFrame: noop, cancelAnimationFrame: noop, performance,
    document, localStorage: { getItem: () => null, setItem: noop, removeItem: noop },
    navigator: { userAgent: 'node', language: 'en' }, location: { href: 'about:blank', search: '' },
    state, userLayers: [], SM: { t: (k) => k },
    showToast: noop, renderLayerList: noop, renderTimeline: noop, updateUI: noop, renderNow: noop,
  };
  sb.window = sb; sb.globalThis = sb; sb.self = sb;
  return sb;
}

function defaultState() {
  return { layers: [], symbols: {}, fps: 24, totalFrames: 24, frame: 0, mode: 'motion', canvasW: 1920, canvasH: 1080, imageMeshes: {}, trackRoles: {}, cameraKeys: [] };
}

// motion.js: returns { SMMotion, state, sandbox }. `state.layers` and
// `state.fps` are read by the evaluator and the expression engine.
function loadMotion(state = defaultState()) {
  const sb = baseSandbox(state);
  vm.runInNewContext(read('motion.js'), sb, { filename: 'src/js/motion.js' });
  if (!sb.SMMotion || typeof sb.SMMotion.valueAtFrame !== 'function') throw new Error('motion.js did not expose SMMotion.valueAtFrame');
  return { SMMotion: sb.SMMotion, state, sandbox: sb };
}

// Pure document-resolution helpers lifted from app.js (read-only over the
// stored frame data: no Paper.js needed as long as no element motion is
// attached to a component sub-layer).
const APP_FUNCTIONS = [
  'getEffectiveStrokes', 'layerHasTimeRange', 'layerInPoint', 'layerOutPoint', '_layerFolderParent', 'resolveLinkedTime',
  '_clampToTimeline', 'autoInPointFromBlankKeyframe', 'autoOutPointFromBlankKeyframe', 'resolveSymbolFrameIdx',
  '_boundsCenterOfStrokes', 'getLFSSubStrokes', 'migrateTimeLinkOffsets', 'effectorChannels',
];
function loadAppHelpers(state = defaultState(), motion = null) {
  const src = read('app.js');
  const code = APP_FUNCTIONS.map((n) => extractFunction(src, n)).join('\n');
  const sb = baseSandbox(state);
  if (motion) sb.SMMotion = motion.SMMotion;
  vm.runInNewContext(code + '\nthis.__api = { getEffectiveStrokes, resolveSymbolFrameIdx, layerInPoint, layerOutPoint, migrateTimeLinkOffsets, effectorChannels };', sb, { filename: 'src/js/app.js (extract)' });
  return Object.assign({ state, sandbox: sb }, sb.__api);
}

// image-mesh.js with the vendored Delaunator, for mesh topology invariants.
function loadImageMesh(state = defaultState()) {
  const sb = baseSandbox(state);
  vm.runInNewContext(read('delaunator.vendor.js'), sb, { filename: 'src/js/delaunator.vendor.js' });
  vm.runInNewContext(read('image-mesh.js'), sb, { filename: 'src/js/image-mesh.js' });
  if (!sb.SMImageMesh) throw new Error('image-mesh.js did not expose SMImageMesh');
  return { SMImageMesh: sb.SMImageMesh, state, sandbox: sb };
}

function loadTextSelector() { return require(path.join(SRC, 'text-selector.js')); }

// tweens.js _cloneLayersForUndo (+ _walkStrokes): the production undo snapshot
// of a layer array — heavy string fields (app.js _HEAVY_STROKE_FIELDS) are
// detached, the tree is cloned natively, the fields are reattached to both
// copies. The constant is read from app.js so the sandbox never drifts from it.
function loadUndoClone() {
  const tweens = read('tweens.js');
  const app = read('app.js');
  const heavy = /var _HEAVY_STROKE_FIELDS\s*=\s*(\[[^\]]*\])/.exec(app);
  if (!heavy) throw new Error('_HEAVY_STROKE_FIELDS not found in app.js');
  const sb = baseSandbox(defaultState());
  vm.runInNewContext(`window._HEAVY_STROKE_FIELDS = ${heavy[1]};\n` + extractFunction(tweens, '_walkStrokes') + '\n' + extractFunction(tweens, '_cloneLayersForUndo') + '\nthis.__api = { _cloneLayersForUndo, _walkStrokes, HEAVY: window._HEAVY_STROKE_FIELDS };', sb, { filename: 'src/js/tweens.js (extract)' });
  return { cloneLayersForUndo: sb.__api._cloneLayersForUndo, walkStrokes: sb.__api._walkStrokes, heavyFields: Array.from(sb.__api.HEAVY), sandbox: sb };
}

// vector-text-bridge.js vectorTextGroupMembers(root): every sibling sharing the
// root's data.groupId. Runs over plain objects shaped like desP's output for
// the fields it reads (data.groupId), so a fixture can prove production
// regroups its persisted text block.
function loadVectorTextGroup() {
  const sb = baseSandbox(defaultState());
  vm.runInNewContext(extractFunction(read('vector-text-bridge.js'), 'vectorTextGroupMembers') + '\nthis.__api = { vectorTextGroupMembers };', sb, { filename: 'src/js/vector-text-bridge.js (extract)' });
  return { vectorTextGroupMembers: sb.__api.vectorTextGroupMembers, sandbox: sb };
}

module.exports = { ROOT, extractFunction, loadMotion, loadAppHelpers, loadImageMesh, loadTextSelector, loadUndoClone, loadVectorTextGroup, defaultState };
