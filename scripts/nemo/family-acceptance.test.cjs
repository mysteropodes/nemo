'use strict';
// CS6 part 2 (#2346) — family acceptance runner, verdicts, result validation and matrix evidence.
// cargo is stubbed: no test here shells out to it.
const assert = require('node:assert/strict');
const test = require('node:test');
const lib = require('./lib/family-acceptance.cjs');
const matrix = require('./lib/parity-matrix.cjs');
const cli = require('./family-acceptance.cjs');

const SHA = 'a'.repeat(40), TREE = 'b'.repeat(40);
const taskIds = new Set(['F09', 'F10', 'I14']);
const check = (id, covers) => ({ id, kind: 'native-test', target: 'application', features: 'test-application', filter: `mod_${id}::`, covers });
const family = { schema: lib.FAMILY_SCHEMA, family: 'demo', tasks: ['F09', 'F10'],
  checks: [check('a', ['positive', 'oracle', 'undo-redo', 'save-reopen']), check('b', ['pixels'])] };
const summary = (p, f = 0, i = 0) => `running tests\ntest result: ${f ? 'FAILED' : 'ok'}. ${p} passed; ${f} failed; ${i} ignored; 0 measured; 100 filtered out; finished in 0.1s\n`;

// exec stub: git answers from `git`, cargo answers per filter from `cargo`.
function stub({ dirty = false, cargo = {} } = {}) {
  const calls = [];
  const exec = (cmd, args) => {
    calls.push([cmd, ...args]);
    if (cmd === 'git') {
      if (args[0] === 'status') return { status: 0, stdout: dirty ? ' M x\n' : '' };
      return { status: 0, stdout: args[1] === 'HEAD^{tree}' ? TREE : SHA };
    }
    const filter = args[args.indexOf('--test') + 2];
    return cargo[filter] || { status: 0, stdout: summary(3) };
  };
  return { exec, calls };
}

test('test result lines are summed across binaries into passed, failed and ignored', () => {
  const text = summary(4, 0, 1) + summary(2, 1);
  assert.deepEqual(lib.parseTestResults(text), { summaries: 2, passed: 6, failed: 1, ignored: 1 });
  assert.deepEqual(lib.parseTestResults('no summary'), { summaries: 0, passed: 0, failed: 0, ignored: 0 });
});

test('the cargo command is locked, featureless by default and names the target and filter', () => {
  const args = lib.cargoArgs(check('a', ['positive']));
  assert.deepEqual(args.slice(0, 7), ['test', '--locked', '--manifest-path', 'native-engine/Cargo.toml', '--no-default-features', '--features', 'test-application']);
  assert.ok(args.join(' ').includes('--test application mod_a::'));
});

test('a filter that matches nothing is a failure, as is a nonzero exit or a missing summary', () => {
  const zero = lib.runCheck(check('a', ['positive']), () => ({ status: 0, stdout: summary(0) }));
  assert.equal(zero.ok, false);
  assert.equal(lib.runCheck(check('a', ['positive']), () => ({ status: 101, stdout: summary(3) })).ok, false);
  assert.equal(lib.runCheck(check('a', ['positive']), () => ({ status: 0, stdout: 'compiled' })).ok, false);
  assert.equal(lib.runCheck(check('a', ['positive']), () => ({ status: 0, stdout: summary(3, 1) })).ok, false);
  assert.equal(lib.runCheck(check('a', ['positive']), () => ({ status: 0, stdout: summary(3) })).ok, true);
});

test('verdict: source-pass needs every check green and all five kinds; a gap is incomplete; any failure is fail', () => {
  const full = { ...family, checks: [check('a', ['positive', 'oracle', 'undo-redo', 'save-reopen']), check('b', ['pixels'])] };
  const { result } = lib.runFamily(full, stub());
  assert.equal(result.verdict, 'source-pass');
  assert.deepEqual(result.kindsMissing, []);
  assert.equal(result.sourceSha, SHA);
  assert.equal(result.tree, TREE);

  const partial = { ...family, checks: [family.checks[0]] };
  const inc = lib.runFamily(partial, stub()).result;
  assert.equal(inc.verdict, 'incomplete');
  assert.deepEqual(inc.kindsMissing, ['pixels']);

  // A failing check removes the kinds it covered and fails the family.
  const bad = lib.runFamily(full, stub({ cargo: { 'mod_b::': { status: 0, stdout: summary(0) } } })).result;
  assert.equal(bad.verdict, 'fail');
  assert.deepEqual(bad.kindsMissing, ['pixels']);
});

test('--write is refused on a dirty tree before any cargo job runs', () => {
  const dirty = stub({ dirty: true });
  const errors = [];
  const original = console.error;
  console.error = (m) => errors.push(m);
  try { assert.equal(cli.main(['components', '--write'], dirty.exec), 2); } finally { console.error = original; }
  assert.ok(errors.some((e) => /dirty tree/.test(e)));
  assert.ok(!dirty.calls.some((c) => c[0] === 'cargo'));
});

test('family declarations are validated: unknown task, bad covers, duplicate id and unsafe filter', () => {
  assert.deepEqual(lib.validateFamily(family, taskIds), []);
  const bad = { ...family, tasks: ['Q99'], checks: [check('a', ['nonsense']), { ...check('a', ['positive']), filter: 'x; rm' }] };
  const problems = lib.validateFamily(bad, taskIds).join('|');
  for (const p of [/unknown task Q99/, /covers must be from/, /duplicate/, /invalid filter/]) assert.match(problems, p);
});

function goodResult() {
  const { result } = lib.runFamily(family, stub());
  return result;
}

test('a good result validates and a stale HEAD is irrelevant (it is a source-level record)', () => {
  assert.deepEqual(lib.validateResult(goodResult(), family, taskIds), []);
});

test('negative controls: malformed result, tampered verdict, task drift and unknown task are rejected', () => {
  const r = goodResult();
  assert.match(lib.validateResult({ ...r, schema: 'x' }, family, taskIds)[0], /schema/);
  assert.match(lib.validateResult({ ...r, sourceSha: 'abc' }, family, taskIds).join('|'), /sourceSha/);
  assert.match(lib.validateResult({ ...r, verdict: 'incomplete' }, family, taskIds).join('|'), /verdict disagrees/);
  assert.match(lib.validateResult({ ...r, tasks: ['F09'] }, family, taskIds).join('|'), /tasks differ/);
  const ghost = { ...family, tasks: ['F09', 'Z01'] };
  assert.match(lib.validateResult({ ...r, tasks: ghost.tasks }, ghost, taskIds).join('|'), /unknown task Z01/);
  assert.match(lib.validateResult(r, undefined, taskIds)[0], /no families/);
  const dropped = { ...r, checks: r.checks.slice(1) };
  assert.match(lib.validateResult(dropped, family, taskIds).join('|'), /checks do not match/);
});

test('the matrix annotates only surfaces whose tasks intersect a family, additively', () => {
  const results = [{ ...goodResult() }];
  const tasks = [{ task: 'F09', outcome: 'C13', title: 'f' }, { task: 'I14', outcome: 'B02', title: 'i' }];
  const baseline = { rows: [{ id: 'dom:#a', kind: 'dom' }, { id: 'dom:#b', kind: 'dom' }] };
  const mapping = { surfaces: { 'dom:#a': { tasks: ['F09'] }, 'dom:#b': { tasks: ['I14'] } } };
  const owners = { snapshotAt: 'x', owners: { F09: [{ issue: 1, state: 'open' }], I14: [{ issue: 2, state: 'open' }] } };
  const input = { config: { baseline: 'b', baselineStatus: 's' }, tasks, baseline, mapping, owners, ownership: { tasks: {} } };
  const without = matrix.buildMatrix(input);
  const withEv = matrix.buildMatrix({ ...input, results });
  assert.deepEqual(withEv.surfaces[0].evidence, [{ family: 'demo', verdict: 'source-pass', sourceSha: SHA }]);
  assert.equal('evidence' in withEv.surfaces[1], false);
  assert.deepEqual(withEv.counts.evidence, { demo: { 'source-pass': 1 } });
  assert.equal(withEv.surfaces[0].status, without.surfaces[0].status);
  assert.deepEqual(withEv.counts.surfaceStatus, without.counts.surfaceStatus);
  assert.match(matrix.renderMarkdown(withEv), /demo \(source-pass 1\)/);
});
