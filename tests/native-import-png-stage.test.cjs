'use strict';
// Actual staging + Core; only filesystem, canvas encoder and correlated native transport are controlled.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const staging = require('../src/js/adapters/native-import-png-stage.js');
const Core = require('../src/js/adapters/native-import-core.js');
const { makeSession, makePorts, layerRow } = require('./helpers/native-import-session.cjs');
const rows = fs.readFileSync(path.join(__dirname, '../native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt'), 'utf8')
  .split('\n').filter(line => line && !line.startsWith('#')).map(line => JSON.parse(line));
const png = Buffer.from(rows.find(row => row.name === 'blue-2x1' && row.kind === 'png').hex, 'hex');
const blob = bytes => new Blob([bytes || png], { type: 'image/png' });
function harness(extra) {
  extra = extra || {};
  const files = new Map(), directories = new Set(), events = [], nativeCalls = [];
  const h = makeSession();
  let nextId = 0, mode = extra.mode || 'success', removeFailure = null, journalFailure = null;
  const memory = {
    async mkdir(name, options) {
      events.push(['mkdir', name, options]);
      if (directories.has(name)) throw new Error('already exists');
      directories.add(name);
    },
    async writeFile(name, bytes) {
      events.push(['png', name]);
      files.set(name, Uint8Array.from(bytes));
      if (extra.partialWrite) throw new Error('partial write');
    },
    async writeTextFile(name, text) {
      events.push(['journal', name, JSON.parse(text).phase]);
      if (journalFailure && journalFailure(name, text)) throw new Error('journal failure');
      files.set(name, text);
    },
    async exists(name) { return files.has(name) || directories.has(name); },
    async remove(name, options) {
      events.push(['remove', name, options]);
      assert.equal(options.recursive, false);
      if (removeFailure && removeFailure(name)) throw new Error('remove failure');
      if (directories.has(name)) {
        if ([...files.keys()].some(key => key.startsWith(name + '/'))) throw new Error('not empty');
        directories.delete(name);
      } else files.delete(name);
    },
  };
  Object.defineProperty(h.session, 'importBatch', { value: async (payload, before) => {
    nativeCalls.push(structuredClone(payload));
    const journal = [...files.entries()].find(([key, text]) => key.endsWith('obligation.json') &&
      typeof text === 'string' && JSON.parse(text).sources.some(source => payload.sources.some(row => row.sourceId === source.sourceId)));
    assert.ok(journal, 'durable obligation precedes actual invocation');
    assert.equal(JSON.parse(journal[1]).phase, 'dispatch_may_have_started');
    for (const source of payload.sources.filter(source => source.mediaType === 'image/png')) assert.deepEqual(files.get(source.source.path), Uint8Array.from(png));
    if (mode === 'refused') throw new Error('Native command refused: unavailable');
    if (mode === 'lost') throw new Error('transport lost after invocation');
    if (extra.onInvoke) extra.onInvoke();
    if (mode === 'hold') return extra.gate;
    const revision = before.contentRevision + 1;
    const layers = payload.layers.map((row, i) => ({ sourceId: row.sourceId, layerUid: 'native-layer:' + revision + ':' + i }));
    const items = payload.records.concat(payload.raster || [], payload.text || []);
    const objects = items.map((row, i) => ({ sourceId: row.sourceId, strokeId: 'native-stroke:' + revision + ':' + i }));
    const assets = (payload.assets || []).concat(payload.sources).map((row, i) => ({ sourceId: row.sourceId, assetId: 'native-asset:' + revision + ':' + i }));
    h.state.contentRevision = revision;
    h.state.documentSnapshotId = 'snapshot-' + revision;
    payload.layers.forEach((row, i) => h.state.layers.push(layerRow(layers[i].layerUid, row.name)));
    const receipt = { apiVersion: 2, requestId: 'project-import-batch-1', instanceId: before.instanceId, documentId: before.documentId,
      contentRevision: revision, ok: true, result: { applied: true, historyEntriesAdded: 1, layers, objects, assets, skipped: payload.skipped || [] } };
    if (mode === 'uncorrelated') receipt.instanceId = 'foreign';
    return receipt;
  } });
  const core = Core.create(makePorts(h, extra.corePorts).ports);
  const stagingPorts = { core, fs: memory, path: { tempDir: () => '/tmp/', join: (...parts) => parts.join('/').split('//').join('/') },
    uuid: () => extra.uuid || '00000000-0000-4000-8000-' + String(++nextId).padStart(12, '0') };
  const port = staging.create(stagingPorts);
  return { ...h, files, directories, events, nativeCalls, core, port, stagingPorts,
    setMode(value) { mode = value; }, setRemoveFailure(fn) { removeFailure = fn; }, setJournalFailure(fn) { journalFailure = fn; } };
}
function batch(source) {
  return { layers: [{ sourceId: 'page', name: 'Pixels' }], records: [], skipped: [], unavailable: [], sources: [source],
    raster: [{ sourceId: 'bitmap', entry: { target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'page', strokeId: '' },
      item: { assetId: source.sourceId, x: 10, y: 20.5, width: 2, height: 1, opacity: 0.5, rotation: 0, isBitmapBrush: false } } }] };
}
async function staged(h, options) {
  const attempt = h.port.createAttempt(h.core.begin().ctx, options);
  const source = await attempt.stageFile('pixels', 'Blue.png', blob());
  return { attempt, source };
}

test('actual PNG staging publishes one source batch and removes PNGs then obligation then empty directory', async () => {
  const h = harness(), { attempt, source } = await staged(h);
  assert.deepEqual(Object.keys(source), ['sourceId', 'name', 'mediaType', 'source']);
  assert.equal(source.mediaType, 'image/png');
  assert.deepEqual(attempt.dimensions('pixels'), { width: 2, height: 1 });
  assert.equal(Object.isFrozen(attempt.dimensions('pixels')), true);
  assert.equal(Object.isFrozen(source.source), true);
  const value = batch(source);
  const pending = attempt.submit(value);
  value.raster[0].entry.item.x = 999;
  const result = await pending;
  assert.equal(h.nativeCalls[0].raster[0].entry.item.x, 10, 'batch is detached before its first await');
  assert.equal(result.status, 'imported');
  assert.equal(result.historyEntriesAdded, 1);
  assert.deepEqual(result.staging, { status: 'cleaned' });
  assert.equal(h.nativeCalls.length, 1);
  assert.equal(h.files.size, 0);
  assert.equal(h.directories.size, 0);
  const removed = h.events.filter(event => event[0] === 'remove').map(event => event[1]);
  assert.match(removed[0], /source-0000\.png$/);
  assert.match(removed[1], /obligation\.json$/);
  assert.equal(removed[2], removed[0].slice(0, removed[0].lastIndexOf('/')));
  assert.deepEqual(await attempt.abort(), { status: 'cleaned' });
});

test('mixed font descriptors remain in original source order beside staged PNGs', async () => {
  const h = harness(), { attempt, source } = await staged(h);
  const value = batch(source);
  value.sources.unshift({ sourceId: 'font', name: 'Roboto', mediaType: 'font/ttf', source: { kind: 'bundled', fontKey: 'Roboto-Regular' } });
  const result = await attempt.submit(value);
  assert.equal(result.status, 'imported');
  assert.deepEqual(result.assets.map(row => row.sourceId), ['font', 'pixels']);
  assert.deepEqual(h.nativeCalls[0].sources.map(row => row.sourceId), ['font', 'pixels']);
});

test('local stale and oversized refusal never invokes and safely cleans its own stages', async () => {
  for (const change of ['stale', 'too_large']) {
    const h = harness(change === 'too_large' ? { corePorts: { maxRequestBytes: 1 } } : {});
    const { attempt, source } = await staged(h);
    if (change === 'stale') h.state.contentRevision++;
    const result = await attempt.submit(batch(source));
    assert.equal(result.status, change === 'stale' ? 'stale' : 'too_large');
    assert.deepEqual(result.staging, { status: 'cleaned' });
    assert.equal(h.nativeCalls.length, 0);
    assert.equal(h.files.size, 0);
  }
});

test('every invoked nonsuccess retains immutable PNGs and durable obligation across a new staging instance', async () => {
  for (const mode of ['refused', 'lost', 'uncorrelated']) {
    const h = harness({ mode }), { attempt, source } = await staged(h);
    const result = await attempt.submit(batch(source));
    assert.equal(result.status, mode === 'refused' ? 'unavailable' : 'unconfirmed');
    assert.equal(result.staging.status, 'retained');
    assert.equal(result.staging.obligationPersisted, true);
    const frozen = [...h.files.entries()].map(([key, value]) => [key, typeof value === 'string' ? value : Buffer.from(value).toString('hex')]);
    await attempt.abort();
    const beforeReloadEvents = h.events.length;
    staging.create(h.stagingPorts);
    assert.equal(h.events.length, beforeReloadEvents, 'reconstruction does not read, sweep or replay');
    assert.deepEqual([...h.files.entries()].map(([key, value]) => [key, typeof value === 'string' ? value : Buffer.from(value).toString('hex')]), frozen);
    assert.equal(h.nativeCalls.length, 1);
  }
});

test('later success uses a distinct directory and cannot remove an older unresolved attempt', async () => {
  const h = harness({ mode: 'refused' }), first = await staged(h);
  await first.attempt.submit(batch(first.source));
  const oldDirectory = first.source.source.path.slice(0, first.source.source.path.lastIndexOf('/'));
  const oldFiles = [...h.files.keys()];
  h.setMode('success');
  const second = await staged(h);
  assert.notEqual(second.source.source.path, first.source.source.path);
  assert.equal((await second.attempt.submit(batch(second.source))).status, 'imported');
  assert.deepEqual([...h.files.keys()], oldFiles);
  assert.equal(h.directories.has(oldDirectory), true);
  assert.equal(h.nativeCalls.length, 2);
});

test('PNG size and IHDR dimensions refuse before whole bytes, canvas encoding or file allocation', async () => {
  const h = harness(), attempt = h.port.createAttempt(h.core.begin().ctx);
  let reads = 0, encoded = 0;
  await assert.rejects(attempt.stageFile('huge', 'Huge.png', { size: 16 * 1024 * 1024 + 1, slice() { reads++; }, arrayBuffer() { reads++; } }), { code: 'png_size' });
  await assert.rejects(attempt.stageCanvas('large', 'Large.png', { width: 8193, height: 1, toBlob() { encoded++; } }), { code: 'png_dimensions' });
  const invalid = Buffer.from(png); invalid.writeUInt32BE(8193, 16);
  await assert.rejects(attempt.stageFile('wide', 'Wide.png', blob(invalid)), { code: 'png_dimensions' });
  assert.equal(reads, 0);
  assert.equal(encoded, 0);
  assert.equal(h.files.size, 0);
  assert.equal(h.directories.size, 0);
  assert.equal(h.nativeCalls.length, 0);
});

test('PSD canvas encoder metadata must match the captured dimensions and never uses a data URL', async () => {
  const h = harness(), attempt = h.port.createAttempt(h.core.begin().ctx);
  const source = await attempt.stageCanvas('pixels', 'PSD.png', { width: 2, height: 1,
    toBlob(done, type) { assert.equal(type, 'image/png'); done(blob()); }, toDataURL() { throw new Error('forbidden'); } });
  assert.equal(source.source.kind, 'file');
  await attempt.abort();
  const changed = h.port.createAttempt(h.core.begin().ctx);
  await assert.rejects(changed.stageCanvas('pixels', 'Changed.png', { width: 3, height: 1, toBlob(done) { done(blob()); } }), { code: 'png_dimensions_changed' });
  assert.equal(h.nativeCalls.length, 0);
});

test('partial PNG writes clean before invoke and cancellation never dispatches', async () => {
  const h = harness({ partialWrite: true }), attempt = h.port.createAttempt(h.core.begin().ctx);
  await assert.rejects(attempt.stageFile('pixels', 'Blue.png', blob()), /partial write/);
  assert.deepEqual(await attempt.abort(), { status: 'cleaned' });
  assert.equal(h.files.size, 0);
  assert.equal(h.nativeCalls.length, 0);
  const other = harness(), signal = { aborted: false }, next = await staged(other, { signal });
  signal.aborted = true;
  const result = await next.attempt.submit(batch(next.source));
  assert.equal(result.status, 'invalid');
  assert.deepEqual(result.staging, { status: 'cleaned' });
  assert.equal(other.nativeCalls.length, 0);
});

test('failed PNG cleanup keeps obligation and committed success without any redispatch', async () => {
  const h = harness(), { attempt, source } = await staged(h);
  h.setRemoveFailure(name => name.endsWith('.png'));
  const result = await attempt.submit(batch(source));
  assert.equal(result.status, 'imported');
  assert.equal(result.staging.status, 'retained');
  assert.deepEqual(result.staging.failures, [source.source.path]);
  assert.equal(result.staging.obligationPersisted, true);
  assert.ok([...h.files.keys()].some(name => name.endsWith('obligation.json')));
  assert.equal(h.state.contentRevision, 8);
  await attempt.abort();
  assert.equal(h.nativeCalls.length, 1);
});

test('final directory cleanup failure recreates obligation and reports persistence failure honestly', async () => {
  for (const failJournal of [false, true]) {
    const h = harness(), { attempt, source } = await staged(h);
    const directory = source.source.path.slice(0, source.source.path.lastIndexOf('/'));
    h.setRemoveFailure(name => name === directory);
    if (failJournal) h.setJournalFailure((name, text) => JSON.parse(text).phase === 'cleanup_pending');
    const result = await attempt.submit(batch(source));
    assert.equal(result.status, 'imported');
    assert.equal(result.staging.status, 'retained');
    assert.equal(result.staging.obligationPersisted, !failJournal);
    assert.equal(h.files.has(source.source.path), false);
    assert.equal(h.files.has(directory + '/obligation.json'), !failJournal);
    assert.equal(h.nativeCalls.length, 1);
  }
});

test('attempts never reuse a colliding directory or submit a staged source twice', async () => {
  const h = harness({ mode: 'refused', uuid: '00000000-0000-4000-8000-000000000001' });
  const first = await staged(h);
  await first.attempt.submit(batch(first.source));
  const keys = [...h.files.keys()];
  const collision = h.port.createAttempt(h.core.begin().ctx);
  await assert.rejects(collision.stageFile('other', 'Other.png', blob()), /already exists/);
  await collision.abort();
  assert.deepEqual([...h.files.keys()], keys);
  assert.equal((await first.attempt.submit(batch(first.source))).status, 'invalid');
  assert.equal(h.nativeCalls.length, 1);
});

test('staging refuses a legacy Core that cannot report the invocation boundary', () => {
  const h = harness();
  assert.throws(() => staging.create({ ...h.stagingPorts, core: { submit() { throw new Error('must not invoke'); } } }), /ports unavailable/);
  assert.equal(h.events.length, 0);
});

test('a failed durable dispatch journal never invokes and leaves cleanup failure explicit', async () => {
  for (const failPngRemoval of [false, true]) {
    const h = harness(), { attempt, source } = await staged(h);
    h.setJournalFailure((name, text) => JSON.parse(text).phase === 'dispatch_may_have_started');
    if (failPngRemoval) h.setRemoveFailure(name => name === source.source.path);
    const result = await attempt.submit(batch(source));
    assert.equal(result.status, 'invalid');
    assert.equal(h.nativeCalls.length, 0);
    assert.equal(result.staging.status, failPngRemoval ? 'retained' : 'cleaned');
    if (failPngRemoval) {
      assert.equal(result.staging.obligationPersisted, true);
      const journal = [...h.files.values()].find(value => typeof value === 'string');
      assert.equal(JSON.parse(journal).phase, 'cleanup_pending');
    }
  }
});

test('abort during an invoked pending command retains files and never waits or retries native work', async () => {
  let entered, release;
  const reached = new Promise(resolve => { entered = resolve; });
  const gate = new Promise(resolve => { release = resolve; });
  const h = harness({ mode: 'hold', gate, onInvoke: entered }), { attempt, source } = await staged(h);
  const pending = attempt.submit(batch(source));
  await reached;
  const state = await attempt.abort();
  assert.equal(state.status, 'retained');
  assert.equal(h.files.has(source.source.path), true);
  assert.equal(h.nativeCalls.length, 1);
  release(undefined);
  assert.equal((await pending).status, 'unconfirmed');
  assert.equal(h.files.has(source.source.path), true);
  assert.equal(h.nativeCalls.length, 1);
});

test('selection and presentation drift during the actual dispatch journal await refuses before marking and cleans', async () => {
  for (const change of ['selection', 'presentation', 'available']) {
    let selection = ['L0'], available = true, release, entered;
    const gate = new Promise(resolve => { release = resolve; });
    const journalEntered = new Promise(resolve => { entered = resolve; });
    const h = harness({ corePorts: { selection: () => selection.slice(), available: () => available } });
    const { attempt, source } = await staged(h);
    const writeTextFile = h.stagingPorts.fs.writeTextFile;
    h.stagingPorts.fs.writeTextFile = async (name, text) => {
      if (JSON.parse(text).phase === 'dispatch_may_have_started') { entered(); await gate; }
      return writeTextFile(name, text);
    };
    const pending = attempt.submit(batch(source));
    await journalEntered;
    assert.equal(h.nativeCalls.length, 0, change);
    if (change === 'selection') selection = ['L9'];
    if (change === 'presentation') h.session.readPresentedObjectSession = () => null;
    if (change === 'available') available = false;
    release();
    const result = await pending;
    assert.equal(result.status, change === 'selection' ? 'stale' : 'unavailable', change);
    assert.deepEqual(result.staging, { status: 'cleaned' }, change);
    assert.equal(h.nativeCalls.length, 0, change);
    assert.equal(h.state.contentRevision, 7, change);
    assert.equal(h.files.size, 0, change);
    assert.equal(h.directories.size, 0, change);
    assert.equal(h.core.isPending(), false, change);
    assert.deepEqual(await attempt.abort(), { status: 'cleaned' }, change);
  }
});
