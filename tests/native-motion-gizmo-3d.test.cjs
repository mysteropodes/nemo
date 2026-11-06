'use strict';
// #2520/#2525: the 3D layer gizmo of the native Motion canvas. Part 1 pins the pure maths against numbers produced by running the baseline functions of
// motion.js@1065e9d8 (`rotate3D`, `project3DToScreen`, `gizmo3DPose`, `gizmo3DAxisScreenPoints`, `gizmo3DRingScreenPoints`) in a vm with a stubbed
// `valueAtFrame`; part 2 drives the real native-motion-canvas.js on a 3D layer row against a session double that records every read and write.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Giz = require('../src/js/adapters/native-motion-gizmo-3d.js');

const near = (a, b, eps) => Math.abs(a - b) <= (eps || 1e-5);
const nearAll = (a, b, eps) => a.length === b.length && a.every((v, i) => near(v, b[i], eps));
const READ = { bounds: [380, 220, 580, 320], position: [40, -20], anchor: [10, 5], rotation: 15, positionZ: -300, rotationX: 30, rotationY: -20, canvasW: 960, canvasH: 540 };

test('rotate and project reproduce the baseline numbers (X, then Y, then Z; camera distance 2000; depth clamp at 5%)', () => {
  assert.ok(nearAll(Giz.rotate(10, 20, 30, 30, -20, 15), [-3.410682, 1.488477, 37.231058], 1e-6));
  assert.deepEqual(Giz.rotate(1, 2, 3, 0, 0, 0), [1, 2, 3], 'no turn is the identity');
  const p = Giz.project(100, -50, -300, 960, 540);
  assert.ok(near(p.x, 597.6470588235294, 1e-9) && near(p.y, 211.1764705882353, 1e-9));
  assert.deepEqual(Giz.project(-280, -120, 0, 960, 540), { x: 200, y: 150 }, 'z = 0 projects to itself');
  assert.equal(Giz.project(100, 0, -1990, 960, 540).x, 480 + 100 / 0.05, 'a point nearer than 5% of the depth is clamped, not mirrored');
});
test('the pose, the three arrows and the three rings sit where the baseline puts them', () => {
  const pose = Giz.pose(READ);
  assert.deepEqual([pose.originWX, pose.originWY, pose.originWZ], [50, -15, -300]);
  const a = Giz.arrows(pose), want = { x: [[538.823529, 252.352941], [722.263412, 302.673089]], y: [[538.823529, 252.352941], [457.497304, 413.010191]], z: [[538.823529, 252.352941], [503.606749, 144.645586]] };
  Giz.AXES.forEach((k) => { assert.ok(nearAll([a[k].origin.x, a[k].origin.y], want[k][0])); assert.ok(nearAll([a[k].tip.x, a[k].tip.y], want[k][1]), 'tip ' + k); });
  const r = Giz.rings(pose), ring = { x: [[479.304406, 369.930988], [512.816077, 172.811487], [602.779025, 126.010989]], y: [[512.816077, 172.811487], [672.606636, 289.051535], [568.282762, 342.451353]],
    z: [[672.606636, 289.051535], [479.304406, 369.930988], [397.854421, 213.683126]] };
  Giz.AXES.forEach((k) => { assert.equal(r[k].length, 33, 'a closed polyline of 32 steps'); [0, 8, 16].forEach((i, n) => assert.ok(nearAll([r[k][i].x, r[k][i].y], ring[k][n]), k + ' ring point ' + i)); });
  assert.deepEqual([r.x[0].x, r.x[0].y], [r.x[32].x, r.x[32].y], 'the ring closes on itself');
});
test('a pose is refused unless every value is finite', () => {
  assert.equal(Giz.pose(Object.assign({}, READ, { positionZ: NaN })), null);
  assert.equal(Giz.pose(Object.assign({}, READ, { bounds: [1, 2, 3] })), null);
  assert.equal(Giz.pose(Object.assign({}, READ, { position: [1] })), null);
  assert.equal(Giz.pose(null), null);
});
test('a preview moves the origin with Position, and the other properties replace their own value only', () => {
  const pose = Giz.pose(READ), moved = Giz.preview(pose, { property: 'position', values: [60, 0] });
  assert.deepEqual([moved.originWX, moved.originWY, moved.originWZ], [70, 5, -300]);
  assert.equal(Giz.preview(pose, { property: 'positionZ', values: [-100] }).originWZ, -100);
  assert.equal(Giz.preview(pose, { property: 'rotationX', values: [45] }).rotX, 45);
  assert.equal(Giz.preview(pose, { property: 'rotationY', values: [5] }).rotY, 5);
  assert.equal(Giz.preview(pose, { property: 'rotation', values: [90] }).rot, 90);
  assert.equal(Giz.preview(pose, null), pose);
  assert.equal(pose.posZ, -300, 'the pose itself is never mutated');
});
test('the closer of an arrow and a ring is grabbed; the ancestors\' affine moves both', () => {
  const pose = Giz.pose(READ), id = (q) => q, a = Giz.arrows(pose);
  const mid = { x: (a.x.origin.x + a.x.tip.x) / 2, y: (a.x.origin.y + a.x.tip.y) / 2 };
  assert.deepEqual(Giz.hit(pose, null, id, mid), { type: 'axis3d', axis: 'x', dist: Giz.hit(pose, null, id, mid).dist });
  assert.ok(Giz.hit(pose, null, id, mid).dist < 1e-9);
  assert.equal(Giz.hit(pose, null, id, { x: 0, y: 0 }), null);
  const ring = Giz.rings(pose).x[8], hit = Giz.hit(pose, null, id, { x: ring.x, y: ring.y });
  assert.equal(hit && hit.type, 'ring3d');
  assert.equal(hit.axis, 'x');
  assert.equal(Giz.hit(null, null, id, mid), null);
  // a parent that shifts everything by (100, 50): the same point is no longer on the arrow, the shifted one is
  const outer = [1, 0, 0, 1, 100, 50];
  const stayed = Giz.hit(pose, outer, id, mid);
  assert.ok(!stayed || stayed.axis !== 'x' || stayed.type !== 'axis3d', 'the unshifted midpoint is no longer on the shifted X arrow');
  assert.equal(Giz.hit(pose, outer, id, { x: mid.x + 100, y: mid.y + 50 }).axis, 'x');
});
test('an arrow drag is the pointer projected on the arrow\'s own screen direction; a ring drag is the pointer turn about the origin', () => {
  const pose = Giz.pose(READ), a = Giz.arrows(pose), from = { x: 600, y: 270 };
  const found = { type: 'axis3d', axis: 'x' }, g = Giz.begin(found, pose, null, from);
  const dx = a.x.tip.x - a.x.origin.x, dy = a.x.tip.y - a.x.origin.y, len = Math.hypot(dx, dy);
  const out = Giz.drag(g, { x: 700, y: 270 });
  assert.equal(out.property, 'position');
  assert.ok(near(out.values[0], 40 + (100 * dx) / len, 1e-9) && out.values[1] === -20, 'only the arrow\'s own Position component moves');
  assert.equal(Giz.drag(Giz.begin({ type: 'axis3d', axis: 'z' }, pose, null, from), { x: 600, y: 270 }), null, 'a pointer that stayed changes nothing');
  const z = Giz.drag(Giz.begin({ type: 'axis3d', axis: 'z' }, pose, null, from), { x: 650, y: 300 });
  assert.equal(z.property, 'positionZ');
  const origin = Giz.arrows(pose).z.origin, ring = Giz.begin({ type: 'ring3d', axis: 'y' }, pose, null, { x: origin.x + 50, y: origin.y });
  const turned = Giz.drag(ring, { x: origin.x, y: origin.y + 50 });
  assert.deepEqual([turned.property, turned.values[0]], ['rotationY', -20 + 90], 'a quarter turn about the origin adds 90 degrees to the stored Rotation Y');
  assert.equal(Giz.drag(Giz.begin({ type: 'ring3d', axis: 'x' }, pose, null, { x: origin.x + 50, y: origin.y }), { x: origin.x + 80, y: origin.y }), null, 'moving along the same ray turns nothing');
});

// ---- part 2: the real canvas on a 3D layer ----
const ROOT = path.join(__dirname, '..', 'src', 'js', 'adapters');
const SOURCES = ['native-object-widget-ui.js', 'native-motion-canvas-math.js', 'native-motion-effector.js', 'native-motion-gizmo-3d.js', 'native-motion-canvas.js'];
const CURVE = path.join(__dirname, '..', 'src', 'js', 'animation', 'curve.js');
const LAYER = 'native-layer:1:0';
const BASE = { bounds: [100, 100, 300, 200], position: [0, 0], anchor: [0, 0], rotation: [0], scale: [100, 100], positionZ: [0], rotationX: [0], rotationY: [0] };
const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
function harness(options) {
  const o = options || {}, values = Object.assign({}, BASE, o.values), keys = o.keys || [], pan = { dx: o.dx || 0, dy: o.dy || 0 };
  const sandbox = { console, setTimeout, clearTimeout, setInterval: () => 0 };
  sandbox.window = sandbox; sandbox.globalThis = sandbox;
  sandbox.state = { appMode: o.appMode || 'motion', tool: o.tool || 'select', currentFrame: o.frame === undefined ? 0 : o.frame, activeLayerIdx: 0, playing: false, activeSymbolId: null };
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
}const plain = (v) => JSON.parse(JSON.stringify(v));
const written = (h) => plain(h.edits.map((e) => ({ property: e.payload.property, action: e.payload.action })));
const three = (extra) => harness(Object.assign({ layer: { threeD: true } }, extra));
// The pose of the fixture layer: bounds centre (200, 150), so the origin projects to (200, 150) on the 960 x 540 canvas.
const POSE = Giz.pose({ bounds: BASE.bounds, position: [0, 0], anchor: [0, 0], rotation: 0, positionZ: 0, rotationX: 0, rotationY: 0, canvasW: 960, canvasH: 540 });

test('a 3D layer reads Position Z and both rotations and draws arrows and rings instead of the box, the scale handles and the rotate ring', async () => {
  const h = three();
  await h.ready();
  const names = h.reads.filter((v) => v.kind === 'rawSample').map((v) => v.property).sort();
  assert.deepEqual(names, ['anchor', 'position', 'positionZ', 'rotation', 'rotationX', 'rotationY', 'scale']);
  const kinds = h.overlay().children.map((c) => c.name);
  assert.equal(kinds.filter((k) => k === 'line').length, 3 + 2, 'three arrows and the two pivot crosshair strokes');
  assert.equal(kinds.filter((k) => k === 'polygon').length, 3 + 3, 'three arrow heads and three rings, and no box outline');
  assert.equal(kinds.filter((k) => k === 'rect').length, 0, 'no scale handles');
  assert.equal(kinds.filter((k) => k === 'circle').length, 1, 'the pivot only, no rotate ring');
  const arrow = h.overlay().children.find((c) => c.name === 'line' && c.attrs.stroke === 'rgba(235,70,70,1)');
  assert.deepEqual([arrow.attrs.x1, arrow.attrs.y1, arrow.attrs.x2, arrow.attrs.y2], [200, 150, 380, 150], 'the X arrow is 180 canvas units long at depth 0');
  assert.equal(arrow.attrs.stroke, 'rgba(235,70,70,1)');
});
test('a flat layer keeps its box and reads none of the 3D values', async () => {
  const h = harness();
  await h.ready();
  assert.equal(h.reads.filter((v) => v.kind === 'rawSample').length, 4);
  assert.equal(h.overlay().children[0].attrs.points, '100,100 300,100 300,200 100,200');
});
test('dragging the Z arrow is ONE positionZ setValue at the playhead from the pointer projected on the arrow', async () => {
  const h = three();
  await h.ready();
  const z = Giz.arrows(POSE).z, from = [z.origin.x + 0.7 * (z.tip.x - z.origin.x), z.origin.y + 0.7 * (z.tip.y - z.origin.y)];
  const dx = z.tip.x - z.origin.x, dy = z.tip.y - z.origin.y, len = Math.hypot(dx, dy);
  const r = await h.drag(from, [from[0] + 20, from[1]]);
  assert.equal(r.d.stopped, true, 'the press is taken');
  const edit = written(h);
  assert.equal(edit.length, 1);
  assert.deepEqual([edit[0].property, edit[0].action.kind, edit[0].action.frame], ['positionZ', 'setValue', 0]);
  assert.ok(near(edit[0].action.values[0], (20 * dx) / len, 1e-5), 'along the arrow: ' + edit[0].action.values[0]);
  assert.deepEqual(PIN.map((k) => h.edits[0].pin[k]), ['i', 'd', 1, 4, 'native-object:d:4']);
});
test('dragging the X arrow writes Position with only its own component changed', async () => {
  const h = three({ values: { position: [10, 30], bounds: [90, 70, 290, 170] } });
  await h.ready();
  const pose = Giz.pose({ bounds: [90, 70, 290, 170], position: [10, 30], anchor: [0, 0], rotation: 0, positionZ: 0, rotationX: 0, rotationY: 0, canvasW: 960, canvasH: 540 });
  const x = Giz.arrows(pose).x, mid = [(x.origin.x + x.tip.x) / 2, (x.origin.y + x.tip.y) / 2];
  await h.drag(mid, [mid[0] + 40, mid[1] + 25]);
  assert.deepEqual(written(h), [{ property: 'position', action: { kind: 'setValue', frame: 0, values: [50, 30] } }], 'the Y component stays at 30 and the pointer\'s vertical travel is ignored');
});
test('dragging a ring writes the matching rotation from the pointer turn; the closer of arrow and ring wins', async () => {
  const z = three(); await z.ready();
  const c = POSE, ringZ = Giz.rings(c).z[4]; // 45 degrees on the Z ring, away from every arrow
  await z.drag([ringZ.x, ringZ.y], [200, 280]);
  assert.deepEqual(written(z), [{ property: 'rotation', action: { kind: 'setValue', frame: 0, values: [45] } }]);
  const x = three(); await x.ready();
  const ringX = Giz.rings(c).x[4], start = Math.atan2(ringX.y - 150, ringX.x - 200) * 180 / Math.PI;
  await x.drag([ringX.x, ringX.y], [250, 150]);
  const edit = written(x);
  assert.equal(edit[0].property, 'rotationX');
  assert.ok(near(edit[0].action.values[0], 0 - start, 1e-4), 'the turn from the start angle to 0 degrees: ' + edit[0].action.values[0]);
  const y = three({ values: { rotationY: [20] } }); await y.ready();
  const ringY = Giz.rings(Giz.pose({ bounds: BASE.bounds, position: [0, 0], anchor: [0, 0], rotation: 0, positionZ: 0, rotationX: 0, rotationY: 20, canvasW: 960, canvasH: 540 })).y[4];
  await y.drag([ringY.x, ringY.y], [ringY.x, ringY.y + 60]);
  assert.equal(written(y)[0].property, 'rotationY');
});
test('the box body, the scale handles and the rotate ring are not grabs on a 3D layer', async () => {
  const h = three(); await h.ready();
  for (const p of [[250, 170], [300, 200], [231.2, 168]]) { // the body, a corner, and a point of the flat box's rotate ring
    const r = await h.drag(p, [p[0] + 50, p[1] + 30]);
    assert.notEqual(r.d.stopped, true, 'the press is left to the rest of the page');
  }
  assert.deepEqual(written(h), [], 'no gesture reaches the box on a 3D layer');
});
test('a click under the threshold, Escape, a locked layer and a stale revision write nothing', async () => {
  const a = three(); await a.ready();
  const z = Giz.arrows(POSE).z, mid = [(z.origin.x + z.tip.x) / 2, (z.origin.y + z.tip.y) / 2];
  await a.drag(mid, [mid[0] + 1, mid[1]]);
  assert.equal(a.edits.length, 0, 'under the click threshold');
  const b = three(); await b.ready();
  b.fire('pointerdown', b.at(mid[0], mid[1])); b.fire('pointermove', b.at(mid[0] + 30, mid[1])); b.fire('keydown', { key: 'Escape' }); b.fire('pointerup', b.at(mid[0] + 30, mid[1]));
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(b.edits.length, 0, 'Escape cancels the gesture');
  const c = harness({ layer: { threeD: true, locked: true } }); await c.ready();
  const r = await c.drag(mid, [mid[0] + 30, mid[1]]);
  assert.equal(c.edits.length, 0, 'a locked layer takes no gesture');
  assert.notEqual(r.d.stopped, true);
  const d = three(); await d.ready();
  d.fire('pointerdown', d.at(mid[0], mid[1])); d.bump(); d.fire('pointermove', d.at(mid[0] + 30, mid[1])); d.fire('pointerup', d.at(mid[0] + 30, mid[1]));
  await new Promise((r2) => setTimeout(r2, 0));
  assert.equal(d.edits.length, 0, 'a document change in the middle of the gesture commits nothing');
});
test('the preview follows the drag: the gizmo is redrawn from the previewed value before anything is written', async () => {
  const h = three(); await h.ready();
  const z = Giz.arrows(POSE).z, mid = [(z.origin.x + z.tip.x) / 2, (z.origin.y + z.tip.y) / 2];
  const before = JSON.stringify(h.overlay().children.map((c) => c.attrs));
  h.fire('pointerdown', h.at(mid[0], mid[1])); h.fire('pointermove', h.at(mid[0] + 20, mid[1] + 8)); h.flush();
  const during = JSON.stringify(h.overlay().children.map((c) => c.attrs));
  assert.notEqual(before, during);
  assert.equal(h.edits.length, 0, 'nothing is written while the pointer is down');
  h.fire('pointerup', h.at(mid[0] + 20, mid[1] + 8));
});
