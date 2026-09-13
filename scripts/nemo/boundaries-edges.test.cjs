'use strict';
// P10/#1012 — application dependency edges through the normal gate.
//
// Negative controls on a synthetic root: the migrated slice (domain /
// application / adapters / bootstrap) must FAIL the same local boundary
// command on a forbidden layer edge, a private import, an implicit
// window.SM* global and an import cycle, and PASS on an allowed edge; the
// unmigrated classic scripts are counted under a no-growth ceiling and
// labelled unresolved — never given a dependency-graph pass. The last test
// runs the real policy against the real profile at HEAD.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { checkProfile } = require('./lib/boundaries.cjs');
const { checkApplicationEdges, validateEdgesPolicy } = require('./lib/boundaries-application.cjs');

const ROOT = path.resolve(__dirname, '../..');
const read = (name) => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles', name), 'utf8'));

const POLICY = { schemaVersion: 1, policyId: 'nemo.app-js.edges', status: 'adopted',
  enforcedLayers: ['domain', 'application', 'adapters', 'bootstrap'], legacyLayers: ['app-legacy'],
  enforcedRules: ['layer-violation', 'private-import', 'global-state', 'cycle', 'unsupported-import', 'unprofiled-local-import', 'unsupported-global', 'expired-exception'],
  legacyUnresolvedCeiling: 1 };

// A tiny application: one domain kernel, one application service, one adapter,
// one legacy classic script that touches window.SMLegacy (its only finding).
function scaffold(t, files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-edges-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const base = {
    'src/js/domain/kernel.js': "module.exports = { add: (a, b) => a + b };\n",
    'src/js/domain/private.js': "module.exports = { secret: 1 };\n",
    'src/js/application/service.js': "const kernel = require('../domain/kernel.js');\nmodule.exports = { run: () => kernel.add(1, 2) };\n",
    'src/js/adapters/bridge.js': "const service = require('../application/service.js');\nwindow.SMBridge = service;\n",
    'src/js/legacy.js': "window.SMLegacy = true;\n",
  };
  for (const [rel, text] of Object.entries({ ...base, ...files })) {
    const abs = path.join(root, rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    if (text !== null) fs.writeFileSync(abs, text);
  }
  const profile = {
    modules: [
      { id: 'app.kernel', layer: 'domain', dir: 'src/js/domain', files: ['kernel.js', 'private.js'], publicApi: ['kernel.js'], sizeProfile: 'k' },
      { id: 'app.service', layer: 'application', dir: 'src/js/application', files: ['service.js'], publicApi: ['service.js'], sizeProfile: 'k' },
      { id: 'app.bridge', layer: 'adapters', dir: 'src/js/adapters', files: ['bridge.js'], publicApi: ['bridge.js'], sizeProfile: 'k' },
      { id: 'app.legacy', layer: 'app-legacy', dir: 'src/js', files: ['legacy.js'], publicApi: ['legacy.js'], sizeProfile: 'k' },
    ],
    layerRules: { domain: { allowedLayers: ['domain'] }, application: { allowedLayers: ['domain'] },
      adapters: { allowedLayers: ['application', 'domain'] }, 'app-legacy': { allowedLayers: ['app-legacy', 'domain'] } },
    sizeProfiles: { k: { warn: 1000, hardMax: 2000 } },
    exceptions: [],
  };
  return { root, profile };
}

function gate(root, profile, policy = POLICY) {
  return checkApplicationEdges(profile, checkProfile(profile, { root }), policy);
}

test('allowed edges pass: adapter → application → domain, legacy under its ceiling', (t) => {
  const { root, profile } = scaffold(t, {});
  const r = gate(root, profile);
  assert.equal(r.ok, true, JSON.stringify(r.problems));
  assert.equal(r.enforced.modules, 3);
  assert.deepEqual(r.enforced.violations, []);
  assert.deepEqual({ modules: r.legacy.modules, findings: r.legacy.findings, byRule: r.legacy.byRule, ok: r.legacy.ok }, { modules: 1, findings: 1, byRule: { 'global-state': 1 }, ok: true });
  assert.match(r.legacy.unresolved, /not analyzed/, 'legacy is labelled unresolved, not covered');
  assert.deepEqual(r.unclassifiedLayers, []);
});

test('forbidden layer edge fails: a domain kernel importing an adapter', (t) => {
  const { root, profile } = scaffold(t, {
    'src/js/domain/kernel.js': "const bridge = require('../adapters/bridge.js');\nmodule.exports = { add: (a, b) => a + b, bridge };\n",
    'src/js/adapters/bridge.js': "window.SMBridge = 1;\n",   // no back-edge, so this is a pure layer violation
  });
  const r = gate(root, profile);
  assert.equal(r.ok, false);
  assert.deepEqual(r.enforced.violations.map((v) => [v.rule, v.module, v.detail.toLayer]), [['layer-violation', 'app.kernel', 'adapters']]);
});

test('private import fails: the service reaching a file the kernel module does not export', (t) => {
  const { root, profile } = scaffold(t, { 'src/js/application/service.js': "const p = require('../domain/private.js');\nmodule.exports = { run: () => p.secret };\n" });
  const r = gate(root, profile);
  assert.equal(r.ok, false);
  assert.deepEqual(r.enforced.violations.map((v) => [v.rule, v.module, v.detail.targetFile]), [['private-import', 'app.service', 'src/js/domain/private.js']]);
});

test('implicit global fails in domain/application but is allowed for adapters', (t) => {
  const { root, profile } = scaffold(t, { 'src/js/domain/kernel.js': "module.exports = { add: (a, b) => a + b + (window.SMState ? 1 : 0) };\n" });
  const r = gate(root, profile);
  assert.equal(r.ok, false);
  assert.deepEqual(r.enforced.violations.map((v) => [v.rule, v.module, v.detail.global]), [['global-state', 'app.kernel', 'window.SMState']]);
  // The adapter's own window.SMBridge write is not a finding (adapters/bootstrap may).
  assert.equal(r.enforced.violations.some((v) => v.module === 'app.bridge'), false);
});

test('import cycle fails: service ↔ kernel', (t) => {
  const { root, profile } = scaffold(t, {
    'src/js/domain/kernel.js': "const service = require('../application/service.js');\nmodule.exports = { add: (a, b) => a + b, service };\n",
  });
  const r = gate(root, profile);
  assert.equal(r.ok, false);
  const rules = r.enforced.violations.map((v) => v.rule).sort();
  assert.deepEqual(rules, ['cycle', 'layer-violation'], 'the back-edge is both a forbidden layer edge and a cycle');
  assert.deepEqual(r.enforced.violations.find((v) => v.rule === 'cycle').detail.path, ['app.kernel', 'app.service', 'app.kernel']);
});

test('legacy findings above the ceiling fail as no-growth, while an unchanged migrated slice still passes', (t) => {
  const { root, profile } = scaffold(t, { 'src/js/legacy.js': "window.SMLegacy = true;\nwindow.SMMore = true;\n" });
  const r = gate(root, profile);
  assert.equal(r.ok, false);
  assert.deepEqual(r.enforced.violations, []);
  assert.deepEqual({ findings: r.legacy.findings, ok: r.legacy.ok }, { findings: 2, ok: false });
  assert.match(r.problems[0], /legacy-unresolved findings grew to 2 above the ceiling 1/);
  // Raising the ceiling is the only way to admit it — and it is an explicit policy change.
  assert.equal(gate(root, profile, { ...POLICY, legacyUnresolvedCeiling: 2 }).ok, true);
});

test('a profile layer the policy does not classify fails rather than silently passing', (t) => {
  const { root, profile } = scaffold(t, {});
  profile.modules[3].layer = 'mystery';
  profile.layerRules.mystery = { allowedLayers: ['*'] };
  const r = gate(root, profile);
  assert.equal(r.ok, false);
  assert.deepEqual(r.unclassifiedLayers, ['mystery']);
  assert.equal(r.unattributed.length, 1, 'its finding is reported as unattributed, not dropped');
});

test('an expired exception is an enforced failure regardless of the file it shields', (t) => {
  const { root, profile } = scaffold(t, {});
  profile.exceptions = [{ path: 'src/js/legacy.js', rule: 'global-state', owner: 'o', issue: '1', reason: 'r', expires: '2000-01-01' }];
  const r = gate(root, profile);
  assert.equal(r.ok, false);
  assert.deepEqual(r.enforced.violations.map((v) => v.rule), ['expired-exception']);
  assert.deepEqual(r.problems, ['1 expired exception(s) must be renewed or retired'], 'named as an expired waiver, not as an edge finding');
});

test('the edges policy is validated before use', () => {
  assert.throws(() => validateEdgesPolicy({ ...POLICY, policyId: 'other' }), /policyId/);
  assert.throws(() => validateEdgesPolicy({ ...POLICY, status: 'draft' }), /adopted/);
  assert.throws(() => validateEdgesPolicy({ ...POLICY, enforcedLayers: [] }), /enforcedLayers/);
  assert.throws(() => validateEdgesPolicy({ ...POLICY, legacyLayers: ['domain'] }), /both enforced and legacy/);
  assert.throws(() => validateEdgesPolicy({ ...POLICY, legacyUnresolvedCeiling: -1 }), /legacyUnresolvedCeiling/);
});

test('the adopted policy holds at HEAD: migrated slice clean, legacy exactly at its ceiling, every profile layer classified', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.edges.json');
  const r = checkApplicationEdges(profile, checkProfile(profile, { root: ROOT }), policy);
  assert.equal(r.ok, true, JSON.stringify(r.problems));
  assert.deepEqual(r.enforced.violations, []);
  assert.equal(r.enforced.modules, profile.modules.filter((m) => policy.enforcedLayers.includes(m.layer)).length);
  assert.equal(r.legacy.findings, policy.legacyUnresolvedCeiling, 'the ceiling is the exact adoption count; lower it with the next migration');
  assert.deepEqual(r.unclassifiedLayers, []);
  assert.deepEqual(r.unattributed, []);
});
