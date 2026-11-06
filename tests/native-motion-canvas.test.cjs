'use strict';
// #2520/#2525: the whole-layer Motion gizmo and its pointer gestures on a native object project. The real native-motion-canvas.js, its maths
// and native-object-widget-ui.js (the presentation proof and canvas mapping) run in one vm context against a session double that records every
// read and write. Gesture fixtures: click, drag, Shift axis lock, cancel, frame change, replaced owner, locked and parented layers.
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
  sandbox.state = { appMode: o.appMode || 'motion', tool: o.tool || 'select', currentFrame: o.frame === undefined ? 0 : o.frame, activeLayerIdx: 0, playing: false, activeSymbolId: o.component ? 'comp-a' : null };
  const toasts = [], reads = [], edits = [], listeners = [], frames = [];
  sandbox.showToast = (m) => toasts.push(m);
  let revision = 4, busyQueue = false, shown = null, shownKey = '';
  const layer = Object.assign({ layerUid: LAYER, visible: true, locked: false, parentLayerUid: null }, o.layer);
  const info = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 'native-object:d:' + revision, canvasW: 960, canvasH: 540, layers: [layer] });
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
  const ctl = sandbox.NemoNativeMotionCanvas.desktop(vm.runInContext('globalThis', sandbox));
  const flush = () => { for (let n = 0; n < 3 && frames.length; n++) frames.splice(0).forEach((fn) => fn()); };
  const fire = (type, event) => {
    const e = Object.assign({ target: sandbox.canvas, button: 0, pointerId: 1, stopped: false, prevented: false, stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; } }, event);
    for (const l of listeners.filter((x) => x.type === type)) { l.fn(e); if (e.stopped) break; }
    return e;
  };
  const at = (x, y, extra) => Object.assign({ clientX: x + pan.dx, clientY: y + pan.dy }, extra);
  const overlay = () => sandbox.document.body.children.find((c) => c.attrs['data-native-motion-canvas'] !== undefined);
  return { sandbox, ctl, flush, fire, at, toasts, reads, edits, listeners, overlay, bump: () => { revision += 1; }, busy: (v) => { busyQueue = v; }, layer,
    ready: async () => { await ctl.refresh(); flush(); },
    drag: async (from, to, extra, up) => { const d = fire('pointerdown', at(from[0], from[1], extra)); fire('pointermove', at(to[0], to[1], extra)); const u = up === false ? null : fire('pointerup', at(to[0], to[1], extra)); await new Promise((r) => setTimeout(r, 0)); return { d, u }; } };
}
const plain = (v) => JSON.parse(JSON.stringify(v));
const written = (h) => plain(h.edits.map((e) => ({ property: e.payload.property, action: e.payload.action })));

test('the gizmo draws the box, handles, ring and pivot from the native reads', async () => {
  const h = harness();
  await h.ready();
  assert.deepEqual(plain(h.reads.map((v) => v.kind)).sort(), ['authoredKeys', 'layerBounds', 'rawSample', 'rawSample', 'rawSample', 'rawSample']);
  const svg = h.overlay(), kinds = svg.children.map((c) => c.name);
  assert.equal(svg.children[0].attrs.points, '100,100 300,100 300,200 100,200');
  assert.equal(kinds.filter((k) => k === 'rect').length, 8, 'four corners and four edge midpoints');
  assert.equal(kinds.filter((k) => k === 'circle').length, 2, 'the Rotate ring and the pivot');
  const ring = svg.children.find((c) => c.name === 'circle');
  assert.deepEqual(plain([ring.attrs.cx, ring.attrs.cy, ring.attrs.r]), [200, 150, 36]);
});
test('a body drag is ONE setValue at the playhead; Shift locks the axis; a click writes nothing', async () => {
  const h = await (async () => { const x = harness(); await x.ready(); return x; })();
  await h.drag([200, 150], [260, 170]);
  assert.deepEqual(written(h), [{ property: 'position', action: { kind: 'setValue', frame: 0, values: [60, 20] } }]);
  assert.deepEqual(PIN.map((k) => h.edits[0].pin[k]), ['i', 'd', 1, 4, 'native-object:d:4']);
  assert.deepEqual(plain(h.edits[0].payload.target), { contextId: 'scene-root', layerUid: LAYER });
  const k = harness(); await k.ready();
  await k.drag([200, 150], [260, 170], { shiftKey: true });
  assert.deepEqual(written(k)[0].action.values, [60, 0]);
  const c = harness(); await c.ready();
  const r = await c.drag([200, 150], [201, 151]);
  assert.equal(c.edits.length, 0, 'under the click threshold');
  assert.equal(r.d.stopped, true, 'the press itself is taken');
});
test('the Rotate ring writes rotation and the scale handles write scale, each from the start values', async () => {
  const a = harness(); await a.ready();
  await a.drag([236, 150], [200, 186]);
  assert.deepEqual(written(a), [{ property: 'rotation', action: { kind: 'setValue', frame: 0, values: [90] } }]);
  const b = harness(); await b.ready();
  await b.drag([300, 200], [400, 250]);
  assert.deepEqual(written(b), [{ property: 'scale', action: { kind: 'setValue', frame: 0, values: [200, 200] } }], 'a corner scales both axes by the distance ratio');
  const c = harness(); await c.ready();
  await c.drag([300, 150], [400, 150]);
  assert.deepEqual(written(c), [{ property: 'scale', action: { kind: 'setValue', frame: 0, values: [200, 100] } }], 'an edge scales its own axis');
  const d = harness({ values: { rotation: [30] } }); await d.ready();
  await d.drag([200 + 36 * Math.cos(Math.PI / 6), 150 + 36 * Math.sin(Math.PI / 6)], [200 + 36 * Math.cos(Math.PI / 3), 150 + 36 * Math.sin(Math.PI / 3)]);
  assert.equal(Math.round(written(d)[0].action.values[0] * 1e4) / 1e4, 60, 'a turn adds the pointer angle to the stored rotation');
});
test('Alt+drag moves the anchor; with Rotation or Scale set the SAME edit carries keepPose so Position follows in one revision', async () => {
  const a = harness(); await a.ready();
  await a.drag([200, 150], [220, 160], { altKey: true });
  assert.deepEqual(written(a), [{ property: 'anchor', action: { kind: 'setValue', frame: 0, values: [20, 10] } }], 'no compensation needed at Rotation 0 and Scale 100');
  const b = harness({ values: { rotation: [30] } }); await b.ready();
  await b.drag([200, 150], [220, 160], { altKey: true });
  const w = written(b);
  assert.equal(w.length, 1, 'one edit, not an anchor edit plus a Position edit');
  assert.equal(w[0].property, 'anchor');
  assert.equal(w[0].action.keepPose, true);
  // the pointer in the layer's own space: inverse of the 30 degree turn about the pivot, minus the bounds centre
  const c30 = Math.cos(Math.PI / 6), s30 = Math.sin(Math.PI / 6);
  assert.deepEqual(w[0].action.values.map((v) => Math.round(v * 1e4) / 1e4), [Math.round((20 * c30 + 10 * s30) * 1e4) / 1e4, Math.round((-20 * s30 + 10 * c30) * 1e4) / 1e4]);
  assert.deepEqual(b.toasts, []);
  const c = harness({ values: { scale: [200, 100] } }); await c.ready();
  await c.drag([200, 150], [220, 160], { altKey: true });
  assert.equal(written(c)[0].action.keepPose, true, 'a Scale other than 100 percent also needs the compensation');
});
test('the box stays put while a keepPose anchor is dragged (the preview applies the same Position follow)', async () => {
  const h = harness({ values: { rotation: [30], scale: [150, 100] } }); await h.ready();
  const before = h.overlay().children[0].attrs.points.split(' ').map((p) => p.split(',').map(Number));
  await h.drag([200, 150], [260, 190], { altKey: true }, false);
  h.flush();
  const during = h.overlay().children.find((c) => c.name === 'polygon' && c.attrs.stroke === 'rgba(255,184,108,1)').attrs.points.split(' ').map((p) => p.split(',').map(Number));
  assert.ok(during.every((q, i) => Math.abs(q[0] - before[i][0]) < 1e-6 && Math.abs(q[1] - before[i][1]) < 1e-6), 'corners unchanged: ' + JSON.stringify([before, during]));
  assert.equal(h.edits.length, 0, 'nothing is written before the release');
});
test('a Position key diamond is grabbed on its own frame and written as setKey', async () => {
  const keys = [{ frame: 0, v: [0, 0] }, { frame: 10, v: [100, 50] }];
  const h = harness({ keys }); await h.ready();
  await h.drag([200, 150], [230, 170]);
  assert.deepEqual(written(h), [{ property: 'position', action: { kind: 'setKey', frame: 0, values: [30, 20] } }]);
  const away = harness({ keys, frame: 5, values: { position: [50, 25] } }); await away.ready();
  await away.drag([250, 175], [270, 185]);
  assert.deepEqual(written(away), [{ property: 'position', action: { kind: 'setValue', frame: 5, values: [70, 35] } }], 'off a key the drag keys the playhead, never an existing key');
  const svg = harness({ keys }); await svg.ready();
  assert.ok(svg.overlay().children.some((c) => c.name === 'polyline'), 'the Position path is drawn between the keys');
});
test('Escape, a frame change and a replaced owner commit nothing', async () => {
  const a = harness(); await a.ready();
  a.fire('pointerdown', a.at(200, 150)); a.fire('pointermove', a.at(260, 170)); a.fire('keydown', { key: 'Escape' }); a.fire('pointerup', a.at(260, 170));
  assert.equal(a.edits.length, 0);
  const b = harness(); await b.ready();
  b.fire('pointerdown', b.at(200, 150)); b.fire('pointermove', b.at(260, 170)); b.sandbox.state.currentFrame = 3; b.fire('pointerup', b.at(260, 170));
  assert.equal(b.edits.length, 0, 'the playhead moved mid-gesture');
  const c = harness(); await c.ready();
  c.fire('pointerdown', c.at(200, 150)); c.fire('pointermove', c.at(260, 170)); c.bump(); c.fire('pointerup', c.at(260, 170));
  assert.equal(c.edits.length, 0, 'the document changed under the gesture');
  assert.equal(c.ctl.dragging(), false);
});
test('a locked layer, several selected layers, another mode and a component context take no gesture', async () => {
  for (const [name, opts, prep] of [['locked', { layer: { locked: true } }], ['anim2d', { appMode: 'anim2d' }],
    ['several', {}, (h) => { h.sandbox._layerSel = [0, 1]; }], ['component', {}, (h) => { h.sandbox.state.activeSymbolId = 'c1'; }]]) {
    const h = harness(opts); if (prep) prep(h);
    await h.ready();
    const { d } = await h.drag([200, 150], [260, 170]);
    assert.equal(d.stopped, false, name + ': the press is left to the other tools');
    assert.equal(h.edits.length, 0, name);
  }
  const draw = harness({ tool: 'draw' }); await draw.ready();
  assert.equal((await draw.drag([200, 150], [260, 170])).d.stopped, false, 'with a drawing tool the box body does not take the press');
  assert.equal((await (async () => { const x = harness({ tool: 'draw' }); await x.ready(); return x.drag([236, 150], [200, 186]); })()).d.stopped, true, 'the Rotate ring does');
});
test('a refused edit is a notice, never a throw, and the pan offset never reaches the written values', async () => {
  const h = harness({ reject: true, dx: 40, dy: 25 }); await h.ready();
  await h.drag([200, 150], [260, 170]);
  assert.equal(h.edits.length, 1);
  assert.deepEqual(plain(h.edits[0].payload.action.values), [60, 20], 'world units, not client pixels');
  assert.match(h.toasts[0], /refused/);
});
test('no Paper or legacy layer model is read: the module needs only the native session', async () => {
  const h = harness(); await h.ready();
  for (const name of ['SMMotion', 'paper', 'userLayers', 'pushUndo']) assert.equal(h.sandbox[name], undefined, name);
});

const AFFINE_90 = [0, 1, -1, 0, 0, 0]; // (x, y) -> (-y, x)
test('a parented layer is drawn and dragged through its ancestors: a translation, then a quarter turn (#2520)', async () => {
  const t = harness({ values: { parentAffine: [1, 0, 0, 1, 40, 30] }, layer: { parentLayerUid: 'native-layer:0:0' } }); await t.ready();
  assert.equal(t.overlay().children[0].attrs.points, '140,130 340,130 340,230 140,230', 'the box sits where the ancestor puts it');
  await t.drag([240, 180], [300, 200]);
  assert.deepEqual(written(t), [{ property: 'position', action: { kind: 'setValue', frame: 0, values: [60, 20] } }]);
  const r = harness({ values: { parentAffine: AFFINE_90 }, layer: { parentLayerUid: 'native-layer:0:0' } }); await r.ready();
  assert.equal(r.overlay().children[0].attrs.points, '-100,100 -100,300 -200,300 -200,100');
  await r.drag([-150, 200], [-150, 260]);
  assert.deepEqual(written(r), [{ property: 'position', action: { kind: 'setValue', frame: 0, values: [60, 0] } }], 'a world delta (0, 60) is the layer-space delta (60, 0)');
  const g = harness({ values: { parentAffine: AFFINE_90 }, layer: { parentLayerUid: 'native-layer:0:0' } }); await g.ready();
  await g.drag([-150, 200], [-120, 260], { shiftKey: true });
  assert.deepEqual(written(g)[0].action.values, [60, 0], 'the axis lock is a world axis: (30, 60) locks to (0, 60), which is (60, 0) in the layer');
});
test('a parented layer keeps its anchor, rotation and key gestures in the ancestors\' frame (#2520)', async () => {
  const keys = [{ frame: 0, v: [0, 0] }, { frame: 10, v: [100, 50] }];
  const h = harness({ keys, values: { parentAffine: AFFINE_90 }, layer: { parentLayerUid: 'native-layer:0:0' } }); await h.ready();
  await h.drag([-150, 200], [-170, 230]); // the key at frame 0 is at the pivot (200, 150) -> (-150, 200); world (-20, 30) -> local (30, 20)
  assert.deepEqual(written(h), [{ property: 'position', action: { kind: 'setKey', frame: 0, values: [30, 20] } }]);
  const a = harness({ values: { parentAffine: AFFINE_90 }, layer: { parentLayerUid: 'native-layer:0:0' } }); await a.ready();
  await a.drag([-150, 200], [-160, 220], { altKey: true });
  assert.deepEqual(written(a), [{ property: 'anchor', action: { kind: 'setValue', frame: 0, values: [20, 10] } }], 'world (-10, 20) is local (20, 10)');
  const mirrored = harness({ values: { parentAffine: [-1, 0, 0, 1, 0, 0] }, layer: { parentLayerUid: 'native-layer:0:0' } }); await mirrored.ready();
  const ring = [-200 + 36, 150];
  await mirrored.drag(ring, [-200, 150 + 36]);
  assert.equal(Math.round(written(mirrored)[0].action.values[0] * 1e4) / 1e4, -90, 'a mirroring ancestor reverses the sense of a turn');
  const collapsed = harness({ values: { parentAffine: [0, 0, 0, 0, 0, 0] }, layer: { parentLayerUid: 'native-layer:0:0' } }); await collapsed.ready();
  assert.equal((await collapsed.drag([200, 150], [260, 170])).d.stopped, false, 'a collapsed ancestor leaves no box to grab');
});
test('spatial key handles hIn / hOut are drawn, grabbed on any frame and written as setKeyHandle (#2520)', async () => {
  const keys = [{ frame: 0, v: [0, 0], hOut: [50, 0] }, { frame: 10, v: [100, 50], hIn: [-40, 0] }];
  const h = harness({ keys, frame: 5, values: { position: [50, 25] } }); await h.ready();
  const circles = h.overlay().children.filter((c) => c.name === 'circle' && c.attrs.r === 4);
  assert.equal(circles.length, 2, 'both handles are drawn');
  await h.drag([250, 150], [270, 180]);
  assert.deepEqual(written(h), [{ property: 'position', action: { kind: 'setKeyHandle', frame: 0, handle: 'hOut', values: [70, 30] } }], 'relative to its key, at the key\'s own frame');
  const i = harness({ keys, frame: 5, values: { position: [50, 25] } }); await i.ready();
  await i.drag([260, 200], [280, 190]); // the hIn handle of the key at frame 10 sits at pivot + (100, 50) + (-40, 0) = (260, 200)
  assert.deepEqual(written(i), [{ property: 'position', action: { kind: 'setKeyHandle', frame: 10, handle: 'hIn', values: [-20, -10] } }]);
  const zero = [{ frame: 0, v: [0, 0] }, { frame: 10, v: [100, 50] }];
  const onKey = harness({ keys: zero }); await onKey.ready();
  await onKey.drag([200, 150], [230, 170]);
  assert.equal(written(onKey)[0].action.kind, 'setKey', 'a zero handle never hides the current frame\'s key');
  const away = harness({ keys: zero, frame: 5, values: { position: [50, 25] } }); await away.ready();
  await away.drag([200, 150], [230, 170]);
  assert.deepEqual(written(away), [{ property: 'position', action: { kind: 'setKeyHandle', frame: 0, handle: 'hOut', values: [30, 20] } }], 'on another frame the zero handle is pulled out of its key');
});
test('the Position path is sampled once per frame through each key\'s easing (#2520)', async () => {
  const keys = [{ frame: 0, v: [0, 0] }, { frame: 10, v: [100, 50] }];
  const h = harness({ keys }); await h.ready();
  const kids = h.overlay().children, line = kids.find((c) => c.name === 'polyline');
  const pts = line.attrs.points.split(' ').map((p) => p.split(',').map(Number));
  assert.equal(pts.length, 11, 'one sample per frame from frame 0 to frame 10');
  assert.deepEqual(pts[0], [200, 150]); assert.deepEqual(pts[10], [300, 200]);
  const gaps = pts.slice(1).map((p, i) => p[0] - pts[i][0]);
  assert.ok(gaps[0] < gaps[5] && gaps[9] < gaps[5], 'dense at both ends of the default ease, wide in the middle: ' + gaps.join(','));
  assert.equal(kids.filter((c) => c.name === 'circle' && c.attrs.r === 1.3).length, 11, 'a small dot at every frame');
  const hold = harness({ keys: [{ frame: 0, v: [0, 0], hold: true }, { frame: 10, v: [100, 50] }] }); await hold.ready();
  const held = hold.overlay().children.find((c) => c.name === 'polyline').attrs.points.split(' ');
  assert.equal(new Set(held.slice(0, 10)).size, 1, 'a hold key stays on its point until the next key');
});
test('a layer the native read cannot box says so once instead of staying silent (#2520)', async () => {
  const h = harness({ boundsError: 'unavailable' });
  await h.ready(); await h.ctl.refresh(); await h.ctl.refresh();
  assert.equal(h.toasts.length, 1);
  assert.match(h.toasts[0], /Motion box is not available/);
  assert.equal((await h.drag([200, 150], [260, 170])).d.stopped, false);
  assert.equal(h.edits.length, 0);
  const transient = harness({ boundsError: 'stale_revision' }); await transient.ready();
  assert.deepEqual(transient.toasts, [], 'only a typed unavailable is a notice');
});

// #2256 B-comp-edit-rest: the whole-layer gizmo on the active layer of an ENTERED component (baseline enterSymbol swapped state.layers, so the Motion
// box worked on the component's layers). Every read and the edit carry the held context id and the entered row's uid.
test('inside a component the gizmo is drawn for the entered row, every read and the edit carry the held context id', async () => {
  const h = harness({ component: true });
  await h.ready();
  assert.ok(h.overlay(), 'the box is drawn');
  assert.ok(h.reads.length >= 6 && plain(h.reads.map((v) => v.target.contextId)).every((id) => id === 'comp-a'));
  assert.ok(h.reads.every((v) => v.target.layerUid === 'in-1'), 'the entered row, not the scene layer');
  await h.drag([200, 150], [260, 170]);
  assert.deepEqual(written(h), [{ property: 'position', action: { kind: 'setValue', frame: 0, values: [60, 20] } }]);
  assert.deepEqual(plain(h.edits[0].payload.target), { contextId: 'comp-a', layerUid: 'in-1' });
});
test('inside a component a locked row, several selected rows and another mode take no gesture; the scene is unchanged', async () => {
  for (const [name, opts, prep] of [['locked row', { component: true, row: { locked: true } }], ['anim2d', { component: true, appMode: 'anim2d' }],
    ['several', { component: true }, (h) => { h.sandbox._layerSel = [0, 1]; }]]) {
    const h = harness(opts); if (prep) prep(h);
    await h.ready();
    const { d } = await h.drag([200, 150], [260, 170]);
    assert.equal(d.stopped, false, name);
    assert.equal(h.edits.length, 0, name);
  }
  const scene = harness(); await scene.ready();
  assert.ok(scene.reads.every((v) => v.target.contextId === 'scene-root' && v.target.layerUid === LAYER), 'the scene root keeps its own context');
});
