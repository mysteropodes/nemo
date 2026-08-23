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
