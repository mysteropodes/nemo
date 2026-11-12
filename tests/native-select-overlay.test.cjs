'use strict';
// #2372 SEL-F2: a Select pick on a native object project must be visible. The overlay is a disposable SVG over the canvas
// (outline, oriented box, 8 handles, hover box), mapped through the viewport underlay, dropped when the pick context changes.
const test = require('node:test'), assert = require('node:assert/strict');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Paper = require('./helpers/native-import-paper.cjs');

function el(tag) {
  const n = { tag, attrs: {}, style: {}, children: [], removed: false,
    setAttribute(k, v) { this.attrs[k] = String(v); },
    appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
    remove() { this.removed = true; if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); } };
  return n;
}
function harness(transform = [1, 0, 0, 1, 0, 0]) {
  const paper = Paper.load(), body = el('body'), frames = [];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 'native-object:d:0', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L' }] };
  let presentation = { ...pin, owner: 'native', status: 'presented', contextId: 'scene-root', frame: 0 };
  const rect = { left: 10, top: 20, width: 100, height: 100 };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ ...pin, presentation }) };
  const root = { paper, Path: paper.Path, Point: paper.Point, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} },
    document: { body, getElementById: () => ({ getBoundingClientRect: () => rect }), createElementNS: (_ns, tag) => el(tag) },
    requestAnimationFrame: (fn) => frames.push(fn),
    NemoNativeObjectProject: { desktop: () => session }, NemoNativeViewportUnderlay: { canvasMapping: { transform: () => transform } },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay };
  const quad = (strokeId, pts, extra) => {
    const p = new paper.Path({ insert: false, closed: true, segments: pts });
    p.data = { strokeId, ...extra };
    return p;
  };
  return { root, body, rect, quad, frames, setPin: (n) => { pin = { ...pin, ...n }; presentation = { ...presentation, ...n }; },
    setView: (t) => { transform = t; presentation = { ...presentation }; },
    tick: () => { const f = frames.splice(0); f.forEach((fn) => fn()); },
    svgs: () => body.children.filter((c) => c.tag === 'svg') };
}
const num = (a) => a.split(/[ ,]+/).map(Number);
const SQUARE = [[40, 40], [80, 40], [80, 90], [40, 90]];

test('a pick creates one overlay with the outline, the box and 8 handles at the expected screen geometry', () => {
  const h = harness([2, 0, 0, 2, 5, 3]);
  Intent.applyPick(h.root, h.quad('a', SQUARE), false);
  assert.equal(h.svgs().length, 1);
  const [svg] = h.svgs();
  assert.equal(svg.attrs.viewBox, '0 0 200 200');
  assert.equal(svg.attrs.preserveAspectRatio, 'none');
  assert.equal(svg.style.position, 'fixed');
  assert.equal(svg.style.pointerEvents, 'none');
  assert.equal(svg.style.zIndex, '20');
  assert.deepEqual([svg.style.left, svg.style.top, svg.style.width, svg.style.height], ['10px', '20px', '100px', '100px']);
  const outline = svg.children.find((c) => c.tag === 'path'), box = svg.children.find((c) => c.tag === 'polygon');
  assert.match(outline.attrs.d, /^M 85 83 C/);
  assert.equal(outline.attrs.stroke, 'rgb(74,158,255)');
  // world (40,40)-(80,90) through [2 0 0 2 5 3]
  assert.deepEqual(box.attrs.points.split(' ').map(num), [[85, 83], [165, 83], [165, 183], [85, 183]]);
  assert.equal(box.attrs['vector-effect'], 'non-scaling-stroke');
  const handles = svg.children.filter((c) => c.tag === 'rect');
  assert.equal(handles.length, 8);
  // 3.5 screen px at 100 px for 200 canvas units = 7 units half-size; centres: nw, n, ne, e, se, s, sw, w
  const centres = handles.map((r) => [Number(r.attrs.x) + Number(r.attrs.width) / 2, Number(r.attrs.y) + Number(r.attrs.height) / 2]);
  assert.deepEqual(centres, [[85, 83], [125, 83], [165, 83], [165, 133], [165, 183], [125, 183], [85, 183], [85, 133]]);
  assert.ok(handles.every((r) => Number(r.attrs.width) === 14 && r.attrs.fill === '#ffffff' && r.attrs.stroke === 'rgb(74,158,255)' && r.attrs['stroke-width'] === '1.2'));
});

test('a rotated shape draws its oriented box, not the axis-aligned union', () => {
  const h = harness();
  const quad = h.quad('r', SQUARE, { boxAngle: 30 });
  quad.rotate(30, quad.bounds.center);
  Intent.applyPick(h.root, quad, false);
  const box = h.svgs()[0].children.find((c) => c.tag === 'polygon'), pts = box.attrs.points.split(' ').map(num);
  const side = (a, b) => Math.hypot(pts[b][0] - pts[a][0], pts[b][1] - pts[a][1]);
  assert.ok(Math.abs(side(0, 1) - 40) < 1e-6 && Math.abs(side(1, 2) - 50) < 1e-6, pts.join('|'));
  assert.ok(Math.abs(Math.atan2(pts[1][1] - pts[0][1], pts[1][0] - pts[0][0]) * 180 / Math.PI - 30) < 1e-6);
});

test('Shift toggles an item in and out of the overlay; an empty pick clears it, Shift keeps it', () => {
  const h = harness(), a = h.quad('a', SQUARE), b = h.quad('b', [[100, 100], [150, 100], [150, 150], [100, 150]]);
  Intent.applyPick(h.root, a, false);
  Intent.applyPick(h.root, b, true);
  assert.equal(h.svgs().length, 1);
  assert.equal(h.svgs()[0].children.filter((c) => c.tag === 'path').length, 2);
  assert.equal(h.svgs()[0].children.find((c) => c.tag === 'polygon').attrs.points, '40,40 150,40 150,150 40,150');
  Intent.applyPick(h.root, h.quad('a', SQUARE), true);
  assert.equal(h.svgs()[0].children.filter((c) => c.tag === 'path').length, 1);
  Intent.applyPick(h.root, null, true);
  assert.equal(h.svgs().length, 1);
  Intent.applyPick(h.root, null, false);
  assert.equal(h.svgs().length, 0);
  assert.equal(h.body.children.length, 0);
});

test('no overlay for an empty selection or without a native presentation', () => {
  const h = harness();
  Overlay.show(h.root);
  assert.equal(h.svgs().length, 0);
  h.root.NemoNativeObjectProject = { desktop: () => null };
  Intent.applyPick(h.root, h.quad('a', SQUARE), false);
  assert.equal(h.svgs().length, 0);
});

test('a stale tool, frame, layer, revision or selection removes the overlay', () => {
  const changes = [(h) => { h.root.state.tool = 'draw'; }, (h) => { h.root.state.currentFrame = 3; }, (h) => { h.root.state.activeLayerIdx = 1; },
    (h) => { h.root.state.appMode = 'motion'; }, (h) => h.setPin({ contentRevision: 1, documentSnapshotId: 'native-object:d:1' }),
    (h) => h.setPin({ documentId: 'replacement' }), (h) => { h.root.selectedPaths = []; }, (h) => { h.root.NemoNativeObjectProject = { desktop: () => null }; }];
  for (const change of changes) {
    const h = harness();
    Intent.applyPick(h.root, h.quad('a', SQUARE), false);
    const [svg] = h.svgs();
    assert.ok(svg);
    change(h);
    Overlay.notify(h.root);
    h.tick();
    assert.equal(h.svgs().length, 0, String(change));
    assert.equal(svg.removed, true);
  }
});

test('pan and zoom redraw the same overlay in place on a change event; an unchanged view is untouched', () => {
  const h = harness();
  Intent.applyPick(h.root, h.quad('a', SQUARE), false);
  const first = h.svgs()[0];
  Overlay.notify(h.root);
  h.tick();
  assert.equal(h.svgs()[0], first, 'unchanged view is not rebuilt');
  h.setView([2, 0, 0, 2, 0, 0]);
  Overlay.notify(h.root);
  h.tick();
  assert.equal(h.svgs().length, 1);
  assert.equal(h.svgs()[0].children.find((c) => c.tag === 'polygon').attrs.points, '80,80 160,80 160,180 80,180');
  h.root.selectedPaths = [];
  Overlay.notify(h.root);
  h.tick();
  assert.equal(h.svgs().length, 0);
});

test('an idle overlay queues no frame work; a change event queues exactly one frame, coalesced, never re-armed (#2372 REV-7)', () => {
  const h = harness();
  Intent.applyPick(h.root, h.quad('a', SQUARE), false);
  assert.equal(h.frames.length, 0, 'drawing a selection arms no watcher');
  for (let i = 0; i < 5; i++) h.tick();
  assert.equal(h.frames.length, 0, 'ticks with nothing changed do no work');
  Overlay.notify(h.root); Overlay.notify(h.root); Overlay.notify(h.root);
  assert.equal(h.frames.length, 1, 'repeated events coalesce into one frame callback');
  let captures = 0;
  const I = h.root.NemoSelectCanvasIntent;
  h.root.NemoSelectCanvasIntent = { ...I, presentedState: (r) => { captures++; return I.presentedState(r); } };
  h.tick();
  assert.equal(captures, 1);
  assert.equal(h.frames.length, 0, 'the frame callback does not re-arm itself');
  h.tick(); h.tick();
  assert.equal(captures, 1, 'no polling while nothing changes');
  Overlay.clear(h.root);
  Overlay.notify(h.root);
  assert.equal(h.frames.length, 0, 'nothing drawn, nothing queued');
});

test('a window resize and a viewport-underlay presentation both notify the overlay', () => {
  const h = harness(), listeners = {};
  h.root.addEventListener = (type, fn) => { listeners[type] = fn; };
  Intent.applyPick(h.root, h.quad('a', SQUARE), false);
  assert.equal(typeof listeners.resize, 'function');
  h.rect.width = 200;
  listeners.resize();
  h.tick();
  assert.equal(h.svgs()[0].style.width, '200px');
  const Underlay = require('../src/js/adapters/native-viewport-underlay.js');
  const calls = [];
  h.root.NemoNativeSelectOverlay = { notify: (r) => calls.push(r) };
  const desktop = Underlay.desktop({ ...h.root, document: { documentElement: null, getElementById: () => ({}), body: h.body } });
  assert.equal(typeof desktop.surfaceChanged, 'function');
  try { desktop.surfaceChanged(null); } catch (_) { /* the underlay port is not under test */ }
  assert.equal(calls.length, 1, 'a surface change notifies the Select overlay on its root');
});

test('hover shows a 1.5px box for a picked, unselected path and not for a selected one', () => {
  const h = harness(), a = h.quad('a', SQUARE), b = h.quad('b', [[100, 100], [150, 100], [150, 150], [100, 150]]);
  Overlay.hover(h.root, a);
  const hover = h.svgs()[0].children.find((c) => c.tag === 'polygon');
  assert.equal(hover.attrs['stroke-width'], '1.5');
  assert.equal(hover.attrs.points, '40,40 80,40 80,90 40,90');
  assert.equal(h.svgs()[0].children.filter((c) => c.tag === 'rect').length, 0, 'a hover box carries no handles');
  Intent.applyPick(h.root, a, false);
  assert.equal(h.svgs()[0].children.filter((c) => c.tag === 'polygon').length, 1, 'show() drops the hover');
  Overlay.hover(h.root, h.quad('a', SQUARE));
  assert.equal(h.svgs()[0].children.filter((c) => c.tag === 'polygon').length, 1, 'the selected path keeps only its selection box');
  Overlay.hover(h.root, b);
  assert.equal(h.svgs()[0].children.filter((c) => c.tag === 'polygon').length, 2);
  Overlay.hover(h.root, null);
  assert.equal(h.svgs()[0].children.filter((c) => c.tag === 'polygon').length, 1);
});

test('hoverAt coalesces picks and discards one whose context changed; a button press or other tool clears it', async () => {
  const h = harness(), a = h.quad('a', SQUARE), reads = [];
  let release;
  h.root.NemoSelectCanvasIntent = { ...Intent, pick: (_root, x) => { reads.push(x); return new Promise((done) => { release = () => done(a); }); } };
  Overlay.hoverAt(h.root, { clientX: 1, clientY: 1, buttons: 0 });
  Overlay.hoverAt(h.root, { clientX: 2, clientY: 1, buttons: 0 });
  Overlay.hoverAt(h.root, { clientX: 3, clientY: 1, buttons: 0 });
  assert.deepEqual(reads, [1], 'one read in flight at a time');
  h.root.state.currentFrame = 4;
  release();
  await new Promise((done) => setImmediate(done));
  assert.equal(h.svgs().length, 0, 'a read from another context is discarded');
  assert.deepEqual(reads, [1, 3], 'the latest position is read next, skipped ones are not');
  release();
  await new Promise((done) => setImmediate(done));
  assert.equal(h.svgs().length, 1);
  Overlay.hoverAt(h.root, { clientX: 3, clientY: 1, buttons: 1 });
  assert.equal(h.svgs().length, 0, 'a pressed button clears the hover');
  Overlay.hover(h.root, a);
  h.root.state.tool = 'draw';
  Overlay.hoverAt(h.root, { clientX: 3, clientY: 1, buttons: 0 });
  assert.equal(h.svgs().length, 0);
});

test('the pointer intent reports hover only while no gesture is pending and hides it on press', () => {
  const calls = [];
  const intent = Intent.create({ context: () => 'c', probe: () => Object.freeze({ mode: 'unavailable' }), select() {}, hover: (e) => calls.push(e && e.clientX) });
  const ev = (x, extra) => ({ clientX: x, clientY: 0, pointerId: 1, button: 0, buttons: 0, stopImmediatePropagation() {}, preventDefault() {}, currentTarget: null, ...extra });
  intent.move(ev(5));
  intent.down(ev(6));
  intent.move(ev(7, { buttons: 1 }));
  intent.up(ev(7));
  intent.move(ev(8));
  assert.deepEqual(calls, [5, null, 8]);
  const quiet = Intent.create({ context: () => 'c', probe: () => null, select() {} });
  assert.doesNotThrow(() => { quiet.move(ev(1)); quiet.down(ev(1)); });
});

test('a throwing hover read does not disable later hovers (#2372 REV-7)', async () => {
  const h = harness(), a = h.quad('a', SQUARE);
  let throwNow = true;
  h.root.NemoSelectCanvasIntent = { ...Intent, pick: async () => a,
    pickContext: (r) => { if (throwNow) { throwNow = false; throw new Error('context read failed'); } return Intent.pickContext(r); } };
  Overlay.hoverAt(h.root, { clientX: 1, clientY: 1, buttons: 0 });
  await new Promise((done) => setImmediate(done));
  assert.equal(h.svgs().length, 0);
  Overlay.hoverAt(h.root, { clientX: 2, clientY: 1, buttons: 0 });
  await new Promise((done) => setImmediate(done));
  assert.equal(h.svgs().length, 1, 'the next hover is read after the failed one');
  // A throw inside the redraw itself is contained the same way.
  Overlay.hover(h.root, null);
  const create = h.root.document.createElementNS;
  let failing = true;
  h.root.document.createElementNS = (ns, tag) => { if (failing) { failing = false; throw new Error('svg failed'); } return create(ns, tag); };
  Overlay.hoverAt(h.root, { clientX: 3, clientY: 1, buttons: 0 });
  await new Promise((done) => setImmediate(done));
  Overlay.hoverAt(h.root, { clientX: 4, clientY: 1, buttons: 0 });
  await new Promise((done) => setImmediate(done));
  assert.equal(h.svgs().length, 1, 'hover still works after a throwing redraw');
});
