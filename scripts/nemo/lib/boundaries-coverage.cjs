'use strict';
// Exact-path source coverage, separate from dependency rules and size ratchets.
// The caller owns source discovery and reviewed exclusion provenance. Never seed
// sourcePaths from the candidate/baseline or infer exclusions from file names.

const fs = require('node:fs');
const path = require('node:path');

function invalid(message) {
  throw new Error(`invalid source coverage: ${message}`);
}

function relativePath(value, label, allowDot = false) {
  if (allowDot && value === '.') return;
  if (typeof value !== 'string' || !value.trim() || /[\\\x00-\x1f\x7f]/.test(value)
    || path.posix.isAbsolute(value) || /^[A-Za-z]:/.test(value)
    || value.split('/').some((part) => !part || part === '.' || part === '..')) {
    invalid(`${label} must be a normalized repository-relative path`);
  }
}

function pathCounts(values, label) {
  if (!Array.isArray(values)) invalid(`${label} must be an array`);
  const counts = new Map();
  for (const file of values) {
    relativePath(file, label);
    counts.set(file, (counts.get(file) || 0) + 1);
  }
  return counts;
}

function declarations(profile) {
  if (!profile || !Array.isArray(profile.modules)) invalid('profile.modules must be an array');
  const files = new Map();
  const ids = new Set();
  for (const module of profile.modules) {
    if (!module || typeof module.id !== 'string' || !module.id.trim()) invalid('module.id must be a nonempty string');
    if (ids.has(module.id)) invalid(`duplicate module id ${module.id}`);
    ids.add(module.id);
    relativePath(module.dir, 'module.dir', true);
    if (!Array.isArray(module.files)) invalid('module.files must be an array');
    for (const file of module.files) {
      relativePath(file, 'module.files');
      const sourcePath = path.posix.join(module.dir, file);
      if (!files.has(sourcePath)) files.set(sourcePath, []);
      files.get(sourcePath).push(module.id);
    }
  }
  for (const owners of files.values()) owners.sort();
  return files;
}

function sourceState(root, file) {
  try {
    const physical = fs.realpathSync(path.resolve(root, ...file.split('/')));
    const relative = path.relative(root, physical);
    if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
      return { rule: 'coverage-outside-root', message: 'Source resolves outside the repository root' };
    }
    const stat = fs.statSync(physical, { bigint: true });
    if (!stat.isFile()) {
      return { rule: 'coverage-not-file', message: 'Source path is not a regular file' };
    }
    // Hard links have distinct realpaths; device/inode identifies the file.
    // BigInt preserves identity without rounding large filesystem values.
    return { physical: `${stat.dev}:${stat.ino}` };
  } catch (error) {
    if (['ENOENT', 'ENOTDIR'].includes(error.code)) {
      return { rule: 'coverage-missing-file', message: 'Source path does not exist' };
    }
    // Do not swallow an unreadable tree or leak absolute host paths in errors.
    throw new Error(`source coverage could not inspect ${file} (${error.code || 'filesystem error'})`);
  }
}

/**
 * Compare an independently selected source set with profile.modules' dir/files.
 * @param {object} profile - Uses the existing profile declaration shape. Other
 *   policy fields are left to checkProfile/validateProfile; this is not a parser.
 * @param {object} opts
 * @param {string[]} opts.sourcePaths - Required, normalized POSIX paths relative
 *   to root, including any excluded inputs. No walking or extension filtering.
 * @param {string[]} [opts.exclusions=[]] - Exact paths only; no globs, prefixes,
 *   regexes, content classification, baseline lookup, or built-in vendor policy.
 * @param {string} [opts.root] - Repository root (default cwd).
 *
 * Every declaration and exclusion must belong to the selected set, exist as a
 * regular file inside root, and be unambiguous. Exclusion/declaration overlap and
 * physical aliases fail explicitly; exclusion never transfers to an alias.
 * Empty sets are valid (e.g. after actual deletion). A stale or incomplete source
 * list can omit unknown files: discovery completeness and policy trust must be
 * enforced by the caller. Counts are unique input paths, even on a failed report.
 * Malformed input/uninspectable roots throw; coverage failures return violations
 * with the checker's rule/module/file/line/message/detail fields, sorted by path
 * then rule. Inputs are not mutated. No source content is read or evaluated.
 */
function checkSourceCoverage(profile, opts = {}) {
  if (!opts || typeof opts !== 'object' || Array.isArray(opts)) invalid('options must be an object');
  for (const key of Object.keys(opts)) {
    if (!['root', 'sourcePaths', 'exclusions'].includes(key)) invalid(`unknown option ${key}`);
  }
  const selected = pathCounts(opts.sourcePaths, 'sourcePaths');
  const excluded = pathCounts(opts.exclusions === undefined ? [] : opts.exclusions, 'exclusions');
  const declared = declarations(profile);
  if (opts.root !== undefined && (typeof opts.root !== 'string' || !opts.root.trim())) invalid('root must be a nonempty string');
  let root;
  try {
    root = fs.realpathSync(opts.root === undefined ? process.cwd() : opts.root);
    if (!fs.statSync(root).isDirectory()) invalid('root must be an existing directory');
  } catch {
    invalid('root must be an accessible directory');
  }

  const violations = [];
  const physicalPaths = new Map();
  const add = (rule, file, message, detail = {}) => {
    const owners = declared.get(file) || [];
    violations.push({ rule, module: owners.length === 1 ? owners[0] : null, file, line: null, message, detail });
  };
  const files = [...new Set([...selected.keys(), ...declared.keys(), ...excluded.keys()])].sort();
  for (const file of files) {
    const owners = declared.get(file) || [];
    if (selected.get(file) > 1) add('coverage-duplicate-selection', file, 'Source path is selected more than once', { count: selected.get(file) });
    if (excluded.get(file) > 1) add('coverage-duplicate-exclusion', file, 'Source path is excluded more than once', { count: excluded.get(file) });
    if (owners.length > 1) add('coverage-duplicate-declaration', file, 'Source path is declared more than once', { modules: owners });
    if (selected.has(file) && !owners.length && !excluded.has(file)) {
      add('coverage-unprofiled-source', file, 'Selected source has no profile declaration or exact exclusion');
    }
    if (!selected.has(file) && owners.length) add('coverage-unselected-declaration', file, 'Declared source is absent from the selected source set');
    if (!selected.has(file) && excluded.has(file)) add('coverage-unselected-exclusion', file, 'Excluded source is absent from the selected source set');
    if (excluded.has(file) && owners.length) add('coverage-excluded-declaration', file, 'Source is both declared and excluded');

    const state = sourceState(root, file);
    if (state.rule) add(state.rule, file, state.message);
    else {
      if (!physicalPaths.has(state.physical)) physicalPaths.set(state.physical, []);
      physicalPaths.get(state.physical).push(file);
    }
  }
  for (const aliases of physicalPaths.values()) {
    if (aliases.length > 1) add('coverage-aliased-source', aliases[0], 'Multiple paths resolve to the same physical source', { paths: aliases });
  }
  const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;
  violations.sort((a, b) => compare(a.file, b.file) || compare(a.rule, b.rule));
  return {
    ok: violations.length === 0,
    sourcePathCount: selected.size,
    declaredPathCount: declared.size,
    excludedPathCount: excluded.size,
    violations,
  };
}

module.exports = { checkSourceCoverage };
