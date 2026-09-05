'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { compareSizeBaseline } = require('./lib/boundaries-ratchet.cjs');

function makeRoot() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-boundaries-ratchet-'));
  test.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

function write(root, relative, source) {
  const target = path.join(root, relative);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, source);
}

function profile({ profileName = 'legacy', hardMax = 10, ceiling = 20, sourcePath = 'src/legacy.cjs' } = {}) {
  const slash = sourcePath.lastIndexOf('/');
  const dir = sourcePath.slice(0, slash);
  const file = sourcePath.slice(slash + 1);
  return {
    modules: [{ id: 'legacy.module', layer: 'domain', dir, files: [file], publicApi: [file], sizeProfile: profileName }],
    sizeProfiles: { [profileName]: { warn: Math.min(5, hardMax), hardMax } },
    layerRules: { domain: { allowedLayers: ['domain'] } },
    exceptions: ceiling === null ? [] : [{
      path: sourcePath, rule: 'size', ceiling, owner: 'maintainer', issue: '901', expires: '2999-01-01', reason: 'legacy module',
    }],
  };
}

function lines(count) {
  return Array.from({ length: count }, (_, i) => `const value${i} = ${i};`).join('\n');
}

test('an exact-path size exception ceiling cannot grow', () => {
  const root = makeRoot();
  write(root, 'src/legacy.cjs', lines(18));
  const report = compareSizeBaseline(profile(), profile({ ceiling: 21 }), { root });
  assert.equal(report.ok, false);
  assert.equal(report.violations[0].rule, 'size-baseline-growth');
  assert.deepEqual(report.violations[0].detail, {
    priorCeiling: 20, candidateCeiling: 21, actualLines: 18,
    priorSizeProfile: 'legacy', candidateSizeProfile: 'legacy',
  });
});

test('renaming or reassigning a size profile cannot raise an exact-path ceiling', () => {
  const root = makeRoot();
  write(root, 'src/legacy.cjs', lines(18));
  const baseline = profile({ ceiling: null, hardMax: 20 });
  const candidate = profile({ profileName: 'renamed-large', ceiling: null, hardMax: 25 });
  candidate.modules[0].id = 'reassigned.module';
  const report = compareSizeBaseline(baseline, candidate, { root });
  assert.equal(report.ok, false);
  const pathGrowth = report.violations.find((item) => item.rule === 'size-baseline-growth');
  assert.ok(pathGrowth);
  assert.equal(pathGrowth.detail.priorSizeProfile, 'legacy');
  assert.equal(pathGrowth.detail.candidateSizeProfile, 'renamed-large');
});

test('retired-path debt cannot move under a newly inflated ordinary profile', () => {
  const root = makeRoot();
  write(root, 'src/replacement.cjs', lines(7));
  const baseline = profile({ hardMax: 3, ceiling: 6 });
  const candidate = profile({ profileName: 'renamed-large', hardMax: 50, ceiling: null, sourcePath: 'src/replacement.cjs' });
  const report = compareSizeBaseline(baseline, candidate, { root });
  assert.equal(report.ok, false);
  assert.equal(report.violations[0].rule, 'size-baseline-new-profile');
  assert.deepEqual(report.violations[0].detail, {
    sizeProfile: 'renamed-large', priorOrdinaryMax: 3, candidateHardMax: 50,
  });
});

test('prototype property names cannot conceal a newly inflated ordinary profile', async (t) => {
  for (const profileName of ['toString', 'constructor', '__proto__']) await t.test(profileName, () => {
    const root = makeRoot();
    write(root, 'src/replacement.cjs', lines(7));
    const baseline = profile({ hardMax: 3, ceiling: 6 });
    const candidate = JSON.parse(JSON.stringify(
      profile({ profileName, hardMax: 50, ceiling: null, sourcePath: 'src/replacement.cjs' }),
    ));
    assert.equal(Object.hasOwn(candidate.sizeProfiles, profileName), true);

    const report = compareSizeBaseline(baseline, candidate, { root });
    assert.equal(report.ok, false);
    const violation = report.violations.find((item) => item.rule === 'size-baseline-new-profile');
    assert.ok(violation);
    assert.deepEqual(violation.detail, { sizeProfile: profileName, priorOrdinaryMax: 3, candidateHardMax: 50 });
  });
});

test('raising a retained named ordinary profile fails even when only a new path uses it', () => {
  const root = makeRoot();
  write(root, 'src/replacement.cjs', lines(7));
  const baseline = profile({ hardMax: 3, ceiling: 6 });
  const candidate = profile({ hardMax: 50, ceiling: null, sourcePath: 'src/replacement.cjs' });
  const report = compareSizeBaseline(baseline, candidate, { root });
  assert.equal(report.ok, false);
  assert.equal(report.violations[0].rule, 'size-baseline-profile-growth');
  assert.deepEqual(report.violations[0].detail, {
    sizeProfile: 'legacy', priorHardMax: 3, candidateHardMax: 50,
  });
});

test('same-budget profile rename and a new file remain valid', () => {
  const root = makeRoot();
  write(root, 'src/replacement.cjs', lines(3));
  const baseline = profile({ hardMax: 3, ceiling: null });
  const candidate = profile({ profileName: 'renamed-same-budget', hardMax: 3, ceiling: null, sourcePath: 'src/replacement.cjs' });
  const report = compareSizeBaseline(baseline, candidate, { root });
  assert.equal(report.ok, true);
  assert.deepEqual(report.removals, [{ file: 'src/legacy.cjs', priorCeiling: 3, reason: 'source-removed' }]);
});

test('deleting a still-present path from the candidate profile fails', () => {
  const root = makeRoot();
  write(root, 'src/legacy.cjs', lines(8));
  write(root, 'src/other.cjs', lines(2));
  const candidate = profile({ sourcePath: 'src/other.cjs', ceiling: null });
  const report = compareSizeBaseline(profile(), candidate, { root });
  assert.equal(report.ok, false);
  assert.equal(report.violations[0].rule, 'size-baseline-path');
  assert.equal(report.violations[0].detail.actualLines, 8);
});

test('moving a legacy exception to a new path is rejected', () => {
  const root = makeRoot();
  write(root, 'src/renamed.cjs', lines(18));
  const report = compareSizeBaseline(profile(), profile({ sourcePath: 'src/renamed.cjs' }), { root });
  assert.equal(report.ok, false);
  assert.deepEqual(report.violations.map((item) => item.rule), ['size-baseline-new-exception']);
  assert.deepEqual(report.removals, [{ file: 'src/legacy.cjs', priorCeiling: 20, reason: 'source-removed' }]);
});

test('deleting exception accountability without lowering its allowance fails', () => {
  const root = makeRoot();
  write(root, 'src/legacy.cjs', lines(18));
  const candidate = profile({ ceiling: null, hardMax: 20 });
  const report = compareSizeBaseline(profile(), candidate, { root });
  assert.equal(report.ok, false);
  assert.ok(report.violations.some((item) => item.rule === 'size-baseline-exception'));
});

test('documented ceiling reductions and retired source paths pass', () => {
  const root = makeRoot();
  write(root, 'src/legacy.cjs', lines(15));
  const reduced = compareSizeBaseline(profile(), profile({ ceiling: 15 }), { root });
  assert.equal(reduced.ok, true);
  assert.deepEqual(reduced.reductions, [{ file: 'src/legacy.cjs', priorCeiling: 20, candidateCeiling: 15, actualLines: 15 }]);

  fs.rmSync(path.join(root, 'src/legacy.cjs'));
  const candidate = profile({ sourcePath: 'src/replacement.cjs', ceiling: null });
  write(root, 'src/replacement.cjs', lines(4));
  const retired = compareSizeBaseline(profile(), candidate, { root });
  assert.equal(retired.ok, true);
  assert.deepEqual(retired.removals, [{ file: 'src/legacy.cjs', priorCeiling: 20, reason: 'source-removed' }]);
});

test('removing an exception after shrinking below the ordinary hard maximum passes', () => {
  const root = makeRoot();
  write(root, 'src/legacy.cjs', lines(8));
  const report = compareSizeBaseline(profile(), profile({ ceiling: null }), { root });
  assert.equal(report.ok, true);
  assert.deepEqual(report.reductions, [{ file: 'src/legacy.cjs', priorCeiling: 20, candidateCeiling: 10, actualLines: 8 }]);
});

test('missing and malformed baseline objects fail explicitly', () => {
  const candidate = profile();
  assert.throws(() => compareSizeBaseline(null, candidate), /invalid baseline profile: baseline is required/);
  assert.throws(() => compareSizeBaseline({}, candidate), /invalid baseline profile: invalid profile/);
  assert.throws(() => compareSizeBaseline(candidate, {}), /invalid candidate profile: invalid profile/);
});

test('CLI baseline mode reports ratchet failures while the original invocation remains compatible', () => {
  const root = makeRoot();
  const cli = path.join(__dirname, 'boundaries.cjs');
  const baselinePath = path.join(root, 'baseline.json');
  const candidatePath = path.join(root, 'candidate.json');
  write(root, 'src/legacy.cjs', lines(18));
  fs.writeFileSync(baselinePath, JSON.stringify(profile()));
  fs.writeFileSync(candidatePath, JSON.stringify(profile({ ceiling: 21 })));

  const original = spawnSync(process.execPath, [cli, candidatePath, '--root', root, '--json'], { encoding: 'utf8' });
  assert.equal(original.status, 0);
  assert.equal(JSON.parse(original.stdout).ratchet, undefined);

  const ratcheted = spawnSync(process.execPath, [cli, candidatePath, '--baseline', baselinePath, '--root', root, '--json'], { encoding: 'utf8' });
  assert.equal(ratcheted.status, 1);
  const report = JSON.parse(ratcheted.stdout);
  assert.equal(report.ok, false);
  assert.equal(report.ratchet.violations[0].rule, 'size-baseline-growth');

  const missing = spawnSync(process.execPath, [cli, candidatePath, '--baseline', path.join(root, 'missing.json'), '--root', root], { encoding: 'utf8' });
  assert.equal(missing.status, 2);
  assert.match(missing.stderr, /could not read\/parse baseline/);
});

test('CLI still rejects actual source growth beyond an unchanged committed ceiling', () => {
  const root = makeRoot();
  const cli = path.join(__dirname, 'boundaries.cjs');
  const baselinePath = path.join(root, 'baseline.json');
  const candidatePath = path.join(root, 'candidate.json');
  write(root, 'src/legacy.cjs', lines(21));
  fs.writeFileSync(baselinePath, JSON.stringify(profile()));
  fs.writeFileSync(candidatePath, JSON.stringify(profile()));

  const run = spawnSync(process.execPath, [cli, candidatePath, '--baseline', baselinePath, '--root', root, '--json'], { encoding: 'utf8' });
  assert.equal(run.status, 1);
  const report = JSON.parse(run.stdout);
  assert.equal(report.violations[0].rule, 'size');
  assert.equal(report.violations[0].detail.lines, 21);
  assert.equal(report.ratchet.ok, true, 'unchanged policy is clean; the source-size check is the failing gate');
});
