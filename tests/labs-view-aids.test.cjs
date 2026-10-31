'use strict';
// CS8-B09 U31 (#2374): the view-only Labs aids (canvas grid, mirror check, out-of-pegs ghosts, lagoon tool ring)
// over a fake DOM. Each leaves the document state and its content revision untouched and never reaches the native
// document; disabling removes every overlay, pending frame and render wrapper the aid added.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const read = (name) => fs.readFileSync(path.join(__dirname, '..', 'src/js/labs', name), 'utf8');
function element(tag, id) {
  const listeners = {};
  const el = { tagName: tag, id: id || '', style: {}, children: [], parentNode: null, textContent: '', title: '',
    offsetLeft: 0, offsetTop: 0, width: 0, height: 0,
    appendChild(child) { child.parentNode = el; el.children.push(child); return child; },
    remove() { if (el.parentNode) el.parentNode.children = el.parentNode.children.filter((c) => c !== el); el.parentNode = null; },
    addEventListener(kind, fn) { (listeners[kind] = listeners[kind] || []).push(fn); },
    fire(kind, event = {}) { for (const fn of listeners[kind] || []) fn(Object.assign({ target: el, stopPropagation() {}, preventDefault() {} }, event)); },
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 400, bottom: 300, width: 400, height: 300 }),
    getContext: () => new Proxy({}, { get: () => function () {} }) };
  return el;
}
function harness(file, globals = {}) {
  const on = new Set(), features = {}, docListeners = {}, frames = new Map(), winListeners = {};
  const body = element('body'), area = element('div', 'canvas-area'), canvas = element('canvas', 'drawing-canvas');
  body.appendChild(area); area.appendChild(canvas);
  const state = { layers: [{ frames: [{ strokes: [{ id: 's1' }] }] }], contentRevision: 9, appMode: 'animation', canvasW: 1920, canvasH: 1080 };
  let nextFrame = 0;
  const store = new Map();
  const root = Object.assign({ state, SM: { t: (k) => k }, console, devicePixelRatio: 1,
    localStorage: { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, String(v)) },
    requestAnimationFrame(fn) { frames.set(++nextFrame, fn); return nextFrame; }, cancelAnimationFrame(id) { frames.delete(id); },
    addEventListener(kind, fn) { (winListeners[kind] = winListeners[kind] || []).push(fn); },
    document: { body, createElement: (tag) => element(tag),
      getElementById: (id) => [area, canvas].find((e) => e.id === id) || body.children.find((e) => e.id === id) || null,
      addEventListener(kind, fn) { (docListeners[kind] = docListeners[kind] || []).push(fn); } },
    NemoNativeObjectProject: { desktop() { throw Error('a view aid must not touch the native document'); } },
    SMLabs: { isOn: (name) => on.has(name), register(name, spec) { features[name] = spec; } } }, globals);
  root.window = root;
  vm.createContext(root);
  vm.runInContext(read(file), root, { filename: file });
  const before = JSON.stringify(state);
  return { root, state, body, area, canvas, frames,
    enable(name) { on.add(name); if (features[name].onEnable) features[name].onEnable(); },
    disable(name) { on.delete(name); if (features[name].onDisable) features[name].onDisable(); },
    key(kind, key, extra = {}) { for (const fn of docListeners[kind] || []) fn(Object.assign({ key, target: body, stopPropagation() {}, preventDefault() {} }, extra)); },
    pointer(kind, event) { for (const fn of docListeners[kind] || []) fn(event); },
    tick() { const due = [...frames.entries()]; frames.clear(); for (const [, fn] of due) fn(); },
    untouched() { assert.equal(JSON.stringify(state), before); } };
}

test('canvas grid draws one overlay beside the drawing canvas; disable removes it and its pending frame', () => {
  const h = harness('canvas-grid.js', { SMEngineBridge: { isEnabled: () => true, screenToWorld: (x, y) => [x * 2, y * 2] } });
  h.enable('canvas-grid');
  h.tick();
  const overlay = () => h.area.children.filter((c) => c.id === 'labs-grid-overlay');
  assert.equal(overlay().length, 1);
  assert.equal(overlay()[0].style.cssText.includes('pointer-events:none'), true);
  h.tick();
  assert.equal(overlay().length, 1, 'an unchanged view reuses the same overlay');
  assert.equal(h.frames.size, 1, 'the grid keeps one frame scheduled while on');
  h.disable('canvas-grid');
  assert.equal(overlay().length, 0);
  assert.equal(h.frames.size, 0, 'no frame stays scheduled after disable');
  h.untouched();
});

test('desktop mirror check re-presents the native surface with the mirror while held and without it after (#2374)', () => {
  const shown = [];
  const session = { isActive: () => true, renderPreview: (frame) => shown.push([frame, h.root.SMLabs.isMirrored()]) };
  const h = harness('mirror-check.js', { __TAURI__: {}, NemoNativeObjectProject: { desktop: () => session } });
  h.state.currentFrame = 4;
  const before = JSON.stringify(h.state);
  h.enable('mirror-check');
  h.key('keydown', 'm');
  h.key('keydown', 'm', { repeat: true });
  h.key('keyup', 'm');
  h.key('keydown', 'm');
  h.pointer('pointerdown', {});
  assert.deepEqual(shown, [[4, true], [4, false], [4, true], [4, false]], 'held, released, held, clicked: one preview each; repeats add none');
  assert.equal(JSON.stringify(h.state), before, 'view only: no document state changes');
});
test('mirror check flips only the view while held, ignores the key when off, and disable restores the view', () => {
  const h = harness('mirror-check.js');
  h.key('keydown', 'm');
  assert.equal(h.area.style.transform, undefined, 'off: the key does nothing');
  h.enable('mirror-check');
  h.key('keydown', 'm');
  assert.equal(h.area.style.transform, 'scaleX(-1)');
  h.key('keyup', 'm');
  assert.equal(h.area.style.transform, '');
  h.key('keydown', 'M');
  h.pointer('pointerdown', {});
  assert.equal(h.area.style.transform, '', 'a click restores the view so a stroke is never drawn mirrored');
  h.key('keydown', 'm');
  h.disable('mirror-check');
  assert.equal(h.area.style.transform, '');
  h.key('keydown', 'm', { metaKey: true });
  assert.equal(h.area.style.transform, '');
  h.untouched();
});

test('out-of-pegs offsets only the onion ghosts; disable restores the original renderOS and the pegs', () => {
  const moves = [], calls = [];
  const layer = (name) => ({ translate(p) { moves.push([name, p.x, p.y]); } });
  function renderOS() { calls.push('render'); }
  const h = harness('out-of-pegs.js', { onionPrevLayer: layer('prev'), onionNextLayer: layer('next'), renderOS,
    Point: function (x, y) { this.x = x; this.y = y; }, showToast() {} });
  h.enable('out-of-pegs');
  assert.notEqual(h.root.renderOS, renderOS, 'enable wraps the ghost render');
  assert.deepEqual([...h.root.SMLabs.setPegOffset('prev', 12, -4)], [12, -4]);
  assert.deepEqual(moves, [['prev', 12, -4]]);
  h.disable('out-of-pegs');
  assert.equal(h.root.renderOS, renderOS, 'disable restores the original function');
  moves.length = 0;
  h.root.renderOS();
  assert.deepEqual(moves, [], 'no offset survives disable');
  h.untouched();
});

test('lagoon ring opens only when on, switches the tool and leaves nothing behind on Escape or disable', () => {
  const tools = [];
  const h = harness('lagoon-menu.js');
  h.root.SM.setTool = (t) => tools.push(t);
  h.key('keydown', 'q');
  assert.equal(h.body.children.length, 1, 'off: no ring');
  h.enable('lagoon-menu');
  h.pointer('pointermove', { clientX: 200, clientY: 150 });
  h.key('keydown', 'q');
  const ring = h.body.children[1];
  assert.equal(ring.children.length, 7, 'six tools and the centre dot');
  ring.children[1].fire('pointerdown');
  assert.deepEqual(tools, ['eraser']);
  assert.equal(h.body.children.length, 1, 'choosing a tool closes the ring');
  h.key('keydown', 'q');
  h.key('keydown', 'Escape');
  assert.equal(h.body.children.length, 1);
  h.key('keydown', 'q');
  h.disable('lagoon-menu');
  assert.equal(h.body.children.length, 1);
  h.untouched();
});

test('the shadow-guides button flips a view flag only: no persisted or native reader, one re-render', () => {
  const timeline = fs.readFileSync(path.join(__dirname, '..', 'src/js/timeline.js'), 'utf8');
  const at = timeline.indexOf('toggleShadowGuides:function(){');
  let depth = 0, i = timeline.indexOf('{', at);
  for (; i < timeline.length; i++) { if (timeline[i] === '{') depth++; else if (timeline[i] === '}' && --depth === 0) break; }
  const h = harness('lagoon-menu.js');
  const button = element('button', 'btn-shadow-guides'), classes = new Set(['active']);
  button.classList = { toggle: (c, v) => (v ? classes.add(c) : classes.delete(c)) };
  h.body.appendChild(button);
  let renders = 0;
  h.root.SMEngineBridge = { renderNow: () => renders++ };
  const before = JSON.stringify(Object.assign({}, h.state, { showShadowGuides: undefined }));
  h.state.showShadowGuides = true;
  const toggle = vm.runInContext(`({${timeline.slice(at, i + 1)}}).toggleShadowGuides`, h.root);
  toggle();
  assert.deepEqual([h.state.showShadowGuides, classes.has('active'), renders], [false, false, 1]);
  toggle();
  assert.deepEqual([h.state.showShadowGuides, classes.has('active'), renders], [true, true, 2]);
  assert.equal(JSON.stringify(Object.assign({}, h.state, { showShadowGuides: undefined })), before);
  const readers = [];
  const walk = (dir) => { for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p); else if (/\.js$/.test(e.name) && fs.readFileSync(p, 'utf8').includes('showShadowGuides')) readers.push(path.relative(path.join(__dirname, '..'), p));
  } };
  walk(path.join(__dirname, '..', 'src/js'));
  // #2373: on a native object project the flag also rides the preview request (view-only): the toggle in native-view-aids-ui.js and
  // the request port in native-object-project-desktop.js. Still no serializer: the export request builders never read it.
  assert.deepEqual(readers.sort(), ['src/js/adapters/native-object-project-desktop.js', 'src/js/adapters/native-view-aids-ui.js', 'src/js/app.js', 'src/js/engine-bridge.js', 'src/js/timeline.js'],
    'the default, the two render readers, the toggle and the two native view-only readers; no serializer reads the flag');
});

test('Roll only moves the playhead around the origin while R is held and returns to it on release or disable', () => {
  const jumps = [], timers = new Map();
  let next = 0;
  const h = harness('flip-roll.js', { goToFrame(f) { jumps.push(f); h.state.currentFrame = f; },
    setInterval(fn) { timers.set(++next, fn); return next; }, clearInterval(id) { timers.delete(id); } });
  Object.assign(h.state, { currentFrame: 5, totalFrames: 7, playing: false });
  const doc = () => JSON.stringify({ layers: h.state.layers, contentRevision: h.state.contentRevision }), before = doc();
  h.key('keydown', 'r');
  assert.equal(timers.size, 0, 'off: R does nothing');
  h.enable('flip-roll');
  h.key('keydown', 'r');
  for (let i = 0; i < 5; i++) [...timers.values()][0]();
  assert.deepEqual(jumps, [4, 5, 6, 3, 4], 'origin-2..origin+2 clamped to the last frame, cycling');
  h.key('keyup', 'r');
  assert.equal(h.state.currentFrame, 5);
  assert.equal(timers.size, 0);
  h.key('keydown', 'r');
  h.disable('flip-roll');
  assert.deepEqual([h.state.currentFrame, timers.size], [5, 0]);
  assert.equal(doc(), before, 'the document is untouched');
});
