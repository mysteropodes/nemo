'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { readyRows, runInstalledRows, checkTaskCoverage, taskSurfaceIndex, DECLARATION_SCHEMA, OperatorInputError } = require('../cs7-runner.cjs');
const identity = { sourceSha: 'a'.repeat(40), tree: 'b'.repeat(40), driverSha: 'c'.repeat(40),
  executableSha256: 'd'.repeat(64), sessionId: 'one-lifetime', pid: 123, dirty: false };
function setup(count = 5) {
  const calls = [], rows = Array.from({ length: count }, (_, i) => ({ id: 'row-' + i, issue: 1000 + i,
    status: 'Ready', tasks: ['F03'], surfaces: ['opacity'], dependsOn: [], scenario: 'case-' + i,
    evidence: 'Source complete: https://github.com/mysteropodes/nemo/pull/100 at ' + 'a'.repeat(40),
    checks: [{ id: 'workflow', clause: 'Original installed workflow', covers: ['positive', 'undo-redo', 'save-reopen'] }] }));
  const scenarios = Object.fromEntries(rows.map(row => [row.scenario, { run: async () => {
    calls.push(row.id); return { checks: [{ id: 'workflow', status: 'pass' }], safeToContinue: true };
  } }]));
  const session = { start: async () => { calls.push('start'); return identity; },
    identity: async () => ({ ...identity }), cleanup: async () => { calls.push('cleanup'); } };
  return { rows, scenarios, session, calls };
}
function ready(state) {
  return readyRows([{ schema: DECLARATION_SCHEMA, rows: state.rows }], {
    taskIds: new Set(['F03']), surfaceIds: new Set(['opacity']), taskSurfaces: new Map([['F03', new Set(['opacity'])]]), scenarios: state.scenarios,
  });
}
test('five declared Ready rows use exactly one candidate lifetime and cleanup', async () => {
  const state = setup(); state.rows = ready(state);
  const result = await runInstalledRows(state);
  assert.deepEqual(state.calls, ['start', 'row-0', 'row-1', 'row-2', 'row-3', 'row-4', 'cleanup']);
  assert.equal(result.verdict, 'installed-pass');
  assert.equal(result.sourceSha, identity.sourceSha);
  assert.equal(result.installed.operatorInputFailures, 0);
  assert.equal(result.checks.length, 5);
  assert.ok(result.checks.every(c => c.passed === 1 && c.failed === 0 && c.ignored === 0));
  assert.equal(result.format, 'nemo.parity-family-result/1');
  assert.notEqual(result.schema, result.format); // Cannot silently become headless evidence.
  assert.deepEqual(result.tasks, ['F03']);
  assert.deepEqual(result.expected.tasks, ['F03']);
});
test('Ready is never inferred from closed issues, empty input or missing scripts', () => {
  for (const mutate of [s => { s.rows = []; }, s => { s.rows[0].status = 'issues-closed'; },
    s => { s.rows[0].scenario = 'missing'; }, s => { s.rows[0].surfaces = ['missing']; },
    s => { s.rows.push(s.rows[0]); }, s => { s.rows[0].checks.push(s.rows[0].checks[0]); }]) {
    const state = setup(); mutate(state); assert.throws(() => ready(state));
  }
});
test('dependencies are ordered and cycles or Blocked dependencies refuse before launch', () => {
  const state = setup(2); state.rows[0].dependsOn = ['row-1'];
  assert.deepEqual(ready(state).map(r => r.id), ['row-1', 'row-0']);
  state.rows[1].dependsOn = ['row-0']; assert.throws(() => ready(state), /cycle/);
  state.rows[1].dependsOn = []; state.rows[1].status = 'Blocked'; state.rows[1].reason = 'Source unavailable';
  assert.throws(() => ready(state), /Blocked/);
});
test('operator failure aborts without replay and retains every unrun obligation', async () => {
  const state = setup(); let attempts = 0;
  state.scenarios['case-1'].run = async () => { attempts++; throw new OperatorInputError('private path and token'); };
  const result = await runInstalledRows(state);
  assert.equal(attempts, 1);
  assert.deepEqual(state.calls, ['start', 'row-0', 'cleanup']);
  assert.equal(result.installed.operatorInputFailures, 1);
  assert.equal(result.installed.rows.length, 5);
  assert.equal(result.installed.rows[0].status, 'pass');
  assert.deepEqual(result.installed.rows[0].tasks, ['F03']);
  assert.deepEqual(result.installed.rows[1].tasks, []);
  assert.deepEqual(result.installed.rows[1].expected.tasks, ['F03']);
  assert.deepEqual(result.installed.rows[4].partialTasks, {});
  assert.deepEqual(result.installed.rows[4].expected.tasks, ['F03']);
  assert.deepEqual(result.tasks, ['F03']);
  assert.equal(result.installed.rows[4].checks[0].status, 'unrun');
  assert.equal(result.verdict, 'fail');
  assert.ok(!JSON.stringify(result).includes('private path and token'));
});
test('product failure blocks its dependents but permits explicitly safe independent rows', async () => {
  const state = setup(3); state.rows[1].dependsOn = ['row-0'];
  state.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'fail' }], safeToContinue: true });
  const result = await runInstalledRows(state);
  assert.deepEqual(state.calls, ['start', 'row-2', 'cleanup']);
  assert.deepEqual(result.installed.rows.map(r => r.status), ['fail', 'dependency-blocked', 'pass']);
  assert.equal(result.installed.operatorInputFailures, 0);
});
test('candidate drift before or after any row prevents a passing receipt', async () => {
  for (const changeAt of [1,2,5,10]) {
    const state = setup(); let count = 0;
    state.session.identity = async () => (++count === changeAt ? { ...identity, pid: 456 } : identity);
    const result = await runInstalledRows(state);
    assert.equal(result.verdict, 'fail'); assert.equal(result.installed.operatorInputFailures, 1);
    assert.equal(state.calls.at(-1), 'cleanup');
  }
});
test('partial startup, invalid assertions and failed cleanup cannot pass', async () => {
  for (const mutate of [s => { s.session.start = async () => { throw Error('partial start'); }; },
    s => { s.session.cleanup = async () => { throw Error('still alive'); }; },
    s => { s.scenarios['case-0'].run = async () => ({ checks: [], safeToContinue: true }); },
    s => { s.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'pass' }, { id: 'workflow', status: 'pass' }], safeToContinue: true }); }]) {
    const state = setup(); mutate(state); const result = await runInstalledRows(state);
    assert.equal(result.verdict, 'fail');
    if (state.session.cleanup !== undefined && state.calls.includes('cleanup')) {
      if (result.installed.cleanup === 'incomplete') assert.deepEqual(result.tasks, []);
    }
  }
});
test('unsafe product continuation and unrun clauses remain incomplete', async () => {
  const state = setup();
  state.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'unrun' }], safeToContinue: false });
  const result = await runInstalledRows(state);
  assert.equal(result.verdict, 'incomplete');
  assert.deepEqual(state.calls, ['start', 'cleanup']);
  assert.equal(result.installed.terminalCode, 'scenario-stopped');
});
test('journal failure still cleans up and cannot leave a passing receipt', async () => {
  const state = setup(); let writes = 0;
  const result = await runInstalledRows({ ...state, save: async () => { if (++writes === 1) throw Error('disk full'); } });
  assert.equal(result.verdict, 'fail'); assert.equal(state.calls.at(-1), 'cleanup');
});
test('Ready needs cited source evidence and clauses; unknown-task Blocked rows retain their reason', () => {
  for (const change of [s => { delete s.rows[0].evidence; }, s => { s.rows[0].evidence = 'merged'; },
    s => { delete s.rows[0].checks[0].clause; }, s => { s.rows[0].tasks = []; }]) {
    const state = setup(); change(state); assert.throws(() => ready(state));
  }
  const state = setup(2); state.rows[1].status = 'Blocked'; state.rows[1].tasks = [];
  state.rows[1].reason = 'Original task citation not yet supplied';
  assert.deepEqual(ready(state).map(r => r.id), ['row-0']);
  delete state.rows[1].reason; assert.throws(() => ready(state), /reason/);
});
test('installed collection is verified after owner cleanup, never from a provisional copy', async () => {
  const state = setup(2), journals = [];
  state.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'unrun' }],
    safeToContinue: true, finalize: async () => {
      assert.equal(state.calls.at(-1), 'cleanup'); state.calls.push('verify-retained-copy');
      return [{ id: 'workflow', status: 'pass' }];
    } });
  const result = await runInstalledRows({ ...state, save: async r => journals.push(structuredClone(r)) });
  assert.equal(journals[0].rows[0].status, 'pending-verification');
  assert.equal(journals[0].rows[0].checks[0].status, 'unrun');
  assert.deepEqual(journals[0].rows[0].tasks, []);
  assert.deepEqual(journals[0].rows[0].expected.tasks, ['F03']);
  assert.deepEqual(state.calls, ['start', 'row-1', 'cleanup', 'verify-retained-copy']);
  assert.equal(result.installed.rows[0].checks[0].clause, 'Original installed workflow');
  assert.equal(result.verdict, 'installed-pass');
  assert.deepEqual(result.tasks, ['F03']);
});
test('failed cleanup prevents final verification and a broken finalizer is not an input failure', async () => {
  for (const badCleanup of [true, false]) {
    const state = setup(1); let finalized = 0;
    state.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'unrun' }],
      safeToContinue: true, finalize: async () => { finalized++; throw Error('private artifact path'); } });
    if (badCleanup) state.session.cleanup = async () => { throw Error('still alive'); };
    const result = await runInstalledRows(state);
    assert.equal(finalized, badCleanup ? 0 : 1);
    assert.equal(result.verdict, 'fail');
    assert.equal(result.installed.operatorInputFailures, 0);
    assert.equal(result.installed.terminalCode, badCleanup ? 'cleanup-incomplete' : 'evidence-finalization-failed');
    assert.equal(result.installed.rows[0].checks[0].status, 'unrun');
    assert.deepEqual(result.installed.rows[0].tasks, []);
    assert.deepEqual(result.tasks, []);
    assert.deepEqual(result.expected.tasks, ['F03']);
    assert.ok(!JSON.stringify(result).includes('private artifact path'));
  }
});
test('shared evidence cannot overwrite earlier rows, name undeclared rows or repeat a row', async () => {
  for (const ids of [['row-0'], ['missing'], ['row-1', 'row-1']]) {
    const state = setup(3);
    const observed = { checks: [{ id: 'workflow', status: 'fail' }], safeToContinue: false };
    state.scenarios['case-0'].run = async () => ({ ...observed,
      sharedResults: ids.map(rowId => ({ rowId, result: observed })) });
    const result = await runInstalledRows(state);
    assert.equal(result.installed.terminalCode, 'installed-precondition-failed');
    assert.equal(result.verdict, 'fail');
    assert.deepEqual(state.calls, ['start', 'cleanup']);
  }
});
test('an unsafe shared result stops subsequent input even if its consumer permits continuation', async () => {
  const state = setup(3);
  state.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'unrun' }], safeToContinue: true,
    sharedResults: [{ rowId: 'row-1', result: { checks: [{ id: 'workflow', status: 'fail' }], safeToContinue: false } }] });
  const result = await runInstalledRows(state);
  assert.equal(result.installed.rows[1].status, 'fail');
  assert.equal(result.installed.rows[2].status, 'unrun');
  assert.deepEqual(state.calls, ['start', 'cleanup']);
  assert.equal(result.installed.operatorInputFailures, 0);
  assert.equal(result.installed.terminalCode, 'scenario-stopped');
});
test('finalization cannot erase an observed failure or omit a declared clause', async () => {
  for (const finalChecks of [[], [{ id: 'workflow', status: 'pass' }]]) {
    const state = setup(1);
    state.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'fail' }],
      safeToContinue: true, finalize: async () => finalChecks });
    const result = await runInstalledRows(state);
    assert.equal(result.verdict, 'fail');
    assert.equal(result.installed.rows[0].checks[0].status, 'fail');
    assert.equal(result.installed.terminalCode, 'evidence-finalization-failed');
  }
});
test('an earlier safe stop cannot hide a later evidence finalization failure', async () => {
  const state = setup(1);
  state.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'unrun' }],
    safeToContinue: false, finalize: async () => { throw Error('copy not retained'); } });
  const result = await runInstalledRows(state);
  assert.equal(result.verdict, 'fail');
  assert.equal(result.installed.terminalCode, 'scenario-stopped');
  assert.equal(result.installed.evidenceFailures, 1);
  assert.equal(result.installed.operatorInputFailures, 0);
});
test('the original Text20 phase artifact is conserved byte for byte', () => {
  const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
  const bytes = fs.readFileSync(path.join(__dirname, '../cs7-scenarios/text20-phases.json'));
  assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),
    'ba56fcf1db807ed2bd10e79dfd19242d43f3827fcc3aea1acb9ca596710e201d');
  assert.equal(JSON.parse(bytes).length, 20);
});

// ---- task coverage: a receipt must not read as full acceptance of a task whose surfaces it does not all exercise ----
const index = surfaces => new Map(Object.entries(surfaces).map(([task, ids]) => [task, new Set(ids)]));
const declared = (extra, count = 1) => { const state = setup(count); Object.assign(state.rows[0], extra); return state; };
function readyWith(state, taskSurfaces) {
  return readyRows([{ schema: DECLARATION_SCHEMA, rows: state.rows }], { taskIds: new Set(['F03', 'F04']), surfaceIds: new Set(['opacity', 'blur', 'mask']), taskSurfaces, scenarios: state.scenarios });
}

test('a task listed in tasks must have every one of its matrix surfaces in the row', () => {
  const surfaces = index({ F03: ['opacity', 'blur'], F04: [] });
  assert.throws(() => readyWith(declared({ tasks: ['F03'], surfaces: ['opacity'] }), surfaces), /task F03 without all its matrix surfaces \(1 missing\)/);
  assert.equal(readyWith(declared({ tasks: ['F03'], surfaces: ['opacity', 'blur'] }), surfaces).length, 1);
  assert.equal(readyWith(declared({ tasks: ['F04'], surfaces: ['opacity'] }), surfaces).length, 1);   // a task with no matrix surface
  assert.throws(() => readyWith(declared({}), undefined), /task coverage index required/);
});

test('partialTasks must state exactly the matrix surfaces the row does not exercise, and never repeat a task', () => {
  const surfaces = index({ F03: ['opacity', 'blur', 'mask'], F04: ['opacity'] });
  const ok = declared({ tasks: ['F04'], partialTasks: { F03: ['blur', 'mask'] }, surfaces: ['opacity'] });
  assert.equal(readyWith(ok, surfaces).length, 1);
  for (const [bad, pattern] of [
    [{ F03: ['blur'] }, /differs from the matrix surfaces/],                    // under-reports what is missing
    [{ F03: ['blur', 'mask', 'opacity'] }, /differs from the matrix surfaces/], // claims a covered surface is missing
    [{ F03: [] }, /must list its missing surfaces/],
    [{ F03: ['blur', 'blur', 'mask'] }, /must list its missing surfaces/],
    [{ F04: ['x'] }, /unknown or also in tasks/],
    [{ F99: ['x'] }, /unknown or also in tasks/],
    [['F03'], /partialTasks must be an object|must list|differs|unknown/],
    [null, /partialTasks must be an object/],
  ]) {
    assert.throws(() => readyWith(declared({ tasks: ['F04'], partialTasks: bad, surfaces: ['opacity'] }), surfaces), pattern, JSON.stringify(bad));
  }
  // A row whose every task is partial is still a valid Ready row; one with neither is not.
  assert.equal(readyWith(declared({ tasks: [], partialTasks: { F03: ['blur', 'mask'] }, surfaces: ['opacity'] }), surfaces).length, 1);
  assert.throws(() => readyWith(declared({ tasks: [], surfaces: ['opacity'] }), surfaces));
});

test('the receipt publishes tasks and partialTasks separately, per run and per row', async () => {
  const surfaces = index({ F03: ['opacity', 'blur', 'mask'], F04: ['opacity'] });
  const state = declared({ tasks: ['F04'], partialTasks: { F03: ['blur', 'mask'] }, surfaces: ['opacity'] }, 2);
  Object.assign(state.rows[1], { tasks: [], partialTasks: { F03: ['mask', 'blur'] }, surfaces: ['opacity'] });
  state.rows = readyWith(state, surfaces);
  const saved = [];
  const result = await runInstalledRows({ ...state, save: async r => saved.push(r) });
  assert.equal(result.verdict, 'installed-pass');
  assert.deepEqual(result.tasks, ['F04']);
  assert.deepEqual(Object.keys(result.partialTasks), ['F03']);
  assert.deepEqual(result.partialTasks.F03.slice().sort(), ['blur', 'mask']);
  assert.deepEqual(result.installed.rows[0].partialTasks, { F03: ['blur', 'mask'] });
  assert.deepEqual(result.installed.rows[0].tasks, ['F04']);
  assert.ok(!result.tasks.includes('F03'));
  // Two rows that each miss different surfaces of the same task publish only what neither exercised.
  const two = declared({ tasks: ['F04'], partialTasks: { F03: ['blur', 'mask'] }, surfaces: ['opacity'] }, 2);
  Object.assign(two.rows[1], { tasks: [], partialTasks: { F03: ['opacity', 'blur'] }, surfaces: ['mask'] });
  two.rows = readyWith(two, surfaces);
  assert.deepEqual((await runInstalledRows(two)).partialTasks, { F03: ['blur'] });
});

test('a task fully covered by one run row is never also reported partial', async () => {
  const surfaces = index({ F03: ['opacity', 'blur'] });
  const state = declared({ tasks: ['F03'], surfaces: ['opacity', 'blur'] }, 2);
  Object.assign(state.rows[1], { tasks: [], partialTasks: { F03: ['blur'] }, surfaces: ['opacity'] });
  state.rows = readyWith(state, surfaces);
  const result = await runInstalledRows(state);
  assert.deepEqual(result.tasks, ['F03']);
  assert.deepEqual(result.partialTasks, {});
});

test('a pass of fewer rows than every Ready row has its own verdict; all-ready keeps installed-pass', async () => {
  for (const [selection, verdict] of [
    [{ omittedReady: ['row-9'] }, 'installed-subset-pass'],
    [{ omittedReady: [] }, 'installed-pass'],
    [undefined, 'installed-pass'],
  ]) {
    const state = setup(2); state.rows = ready(state);
    const result = await runInstalledRows({ ...state, ...(selection ? { selection } : {}) });
    assert.equal(result.verdict, verdict);
    assert.equal(result.selection?.scope, selection ? (selection.omittedReady.length ? 'subset' : 'all-ready') : undefined);
    assert.deepEqual(result.tasks, ['F03']);
  }
  // A failure is a failure whatever the selection.
  const state = setup(1); state.rows = ready(state);
  state.scenarios['case-0'] = { run: async () => ({ checks: [{ id: 'workflow', status: 'fail' }], safeToContinue: false }) };
  assert.equal((await runInstalledRows({ ...state, selection: { omittedReady: ['row-9'] } })).verdict, 'fail');
});

test('declared coverage is expected scope until a row passes safely and cleanup completes', async () => {
  const state = setup(3);
  state.rows[0].partialTasks = { F04: ['blur'] };
  state.rows[1].dependsOn = ['row-0'];
  state.scenarios['case-0'].run = async () => ({ checks: [{ id: 'workflow', status: 'pass' }], safeToContinue: false });
  state.scenarios['case-2'].run = async () => ({ checks: [{ id: 'workflow', status: 'fail' }], safeToContinue: true });
  const result = await runInstalledRows({ ...state, selection: { omittedReady: ['row-9'] } });
  assert.deepEqual(result.expected.tasks, ['F03']);
  assert.deepEqual(result.expected.partialTasks.F04, ['blur']);
  assert.deepEqual(result.tasks, []);
  assert.deepEqual(result.partialTasks, {});
  assert.deepEqual(result.surfaces, []);
  assert.deepEqual(result.installed.rows.map(r => r.tasks), [[], [], []]);
  assert.deepEqual(result.installed.rows.map(r => r.expected.tasks), [['F03'], ['F03'], ['F03']]);
  assert.deepEqual(result.installed.rows[0].expected.partialTasks, { F04: ['blur'] });
});

test('checkTaskCoverage and taskSurfaceIndex read the real matrix shape', () => {
  const idx = taskSurfaceIndex({ tasks: [{ task: 'A' }, { task: 'B' }], surfaces: [{ id: 's1', tasks: ['A', 'B'] }, { id: 's2', tasks: ['A'] }, { id: 's3', tasks: ['Z'] }] });
  assert.deepEqual([...idx.get('A')].sort(), ['s1', 's2']);
  assert.deepEqual([...idx.get('B')], ['s1']);
  assert.equal(idx.has('Z'), false);
  assert.doesNotThrow(() => checkTaskCoverage({ tasks: ['B'], surfaces: ['s1'], partialTasks: { A: ['s2'] } }, idx));
  assert.throws(() => checkTaskCoverage({ tasks: ['A'], surfaces: ['s1'] }, idx), /without all its matrix surfaces/);
});
