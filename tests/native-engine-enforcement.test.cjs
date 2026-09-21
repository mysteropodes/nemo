'use strict';

// N19 (#1351) — native-engine boundaries and normal local validation.
// Positive assertions pin the adopted graph and Cargo/module mapping. Negative
// controls run only in an isolated local clone, then prove both clone and source
// bytes were restored, so validation never edits the working candidate.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { checkRustCrate } = require('../scripts/nemo/lib/boundaries-rust.cjs');
const { checkSourceCoverage } = require('../scripts/nemo/lib/boundaries-coverage.cjs');
const { discoverRepositoryFiles } = require('../scripts/nemo/lib/boundaries-repository.cjs');
const { JOBS, PROFILES } = require('../scripts/nemo/lib/jobs.cjs');
const ci = require('../scripts/nemo/ci.cjs');

const ROOT = path.resolve(__dirname, '..');
const PROFILE = 'engineering/boundaries/profiles/rust.profile.json';
const POLICY = 'engineering/boundaries/profiles/native-engine.edges.json';
const CANDIDATE_FILES = [
  POLICY,
  PROFILE,
  'tests/native-engine-enforcement.test.cjs',
  'scripts/nemo/ci.cjs',
  'scripts/nemo/ci.test.cjs',
  'scripts/nemo/lib/jobs.cjs',
  'package.json',
  'engineering/inventory/baseline-manifest.json',
  'engineering/inventory/surfaces.json',
  'engineering/inventory/surfaces.csv',
  'engineering/inventory/SURFACES.md',
  'engineering/remediation/EXECUTION_PLAN.en.md',
  'engineering/remediation/EXECUTION_PLAN.fr.md',
];

const EXPECTED_MODULES = [
  'rust.native.engine', 'rust.native.engine.document', 'rust.native.engine.codec',
  'rust.native.engine.request-receipts', 'rust.native.engine.revision', 'rust.native.engine.commands',
  'rust.native.engine.history', 'rust.native.engine.evaluation', 'rust.native.engine.resource-leases',
  'rust.native.engine.scheduler', 'rust.native.engine.render-scene', 'rust.native.engine.compositor',
  'rust.native.engine.desktop-viewport', 'rust.native.engine.png-output', 'rust.native.engine.export-job',
  'rust.native.engine.protocol', 'rust.native.engine.application',
];

const EXPECTED_EDGES = [
  'application->commands', 'application->document', 'application->export-job', 'application->history',
  'application->protocol', 'application->render-scene', 'application->request-receipts', 'application->revision',
  'codec->document', 'commands->document', 'commands->request-receipts', 'commands->revision',
  'compositor->render-scene', 'desktop-viewport->compositor', 'desktop-viewport->render-scene',
  'desktop-viewport->scheduler', 'evaluation->document', 'evaluation->revision', 'export-job->history',
  'export-job->png-output', 'export-job->render-scene', 'export-job->resource-leases',
  'export-job->revision', 'export-job->scheduler', 'history->commands', 'history->document',
  'history->request-receipts', 'history->revision', 'png-output->compositor', 'png-output->render-scene',
  'protocol->commands', 'protocol->export-job', 'render-scene->evaluation', 'render-scene->scheduler',
  'resource-leases->revision', 'revision->document', 'scheduler->resource-leases', 'scheduler->revision',
].sort();

const PRODUCTION = {
  codec: ['codec', 'document'],
  commands: ['commands', 'request_receipts', 'revision'],
  history: ['history', 'transaction'],
  evaluation: ['evaluation'],
  scheduler: ['resource_leases', 'scheduler'],
  compositor: ['compositor', 'render_scene'],
  viewport: ['desktop_viewport'],
  export_job: ['export_job', 'png_output'],
  application: ['application', 'protocol'],
};

const TEST_TARGETS = {
  codec: '../tests/codec.rs', commands: '../tests/commands.rs', history: '../tests/history.rs',
  evaluation: '../tests/evaluation.rs', scheduler: '../tests/scheduler.rs', compositor: '../tests/compositor.rs',
  viewport: '../tests/desktop_viewport.rs', export_job: '../tests/export_job.rs', application: '../tests/application.rs',
};
const ENGINE_MACRO = '($($feature:literal => $($module:ident),+;)*) => { $( $( #[cfg(feature = $feature)] pub mod $module; )+ )* };';
const TEST_MACRO = '($($feature:literal => $module:ident = $path:literal;)*) => { $( #[cfg(feature = $feature)] #[path = $path] mod $module; )* };';

function readJson(root, relative) { return JSON.parse(fs.readFileSync(path.join(root, relative), 'utf8')); }

function section(text, start) {
  const at = text.indexOf(start);
  assert.notEqual(at, -1, `${start} missing`);
  const rest = text.slice(at + start.length);
  const next = rest.search(/^\s*\[/m);
  return next === -1 ? rest : rest.slice(0, next);
}

function macroMappings(text, name) {
  const match = new RegExp(`${name}!\\s*\\{([\\s\\S]*?)\\n\\}`).exec(text);
  assert.ok(match, `${name}! invocation missing`);
  return Object.fromEntries([...match[1].matchAll(/"([^"]+)"\s*=>\s*([^;]+);/g)].map((entry) => [
    entry[1], entry[2].split(',').map((item) => item.trim()),
  ]));
}

function macroDefinition(text, name) {
  const at = text.indexOf(`macro_rules! ${name}`);
  assert.notEqual(at, -1, `${name} definition missing`);
  const open = text.indexOf('{', at);
  let depth = 0, close = -1;
  for (let i = open; i < text.length; i++) {
    if (text[i] === '{') depth++;
    else if (text[i] === '}' && --depth === 0) { close = i; break; }
  }
  assert.notEqual(close, -1, `${name} definition is unclosed`);
  return text.slice(open + 1, close).replace(/\s+/g, ' ').trim();
}

function assertMacroDefinitions(text) {
  assert.equal(macroDefinition(text, 'engine_modules'), ENGINE_MACRO, 'engine_modules definition drift');
  assert.equal(macroDefinition(text, 'test_modules'), TEST_MACRO, 'test_modules definition drift');
}

function cargoContract(text) {
  const features = [...section(text, '[features]').matchAll(/^([A-Za-z0-9_-]+)\s*=/gm)].map((match) => match[1]);
  const targets = {};
  for (const match of text.matchAll(/\[\[test\]\]([\s\S]*?)(?=\n\[\[test\]\]|$)/g)) {
    const name = /^name\s*=\s*"([^"]+)"/m.exec(match[1])?.[1];
    targets[name] = {
      path: /^path\s*=\s*"([^"]+)"/m.exec(match[1])?.[1],
      feature: /^required-features\s*=\s*\["([^"]+)"\]/m.exec(match[1])?.[1],
    };
  }
  return { features, targets };
}

function snapshot(root, files) {
  return new Map(files.map((relative) => [relative, fs.existsSync(path.join(root, relative))
    ? fs.readFileSync(path.join(root, relative)) : null]));
}

function restore(root, bytes) {
  for (const [relative, value] of bytes) {
    const target = path.join(root, relative);
    if (value === null) fs.rmSync(target, { force: true });
    else { fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, value); }
  }
}

function copyCandidate(root) {
  for (const relative of CANDIDATE_FILES) {
    const target = path.join(root, relative);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(path.join(ROOT, relative), target);
  }
}

function nativeResult(root) {
  const result = ci.rustBoundaries(root);
  return result.crates.find((crate) => crate.crate === 'nemo-native-engine');
}

test('native-engine policy adopts the exact clean internal graph with no exceptions', () => {
  const profile = readJson(ROOT, PROFILE), policy = readJson(ROOT, POLICY);
  assert.deepEqual(policy.profileModules, EXPECTED_MODULES);
  assert.deepEqual(policy.exceptions, []);
  const result = checkRustCrate(profile, policy, { root: ROOT });
  assert.equal(result.ok, true, JSON.stringify(result.violations, null, 2));
  assert.equal(result.moduleCount, 17);
  assert.equal(result.edges.length, 38);
  assert.deepEqual(result.exceptionsApplied, []);
  assert.deepEqual(result.unsupported, []);
  const short = (id) => id.replace('rust.native.engine.', '').replace('rust.native.engine', 'root');
  assert.deepEqual(result.edges.map((edge) => `${short(edge.from)}->${short(edge.to)}`).sort(), EXPECTED_EDGES);
});

test('Cargo features, targets and declaration macros are one exact contract', () => {
  const cargoText = fs.readFileSync(path.join(ROOT, 'native-engine/Cargo.toml'), 'utf8');
  const rootText = fs.readFileSync(path.join(ROOT, 'native-engine/src/lib.rs'), 'utf8');
  const policy = readJson(ROOT, POLICY), cargo = cargoContract(cargoText);
  assert.deepEqual(cargo.features, policy.features.declared);
  assertMacroDefinitions(rootText);
  assert.deepEqual(macroMappings(rootText, 'engine_modules'), PRODUCTION);
  const testMacros = macroMappings(rootText, 'test_modules');
  assert.deepEqual(Object.keys(testMacros), Object.keys(TEST_TARGETS).map((name) => `test-${name}`));
  for (const [name, source] of Object.entries(TEST_TARGETS)) {
    assert.deepEqual(testMacros[`test-${name}`], [`${name}_tests = "${source}"`]);
    assert.deepEqual(cargo.targets[name], { path: 'src/lib.rs', feature: `test-${name}` });
  }
  assert.deepEqual(Object.keys(cargo.targets), Object.keys(TEST_TARGETS));
  const privateModules = rootText.replace('pub mod $module;', 'mod $module;');
  assert.notEqual(privateModules, rootText, 'engine module visibility token was not found');
  assert.throws(() => assertMacroDefinitions(privateModules), /engine_modules definition drift/);
});

test('native-engine validation is required in normal graphs and staged adapters stay covered', () => {
  assert.equal(JOBS['test:rust-native-engine']?.required, true);
  assert.ok(PROFILES.quick.includes('test:rust-native-engine'));
  assert.ok(PROFILES.full.includes('test:rust-native-engine'));
  assert.ok(PROFILES.quick.includes('test:unit') && PROFILES.full.includes('test:unit'));
  assert.ok(ci.QUICK.includes('test:rust-native-engine') && ci.QUICK.includes('test:unit'));
  assert.equal(readJson(ROOT, 'package.json').scripts['test:rust-native-engine'],
    'node scripts/nemo/job.cjs test:rust-native-engine');
  for (const file of ['native-viewport-adapter.test.cjs', 'native-application-transport.test.cjs',
    'native-opacity-editor-adapter.test.cjs', 'native-opacity-render-export-adapter.test.cjs']) {
    assert.equal(fs.existsSync(path.join(ROOT, 'tests', file)), true, `${file} missing from test:unit discovery`);
  }
});

test('isolated negative controls fail closed and restore every touched byte', (t) => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-n19-enforcement-'));
  const clone = path.join(temp, 'repo');
  t.after(() => fs.rmSync(temp, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }));
  const cloned = spawnSync('git', ['clone', '--quiet', '--local', ROOT, clone], { encoding: 'utf8' });
  assert.equal(cloned.status, 0, cloned.stderr);
  copyCandidate(clone);
  const touched = [...new Set(CANDIDATE_FILES.concat([
    'native-engine/src/document.rs', 'native-engine/src/lib.rs', 'src/js/adapters/n19-unregistered.js',
    'native-engine/src/n19_unregistered.rs',
  ]))];
  const original = snapshot(clone, touched), source = snapshot(ROOT, CANDIDATE_FILES);
  try {
    const rustExtra = 'native-engine/src/n19_unregistered.rs';
    fs.writeFileSync(path.join(clone, rustExtra), 'pub fn unregistered() {}\n');
    const repo = discoverRepositoryFiles({ root: clone });
    const selected = repo.entries.filter((entry) => entry.path.endsWith('.rs')).map((entry) => entry.path);
    const rustCoverage = checkSourceCoverage(readJson(clone, PROFILE), {
      root: clone, sourcePaths: selected, exclusions: readJson(clone, 'engineering/boundaries/profiles/rust.coverage.json').exclusions,
    });
    assert.equal(rustCoverage.ok, false);
    assert.ok(rustCoverage.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === rustExtra));
    restore(clone, new Map([[rustExtra, original.get(rustExtra)]]));

    const appExtra = 'src/js/adapters/n19-unregistered.js';
    fs.writeFileSync(path.join(clone, appExtra), 'export const unregistered = true;\n');
    const appScratch = fs.mkdtempSync(path.join(temp, 'app-baseline-'));
    const base = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: clone, encoding: 'utf8' }).stdout.trim();
    assert.throws(() => ci.applicationBoundaries(base, appScratch, clone),
      /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
    restore(clone, new Map([[appExtra, original.get(appExtra)]]));

    const documentPath = path.join(clone, 'native-engine/src/document.rs');
    fs.appendFileSync(documentPath, '\nuse crate::application;\n');
    const edge = nativeResult(clone);
    assert.equal(edge.ok, false);
    assert.ok(edge.violations.some((entry) => entry.rule === 'layer-violation'
      && entry.file === 'native-engine/src/document.rs' && entry.detail.targetModule === 'rust.native.engine.application'));
    restore(clone, new Map([['native-engine/src/document.rs', original.get('native-engine/src/document.rs')]]));

    const rootPath = path.join(clone, 'native-engine/src/lib.rs');
    fs.appendFileSync(rootPath, '\n#[cfg(feature = "n19-undeclared")] pub mod n19_undeclared {}\n');
    const feature = nativeResult(clone);
    assert.equal(feature.ok, false);
    assert.ok(feature.violations.some((entry) => entry.rule === 'undeclared-feature'
      && entry.detail.feature === 'n19-undeclared'));
    restore(clone, new Map([['native-engine/src/lib.rs', original.get('native-engine/src/lib.rs')]]));

    const jobsPath = path.join(clone, 'scripts/nemo/lib/jobs.cjs');
    const jobsText = fs.readFileSync(jobsPath, 'utf8');
    const removed = jobsText.replace(/^\s*'test:rust-native-engine': \{ run: jobTestNativeEngine, required: true \},\n/m, '');
    assert.notEqual(removed, jobsText, 'native-engine job registry line was not found');
    fs.writeFileSync(jobsPath, removed);
    const verify = spawnSync(process.execPath, ['scripts/nemo/verify.cjs', '--jobs', 'test:rust-native-engine', '--json'],
      { cwd: clone, encoding: 'utf8' });
    assert.notEqual(verify.status, 0);
    assert.match(verify.stderr, /unknown job "test:rust-native-engine"/);
  } finally {
    restore(clone, original);
    for (const [relative, bytes] of original) {
      const current = fs.existsSync(path.join(clone, relative)) ? fs.readFileSync(path.join(clone, relative)) : null;
      assert.deepEqual(current, bytes, `isolated clone did not restore ${relative}`);
    }
    for (const [relative, bytes] of source) assert.deepEqual(fs.readFileSync(path.join(ROOT, relative)), bytes,
      `negative control changed source worktree ${relative}`);
  }
});
