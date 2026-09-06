'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { checkApplicationPolicy } = require('./lib/boundaries-application.cjs');

const ROOT = path.resolve(__dirname, '../..');
const read = (name) => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles', name), 'utf8'));

test('adopted application policy composes fresh discovery, exact coverage and provenance pins', () => {
  const result = checkApplicationPolicy(read('app-js.profile.json'), read('app-js.coverage.json'), { root: ROOT });
  assert.equal(result.ok, true);
  assert.deepEqual({ selected: result.sourcePathCount, retained: result.retainedPathCount, excluded: result.excludedPathCount },
    { selected: 152, retained: 140, excluded: 12 });
});

test('provisional policy and profile membership drift fail before becoming a standard pass', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  assert.throws(() => checkApplicationPolicy(profile, { ...policy, status: 'provisional' }, { root: ROOT }), /status must be adopted/);
  const changed = structuredClone(policy);
  changed.retainedSources[0].moduleId = 'candidate-controlled-reassignment';
  assert.throws(() => checkApplicationPolicy(profile, changed, { root: ROOT }), /paths and module IDs/);
});

test('source, profile and exclusion provenance cannot drift behind unchanged policy', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  for (const mutate of [
    (copy) => { copy.provenance.sourceRootTree = '0'.repeat(40); },
    (copy) => { copy.provenance.bootstrap.sha256 = '0'.repeat(64); },
    (copy) => { copy.exclusions[0].gitBlob = '0'.repeat(40); },
    (copy) => { copy.provenance.inventoryDigest.value = '0'.repeat(64); },
  ]) {
    const changed = structuredClone(policy);
    mutate(changed);
    assert.throws(() => checkApplicationPolicy(profile, changed, { root: ROOT }), /pin changed|SHA-256 changed|Git blob changed|digest changed/);
  }
});
