'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { checkApplicationPolicy, checkApplicationSize } = require('./lib/boundaries-application.cjs');

const ROOT = path.resolve(__dirname, '../..');
const read = (name) => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles', name), 'utf8'));

test('adopted application policy composes fresh discovery, exact coverage and provenance pins', () => {
  const result = checkApplicationPolicy(read('app-js.profile.json'), read('app-js.coverage.json'), { root: ROOT });
  assert.equal(result.ok, true);
  assert.deepEqual({ selected: result.sourcePathCount, retained: result.retainedPathCount, excluded: result.excludedPathCount },
    { selected: 154, retained: 142, excluded: 12 });
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
    (copy) => { copy.exclusions[0].provenance.gitBlob = '0'.repeat(40); },
    (copy) => { copy.provenance.inventoryDigest.value = '0'.repeat(64); },
  ]) {
    const changed = structuredClone(policy);
    mutate(changed);
    assert.throws(() => checkApplicationPolicy(profile, changed, { root: ROOT }), /pin changed|SHA-256 changed|Git blob changed|digest changed/);
  }
});

test('every exclusion retains explicit classification, evidence and nested content pins', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  for (const mutate of [
    (copy) => { copy.exclusions[0].category = 'handwritten'; },
    (copy) => { copy.exclusions[0].component = ''; },
    (copy) => { copy.exclusions[0].reason = ''; },
    (copy) => { copy.exclusions[0].evidence = []; },
    (copy) => { delete copy.exclusions[0].provenance.sha256; },
    (copy) => { delete copy.exclusions[0].provenance.gitBlob; },
  ]) {
    const changed = structuredClone(policy);
    mutate(changed);
    assert.throws(() => checkApplicationPolicy(profile, changed, { root: ROOT }), /category|component|reason|evidence|pin is missing/);
  }
});

test('application size enforcement ignores known graph debt but rejects growth and expired exceptions', (t) => {
  const current = checkApplicationSize(read('app-js.profile.json'), { root: ROOT });
  assert.equal(current.ok, true);
  assert.equal(current.exceptionsApplied.length, 36);

  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo app size '));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'src/js'), { recursive: true });
  fs.writeFileSync(path.join(root, 'src/js/app.js'), 'window.SMLegacy = true;\nconst grew = true;\n');
  const profile = {
    modules: [{ id: 'app.legacy', layer: 'app-legacy', dir: 'src/js', files: ['app.js'],
      publicApi: ['app.js'], sizeProfile: 'legacy' }],
    layerRules: { 'app-legacy': { allowedLayers: ['app-legacy'] } },
    sizeProfiles: { legacy: { warn: 1, hardMax: 1 } },
    exceptions: [],
  };
  const growth = checkApplicationSize(profile, { root });
  assert.equal(growth.ok, false);
  assert.deepEqual(growth.violations.map((violation) => violation.rule), ['size']);
  profile.exceptions.push({ path: 'src/js/app.js', rule: 'size', owner: 'owner', issue: '901',
    reason: 'fixture', expires: '2026-01-01', ceiling: 2 });
  const expired = checkApplicationSize(profile, { root, now: new Date('2026-09-06T00:00:00Z') });
  assert.equal(expired.ok, false);
  assert.deepEqual(expired.violations.map((violation) => violation.rule).sort(), ['expired-exception', 'size']);
});
