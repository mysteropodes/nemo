'use strict';
// #2255: the Rig canvas tool on a native object project. The real native-object-rig-canvas.js, the weight popover, the widget UI's proof
// and mapping, the rig adapter and rig-bridge.js run in one vm context against a session double with the real command-route semantics and
// a fake `readRig`; the tests observe the exact native commands (one per gesture), the document-capture listeners, the typed unavailables
// and that no legacy rig writer is ever reached.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { createFixture } = require('./native-object-rig-support.cjs');

const ROOT = path.join(__dirname, '..', 'src', 'js');
const SOURCES = ['adapters/native-object-rig-contract.js', 'adapters/native-object-rig-command.js', 'adapters/native-object-rig.js', 'adapters/native-object-widget-ui.js',
  'adapters/native-object-rig-weights-ui.js', 'adapters/native-object-rig-canvas.js', 'rig-bridge.js'];
const OP = 'command.document.object.rig.edit';
const UID = 'native-layer:1:0';
const LEGACY = ['rigResetPose', 'rigCommitFrame', 'rigSetIK', 'rigBindStroke', 'rigBindMesh', 'rigAutoAssignLayer', 'ensureLayerRig', 'rigFreshId', 'rigWeighOnePoint', 'rigDragIKEnd',
  'applyRigDeform', 'pushUndo', 'SMMotion', 'createUserLayer', 'saveAllLayerFrames'];
const vert = (boneId, vi, point, hi, ho) => ({ boneId, vi, point, handleIn: hi, handleOut: ho });
function rigData() {
  return {
    unavailable: null,
    parts: {
      bones: [{ boneId: 'bone1', closed: false, radius: null, count: 2, center: [150, 100] }, { boneId: 'bone2', closed: false, radius: 60, count: 2, center: [150, 300] }],
      vertices: [vert('bone1', 0, [100, 100], [0, 0], [20, 0]), vert('bone1', 1, [200, 100], [-20, 0], [0, 0]), vert('bone2', 0, [100, 300], [0, 0], [0, 0]), vert('bone2', 1, [200, 300], [0, 0], [0, 0])],
      chains: [], binds: [{ index: 0, strokeId: 's1', meshId: null, points: 2, rotate: false, state: 'posed' }],
      bound: [[0, 0, 120, 130], [0, 1, 180, 130]],
    },
    weights: { '0:1': [{ entry: 0, boneId: 'bone1', ptIndex: 1, offset: null, w: 0.75 }, { entry: 1, boneId: 'bone1', ptIndex: null, offset: 12.5, w: 0.25 }] },
  };
}
class El {
  constructor(tag) { this.tag = tag; this.children = []; this.handlers = {}; this.style = {}; this.className = ''; this.textContent = ''; this.value = ''; this.connected = false; }
  appendChild(c) { this.children.push(c); c.parent = this; return c; }
  set innerHTML(v) { this.children = []; }
  addEventListener(type, fn) { (this.handlers[type] = this.handlers[type] || []).push(fn); }
  remove() { this.removed = true; }
  contains() { return true; }
  find(cls) { const out = []; (function walk(n) { if ((n.className || '').split(' ').includes(cls)) out.push(n); n.children.forEach(walk); })(this); return out; }
  fire(type) { (this.handlers[type] || []).forEach((fn) => fn({ target: this })); }
}

function harness(options) {
  options = options || {};
  const fx = createFixture({}), data = rigData(), listeners = [], fields = {}, legacy = [], toasts = [], reads = [], frames = [], body = new El('body');
  const sandbox = { console, setTimeout, clearTimeout };
  sandbox.window = sandbox; sandbox.globalThis = sandbox;
  sandbox.SM = { t: (key) => key };
  sandbox.state = { layers: [], currentFrame: 0, activeLayerIdx: 0, playing: false, tool: 'rig', rigSubMode: options.mode || 'draw' };
  sandbox.showToast = (message) => toasts.push(message);
  for (const name of LEGACY) Object.defineProperty(sandbox, name, { configurable: true, get() { legacy.push(name); throw new Error('legacy writer reached: ' + name); } });
  sandbox.canvas = { tag: 'canvas' };
  const area = { contains: (target) => target === sandbox.canvas, listeners: [], addEventListener(type, fn, opts) { this.listeners.push({ type, fn, opts }); } };
  sandbox.document = { readyState: 'complete', body, documentElement: body,
    addEventListener(type, fn, opts) { listeners.push({ type, fn, opts }); }, removeEventListener() {},
    getElementById(id) { return id === 'canvas-area' ? area : fields[id] || null; },
    createElement: (tag) => new El(tag),
    createElementNS() { const node = { style: {}, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = v; }, appendChild(c) { this.children.push(c); this.firstChild = this.children[0]; }, removeChild() { this.children.shift(); this.firstChild = this.children[0]; }, isConnected: true }; return node; } };
  sandbox.requestAnimationFrame = (fn) => { frames.push(fn); return frames.length; };
  const flushFrames = () => { for (let n = 0; n < 4 && frames.length; n++) frames.splice(0).forEach((fn) => fn()); };
  const view = { dx: 0, dy: 0 }; // pan offset of the viewport; zoom changes the same mapping
  const live = fx.session.info;
  const info = () => Object.assign(live(), { canvasW: 960, canvasH: 540, layers: [{ layerUid: UID }] });
  const presentation = () => Object.assign(live(), { owner: 'native', status: 'presented', contextId: 'scene-root', frame: sandbox.state.currentFrame });
  let shown = null, shownKey = '';
  Object.assign(fx.session, { info, readSubscriptionGeneration: () => 1,
    readPresentedObjectSession() {
      const key = fx.state.revision + ':' + sandbox.state.currentFrame;
      if (shownKey !== key) { shownKey = key; shown = Object.assign(info(), { available: true, kind: 'native-object', presentation: presentation() }); }
      return shown;
    },
    async readRig(selector, pin, parts) {
      reads.push({ selector, pin: pin.contentRevision, parts: Array.from(parts) });
      const out = {};
      parts.forEach((p) => { out[p] = data.parts[p]; });
      return Object.freeze({ info: pin, presentation: shown.presentation, layerUid: selector.layerUid, unavailable: data.unavailable, parts: Object.freeze(out) });
    },
    async readRigWeights(selector, pin, bind, vertex) {
      reads.push({ selector, weights: [bind, vertex] });
      return Object.freeze({ info: pin, presentation: shown.presentation, layerUid: selector.layerUid, unavailable: null, parts: Object.freeze({ weights: data.weights[bind + ':' + vertex] || [] }) });
    } });
  sandbox.NemoNativeObjectProject = { desktop: () => (options.legacy ? null : fx.session) };
  sandbox.NemoNativeViewportUnderlay = { canvasMapping: Object.freeze({ bounds: () => ({ left: 0, top: 0, width: 960, height: 540 }), matchesView: () => true,
    point: (e) => ({ x: e.clientX - view.dx, y: e.clientY - view.dy }), screen: (p) => ({ x: p.x + view.dx, y: p.y + view.dy }) }) };
  vm.createContext(sandbox);
  const intoContext = vm.runInContext('(text) => JSON.parse(text)', sandbox), route = fx.session.runResourceCommand;
  fx.session.runResourceCommand = (action) => route((ctx) => action({ dispatch: async (request) => intoContext(JSON.stringify(await ctx.dispatch(request))), reconcile: ctx.reconcile }));
  for (const file of SOURCES) vm.runInContext(fs.readFileSync(path.join(ROOT, file), 'utf8'), sandbox, { filename: file });
  const fire = (type, event) => {
    const e = Object.assign({ target: sandbox.canvas, button: 0, pointerId: 1, stopped: false, prevented: false,
      stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; } }, event);
    for (const l of [...listeners.filter((x) => x.type === type), ...area.listeners.filter((x) => x.type === type)]) { l.fn(e); if (e.stopped) break; }
    return e;
  };
  const win = vm.runInContext('globalThis', sandbox);
  return { view, fx, sandbox, data, listeners, area, fields, legacy, toasts, reads, body, fire, flushFrames, win,
    ui: () => sandbox.NemoNativeObjectRigCanvas.desktop(win),
    sent: () => fx.owner.log.received.map((r) => [r.operation, r.payload]),
    settle: async () => { for (let n = 0; n < 6; n++) { await new Promise((resolve) => setImmediate(resolve)); flushFrames(); } } };
}
async function started(options) {
  const h = harness(options);
  h.fire('pointermove', { clientX: 1, clientY: 1 });
  await h.settle();
  return h;
}
const edit = (action) => [OP, { command: 'object.rig.edit', layerUid: UID, action }];
const gesture = async (h, steps) => { for (const [type, event] of steps) { h.fire(type, event); } await h.settle(); };

test('the canvas listeners sit on document in the capture phase and the legacy bridge takes nothing on a native project', async () => {
  const h = await started();
  for (const type of ['pointerdown', 'pointermove', 'pointerup', 'pointercancel', 'keydown']) {
    const list = h.listeners.filter((l) => l.type === type);
    assert.ok(list.length >= 1 && list.every((l) => l.opts && l.opts.capture === true), type);
  }
  assert.equal(h.area.listeners.some((l) => l.type === 'pointerdown' && l.opts && l.opts.capture), true, 'rig-bridge.js is loaded');
  const vmEval = (text) => vm.runInContext(text, h.sandbox);
  assert.equal(vmEval('NemoNativeObjectRigCanvas.active(window)'), true);
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.reads[0].parts, ['bones', 'vertices', 'chains', 'binds'], 'Tracer and Déplacer read no bound vertices');
});

test('other tools, playback, a secondary button and a target outside the canvas are left alone; a legacy project is never taken', async () => {
  const h = await started();
  h.sandbox.state.tool = 'draw';
  assert.equal(h.fire('pointerdown', { clientX: 100, clientY: 100 }).stopped, false);
  h.sandbox.state.tool = 'rig'; h.sandbox.state.playing = true;
  assert.equal(h.fire('pointerdown', { clientX: 100, clientY: 100 }).stopped, false);
  h.sandbox.state.playing = false;
  assert.equal(h.fire('pointerdown', { clientX: 100, clientY: 100, button: 2 }).stopped, false);
  assert.equal(h.fire('pointerdown', { clientX: 100, clientY: 100, target: { tag: 'panel' } }).stopped, false);
  assert.deepEqual(h.sent(), []);
  const legacy = harness({ legacy: true });
  assert.equal(legacy.fire('pointerdown', { clientX: 1, clientY: 1 }).stopped, false, 'a legacy project keeps rig-bridge.js');
  assert.deepEqual(legacy.sent(), []);
});

test('Déplacer: an anchor drag is a local preview and ONE moveSegment on release', async () => {
  const h = await started({ mode: 'move' });
  assert.equal(h.fire('pointerdown', { clientX: 203, clientY: 98 }).stopped, true);
  h.fire('pointermove', { clientX: 230, clientY: 130 });
  h.fire('pointermove', { clientX: 240, clientY: 150 });
  assert.deepEqual(h.sent(), [], 'nothing is written while the pointer moves');
  assert.equal(h.fire('pointerup', { clientX: 240, clientY: 150 }).stopped, true);
  await h.settle();
  assert.deepEqual(h.sent(), [edit({ kind: 'moveSegment', boneId: 'bone1', vi: 1, part: 'point', to: [240, 150] })]);
  assert.deepEqual(h.toasts, []);
  assert.deepEqual(h.legacy, []);
});

test('Déplacer: a handle drag mirrors the other handle unless Alt breaks the tangent, and a miss writes nothing', async () => {
  const h = await started({ mode: 'move' });
  await gesture(h, [['pointerdown', { clientX: 120, clientY: 100 }], ['pointermove', { clientX: 130, clientY: 120 }], ['pointerup', { clientX: 130, clientY: 120 }]]);
  await gesture(h, [['pointerdown', { clientX: 180, clientY: 100 }], ['pointermove', { clientX: 170, clientY: 90, altKey: true }], ['pointerup', { clientX: 170, clientY: 90, altKey: true }]]);
  assert.deepEqual(h.sent(), [edit({ kind: 'moveSegment', boneId: 'bone1', vi: 0, part: 'handleOut', to: [130, 120], breakTangent: false }),
    edit({ kind: 'moveSegment', boneId: 'bone1', vi: 1, part: 'handleIn', to: [170, 90], breakTangent: true })]);
  assert.equal(h.fire('pointerdown', { clientX: 600, clientY: 400 }).stopped, true, 'a miss still takes the gesture so nothing else draws');
  h.fire('pointerup', { clientX: 600, clientY: 400 });
  assert.equal(h.sent().length, 2);
});

test('Déplacer: Alt on an anchor is the typed rig_pose_drag_unavailable and sends nothing', async () => {
  const h = await started({ mode: 'move' });
  assert.equal(h.fire('pointerdown', { clientX: 100, clientY: 100, altKey: true }).stopped, true);
  await h.settle();
  assert.match(h.toasts[0], /rig_pose_drag_unavailable/);
  h.fire('pointermove', { clientX: 150, clientY: 150 });
  h.fire('pointerup', { clientX: 150, clientY: 150 });
  await h.settle();
  assert.deepEqual(h.sent(), []);
  assert.deepEqual(h.legacy, []);
});

test('Assigner: a rim drag is ONE setBoneRadius; the radius floor is 1 and the bone keeps its own radius over the panel default', async () => {
  const h = await started({ mode: 'assign' });
  assert.deepEqual(h.reads[0].parts, ['bones', 'vertices', 'chains', 'binds', 'bound']);
  // bone1 has no radius: the panel default 200 around its centre (150,100); its rim at (350,100).
  await gesture(h, [['pointerdown', { clientX: 350, clientY: 103 }], ['pointermove', { clientX: 330, clientY: 100 }], ['pointermove', { clientX: 150, clientY: 100 }], ['pointerup', { clientX: 150, clientY: 100 }]]);
  // bone2 has radius 60 around (150,300): its rim at (210,300).
  await gesture(h, [['pointerdown', { clientX: 210, clientY: 300 }], ['pointermove', { clientX: 250, clientY: 340 }], ['pointerup', { clientX: 250, clientY: 340 }]]);
  assert.deepEqual(h.sent(), [edit({ kind: 'setBoneRadius', boneId: 'bone1', radius: 1 }), edit({ kind: 'setBoneRadius', boneId: 'bone2', radius: Math.hypot(100, 40) })]);
  assert.deepEqual(h.toasts, []);
});

test('Assigner: a bound vertex opens the weight popover; each edit is ONE native command and the popover shows the re-read entries', async () => {
  const h = await started({ mode: 'assign' });
  assert.equal(h.fire('pointerdown', { clientX: 181, clientY: 131 }).stopped, true);
  await h.settle();
  const pop = h.body.children.find((c) => c.className === 'rig-weight-popover');
  assert.ok(pop && pop.className === 'rig-weight-popover');
  assert.equal(pop.find('rig-weight-title')[0].textContent, 'Poids du vertex #2');
  const inputs = pop.find('rig-weight-input');
  assert.deepEqual(inputs.map((i) => i.value), ['75', '25']);
  assert.deepEqual(h.sent(), [], 'opening the popover changes nothing');
  inputs[0].value = '40'; inputs[0].fire('change');
  await h.settle();
  const target = { strokeId: 's1' };
  assert.deepEqual(h.sent(), [edit({ kind: 'setWeight', target, vertex: 1, entry: 0, percent: 40 })]);
  pop.find('rig-weight-del')[1].fire('click');
  await h.settle();
  const add = pop.find('rig-weight-add')[0];
  assert.deepEqual(Array.from(add.children[0].children, (o) => o.value), ['bone2'], 'only the bones that do not already weigh this vertex');
  add.children[0].value = 'bone2'; add.children[1].fire('click');
  await h.settle();
  pop.find('pbtn').filter((b) => b.textContent.startsWith('Réinit'))[0].fire('click');
  await h.settle();
  assert.deepEqual(h.sent().slice(1), [edit({ kind: 'removeWeight', target, vertex: 1, entry: 1 }), edit({ kind: 'addWeight', target, vertex: 1, boneId: 'bone2' }),
    edit({ kind: 'resetWeights', target, vertex: 1, radius: 200, softness: 0 })]);
  assert.equal(pop.find('rig-weight-input').length, 2, 'the same popover is redrawn in place');
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.toasts, []);
});

test('Tracer: click, click and drag, double click is ONE drawBone with symmetric handles; clicking the first anchor closes it', async () => {
  const h = await started({ mode: 'draw' });
  await gesture(h, [['pointerdown', { clientX: 400, clientY: 400 }], ['pointerup', { clientX: 400, clientY: 400 }],
    ['pointerdown', { clientX: 500, clientY: 400 }], ['pointermove', { clientX: 520, clientY: 430 }], ['pointerup', { clientX: 520, clientY: 430 }]]);
  assert.deepEqual(h.sent(), [], 'nothing is written while the bone is being drawn');
  const now = Date.now;
  Date.now = () => now() + 1000;
  try { await gesture(h, [['pointerdown', { clientX: 600, clientY: 500 }], ['pointerup', { clientX: 600, clientY: 500 }]]); } finally { Date.now = now; }
  h.fire('pointerdown', { clientX: 602, clientY: 501 });
  await h.settle();
  assert.deepEqual(h.sent(), [edit({ kind: 'drawBone', closed: false, segments: [
    { point: [400, 400], handleIn: [0, 0], handleOut: [0, 0] },
    { point: [500, 400], handleIn: [-20, -30], handleOut: [20, 30] },
    { point: [600, 500], handleIn: [0, 0], handleOut: [0, 0] }] })]);
  await gesture(h, [['pointerup', {}], ['pointerdown', { clientX: 400, clientY: 600 }], ['pointerup', { clientX: 400, clientY: 600 }],
    ['pointerdown', { clientX: 500, clientY: 650 }], ['pointerup', { clientX: 500, clientY: 650 }], ['pointerdown', { clientX: 450, clientY: 700 }], ['pointerup', { clientX: 450, clientY: 700 }],
    ['pointerdown', { clientX: 402, clientY: 601 }], ['pointerup', {}]]);
  assert.equal(h.sent().length, 2);
  assert.equal(h.sent()[1][1].action.closed, true);
  assert.equal(h.sent()[1][1].action.segments.length, 3);
});

test('Tracer: a plain click on a bone tip branches a new bone from that exact tip, Alt extends the same bone, Enter finishes', async () => {
  const h = await started({ mode: 'draw' });
  await gesture(h, [['pointerdown', { clientX: 203, clientY: 102 }], ['pointerup', { clientX: 203, clientY: 102 }], ['pointerdown', { clientX: 300, clientY: 200 }], ['pointerup', { clientX: 300, clientY: 200 }]]);
  h.fire('keydown', { key: 'Enter' });
  await h.settle();
  assert.deepEqual(h.sent()[0][1].action.segments[0], { point: [200, 100], handleIn: [0, 0], handleOut: [0, 0] });
  assert.equal(Object.prototype.hasOwnProperty.call(h.sent()[0][1].action, 'boneId'), false, 'a branch is a fresh bone');
  await gesture(h, [['pointerdown', { clientX: 100, clientY: 102, altKey: true }], ['pointerup', {}], ['pointerdown', { clientX: 50, clientY: 50 }], ['pointerup', {}]]);
  h.fire('keydown', { key: 'Enter' });
  await h.settle();
  const extended = h.sent()[1][1].action;
  assert.equal(extended.boneId, 'bone1');
  assert.deepEqual(extended.segments.map((s) => s.point), [[200, 100], [100, 100], [50, 50]], 'the open first end is reversed so the new anchors extend it');
  assert.deepEqual(extended.segments[1].handleIn, [20, 0], 'a reversed bone swaps each handle pair');
});

test('a layer with motion or an expression is a typed rig_presentation_unavailable: the gesture is taken and nothing is sent', async () => {
  const h = harness({ mode: 'move' });
  h.data.unavailable = 'motion';
  h.fire('pointermove', { clientX: 1, clientY: 1 });
  await h.settle();
  assert.equal(h.fire('pointerdown', { clientX: 100, clientY: 100 }).stopped, true);
  assert.match(h.toasts[0], /rig_presentation_unavailable: motion/);
  assert.deepEqual(h.sent(), []);
  assert.deepEqual(h.legacy, []);
});

test('the overlay re-reads on a revision change and a frame change by itself, and stops when the tool is left', async () => {
  const h = await started({ mode: 'move' });
  const base = h.reads.length;
  h.fx.state.revision += 1;
  await h.settle();
  assert.equal(h.reads.length, base + 1);
  assert.equal(h.reads[h.reads.length - 1].pin, h.fx.state.revision);
  h.sandbox.state.currentFrame = 4;
  await h.settle();
  assert.equal(h.reads.length, base + 2);
  assert.equal(h.reads[h.reads.length - 1].selector.frame, 4);
  await h.settle();
  assert.equal(h.reads.length, base + 2, 'an unchanged pin and frame cost no read');
  h.sandbox.state.tool = 'draw';
  h.fx.state.revision += 1;
  await h.settle();
  assert.equal(h.reads.length, base + 2, 'nothing is read while another tool is active');
});

test('a Space-held or panning pointerdown in the Rig tool is not consumed: the pan gesture keeps it, and nothing is sent', async () => {
  for (const flag of ['spaceDown', 'isPanning']) {
    const h = await started({ mode: 'move' });
    h.sandbox.state[flag] = true;
    const e = h.fire('pointerdown', { clientX: 100, clientY: 100 });
    assert.equal(e.stopped, false, flag);
    assert.equal(e.prevented, false, flag);
    h.fire('pointerup', { clientX: 140, clientY: 120 });
    await h.settle();
    assert.deepEqual(h.sent(), [], flag);
    assert.deepEqual(h.toasts, [], flag);
    h.sandbox.state[flag] = false;
    assert.equal(h.fire('pointerdown', { clientX: 100, clientY: 100 }).stopped, true, flag + ' released: the tool takes the gesture again');
  }
});

test('the rig overlay follows a pan or zoom of the view by itself, with no revision or frame change', async () => {
  const h = await started({ mode: 'move' });
  const svg = h.body.children[0], firstPath = () => svg.children.find((n) => n.attrs && n.attrs.d);
  const before = firstPath().attrs.d, base = h.reads.length;
  h.view.dx = 30; h.view.dy = -10;
  await h.settle();
  const after = firstPath().attrs.d;
  assert.notEqual(after, before);
  assert.match(after, /^M 130 90 /, 'the first anchor (100, 100) is drawn at the panned screen position');
  assert.equal(h.reads.length, base, 'a view change repaints without a native read');
});
