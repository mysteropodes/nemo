'use strict';
// #2372 X-group-canvas-ops: on a native object project Cmd+G, Cmd+Shift+G, "Sortir du groupe" and "Aplatir" send ONE native request each through the
// existing group intent (object.group.create / ungroup / member.remove, boolean.flatten) instead of the legacy group-bridge writers, which edit
// ld.groups and take a legacy history checkpoint a native project refuses. The real timeline.js key branches, combine panel and buttons and the
// real group-bridge.js entry points run in a sandbox; the real group intent, component-intent ports, edit contract and select-canvas-intent run
// against a stub session client whose owner keeps each record's groupPath.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const GroupIntent = require('../src/js/adapters/native-object-group-intent.js'), ComponentIntent = require('../src/js/adapters/native-object-component-intent.js'), EditContract = require('../src/js/adapters/native-object-edit-contract.js');
const NativeGroup = require('../src/js/adapters/native-select-group.js');
const Paper = require('./helpers/native-import-paper.cjs');

const read = (f) => fs.readFileSync(path.join(__dirname, '..', 'src', 'js', f), 'utf8');
const TIMELINE = read('timeline.js'), GROUP = read('group-bridge.js');
function extract(src, name) {
  const from = src.indexOf('function ' + name + '(');
  assert.ok(from >= 0, name);
  let depth = 0;
  for (let i = src.indexOf('{', from); i < src.length; i++) { if (src[i] === '{') depth++; else if (src[i] === '}' && --depth === 0) return src.slice(from, i + 1); }
  throw new Error('unterminated ' + name);
}
const between = (src, a, b) => { const i = src.indexOf(a), j = src.indexOf(b, i); assert.ok(i >= 0 && j > i, a); return src.slice(i, j); };
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
const settle = async () => { for (let i = 0; i < 12; i++) await tick(); };
const LAYERS = [{ layerUid: 'L1' }, { layerUid: 'L2' }];
const FLAT = () => [quad('L1', 'a', 10, 10, 40, 40), quad('L1', 'b', 30, 30, 70, 70), quad('L1', 'c', 60, 60, 90, 90)];
const IN = (id, mode, name) => ({ groupPath: [{ id, mode, ...(name ? { name } : {}) }] });
const COMBINED = () => [quad('L1', 'a', 10, 10, 40, 40, IN('G', 'unite')), quad('L1', 'b', 30, 30, 70, 70, IN('G', 'unite')), quad('L1', 'c', 60, 60, 90, 90)];
const PLAIN_GROUP = () => [quad('L1', 'a', 10, 10, 40, 40, IN('P', 'none')), quad('L1', 'b', 30, 30, 70, 70, IN('P', 'none')), quad('L1', 'c', 60, 60, 90, 90)];

function button(id) {
  const classes = new Set();
  return { id, disabled: false, style: {}, classList: { toggle: (c, on) => { if (on) classes.add(c); else classes.delete(c); }, contains: (c) => classes.has(c) }, addEventListener(type, fn) { this.on = this.on || {}; this.on[type] = fn; } };
}
const refused = (request, code) => ({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: false, error: { code, message: code } });

// opts: native, route, answer(request, ctl). The owner keeps a record's groupPath current for the four operations under test.
function harness(items = FLAT(), opts = {}) {
  const { native = true, route = true, adapter = true } = opts;
  const paper = Paper.load(), toasts = [], sent = [], legacy = [], gate = { hold: null };
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 's7', canvasW: 200, canvasH: 200, layers: LAYERS };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const find = (id) => store.find((i) => i.stableTarget.strokeId === id);
  const bump = () => { pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) }; return pin.contentRevision; };
  const reads = { n: 0 };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => native, requestId: (label) => label + '-' + (sent.length + 1),
    readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(selector, _pin, wanted) {
      reads.n++;
      return { items: store.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  const own = (request) => {
    const p = request.payload, drop = (id, gid) => { const it = find(id); it.groupPath = (it.groupPath || []).filter((g) => g.id !== gid); if (!it.groupPath.length) delete it.groupPath; };
    if (request.operation.endsWith('group.create')) p.strokeIds.forEach((id) => { const it = find(id); it.groupPath = [{ id: p.groupId, name: p.name, mode: 'none' }, ...(it.groupPath || [])]; });
    else if (request.operation.endsWith('group.ungroup')) { const outer = new Set(p.strokeIds.map((id) => find(id).groupPath[0].id)); store.forEach((it) => outer.forEach((g) => drop(it.stableTarget.strokeId, g))); }
    else if (request.operation.endsWith('member.remove')) {
      const it = find(p.strokeId), g = it.groupPath[it.groupPath.length - 1].id; drop(p.strokeId, g);
      const left = store.filter((o) => (o.groupPath || []).some((x) => x.id === g));
      if (left.length < 2) left.forEach((o) => drop(o.stableTarget.strokeId, g));
    } else if (request.operation.endsWith('boolean.flatten')) {
      const parts = store.filter((o) => (o.groupPath || []).some((x) => x.id === p.groupId));
      const at = store.indexOf(parts[0]), b = [Math.min(...parts.map((o) => o.bounds[0])), Math.min(...parts.map((o) => o.bounds[1])), Math.max(...parts.map((o) => o.bounds[2])), Math.max(...parts.map((o) => o.bounds[3]))];
      parts.forEach((o) => store.splice(store.indexOf(o), 1));
      store.splice(at, 0, quad(p.layerUid, 'island-1', ...b));
    }
  };
  if (route) {
    session.runResourceCommand = async (action) => action({
      async dispatch(request) {
        sent.push(JSON.parse(JSON.stringify(request)));
        if (gate.hold) await gate.hold;
        if (opts.answer) return opts.answer(request, { bump });
        own(request);
        bump();
        return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: pin.contentRevision, ok: true, result: { applied: true, layerUid: request.payload.layerUid } };
      },
      async reconcile() {},
    });
  }
  const buttons = {};
  const ids = ['combine-existing-row', 'btn-combine-remove', 'btn-combine-flatten', ...['unite', 'subtract', 'intersect', 'exclude'].flatMap((m) => ['btn-combine-' + m, 'btn-bool-' + m])];
  ids.forEach((id) => { buttons[id] = button(id); });
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: LAYERS, selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() { root.updates = (root.updates || 0) + 1; }, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    SM: { t: (key) => ({ toastSelectAtLeast2ElementsToGroup: 'Select at least 2 elements to create a group', toastGroupCreatedSuffix: 'Group created (', toastElementsCloseParen: ' elements)',
      toastGroupUngrouped: 'Group ungrouped', toastSelectGroupToUngroupFirst: 'Select a group to ungroup first', toastSelectionHasNoGroup: 'The selection contains no group', autoNameGroup: 'Group' }[key] || key) },
    setTimeout: (fn) => fn, clearTimeout() {},
    document: { getElementById: (id) => buttons[id] || { getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }, createElementNS: (_n, tag) => el(tag), body: el('body'), addEventListener() {}, removeEventListener() {} },
    requestAnimationFrame: () => {}, pushUndo() { legacy.push('pushUndo'); throw new Error('native authority denied legacy history checkpoint'); },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay,
    NemoNativeObjectGroupIntent: GroupIntent, NemoNativeObjectComponentIntent: ComponentIntent, NemoNativeObjectEditContract: EditContract };
  if (adapter) root.NemoNativeSelectGroup = NativeGroup;
  const h = { root, toasts, sent, legacy, store, gate, session, buttons, reads, bump, find,
    async pick(...want) {
      const layers = await Intent.layerPaths(root, (item) => want.indexOf(item.stableTarget.strokeId) >= 0, false, 0);
      layers.paths.sort((x, y) => want.indexOf(x.data.strokeId) - want.indexOf(y.data.strokeId));
      root.selectedPaths = layers.paths;
      return layers.paths;
    },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId) };
  // the real group-bridge entry points, the real timeline.js key branches, panel and combine buttons (legacy stand-ins record being reached)
  const listeners = {}, legacyCalls = [], keyLog = [];
  const sandbox = { window: root, document: { getElementById: (id) => ({ ...(buttons[id] || {}), addEventListener: (type, fn) => { listeners[id + ':' + type] = fn; } }) }, Object, Array, console, Date, Math,
    state: root.state, SM: root.SM, showToast: (t) => toasts.push(t), pushUndo: root.pushUndo, saveActiveLayerFrame() { legacy.push('saveActiveLayerFrame'); }, updateUI() {},
    userLayers: [{ children: [] }], _layerSel: [], groupSelectionIntoFolder: () => keyLog.push('folder'), COMBINE_MODE_BTN_IDS: { unite: 'btn-combine-unite', subtract: 'btn-combine-subtract', intersect: 'btn-combine-intersect', exclude: 'btn-combine-exclude' },
    updateCombinePanel: () => keyLog.push('panel'), NemoNativeSelectGroup: adapter ? NativeGroup : undefined };
  Object.defineProperty(sandbox, 'selectedPaths', { get: () => root.selectedPaths });
  root.SMGroup = {};
  vm.createContext(sandbox);
  const bridge = ['groupSelection', 'ungroupSelection', 'removeMemberFromGroup', 'flattenGroup'].map((n) => extract(GROUP, n)).join('\n');
  vm.runInContext(bridge + '\nthis.SMGroup = window.SMGroup = { groupSelection, ungroupSelection, removeMemberFromGroup, flattenGroup };', sandbox);
  vm.runInContext('function key(event){\n' + between(TIMELINE, 'var _gLayerRows=', '  // Ctrl/Cmd+G "group into folder"') + '\n}', sandbox);
  vm.runInContext(extract(TIMELINE, 'updateCombinePanel').replace('function updateCombinePanel', 'function legacyUpdateCombinePanel') + '\nthis.panel = legacyUpdateCombinePanel;', sandbox);
  vm.runInContext('(function(){' + between(TIMELINE, '  function activeCombineGid', '})();\n// Save/Save As') + '})();', sandbox);
  h.sandbox = sandbox; h.listeners = listeners; h.keyLog = keyLog;
  h.key = async (shiftKey) => {
    let prevented = false;
    sandbox.key({ metaKey: true, ctrlKey: false, shiftKey, key: shiftKey ? 'G' : 'g', preventDefault() { prevented = true; } });
    await settle();
    return prevented;
  };
  h.panel = async () => { sandbox.panel(); await settle(); };
  h.click = async (id) => { listeners[id + ':click']({}); await settle(); };
  return h;
}

test('Cmd+G is ONE object.group.create request over the selected ids in paint order; the members stay selected', async () => {
  const h = harness();
  await h.pick('b', 'a');
  assert.equal(await h.key(false), true, 'the key is taken');
  assert.equal(h.sent.length, 1, 'one request is one revision and one history entry');
  const [r] = h.sent;
  assert.equal(r.operation, 'command.document.object.group.create');
  assert.equal(r.expectedRevision, 7, 'pinned to the presented revision');
  assert.match(r.payload.groupId, /^grp_[a-z0-9]+_\d+$/);
  assert.deepEqual({ ...r.payload, groupId: 'G' }, { command: 'object.group.create', layerUid: 'L1', contextId: 'scene-root', groupId: 'G', name: 'Group', strokeIds: ['a', 'b'] });
  assert.deepEqual(h.legacy, [], 'no legacy history checkpoint, no ld.groups writer');
  assert.deepEqual(h.toasts, ['Group created (2 elements)']);
  assert.deepEqual(h.ids().sort(), ['a', 'b']);
  assert.deepEqual(h.keyLog, [], 'the layer-folder fallback is not reached');
});

test('grouping counts UNITS: a fully selected group nested with a shape is 2 units; one whole group alone is nothing to group', async () => {
  const h = harness(PLAIN_GROUP());
  await h.pick('a', 'b', 'c');
  await h.key(false);
  assert.equal(h.sent.length, 1);
  assert.deepEqual(h.toasts, ['Group created (2 elements)'], 'P (a, b) and c');
  assert.deepEqual(h.sent[0].payload.strokeIds, ['a', 'b', 'c']);
  const once = harness(PLAIN_GROUP());
  await once.pick('a', 'b');
  await once.key(false);
  assert.equal(once.sent.length, 0);
  assert.deepEqual(once.toasts, ['Select at least 2 elements to create a group']);
  const part = harness(PLAIN_GROUP());
  await part.pick('a', 'c');
  await part.key(false);
  assert.equal(part.sent.length, 1, 'a group only PARTLY selected contributes its selected shapes as loose leaves');
  assert.deepEqual(part.toasts, ['Group created (2 elements)']);
});

test('Cmd+G with fewer than two shapes, shapes of two layers, another tool or a moved-on document sends nothing', async () => {
  const one = harness();
  await one.pick('a');
  assert.equal(await one.key(false), false, 'one shape is not taken: it falls through to the layer-folder branch exactly as before');
  const tool = harness();
  await tool.pick('a', 'b');
  tool.root.state.tool = 'brush';
  await tool.sandbox.SMGroup.groupSelection();
  await settle();
  assert.deepEqual(tool.toasts, ['Select at least 2 elements to create a group']);
  const layers = harness([quad('L1', 'a', 10, 10, 40, 40), quad('L2', 'b', 30, 30, 70, 70)]);
  const first = await Intent.layerPaths(layers.root, (i) => i.stableTarget.strokeId === 'a', false, 0), second = await Intent.layerPaths(layers.root, (i) => i.stableTarget.strokeId === 'b', false, 1);
  layers.root.selectedPaths = [...first.paths, ...second.paths];
  await layers.key(false);
  assert.deepEqual(layers.toasts, ['Select shapes on one layer to group them.']);
  const gone = harness();
  await gone.pick('a', 'b');
  gone.store.splice(0, 1);
  await gone.key(false);
  assert.deepEqual(gone.toasts, ['The selection changed — nothing was changed.']);
  for (const h of [one, tool, layers, gone]) { assert.equal(h.sent.length, 0); assert.deepEqual(h.legacy, []); }
});

test('Cmd+Shift+G is ONE object.group.ungroup request over the grouped ids only, then the group is gone', async () => {
  const h = harness(PLAIN_GROUP());
  await h.pick('a', 'c');
  assert.equal(await h.key(true), true);
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, 'command.document.object.group.ungroup');
  assert.deepEqual(h.sent[0].payload, { command: 'object.group.ungroup', layerUid: 'L1', contextId: 'scene-root', strokeIds: ['a'] });
  assert.deepEqual(h.toasts, ['Group ungrouped']);
  assert.deepEqual(h.legacy, []);
  assert.equal(h.find('b').groupPath, undefined, 'the owner dissolved the group');
  assert.deepEqual(h.ids().sort(), ['a', 'c']);
});

test('Cmd+Shift+G on a selection without a group, or without a selection, says so and sends nothing', async () => {
  const h = harness();
  await h.pick('a', 'b');
  await h.key(true);
  assert.deepEqual(h.toasts, ['The selection contains no group']);
  h.root.selectedPaths = [];
  await h.sandbox.SMGroup.ungroupSelection();
  await settle();
  assert.deepEqual(h.toasts.slice(-1), ['Select a group to ungroup first']);
  assert.equal(h.sent.length, 0);
  assert.deepEqual(h.legacy, []);
});

test('Sortir du groupe is ONE object.group.member.remove request for the one selected shape; a two-member group dissolves in the owner', async () => {
  const h = harness(COMBINED());
  await h.pick('a');
  await h.panel();
  assert.equal(h.buttons['combine-existing-row'].style.display, 'flex');
  await h.click('btn-combine-remove');
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, 'command.document.object.group.member.remove');
  assert.deepEqual(h.sent[0].payload, { command: 'object.group.member.remove', layerUid: 'L1', contextId: 'scene-root', strokeId: 'a' });
  assert.deepEqual(h.legacy, []);
  assert.equal(h.find('a').groupPath, undefined);
  assert.equal(h.find('b').groupPath, undefined, 'one entry left is not a group any more');
  assert.deepEqual(h.ids(), ['a']);
  await h.panel();
  assert.equal(h.buttons['combine-existing-row'].style.display, 'none', 'the panel follows the new revision');
});

test('Sortir du groupe needs exactly one grouped shape; the legacy entry point routes to the owner and never writes', async () => {
  const many = harness(COMBINED());
  await many.pick('a', 'b');
  await many.click('btn-combine-remove');
  const loose = harness(COMBINED());
  await loose.pick('c');
  await loose.click('btn-combine-remove');
  assert.equal(many.sent.length + loose.sent.length, 0);
  const viaBridge = harness(COMBINED());
  const [p] = await viaBridge.pick('b');
  viaBridge.sandbox.SMGroup.removeMemberFromGroup(p, { groups: {} }, {});
  await settle();
  assert.equal(viaBridge.sent.length, 1);
  assert.equal(viaBridge.sent[0].payload.strokeId, 'b');
  viaBridge.sandbox.SMGroup.removeMemberFromGroup(p, {}, {}, { skipUndo: true, silent: true });
  await settle();
  assert.equal(viaBridge.sent.length, 1, 'an internal caller with options never reaches the legacy writer on a native project');
  assert.deepEqual(viaBridge.legacy, []);
});

test('Aplatir is ONE object.boolean.flatten request naming the combine group and the frame; the result island is selected', async () => {
  const h = harness(COMBINED());
  await h.pick('b');
  await h.panel();
  await h.click('btn-combine-flatten');
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, 'command.document.object.boolean.flatten');
  assert.equal(h.sent[0].expectedRevision, 7);
  assert.deepEqual(h.sent[0].payload, { command: 'object.boolean.flatten', layerUid: 'L1', contextId: 'scene-root', frame: 0, groupId: 'G' });
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.toasts, ['Groupe aplati']);
  assert.deepEqual(h.ids(), ['island-1'], 'the baseline selects the flattened result');
  assert.deepEqual(h.store.map((i) => i.stableTarget.strokeId), ['island-1', 'c']);
});

test('Aplatir acts on a combine group only: a plain group, a part of the group and an ungrouped shape send nothing', async () => {
  for (const [items, want] of [[PLAIN_GROUP(), ['a']], [COMBINED(), ['a', 'c']], [FLAT(), ['a']]]) {
    const h = harness(items);
    await h.pick(...want);
    await h.click('btn-combine-flatten');
    assert.equal(h.sent.length, 0);
    assert.deepEqual(h.legacy, []);
  }
});

test('the combine panel reads the group from the native listing: the row, the active mode and the enabled state', async () => {
  const none = harness();
  await none.panel();
  assert.equal(none.buttons['combine-existing-row'].style.display, 'none');
  assert.equal(none.buttons['btn-combine-unite'].disabled, true, 'nothing to combine');
  await none.pick('a');
  await none.panel();
  assert.equal(none.buttons['btn-combine-unite'].disabled, true, 'one loose shape cannot be combined');
  await none.pick('a', 'b');
  await none.panel();
  assert.equal(none.buttons['btn-combine-unite'].disabled, false, 'two shapes can');
  assert.equal(none.buttons['btn-combine-unite'].classList.contains('ac'), true, 'unite is the default mode');

  const sub = harness([quad('L1', 'a', 10, 10, 40, 40, IN('G', 'subtract')), quad('L1', 'b', 30, 30, 70, 70, IN('G', 'subtract'))]);
  await sub.pick('a', 'b');
  await sub.panel();
  assert.equal(sub.buttons['combine-existing-row'].style.display, 'flex', 'the selection IS the group');
  assert.equal(sub.buttons['btn-combine-subtract'].classList.contains('ac'), true);
  assert.equal(sub.buttons['btn-bool-subtract'].classList.contains('ac'), true, 'the mode is mirrored on the boolean row');
  assert.equal(sub.buttons['btn-combine-unite'].classList.contains('ac'), false);
  assert.equal(sub.buttons['btn-combine-unite'].disabled, false);

  const nested = harness(PLAIN_GROUP());
  await nested.pick('a');
  await nested.panel();
  assert.equal(nested.buttons['combine-existing-row'].style.display, 'none', 'a plain group (mode none) has no Remove / Flatten row');
});

test('the panel read is cached per selection and revision: repeated updates read once, a new revision reads again', async () => {
  const h = harness(COMBINED());
  await h.pick('a');
  await h.panel();
  const n = h.reads.n;
  await h.panel(); await h.panel();
  assert.equal(h.reads.n, n, 'the same key paints from the cache');
  assert.equal(h.buttons['combine-existing-row'].style.display, 'flex');
  h.bump();
  await h.panel();
  assert.ok(h.reads.n > n, 'a new revision reads again');
});

test('a refusal by the owner is a notice, not a throw, and changes nothing; a stale revision is reported, not retried', async () => {
  const locked = harness(FLAT(), { answer: (request) => refused(request, 'unavailable') });
  await locked.pick('a', 'b');
  await locked.key(false);
  assert.equal(locked.sent.length, 1);
  assert.match(locked.toasts.join('|'), /Group is not available for this selection/);
  assert.deepEqual(locked.ids(), ['a', 'b'], 'the selection is untouched');
  const stale = harness(PLAIN_GROUP(), { answer: (request) => refused(request, 'stale_revision') });
  await stale.pick('a');
  await stale.key(true);
  assert.equal(stale.sent.length, 1);
  assert.deepEqual(stale.toasts, ['The document changed — nothing was changed.']);
  const flat = harness(COMBINED(), { answer: (request) => refused(request, 'unavailable') });
  await flat.pick('a');
  await flat.panel();
  await flat.click('btn-combine-flatten');
  assert.match(flat.toasts.join('|'), /Flatten is not available/);
  assert.deepEqual(flat.ids(), ['a']);
});

test('two quick gestures are two serialized requests: the second reads the selection the first left', async () => {
  const h = harness();
  await h.pick('a', 'b');
  let release; h.gate.hold = new Promise((done) => { release = done; });
  const first = h.key(false);
  await settle();
  const second = h.key(true);
  await settle();
  assert.equal(h.sent.length, 1, 'the second waits for the first');
  h.gate.hold = null; release();
  await first; await second; await settle();
  assert.deepEqual(h.sent.map((r) => r.operation.split('.').pop()), ['create', 'ungroup']);
  assert.deepEqual(h.legacy, []);
});

test('without the native owner route a gesture is a notice and a return: it never throws and never reaches the legacy writer', async () => {
  const h = harness(FLAT(), { route: false });
  await h.pick('a', 'b');
  await h.key(false);
  await h.key(true);
  assert.equal(h.sent.length, 0);
  assert.equal(h.toasts.length, 2);
  assert.deepEqual(h.legacy, []);
});

test('a legacy project keeps the legacy writers byte for byte', async () => {
  const h = harness(FLAT(), { native: false });
  h.root.selectedPaths = [{ data: { strokeId: 'a' } }, { data: { strokeId: 'b' } }];
  assert.equal(NativeGroup.native(h.root), false);
  h.sandbox.SMGroup = { groupSelection: () => h.keyLog.push('legacy-group'), ungroupSelection: () => h.keyLog.push('legacy-ungroup') };
  await h.key(false);
  await h.key(true);
  assert.equal(h.sent.length, 0);
  assert.deepEqual(h.keyLog, ['legacy-group', 'legacy-ungroup']);
  assert.doesNotThrow(() => h.sandbox.panel(), 'the legacy panel path runs');
  assert.equal(h.reads.n, 0, 'and reads nothing from the native listing');
});

test('the module stays inside the adapter size contract and writes no Paper / ld.groups group', () => {
  const src = read('adapters/native-select-group.js');
  assert.ok(src.split('\n').filter((l) => l.trim()).length <= 500);
  assert.doesNotMatch(src.replace(/\/\*[\s\S]*?\*\//, ''), /foldBooleanOp|insertBooleanResult|pushUndo|ld\.groups|dissolveGroup|GeometryWasm/);
});

test('a combine mode picked from the group row is ONE object.group.mode.set request naming the group; the current mode is a noop', async () => {
  const h = harness(COMBINED());
  await h.pick('b');
  assert.deepEqual(await NativeGroup.setMode(h.root, 'unite'), { status: 'noop' }, 'the mode it already has changes nothing');
  assert.equal(h.sent.length, 0);
  const out = await NativeGroup.setMode(h.root, 'subtract');
  assert.equal(out.status, 'applied');
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, 'command.document.object.group.mode.set');
  assert.equal(h.sent[0].expectedRevision, 7);
  assert.deepEqual(h.sent[0].payload, { command: 'object.group.mode.set', layerUid: 'L1', contextId: 'scene-root', groupId: 'G', mode: 'subtract' });
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(await NativeGroup.setMode(h.root, 'bogus'), { status: 'noop' });
});
