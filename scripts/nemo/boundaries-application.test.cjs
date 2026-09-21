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
  const policy = read('app-js.coverage.json');
  const result = checkApplicationPolicy(read('app-js.profile.json'), policy, { root: ROOT });
  assert.equal(result.ok, true);
  // Counted against the policy's own sets, never pinned to `{173, 161, 12}`.
  // That triple was the merge hazard of #1316 twice over: it lived here AND in
  // the artifact's `snapshotCounts`, so two branches each adding one module
  // wrote the same bumped number in both places and git merged them silently.
  // Every discovered source is either retained or excluded — state that.
  assert.equal(result.retainedPathCount, policy.retainedSources.length);
  assert.equal(result.excludedPathCount, policy.exclusions.length);
  assert.equal(result.sourcePathCount, result.retainedPathCount + result.excludedPathCount);
  assert.ok(result.sourcePathCount > 0, 'discovery found application sources at all');
  // The breakdowns are computed too, so they cannot drift from the lists.
  assert.deepEqual(result.snapshotCounts.byExclusionCategory,
    policy.exclusions.reduce((out, record) => ({ ...out, [record.category]: (out[record.category] || 0) + 1 }), {}));
  assert.equal(Object.values(result.snapshotCounts.retainedExecutionClasses).reduce((a, b) => a + b, 0),
    policy.retainedSources.length);
});

test('a source accounted for by neither list fails on the identity, not on a stale number', () => {
  // Removing the equation from the checker must make this test red. Dropping an
  // exclusion leaves 173 discovered against 161 + 11 accounted for, so the
  // equation fires before anything else and names both sides. Without it the
  // call would return ok:false from coverage instead of throwing, which is why
  // this asserts the throw and its message rather than just a failure.
  const policy = read('app-js.coverage.json');
  assert.throws(() => checkApplicationPolicy(read('app-js.profile.json'),
    { ...policy, exclusions: policy.exclusions.slice(1) }, { root: ROOT }),
  /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('a stored snapshotCounts block is rejected rather than trusted', () => {
  // Storing the counts beside the lists is what made them a merge hazard. The
  // checker computes them, so a reintroduced block is refused outright instead
  // of quietly becoming a second source of truth again.
  const policy = read('app-js.coverage.json');
  assert.equal(policy.snapshotCounts, undefined, 'the committed policy stores no counts');
  assert.throws(() => checkApplicationPolicy(read('app-js.profile.json'),
    { ...policy, snapshotCounts: { selectedSources: 173, retainedSources: 161, exclusions: 12 } }, { root: ROOT }),
  /snapshotCounts is computed/);
});

test('provisional policy and profile membership drift fail before becoming a standard pass', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  assert.throws(() => checkApplicationPolicy(profile, { ...policy, status: 'provisional' }, { root: ROOT }), /status must be adopted/);
  const changed = structuredClone(policy);
  changed.retainedSources[0].moduleId = 'candidate-controlled-reassignment';
  assert.throws(() => checkApplicationPolicy(profile, changed, { root: ROOT }), /paths and module IDs/);
});

test('N13 native viewport adapter cannot be omitted from fresh application discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-viewport.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.viewport.adapter'
    && entry.executionClass === 'classic-without-load-site'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.viewport.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N15 native application adapter cannot be omitted from fresh application discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-application.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.application.adapter'
    && entry.executionClass === 'classic-without-load-site'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.application.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N17 native opacity editor adapter cannot be omitted from fresh application discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-opacity-editor.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.opacity.editor.adapter'
    && entry.executionClass === 'classic-without-load-site'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.opacity.editor.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N17 native opacity selection adapter cannot be omitted from fresh application discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-opacity-selection.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.opacity.selection.adapter'
    && entry.executionClass === 'classic-without-load-site'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.opacity.selection.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
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
