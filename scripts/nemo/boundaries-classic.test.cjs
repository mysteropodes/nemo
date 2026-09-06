'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const vm = require('node:vm');
const { checkClassicScripts } = require('./lib/boundaries-classic.cjs');
const ROOT = path.resolve(__dirname, '../..');
const roots = new Set();
test.afterEach(() => { for (const root of roots) fs.rmSync(root, { recursive: true, force: true }); roots.clear(); });
const hash = (source) => crypto.createHash('sha1').update(`blob ${Buffer.byteLength(source)}\0`).update(source).digest('hex');
const source = (f, file) => fs.readFileSync(path.join(f.root, file), 'utf8');
function write(f, file, code) {
  fs.mkdirSync(path.dirname(path.join(f.root, file)), { recursive: true });
  fs.writeFileSync(path.join(f.root, file), code);
  f.contract.sources[file] = hash(code);
}
function anchor(f, file, text) {
  const s = source(f, file), index = s.indexOf(text);
  assert.ok(index >= 0, `fixture anchor missing: ${text}`);
  return { file, line: s.slice(0, index).split('\n').length, anchor: text };
}
const moduleRecord = (id, file, layer = 'domain') => ({ id, dir: '.', files: [file], publicApi: [file], layer, sizeProfile: 'test' });
function provider(f, symbol, file) {
  return { id: symbol, symbol, publication: 'classic-var', publicMembers: ['run'], ...anchor(f, file, `var ${symbol} =`), api: anchor(f, file, 'return { run: run };') };
}
function use(f, symbol, file, phase = 'load', member = 'run') {
  return { id: `${file}.${symbol}`, symbol, member, access: 'read', phase, ...anchor(f, file, `${symbol}.${member}`) };
}
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-classic-')); roots.add(root);
  const f = { root, contract: { version: 1, scope: { symbols: ['SMCurve'], loaders: ['vm'] }, sources: {}, providers: [], uses: [], loaders: [] } };
  write(f, 'curve.js', 'var SMCurve = (function () {\n  function run() { return 7; }\n  return { run: run };\n})();\n');
  write(f, 'motion.js', 'var captured = SMCurve.run;\n');
  write(f, 'load.cjs', "for (const file of ['curve.js', 'motion.js']) execute(file);\n");
  f.contract.providers.push(provider(f, 'SMCurve', 'curve.js'));
  f.contract.uses.push(use(f, 'SMCurve', 'motion.js'));
  f.contract.loaders.push({ id: 'vm', kind: 'vm', ...anchor(f, 'load.cjs', source(f, 'load.cjs').trim()), occurrences: [{ id: 'curve', file: 'curve.js', realm: 'page' }, { id: 'motion', file: 'motion.js', realm: 'page' }], order: ['curve', 'motion'], readiness: [] });
  f.profile = { modules: [moduleRecord('curve', 'curve.js'), moduleRecord('motion', 'motion.js', 'application')], sizeProfiles: { test: { warn: 300, hardMax: 400 } }, layerRules: { application: { allowedLayers: ['domain'] }, domain: { allowedLayers: ['domain', 'application'] } } };
  f.requiredScope = structuredClone(f.contract.scope);
  return f;
}
const check = (f, extra = {}) => checkClassicScripts(f.contract, { root: f.root, profile: f.profile, requiredScope: f.requiredScope, ...extra });
function rejects(f, rule, extra) {
  const r = check(f, extra); assert.equal(r.ok, false, JSON.stringify(r));
  assert.ok(r.violations.some((v) => v.rule === rule), `${rule}: ${JSON.stringify(r.violations)}`); return r;
}
function replace(f, file, before, after) { write(f, file, source(f, file).replace(before, after)); }
function htmlFixture() {
  const f = fixture();
  write(f, 'index.html', '<script src="curve.js"></script><script src="motion.js"></script>');
  f.contract.scope.loaders = ['html']; f.requiredScope = structuredClone(f.contract.scope);
  f.contract.loaders = [{ id: 'html', kind: 'html', sourceRoot: 'src', ...anchor(f, 'index.html', '<script src="curve.js"></script>'), readiness: [] }];
  return f;
}
// Deliberately tiny fixture adapter. Real parse5/helper integration is separately
// exercised below when explicit dependency paths are supplied; never used for app HTML.
function fixtureInventory(html) {
  const scripts = [...html.matchAll(/<script([^>]*)src="([^"]+)"([^>]*)><\/script>/g)].map((m, i) => ({ id: `tag:${i}`, index: i, kind: m[0].includes('type="module"') ? 'module' : 'classic', context: 'document', mode: 'external', execution: /async|defer|type="module"/.test(m[0]) ? 'unknown' : 'eager-classic', src: { status: 'local', path: m[2] } }));
  return { scripts, eagerOrder: scripts.filter((s) => s.execution === 'eager-classic').map((s) => s.id), diagnostics: [] };
}

test('valid dependency points consumer to provider and order points provider to consumer', () => {
  const f = fixture(), r = check(f);
  assert.deepEqual(r.violations, []); assert.equal(r.ok, true);
  assert.deepEqual(r.dependencies, [{ fromFile: 'motion.js', toFile: 'curve.js', symbol: 'SMCurve', member: 'run', phase: 'load', use: 'motion.js.SMCurve' }]);
  assert.equal(r.requirements[0].before, 'curve'); assert.equal(r.requirements[0].after, 'motion');
  const context = vm.createContext({});
  vm.runInContext(source(f, 'curve.js'), context); vm.runInContext(source(f, 'motion.js'), context);
  assert.equal(context.captured, context.SMCurve.run); assert.equal(context.captured(), 7);
});
test('missing providers and deleted use declarations fail despite unchanged valid source', () => {
  const f = fixture(); f.contract.providers = []; rejects(f, 'missing-provider');
  const g = fixture(); g.contract.uses = []; rejects(g, 'unmodeled-global-use');
});
test('private member cannot be exposed by file-level publicApi', () => {
  const f = fixture(); replace(f, 'motion.js', 'SMCurve.run', 'SMCurve.privateHelper');
  f.contract.uses = [use(f, 'SMCurve', 'motion.js', 'load', 'privateHelper')]; rejects(f, 'private-global-member');
});
test('removed or fictitious public member cannot satisfy the source API', () => {
  const f = fixture(); f.contract.providers[0].publicMembers = ['absent']; rejects(f, 'invalid-public-members');
  const g = fixture(); replace(g, 'curve.js', 'return { run: run };', 'return {};'); rejects(g, 'invalid-source-anchor');
});
test('profile privacy and layer restrictions apply to classic edges', () => {
  const f = fixture(); f.profile.modules[0].publicApi = []; rejects(f, 'private-import');
  const g = fixture(); g.profile.layerRules.application.allowedLayers = []; rejects(g, 'layer-violation');
});
for (const expression of ['SMCurve.run = () => 0;', 'SMCurve.run++;', 'delete SMCurve.run;', '++SMCurve.run;', 'SMCurve.run += 1;']) {
  test(`production write rejected: ${expression}`, () => {
    const f = fixture(); write(f, 'motion.js', expression); f.contract.uses = [use(f, 'SMCurve', 'motion.js')]; rejects(f, 'unauthorized-global-write');
  });
}
test('whole-global writes, alias/destructuring, shadowing, computed and globalThis do not pass', () => {
  for (const expression of ['SMCurve = {};', 'const alias = SMCurve;', 'const {run} = SMCurve;', 'function f(SMCurve) { return SMCurve.run; }', 'globalThis.SMCurve.run();', "globalThis['SMCurve'].run();", 'window[name].run();']) {
    const f = fixture(); write(f, 'motion.js', expression); f.contract.uses = []; assert.equal(check(f).ok, false, expression);
  }
});
test('comments, strings, and regular expressions do not become dependencies', () => {
  const f = fixture(); write(f, 'motion.js', '// SMCurve.fake\nconst text = "SMCurve.fake"; const re = /SMCurve.fake/;\nvar captured = SMCurve.run;\n');
  f.contract.uses = [use(f, 'SMCurve', 'motion.js')]; assert.deepEqual(check(f).violations, []);
});
test('template substitutions remain visible', () => {
  const f = fixture(); write(f, 'motion.js', 'const text = `literal SMCurve.fake ${SMCurve.run()}`;'); f.contract.uses = []; rejects(f, 'unmodeled-global-use');
});
test('additional window.SM access retains global-state control', () => {
  const f = fixture(); write(f, 'motion.js', source(f, 'motion.js') + 'window.SMSecret.run();\n');
  const r = rejects(f, 'global-state'); assert.equal(r.classicOk, true); assert.equal(r.ok, false);
});
test('stale source cannot validate manual binding or loader facts', () => {
  for (const file of ['curve.js', 'motion.js', 'load.cjs']) { const f = fixture(); fs.appendFileSync(path.join(f.root, file), '\n// changed'); rejects(f, 'stale-source'); }
});
test('missing physical source, missing pin and unprofiled provider fail', () => {
  const f = fixture(); fs.unlinkSync(path.join(f.root, 'curve.js')); rejects(f, 'source-unavailable');
  const g = fixture(); delete g.contract.sources['curve.js']; rejects(g, 'invalid-classic-contract');
  const h = fixture(); h.profile.modules.shift(); rejects(h, 'unprofiled-classic-source');
});
test('missing VM provider is rejected and actual consumer startup throws', () => {
  const f = fixture(); f.contract.loaders[0].occurrences.shift(); f.contract.loaders[0].order.shift(); rejects(f, 'missing-loader-provider');
  assert.throws(() => vm.runInNewContext(source(f, 'motion.js')), { name: 'ReferenceError' });
});
test('reversed VM order fails independently of valid dependency resolution', () => {
  const f = fixture(); f.contract.loaders[0].order.reverse();
  const r = rejects(f, 'load-order'); assert.equal(r.dependencies.length, 1);
});
test('loaders cannot borrow providers across loaders or realms', () => {
  const f = fixture(); f.contract.loaders[0].occurrences[0].realm = 'other'; rejects(f, 'missing-loader-provider');
  const g = fixture(), second = structuredClone(g.contract.loaders[0]); second.id = 'second';
  g.contract.loaders.push(second); g.contract.scope.loaders.push('second'); g.requiredScope.loaders.push('second');
  second.occurrences.shift(); second.order.shift(); rejects(g, 'missing-loader-provider');
});
test('repeated provider occurrences and omitted consumer applicability fail', () => {
  const f = fixture(), l = f.contract.loaders[0]; l.occurrences.push({ ...l.occurrences[0], id: 'again' }); l.order.push('again'); rejects(f, 'duplicate-provider-installation');
  const g = fixture(); g.contract.loaders[0].occurrences.pop(); g.contract.loaders[0].order.pop(); rejects(g, 'missing-loader-consumer');
});
test('call-phase dependency requires source-pinned invocation readiness', () => {
  const f = fixture(); f.contract.uses[0].phase = 'call'; rejects(f, 'unknown-readiness');
  write(f, 'load.cjs', source(f, 'load.cjs') + 'invokeAfterLoading();\n');
  f.contract.loaders[0].readiness.push({ use: f.contract.uses[0].id, after: ['curve', 'motion'], ...anchor(f, 'load.cjs', 'invokeAfterLoading();') });
  assert.equal(check(f).ok, true); f.contract.loaders[0].readiness[0].after.pop(); rejects(f, 'unknown-readiness');
});
test('call-phase cycles fail even with reviewed readiness and no reversed load dependency', () => {
  const f = fixture(); write(f, 'motion.js', 'var SMMotion = (function () {\n function run() { return SMCurve.run(); }\n return { run: run };\n})();\n');
  replace(f, 'curve.js', 'return 7;', 'return SMMotion.run();');
  f.contract.scope.symbols.push('SMMotion'); f.requiredScope.symbols.push('SMMotion');
  f.contract.providers.push(provider(f, 'SMMotion', 'motion.js'));
  f.contract.uses = [use(f, 'SMCurve', 'motion.js', 'call'), use(f, 'SMMotion', 'curve.js', 'call')];
  write(f, 'load.cjs', source(f, 'load.cjs') + 'invokeAfterLoading();\n');
  f.contract.loaders[0].readiness = f.contract.uses.map((u) => ({ use: u.id, after: ['curve', 'motion'], ...anchor(f, 'load.cjs', 'invokeAfterLoading();') }));
  const r = rejects(f, 'cycle'); assert.equal(r.violations.some((v) => v.rule === 'load-order' || v.rule === 'unknown-readiness'), false);
});
test('an import back edge and classic edge form one module cycle', () => {
  const f = fixture(); write(f, 'curve.js', source(f, 'curve.js') + "require('./motion.js');\n"); rejects(f, 'cycle');
});
test('intra-module dependency suppresses only the graph edge, preserving member privacy', () => {
  const f = fixture(); f.profile.modules = [{ ...f.profile.modules[0], files: ['curve.js', 'motion.js'] }]; assert.equal(check(f).ok, true);
  replace(f, 'motion.js', 'SMCurve.run', 'SMCurve.privateHelper'); f.contract.uses = [use(f, 'SMCurve', 'motion.js', 'load', 'privateHelper')]; rejects(f, 'private-global-member');
});
test('missing or malformed contract and narrowed adoption cannot pass', () => {
  const f = fixture();
  for (const contract of [undefined, null, {}, { ...f.contract, version: 2 }, { ...f.contract, extra: true }, { ...f.contract, providers: null }]) assert.equal(checkClassicScripts(contract, f).ok, false);
  f.contract.scope.symbols = []; rejects(f, 'invalid-classic-contract');
  const g = fixture(); g.requiredScope.loaders.push('mandatory'); rejects(g, 'invalid-classic-contract');
  const h = fixture(); h.contract.loaders[0].order.push('curve'); rejects(h, 'invalid-classic-contract');
});
test('source anchors and unique declaration IDs cannot silently drift', () => {
  const f = fixture(); f.contract.uses[0].line++; rejects(f, 'invalid-source-anchor');
  const g = fixture(); g.contract.providers[0].id = g.contract.uses[0].id; rejects(g, 'invalid-classic-contract');
});
test('HTML requires the real inventory prerequisite; malformed inventory fails', () => {
  const f = htmlFixture(); rejects(f, 'loader-unavailable');
  for (const inventoryHTML of [() => null, () => ({}), async () => fixtureInventory(source(f, 'index.html'))]) rejects(f, 'loader-unavailable', { inventoryHTML });
});
test('HTML fixture derives order independently from dependency declarations', () => {
  const f = htmlFixture(); assert.equal(check(f, { inventoryHTML: fixtureInventory }).ok, true);
  write(f, 'index.html', '<script src="motion.js"></script><script src="curve.js"></script>');
  f.contract.loaders[0].line = 1; rejects(f, 'load-order', { inventoryHTML: fixtureInventory });
});
for (const scheduling of ['async', 'defer', 'type="module"']) {
  test(`HTML ${scheduling} before consumer does not establish readiness`, () => {
    const f = htmlFixture(); replace(f, 'index.html', '<script src="curve.js">', `<script ${scheduling} src="curve.js">`);
    f.contract.loaders[0] = { ...f.contract.loaders[0], ...anchor(f, 'index.html', `<script ${scheduling} src="curve.js"></script>`) };
    rejects(f, 'unknown-readiness', { inventoryHTML: fixtureInventory });
  });
}

function production() {
  const providerFile = 'src/js/animation/curve.js', consumerFile = 'src/js/motion.js';
  const f = { root: ROOT, contract: { version: 1, scope: { symbols: ['SMAnimationCurve'], loaders: ['animation', 'fixtures'] }, sources: {
    [providerFile]: 'c42f255ee8f204806e6d3b539951864c97a86cfd', [consumerFile]: '287ea729d0909a3be560f57e9726fd7d85425a2e',
    'tests/animation/load-motion.cjs': '1be9d8c075c0a4851c6ddffb0108204639418509', 'tests/fixtures/lib/sandbox.cjs': 'b8e4246b250e2aab02ab8470adac7d1a1f5239c1',
  }, providers: [], uses: [], loaders: [] } };
  const symbol = 'SMAnimationCurve';
  f.contract.providers = [{ id: 'curve', symbol, publication: 'classic-var', publicMembers: ['evalCurvePoints'], ...anchor(f, providerFile, 'var SMAnimationCurve ='), api: anchor(f, providerFile, 'return { evalCurvePoints: evalCurvePoints };'), compatibility: anchor(f, providerFile, "if (typeof module !== 'undefined' && module.exports) module.exports = SMAnimationCurve;") }];
  f.contract.uses = [{ id: 'motion.curve.capture', symbol, member: 'evalCurvePoints', access: 'read', phase: 'load', ...anchor(f, consumerFile, 'var evalCurvePoints = SMAnimationCurve.evalCurvePoints;') }];
  const loaders = [['animation', 'tests/animation/load-motion.cjs', "for (const file of ['src/js/animation/curve.js', 'src/js/motion.js'])"], ['fixtures', 'tests/fixtures/lib/sandbox.cjs', "const MOTION_PRELUDE = ['animation/curve.js'];"]];
  f.contract.loaders = loaders.map(([id, file, text]) => ({ id, kind: 'vm', ...anchor(f, file, text), occurrences: [{ id: `${id}.curve`, file: providerFile, realm: id }, { id: `${id}.motion`, file: consumerFile, realm: id }], order: [`${id}.curve`, `${id}.motion`], readiness: [] }));
  // Use the adopted source owners/layers without relabeling legacy globals.
  const app = require('../../engineering/boundaries/profiles/app-js.profile.json');
  f.profile = { ...app, modules: app.modules.filter((m) => ['app.animation.curve', 'app.motion'].includes(m.id)), exceptions: app.exceptions.filter((e) => e.path === consumerFile || e.path === providerFile) };
  f.requiredScope = structuredClone(f.contract.scope);
  return f;
}
test('exact production Curve/Motion bytes pass both reviewed VM loader contracts', () => {
  const f = production(), r = check(f); assert.equal(r.classicOk, true, JSON.stringify(r.violations)); assert.equal(r.ok, false);
  assert.ok(r.violations.every((v) => v.rule === 'global-state')); assert.equal(r.requirements.length, 2);
  const baseline = require('./lib/boundaries.cjs').checkProfile(f.profile, { root: ROOT });
  const names = (report) => report.violations.filter((v) => v.rule === 'global-state').map((v) => v.detail.global).sort();
  assert.deepEqual(names(r), names(baseline), 'all preexisting global diagnostics remain visible');
  const sb = require('../../tests/fixtures/lib/sandbox.cjs').loadMotion();
  assert.equal(sb.SMMotion.evalCurvePoints, sb.SMAnimationCurve.evalCurvePoints);
  assert.equal(sb.SMMotion.evalCurvePoints(null, 2), 2);
  assert.equal(sb.SMMotion.evalCurvePoints([{ x: 0, y: 0 }, { x: 1, y: 1 }], 0.25), 0.25);
});
// Integration dependency locations are supplied explicitly because this base
// contains neither helper nor parse5. The default suite also proves absence fails.
if (process.env.NEMO_CLASSIC_HTML_HELPER || process.env.NEMO_CLASSIC_PARSE5) {
  test('production HTML and VM contract with the exact external helper and real parse5', async () => {
    assert.ok(process.env.NEMO_CLASSIC_HTML_HELPER && process.env.NEMO_CLASSIC_PARSE5, 'both integration prerequisites required');
    const helperBytes = fs.readFileSync(process.env.NEMO_CLASSIC_HTML_HELPER, 'utf8');
    assert.equal(hash(helperBytes), '19aa555dae4c0d952aa557d61cc5ecb036d61a70');
    const { inventoryHtmlScripts } = require(process.env.NEMO_CLASSIC_HTML_HELPER);
    const { parse } = await import(require('node:url').pathToFileURL(process.env.NEMO_CLASSIC_PARSE5).href);
    const inventoryHTML = (html, opts) => inventoryHtmlScripts(html, { ...opts, parseHTML: parse });
    const f = production(); f.contract.sources['src/index.html'] = '5176c7a71ce55a95ce55697c05ca65895be36cb8';
    f.contract.scope.loaders.push('app'); f.requiredScope.loaders.push('app');
    f.contract.loaders.push({ id: 'app', kind: 'html', sourceRoot: 'src', ...anchor(f, 'src/index.html', '<script src="js/animation/curve.js"></script>'), readiness: [] });
    const r = check(f, { inventoryHTML }); assert.equal(r.classicOk, true, JSON.stringify(r.violations)); assert.equal(r.ok, false);
    assert.ok(r.violations.every((v) => v.rule === 'global-state')); assert.equal(r.requirements.length, 3);
  });
}

for (const expression of ['SMCurve.run ??= () => 0;', 'SMCurve.run <<= 1;', '({x: SMCurve.run} = {});', 'for(SMCurve.run of []) {}', '(SMCurve.run)++;', 'delete (SMCurve.run);', 'with ({["SMCurve"]: {run(){}}}) { SMCurve.run(); }', "eval?.('var SMCurve = {}'); SMCurve.run();", 'SMCurve.runé();', 'éSMCurve.run();']) {
  test(`unsupported binding cannot be mislabeled a read: ${expression}`, () => {
    const f = fixture(); write(f, 'motion.js', expression); f.contract.uses = [use(f, 'SMCurve', 'motion.js')];
    assert.equal(check(f).ok, false, expression);
  });
}
test('API text in a comment or outside the provider is not executable publication', () => {
  for (const code of ['var SMCurve = {}; // return { run: run };', 'var SMCurve = {};\n(function () { function run() {} return { run: run }; })();']) {
    const f = fixture(); write(f, 'curve.js', code); f.contract.providers = [provider(f, 'SMCurve', 'curve.js')]; rejects(f, 'invalid-public-members');
  }
});
test('conditional provider and undefined public function fail source verification', () => {
  const f = fixture(); write(f, 'curve.js', 'if (false) ' + source(f, 'curve.js')); f.contract.providers = [provider(f, 'SMCurve', 'curve.js')]; rejects(f, 'unsupported-binding');
  const g = fixture(); replace(g, 'curve.js', 'function run()', 'function absent()'); rejects(g, 'invalid-public-members');
});
test('malformed HTML IDs, diagnostics and rejected promises cannot pass', () => {
  const f = htmlFixture();
  const bad = fixtureInventory(source(f, 'index.html')); bad.scripts[0].id = undefined; bad.eagerOrder[0] = undefined;
  rejects(f, 'loader-unavailable', { inventoryHTML: () => bad });
  rejects(f, 'loader-unavailable', { inventoryHTML: () => Promise.reject(new Error('parser unavailable')) });
  rejects(f, 'html-inventory', { inventoryHTML: (html) => ({ ...fixtureInventory(html), diagnostics: [{ code: 'parse-error' }] }) });
});

test('provider initialization cannot hide a conditional API return or early unavailable result', () => {
  for (const prefix of ['if (false) ', 'if (true) return; ']) {
    const f = fixture(); replace(f, 'curve.js', 'return { run: run };', prefix + 'return { run: run };');
    f.contract.providers = [provider(f, 'SMCurve', 'curve.js')]; rejects(f, 'unsupported-binding');
  }
});
test('optional and bracket global controls retain explicit failure', () => {
  for (const expression of ['window?.SMSecret.run();', 'window?.[name].run();', "window['SMSecret'].run();"]) {
    const f = fixture(); write(f, 'motion.js', source(f, 'motion.js') + expression); assert.equal(check(f).ok, false, expression);
  }
});
test('HTML inventory alone accepts missing provider while dependency checker rejects it', () => {
  const f = htmlFixture(); write(f, 'index.html', '<script src="motion.js"></script>');
  f.contract.loaders[0] = { ...f.contract.loaders[0], ...anchor(f, 'index.html', '<script src="motion.js"></script>') };
  assert.deepEqual(fixtureInventory(source(f, 'index.html')).diagnostics, []);
  rejects(f, 'missing-loader-provider', { inventoryHTML: fixtureInventory });
});
