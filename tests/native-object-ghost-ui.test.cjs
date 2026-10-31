'use strict';
// #2358 GHOST-UNDER: onion skin and Ghost All are composed UNDER the live frame by the native renderer. The real native-object-ghost-ui.js runs
// in one vm context against a session double; the tests observe the exact selection a preview request carries, that a changed selection
// re-presents the current frame through the session (so the native composition is requested), that no JS overlay is ever drawn, that a
// renderer refusal is said once, and that no legacy writer is reached.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');

const FILE = path.join(__dirname, '..', 'src', 'js', 'adapters', 'native-object-ghost-ui.js');
const UID = 'native-layer:1:0';
const sel = (h) => JSON.parse(JSON.stringify(h.ui().selection(h.info())));

function harness(options) {
  options = options || {};
  const legacy = [], toasts = [], renders = [], store = {}, created = [], timers = [], clock = { t: 1000 };
  const sandbox = { console, setTimeout: (fn, ms) => { timers.push({ fn, ms }); return timers.length; }, clearTimeout() {}, performance: { now: () => clock.t } };
  sandbox.window = sandbox; sandbox.globalThis = sandbox;
  sandbox.state = { currentFrame: 4, totalFrames: 10, activeLayerIdx: 0, playing: false, onionSkin: false, ghostAllFrames: false, onionIn: 1, onionOut: 8,
    onionPrevOpacity: 30, onionNextOpacity: 40, onionMode: 'tinted', layers: [] };
  sandbox.showToast = (message) => toasts.push(message);
  sandbox.localStorage = { getItem: (k) => (Object.prototype.hasOwnProperty.call(store, k) ? store[k] : null) };
  for (const name of ['SMMotion', 'pushUndo', 'pushUndoLayers', 'saveAllLayerFrames', 'saveActiveLayerFrame', 'createUserLayer', 'loadFrame', 'activateUL', 'renderOS']) {
    Object.defineProperty(sandbox, name, { configurable: true, get() { legacy.push(name); throw new Error('legacy writer reached: ' + name); } });
  }
  const node = () => { const n = { style: {}, children: [], setAttribute() {}, appendChild(c) { n.children.push(c); return c; } }; created.push(n); return n; };
  sandbox.document = { body: node(), documentElement: node(), createElementNS: () => node(), createElement: () => node() };
  created.length = 0;
  sandbox.addEventListener = () => {};
  sandbox.requestAnimationFrame = () => { throw new Error('the onion adapter never follows animation frames'); };
  let presentation = null;
  const info = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 'native-object:d:7', totalFrames: 10,
    layers: [{ layerUid: UID }, { layerUid: 'native-layer:2:0' }] });
  const session = { blocksLegacy: () => true, isActive: () => true, info,
    readPresentedObjectSession: () => ({ available: true, presentation }),
    renderPreview(frame) { renders.push({ frame, selection: JSON.parse(JSON.stringify(sandbox.NemoNativeObjectGhostUI.selection(sandbox, info()))) }); return options.reject ? Promise.reject(new Error('busy')) : Promise.resolve(); } };
  if (!options.noSession) sandbox.NemoNativeObjectProject = { desktop: () => session };
  vm.createContext(sandbox);
  vm.runInContext(fs.readFileSync(FILE, 'utf8'), sandbox, { filename: FILE });
  const global = vm.runInContext('globalThis', sandbox);
  const ui = () => sandbox.NemoNativeObjectGhostUI.desktop(global);
  const turn = async () => { ui().check(); await new Promise((resolve) => setImmediate(resolve)); };
  return { sandbox, session, timers, clock, legacy, toasts, renders, store, created, ui, turn, present: (p) => { presentation = p; }, info };
}

test('nothing is selected, requested or drawn while onion skin and Ghost All are off or the animation plays', async () => {
  const h = harness();
  assert.equal(sel(h), null);
  await h.turn();
  assert.deepEqual(h.renders, []);
  h.sandbox.state.onionSkin = true; h.sandbox.state.playing = true;
  assert.equal(sel(h), null, 'the ghosts do not follow playback');
  assert.deepEqual(h.legacy, []);
});

test('the selection carries the baseline settings and does not depend on the frame', () => {
  const h = harness();
  h.sandbox.state.onionSkin = true; h.sandbox.state.ghostAllFrames = true;
  const base = { layerUid: UID, onion: true, ghostAll: true, from: 1, to: 8, previousOpacity: 30, nextOpacity: 40, mode: 'tinted' };
  assert.deepEqual(sel(h), base);
  h.sandbox.state.currentFrame = 9;
  assert.deepEqual(sel(h), base, 'a frame change alone never changes what is asked for');
  h.sandbox.state.activeLayerIdx = 1;
  assert.equal(sel(h).layerUid, 'native-layer:2:0');
  Object.assign(h.sandbox.state, { activeLayerIdx: 0, onionIn: -3, onionOut: 99.7, onionPrevOpacity: 180, onionNextOpacity: -4, onionMode: 'bogus', onionSkin: false });
  Object.assign(h.store, { 'nemo-onion-tint-prev': '#aabbcc', 'nemo-onion-tint-next': 'red', 'nemo-onion-shift': JSON.stringify({ x: 4, y: -3, next: true }) });
  assert.deepEqual(sel(h), { layerUid: UID, onion: false, ghostAll: true, from: 0, to: 99, previousOpacity: 100, nextOpacity: 0, mode: 'tinted',
    previousTint: '#aabbcc', shift: { x: 4, y: -3, next: true } });
});

test('turning onion skin on or changing a setting asks the session to present the current frame again; an unchanged selection costs nothing', async () => {
  const h = harness();
  h.ui().install();
  assert.deepEqual(h.timers.map((t) => t.ms), [150], 'one slow poll, no animation frames');
  await h.turn();
  assert.deepEqual(h.renders, []);
  h.sandbox.state.onionSkin = true;
  await h.turn(); await h.turn();
  assert.equal(h.renders.length, 1);
  assert.equal(h.renders[0].frame, 4);
  assert.equal(h.renders[0].selection.onion, true, 'the request that the session builds carries the new selection');
  h.sandbox.state.currentFrame = 5;
  await h.turn();
  assert.equal(h.renders.length, 1, 'a moved frame is re-presented by the normal frame flow, not by this adapter');
  h.sandbox.state.onionMode = 'outline';
  await h.turn();
  assert.equal(h.renders.length, 2);
  h.sandbox.state.onionSkin = false;
  await h.turn();
  assert.equal(h.renders.length, 3, 'turning it off re-presents the frame without ghosts');
  assert.equal(h.renders[2].selection, null);
  assert.deepEqual(h.legacy, []);
});

test('no JS overlay is ever drawn: the adapter creates no DOM node and schedules no animation frame', async () => {
  const h = harness();
  h.sandbox.state.onionSkin = true; h.sandbox.state.ghostAllFrames = true;
  h.ui().install();
  for (let n = 0; n < 3; n++) { await h.turn(); h.timers.splice(0).forEach((t) => t.fn()); }
  assert.deepEqual(h.created, []);
  assert.equal(h.renders.length >= 1, true);
});

test('without a native session nothing is requested', async () => {
  const h = harness({ noSession: true });
  h.sandbox.state.onionSkin = true;
  h.ui().install();
  await h.turn();
  assert.deepEqual(h.renders, []);
  assert.deepEqual(h.legacy, []);
});

test('a refused re-presentation is retried only after a pause, never every poll', async () => {
  const h = harness({ reject: true });
  h.sandbox.state.onionSkin = true;
  await h.turn(); await h.turn();
  assert.equal(h.renders.length, 1);
  h.clock.t += 1600;
  await h.turn();
  assert.equal(h.renders.length, 2);
});

test('a renderer refusal is said once per layer and reason; stale, empty and composited answers are silent', async () => {
  const h = harness();
  const sel = { layerUid: UID, onion: true, ghostAll: false, from: 1, to: 8, previousOpacity: 30, nextOpacity: 40, mode: 'tinted' };
  h.sandbox.state.onionSkin = true;
  for (const ghosts of [{ selected: sel, status: 'composited', frames: 2 }, { selected: sel, status: 'empty', frames: 0 },
    { selected: sel, status: 'unavailable', frames: 0, reason: 'stale_revision' }]) {
    h.present({ ghosts }); await h.turn();
  }
  assert.deepEqual(h.toasts, []);
  const refused = { selected: sel, status: 'unavailable', frames: 0, reason: 'ghost_view_unavailable' };
  h.present({ ghosts: refused }); await h.turn(); await h.turn();
  assert.deepEqual(h.toasts, ['Onion skin is unavailable for this layer (ghost_view_unavailable).']);
  h.present({ ghosts: { ...refused, reason: 'ghost_budget' } }); await h.turn();
  assert.equal(h.toasts.length, 2);
});
