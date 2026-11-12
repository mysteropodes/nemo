'use strict';
// #2372 UNAVAIL-NOTICES: inspector controls with no native owner (propagate-across-frames, multi-frame ghost selection, stroke-gradient-along-path, custom
// shader library) are shown unavailable on a native object project and, if still activated, say ONE stable sentence before any writer: no pushUndo, no
// saveActiveLayerFrame, no Paper write. The real fill-bridge.js, effects-panel.js and the real timeline.js functions run in a vm; legacy projects are untouched.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Unavailable = require('../src/js/adapters/native-inspector-unavailable.js');

const ROOT = path.join(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(ROOT, 'src/js', file), 'utf8');
const IDS = ['btn-fill-propagate', 'btn-ghost-select', 'p-strokegrad-along', 'p-strokegrad-from', 'p-strokegrad-from-c', 'p-strokegrad-to', 'p-strokegrad-to-c', 'new-custom-shader'];

class El {
  constructor(id) { this.id = id; this.listeners = {}; this.dataset = {}; this.style = {}; this.attrs = {}; this.title = ''; this.checked = false; this.value = '#ff0000'; this.children = []; this.className = ''; }
  addEventListener(type, fn) { (this.listeners[type] = this.listeners[type] || []).push(fn); }
  setAttribute(k, v) { this.attrs[k] = String(v); }
  removeAttribute(k) { delete this.attrs[k]; if (k === 'title') this.title = ''; }
  appendChild(c) { c.parentElement = this; this.children.push(c); return c; }
  remove() {}
  getBoundingClientRect() { return { right: 0, left: 0, top: 0 }; }
  click() {
    const event = { type: 'click', target: this, stopped: false, prevented: false, stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; }, stopPropagation() {} };
    fire(this.doc, event, true);
    if (!event.stopped) for (const fn of this.listeners.click || []) { fn.call(this, event); if (event.stopped) break; }
    return event;
  }
}
function fire(doc, event, capture) { for (const fn of doc.capture[event.type] || []) { if (event.stopped) break; fn(event); } return event; }
function fakeDoc(ids) {
  const nodes = new Map(), created = [];
  const doc = { capture: {}, readyState: 'complete', nodes, created, body: new El('body'),
    addEventListener(type, fn, cap) { if (cap) (doc.capture[type] = doc.capture[type] || []).push(fn); },
    getElementById(id) { return nodes.get(id) || null; },
    createElement(tag) { const el = new El(null); el.tag = tag; el.doc = doc; Object.defineProperty(el, 'innerHTML', { set() {}, get() { return ''; } }); created.push(el); return el; } };
  doc.body.doc = doc;
  ids.forEach((id) => { const el = new El(id); el.doc = doc; nodes.set(id, el); });
  return doc;
}
// A root the way adapters see one: a native project is the Select style route reporting native.
function rootFor({ native, ids = IDS.filter((id) => id !== 'new-custom-shader') }) {
  const doc = fakeDoc(ids), toasts = [], writes = [];
  const root = { document: doc, toasts, writes, showToast: (m) => toasts.push(m), NemoNativeSelectionStyle: { native: () => native } };
  root.window = root;
  return root;
}

test('the table covers every control without a native owner, one stable sentence each, and leaves #btn-grad-add-stop to its own toast', () => {
  assert.deepEqual(Object.keys(Unavailable.TABLE).sort(), IDS.concat(['btn-mgraph', 'btn-revision-view']).sort());
  for (const id of IDS) {
    const notice = Unavailable.noticeFor(id);
    assert.match(notice, /^[A-Z][^.]*not available on this project yet\.$/, id);
    assert.ok(Unavailable.TABLE[id].reason, id);
  }
  assert.equal(Unavailable.noticeFor('btn-grad-add-stop'), null);
  assert.equal(new Set(['btn-fill-propagate', 'btn-ghost-select', 'p-strokegrad-along', 'new-custom-shader'].map(Unavailable.noticeFor)).size, 4);
  assert.equal(new Set(IDS.filter((id) => id.startsWith('p-strokegrad')).map(Unavailable.noticeFor)).size, 1);
  assert.throws(() => { Unavailable.TABLE.x = 1; });
});

test('sync marks every listed control unavailable on a native project (aria-disabled + a title naming why) and restores it on a legacy one', () => {
  const root = rootFor({ native: true });
  root.document.nodes.get('btn-ghost-select').title = 'Select all keyframes content';
  assert.equal(Unavailable.sync(root), true);
  for (const id of IDS.filter((x) => x !== 'new-custom-shader')) {
    const el = root.document.nodes.get(id);
    assert.equal(el.attrs['aria-disabled'], 'true', id);
    assert.equal(el.title, Unavailable.noticeFor(id), id);
    assert.equal(el.dataset.nativeUnavailable, '1', id);
  }
  Unavailable.sync(root); // idempotent: the base title is remembered once
  assert.equal(root.document.nodes.get('btn-ghost-select').dataset.nativeBaseTitle, 'Select all keyframes content');
  root.NemoNativeSelectionStyle.native = () => false;
  assert.equal(Unavailable.sync(root), false);
  for (const id of IDS.filter((x) => x !== 'new-custom-shader')) {
    const el = root.document.nodes.get(id);
    assert.equal(el.attrs['aria-disabled'], undefined, id);
    assert.equal(el.dataset.nativeUnavailable, undefined, id);
  }
  assert.equal(root.document.nodes.get('btn-ghost-select').title, 'Select all keyframes content');
  assert.equal(root.document.nodes.get('btn-fill-propagate').title, '');
});

test('an activation of a marked control says why and goes no further; on a legacy project nothing is intercepted', () => {
  const root = rootFor({ native: true });
  Unavailable.sync(root);
  let ran = 0;
  for (const id of IDS.filter((x) => x !== 'new-custom-shader')) root.document.nodes.get(id).addEventListener('click', () => { ran += 1; });
  for (const id of IDS.filter((x) => x !== 'new-custom-shader')) {
    const event = root.document.nodes.get(id).click();
    assert.equal(event.prevented, true, id);
    assert.equal(event.stopped, true, id);
  }
  assert.equal(ran, 0);
  assert.equal(root.toasts.length, 7);
  assert.equal(root.toasts[0], Unavailable.noticeFor('btn-fill-propagate'));
  // The swatch wrapper of a colour input is reached through its child: the closest marked ancestor answers.
  const child = new El(null); child.doc = root.document; child.parentElement = root.document.nodes.get('p-strokegrad-from');
  const nested = { type: 'click', target: child, stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; } };
  fire(root.document, nested);
  assert.equal(nested.stopped, true);
  root.NemoNativeSelectionStyle.native = () => false;
  Unavailable.sync(root);
  const before = root.toasts.length;
  const legacy = root.document.nodes.get('btn-ghost-select').click();
  assert.equal(legacy.prevented, false);
  assert.equal(ran, 1);
  assert.equal(root.toasts.length, before);
});

test('refuse is the guard: a stable sentence on native, false for an unknown id or a legacy project, never a throw', () => {
  const native = rootFor({ native: true }), legacy = rootFor({ native: false });
  assert.equal(Unavailable.refuse(native, 'btn-ghost-select'), true);
  assert.equal(Unavailable.refuse(native, 'btn-ghost-select'), true);
  assert.deepEqual(native.toasts, [Unavailable.noticeFor('btn-ghost-select'), Unavailable.noticeFor('btn-ghost-select')]);
  assert.equal(Unavailable.refuse(native, 'btn-grad-add-stop'), false);
  assert.equal(Unavailable.refuse(legacy, 'btn-ghost-select'), false);
  assert.equal(Unavailable.refuse({}, 'btn-ghost-select'), false);
  assert.equal(Unavailable.refuse({ NemoNativeSelectionStyle: { native() { throw new Error('x'); } } }, 'btn-ghost-select'), false);
  assert.deepEqual(legacy.toasts, []);
  const entry = { label: 'x', action() { throw new Error('legacy writer'); } };
  assert.equal(Unavailable.menuItem(legacy, 'new-custom-shader', entry), entry, 'a legacy project keeps the baseline entry');
  assert.equal(Unavailable.menuItem(native, 'btn-grad-add-stop', entry), entry);
  const listed = Unavailable.menuItem(native, 'new-custom-shader', entry);
  assert.equal(listed.unavailable, Unavailable.noticeFor('new-custom-shader'));
  listed.action();
  assert.equal(native.toasts[native.toasts.length - 1], Unavailable.noticeFor('new-custom-shader'));
});

function trap(name, calls) { return () => { calls.push(name); }; }

test('the real fill-bridge Propagate button: native toasts once and reaches no writer; a legacy project keeps the baseline path', () => {
  for (const native of [true, false]) {
    const root = rootFor({ native, ids: ['btn-fill-propagate', 'canvas-area'] });
    const calls = [];
    const layer = { children: [] };
    const context = { window: root, document: root.document, state: { layers: [{ locked: false }], activeLayerIdx: 0, tool: 'fill' }, userLayers: [layer], SM: { t: (k) => k },
      showToast: root.showToast, pushUndo: trap('pushUndo', calls), saveActiveLayerFrame: trap('saveActiveLayerFrame', calls), updateUI: trap('updateUI', calls),
      fillPropagateAcrossFrames: trap('fillPropagateAcrossFrames', calls), SMEngineBridge: { isEnabled: () => true, renderNow: trap('renderNow', calls) }, console };
    Object.assign(root, context, { document: root.document });
    root.window = root; root.NemoNativeInspectorUnavailable = Unavailable;
    root.document.nodes.get('canvas-area').addEventListener = () => {};
    vm.runInNewContext(read('fill-bridge.js'), root, { filename: 'fill-bridge.js' });
    root.document.nodes.get('btn-fill-propagate').listeners.click.forEach((fn) => fn());
    if (native) {
      assert.deepEqual(root.toasts, [Unavailable.noticeFor('btn-fill-propagate')]);
      assert.deepEqual(calls, []);
    } else {
      assert.deepEqual(root.toasts, ['toastPropagateNoFill'], 'the baseline path still runs (no fill to propagate here)');
    }
  }
});

// The real timeline.js code, cut out by its markers.
function timelineSlice(from, to) {
  const text = read('timeline.js'), a = text.indexOf(from), b = text.indexOf(to, a);
  assert.ok(a >= 0 && b > a, `${from} .. ${to}`);
  return text.slice(a, b);
}
function timelineRoot(native) {
  const root = rootFor({ native, ids: IDS.filter((id) => id !== 'new-custom-shader').concat(['btn-ghost-all']) });
  const calls = [];
  const sd = { isRaster: false };
  const throwing = (name) => () => { calls.push(name); throw new Error(`${name} reached`); };
  Object.assign(root, { state: { activeLayerIdx: 0, currentFrame: 0, ghostAllFrames: false, selectedStrokeIndices: [], layers: [{ symbolId: null, frames: [{}, { isKeyframe: true, strokes: [sd] }] }] },
    userLayers: [{ children: [] }], selectedPaths: [{ data: { strokeGradientAlongPath: { from: '#111111', to: '#222222' } } }], SM: { t: (k) => k, setTool: throwing('setTool') },
    SMEngineBridge: { renderNow: throwing('renderNow') }, pushUndo: throwing('pushUndo'), saveActiveLayerFrame: throwing('saveActiveLayerFrame'), updateUI: throwing('updateUI'),
    renderOS: throwing('renderOS'), renderArcs: throwing('renderArcs'), desP: throwing('desP'), Path: function Path() {}, getSI: () => 0, isSelectablePathChild: () => true, _ghostProxyActive: false,
    NemoNativeInspectorUnavailable: Unavailable });
  root.showToast = (m) => root.toasts.push(m);
  root.calls = calls;
  vm.createContext(root);
  return root;
}

test('selectGhostAll (the real function) on native: one notice, no pushUndo, no Paper write; legacy keeps the baseline entry', () => {
  const src = timelineSlice('function selectGhostAll(){', 'function clearGhostSelection(){');
  const native = timelineRoot(true);
  vm.runInContext(src + '\nthis.selectGhostAll = selectGhostAll;', native);
  native.selectGhostAll();
  assert.deepEqual(native.toasts, [Unavailable.noticeFor('btn-ghost-select')]);
  assert.deepEqual(native.calls, []);
  assert.equal(native.state.ghostAllFrames, false);
  const legacy = timelineRoot(false);
  vm.runInContext(src + '\nthis.selectGhostAll = selectGhostAll;', legacy);
  assert.throws(() => legacy.selectGhostAll(), /renderOS reached/, 'the baseline path starts (Ghost All self-enables, then the Paper writers run)');
  assert.equal(legacy.state.ghostAllFrames, true);
  assert.deepEqual(legacy.toasts, []);
});

test('the stroke-gradient-along-path handlers (the real code) on native: notice only, no pushUndo / saveActiveLayerFrame, the stored gradient untouched', () => {
  const src = timelineSlice('function applyStrokeGradAlongToSelection(mutate', '// Figma-style color row');
  for (const native of [true, false]) {
    const root = timelineRoot(native);
    const along = root.document.nodes.get('p-strokegrad-along'), fromC = root.document.nodes.get('p-strokegrad-from-c'), toC = root.document.nodes.get('p-strokegrad-to-c');
    vm.runInContext(src, root);
    const before = JSON.stringify(root.selectedPaths[0].data);
    along.checked = false;
    if (!native) { assert.throws(() => along.listeners.change[0].call(along), /pushUndo reached/); assert.equal(root.calls[0], 'pushUndo', 'the baseline writer is reached on a legacy project'); assert.deepEqual(root.toasts, []); continue; }
    along.listeners.change[0].call(along);
    fromC.value = '#00ff00'; fromC.listeners.input[0].call(fromC);
    toC.value = '#00ffff'; toC.listeners.input[0].call(toC);
    assert.deepEqual(root.toasts, [Unavailable.noticeFor('p-strokegrad-along'), Unavailable.noticeFor('p-strokegrad-from-c'), Unavailable.noticeFor('p-strokegrad-to-c')]);
    assert.deepEqual(root.calls, []);
    assert.equal(JSON.stringify(root.selectedPaths[0].data), before);
    assert.equal(root.document.nodes.get('p-strokegrad-from').style.background, undefined, 'the swatch does not pretend to change');
    assert.equal(along.checked, true, 'the checkbox reverts');
  }
});

test('updateUI repaints the marks (a thin hook ahead of the native early return)', () => {
  const text = read('timeline.js'), at = text.indexOf('function updateUI(frameOnly){');
  const head = text.slice(at, at + 400);
  assert.ok(head.indexOf('NemoNativeInspectorUnavailable.sync(window)') > 0);
  assert.ok(head.indexOf('NemoNativeInspectorUnavailable.sync(window)') < head.indexOf('paintUI'), 'before the native early return');
});

function effectsHarness(native) {
  const root = rootFor({ native, ids: ['effects-stack-sec', 'p-add-effect-btn'] }), opened = [];
  Object.assign(root, { state: { activeLayerIdx: 0, currentFrame: 0, layers: [{}], customEffects: [] }, SM: { t: (k) => k }, console, innerWidth: 1000, innerHeight: 800, setTimeout, clearTimeout,
    openCustomEffectEditor: (def) => opened.push(def), NemoNativeInspectorUnavailable: Unavailable });
  root.window = root;
  root.addEventListener = () => {};
  vm.runInNewContext(read('effects-panel.js'), root, { filename: 'effects-panel.js' });
  root.document.nodes.get('p-add-effect-btn').listeners.click[0]({ stopPropagation() {} });
  return { root, opened, created: root.document.created };
}

test('effects panel: "+ New custom shader" is listed unavailable on native (title, aria-disabled) and says so when activated; legacy opens the editor', () => {
  for (const native of [true, false]) {
    const { root, opened, created } = effectsHarness(native);
    // The Custom category row listens for mouseenter and click; the last created submenu row is the "+ New custom shader…" entry.
    const rows = created.filter((el) => el.className === 'fx-addmenu-cat');
    const custom = rows[rows.length - 1];
    assert.ok(custom && custom.listeners.click, 'the custom category is listed');
    custom.listeners.click[0]({ stopPropagation() {} });
    const entry = created.filter((el) => el.className === 'fx-submenu-item').pop();
    assert.ok(entry, 'the entry is listed');
    if (native) {
      assert.equal(entry.attrs['aria-disabled'], 'true');
      assert.equal(entry.title, Unavailable.noticeFor('new-custom-shader'));
    } else {
      assert.equal(entry.attrs['aria-disabled'], undefined);
      assert.equal(entry.title, '');
    }
    entry.listeners.click[0]({ stopPropagation() {} });
    if (native) { assert.deepEqual(root.toasts, [Unavailable.noticeFor('new-custom-shader')]); assert.deepEqual(opened, []); }
    else { assert.deepEqual(root.toasts, []); assert.deepEqual(opened, [null]); }
  }
});
