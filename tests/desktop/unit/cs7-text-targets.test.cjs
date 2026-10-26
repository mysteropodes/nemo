'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const { sharedWorkflow, targetWorkflow, GROUPS } = require('../cs7-scenarios/text20-shared.cjs');
const phases = require('../cs7-scenarios/text20-phases.json');
const textRow = { issue: 2267, checks: [{ id: 'original-text20' }] };
const targetRow = { issue: 2264, checks: ['text-edit', 'font-bind', 'reopen'].map(id => ({ id })) };
function fixture(failed) {
  let runs = 0, finalized = 0;
  const workflow = { run: async () => {
    runs++;
    let stopped = false;
    const phaseResults = phases.map(p => {
      const status = stopped ? 'unrun' : p.name === failed ? 'fail' : 'pass';
      if (status === 'fail') stopped = true;
      return { phase: p.name, status };
    });
    return { checks: [{ id: 'original-text20', status: failed ? 'fail' : 'unrun' }],
      safeToContinue: !failed, phaseResults, finalize: async () => {
        finalized++; return [{ id: 'original-text20', status: failed ? 'fail' : 'pass' }];
      } };
  } };
  const qualification = { source: 'a'.repeat(40), inputs: 'b'.repeat(64), runtime: { task: 'one-batch' } };
  return { workflow, qualification, counts: () => ({ runs, finalized }) };
}
test('provider and later Text20 rows share one qualified twenty-phase execution and one cleanup finalizer', async () => {
  const f = fixture(), session = {};
  const provider = targetWorkflow(sharedWorkflow(f.workflow, f.qualification));
  const later = sharedWorkflow(f.workflow, structuredClone(f.qualification));
  const a = await provider.run({ session, row: targetRow });
  const b = await later.run({ session, row: textRow });
  assert.deepEqual(f.counts(), { runs: 1, finalized: 0 });
  assert.deepEqual(a.checks.map(c => c.status), ['unrun', 'unrun', 'unrun']);
  assert.equal(b.phaseResults.length, 20);
  assert.deepEqual((await a.finalize()).map(c => c.status), ['pass', 'pass', 'pass']);
  assert.equal((await b.finalize())[0].status, 'pass');
  assert.deepEqual(f.counts(), { runs: 1, finalized: 1 });
});
test('different session cannot reuse the prior lifetime, and changed qualification cannot reuse its result', async () => {
  const f = fixture(), session = {}, a = sharedWorkflow(f.workflow, f.qualification);
  await a.run({ session, row: textRow });
  await assert.rejects(sharedWorkflow(f.workflow, { ...f.qualification, inputs: 'c'.repeat(64) }).run({ session, row: textRow }), /qualification differs/);
  assert.equal(f.counts().runs, 1);
  await sharedWorkflow(f.workflow, f.qualification).run({ session: {}, row: textRow });
  assert.equal(f.counts().runs, 2);
});
test('a rejected operator run is retained and never replayed by the later consumer', async () => {
  let runs = 0;
  const workflow = { run: async () => { runs++; throw Error('operator failure'); } }, session = {};
  for (let n = 0; n < 2; n++) await assert.rejects(sharedWorkflow(workflow, {}).run({ session, row: textRow }), /operator failure/);
  assert.equal(runs, 1);
});
test('a failed font phase remains failed and all later target evidence stays unrun', async () => {
  const f = fixture('font-bind'), session = {};
  const result = await targetWorkflow(sharedWorkflow(f.workflow, f.qualification)).run({ session, row: targetRow });
  assert.equal(result.safeToContinue, false);
  assert.deepEqual(result.checks.map(c => c.status), ['unrun', 'fail', 'unrun']);
  assert.deepEqual((await result.finalize()).map(c => c.status), ['unrun', 'fail', 'unrun']);
});
test('later tool failure cannot erase earned provider phases or the original twenty-phase failure', async () => {
  const f = fixture('locked-placement'), session = {}, shared = sharedWorkflow(f.workflow, f.qualification);
  const result = await targetWorkflow(shared).run({ session, row: targetRow });
  assert.deepEqual(result.checks.map(c => c.status), ['unrun', 'unrun', 'unrun']);
  assert.equal(result.safeToContinue, false);
  assert.deepEqual((await result.finalize()).map(c => c.status), ['pass', 'pass', 'pass']);
  const original = await shared.run({ session, row: textRow });
  assert.equal(original.checks[0].status, 'fail');
  assert.deepEqual(f.counts(), { runs: 1, finalized: 1 });
});
test('missing original phases and reordered provider clauses fail closed', async () => {
  const original = { run: async () => ({ phaseResults: [], safeToContinue: true, finalize: async () => [] }) };
  await assert.rejects(targetWorkflow(original).run({ session: {}, row: targetRow }), /phase evidence missing/);
  await assert.rejects(targetWorkflow(original).run({ session: {}, row: { ...targetRow, checks: [...targetRow.checks].reverse() } }), /clauses changed/);
});
test('terminal verification rejection remains terminal and is not retried', async () => {
  const f = fixture(), originalRun = f.workflow.run;
  let attempts = 0;
  f.workflow.run = async (...args) => ({ ...await originalRun(...args), finalize: async () => { attempts++; throw Error('cleanup evidence refused'); } });
  const shared = sharedWorkflow(f.workflow, f.qualification), session = {};
  const provider = await targetWorkflow(shared).run({ session, row: targetRow });
  const later = await shared.run({ session, row: textRow });
  await assert.rejects(provider.finalize(), /cleanup evidence refused/);
  await assert.rejects(later.finalize(), /cleanup evidence refused/);
  assert.equal(attempts, 1);
});
test('actual batch runner withholds both shared receipts when owner cleanup fails', async () => {
  const { runInstalledRows } = require('../cs7-runner.cjs');
  const f = fixture(), shared = sharedWorkflow(f.workflow, f.qualification);
  const identity = { sourceSha: 'a'.repeat(40), tree: 'b'.repeat(40), driverSha: 'c'.repeat(40),
    executableSha256: 'd'.repeat(64), sessionId: 'batch', pid: 123, dirty: false };
  const rows = [targetRow, textRow].map((row, n) => ({ ...row, id: n ? 'text20' : 'text-targets', scenario: 'case-' + n,
    tasks: [], surfaces: [], dependsOn: [], checks: row.checks.map(c => ({ ...c, clause: c.id, covers: ['positive'] })) }));
  const result = await runInstalledRows({ rows,
    scenarios: { 'case-0': targetWorkflow(shared), 'case-1': shared },
    session: { start: async () => identity, identity: async () => identity,
      cleanup: async () => { throw Error('owner remains alive'); } } });
  assert.equal(result.verdict, 'fail');
  assert.equal(result.installed.cleanup, 'incomplete');
  assert.ok(result.installed.rows.every(r => r.checks.every(c => c.status === 'unrun')));
  assert.deepEqual(f.counts(), { runs: 1, finalized: 0 });
});
test('actual runner records the original late failure before stopping, without executing its later row', async () => {
  const { runInstalledRows } = require('../cs7-runner.cjs');
  const f = fixture('locked-placement'), shared = sharedWorkflow(f.workflow, f.qualification);
  const identity = { sourceSha: 'a'.repeat(40), tree: 'b'.repeat(40), driverSha: 'c'.repeat(40),
    executableSha256: 'd'.repeat(64), sessionId: 'batch', pid: 123, dirty: false };
  const rows = [targetRow, textRow].map((row, n) => ({ ...row, id: n ? 'text20' : 'text-targets', scenario: 'case-' + n,
    tasks: [], surfaces: [], dependsOn: [], checks: row.checks.map(c => ({ ...c, clause: c.id, covers: ['positive'] })) }));
  let laterCalls = 0;
  const result = await runInstalledRows({ rows,
    scenarios: { 'case-0': targetWorkflow(shared), 'case-1': { run: async () => { laterCalls++; throw Error('later input forbidden'); } } },
    session: { start: async () => identity, identity: async () => identity, cleanup: async () => {} } });
  assert.deepEqual(result.installed.rows.map(r => r.status), ['pass', 'fail']);
  assert.equal(result.installed.rows[1].checks[0].status, 'fail');
  assert.equal(result.verdict, 'fail');
  assert.equal(result.installed.operatorInputFailures, 0);
  assert.equal(result.installed.terminalCode, 'scenario-stopped');
  assert.equal(laterCalls, 0);
  assert.deepEqual(f.counts(), { runs: 1, finalized: 1 });
});
test('successful shared workflow credits both rows without scheduling later Text20 input', async () => {
  const { runInstalledRows } = require('../cs7-runner.cjs');
  const f = fixture(), shared = sharedWorkflow(f.workflow, f.qualification);
  const identity = { sourceSha: 'a'.repeat(40), tree: 'b'.repeat(40), driverSha: 'c'.repeat(40),
    executableSha256: 'd'.repeat(64), sessionId: 'batch', pid: 123, dirty: false };
  const rows = [targetRow, textRow].map((row, n) => ({ ...row, id: n ? 'text20' : 'text-targets', scenario: 'case-' + n,
    tasks: [], surfaces: [], dependsOn: [], checks: row.checks.map(c => ({ ...c, clause: c.id, covers: ['positive'] })) }));
  let laterCalls = 0;
  const result = await runInstalledRows({ rows,
    scenarios: { 'case-0': targetWorkflow(shared), 'case-1': { run: async () => { laterCalls++; throw Error('shared workflow replayed'); } } },
    session: { start: async () => identity, identity: async () => identity, cleanup: async () => {} } });
  assert.equal(result.verdict, 'installed-pass');
  assert.deepEqual(result.installed.rows.map(r => r.status), ['pass', 'pass']);
  assert.equal(laterCalls, 0);
  assert.deepEqual(f.counts(), { runs: 1, finalized: 1 });
});
test('declaration phase evidence exactly matches the executed provider mapping', () => {
  const declaration = require('../../../engineering/inventory/parity/installed/package-b.json');
  assert.deepEqual(declaration.rows.find(r => r.id === 'text-targets').phaseEvidence, GROUPS);
});
test('provider credit requires an original cleanup finalizer even after product failure', async () => {
  const f = fixture('locked-placement'), run = f.workflow.run;
  f.workflow.run = async () => { const result = await run(); delete result.finalize; return result; };
  await assert.rejects(targetWorkflow(sharedWorkflow(f.workflow, {})).run({ session: {}, row: targetRow }), /cleanup finalizer required/);
});
