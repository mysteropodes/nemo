'use strict';
// #2255: the ordinary rig-widget.js on a native object project. The real rig-widget.js, native-object-widget-ui.js and the widget
// adapter run in one vm context against a session double with the real command-route semantics; the tests observe the exact native
// requests, the document-capture listeners and that no legacy writer is ever reached.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { createFixture } = require('./native-object-rig-support.cjs');

const ROOT = path.join(__dirname, '..', 'src', 'js');
const SOURCES = ['adapters/native-object-rig-contract.js', 'adapters/native-object-rig-command.js', 'adapters/native-object-widget.js',
  'adapters/native-object-query-contract.js', 'adapters/native-object-widget-ui.js', 'rig-widget.js'];
const OP = 'command.document.object.widget.';
const UID = 'native-layer:1:0', LOCKED = 'native-layer:2:0';
const axis = (label) => ({ key: 'xc_' + label, min: -100, max: 100, rest: 0, label, value: 0 });
const joystick = (uid, over) => Object.assign({ layerUid: uid, visible: true, locked: false, kind: 'joystick', x: axis('X'), y: axis('Y'),
  center: [960, 540], angle: 0, size: [200, 200], puck: [0, 0], rest: [0, 0], unavailable: null }, over || {});

function harness(options) {
  options = options || {};
  const fx = createFixture({});
  const items = options.items || [joystick(UID), joystick(LOCKED, { locked: true, center: [200, 100], size: [100, 100] })];
  const listeners = [], fields = {}, legacy = [], toasts = [], reads = [];
  // The context keeps its own intrinsics: injecting the outer Object or Array would break the contract's own prototype checks.
  const sandbox = { console, setTimeout, clearTimeout };
  sandbox.window = sandbox; sandbox.globalThis = sandbox;
  sandbox.SM = { t: (key) => key };
  sandbox.view = { zoom: 1 };
  sandbox.state = { layers: [], currentFrame: 0, activeLayerIdx: 0, playing: false, canvasW: 960, canvasH: 540 };
  sandbox.showToast = (message) => toasts.push(message);
  // Every legacy writer and reader of the JS layer model fails loudly.
  for (const name of ['SMMotion', 'pushUndo', 'pushUndoLayers', 'saveAllLayerFrames', 'saveActiveLayerFrame', 'createUserLayer', 'loadFrame', 'activateUL']) {
    Object.defineProperty(sandbox, name, { configurable: true, get() { legacy.push(name); throw new Error('legacy writer reached: ' + name); } });
  }
  const area = { contains: (target) => target === sandbox.canvas };
  sandbox.canvas = { tag: 'canvas' };
  sandbox.document = {
    readyState: 'loading', body: null, documentElement: { appendChild() {} },
    addEventListener(type, fn, opts) { listeners.push({ type, fn, opts }); },
    getElementById(id) { return id === 'canvas-area' ? area : fields[id] || null; },
    createElementNS() { const node = { style: {}, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = v; }, appendChild(c) { this.children.push(c); this.firstChild = this.children[0]; }, removeChild() { this.children.shift(); this.firstChild = this.children[0]; }, isConnected: true }; return node; },
  };
  sandbox.document.body = { appendChild() {} };
  sandbox.requestAnimationFrame = (fn) => fn();
  const live = fx.session.info;
  const presentation = () => Object.assign(live(), { owner: 'native', status: 'presented', contextId: 'scene-root', frame: 0 });
  let shown = null, shownRevision = -1;
  const info = () => Object.assign(live(), { canvasW: 960, canvasH: 540, layers: [{ layerUid: UID }, { layerUid: LOCKED }] });
  Object.assign(fx.session, {
    info,
    readSubscriptionGeneration: () => 1,
    readPresentedObjectSession() {
      if (shownRevision !== fx.state.revision) { shownRevision = fx.state.revision; shown = Object.assign(info(), { available: true, kind: 'native-object', presentation: presentation() }); }
      return shown;
    },
    async readWidgets(selector, pin) {
      reads.push({ selector, pin: pin.contentRevision });
      return Object.freeze({ info: pin, presentation: shown.presentation, items: options.items || items });
    },
  });
  sandbox.NemoNativeObjectProject = { desktop: () => fx.session, selectLayerUI: (win, index) => { sandbox.selected = index; return true; } };
  sandbox.NemoNativeViewportUnderlay = { canvasMapping: Object.freeze({ bounds: () => ({ left: 0, top: 0, width: 960, height: 540 }), matchesView: () => true,
    point: (e) => ({ x: e.clientX, y: e.clientY }), screen: (p) => ({ x: p.x, y: p.y }) }) };
  vm.createContext(sandbox);
  // The owner double lives in this realm; the contract checks prototypes of its own realm, so its replies are rebuilt in the context.
  const intoContext = vm.runInContext('(text) => JSON.parse(text)', sandbox), route = fx.session.runResourceCommand;
  fx.session.runResourceCommand = (action) => route((ctx) => action({ dispatch: async (request) => intoContext(JSON.stringify(await ctx.dispatch(request))), reconcile: ctx.reconcile }));
  for (const file of SOURCES) vm.runInContext(fs.readFileSync(path.join(ROOT, file), 'utf8'), sandbox, { filename: file });
  const fire = (type, event) => {
    const e = Object.assign({ target: sandbox.canvas, button: 0, pointerId: 1, stopped: false, prevented: false,
      stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; } }, event);
    for (const l of listeners.filter((entry) => entry.type === type)) { l.fn(e); if (e.stopped) break; }
    return e;
  };
  return { fx, sandbox, listeners, fields, legacy, toasts, reads, fire,
    // Inside the context `window` is its global object, not the sandbox object: the controller is keyed by that one.
    ui: () => sandbox.NemoNativeObjectWidgetUI.desktop(vm.runInContext('globalThis', sandbox)),
      sent: () => { assert.deepEqual(toasts.filter((t) => !/widget_move_unavailable|^toastWidgetLayerAdded$/.test(t)), [], 'a refused or uncorrelated command would have been toasted'); return fx.owner.log.received.map((r) => [r.operation, r.payload]); } };
}
async function started(options) {
  const h = harness(options);
  h.fire('DOMContentLoaded', {});
  await h.ui().refresh();
  return h;
}

test('the widget listeners sit on document in the capture phase, so motion.js on #canvas-area cannot take the gesture first', async () => {
  const h = await started();
  const byType = Object.fromEntries(['pointerdown', 'pointermove', 'pointerup', 'pointercancel', 'dblclick'].map((t) => [t, h.listeners.filter((l) => l.type === t)]));
  // The legacy bridge and the native route both listen on document, both with `capture: true`; none on #canvas-area.
  for (const [type, list] of Object.entries(byType)) {
    assert.ok(list.length >= 2, type);
    assert.ok(list.every((l) => l.opts && l.opts.capture === true), type + ' capture');
  }
  assert.deepEqual(h.legacy, []);
});

test('a puck drag is a local preview and ONE widget.write with the world point, the composed anchor and the angle', async () => {
  const h = await started();
  const down = h.fire('pointerdown', { clientX: 960, clientY: 540 });
  assert.equal(down.stopped, true, 'the gesture is taken from motion.js and the brush');
  h.fire('pointermove', { clientX: 990, clientY: 500 });
  h.fire('pointermove', { clientX: 1010, clientY: 440 });
  assert.deepEqual(h.sent(), [], 'nothing is written while the pointer moves');
  const up = h.fire('pointerup', { clientX: 1010, clientY: 440 });
  assert.equal(up.stopped, true);
  await h.ui().refresh();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(h.toasts, []);
  assert.deepEqual(h.sent(), [[OP + 'write', { command: 'object.widget.write', layerUid: UID, frame: 0, to: { kind: 'world', point: [1010, 440], anchor: [960, 540], angleDegrees: 0 } }]]);
  assert.deepEqual(h.legacy, []);
});

test('a pad body click is consumed with a typed unavailable, a locked or hidden pad and an empty canvas are left to the pen', async () => {
  const h = await started({ items: [joystick(UID), joystick(LOCKED, { locked: true, center: [200, 100], size: [100, 100] }), joystick('native-layer:3:0', { visible: false, center: [700, 100], size: [100, 100] })] });
  const body = h.fire('pointerdown', { clientX: 1040, clientY: 620 });
  assert.equal(body.stopped, true);
  assert.match(h.toasts[0], /widget_move_unavailable/);
  assert.deepEqual(h.sent(), []);
  for (const point of [[200, 100], [700, 100], [10, 500]]) assert.equal(h.fire('pointerdown', { clientX: point[0], clientY: point[1] }).stopped, false, point.join());
  assert.equal(h.fire('pointerdown', { clientX: 960, clientY: 540, button: 2 }).stopped, false, 'a secondary button never grabs a pad');
  assert.deepEqual(h.legacy, []);
});

test('a double click returns the puck to rest with one widget.write', async () => {
  const h = await started();
  assert.equal(h.fire('dblclick', { clientX: 1000, clientY: 560 }).stopped, true);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(h.sent(), [[OP + 'write', { command: 'object.widget.write', layerUid: UID, frame: 0, to: { kind: 'rest' } }]]);
  assert.equal(h.fire('dblclick', { clientX: 5, clientY: 5 }).stopped, false);
});

test('a rotated pad maps the pointer through its angle and keeps the slider on its one axis', async () => {
  const slider = joystick(UID, { kind: 'slider', y: null, angle: 90, size: [200, 26], center: [100, 100], puck: [0, 0], rest: [0, 0] });
  const h = await started({ items: [slider] });
  // At 90 degrees the pad's local X runs down the screen: the world point (100, 150) is local (50, 0).
  assert.equal(h.fire('pointerdown', { clientX: 100, clientY: 100 }).stopped, true);
  h.fire('pointermove', { clientX: 130, clientY: 150 });
  h.fire('pointerup', { clientX: 130, clientY: 150 });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(h.sent(), [[OP + 'write', { command: 'object.widget.write', layerUid: UID, frame: 0, to: { kind: 'world', point: [130, 150], anchor: [100, 100], angleDegrees: 90 } }]]);
});

test('a pad whose placement is unavailable never takes a gesture and draws nothing', async () => {
  const lost = joystick(UID, { center: null, angle: null, size: null, puck: null, rest: null, unavailable: 'parent_geometry' });
  const h = await started({ items: [lost] });
  assert.equal(h.fire('pointerdown', { clientX: 960, clientY: 540 }).stopped, false);
  assert.deepEqual(h.sent(), []);
});

test('creating a widget from the ordinary menu is one widget.create at the canvas centre, never the JS layer model', async () => {
  const h = await started();
  assert.equal(h.sandbox.SMRigWidget.addWidgetLayer('slider'), -1);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(h.sent(), [[OP + 'create', { command: 'object.widget.create', kind: 'slider', name: 'Slider 3', pos: [480, 270] }]]);
  assert.equal(h.sandbox.selected, undefined, 'the double does not list the new layer, so nothing is selected by guess');
  assert.deepEqual(h.toasts, ['toastWidgetLayerAdded']);
  h.sandbox.SMRigWidget.addWidgetLayer('anything');
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.fx.owner.log.received[1].payload.kind, 'joystick', 'an unknown kind is a joystick, as in the baseline');
  assert.deepEqual(h.legacy, []);
});

test('the right panel edits one setting per field through widget.settings and the legacy field handlers stay inert', async () => {
  const h = harness();
  const field = (value) => { const f = { value, handlers: [], style: {}, textContent: '', addEventListener(type, fn) { if (type === 'change') this.handlers.push(fn); } }; return f; };
  for (const id of ['p-widget-sec', 'p-widget-w', 'p-widget-h', 'p-widget-xlabel', 'p-widget-xmin', 'p-widget-xmax', 'p-widget-xrest', 'p-widget-yrange-row',
    'p-widget-yrest-row', 'p-widget-ylabel', 'p-widget-ymin', 'p-widget-ymax', 'p-widget-yrest', 'p-widget-xadd', 'p-widget-yadd']) h.fields[id] = field('');
  h.fire('DOMContentLoaded', {});
  await h.ui().refresh();
  h.sandbox.renderRigWidgetPanel();
  await h.ui().refresh();
  h.sandbox.renderRigWidgetPanel();
  assert.equal(h.fields['p-widget-sec'].style.display, '');
  assert.deepEqual(['p-widget-w', 'p-widget-h', 'p-widget-xmin', 'p-widget-xmax', 'p-widget-xrest', 'p-widget-ymax'].map((id) => h.fields[id].value), [200, 200, -100, 100, 0, 100]);
  assert.equal(h.fields['p-widget-xlabel'].textContent, 'X');
  assert.equal(h.fields['p-widget-xadd'].style.display, 'none', 'the property-link button has no native route');
  const edits = [['p-widget-xmax', '50', { x: { max: 50 } }], ['p-widget-yrest', '-20', { y: { rest: -20 } }], ['p-widget-w', '300', { size: [300, 200] }], ['p-widget-h', '2', { size: [200, 8] }]];
  for (const [id, value, set] of edits) {
    h.fields[id].value = value;
    h.fields[id].handlers.forEach((fn) => fn());
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(h.sent().pop(), [OP + 'settings', { command: 'object.widget.settings', layerUid: UID, set }], id);
  }
  h.fields['p-widget-xmin'].value = 'abc';
  h.fields['p-widget-xmin'].handlers.forEach((fn) => fn());
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.sent().length, edits.length, 'a non-number is never sent');
  assert.deepEqual(h.legacy, []);
});

test('a gesture whose document moved underneath it is dropped, never sent against a newer revision', async () => {
  const h = await started();
  h.fx.owner.log.received.length = 0;
  h.fx.state.active = true;
  const before = h.reads.length;
  h.fire('pointerdown', { clientX: 960, clientY: 540 });
  h.fire('pointermove', { clientX: 1000, clientY: 540 });
  h.fx.state.revision += 1; // the document moved under the drag: its pin is stale
  h.fire('pointerup', { clientX: 1000, clientY: 540 });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(h.sent(), [], 'a stale gesture is dropped, not sent');
  assert.ok(h.reads.length >= before);
});
