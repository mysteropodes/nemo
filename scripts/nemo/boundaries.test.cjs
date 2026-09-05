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
const { Script } = require('node:vm');
const { checkProfile, extractImports, countNonBlankLines, validateProfile } = require('./lib/boundaries.cjs');

const roots = new Set();
test.afterEach(() => { for (const root of roots) fs.rmSync(root, { recursive: true, force: true }); roots.clear(); });
function makeRoot() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-boundaries-'));
  roots.add(root);
  return root;
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


function fixtureProfile() {
  return {
    modules: [{ id: 'a', layer: 'domain', dir: 'domain', files: ['a.cjs'], publicApi: ['a.cjs'], sizeProfile: 'small' }],
    sizeProfiles: SIZE_PROFILES,
    layerRules: LAYER_RULES,
  };
}
function sizeException() {
  return { path: 'domain/a.cjs', rule: 'size', ceiling: 20, owner: 'test-owner', issue: '901', expires: '2999-01-01', reason: 'bounded fixture' };
}

test('invalid profiles fail before checking source or granting an exception', async (t) => {
  const cases = {
    'missing warn/hardMax': (p) => { p.sizeProfiles = { small: {} }; },
    'nonfinite hardMax': (p) => { p.sizeProfiles = { small: { warn: 5, hardMax: Infinity } }; },
    'inverted thresholds': (p) => { p.sizeProfiles = { small: { warn: 11, hardMax: 10 } }; },
    'empty module list': (p) => { p.modules = []; },
    'duplicate module id': (p) => { p.modules.push({ ...p.modules[0], dir: 'other' }); },
    'overlapping file ownership': (p) => { p.modules.push({ ...p.modules[0], id: 'b' }); },
    'unlisted public API': (p) => { p.modules[0].publicApi.push('hidden.cjs'); },
    'escaped file path': (p) => { p.modules[0].files = ['../a.cjs']; },
    'missing layer rule list': (p) => { p.layerRules = { domain: {} }; },
    'missing size ceiling': (p) => { delete p.exceptions[0].ceiling; },
    'nonfinite size ceiling': (p) => { p.exceptions[0].ceiling = NaN; },
    'blank owner': (p) => { p.exceptions[0].owner = ' '; },
    'missing issue': (p) => { delete p.exceptions[0].issue; },
    'missing reason': (p) => { delete p.exceptions[0].reason; },
    'invalid date': (p) => { p.exceptions[0].expires = 'not-a-date'; },
    'impossible calendar date': (p) => { p.exceptions[0].expires = '2026-02-30'; },
    'orphan exception': (p) => { p.exceptions[0].path = 'domain/other.cjs'; },
    'duplicate exception': (p) => { p.exceptions.push({ ...p.exceptions[0] }); },
    'unknown profile field': (p) => { p.ignoredPolicy = true; },
  };
  for (const [name, mutate] of Object.entries(cases)) await t.test(name, () => {
    const p = structuredClone(fixtureProfile()); p.exceptions = [sizeException()]; mutate(p);
    assert.throws(() => validateProfile(p), /invalid profile/);
  });
});

test('valid size exception cannot hide growth beyond its finite ceiling', () => {
  const root = makeRoot(), p = fixtureProfile();
  p.exceptions = [sizeException()];
  write(root, 'domain/a.cjs', Array.from({ length: 21 }, (_, i) => `const x${i} = ${i};`).join('\n'));
  const report = checkProfile(p, { root });
  assert.equal(report.ok, false);
  assert.equal(report.violations[0].rule, 'size');
  assert.equal(report.violations[0].detail.ceiling, 20);
});

test('exception expiry is checked for every supported rule, even without a current violation', async (t) => {
  for (const rule of ['size', 'global-state', 'private-import', 'layer-violation', 'cycle']) await t.test(rule, () => {
    const root = makeRoot(), p = fixtureProfile(), exception = { ...sizeException(), rule, expires: '2026-09-05' };
    if (rule !== 'size') delete exception.ceiling;
    p.exceptions = [exception]; write(root, 'domain/a.cjs', 'module.exports = 1;');
    const report = checkProfile(p, { root, now: new Date('2026-09-05') });
    assert.deepEqual(report.violations.map((v) => v.rule), ['expired-exception']);
    assert.equal(report.exceptionsApplied.length, 0);
  });
});

test('non-size exceptions apply only to their exact rule and file', () => {
  const root = makeRoot(), p = fixtureProfile();
  p.exceptions = [{ ...sizeException(), rule: 'global-state' }]; delete p.exceptions[0].ceiling;
  p.modules[0].files.push('b.cjs');
  write(root, 'domain/a.cjs', 'window . SMProject.getProjectKey();');
  write(root, 'domain/b.cjs', "window['SMProject'].getProjectKey();");
  const report = checkProfile(p, { root });
  assert.equal(report.exceptionsApplied.length, 1);
  assert.deepEqual(report.violations.map((v) => v.file), ['domain/b.cjs']);
});

test('legal JS spelling variants enforce the same private import and layer edge', async (t) => {
  const variants = [
    "require ('../adapter/private.cjs');", "require /* comment */ ('../adapter/private.cjs');", "require?.('../adapter/private.cjs');",
    "(left + right) / require('../adapter/private.cjs') / 2;", "ratio /= import('../adapter/private.cjs');",
    "const marker = '.'\nrequire('../adapter/private.cjs');",
    "import ('../adapter/private.cjs');", "import/*comment*/('../adapter/private.cjs');",
    "import{default as x}from '../adapter/private.cjs';", "export{default}from '../adapter/private.cjs';",
    "import '../adapter/private.cjs';", "require(`../adapter/private.cjs`);",
    "const s = `value ${require ('../adapter/private.cjs')}`;",
    "import('../adapter/private.cjs', { with: { type: 'json' } });",
  ];
  for (const source of variants) await t.test(source, () => {
    const root = makeRoot(), p = fixtureProfile();
    p.modules.push({ id: 'b', layer: 'adapters', dir: 'adapter', files: ['private.cjs'], publicApi: [], sizeProfile: 'small' });
    write(root, 'domain/a.cjs', source); write(root, 'adapter/private.cjs', 'module.exports = 1;');
    const report = checkProfile(p, { root });
    assert.deepEqual(report.violations.map((v) => v.rule), ['private-import', 'layer-violation']);
  });
});

test('cycle edges survive whitespace and export syntax; a cycle exception has exact edge provenance', () => {
  const root = makeRoot(), p = fixtureProfile();
  p.modules.push({ ...p.modules[0], id: 'b', dir: 'other' });
  write(root, 'domain/a.cjs', "require ('../other/a.cjs');");
  write(root, 'other/a.cjs', "export{default}from '../domain/a.cjs';");
  assert.ok(checkProfile(p, { root }).violations.some((v) => v.rule === 'cycle'));
  p.exceptions = [{ ...sizeException(), rule: 'cycle' }]; delete p.exceptions[0].ceiling;
  const report = checkProfile(p, { root });
  assert.equal(report.ok, true); assert.equal(report.exceptionsApplied[0].rule, 'cycle');
});

test('a cycle exception removes only the matching source file contribution', () => {
  const root = makeRoot(), p = fixtureProfile();
  p.modules[0].files.push('a2.cjs'); p.modules[0].publicApi.push('a2.cjs');
  p.modules.push({ ...p.modules[0], id: 'b', dir: 'other', files: ['b.cjs'], publicApi: ['b.cjs'] });
  p.exceptions = [{ ...sizeException(), rule: 'cycle' }]; delete p.exceptions[0].ceiling;
  write(root, 'domain/a.cjs', "require('../other/b.cjs');");
  write(root, 'domain/a2.cjs', "require('../other/b.cjs');");
  write(root, 'other/b.cjs', "require('../domain/a2.cjs');");

  const report = checkProfile(p, { root });
  assert.equal(report.ok, false);
  assert.ok(report.violations.some((v) => v.rule === 'cycle'));
  assert.deepEqual(report.exceptionsApplied.map((e) => e.path), ['domain/a.cjs']);
});

test('global access variants cannot bypass the domain rule', async (t) => {
  for (const source of ['window . SMProject.run();', "window['SMProject'].run();", 'window?.SMProject.run();', "window?.['SMProject'].run();", 'window[`SMProject`].run();']) await t.test(source, () => {
    const root = makeRoot(); write(root, 'domain/a.cjs', source);
    assert.equal(checkProfile(fixtureProfile(), { root }).violations[0].rule, 'global-state');
  });
});

test('comments, strings, template text and ordinary regex literals do not invent dependencies/globals', () => {
  const root = makeRoot();
  write(root, 'domain/a.cjs', [
    "// require('../other/a.cjs'); window.SMProject.run();",
    "/* import '../other/a.cjs'; window.SMProject.run(); */",
    'const s = "require(\'../other/a.cjs\'); window.SMProject.run();";',
    "const t = `window.SMProject.run(); require('../other/a.cjs')`;",
    'const r = /window.SMProject/;',
  ].join('\n'));
  assert.equal(checkProfile(fixtureProfile(), { root }).ok, true);
});

test('object method declarations named require are clean while loader calls still fail', () => {
  const root = makeRoot();
  write(root, 'domain/a.cjs', 'const obj = { require(value) { return value; } };');
  assert.equal(checkProfile(fixtureProfile(), { root }).ok, true);

  write(root, 'domain/a.cjs', 'const obj = { require(value) { return value; } }; require(value);');
  assert.equal(checkProfile(fixtureProfile(), { root }).violations[0].rule, 'unsupported-import');
});

test('nonliteral imports fail explicitly instead of silently leaving the graph', () => {
  for (const source of ["require('../other/' + name);", 'import(target);', 'import(`../other/${name}.cjs`);']) {
    const root = makeRoot(); write(root, 'domain/a.cjs', source);
    assert.equal(checkProfile(fixtureProfile(), { root }).violations[0].rule, 'unsupported-import');
  }
});

test('CLI rejects malformed profiles and unknown/trailing/missing arguments with exit 2', () => {
  const { spawnSync } = require('node:child_process');
  const root = makeRoot(), profilePath = path.join(root, 'profile.json');
  write(root, 'domain/a.cjs', 'module.exports = 1;'); fs.writeFileSync(profilePath, JSON.stringify(fixtureProfile()));
  const cli = path.join(__dirname, 'boundaries.cjs');
  const run = (extra) => spawnSync(process.execPath, [cli, profilePath, '--root', root, '--json', ...extra], { encoding: 'utf8' });
  assert.equal(run([]).status, 0);
  for (const args of [['--unknown'], ['extra.json'], ['--root'], ['--root', '--json']]) assert.equal(run(args).status, 2);
  const invalid = fixtureProfile(); invalid.sizeProfiles = { small: {} };
  fs.writeFileSync(profilePath, JSON.stringify(invalid));
  assert.equal(run([]).status, 2);
});


test('unsupported lexical forms fail closed and computed globals are explicit', () => {
  const root = makeRoot(), p = fixtureProfile();
  write(root, 'domain/a.cjs', 'window[member].run();');
  assert.equal(checkProfile(p, { root }).violations[0].rule, 'unsupported-global');
  write(root, 'domain/a.cjs', "require('\\056/hidden.cjs');");
  assert.throws(() => checkProfile(p, { root }), /numeric string escapes/);
});

test('real application division preserves following imports and global diagnostics', async (t) => {
  // Exact counterexamples from the R05 application review at d499521.
  const cases = {
    'camera.js:59': 'u = (lo + hi) / 2;',
    'app.js:599': 'function _r3(v){return isFinite(v)?Math.round(v*1000)/1000:0;}',
    'motion.js:1182': 'var tx = (next.x - prev.x) / 2;',
    'timeline.js:104': 'var steps=Math.floor((now-playClock)/frameMs);',
    'tools.js:13': 'var step=Math.PI/4,angle=Math.round(Math.atan2(dy,dx)/step)*step;',
  };
  for (const [location, source] of Object.entries(cases)) await t.test(location, () => {
    new Script(source); // Syntax validation only; never execute application code.
    const root = makeRoot();
    write(root, 'domain/a.cjs', `${source}\nwindow.SMReal.run();\nrequire(target);\nimport('./real.js');`);
    assert.deepEqual(extractImports(fs.readFileSync(path.join(root, 'domain/a.cjs'), 'utf8')), [{ specifier: './real.js', line: 4 }]);
    assert.deepEqual(checkProfile(fixtureProfile(), { root }).violations.map(v => [v.rule, v.line]), [
      ['unsupported-import', 3], ['global-state', 2],
    ]);
  });
});

test('division and regex lexical goals retain real findings and keep regex text opaque', async (t) => {
  const cases = [
    '(a + b) / window.SMReal / 2;',
    'fn() / window.SMReal / 2;',
    'obj.if(value) / window.SMReal / 2;',
    'obj.return / window.SMReal / 2;',
    'value++ / window.SMReal / 2;',
    '({ value: 1 }) / window.SMReal / 2;',
    'if (ok) "value" / window.SMReal / 2;',
    '"}" / window.SMReal / 2;',
    '(function () {}) / window.SMReal / 2;',
    '(class {}) / window.SMReal / 2;',
    'const s = `${(a + b) / window.SMReal / 2}`;',
    'const s = `${/window.SMPhantom/.source}`; window.SMReal;',
    'if (ok) /window.SMPhantom/.test(value); window.SMReal;',
    'if (ok) /require\\("phantom"\\)|window.SMPhantom/.test(value); window.SMReal;',
    'while (ok) /window.SMPhantom/.test(value); window.SMReal;',
    'for (; ok;) /window.SMPhantom/.test(value); window.SMReal;',
    'async function f() { for await (const value of values) /window.SMPhantom/.test(value); } window.SMReal;',
    'const f = () => /window.SMPhantom/; window.SMReal;',
    'const r = /[\\/]/g; const value = 4 / /window.SMPhantom/.source.length; window.SMReal;',
    'value /= /window.SMPhantom/.source.length; window.SMReal;',
    'function f() { return ({}) / window.SMReal / 2; }',
  ];
  for (const source of cases) await t.test(source, () => {
    new Script(source);
    const root = makeRoot();
    write(root, 'domain/a.cjs', `${source}\nimport('./real.js');`);
    assert.deepEqual(extractImports(source + "\nimport('./real.js');").map(x => x.specifier), ['./real.js']);
    const report = checkProfile(fixtureProfile(), { root });
    assert.deepEqual(report.violations.map(v => [v.rule, v.detail?.global]), [['global-state', 'window.SMReal']]);
  });
});

test('slash directly after a brace remains an explicit scope limitation', () => {
  // These need block/object/function context beyond this control-parenthesis fix.
  // Keep rejecting them rather than hiding real findings inside a guessed regex.
  for (const source of [
    'if (ok) {} /window.SMPhantom/.test(value);',
    'function f() {} /window.SMPhantom/.test(value);',
    'const f = function () {} / window.SMReal / 2;',
    'const C = class {} / window.SMReal / 2;',
    'function f() { return {} / window.SMReal / 2; }',
  ]) {
    new Script(source);
    assert.throws(() => extractImports(source), /ambiguous slash after } at line/);
  }
});

test('malformed slash literals and delimiter contexts remain rejected', async (t) => {
  for (const source of [
    'if (ok) /unterminated', 'const r = /[abc/;', 'const r = /(/;', 'const r = /x/gg;',
    'const r = /x\\\n/;', 'const ratio = (a + b) /;', 'const ratio = (a + b) /',
    'const ratio = (a + b] / 2;', 'const ratio = (a + b / 2;', 'const s = `value ${(a + b) / 2`;',
  ]) await t.test(source, () => {
    assert.throws(() => new Script(source), SyntaxError);
    assert.throws(() => extractImports(source));
  });
});

test('the actual application JS corpus scans, including previously omitted large files', () => {
  const sourceRoot = path.resolve(__dirname, '../../src/js');
  const files = fs.readdirSync(sourceRoot, { recursive: true }).filter(file => /\.m?js$/.test(file)).sort();
  assert.ok(files.includes('camera.js') && files.includes('motion.js'));
  // Include vendor files in this lexical regression too; scanning is not policy adoption.
  for (const file of files) assert.doesNotThrow(() => extractImports(fs.readFileSync(path.join(sourceRoot, file), 'utf8')), file);
});
