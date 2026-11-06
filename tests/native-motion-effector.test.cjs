'use strict';
// #2520/#2525: the effector handles of a duplicator layer on the Motion canvas of a native object project. The real native-motion-canvas.js, its maths,
// native-motion-effector.js and native-object-widget-ui.js run in one vm context against session / component / duplicator doubles that record every
// read and write. A handle is a crosshair, a circle and a falloff ring; a drag is ONE object.duplicator.config effectorPos edit on release.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');

const ROOT = path.join(__dirname, '..', 'src', 'js', 'adapters');
const SOURCES = ['native-object-widget-ui.js', 'native-motion-canvas-math.js', 'native-motion-effector.js', 'native-motion-canvas.js'];
const CURVE = path.join(__dirname, '..', 'src', 'js', 'animation', 'curve.js');
const LAYER = 'native-layer:1:0', PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
const BASE = { bounds: [100, 100, 300, 200], position: [0, 0], anchor: [0, 0], rotation: [0], scale: [100, 100] };

function harness(options) {
  const o = options || {}, values = Object.assign({}, BASE, o.values), keys = o.keys || [], pan = { dx: o.dx || 0, dy: o.dy || 0 };
  const sandbox = { console, setTimeout, clearTimeout, setInterval: () => 0 };
  sandbox.window = sandbox; sandbox.globalThis = sandbox;
  sandbox.state = { appMode: o.appMode || 'motion', tool: o.tool || 'select', currentFrame: o.frame === undefined ? 0 : o.frame, activeLayerIdx: 0, playing: false, activeSymbolId: null };
  const toasts = [], reads = [], edits = [], listeners = [], frames = [];
  sandbox.showToast = (m) => toasts.push(m);
  let revision = 4, busyQueue = false, shown = null, shownKey = '';
  const layer = Object.assign({ layerUid: LAYER, visible: true, locked: false, parentLayerUid: null, hasDuplicator: true }, o.layer);
  const effectors = (o.effectors || []).map((e) => JSON.parse(JSON.stringify(e))), comp = [], configs = [];
  const info = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 'native-object:d:' + revision, canvasW: 960, canvasH: 540, layers: [layer] });
  const session = {
    blocksLegacy: () => true, isActive: () => true, info, readSubscriptionGeneration: () => 1,
    readPresentedObjectSession(why) {
      if (busyQueue) { if (why) why('queue-pending'); return null; }
      const key = revision + ':' + sandbox.state.currentFrame;
      if (key !== shownKey) { shownKey = key; shown = Object.assign(info(), { kind: 'native-object', available: true, presentation: Object.assign(info(), { owner: 'native', status: 'presented', contextId: 'scene-root', frame: sandbox.state.currentFrame }) }); }
      return shown;
    },
    async readProperty(selector) {
      reads.push(selector.view);
      const v = selector.view;
      if (v.kind === 'layerBounds') {
        if (o.boundsError) { const e = new Error('unavailable'); e.code = o.boundsError; throw e; }
        return { kind: 'layerBounds', bounds: values.bounds, parentLayerUid: layer.parentLayerUid, parentAffine: values.parentAffine || [1, 0, 0, 1, 0, 0], frame: v.frame };
      }
      if (v.kind === 'rawSample') return { kind: 'rawSample', value: values[v.property], frame: v.frame };
      return { kind: 'authoredKeys', items: keys, nextOffset: null, trackPresent: keys.length > 0 };
    },
    async editProperty(payload, pin, current) {
      edits.push({ payload, pin });
      if (o.reject) { const e = new Error('Native property edit refused'); e.code = 'stale_revision'; throw e; }
      if (current() !== true) throw new Error('stale');
      revision += 1;
      return { ok: true, result: { applied: true, historyEntriesAdded: 1 } };
    },
  };
  sandbox.NemoNativeObjectProject = { desktop: () => session };
  sandbox.NemoNativeObjectComponent = { desktop: () => ({ read: async (sel) => { comp.push(sel); return o.componentFail ? { ok: false, code: 'unavailable' } : { ok: true, result: { kind: 'duplicator', layerUid: sel.layerUid,
    atRevision: o.staleRead ? revision - 1 : revision, documentSnapshotId: 'native-object:d:' + revision, effectors: JSON.parse(JSON.stringify(effectors)) } }; } }) };
  sandbox.NemoNativeObjectDuplicator = { desktop: () => ({ configure: async (uid, patch) => { configs.push({ uid, patch });
    if (o.configReject) return { ok: false, code: 'stale_selection', message: 'The layer is no longer in the project.' };
    effectors[patch.effectorPos.index].pos = { x: patch.effectorPos.x, y: patch.effectorPos.y }; revision += 1; return { ok: true, result: { applied: true, historyEntriesAdded: 1 } }; } }) };
  sandbox.nativeCompositingRefresh = () => { sandbox.refreshes = (sandbox.refreshes || 0) + 1; };
  sandbox.NemoNativeViewportUnderlay = { canvasMapping: Object.freeze({ bounds: () => ({ left: 0, top: 0, width: 960, height: 540 }), matchesView: () => true,
    point: (e) => ({ x: e.clientX - pan.dx, y: e.clientY - pan.dy }), screen: (p) => ({ x: p.x + pan.dx, y: p.y + pan.dy }) }) };
  const area = { contains: (t) => t === sandbox.canvas };
  sandbox.canvas = { tag: 'canvas' };
  const make = (name) => { const n = { name, style: {}, attrs: {}, children: [], parentNode: null, setAttribute(k, v) { this.attrs[k] = v; }, appendChild(c) { c.parentNode = this; this.children.push(c); this.firstChild = this.children[0]; return c; },
    removeChild(c) { this.children.splice(this.children.indexOf(c), 1); this.firstChild = this.children[0]; c.parentNode = null; } }; return n; };
  sandbox.document = { body: make('body'), documentElement: make('html'), addEventListener(type, fn, opts) { listeners.push({ type, fn, opts }); },
    getElementById: (id) => (id === 'canvas-area' ? area : null), createElementNS: (ns, name) => make(name) };
  sandbox.requestAnimationFrame = (fn) => { frames.push(fn); return frames.length; };
  vm.createContext(sandbox);
  vm.runInContext(fs.readFileSync(CURVE, 'utf8'), sandbox, { filename: 'curve.js' });
  for (const file of SOURCES) vm.runInContext(fs.readFileSync(path.join(ROOT, file), 'utf8'), sandbox, { filename: file });
  const ctl = sandbox.NemoNativeMotionCanvas.desktop(vm.runInContext('globalThis', sandbox));
  const flush = () => { for (let n = 0; n < 3 && frames.length; n++) frames.splice(0).forEach((fn) => fn()); };
  const fire = (type, event) => {
    const e = Object.assign({ target: sandbox.canvas, button: 0, pointerId: 1, stopped: false, prevented: false, stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; } }, event);
    for (const l of listeners.filter((x) => x.type === type)) { l.fn(e); if (e.stopped) break; }
    return e;
  };
  const at = (x, y, extra) => Object.assign({ clientX: x + pan.dx, clientY: y + pan.dy }, extra);
  const overlay = () => sandbox.document.body.children.find((c) => c.attrs['data-native-motion-canvas'] !== undefined);
  return { configs, comp, effectors, sandbox, ctl, flush, fire, at, toasts, reads, edits, listeners, overlay, bump: () => { revision += 1; }, busy: (v) => { busyQueue = v; }, layer,
    ready: async () => { await ctl.refresh(); flush(); },
    drag: async (from, to, extra, up) => { const d = fire('pointerdown', at(from[0], from[1], extra)); fire('pointermove', at(to[0], to[1], extra)); const u = up === false ? null : fire('pointerup', at(to[0], to[1], extra)); await new Promise((r) => setTimeout(r, 0)); return { d, u }; } };
}

const plain = (v) => JSON.parse(JSON.stringify(v));
const TWO = [{ pos: { x: 200, y: 150 }, radius: 40 }, { pos: null, radius: 10 }, { pos: { x: 400, y: 300 }, radius: 25 }];
const glyphs = (h) => h.overlay().children.filter((c) => c.attrs.stroke === 'rgba(255,120,220,1)' || c.attrs.stroke === 'rgba(255,120,220,.35)');

test('a duplicator layer draws a crosshair, a circle and a falloff ring per effector that has an anchor, in the baseline colours', async () => {
  const h = harness({ effectors: TWO });
  await h.ready();
  assert.deepEqual(plain(h.comp), [{ kind: 'duplicator', layerUid: LAYER }]);
  const g = glyphs(h);
  assert.equal(g.length, 8, 'two anchored effectors, four glyphs each; the unanchored one has none');
  const rings = g.filter((c) => c.attrs.stroke === 'rgba(255,120,220,.35)');
  assert.deepEqual(plain(rings.map((c) => [c.attrs.cx, c.attrs.cy, c.attrs.r])), [[200, 150, 40], [400, 300, 25]]);
  const dots = g.filter((c) => c.name === 'circle' && c.attrs.r === 5);
  assert.deepEqual(plain(dots.map((c) => [c.attrs.cx, c.attrs.cy])), [[200, 150], [400, 300]]);
});

test('dragging a handle is ONE effectorPos config edit with the stored anchor, and nothing before the release', async () => {
  const h = harness({ effectors: TWO });
  await h.ready();
  const d = h.fire('pointerdown', h.at(200, 150));
  assert.equal(d.stopped, true, 'the press is taken from the canvas');
  h.fire('pointermove', h.at(260, 190));
  h.flush();
  assert.equal(h.configs.length, 0, 'a preview only');
  const ring = h.overlay().children.find((c) => c.attrs.stroke === 'rgba(255,120,220,.35)');
  assert.deepEqual(plain([ring.attrs.cx, ring.attrs.cy]), [260, 190], 'the handle follows the pointer');
  h.fire('pointerup', h.at(260, 190));
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(plain(h.configs), [{ uid: LAYER, patch: { effectorPos: { index: 0, x: 260, y: 190 } } }]);
  assert.equal(h.sandbox.refreshes, 1, 'the compositing refresh the panel also asks for');
  await h.ready();
  assert.deepEqual(plain(glyphs(h).filter((c) => c.attrs.r === 5).map((c) => [c.attrs.cx, c.attrs.cy])), [[260, 190], [400, 300]], 'the new revision reads back the stored anchor');
});

test('the topmost handle wins an overlap, and the box gestures win over a handle', async () => {
  const stacked = harness({ effectors: [{ pos: { x: 500, y: 400 }, radius: 5 }, { pos: { x: 502, y: 401 }, radius: 5 }] });
  await stacked.ready();
  stacked.fire('pointerdown', stacked.at(501, 400)); stacked.fire('pointermove', stacked.at(540, 440)); stacked.fire('pointerup', stacked.at(540, 440));
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(stacked.configs[0].patch.effectorPos.index, 1, 'the last-added effector is picked first');
  const onCorner = harness({ effectors: [{ pos: { x: 300, y: 200 }, radius: 5 }] }); // exactly on the box corner: the scale handle comes first
  await onCorner.ready();
  await onCorner.drag([300, 200], [400, 250]);
  assert.equal(onCorner.configs.length, 0);
  assert.equal(onCorner.edits.length, 1);
  assert.equal(onCorner.edits[0].payload.property, 'scale');
});

test('the handle is grabbed on the layer the duplicator locks, and a click, Escape and a frame change store nothing', async () => {
  const locked = harness({ effectors: TWO, layer: { locked: true } });
  await locked.ready();
  await locked.drag([200, 150], [230, 150]);
  assert.equal(locked.configs.length, 1, 'a duplicator layer is locked by the duplicator itself');
  assert.equal(locked.edits.length, 0, 'but its box gestures stay refused');
  await locked.drag([100, 150], [150, 150]);
  assert.equal(locked.edits.length, 0, 'a locked layer takes no box gesture');
  const click = harness({ effectors: TWO }); await click.ready();
  const r = await click.drag([200, 150], [201, 151]);
  assert.equal(click.configs.length, 0); assert.equal(r.d.stopped, true);
  const esc = harness({ effectors: TWO }); await esc.ready();
  esc.fire('pointerdown', esc.at(200, 150)); esc.fire('pointermove', esc.at(300, 150));
  const k = esc.fire('keydown', { key: 'Escape' });
  esc.fire('pointerup', esc.at(300, 150)); await new Promise((r2) => setTimeout(r2, 0));
  assert.equal(k.stopped, true); assert.equal(esc.configs.length, 0);
  const moved = harness({ effectors: TWO }); await moved.ready();
  moved.fire('pointerdown', moved.at(200, 150)); moved.fire('pointermove', moved.at(300, 150));
  moved.sandbox.state.currentFrame = 3;
  moved.fire('pointerup', moved.at(300, 150)); await new Promise((r2) => setTimeout(r2, 0));
  assert.equal(moved.configs.length, 0, 'a frame change in the middle of the gesture commits nothing');
});

test('a stale read, a refusal and a replaced revision store nothing and say why', async () => {
  const stale = harness({ effectors: TWO, staleRead: true });
  await stale.ready();
  assert.equal(glyphs(stale).length, 0, 'a descriptor not pinned to the shown revision draws no handle');
  const refused = harness({ effectors: TWO, configReject: true }); await refused.ready();
  await refused.drag([200, 150], [260, 150]);
  assert.match(refused.toasts[0], /no longer in the project\. \(stale_selection\)/);
  const bumped = harness({ effectors: TWO }); await bumped.ready();
  bumped.fire('pointerdown', bumped.at(200, 150)); bumped.fire('pointermove', bumped.at(260, 150));
  bumped.bump();
  bumped.fire('pointerup', bumped.at(260, 150)); await new Promise((r) => setTimeout(r, 0));
  assert.equal(bumped.configs.length, 0, 'the document moved under the gesture');
  const failed = harness({ effectors: TWO, componentFail: true }); await failed.ready();
  assert.equal(glyphs(failed).length, 0);
});

test('a parented duplicator layer places and drags the handle through the ancestors map', async () => {
  const h = harness({ effectors: [{ pos: { x: 10, y: 20 }, radius: 30 }], values: { parentAffine: [2, 0, 0, 2, 100, 50] }, layer: { parentLayerUid: 'native-layer:0:0' } });
  await h.ready();
  const dot = glyphs(h).find((c) => c.name === 'circle' && c.attrs.r === 5);
  assert.deepEqual(plain([dot.attrs.cx, dot.attrs.cy]), [120, 90], 'x = 2 * 10 + 100, y = 2 * 20 + 50');
  const ring = glyphs(h).find((c) => c.attrs.stroke === 'rgba(255,120,220,.35)');
  assert.equal(ring.attrs.r, 30, 'the falloff ring is a document distance, not run through the ancestors');
  await h.drag([120, 90], [220, 190]);
  assert.deepEqual(plain(h.configs[0].patch.effectorPos), { index: 0, x: 60, y: 70 }, 'the pointer comes back through the inverse of the ancestors map');
});

test('a duplicator host with no geometry still offers its handles; one with a parent does not guess a map', async () => {
  const bare = harness({ effectors: TWO, boundsError: 'unavailable' });
  await bare.ready();
  assert.equal(glyphs(bare).length, 8);
  assert.equal(bare.overlay().children.filter((c) => c.name === 'rect').length, 0, 'no box without bounds');
  await bare.drag([200, 150], [250, 150]);
  assert.deepEqual(plain(bare.configs[0].patch.effectorPos), { index: 0, x: 250, y: 150 });
  const parented = harness({ effectors: TWO, boundsError: 'unavailable', layer: { parentLayerUid: 'native-layer:0:0' } });
  await parented.ready();
  assert.equal(glyphs(parented).length, 0);
  assert.equal(parented.toasts.length, 1, 'the unboxed notice');
  const plainLayer = harness({ boundsError: 'unavailable', layer: { hasDuplicator: false } });
  await plainLayer.ready();
  assert.equal(plainLayer.comp.length, 0, 'a layer without a duplicator reads no descriptor');
});
