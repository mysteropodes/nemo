'use strict';
// G39 (#2376): Motion presets under native authority. Capture reads the selected layer's transform tracks through the native
// property read and keeps the baseline preset shape; apply sends ONE native command (one history entry) for every property;
// the library lives in application storage; the legacy layer model is never read or written. Real adapter, rig command and
// contract run against a fake session that answers reads from an in-memory track table.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Preset = require('../src/js/adapters/native-object-motion-preset.js');

const source = (name) => fs.readFileSync(path.join(__dirname, '../src/js', name), 'utf8');
const plain = (value) => JSON.parse(JSON.stringify(value));
const poisoned = () => { throw Error('legacy writer reached'); };
const curve = [{ x: 0, y: 0 }, { x: 0.5, y: 0.7, tx: 0.1, ty: 0.2 }, { x: 1, y: 1 }];
const rotationKeys = Array.from({ length: 11 }, (_, i) => ({ frame: i * 2, v: [i * 10] }));
const POSITION = [{ frame: 0, v: [10, 20], curvePoints: curve, hIn: [0, 0], hOut: [4, 5] }, { frame: 24, v: [300, 40], hold: true, lockTo: 'out' }];
const DIMENSIONS = { position: 2, anchor: 2, rotation: 1, scale: 2, opacity: 1 };

// layer uid -> property -> { keys, fixed }
function tracks() {
  return {
    'native-layer:1:0': { position: { keys: POSITION }, rotation: { keys: rotationKeys }, opacity: { keys: [], fixed: [75] } },
    'native-layer:2:0': {},
  };
}
function harness(options = {}) {
  const table = options.table || tracks(), sent = [], toasts = [], reads = [], stored = new Map(options.store || []);
  const pin = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 'snap-4',
    layers: [{ layerUid: 'native-layer:1:0', name: 'Ink' }, { layerUid: 'native-layer:2:0', name: 'Color' }] };
  const base = (view) => ({ instanceId: pin.instanceId, documentId: pin.documentId, atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId,
    target: view.target, property: view.property, dimension: DIMENSIONS[view.property] });
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (sent.length + 1),
    async readProperty(selector) {
      const view = selector.view, record = (table[view.target.layerUid] || {})[view.property] || { keys: [] };
      reads.push(view.kind);
      if (view.kind === 'storedProperty') {
        return Object.assign(base(view), { kind: 'storedProperty', items: [{ property: view.property, dimension: DIMENSIONS[view.property],
          authored: { availability: 'available', keyCount: record.keys.length, hasStaticOverride: !!record.fixed, expressionEnabled: false } }] });
      }
      if (view.kind === 'authoredKeys') {
        const items = record.keys.slice(view.offset, view.offset + view.limit), next = view.offset + items.length;
        return Object.assign(base(view), { kind: 'authoredKeys', trackPresent: record.keys.length > 0, items: plain(items), nextOffset: next < record.keys.length ? next : null });
      }
      return Object.assign(base(view), { kind: 'rawSample', frame: view.frame, value: record.fixed || [0] });
    },
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        if (options.refuse) return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId,
          contentRevision: pin.contentRevision, ok: false, error: { code: options.refuse, message: 'refused' } };
        return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId,
          contentRevision: pin.contentRevision + 1, ok: true, result: { applied: true, historyEntriesAdded: 1 } };
      },
      async reconcile() {},
    }) };
  return { session, sent, toasts, reads, stored, pin };
}
const adapter = (h) => Preset.create(h.session);

test('capture yields the baseline preset shape from the native tracks, paging past eight keys', async () => {
  const h = harness(), captured = await adapter(h).capture(0, 6);
  assert.deepEqual(plain(Object.keys(captured.motion)), ['position', 'rotation']);
  assert.deepEqual(plain(captured.motion.position), { keys: POSITION }, 'curve points, handles, hold and lock survive');
  assert.equal(captured.motion.rotation.keys.length, 11, 'two authored-key pages');
  assert.deepEqual(plain(captured.motionStatic), { opacity: [75] });
  assert.ok(h.reads.filter((k) => k === 'authoredKeys').length >= 3, 'rotation took two pages and position one');
  assert.equal(h.reads.includes('rawSample'), true, 'an unanimated static is sampled once');
  assert.deepEqual(plain(await adapter(h).capture(1, 0)), { motion: null, motionStatic: null }, 'a layer with no transform animation captures nothing');
});

test('apply is one native command listing every transform property; unheld ones are cleared', async () => {
  const h = harness(), captured = await adapter(h).capture(0, 6);
  const receipt = await adapter(h).apply(1, { label: 'Move', motion: captured.motion, motionStatic: captured.motionStatic });
  assert.equal(h.sent.length, 1, 'one request is one history entry');
  const request = h.sent[0];
  assert.equal(request.operation, 'command.document.object.property.preset.apply');
  assert.equal(request.expectedRevision, 4);
  assert.deepEqual(plain(request.payload.target), { contextId: 'scene-root', layerUid: 'native-layer:2:0' });
  const byName = Object.fromEntries(request.payload.properties.map((row) => [row.property, row]));
  assert.deepEqual(Object.keys(byName), ['position', 'anchor', 'rotation', 'scale', 'opacity']);
  assert.deepEqual(plain(byName.position.keys), POSITION, 'exact keys, no frame offset (the baseline replaces the tracks as stored)');
  assert.equal(byName.rotation.keys.length, 11);
  assert.deepEqual(plain(byName.opacity), { property: 'opacity', keys: [], static: [75] });
  assert.deepEqual(plain(byName.anchor), { property: 'anchor', keys: [] });
  assert.equal(receipt.applied, true); assert.equal(receipt.historyEntriesAdded, 1);
});

test('a stored baseline preset is cleaned to the native transform and names what it skips', async () => {
  const legacy = { label: 'Old', motion: { position: { keys: [{ frame: 0, v: [1, 2], curvePoints: null, lockTo: null, ease: 'x' }] }, order: { keys: [] }, positionZ: { keys: [] } }, motionStatic: { rotation: 15, volume: [1] } };
  assert.deepEqual(Preset.skipped(legacy), ['order', 'positionZ', 'volume']);
  const rows = Preset.rows(legacy), byName = Object.fromEntries(rows.map((row) => [row.property, row]));
  assert.deepEqual(plain(byName.position.keys), [{ frame: 0, v: [1, 2] }], 'null members and unknown members are dropped');
  assert.deepEqual(plain(byName.rotation.static), [15]);
  const h = harness(); const receipt = await adapter(h).apply(1, legacy);
  assert.deepEqual(plain(receipt.skipped), ['order', 'positionZ', 'volume']);
});

test('a refused apply surfaces a typed error and a missing layer never reaches the owner', async () => {
  const refused = harness({ refuse: 'not_found' });
  await assert.rejects(adapter(refused).apply(1, { motion: { position: { keys: POSITION } } }), (error) => error.code === 'not_found');
  const h = harness();
  await assert.rejects(adapter(h).apply(9, { motion: {} }), (error) => error.code === 'not_found');
  assert.equal(h.sent.length, 0);
});

// ---- the panel (real DOM callbacks of motion-preset-picker.js) ----
function element() {
  return { children: [], events: {}, style: {}, className: '', textContent: '', title: '',
    appendChild(child) { this.children.push(child); return child; }, addEventListener(kind, fn) { this.events[kind] = fn; },
    set innerHTML(value) { assert.equal(value, ''); this.children = []; }, get innerHTML() { return ''; } };
}
function panel(options = {}) {
  const h = harness(options), nodes = new Map(), prompts = [];
  const state = { activeLayerIdx: options.layer === undefined ? 0 : options.layer, currentFrame: 6 };
  Object.defineProperty(state, 'layers', { get: poisoned });
  const context = { state, console, SM: { t: (k) => k }, showToast: (m) => h.toasts.push(m),
    prompt: (label, fallback) => { prompts.push([label, fallback]); return options.name === undefined ? 'Captured' : options.name; },
    document: { getElementById: (id) => { if (!nodes.has(id)) nodes.set(id, element()); return nodes.get(id); }, createElement: element },
    localStorage: { getItem: (k) => h.stored.get(k) || null, setItem: (k, v) => { h.stored.set(k, v); } },
    NemoNativeObjectProject: { desktop: () => h.session }, NemoNativeObjectMotionPreset: Preset,
    n20AllowLegacyWrite: () => false, saveAllLayerFrames: poisoned, pushUndoLayers: poisoned, loadFrame: poisoned, updateUI: poisoned };
  context.window = context;
  vm.runInNewContext(source('motion-preset-picker.js'), context, { filename: 'motion-preset-picker.js' });
  const settle = async () => { for (let i = 0; i < 12; i += 1) await new Promise((resolve) => setImmediate(resolve)); };
  const click = async (node) => { node.events.click({ stopPropagation() {}, preventDefault() {} }); await settle(); };
  return Object.assign(h, { nodes, prompts, click, settle, state, grid: () => nodes.get('motion-preset-grid') });
}
const library = (h) => JSON.parse(h.stored.get('nemo-motion-presets'));

test('panel: Save captures the native layer into the application-storage library, then applies to another layer as one command', async () => {
  const h = panel();
  await h.click(h.nodes.get('btn-mp-save'));
  const ids = Object.keys(library(h));
  assert.equal(ids.length, 1);
  assert.deepEqual(h.prompts[0], ['Nom du preset', 'Ink'], 'the baseline default name is the layer name');
  const saved = library(h)[ids[0]];
  assert.equal(saved.label, 'Captured'); assert.equal(saved.categoryId, 'default');
  assert.deepEqual(Object.keys(saved.motion), ['position', 'rotation']); assert.deepEqual(saved.motionStatic, { opacity: [75] });
  assert.ok(h.toasts.some((t) => /enregistr/.test(t)));
  assert.equal(h.grid().children.length, 1, 'the new preset is listed');
  h.state.activeLayerIdx = 1;
  await h.click(h.grid().children[0]);
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].payload.target.layerUid, 'native-layer:2:0');
  assert.ok(h.toasts.some((t) => /appliqu/.test(t)));
});

test('panel: nothing to capture, a cancelled name, a refusal and Delete leave the library and the legacy model intact', async () => {
  const empty = panel({ layer: 1 });
  await empty.click(empty.nodes.get('btn-mp-save'));
  assert.deepEqual(empty.toasts, ['hsLayerNoTransform']);
  assert.equal(empty.stored.has('nemo-motion-presets'), false);
  const cancelled = panel({ name: '' });
  await cancelled.click(cancelled.nodes.get('btn-mp-save'));
  assert.equal(cancelled.stored.has('nemo-motion-presets'), false);
  const preset = { saved: { label: 'Move', categoryId: 'default', motion: { position: { keys: POSITION } }, motionStatic: null } };
  const refused = panel({ refuse: 'invalid_request', store: [['nemo-motion-presets', JSON.stringify(preset)]] });
  await refused.click(refused.grid().children[0]);
  assert.ok(refused.toasts.some((t) => /refused/.test(t)));
  const listed = panel({ store: [['nemo-motion-presets', JSON.stringify(preset)]] });
  const del = listed.grid().children[0].children[2];
  await listed.click(del);
  assert.deepEqual(library(listed), {}, 'Delete removes it from application storage');
  assert.equal(listed.sent.length, 0, 'Delete never reaches the document');
});
