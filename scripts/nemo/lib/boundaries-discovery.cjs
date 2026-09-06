'use strict';

const fs = require('node:fs');
const path = require('node:path');

function invalid(message) {
  throw new Error(`invalid source discovery: ${message}`);
}

function relativePath(value, label) {
  if (value === '.') return;
  if (typeof value !== 'string' || !value.trim() || /[\\\x00-\x1f\x7f]/.test(value)
    || path.posix.isAbsolute(value) || /^[A-Za-z]:/.test(value)
    || value.split('/').some((part) => !part || part === '.' || part === '..')) {
    invalid(`${label} must be a normalized repository-relative path`);
  }
}

function validateOptions(opts) {
  if (!opts || typeof opts !== 'object' || Array.isArray(opts)) invalid('options must be an object');
  for (const key of Object.keys(opts)) {
    if (!['root', 'sourceRoots', 'extensions'].includes(key)) invalid(`unknown option ${key}`);
  }
  if (typeof opts.root !== 'string' || !opts.root.trim() || /[\x00-\x1f\x7f]/.test(opts.root)) {
    invalid('root must be an explicit nonempty directory path');
  }
  if (!Array.isArray(opts.sourceRoots) || !opts.sourceRoots.length) invalid('sourceRoots must be a nonempty array');
  for (const dir of opts.sourceRoots) relativePath(dir, 'sourceRoots');
  if (!Array.isArray(opts.extensions) || !opts.extensions.length) invalid('extensions must be a nonempty array');
  for (const extension of opts.extensions) {
    if (typeof extension !== 'string' || !/^(?:\.[A-Za-z0-9_-]+)+$/.test(extension)) {
      invalid('extensions must be literal dot-prefixed suffixes (for example .ts or .d.ts)');
    }
  }
}

// Keep filesystem failures explicit without exposing absolute host paths.
function inspect(label, action) {
  try {
    return action();
  } catch (error) {
    throw new Error(`source discovery could not inspect ${label} (${error.code || 'filesystem error'})`);
  }
}

function entryState(root, relative) {
  const absolute = path.resolve(root, ...relative.split('/'));
  const stat = inspect(relative, () => fs.lstatSync(absolute));
  if (stat.isSymbolicLink()) invalid(`symlink traversal is not supported: ${relative}`);
  const physical = inspect(relative, () => fs.realpathSync(absolute));
  const inside = path.relative(root, physical);
  if (inside === '..' || inside.startsWith(`..${path.sep}`) || path.isAbsolute(inside)) {
    invalid(`path resolves outside root: ${relative}`);
  }
  if (!stat.isDirectory() && !stat.isFile()) invalid(`path is not a regular file or directory: ${relative}`);
  inspect(relative, () => fs.accessSync(absolute, fs.constants.R_OK | (stat.isDirectory() ? fs.constants.X_OK : 0)));
  return { absolute, stat };
}

function validateSourceRoot(root, relative) {
  // Inspect every component: lstat on just the final directory would follow a
  // symlink in an ancestor. Validate even roots already covered by another root.
  let current = '.';
  for (const part of relative === '.' ? [] : relative.split('/')) {
    current = current === '.' ? part : `${current}/${part}`;
    if (!entryState(root, current).stat.isDirectory()) invalid(`source root must be a directory: ${current}`);
  }
}

function directoryNames(absolute, relative) {
  // Decoding an invalid filename to U+FFFD could alias a different real entry
  // and silently omit the original. Reject names that cannot round-trip.
  return inspect(relative, () => fs.readdirSync(absolute, { encoding: 'buffer' })).map((bytes) => {
    const name = bytes.toString('utf8');
    if (!Buffer.from(name, 'utf8').equals(bytes)) invalid(`non-UTF-8 entry name in ${relative}`);
    return name;
  }).sort();
}

/**
 * Discover actual working-tree sources, independent of declarations and Git.
 * @param {object} opts - All three fields are required; no implicit policy.
 * @param {string} opts.root - Existing readable/searchable repository directory.
 *   Canonicalized as the trust boundary, so an explicitly symlinked root works.
 * @param {string[]} opts.sourceRoots - Nonempty array of normalized relative
 *   POSIX directory paths; '.' selects root. Every root is validated, including
 *   duplicates and overlaps. Symlinks below root are rejected, never followed.
 * @param {string[]} opts.extensions - Nonempty array of case-sensitive literal
 *   suffixes, each made of dot-prefixed alphanumeric/underscore/hyphen segments.
 *   '.ts' includes '.d.ts'; '.d.ts' alone selects only that compound suffix.
 * @returns {string[]} Sorted, unique, normalized relative POSIX file paths.
 *
 * Traverses untracked/ignored/hidden/vendor/generated names equally, with no
 * exclusions. Unreadable entries, unsupported file types and paths that cannot
 * be represented in the output contract throw, even with an unmatched suffix.
 * No contents are read or evaluated. Inputs are not mutated; each call walks
 * afresh. This is a synchronous scan, not an atomic snapshot of concurrent edits.
 */
function discoverSourcePaths(opts) {
  validateOptions(opts);
  const root = inspect('root', () => fs.realpathSync(opts.root));
  if (!entryState(root, '.').stat.isDirectory()) invalid('root must be a directory');
  for (const dir of opts.sourceRoots) validateSourceRoot(root, dir);

  const pending = [...new Set(opts.sourceRoots)].sort().reverse();
  const visited = new Set();
  const sources = new Set();
  while (pending.length) {
    const dir = pending.pop();
    if (visited.has(dir)) continue;
    const state = entryState(root, dir);
    if (!state.stat.isDirectory()) invalid(`source root must be a directory: ${dir}`);
    const names = directoryNames(state.absolute, dir);
    visited.add(dir);
    for (const name of names) {
      const relative = dir === '.' ? name : `${dir}/${name}`;
      relativePath(relative, 'entry');
      const entry = entryState(root, relative);
      if (entry.stat.isDirectory()) pending.push(relative);
      else if (opts.extensions.some((extension) => name.endsWith(extension))) sources.add(relative);
    }
  }
  return [...sources].sort();
}

module.exports = { discoverSourcePaths };
