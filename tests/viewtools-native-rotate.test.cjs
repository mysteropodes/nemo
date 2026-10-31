'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm');
// The actual viewtools-bridge.js on a controlled canvas area. Native ownership and the legacy engine are controlled ports.
function fixture({ native, engine = true, tool = 'rotate', alt = false }) {
  const handlers = {}, toasts = [], writes = [], renders = [];
  const area = { addEventListener(type, fn) { handlers[type] = fn; } };
  const canvas = { getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 450 }) };
  const state = { tool, altDown: alt, spaceDown: false, playing: false, canvasW: 800, canvasH: 450, canvasRotation: 0 };
  const view = { zoom: 1, center: { x: 400, y: 225 }, projectToView: p => p, viewToProject: p => p };
  const session = { blocksLegacy: () => native };
  const root = { state, view, Point: function (x, y) { this.x = x; this.y = y; }, readyState: 'complete', showToast: text => toasts.push(text),
    SMEngineBridge: { isEnabled: () => engine, suspend() { writes.push('suspend'); }, resume() { writes.push('resume'); }, renderNow: flag => renders.push(flag === true ? 'viewport' : 'full') },
    NemoNativeObjectProject: { desktop: () => session }, addEventListener() {},
    document: { readyState: 'complete', getElementById: id => id === 'canvas-area' ? area : id === 'drawing-canvas' ? canvas : null, addEventListener() {} } };
  root.window = root; const context = vm.createContext(root);
  vm.runInContext(fs.readFileSync(require.resolve('../src/js/viewtools-bridge.js'), 'utf8'), context, { filename: 'viewtools-bridge.js' });
  function pointer(type, extra) {
    const event = { type, clientX: 400, clientY: 225, shiftKey: false, altKey: alt, stopped: false, prevented: false,
      stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; }, ...extra };
    handlers[type](event); return event;
  }
  return { state, toasts, writes, renders, pointer, handlers };
}
test('native Rotate tool says rotation is unavailable once per gesture, consumes the sequence and writes nothing', () => {
  const f = fixture({ native: true });
  const down = f.pointer('pointerdown', { clientX: 500, clientY: 225 });
  assert.deepEqual(f.toasts, ['Canvas rotation is unavailable in native projects']); assert(down.stopped && down.prevented);
  assert(f.pointer('pointermove', { clientX: 400, clientY: 325 }).stopped); assert(f.pointer('pointermove', { clientX: 300, clientY: 225 }).stopped);
  assert(f.pointer('pointerup').stopped);
  assert.equal(f.toasts.length, 1); assert.equal(f.state.canvasRotation, 0, 'no rotation state'); assert.deepEqual(f.writes, []); assert.deepEqual(f.renders, []);
  f.pointer('pointerdown'); f.pointer('pointerup'); assert.equal(f.toasts.length, 2, 'every new gesture is announced');
  assert(!f.pointer('pointermove').stopped, 'a finished gesture no longer consumes moves');
});
test('native Alt-drag rotate shortcut is announced only where Alt has no other native meaning and is never silent', () => {
  const f = fixture({ native: true, tool: 'eyedropper', alt: true });
  assert(f.pointer('pointerdown').stopped); assert.equal(f.toasts.length, 1); f.pointer('pointermove'); f.pointer('pointerup');
  assert.equal(f.state.canvasRotation, 0); assert.deepEqual(f.renders, []);
  for (const tool of ['select', 'subselect', 'pen', 'rect', 'draw', 'eraser']) {
    const g = fixture({ native: true, tool, alt: true });
    assert(!g.pointer('pointerdown').stopped, tool + ' keeps its own Alt gesture'); assert.deepEqual(g.toasts, []);
  }
  const space = fixture({ native: true, tool: 'rotate' }); space.state.spaceDown = true;
  space.pointer('pointerdown'); assert.deepEqual(space.toasts, [], 'Space-pan keeps priority over the rotate notice');
});
test('legacy projects keep the engine-backed rotate gesture and native ownership does not depend on the legacy engine flag', () => {
  const legacy = fixture({ native: false });
  legacy.pointer('pointerdown', { clientX: 500, clientY: 225 }); legacy.pointer('pointermove', { clientX: 400, clientY: 325 });
  assert.notEqual(legacy.state.canvasRotation, 0); assert.deepEqual(legacy.toasts, []); assert(legacy.renders.includes('viewport'));
  const off = fixture({ native: true, engine: false });
  assert(off.pointer('pointerdown').stopped); assert.equal(off.toasts.length, 1);
});
