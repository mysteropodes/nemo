'use strict';
// Self-contained module-profile / dependency rule checker — R05 first increment
// (engineering/remediation/04_MODULARITY_POLICY.md).
//
// This checks a hand- or tool-authored *profile* (a bounded list of modules the
// caller is reviewing), not the whole application. It has no dependency on the
// R01 current/target inventory or the R03 `scripts/nemo/inventory.cjs` scan —
// see engineering/boundaries/README.md for the exact contract those work
// packages should satisfy so their output can be handed to `checkProfile`
// directly instead of a hand-authored profile.
//
// Rules implemented:
//   cycle             — an import cycle between two or more declared modules.
//   private-import    — a module reached through a file its owning module does
//                        not list in `publicApi` (a deep/back-door import).
//   layer-violation   — an import that crosses into a layer the importer's
//                        layer is not allowed to depend on (`layerRules`).
//   global-state      — `window.SM*` access from a layer other than
//                        `adapters`/`bootstrap` (the legacy global bypass the
//                        policy calls out).
//   size              — nonblank physical line count over the module's
//                        `sizeProfile` hard maximum, honoring a non-expired
//                        exception's raised ceiling.
//   expired-exception — an exception entry whose `expires` date is on/before
//                        the check's `now`; it stops shielding its rule and is
//                        itself reported as a violation.
//
// Known limitations (v1, intentionally not solved here): import extraction is
// regex-based (no real parser), so a specifier inside a comment or string
// literal that happens to look like `require('x')` would be misread; bare
// (non-relative) specifiers are treated as external and never checked. Both
// are acceptable for a bounded, hand-authored profile and are expected to be
// superseded once R01/R03 supply a real parsed inventory.

const fs = require('node:fs');
const path = require('node:path');

const GLOBAL_SM_RE = /\bwindow\.(SM[A-Za-z0-9_]*)\b/g;

// require('x') / require("x")
const REQUIRE_RE = /require\(\s*(['"])([^'"]+)\1\s*\)/g;
// import ... from 'x' (also matches multi-line named imports)
const IMPORT_FROM_RE = /\bimport\s+[^'";]*?\sfrom\s+(['"])([^'"]+)\1/g;
// import 'x' (side-effect only)
const IMPORT_BARE_RE = /\bimport\s+(['"])([^'"]+)\1/g;
// export ... from 'x'
const EXPORT_FROM_RE = /\bexport\s+[^'";]*?\sfrom\s+(['"])([^'"]+)\1/g;
// dynamic import('x')
const DYNAMIC_IMPORT_RE = /\bimport\(\s*(['"])([^'"]+)\1\s*\)/g;

function lineAt(text, index) {
  let line = 1;
  for (let i = 0; i < index && i < text.length; i++) if (text.charCodeAt(i) === 10) line++;
  return line;
}

/** Extract `{ specifier, line }` for every static/dynamic import-like reference. */
function extractImports(source) {
  const out = [];
  for (const re of [REQUIRE_RE, IMPORT_FROM_RE, IMPORT_BARE_RE, EXPORT_FROM_RE, DYNAMIC_IMPORT_RE]) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(source))) out.push({ specifier: m[2], line: lineAt(source, m.index) });
  }
  return out;
}

function countNonBlankLines(source) {
  const lines = source.split(/\r?\n/);
  if (lines.length && lines[lines.length - 1] === '') lines.pop();
  return lines.filter((l) => l.trim().length > 0).length;
}

function toPosix(p) {
  return p.split(path.sep).join('/');
}

/** Resolve a relative specifier against the file that imported it. Returns an
 * absolute path that exists on disk, or null (external, or not found). */
function resolveSpecifier(specifier, fromFile) {
  if (!specifier.startsWith('.') && !specifier.startsWith('/')) return null;
  const base = path.resolve(path.dirname(fromFile), specifier);
  const candidates = [
    base,
    `${base}.cjs`, `${base}.js`, `${base}.mjs`,
    path.join(base, 'index.cjs'), path.join(base, 'index.js'), path.join(base, 'index.mjs'),
  ];
  for (const c of candidates) {
    try {
      if (fs.statSync(c).isFile()) return c;
    } catch { /* try next candidate */ }
  }
  return null;
}

/** Build absPath -> { module, relFile, isPublic } for every file every module declares. */
function buildFileIndex(profile, root) {
  const index = new Map();
  for (const m of profile.modules) {
    for (const f of m.files) {
      const abs = path.resolve(root, m.dir, f);
      index.set(abs, { module: m, relFile: toPosix(path.join(m.dir, f)), isPublic: (m.publicApi || []).includes(f) });
    }
  }
  return index;
}

function findException(profile, relPath, rule) {
  return (profile.exceptions || []).find((e) => e.path === relPath && e.rule === rule);
}

/** Directed-graph cycle detection (DFS with a recursion stack). Returns one
 * reported cycle (list of module ids, first repeated at the end) per back-edge
 * found; the same simple cycle is not reported twice. */
function findCycles(edges) {
  const state = new Map(); // undefined=unvisited, 1=visiting, 2=done
  const stack = [];
  const cycles = [];
  function dfs(node) {
    state.set(node, 1);
    stack.push(node);
    for (const next of edges.get(node) || []) {
      if (state.get(next) === 1) {
        const idx = stack.indexOf(next);
        cycles.push(stack.slice(idx).concat(next));
      } else if (!state.get(next)) {
        dfs(next);
      }
    }
    stack.pop();
    state.set(node, 2);
  }
  for (const node of edges.keys()) if (!state.get(node)) dfs(node);
  return cycles;
}

/**
 * Run every rule over `profile` and return a report.
 * @param {object} profile - see engineering/boundaries/profile.schema.json
 * @param {object} [opts]
 * @param {string} [opts.root] - base dir module `dir`s are relative to (default cwd)
 * @param {Date}   [opts.now]  - clock used for exception expiry (default: real now)
 */
function checkProfile(profile, opts = {}) {
  const root = opts.root || process.cwd();
  const now = opts.now || new Date();
  const fileIndex = buildFileIndex(profile, root);
  const violations = [];
  const warnings = [];
  const exceptionsApplied = [];
  const edges = new Map(profile.modules.map((m) => [m.id, new Set()]));

  for (const m of profile.modules) {
    for (const f of m.files) {
      const abs = path.resolve(root, m.dir, f);
      const relPath = toPosix(path.join(m.dir, f));
      const source = fs.readFileSync(abs, 'utf8');

      // --- size ---
      const lines = countNonBlankLines(source);
      const sizeProfile = profile.sizeProfiles && profile.sizeProfiles[m.sizeProfile];
      if (!sizeProfile) {
        throw new Error(`checkProfile: module "${m.id}" declares unknown sizeProfile "${m.sizeProfile}"`);
      }
      const exception = findException(profile, relPath, 'size');
      let shielded = false;
      if (exception) {
        const expired = new Date(exception.expires) <= now;
        if (expired) {
          violations.push({
            rule: 'expired-exception', module: m.id, file: relPath, line: null,
            message: `Exception for "size" on ${relPath} expired ${exception.expires} and no longer applies`,
            detail: { exception },
          });
        } else {
          exceptionsApplied.push({ ...exception, actualLines: lines });
          shielded = true;
          if (lines > exception.ceiling) {
            violations.push({
              rule: 'size', module: m.id, file: relPath, line: null,
              message: `${lines} nonblank lines exceeds excepted ceiling ${exception.ceiling}`,
              detail: { lines, ceiling: exception.ceiling, exception: true },
            });
          }
        }
      }
      if (!shielded) {
        if (lines > sizeProfile.hardMax) {
          violations.push({
            rule: 'size', module: m.id, file: relPath, line: null,
            message: `${lines} nonblank lines exceeds hard maximum ${sizeProfile.hardMax} for profile "${m.sizeProfile}"`,
            detail: { lines, hardMax: sizeProfile.hardMax },
          });
        } else if (lines > sizeProfile.warn) {
          warnings.push({
            rule: 'size', module: m.id, file: relPath, line: null,
            message: `${lines} nonblank lines exceeds warn threshold ${sizeProfile.warn} for profile "${m.sizeProfile}"`,
            detail: { lines, warn: sizeProfile.warn },
          });
        }
      }

      // --- global-state ---
      if (m.layer !== 'adapters' && m.layer !== 'bootstrap') {
        GLOBAL_SM_RE.lastIndex = 0;
        const seen = new Set();
        let gm;
        while ((gm = GLOBAL_SM_RE.exec(source))) {
          if (seen.has(gm[1])) continue;
          seen.add(gm[1]);
          violations.push({
            rule: 'global-state', module: m.id, file: relPath, line: lineAt(source, gm.index),
            message: `Implicit global "window.${gm[1]}" accessed from layer "${m.layer}" (only adapters/bootstrap may)`,
            detail: { global: `window.${gm[1]}` },
          });
        }
      }

      // --- imports: private-import, layer-violation, cycle edges ---
      for (const { specifier, line } of extractImports(source)) {
        const abs2 = resolveSpecifier(specifier, abs);
        if (!abs2) continue; // external or unresolved: not modeled in v1
        const target = fileIndex.get(abs2);
        if (!target || target.module.id === m.id) continue; // outside profile, or intra-module
        edges.get(m.id).add(target.module.id);

        if (!target.isPublic) {
          violations.push({
            rule: 'private-import', module: m.id, file: relPath, line,
            message: `${relPath} imports "${specifier}" (${target.relFile}), which module "${target.module.id}" does not list in publicApi`,
            detail: { specifier, targetModule: target.module.id, targetFile: target.relFile },
          });
        }

        const rule = profile.layerRules && profile.layerRules[m.layer];
        if (rule && Array.isArray(rule.allowedLayers) && !rule.allowedLayers.includes('*')) {
          if (!rule.allowedLayers.includes(target.module.layer)) {
            violations.push({
              rule: 'layer-violation', module: m.id, file: relPath, line,
              message: `Layer "${m.layer}" (module "${m.id}") may not depend on layer "${target.module.layer}" (module "${target.module.id}")`,
              detail: { fromLayer: m.layer, toLayer: target.module.layer, targetModule: target.module.id },
            });
          }
        }
      }
    }
  }

  // --- cycle ---
  for (const cyclePath of findCycles(edges)) {
    violations.push({
      rule: 'cycle', module: cyclePath[0], file: null, line: null,
      message: `Import cycle: ${cyclePath.join(' -> ')}`,
      detail: { path: cyclePath },
    });
  }

  return {
    ok: violations.length === 0,
    moduleCount: profile.modules.length,
    violations,
    warnings,
    exceptionsApplied,
  };
}

module.exports = {
  checkProfile,
  extractImports,
  countNonBlankLines,
  resolveSpecifier,
  findCycles,
};
