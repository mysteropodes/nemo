'use strict';
// #2372/#2256 X-shape-clipboard-duplicate: on a native object project Cmd+D / Dupliquer, Cmd+C / Copier, Cmd+X / Couper and Cmd+V / Coller send ONE native
// request each through the layer lifecycle intent (object.selection.duplicate, object.clipboard.copy / cut / paste) instead of the legacy Paper clone and
// clipboard writers, which edit Paper items a native project does not have. The real tools.js entry points run in a sandbox; the real lifecycle intent,
// component-intent ports, edit contract and select-canvas-intent run against a stub session client whose owner keeps a layer listing, hands back the clip a
// copy or cut makes and puts a pasted clip on the destination layer (12 px only over the cell it came from, as the engine does).
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Life = require('../src/js/adapters/native-object-layer-lifecycle-intent.js'), ComponentIntent = require('../src/js/adapters/native-object-component-intent.js'), EditContract = require('../src/js/adapters/native-object-edit-contract.js');
const Clipboard = require('../src/js/adapters/native-selection-clipboard.js');
const Arbiter = require('../src/js/application/keyboard-arbiter.js');
const Paper = require('./helpers/native-import-paper.cjs');

const read = (f) => fs.readFileSync(path.join(__dirname, '..', 'src', 'js', f), 'utf8');
const TOOLS = read('tools.js'), TIMELINE = read('timeline.js'), BRIDGE = read('select-bridge.js');
function extract(src, name) {
  const from = src.indexOf('function ' + name + '(');
  assert.ok(from >= 0, name);
  let depth = 0;
  for (let i = src.indexOf('{', from); i < src.length; i++) { if (src[i] === '{') depth++; else if (src[i] === '}' && --depth === 0) return src.slice(from, i + 1); }
  throw new Error('unterminated ' + name);
}
function el(tag) {
  return { tag, attrs: {}, style: {}, children: [], setAttribute(k, v) { this.attrs[k] = String(v); },
    appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); } };
}
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (layer, id, x0, y0, x1, y1, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: layer, frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 }, ...extra });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 14; i++) await tick(); };
const LAYERS = [{ layerUid: 'L1' }, { layerUid: 'L2' }];
const FLAT = () => [quad('L1', 'a', 10, 10, 40, 40), quad('L1', 'b', 30, 30, 70, 70), quad('L1', 'c', 60, 60, 90, 90), quad('L2', 'z', 0, 0, 20, 20)];
const SHIFT = 12;

// opts: native, route, answer(request, ctl), pad (bytes of padding on every clip record's geometry).
function harness(items = FLAT(), opts = {}) {
  const { native = true, route = true, adapter = true, pad = 0 } = opts;
  const paper = Paper.load(), toasts = [], sent = [], legacy = [], gate = { hold: null };
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 's7', canvasW: 200, canvasH: 200, layers: LAYERS };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const find = (id) => store.find((i) => i.stableTarget.strokeId === id);
  const bump = () => { pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) }; return pin.contentRevision; };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => native, requestId: (label) => label + '-' + (sent.length + 1),
    readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(selector, _pin, wanted) {
      return { items: store.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  let fresh = 0;
  const shifted = (item, by) => ({ ...item, bounds: item.bounds.map((n) => n + by), segments: item.segments.map((s) => ({ ...s, point: [s.point[0] + by, s.point[1] + by] })) });
  const clone = (item, layer, by, id) => { const c = shifted(JSON.parse(JSON.stringify(item)), by); c.stableTarget = { ...c.stableTarget, layerUid: layer, strokeId: id }; return c; };
  const clipOf = (p) => ({ contextId: p.contextId, layerUid: p.layerUid, frame: p.frame, records: p.strokeIds.map((id) => {
    const item = find(id);
    return { schemaVersion: 1, family: 'closed-solid-fill-cubic-path', target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: p.frame }, layerUid: p.layerUid, strokeId: id },
      geometry: { kind: 'cubic-path', closed: true, segments: item.segments, ...(pad ? { pad: 'x'.repeat(pad) } : {}) } };
  }) });
  const own = (request) => {
    const p = request.payload, op = request.operation;
    if (op.endsWith('selection.duplicate')) { p.strokeIds.forEach((id) => store.push(clone(find(id), p.layerUid, SHIFT, id + '-d' + (++fresh)))); return { applied: true, historyEntriesAdded: 1 }; }
    if (op.endsWith('clipboard.copy')) return { applied: false, historyEntriesAdded: 0, clipboard: p.strokeIds.length ? clipOf(p) : null };
    if (op.endsWith('clipboard.cut')) {
      const clip = p.strokeIds.length ? clipOf(p) : null;
      p.strokeIds.forEach((id) => store.splice(store.indexOf(find(id)), 1));
      return { applied: clip !== null, historyEntriesAdded: clip ? 1 : 0, clipboard: clip };
    }
    if (op.endsWith('clipboard.paste')) {
      const by = p.clip.layerUid === p.layerUid && p.clip.frame === p.frame ? SHIFT : 0;
      p.clip.records.forEach((r) => { const src = { stableTarget: r.target, closed: true, count: r.geometry.segments.length, role: 'plain', filled: true, segments: r.geometry.segments,
        bounds: [r.geometry.segments[0].point[0], r.geometry.segments[0].point[1], r.geometry.segments[2].point[0], r.geometry.segments[2].point[1]], matrix: [1, 0, 0, 1, 0, 0], style: {} };
        store.push(clone(src, p.layerUid, by, r.target.strokeId + '-p' + (++fresh))); });
      return { applied: true, historyEntriesAdded: 1 };
    }
    return { applied: true, historyEntriesAdded: 1 };
  };
  if (route) {
    session.runResourceCommand = async (action) => action({
      async dispatch(request) {
        sent.push(JSON.parse(JSON.stringify(request)));
        if (gate.hold) await gate.hold;
        if (opts.answer) return opts.answer(request, { bump });
        const before = pin.contentRevision, result = own(request);
        if (result.applied) bump();
        return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: pin.contentRevision === before || result.applied ? pin.contentRevision : before, ok: true, result };
      },
      async reconcile() {},
    });
  }
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: LAYERS, selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() { root.updates = (root.updates || 0) + 1; }, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    SM: { t: (key) => ({ toastCopied: 'Copied (', toastCut: 'Cut (', toastPasted: 'Pasted (', toastNothingToPaste: 'Nothing to paste' }[key] || key) },
    setTimeout: (fn) => fn, clearTimeout() {},
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag), body: el('body'), addEventListener() {}, removeEventListener() {} },
    requestAnimationFrame: () => {}, pushUndo() { legacy.push('pushUndo'); throw new Error('native authority denied legacy history checkpoint'); },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay,
    NemoNativeObjectLayerLifecycleIntent: Life, NemoNativeObjectComponentIntent: ComponentIntent, NemoNativeObjectEditContract: EditContract };
  if (adapter) root.NemoNativeSelectionClipboard = Clipboard;
  return { root, toasts, sent, legacy, store, gate, session, bump, find,
    async pick(...want) {
      const idx = Math.max(0, LAYERS.findIndex((l) => store.find((i) => i.stableTarget.strokeId === want[0] && i.stableTarget.layerUid === l.layerUid)));
      const layers = await Intent.layerPaths(root, (item) => want.indexOf(item.stableTarget.strokeId) >= 0, false, idx);
      layers.paths.sort((x, y) => want.indexOf(x.data.strokeId) - want.indexOf(y.data.strokeId));
      root.selectedPaths = layers.paths;
      return layers.paths;
    },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId), layer: (uid) => store.filter((i) => i.stableTarget.layerUid === uid).map((i) => i.stableTarget.strokeId) };
}

test('Dupliquer / Cmd+D is ONE object.selection.duplicate request over the selected ids in paint order; the clones become the selection', async () => {
  const h = harness();
  await h.pick('b', 'a');
  const out = await Clipboard.duplicate(h.root);
  assert.equal(out.status, 'applied');
  assert.equal(h.sent.length, 1, 'one request is one revision and one history entry');
  const [r] = h.sent;
  assert.equal(r.operation, 'command.document.object.selection.duplicate');
  assert.equal(r.expectedRevision, 7, 'pinned to the presented revision');
  assert.deepEqual(r.payload, { command: 'object.selection.duplicate', contextId: 'scene-root', layerUid: 'L1', frame: 0, mode: 'animation2d', strokeIds: ['a', 'b'] });
  assert.deepEqual(h.legacy, [], 'no legacy history checkpoint, no Paper clone');
  assert.deepEqual(h.layer('L1'), ['a', 'b', 'c', 'a-d1', 'b-d2'], 'the owner put the copies on top of the layer');
  assert.deepEqual(h.ids(), ['a-d1', 'b-d2'], 'the copies are selected, the sources are not (baseline)');
  assert.deepEqual(h.toasts, [], 'the baseline duplicate says nothing');
});

test('Motion mode sends the owner the "motion" edit mode (the key the held frame inherits is edited, not promoted)', async () => {
  const h = harness();
  h.root.state.appMode = 'motion';
  await h.pick('a');
  await Clipboard.duplicate(h.root);
  assert.equal(h.sent[0].payload.mode, 'motion');
});

test('Cmd+C is ONE object.clipboard.copy request that writes nothing; Cmd+V then pastes the owner\'s clip once, nudged only over its own cell', async () => {
  const h = harness();
  await h.pick('a', 'c');
  assert.equal(Clipboard.hasClip(h.root), false);
  const copied = await Clipboard.copy(h.root);
  assert.deepEqual(copied, { status: 'copied', count: 2 });
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, 'command.document.object.clipboard.copy');
  assert.deepEqual(h.sent[0].payload, { command: 'object.clipboard.copy', contextId: 'scene-root', layerUid: 'L1', frame: 0, mode: 'animation2d', strokeIds: ['a', 'c'] });
  assert.equal(h.sent[0].expectedRevision, 7);
  assert.equal(h.session.info().contentRevision, 7, 'copy changes no revision');
  assert.deepEqual(h.toasts, ['Copied (2)']);
  assert.equal(Clipboard.hasClip(h.root), true);
  assert.equal(h.root._lastClipKind, 'canvas', 'the keyboard arbiter pastes the clipboard filled last');
  assert.deepEqual(h.legacy, []);
  // over the cell it came from: the owner nudges
  const same = await Clipboard.paste(h.root);
  assert.equal(same.status, 'applied');
  assert.equal(h.sent.length, 2);
  assert.equal(h.sent[1].operation, 'command.document.object.clipboard.paste');
  assert.deepEqual({ ...h.sent[1].payload, clip: undefined }, { command: 'object.clipboard.paste', contextId: 'scene-root', layerUid: 'L1', frame: 0, mode: 'animation2d', clip: undefined });
  assert.deepEqual(h.sent[1].payload.clip.records.map((r) => r.target.strokeId), ['a', 'c']);
  assert.equal(h.sent[1].expectedRevision, 7);
  assert.deepEqual(h.layer('L1'), ['a', 'b', 'c', 'a-p1', 'c-p2']);
  assert.equal(h.find('a-p1').bounds[0], 10 + SHIFT, 'the owner offsets a paste over its source by 12 px');
  assert.deepEqual(h.ids(), ['a-p1', 'c-p2'], 'the pasted shapes are selected');
  assert.deepEqual(h.toasts.slice(-1), ['Pasted (2)']);
  // another layer: lands exactly in place
  h.root.state.activeLayerIdx = 1;
  await Clipboard.paste(h.root);
  assert.equal(h.sent[2].payload.layerUid, 'L2');
  assert.deepEqual(h.layer('L2'), ['z', 'a-p3', 'c-p4']);
  assert.equal(h.find('a-p3').bounds[0], 10, 'a paste elsewhere lands in place');
  assert.deepEqual(h.legacy, []);
});

test('Cmd+X is ONE object.clipboard.cut request: the shapes leave in one revision, the clip stays for a paste, the selection is empty', async () => {
  const h = harness();
  await h.pick('b');
  const out = await Clipboard.cut(h.root);
  assert.equal(out.status, 'applied');
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, 'command.document.object.clipboard.cut');
  assert.deepEqual(h.layer('L1'), ['a', 'c']);
  assert.deepEqual(h.ids(), []);
  assert.deepEqual(h.toasts, ['Cut (1)']);
  assert.equal(Clipboard.hasClip(h.root), true);
  await Clipboard.paste(h.root);
  assert.deepEqual(h.layer('L1'), ['a', 'c', 'b-p1']);
  assert.equal(h.find('b-p1').bounds[0], 30 + SHIFT, 'the clip remembers the cell it came from, so the owner nudges by the baseline same-place rule even after a cut');
});

test('a paste with nothing copied says so and sends nothing; copy and cut with no selection are silent', async () => {
  const h = harness();
  assert.deepEqual(await Clipboard.paste(h.root), { status: 'noop' });
  assert.deepEqual(h.toasts, ['Nothing to paste']);
  assert.deepEqual(await Clipboard.copy(h.root), { status: 'noop', failed: true });
  assert.deepEqual(await Clipboard.cut(h.root), { status: 'noop', failed: true });
  assert.deepEqual(await Clipboard.duplicate(h.root), { status: 'noop', failed: true });
  assert.equal(h.sent.length, 0);
  assert.deepEqual(h.toasts, ['Nothing to paste']);
});

test('shapes of two layers, a selection the document moved on from, and a missing owner route send nothing and say why', async () => {
  const layers = harness();
  const first = await Intent.layerPaths(layers.root, (i) => i.stableTarget.strokeId === 'a', false, 0), second = await Intent.layerPaths(layers.root, (i) => i.stableTarget.strokeId === 'z', false, 1);
  layers.root.selectedPaths = [...first.paths, ...second.paths];
  await Clipboard.duplicate(layers.root);
  assert.deepEqual(layers.toasts, ['Select shapes on one layer to duplicate them.']);
  const gone = harness();
  await gone.pick('a', 'b');
  gone.store.splice(0, 1);
  await Clipboard.copy(gone.root);
  assert.deepEqual(gone.toasts, ['The selection changed — nothing was changed.']);
  assert.equal(Clipboard.hasClip(gone.root), false);
  const none = harness(FLAT(), { route: false });
  await none.pick('a');
  await Clipboard.cut(none.root);
  assert.deepEqual(none.toasts, ['Cut is not available for this selection (locked, hidden or unsupported shapes are refused).'], 'no declared owner route is a typed unavailable, not a write');
  for (const h of [layers, gone, none]) { assert.equal(h.sent.length, 0); assert.deepEqual(h.legacy, []); }
});

test('an owner refusal is a notice and a resolved status; nothing is selected, kept or thrown', async () => {
  const refuse = (code) => (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: false, error: { code, message: code } });
  const h = harness(FLAT(), { answer: refuse('unavailable') });
  await h.pick('a');
  assert.equal((await Clipboard.duplicate(h.root)).status, 'unavailable');
  assert.equal((await Clipboard.copy(h.root)).status, 'unavailable');
  assert.equal(Clipboard.hasClip(h.root), false, 'a refused copy keeps no clip');
  assert.deepEqual(h.toasts, ['Duplicate is not available for this selection (locked, hidden or unsupported shapes are refused).', 'Copy is not available for this selection (locked, hidden or unsupported shapes are refused).']);
  assert.deepEqual(h.ids(), ['a'], 'the selection stands');
  const large = harness(FLAT(), { answer: refuse('invalid_request') });
  await large.pick('a');
  await Clipboard.cut(large.root);
  assert.deepEqual(large.toasts, ['Too much to cut at once — select fewer shapes.'], 'a reply over 64 KiB is refused by the owner before the cut publishes');
  assert.deepEqual(large.ids(), ['a']);
  const stale = harness(FLAT(), { answer: refuse('stale_revision') });
  await stale.pick('a');
  await Clipboard.cut(stale.root);
  assert.deepEqual(stale.toasts, ['The document changed — nothing was changed.']);
});

test('a clip over the old 4096 bytes copies and pastes (64 KiB), a paste over 64 KiB is a typed request_too_large notice and sends nothing', async () => {
  const big = harness(FLAT(), { pad: 6000 });
  await big.pick('a', 'b');
  assert.equal((await Clipboard.copy(big.root)).status, 'copied');
  assert.ok(JSON.stringify(big.sent[0]).length < 4096, 'the copy request itself is small');
  const paste = await Clipboard.paste(big.root);
  assert.equal(paste.status, 'applied');
  const size = Buffer.byteLength(JSON.stringify(big.sent[1]));
  assert.ok(size > 4096 && size <= 65536, String(size));
  const huge = harness(FLAT(), { pad: 40000 });
  await huge.pick('a', 'b');
  await Clipboard.copy(huge.root);
  assert.equal(Clipboard.hasClip(huge.root), false, 'a reply over 64 KiB is refused whole: no clip, no revision');
  assert.ok(/Copy .*(could not be applied|not available)/.test(huge.toasts.slice(-1)[0]), huge.toasts.join('|'));
  assert.equal(huge.session.info().contentRevision, 7);
});

test('a second gesture waits for the first (one edit at a time) and reads the selection it left', async () => {
  const h = harness();
  await h.pick('a');
  let release; h.gate.hold = new Promise((done) => { release = done; });
  const first = Clipboard.duplicate(h.root), second = Clipboard.duplicate(h.root);
  await settle();
  assert.equal(h.sent.length, 1, 'the second waits');
  h.gate.hold = null; release();
  await Promise.all([first, second]);
  assert.equal(h.sent.length, 2);
  assert.deepEqual(h.sent[1].payload.strokeIds, ['a-d1'], 'the second duplicate copies the selection the first one left');
});

test('a project without a native owner is not this module\'s: native() is false and no request is sent', async () => {
  const h = harness(FLAT(), { native: false });
  assert.equal(Clipboard.native(h.root), false);
  await Clipboard.duplicate(h.root);
  assert.equal(h.sent.length, 0);
});

// ---- the callers ------------------------------------------------------------------------------------------------------------------------------
function legacySandbox(h) {
  const reached = [];
  const sandbox = { window: h.root, state: h.root.state, selectedPaths: h.root.selectedPaths, SM: h.root.SM, showToast: (t) => h.toasts.push(t), SMEngineBridge: h.root.SMEngineBridge,
    allowLegacySelectionEdit: () => { reached.push('allowLegacySelectionEdit'); return false; }, _canvasClip: null, Date, Math };
  vm.createContext(sandbox);
  vm.runInContext(['duplicateSelection', 'copySelection', 'cutSelection', 'pasteSelection'].map((n) => extract(TOOLS, n)).join('\n') +
    '\nthis.api = { duplicateSelection, copySelection, cutSelection, pasteSelection };', sandbox);
  return { api: sandbox.api, reached };
}

test('the legacy tools.js entry points (Cmd+D / Cmd+C / Cmd+X / Cmd+V, the old menu rows) hand a native project to the owner and never reach the Paper writer', async () => {
  const h = harness();
  const { api, reached } = legacySandbox(h);
  await h.pick('a');
  api.copySelection(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.clipboard.copy');
  api.pasteSelection(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.clipboard.paste');
  await h.pick('b');
  api.duplicateSelection(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.selection.duplicate');
  await h.pick('c');
  api.cutSelection(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.clipboard.cut');
  assert.equal(h.sent.length, 4);
  assert.deepEqual(reached, [], 'the legacy guard was never consulted: no Paper clone, no pushUndo');
  assert.deepEqual(h.legacy, []);
});

test('without a native owner the same entry points still run the legacy writer', () => {
  const h = harness(FLAT(), { native: false });
  const { api, reached } = legacySandbox(h);
  api.duplicateSelection();
  api.cutSelection();
  api.pasteSelection();
  assert.deepEqual(reached, ['allowLegacySelectionEdit', 'allowLegacySelectionEdit', 'allowLegacySelectionEdit']);
  assert.equal(h.sent.length, 0);
});

test('the keyboard: a native clip makes Cmd+V paste shapes; the Cmd+D chain still reaches duplicateSelection for a canvas selection', async () => {
  const h = harness();
  await h.pick('a');
  await Clipboard.copy(h.root);
  // timeline.js builds the arbiter context itself: its canvasClipboard answer must include the native clip
  const ctxLine = TIMELINE.split('\n').find((l) => l.includes('canvasClipboard:'));
  assert.match(ctxLine, /NemoNativeSelectionClipboard\.hasClip\(window\)/);
  const ctx = (clip) => ({ canvasClipboard: clip, shapeSelection: true, lastClipKind: h.root._lastClipKind });
  const key = { key: 'v', metaKey: true, target: { tagName: 'DIV' } };
  assert.equal(Arbiter.decide(key, ctx(Clipboard.hasClip(h.root))).kind, 'pasteShapes');
  assert.equal(Arbiter.decide({ ...key, key: 'c' }, ctx(true)).kind, 'copyShapes');
  assert.equal(Arbiter.decide({ ...key, key: 'x' }, ctx(true)).kind, 'cutShapes');
  assert.match(TIMELINE, /if\(selectedPaths\.length\)duplicateSelection\(\);/, 'Cmd+D on a canvas selection runs duplicateSelection, which the owner takes');
});

test('the Select tool\'s contextmenu listener gives a native project the menu module instead of dropping the event', () => {
  assert.match(BRIDGE, /if \(!nativePointerEvent\('cancelEvent', e\)\) \{ onContext\(e\); return; \} if \(shouldIntercept\(\) && window\.NemoNativeSelectMenu\) window\.NemoNativeSelectMenu\.open\(window, e\);/);
});

// ---- the Select right-click menu --------------------------------------------------------------------------------------------------------------
const Menu = require('../src/js/adapters/native-select-menu.js'), NativeGroup = require('../src/js/adapters/native-select-group.js');
const NativeDelete = require('../src/js/adapters/native-selection-delete.js'), NativeBoolean = require('../src/js/adapters/native-selection-boolean.js'), GroupIntent = require('../src/js/adapters/native-object-group-intent.js');
function menuHarness(items, opts) {
  const h = harness(items, opts), shown = [];
  Object.assign(h.root, { NemoNativeSelectMenu: Menu, NemoNativeSelectGroup: NativeGroup, NemoNativeSelectionDelete: NativeDelete, NemoNativeSelectionBoolean: NativeBoolean, NemoNativeObjectGroupIntent: GroupIntent,
    showContextMenu: (x, y, rows) => shown.push({ x, y, rows }), SMEngineBridge: { renderNow() {}, isEnabled: () => true } });
  return Object.assign(h, { shown, labels: () => shown.slice(-1)[0].rows.map((r) => r.sep ? '---' : r.label) });
}
const IN = (id, mode) => ({ groupPath: [{ id, mode }] });

test('right-click on an unselected shape selects it like a click, then offers the rows that have a native route', async () => {
  const h = menuHarness();
  assert.equal(await Menu.open(h.root, { clientX: 20, clientY: 20 }) instanceof Array, true);
  assert.deepEqual(h.ids(), ['a'], 'the shape under the pointer became the selection');
  assert.equal(h.shown[0].x, 20);
  assert.deepEqual(h.labels(), ['Copier', 'Couper', 'Coller', 'Dupliquer', 'Supprimer']);
  const paste = h.shown[0].rows.find((r) => r.label === 'Coller');
  assert.equal(paste.disabled, true, 'Coller is disabled while no clip is held');
  assert.equal(h.shown[0].rows.find((r) => r.label === 'Copier').shortcut, '⌘C');
  assert.deepEqual(h.sent, [], 'opening the menu writes nothing');
});

test('right-click on a shape already in a multi-selection keeps the whole selection; the rows follow it', async () => {
  const h = menuHarness();
  await h.pick('a', 'b');
  await Menu.open(h.root, { clientX: 35, clientY: 35 });
  assert.deepEqual(h.ids(), ['a', 'b']);
  assert.deepEqual(h.labels(), ['Copier', 'Couper', 'Coller', 'Dupliquer', 'Supprimer la sélection', '---', 'Grouper', 'Combiner (Union) — non destructif']);
});

test('the menu rows run the native routes: Dupliquer, Copier then Coller (enabled once a clip is held), Couper', async () => {
  const h = menuHarness();
  await h.pick('a');
  await Menu.open(h.root, { clientX: 20, clientY: 20 });
  const row = (label) => h.shown.slice(-1)[0].rows.find((r) => r.label === label);
  row('Copier').action(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.clipboard.copy');
  await Menu.open(h.root, { clientX: 20, clientY: 20 });
  assert.equal(row('Coller').disabled, false);
  row('Coller').action(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.clipboard.paste');
  await h.pick('b');
  await Menu.open(h.root, { clientX: 35, clientY: 35 });
  row('Dupliquer').action(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.selection.duplicate');
  await h.pick('c');
  await Menu.open(h.root, { clientX: 80, clientY: 80 });
  row('Couper').action(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.clipboard.cut');
  assert.deepEqual(h.legacy, []);
});

test('right-click on an empty spot opens nothing, or Coller alone when a clip is held', async () => {
  const h = menuHarness();
  await Menu.open(h.root, { clientX: 150, clientY: 150 });
  assert.equal(h.shown.length, 0);
  await h.pick('a');
  await Clipboard.copy(h.root);
  h.root.selectedPaths = [];
  await Menu.open(h.root, { clientX: 150, clientY: 150 });
  assert.deepEqual(h.labels(), ['Coller']);
  h.shown[0].rows[0].action(); await settle();
  assert.equal(h.sent.slice(-1)[0].operation, 'command.document.object.clipboard.paste');
});

test('a grouped shape offers Dissocier; a combine group offers Sortir du groupe and Aplatir', async () => {
  const grouped = menuHarness([quad('L1', 'a', 10, 10, 40, 40, IN('P', 'none')), quad('L1', 'b', 30, 30, 70, 70, IN('P', 'none')), quad('L1', 'c', 60, 60, 90, 90)]);
  await Menu.open(grouped.root, { clientX: 20, clientY: 20 });
  assert.deepEqual(grouped.labels(), ['Copier', 'Couper', 'Coller', 'Dupliquer', 'Supprimer', '---', 'Dissocier']);
  const combined = menuHarness([quad('L1', 'a', 10, 10, 40, 40, IN('G', 'unite')), quad('L1', 'b', 30, 30, 70, 70, IN('G', 'unite'))]);
  await Menu.open(combined.root, { clientX: 20, clientY: 20 });
  assert.deepEqual(combined.labels().slice(6), ['Dissocier', 'Sortir du groupe', 'Aplatir']);
});

test('the menu belongs to the Select tool on a native project only: any other tool, a running playback or a legacy project is not taken', () => {
  const h = menuHarness();
  h.root.state.tool = 'brush';
  assert.equal(Menu.open(h.root, { clientX: 20, clientY: 20 }), false);
  h.root.state.tool = 'select'; h.root.state.playing = true;
  assert.equal(Menu.open(h.root, { clientX: 20, clientY: 20 }), false);
  const legacy = menuHarness(FLAT(), { native: false });
  assert.equal(Menu.open(legacy.root, { clientX: 20, clientY: 20 }), false);
  assert.equal(h.shown.length + legacy.shown.length, 0);
});

test('Supprimer from the menu runs the native delete (the same removal as the key); with nothing selected it resolves to null', async () => {
  const h = menuHarness();
  assert.equal(await NativeDelete.removeSelection(h.root), null);
  const calls = [];
  h.root.NemoNativeSelectionDelete = { removeSelection: (host) => { calls.push(host); return Promise.resolve('applied'); } };
  await h.pick('a');
  await Menu.open(h.root, { clientX: 20, clientY: 20 });
  h.shown[0].rows.find((r) => r.label === 'Supprimer').action();
  assert.deepEqual(calls, [h.root]);
  assert.equal(await NativeDelete.removeSelection(Object.assign(menuHarness().root, {})), null);
});
