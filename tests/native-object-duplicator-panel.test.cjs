'use strict';
// #2256 Q4: the Duplicator inspector (#duplicator-sec) on a NATIVE object project. The panel is the real native-object-duplicator-panel.js over the
// real component read adapter and duplicator intents; the engine is a model that answers the real wire contracts (the descriptor read and the
// config / path commands). A fake event dispatcher runs the section's CAPTURE listeners before the legacy listeners bound on the fields, like the DOM.
const test = require('node:test');
const assert = require('node:assert/strict');
const Panel = require('../src/js/adapters/native-object-duplicator-panel.js');
const Duplicator = require('../src/js/adapters/native-object-duplicator.js');
const Component = require('../src/js/adapters/native-object-component.js');

const READ = 'query.document.object.component.read', CONFIG = 'command.document.object.duplicator.config', PATH = 'command.document.object.duplicator.path';
const settle = async () => { for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setImmediate(resolve)); };
const IDS = ['duplicator-sec', 'dup-mode', 'dup-source-sec', 'dup-source-add', 'dup-grid-row', 'dup-grid-spacing-row', 'dup-count-row', 'dup-radial-row', 'dup-radial-orient-row',
  'dup-path-row', 'dup-path-align-row', 'dup-rows', 'dup-cols', 'dup-spacingx', 'dup-spacingy', 'dup-count', 'dup-radius', 'dup-startangle', 'dup-radial-orient', 'dup-path-align',
  'dup-path-layer', 'dup-seed', 'btn-dup-reseed', 'dup-rand-pos', 'dup-rand-rot', 'dup-rand-scale', 'dup-rand-op', 'dup-rand-hue', 'dup-anim-hdr-row', 'dup-anim-enabled', 'dup-anim-row',
  'dup-anim-offset', 'dup-anim-direction', 'btn-dup-add-effector', 'dup-edit-source-row', 'btn-dup-edit-source'];

function node(id, tag) {
  const listeners = { capture: {}, own: {} }, kids = [];
  const el = { id, tagName: tag || 'DIV', style: {}, dataset: {}, attributes: {}, value: '', checked: false, title: '', textContent: '', children: kids,
    addEventListener(type, fn, capture) { ((capture ? listeners.capture : listeners.own)[type] = (capture ? listeners.capture : listeners.own)[type] || []).push(fn); },
    setAttribute(k, v) { this.attributes[k] = String(v); }, removeAttribute(k) { delete this.attributes[k]; },
    appendChild(k) { kids.push(k); k.parentNode = this; return k; }, listeners };
  Object.defineProperty(el, 'innerHTML', { set() { kids.splice(0); }, get: () => '' });
  return el;
}
function fire(section, target, type) {
  const event = { type, target, stopped: false, stopPropagation() { this.stopped = true; }, stopImmediatePropagation() { this.stopped = true; } };
  (section.listeners.capture[type] || []).forEach((fn) => { if (!event.stopped) fn(event); });
  if (!event.stopped) (target.listeners.own[type] || []).forEach((fn) => fn(event));
  return event;
}
const DEFAULT = () => ({ mode: 'grid', rows: 2, cols: 3, spacingX: 150, spacingY: 150, count: 8, radius: 200, startAngle: 0, radialOrient: false, pathAlignTangent: true, pathLayerUid: null,
  seed: 17, sourceMode: 'shape', staggerRandom: { position: false, rotation: false, scale: false, opacity: false, hue: false }, timeOffset: { enabled: false, offsetFrames: 1, direction: 'forward' } });

function harness(options = {}) {
  const sent = [], toasts = [], legacy = [], nodes = new Map();
  IDS.forEach((id) => nodes.set(id, node(id, id === 'dup-mode' || id === 'dup-path-layer' || id === 'dup-anim-direction' || id === 'dup-source-add' ? 'SELECT' : 'DIV')));
  const section = nodes.get('duplicator-sec');
  const dup = Object.assign(DEFAULT(), options.dup);
  let revision = 4, refresh = 0;
  const rows = () => [{ layerUid: 'L-a', name: 'Alpha', kind: 'draw', hasDuplicator: true, dupSourceMode: dup.sourceMode }, { layerUid: 'L-b', name: 'Beta', kind: 'draw' },
    { layerUid: 'L-c', name: 'Gamma', kind: 'draw', hasDuplicator: true }, { layerUid: 'L-f', name: 'Folder', kind: 'folder' }, { layerUid: 'L-d', name: 'Delta', kind: 'draw' }];
  const info = () => ({ apiVersion: 2, instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: revision,
    documentSnapshotId: 'snap-' + revision, layers: rows() });
  const base = (request, ok) => ({ apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: revision, ok });
  async function dispatch(request) {
    sent.push(JSON.parse(JSON.stringify(request)));
    if (options.refuse && request.operation !== READ) return Object.assign(base(request, false), { error: { code: options.refuse, message: options.message || 'refused' } });
    if (request.operation === READ) {
      const selector = request.payload.selector;
      return Object.assign(base(request, true), { result: Object.assign({ atRevision: revision, documentSnapshotId: 'snap-' + revision, kind: 'duplicator', layerUid: selector.layerUid },
        JSON.parse(JSON.stringify(dup))) });
    }
    const patch = request.payload;
    let changed = false;
    const put = (target, key, value) => { if (JSON.stringify(target[key]) !== JSON.stringify(value)) { target[key] = value; changed = true; } };
    if (request.operation === PATH) put(dup, 'pathLayerUid', patch.pathLayerUid || null);
    else Object.keys(patch).filter((key) => key !== 'command' && key !== 'layerUid').forEach((key) => {
      if (key === 'staggerRandom' || key === 'timeOffset') Object.keys(patch[key]).forEach((member) => put(dup[key], member, patch[key][member]));
      else put(dup, key, patch[key]);
    });
    const response = Object.assign(base(request, true), { contentRevision: revision + (changed ? 1 : 0), result: { applied: changed, historyEntriesAdded: changed ? 1 : 0 } });
    if (changed) revision += 1;
    return response;
  }
  const session = { info, isActive: () => true, blocksLegacy: () => options.blocks !== false, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({ dispatch, async reconcile() {} }), runComponentCommand: (action) => action({ dispatch, async reconcile() {} }) };
  const root = { state: { activeLayerIdx: options.active === undefined ? 0 : options.active, layers: [{ name: 'stub' }, {}, {}, {}, {}] }, document: { getElementById: (id) => nodes.get(id) || null,
    createElement: (tag) => node('', tag.toUpperCase()) }, NemoNativeObjectProject: { desktop: () => session }, NemoNativeObjectDuplicator: Duplicator,
    NemoNativeObjectComponent: Component, showToast: (m) => toasts.push(m), nativeCompositingRefresh() { refresh += 1; } };
  // The legacy panel writers, bound on the fields themselves: a native project must never reach them.
  IDS.forEach((id) => { const n = nodes.get(id); ['change', 'click'].forEach((type) => n.addEventListener(type, () => legacy.push(id + ':' + type))); });
  return { root, sent, toasts, legacy, nodes, section, dup, refreshes: () => refresh, revision: () => revision, sync: () => Panel.sync(root) };
}
const writes = (h) => h.sent.filter((r) => r.operation !== READ);

test('the section shows for the active layer that hosts a duplicator and is filled from ONE pinned descriptor read', async () => {
  const h = harness({ dup: { mode: 'radial', rows: 4, cols: 5, count: 12, radius: 321, startAngle: 30, radialOrient: true, seed: 4242,
    staggerRandom: { position: true, rotation: false, scale: false, opacity: false, hue: true }, timeOffset: { enabled: true, offsetFrames: 3, direction: 'centerOut' } } });
  assert.equal(h.sync(), true, 'the native project owns the panel');
  await settle();
  assert.equal(h.section.style.display, 'block');
  const reads = h.sent.filter((r) => r.operation === READ);
  assert.equal(reads.length, 1);
  assert.deepEqual(reads[0].payload, { atRevision: 4, documentSnapshotId: 'snap-4', selector: { kind: 'duplicator', layerUid: 'L-a' } });
  assert.equal('expectedRevision' in reads[0], false, 'a pinned read carries no expectedRevision');
  const value = (id) => h.nodes.get(id).value, checked = (id) => h.nodes.get(id).checked;
  assert.deepEqual(['dup-mode', 'dup-rows', 'dup-cols', 'dup-count', 'dup-radius', 'dup-startangle', 'dup-seed', 'dup-anim-offset', 'dup-anim-direction'].map(value),
    ['radial', '4', '5', '12', '321', '30', '4242', '3', 'centerOut']);
  assert.deepEqual(['dup-radial-orient', 'dup-rand-pos', 'dup-rand-rot', 'dup-rand-hue', 'dup-anim-enabled'].map(checked), [true, true, false, true, true]);
  assert.equal(h.legacy.length, 0);
  assert.equal(writes(h).length, 0, 'reading writes nothing');
});

test('rows follow the mode like the baseline: grid rows, count for the others, radial and path extras, object mode hides the shape-only rows', async () => {
  const shown = (h, id) => h.nodes.get(id).style.display;
  const grid = harness(); grid.sync(); await settle();
  assert.deepEqual(['dup-grid-row', 'dup-grid-spacing-row', 'dup-count-row', 'dup-radial-row', 'dup-radial-orient-row', 'dup-path-row', 'dup-path-align-row'].map((id) => shown(grid, id)),
    ['flex', 'flex', 'none', 'none', 'none', 'none', 'none']);
  assert.equal(shown(grid, 'dup-anim-row'), 'none', 'the temporal row waits for its checkbox');
  const radial = harness({ dup: { mode: 'radial' } }); radial.sync(); await settle();
  assert.deepEqual(['dup-grid-row', 'dup-count-row', 'dup-radial-row', 'dup-radial-orient-row', 'dup-path-row'].map((id) => shown(radial, id)), ['none', 'flex', 'flex', 'flex', 'none']);
  const path = harness({ dup: { mode: 'path' } }); path.sync(); await settle();
  assert.deepEqual(['dup-count-row', 'dup-radial-row', 'dup-path-row', 'dup-path-align-row'].map((id) => shown(path, id)), ['flex', 'none', 'flex', 'flex']);
  const objects = harness({ dup: { sourceMode: 'objects', timeOffset: { enabled: true, offsetFrames: 1, direction: 'forward' } } }); objects.sync(); await settle();
  assert.deepEqual(['dup-anim-hdr-row', 'dup-edit-source-row', 'dup-anim-row', 'dup-source-sec'].map((id) => shown(objects, id)), ['none', 'none', 'none', 'block']);
  const shape = harness({ dup: { timeOffset: { enabled: true, offsetFrames: 1, direction: 'forward' } } }); shape.sync(); await settle();
  assert.deepEqual(['dup-anim-hdr-row', 'dup-edit-source-row', 'dup-anim-row'].map((id) => shown(shape, id)), ['flex', 'flex', 'flex']);
});

test('the path-layer select lists path-capable layers other than the host and other duplicators, and shows the stored one', async () => {
  const h = harness({ dup: { mode: 'path', pathLayerUid: 'L-d' } }); h.sync(); await settle();
  const select = h.nodes.get('dup-path-layer');
  assert.deepEqual(select.children.map((o) => [o.value, o.textContent]), [['', '—'], ['L-b', 'Beta'], ['L-d', 'Delta']], 'no host, no other duplicator, no folder');
  assert.equal(select.value, 'L-d');
});

test('the section is hidden when the active layer hosts no duplicator, and the legacy body runs only for a legacy project', async () => {
  const none = harness({ active: 1 }); assert.equal(none.sync(), true); await settle();
  assert.equal(none.section.style.display, 'none');
  assert.equal(none.sent.length, 0, 'nothing is read for a layer without a duplicator');
  const legacy = harness({ blocks: false });
  assert.equal(legacy.sync(), false, 'a legacy project keeps its own panel');
  assert.equal(legacy.sent.length, 0);
});

const CASES = [
  ['dup-mode', 'value', 'path', { mode: 'path' }], ['dup-rows', 'value', '5', { rows: 5 }], ['dup-cols', 'value', '7', { cols: 7 }],
  ['dup-spacingx', 'value', '33.5', { spacingX: 33.5 }], ['dup-spacingy', 'value', '-12', { spacingY: -12 }], ['dup-count', 'value', '40', { count: 40 }],
  ['dup-radius', 'value', '310', { radius: 310 }], ['dup-startangle', 'value', '45', { startAngle: 45 }], ['dup-seed', 'value', '999999', { seed: 999999 }],
  ['dup-radial-orient', 'checked', true, { radialOrient: true }], ['dup-path-align', 'checked', false, { pathAlignTangent: false }],
  ['dup-rand-pos', 'checked', true, { staggerRandom: { position: true } }], ['dup-rand-rot', 'checked', true, { staggerRandom: { rotation: true } }],
  ['dup-rand-scale', 'checked', true, { staggerRandom: { scale: true } }], ['dup-rand-op', 'checked', true, { staggerRandom: { opacity: true } }],
  ['dup-rand-hue', 'checked', true, { staggerRandom: { hue: true } }], ['dup-anim-enabled', 'checked', true, { timeOffset: { enabled: true } }],
  ['dup-anim-offset', 'value', '2.5', { timeOffset: { offsetFrames: 2.5 } }], ['dup-anim-direction', 'value', 'centerOut', { timeOffset: { direction: 'centerOut' } }],
];
for (const [id, member, input, patch] of CASES) {
  test(`${id}: one change is exactly ONE config request naming only that member, and the legacy writer never runs`, async () => {
    const h = harness(); h.sync(); await settle();
    h.nodes.get(id)[member] = input;
    const event = fire(h.section, h.nodes.get(id), 'change');
    assert.equal(event.stopped, true, 'the capture listener keeps the event from the legacy field listener');
    await settle();
    const sent = writes(h);
    assert.equal(sent.length, 1, 'one request');
    assert.equal(sent[0].operation, CONFIG);
    assert.deepEqual(sent[0].payload, Object.assign({ command: 'object.duplicator.config', layerUid: 'L-a' }, patch));
    assert.equal(sent[0].expectedRevision, 4);
    assert.deepEqual(h.legacy, [], 'no legacy listener ran');
    assert.equal(h.root.state.layers[0].duplicator, undefined, 'ld.duplicator is never written');
    assert.equal(h.revision(), 5);
    assert.equal(h.refreshes(), 1, 'the canvas repaints from the confirmed revision');
    assert.equal(h.sent.filter((r) => r.operation === READ).length, 2, 'the panel is read again at the new revision');
  });
}

test('rows, cols and count are clamped with the baseline wireNum bounds before they are sent', async () => {
  const h = harness(); h.sync(); await settle();
  for (const [id, raw, patch] of [['dup-rows', '99', { rows: 30 }], ['dup-rows', '0', { rows: 1 }], ['dup-cols', '-3', { cols: 1 }], ['dup-count', '9000', { count: 500 }], ['dup-seed', '-4', { seed: 0 }], ['dup-seed', '3000000.7', { seed: 999999 }]]) {
    h.sent.length = 0; h.nodes.get(id).value = raw; fire(h.section, h.nodes.get(id), 'change'); await settle();
    assert.deepEqual(writes(h).map((r) => r.payload), [Object.assign({ command: 'object.duplicator.config', layerUid: 'L-a' }, patch)], `${id} ${raw}`);
  }
});

test('a value that is not a number sends nothing and the panel shows the stored value again', async () => {
  const h = harness(); h.sync(); await settle();
  h.nodes.get('dup-rows').value = 'abc'; fire(h.section, h.nodes.get('dup-rows'), 'change'); await settle();
  assert.equal(writes(h).length, 0);
  assert.equal(h.nodes.get('dup-rows').value, '2');
});

test('a scrub previews in the field and sends its single request on release: the numeric fields declare the static scrub contract of ui.js', async () => {
  const h = harness(); h.sync(); await settle();
  ['dup-rows', 'dup-cols', 'dup-spacingx', 'dup-spacingy', 'dup-count', 'dup-radius', 'dup-startangle', 'dup-seed', 'dup-anim-offset'].forEach((id) => {
    assert.equal(h.nodes.get(id).dataset.nativeOpacityScrub, 'static', id);
  });
  assert.equal(h.nodes.get('dup-mode').dataset.nativeOpacityScrub, undefined);
  // ui.js moves the field value during the drag with no events (nativeOpacity) and dispatches ONE change on release.
  for (const during of ['6', '7', '8', '9']) h.nodes.get('dup-cols').value = during;
  assert.equal(h.sent.filter((r) => r.operation !== READ).length, 0, 'no request while dragging');
  fire(h.section, h.nodes.get('dup-cols'), 'change'); await settle();
  assert.deepEqual(writes(h).map((r) => r.payload.cols), [9]);
  const legacyProject = harness({ blocks: false }); Panel.sync(legacyProject.root);
  assert.equal(legacyProject.nodes.get('dup-cols').dataset.nativeOpacityScrub, undefined, 'a legacy project keeps the live scrub');
});

test('reseed draws Math.floor(random()*1e6) and sends it as ONE config request', async () => {
  const h = harness(); h.sync(); await settle();
  const original = Math.random; Math.random = () => 0.482913;
  try { fire(h.section, h.nodes.get('btn-dup-reseed'), 'click'); await settle(); } finally { Math.random = original; }
  const sent = writes(h);
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].payload, { command: 'object.duplicator.config', layerUid: 'L-a', seed: 482913 });
  assert.deepEqual(h.legacy, []);
});

test('the path-layer select keeps the existing duplicator.path command; the empty option clears the path', async () => {
  const h = harness({ dup: { mode: 'path' } }); h.sync(); await settle();
  h.nodes.get('dup-path-layer').value = 'L-b'; fire(h.section, h.nodes.get('dup-path-layer'), 'change'); await settle();
  h.nodes.get('dup-path-layer').value = ''; fire(h.section, h.nodes.get('dup-path-layer'), 'change'); await settle();
  assert.deepEqual(writes(h).map((r) => [r.operation, r.payload]), [
    [PATH, { command: 'object.duplicator.path', layerUid: 'L-a', pathLayerUid: 'L-b' }], [PATH, { command: 'object.duplicator.path', layerUid: 'L-a', pathLayerUid: null }]]);
  assert.deepEqual(h.legacy, []);
});

test('a refusal toasts the owner reason, changes nothing and shows the stored values again', async () => {
  const h = harness({ refuse: 'invalid_request', message: 'A duplicator setting is not valid.' }); h.sync(); await settle();
  h.nodes.get('dup-mode').value = 'path'; fire(h.section, h.nodes.get('dup-mode'), 'change'); await settle();
  assert.deepEqual(h.toasts, ['Duplicator refused: A duplicator setting is not valid.']);
  assert.equal(h.nodes.get('dup-mode').value, 'grid', 'the field is repainted from the stored descriptor');
  assert.equal(h.revision(), 4);
  assert.equal(h.refreshes(), 0);
  assert.equal(h.legacy.length, 0);
});

test('the controls with no native owner say so instead of writing a stub row, and are marked unavailable', async () => {
  const h = harness(); h.sync(); await settle();
  for (const id of ['btn-dup-add-effector', 'btn-dup-edit-source']) {
    assert.equal(h.nodes.get(id).dataset.nativeUnavailable, '1');
    assert.equal(h.nodes.get(id).attributes['aria-disabled'], 'true');
    fire(h.section, h.nodes.get(id), 'click');
  }
  h.nodes.get('dup-source-add').value = 'L-b'; fire(h.section, h.nodes.get('dup-source-add'), 'change');
  await settle();
  assert.deepEqual(h.toasts, ['Duplicator effectors is not available on this project yet.', 'Editing the duplicator source shape is not available on this project yet.',
    'The duplicator source pool is not available on this project yet.']);
  assert.equal(writes(h).length, 0);
  assert.deepEqual(h.legacy, []);
});

test('a legacy project is not intercepted: its field listeners still run and no native request is sent', async () => {
  const h = harness({ blocks: false }); Panel.sync(h.root);
  h.nodes.get('dup-rows').value = '5';
  const event = fire(h.section, h.nodes.get('dup-rows'), 'change');
  assert.equal(event.stopped, false);
  assert.deepEqual(h.legacy, ['dup-rows:change']);
  assert.equal(h.sent.length, 0);
});

test('the panel and its intents never call the legacy duplicator writers', async () => {
  const h = harness(); const calls = [];
  ['toggleLayerDuplicator', 'convertSelectionToObjectDuplicator', 'applyLayerDuplicator', 'dupRefresh', 'dupOf', 'pushUndo', 'loadFrame'].forEach((name) => { h.root[name] = () => calls.push(name); });
  h.sync(); await settle();
  h.nodes.get('dup-rows').value = '4'; fire(h.section, h.nodes.get('dup-rows'), 'change'); await settle();
  fire(h.section, h.nodes.get('btn-dup-reseed'), 'click'); await settle();
  assert.deepEqual(calls, []);
});
