'use strict';
// B-eraser-tool: the native Eraser adapter. Real: the adapter, the group intent, the edit contract and the common v2 SDK (so every
// request and receipt is validated, including the joinHistory disposition) and the shared presentation wait. Simulated: the DOM, the
// presented layer read (hit geometry) and the owner's replies. No installed-app claim.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { createHarness } = require('./native-object-edit-harness.cjs');
const Eraser = require('../src/js/adapters/native-object-eraser-tool.js');
const Group = require('../src/js/adapters/native-object-group-intent.js');
const Contract = require('../src/js/adapters/native-object-edit-contract.js');
const Wait = require('../src/js/adapters/native-presentation-wait.js');
const ROOT = path.resolve(__dirname, '..');

class El {
  constructor(tag) { this.tag = tag; this.attributes = {}; this.style = {}; this.children = []; this.captured = new Set(); this.listeners = {}; }
  setAttribute(k, v) { this.attributes[k] = String(v); } appendChild(c) { this.children.push(c); c.parent = this; return c; }
  get firstChild() { return this.children[0] || null; } remove() { if (this.parent) this.parent.children.splice(this.parent.children.indexOf(this), 1); this.parent = null; }
  setPointerCapture(id) { this.captured.add(id); } releasePointerCapture(id) { this.captured.delete(id); }
  addEventListener(k, fn, o) { (this.listeners[k] = this.listeners[k] || []).push({ fn, capture: !!(o && o.capture) }); }
}
class Pt { constructor(x, y) { this.x = x; this.y = y; } }
// A rectangular "path": hitTest answers by its bounds grown by the tolerance, as Paper's hitTest does for a filled shape.
function shape(id, x0, y0, x1, y1, extra = {}) {
  return { data: { strokeId: id, nativeMatrix: [1, 0, 0, 1, 0, 0], nativeRole: 'shape', ...extra },
    hitTest(at, o) { const t = o.tolerance || 0; return at.x >= x0 - t && at.x <= x1 + t && at.y >= y0 - t && at.y <= y1 + t ? { item: this } : null; } };
}
async function fixture(options = {}) {
  const log = [], reads = [], toasts = [];
  const h = createHarness({ strokes: {}, sessionStyle: true, step: (request) => {
    log.push(structuredClone(request));
    if (options.refuse && options.refuse(request, log.length)) return { error: { code: 'unavailable', message: 'refused' } };
    const applied = options.applied ? options.applied(request, log.length) : true;
    return { applied, result: { applied, historyEntriesAdded: applied ? (request.payload.joinHistory === true ? 0 : 1) : 0 } };
  } });
  const flags = { locked: false, visible: true, facts: {} };
  const canvas = new El('canvas'), area = new El('div'), body = new El('body');
  canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 640, height: 360 });
  const fields = { 'p-erasersize': { value: '14' } };
  const doc = { body, documentElement: { clientWidth: 1000, clientHeight: 800, classList: { contains: () => true } },
    getElementById: (id) => ({ 'drawing-canvas': canvas, 'canvas-area': area }[id] || fields[id] || null), createElementNS: (ns, tag) => new El(tag) };
  const session = { blocksLegacy: () => true, isActive: () => true, requestId: (l) => h.ports.requestId(l),
    info: () => ({ ...h.ports.pin(), canvasW: 640, canvasH: 360, lifecycleGeneration: 1, layers: [{ layerUid: 'layer-a', locked: flags.locked, visible: flags.visible, ...flags.facts }] }),
    readPresentedObjectSession(why) { if (options.busy && options.busy()) { if (why) why('queue-pending'); return null; } const pin = session.info(); return { ...pin, presentation: { ...pin, owner: 'native', status: 'presented', frame: root.state.currentFrame } }; } };
  const world = options.shapes || [shape('s1', 100, 100, 300, 200)];
  const root = { document: doc, Point: Pt, state: { tool: 'eraser', appMode: 'anim2d', playing: false, currentFrame: 0, activeLayerIdx: 0, eraserSize: 14, stabilizer: 0, pressureMin: 5, pressureMax: 170,
      layers: [{ layerUid: 'layer-a' }] },
    showToast: (t) => toasts.push(t), setTimeout, Date,
    NemoNativeObjectProject: { desktop: () => session }, NemoNativeObjectVectorStroke: require('../src/js/adapters/native-object-vector-stroke.js'), NemoNativePresentationWait: Wait, NemoNativeObjectGroupIntent: Group, NemoNativeObjectEditContract: Contract,
    NemoNativeObjectComponentIntent: { portsFromSession: () => ({ ...h.ports, readDeclared: () => true }) },
    NemoSelectCanvasIntent: { async layerPaths(r, wanted, centres, idx) {
      reads.push(idx); const paths = typeof world === 'function' ? world() : world;
      return { layerUid: 'layer-a', frame: root.state.currentFrame, paths: paths.filter((p) => wanted({ role: p.data.nativeRole || 'shape' }, p.bounds || [-1e9, -1e9, 1e9, 1e9])) }; } } };
  root.window = root;
  const mapping = { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0], matchesView: () => true };
  const tool = Eraser.create(root, { mapping });
  const ev = (x, y, extra = {}) => ({ pointerId: 1, isPrimary: true, button: 0, clientX: x, clientY: y, target: area, stopped: false, prevented: false,
    stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; }, ...extra });
  return { tool, root, h, log, toasts, reads, flags, area, canvas, body, fields, session, ev,
    async send(kind, x, y, extra) { const e = ev(x, y, extra); const handled = tool.handle(kind, e); await tool.whenSettled(); await new Promise(setImmediate); return { handled, e }; } };
}
const bbox = (points) => ({ x0: Math.min(...points.map((p) => p[0])), x1: Math.max(...points.map((p) => p[0])), y0: Math.min(...points.map((p) => p[1])), y1: Math.max(...points.map((p) => p[1])) });

test('the press bites a circle of eraserSize/2 on the top-most hit and the request is the declared closed erase', async () => {
  const f = await fixture();
  const down = await f.send('down', 150, 150);
  assert.equal(down.handled, true); assert.equal(down.e.stopped, true, 'the press is consumed');
  assert.equal(f.log.length, 1);
  const r = f.log[0];
  assert.equal(r.operation, 'command.document.object.boolean.erase');
  assert.deepEqual(Object.keys(r.payload).sort(), ['command', 'contextId', 'footprint', 'frame', 'layerUid', 'strokeId']);
  assert.deepEqual([r.payload.layerUid, r.payload.contextId, r.payload.frame, r.payload.strokeId], ['layer-a', 'scene-root', 0, 's1']);
  const b = bbox(r.payload.footprint);
  assert.ok(Math.abs(b.x0 - 143) < 0.01 && Math.abs(b.x1 - 157) < 0.01 && Math.abs(b.y0 - 143) < 0.01 && Math.abs(b.y1 - 157) < 0.01, JSON.stringify(b));
  assert.ok(r.payload.footprint.length >= 8 && r.payload.footprint.length <= 96);
  assert.equal(r.payload.joinHistory, undefined, 'the first bite adds the gesture\'s entry');
});

test('a drag bites the swept capsule from the previous sample and every bite after the first joins the gesture entry: one undo step', async () => {
  const f = await fixture();
  await f.send('down', 120, 150); await f.send('move', 160, 150); await f.send('move', 200, 150); await f.send('up', 200, 150);
  assert.equal(f.log.length, 3);
  assert.deepEqual(f.log.map((r) => r.payload.joinHistory), [undefined, true, true]);
  const second = bbox(f.log[1].payload.footprint);
  assert.ok(Math.abs(second.x0 - 113) < 0.01 && Math.abs(second.x1 - 167) < 0.01 && Math.abs(second.y0 - 143) < 0.01 && Math.abs(second.y1 - 157) < 0.01, 'capsule 120..160 at radius 7: ' + JSON.stringify(second));
  assert.deepEqual(f.log.map((r) => r.expectedRevision), [7, 8, 9], 'a revision per bite, each pinned to the last');
  const entries = f.log.map((r) => (r.payload.joinHistory === true ? 0 : 1)).reduce((a, n) => a + n, 0);
  assert.equal(entries, 1, 'the whole drag is a single history entry');
});

test('a bite that changes nothing does not join, and a miss sends nothing', async () => {
  const f = await fixture({ applied: (request, n) => n > 1 });
  await f.send('down', 150, 150); await f.send('move', 160, 150); await f.send('move', 170, 150);
  assert.deepEqual(f.log.map((r) => r.payload.joinHistory), [undefined, undefined, true], 'the entry only exists once a bite applied');
  const miss = await fixture();
  await miss.send('down', 500, 300); await miss.send('move', 520, 300);
  assert.equal(miss.log.length, 0, 'nothing under the pointer: no request, no revision');
});

test('an exact hit wins over a tolerance hit on another item; a near miss is caught within max(8, radius) of the pointer', async () => {
  const f = await fixture({ shapes: [shape('under', 100, 100, 200, 200), shape('near', 205, 100, 260, 200)] });
  await f.send('down', 198, 150);
  assert.equal(f.log[0].payload.strokeId, 'under', 'the shape squarely under the cursor, not the closer-edged neighbour');
  const g = await fixture({ shapes: [shape('only', 100, 100, 200, 200)] });
  await g.send('down', 206, 150);
  assert.equal(g.log[0].payload.strokeId, 'only', '6 px off its edge, inside the 8 px tolerance');
  const far = await fixture({ shapes: [shape('only', 100, 100, 200, 200)] });
  await far.send('down', 215, 150);
  assert.equal(far.log.length, 0);
});

test('an undo or any other edit under the gesture ends it with no further write', async () => {
  const f = await fixture();
  await f.send('down', 120, 150); await f.send('move', 140, 150);
  assert.equal(f.log.length, 2);
  f.h.state.revision += 1; // the document moved on (an undo, another edit)
  await f.send('move', 160, 150); await f.send('move', 180, 150); await f.send('up', 180, 150);
  assert.equal(f.log.length, 2, 'zero revisions after the change');
  assert.equal(f.tool.isDragging(), false);
});

test('scrubbing to another frame mid-gesture ends it, and a tool switch hides the ring', async () => {
  const f = await fixture();
  await f.send('down', 120, 150);
  f.root.state.currentFrame = 3;
  await f.send('move', 140, 150);
  assert.equal(f.log.length, 1);
  const g = await fixture();
  await g.send('move', 120, 150);
  assert.ok(g.tool.ringElement(), 'hover shows the ring');
  g.root.state.tool = 'draw';
  const handled = await g.send('move', 125, 150);
  assert.equal(handled.handled, false); assert.equal(g.tool.ringElement(), null);
});

test('a locked or hidden layer is refused out loud with the baseline wording and nothing is sent', async () => {
  const f = await fixture();
  f.flags.locked = true;
  await f.send('down', 150, 150);
  assert.deepEqual(f.toasts, ['Calque verrouillé — déverrouille-le (cadenas) pour dessiner dessus']);
  const g = await fixture();
  g.flags.visible = false;
  await g.send('down', 150, 150);
  assert.equal(g.toasts.length, 1);
  assert.equal(f.log.length + g.log.length, 0);
});

test('the hover ring follows the pointer at the eraser radius, with no press and no write', async () => {
  const f = await fixture();
  const move = await f.send('move', 300, 120);
  assert.equal(move.handled, true);
  const ring = f.tool.ringElement(), circle = ring.firstChild;
  assert.deepEqual([circle.attributes.cx, circle.attributes.cy, circle.attributes.r], ['300', '120', '7']);
  f.root.state.eraserSize = 40;
  await f.send('move', 310, 120);
  assert.equal(f.tool.ringElement().firstChild.attributes.r, '20');
  assert.equal(f.log.length, 0);
});

test('Alt+drag resizes the eraser (0.5 per pixel, floor 2) with the baseline toast and writes nothing', async () => {
  const f = await fixture();
  await f.send('down', 100, 100, { altKey: true });
  await f.send('move', 160, 100, { altKey: true });
  assert.equal(f.root.state.eraserSize, 44);
  assert.equal(f.fields['p-erasersize'].value, 44);
  await f.send('move', -400, 100, { altKey: true });
  assert.equal(f.root.state.eraserSize, 2);
  await f.send('up', -400, 100, { altKey: true });
  assert.equal(f.toasts[f.toasts.length - 1], 'Taille de la gomme : 2px');
  assert.equal(f.log.length, 0);
});

test('a refused shape is said once and the sweep goes on; a stale revision ends the gesture', async () => {
  const f = await fixture({ refuse: (request, n) => n === 1 });
  await f.send('down', 120, 150); await f.send('move', 140, 150); await f.send('move', 160, 150);
  assert.deepEqual(f.toasts, ['La gomme ne peut pas couper cette forme.'], 'one notice for a run of refusals');
  assert.equal(f.log.length, 3);
  const g = await fixture({ refuse: () => true });
  g.h.state.revision = 7;
  await g.send('down', 120, 150); await g.send('move', 140, 150); await g.send('move', 160, 150); await g.send('move', 180, 150);
  assert.equal(g.toasts.length, 1, 'repeated refusals inside 1.5 s are one notice');
});

test('a busy native queue is waited out instead of dropping the bite', async () => {
  let busy = true;
  const f = await fixture({ busy: () => busy });
  f.tool.handle('down', f.ev(150, 150));
  await new Promise((r) => setTimeout(r, 90));
  assert.equal(f.log.length, 0);
  busy = false;
  await f.tool.whenSettled();
  assert.equal(f.log.length, 1);
});

test('the capsule is the swept stadium, a circle for the first sample, and maps into a record\'s own space', () => {
  const circle = Eraser.capsule(null, [10, 10], 5), b = bbox(circle);
  assert.ok(Math.abs(b.x0 - 5) < 1e-6 && Math.abs(b.x1 - 15) < 1e-6 && circle.length % 2 === 0);
  const cap = Eraser.capsule([0, 0], [40, 0], 5), c = bbox(cap);
  assert.ok(Math.abs(c.x0 + 5) < 1e-6 && Math.abs(c.x1 - 45) < 1e-6 && Math.abs(c.y0 + 5) < 1e-6 && Math.abs(c.y1 - 5) < 1e-6);
  assert.deepEqual(Eraser.capsule([0, 0], [0.2, 0], 5).length, circle.length, 'a sweep under half a unit is a circle');
  const moved = Eraser.inRecord([[10, 20], [30, 40], [50, 20]], [2, 0, 0, 2, 4, 6]);
  assert.deepEqual(moved, [[3, 7], [13, 17], [23, 7]]);
  assert.equal(Eraser.inRecord([[0, 0]], [0, 0, 0, 0, 0, 0]), null);
});

test('the stabilizer averages the last samples and a pen\'s pressure scales the radius', async () => {
  const f = await fixture({ shapes: [shape('wide', -1000, -1000, 1000, 1000)] });
  f.root.state.stabilizer = 1;
  await f.send('down', 100, 100); await f.send('move', 130, 100);
  const centre = (r) => { const b = bbox(r.payload.footprint); return (b.x0 + b.x1) / 2; };
  assert.ok(Math.abs(centre(f.log[0]) - 100) < 0.5);
  assert.ok(Math.abs(centre(f.log[1]) - 107.5) < 0.5, 'capsule from 100 to the 2-sample average 115');
  const g = await fixture({ shapes: [shape('wide', -1000, -1000, 1000, 1000)] });
  await g.send('down', 100, 100, { pointerType: 'pen', pressure: 0.5 });
  const b = bbox(g.log[0].payload.footprint);
  assert.ok(Math.abs((b.x1 - b.x0) / 2 - 7 * (0.05 + (1.7 - 0.05) * 0.5)) < 0.05, 'radius 7 x (lo + (hi - lo) x pressure)');
});

test('eraser-bridge.js hands every native pointer event to the adapter and refuses out loud without it', () => {
  const src = fs.readFileSync(path.join(ROOT, 'src/js/eraser-bridge.js'), 'utf8');
  const handlers = {}, toasts = [];
  const area = { addEventListener(type, fn) { handlers[type] = fn; } };
  const calls = [];
  const context = { state: { tool: 'eraser', layers: [{ locked: false }], activeLayerIdx: 0, playing: false, eraserSize: 14 }, document: { readyState: 'complete', addEventListener() {}, getElementById: () => area },
    showToast: (t) => toasts.push(t), NemoNativeViewportUnderlay: { canvasMapping: {} },
    NemoNativeObjectProject: { desktop: () => ({ blocksLegacy: () => true }) }, SMEngineBridge: { isEnabled: () => true, nativeEditGuard: { allow: () => false }, suspend() { throw new Error('legacy suspended'); } } };
  context.window = context; vm.createContext(context);
  vm.runInContext(src, context);
  const event = (extra) => ({ target: area, pointerId: 1, stopImmediatePropagation() { this.stopped = true; }, preventDefault() {}, ...extra });
  const press = event({ type: 'pointerdown' });
  handlers.pointerdown(press);
  assert.equal(press.stopped, true, 'consumed');
  assert.deepEqual(toasts, ['La gomme n’est pas disponible sur ce projet pour le moment.']);
  context.NemoNativeObjectEraserTool = { create: () => ({ handle: (kind) => { calls.push(kind); return true; } }) };
  handlers.pointermove(event({ type: 'pointermove' })); handlers.pointerup(event({ type: 'pointerup' })); handlers.pointercancel(event({ type: 'pointercancel' }));
  handlers.pointerdown(event({ type: 'pointerdown' }));
  assert.deepEqual(calls, ['move', 'up', 'cancel', 'down']);
});

// R-lane-a-fixes: the baseline eraser-bridge has no mode check (feedback #135 maps the point through the layer's Motion), so Motion bites like Animation 2D.
// The hit's presented matrix carries the layer's Motion (position, rotation, scale) into the record's own space, whatever the mode.
test('Motion mode erases too: the footprint reaches the record through the hit\'s presented matrix, one revision and one undo entry per gesture', async () => {
  // The layer is moved by (+40, -20) and turned a quarter: world = M * local, so the footprint is sent as M^-1 * world.
  const matrix = [0, 1, -1, 0, 40, -20];
  const f = await fixture({ shapes: [shape('s1', 100, 100, 300, 200, { nativeMatrix: matrix })] });
  f.root.state.appMode = 'motion';
  await f.send('down', 150, 150);
  assert.equal(f.log.length, 1, f.toasts.join('|'));
  assert.deepEqual([f.log[0].payload.layerUid, f.log[0].payload.frame, f.log[0].payload.strokeId], ['layer-a', 0, 's1']);
  const b = bbox(f.log[0].payload.footprint);
  // world (150, 150) -> local (x, y): x' = -y + 40, y' = x - 20  =>  y = 40 - 150 = -110, x = 150 + 20 = 170
  assert.ok(Math.abs((b.x0 + b.x1) / 2 - 170) < 0.01 && Math.abs((b.y0 + b.y1) / 2 + 110) < 0.01, JSON.stringify(b));
  await f.send('move', 160, 150);
  assert.equal(f.log.length, 2);
  assert.equal(f.log[1].payload.joinHistory, true, 'the rest of the drag continues the gesture entry');
  await f.send('up', 160, 150);
  assert.equal(f.toasts.length, 0);
});

test('Motion mode: Alt+drag resizes the eraser, the hover ring follows, and storyboard or playback stay refused', async () => {
  const f = await fixture();
  f.root.state.appMode = 'motion';
  await f.send('down', 100, 100, { altKey: true });
  await f.send('move', 160, 100, { altKey: true });
  await f.send('up', 160, 100, { altKey: true });
  assert.equal(f.root.state.eraserSize, 44);
  assert.equal(f.log.length, 0);
  await f.send('move', 200, 120);
  assert.ok(f.tool.ringElement(), 'the ring is drawn in Motion');
  const g = await fixture();
  g.root.state.appMode = 'storyboard';
  await g.send('down', 150, 150);
  assert.equal(g.log.length, 0);
  assert.deepEqual(g.toasts, ['La gomme n’est disponible qu’en Animation 2D et en Motion, hors lecture.']);
});

test('the refusals follow the baseline editRefusalReason: component, duplicator, path effects, locked, then video and montage, each with its wording', async () => {
  const cases = [
    [{ kind: 'component' }, 'Calque composant — double-clique dessus pour entrer dedans et dessiner'],
    [{ hasDuplicator: true }, 'Calque duplicateur — « Modifier la forme source » (panneau Duplicator) pour éditer'],
    [{ pathFx: [{ type: 'zigzag', enabled: true }] }, 'Effets de tracé actifs — désactive-les (panneau Effets) pour redessiner dessus'],
    [{ pathFxOversize: true }, 'Effets de tracé actifs — désactive-les (panneau Effets) pour redessiner dessus'],
    [{ kind: 'video' }, 'Calque vidéo — dessine sur un calque normal au-dessus, ou active Masque pour masquer directement cette vidéo'],
    [{ kind: 'montage' }, 'Calque montage — son contenu s’édite dans le StoryBoard'],
  ];
  for (const [facts, want] of cases) {
    const f = await fixture();
    f.flags.facts = facts;
    await f.send('down', 150, 150);
    assert.deepEqual(f.toasts, [want], JSON.stringify(facts));
    assert.equal(f.log.length, 0);
    // a duplicator and a component force-lock their layer: said before the lock, which would be the useless half of the truth
    const g = await fixture();
    g.flags.facts = facts;
    g.flags.locked = true;
    await g.send('down', 150, 150);
    const first = facts.kind === 'video' || facts.kind === 'montage' ? 'Calque verrouillé — déverrouille-le (cadenas) pour dessiner dessus' : want;
    assert.deepEqual(g.toasts, [first], 'locked + ' + JSON.stringify(facts));
  }
  const disabled = await fixture();
  disabled.flags.facts = { pathFx: [{ type: 'zigzag', enabled: false }] };
  await disabled.send('down', 150, 150);
  assert.equal(disabled.log.length, 1, 'a disabled path effect does not refuse');
  const edit = await fixture();
  edit.flags.facts = { hasDuplicator: true };
  edit.root.state.layers[0]._dupEditSource = true;
  await edit.send('down', 150, 150);
  assert.equal(edit.log.length, 1, 'while the duplicator\'s source is being edited the eraser works');
  const mask = await fixture();
  mask.flags.facts = { kind: 'video' };
  mask.root.state.maskMode = true;
  await mask.send('down', 150, 150);
  assert.equal(mask.log.length, 1, 'Mask mode may erase on a video layer');
});
