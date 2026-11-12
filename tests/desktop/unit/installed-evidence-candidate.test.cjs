'use strict';
// The candidate guards and the entry point: the runner refuses unless the instance it talks to is the candidate, never starts
// anything on a bad request, always cleans up, and writes a receipt whose verdict decides the exit status. Stubs only.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { verifyPackage, verifyBinding } = require('../installed-evidence/candidate.cjs');
const { main, cli, parse, refusalReason } = require('../installed-evidence/run.cjs');
const K = require('../installed-evidence/kit.cjs');
const P = require('../installed-evidence/payloads.cjs');
const probe = require('../installed-evidence/png-probe.cjs');
const { createStubApp, INSTANCE, BUILD } = require('./installed-evidence-stubs.cjs');

const HEAD = 'a'.repeat(40), EXE = 'e'.repeat(64), MCP = 'm'.repeat(64).replace(/m/g, 'c');
const proof = (over = {}) => ({ status: 'pass', details: { source: { head: HEAD, dirty: false }, cleanup: { stopped: true, released: true } },
  artifacts: [{ path: 'x/Nemo.app/Contents/MacOS/nemo', sha256: EXE }, { path: 'x/Nemo.app/Contents/MacOS/nemo-mcp', sha256: MCP }], ...over });
const source = (over = {}) => ({ head: HEAD, dirty: false, ...over });
const refuses = (fn, pattern) => assert.throws(fn, error => pattern.test(error.message), String(pattern));

test('package proof: lists the app executable and the bundled nemo-mcp by sha256 for this clean head, or refuses', () => {
  assert.deepEqual(verifyPackage({ proof: proof(), source: source(), executableSha256: EXE, mcpSha256: MCP }), { executableSha256: EXE, mcpSha256: MCP });
  const bad = (p, s, e, m, pattern) => refuses(() => verifyPackage({ proof: p, source: s, executableSha256: e, mcpSha256: m }), pattern);
  bad(proof(), source(), 'f'.repeat(64), MCP, /app executable differs/);
  bad(proof(), source(), EXE, 'f'.repeat(64), /nemo-mcp differs/);
  bad(proof({ artifacts: proof().artifacts.slice(0, 1) }), source(), EXE, MCP, /nemo-mcp differs/);
  bad(proof({ artifacts: proof().artifacts.slice(1) }), source(), EXE, MCP, /app executable differs/);
  bad(proof({ status: 'fail' }), source(), EXE, MCP, /passing standard build proof/);
  bad(proof(), source({ dirty: true }), EXE, MCP, /clean committed source/);
  bad(proof(), source({ head: 'b'.repeat(40) }), EXE, MCP, /another source head/);
  bad(proof({ details: { source: { head: HEAD, dirty: true }, cleanup: { stopped: true, released: true } } }), source(), EXE, MCP, /another source head/);
  bad(proof({ details: { source: { head: HEAD, dirty: false }, cleanup: { stopped: true, released: false } } }), source(), EXE, MCP, /cleanup/);
  bad(null, source(), EXE, MCP, /passing standard build proof/);
});

test('binding: build id is app version plus the clean head, one discovered application, and the listener is the process we started', () => {
  const record = { instanceId: INSTANCE, port: 4000, buildId: BUILD }, identity = { pid: 77, sourceHead: 'a'.repeat(40), appVersion: '1.2.3' };
  const discovered = [{ instanceId: INSTANCE, buildId: BUILD }];
  assert.deepEqual(verifyBinding({ record, discovered, listeners: [77], identity }), { instanceId: INSTANCE, buildId: BUILD, port: 4000, pid: 77 });
  const bad = (over, pattern) => refuses(() => verifyBinding({ record, discovered, listeners: [77], identity, ...over }), pattern);
  bad({ listeners: [78] }, /not owned by the process/);
  bad({ listeners: [77, 78] }, /not owned by the process/);
  bad({ listeners: [] }, /not owned by the process/);
  bad({ record: { ...record, buildId: BUILD + '-dirty' } }, /not the candidate build/);
  bad({ record: { ...record, buildId: '1.2.4:' + 'a'.repeat(40) } }, /not the candidate build/);
  bad({ record: { ...record, buildId: '1.2.3:' + 'b'.repeat(40) } }, /not the candidate build/);
  bad({ discovered: [] }, /exactly one discovered/);
  bad({ discovered: [discovered[0], discovered[0]] }, /exactly one discovered/);
  bad({ discovered: [{ instanceId: 'another-instance', buildId: BUILD }] }, /not the candidate/);
  bad({ discovered: [{ instanceId: INSTANCE, buildId: '1.2.3:' + 'b'.repeat(40) }] }, /not the candidate/);
  bad({ identity: { ...identity, sourceHead: 'short' } }, /not the candidate build/);
});

function fixture(t, { app = createStubApp(), rows, over = {} } = {}) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'ie-run-')));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const reportDir = path.join(dir, 'report'), work = path.join(dir, 'work'); fs.mkdirSync(reportDir); fs.mkdirSync(work);
  fs.writeFileSync(path.join(dir, 'proof.json'), '{}');
  const state = { opened: 0, cleaned: 0, rechecks: 0 };
  const row = (id, run) => ({ id, lane: '9001', issue: 1, title: id + ' title', tasks: ['T'], run });
  const catalog = rows || [row('9001-one', async ctx => { await K.fresh(ctx); const e = await K.exportPng(ctx, 'blank'); ctx.eq('pixel', 'blank', probe.inkBounds(e.image), null); })];
  const out = [];
  const deps = { catalog, stdout: { write: line => out.push(line) }, stderr: { write: line => out.push(line) },
    async openCandidate() { state.opened++; return { client: app, work, bound: { instanceId: INSTANCE, buildId: BUILD }, async recheck() { state.rechecks++; },
      async cleanup() { state.cleaned++; if (over.cleanupFails) throw new Error('process remains'); }, identity: { sourceHead: HEAD, buildId: BUILD, pid: 1 } }; }, ...over.deps };
  const config = { app: path.join(dir, 'Nemo.app'), buildProof: path.join(dir, 'proof.json'), reportDir, ...over.config };
  return { config, deps, state, reportDir, out, row };
}

test('a passing run writes the receipt and transcript, cleans up once, and exits 0', async t => {
  const f = fixture(t);
  const { exit, receipt } = await main(f.config, f.deps);
  assert.equal(exit, 0);
  assert.deepEqual([receipt.verdict, f.state.opened, f.state.cleaned], ['pass', 1, 1]);
  const written = JSON.parse(fs.readFileSync(path.join(f.reportDir, 'installed-client-evidence.json'), 'utf8'));
  assert.equal(written.schema, 'nemo.installed-client-evidence/2');
  assert.ok(written.rows[0].assertions.length >= 4 && written.rows[0].assertions.every(a => a.verdict === 'pass'));
  assert.ok(fs.readFileSync(path.join(f.reportDir, 'transcript.jsonl'), 'utf8').split('\n').filter(Boolean).every(line => JSON.parse(line).requestDigest));
  assert.equal(JSON.parse(f.out[0]).verdict, 'pass');
  assert.equal((await main(f.config, f.deps)).exit, 2, 'a used report directory is never overwritten');
});

test('a failing row exits 1; an unrun row exits 4; both still clean up', async t => {
  const failing = fixture(t, { rows: [{ id: '9001-x', lane: '9001', issue: 1, title: 'x title', tasks: [], async run(ctx) { await K.fresh(ctx); ctx.eq('pixel', 'forced', 1, 2); } }] });
  assert.equal((await main(failing.config, failing.deps)).exit, 1);
  assert.equal(failing.state.cleaned, 1);
  const unrun = fixture(t, { app: createStubApp({ unavailable: true }), rows: [{ id: '9001-y', lane: '9001', issue: 1, title: 'y title', tasks: [], async run(ctx) {
    await K.fresh(ctx); await K.apply(ctx, 'create', P.vectorCreate(P.vectorRecord({ layerUid: 'L1', segments: P.rectSegments(1, 1, 5, 5), fill: [1, 0, 0, 1] }))); } }] });
  const result = await main(unrun.config, unrun.deps);
  assert.deepEqual([result.exit, result.receipt.rows[0].verdict, unrun.state.cleaned], [4, 'unrun', 1]);
});

test('cleanup failure turns a green run into a failure', async t => {
  const f = fixture(t, { over: { cleanupFails: true } });
  const { exit, receipt } = await main(f.config, f.deps);
  assert.deepEqual([exit, receipt.verdict, receipt.cleanup.ok], [1, 'fail', false]);
});

test('a foreign instance behind the candidate stops the run and the receipt says why', async t => {
  const f = fixture(t, { app: createStubApp({ foreignReply: true }) });
  const { exit, receipt } = await main(f.config, f.deps);
  assert.equal(exit, 1);
  assert.match(receipt.rows[0].reason, /foreign/);
  assert.equal(f.state.cleaned, 1);
});

test('a bad request refuses before any app is started: relative paths, used report directory, unknown lane or row, bad ids', async t => {
  const f = fixture(t);
  const refusals = [{ app: 'Nemo.app' }, { app: path.join(os.tmpdir(), 'Nemo') }, { buildProof: '/does/not/exist.json' }, { reportDir: path.join(os.tmpdir(), 'ie-missing-' + process.pid) },
    { lanes: ['9999'] }, { rows: ['nope'] }, { taskId: '../escape' }, { sourceSha: 'abc' }];
  for (const over of refusals) {
    const result = await main({ ...f.config, ...over }, f.deps);
    assert.equal(result.exit, 2, JSON.stringify(over));
    assert.ok(typeof result.refused === 'string' && result.refused.length > 0);
  }
  fs.writeFileSync(path.join(f.reportDir, 'old.json'), '{}');
  assert.equal((await main(f.config, f.deps)).exit, 2);
  assert.equal(f.state.opened, 0, 'nothing was started');
});

test('a candidate that cannot be proven is refused with exit 2 and a refusal receipt, never a pass', async t => {
  const f = fixture(t, { over: { deps: { async openCandidate() { const error = new Error('registered build is not the candidate build'); error[Symbol.for('nemo.desktop.requireCheck')] = true; throw error; } } } });
  const { exit, receipt, refused } = await main(f.config, f.deps);
  assert.deepEqual([exit, receipt.verdict, refused], [2, 'refused', 'registered build is not the candidate build']);
  assert.equal(JSON.parse(fs.readFileSync(path.join(f.reportDir, 'installed-client-evidence.json'), 'utf8')).rows.length, 0);
  assert.equal(refusalReason(new Error('contains /private/path/secret')), 'Error', 'an unmarked error is never quoted');
});

test('command line: lanes and rows parse, unknown flags refuse, exit status is the verdict', async t => {
  assert.deepEqual(parse(['--app', '/a/N.app', '--build-proof', '/p.json', '--report-dir', '/r', '--lane', '2107,2108', '--row', 'a,b', '--task', 't1']),
    { app: '/a/N.app', buildProof: '/p.json', reportDir: '/r', lanes: ['2107', '2108'], rows: ['a', 'b'], taskId: 't1' });
  assert.deepEqual(parse(['--reserve', 'none']).reserve, []);
  assert.deepEqual(parse(['--reserve', 'gpu-reference,x-y']).reserve, ['gpu-reference', 'x-y']);
  assert.deepEqual(parse(['--lane', 'all']).lanes, ['2107', '2108', '2112', '2113']);
  assert.throws(() => parse(['--nope']), /unknown argument/);
  assert.throws(() => parse(['--app']), /needs a value/);
  const sink = { write() {} };
  assert.equal(await cli(['--nope'], { stderr: sink }), 2);
  const f = fixture(t);
  assert.equal(await cli(['--app', f.config.app, '--build-proof', f.config.buildProof, '--report-dir', f.config.reportDir], f.deps), 0);
});

test('running one row of a larger catalog exits 5 (subset-pass) and says so on stdout and in the receipt; the full catalog exits 0', async t => {
  const mk = (f, id) => f.row(id, async ctx => { await K.fresh(ctx); const e = await K.exportPng(ctx, 'blank'); ctx.eq('pixel', 'blank', probe.inkBounds(e.image), null); });
  const first = fixture(t, { rows: [], over: {} });
  const catalog = [mk(first, '9001-one'), mk(first, '9001-two')];
  const subset = fixture(t, { rows: catalog, over: { config: { rows: ['9001-one'] } } });
  const { exit, receipt } = await main(subset.config, subset.deps);
  assert.deepEqual([exit, receipt.verdict, receipt.selection.omitted, receipt.totals.byLane['9001'].catalogRows], [5, 'subset-pass', ['9001-two'], 2]);
  const line = JSON.parse(subset.out[0]);
  assert.deepEqual([line.verdict, line.selection.rows, line.selection.omitted, line.selection.catalogRows], ['subset-pass', 1, 1, { 9001: 2 }]);
  const whole = fixture(t, { rows: catalog });
  assert.deepEqual([(await main(whole.config, whole.deps)).exit], [0]);
});
