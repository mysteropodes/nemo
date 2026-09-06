'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { discoverSourcePaths } = require('./lib/boundaries-discovery.cjs');
const { checkSourceCoverage } = require('./lib/boundaries-coverage.cjs');

function makeRoot(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-discovery-'));
  fs.mkdirSync(path.join(root, 'src/js'), { recursive: true });
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

function write(root, file) {
  const target = path.join(root, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, 'void 0;\n');
}

function options(root, overrides = {}) {
  return { root, sourceRoots: ['src/js'], extensions: ['.js'], ...overrides };
}

function profile(files) {
  return { modules: [{ id: 'app', dir: 'src/js', files }] };
}

function diagnostics(report) {
  return report.violations.map(({ rule, file }) => [rule, file]);
}

test('discovers untracked and Git-ignored additions, nested sources, and deletions on disk', (t) => {
  const root = makeRoot(t);
  const git = (...args) => execFileSync('git', ['-C', root, ...args], { encoding: 'utf8' }).trim();
  git('init', '--quiet');
  write(root, 'src/js/tracked.js');
  git('add', 'src/js/tracked.js');
  fs.writeFileSync(path.join(root, '.gitignore'), 'src/js/ignored.js\n');
  for (const file of ['src/js/new.js', 'src/js/deep/nested.mjs', 'src/js/ignored.js', 'src/js/types.d.ts', 'src/js/readme.txt']) write(root, file);
  assert.equal(git('ls-files'), 'src/js/tracked.js');
  assert.match(git('ls-files', '--others', '--exclude-standard'), /src\/js\/new\.js/);
  assert.equal(git('check-ignore', 'src/js/ignored.js'), 'src/js/ignored.js');
  const opts = options(root, { extensions: ['.js', '.mjs', '.ts'] });
  assert.deepEqual(discoverSourcePaths(opts), [
    'src/js/deep/nested.mjs', 'src/js/ignored.js', 'src/js/new.js', 'src/js/tracked.js', 'src/js/types.d.ts',
  ]);
  fs.unlinkSync(path.join(root, 'src/js/tracked.js'));
  fs.renameSync(path.join(root, 'src/js/new.js'), path.join(root, 'src/js/renamed.js'));
  assert.deepEqual(discoverSourcePaths(opts), [
    'src/js/deep/nested.mjs', 'src/js/ignored.js', 'src/js/renamed.js', 'src/js/types.d.ts',
  ]);
});

test('uses explicit case-sensitive suffixes, including .ts and compound .d.ts', (t) => {
  const root = makeRoot(t);
  for (const name of ['app.ts', 'types.d.ts', 'types.d.tsx', 'upper.TS', 'entry.js', 'entry.cjs', 'entry.mjs', 'sheet.css']) write(root, `src/js/${name}`);
  assert.deepEqual(discoverSourcePaths(options(root, { extensions: ['.d.ts'] })), ['src/js/types.d.ts']);
  assert.deepEqual(discoverSourcePaths(options(root, { extensions: ['.ts', '.d.ts', '.ts'] })), ['src/js/app.ts', 'src/js/types.d.ts']);
  assert.deepEqual(discoverSourcePaths(options(root, { extensions: ['.TS', '.css', '.cjs'] })), ['src/js/entry.cjs', 'src/js/sheet.css', 'src/js/upper.TS']);
});

test('applies no name-based, hidden-directory, vendor, generated, or gitignore exclusions', (t) => {
  const root = makeRoot(t);
  const expected = [
    'src/js/.hidden/cache.js', 'src/js/generated/schema.js', 'src/js/node_modules/library.js',
    'src/js/runtime.min.js', 'src/js/runtime.vendor.js', 'src/js/vendor/library.js',
  ];
  for (const file of [...expected].reverse()) write(root, file);
  assert.deepEqual(discoverSourcePaths(options(root)), expected);
});

test('sorts and deduplicates overlapping roots without mutating inputs or caching results', (t) => {
  const root = makeRoot(t);
  const expected = ['entry.js', 'scripts/a.js', 'src/js/deep/c.js', 'src/js/z.js'];
  for (const file of [...expected].reverse()) write(root, file);
  const opts = Object.freeze({
    root, sourceRoots: Object.freeze(['src/js/deep', '.', 'src/js', 'src/js', 'scripts']),
    extensions: Object.freeze(['.mjs', '.js']),
  });
  assert.deepEqual(discoverSourcePaths(opts), expected);
  const first = discoverSourcePaths({ ...opts, sourceRoots: [...opts.sourceRoots].reverse() });
  assert.deepEqual(first, expected);
  first.length = 0;
  assert.deepEqual(discoverSourcePaths(opts), expected);
});

test('preserves literal POSIX names and traverses directories with source suffixes', (t) => {
  const root = makeRoot(t);
  const expected = ['src/js/café [1] #.js', 'src/js/directory.js/inside.js', 'src/js/literal*.js'];
  for (const file of expected) write(root, file);
  assert.deepEqual(discoverSourcePaths(options(root)), expected);
});

test('returns an empty selection for an existing tree without matching files', (t) => {
  const root = makeRoot(t);
  assert.deepEqual(discoverSourcePaths(options(root)), []);
  write(root, 'src/js/readme.txt');
  assert.deepEqual(discoverSourcePaths(options(root)), []);
});

test('requires explicit options and refuses policy/declaration/exclusion inputs', (t) => {
  const root = makeRoot(t);
  for (const opts of [undefined, null, [], 'src/js', {}, { root }, { sourceRoots: ['src/js'], extensions: ['.js'] }]) {
    assert.throws(() => discoverSourcePaths(opts), /invalid source discovery:/);
  }
  for (const key of ['sourcePaths', 'profile', 'baseline', 'exclusions']) {
    assert.throws(() => discoverSourcePaths({ ...options(root), [key]: [] }), /unknown option/);
  }
  for (const value of [undefined, null, '', ' ', 42, 'bad\0path', 'bad\npath']) {
    assert.throws(() => discoverSourcePaths(options(value)), /root must be an explicit nonempty directory path/);
  }
});

test('rejects missing, empty, malformed, and ambiguous extension or root selections', (t) => {
  const root = makeRoot(t);
  for (const key of ['sourceRoots', 'extensions']) {
    for (const value of [undefined, null, [], '.js', {}, [undefined], new Array(1)]) {
      assert.throws(() => discoverSourcePaths(options(root, { [key]: value })), /invalid source discovery:/);
    }
  }
  for (const extension of ['', '.', '..', 'js', '*.js', '.j?', '.{js,ts}', '.js/.ts', '.js\\ts', '.js\0', '.js ', 42]) {
    assert.throws(() => discoverSourcePaths(options(root, { extensions: [extension] })), /literal dot-prefixed suffixes/);
  }
});

test('rejects absolute, escaping, or non-normalized source roots', (t) => {
  const root = makeRoot(t);
  for (const dir of ['', ' ', '/tmp', '../src', 'src/../src', './src', 'src//js', 'src/', 'C:/src', 'C:src', 'src\\js', 'src/\0', 42]) {
    assert.throws(() => discoverSourcePaths(options(root, { sourceRoots: [dir] })), /normalized repository-relative path/);
  }
});

test('validates each overlapping source root, including missing paths and regular files', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/app.js');
  for (const invalidRoot of ['src/js/missing', 'src/js/app.js', 'src/js/app.js/child']) {
    for (const sourceRoots of [['src/js', invalidRoot], [invalidRoot, 'src/js'], ['.', invalidRoot]]) {
      assert.throws(() => discoverSourcePaths(options(root, { sourceRoots })), /could not inspect|source root must be a directory/);
    }
  }
});

test('missing repository roots and roots deleted between calls fail with portable errors', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/app.js');
  assert.throws(() => discoverSourcePaths(options(path.join(root, 'src/js/app.js'))), /root must be a directory/);
  assert.throws(() => discoverSourcePaths(options(path.join(root, 'absent'))), (error) => {
    assert.match(error.message, /could not inspect root \(ENOENT\)/);
    assert.equal(error.message.includes(root), false);
    return true;
  });
  assert.deepEqual(discoverSourcePaths(options(root)), ['src/js/app.js']);
  fs.rmSync(path.join(root, 'src/js'), { recursive: true });
  assert.throws(() => discoverSourcePaths(options(root)), /could not inspect src\/js \(ENOENT\)/);
});

test('an explicitly symlinked repository root sets the canonical trust boundary', (t) => {
  const root = makeRoot(t);
  const parent = makeRoot(t);
  write(root, 'src/js/app.js');
  const alias = path.join(parent, 'repository');
  fs.symlinkSync(root, alias, 'dir');
  assert.deepEqual(discoverSourcePaths(options(alias)), ['src/js/app.js']);
});

test('rejects file symlinks even when internal, broken, or outside the selected suffixes', (t) => {
  const root = makeRoot(t);
  const outside = makeRoot(t);
  write(root, 'src/js/app.js');
  write(outside, 'src/js/secret.js');
  const link = path.join(root, 'src/js/link.txt');
  for (const target of ['app.js', 'missing.js', path.join(outside, 'src/js/secret.js')]) {
    fs.symlinkSync(target, link);
    assert.throws(() => discoverSourcePaths(options(root)), (error) => {
      assert.match(error.message, /symlink traversal is not supported: src\/js\/link.txt/);
      assert.equal(error.message.includes(outside), false);
      return true;
    });
    fs.unlinkSync(link);
  }
});

test('rejects directory symlinks, cycles, and symlink ancestors of explicitly selected roots', (t) => {
  const root = makeRoot(t);
  const outside = makeRoot(t);
  write(root, 'src/js/deep/app.js');
  const link = path.join(root, 'src/js/link');
  for (const target of ['.', 'deep', path.join(outside, 'src')]) {
    fs.symlinkSync(target, link, 'dir');
    for (const sourceRoots of [['src/js'], ['src/js/link'], ['src/js/link/js'], ['src/js', 'src/js/link']]) {
      assert.throws(() => discoverSourcePaths(options(root, { sourceRoots })), /symlink traversal is not supported: src\/js\/link/);
    }
    fs.unlinkSync(link);
  }
});

test('rejects encountered paths that cannot be expressed as normalized POSIX paths', { skip: process.platform === 'win32' }, (t) => {
  const root = makeRoot(t);
  for (const name of ['bad\\name.js', 'bad\nname.txt']) {
    const target = path.join(root, 'src/js', name);
    fs.writeFileSync(target, 'void 0;\n');
    assert.throws(() => discoverSourcePaths(options(root)), /entry must be a normalized repository-relative path/);
    fs.unlinkSync(target);
  }
});

test('invalid UTF-8 filenames cannot alias a different entry through replacement decoding', { skip: process.platform === 'win32' }, (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/\uFFFD.js');
  const invalidName = Buffer.concat([Buffer.from(path.join(root, 'src/js') + path.sep), Buffer.from([0xff]), Buffer.from('.js')]);
  try {
    fs.writeFileSync(invalidName, 'void 0;\n');
  } catch (error) {
    if (error.code === 'EILSEQ') return t.skip('Filesystem rejects invalid UTF-8 filenames; fixture cannot be created');
    throw error;
  }
  try {
    assert.throws(() => discoverSourcePaths(options(root)), /non-UTF-8 entry name in src\/js/);
  } finally {
    fs.unlinkSync(invalidName);
  }
});

test('unsupported filesystem entries fail even when their suffix is not selected', { skip: process.platform === 'win32' }, (t) => {
  const root = makeRoot(t);
  for (const name of ['pipe.js', 'pipe.txt']) {
    const target = path.join(root, 'src/js', name);
    execFileSync('mkfifo', [target]);
    assert.throws(() => discoverSourcePaths(options(root)), /path is not a regular file or directory/);
    fs.unlinkSync(target);
  }
});

test('keeps hardlinked paths separate for the coverage policy to validate', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/app.js');
  fs.linkSync(path.join(root, 'src/js/app.js'), path.join(root, 'src/js/alias.js'));
  assert.deepEqual(discoverSourcePaths(options(root)), ['src/js/alias.js', 'src/js/app.js']);
});

const permissionSkip = process.platform === 'win32' || (process.getuid && process.getuid() === 0)
  ? 'Requires POSIX permissions enforced for an unprivileged user' : false;
test('unreadable roots, directories and files fail rather than returning partial coverage', { skip: permissionSkip }, (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/readable.js');
  write(root, 'src/js/deep/blocked.js');
  write(root, 'src/js/blocked.txt');
  for (const [relative, mode] of [
    ['.', 0o300], ['src/js/deep', 0o300], ['src/js/deep', 0o400],
    ['src/js/deep/blocked.js', 0o200], ['src/js/blocked.txt', 0o200],
  ]) {
    const target = path.join(root, relative);
    const original = fs.statSync(target).mode;
    fs.chmodSync(target, mode);
    try {
      assert.throws(() => discoverSourcePaths(options(root)), (error) => {
        assert.match(error.message, /could not inspect .* \(EACCES\)/);
        assert.equal(error.message.includes(root), false);
        return true;
      });
    } finally {
      fs.chmodSync(target, original);
    }
  }
});

test('coverage rejects an omitted new declaration and accepts its legitimate declared addition', (t) => {
  const root = makeRoot(t);
  write(root, 'src/js/retained.js');
  const original = profile(['retained.js']);
  const check = (candidate) => checkSourceCoverage(candidate, { root, sourcePaths: discoverSourcePaths(options(root)) });
  assert.equal(check(original).ok, true);
  write(root, 'src/js/deep/new.js');
  assert.deepEqual(diagnostics(check(original)), [['coverage-unprofiled-source', 'src/js/deep/new.js']]);
  const candidate = profile(['retained.js', 'deep/new.js']);
  assert.deepEqual(check(candidate), {
    ok: true, sourcePathCount: 2, declaredPathCount: 2, excludedPathCount: 0, violations: [],
  });
  fs.unlinkSync(path.join(root, 'src/js/deep/new.js'));
  assert.deepEqual(diagnostics(check(candidate)), [
    ['coverage-missing-file', 'src/js/deep/new.js'], ['coverage-unselected-declaration', 'src/js/deep/new.js'],
  ]);
  assert.equal(check(original).ok, true);
});

test('discovery leaves exact exclusions to coverage without hiding similarly named new files', (t) => {
  const root = makeRoot(t);
  const reviewed = 'src/js/reviewed.vendor.js';
  for (const file of [reviewed, 'src/js/new.vendor.js', 'src/js/generated/new.js']) write(root, file);
  const report = checkSourceCoverage(profile([]), {
    root, sourcePaths: discoverSourcePaths(options(root)), exclusions: [reviewed],
  });
  assert.deepEqual(diagnostics(report), [
    ['coverage-unprofiled-source', 'src/js/generated/new.js'], ['coverage-unprofiled-source', 'src/js/new.vendor.js'],
  ]);
});
