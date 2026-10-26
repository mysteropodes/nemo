'use strict';
// #2342 alias-map acceptance: surface IDs are no longer keyed by source line
// (`dom:<tag>@<line>`, `menu:<file>:<line>`), so no alias map is needed as long
// as nothing under engineering/ still references one. This pins that at zero.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const ROOT = path.resolve(__dirname, '../..');
const LEGACY = [/\bdom:[A-Za-z][A-Za-z0-9-]*@\d+\b/, /\bmenu:[^\s"'`:]+:\d+\b/];

function tracked() {
  const result = spawnSync('git', ['ls-files', '-z', '--', 'engineering'], { cwd: ROOT, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.split('\0').filter(Boolean);
}

test('no legacy line-keyed surface ID is referenced under engineering/', () => {
  const files = tracked();
  assert.ok(files.length > 100, 'engineering/ should be tracked');
  const hits = [];
  for (const file of files) {
    const bytes = fs.readFileSync(path.join(ROOT, file));
    if (bytes.includes(0)) continue;
    bytes.toString('utf8').split('\n').forEach((line, index) => {
      if (LEGACY.some((pattern) => pattern.test(line))) hits.push(`${file}:${index + 1}`);
    });
  }
  assert.deepEqual(hits, []);
});

test('the patterns match the legacy shapes and ignore placeholders', () => {
  const matches = (text) => LEGACY.some((pattern) => pattern.test(text));
  assert.equal(matches('dom:button@1042'), true);
  assert.equal(matches('menu:src/js/menus.js:88'), true);
  assert.equal(matches('dom:<tag>@<digits>'), false);
  assert.equal(matches('menu:<file>:<digits>'), false);
});
