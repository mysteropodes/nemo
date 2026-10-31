'use strict';

// #2348 — the adopted-graph pins live in scripts/nemo/boundaries-*.test.cjs, which `npm test`
// (tests/*.test.cjs) never reached, so pin drift (edge counts, port grants, source-shape
// guards) went unseen on PRs until a manual run. This entry runs that whole suite as one
// child `node --test`, so `npm test`, the JS unit job and `verify` include it.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const DIR = path.join(ROOT, 'scripts/nemo');

test('every scripts/nemo/boundaries-*.test.cjs suite passes (adopted graph and port pins)', () => {
  const files = fs.readdirSync(DIR).filter((f) => /^boundaries-.+\.test\.cjs$/.test(f)).sort()
    .map((f) => path.join('scripts/nemo', f));
  assert.ok(files.length >= 20, `expected the boundaries-*.test.cjs suites, found ${files.length}`);
  // Under `npm test` this file runs with NODE_TEST_CONTEXT set; a nested `node --test` that inherits it runs nothing and
  // exits 0, so the variable is cleared and the nested run must report that it actually executed the suites.
  const env = { ...process.env }; delete env.NODE_TEST_CONTEXT;
  const r = spawnSync(process.execPath, ['--test', ...files], { cwd: ROOT, env, encoding: 'utf8', maxBuffer: 1 << 28 });
  const out = `${r.stdout || ''}${r.stderr || ''}`;
  const ran = Number((out.match(/^\u2139 tests (\d+)$/m) || [])[1] || 0);
  assert.ok(ran >= files.length, `the nested boundaries run executed ${ran} tests; it must run the suites, not exit silently`);
  const failed = [...new Set(out.split('\n').filter((l) => l.startsWith('✖') && !l.startsWith('✖ failing tests')))];
  assert.equal(r.status, 0, `boundaries suites failed:\n${failed.join('\n')}`);
});
