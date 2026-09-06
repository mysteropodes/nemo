'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const { discoverRepositoryFiles } = require('./lib/boundaries-repository.cjs');

function command(root, args, input) {
  const result = spawnSync('git', ['-C', root, ...args], { input, encoding: 'utf8',
    env: Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_'))) });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}
function fixture(t) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-repository-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  command(root, ['init', '-q']);
  command(root, ['config', 'user.name', 'Fixture']);
  command(root, ['config', 'user.email', 'fixture@example.invalid']);
  command(root, ['-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-qm', 'fixture']);
  return root;
}
function write(root, name, bytes = name) {
  fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true });
  fs.writeFileSync(path.join(root, name), bytes);
}
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const scan = (root) => discoverRepositoryFiles({ root });
const codes = (report, name) => report.diagnostics.filter((item) => item.path === name).map((item) => item.code);

test('all roots and suffixes, hidden/vendor/generated names, ignored distinctions and current bytes', (t) => {
  const root = fixture(t);
  write(root, '.gitignore', '*.ignored\ncache/\n');
  const tracked = ['elsewhere/a.rs', 'skins/theme.css', 'novel/file.unheard', '.hidden', 'vendor/a.bin', 'generated/a', 'keep.ignored'];
  for (const name of tracked) write(root, name, 'indexed');
  command(root, ['add', '-f', '--', '.gitignore', ...tracked]);
  write(root, 'skip.ignored');
  write(root, 'cache/excluded');
  write(root, 'new/location.no-known-suffix', Buffer.from([0, 255, 10]));
  write(root, 'elsewhere/a.rs', 'current');
  const result = scan(root);
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.equal(result.head, command(root, ['rev-parse', 'HEAD']));
  assert.deepEqual(result.entries.map((e) => e.path), [...tracked, '.gitignore', 'new/location.no-known-suffix'].sort());
  const entry = result.entries.find((e) => e.path === 'elsewhere/a.rs');
  assert.equal(entry.sha256, sha('current'));
  assert.deepEqual(entry.index, [{ mode: '100644', oid: command(root, ['hash-object', '--stdin'], 'indexed'), stage: 0 }]);
  assert.equal(entry.workingTree.kind, 'file');
  const untracked = result.entries.find((e) => !e.tracked);
  assert.deepEqual(untracked.index, []);
  assert.equal(untracked.sha256, sha(Buffer.from([0, 255, 10])));
  assert.deepEqual(scan(root), result);
  assert.ok(!JSON.stringify(result).includes(root));
});

test('NUL parsing preserves tabs, newlines, quotes, unicode, spaces and option-like names', (t) => {
  const root = fixture(t);
  const names = ['-dash', 'tab\there', 'line\nbreak', 'quote"file', ' spaced ', '雪.unknown'];
  if (process.platform !== 'win32') names.push('back\\slash');
  for (const name of names) write(root, name);
  command(root, ['add', '--', ...names.slice(0, 3)]);
  const result = scan(root);
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.deepEqual(result.entries.map((e) => e.path), names.sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b))));
  for (const entry of result.entries) assert.equal(entry.sha256, sha(entry.path));
});

test('requires explicit top-level Git worktree, rejects policy options and absent HEAD', (t) => {
  const root = fixture(t);
  fs.mkdirSync(path.join(root, 'subtree'));
  for (const options of [undefined, {}, { root, extensions: ['.js'] }, { root, sourceRoots: ['.'] }]) {
    assert.equal(discoverRepositoryFiles(options).ok, false);
  }
  assert.ok(codes(scan(path.join(root, 'subtree')), null).includes('root-not-worktree-top'));
  assert.equal(scan(path.join(root, 'absent')).ok, false);
  const empty = path.join(root, 'empty');
  fs.mkdirSync(empty);
  command(empty, ['init', '-q']);
  assert.equal(scan(empty).ok, false);
  const plain = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-not-git-'));
  t.after(() => fs.rmSync(plain, { recursive: true, force: true }));
  assert.equal(scan(plain).ok, false);
  command(plain, ['init', '--bare', '-q']);
  assert.equal(scan(plain).ok, false);
});

test('missing indexed files remain candidates, including skip-worktree and assume-unchanged', (t) => {
  const root = fixture(t);
  for (const name of ['missing', 'sparse', 'assumed']) write(root, name);
  command(root, ['add', '--', 'missing', 'sparse', 'assumed']);
  command(root, ['update-index', '--skip-worktree', 'sparse']);
  command(root, ['update-index', '--assume-unchanged', 'assumed']);
  for (const name of ['missing', 'sparse', 'assumed']) fs.unlinkSync(path.join(root, name));
  const result = scan(root);
  assert.equal(result.ok, false);
  assert.equal(result.entries.length, 3);
  for (const entry of result.entries) {
    assert.ok(codes(result, entry.path).includes('missing-indexed-path'));
    assert.equal(entry.workingTree.kind, 'missing');
    assert.equal(entry.sha256, undefined);
  }
});

test('working-tree permissions are separate from index mode', { skip: process.platform === 'win32' }, (t) => {
  const root = fixture(t);
  write(root, 'mode');
  command(root, ['add', 'mode']);
  fs.chmodSync(path.join(root, 'mode'), 0o755);
  const result = scan(root);
  assert.equal(result.ok, true);
  assert.equal(result.entries[0].index[0].mode, '100644');
  assert.equal(result.entries[0].workingTree.mode, '0755');
});

test('unmerged index stages are retained and fail explicitly without hashing', (t) => {
  const root = fixture(t);
  write(root, 'conflict', 'working');
  const oids = ['base', 'ours', 'theirs'].map((text) => command(root, ['hash-object', '-w', '--stdin'], text));
  command(root, ['update-index', '--index-info'], oids.map((oid, i) => `100644 ${oid} ${i + 1}\tconflict\n`).join(''));
  const result = scan(root);
  assert.equal(result.ok, false);
  assert.deepEqual(result.entries[0].index.map((s) => s.stage), [1, 2, 3]);
  assert.deepEqual(result.entries[0].index.map((s) => s.oid), oids);
  assert.ok(codes(result, 'conflict').includes('unmerged-index'));
  assert.equal(result.entries[0].sha256, undefined);
});

test('symlink leaves, ancestors, indexed symlinks and gitlinks never read target bytes', { skip: process.platform === 'win32' }, (t) => {
  const root = fixture(t);
  const outside = fixture(t);
  write(outside, 'secret', 'outside sentinel');
  write(root, 'ancestor/secret');
  command(root, ['add', '--', 'ancestor/secret']);
  fs.rmSync(path.join(root, 'ancestor'), { recursive: true });
  fs.symlinkSync(outside, path.join(root, 'ancestor'));
  fs.symlinkSync(path.join(outside, 'secret'), path.join(root, 'leaf'));
  fs.symlinkSync('absent', path.join(root, 'dangling'));
  fs.symlinkSync(path.join(outside, 'secret'), path.join(root, 'indexed'));
  command(root, ['add', '--', 'indexed']);
  fs.unlinkSync(path.join(root, 'indexed'));
  write(root, 'indexed', 'now regular');
  const head = command(root, ['rev-parse', 'HEAD']);
  command(root, ['update-index', '--add', '--cacheinfo', `160000,${head},submodule`]);
  write(root, 'submodule', 'replacement regular');
  const original = fs.readSync;
  const reads = [];
  t.mock.method(fs, 'readSync', function (fd, ...args) {
    reads.push(fs.fstatSync(fd).ino);
    return original.call(fs, fd, ...args);
  });
  const result = scan(root);
  assert.equal(result.ok, false);
  for (const [name, code] of [['ancestor/secret', 'symlink-ancestor'], ['leaf', 'symlink'], ['dangling', 'symlink'], ['indexed', 'indexed-symlink'], ['submodule', 'gitlink']]) {
    assert.ok(codes(result, name).includes(code), JSON.stringify(result));
    assert.equal(result.entries.find((e) => e.path === name).sha256, undefined);
  }
  assert.ok(!reads.includes(fs.statSync(path.join(outside, 'secret')).ino));
  assert.equal(reads.length, 0);
});

test('indexed directory and FIFO replacements fail without blocking', { skip: process.platform === 'win32' }, (t) => {
  const root = fixture(t);
  for (const name of ['directory', 'fifo']) write(root, name);
  command(root, ['add', '--', 'directory', 'fifo']);
  fs.unlinkSync(path.join(root, 'directory'));
  fs.mkdirSync(path.join(root, 'directory'));
  fs.unlinkSync(path.join(root, 'fifo'));
  assert.equal(spawnSync('mkfifo', [path.join(root, 'fifo')]).status, 0);
  const result = scan(root);
  assert.equal(result.ok, false);
  for (const name of ['directory', 'fifo']) assert.ok(codes(result, name).includes('unsupported-type'));
});

test('non-UTF8 Git paths fail explicitly without aliasing a valid replacement character', { skip: process.platform === 'win32' }, (t) => {
  const root = fixture(t);
  const oid = command(root, ['hash-object', '-w', '--stdin'], 'bad name');
  command(root, ['update-index', '-z', '--index-info'], Buffer.concat([Buffer.from(`100644 ${oid}\t`), Buffer.from([255, 0])]));
  write(root, '\uFFFD', 'valid name');
  const result = scan(root);
  assert.equal(result.ok, false);
  assert.ok(result.diagnostics.some((d) => d.code === 'non-utf8-path' && d.pathBytesHex === 'ff'));
  assert.equal(result.entries[0].sha256, sha('valid name'));
});

test('ambient Git directory/index overrides do not redirect discovery; linked worktree works', (t) => {
  const root = fixture(t);
  const other = fixture(t);
  write(root, 'wanted');
  write(other, 'unwanted');
  const overrides = { GIT_DIR: path.join(other, '.git'), GIT_WORK_TREE: other, GIT_INDEX_FILE: path.join(other, '.git/index') };
  for (const [key, value] of Object.entries(overrides)) {
    const previous = process.env[key];
    t.after(() => { if (previous === undefined) delete process.env[key]; else process.env[key] = previous; });
    process.env[key] = value;
  }
  assert.deepEqual(scan(root).entries.map((e) => e.path), ['wanted']);
  const linked = path.join(other, 'linked');
  command(root, ['worktree', 'add', '--detach', linked, 'HEAD']);
  assert.equal(scan(linked).ok, true);
});

test('read errors and concurrent content changes fail without publishing a digest', (t) => {
  const root = fixture(t);
  write(root, 'candidate', 'before');
  const original = fs.readSync;
  let changed = false;
  const mock = t.mock.method(fs, 'readSync', function (...args) {
    const count = original.apply(fs, args);
    if (!changed) { changed = true; write(root, 'candidate', 'changed and longer'); }
    return count;
  });
  let result = scan(root);
  assert.ok(codes(result, 'candidate').includes('working-tree-changed'));
  assert.equal(result.entries[0].sha256, undefined);
  mock.mock.restore();
  t.mock.method(fs, 'readSync', () => { throw Object.assign(new Error('private host path must not leak'), { code: 'EACCES' }); });
  result = scan(root);
  assert.equal(result.ok, false);
  assert.ok(codes(result, 'candidate').includes('EACCES'));
  assert.equal(result.entries[0].sha256, undefined);
  assert.ok(!JSON.stringify(result).includes('private host'));
});

test('leaf swapped to outside symlink immediately before open is not read', { skip: process.platform === 'win32' }, (t) => {
  const root = fixture(t);
  const outside = fixture(t);
  write(outside, 'secret');
  write(root, 'candidate');
  const original = fs.openSync;
  t.mock.method(fs, 'openSync', function (file, ...args) {
    if (file === path.join(root, 'candidate')) {
      fs.unlinkSync(file);
      fs.symlinkSync(path.join(outside, 'secret'), file);
    }
    return original.call(fs, file, ...args);
  });
  const read = t.mock.method(fs, 'readSync', () => assert.fail('must not read outside bytes'));
  const result = scan(root);
  assert.equal(result.ok, false);
  assert.equal(result.entries[0].sha256, undefined);
  assert.equal(read.mock.callCount(), 0);
});

test('repository exclude rules apply only to untracked candidates', (t) => {
  const root = fixture(t);
  write(root, 'retained');
  command(root, ['add', 'retained']);
  fs.appendFileSync(path.join(root, '.git/info/exclude'), '\nretained\nexcluded\n');
  write(root, 'excluded');
  assert.deepEqual(scan(root).entries.map((e) => e.path), ['retained']);
});
