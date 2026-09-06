'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const ci = require('./ci.cjs');
const ROOT = path.resolve(__dirname, '../..');

function passingNeeds() { return Object.fromEntries(ci.LANES.map((lane) => [lane, { result: 'success' }])); }
function receipt(jobs = [{ name: 'test:browser', status: 'pass', exitCode: 0 }]) {
  return { schema: 'nemo.receipt/1', jobs, summary: { overall: 'pass', exitCode: 0 } };
}
function scratch(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo ci test '));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }));
  return dir;
}
function git(root, args) {
  const r = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  return r.stdout.trim();
}

test('aggregate requires every named lane, including absent and unavailable results', () => {
  assert.equal(ci.aggregate(passingNeeds()).ok, true);
  assert.equal(ci.aggregate(null).ok, false);
  assert.equal(ci.aggregate({}).ok, false);
  for (const lane of ci.LANES) {
    for (const result of ['failure', 'cancelled', 'skipped', 'blocked', 'not-run', 'unavailable', 'pending', '', null]) {
      const needs = passingNeeds();
      needs[lane] = { result };
      assert.equal(ci.aggregate(needs).ok, false, `${lane}: ${result}`);
    }
    const needs = passingNeeds();
    delete needs[lane];
    assert.equal(ci.aggregate(needs).ok, false, `missing ${lane}`);
  }
});

test('zero-exit optional jobs cannot conceal blocked/not-run/failure or malformed receipts', () => {
  const required = ['test:browser'];
  assert.equal(ci.validateReceipt(receipt(), required, 0).ok, true);
  for (const status of ['blocked', 'not-run', 'fail', 'failure', 'cancelled', 'unavailable', undefined]) {
    assert.equal(ci.validateReceipt(receipt([{ name: required[0], status }]), required, 0).ok, false, status);
  }
  for (const bad of [null, {}, { schema: 'other', jobs: [] }, receipt([]), receipt([null]),
    receipt([{ name: required[0], status: 'pass' }, { name: required[0], status: 'pass' }]),
    receipt([{ name: required[0], status: 'pass', exitCode: 1 }]),
    { ...receipt(), summary: { overall: 'blocked', exitCode: 0 } }]) {
    assert.equal(ci.validateReceipt(bad, required, 0).ok, false);
  }
  assert.equal(ci.validateReceipt(receipt(), [], 0).ok, false);
  for (const exit of [1, 2, null]) assert.equal(ci.validateReceipt(receipt(), required, exit).ok, false);
});

test('surface applicability exempts explicit tooling/docs only and treats unknown paths conservatively', () => {
  assert.deepEqual(ci.applicability(['engineering/ci/README.md', 'scripts/nemo/ci.cjs',
    'tests/nemo-ci.test.cjs', '.github/workflows/nemo-validation.yml']).required, []);
  for (const file of ['src/js/app.js', 'src/wasm/x.wasm', 'src-tauri/src/lib.rs', 'geometry-wasm/Cargo.toml',
    'vectorize-wasm/src/lib.rs', 'package-lock.json', 'package.json', 'tests/browser/foo.spec.js',
    'tests/animation.test.cjs', '.github/workflows/release.yml', 'new-module/unknown.js',
    'docs/example.js', 'engineering/fixtures/document.json', 'scripts/nemo/build.cjs',
    'scripts/nemo/lib/jobs.cjs', 'scripts/nemo/lib/browser-runtime.cjs']) {
    assert.deepEqual(ci.applicability(['README.md', file]).required, ci.SURFACES, file);
  }
});

const benignPaths = ['scripts/nemo/README.md',
  'engineering/boundaries/profiles/scripts-nemo.fixture/src/cycle-a.cjs'];
for (const file of benignPaths) {
  test(`benign-only ${file} does not require runtime jobs; mixed application changes still do`, () => {
    assert.deepEqual(ci.applicability([file]).required, []);
    assert.deepEqual(ci.applicability([file]).affected, []);
    const mixed = ci.applicability([file, 'src/js/app.js']);
    assert.deepEqual(mixed.required, ci.SURFACES);
    assert.deepEqual(mixed.affected, ['src/js/app.js']);
  });
}

test('renames into and out of exempt command docs and boundary fixtures retain both endpoints', (t) => {
  const root = scratch(t);
  git(root, ['init', '-q']);
  git(root, ['config', 'user.name', 'CI fixture']);
  git(root, ['config', 'user.email', 'ci@example.invalid']);
  git(root, ['config', 'maintenance.auto', 'false']);
  git(root, ['config', 'gc.auto', '0']);
  const appPaths = ['src/js/app.js', 'src/js/other.js'];
  fs.mkdirSync(path.join(root, 'src/js'), { recursive: true });
  for (const file of appPaths) fs.writeFileSync(path.join(root, file), `// ${file}\n`);
  git(root, ['add', '.']);
  git(root, ['-c', 'commit.gpgsign=false', 'commit', '-qm', 'application sources']);
  for (const [from, to] of [[appPaths, benignPaths], [benignPaths, appPaths]]) {
    const base = git(root, ['rev-parse', 'HEAD']);
    for (let i = 0; i < from.length; i++) {
      fs.mkdirSync(path.dirname(path.join(root, to[i])), { recursive: true });
      git(root, ['mv', from[i], to[i]]);
    }
    git(root, ['-c', 'commit.gpgsign=false', 'commit', '-qm', 'rename sources']);
    const files = ci.changedFiles(base, root);
    assert.deepEqual(files.sort(), [...appPaths, ...benignPaths].sort());
    const selection = ci.applicability(files);
    assert.deepEqual(selection.required, ci.SURFACES);
    assert.deepEqual(selection.affected.sort(), appPaths);
  }
  for (const file of ['scripts/nemo/README.cjs',
    'engineering/boundaries/profiles/scripts-nemo.fixture-other/src/cycle-a.cjs',
    'engineering/boundaries/profiles/other.fixture/src/cycle-a.cjs']) {
    assert.deepEqual(ci.applicability([file]).required, ci.SURFACES, file);
  }
});

test('baseline is materialized from the explicit base commit, never candidate policy or a moving branch', (t) => {
  const root = scratch(t);
  git(root, ['init', '-q']);
  git(root, ['config', 'user.name', 'CI fixture']);
  git(root, ['config', 'user.email', 'ci@example.invalid']);
  git(root, ['config', 'maintenance.auto', 'false']);
  git(root, ['config', 'gc.auto', '0']);
  const policy = path.join(root, ci.PROFILE);
  fs.mkdirSync(path.dirname(policy), { recursive: true });
  fs.writeFileSync(policy, '{"protected":true}\n');
  fs.mkdirSync(path.join(root, 'src'), { recursive: true });
  fs.writeFileSync(path.join(root, 'src', 'app.js'), '// app\n');
  git(root, ['add', '.']);
  git(root, ['-c', 'commit.gpgsign=false', 'commit', '-qm', 'base']);
  const base = git(root, ['rev-parse', 'HEAD']);
  fs.writeFileSync(policy, '{"protected":false}\n');
  git(root, ['mv', 'src/app.js', 'README.md']);
  git(root, ['add', '.']);
  git(root, ['-c', 'commit.gpgsign=false', 'commit', '-qm', 'candidate changes policy and renames app']);
  const output = scratch(t);
  const materialized = ci.materializeBaseline(base, output, root);
  assert.deepEqual(JSON.parse(fs.readFileSync(materialized.file)), { protected: true });
  assert.equal(materialized.base, base);
  assert.match(materialized.sha256, /^[a-f0-9]{64}$/);
  assert.deepEqual(ci.applicability(ci.changedFiles(base, root)).required, ci.SURFACES);
  fs.mkdirSync(path.join(root, 'scripts/nemo'), { recursive: true });
  const checker = path.join(root, 'scripts/nemo/boundaries.cjs');
  for (const [report, ok] of [[{ ok: true }, false], [{ ok: true, ratchet: { ok: false } }, false],
    [{ ok: false, ratchet: { ok: true } }, false], [{ ok: true, ratchet: { ok: true } }, true]]) {
    fs.writeFileSync(checker, `const fs = require('node:fs');
const args = process.argv.slice(2);
fs.writeFileSync('boundary-invocation.json', JSON.stringify({ args,
  baseline: JSON.parse(fs.readFileSync(args[args.indexOf('--baseline') + 1])) }));
console.log(${JSON.stringify(JSON.stringify(report))});`);
    assert.equal(ci.boundaries(base, root).ok, ok);
    const invocation = JSON.parse(fs.readFileSync(path.join(root, 'boundary-invocation.json')));
    assert.equal(invocation.args[0], ci.PROFILE);
    assert.deepEqual(invocation.args.slice(3), ['--root', root, '--json']);
    assert.deepEqual(invocation.baseline, { protected: true });
    assert.equal(fs.existsSync(invocation.args[2]), false, 'temporary baseline removed after checker');
  }
  for (const invalid of [undefined, 'origin/main', '--help', '0'.repeat(40)]) {
    assert.throws(() => ci.materializeBaseline(invalid, scratch(t), root));
  }
  fs.rmSync(policy);
  git(root, ['add', '.']);
  git(root, ['-c', 'commit.gpgsign=false', 'commit', '-qm', 'remove profile']);
  assert.throws(() => ci.materializeBaseline(git(root, ['rev-parse', 'HEAD']), scratch(t), root));
});

test('CI invokes established verify with explicit jobs and rejects a real zero-exit blocked receipt', (t) => {
  const root = scratch(t);
  fs.mkdirSync(path.join(root, 'scripts/nemo'), { recursive: true });
  const runner = path.join(root, 'scripts/nemo/verify.cjs');
  fs.writeFileSync(runner, `const fs = require('node:fs');
fs.writeFileSync('invocation.json', JSON.stringify(process.argv.slice(2)));
console.log(${JSON.stringify(JSON.stringify(receipt([{ name: 'test:browser', status: 'blocked', reason: 'fixture runner unavailable' }])))});
`);
  const result = ci.verify(['test:browser'], root);
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(root, 'invocation.json'))), ['--jobs', 'test:browser', '--json']);
  assert.equal(result.ok, false);
  assert.match(result.problems.join('\n'), /blocked.*fixture runner unavailable/);
  fs.writeFileSync(runner, `console.log(${JSON.stringify(JSON.stringify(receipt()))});`);
  assert.equal(ci.verify(['test:browser'], root).ok, true);
  fs.rmSync(runner);
  assert.equal(ci.verify(['test:browser'], root).ok, false);
});

test('workflow invokes each CLI lane and always aggregates exact dependencies without PR privileges', (t) => {
  const workflow = fs.readFileSync(path.join(ROOT, '.github/workflows/nemo-validation.yml'), 'utf8');
  const invocations = [...workflow.matchAll(/^\s+- run: node scripts\/nemo\/ci.cjs (\w+)$/gm)].map((m) => m[1]);
  assert.deepEqual(invocations, [...ci.LANES, 'aggregate']);
  assert.match(workflow, /aggregate:\n\s+name: Nemo \/ required\n\s+if: always\(\)\n\s+needs: \[quick, boundaries, surfaces\]/);
  assert.match(workflow, /NEMO_CI_NEEDS: \$\{\{ toJSON\(needs\) \}\}/);
  assert.match(workflow, /NEMO_CI_BASE_SHA: \$\{\{ github.event.pull_request.base.sha \}\}/);
  assert.doesNotMatch(workflow, /pull_request_target|secrets\.|self-hosted|contents: write|continue-on-error|paths-ignore:|paths:/);
  assert.equal((workflow.match(/uses: actions\/checkout@/g) || []).length,
    (workflow.match(/persist-credentials: false/g) || []).length);
  const dir = scratch(t);
  for (const [needs, status] of [[passingNeeds(), 0], [{ ...passingNeeds(), quick: { result: 'cancelled' } }, 1], [{}, 1]]) {
    const r = spawnSync(process.execPath, ['scripts/nemo/ci.cjs', 'aggregate'], { cwd: ROOT, encoding: 'utf8',
      env: { ...process.env, NEMO_CI_NEEDS: JSON.stringify(needs), NEMO_CI_REPORT_DIR: dir } });
    assert.equal(r.status, status, r.stderr);
    assert.equal(JSON.parse(fs.readFileSync(path.join(dir, 'aggregate.json'))).ok, status === 0);
  }
});
