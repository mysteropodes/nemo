'use strict';
// The installed-evidence kit, wire and runner against a stub app (no app, no nemo-mcp, no installed run): a real pass, a product
// failure, a foreign instance, malformed replies, an unavailable operation, and the rules that stop a vacuous pass.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWire, EvidenceError } = require('../installed-evidence/wire.cjs');
const K = require('../installed-evidence/kit.cjs');
const P = require('../installed-evidence/payloads.cjs');
const probe = require('../installed-evidence/png-probe.cjs');
const { runRows } = require('../installed-evidence/runner.cjs');
const { buildReceipt, exitStatus, EXIT } = require('../installed-evidence/receipt.cjs');
const { createStubApp, INSTANCE, FOREIGN, BUILD } = require('./installed-evidence-stubs.cjs');

function setup(t, flags) {
  const work = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'ie-wire-')));
  t.after(() => fs.rmSync(work, { recursive: true, force: true }));
  const app = createStubApp(flags);
  const wire = createWire({ client: app, expect: { instanceId: INSTANCE, buildId: BUILD } });
  return { app, wire, work, session: { wire, work, recheck: async () => {} } };
}
const createRow = (id = 'mini-create', extra = {}) => ({ id, lane: '9001', issue: 1, title: id, tasks: [], ...extra,
  async run(ctx) {
    await K.fresh(ctx);
    const blank = await K.exportPng(ctx, 'blank');
    ctx.eq('pixel', 'a new project exports no ink', probe.inkBounds(blank.image), null);
    const made = await K.apply(ctx, 'create a rectangle', P.vectorCreate(P.vectorRecord({ layerUid: 'L1', segments: P.rectSegments(20, 20, 160, 140), fill: [0.9, 0.2, 0.2, 1] })));
    const one = await K.exportPng(ctx, 'one');
    ctx.near('pixel', 'the rectangle is on the canvas', probe.inkBounds(one.image).box, [20, 20, 159, 139], 1);
    ctx.ok('read', 'the stroke reads back', (await K.readObject(ctx, P.target('L1', made.result.strokeId))).ok, 'readable');
    await K.step(ctx, 'undo', 'undo');
    ctx.eq('pixel', 'undo restores the blank export', (await K.exportPng(ctx, 'undone')).digest, blank.digest);
    await K.step(ctx, 'redo', 'redo');
    ctx.eq('pixel', 'redo restores the export', (await K.exportPng(ctx, 'redone')).digest, one.digest);
    await K.roundTrip(ctx, 'trip', one);
    await K.apply(ctx, 'a stale-expected-revision command', P.vectorCreate(P.vectorRecord({ layerUid: 'L1', segments: P.rectSegments(1, 1, 5, 5), fill: [1, 1, 1, 1] })), { refused: true, codes: ['stale_revision'], send: { expectedRevision: 99 } });
  } });

test('a real pass: every assertion passes, kinds are mixed, operations carry request and reply digests, exit status 0', async t => {
  const { session, app } = setup(t);
  const [row] = await runRows({ rows: [createRow()], session });
  assert.equal(row.verdict, 'pass', JSON.stringify(row.reason));
  assert.ok(row.assertions.length > 20 && row.assertions.every(a => a.verdict === 'pass' && 'observed' in a && 'expected' in a));
  assert.ok(row.assertions.some(a => a.kind === 'pixel') && row.assertions.some(a => a.kind === 'read') && row.assertions.some(a => a.kind === 'file'));
  assert.ok(row.operations.length > 20 && row.operations.every(o => /^[0-9a-f]{64}$/.test(o.requestDigest) && /^[0-9a-f]{64}$/.test(o.replyDigest)));
  assert.ok(row.operations.some(o => o.operation === 'project.export') && row.operations.some(o => o.operation === 'command.document.object.undo'));
  const receipt = buildReceipt({ candidate: { sourceHead: 'x' }, selection: { lanes: ['9001'] }, rows: [row], cleanup: { ok: true } });
  assert.deepEqual([receipt.verdict, receipt.totals.overall, receipt.totals.byLane['9001']], ['pass', { rows: 1, pass: 1, fail: 0, unrun: 0 }, { rows: 1, pass: 1, fail: 0, unrun: 0 }]);
  assert.equal(exitStatus(receipt.verdict), 0);
  assert.ok(receipt.statement.includes('not the CS7') && receipt.residualHandsOn.length > 0);
  assert.ok(app.state.calls.includes('nemo_discover'));
});

test('a product failure fails the row at the exact assertion, later rows still run, exit status 1', async t => {
  for (const [flags, expected] of [[{ noHistory: true }, /receipt/], [{ brokenUndo: true }, /undo restores the blank export/]]) {
    const { session } = setup(t, flags);
    const rows = await runRows({ rows: [createRow('first'), createRow('second')], session });
    assert.equal(rows[0].verdict, 'fail');
    assert.match(rows[0].reason, expected);
    assert.equal(rows[0].assertions.at(-1).verdict, 'fail');
    assert.notEqual(rows[1].verdict, 'unrun', 'a product failure does not stop the run');
    assert.equal(exitStatus(buildReceipt({ rows, cleanup: { ok: true } }).verdict), EXIT.fail);
  }
});

test('a foreign instance or build stops the run: the row fails and every later row is unrun', async t => {
  for (const flags of [{ foreignReply: true }, { foreignBuild: true }]) {
    const { session } = setup(t, flags);
    const rows = await runRows({ rows: [createRow('a'), createRow('b'), createRow('c')], session });
    assert.equal(rows[0].verdict, 'fail');
    assert.match(rows[0].reason, /foreign/);
    assert.deepEqual(rows.slice(1).map(r => r.verdict), ['unrun', 'unrun']);
    assert.match(rows[1].reason, /run aborted/);
  }
});

test('malformed replies fail the row and are never read as a pass', async t => {
  for (const flags of [{ malformed: 'text' }, { malformed: 'shape' }, { wrongRequestId: true }]) {
    const { session } = setup(t, flags);
    const rows = await runRows({ rows: [createRow('a')], session });
    assert.equal(rows[0].verdict, 'fail', JSON.stringify(flags));
    assert.match(rows[0].reason, /malformed|foreign/, JSON.stringify(flags));
  }
});

test('an unavailable operation makes the row unrun with its reason; the run exits with the distinct status', async t => {
  const { session } = setup(t, { unavailable: true });
  const rows = await runRows({ rows: [createRow('a')], session });
  assert.equal(rows[0].verdict, 'unrun');
  assert.match(rows[0].reason, /not available/);
  const receipt = buildReceipt({ rows, cleanup: { ok: true } });
  assert.equal(receipt.verdict, 'unrun');
  assert.equal(exitStatus('unrun'), EXIT.unrun);
  assert.ok(new Set([EXIT.pass, EXIT.fail, EXIT.refused, EXIT.unrun]).size === 4 && EXIT.unrun !== 3);
});

test('"the command returned ok" is never a pass: a row of replies only, or of nothing, fails', async t => {
  const { session } = setup(t);
  const replyOnly = { id: 'reply-only', lane: '9001', issue: 1, title: 'x', async run(ctx) { await K.fresh(ctx); await K.apply(ctx, 'create', P.vectorCreate(P.vectorRecord({ layerUid: 'L1', segments: P.rectSegments(1, 1, 9, 9), fill: [1, 0, 0, 1] }))); ctx.ok('reply', 'only a reply', true, 'ok'); } };
  const silent = { id: 'silent', lane: '9001', issue: 1, title: 'x', async run() {} };
  // fresh() and apply() also record read assertions, so isolate the reply-only case with a wire-level row.
  const bare = { id: 'bare', lane: '9001', issue: 1, title: 'x', async run(ctx) { ctx.ok('reply', 'only a reply', true, 'ok'); } };
  const rows = await runRows({ rows: [bare, silent, replyOnly], session });
  assert.deepEqual(rows.map(r => r.verdict), ['fail', 'fail', 'pass']);
  assert.match(rows[0].reason, /no independent observation/);
  assert.match(rows[1].reason, /no assertion/);
});

test('an unexpected row error is a failure naming its class, never a pass', async t => {
  const { session } = setup(t);
  const [row] = await runRows({ rows: [{ id: 'bug', lane: '9001', issue: 1, title: 'x', async run() { null.field; } }], session });
  assert.equal(row.verdict, 'fail');
  assert.match(row.reason, /row error \(TypeError\)/);
});

test('a candidate that changes during a row stops the run', async t => {
  const { wire, work } = setup(t);
  let calls = 0;
  const session = { wire, work, recheck: async () => { if (++calls === 3) throw new Error('candidate process changed during the run'); } };
  const rows = await runRows({ rows: [createRow('a'), createRow('b')], session });
  assert.equal(rows[0].verdict, 'pass');
  assert.equal(rows[1].verdict, 'fail');
  assert.match(rows[1].reason, /foreign: before the row/);
});

test('the wire judges raw replies: identity, request echo, revision, typed errors, front-door refusals', async t => {
  const reply = value => ({ result: { structuredContent: value } });
  const client = responses => ({ calls: [], async callTool(name, args) { this.calls.push(name); if (name === 'nemo_discover') return reply({ instances: [{ instanceId: INSTANCE, buildId: BUILD, nativeHostStatus: { instanceId: INSTANCE, available: true, documentId: 'd', contentRevision: 3, lifecycleGeneration: 1 } }] });
    return reply(responses(args)); } });
  const good = args => ({ apiVersion: 2, requestId: args.requestId, instanceId: INSTANCE, documentId: 'd', contentRevision: 3, ok: true, result: { x: 1 } });
  const run = async (responses, call) => createWire({ client: client(responses), expect: { instanceId: INSTANCE, buildId: BUILD } })[call ?? 'query']('query.document.revision', {});
  assert.equal((await run(good)).ok, true);
  const bad = async (responses, kind) => await assert.rejects(run(responses), error => error instanceof EvidenceError && error.kind === kind, kind);
  await bad(args => ({ ...good(args), requestId: 'other' }), 'malformed');
  await bad(args => ({ ...good(args), instanceId: FOREIGN }), 'foreign');
  await bad(args => ({ ...good(args), documentId: 'other-doc' }), 'foreign');
  await bad(args => ({ ...good(args), contentRevision: 'three' }), 'malformed');
  await bad(args => ({ ...good(args), ok: 'yes' }), 'malformed');
  await bad(args => ({ ...good(args), result: undefined }), 'malformed');
  await bad(args => ({ ...good(args), ok: false, result: undefined }), 'malformed');
  await bad(() => ({ ok: false, error: { code: 'unsupported_capability', message: 'x' } }), 'unavailable');
  await bad(() => ({ ok: false, error: { code: 'unavailable', message: 'not registered' } }), 'unavailable');
  const appLevel = await run(args => ({ ...good(args), ok: false, result: undefined, error: { code: 'unavailable', message: 'x', details: { disposition: 'indeterminate' } } }));
  assert.deepEqual([appLevel.ok, appLevel.error.code], [false, 'unavailable'], 'an unavailable answer from the application itself is an observation, not an absent capability');
  const typed = await run(args => ({ ...good(args), ok: false, result: undefined, error: { code: 'stale_revision', message: 'stale' } }));
  assert.equal(typed.ok, false);
  const none = createWire({ client: { async callTool() { throw new Error('socket closed'); } }, expect: { instanceId: INSTANCE, buildId: BUILD } });
  await assert.rejects(none.discover(), error => error.kind === 'transport');
  const two = createWire({ client: { async callTool() { return reply({ instances: [{}, {}] }); } } });
  await assert.rejects(two.discover(), error => error.kind === 'foreign');
});

test('png probes measure ink, colours, differences and coverage on decoded pixels', () => {
  const image = probe.decodeExport(K.solidPng(20, 10, [200, 0, 0, 255]));
  assert.deepEqual(probe.inkBounds(image), { box: [0, 0, 19, 9], count: 200 });
  assert.equal(probe.countColor(image, [200, 0, 0]), 200);
  const half = probe.decodeExport(K.solidPng(20, 10, [200, 0, 0, 128]));
  assert.equal(probe.differing(image, half), 200);
  assert.equal(probe.differing(image, image), 0);
  assert.equal(probe.differing(image, probe.decodeExport(K.solidPng(10, 10, [0, 0, 0, 255]))), Infinity);
  assert.deepEqual(probe.over(half, 1, 1), [227, 127, 127]);
  assert.ok(Math.abs(probe.coverageRatio(image, half, [0, 0, 19, 9]) - 0.5) < 0.05);
  assert.throws(() => probe.decodeExport(Buffer.from('nope')), /not a PNG/);
});
