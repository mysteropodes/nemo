'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { checkCoverageFreeze, POLICY, FROZEN, HINT } = require('./lib/boundaries-freeze.cjs');

const ROOT = path.resolve(__dirname, '../..');
const policy = () => JSON.parse(fs.readFileSync(path.join(ROOT, POLICY), 'utf8'));
const frozen = () => JSON.parse(fs.readFileSync(path.join(ROOT, FROZEN), 'utf8'));

test('committed rust.coverage.json matches its frozen key list', () => {
  const report = checkCoverageFreeze(ROOT);
  assert.equal(report.ok, true, JSON.stringify(report.violations));
});

test('a new top-level key fails with the provenance guidance', () => {
  const candidate = { ...policy(), nativeFakeRegistration: 'prose' };
  const report = checkCoverageFreeze(ROOT, { policy: candidate });
  assert.equal(report.ok, false);
  assert.deepEqual(report.violations.map((v) => v.key), ['nativeFakeRegistration']);
  assert.ok(report.violations[0].message.startsWith(HINT));
});

test('a new key inside provenance fails', () => {
  const base = policy();
  const candidate = { ...base, provenance: { ...base.provenance, nativeFakeDelivery: 'prose' } };
  const report = checkCoverageFreeze(ROOT, { policy: candidate });
  assert.deepEqual(report.violations.map((v) => v.key), ['provenance.nativeFakeDelivery']);
});

test('removing a key and editing a policy field both stay allowed', () => {
  const candidate = policy();
  const gone = Object.keys(candidate.provenance)[0];
  delete candidate.provenance[gone];
  candidate.sizePolicy = { ...candidate.sizePolicy, warn: candidate.sizePolicy.warn };
  candidate.retainedCeilings = { ...candidate.retainedCeilings, 'edited/path.rs': 1 };
  assert.equal(checkCoverageFreeze(ROOT, { policy: candidate }).ok, true);
});

test('an absent policy is skipped, a malformed allow-list is rejected', () => {
  const empty = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-freeze-'));
  test.after(() => fs.rmSync(empty, { recursive: true, force: true }));
  assert.equal(checkCoverageFreeze(empty), null);
  assert.throws(() => checkCoverageFreeze(ROOT, { policy: policy(), frozen: { ...frozen(), topLevel: 'x' } }), /topLevel/);
});

test('frozen lists are sorted so additions and removals diff cleanly', () => {
  const list = frozen();
  assert.deepEqual(list.topLevel, [...list.topLevel].sort());
  assert.deepEqual(list.provenance, [...list.provenance].sort());
});

test('the ci boundaries lane consumes the freeze check', () => {
  assert.match(fs.readFileSync(path.join(__dirname, 'ci.cjs'), 'utf8'), /repository\.ok && \(!freeze \|\| freeze\.ok\)/);
});
