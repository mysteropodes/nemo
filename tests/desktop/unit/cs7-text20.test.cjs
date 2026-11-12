'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { createWorkflow, prepare } = require('../cs7-scenarios/text20.cjs');
const phases = require('../cs7-scenarios/text20-phases.json');
function fixture(t, failAt) {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-text20-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const identity = { sourceSha: 'a'.repeat(40), sessionId: 'one-batch', pid: 321 }, calls = [];
  const c = { sourceHead: identity.sourceSha, taskId: identity.sessionId, reportRoot: path.join(directory, 'text20'), deadlineMs: 10000 };
  const original = { validatePhase: phase => { if (phase.name === failAt) throw Error('original oracle refused'); return { phase: phase.name }; } };
  const workflow = createWorkflow({ packet: { directory }, original, c, modules: {}, inputs: { limits: ['Independent review remains required.'] } });
  const session = {
    runtimeContext: async () => ({ identity, snapshot: { app: { pid: 321 } } }),
    requestEvidence: async request => { calls.push(request.phase); return { phase: request.phase }; },
  };
  return { workflow, session, calls, c, row: { id: 'text20', issue: 2267, checks: [{ id: 'original-text20' }] } };
}
test('Text20 uses all original phases in one existing lifetime and awaits batch cleanup', async t => {
  const f = fixture(t); const result = await f.workflow.run(f);
  assert.deepEqual(f.calls, phases.map(p => p.name));
  assert.equal(result.checks[0].status, 'unrun');
  assert.equal(typeof result.finalize, 'function');
  const before = JSON.parse(fs.readFileSync(path.join(f.c.reportRoot, 'receipt.json')));
  assert.equal(before.status, 'operator-complete'); assert.equal(before.installedAcceptance, 'not-established');
  assert.equal(before.phases.filter(p => p.status === 'pass').length, 20);
  assert.equal(before.cleanup, undefined);
  assert.deepEqual(await result.finalize(), [{ id: 'original-text20', status: 'pass' }]);
  await assert.rejects(f.workflow.run(f), /cannot restart/);
});
test('first original phase failure stops the workflow and preserves the entire unrun tail', async t => {
  const f = fixture(t, 'point-edit'); const result = await f.workflow.run(f);
  assert.equal(result.safeToContinue, false); assert.equal(result.checks[0].status, 'fail');
  assert.equal(typeof result.finalize, 'function');
  assert.deepEqual(f.calls, ['open', 'blank-cancel', 'point-create', 'point-edit']);
  const report = JSON.parse(fs.readFileSync(path.join(f.c.reportRoot, 'receipt.json')));
  assert.equal(report.actions.length, 3);
  assert.deepEqual(report.phases.map(p => p.status), ['pass', 'pass', 'pass', 'fail', ...Array(16).fill('unrun')]);
  assert.equal(report.cleanup, undefined);
  assert.deepEqual(await result.finalize(), [{ id: 'original-text20', status: 'fail' }]);
  const terminal = JSON.parse(fs.readFileSync(path.join(f.c.reportRoot, 'receipt.json')));
  assert.equal(terminal.cleanup, 'complete');
  assert.equal(terminal.status, 'partial');
  assert.deepEqual(terminal.phases, report.phases);
  assert.deepEqual(terminal.failure, report.failure);
});
test('foreign lifetime refuses before any Text20 operator request', async t => {
  const f = fixture(t); f.session.runtimeContext = async () => ({ identity: { sourceSha: 'b'.repeat(40), sessionId: 'one-batch' } });
  await assert.rejects(f.workflow.run(f), /owner differs/); assert.equal(f.calls.length, 0);
});
test('an unreviewed private driver is refused before it can execute', t => {
  const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-text20-packet-')));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const marker = path.join(directory, 'executed');
  fs.writeFileSync(path.join(directory, 'driver.cjs'), 'require("fs").writeFileSync(' + JSON.stringify(marker) + ', "bad");');
  assert.throws(() => prepare({ config: { text20: { directory } }, root: directory }), /changed/);
  assert.equal(fs.existsSync(marker), false);
});
