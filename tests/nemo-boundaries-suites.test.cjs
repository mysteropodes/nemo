'use strict';

// #2348 — the adopted-graph pins live in scripts/nemo/boundaries-*.test.cjs, which `npm test`
// (tests/*.test.cjs) never reached, so pin drift (edge counts, port grants, source-shape
// guards) went unseen on PRs until a manual run. This entry runs that whole suite as one
// child `node --test`, so `npm test`, the JS unit job and `verify` include it.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const DIR = path.join(ROOT, 'scripts/nemo');

test('every scripts/nemo/boundaries-*.test.cjs suite passes (adopted graph and port pins)', () => {
  const files = fs.readdirSync(DIR).filter((f) => /^boundaries-.+\.test\.cjs$/.test(f)).sort()
    .map((f) => path.join('scripts/nemo', f));
  assert.ok(files.length >= 20, `expected the boundaries-*.test.cjs suites, found ${files.length}`);
  // Under `npm test` this file runs with NODE_TEST_CONTEXT set; a nested `node --test` that inherits it runs nothing and
  // exits 0, so the variable is cleared. A second reporter writes how many tests each file executed, so every suite file
  // must run at least one test (a summed count, or one parsed from a reporter's text, would hide an empty file).
  const env = { ...process.env }; delete env.NODE_TEST_CONTEXT;
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-boundaries-'));
  const counts = path.join(tmp, 'counts.json');
  try {
    const r = spawnSync(process.execPath, ['--test', '--test-reporter=spec', '--test-reporter-destination=stdout',
      '--test-reporter=./tests/helpers/per-file-test-count-reporter.mjs', `--test-reporter-destination=${counts}`, ...files],
    { cwd: ROOT, env, encoding: 'utf8', maxBuffer: 1 << 28 });
    const out = `${r.stdout || ''}${r.stderr || ''}`;
    const ran = fs.existsSync(counts) ? JSON.parse(fs.readFileSync(counts, 'utf8') || '{}') : {};
    const empty = files.filter((f) => !(ran[path.join(ROOT, f)] > 0));
    assert.deepEqual(empty, [], `boundaries suite files that executed no test: ${empty.join(', ')}`);
    const failed = [...new Set(out.split('\n').filter((l) => l.startsWith('✖') && !l.startsWith('✖ failing tests')))];
    assert.equal(r.status, 0, `boundaries suites failed:\n${failed.join('\n')}`);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
