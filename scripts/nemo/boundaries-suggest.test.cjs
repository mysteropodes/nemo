'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { suggest, render } = require('./lib/boundaries-suggest.cjs');

const CLI = path.join(__dirname, 'boundaries.cjs');
const DIR = 'engineering/boundaries/profiles';

function write(root, relative, text) {
  const target = path.join(root, relative);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, text);
}

const mod = (id, dir, files, extra = {}) => ({ id, layer: 'domain', dir, files, publicApi: files, sizeProfile: 'p', ...extra });

// A scratch repository with one profile per area and a few unowned files.
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-suggest-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const profile = (modules) => JSON.stringify({ modules });
  write(root, `${DIR}/scripts-nemo.profile.json`, profile([
    mod('nemo.a', 'scripts/nemo', ['a.cjs']), mod('nemo.b', 'scripts/nemo', ['b.cjs']),
    mod('nemo.test.a', 'scripts/nemo', ['a.test.cjs'], { layer: 'tooling-test', sizeProfile: 'Test file' }),
    mod('nemo.test.b', 'scripts/nemo', ['b.test.cjs'], { layer: 'tooling-test', sizeProfile: 'Test file' }),
  ]));
  write(root, `${DIR}/app-js.profile.json`, profile([
    mod('app.adapters', 'src/js/adapters', ['one.js', 'two.js']), mod('app.other', 'src/js/other', ['x.js']),
    mod('app.root.a', 'src/js', ['r.js']), mod('app.root.b', 'src/js', ['s.js']),
  ]));
  write(root, `${DIR}/app-js.coverage.json`, JSON.stringify({
    scope: { sourceRoot: 'src', sourceKinds: ['.js'] }, exclusions: [{ path: 'src/js/vendor.js' }],
    retainedSources: [{ path: 'src/js/adapters/one.js', moduleId: 'app.adapters', executionClass: 'imported-module', loadSites: [] }],
  }));
  write(root, `${DIR}/rust.profile.json`, profile([
    mod('rust.core', 'crate/src', ['lib.rs']), mod('rust.edit', 'crate/src', ['edit.rs']),
    mod('rust.tests', 'crate/tests', ['t.rs']),
  ]));
  write(root, `${DIR}/mcp-rust.profile.json`, profile([]));
  write(root, `${DIR}/rust.vendor.json`, JSON.stringify({ root: 'crate/vendor' }));
  for (const f of ['scripts/nemo/a.cjs', 'scripts/nemo/b.cjs', 'scripts/nemo/a.test.cjs', 'scripts/nemo/b.test.cjs',
    'scripts/nemo/new-tool.cjs', 'scripts/nemo/new-tool.test.cjs', 'src/js/adapters/one.js', 'src/js/adapters/two.js',
    'src/js/adapters/three.js', 'src/js/other/x.js', 'src/js/r.js', 'src/js/s.js', 'src/js/vendor.js', 'src/js/loose.js',
    'crate/vendor/v.rs', 'crate/src/t_file.rs']) write(root, f, '// x\n');
  write(root, 'crate/src/lib.rs', 'mod edit;\nmod edit_helper;\n#[path = "../tests/t.rs"]\nmod t_tests;\n');
  write(root, 'crate/src/edit.rs', 'mod nested;\n');
  write(root, 'crate/src/edit_helper.rs', '// x\n');
  write(root, 'crate/src/nested.rs', '// x\n');
  write(root, 'crate/tests/t.rs', '// x\n');
  for (const args of [['init', '-q'], ['add', '-A'], ['-c', 'user.name=t', '-c', 'user.email=t@example.test', 'commit', '-q', '-m', 'fixture']]) {
    assert.equal(spawnSync('git', args, { cwd: root }).status, 0);
  }
  return root;
}

const find = (result, file) => result.entries.find((e) => e.file === file);

test('with no paths it lists exactly the unowned files, minus exclusions and vendor', (t) => {
  const root = fixture(t);
  const files = suggest(root).entries.map((e) => e.file).sort();
  assert.deepEqual(files, ['crate/src/edit_helper.rs', 'crate/src/nested.rs', 'crate/src/t_file.rs',
    'scripts/nemo/new-tool.cjs', 'scripts/nemo/new-tool.test.cjs', 'src/js/adapters/three.js', 'src/js/loose.js']);
});

test('Rust follows the mod declaration to the declaring file\'s module', (t) => {
  const root = fixture(t);
  const result = suggest(root, ['crate/src/edit_helper.rs', 'crate/src/nested.rs']);
  const helper = find(result, 'crate/src/edit_helper.rs');
  assert.equal(helper.module, 'rust.core');
  assert.match(helper.candidates[0].reasons[0], /crate\/src\/lib\.rs declares `mod edit_helper`/);
  assert.equal(find(result, 'crate/src/nested.rs').module, 'rust.edit');
});

test('Rust follows #[path] and falls back to siblings, listing candidates when ambiguous', (t) => {
  const root = fixture(t);
  write(root, 'crate/tests/t2.rs', '// x\n');
  write(root, 'crate/src/lib.rs', 'mod edit;\n#[path = "../tests/t2.rs"]\nmod t2_tests;\n');
  const viaPath = find(suggest(root, ['crate/tests/t2.rs']), 'crate/tests/t2.rs');
  assert.equal(viaPath.module, 'rust.core');
  assert.match(viaPath.candidates[0].reasons[0], /names "\.\.\/tests\/t2\.rs"/);
  const orphan = find(suggest(root, ['crate/src/t_file.rs']), 'crate/src/t_file.rs');
  assert.equal(orphan.module, undefined);
  assert.deepEqual(orphan.candidates.map((c) => c.module).sort(), ['rust.core', 'rust.edit']);
  assert.match(orphan.note, /ambiguous/);
});

test('JS uses the module owning sibling files, and one-module-per-file directories get a new entry', (t) => {
  const root = fixture(t);
  const result = suggest(root, ['src/js/adapters/three.js', 'scripts/nemo/new-tool.test.cjs', 'src/js/loose.js']);
  const three = find(result, 'src/js/adapters/three.js');
  assert.equal(three.module, 'app.adapters');
  assert.equal(three.coverage.moduleId, 'app.adapters');
  assert.equal(three.coverage.executionClass, 'imported-module');
  const tool = find(result, 'scripts/nemo/new-tool.test.cjs');
  assert.deepEqual(tool.newModule, { id: 'nemo.test.newTool', layer: 'tooling-test', dir: 'scripts/nemo',
    files: ['new-tool.test.cjs'], publicApi: ['new-tool.test.cjs'], sizeProfile: 'Test file' });
  const loose = find(result, 'src/js/loose.js');
  assert.deepEqual(loose.candidates.map((c) => c.module).sort(), ['app.root.a', 'app.root.b']);
});

test('owned and out-of-scope paths are reported, and nothing is written', (t) => {
  const root = fixture(t);
  const before = spawnSync('git', ['status', '--porcelain', '-uall'], { cwd: root, encoding: 'utf8' }).stdout;
  const result = suggest(root, ['scripts/nemo/a.cjs', 'README.md']);
  assert.equal(find(result, 'scripts/nemo/a.cjs').status, 'owned');
  assert.deepEqual(result.unmatched, ['README.md']);
  assert.match(render(result), /no profile area covers this path/);
  assert.equal(spawnSync('git', ['status', '--porcelain', '-uall'], { cwd: root, encoding: 'utf8' }).stdout, before);
});

test('CLI --suggest prints the entry, honors --root and --json, and rejects unknown options', (t) => {
  const root = fixture(t);
  const text = spawnSync(process.execPath, [CLI, '--suggest', 'src/js/adapters/three.js', '--root', root], { encoding: 'utf8' });
  assert.equal(text.status, 0, text.stderr);
  assert.match(text.stdout, /add "three\.js" to modules\[id="app\.adapters"\]\.files/);
  const json = spawnSync(process.execPath, [CLI, '--suggest', '--json', '--root', root], { encoding: 'utf8' });
  assert.equal(JSON.parse(json.stdout).entries.length, 7);
  assert.equal(spawnSync(process.execPath, [CLI, '--suggest', '--bogus'], { encoding: 'utf8' }).status, 2);
});

test('the committed repository has no unowned file (the gate and --suggest agree)', () => {
  const result = suggest(path.resolve(__dirname, '../..'));
  const pending = result.entries.map((e) => e.file).filter((f) => !/boundaries-(?:freeze|suggest)|legacy-surface-ids/.test(f));
  assert.deepEqual(pending, []);
});
