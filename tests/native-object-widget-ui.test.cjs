'use strict';
// #2255: the ordinary rig-widget.js on a native object project. The real rig-widget.js, native-object-widget-ui.js and the widget
// adapter run in one vm context against a session double with the real command-route semantics; the tests observe the exact native
// requests, the document-capture listeners and that no legacy writer is ever reached.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { createFixture } = require('./native-object-rig-support.cjs');

const ROOT = path.join(__dirname, '..', 'src', 'js');
const SOURCES = ['adapters/native-presentation-wait.js', 'adapters/native-object-rig-contract.js', 'adapters/native-object-rig-command.js', 'adapters/native-object-widget.js',
  'adapters/native-object-query-contract.js', 'adapters/native-object-widget-ui.js', 'rig-widget.js'];
const OP = 'command.document.object.widget.';
const UID = 'native-layer:1:0', LOCKED = 'native-layer:2:0';
const axis = (label) => ({ key: 'xc_' + label, min: -100, max: 100, rest: 0, label, value: 0 });
const joystick = (uid, over) => Object.assign({ layerUid: uid, visible: true, locked: false, kind: 'joystick', x: axis('X'), y: axis('Y'),
  center: [960, 540], angle: 0, size: [200, 200], puck: [0, 0], rest: [0, 0], anchor: [960, 540], linear: [1, 0, 0, 1], unavailable: null }, over || {});

function harness(options) {
  options = options || {};
  const fx = createFixture({});
  const items = options.items || [joystick(UID), joystick(LOCKED, { locked: true, center: [200, 100], size: [100, 100] })];
  const listeners = [], fields = {}, legacy = [], toasts = [], reads = [], created = [], pan = { dx: 0, dy: 0 };
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
    createElementNS() { const node = { style: {}, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = v; }, appendChild(c) { this.children.push(c); this.firstChild = this.children[0]; }, removeChild() { this.children.shift(); this.firstChild = this.children[0]; }, isConnected: true }; created.push(node); return node; },
  };
  sandbox.document.body = { appendChild() {} };
  const frames = [];
  sandbox.requestAnimationFrame = (fn) => { frames.push(fn); return frames.length; };
  const flushFrames = () => { for (let n = 0; n < 4 && frames.length; n++) frames.splice(0).forEach((fn) => fn()); };
  const live = fx.session.info;
  const presentation = () => Object.assign(live(), { owner: 'native', status: 'presented', contextId: 'scene-root', frame: sandbox.state.currentFrame });
  let shown = null, shownRevision = -1;
  const info = () => Object.assign(live(), { canvasW: 960, canvasH: 540, layers: [{ layerUid: UID }, { layerUid: LOCKED }] });
  Object.assign(fx.session, {
    info,
    readSubscriptionGeneration: () => 1,
    readPresentedObjectSession() {
      if (shownRevision !== fx.state.revision || shown.presentation.frame !== sandbox.state.currentFrame) { shownRevision = fx.state.revision; shown = Object.assign(info(), { available: true, kind: 'native-object', presentation: presentation() }); }
      return shown;
    },
    async readWidgets(selector, pin) {
      reads.push({ selector, pin: pin.contentRevision });
      const read = Object.freeze({ info: pin, presentation: shown.presentation, items: options.items || items });
      if (options.representOnRead) shownRevision = -1; // installed: the read's completion re-presents the same revision as a new object
      return read;
    },
  });
  sandbox.NemoNativeObjectProject = { desktop: () => fx.session, selectLayerUI: (win, index) => { sandbox.selected = index; return true; } };
  sandbox.NemoNativeViewportUnderlay = { canvasMapping: Object.freeze({ bounds: () => ({ left: 0, top: 0, width: 960, height: 540 }), matchesView: () => true,
    point: (e) => ({ x: e.clientX - pan.dx, y: e.clientY - pan.dy }), screen: (p) => ({ x: p.x + pan.dx, y: p.y + pan.dy }) }) };
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
  return { represent: () => { shownRevision = -1; }, pan, created, fx, sandbox, listeners, fields, legacy, toasts, reads, fire, flushFrames, frames,
    // Inside the context `window` is its global object, not the sandbox object: the controller is keyed by that one.
    ui: () => sandbox.NemoNativeObjectWidgetUI.desktop(vm.runInContext('globalThis', sandbox)),
      sent: () => { assert.deepEqual(toasts.filter((t) => !/^toastWidgetLayerAdded$/.test(t)), [], 'a refused or uncorrelated command would have been toasted'); return fx.owner.log.received.map((r) => [r.operation, r.payload]); } };
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

test('a locked or hidden pad and an empty canvas are left to the pen, and a secondary button never grabs a pad', async () => {
  const h = await started({ items: [joystick(UID), joystick(LOCKED, { locked: true, center: [200, 100], size: [100, 100] }), joystick('native-layer:3:0', { visible: false, center: [700, 100], size: [100, 100] })] });
  for (const point of [[200, 100], [700, 100], [10, 500]]) assert.equal(h.fire('pointerdown', { clientX: point[0], clientY: point[1] }).stopped, false, point.join());
  assert.equal(h.fire('pointerdown', { clientX: 960, clientY: 540, button: 2 }).stopped, false);
  assert.deepEqual(h.sent(), []);
  assert.deepEqual(h.legacy, []);
});

test('a pad body drag moves the whole pad: a local preview and ONE widget.settings with the new world pos', async () => {
  const h = await started();
  assert.equal(h.fire('pointerdown', { clientX: 1040, clientY: 620 }).stopped, true, 'the body is taken from motion.js and the brush');
  h.fire('pointermove', { clientX: 1060, clientY: 600 });
  h.fire('pointermove', { clientX: 1090, clientY: 660 });
  assert.deepEqual(h.sent(), [], 'nothing is written while the pointer moves');
  assert.equal(h.fire('pointerup', { clientX: 1090, clientY: 660 }).stopped, true);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(h.sent(), [[OP + 'settings', { command: 'object.widget.settings', layerUid: UID, set: { pos: [1010, 580] } }]]);
  assert.deepEqual(h.legacy, []);
});

test('a pad under a rotated, scaled parent moves through the inverse of the parents\' linear part', async () => {
  // A quarter turn doubled: x steps to +2y, y steps to -2x. A world delta (+20, +40) is the local (20, -10).
  const turned = joystick(UID, { anchor: [100, 50], linear: [0, 2, -2, 0], center: [500, 300], angle: 90 });
  const h = await started({ items: [turned] });
  h.fire('pointerdown', { clientX: 540, clientY: 380 });
  h.fire('pointermove', { clientX: 560, clientY: 420 });
  h.fire('pointerup', { clientX: 560, clientY: 420 });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(h.sent(), [[OP + 'settings', { command: 'object.widget.settings', layerUid: UID, set: { pos: [120, 40] } }]]);
});

test('a pad whose parents collapse the plane is a typed widget_move_unavailable and writes nothing; a click without a drag writes nothing', async () => {
  const h = await started({ items: [joystick(UID, { linear: [1, 2, 2, 4] })] });
  h.fire('pointerdown', { clientX: 1040, clientY: 620 });
  h.fire('pointermove', { clientX: 1060, clientY: 640 });
  h.fire('pointerup', { clientX: 1060, clientY: 640 });
  await new Promise((resolve) => setImmediate(resolve));
  assert.match(h.toasts[0], /widget_move_unavailable/);
  assert.equal(h.fx.owner.log.received.length, 0);
  const still = await started();
  still.fire('pointerdown', { clientX: 1040, clientY: 620 });
  still.fire('pointerup', { clientX: 1040, clientY: 620 });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(still.sent(), []);
});

test('the overlay re-reads on a revision change and on a frame change by itself, without a panel update', async () => {
  const h = await started();
  h.flushFrames();
  const base = h.reads.length;
  h.fx.state.revision += 1;
  h.flushFrames();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.reads.length, base + 1, 'one read for the new revision');
  assert.equal(h.reads[h.reads.length - 1].pin, h.fx.state.revision);
  h.sandbox.state.currentFrame = 3;
  h.flushFrames();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.reads.length, base + 2, 'one read for the new frame');
  assert.equal(h.reads[h.reads.length - 1].selector.frame, 3);
  h.flushFrames();
  assert.equal(h.reads.length, base + 2, 'an unchanged pin and frame cost no read');
  h.sandbox.state.playing = true;
  h.flushFrames();
  assert.equal(h.reads.length, base + 2, 'nothing is read while playing');
  assert.deepEqual(h.legacy, []);
});

test('a read that re-presents the same revision still draws its pads, and new presentations never read again (installed, #2255)', async () => {
  const h = await started({ representOnRead: true });
  h.flushFrames(); await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.ui().items().length, 2, 'the landed read is kept and its pads are drawn on the new presentation');
  const base = h.reads.length;
  for (let i = 0; i < 5; i++) { h.represent(); h.flushFrames(); await new Promise((resolve) => setImmediate(resolve)); }
  assert.equal(h.reads.length, base, 'same revision and frame, new presentation objects: no read (no loop)');
  assert.equal(h.ui().items().length, 2, 'the pads stay');
  h.fx.state.revision += 1; h.flushFrames(); await new Promise((resolve) => setImmediate(resolve)); h.flushFrames();
  assert.equal(h.reads.length, base + 1, 'a new revision is read once'); assert.equal(h.ui().items().length, 2);
});

test('Widget settings on a native layer row edits the axis range and the size through widget.settings, with the baseline prompts', async () => {
  const h = await started();
  const prompts = [], menus = [];
  h.sandbox.showContextMenu = (x, y, entries) => menus.push({ x, y, entries });
  h.sandbox.prompt = (text, value) => { prompts.push([text, value]); return prompts.length === 1 ? '-50, 50, 5' : '300, 4'; };
  h.sandbox.SMRigWidget.openWidgetMenu(10, 20, 0);
  assert.equal(menus.length, 1);
  assert.deepEqual([menus[0].x, menus[0].y], [10, 20]);
  assert.deepEqual(Array.from(menus[0].entries, (e) => e.label), ['widgetMenuAxisRangePrefixX  (-100…100)', 'widgetMenuAxisRangePrefixY  (-100…100)', 'widgetMenuSizeEllipsis']);
  menus[0].entries[0].action();
  await new Promise((resolve) => setImmediate(resolve));
  menus[0].entries[2].action();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(prompts, [['promptWidgetRange', '-100, 100, 0'], ['promptWidgetSize', '200, 200']]);
  assert.deepEqual(h.sent(), [[OP + 'settings', { command: 'object.widget.settings', layerUid: UID, set: { x: { min: -50, max: 50, rest: 5 } } }],
    [OP + 'settings', { command: 'object.widget.settings', layerUid: UID, set: { size: [300, 8] } }]]);
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
  assert.equal(h.fields['p-widget-xadd'].style.display, '', 'D18-C: the property-link button routes to the native link menu');
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

const PANEL_IDS = ['p-widget-sec', 'p-widget-w', 'p-widget-h', 'p-widget-xlabel', 'p-widget-xmin', 'p-widget-xmax', 'p-widget-xrest', 'p-widget-yrange-row',
  'p-widget-yrest-row', 'p-widget-ylabel', 'p-widget-ymin', 'p-widget-ymax', 'p-widget-yrest', 'p-widget-xadd', 'p-widget-yadd'];
const panelField = (value) => ({ value, handlers: [], style: {}, textContent: '', addEventListener(type, fn) { if (type === 'change') this.handlers.push(fn); } });

test('the right panel appears with its values when the pending read lands, without a second renderRigWidgetPanel() call', async () => {
  const h = harness();
  PANEL_IDS.forEach((id) => { h.fields[id] = panelField(''); });
  h.fire('DOMContentLoaded', {});
  h.sandbox.renderRigWidgetPanel();
  assert.equal(h.fields['p-widget-sec'].style.display, 'none', 'no read has landed yet, so nothing is shown');
  await new Promise((resolve) => setImmediate(resolve));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.fields['p-widget-sec'].style.display, '', 'the landed read shows the section by itself');
  assert.deepEqual(['p-widget-w', 'p-widget-h', 'p-widget-xmin', 'p-widget-xmax', 'p-widget-xrest', 'p-widget-ymax'].map((id) => h.fields[id].value), [200, 200, -100, 100, 0, 100]);
  assert.equal(h.fields['p-widget-xlabel'].textContent, 'X');
  assert.deepEqual(h.legacy, []);
});

test('the right panel repaints after a settings edit lands, from the new native read', async () => {
  const items = [joystick(UID)];
  const h = harness({ items });
  PANEL_IDS.forEach((id) => { h.fields[id] = panelField(''); });
  h.fire('DOMContentLoaded', {});
  h.sandbox.renderRigWidgetPanel();
  await new Promise((resolve) => setImmediate(resolve));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.fields['p-widget-xmax'].value, 100);
  // The native owner applies the edit and the next read reports it (the double reads the shared `items`).
  const route = h.fx.session.runResourceCommand;
  h.fx.session.runResourceCommand = (action) => route(action).then((out) => { items[0] = Object.assign({}, items[0], { x: Object.assign({}, items[0].x, { max: 50 }) }); return out; });
  h.fields['p-widget-xmax'].value = '50';
  h.fields['p-widget-xmax'].handlers.forEach((fn) => fn());
  for (let n = 0; n < 6; n++) await new Promise((resolve) => setImmediate(resolve));
  h.flushFrames();
  assert.equal(h.sent().pop()[1].set.x.max, 50);
  assert.equal(Number(h.fields['p-widget-xmax'].value), 50);
  items[0] = Object.assign({}, items[0], { x: Object.assign({}, items[0].x, { max: 80 }) });
  h.fx.state.revision += 1;
  h.flushFrames();
  for (let n = 0; n < 4; n++) await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.fields['p-widget-xmax'].value, 80, 'a later native revision repaints the panel without another renderRigWidgetPanel()');
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

test('a Space-held or panning pointerdown over a pad is not consumed, so the pan gesture keeps it', async () => {
  for (const flag of ['spaceDown', 'isPanning']) {
    const h = await started();
    h.sandbox.state[flag] = true;
    const e = h.fire('pointerdown', { clientX: 960, clientY: 540 });
    assert.equal(e.stopped, false, flag);
    assert.equal(e.prevented, false, flag);
    h.fire('pointerup', { clientX: 1000, clientY: 540 });
    assert.deepEqual(h.sent(), [], flag);
    h.sandbox.state[flag] = false;
    assert.equal(h.fire('pointerdown', { clientX: 960, clientY: 540 }).stopped, true, flag + ' released: the pad takes the gesture again');
    h.fire('pointerup', { clientX: 960, clientY: 540 });
  }
});

test('the widget overlay follows a pan of the view by itself, with no revision or frame change and no read', async () => {
  const h = await started();
  h.flushFrames();
  const svg = h.created[0], polygon = () => svg.children.find((n) => n.attrs && n.attrs.points);
  const before = polygon().attrs.points, base = h.reads.length;
  h.pan.dx = 40; h.pan.dy = -25;
  h.flushFrames();
  const after = polygon().attrs.points;
  assert.notEqual(after, before);
  const first = (text) => text.split(' ')[0].split(',').map(Number);
  assert.deepEqual([first(after)[0] - first(before)[0], first(after)[1] - first(before)[1]], [40, -25]);
  assert.equal(h.reads.length, base);
});

// #2372 presentation-busy-audit: the native queue hides the presentation while ANY command is pending (queue-pending).
function hideable(h) {
  const real = h.fx.session.readPresentedObjectSession, state = { hidden: false };
  h.fx.session.readPresentedObjectSession = function (why) { if (state.hidden) { if (why) why('queue-pending'); return null; } return real.call(this); };
  h.sandbox.setTimeout = (fn) => setImmediate(fn);
  return state;
}
const spin = async (n) => { for (let i = 0; i < n; i++) await new Promise((resolve) => setImmediate(resolve)); };

test('a puck release in the queue-pending window waits for the presentation and then writes once; the drag survives the hidden frames', async () => {
  const h = await started(), q = hideable(h);
  h.fire('pointerdown', { clientX: 960, clientY: 540 });
  q.hidden = true;
  h.fire('pointermove', { clientX: 990, clientY: 500 });
  h.flushFrames(); // the overlay watcher runs while the presentation is hidden: the pad and the drag stay
  h.fire('pointermove', { clientX: 1010, clientY: 440 });
  h.fire('pointerup', { clientX: 1010, clientY: 440 });
  await spin(4);
  assert.deepEqual(h.sent(), [], 'nothing is written while the presentation is hidden');
  q.hidden = false;
  await spin(12);
  assert.deepEqual(h.toasts, []);
  assert.deepEqual(h.sent(), [[OP + 'write', { command: 'object.widget.write', layerUid: UID, frame: 0, to: { kind: 'world', point: [1010, 440], anchor: [960, 540], angleDegrees: 0 } }]]);
});

test('a puck release whose playhead moved during the wait still refuses: nothing is written', async () => {
  const h = await started(), q = hideable(h);
  h.fire('pointerdown', { clientX: 960, clientY: 540 });
  h.fire('pointermove', { clientX: 1010, clientY: 440 });
  q.hidden = true;
  h.fire('pointerup', { clientX: 1010, clientY: 440 });
  h.sandbox.state.currentFrame = 1;
  q.hidden = false;
  await spin(12);
  assert.deepEqual(h.sent(), []);
});

test('a queue that never frees gives a puck release one clear notice and writes nothing', async () => {
  const h = await started(), q = hideable(h);
  h.fire('pointerdown', { clientX: 960, clientY: 540 });
  h.fire('pointermove', { clientX: 1010, clientY: 440 });
  q.hidden = true;
  h.fire('pointerup', { clientX: 1010, clientY: 440 });
  await spin(40);
  assert.deepEqual(h.toasts, ['The canvas is still busy - try the pad again.']);
  assert.deepEqual(h.fx.owner.log.received, []);
});
