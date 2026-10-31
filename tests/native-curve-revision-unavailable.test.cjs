'use strict';
// #2371: the Curve editor (#btn-mgraph) and the Revision view cycle (#btn-revision-view) have no native owner. On a native object project the real
// motion-graph.js and the real unavailable adapter run in a vm: click / right-click / Alt+click / Shift+F3 toggle() leave #frame-grid visible, toast the stable
// sentence ONCE and change nothing; the revision handler leaves state.revisionView 'all'. A legacy project behaves as the baseline.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Unavailable = require('../src/js/adapters/native-inspector-unavailable.js');
const SRC = (f) => fs.readFileSync(path.join(__dirname, '..', 'src/js', f), 'utf8');

const CURVE = 'Curve editor is not available on native projects yet; use the key ease presets in the property panel';
const REVISION = 'Revision view is not available on native projects';

class El {
  constructor(id, doc) { this.id = id; this.doc = doc; this.listeners = {}; this.dataset = {}; this.style = {}; this.attrs = {}; this.title = ''; this.textContent = ''; this.children = [];
    const set = new Set(); this.classList = { toggle: (c, on) => { (on === undefined ? !set.has(c) : on) ? set.add(c) : set.delete(c); }, contains: (c) => set.has(c), add: (c) => set.add(c), remove: (c) => set.delete(c) }; }
  addEventListener(type, fn) { (this.listeners[type] = this.listeners[type] || []).push(fn); }
  setAttribute(k, v) { this.attrs[k] = String(v); }
  removeAttribute(k) { delete this.attrs[k]; }
  appendChild(c) { c.parentNode = this; this.children.push(c); return c; }
  querySelector() { return null; }
  querySelectorAll() { return []; }
  get innerHTML() { return ''; }
  set innerHTML(_) {}
  getBoundingClientRect() { return { left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0 }; }
  dispatch(type, extra) {
    const event = Object.assign({ type, target: this, stopped: false, prevented: false, stopImmediatePropagation() { this.stopped = true; }, stopPropagation() {}, preventDefault() { this.prevented = true; } }, extra);
    for (const fn of this.doc.capture[type] || []) { if (event.stopped) break; fn(event); }
    if (!event.stopped) for (const fn of this.listeners[type] || []) { fn.call(this, event); if (event.stopped) break; }
    return event;
  }
}
function world(native) {
  const doc = { capture: {}, bubble: {}, nodes: new Map(), readyState: 'complete',
    addEventListener(type, fn, cap) { const t = cap ? doc.capture : doc.bubble; (t[type] = t[type] || []).push(fn); },
    getElementById(id) { return doc.nodes.get(id) || null; },
    createElement() { return new El(null, doc); }, querySelector() { return null; }, querySelectorAll() { return []; } };
  ['btn-mgraph', 'btn-revision-view', 'frame-grid', 'fg-wrap'].forEach((id) => doc.nodes.set(id, new El(id, doc)));
  const toasts = [], calls = [];
  const win = { document: doc, FC: 30, toasts, calls, showToast: (m) => toasts.push(m), NemoNativeSelectionStyle: { native: () => native },
    SMMotion: new Proxy({}, { get(_, k) { return () => { calls.push('SMMotion.' + String(k)); return []; }; } }), SMEngineBridge: { renderNow: () => calls.push('renderNow') }, addEventListener() {}, SM: { t: (k) => k }, state: { totalFrames: 10, layers: [], activeLayerIdx: 0 } };
  win.window = win;
  const ctx = vm.createContext(Object.assign(win, { console, requestAnimationFrame: (f) => f() }));
  vm.runInContext(SRC('adapters/native-inspector-unavailable.js'), ctx);
  vm.runInContext(SRC('motion-graph.js'), ctx);
  (doc.bubble.DOMContentLoaded || []).forEach((fn) => fn());
  win.NemoNativeInspectorUnavailable.sync(win);
  return { win, doc, toasts, calls, ctx };
}

test('native: every Curve editor gesture is refused once, the native grid stays visible and the button reads aria-disabled', () => {
  const { win, doc, toasts, calls } = world(true);
  const b = doc.nodes.get('btn-mgraph'), grid = doc.nodes.get('frame-grid');
  assert.equal(b.attrs['aria-disabled'], 'true');
  assert.equal(b.title, CURVE);
  const gestures = [() => b.dispatch('click', {}), () => b.dispatch('contextmenu', {}), () => b.dispatch('click', { altKey: true })];
  gestures.forEach((g, i) => { const e = g(); assert.equal(e.stopped, true, 'gesture ' + i); assert.equal(e.prevented, true, 'gesture ' + i); assert.deepEqual(toasts, Array(i + 1).fill(CURVE)); });
  assert.equal(win.SMMotionGraph.toggle(), false); // Shift+F3 goes through toggle()
  assert.equal(toasts.length, 4);
  assert.equal(win.SMMotionGraph.isOn(), false);
  assert.equal(win.SMMotionGraph.mode(), 'value');
  assert.equal(grid.style.visibility, undefined);
  assert.equal(b.classList.contains('on'), false);
  assert.deepEqual(calls, [], 'no legacy holder, no repaint');
  assert.equal(doc.nodes.get('fg-wrap').children.length, 0);
});

test('native: the bubble handlers refuse on their own too (the capture guard is not the only line)', () => {
  const { win, doc, toasts } = world(true);
  const b = doc.nodes.get('btn-mgraph');
  b.listeners.click[0].call(b, {});
  b.listeners.contextmenu[0].call(b, { preventDefault() {} });
  assert.deepEqual(toasts, [CURVE, CURVE]);
  assert.equal(win.SMMotionGraph.isOn(), false);
  assert.equal(doc.nodes.get('frame-grid').style.visibility, undefined);
});

test('legacy: the Curve editor behaves as the baseline (click hides the grid, click again restores it, no toast)', () => {
  const { win, doc, toasts } = world(false);
  const b = doc.nodes.get('btn-mgraph');
  assert.equal(b.attrs['aria-disabled'], undefined);
  assert.equal(b.dispatch('click', {}).prevented, false);
  assert.equal(win.SMMotionGraph.isOn(), true);
  assert.equal(doc.nodes.get('frame-grid').style.visibility, 'hidden');
  b.dispatch('contextmenu', {});
  assert.equal(win.SMMotionGraph.mode(), 'speed');
  b.dispatch('click', {});
  assert.equal(win.SMMotionGraph.isOn(), false);
  assert.equal(doc.nodes.get('frame-grid').style.visibility, '');
  assert.deepEqual(toasts, []);
});

// The real timeline.js handler, extracted textually: it is one statement in a 11k-line file.
function revisionHandler(win, state) {
  const src = SRC('timeline.js');
  const start = src.indexOf("document.getElementById('btn-revision-view').addEventListener('click',function(){");
  const end = src.indexOf("document.getElementById('btn-os-outline')", start);
  assert.ok(start > 0 && end > start);
  const body = src.slice(start, end).replace("document.getElementById('btn-revision-view').addEventListener('click',", '(').replace(/\);\s*$/, ')');
  const REVISION_VIEW_LABELS = { all: 'Tout', mine: 'Mes traits', revisions: 'Corrections' }, REVISION_VIEW_ORDER = ['all', 'mine', 'revisions'];
  return vm.runInContext(body, Object.assign(win, { state, REVISION_VIEW_LABELS, REVISION_VIEW_ORDER }));
}
test('native: the Revision view cycle is refused, state.revisionView stays all; legacy still cycles', () => {
  for (const native of [true, false]) {
    const w = world(native), state = { revisionView: 'all' }, btn = w.doc.nodes.get('btn-revision-view');
    const handler = revisionHandler(w.win, state);
    handler.call(btn);
    if (native) {
      assert.equal(state.revisionView, 'all');
      assert.equal(btn.textContent, '');
      assert.equal(btn.classList.contains('active'), false);
      assert.deepEqual(w.toasts, [REVISION]);
      assert.deepEqual(w.calls, []);
      assert.equal(btn.attrs['aria-disabled'], 'true');
      assert.equal(btn.title, REVISION);
      assert.equal(btn.dispatch('click', {}).stopped, true); // the capture guard refuses too, without reaching the handler
    } else {
      assert.equal(state.revisionView, 'mine');
      assert.equal(btn.textContent, 'Mes traits');
      assert.deepEqual(w.toasts, []);
      assert.deepEqual(w.calls, ['renderNow']);
    }
  }
});
