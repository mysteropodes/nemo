'use strict';
// Temporary adoption seam: size-only checks over explicitly declared UTF-8 text.
// No lexical/dependency analysis, content execution, or scanner-derived waivers.
// Callers MUST separately require complete coverage, provenance, compareSizeBaseline,
// and graph/language checks where applicable. This API cannot adopt policy, replace
// aggregation, or establish language-independent dependency acceptance. The application
// boundary lane uses this helper for size alongside its independent policy checks.
const fs = require('node:fs');
const path = require('node:path');
const { TextDecoder } = require('node:util');
const { validateProfile, countNonBlankLines } = require('./boundaries.cjs');

// Resolve the caller's root once (including macOS /tmp aliases). Below that root,
// reject every symlink, even internal ones: exact-path ownership must be unambiguous.
// Preflight ALL paths before reading contents; directories/devices/FIFOs are not text.
// The caller must provide a stable checkout; this is not a concurrent filesystem sandbox.
function sourceFiles(profile, root) {
  const files = [], physical = new Set();
  for (const module of profile.modules) for (const file of module.files) {
    const relative = path.posix.join(module.dir, file);
    if (/[\x00-\x1f\x7f:]/.test(relative)) throw new Error('invalid profile: unsafe source path');
    let absolute = root, stat;
    const components = relative.split('/');
    for (const [index, component] of components.entries()) {
      absolute = path.join(absolute, component);
      stat = fs.lstatSync(absolute);
      if (stat.isSymbolicLink()) throw new Error('invalid profile: symlink source paths are unsupported');
      if (index < components.length - 1 && !stat.isDirectory()) throw new Error('invalid profile: source parent must be a directory');
    }
    if (!stat.isFile()) throw new Error('invalid profile: source must be a regular file');
    const identity = `${stat.dev}:${stat.ino}`;
    if (physical.has(identity)) throw new Error('invalid profile: multiple files refer to the same physical source');
    physical.add(identity);
    files.push({ module, relative, absolute });
  }
  return files;
}

/** Size/expired-exception report used by checkApplicationSize, without a
 * JS scanner. Expired exceptions of ALL rules remain violations; only size
 * exceptions are applied. Counts include nonblank comments, with LF/CRLF parity.
 * @param {object} profile - engineering/boundaries/profile.schema.json
 * @param {object} [opts]
 * @param {string} [opts.root] - existing source root (default cwd)
 * @param {Date} [opts.now] - exception clock (default current date)
 */
function checkSourceSizes(profile, opts = {}) {
  validateProfile(profile);
  const now = opts.now === undefined ? new Date() : opts.now;
  if (!(now instanceof Date) || !Number.isFinite(now.getTime())) throw new Error('invalid check clock');
  const requestedRoot = opts.root === undefined ? process.cwd() : opts.root;
  if (typeof requestedRoot !== 'string' || !requestedRoot.trim()) throw new Error('invalid source root');
  const root = fs.realpathSync(requestedRoot);
  if (!fs.statSync(root).isDirectory()) throw new Error('invalid source root: must be a directory');
  const files = sourceFiles(profile, root);
  const violations = [], warnings = [], exceptionsApplied = [], active = new Map();
  for (const exception of profile.exceptions || []) {
    if (new Date(exception.expires) <= now) {
      violations.push({ rule: 'expired-exception', module: null, file: exception.path, line: null,
        message: `Exception for "${exception.rule}" on ${exception.path} expired ${exception.expires}`,
        detail: { exception } });
    } else if (exception.rule === 'size') active.set(exception.path, exception);
  }
  const decoder = new TextDecoder('utf-8', { fatal: true });
  for (const { module, relative, absolute } of files) {
    const lines = countNonBlankLines(decoder.decode(fs.readFileSync(absolute)));
    const limits = profile.sizeProfiles[module.sizeProfile], exception = active.get(relative);
    const location = { rule: 'size', module: module.id, file: relative, line: null };
    if (exception) {
      exceptionsApplied.push({ ...exception, actualLines: lines });
      if (lines > exception.ceiling) violations.push({ ...location,
        message: `${lines} nonblank lines exceeds excepted ceiling ${exception.ceiling}`,
        detail: { lines, ceiling: exception.ceiling, exception: true } });
    } else if (lines > limits.hardMax) {
      violations.push({ ...location,
        message: `${lines} nonblank lines exceeds hard maximum ${limits.hardMax} for profile "${module.sizeProfile}"`,
        detail: { lines, hardMax: limits.hardMax } });
    } else if (lines > limits.warn) {
      warnings.push({ ...location,
        message: `${lines} nonblank lines exceeds warn threshold ${limits.warn} for profile "${module.sizeProfile}"`,
        detail: { lines, warn: limits.warn } });
    }
  }
  return { ok: violations.length === 0, violations, warnings, exceptionsApplied };
}
module.exports = { checkSourceSizes };
