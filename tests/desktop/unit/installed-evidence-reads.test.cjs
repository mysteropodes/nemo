'use strict';
// The reads and observations the new lane rows depend on, against a stub that answers with the shapes the packaged candidate answered
// (literal copies below, captured from candidate runs; the stub's own replies are compared with them), plus the receipt's trust rules:
// coverage that counts only passing rows, and every retry recorded as data. Stubs only: nothing here starts an app.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWire } = require('../installed-evidence/wire.cjs');
const K = require('../installed-evidence/kit.cjs');
const P = require('../installed-evidence/payloads.cjs');
const probe = require('../installed-evidence/png-probe.cjs');
const { runRows } = require('../installed-evidence/runner.cjs');
const { buildReceipt } = require('../installed-evidence/receipt.cjs');
const { laneCoverage, ASSIGNED } = require('../installed-evidence/coverage.cjs');
const { createStubApp, INSTANCE, BUILD } = require('./installed-evidence-stubs.cjs');

function setup(t, flags) {
  const work = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'ie-reads-')));
  t.after(() => fs.rmSync(work, { recursive: true, force: true }));
  const app = createStubApp(flags);
  const wire = createWire({ client: app, expect: { instanceId: INSTANCE, buildId: BUILD } });
  return { app, wire, work, session: { wire, work, recheck: async () => {} } };
}
const RED = [0.9, 0.2, 0.2, 1], WHITE = [1, 1, 1, 1];
const mini = (id, run) => ({ id, lane: '9001', issue: 1, title: id + ' title', tasks: [], run });
const make = (ctx, layerUid, [x0, y0, x1, y1], fill) => K.apply(ctx, 'create', P.vectorCreate(P.vectorRecord({ layerUid, segments: P.rectSegments(x0, y0, x1, y1), fill })));
// Replies of the packaged candidate (captured in candidate runs; ids shortened). The shapes are what a row reads.
const REAL = {
  hitTest: { atRevision: 3, contextId: 'scene-root', cursor: 0, documentSnapshotId: 'native-object:native-object-document-2:3', frame: 0, hitTest: true, kind: 'subselect', layerMatrix: [1, 0, 0, 1, 0, 0], layerUid: 'layer-1', nextCursor: null, preview: false, total: 1, viewGeneration: 8, workId: 'native-work-8',
    items: [{ bounds: [40, 40, 240, 200], closed: true, count: 4, filled: true, matrix: [1, 0, 0, 1, 0, 0], role: 'plain', stableTarget: { contextId: 'scene-root', frameScope: { frame: 0, kind: 'authored' }, layerUid: 'layer-1', strokeId: 'native-stroke:2:0' }, strokeWidth: 0,
      groupPath: [{ id: 'g', mode: 'none', name: 'Outer' }], style: { cap: 'round', fill: '#e63333', join: 'round', opacity: 1, strokeStyle: 'solid', width: 4 } }] },
  page: { atRevision: 4, documentSnapshotId: 'native-object:native-object-document-1:4', subselect: { closed: true, cursor: 0, nextCursor: 16, preview: false, stableTarget: { contextId: 'scene-root', frameScope: { frame: 0, kind: 'authored' }, layerUid: 'layer-1', strokeId: 'native-stroke:2:0' }, total: 182,
    segments: [{ handleIn: [0, 0], handleOut: [0, 0], point: [240, 110] }] } },
  element: { atRevision: 7, dimension: 2, documentId: 'native-object-document-1', documentSnapshotId: 'native-object:native-object-document-1:7', frame: 0, instanceId: INSTANCE, kind: 'rawSample', property: 'position', target: { contextId: 'scene-root', layerUid: 'layer-1', strokeId: 'native-stroke:7:0' }, value: [0, 0] },
  cancelled: { code: 'cancelled_before_dispatch', details: { reason: 'cancelled' }, message: 'Native brush resource request refused.' },
  overQuota: { code: 'unavailable', details: { reason: 'over-quota' }, message: 'Native brush resource request refused.' },
  receipt: { applied: true, historyEntriesAdded: 1 },
};
const shape = value => Array.isArray(value) ? (value.length ? [shape(value[0])] : []) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map(k => [k, shape(value[k])])) : typeof value;
const optionalTop = { groupPath: 1, strokeWidth: 1 };

test('the stub answers the hit-test listing, the anchor page, the element property read and the canvas edit with the packaged candidate\'s reply shapes', async t => {
  const { wire, work } = setup(t);
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  const { layerUid } = await K.fresh(ctx);
  const made = await make(ctx, layerUid, [40, 40, 240, 200], RED);
  await K.apply(ctx, 'group', P.groupCreate(layerUid, 'g', 'G', [made.result.strokeId]));
  const view = P.presentationRead((await wire.discover()).contentRevision), shown = await wire.query(view.operation, view.payload);
  const { revision, pin } = await K.pinNow(ctx);
  const listing = P.hitTestRead(revision, pin, { layerUid, frame: shown.result.frame, workId: shown.result.workId, viewGeneration: shown.result.viewGeneration }), res = await wire.query(listing.operation, listing.payload);
  assert.ok(res.ok);
  assert.deepEqual(Object.keys(res.result).sort(), Object.keys(REAL.hitTest).sort());
  assert.deepEqual(shape(res.result.items[0].style), shape({ ...REAL.hitTest.items[0].style }));
  assert.deepEqual(Object.keys(res.result.items[0]).sort(), Object.keys(REAL.hitTest.items[0]).sort());
  assert.deepEqual(shape(res.result.items[0].groupPath), shape(REAL.hitTest.items[0].groupPath));
  const page = P.subselectRead(revision, pin, P.target(layerUid, made.result.strokeId)), pageRes = await wire.query(page.operation, page.payload);
  assert.deepEqual(shape(pageRes.result.subselect.segments), shape(REAL.page.subselect.segments));
  assert.deepEqual(Object.keys(pageRes.result.subselect).sort(), Object.keys(REAL.page.subselect).filter(k => k !== 'pointTypes').sort());
  const set = await wire.command(P.elementValue(layerUid, made.result.strokeId, 'position', [100, 0]).operation, P.elementValue(layerUid, made.result.strokeId, 'position', [100, 0]).payload);
  assert.deepEqual(set.result, REAL.receipt);
  const { revision: r2, pin: p2 } = await K.pinNow(ctx), read = P.elementRead(r2, p2, layerUid, made.result.strokeId, 'position'), got = await wire.query(read.operation, read.payload);
  assert.deepEqual(Object.keys(got.result).sort(), Object.keys(REAL.element).sort());
  assert.deepEqual(got.result.value, [100, 0]);
  const bg = P.canvasBackground('#00000000'), edit = await wire.command(bg.operation, bg.payload);
  assert.deepEqual(edit.result, REAL.receipt);
});

test('the listing reads a text block, a nested group path and a dynamic shape exactly as the candidate answered them', async t => {
  const TEXT = { bounds: [25.94140625, 35.709374999999994, 235.19140625, 88.303125], closed: true, count: 0, filled: true, matrix: [1, 0, 0, 1, 0, 0], role: 'text', stableTarget: { contextId: 'scene-root', frameScope: { frame: 0, kind: 'authored' }, layerUid: 'layer-1', strokeId: 'native-text:2:0' }, style: { fill: '#cc2200', opacity: 1 } };
  const NESTED = { bounds: [130, 100, 180, 220], closed: true, count: 4, filled: true, groupPath: [{ id: 'g_outer', mode: 'none', name: 'Outer' }, { id: 'g_inner', mode: 'none', name: 'Inner' }], matrix: [1, 0, 0, 1, 0, 0], role: 'plain', stableTarget: { contextId: 'scene-root', frameScope: { frame: 0, kind: 'authored' }, layerUid: 'layer-1', strokeId: 'native-stroke:3:0' }, strokeWidth: 0, style: { cap: 'round', fill: '#0045cc', join: 'round', opacity: 1, strokeStyle: 'solid', width: 4 } };
  const SHAPE = { bounds: [40.00003807717506, 60.028201010546226, 240, 159.9703146890807], closed: true, count: 182, filled: true, matrix: [1, 0, 0, 1, 0, 0], paramShape: { innerRadius: 0.5, kind: 'ellipse', startAngle: 0, sweep: 359.9 }, role: 'shape', stableTarget: { contextId: 'scene-root', frameScope: { frame: 0, kind: 'authored' }, layerUid: 'layer-1', strokeId: 'native-stroke:2:0' }, strokeWidth: 0, style: { cap: 'round', fill: '#e63333', join: 'round', opacity: 1, strokeStyle: 'solid', width: 4 } };
  const { wire, work } = setup(t, { initial: { listingExtra: [TEXT, NESTED, SHAPE] } });
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  const { layerUid } = await K.fresh(ctx);
  const items = await K.hitTest(ctx, layerUid);
  assert.equal(items.length, 3);
  const text = K.itemOf(items, 'native-text:2:0');
  assert.deepEqual([text.role, text.count, text.matrix, text.bounds], ['text', 0, [1, 0, 0, 1, 0, 0], TEXT.bounds]);
  assert.deepEqual(K.groupIds(K.itemOf(items, 'native-stroke:3:0')), ['g_outer', 'g_inner']);
  assert.deepEqual(K.itemOf(items, 'native-stroke:2:0').paramShape, SHAPE.paramShape);
  assert.deepEqual(K.groupIds(text), []);
});

test('transparent canvas: one revision; the export carries paint as alpha; white paint is alpha 255 with colour distance 0 where the opaque export can not see it', async t => {
  const { wire, work } = setup(t);
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  const { layerUid } = await K.fresh(ctx);
  await make(ctx, layerUid, [20, 20, 100, 60], RED);
  await make(ctx, layerUid, [200, 20, 260, 60], WHITE);
  const opaque = await K.exportPng(ctx, 'opaque');
  assert.equal(opaque.image.inkMode, 'background');
  assert.deepEqual([probe.alphaAt(opaque.image, 5, 5), probe.alphaAt(opaque.image, 230, 40)], [255, 255], 'an opaque canvas is alpha 255 everywhere');
  assert.equal(probe.inkAt(opaque.image, 230, 40), 0, 'on the opaque canvas white paint reads as clear: the blind spot this observation exists for');
  const before = await wire.discover();
  await K.transparentCanvas(ctx);
  assert.equal((await wire.discover()).contentRevision, before.contentRevision + 1);
  assert.equal(ctx.canvas, 'transparent');
  const clear = await K.exportPng(ctx, 'clear');
  assert.equal(clear.image.inkMode, 'alpha');
  assert.deepEqual([probe.alphaAt(clear.image, 5, 5), probe.alphaAt(clear.image, 230, 40), probe.alphaAt(clear.image, 60, 40)], [0, 255, 255]);
  assert.deepEqual([probe.inkAt(clear.image, 5, 5), probe.inkAt(clear.image, 230, 40)], [0, 255], 'on the transparent canvas white paint is ink');
  assert.deepEqual(probe.over(clear.image, 230, 40), [255, 255, 255]);
  assert.deepEqual(probe.inkBounds(clear.image).box, [20, 20, 259, 59]);
  assert.notEqual(clear.digest, opaque.digest);
  await K.step(ctx, 'undo', 'undo');
  assert.equal((await K.exportPng(ctx, 'again')).digest, opaque.digest, 'undo of the canvas edit restores the opaque export exactly');
});

test('on a transparent canvas unpainted pixels are alpha 0 and the ink count is the painted area', async t => {
  const { wire, work } = setup(t);
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  const { layerUid } = await K.fresh(ctx, { transparent: true });
  await make(ctx, layerUid, [20, 20, 100, 60], RED);
  const clear = await K.exportPng(ctx, 'clear');
  assert.equal(probe.alphaAt(clear.image, 300, 300), 0, 'unpainted pixels are alpha 0 on a transparent canvas');
  assert.equal(probe.inkBounds(clear.image).count, 80 * 40);
});

test('the hit-test listing is paged, carries the nested group path and is retried (and the retry recorded) when the presentation changed', async t => {
  const { wire, work, session } = setup(t, { listingRace: 2 });
  const [result] = await runRows({ rows: [mini('listing', async ctx => {
    const { layerUid } = await K.fresh(ctx);
    const ids = [];
    for (let i = 0; i < 10; i++) ids.push((await make(ctx, layerUid, [10 * i, 10, 10 * i + 8, 20], RED)).result.strokeId);
    await K.apply(ctx, 'group', P.groupCreate(layerUid, 'g', 'G', ids.slice(0, 3)));
    const items = await K.hitTest(ctx, layerUid);
    ctx.eq('read', 'every record is listed across pages of eight, in order', items.map(i => i.stableTarget.strokeId), ids);
    ctx.eq('read', 'the group path is read per record', items.map(i => K.groupIds(i)), ids.map((_, i) => (i < 3 ? ['g'] : [])));
  })], session });
  assert.equal(result.verdict, 'pass', result.reason);
  assert.equal(result.retried, true);
  assert.deepEqual(result.retries.map(r => [r.operation, r.refused, r.attempts, r.accepted]), [['query.document.object subselect hit-test', 2, 3, true]]);
  assert.equal(result.retries[0].refusals.every(x => x.code === 'unavailable' && /presentation changed/.test(x.message)), true);
  assert.ok(work);
});

test('the anchors of a record with more than sixteen anchors are paged', async t => {
  const { wire, work } = setup(t);
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  const { layerUid } = await K.fresh(ctx);
  const segs = Array.from({ length: 40 }, (_, i) => P.seg(i, i * 2));
  const made = await K.apply(ctx, 'create', P.vectorCreate(P.vectorRecord({ layerUid, segments: segs, fill: RED })));
  const anchors = await K.anchors(ctx, P.target(layerUid, made.result.strokeId));
  assert.equal(anchors.length, 40);
  assert.deepEqual(anchors.slice(0, 2), [[0, 0], [1, 2]]);
  assert.deepEqual(anchors[39], [39, 78]);
});

test('every retry is data: the row says it was retried, the receipt counts operations, refusals and rows, and an exhausted retry is visible', async t => {
  const { session } = setup(t, { saveRace: true });
  const rows = await runRows({ rows: [
    mini('needs-a-retry', async ctx => {
      const { layerUid } = await K.fresh(ctx);
      await make(ctx, layerUid, [10, 10, 50, 50], RED);
      const saved = await K.saveAs(ctx, 'once');
      ctx.ok('file', 'saved', saved.bytes.length > 0, saved.bytes.length, '>0');
    }),
    mini('needs-nothing', async ctx => {
      await K.fresh(ctx);
      ctx.ok('read', 'nothing to retry', true, true);
    })], session });
  const [a, b] = rows;
  assert.equal(a.verdict, 'pass', a.reason);
  assert.equal(a.retried, true);
  assert.deepEqual(a.retries.map(r => [r.operation, r.attempts, r.refused, r.accepted]), [['project.saveAs', 2, 1, true]]);
  assert.match(a.retries[0].refusals[0].message, /pin-changed-before-picker/);
  assert.equal(a.retries[0].refusals[0].code, 'unavailable');
  assert.equal(b.retried, false);
  assert.deepEqual(b.retries, []);
  const receipt = buildReceipt({ rows: rows.map(r => ({ ...r, lane: '9001' })), selection: { lanes: ['9001'], rows: rows.map(r => r.id), omitted: [], catalogRows: { 9001: 2 } }, cleanup: { ok: true } });
  const totals = receipt.totals.overall.retries;
  assert.equal(totals.retriedOperations >= 1 && totals.refusedAttempts >= 1 && totals.rows === 1, true);
  assert.equal(totals.byOperation['project.saveAs'].retriedOperations, totals.retriedOperations);
  assert.equal(totals.exhausted, 0);
  assert.deepEqual(receipt.totals.byLane['9001'].retries, totals);
  assert.match(receipt.statement, /retries/);
});

test('a retry that runs out of attempts is counted as exhausted and the row fails on the save', async t => {
  const { session } = setup(t, { saveRace: 'always' });
  const [result] = await runRows({ rows: [mini('never-saves', async ctx => { await K.fresh(ctx); await K.saveAs(ctx, 'x'); ctx.ok('file', 'x', true, true); })], session });
  assert.equal(result.verdict, 'fail');
  assert.equal(result.retries.length >= 1 && result.retries.every(r => r.accepted === false && r.attempts === 4 && r.refused === 4), true, JSON.stringify(result.retries.map(r => [r.attempts, r.refused])));
  const receipt = buildReceipt({ rows: [{ ...result, lane: '9001' }], selection: { lanes: ['9001'], rows: [result.id], omitted: [], catalogRows: { 9001: 1 } }, cleanup: { ok: true } });
  assert.ok(receipt.totals.overall.retries.exhausted >= 1);
});

test('a bitmap-brush publication cancelled before dispatch, and one past the shared quota, are the owner\'s typed refusals and change nothing', async t => {
  const { session } = setup(t, { quotaBytes: 2000 });
  const [result] = await runRows({ rows: [mini('brush', async ctx => {
    await K.fresh(ctx);
    const tip = (size, name, rgb) => K.apply(ctx, 'attach ' + name, () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name, path: K.stage(ctx, name + '.png', K.solidPng(size, size, [...rgb, 255])), lifecycleGeneration })));
    const small = await tip(8, 'small', [1, 2, 3]), big = await tip(16, 'big', [4, 5, 6]), big2 = await tip(16, 'big2', [7, 8, 9]);
    const inspect = async id => { const { revision, pin } = await K.pinNow(ctx), b = P.brushInspect(revision, pin, id), r = await ctx.wire.query(b.operation, b.payload); return r.result.brush; };
    const publish = (label, asset, id, size, opts) => K.resource(ctx, label, () => K.pinned(ctx, ({ pin }) => P.brushPublish(pin, asset.result.assetId, id, size, size)), opts);
    const cancelled = await publish('cancelled', small, 'b0', 8, { refused: true, codes: ['cancelled_before_dispatch'], send: { cancelledBeforeDispatch: true }, details: e => e.details.reason === 'cancelled' });
    ctx.eq('reply', 'the refusal is the real cancellation reply', cancelled.res.error, REAL.cancelled);
    ctx.eq('read', 'nothing was published', (await inspect('b0')).resident, false);
    await publish('publish after the cancellation', small, 'b0', 8, { applied: true });
    await publish('first large', big, 'b1', 16, { applied: true });
    await publish('second large is over quota', big2, 'b2', 16, { refused: true, codes: ['unavailable'], details: e => e.details.reason === 'over-quota' });
    const refused = await ctx.wire.command(...(x => [x.operation, x.payload])(await K.pinned(ctx, ({ pin }) => P.brushPublish(pin, big2.result.assetId, 'b2', 16, 16))));
    ctx.eq('reply', 'the refusal is the real over-quota reply', refused.error, REAL.overQuota);
    ctx.eq('read', 'the refused brush is not resident and the first is unchanged', [(await inspect('b2')).resident, (await inspect('b1')).bytes], [false, 16 * 16 * 4 + 128]);
    await publish('replace the first by the small tip', small, 'b1', 8, { applied: true });
    await publish('the refused bitmap now fits', big2, 'b2', 16, { applied: true });
  })], session });
  assert.equal(result.verdict, 'pass', result.reason);
});

// ---- coverage: only a passing row covers -----------------------------------------------------------------------------------------
const row = (id, lane, tasks, verdict, reason) => ({ id, lane, tasks, verdict, ...(reason ? { reason } : {}) });
test('coverage: an id is covered only by a passing row; failing rows are attempted-not-passed; unrun and unselected rows are never attempted', () => {
  const rows = [row('r1', '2107', ['G14'], 'pass'), row('r2', '2107', ['G14'], 'fail', 'x'), row('r3', '2107', ['G22'], 'fail', 'boom'), row('r4', '2107', ['G13'], 'unrun', 'blocked')];
  const c = laneCoverage('2107', rows);
  assert.deepEqual(c.status, { G14: 'covered', G22: 'attempted-not-passed', G13: 'never-attempted' });
  assert.deepEqual(c.coveredByRows, { G14: ['r1'], G22: [], G13: [] });
  assert.deepEqual(Object.keys(c.attemptedNotPassed), ['G22']);
  assert.deepEqual(c.attemptedNotPassed.G22, [{ row: 'r3', verdict: 'fail', reason: 'boom' }]);
  assert.deepEqual(c.alsoNotPassed.G14, [{ row: 'r2', verdict: 'fail', reason: 'x' }], 'a covered id still lists its failing row');
  assert.deepEqual(c.neverAttempted, ['G13']);
  assert.deepEqual(c.coveredIds, ['G14']);
  assert.deepEqual(c.uncoveredIds, ['G22', 'G13']);
  assert.equal(c.laneComplete, false);
});

test('coverage: an id with an unobserved part is partial even when its rows pass, and failing rows never make it covered', () => {
  const rows = [row('t1', '2112', ['G38b'], 'pass'), row('t2', '2112', ['G01'], 'pass')];
  const c = laneCoverage('2112', rows);
  assert.equal(c.status.G38b, 'covered');
  assert.deepEqual(c.partialIds, ['G38b']);
  assert.ok(c.coveredIds.includes('G01') && !c.coveredIds.includes('G38b') && c.uncoveredIds.includes('G38b'));
  const failed = laneCoverage('2112', [row('t1', '2112', ['G38b'], 'fail', 'x')]);
  assert.equal(failed.status.G38b, 'attempted-not-passed');
  assert.deepEqual(failed.partialIds, []);
});

test('coverage: the receipt shows covered, attempted-not-passed and never-attempted ids separately, per lane, and the failing rows never count as covering', () => {
  const rows = [row('f07-fail', '2108', ['F07'], 'fail', 'reorder'), row('d32-pass', '2108', ['G24'], 'pass')];
  const receipt = buildReceipt({ rows: rows.map(r => ({ ...r, operations: [], assertions: [], retries: [] })), selection: { lanes: ['2108'], rows: rows.map(r => r.id), omitted: ['other'], catalogRows: { 2108: 3 } }, cleanup: { ok: true } });
  const c = receipt.coverage['2108'];
  assert.equal(c.status.F07, 'attempted-not-passed');
  assert.equal(c.status.G24, 'covered');
  for (const id of ASSIGNED['2108'].filter(id => !['F07', 'G24'].includes(id))) assert.equal(c.status[id], 'never-attempted', id);
  assert.ok(!c.coveredIds.includes('F07') && c.neverAttempted.includes('F15'));
  assert.equal(receipt.verdict, 'fail');
});
