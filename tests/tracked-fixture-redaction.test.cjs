'use strict';
// CS8-D08 G47 (#2381): no credential or private-machine data enters tracked fixtures, receipts or tests. Synthetic
// placeholder paths used by redaction tests are allowed; the four pre-existing real paths below are recorded
// exceptions awaiting their own disposition (frozen census evidence and a pinned oracle need their owning
// refreeze/regeneration), and any new occurrence fails.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const ROOT = path.join(__dirname, '..');
const SCOPES = ['tests', 'native-engine/tests', 'nemo-mcp/tests', 'src-tauri/tests', 'engineering'];
const TEXT = /\.(json|txt|md|cjs|js|mjs|rs|html|csv|svg|ya?ml)$/;
const CREDENTIALS = [
  /ghp_[A-Za-z0-9]{36}/, /github_pat_[A-Za-z0-9_]{20,}/, /xox[bpas]-[A-Za-z0-9-]{10,}/, /AKIA[0-9A-Z]{16}/,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/, /\bsk-[A-Za-z0-9]{32,}/,
];
const PRIVATE_PATH = /(?:\/Users|\/home)\/([a-z][a-z0-9._-]*)\//g;
const SYNTHETIC_USERS = new Set(['a', 'private', 'secret', 'example', 'name']);   // placeholders of redaction and absolute-path refusal tests
const KNOWN = new Map([
  ['engineering/inventory/census/C19ad-timeline-onkeydown-chords-motion-overrides.json', 1],
  ['engineering/inventory/census/C19ae-timeline-onkeydown-editing-navigation-delete.json', 1],
  ['engineering/inventory/census/C19ai-timeline-blend-matte-dropdowns-view-tween-buttons.json', 1],
  ['native-engine/tests/fixtures/native-media-export-plan/oracle.txt', null],
]);

function tracked() {
  return execFileSync('git', ['ls-files', '--', ...SCOPES], { cwd: ROOT, encoding: 'utf8' })
    .split('\n').filter((file) => file && TEXT.test(file) && fs.existsSync(path.join(ROOT, file)));
}

test('tracked fixtures, receipts and tests hold no credential', () => {
  const hits = [];
  for (const file of tracked()) {
    const text = fs.readFileSync(path.join(ROOT, file), 'utf8');
    for (const pattern of CREDENTIALS) if (pattern.test(text)) hits.push(`${file}: ${pattern}`);
  }
  assert.deepEqual(hits, []);
});

test('no new private-machine path: only synthetic placeholders and the recorded exceptions remain', () => {
  const found = new Map();
  for (const file of tracked()) {
    const text = fs.readFileSync(path.join(ROOT, file), 'utf8');
    for (const match of text.matchAll(PRIVATE_PATH)) {
      if (SYNTHETIC_USERS.has(match[1]) || file === 'tests/tracked-fixture-redaction.test.cjs') continue;
      found.set(file, (found.get(file) || 0) + 1);
    }
  }
  const unexpected = [...found].filter(([file, count]) => !KNOWN.has(file) || (KNOWN.get(file) !== null && count > KNOWN.get(file)));
  assert.deepEqual(unexpected, [], 'a real user path entered a tracked file');
  for (const file of KNOWN.keys()) assert.ok(found.has(file), `${file} no longer needs its exception; remove it from KNOWN`);
});
