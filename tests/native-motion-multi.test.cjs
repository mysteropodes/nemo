'use strict';
// #2520/#2525: the multi-layer Motion box on a native object project. The real native-motion-multi.js, its maths and native-object-widget-ui.js (the
// presentation proof and canvas mapping) run in one vm context against a session double that records every read and write. A gesture over two or more
// selected layers is ONE `object.property.batch`: the baseline multiLayerMove / multiLayerScale / multiLayerRotate (motion.js@1065e9d8) with exact values.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');

const ROOT = path.join(__dirname, '..', 'src', 'js', 'adapters');
const SOURCES = ['native-object-widget-ui.js', 'native-motion-canvas-math.js', 'native-motion-multi.js'];
const CURVE = path.join(__dirname, '..', 'src', 'js', 'animation', 'curve.js');
const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
const BASE = { bounds: [100, 100, 300, 200], position: [0, 0], anchor: [0, 0], rotation: [0], scale: [100, 100] };
const TWO = { B: { bounds: [400, 100, 500, 200] } };

function harness(options) {
  const o = options || {}, values = Object.assign({}, BASE, o.values), keys = o.keys || [], pan = { dx: o.dx || 0, dy: o.dy || 0 };
  const sandbox = { console, setTimeout, clearTimeout, setInterval: () => 0 };
  sandbox.window = sandbox; sandbox.globalThis = sandbox;
  sandbox.state = { appMode: o.appMode || 'motion', tool: o.tool || 'select', currentFrame: o.frame === undefined ? 0 : o.frame, activeLayerIdx: 0, playing: false, activeSymbolId: o.component ? 'comp-a' : null };
  const toasts = [], reads = [], edits = [], listeners = [], frames = [];
  sandbox.showToast = (m) => toasts.push(m);
  let revision = 4, busyQueue = false, shown = null, shownKey = '';
  const layers = (o.layers || [{ layerUid: 'A' }, { layerUid: 'B' }]).map((l) => Object.assign({ visible: true, locked: false, parentLayerUid: null }, l)), layer = layers[0];
  sandbox._layerSel = o.sel || layers.map((_, i) => i);
  const info = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 'native-object:d:' + revision, canvasW: 960, canvasH: 540, layers });
  const session = {
    blocksLegacy: () => true, isActive: () => true, info, readSubscriptionGeneration: () => 1,
    readPresentedObjectSession(why) {
      if (busyQueue) { if (why) why('queue-pending'); return null; }
      const key = revision + ':' + sandbox.state.currentFrame;
      if (key !== shownKey) { shownKey = key; shown = Object.assign(info(), { kind: 'native-object', available: true, presentation: Object.assign(info(), { owner: 'native', status: 'presented', contextId: o.component ? 'comp-a' : 'scene-root', frame: sandbox.state.currentFrame }) }); }
      return shown;
    },
    async readProperty(selector) {
      reads.push(selector.view);
      const v = selector.view, vals = Object.assign({}, values, (o.perLayer || {})[v.target && v.target.layerUid]);
      if (v.kind === 'layerBounds') {
        if (o.boundsError) { const e = new Error('unavailable'); e.code = o.boundsError; throw e; }
        return { kind: 'layerBounds', bounds: vals.bounds, parentLayerUid: layer.parentLayerUid, parentAffine: vals.parentAffine || [1, 0, 0, 1, 0, 0], frame: v.frame };
      }
      if (v.kind === 'rawSample') return { kind: 'rawSample', value: vals[v.property], frame: v.frame };
      return { kind: 'authoredKeys', items: keys, nextOffset: null, trackPresent: keys.length > 0 };
    },
    async editPropertyBatch(payload, pin, current) {
      edits.push({ payload, pin });
      if (o.reject) { const e = new Error('Native property edit refused'); e.code = 'stale_revision'; throw e; }
      if (current() !== true) throw new Error('stale');
      revision += 1;
      return { ok: true, result: { applied: true, historyEntriesAdded: 1 } };
    },
  };
  if (o.component) sandbox.NemoNativeComponentContext = { presented: () => ({ contextId: 'comp-a', totalFrames: 12, fps: 12 }), layers: () => [Object.assign({ layerUid: 'in-1', visible: true, locked: false }, o.row)] };
  sandbox.NemoNativeObjectProject = { desktop: () => session };
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
  const ctl = sandbox.NemoNativeMotionMulti.desktop(vm.runInContext('globalThis', sandbox));
  const flush = () => { for (let n = 0; n < 3 && frames.length; n++) frames.splice(0).forEach((fn) => fn()); };
  const fire = (type, event) => {
    const e = Object.assign({ target: sandbox.canvas, button: 0, pointerId: 1, stopped: false, prevented: false, stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; } }, event);
    for (const l of listeners.filter((x) => x.type === type)) { l.fn(e); if (e.stopped) break; }
    return e;
  };
  const at = (x, y, extra) => Object.assign({ clientX: x + pan.dx, clientY: y + pan.dy }, extra);
  const overlay = () => sandbox.document.body.children.find((c) => c.attrs['data-native-motion-multi'] !== undefined);
  return { sandbox, ctl, flush, fire, at, toasts, reads, edits, listeners, overlay, bump: () => { revision += 1; }, busy: (v) => { busyQueue = v; }, layer,
    ready: async () => { await ctl.refresh(); flush(); },
    drag: async (from, to, extra, up) => { const d = fire('pointerdown', at(from[0], from[1], extra)); fire('pointermove', at(to[0], to[1], extra)); const u = up === false ? null : fire('pointerup', at(to[0], to[1], extra)); await new Promise((r) => setTimeout(r, 0)); return { d, u }; } };
}

const plain = (v) => JSON.parse(JSON.stringify(v));
const batch = (h) => plain(h.edits.map((e) => e.payload));
const byLayer = (payload) => plain(payload.edits.map((e) => [e.target.layerUid, e.property, e.action.values]));

test('the union box, its four corner squares and the Rotate ring are drawn from the per-layer native reads', async () => {
  const h = harness({ perLayer: TWO });
  await h.ready();
  const svg = h.overlay(), kinds = svg.children.map((c) => c.name);
  assert.equal(svg.children[0].attrs.points, '100,100 500,100 500,200 100,200');
  assert.equal(kinds.filter((k) => k === 'rect').length, 4);
  const ring = svg.children.find((c) => c.name === 'circle');
  assert.deepEqual(plain([ring.attrs.cx, ring.attrs.cy, ring.attrs.r]), [300, 150, 36]);
});
test('a body drag moves every layer by the same delta in ONE batch; Shift locks the axis; a click writes nothing', async () => {
  const h = harness({ perLayer: TWO }); await h.ready();
  await h.drag([300, 150], [330, 170]);
  assert.equal(h.edits.length, 1, 'one request for two layers');
  assert.deepEqual(byLayer(h.edits[0].payload), [['A', 'position', [30, 20]], ['B', 'position', [30, 20]]]);
  assert.equal(h.edits[0].payload.command, 'object.property.batch');
  assert.deepEqual(plain(h.edits[0].payload.edits[0].target), { contextId: 'scene-root', layerUid: 'A' });
  assert.deepEqual(plain(h.edits[0].payload.edits[0].action), { kind: 'setValue', frame: 0, values: [30, 20] });
  assert.deepEqual(PIN.map((k) => h.edits[0].pin[k]), ['i', 'd', 1, 4, 'native-object:d:4']);
  const k = harness({ perLayer: TWO }); await k.ready();
  await k.drag([300, 150], [330, 170], { shiftKey: true });
  assert.deepEqual(byLayer(k.edits[0].payload), [['A', 'position', [30, 0]], ['B', 'position', [30, 0]]]);
  const c = harness({ perLayer: TWO }); await c.ready();
  const r = await c.drag([300, 150], [301, 151]);
  assert.equal(c.edits.length, 0, 'under the click threshold');
  assert.equal(r.d.stopped, true, 'the press itself is taken');
  const outside = harness({ perLayer: TWO }); await outside.ready();
  assert.equal(outside.fire('pointerdown', outside.at(300, 400)).stopped, false, 'outside the union box and ring the press is left alone');
});
test('a corner scales every layer by the same ratio about the union centre and each Position follows', async () => {
  const h = harness({ perLayer: TWO }); await h.ready();
  await h.drag([500, 200], [700, 250]);
  assert.deepEqual(byLayer(h.edits[0].payload), [['A', 'position', [-100, 0]], ['A', 'scale', [200, 200]], ['B', 'position', [150, 0]], ['B', 'scale', [200, 200]]]);
  const floor = harness({ perLayer: TWO }); await floor.ready();
  await floor.drag([500, 200], [301, 150]);
  assert.deepEqual(byLayer(floor.edits[0].payload).filter((e) => e[1] === 'scale').map((e) => e[2]), [[1, 1], [1, 1]], 'never below one percent');
});
test('the Rotate ring rotates every layer by the same angle about the union centre and each Position follows', async () => {
  const h = harness({ perLayer: TWO }); await h.ready();
  await h.drag([336, 150], [300, 186]);
  assert.deepEqual(byLayer(h.edits[0].payload), [['A', 'position', [100, -100]], ['A', 'rotation', [90]], ['B', 'position', [-150, 150]], ['B', 'rotation', [90]]]);
});
test('locked, hidden and 3D layers are left out; fewer than two drawn layers means no box', async () => {
  const three = harness({ layers: [{ layerUid: 'A' }, { layerUid: 'B' }, { layerUid: 'C', locked: true }, { layerUid: 'D', threeD: true }, { layerUid: 'E', visible: false }], perLayer: TWO });
  await three.ready();
  assert.equal(three.overlay().children[0].attrs.points, '100,100 500,100 500,200 100,200');
  await three.drag([300, 150], [310, 150]);
  assert.deepEqual(byLayer(three.edits[0].payload).map((e) => e[0]), ['A', 'B']);
  const one = harness({ layers: [{ layerUid: 'A' }, { layerUid: 'B', locked: true }] }); await one.ready();
  assert.equal(one.overlay().children.length, 0);
  assert.equal(one.fire('pointerdown', one.at(200, 150)).stopped, false);
  const single = harness({ sel: [0] }); await single.ready();
  assert.equal(single.overlay().children.length, 0, 'the single-layer gizmo owns one selected layer');
});
test('Escape, a changed document, a refused edit and Motion off each commit nothing', async () => {
  const esc = harness({ perLayer: TWO }); await esc.ready();
  esc.fire('pointerdown', esc.at(300, 150)); esc.fire('pointermove', esc.at(340, 150)); esc.fire('keydown', { key: 'Escape' }); esc.fire('pointerup', esc.at(340, 150));
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(esc.edits.length, 0);
  const stale = harness({ perLayer: TWO }); await stale.ready();
  stale.fire('pointerdown', stale.at(300, 150)); stale.fire('pointermove', stale.at(340, 150)); stale.bump(); stale.fire('pointerup', stale.at(340, 150));
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(stale.edits.length, 0);
  const refused = harness({ perLayer: TWO, reject: true }); await refused.ready();
  await refused.drag([300, 150], [340, 150]);
  assert.equal(refused.edits.length, 1); assert.match(refused.toasts[0], /refused \(stale_revision\)/);
  const anim = harness({ perLayer: TWO, appMode: 'animation' }); await anim.ready();
  assert.equal(anim.fire('pointerdown', anim.at(300, 150)).stopped, false);
  assert.equal(anim.overlay().children.length, 0);
});
test('more entries than one batch holds is refused with a notice and nothing is written', async () => {
  const layers = Array.from({ length: 40 }, (_, i) => ({ layerUid: 'L' + i }));
  const spread = {}; layers.forEach((l, i) => { spread[l.layerUid] = { bounds: [i * 20, 100, i * 20 + 10, 200] }; });
  const move = harness({ layers, perLayer: spread }); await move.ready();
  await move.drag([200, 150], [210, 150]);
  assert.equal(move.edits[0].payload.edits.length, 40, 'forty Position edits fit');
  const scale = harness({ layers, perLayer: spread }); await scale.ready();
  await scale.drag([790, 200], [1200, 250]);
  assert.equal(scale.edits.length, 0); assert.match(scale.toasts[0], /Select fewer layers/);
});
