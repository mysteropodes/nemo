// Behavioral tests for scripts/nemo/lib/boundaries.cjs (R05 first increment).
// Each violation category gets a deliberately broken fixture (proving the
// checker rejects it) and there is one fixture per category that is valid
// (proving the checker does not cry wolf on a clean module). Fixtures are
// written to a throwaway temp dir per test — real files on disk, not mocks,
// so `resolveSpecifier`'s fs.statSync calls exercise the same path a real
// profile run would.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { checkProfile, extractImports, countNonBlankLines } = require('./lib/boundaries.cjs');

function makeRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-boundaries-'));
}

function write(root, relPath, content) {
  const abs = path.join(root, relPath);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, content);
}

const SIZE_PROFILES = { small: { warn: 5, hardMax: 10 } };
const LAYER_RULES = {
  domain: { allowedLayers: ['domain', 'shared'] },
  application: { allowedLayers: ['domain', 'application', 'shared'] },
};

test('a clean two-module profile passes with no violations', () => {
  const root = makeRoot();
  write(root, 'domain/track/index.cjs', "module.exports = { evaluate: () => 1 };\n");
  write(root, 'application/use-case.cjs', "const track = require('../domain/track/index.cjs');\nmodule.exports = () => track.evaluate();\n");

  const profile = {
    modules: [
      { id: 'domain.track', layer: 'domain', dir: 'domain/track', files: ['index.cjs'], publicApi: ['index.cjs'], sizeProfile: 'small' },
      { id: 'application.useCase', layer: 'application', dir: 'application', files: ['use-case.cjs'], publicApi: ['use-case.cjs'], sizeProfile: 'small' },
    ],
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
  };

  const report = checkProfile(profile, { root });
  assert.equal(report.ok, true);
  assert.deepEqual(report.violations, []);
});

test('an import cycle between two modules is rejected', () => {
  const root = makeRoot();
  write(root, 'domain/a/index.cjs', "const b = require('../b/index.cjs');\nmodule.exports = { b };\n");
  write(root, 'domain/b/index.cjs', "const a = require('../a/index.cjs');\nmodule.exports = { a };\n");

  const profile = {
    modules: [
      { id: 'domain.a', layer: 'domain', dir: 'domain/a', files: ['index.cjs'], publicApi: ['index.cjs'], sizeProfile: 'small' },
      { id: 'domain.b', layer: 'domain', dir: 'domain/b', files: ['index.cjs'], publicApi: ['index.cjs'], sizeProfile: 'small' },
    ],
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
  };

  const report = checkProfile(profile, { root });
  assert.equal(report.ok, false);
  const cycle = report.violations.find((v) => v.rule === 'cycle');
  assert.ok(cycle, 'expected a cycle violation');
  assert.ok(cycle.detail.path.includes('domain.a') && cycle.detail.path.includes('domain.b'));
});

test('reaching a non-public file of another module is a private-import violation', () => {
  const root = makeRoot();
  write(root, 'domain/track/index.cjs', "module.exports = { evaluate: () => 1 };\n");
  write(root, 'domain/track/internal.cjs', "module.exports = { secret: () => 2 };\n");
  write(root, 'application/use-case.cjs', "const internal = require('../domain/track/internal.cjs');\nmodule.exports = () => internal.secret();\n");

  const profile = {
    modules: [
      { id: 'domain.track', layer: 'domain', dir: 'domain/track', files: ['index.cjs', 'internal.cjs'], publicApi: ['index.cjs'], sizeProfile: 'small' },
      { id: 'application.useCase', layer: 'application', dir: 'application', files: ['use-case.cjs'], publicApi: ['use-case.cjs'], sizeProfile: 'small' },
    ],
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
  };

  const report = checkProfile(profile, { root });
  assert.equal(report.ok, false);
  const v = report.violations.find((x) => x.rule === 'private-import');
  assert.ok(v, 'expected a private-import violation');
  assert.equal(v.detail.targetModule, 'domain.track');
  assert.equal(v.detail.targetFile, 'domain/track/internal.cjs');
});

test('an import that crosses into a disallowed layer is a layer-violation', () => {
  const root = makeRoot();
  write(root, 'adapters/paper-adapter.cjs', "module.exports = { draw: () => {} };\n");
  write(root, 'domain/track/index.cjs', "const adapter = require('../../adapters/paper-adapter.cjs');\nmodule.exports = { adapter };\n");

  const profile = {
    modules: [
      { id: 'domain.track', layer: 'domain', dir: 'domain/track', files: ['index.cjs'], publicApi: ['index.cjs'], sizeProfile: 'small' },
      { id: 'adapters.paper', layer: 'adapters', dir: 'adapters', files: ['paper-adapter.cjs'], publicApi: ['paper-adapter.cjs'], sizeProfile: 'small' },
    ],
    layerRules: LAYER_RULES, // domain may only depend on domain/shared
    sizeProfiles: SIZE_PROFILES,
  };

  const report = checkProfile(profile, { root });
  assert.equal(report.ok, false);
  const v = report.violations.find((x) => x.rule === 'layer-violation');
  assert.ok(v, 'expected a layer-violation');
  assert.equal(v.detail.fromLayer, 'domain');
  assert.equal(v.detail.toLayer, 'adapters');
});

test('window.SM* access outside adapters/bootstrap is a global-state violation', () => {
  const root = makeRoot();
  write(root, 'application/use-case.cjs', "function run() {\n  return window.SMProject.getProjectKey();\n}\nmodule.exports = { run };\n");

  const profile = {
    modules: [
      { id: 'application.useCase', layer: 'application', dir: 'application', files: ['use-case.cjs'], publicApi: ['use-case.cjs'], sizeProfile: 'small' },
    ],
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
  };

  const report = checkProfile(profile, { root });
  assert.equal(report.ok, false);
  const v = report.violations.find((x) => x.rule === 'global-state');
  assert.ok(v, 'expected a global-state violation');
  assert.equal(v.detail.global, 'window.SMProject');
});

test('window.SM* access from an adapters module is allowed', () => {
  const root = makeRoot();
  write(root, 'adapters/bridge.cjs', "function sync() {\n  return window.SMProject.getProjectKey();\n}\nmodule.exports = { sync };\n");

  const profile = {
    modules: [
      { id: 'adapters.bridge', layer: 'adapters', dir: 'adapters', files: ['bridge.cjs'], publicApi: ['bridge.cjs'], sizeProfile: 'small' },
    ],
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
  };

  const report = checkProfile(profile, { root });
  assert.equal(report.ok, true);
});

test('a file over its size profile hard maximum fails, at/under it passes', () => {
  const root = makeRoot();
  const longBody = Array.from({ length: 11 }, (_, i) => `const x${i} = ${i};`).join('\n');
  write(root, 'domain/big.cjs', `${longBody}\nmodule.exports = {};\n`);
  write(root, 'domain/ok.cjs', "const a = 1;\nconst b = 2;\nmodule.exports = { a, b };\n");

  const profile = {
    modules: [
      { id: 'domain.big', layer: 'domain', dir: 'domain', files: ['big.cjs'], publicApi: ['big.cjs'], sizeProfile: 'small' },
    ],
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
  };
  const bigReport = checkProfile(profile, { root });
  assert.equal(bigReport.ok, false);
  const sizeViolation = bigReport.violations.find((v) => v.rule === 'size');
  assert.ok(sizeViolation);
  assert.equal(sizeViolation.detail.hardMax, 10);

  const okProfile = {
    modules: [{ id: 'domain.ok', layer: 'domain', dir: 'domain', files: ['ok.cjs'], publicApi: ['ok.cjs'], sizeProfile: 'small' }],
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
  };
  const okReport = checkProfile(okProfile, { root });
  assert.equal(okReport.ok, true);
});

test('a non-expired exception raises the ceiling; an expired one no longer shields and is itself reported', () => {
  const root = makeRoot();
  const longBody = Array.from({ length: 11 }, (_, i) => `const x${i} = ${i};`).join('\n');
  write(root, 'domain/big.cjs', `${longBody}\nmodule.exports = {};\n`);

  const baseModules = [{ id: 'domain.big', layer: 'domain', dir: 'domain', files: ['big.cjs'], publicApi: ['big.cjs'], sizeProfile: 'small' }];

  const shielded = checkProfile({
    modules: baseModules,
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
    exceptions: [{ path: 'domain/big.cjs', rule: 'size', ceiling: 20, owner: 'test-owner', issue: '901', expires: '2999-01-01', reason: 'fixture' }],
  }, { root, now: new Date('2026-01-01') });
  assert.equal(shielded.ok, true);
  assert.equal(shielded.exceptionsApplied.length, 1);

  const expired = checkProfile({
    modules: baseModules,
    layerRules: LAYER_RULES,
    sizeProfiles: SIZE_PROFILES,
    exceptions: [{ path: 'domain/big.cjs', rule: 'size', ceiling: 20, owner: 'test-owner', issue: '901', expires: '2020-01-01', reason: 'fixture' }],
  }, { root, now: new Date('2026-01-01') });
  assert.equal(expired.ok, false);
  assert.ok(expired.violations.find((v) => v.rule === 'expired-exception'));
  assert.ok(expired.violations.find((v) => v.rule === 'size'), 'size should fail again once its exception expired');
});

test('extractImports finds require/import/dynamic-import/export-from specifiers', () => {
  const source = [
    "const a = require('./a.cjs');",
    "import b from './b.js';",
    "import './c.js';",
    "export { x } from './d.js';",
    "async function f() { const e = await import('./e.js'); }",
  ].join('\n');
  const specs = extractImports(source).map((i) => i.specifier).sort();
  assert.deepEqual(specs, ['./a.cjs', './b.js', './c.js', './d.js', './e.js']);
});

test('countNonBlankLines ignores whitespace-only lines and a trailing newline', () => {
  assert.equal(countNonBlankLines('a\n\nb\n   \nc\n'), 3);
  assert.equal(countNonBlankLines('a\nb'), 2);
});
