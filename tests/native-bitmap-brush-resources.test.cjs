'use strict';
/* Lane-7 brush resource adapter: publishing a brush bitmap to the native owner and the canvas -> PNG -> catalog route.
 * The oracle (tests/native-bitmap-brush-resources-fixtures/canvas-oracle.json) is produced by the VERBATIM bakeToCanvas
 * and image-budget.js of the baseline (commit fbe4b8a3); the owner and session are doubles that follow the documented
 * receipt rules only (see the support file). */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Adapter = require('../src/js/adapters/native-bitmap-brush-resources.js');
const Contract = require('../src/js/adapters/native-bitmap-brush-resources-contract.js');
const { createFixture, forbidLegacyWriters, pngBytes, PUBLISH } = require('./native-bitmap-brush-resources-support.cjs');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-bitmap-brush-resources-fixtures', 'canvas-oracle.json'), 'utf8'));
const ROW = (name) => oracle.rows.find((r) => r.name === name);
const BAKE = ROW('straight 100px'); // 241 x 41, 39524 baseline bytes
const ASSETS = {
  'asset-project-asset-1': { width: 241, height: 41 },
  'asset-project-asset-2': { width: 241, height: 41 },
  'asset-project-asset-big': { width: 4096, height: 4096 },
};
const SIZE = { width: 241, height: 41 };
const trap = () => forbidLegacyWriters(['SMEngineBridge', 'NemoImageBudget', 'SMBitmapBrush', 'paper', 'state', 'localStorage', 'registeredImageIds', 'engine', 'userLayers', 'SMMotion']);
const fixture = (options) => createFixture(Object.assign({ assets: ASSETS }, options));
async function code(promise) { try { await promise; } catch (error) { return error; } throw new Error('expected a rejection'); }

test('the oracle is the baseline: 21 verbatim bakes, every canvas within 4096 and every cost width * height * 4', () => {
  assert.equal(oracle.provenance.baseline, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  assert.equal(oracle.provenance.maxTexLine, 'var MAX_TEX = 4096;');
  assert.equal(oracle.provenance.imageBudgetSha256, 'c93eb2d0ef588abd31d7c2ca2d4d04d255351447e1c16e1ceb2465c22060966f');
  assert.equal(oracle.rows.length, 21);
  assert.deepEqual([BAKE.canvas, BAKE.baselineBytes], [{ width: 241, height: 41 }, 39524]);
  assert.deepEqual([ROW('size 400 dot').canvas, ROW('size 400 dot').baselineBytes], [{ width: 1016, height: 1016 }, 4129024]);
  assert.deepEqual([ROW('huge both axes 20000').canvas, ROW('huge both axes 20000').baselineBytes], [{ width: 4096, height: 4096 }, 67108864]);
  assert.deepEqual(oracle.stats, { maxW: 4096, maxH: 4096, atLimit: 9 });
});

test('checkSize accepts exactly what the baker can produce and returns the baseline byte cost', () => {
  for (const row of oracle.rows) {
    const size = Adapter.checkSize(row.canvas.width, row.canvas.height);
    assert.equal(size.bytes, row.baselineBytes, row.name);
    assert.equal(Contract.bytesFor(row.canvas.width, row.canvas.height), row.baselineBytes, row.name);
  }
  // The verbatim noteRegistered sequences (an overwritten id keeps only its last size).
  for (const seq of oracle.sequences) {
    const last = new Map();
    for (const step of seq.steps) last.set(step.id, Contract.bytesFor(step.w, step.h));
    assert.equal([...last.values()].reduce((a, b) => a + b, 0), seq.totalBytes);
    assert.equal(last.size, seq.count);
  }
  for (const [w, h] of [[0, 1], [1, 0], [4097, 1], [1, 4097], [-1, 5], [1.5, 2], [NaN, 2], [Infinity, 2], ['3', 3], [undefined, 3], [null, null]]) {
    assert.throws(() => Adapter.checkSize(w, h), (e) => e.code === 'invalid_request' && e.reason === 'local_validation', String([w, h]));
  }
  assert.deepEqual(Adapter.checkSize(4096, 4096), { width: 4096, height: 4096, bytes: 67108864 });
  assert.deepEqual(Adapter.checkSize(1, 1), { width: 1, height: 1, bytes: 4 });
});

test('publishAsset sends ONE command with the pinned request and returns the correlated, frozen receipt', async () => {
  const guard = trap();
  const f = fixture();
  const client = Adapter.create(f.session);
  const receipt = await client.publishAsset('i1:abc', 'asset-project-asset-1', SIZE);
  guard.restore();
  assert.deepEqual(f.owner.log.received, [{
    apiVersion: 2, requestId: 'project-brush-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: PUBLISH,
    payload: { command: 'object.brush.bitmap.publish', documentSnapshotId: 'snapshot-7', assetId: 'asset-project-asset-1', brushId: 'i1:abc', width: 241, height: 41 },
  }]);
  assert.deepEqual(receipt, {
    requestId: 'project-brush-1', contentRevision: 7, applied: true, historyEntriesAdded: 0, brushId: 'i1:abc', version: '1', width: 241, height: 41, bytes: BAKE.baselineBytes,
  });
  assert.equal(Object.isFrozen(receipt), true);
  assert.deepEqual(f.events.reconciled, [7], 'the session is reconciled at the receipt revision, which a publication does not move');
  assert.equal(f.state.revision, 7);
});

test('the same asset again reuses the retained bitmap; another asset under the id is the next version', async () => {
  const f = fixture();
  const client = Adapter.create(f.session);
  const first = await client.publishAsset('i1:abc', 'asset-project-asset-1', SIZE);
  const again = await client.publishAsset('i1:abc', 'asset-project-asset-1', SIZE);
  const other = await client.publishAsset('i1:abc', 'asset-project-asset-2', SIZE);
  assert.deepEqual([first, again, other].map((r) => [r.applied, r.version, r.contentRevision]), [[true, '1', 7], [false, '1', 7], [true, '2', 7]]);
  assert.equal(f.owner.log.executed.length, 3);
  assert.deepEqual(f.owner.log.received.map((r) => r.requestId), ['project-brush-1', 'project-brush-2', 'project-brush-3']);
});

test('the pin is read at execution: queued calls send the revision and snapshot current when each runs', async () => {
  const f = fixture();
  const client = Adapter.create(f.session);
  const a = client.publishAsset('b1', 'asset-project-asset-1', SIZE);
  const edit = f.session.runFontResource(async () => { f.state.revision = 8; });
  const b = client.publishAsset('b2', 'asset-project-asset-2', SIZE);
  await Promise.all([a, edit, b]);
  assert.deepEqual(f.owner.log.received.map((r) => [r.expectedRevision, r.payload.documentSnapshotId]), [[7, 'snapshot-7'], [8, 'snapshot-8']]);
});

test('the alias route and the font-named route are the same serialized route; without either the answer is typed unavailability', async () => {
  const alias = fixture({ aliasRoute: true });
  assert.equal((await Adapter.create(alias.session).publishAsset('b1', 'asset-project-asset-1', SIZE)).applied, true);
  const bare = fixture({ noRoute: true });
  const error = await code(Adapter.create(bare.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'resource_route_unavailable']);
  assert.equal(bare.owner.log.received.length, 0);
  const inactive = fixture();
  inactive.state.active = false;
  const none = await code(Adapter.create(inactive.session).readScene({ frame: 0 }));
  assert.deepEqual([none.code, none.reason], ['unavailable', 'no_native_project']);
  assert.equal(inactive.owner.log.received.length, 0);
  const busy = fixture();
  busy.session.runFontResource = () => Promise.reject(new Error('Native font owner busy or unavailable'));
  const refused = await code(Adapter.create(busy.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
  assert.deepEqual([refused.code, refused.reason], ['unavailable', 'native_project_busy']);
});

test('a failure inside the route after it started is not mislabelled as a busy route', async () => {
  const f = fixture();
  const live = f.session.info;
  let reads = 0;
  f.session.info = () => { reads += 1; if (reads > 1) throw new Error('session info exploded'); return live(); };
  const error = await code(Adapter.create(f.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
  assert.equal(error.message, 'session info exploded');
  assert.equal(error.code, undefined, 'an unexpected failure reaches the caller unchanged');
  assert.equal(f.owner.log.received.length, 0);
});

test('local refusals happen before any request is minted or sent', async () => {
  const f = fixture();
  const client = Adapter.create(f.session);
  const bad = [
    ['blank brush id', ' ', 'asset-project-asset-1', SIZE], ['empty brush id', '', 'asset-project-asset-1', SIZE], ['NUL in brush id', 'a\0b', 'asset-project-asset-1', SIZE],
    ['256-byte brush id', 'x'.repeat(256), 'asset-project-asset-1', SIZE], ['two-byte characters over 255 bytes', 'é'.repeat(128), 'asset-project-asset-1', SIZE],
    ['non-string brush id', 5, 'asset-project-asset-1', SIZE], ['blank asset id', 'b1', '  ', SIZE], ['missing asset id', 'b1', undefined, SIZE],
    ['no size', 'b1', 'asset-project-asset-1', undefined], ['width over 4096', 'b1', 'asset-project-asset-1', { width: 4097, height: 4 }],
    ['fractional height', 'b1', 'asset-project-asset-1', { width: 4, height: 4.5 }], ['zero width', 'b1', 'asset-project-asset-1', { width: 0, height: 4 }],
  ];
  for (const [label, brush, asset, size] of bad) {
    const error = await code(client.publishAsset(brush, asset, size));
    assert.deepEqual([error.code, error.reason], ['invalid_request', 'local_validation'], label);
  }
  assert.deepEqual(f.owner.log.received, []);
  assert.deepEqual(f.events.ids, [], 'no request id was consumed');
  // The byte bounds are inclusive: 255 bytes pass.
  assert.equal((await client.publishAsset('x'.repeat(255), 'asset-project-asset-1', SIZE)).applied, true);
  assert.equal((await client.publishAsset('é'.repeat(127), 'asset-project-asset-1', SIZE)).applied, true);
  assert.equal((await client.publishAsset('data:looks-binary', 'asset-project-asset-1', SIZE)).applied, true);
});

test('a native refusal is passed through typed, reconciled and never retried', async () => {
  const cases = [
    [{ code: 'not_found', reason: 'missing_asset' }, { assetId: 'asset-missing', size: SIZE }],
    [{ code: 'invalid_request', reason: 'dimension_mismatch' }, { assetId: 'asset-project-asset-1', size: { width: 240, height: 41 } }],
  ];
  for (const [expected, input] of cases) {
    const f = fixture();
    const error = await code(Adapter.create(f.session).publishAsset('b1', input.assetId, input.size));
    assert.deepEqual([error.code, error.reason, error.requestId, error.contentRevision], [expected.code, expected.reason, 'project-brush-1', 7]);
    assert.equal(f.owner.log.received.length, 1, 'a refusal is not retried');
    assert.deepEqual(f.events.reconciled, [7]);
  }
  const stale = fixture({ refuse: () => ({ code: 'stale_revision', message: 'stale' }) });
  const error = await code(Adapter.create(stale.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
  assert.deepEqual([error.code, error.reason], ['stale_revision', null]);
});

test('an ambiguous transport failure re-sends the SAME request: one execution, the stored receipt, a late reconcile', async () => {
  for (const fault of ['before', 'after']) {
    const f = fixture({ faults: [fault] });
    const receipt = await Adapter.create(f.session).publishAsset('b1', 'asset-project-asset-1', SIZE);
    assert.equal(receipt.applied, true, fault);
    assert.equal(f.owner.log.executed.length, 1, fault);
    assert.equal(f.owner.log.received.length, 2, fault);
    assert.deepEqual(f.owner.log.received[0], f.owner.log.received[1], fault);
    assert.deepEqual(f.events.reconciled, [undefined], 'the answer to a second send is a stored receipt: reconciled without the revision equality');
  }
});

test('two lost deliveries are indeterminate (a publication moves no revision to tell them apart); replay returns the stored receipt', async () => {
  // Lost after execution, document edited since: the owner answers the STORED receipt (original revision) and executes nothing again.
  const executed = fixture({ faults: ['after', 'after'] });
  const lost = await code(Adapter.create(executed.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
  assert.equal(lost.code, 'brush_publish_indeterminate');
  assert.equal(lost.requestId, 'project-brush-1');
  assert.equal(executed.owner.log.executed.length, 1);
  executed.state.revision += 2;
  const stored = await lost.replay();
  assert.equal(executed.owner.log.executed.length, 1, 'the replay executes at most once');
  assert.deepEqual([stored.requestId, stored.contentRevision, stored.applied, stored.version], ['project-brush-1', 7, true, '1']);
  assert.deepEqual(executed.events.reconciled, [undefined, undefined], 'neither the failure nor the replay reconciles with a revision');
  // Lost before execution, document unchanged: the replay is the one execution.
  const undelivered = fixture({ faults: ['before', 'before'] });
  const never = await code(Adapter.create(undelivered.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
  assert.equal(never.code, 'brush_publish_indeterminate');
  assert.equal(undelivered.owner.log.executed.length, 0);
  assert.equal((await never.replay()).applied, true);
  assert.equal(undelivered.owner.log.executed.length, 1);
  // Lost before execution, document edited since: the replay is a typed stale refusal, never a publication at the new revision.
  const late = fixture({ faults: ['before', 'before'] });
  const gone = await code(Adapter.create(late.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
  late.state.revision += 1;
  const refused = await code(gone.replay());
  assert.deepEqual([refused.code, late.owner.log.executed.length], ['stale_revision', 0]);
  const f = fixture({ faults: ['after', 'after'] });
  const error = await code(Adapter.create(f.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
  f.state.document = 'document-B';
  const stale = await code(error.replay());
  assert.equal(stale.code, 'stale_owner');
  assert.equal(f.owner.log.received.length, 2, 'a replay against another project sends nothing');
});

test('a receipt that does not belong to the request is uncorrelated and resynchronizes the session', async () => {
  const flaws = [
    ['foreign request id', (r) => { r.requestId = 'foreign'; }], ['foreign instance', (r) => { r.instanceId = 'instance-Z'; }],
    ['revision advanced', (r) => { r.contentRevision = 8; }], ['history entry claimed', (r) => { r.result.historyEntriesAdded = 1; }],
    ['another brush', (r) => { r.result.brushId = 'other'; }], ['another width', (r) => { r.result.width = 240; r.result.bytes = 240 * 41 * 4; }],
    ['wrong cost', (r) => { r.result.bytes += 4; }], ['extra member', (r) => { r.result.padding = 1; }], ['missing version', (r) => { delete r.result.version; }],
    ['numeric version', (r) => { r.result.version = 1; }], ['zero version', (r) => { r.result.version = '0'; }], ['applied not boolean', (r) => { r.result.applied = 1; }],
  ];
  for (const [label, flaw] of flaws) {
    const f = fixture({ respond: (response) => { flaw(response); return response; } });
    const error = await code(Adapter.create(f.session).publishAsset('b1', 'asset-project-asset-1', SIZE));
    assert.equal(error.code, 'uncorrelated', label);
    assert.deepEqual(f.events.reconciled, [undefined], label + ': resynchronized once, without a revision');
  }
});

test('publishCanvas forwards the exact PNG bytes and ids: canvas -> encoder -> host byte route -> catalog -> publish', async () => {
  const guard = trap();
  const f = fixture();
  const png = pngBytes(241, 41, 100);
  const calls = [];
  const canvas = { width: 241, height: 41, toBlob(cb, type) { calls.push(['toBlob', type]); cb(new Blob([png])); } };
  const client = Adapter.create(f.session, {
    stage(bytes, details) { calls.push(['stage', bytes, details]); return Promise.resolve('/stage/brush-1.png'); },
    assets: { attachPath(file) { calls.push(['attach', file]); return Promise.resolve({ applied: true, assetId: 'asset-project-asset-1' }); } },
  });
  const done = await client.publishCanvas('i1:abc', canvas);
  guard.restore();
  assert.deepEqual(calls.map((c) => c[0]), ['toBlob', 'stage', 'attach']);
  assert.equal(calls[0][1], 'image/png');
  assert.deepEqual(Array.from(calls[1][1]), Array.from(png), 'the staged bytes are the encoder bytes, untouched');
  assert.deepEqual(calls[1][2], { brushId: 'i1:abc', width: 241, height: 41 });
  assert.equal(calls[2][1], '/stage/brush-1.png');
  assert.equal(f.owner.log.received.length, 1);
  assert.equal(f.owner.log.received[0].payload.assetId, 'asset-project-asset-1');
  assert.deepEqual([done.brushId, done.assetId, done.publish.version, done.publish.bytes], ['i1:abc', 'asset-project-asset-1', '1', 39524]);
  assert.equal(Object.isFrozen(done), true);
});

test('the canvas is only read for its size and its encoding; nothing else of it (or the page) is touched', async () => {
  const touched = [];
  const real = { width: 241, height: 41, toBlob(cb) { cb(new Blob([pngBytes(241, 41)])); }, getContext() { throw new Error('pixels must not be read'); }, toDataURL() { throw new Error('no data url'); } };
  const canvas = new Proxy(real, { get(target, key) { touched.push(String(key)); return target[key]; } });
  const f = fixture();
  const client = Adapter.create(f.session, { stage: () => '/s.png', assets: { attachPath: () => ({ assetId: 'asset-project-asset-1' }) } });
  await client.publishCanvas('b1', canvas);
  assert.deepEqual([...new Set(touched)].sort(), ['height', 'toBlob', 'width']);
});

test('an injected encoder replaces the DOM one; the bytes it returns are validated before anything is staged', async () => {
  const effects = [];
  const ports = (bytes) => ({
    encodePng: () => Promise.resolve(bytes), stage: () => { effects.push('stage'); return '/s.png'; },
    assets: { attachPath: () => { effects.push('attach'); return { assetId: 'asset-project-asset-1' }; } },
  });
  const canvas = { width: 241, height: 41 };
  const flawed = [
    ['not bytes', 'png', 'invalid_request', 'local_validation'], ['too short', new Uint8Array(10), 'invalid_request', 'local_validation'],
    ['bad signature', Object.assign(pngBytes(241, 41), { 0: 0 }), 'invalid_request', 'local_validation'],
    ['no IHDR', Object.assign(pngBytes(241, 41), { 12: 0x58 }), 'invalid_request', 'local_validation'],
    ['width differs from the canvas', pngBytes(240, 41), 'invalid_request', 'png_dimension_mismatch'],
    ['height differs from the canvas', pngBytes(241, 40), 'invalid_request', 'png_dimension_mismatch'],
    ['swapped sides', pngBytes(41, 241), 'invalid_request', 'png_dimension_mismatch'],
  ];
  for (const [label, bytes, expectedCode, reason] of flawed) {
    const f = fixture();
    const error = await code(Adapter.create(f.session, ports(bytes)).publishCanvas('b1', canvas));
    assert.deepEqual([error.code, error.reason], [expectedCode, reason], label);
    assert.deepEqual(effects, [], label + ': nothing staged or attached');
    assert.equal(f.owner.log.received.length, 0, label);
  }
  const f = fixture();
  await Adapter.create(f.session, ports(pngBytes(241, 41))).publishCanvas('b1', canvas);
  assert.deepEqual(effects, ['stage', 'attach']);
});

test('missing host routes and unsupported canvases are typed and have no side effect', async () => {
  const effects = [];
  const canvas = { width: 241, height: 41, toBlob(cb) { effects.push('toBlob'); cb(new Blob([pngBytes(241, 41)])); } };
  const assets = { attachPath: () => { effects.push('attach'); return { assetId: 'asset-project-asset-1' }; } };
  const f = fixture();
  const cases = [
    ['no byte route', {}, canvas, 'unavailable', 'brush_stage_route_unavailable'],
    ['no catalog adapter', { stage: () => '/s.png' }, canvas, 'unavailable', 'asset_adapter_unavailable'],
    ['catalog adapter without attachPath', { stage: () => '/s.png', assets: {} }, canvas, 'unavailable', 'asset_adapter_unavailable'],
    ['canvas over 4096', { stage: () => '/s.png', assets }, { width: 4097, height: 5, toBlob() { effects.push('toBlob'); } }, 'invalid_request', 'local_validation'],
    ['no canvas', { stage: () => '/s.png', assets }, null, 'invalid_request', 'local_validation'],
    ['blank brush id', { stage: () => '/s.png', assets }, canvas, 'invalid_request', 'local_validation', ' '],
    ['canvas without an encoder', { stage: () => '/s.png', assets }, { width: 241, height: 41 }, 'unavailable', 'png_encoder_unavailable'],
    ['encoder yields no blob', { stage: () => '/s.png', assets }, { width: 241, height: 41, toBlob(cb) { cb(null); } }, 'unavailable', 'png_encode_failed'],
  ];
  for (const [label, ports, input, expectedCode, reason, brush] of cases) {
    const error = await code(Adapter.create(f.session, ports).publishCanvas(brush || 'b1', input));
    assert.deepEqual([error.code, error.reason], [expectedCode, reason], label);
  }
  assert.deepEqual(effects, [], 'no canvas was encoded, nothing was attached for a refused call');
  assert.equal(f.owner.log.received.length, 0);
});

test('a failed stage or catalog step stops before the publication; a refused publication reports the held asset', async () => {
  const canvas = { width: 241, height: 41, toBlob(cb) { cb(new Blob([pngBytes(241, 41)])); } };
  const f = fixture();
  const empty = await code(Adapter.create(f.session, { stage: () => '', assets: { attachPath: () => { throw new Error('no'); } } }).publishCanvas('b1', canvas));
  assert.deepEqual([empty.code, empty.reason], ['unavailable', 'brush_stage_failed']);
  const duplicate = await code(Adapter.create(f.session, { stage: () => '/s.png', assets: { attachPath: () => ({ applied: false, assetId: null }) } }).publishCanvas('b1', canvas));
  assert.deepEqual([duplicate.code, duplicate.reason], ['asset_id_unavailable', 'duplicate_attach_without_id']);
  assert.equal(f.owner.log.received.length, 0, 'no publication without an asset id');
  const refused = await code(Adapter.create(f.session, { stage: () => '/s.png', assets: { attachPath: () => ({ applied: true, assetId: 'asset-missing' }) } }).publishCanvas('b1', canvas));
  assert.deepEqual([refused.code, refused.reason, refused.assetId], ['not_found', 'missing_asset', 'asset-missing']);
  const gate = await code(Adapter.create(f.session, { stage: () => '/s.png', assets: { attachPath: () => Promise.reject(Object.assign(new Error('closed'), { code: 'unavailable', reason: 'asset_host_route_unavailable' })) } }).publishCanvas('b1', canvas));
  assert.deepEqual([gate.code, gate.reason], ['unavailable', 'asset_host_route_unavailable']);
  assert.equal(f.owner.log.received.length, 1, 'only the refused publication reached the owner');
});

test('desktop(): one binding per host, routes read at call time, the catalog gate is never opened by this client', async () => {
  const f = fixture();
  const seen = [];
  const host = {
    NemoNativeObjectProject: { desktop: () => Object.assign({ blocksLegacy: () => true }, f.session) },
    NemoNativeObjectRasterAsset: { desktop: (...args) => { seen.push(args.length); return { attachPath: () => ({ assetId: 'asset-project-asset-1' }) }; } },
  };
  const first = Adapter.desktop(host);
  assert.equal(Adapter.desktop(host), first);
  const canvas = { width: 241, height: 41, toBlob(cb) { cb(new Blob([pngBytes(241, 41)])); } };
  const unrouted = await code(first.publishCanvas('b1', canvas));
  assert.equal(unrouted.reason, 'brush_stage_route_unavailable');
  Adapter.desktop(host, { stage: () => '/late.png' });
  const done = await first.publishCanvas('b1', canvas);
  assert.equal(done.assetId, 'asset-project-asset-1');
  assert.deepEqual(seen, [1, 1], 'raster.desktop(host) is called with the host only: no assetRoute option');
  assert.equal(Adapter.desktop({ NemoNativeObjectProject: { desktop: () => ({ blocksLegacy: () => false }) } }), null);
  assert.equal(Adapter.desktop({}), null);
});

test('the adapter holds no cache, no byte budget and no engine access: its source names none of them', () => {
  const source = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'adapters', 'native-bitmap-brush-resources.js'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '').replace(/\s\/\/.*$/gm, '');
  for (const name of ['retire_images', 'register_image', 'registeredImageIds', 'NemoImageBudget', 'SMEngineBridge', 'SMBitmapBrush', 'setImageBudgetBytes', 'localStorage',
    'getImageData', 'toDataURL', 'getContext', 'paper', 'Paper', 'userLayers', 'evict', 'budget', 'Budget', 'lru', 'LRU']) {
    assert.equal(source.includes(name), false, name);
  }
});
