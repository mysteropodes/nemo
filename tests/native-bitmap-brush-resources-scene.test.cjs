'use strict';
/* Lane-7 brush resource adapter: inspecting a brush row, consuming the brush part of one pinned scene, and re-publishing
 * the bakes the scene reports missing. The owner and session are doubles that follow the documented rules only. */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Adapter = require('../src/js/adapters/native-bitmap-brush-resources.js');
const { createFixture, forbidLegacyWriters, pngBytes, PUBLISH, QUERY } = require('./native-bitmap-brush-resources-support.cjs');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-bitmap-brush-resources-fixtures', 'canvas-oracle.json'), 'utf8'));
const BYTES = oracle.rows.find((r) => r.name === 'straight 100px').baselineBytes; // 241 x 41
const ink = (version) => ({ version, width: 241, height: 41, bytes: BYTES });
const ENTRIES = () => [
  { sourceObjectIndex: 5, layerUid: 'L1', strokeId: 'native-brush:1:0', brushId: 'i1:aaa', ink: ink('3') },
  { sourceObjectIndex: 2, layerUid: 'L0', strokeId: 'native-brush:2:0', brushId: 'i1:m1', unavailable: { code: 'ink-missing' } },
  { sourceObjectIndex: 9, layerUid: 'L1', strokeId: 'native-brush:3:0', brushId: 'i1:aaa', ink: ink('3') },
  { sourceObjectIndex: 11, layerUid: 'L2', strokeId: 'native-brush:4:0', brushId: 'bmbtrim:11:x', unavailable: { code: 'ink-refused: over budget' } },
  { sourceObjectIndex: 12, layerUid: 'L2', strokeId: 'native-brush:5:0', brushId: 'i1:m1', unavailable: { code: 'ink-missing' } },
  { sourceObjectIndex: 14, layerUid: 'L3', strokeId: 'native-brush:6:0', brushId: 'i1:m2', unavailable: { code: 'ink-missing' } },
  { sourceObjectIndex: 15, layerUid: 'L3', strokeId: 'native-brush:7:0', brushId: 'Zed', ink: ink('1') },
];
const ASSETS = { 'asset-project-asset-1': { width: 241, height: 41 } };
const trap = () => forbidLegacyWriters(['SMEngineBridge', 'NemoImageBudget', 'SMBitmapBrush', 'paper', 'state', 'localStorage', 'registeredImageIds', 'engine', 'userLayers', 'SMMotion']);
const fixture = (options) => createFixture(Object.assign({ assets: ASSETS, scene: ENTRIES() }, options));
async function code(promise) { try { await promise; } catch (error) { return error; } throw new Error('expected a rejection'); }
const canvas = () => ({ width: 241, height: 41, toBlob(cb) { cb(new Blob([pngBytes(241, 41)])); } });
const ports = () => ({ stage: () => '/stage/b.png', assets: { attachPath: () => ({ applied: true, assetId: 'asset-project-asset-1' }) } });

test('inspect returns the store row of one brush: resident or not, frozen, from the pinned snapshot', async () => {
  const f = fixture();
  const client = Adapter.create(f.session);
  assert.deepEqual(await client.inspect('b1'), { brushId: 'b1', resident: false });
  await client.publishAsset('b1', 'asset-project-asset-1', { width: 241, height: 41 });
  const row = await client.inspect('b1');
  assert.deepEqual(row, { brushId: 'b1', resident: true, version: '1', width: 241, height: 41, bytes: BYTES });
  assert.equal(Object.isFrozen(row), true);
  const query = f.owner.log.received[f.owner.log.received.length - 1];
  assert.deepEqual(query, { apiVersion: 2, requestId: 'project-brush-3', instanceId: 'instance-A', documentId: 'document-A', operation: QUERY,
    payload: { action: 'inspect', atRevision: 7, documentSnapshotId: 'snapshot-7', brushId: 'b1' } });
  assert.equal('expectedRevision' in query, false, 'a read forbids an expected revision');
  const error = await code(client.inspect('  '));
  assert.deepEqual([error.code, error.reason], ['invalid_request', 'local_validation']);
});

test('an inspect answer for another brush, another snapshot or with a lying cost is uncorrelated', async () => {
  const flaws = [
    ['another brush', (r) => { r.result.brush.brushId = 'other'; }], ['another revision', (r) => { r.result.atRevision = 6; }],
    ['another snapshot', (r) => { r.result.documentSnapshotId = 'snapshot-6'; }], ['envelope revision', (r) => { r.contentRevision = 6; }],
    ['resident without a cost', (r) => { r.result.brush = { brushId: 'b1', resident: true }; }],
    ['absent row carrying a version', (r) => { r.result.brush = { brushId: 'b1', resident: false, version: '1' }; }],
    ['cost not width * height * 4', (r) => { r.result.brush = { brushId: 'b1', resident: true, version: '1', width: 2, height: 2, bytes: 15 }; }],
  ];
  for (const [label, flaw] of flaws) {
    const f = fixture({ respond: (response) => { flaw(response); return response; } });
    const error = await code(Adapter.create(f.session).inspect('b1'));
    assert.equal(error.code, 'uncorrelated', label);
  }
});

test('readScene pages the brush part of the scene, one entry per request, all pinned to one snapshot and frame', async () => {
  const guard = trap();
  const f = fixture();
  const report = await Adapter.create(f.session).readScene({ frame: 12 });
  guard.restore();
  const asked = f.owner.log.received.map((r) => r.payload);
  assert.deepEqual(asked.map((p) => p.offset), [0, 1, 2, 3, 4, 5, 6]);
  for (const p of asked) {
    assert.deepEqual(Object.assign({}, p, { offset: 0 }), { action: 'scene', atRevision: 7, documentSnapshotId: 'snapshot-7', contextId: 'scene-root', frame: 12, offset: 0, limit: 1 });
  }
  assert.deepEqual(f.owner.log.received.map((r) => r.operation), new Array(7).fill(QUERY));
  assert.deepEqual(report.pin, { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 3, contentRevision: 7, documentSnapshotId: 'snapshot-7', contextId: 'scene-root', frame: 12 });
  assert.deepEqual(report.entries.map((e) => e.sourceObjectIndex), [5, 2, 9, 11, 12, 14, 15], 'the owner order is kept, never sorted');
  assert.deepEqual(report.entries[0], { sourceObjectIndex: 5, layerUid: 'L1', strokeId: 'native-brush:1:0', brushId: 'i1:aaa', ink: ink('3'), unavailable: null });
  assert.deepEqual(report.entries[1], { sourceObjectIndex: 2, layerUid: 'L0', strokeId: 'native-brush:2:0', brushId: 'i1:m1', ink: null, unavailable: 'ink-missing' });
  assert.deepEqual(report.resources, [{ brushId: 'Zed', version: '1' }, { brushId: 'i1:aaa', version: '3' }], 'a shared brush is one resource; byte order');
  assert.equal(f.state.revision, 7);
  for (const part of [report, report.pin, report.entries, report.entries[0], report.entries[0].ink, report.resources, report.unavailable]) assert.equal(Object.isFrozen(part), true);
});

test('an unavailable ink is never dropped: it stays a typed entry with its owner code and marks the scene incomplete', async () => {
  const f = fixture();
  const report = await Adapter.create(f.session).readScene({ frame: 0 });
  assert.equal(report.entries.length, 7);
  assert.equal(report.complete, false);
  assert.deepEqual(report.unavailable.map((e) => [e.sourceObjectIndex, e.unavailable]), [[2, 'ink-missing'], [11, 'ink-refused: over budget'], [12, 'ink-missing'], [14, 'ink-missing']]);
  const ready = fixture({ scene: ENTRIES().filter((e) => e.ink) });
  const whole = await Adapter.create(ready.session).readScene({ frame: 0 });
  assert.deepEqual([whole.complete, whole.unavailable.length, whole.entries.length], [true, 0, 3]);
  const empty = fixture({ scene: [] });
  const none = await Adapter.create(empty.session).readScene({ frame: 0, contextId: 'comp-1' });
  assert.deepEqual([none.complete, none.entries, none.resources, empty.owner.log.received.length], [true, [], [], 1]);
  assert.equal(empty.owner.log.received[0].payload.contextId, 'comp-1');
});

test('resources sort by UTF-8 bytes like the native ordered map, not by UTF-16 code units', async () => {
  const scene = ['\u{1F600}', '～', 'b', 'B'].map((brushId, i) => ({ sourceObjectIndex: i, layerUid: 'L', strokeId: 's' + i, brushId, ink: ink('1') }));
  const f = fixture({ scene });
  const report = await Adapter.create(f.session).readScene({ frame: 0 });
  const ids = report.resources.map((r) => r.brushId);
  assert.deepEqual(ids, ['B', 'b', '\uFF5E', '\u{1F600}']);
  assert.notDeepEqual(ids.slice().sort(), ids, 'the default string order (UTF-16 code units) would differ');
});

test('one brush declared at two versions (or two sizes) in one scene is a typed conflict; a record claimed twice is refused', async () => {
  const twice = ENTRIES().filter((e) => e.ink).slice(0, 2);
  twice[1].ink = ink('4');
  const versions = fixture({ scene: twice });
  const conflict = await code(Adapter.create(versions.session).readScene({ frame: 0 }));
  assert.deepEqual([conflict.code, conflict.brushId], ['scene_resource_conflict', 'i1:aaa']);
  const sized = ENTRIES().filter((e) => e.ink).slice(0, 2);
  sized[1].ink = { version: '3', width: 100, height: 100, bytes: 40000 };
  const other = await code(Adapter.create(fixture({ scene: sized }).session).readScene({ frame: 0 }));
  assert.equal(other.code, 'scene_resource_conflict');
  const dup = ENTRIES().slice(0, 3);
  dup[2].sourceObjectIndex = 5;
  const claimed = await code(Adapter.create(fixture({ scene: dup }).session).readScene({ frame: 0 }));
  assert.deepEqual([claimed.code, claimed.sourceObjectIndex], ['scene_duplicate_claim', 5]);
});

test('a scene page that does not belong to the request is uncorrelated', async () => {
  const flaws = [
    ['another frame', (p) => { p.frame += 1; }], ['another context', (p) => { p.contextId = 'other'; }], ['another offset', (p) => { p.offset += 1; }],
    ['another snapshot', (p) => { p.documentSnapshotId = 'snapshot-6'; }], ['two entries in one page', (p) => { p.entries.push(structuredClone(p.entries[0])); }],
    ['wrong next offset', (p) => { p.nextOffset = p.nextOffset === null ? 1 : null; }], ['entry total beyond the bound', (p) => { p.totalEntries = 4097; }],
    ['entry with both ink and a reason', (p) => { p.entries[0].unavailable = { code: 'ink-missing' }; }],
    ['entry with neither', (p) => { delete p.entries[0].ink; delete p.entries[0].unavailable; }],
    ['ink cost not width * height * 4', (p) => { p.entries[0].ink = Object.assign({}, p.entries[0].ink, { bytes: 4 }); }],
    ['blank reason code', (p) => { delete p.entries[0].ink; p.entries[0].unavailable = { code: ' ' }; }],
    ['negative source index', (p) => { p.entries[0].sourceObjectIndex = -1; }], ['extra entry member', (p) => { p.entries[0].extra = 1; }],
  ];
  for (const [label, flaw] of flaws) {
    const f = fixture({ shapePage: (page) => { flaw(page); return page; } });
    const error = await code(Adapter.create(f.session).readScene({ frame: 3 }));
    assert.equal(error.code, 'uncorrelated', label);
  }
  // A total that changes between pages cannot be one pinned scene.
  let page = 0;
  const moving = fixture({ shapePage: (p) => { page += 1; if (page === 2) p.totalEntries = 9; return p; } });
  const error = await code(Adapter.create(moving.session).readScene({ frame: 0 }));
  assert.equal(error.code, 'uncorrelated');
  assert.match(error.message, /total changed/);
});

test('a project that moves while the scene is read makes the read stale, not a mixed report', async () => {
  const f = fixture({ respond(response, request) { if (request.payload.offset === 2) f.state.revision = 8; return response; } });
  const error = await code(Adapter.create(f.session).readScene({ frame: 0 }));
  assert.equal(error.code, 'stale_snapshot');
  assert.equal(f.owner.log.received.length, 3, 'the read stops at the first stale page');
  const replaced = fixture({ respond(response) { replaced.state.document = 'document-B'; return response; } });
  assert.equal((await code(Adapter.create(replaced.session).inspect('b1'))).code, 'stale_snapshot');
});

test('a read refusal is typed and not retried; a lost read is unavailability, not a guess', async () => {
  const refused = fixture({ refuse: () => ({ code: 'unavailable', message: 'x', details: { reason: 'scene_producer_unavailable' } }) });
  const error = await code(Adapter.create(refused.session).readScene({ frame: 0 }));
  assert.deepEqual([error.code, error.reason, refused.owner.log.received.length], ['unavailable', 'scene_producer_unavailable', 1]);
  const lost = fixture({ faults: ['before'] });
  const down = await code(Adapter.create(lost.session).readScene({ frame: 0 }));
  assert.deepEqual([down.code, down.reason, lost.owner.log.received.length], ['unavailable', 'transport_failure', 1]);
  const rejected = fixture({ refuse: () => ({ code: 'stale_revision', message: 'x' }) });
  assert.equal((await code(Adapter.create(rejected.session).inspect('b1'))).code, 'stale_revision');
});

test('readScene validates its selector locally and sends nothing for a bad frame or context', async () => {
  const f = fixture();
  const client = Adapter.create(f.session);
  for (const options of [undefined, {}, { frame: -1 }, { frame: 1.5 }, { frame: '3' }, { frame: 4294967296 }, { frame: NaN }, { frame: 0, contextId: '' }, { frame: 0, contextId: 'a b' }, { frame: 0, contextId: 7 }]) {
    const error = await code(client.readScene(options));
    assert.deepEqual([error.code, error.reason], ['invalid_request', 'local_validation'], JSON.stringify(options));
  }
  assert.deepEqual(f.owner.log.received, []);
  assert.deepEqual(f.events.ids, []);
  assert.equal((await client.readScene({ frame: 4294967295 })).pin.frame, 4294967295);
});

test('republishMissing re-publishes each ink-missing brush once, in report order, with a typed outcome per brush', async () => {
  const guard = trap();
  const f = fixture();
  const client = Adapter.create(f.session, ports());
  const report = await client.readScene({ frame: 0 });
  const asked = [];
  const outcomes = await client.republishMissing(report, (entry) => { asked.push([entry.brushId, entry.sourceObjectIndex]); return entry.brushId === 'i1:m2' ? null : canvas(); });
  guard.restore();
  assert.deepEqual(asked, [['i1:m1', 2], ['i1:m2', 14]], 'one attempt per brush id, only for ink-missing, the first stroke names it');
  assert.deepEqual(outcomes.map((o) => [o.brushId, o.ok]), [['i1:m1', true], ['i1:m2', false]]);
  assert.deepEqual([outcomes[0].receipt.publish.version, outcomes[0].receipt.publish.applied, outcomes[0].receipt.assetId], ['1', true, 'asset-project-asset-1']);
  assert.deepEqual([outcomes[1].code, outcomes[1].reason], ['no_bake', 'supplier_returned_nothing']);
  assert.equal(Object.isFrozen(outcomes), true);
  const publishes = f.owner.log.received.filter((r) => r.operation === 'command.document.object.brush.bitmap.publish');
  assert.deepEqual(publishes.map((r) => r.payload.brushId), ['i1:m1']);
});

test('a failing brush never stops or hides the others', async () => {
  const f = fixture();
  const client = Adapter.create(f.session, ports());
  const report = await client.readScene({ frame: 0 });
  let n = 0;
  const outcomes = await client.republishMissing(report, () => { n += 1; if (n === 1) throw Object.assign(new Error('bake lost'), { code: 'bake_failed', reason: 'tip' }); return canvas(); });
  assert.deepEqual(outcomes.map((o) => [o.brushId, o.ok, o.code || null, o.reason || null]), [['i1:m1', false, 'bake_failed', 'tip'], ['i1:m2', true, null, null]]);
  // The owner refuses the first publication (over budget): that brush reports the typed refusal and the next one is still published.
  const refusing = fixture({ refuse: (r) => (r.operation === PUBLISH && r.payload.brushId === 'i1:m1' ? { code: 'unavailable', message: 'x', details: { reason: 'over_budget' } } : null) });
  const wired = Adapter.create(refusing.session, ports());
  const second = await wired.republishMissing(await wired.readScene({ frame: 0 }), () => canvas());
  assert.deepEqual(second.map((o) => [o.brushId, o.ok, o.code || null, o.reason || null]), [['i1:m1', false, 'unavailable', 'over_budget'], ['i1:m2', true, null, null]]);
  assert.equal(second[0].error.assetId, 'asset-project-asset-1', 'the catalog entry that was added is named');
});

test('republishMissing only accepts a current report produced by readScene', async () => {
  const f = fixture();
  const client = Adapter.create(f.session, ports());
  const report = await client.readScene({ frame: 0 });
  let supplied = 0;
  const supply = () => { supplied += 1; return canvas(); };
  const forged = await code(client.republishMissing(Object.assign({}, report), supply));
  assert.deepEqual([forged.code, forged.reason], ['invalid_request', 'local_validation']);
  const other = await code(Adapter.create(f.session, ports()).republishMissing(report, supply));
  assert.equal(other.code, 'invalid_request', 'a report belongs to the client that read it');
  assert.equal((await code(client.republishMissing(report))).code, 'invalid_request');
  f.state.revision = 8;
  const moved = await code(client.republishMissing(report, supply));
  assert.equal(moved.code, 'stale_snapshot');
  assert.equal(supplied, 0);
  const done = fixture({ scene: ENTRIES().filter((e) => e.ink) });
  const quiet = Adapter.create(done.session, ports());
  assert.deepEqual(await quiet.republishMissing(await quiet.readScene({ frame: 0 }), supply), []);
  assert.equal(supplied, 0);
});
