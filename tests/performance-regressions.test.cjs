const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');

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
