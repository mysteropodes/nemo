'use strict';
// Actual Desktop/Core/Stage/Drop composition; controlled FS/native replies prove caller behavior only.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Desktop = require('../src/js/adapters/native-import-desktop.js');
const Stage = require('../src/js/adapters/native-import-png-stage.js');
const { makeSession } = require('./helpers/native-import-session.cjs');
const paper = require('./helpers/native-import-paper.cjs').load();
const rows = fs.readFileSync(path.join(__dirname, '../native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt'), 'utf8')
  .split('\n').filter(line => line && !line.startsWith('#')).map(line => JSON.parse(line));
const png = Buffer.from(rows.find(row => row.kind === 'png' && row.name === 'blue-2x1').hex, 'hex');
const file = name => Object.assign(new Blob([png], { type: 'image/png' }), { name });
const latch = () => { let resolve; const promise = new Promise(value => { resolve = value; }); return { promise, resolve }; };

function fixture(options) {
  const o = options || {}, h = makeSession({ canvasW: o.canvasW || 1, canvasH: o.canvasH || 1 });
  const files = new Map(), directories = new Set(), events = [], calls = [], toasts = [], listeners = new Map();
  const seen = {}, nodes = {}, optional = new Set(['btn-import-psd', 'psd-input', 'figma-token', 'figma-token-save', 'figma-url', 'figma-import-btn']);
  let mode = o.mode || 'success', sequence = 0, subscription = 1;
  for (const id of ['canvas-area', 'timeline-area']) nodes[id] = {
    addEventListener(kind, callback) { listeners.set(id + ':' + kind, callback); },
    removeEventListener(kind) { listeners.delete(id + ':' + kind); },
  };
  const memory = {
    async mkdir(name, config) {
      assert.equal(config.recursive, false);
      assert.equal(directories.has(name), false);
      directories.add(name); events.push(['mkdir', name]);
    },
    async writeTextFile(name, text) { files.set(name, text); events.push(['journal', JSON.parse(text).phase]); },
    async writeFile(name, bytes) {
      files.set(name, Uint8Array.from(bytes)); events.push(['png', name]);
      if (o.writeEntered) o.writeEntered.resolve();
      if (o.writeGate) await o.writeGate.promise;
    },
    async exists(name) { return files.has(name) || directories.has(name); },
    async remove(name, config) {
      assert.equal(config.recursive, false);
      events.push(['remove', name]);
      if (o.cleanupFailure && name.endsWith('.png')) throw new Error('controlled cleanup failure');
      if (directories.has(name)) {
        assert.equal([...files.keys()].some(key => key.startsWith(name + '/')), false);
        directories.delete(name);
      } else files.delete(name);
    },
  };
  h.session.isActive = () => true;
  h.session.readSubscriptionGeneration = () => subscription;
  const root = {
    state: { layers: [{ layerUid: 'L0' }], activeLayerIdx: 0, currentFrame: 0, activeSymbolId: null }, _layerSel: [0],
    NemoNativeObjectProject: { desktop: () => h.session, selectLayerUI() { assert.fail('existing-layer drop must not select a new layer'); } },
    NemoNativeObjectProjectFile: { projectionReadyForUI: () => true },
    paper: { project: { importSVG() { assert.fail('PNG drop must not use Paper parsing'); } }, Matrix: paper.Matrix,
      Path: paper.Path, CompoundPath: paper.CompoundPath, Segment: paper.Segment, Point: paper.Point },
    __TAURI__: { fs: memory, path: { tempDir: () => '/tmp/', join: (...parts) => parts.join('/').replace(/\/+/g, '/') } },
    crypto: { randomUUID: () => '00000000-0000-4000-8000-' + String(++sequence).padStart(12, '0') },
    SM: { t: key => ({ toastImageImported: 'Image imported', toastImagesImportedSuffix: ' images imported', toastInvalidLockedLayer: 'Locked layer' }[key] || key) },
    showToast: text => toasts.push(text),
    document: {
      readyState: 'complete',
      getElementById(id) { assert.ok(id in nodes || optional.has(id), 'unknown DOM id: ' + id); return nodes[id] || null; },
      addEventListener(kind, callback) { listeners.set('document:' + kind, callback); },
      removeEventListener(kind) { listeners.delete('document:' + kind); },
    },
  };
  for (const [name, suffix] of Object.entries({ Core: 'core', Svg: 'svg', Drop: 'drop', Figma: 'figma', Psd: 'psd', Montage: 'montage' })) {
    const module = require('../src/js/adapters/native-import-' + suffix + '.js');
    root['NemoNativeImport' + name] = ['Figma', 'Psd'].includes(name) ? { ...module, create(ports) {
      seen[name] = ports.stage;
      return module.create(ports);
    } } : module;
  }
  if (!o.noStage) root.NemoNativeImportPngStage = { create(ports) {
    seen.core = ports.core;
    const value = Stage.create(ports); seen.stage = value; return value;
  } };
  Object.defineProperty(h.session, 'importBatch', { value: async (payload, before) => {
    assert.equal(controller.core.isPending(), true, 'the real marker runs while Core is pending');
    calls.push(structuredClone(payload));
    const journal = [...files].find(([name, value]) => name.endsWith('obligation.json') && typeof value === 'string' &&
      JSON.parse(value).sources.some(source => payload.sources.some(row => row.sourceId === source.sourceId)));
    assert.ok(journal, 'durable obligation precedes the actual native invocation');
    assert.equal(JSON.parse(journal[1]).phase, 'dispatch_may_have_started');
    for (const source of payload.sources) assert.deepEqual(files.get(source.source.path), Uint8Array.from(png));
    if (mode === 'lost') throw new Error('controlled lost transport after invoke');
    if (mode === 'refused') throw new Error('Native command refused: unavailable');
    const revision = before.contentRevision + 1;
    h.state.contentRevision = revision; h.state.documentSnapshotId = 'snapshot-' + revision;
    return { apiVersion: 2, requestId: 'desktop-png', instanceId: before.instanceId, documentId: before.documentId,
      contentRevision: revision, ok: true, result: { applied: true, historyEntriesAdded: 1, layers: [], skipped: [],
        objects: payload.raster.map((row, i) => ({ sourceId: row.sourceId, strokeId: 'native-image:' + revision + ':' + i })),
        assets: payload.sources.map((row, i) => ({ sourceId: row.sourceId, assetId: 'native-asset:' + revision + ':' + i })) } };
  } });
  root.NemoNativeImportPngProvider = require('../src/js/adapters/native-import-png-provider.js');
  const controller = Desktop.install(root);
  async function drop(list) {
    let prevented = false;
    const result = await listeners.get('canvas-area:drop')({
      dataTransfer: { types: ['Files'], files: list }, currentTarget: { classList: { remove() {} } },
      preventDefault() { prevented = true; },
    });
    assert.equal(prevented, true);
    return result;
  }
  return { root, h, controller, files, directories, calls, events, toasts, seen, drop,
    cancelSubscription() { subscription++; }, setMode(value) { mode = value; } };
}

test('optional staging leaves old Desktop hosts unavailable and injects one actual stage into both callers', async () => {
  const absent = fixture({ noStage: true });
  assert.equal(absent.controller.image.available(), false);
  assert.equal((await absent.controller.image.importFiles([file('Blue.png')])).reason, 'native_drop_handle_absent');
  assert.equal(absent.calls.length, 0);
  const present = fixture();
  assert.equal(present.controller.image.available(), true);
  assert.equal(present.seen.core, present.controller.core);
  assert.equal(present.seen.Psd, present.seen.stage);
  assert.equal(present.seen.Figma, present.seen.stage);
  absent.controller.dispose(); present.controller.dispose();
});

test('ordinary PNG drop preserves active layer, frame, shrink-only fit and offsets in one actual Core batch', async () => {
  const f = fixture();
  try {
    const result = await f.drop([file('Blue.png'), file('Other.png')]);
    const out = result.applied[0].result;
    assert.equal(out.status, 'imported'); assert.equal(out.readback, 'confirmed');
    assert.equal(out.historyEntriesAdded, 1); assert.equal(f.calls.length, 1);
    assert.equal(f.h.state.contentRevision, 8);
    const payload = f.calls[0];
    assert.deepEqual(payload.layers, []); assert.deepEqual(payload.records, []);
    assert.deepEqual(payload.raster.map(row => row.entry.target), [0, 1].map(() => ({
      contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'L0', strokeId: '',
    })));
    assert.deepEqual(payload.raster.map(row => row.entry.item), [0, 1].map(i => ({
      assetId: 'image:' + i + ':pixels', x: 0.5 + i * 24, y: 0.5 + i * 24,
      width: 1, height: 0.5, opacity: 1, rotation: 0,
    })));
    assert.deepEqual(payload.sources.map(row => row.name), ['Blue.png', 'Other.png']);
    assert.equal(payload.sources.every(row => row.mediaType === 'image/png' && row.source.kind === 'file'), true);
    assert.equal(f.files.size, 0); assert.equal(f.directories.size, 0);
    assert.deepEqual(f.toasts, ['Import : 2 image(s)', '2 images imported']);
    assert.equal(f.root.state.activeLayerIdx, 0);
  } finally { f.controller.dispose(); }
});

test('actual attached mixed SVG and PNG drop refuses before either native group or any staging work', async () => {
  const f = fixture();
  try {
    const result = await f.drop([file('Blue.png'), { name: 'Vector.svg', type: 'image/svg+xml', text: async () => '<svg/>' }]);
    assert.equal(result.status, 'unavailable');
    assert.equal(result.reason, 'native_mixed_import_batch_unavailable');
    assert.deepEqual(result.applied, []);
    assert.equal(f.calls.length, 0); assert.equal(f.events.length, 0);
    assert.equal(f.h.state.contentRevision, 7);
    assert.deepEqual(f.toasts, ['Import PNG images separately from other file groups. Nothing was imported.']);
  } finally { f.controller.dispose(); }
});

test('native sequence dependency and non-PNG groups refuse before staging instead of becoming stills', async () => {
  for (const names of [['shot001.png', 'shot002.PNG'], ['take_9.png', 'take_11.png'], ['same1.png', 'same1.png']]) {
    const f = fixture();
    try {
      const result = await f.controller.image.importFiles(names.map(file));
      assert.equal(result.status, 'unavailable'); assert.equal(result.reason, 'native_sequence_unavailable');
      assert.equal(f.calls.length, 0); assert.equal(f.events.length, 0);
    } finally { f.controller.dispose(); }
  }
  const f = fixture();
  try {
    assert.equal((await f.controller.image.importFiles([file('Blue.png'), { name: 'photo.jpg', type: 'image/jpeg' }])).reason, 'native_png_only');
    assert.equal(f.events.length, 0);
    assert.equal((await f.controller.image.importFiles([file('shot1.png'), file('shot2.png'), file('Other.png')])).status, 'imported');
    assert.equal(f.calls.length, 1, 'a stray file prevents original sequence classification');
  } finally { f.controller.dispose(); }
});

test('real staging waits safely and rejects owner, subscription, selection, frame or disposal drift before invoke', async () => {
  for (const change of [f => { f.h.state.lifecycleGeneration++; }, f => f.cancelSubscription(),
    f => { f.root._layerSel = []; }, f => { f.root.state.currentFrame++; }, f => f.controller.dispose()]) {
    const entered = latch(), gate = latch(), f = fixture({ writeEntered: entered, writeGate: gate });
    const pending = f.controller.image.importFiles([file('Blue.png')]);
    try {
      assert.equal(await Promise.race([entered.promise.then(() => true), pending.then(() => false)]), true, 'the real stage must reach its controlled write');
      assert.equal((await f.controller.image.importFiles([file('Concurrent.png')])).status, 'busy');
      assert.equal((await f.controller.image.recover()).status, 'busy');
      change(f); gate.resolve();
      const result = await pending;
      assert.equal(result.status, 'stale');
      assert.deepEqual(result.staging, { status: 'cleaned' });
      assert.equal(f.calls.length, 0); assert.equal(f.files.size, 0); assert.equal(f.directories.size, 0);
    } finally { gate.resolve(); f.controller.dispose(); await pending; }
  }
});

test('postinvoke uncertainty blocks repeated drops and recovery; a new owner never deletes old staged resources', async () => {
  const f = fixture({ mode: 'lost' });
  try {
    const first = (await f.drop([file('Blue.png')])).applied[0].result;
    assert.equal(first.status, 'unconfirmed'); assert.equal(first.staging.status, 'retained');
    const oldPaths = [...f.files.keys()], oldDirectories = [...f.directories];
    assert.ok(oldPaths.some(name => name.endsWith('.png'))); assert.equal(f.calls.length, 1);
    assert.equal((await f.controller.image.importFiles([file('Again.png')])).status, 'unconfirmed');
    assert.equal((await f.controller.image.recover()).recovery, 'reopen-owner');
    assert.equal(f.calls.length, 1, 'there is no automatic or exact-request batch replay port');
    f.h.state.lifecycleGeneration++; f.setMode('success');
    const next = await f.controller.image.importFiles([file('Fresh.png')]);
    assert.equal(next.status, 'imported'); assert.equal(f.calls.length, 2);
    for (const name of oldPaths) assert.equal(f.files.has(name), true, 'old obligation is not swept on replacement');
    for (const name of oldDirectories) assert.equal(f.directories.has(name), true);
  } finally { f.controller.dispose(); }
});

test('cleanup failure remains separate from confirmed import success and exposes no resource path in its notice', async () => {
  const f = fixture({ cleanupFailure: true });
  try {
    const result = (await f.drop([file('Blue.png')])).applied[0].result;
    assert.equal(result.status, 'imported'); assert.equal(result.readback, 'confirmed');
    assert.equal(result.staging.status, 'retained'); assert.equal(result.staging.obligationPersisted, true);
    assert.equal(f.h.state.contentRevision, 8); assert.equal(f.calls.length, 1);
    assert.ok(f.toasts.includes('The import is confirmed, but temporary resources still need cleanup.'));
    assert.equal(f.toasts.some(text => text.includes('/tmp/') || text.includes('obligation.json')), false);
    assert.equal(f.controller.image.status(), null);
  } finally { f.controller.dispose(); }
});
