'use strict';
/* Lane-6 image import / removal / raster-item adapter. Real adapter, command and contract modules over a session double
 * with the real route semantics (see native-object-item-support.cjs). Nothing here establishes native or installed
 * execution: the owner is a double and the host byte-source route is not registered. */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Raster = require('../src/js/adapters/native-object-raster-asset.js');
const Contract = require('../src/js/adapters/native-object-item-contract.js');
const { createFixture, forbidLegacyWriters } = require('./native-object-item-support.cjs');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-item-fixtures', 'raster-asset-oracle.json'), 'utf8'));
const ATTACH = 'command.document.object.asset.attach';
const REMOVE = 'command.document.object.asset.remove';
const ITEM = 'command.document.object.item.edit';
const PATH = '/Users/a/Pictures/photo.png';
const rejection = (promise) => promise.then(() => assert.fail('expected a rejection'), (error) => error);
const adapter = (options, extra) => {
  const fixture = createFixture(options);
  return Object.assign(fixture, { raster: Raster.create(fixture.session, Object.assign({ assetRoute: true }, extra)) });
};

test('baseline name and declared type derivation: 15 oracle rows plus literal anchors', () => {
  assert.equal(oracle.baseline, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  assert.equal(oracle.rows.length, 15);
  for (const row of oracle.rows) assert.deepEqual(Raster.describePath(row.path), { name: row.name, mediaType: row.mediaType }, row.path);
  const literal = [
    ['/Users/a/Pictures/photo.png', 'photo', 'image/png'],
    ['C:\\Art\\Sprite.PNG', 'Sprite', 'image/png'],
    ['/x/archive.tar.gz', 'archive.tar', 'image/png'],
    ['/x/.png', '', 'image/png'],
    ['/x/trailing/', '', 'image/png'],
    ['/x/pic.jpeg', 'pic', 'image/jpeg'],
    ['/x/logo.svg', 'logo', 'image/svg+xml'],
    ['/x/photo.tiff', 'photo', 'image/png'],
    ['/x/dir.d/noext', 'noext', 'image/png'],
  ];
  for (const [p, name, mediaType] of literal) assert.deepEqual(Raster.describePath(p), { name, mediaType }, p);
  assert.deepEqual(Raster.IMAGE_EXTENSIONS, oracle.dialogExtensions);
  assert.deepEqual(Raster.IMAGE_EXTENSIONS, ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'avif']);
  for (const row of oracle.rows) assert.ok(Contract.MEDIA.includes(row.mediaType), row.mediaType);
  assert.equal(Raster.describePath('/x/a.constructor').mediaType, 'image/png', 'an inherited property name is not a media type');
  for (const bad of ['', null, undefined, 5, {}, '/x\u0000y.png']) assert.throws(() => Raster.describePath(bad), (e) => e.code === 'invalid_request');
});

test('attach sends one command with the pinned revision, a stable request id and the baseline name', async () => {
  const f = adapter();
  const receipt = await f.raster.attachPath(PATH);
  assert.equal(f.owner.log.received.length, 1);
  assert.deepEqual(f.owner.log.received[0], {
    apiVersion: 2, requestId: 'project-asset-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: ATTACH,
    payload: { documentSnapshotId: 'snapshot-7', name: 'photo', mediaType: 'image/png', source: { kind: 'file', path: PATH } },
  });
  assert.deepEqual(receipt, { requestId: 'project-asset-1', contentRevision: 8, applied: true, historyEntriesAdded: 1, assetId: 'asset-project-asset-1' });
  assert.ok(Object.isFrozen(receipt));
  assert.deepEqual(f.events.reconciled, [8]);
  assert.equal(f.state.revision, 8);
});

test('an exact duplicate is applied=false: no new id, no revision, the session is reconciled at the same revision', async () => {
  const f = adapter();
  await f.raster.attachPath(PATH);
  const again = await f.raster.attachPath(PATH);
  assert.deepEqual(again, { requestId: 'project-asset-2', contentRevision: 8, applied: false, historyEntriesAdded: 0, assetId: null });
  assert.deepEqual(f.events.reconciled, [8, 8]);
  assert.equal(f.owner.assets.size, 1);
});

test('queued commands read the current pin at execution, never the values of the click', async () => {
  const f = adapter();
  const first = f.raster.attachPath('/x/one.png');
  const second = f.raster.attachPath('/x/two.png');
  await Promise.all([first, second]);
  const [a, b] = f.owner.log.received;
  assert.deepEqual([a.requestId, a.expectedRevision, a.payload.documentSnapshotId], ['project-asset-1', 7, 'snapshot-7']);
  assert.deepEqual([b.requestId, b.expectedRevision, b.payload.documentSnapshotId], ['project-asset-2', 8, 'snapshot-8']);
});

test('absent native support is typed unavailability: no route, no dispatch, no request id', async () => {
  for (const route of [undefined, false, 1, 'yes', () => false, () => 1]) {
    const f = adapter({}, { assetRoute: route });
    for (const call of [() => f.raster.attachPath(PATH), () => f.raster.remove('asset-x')]) {
      const error = await rejection(call());
      assert.deepEqual([error.code, error.reason], ['unavailable', 'asset_host_route_unavailable']);
    }
    assert.deepEqual([f.owner.log.received.length, f.events.runs, f.events.ids.length], [0, 0, 0]);
  }
  let on = false;
  const f = adapter({}, { assetRoute: () => on });
  await rejection(f.raster.attachPath(PATH));
  on = true;
  assert.equal((await f.raster.attachPath(PATH)).applied, true);
  const bare = adapter({ noRoute: true });
  const error = await rejection(bare.raster.attachPath(PATH));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'resource_route_unavailable']);
  assert.equal(bare.owner.log.received.length, 0);
});

test('an ambiguous transport outcome replays the SAME request: the owner executes at most once', async () => {
  for (const [faults, executed] of [[['after'], 1], [['before'], 1]]) {
    const f = adapter({ faults });
    const receipt = await f.raster.attachPath(PATH);
    assert.equal(receipt.applied, true);
    assert.equal(f.owner.log.executed.length, executed);
    assert.equal(f.owner.log.received.length, 2);
    assert.deepEqual(f.owner.log.received[0], f.owner.log.received[1]);
    assert.equal(f.state.revision, 8);
  }
});

test('two lost deliveries: not-delivered when the document did not advance, indeterminate with a replay when it did', async () => {
  const notDelivered = adapter({ faults: ['before', 'before'] });
  const failed = await rejection(notDelivered.raster.attachPath(PATH));
  assert.deepEqual([failed.code, failed.reason, failed.requestId], ['unavailable', 'transport_failure', 'project-asset-1']);
  assert.deepEqual([notDelivered.owner.log.executed.length, notDelivered.state.revision], [0, 7]);

  const lost = adapter({ faults: ['after', 'after'] });
  const unknown = await rejection(lost.raster.attachPath(PATH));
  assert.deepEqual([unknown.code, unknown.requestId], ['asset_command_indeterminate', 'project-asset-1']);
  assert.equal(typeof unknown.replay, 'function');
  assert.deepEqual([lost.owner.log.executed.length, lost.state.revision], [1, 8]);
  const receipt = await unknown.replay();
  assert.deepEqual(receipt, { requestId: 'project-asset-1', contentRevision: 8, applied: true, historyEntriesAdded: 1, assetId: 'asset-project-asset-1' });
  assert.equal(lost.owner.log.executed.length, 1, 'the replay returned the stored receipt');
  lost.state.generation += 1;
  const stale = await rejection(unknown.replay());
  assert.equal(stale.code, 'stale_owner');
});

test('a replay after the document moved on returns the STORED receipt and never wedges the session', async () => {
  const lost = adapter({ faults: ['after', 'after'] });
  const unknown = await rejection(lost.raster.attachPath(PATH));
  assert.equal(unknown.code, 'asset_command_indeterminate');
  lost.state.revision += 2; // other edits published between the lost reply and the replay
  const receipt = await unknown.replay();
  assert.deepEqual(receipt, { requestId: 'project-asset-1', contentRevision: 8, applied: true, historyEntriesAdded: 1, assetId: 'asset-project-asset-1' });
  assert.equal(lost.owner.log.executed.length, 1);
  assert.ok(lost.events.reconciled.every((revision) => revision === undefined), 'no reconcile asserted the stored revision against the moved document');
});

test('the not-delivered decision reads a RECONCILED pin: a command that did execute is indeterminate even when the pin was stale', async () => {
  // The session pin (like the real session.info()) only moves when the session is reconciled; without that reconcile the stale pin would equal the sent revision.
  const lost = adapter({ faults: ['after', 'after'], snapshotInfo: true });
  const unknown = await rejection(lost.raster.attachPath(PATH));
  assert.deepEqual([unknown.code, lost.state.revision], ['asset_command_indeterminate', 8]);
  const clean = adapter({ faults: ['before', 'before'], snapshotInfo: true });
  const failed = await rejection(clean.raster.attachPath(PATH));
  assert.deepEqual([failed.code, failed.reason], ['unavailable', 'transport_failure']);
});

test('the answer to the second send is the stored receipt: an edit published meanwhile never wedges the session', async () => {
  const f = adapter({ faults: ['after'], respond: (stored) => { f.state.revision += 1; return stored; } });
  const receipt = await f.raster.attachPath(PATH);
  assert.deepEqual(receipt, { requestId: 'project-asset-1', contentRevision: 8, applied: true, historyEntriesAdded: 1, assetId: 'asset-project-asset-1' });
  assert.equal(f.state.revision, 9);
  assert.equal(f.owner.log.executed.length, 1);
  assert.ok(f.events.reconciled.every((revision) => revision === undefined), 'no reconcile asserted the stored revision against the moved document');
});

test('a reconcile failure of the session is typed, and a rejection of the shared route before the action is typed busy', async () => {
  const broken = adapter({ reconcileError: new Error('Native font import revision is uncorrelated') });
  const failure = await rejection(broken.raster.attachPath(PATH));
  assert.deepEqual([failure.code, failure.reason, failure.requestId], ['unavailable', 'reconcile_failed', 'project-asset-1']);
  const f = createFixture();
  const busy = Object.assign({}, f.session, { runResourceCommand: () => Promise.reject(new Error('Native font owner busy or unavailable')) });
  const error = await rejection(Raster.create(busy, { assetRoute: true }).attachPath(PATH));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'native_project_busy']);
  assert.equal(f.owner.log.received.length, 0);
});

test('an inactive native project is typed unavailability before any run or request id', async () => {
  const f = adapter();
  f.state.active = false;
  const error = await rejection(f.raster.attachPath(PATH));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'no_native_project']);
  assert.deepEqual([f.owner.log.received.length, f.events.runs, f.events.ids.length], [0, 0, 0]);
});

test('a native refusal is passed through typed and is never retried', async () => {
  const refused = [
    [{ code: 'unavailable', message: 'refused', details: { reason: 'decoder_unavailable' } }, 'unavailable', 'decoder_unavailable'],
    [{ code: 'invalid_request', message: 'refused', details: { reason: 'media_type_mismatch' } }, 'invalid_request', 'media_type_mismatch'],
    [{ code: 'stale_revision', message: 'refused' }, 'stale_revision', null],
  ];
  for (const [error, code, reason] of refused) {
    const f = adapter({ refuse: () => error });
    const failure = await rejection(f.raster.attachPath('/x/photo.tiff'));
    assert.deepEqual([failure.code, failure.reason, failure.requestId, failure.contentRevision], [code, reason, 'project-asset-1', 7]);
    assert.equal(f.owner.log.received.length, 1);
    assert.deepEqual(f.events.reconciled, [7]);
    assert.equal(f.state.revision, 7);
  }
});

test('an uncorrelated or malformed receipt is refused and the session is resynchronized', async () => {
  const flaws = [
    (r) => { r.requestId = 'other'; },
    (r) => { r.apiVersion = 1; },
    (r) => { r.result.assetId = 'asset-x'; },
    (r) => { r.result.historyEntriesAdded = 0; },
    (r) => { r.contentRevision = 7; },
    (r) => { delete r.result.applied; },
  ];
  for (const flaw of flaws) {
    const f = adapter({ respond: (response) => { flaw(response); return response; } });
    const failure = await rejection(f.raster.attachPath(PATH));
    assert.equal(failure.code, 'uncorrelated', String(flaw));
    assert.equal(f.owner.log.received.length, 1);
  }
});

test('invalid input is refused locally before any dispatch', async () => {
  const f = adapter();
  for (const bad of ['', '/x\u0000.png', 5, null]) assert.equal((await rejection(f.raster.attachPath(bad))).code, 'invalid_request');
  for (const bad of ['', 5, null, 'a\u0000']) assert.equal((await rejection(f.raster.remove(bad))).code, 'invalid_request');
  const large = await rejection(f.raster.attachPath('/' + 'd'.repeat(4000) + '/a.png'));
  assert.deepEqual([large.code, large.reason], ['invalid_request', 'request_size']);
  assert.deepEqual(await rejection(f.raster.attachPath('/' + 'd'.repeat(4100) + '/a.png')).then((e) => [e.code, e.reason]), ['invalid_request', 'local_validation']);
  assert.equal((await rejection(f.raster.attachPath('/x/' + 'n'.repeat(300) + '.png'))).code, 'invalid_request', 'a name over 255 bytes');
  assert.equal(f.owner.log.received.length, 0);
});

test('remove names the asset and the pinned snapshot; an absent asset is a silent no-op receipt', async () => {
  const f = adapter();
  await f.raster.attachPath(PATH);
  const removed = await f.raster.remove('asset-project-asset-1');
  assert.deepEqual(f.owner.log.received[1].payload, { documentSnapshotId: 'snapshot-8', assetId: 'asset-project-asset-1' });
  assert.equal(f.owner.log.received[1].operation, REMOVE);
  assert.deepEqual(removed, { requestId: 'project-asset-2', contentRevision: 9, applied: true, historyEntriesAdded: 1 });
  const absent = await f.raster.remove('asset-project-asset-1');
  assert.deepEqual(absent, { requestId: 'project-asset-3', contentRevision: 9, applied: false, historyEntriesAdded: 0 });
});

test('raster item edits send the closed owner payloads and report the owner-allocated id', async () => {
  const f = adapter();
  const created = await f.raster.createItem('L0', 0, { assetId: 'asset-project-asset-1', x: 10, y: 20, width: 64, height: 48 });
  assert.deepEqual(f.owner.log.received[0], {
    apiVersion: 2, requestId: 'project-asset-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: ITEM,
    payload: { command: 'object.raster.create', target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'L0', strokeId: '' },
      item: { assetId: 'asset-project-asset-1', x: 10, y: 20, width: 64, height: 48 } },
  });
  assert.deepEqual(created, { requestId: 'project-asset-1', contentRevision: 8, applied: true, historyEntriesAdded: 1, operation: 'raster.create', strokeId: 'native-raster:8:0' });
  const target = { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'L0', strokeId: created.strokeId };
  const steps = [
    ['moveItem', [target, 1, 2.5], { command: 'object.raster.move', stableTarget: target, x: 1, y: 2.5 }, 'raster.move'],
    ['resizeItem', [target, 3, 4], { command: 'object.raster.resize', stableTarget: target, width: 3, height: 4 }, 'raster.resize'],
    ['rotateItem', [target, 190], { command: 'object.raster.rotate', stableTarget: target, rotation: 190 }, 'raster.rotate'],
    ['opacityItem', [target, 0.5], { command: 'object.raster.opacity', stableTarget: target, opacity: 0.5 }, 'raster.opacity'],
    ['deleteItem', [target], { command: 'object.raster.delete', stableTarget: target }, 'raster.delete'],
  ];
  let sent = 1;
  for (const [method, args, payload, operation] of steps) {
    const receipt = await f.raster[method](...args);
    assert.deepEqual(f.owner.log.received[sent].payload, payload, method);
    assert.deepEqual([receipt.operation, receipt.strokeId, receipt.applied], [operation, created.strokeId, true], method);
    sent += 1;
  }
  const gone = await f.raster.deleteItem(target);
  assert.deepEqual([gone.applied, gone.historyEntriesAdded], [false, 0]);
  for (const bad of [() => f.raster.moveItem(target, '1', 2), () => f.raster.createItem('L0', -1, { assetId: 'a', x: 0, y: 0, width: 1, height: 1 }),
    () => f.raster.createItem('L0', 0, { assetId: 'a', x: 0, y: 0, width: 1, height: 1, isText: true }), () => f.raster.deleteItem({ layerUid: 'L0' })]) {
    assert.equal((await rejection(bad())).code, 'invalid_request');
  }
});

test('a receipt for another operation or stroke is not accepted', async () => {
  const target = { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'L0', strokeId: 'native-raster:3:0' };
  for (const flaw of [(r) => { r.result.operation = 'raster.resize'; }, (r) => { r.result.strokeId = 'native-raster:3:1'; }]) {
    const f = adapter({ respond: (response) => { flaw(response); return response; } });
    assert.equal((await rejection(f.raster.moveItem(target, 1, 2))).code, 'uncorrelated');
  }
});

test('the legacy media-library and Paper writers are never reached', async () => {
  const guard = forbidLegacyWriters(['state', 'paper', 'SMMediaLibrary', 'project', 'userLayers']);
  try {
    const f = adapter();
    await f.raster.attachPath(PATH);
    await f.raster.remove('asset-project-asset-1');
    await f.raster.createItem('L0', 0, { assetId: 'a', x: 0, y: 0, width: 1, height: 1 });
  } finally { guard.restore(); }
  for (const file of ['native-object-raster-asset.js', 'native-object-item-command.js', 'native-object-item-contract.js']) {
    const source = fs.readFileSync(path.join(__dirname, '..', 'src/js/adapters', file), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
    assert.doesNotMatch(source, /mediaLibrary|SMMediaLibrary|\bpaper\b|\bPaper\b|new Raster|insertChild|localStorage|sessionStorage|indexedDB|state\./, file);
  }
});

test('desktop binding: needs the native session, is cached per host and cancels cleanly', async () => {
  const f = createFixture();
  const opened = [];
  let bound = true;
  const host = {
    NemoNativeObjectProject: { desktop: () => Object.assign({ blocksLegacy: () => bound }, f.session) },
    __TAURI__: { dialog: { open: async (options) => { opened.push(options); return opened.length === 1 ? '/x/pick.png' : null; } } },
  };
  const unrouted = Raster.desktop(host);
  assert.equal(Raster.desktop(host), unrouted, 'one binding per host');
  assert.equal((await rejection(unrouted.attachPath(PATH))).reason, 'asset_host_route_unavailable');
  assert.equal(Raster.desktop(host, { assetRoute: true }), unrouted, 'flipping the route later keeps the one binding');
  assert.equal((await unrouted.attachPath(PATH)).applied, true, 'and the route is read at call time, not pinned by the first caller');
  assert.equal(Raster.desktop(host, { assetRoute: false }), unrouted);
  assert.equal((await rejection(unrouted.remove('asset-project-asset-1'))).reason, 'asset_host_route_unavailable');
  const routedHost = Object.assign({}, host);
  const routed = Raster.desktop(routedHost, { assetRoute: true });
  const receipt = await routed.importPicked();
  assert.equal(receipt.applied, true);
  assert.deepEqual(opened[0], { title: 'Import Image', multiple: false, filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'avif'] }, { name: 'All files', extensions: ['*'] }] });
  assert.equal(await routed.importPicked(), null);
  assert.equal(f.owner.log.received.length, 2, 'a cancelled picker sends nothing (one attach through the late-routed binding, one import)');
  bound = false;
  assert.equal(Raster.desktop({ NemoNativeObjectProject: host.NemoNativeObjectProject }), null);
  assert.equal(Raster.desktop({}), null);
  const noPicker = Raster.create(f.session, { assetRoute: true });
  assert.equal((await rejection(noPicker.importPicked())).reason, 'picker_unavailable');
});
