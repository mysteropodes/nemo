const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');

function timelineCellRenderer(state) {
  const source = fs.readFileSync(path.join(root, 'src/js/timeline.js'), 'utf8');
  const start = source.indexOf('function renderKeyframeCellsInto(');
  const end = source.indexOf('// ---- HELD-KEYFRAME SPAN SHRINK/TRIM HANDLE ----', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  const document = {
    createElement() {
      const classes = new Set();
      const el = {
        children: [],
        dataset: {},
        style: { setProperty() {} },
        classList: {
          add(...names) { names.forEach((name) => classes.add(name)); },
          contains(name) { return classes.has(name); },
        },
        appendChild(child) { this.children.push(child); },
      };
      Object.defineProperty(el, 'className', {
        get() { return [...classes].join(' '); },
        set(value) { classes.clear(); String(value).split(/\s+/).filter(Boolean).forEach((name) => classes.add(name)); },
      });
      return el;
    },
  };
  const sandbox = {
    document,
    state,
    selHas() { return false; },
    hexToRgbTriplet() { return '0,0,0'; },
  };
  vm.runInNewContext(`${source.slice(start, end)}\nthis.renderCells = renderKeyframeCellsInto;`, sandbox);
  return { render: sandbox.renderCells, document };
}

test('only the promoted timeline zoom module is loaded', () => {
  const html = fs.readFileSync(path.join(root, 'src/index.html'), 'utf8');
  assert.equal((html.match(/<script src="js\/timeline-zoom\.js"><\/script>/g) || []).length, 1);
  assert.equal(html.includes('<script src="js/labs/timeline-zoom.js"></script>'), false);
});

test('cached playback keeps its canvas backing store between equal-sized frames', async () => {
  let previewCanvas;
  let widthWrites = 0;
  let heightWrites = 0;
  let backingWidth = 0;
  let backingHeight = 0;
  const context2d = {
    clearRect() {},
    drawImage() {},
  };
  const rustCanvas = {
    style: {},
    parentNode: {
      insertBefore(node) { previewCanvas = node; },
    },
    nextSibling: null,
  };
  const document = {
    body: { contains(node) { return node === previewCanvas; } },
    getElementById(id) {
      if (id === 'rust-canvas') return rustCanvas;
      if (id === 'bake-preview-canvas') return previewCanvas || null;
      return null;
    },
    createElement(tag) {
      assert.equal(tag, 'canvas');
      const canvas = {
        id: '',
        style: {},
        getContext(kind) { assert.equal(kind, '2d'); return context2d; },
      };
      Object.defineProperty(canvas, 'width', {
        get() { return backingWidth; },
        set(value) { widthWrites++; backingWidth = value; },
      });
      Object.defineProperty(canvas, 'height', {
        get() { return backingHeight; },
        set(value) { heightWrites++; backingHeight = value; },
      });
      return canvas;
    },
  };
  const state = { canvasW: 4, canvasH: 3, totalFrames: 1, layers: [] };
  const window = {
    SMEngineBridge: {
      isEnabled() { return true; },
      beginEffectsExport() {},
      endEffectsExport() {},
      resizeEngineOffscreen() {},
      async renderFrameRawPixels() { return new Uint8ClampedArray(state.canvasW * state.canvasH * 4); },
    },
  };
  class ImageDataStub {
    constructor(data, width, height) { this.data = data; this.width = width; this.height = height; }
  }
  const sandbox = {
    console,
    document,
    state,
    window,
    ImageData: ImageDataStub,
    createImageBitmap: async () => ({ close() {} }),
    Map,
    Math,
    Uint8ClampedArray,
  };
  vm.runInNewContext(
    fs.readFileSync(path.join(root, 'src/js/playback-cache.js'), 'utf8'),
    sandbox,
    { filename: 'playback-cache.js' },
  );

  await window.SMPlaybackCache.bakeRange(0, 0, { scale: 1 });
  assert.equal(window.SMPlaybackCache.blitFrame(0), true);
  assert.equal(window.SMPlaybackCache.blitFrame(0), true);
  assert.equal(widthWrites, 1);
  assert.equal(heightWrites, 1);
});

test('timeline held-span rendering is linear and preserves inherited classes', () => {
  const frames = [
    { strokes: [], isKeyframe: false, isInterpolated: false },
    { strokes: [], isKeyframe: true, isInterpolated: false },
    { strokes: [], isKeyframe: false, isInterpolated: false },
    { strokes: [], isKeyframe: false, isInterpolated: true },
    { strokes: [], isKeyframe: false, isInterpolated: false },
    { strokes: [{}], isKeyframe: true, isInterpolated: false },
    { strokes: [], isKeyframe: false, isInterpolated: false },
    { strokes: [], isKeyframe: false, isInterpolated: false },
  ];
  const state = {
    totalFrames: frames.length,
    currentFrame: -1,
    waIn: 0,
    waOut: frames.length - 1,
    symbols: {},
    layers: [{ frames }],
  };
  const harness = timelineCellRenderer(state);
  const row = harness.document.createElement('div');
  harness.render(row, 0);
  const has = (frame, name) => row.children[frame].classList.contains(name);
  assert.equal(has(0, 'span-empty'), false);
  assert.equal(has(1, 'kf-empty'), true);
  assert.equal(has(2, 'span-empty'), true);
  assert.equal(has(3, 'tw'), true);
  assert.equal(has(4, 'span-empty'), true);
  assert.equal(has(4, 'span-end'), true);
  assert.equal(has(5, 'kf-full'), true);
  assert.equal(has(6, 'span-full'), true);
  assert.equal(has(7, 'span-full'), true);
  assert.equal(has(7, 'span-end'), true);

  let indexedReads = 0;
  const emptyFrames = new Proxy(Array.from({ length: 1000 }, () => ({
    strokes: [], isKeyframe: false, isInterpolated: false,
  })), {
    get(target, property, receiver) {
      if (/^\d+$/.test(String(property))) indexedReads++;
      return Reflect.get(target, property, receiver);
    },
  });
  const longState = { ...state, totalFrames: 1000, waOut: 999, layers: [{ frames: emptyFrames }] };
  const longHarness = timelineCellRenderer(longState);
  longHarness.render(longHarness.document.createElement('div'), 0);
  assert.ok(indexedReads < 2000, `expected a linear frame walk, observed ${indexedReads} indexed reads`);
});

test('undo can reuse an explicit frame save without saving twice', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/tweens.js'), 'utf8');
  const start = source.indexOf('function pushUndoLayers(');
  const end = source.indexOf('function restoreLayersSnapshot(', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  let saves = 0;
  const window = { _scrubLiveActive: false };
  const state = { undoStack: [], undoLabels: [], redoStack: [{}], redoLabels: [{}], maxUndo: 60 };
  const sandbox = {
    window,
    state,
    saveAllLayerFrames() { saves++; },
    layersSnapshotNow() { return { snapshot: true }; },
    _actionLabelNow() { return { label: 'test' }; },
  };
  vm.runInNewContext(`${source.slice(start, end)}\nthis.pushUndoLayersTest = pushUndoLayers;`, sandbox);

  sandbox.pushUndoLayersTest(true);
  assert.equal(saves, 0);
  assert.equal(state.undoStack.length, 1);
  assert.equal(state.redoStack.length, 0);

  sandbox.pushUndoLayersTest();
  assert.equal(saves, 1);
  assert.equal(state.undoStack.length, 2);

  window._scrubLiveActive = true;
  sandbox.pushUndoLayersTest();
  assert.equal(saves, 1);
  assert.equal(state.undoStack.length, 2);
});

test('explicit save plus undo call sites declare that the save is reusable', () => {
  for (const file of ['src/js/app.js', 'src/js/images.js', 'src/js/timeline.js']) {
    const source = fs.readFileSync(path.join(root, file), 'utf8');
    assert.equal(
      /saveAllLayerFrames\(\);\s*pushUndo(?:Layers)?\(\)/.test(source),
      false,
      `${file} still performs two consecutive full saves`,
    );
  }
});

test('stroke modeler prefers packed wasm output and preserves legacy fallback', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/stroke-modeler.js'), 'utf8');
  const packedCalls = [];
  class PackedModeler {
    down_packed(...args) { packedCalls.push(['down', ...args]); return new Float64Array([1, 2, 0.25]); }
    move_packed(...args) { packedCalls.push(['move', ...args]); return new Float64Array([3, 4, 0.5, 5, 6, 0.75]); }
    up_packed(...args) { packedCalls.push(['up', ...args]); return new Float64Array(0); }
    down() { throw new Error('packed modeler must not use JSON'); }
    move() { throw new Error('packed modeler must not use JSON'); }
    up() { throw new Error('packed modeler must not use JSON'); }
  }
  const packedWindow = { GeometryWasm: { ready: true, StrokeModeler: PackedModeler } };
  vm.runInNewContext(source, { window: packedWindow, console, JSON, Math, Array, Float64Array });
  const packed = packedWindow.SMStrokeModeler.create(2, 1.5);
  assert.equal(packed.packed, true);
  assert.equal(packed.downPacked(10, 20, 1, 0.4) instanceof Float64Array, true);
  assert.equal(JSON.stringify(packed.down(10, 20, 1, 0.4)), JSON.stringify([{ x: 1, y: 2, p: 0.25 }]));
  assert.equal(JSON.stringify(packed.move(11, 21, 1.1, 0.6)), JSON.stringify([
    { x: 3, y: 4, p: 0.5 },
    { x: 5, y: 6, p: 0.75 },
  ]));
  assert.equal(JSON.stringify(packed.up(12, 22, 1.2, 0.7)), '[]');
  assert.deepEqual(packedCalls.map((call) => call[0]), ['down', 'down', 'move', 'up']);

  class LegacyModeler {
    down() { return '[{"x":7,"y":8,"p":0.9}]'; }
    move() { return '[]'; }
    up() { return '[]'; }
  }
  const legacyWindow = { GeometryWasm: { ready: true, StrokeModeler: LegacyModeler } };
  vm.runInNewContext(source, { window: legacyWindow, console, JSON, Math, Array });
  assert.equal(JSON.stringify(legacyWindow.SMStrokeModeler.create(1, 1).down(0, 0, 0, 1)), '[{"x":7,"y":8,"p":0.9}]');
});

test('draw bridge consumes packed modeler triplets without object unpacking', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/draw-bridge.js'), 'utf8');
  assert.match(source, /if \(modeler\.packed\) modeler\.downPacked\(/);
  assert.match(source, /var packedOuts = modeler\.movePacked\(/);
  assert.match(source, /mpi \+= 3/);
  assert.match(source, /var packedOuts = modeler\.upPacked\(/);
});

test('motion key lookup is logarithmic and selects the same containing segment', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/motion.js'), 'utf8');
  const start = source.indexOf('function segmentIndexAtFrame(');
  const end = source.indexOf('// The value of `prop`', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  const sandbox = { Math };
  vm.runInNewContext(`${source.slice(start, end)}\nthis.segmentIndexAtFrameTest = segmentIndexAtFrame;`, sandbox);
  const find = sandbox.segmentIndexAtFrameTest;
  const keys = [{ frame: 0 }, { frame: 3 }, { frame: 10 }, { frame: 11 }, { frame: 40 }];
  assert.equal(find(keys, 0), 0);
  assert.equal(find(keys, 2.9), 0);
  assert.equal(find(keys, 3), 1);
  assert.equal(find(keys, 10.5), 2);
  assert.equal(find(keys, 39.9), 3);

  let reads = 0;
  const many = Array.from({ length: 4096 }, (_, index) => ({
    get frame() { reads++; return index * 2; },
  }));
  assert.equal(find(many, 7001), 3500);
  assert.ok(reads < 50, `expected logarithmic lookup, observed ${reads} frame reads`);

  const evalEnd = source.indexOf('function rawValueAtFrame(', start);
  const evalSandbox = {
    Math,
    DEFAULT_CURVE: [],
    evalCurvePoints(_curve, t) { return t; },
  };
  vm.runInNewContext(`${source.slice(start, evalEnd)}\nthis.evalTrackTest = evalTrack;`, evalSandbox);
  const track = { keys: [
    { frame: 0, v: [10] },
    { frame: 10, v: [30], hold: true },
    { frame: 20, v: [70] },
  ] };
  assert.equal(evalSandbox.evalTrackTest(track, -2, 99), 10);
  assert.equal(evalSandbox.evalTrackTest(track, 5, 99), 20);
  assert.equal(evalSandbox.evalTrackTest(track, 10, 99), 30);
  assert.equal(evalSandbox.evalTrackTest(track, 19.9, 99), 30);
  assert.equal(evalSandbox.evalTrackTest(track, 20, 99), 70);
  assert.equal(evalSandbox.evalTrackTest(null, 5, 99), 99);
});

test('exact motion key lookup is logarithmic and preserves missing-key behavior', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/motion.js'), 'utf8');
  const start = source.indexOf('function keyAt(');
  const end = source.indexOf('function staticValue(', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  const sandbox = {};
  vm.runInNewContext(`${source.slice(start, end)}\nthis.keyAtTest = keyAt;`, sandbox);
  const find = sandbox.keyAtTest;
  const keys = [{ frame: 0 }, { frame: 3 }, { frame: 10 }, { frame: 11 }, { frame: 40 }];
  const track = { keys };
  assert.equal(find(track, 0), keys[0]);
  assert.equal(find(track, 10), keys[2]);
  assert.equal(find(track, 40), keys[4]);
  assert.equal(find(track, 9), null);
  assert.equal(find(track, 41), null);
  assert.equal(find({ keys: [] }, 0), null);
  assert.equal(find(null, 0), null);

  let reads = 0;
  const many = Array.from({ length: 4096 }, (_, index) => ({
    get frame() { reads++; return index * 2; },
  }));
  assert.equal(find({ keys: many }, 7000), many[3500]);
  assert.ok(reads < 50, `expected logarithmic exact lookup, observed ${reads} frame reads`);

  assert.equal(
    /(?:\w+\.)?track\.keys\.find\(function\s*\([^)]*\)\s*\{\s*return\s+[^;]*\.frame\s*===/.test(source),
    false,
    'motion.js still bypasses keyAt with a local linear exact-frame scan',
  );
});

test('expression nearestKey lookup is logarithmic and keeps earlier-key ties', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/motion.js'), 'utf8');
  const start = source.indexOf('function nearestKeyIndex(');
  const end = source.indexOf('function exprNearestKey(', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  const sandbox = {};
  vm.runInNewContext(`${source.slice(start, end)}\nthis.nearestKeyIndexTest = nearestKeyIndex;`, sandbox);
  const find = sandbox.nearestKeyIndexTest;
  const keys = [{ frame: 0 }, { frame: 3 }, { frame: 10 }, { frame: 20 }, { frame: 40 }];
  assert.equal(find(keys, -10), 0);
  assert.equal(find(keys, 0), 0);
  assert.equal(find(keys, 2), 1);
  assert.equal(find(keys, 6.5), 1);
  assert.equal(find(keys, 9), 2);
  assert.equal(find(keys, 30), 3);
  assert.equal(find(keys, 35), 4);
  assert.equal(find(keys, 100), 4);

  let reads = 0;
  const many = Array.from({ length: 4096 }, (_, index) => ({
    get frame() { reads++; return index * 2; },
  }));
  assert.equal(find(many, 7001), 3500);
  assert.ok(reads < 50, `expected logarithmic nearest-key lookup, observed ${reads} frame reads`);
});

test('motion drag timeline rebuilds are coalesced and flushed on release', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/motion.js'), 'utf8');
  const start = source.indexOf('var _motionDragTimelineRaf = 0;');
  const end = source.indexOf('// Drag-to-retime a keyframe', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  let nextId = 1;
  let renders = 0;
  const callbacks = new Map();
  const sandbox = {
    requestAnimationFrame(callback) { const id = nextId++; callbacks.set(id, callback); return id; },
    cancelAnimationFrame(id) { callbacks.delete(id); },
    renderTimeline() { renders++; },
  };
  vm.runInNewContext(
    `${source.slice(start, end)}\nthis.requestTest = requestMotionDragTimelineRender; this.flushTest = flushMotionDragTimelineRender;`,
    sandbox,
  );

  for (let i = 0; i < 40; i++) sandbox.requestTest();
  assert.equal(callbacks.size, 1);
  assert.equal(renders, 0);
  const firstCallback = callbacks.values().next().value;
  callbacks.clear();
  firstCallback();
  assert.equal(renders, 1);

  for (let i = 0; i < 40; i++) sandbox.requestTest();
  assert.equal(callbacks.size, 1);
  sandbox.flushTest();
  assert.equal(callbacks.size, 0);
  assert.equal(renders, 2);
  sandbox.flushTest();
  assert.equal(renders, 2);

  const dragSection = source.slice(source.indexOf('function onDragMove('), source.indexOf('document.addEventListener(\'mousemove\', onDragMove)'));
  assert.equal((dragSection.match(/requestMotionDragTimelineRender\(\)/g) || []).length, 4);
});

test('numeric scrub coalesces input and change work and flushes its final value', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/ui.js'), 'utf8');
  const start = source.indexOf('var scrubState=null;');
  const end = source.indexOf('})();', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);

  const documentListeners = new Map();
  const windowListeners = new Map();
  const callbacks = new Map();
  let nextId = 1;
  let undoCount = 0;
  const document = {
    activeElement: null,
    addEventListener(type, callback) { documentListeners.set(type, callback); },
  };
  const window = {
    pushUndo() { undoCount++; },
    addEventListener(type, callback) { windowListeners.set(type, callback); },
  };
  class EventStub {
    constructor(type, options) { this.type = type; this.bubbles = !!(options && options.bubbles); }
  }
  vm.runInNewContext(source.slice(start, end), {
    document,
    window,
    Event: EventStub,
    Math,
    requestAnimationFrame(callback) { const id = nextId++; callbacks.set(id, callback); return id; },
    cancelAnimationFrame(id) { callbacks.delete(id); },
  });

  const classes = new Set();
  const eventCounts = { input: 0, change: 0 };
  const input = {
    value: '10', min: '', max: '', dataset: { step: '1' },
    closest(selector) { return selector === 'input.scrub' ? this : null; },
    setPointerCapture() {}, focus() {}, select() {},
    classList: { add(name) { classes.add(name); }, remove(name) { classes.delete(name); } },
    dispatchEvent(event) { eventCounts[event.type]++; },
  };
  documentListeners.get('pointerdown')({
    target: input, pointerId: 7, clientX: 100, preventDefault() {},
  });
  for (let i = 1; i <= 40; i++) {
    documentListeners.get('pointermove')({ pointerId: 7, clientX: 103 + i });
  }
  assert.equal(callbacks.size, 1);
  assert.deepEqual(eventCounts, { input: 0, change: 0 });
  assert.equal(undoCount, 1);
  const firstCallback = callbacks.values().next().value;
  callbacks.clear();
  firstCallback();
  assert.deepEqual(eventCounts, { input: 1, change: 1 });

  for (let i = 1; i <= 20; i++) {
    documentListeners.get('pointermove')({ pointerId: 7, clientX: 143 + i });
  }
  assert.equal(callbacks.size, 1);
  documentListeners.get('pointerup')({ pointerId: 7 });
  assert.equal(callbacks.size, 0);
  assert.deepEqual(eventCounts, { input: 2, change: 2 });
  assert.equal(window._scrubLiveActive, false);
  assert.equal(classes.has('scrubbing'), false);
});
