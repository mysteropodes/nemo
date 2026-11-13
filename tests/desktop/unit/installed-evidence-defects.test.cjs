'use strict';
// Fault injection for the installed-evidence kit and rows. Each test switches ONE misbehaviour on in the stub app and shows the
// row (or the kit) now fails where the earlier assertion would have passed, and that the same row passes against the
// well-behaved stub. Stubs only: no app is started and nothing here is installed evidence.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWire } = require('../installed-evidence/wire.cjs');
const K = require('../installed-evidence/kit.cjs');
const P = require('../installed-evidence/payloads.cjs');
const probe = require('../installed-evidence/png-probe.cjs');
const { runRows, allRows } = require('../installed-evidence/runner.cjs');
const lane2108 = require('../installed-evidence/lanes/lane-2108.cjs');
const { createStubApp, INSTANCE, BUILD } = require('./installed-evidence-stubs.cjs');

function setup(t, flags) {
  const work = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'ie-defects-')));
  t.after(() => fs.rmSync(work, { recursive: true, force: true }));
  const app = createStubApp(flags);
  const wire = createWire({ client: app, expect: { instanceId: INSTANCE, buildId: BUILD } });
  return { app, wire, work, session: { wire, work, recheck: async () => {} } };
}
const catalog = allRows();
const lane = id => catalog.find(r => r.id === id);
const mini = (id, run) => ({ id, lane: '9001', issue: 1, title: id + ' title', tasks: [], run });
const RED = [0.9, 0.2, 0.2, 1];
const rect = (layerUid, x0, y0, x1, y1) => P.vectorCreate(P.vectorRecord({ layerUid, segments: P.rectSegments(x0, y0, x1, y1), fill: RED }));
async function verdict(t, flags, row) {
  const { session } = setup(t, flags);
  const [result] = await runRows({ rows: [row], session });
  return result;
}

// ---- the lane rows run for real against the well-behaved stub --------------------------------------------------------------
test('the new and changed lane rows pass against a stub that follows the contract (the stub is not what is being proved)', async t => {
  for (const id of ['2113-image-mesh-sculpt', '2112-asset-attach-dedup-refusals', '2112-asset-remove-lifetime']) {
    const result = await verdict(t, {}, lane(id));
    assert.equal(result.verdict, 'pass', id + ': ' + result.reason);
    assert.ok(result.assertions.some(a => a.kind === 'pixel') && result.assertions.some(a => a.kind === 'read'), id);
  }
});

// ---- defect 1 and 2: the mesh ---------------------------------------------------------------------------------------------
test('image.mesh.edit answers only its receipt: the row reads the mesh id through the imageMesh view instead of the command reply', async t => {
  const { wire, work } = setup(t);
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  const { layerUid } = await K.fresh(ctx);
  const att = await K.apply(ctx, 'attach', () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name: 'n', path: K.stage(ctx, 'q.png', K.quadrantPng(8, 8, [[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]])), lifecycleGeneration })));
  assert.ok(layerUid);
  const mesh = lane('2113-image-mesh-sculpt');
  assert.ok(mesh);
  const bound = await K.apply(ctx, 'bind', P.meshAttach(att.result.target, { outline: [[0, 0], [1, 0], [1, 1]], verts: [[0, 0], [1, 0], [1, 1]], tris: [0, 1, 2], offsets: [[0, 0], [0, 0], [0, 0]], cols: 1, rows: 1 }));
  assert.deepEqual(Object.keys(bound.res.result).sort(), ['applied', 'historyEntriesAdded'], 'the real reply carries no mesh id, so a row that requires one can never run');
  const state = await K.meshState(ctx, att.result.target.layerUid);
  assert.match(state.mesh.meshId, /^im_/);
  assert.deepEqual([state.mesh.vertexCount, state.mesh.offsetsPresent, state.offsets.length], [3, true, 3]);
});

// A transcription of the imageMesh result rules nemo-mcp enforces (native_image_mesh_read_contract.rs), to hold the stub's own
// replies to the contract rather than to what the row happens to read.
function meshResultShape(request, result) {
  const view = request.payload.view, keys = Object.keys(result).sort().join();
  assert.equal(keys, 'atRevision,contextId,documentSnapshotId,frame,kind,layerUid,mesh,page,rect,target,viewGeneration,workId');
  for (const key of ['contextId', 'layerUid', 'frame', 'workId', 'viewGeneration']) assert.equal(result[key], view[key], key);
  assert.equal(result.documentSnapshotId, request.payload.documentSnapshotId);
  assert.deepEqual(Object.keys(result.target).sort(), ['contextId', 'frameScope', 'layerUid', 'strokeId']);
  assert.ok(result.rect.width > 0 && result.rect.height > 0 && 'rotation' in result.rect);
  if (result.mesh !== null) {
    assert.deepEqual(Object.keys(result.mesh).sort(), ['cols', 'meshId', 'offsetsPresent', 'outlineCount', 'rows', 'triangleIndexCount', 'vertexCount']);
    assert.ok(result.mesh.outlineCount <= result.mesh.vertexCount && result.mesh.triangleIndexCount % 3 === 0);
  }
  if (view.page.kind === 'selection') return assert.deepEqual(result.page, { kind: 'selection' });
  const { page } = result, end = page.cursor + page.items.length;
  assert.deepEqual(Object.keys(page).sort(), ['cursor', 'items', 'kind', 'nextCursor', 'total']);
  assert.ok(page.items.length <= view.page.limit && (end === page.total ? page.nextCursor === null : page.nextCursor === end));
}
test('the stub holds its own mesh replies to the published result rules', async t => {
  const { wire, work, app } = setup(t);
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  await K.fresh(ctx);
  const file = K.stage(ctx, 'q.png', K.quadrantPng(8, 8, [[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]));
  const att = await K.apply(ctx, 'attach', () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name: 'n', path: file, lifecycleGeneration })));
  await K.apply(ctx, 'bind', P.meshAttach(att.result.target, { outline: [[0, 0], [1, 0], [1, 1]], verts: [[0, 0], [1, 0], [1, 1]], tris: [0, 1, 2], offsets: [[0, 0], [0, 0], [0, 0]], cols: 1, rows: 1 }));
  const seen = [];
  const original = app.callTool.bind(app);
  app.callTool = async (name, args) => { const reply = await original(name, args); if (args && args.payload && args.payload.view && args.payload.view.kind === 'imageMesh') seen.push([args, reply.result.structuredContent.result]); return reply; };
  await K.meshState(ctx, att.result.target.layerUid);
  assert.ok(seen.length >= 2, 'a selection read and an offsets read');
  for (const [request, result] of seen) meshResultShape(request, result);
});

test('a mesh that is acknowledged but never bound, or a sculpt that changes nothing, fails the mesh row (the earlier digest-only check passed on seam noise)', async t => {
  const never = await verdict(t, { meshNeverBound: true }, lane('2113-image-mesh-sculpt'));
  assert.equal(never.verdict, 'fail');
  assert.match(never.reason, /a mesh is bound with an id/);
  const ignored = await verdict(t, { meshOffsetsIgnored: true }, lane('2113-image-mesh-sculpt'));
  assert.equal(ignored.verdict, 'fail');
  assert.match(ignored.reason, /the offset of vertex 2 is the one sent/);
});

// ---- defect 3: self comparison --------------------------------------------------------------------------------------------
test('an earlier export is re-read from disk: a changed or removed file fails, an intact one passes', async t => {
  const row = mutate => mini('intact', async ctx => {
    await K.fresh(ctx);
    const first = await K.exportPng(ctx, 'first');
    // The old check compared the decoded pixels' digest with the digest computed from the same pixels: true by construction.
    assert.equal(probe.pixels(first.image), first.digest);
    mutate(first);
    K.exportIntact(ctx, 'the earlier export is intact', first);
  });
  assert.equal((await verdict(t, {}, row(() => {}))).verdict, 'pass');
  const changed = await verdict(t, {}, row(first => fs.appendFileSync(first.file, 'x')));
  assert.equal(changed.verdict, 'fail');
  assert.match(changed.reason, /the earlier export is intact/);
  assert.equal((await verdict(t, {}, row(first => fs.rmSync(first.file)))).verdict, 'fail');
});

// ---- defect 4 and 5: negative controls prove the owner's rule ---------------------------------------------------------------
test('a refusal counts only when it is a typed refusal by the native owner: front-door replies and the wrong code fail the control', async t => {
  const row = (opts) => mini('control', async ctx => {
    const { layerUid } = await K.fresh(ctx);
    await K.apply(ctx, 'create', rect(layerUid, 10, 10, 50, 50));
    await K.apply(ctx, 'replace a stroke that does not exist', P.vectorReplace(P.vectorRecord({ layerUid, strokeId: 'native-stroke:99:9', segments: P.rectSegments(1, 1, 9, 9), fill: RED })), { refused: true, ...opts });
    ctx.ok('read', 'row observation', true);
  });
  assert.equal((await verdict(t, {}, row())).verdict, 'pass', 'an owner not_found refusal passes the default control');
  const front = await verdict(t, { frontDoorRefusals: true }, row());
  assert.equal(front.verdict, 'fail');
  assert.match(front.reason, /refused by the native owner, not by the transport front door/);
  for (const code of ['unavailable', 'internal', 'stale_revision']) {
    const wrong = await verdict(t, { refusalCode: code }, row());
    assert.equal(wrong.verdict, 'fail', code);
    assert.match(wrong.reason, /typed owner refusal/, code);
  }
  assert.equal((await verdict(t, { refusalCode: 'stale_revision' }, row({ codes: ['stale_revision'] }))).verdict, 'pass', 'a control that names its code accepts exactly it');
  const detail = await verdict(t, {}, row({ details: () => false }));
  assert.match(detail.reason, /refusal details/);
});

test('an oversize control is a valid PNG past the size cap (a header with no pixels is only a decode failure), and a missing source is a typed not-dispatched refusal', async t => {
  const big = probe.decodeExport(K.oversizePng());
  assert.deepEqual([big.width, big.height], [K.OVERSIZE_WIDTH, 1], 'the file decodes, so only a size cap can refuse it');
  assert.ok(K.OVERSIZE_WIDTH > 8192);
  const { wire, work } = setup(t);
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  await K.fresh(ctx);
  const attach = (file, opts) => K.apply(ctx, 'attach', () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name: 'n', path: file, lifecycleGeneration })), opts);
  await attach(K.stage(ctx, 'big.png', K.oversizePng()), { refused: true, codes: ['unavailable'] });
  await attach(K.stage(ctx, 'ok.png', K.solidPng(8192, 1, [1, 2, 3, 255])));
  await attach(path.join(work, 'missing.png'), { refused: true, codes: ['unavailable'], details: e => e.details.disposition === 'notDispatched' && /image source/.test(e.message) });
  await assert.rejects(attach(path.join(work, 'missing.png'), { refused: true }), /typed owner refusal/, 'the default owner codes do not accept an unavailable source');
});

test('a resource publication is judged exactly: no revision, no history; an edit disguised as a publication fails (the earlier check was a non-strict inequality)', async t => {
  const row = mini('publish', async ctx => {
    await K.fresh(ctx);
    const file = K.stage(ctx, 'tip.png', K.solidPng(8, 8, [1, 2, 3, 255]));
    const att = await K.apply(ctx, 'attach', () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name: 'n', path: file, lifecycleGeneration })));
    const publish = (label, assetId, w, h, opts) => K.resource(ctx, label, () => K.pinned(ctx, ({ pin }) => P.brushPublish(pin, assetId, 'b', w, h)), opts);
    const first = await publish('publish', att.result.assetId, 8, 8, { applied: true });
    ctx.eq('reply', 'published size', [first.result.width, first.result.height], [8, 8]);
    await publish('publish again', att.result.assetId, 8, 8, { applied: false });
    await publish('wrong size 32x32 (the transport admits it, the owner refuses it)', att.result.assetId, 32, 32, { refused: true });
    await publish('unknown asset', 'asset-none', 8, 8, { refused: true, codes: ['not_found'] });
  });
  assert.equal((await verdict(t, {}, row)).verdict, 'pass');
  const edit = await verdict(t, { publishAddsHistory: true }, row);
  assert.equal(edit.verdict, 'fail');
  assert.match(edit.reason, /publishing is not an edit/);
  const front = await verdict(t, { frontDoorRefusals: true }, row);
  assert.equal(front.verdict, 'fail');
  assert.match(front.reason, /front door/);
});

test('removing an asset that a placement still uses must be refused: a host that removes it anyway fails the lifetime row', async t => {
  const result = await verdict(t, { removeIgnoresUse: true }, lane('2112-asset-remove-lifetime'));
  assert.equal(result.verdict, 'fail');
  assert.match(result.reason, /remove the asset while a placement uses it/);
});

// ---- defect 6: independent observation, per behaviour -----------------------------------------------------------------------
test('a row or a behaviour that rests on replies and kit bookkeeping alone fails; one real observation passes', async t => {
  const replyOnly = mini('reply-only', async ctx => { const { layerUid } = await K.fresh(ctx); await K.apply(ctx, 'create', rect(layerUid, 1, 1, 9, 9)); ctx.ok('reply', 'only a reply', true, 'ok'); });
  const result = await verdict(t, {}, replyOnly);
  assert.equal(result.verdict, 'fail', 'setup and per-command revision reads are bookkeeping, not an observation');
  assert.match(result.reason, /no independent observation/);
  assert.ok(result.assertions.some(a => a.bookkeeping === true && a.kind === 'read'), 'bookkeeping reads are recorded and marked');
  const behaviour = observed => mini('behaviour', async ctx => {
    const { layerUid } = await K.fresh(ctx);
    await K.apply(ctx, 'create', rect(layerUid, 20, 20, 100, 100));
    const one = await K.exportPng(ctx, 'one');
    ctx.ok('pixel', 'another claim of the row', probe.inkBounds(one.image) !== null, 'ink');
    await ctx.behavior('the second rectangle is drawn', async () => {
      await K.apply(ctx, 'create a second rectangle', rect(layerUid, 200, 20, 300, 100));
      if (observed) ctx.ok('pixel', 'the second rectangle paints', probe.alphaAt((await K.exportPng(ctx, 'two')).image, 250, 60) > 200, 'inked');
    });
  });
  assert.equal((await verdict(t, {}, behaviour(true))).verdict, 'pass');
  const unobserved = await verdict(t, {}, behaviour(false));
  assert.equal(unobserved.verdict, 'fail', 'the row has other claims, but this behaviour was judged on its command reply only');
  assert.match(unobserved.reason, /behaviour "the second rectangle is drawn" is observed independently/);
});

// ---- defect 7: reopen reads the file ---------------------------------------------------------------------------------------
test('save then open must load the file as a new document: a host that keeps the in-memory document fails every round trip', async t => {
  const row = mini('trip', async ctx => {
    const { layerUid } = await K.fresh(ctx);
    await K.apply(ctx, 'create', rect(layerUid, 20, 20, 100, 100));
    await K.roundTrip(ctx, 'trip', await K.exportPng(ctx, 'before'));
  });
  const good = await verdict(t, {}, row);
  assert.equal(good.verdict, 'pass', good.reason);
  assert.ok(good.assertions.some(a => /different document instance/.test(a.id)) && good.assertions.some(a => /lifecycle generation advanced/.test(a.id)));
  const kept = await verdict(t, { keepsInMemory: true }, row);
  assert.equal(kept.verdict, 'fail');
  assert.match(kept.reason, /loaded as a different document instance/);
  assert.ok(kept.assertions.some(a => /reopen: accepted/.test(a.id) && a.verdict === 'pass') && !kept.assertions.some(a => /byte-stable/.test(a.id)), 'the old checks (accepted, pixel-equal, byte-stable) never got the chance to pass by accident');
});

// ---- defect 8: flatten is observed ----------------------------------------------------------------------------------------
test('flatten is judged on the saved project: members and group id consumed, a result present; a flatten that only bumps the revision fails', async t => {
  const row = mini('flatten', async ctx => {
    const { layerUid } = await K.fresh(ctx);
    const a = (await K.apply(ctx, 'a', rect(layerUid, 100, 60, 300, 240))).result.strokeId, b = (await K.apply(ctx, 'b', rect(layerUid, 160, 100, 240, 200))).result.strokeId;
    await K.apply(ctx, 'group', P.groupCreate(layerUid, 'grp_x', 'G', [a, b]));
    const { results, styleId } = await lane2108.helpers.flattenGroup(ctx, layerUid, 'grp_x', [a, b]);
    // The baseline keeps the style member's (the last member's) id on the first island, so no NEW id appears for a one-island result.
    ctx.eq('file', 'the style member is the result and no new id appeared', [styleId, results.length], [b, 0]);
    await K.step(ctx, 'undo', 'undo');
    ctx.eq('read', 'undo restores both records', (await Promise.all([a, b].map(id => K.readObject(ctx, P.target(layerUid, id))))).every(r => r.ok), true);
  });
  assert.equal((await verdict(t, {}, row)).verdict, 'pass');
  const noop = await verdict(t, { flattenNoop: true }, row);
  assert.equal(noop.verdict, 'fail');
  assert.match(noop.reason, /flatten consumed the members/);
  assert.ok(noop.assertions.some(a => /flatten the group: exactly one history entry/.test(a.id) && a.verdict === 'pass'), 'the revision and history bump alone looked fine');
});

// ---- the pin and the layer order on an object project (real reply shapes seen on the packaged candidate) -------------------------
test('an object project refuses the opacity-document queries: the pin comes from the presentation view and the layer order from the saved project', async t => {
  const { wire, work, app } = setup(t);
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  await K.fresh(ctx);
  for (const operation of ['query.document.snapshot.acquire', 'query.document.serialize', 'query.document.evaluate']) {
    const refused = await wire.query(operation, { atRevision: 0 });
    assert.equal(refused.ok, false);
    assert.deepEqual([refused.error.code, refused.error.message], ['invalid_request', 'Unsupported object read operation.'], operation);
  }
  const { revision, pin } = await K.pinNow(ctx);
  assert.equal(revision, 0);
  assert.equal(typeof pin, 'string');
  assert.deepEqual(await K.layerOrder(ctx), ['L1']);
  assert.ok(app);
});

test('a save refused once as unavailable right after a change is retried and the refusal stays on record', async t => {
  const { wire, work } = setup(t, { saveRace: true });
  const ctx = K.createContext({ wire, work, rowId: 'x' });
  const { layerUid } = await K.fresh(ctx);
  await K.apply(ctx, 'create', rect(layerUid, 0, 0, 10, 10));
  const saved = await K.saveAs(ctx, 'race');
  assert.ok(saved.document, 'the retried save wrote the file');
  const refused = ctx.assertions.filter(a => /save attempt 1 refused as unavailable \(pin-changed-before-picker\)/.test(a.id));
  assert.equal(refused.length, 1);
});

test('file names never repeat across rows: two contexts of one run do not share a counter', () => {
  const a = K.createContext({ wire: null, work: '/w', rowId: 'a' }), b = K.createContext({ wire: null, work: '/w', rowId: 'b' });
  const seen = new Set([a.next(), a.next(), b.next(), b.next()]);
  assert.equal(seen.size, 4);
});

test('a released project answers "No native project is open." with no document id: a typed refusal, not a foreign reply', async t => {
  const { wire, work } = setup(t, { released: true });
  await wire.discover();
  const released = await wire.v1('project.saveAs', { path: path.join(work, 'x.json') });
  assert.equal(released.ok, false);
  assert.equal(released.error.message, 'No native project is open.');
});

test('a row marked last runs after every other selected row', async t => {
  const { session } = setup(t, {});
  const order = [];
  const row = (id, last) => ({ ...mini(id, async ctx => { order.push(id); ctx.ok('read', 'x', true, true); }), ...(last ? { last: true } : {}) });
  const results = await runRows({ rows: [row('first-last', true), row('second'), row('third')], session });
  assert.deepEqual(order, ['second', 'third', 'first-last']);
  assert.deepEqual(results.map(r => r.id), order);
});

test('an app that asks to be reopened leaves the row unrun with that reason, not failed', async t => {
  const result = await verdict(t, { unconfirmed: true }, mini('blocked', async ctx => { await K.fresh(ctx); }));
  assert.equal(result.verdict, 'unrun');
  assert.match(result.reason, /blocked: the app asks to be reopened/);
});

// ---- #2108 nested group operand and flatten: the paint rule and the baseline flatten rule are different rules ----------------------------------
test('2108-nested-group-operand-flatten passes against a stub that follows both rules and fails for each rule it breaks', async t => {
  const row = lane('2108-nested-group-operand-flatten');
  assert.ok(row);
  const good = await verdict(t, {}, row);
  assert.equal(good.verdict, 'pass', good.reason);
  const said = text => good.assertions.some(a => text.test(a.id) && a.verdict === 'pass');
  assert.ok(said(/paint: both bars are inked/) && said(/flatten Outer, whose all-leaves fold is empty: refused/) && said(/refused flatten left every record/) && said(/the ink is the refit curve of the all-leaves fold/), 'paint, refusal, unchanged document and the fold are each asserted');
  // Flatten that collapses the nested group into one operand (the paint rule) yields the bars instead of refusing: the baseline rule says it must refuse.
  const operand = await verdict(t, { flattenNestedAsOperand: true }, row);
  assert.equal(operand.verdict, 'fail');
  assert.match(operand.reason, /flatten Outer, whose all-leaves fold is empty/);
  // The baseline JS deleted the members of an empty fold; the native owner must refuse and leave the document alone.
  const deletes = await verdict(t, { flattenEmptyDeletes: true }, row);
  assert.equal(deletes.verdict, 'fail');
  assert.match(deletes.reason, /flatten Outer, whose all-leaves fold is empty/);
  // A flatten that only bumps the revision and the history shows neither the fold nor the consumed members.
  const bump = await verdict(t, { flattenNoop: true }, row);
  assert.equal(bump.verdict, 'fail');
  // An export that ignores group.combine paints the three rectangles as they are: both bars are inked but so is the ring and the gap.
  const ignored = await verdict(t, { groupCombineIgnored: true }, row);
  assert.equal(ignored.verdict, 'fail');
  assert.match(ignored.reason, /paint: the gap between the bars/);
  // The refusal must be the native owner's: a front-door reply or another code does not count.
  for (const flags of [{ frontDoorRefusals: true }, { refusalCode: 'unavailable' }]) {
    const refused = await verdict(t, flags, row);
    assert.equal(refused.verdict, 'fail', JSON.stringify(flags));
  }
});

// ---- #2108 group combine and flatten, the two rules the first installed run showed the rows had wrong ----------------------------------------
test('2108-group-combine-flatten passes against a stub that follows the baseline and fails when the combine styles from the wrong member or flatten leaves members behind', async t => {
  const row = lane('2108-group-combine-flatten');
  assert.ok(row);
  const good = await verdict(t, {}, row);
  assert.equal(good.verdict, 'pass', good.reason);
  assert.ok(good.assertions.some(a => /the ring takes the colour of the topmost member/.test(a.id) && a.verdict === 'pass'));
  // The first installed run: the ring was painted with the inner (topmost) member's blue. A paint that takes the first member's style (the old expectation) now fails.
  const first = await verdict(t, { combineStyleFromFirst: true }, row);
  assert.equal(first.verdict, 'fail');
  assert.match(first.reason, /the ring takes the colour of the topmost member/);
  // A flatten that mints a new id instead of keeping the style member's leaves the style member consumed: the result must be that very record.
  const newId = await verdict(t, { flattenNewId: true }, row);
  assert.equal(newId.verdict, 'fail');
  const noop = await verdict(t, { flattenNoop: true }, row);
  assert.equal(noop.verdict, 'fail');
});

// The expected refit constants of the nested row are Paper.js simplify(2.5) of the overlap rectangle, not numbers typed to match one app run.
test('2108-nested-group-operand-flatten: the refit hull and ink constants are what the shipped Paper.js gives for the overlap rectangle', () => {
  const paper = require('../../helpers/native-import-paper.cjs').load();
  const path = new paper.Path({ insert: false });
  [[200, 220], [200, 100], [250, 100], [250, 220]].forEach(([x, y]) => path.add(new paper.Point(x, y)));
  path.closed = true;
  path.simplify(2.5);
  const hull = path.segments.reduce((a, g) => {
    const xs = [g.point.x, g.point.x + g.handleIn.x, g.point.x + g.handleOut.x], ys = [g.point.y, g.point.y + g.handleIn.y, g.point.y + g.handleOut.y];
    return [Math.min(a[0], ...xs), Math.min(a[1], ...ys), Math.max(a[2], ...xs), Math.max(a[3], ...ys)];
  }, [Infinity, Infinity, -Infinity, -Infinity]);
  const rows = require('../installed-evidence/lanes/rows-2108-groups.cjs');
  assert.equal(path.segments.length, 2);
  assert.ok(probe.near(hull, rows.REFIT_HULL, 0.01), JSON.stringify(hull));
  assert.ok(probe.near([path.bounds.x, path.bounds.y, path.bounds.right - 1, path.bounds.bottom - 1], rows.REFIT_INK, 0.02), JSON.stringify(path.bounds));
});

test('2108-nested-group-operand-flatten fails a flatten that leaves the sharp overlap box, or a new id, instead of the baseline refit of the style member', async t => {
  const row = lane('2108-nested-group-operand-flatten');
  const good = await verdict(t, {}, row);
  assert.equal(good.verdict, 'pass', good.reason);
  assert.ok(good.assertions.some(a => /the result hull is the Paper.js simplify/.test(a.id) && a.verdict === 'pass'));
  const sharp = await verdict(t, { flattenSharpBox: true }, row);
  assert.equal(sharp.verdict, 'fail');
  assert.match(sharp.reason, /two anchors/);
  const newId = await verdict(t, { flattenNewId: true }, row);
  assert.equal(newId.verdict, 'fail');
});
