'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');

function fault(code) { return Object.assign(new Error(code), { code }); }
function git(root, args) {
  // Ambient Git overrides must not redirect this explicit repository/index.
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_')));
  const result = spawnSync('git', ['--no-optional-locks', '-C', root, ...args], {
    env, encoding: 'buffer', maxBuffer: 128 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) throw fault('git-command-failed');
  return result.stdout;
}
function records(bytes) {
  if (!bytes.length) return [];
  if (bytes.at(-1) !== 0) throw fault('invalid-git-output');
  const result = [];
  let start = 0;
  for (let i = 0; i < bytes.length; i++) {
    if (bytes[i] === 0) { result.push(bytes.subarray(start, i)); start = i + 1; }
  }
  return result;
}
function filename(bytes) {
  const name = bytes.toString('utf8');
  if (!Buffer.from(name).equals(bytes)) throw fault('non-utf8-path');
  if (!name || path.isAbsolute(name) || name.split('/').some((part) => !part || part === '.' || part === '..')
    || (process.platform === 'win32' && /[\\:]/.test(name))) throw fault('unsafe-git-path');
  return name;
}
function candidates(root, diagnose) {
  const found = new Map();
  const add = (bytes, index) => {
    let name;
    try { name = filename(bytes); } catch (error) {
      diagnose(error.code, null, { pathBytesHex: bytes.toString('hex') }); return;
    }
    if (!found.has(name)) found.set(name, { path: name, tracked: false, index: [] });
    const entry = found.get(name);
    if (index) { entry.tracked = true; entry.index.push(index); }
  };
  for (const record of records(git(root, ['ls-files', '--cached', '--stage', '-z']))) {
    const tab = record.indexOf(9);
    const match = record.subarray(0, tab).toString('ascii').match(/^([0-7]{6}) ([a-f0-9]{40}|[a-f0-9]{64}) ([0-3])$/);
    if (tab < 0 || !match) throw fault('invalid-index-record');
    add(record.subarray(tab + 1), { mode: match[1], oid: match[2], stage: Number(match[3]) });
  }
  for (const record of records(git(root, ['ls-files', '--others', '--exclude-standard', '-z']))) add(record);
  return [...found.values()].sort((a, b) => Buffer.compare(Buffer.from(a.path), Buffer.from(b.path)));
}
function inspect(root, relative) {
  let current = root;
  const parts = relative.split('/');
  for (let i = 0; i < parts.length; i++) {
    current = path.join(current, parts[i]);
    const stat = fs.lstatSync(current, { bigint: true });
    if (stat.isSymbolicLink()) {
      if (i < parts.length - 1) throw fault('symlink-ancestor');
      return { stat, absolute: current, kind: 'symlink' };
    }
    if (i < parts.length - 1 && !stat.isDirectory()) throw fault('unsupported-ancestor');
    if (i === parts.length - 1) return {
      stat, absolute: current, kind: stat.isFile() ? 'file' : stat.isDirectory() ? 'directory' : 'unsupported',
    };
  }
}
function same(a, b) {
  return ['dev', 'ino', 'mode', 'size', 'mtimeNs', 'ctimeNs'].every((key) => a[key] === b[key]);
}
function digest(root, entry, state) {
  if (fs.constants.O_NOFOLLOW === undefined) throw fault('nofollow-unavailable');
  const fd = fs.openSync(state.absolute, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  try {
    const before = fs.fstatSync(fd, { bigint: true });
    if (!before.isFile() || !same(state.stat, before) || !same(inspect(root, entry.path).stat, before)) {
      throw fault('working-tree-changed');
    }
    const hash = createHash('sha256');
    const buffer = Buffer.allocUnsafe(64 * 1024);
    let length;
    while ((length = fs.readSync(fd, buffer, 0, buffer.length, null))) hash.update(buffer.subarray(0, length));
    if (!same(before, fs.fstatSync(fd, { bigint: true })) || !same(before, inspect(root, entry.path).stat)) {
      throw fault('working-tree-changed');
    }
    return hash.digest('hex');
  } finally { fs.closeSync(fd); }
}

/**
 * Synchronous Git candidate inventory: discoverRepositoryFiles({root}).
 * Returns {head, entries, diagnostics, ok}; entries have {path, tracked, index,
 * workingTree: {kind, mode}, sha256?}. Index mode/OID/stage describe the index,
 * never the current bytes. SHA256 hashes current regular-file bytes only.
 * All tracked paths (including tracked ignored files) and nonignored untracked
 * paths are candidates, with no root/suffix/profile/vendor/generated policy.
 * Untracked ignored files and empty directories are not Git candidates.
 * Paths are repository-relative, byte-order sorted UTF-8; unrepresentable names
 * fail explicitly with hex bytes. No absolute host paths or raw Git stderr leak.
 * Missing paths, conflicts, symlinks/ancestors, gitlinks and unsupported types
 * make ok=false. Unsafe candidates are not read; symlink targets are not read.
 * A root alias is canonicalized; a subtree, bare repo, or absent HEAD fails.
 *
 * This is NOT an atomic snapshot: keep the repository/index stable during use.
 * Descriptor identity/change checks and O_NOFOLLOW protect ordinary replacements;
 * concurrent hostile ancestor swaps require OS sandboxing/openat-style traversal,
 * which this portable Node API does not provide. HEAD does not identify dirty bytes.
 *
 * Handoff only an ok report to existing coverage with independently reviewed
 * classification/exclusions. Coverage currently rejects some legal Git filenames;
 * do not silently drop/normalize those names or treat inventory as coverage/adoption.
 * The existing filesystem discovery helper and all policy consumers are unchanged.
 */
function discoverRepositoryFiles(options) {
  const report = { head: null, entries: [], diagnostics: [], ok: false };
  const diagnose = (code, file = null, extra = {}) => report.diagnostics.push({ code, path: file, ...extra });
  try {
    if (!options || typeof options !== 'object' || Array.isArray(options)
      || Object.keys(options).some((key) => key !== 'root')
      || typeof options.root !== 'string' || !options.root || options.root.includes('\0')) throw fault('invalid-options');
    const root = fs.realpathSync(options.root);
    if (!fs.statSync(root).isDirectory()) throw fault('invalid-root');
    // --show-prefix is empty only at the worktree root (no newline-sensitive paths).
    if (git(root, ['rev-parse', '--is-inside-work-tree']).toString() !== 'true\n'
      || git(root, ['rev-parse', '--show-prefix']).toString() !== '\n') throw fault('root-not-worktree-top');
    const head = git(root, ['rev-parse', '--verify', 'HEAD']).toString().trim();
    if (!/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(head)) throw fault('invalid-head');
    report.head = head;
    report.entries = candidates(root, diagnose);
    for (const entry of report.entries) {
      entry.workingTree = { kind: 'unknown', mode: null };
      const conflict = entry.index.some((item) => item.stage !== 0);
      const gitlink = entry.index.some((item) => item.mode === '160000');
      const indexedSymlink = entry.index.some((item) => item.mode === '120000');
      if (conflict) diagnose('unmerged-index', entry.path);
      if (gitlink) diagnose('gitlink', entry.path);
      if (indexedSymlink) diagnose('indexed-symlink', entry.path);
      try {
        const state = inspect(root, entry.path);
        entry.workingTree = { kind: state.kind, mode: Number(state.stat.mode & 0o7777n).toString(8).padStart(4, '0') };
        if (state.kind !== 'file') diagnose(state.kind === 'symlink' ? 'symlink' : 'unsupported-type', entry.path);
        if (state.kind === 'file' && !conflict && !gitlink && !indexedSymlink) entry.sha256 = digest(root, entry, state);
      } catch (error) {
        const missing = error.code === 'ENOENT';
        if (missing) entry.workingTree.kind = 'missing';
        diagnose(missing ? (entry.tracked ? 'missing-indexed-path' : 'working-tree-changed') : error.code || 'filesystem-error', entry.path);
      }
    }
  } catch (error) { diagnose(error.code || 'repository-error'); }
  report.ok = report.diagnostics.length === 0;
  return report;
}

module.exports = { discoverRepositoryFiles };
