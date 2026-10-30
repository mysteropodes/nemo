'use strict';
// #2358 D23: onion skin and Ghost All on a native object project. The real native-object-ghost-ui.js runs in one vm context against a
// session double and a recording SVG DOM; the tests observe the exact read selector, what is painted, that a result that lands after the
// frame, the revision or the settings moved is discarded, and that no legacy writer is ever reached.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');

const FILE = path.join(__dirname, '..', 'src', 'js', 'adapters', 'native-object-ghost-ui.js');
const UID = 'native-layer:1:0';

function harness(options) {
  options = options || {};
  const legacy = [], toasts = [], reads = [], store = {}, frames = [], held = [];
  const timers = [], clock = { t: 1000 };
  const sandbox = { console, setTimeout: (fn, ms) => { timers.push({ fn, ms }); return timers.length; }, clearTimeout() {}, performance: { now: () => clock.t } };
  sandbox.window = sandbox; sandbox.globalThis = sandbox;
  sandbox.state = { currentFrame: 4, totalFrames: 10, activeLayerIdx: 0, playing: false, onionSkin: false, ghostAllFrames: false, onionIn: 1, onionOut: 8,
    onionPrevOpacity: 30, onionNextOpacity: 40, onionMode: 'tinted', layers: [] };
  sandbox.showToast = (message) => toasts.push(message);
  sandbox.localStorage = { getItem: (k) => (Object.prototype.hasOwnProperty.call(store, k) ? store[k] : null) };
  for (const name of ['SMMotion', 'pushUndo', 'pushUndoLayers', 'saveAllLayerFrames', 'saveActiveLayerFrame', 'createUserLayer', 'loadFrame', 'activateUL', 'renderOS']) {
    Object.defineProperty(sandbox, name, { configurable: true, get() { legacy.push(name); throw new Error('legacy writer reached: ' + name); } });
  }
  const created = [];
  const mkNode = (name) => ({ name, attrs: {}, style: {}, children: [], isConnected: true, setAttribute(k, v) { this.attrs[k] = v; },
    appendChild(c) { this.children.push(c); this.firstChild = this.children[0]; return c; },
    removeChild(c) { this.children.splice(this.children.indexOf(c), 1); this.firstChild = this.children[0] || null; } });
  sandbox.document = { body: mkNode('body'), documentElement: mkNode('html'), createElementNS: (ns, name) => { const n = mkNode(name); created.push(n); return n; } };
  sandbox.addEventListener = () => {};
  sandbox.requestAnimationFrame = (fn) => { frames.push(fn); return frames.length; };
  const flush = () => { for (let n = 0; n < 5 && frames.length; n++) frames.splice(0).forEach((fn) => fn()); };
  let revision = 7, ticket = 1, shown = null, view = { mode: 'canvas', navigation: { zoom: 1, pan: [0, 0] } };
  const info = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 'native-object:d:' + revision,
    canvasW: 960, canvasH: 540, totalFrames: 10, layers: [{ layerUid: UID }, { layerUid: 'native-layer:2:0' }] });
  const presented = () => {
    if (!shown || shown.revision !== revision || shown.frame !== sandbox.state.currentFrame) {
      const p = Object.assign(info(), { owner: 'native', status: 'presented', contextId: 'scene-root', frame: sandbox.state.currentFrame, workId: 'native-work-1', viewGeneration: 1, view });
      shown = { revision, frame: sandbox.state.currentFrame, value: Object.assign(info(), { available: true, presentation: p }) };
    }
    return shown.value;
  };
  const session = { blocksLegacy: () => true, isActive: () => true, info, readSubscriptionGeneration: () => ticket, readPresentedObjectSession: presented,
    async readGhosts(selector, pin) {
      reads.push({ selector: JSON.parse(JSON.stringify(selector)), revision: pin.contentRevision });
      if (options.hold) await new Promise((resolve) => held.push(resolve));
      if (options.fail) throw Object.assign(new Error('refused'), { code: options.fail, reason: options.fail === 'stale_revision' ? 'ghost_view_moved' : 'ghost_geometry' });
      return Object.freeze({ info: pin, presentation: presented().presentation, layerUid: selector.layerUid, frames: options.frames || [
        { frame: 3, kind: 'previous', opacity: 0.3, translation: [4, -3], paths: [{ sourceId: 'a', closed: true, width: 2, fill: '#33669980', stroke: '#aabbcc4d', opacity: 0.3, count: 3,
          segments: [[0, 0], [100, 0, 0, 0, 10, 5], [100, 100]] }] },
        { frame: 0, kind: 'allKeys', opacity: 0.34, translation: [0, 0], paths: [{ sourceId: 'b', closed: false, width: 1, fill: null, stroke: '#adc0ffff', opacity: 0.4, count: 2,
          segments: [[0, 0], [10, 10]] }] }] });
    } };
  if (!options.noSession) sandbox.NemoNativeObjectProject = { desktop: () => session };
  sandbox.NemoNativeViewportUnderlay = { canvasMapping: Object.freeze({ bounds: () => ({ left: 10, top: 20, width: 960, height: 540 }), matchesView: () => true,
    screen: (p) => ({ x: p.x + 10, y: p.y + 20 }) }) };
  vm.createContext(sandbox);
  vm.runInContext(fs.readFileSync(FILE, 'utf8'), sandbox, { filename: FILE });
  const global = vm.runInContext('globalThis', sandbox);
  const ui = () => sandbox.NemoNativeObjectGhostUI.desktop(global);
  const svg = () => created.find((n) => n.name === 'svg');
  const paths = () => { const s = svg(); const g = s && s.children.find((c) => c.name === 'g'); return g ? g.children : []; };
  return { sandbox, session, timers, clock, frames, legacy, toasts, reads, store, flush, held, ui, svg, paths, store,
    bump: () => { revision += 1; }, ticket: () => { ticket += 1; }, setView: (v) => { view = v; shown = null; } };
}
const settle = () => new Promise((resolve) => setImmediate(resolve));

test('with onion skin and Ghost All off nothing is read or painted, and no legacy writer is reached', async () => {
  const h = harness();
  await h.ui().refresh(); h.flush();
  assert.deepEqual(h.reads, []);
  assert.deepEqual(h.paths(), []);
  assert.deepEqual(h.legacy, []);
});

test('turning onion skin on asks the native read for the baseline range and paints the ghosts as tinted overlay paths', async () => {
  const h = harness();
  h.sandbox.state.onionSkin = true; h.sandbox.state.ghostAllFrames = true;
  await h.ui().refresh(); h.flush();
  assert.equal(h.reads.length, 1);
  assert.deepEqual(h.reads[0].selector, { contextId: 'scene-root', frame: 4, layerUid: UID, onion: true, ghostAll: true, from: 1, to: 8, previousOpacity: 30, nextOpacity: 40, mode: 'tinted' });
  const shown = h.paths();
  assert.equal(shown.length, 2);
  // Ghost 'previous': a closed path through the viewport mapping, the ghost shift of (4, -3), a fill and the tinted stroke.
  assert.equal(shown[0].attrs.d, 'M14 17 C14 17 114 17 114 17 C124 22 114 117 114 117 C114 117 14 17 14 17Z');
  assert.deepEqual([shown[0].attrs.fill, shown[0].attrs.stroke, shown[0].attrs.opacity], ['#33669980', '#aabbcc4d', 0.3]);
  assert.deepEqual([shown[1].attrs.fill, shown[1].attrs.stroke, shown[1].attrs.d], ['none', '#adc0ffff', 'M10 20 C10 20 20 30 20 30']);
  const clip = h.svg().children[0].children[0].children[0];
  assert.deepEqual([clip.attrs.x, clip.attrs.y, clip.attrs.width, clip.attrs.height], [10, 20, 960, 540]);
  assert.equal(h.svg().style.pointerEvents, 'none');
  assert.deepEqual(h.toasts, []);
  assert.deepEqual(h.legacy, []);
});

test('the baseline preferences travel in the selector: range clamped to the playhead, tints and the onion shift', async () => {
  const h = harness();
  Object.assign(h.sandbox.state, { onionSkin: true, onionIn: 6, onionOut: 2, onionMode: 'outline' });
  h.store['nemo-onion-tint-prev'] = '#102030'; h.store['nemo-onion-tint-next'] = '#ff4d4d';
  h.store['nemo-onion-shift'] = JSON.stringify({ x: 5, y: -2, next: true });
  await h.ui().refresh();
  assert.deepEqual(h.reads[0].selector, { contextId: 'scene-root', frame: 4, layerUid: UID, onion: true, ghostAll: false, from: 4, to: 4, previousOpacity: 30, nextOpacity: 40,
    mode: 'outline', previousTint: '#102030', nextTint: '#ff4d4d', shift: { x: 5, y: -2, next: true } });
  // The default prev tint is left to the engine; a malformed stored tint falls back to it.
  const g = harness();
  g.sandbox.state.onionSkin = true; g.store['nemo-onion-tint-prev'] = 'red';
  await g.ui().refresh();
  assert.equal('previousTint' in g.reads[0].selector, false);
});

test('an unchanged frame, revision and settings cost no second read; a changed one asks again', async () => {
  const h = harness();
  h.sandbox.state.onionSkin = true;
  await h.ui().refresh(); await h.ui().refresh(); await h.ui().refresh();
  assert.equal(h.reads.length, 1);
  h.sandbox.state.currentFrame = 5;
  await h.ui().refresh();
  assert.deepEqual(h.reads.map((r) => r.selector.frame), [4, 5]);
  h.bump();
  await h.ui().refresh();
  assert.equal(h.reads.length, 3);
  h.sandbox.state.onionPrevOpacity = 55;
  await h.ui().refresh();
  assert.equal(h.reads.length, 4);
  assert.equal(h.reads[3].selector.previousOpacity, 55);
});

test('a result that lands after the frame moved is discarded and the new frame is asked for', async () => {
  const h = harness({ hold: true });
  h.sandbox.state.onionSkin = true;
  const first = h.ui().refresh();
  await settle();
  assert.equal(h.reads.length, 1);
  h.sandbox.state.currentFrame = 5;                       // the playhead moved while the read was in flight
  const second = h.ui().refresh();
  await settle();
  h.held[0]();                                            // the old answer lands
  await first; h.flush();
  assert.deepEqual(h.paths(), [], 'the stale ghosts are never painted');
  await settle();                                         // the new frame's read runs after the old one
  h.held[1]();
  await second; h.flush();
  assert.deepEqual(h.reads.map((r) => r.selector.frame), [4, 5]);
  assert.equal(h.paths().length, 2, 'only the current frame is shown');
});

test('a revision, subscription or settings change while a read is in flight drops its result', async () => {
  for (const change of [(h) => h.bump(), (h) => h.ticket(), (h) => { h.sandbox.state.onionMode = 'default'; }]) {
    const h = harness({ hold: true });
    h.sandbox.state.onionSkin = true;
    const read = h.ui().refresh();
    await settle();
    change(h);
    h.held[0]();
    await read; h.flush();
    assert.deepEqual(h.paths(), []);
  }
});

test('playing hides the ghosts, turning onion skin off clears them, and a new presentation of the frame is a new pin', async () => {
  const h = harness();
  h.sandbox.state.onionSkin = true;
  await h.ui().refresh(); h.flush();
  assert.equal(h.paths().length, 2);
  h.setView({ mode: 'canvas', navigation: { zoom: 2, pan: [10, 0] } });
  await h.ui().refresh(); h.flush();
  assert.equal(h.reads.length, 2, 'a new presentation of the same frame is a new pin');
  h.sandbox.state.playing = true;
  await h.ui().refresh(); h.flush();
  assert.deepEqual(h.paths(), []);
  h.sandbox.state.playing = false; h.sandbox.state.onionSkin = false;
  await h.ui().refresh(); h.flush();
  assert.deepEqual(h.paths(), []);
  assert.deepEqual(h.legacy, []);
});

test('a refused read paints nothing and is said once per layer and reason; a stale refusal is silent', async () => {
  const h = harness({ fail: 'unavailable' });
  h.sandbox.state.onionSkin = true;
  await h.ui().refresh(); h.bump(); await h.ui().refresh(); h.flush();
  assert.deepEqual(h.paths(), []);
  assert.equal(h.toasts.length, 1);
  assert.match(h.toasts[0], /Onion skin is unavailable/);
  const quiet = harness({ fail: 'stale_revision' });
  quiet.sandbox.state.onionSkin = true;
  await quiet.ui().refresh();
  assert.deepEqual(quiet.toasts, []);
});

test('a refused read is not retried every frame: the failed key is asked again only when it changes (stale after a short pause)', async () => {
  const h = harness({ fail: 'unavailable' });
  h.sandbox.state.onionSkin = true;
  await h.ui().refresh();
  for (let n = 0; n < 5; n++) { await h.ui().refresh(); h.flush(); }
  assert.equal(h.reads.length, 1, 'one failing read per key, not one per animation frame');
  h.bump(); await h.ui().refresh();
  assert.equal(h.reads.length, 2, 'a changed key asks again');
  const stale = harness({ fail: 'stale_revision' });
  stale.sandbox.state.onionSkin = true;
  await stale.ui().refresh(); await stale.ui().refresh();
  assert.equal(stale.reads.length, 1, 'inside the short window a stale refusal is not repeated');
  stale.clock.t += 400; await stale.ui().refresh();
  assert.equal(stale.reads.length, 2, 'after the short window it is asked again');
});

test('a frame change clears the old ghosts at once, before the new read lands', async () => {
  const h = harness({ hold: true });
  h.sandbox.state.onionSkin = true;
  const first = h.ui().refresh(); await settle(); h.held[0](); await first; h.flush();
  assert.equal(h.paths().length, 2);
  h.sandbox.state.currentFrame = 5;
  const next = h.ui().refresh(); h.flush();
  assert.deepEqual(h.paths(), [], 'the previous frame ghosts are gone while the new read is pending');
  await settle(); h.held[1](); await next; h.flush();
  assert.equal(h.paths().length, 2);
});

test('the ghost overlay sits just below the select overlay (z-index 15)', async () => {
  const h = harness();
  h.sandbox.state.onionSkin = true;
  await h.ui().refresh(); h.flush();
  assert.equal(h.svg().style.zIndex, '15');
});

test('without a native session no animation frame is ever scheduled, only the slow timer', async () => {
  const h = harness({ noSession: true });
  h.sandbox.state.onionSkin = true; h.sandbox.state.ghostAllFrames = true;
  h.ui().install();
  for (let n = 0; n < 3; n++) { const t = h.timers.shift(); assert.ok(t, 'a slow timer is armed'); assert.ok(t.ms >= 300); t.fn(); await settle(); }
  assert.equal(h.frames.length, 0, 'no requestAnimationFrame while there is no native-active session');
  h.ui().dispose();
});

test('with a native session the loop follows on animation frames and a refused key goes back to the slow timer', async () => {
  const h = harness({ fail: 'unavailable' });
  h.sandbox.state.onionSkin = true;
  h.ui().install();
  h.timers.shift().fn(); await settle();
  assert.ok(h.frames.length >= 1, 'a live native session is followed per animation frame');
  for (let n = 0; n < 6; n++) { h.flush(); await settle(); }
  assert.equal(h.reads.length, 1);
  assert.ok(h.timers.length >= 1 || h.frames.length === 0, 'the refused key no longer spins on animation frames');
  h.ui().dispose();
});
