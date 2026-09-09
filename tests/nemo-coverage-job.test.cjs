'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { evaluateCoverageSummary } = require('../scripts/nemo/lib/jobs.cjs');

const root = '/repo';
const entry = (linesPct, branchesPct) => ({ lines: { pct: linesPct }, branches: { pct: branchesPct } });

test('evaluateCoverageSummary passes when every included file meets the threshold', () => {
  const config = { include: ['a.js', 'b.js'], lines: 90, branches: 80, exceptions: [] };
  const summary = { '/repo/a.js': entry(100, 100), '/repo/b.js': entry(90, 80) };
  const result = evaluateCoverageSummary(summary, config, root);
  assert.equal(result.ok, true);
  assert.deepEqual(result.violations, []);
  assert.equal(result.perFile.length, 2);
});

test('evaluateCoverageSummary fails a file below the lines threshold', () => {
  const config = { include: ['a.js'], lines: 90, branches: 80, exceptions: [] };
  const summary = { '/repo/a.js': entry(89.9, 100) };
  const result = evaluateCoverageSummary(summary, config, root);
  assert.equal(result.ok, false);
  assert.equal(result.violations.length, 1);
  assert.match(result.violations[0], /a\.js: lines 89\.9% < 90%/);
});

test('evaluateCoverageSummary fails a file below the branches threshold', () => {
  const config = { include: ['a.js'], lines: 90, branches: 80, exceptions: [] };
  const summary = { '/repo/a.js': entry(100, 79) };
  const result = evaluateCoverageSummary(summary, config, root);
  assert.equal(result.ok, false);
  assert.match(result.violations[0], /a\.js: branches 79% < 80%/);
});

test('a declared exception is reported but never counted as a violation, however low its coverage', () => {
  const config = {
    include: ['a.js', 'negative-control.js'], lines: 90, branches: 80,
    exceptions: [{ path: 'negative-control.js', reason: 'deliberately unimported' }],
  };
  const summary = { '/repo/a.js': entry(95, 90), '/repo/negative-control.js': entry(0, 0) };
  const result = evaluateCoverageSummary(summary, config, root);
  assert.equal(result.ok, true);
  assert.deepEqual(result.violations, []);
  const exceptionEntry = result.perFile.find((f) => f.path === 'negative-control.js');
  assert.equal(exceptionEntry.exception, 'deliberately unimported');
  assert.equal(exceptionEntry.linesPct, 0);
});

test('an included file missing from the coverage summary is a violation unless it is a declared exception', () => {
  const config = { include: ['missing.js', 'excepted-missing.js'], lines: 90, branches: 80,
    exceptions: [{ path: 'excepted-missing.js', reason: 'never instrumented, reviewed' }] };
  const summary = {};
  const result = evaluateCoverageSummary(summary, config, root);
  assert.equal(result.ok, false);
  assert.equal(result.violations.length, 1);
  assert.match(result.violations[0], /missing\.js: missing from coverage summary/);
  const exceptionEntry = result.perFile.find((f) => f.path === 'excepted-missing.js');
  assert.equal(exceptionEntry.exception, 'never instrumented, reviewed');
  assert.equal(exceptionEntry.linesPct, null);
});

test('legacy modules outside .c8rc.json\'s include list are never evaluated', () => {
  const config = { include: ['a.js'], lines: 90, branches: 80, exceptions: [] };
  const summary = { '/repo/a.js': entry(100, 100), '/repo/legacy/whatever.js': entry(0, 0) };
  const result = evaluateCoverageSummary(summary, config, root);
  assert.equal(result.ok, true);
  assert.equal(result.perFile.length, 1);
});
