'use strict';
// #2255: the ordinary Rig panel and Rig tool on a native object project. The real native-object-rig-ui.js and rig-bridge.js run in one
// vm context against a session double with the real command-route semantics: the panel buttons are exact native commands, the canvas
// gestures belong to native-object-rig-canvas.js (its own test), and a legacy (non-native) project is left to the old handlers.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { createFixture } = require('./native-object-rig-support.cjs');

const ROOT = path.join(__dirname, '..', 'src', 'js');
const OP = 'command.document.object.rig.';
const UID = 'native-layer:1:0';
const LEGACY = ['rigResetPose', 'rigCommitFrame', 'rigSetIK', 'rigBindStroke', 'rigBindMesh', 'rigAutoAssignLayer', 'ensureLayerRig', 'rigFreshId', 'rigWeighOnePoint',
  'pushUndo', 'SMMotion', 'createUserLayer', 'saveAllLayerFrames'];

function harness(options) {
  options = options || {};
  const fx = createFixture({});
  const listeners = [], area = { listeners: [], addEventListener(type, fn, opts) { this.listeners.push({ type, fn, opts }); }, contains: (t) => t === canvas }, legacy = [], toasts = [];
  const canvas = { tag: 'canvas' }, fields = {};
  const sandbox = { console, setTimeout, clearTimeout };
  sandbox.SM = { t: (key) => key };
  sandbox.state = { currentFrame: 3, activeLayerIdx: 0, playing: false, tool: 'rig', rigSubMode: 'draw', layers: [] };
  sandbox.showToast = (message) => toasts.push(message);
  for (const name of LEGACY) Object.defineProperty(sandbox, name, { configurable: true, get() { legacy.push(name); throw new Error('legacy writer reached: ' + name); } });
  sandbox.document = { readyState: 'complete', addEventListener(type, fn, opts) { listeners.push({ type, fn, opts }); },
    getElementById(id) { return id === 'canvas-area' ? area : fields[id] || null; } };
  const live = fx.session.info;
  fx.session.info = () => Object.assign(live(), { layers: [{ layerUid: UID }] });
  sandbox.NemoNativeObjectProject = { desktop: () => (options.legacy ? null : fx.session) };
  vm.createContext(sandbox);
  vm.runInContext('globalThis.window = globalThis', sandbox);
  const intoContext = vm.runInContext('(text) => JSON.parse(text)', sandbox), route = fx.session.runResourceCommand;
  fx.session.runResourceCommand = (action) => route((ctx) => action({ dispatch: async (request) => intoContext(JSON.stringify(await ctx.dispatch(request))), reconcile: ctx.reconcile }));
  for (const file of ['adapters/native-object-rig-contract.js', 'adapters/native-object-rig-command.js', 'adapters/native-object-rig.js', 'adapters/native-object-rig-ui.js', 'rig-bridge.js'])
    vm.runInContext(fs.readFileSync(path.join(ROOT, file), 'utf8'), sandbox, { filename: file });
  // Capture order of a real browser: document listeners (capture) before the listeners of #canvas-area.
  const fire = (type, event) => {
    const e = Object.assign({ target: canvas, button: 0, stopped: false, stopImmediatePropagation() { this.stopped = true; }, preventDefault() {} }, event);
    for (const l of [...listeners.filter((x) => x.type === type), ...area.listeners.filter((x) => x.type === type)]) { l.fn(e); if (e.stopped) break; }
    return e;
  };
  return { fx, sandbox, listeners, area, fields, legacy, toasts, fire,
    sent: () => fx.owner.log.received.map((r) => [r.operation, r.payload]),
    ui: () => sandbox.NemoNativeObjectRigUI,
    win: () => vm.runInContext('globalThis', sandbox) };
}
const turn = () => new Promise((resolve) => setImmediate(resolve));

test('Auto-assign, Commit and Reset are one native command each on the active layer at the playhead', async () => {
  const h = harness();
  const el = (id, props) => { h.fields[id] = Object.assign({}, props); };
  el('rig-weight-radius', { value: '120' }); el('rig-rotate-mode', { checked: true }); el('rig-falloff-softness', { value: '25' });
  el('rig-drive-shapes', { checked: true }); el('rig-drive-meshes', { checked: false });
  assert.equal(h.ui().panel(h.win(), 'autoAssign'), true);
  await turn();
  assert.equal(h.ui().panel(h.win(), 'commit'), true);
  await turn();
  assert.equal(h.ui().panel(h.win(), 'reset'), true);
  await turn();
  assert.deepEqual(h.sent(), [
    [OP + 'edit', { command: 'object.rig.edit', layerUid: UID, action: { kind: 'autoAssign', frame: 3, defaultRadius: 120, rotate: true, softness: 0.25, paths: true, meshes: false } }],
    [OP + 'pose', { command: 'object.rig.pose', layerUid: UID, action: { kind: 'commit', frame: 3 } }],
    [OP + 'pose', { command: 'object.rig.pose', layerUid: UID, action: { kind: 'reset' } }],
  ]);
  assert.deepEqual(h.toasts, ['toastRigPoseReset']);
  assert.deepEqual(h.legacy, []);
});

test('an absent panel field is the baseline default and a refusal is a typed message', async () => {
  const h = harness();
  h.ui().panel(h.win(), 'autoAssign');
  await turn();
  assert.deepEqual(h.sent()[0][1].action, { kind: 'autoAssign', frame: 3, defaultRadius: 200, rotate: false, softness: 0, paths: true, meshes: true });
  const refusing = harness();
  refusing.fx.session.info = () => ({ instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 3, contentRevision: 7, documentSnapshotId: 's', layers: [] });
  assert.equal(refusing.ui().panel(refusing.win(), 'reset'), true);
  assert.match(refusing.toasts[0], /unavailable/i);
  assert.deepEqual(refusing.sent(), []);
});

test('a legacy project keeps the old panel handlers: the native route answers false and sends nothing', async () => {
  const h = harness({ legacy: true });
  for (const action of ['autoAssign', 'commit', 'reset']) assert.equal(h.ui().panel(h.win(), action), false, action);
  assert.deepEqual(h.sent(), []);
  assert.equal(h.fire('pointerdown', {}).stopped, false, 'the Rig tool is untouched on a legacy project');
});

test('the panel owns no canvas listener (the canvas gestures are native-object-rig-canvas.js) and the pose / IK drag stays a typed unavailable at the adapter', async () => {
  const h = harness();
  assert.deepEqual(h.listeners.map((l) => l.type), [], 'native-object-rig-ui.js listens to nothing on document');
  assert.equal(h.area.listeners.some((l) => l.type === 'pointerdown' && l.opts && l.opts.capture), true, 'rig-bridge.js is loaded and listening on #canvas-area');
  assert.equal(h.fire('pointerdown', {}).stopped, false, 'with no canvas module loaded nothing takes the gesture here');
  assert.deepEqual(h.sent(), []);
  assert.deepEqual(h.legacy, []);
  const rig = h.sandbox.NemoNativeObjectRig.desktop(h.win());
  for (const name of ['dragPose', 'dragIkEnd']) {
    const error = await rig[name]().catch((e) => e);
    assert.deepEqual([error.code, error.reason], ['unavailable', 'rig_pose_drag_unavailable'], name);
  }
});
