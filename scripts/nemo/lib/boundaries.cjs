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
// This bounded checker uses a lexical scanner, not a full JavaScript AST or
// binding resolver. Unsupported dynamic loads and local coverage gaps fail explicitly;
// bare external specifiers remain outside the graph. See the README before adoption.

const fs = require('node:fs');
const path = require('node:path');
const { resolveImport, resolveSpecifier } = require('./boundaries-resolver.cjs');

const EXCEPTION_RULES = ['size', 'private-import', 'layer-violation', 'global-state', 'cycle'];

/** Reject malformed policy before looking at source; comparisons must never see NaN/undefined. */
function validateProfile(profile) {
  const fail = (message) => { throw new Error(`invalid profile: ${message}`); };
  const object = (value, keys, label) => {
    if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${label} must be an object`);
    for (const key of Object.keys(value)) if (!keys.includes(key)) fail(`${label}: unknown field ${key}`);
  };
  const text = (value, label) => { if (typeof value !== 'string' || !value.trim()) fail(`${label} must be a nonempty string`); };
  const list = (value, label) => {
    if (!Array.isArray(value)) fail(`${label} must be an array`);
    for (const item of value) text(item, label);
    if (new Set(value).size !== value.length) fail(`${label} contains duplicates`);
  };
  const relative = (value, label, allowDot = false) => {
    text(value, label);
    if (allowDot && value === '.') return;
    if (value.includes('\\') || path.posix.isAbsolute(value) || value.split('/').some((x) => !x || x === '.' || x === '..')) fail(`${label} must be a normalized relative path`);
  };
  const integer = (value, label) => { if (!Number.isSafeInteger(value) || value < 0) fail(`${label} must be a finite nonnegative integer`); };
  object(profile, ['modules', 'sizeProfiles', 'layerRules', 'exceptions'], 'profile');
  if (!Array.isArray(profile.modules) || !profile.modules.length) fail('modules must be a nonempty array');
  if (!profile.sizeProfiles || typeof profile.sizeProfiles !== 'object' || Array.isArray(profile.sizeProfiles)) fail('sizeProfiles must be an object');
  for (const [name, limits] of Object.entries(profile.sizeProfiles)) {
    text(name, 'sizeProfile name');
    object(limits, ['warn', 'hardMax'], `sizeProfiles.${name}`);
    integer(limits.warn, `${name}.warn`); integer(limits.hardMax, `${name}.hardMax`);
    if (limits.warn > limits.hardMax) fail(`${name}.warn exceeds hardMax`);
  }
  const ids = new Set(), files = new Map();
  for (const m of profile.modules) {
    object(m, ['id', 'layer', 'dir', 'files', 'publicApi', 'sizeProfile'], 'module');
    text(m.id, 'module.id'); text(m.layer, 'module.layer'); text(m.sizeProfile, 'module.sizeProfile');
    if (ids.has(m.id)) fail(`duplicate module id ${m.id}`);
    ids.add(m.id);
    relative(m.dir, `${m.id}.dir`, true);
    list(m.files, `${m.id}.files`); list(m.publicApi, `${m.id}.publicApi`);
    if (!m.files.length) fail(`${m.id}.files must not be empty`);
    if (!Object.hasOwn(profile.sizeProfiles, m.sizeProfile)) fail(`unknown sizeProfile ${m.sizeProfile}`);
    for (const f of m.files) {
      relative(f, `${m.id}.files`);
      const rel = path.posix.join(m.dir, f);
      if (files.has(rel)) fail(`file ${rel} belongs to multiple modules`);
      files.set(rel, m);
    }
    for (const f of m.publicApi) if (!m.files.includes(f)) fail(`${m.id}.publicApi contains unlisted file ${f}`);
  }
  if (profile.layerRules !== undefined) {
    if (!profile.layerRules || typeof profile.layerRules !== 'object' || Array.isArray(profile.layerRules)) fail('layerRules must be an object');
    for (const [name, rule] of Object.entries(profile.layerRules)) {
      text(name, 'layer name'); object(rule, ['allowedLayers'], `layerRules.${name}`);
      list(rule.allowedLayers, `${name}.allowedLayers`);
    }
  }
  if (profile.exceptions !== undefined && !Array.isArray(profile.exceptions)) fail('exceptions must be an array');
  const exceptions = new Set();
  for (const e of profile.exceptions || []) {
    object(e, ['path', 'rule', 'ceiling', 'owner', 'issue', 'expires', 'reason'], 'exception');
    for (const key of ['path', 'rule', 'owner', 'issue', 'expires', 'reason']) text(e[key], `exception.${key}`);
    if (!EXCEPTION_RULES.includes(e.rule)) fail(`unknown exception rule ${e.rule}`);
    if (!files.has(e.path)) fail(`exception path ${e.path} is not a declared file`);
    const key = `${e.path}:${e.rule}`;
    if (exceptions.has(key)) fail(`duplicate exception ${key}`);
    exceptions.add(key);
    const date = new Date(e.expires);
    if (!/^\d{4}-\d{2}-\d{2}$/.test(e.expires) || !Number.isFinite(date.getTime()) || date.toISOString().slice(0, 10) !== e.expires) fail(`invalid exception expiry ${e.expires}`);
    if (e.rule === 'size') {
      integer(e.ceiling, 'size exception ceiling');
      if (e.ceiling < profile.sizeProfiles[files.get(e.path).sizeProfile].hardMax) fail('size exception ceiling must not lower hardMax');
    } else if (e.ceiling !== undefined) fail('ceiling is only valid for size exceptions');
  }
}

/** Small lexical scanner, not an AST/binding analyzer. Comments and literal text are opaque;
 * template substitutions are scanned recursively so imports inside them stay in the graph. */
function tokenize(source) {
  const tokens = [];
  let i = 0, line = 1, lineOffset = 0;
  // Token starts advance monotonically, including template substitutions. Avoid
  // repeatedly splitting whole prefixes of the large application source files.
  const lineAt = (index) => {
    while (lineOffset < index) if (source[lineOffset++] === '\n') line++;
    return line;
  };
  const add = (type, value, start) => tokens.push({ type, value, line: lineAt(start) });
  function escape() {
    const ch = source[i++];
    const simple = { n: '\n', r: '\r', t: '\t', b: '\b', f: '\f', v: '\v', '0': '\0', '\n': '', '\r': '' };
    if (/[1-9]/.test(ch) || (ch === '0' && /[0-9]/.test(source[i] || ''))) throw new Error('legacy numeric string escapes are unsupported');
    if (ch === 'u' || ch === 'x') {
      let hex;
      if (ch === 'u' && source[i] === '{') { const end = source.indexOf('}', ++i); hex = source.slice(i, end); i = end + 1; }
      else { const length = ch === 'u' ? 4 : 2; hex = source.slice(i, i + length); i += length; }
      if (!hex || !/^[0-9a-f]+$/i.test(hex)) throw new Error('unsupported invalid string escape');
      return String.fromCodePoint(parseInt(hex, 16));
    }
    if (ch === '\r' && source[i] === '\n') i++;
    return Object.hasOwn(simple, ch) ? simple[ch] : ch;
  }
  function literal(quote, start) {
    let value = '', interpolated = false;
    const token = { type: 'string', value: '', line: lineAt(start) };
    tokens.push(token);
    while (i < source.length) {
      const ch = source[i++];
      if (ch === quote) { token.value = value; if (interpolated) add('template-end', '', i - 1); return; }
      if (ch === '\\') value += escape();
      else if (quote === '`' && ch === '$' && source[i] === '{') {
        i++; interpolated = true; token.type = 'template';
        scan(true); // consumes its matching closing brace, including nested objects/templates
      } else value += ch;
    }
    throw new Error(`unterminated ${interpolated ? 'template' : 'string'} literal at line ${token.line}`);
  }
  function scan(substitution = false) {
    const firstToken = tokens.length, contexts = [];
    let closedControl = false;
    while (i < source.length) {
      const start = i, ch = source[i];
      if (/\s/.test(ch)) { i++; continue; }
      if (source.startsWith('//', i) || (i === 0 && source.startsWith('#!', i))) { while (i < source.length && source[i] !== '\n') i++; continue; }
      if (source.startsWith('/*', i)) {
        const end = source.indexOf('*/', i + 2);
        if (end < 0) throw new Error('unterminated block comment');
        i = end + 2; continue;
      }
      if (ch === '"' || ch === "'" || ch === '`') { i++; literal(ch, start); closedControl = false; continue; }
      const previous = tokens.length === firstToken ? null : tokens.at(-1);
      // A control header starts a statement; a call/group closes an expression.
      // Brace endings still need block/object/function context: do not guess.
      if (ch === '/' && previous?.type === 'punct' && previous.value === '}') throw new Error(`ambiguous slash after } at line ${lineAt(start)}; requires an AST inventory`);
      if (previous?.type === 'punct' && ['/', '/='].includes(previous.value) && ')}];,*%=&|<>?:'.includes(ch)) throw new Error(`missing division operand at line ${lineAt(start)}`);
      const keyword = previous?.type === 'name' && !['.', '?.'].includes(tokens.at(-2)?.value);
      const expressionStart = !previous || closedControl || (keyword && /^(return|throw|yield|await|case|delete|void|typeof|new|else|do|in|instanceof)$/.test(previous.value)) || (previous.type === 'punct' && ![')', ']', '}', '++', '--', '.', '?.'].includes(previous.value));
      closedControl = false;
      if (ch === '/' && expressionStart) {
        i++; let bracket = false, closed = false;
        while (i < source.length) {
          const c = source[i++];
          if (/[\n\r\u2028\u2029]/.test(c) || (c === '\\' && /[\n\r\u2028\u2029]/.test(source[i] || ''))) break;
          if (c === '\\') { i++; continue; }
          if (c === '[') bracket = true;
          if (c === ']') bracket = false;
          if (c === '/' && !bracket) { closed = true; break; }
        }
        if (!closed) throw new Error(`unsupported or unterminated regex at line ${lineAt(start)}`);
        const patternEnd = i - 1;
        while (/[a-z]/i.test(source[i] || '') && i < source.length) i++;
        try { new RegExp(source.slice(start + 1, patternEnd), source.slice(patternEnd + 1, i)); }
        catch { throw new Error(`invalid or unsupported regex at line ${lineAt(start)}`); }
        add('regex', '', start); continue;
      }
      if (/[A-Za-z_$]/.test(ch)) {
        i++; while (i < source.length && /[A-Za-z0-9_$]/.test(source[i])) i++;
        add('name', source.slice(start, i), start); continue;
      }
      if (ch === '\\') throw new Error(`escaped identifiers are unsupported at line ${lineAt(start)}`);
      if (/\d/.test(ch)) { i++; while (i < source.length && /[\w.]/.test(source[i])) i++; add('number', source.slice(start, i), start); continue; }
      if (ch === '}' && substitution && !contexts.length) { i++; return; }
      if ('({['.includes(ch)) contexts.push({ open: ch, control: ch === '(' && keyword && (/^(if|while|for|with|switch|catch)$/.test(previous.value) || (previous.value === 'await' && tokens.at(-2)?.value === 'for')) });
      if (')}]'.includes(ch)) {
        const context = contexts.pop();
        if (!context || context.open !== { ')': '(', '}': '{', ']': '[' }[ch]) throw new Error(`unmatched ${ch} at line ${lineAt(start)}`);
        closedControl = context.control;
      }
      if (['?.', '++', '--', '/='].includes(source.slice(i, i + 2))) i += 2;
      else i++;
      add('punct', source.slice(start, i), start);
    }
    if (tokens.at(-1)?.type === 'punct' && ['/', '/='].includes(tokens.at(-1).value)) throw new Error('missing division operand at end of source');
    if (contexts.length) throw new Error('unterminated delimiter context');
    if (substitution) throw new Error('unterminated template substitution');
  }
  scan();
  return tokens;
}

function analyzeSource(source) {
  const tokens = tokenize(source), imports = [], globals = [], unsupported = [];
  const property = (i) => tokens[i - 1]?.type === 'punct' && ['.', '?.'].includes(tokens[i - 1].value);
  const methodDeclaration = (i, openIndex) => {
    if (tokens[i - 1]?.value !== '{' && tokens[i - 1]?.value !== ',') return false;
    let depth = 0;
    for (let j = openIndex; j < tokens.length; j++) {
      if (tokens[j].value === '(') depth++;
      else if (tokens[j].value === ')' && --depth === 0) {
        // A newline could instead terminate a real call before a standalone block.
        return tokens[j + 1]?.value === '{' && tokens[j + 1].line === tokens[j].line;
      }
    }
    return false;
  };
  const recordImport = (token, sourceToken) => {
    if (token?.type === 'string') imports.push({ specifier: token.value, line: sourceToken.line, kind: sourceToken.value === 'require' ? 'require' : 'import' });
    else unsupported.push({ line: sourceToken.line, message: 'Import/require target must be a literal string; declare or rewrite dynamic loading before checking this profile' });
  };
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i], next = tokens[i + 1];
    if (token.type !== 'name' || property(i)) continue;
    const callOffset = token.value === 'require' && next?.value === '?.' ? 2 : 1;
    if ((token.value === 'require' || token.value === 'import') && tokens[i + callOffset]?.value === '(') {
      if (token.value === 'require' && callOffset === 1 && methodDeclaration(i, i + callOffset)) continue;
      const target = tokens[i + callOffset + 1], tail = tokens[i + callOffset + 2]?.value;
      recordImport([')', ','].includes(tail) ? target : null, token);
    } else if (token.value === 'import' && next?.type === 'string') recordImport(next, token);
    else if (['import', 'export'].includes(token.value) && !['.', '('].includes(next?.value)) {
      for (let j = i + 1; j < tokens.length && ![';', '='].includes(tokens[j].value); j++) {
        if (tokens[j].value === 'from' && tokens[j + 1]?.type === 'string') { recordImport(tokens[j + 1], token); break; }
        if (j > i + 1 && ['import', 'export', 'const', 'function', 'class'].includes(tokens[j].value)) break;
      }
    }
    if (token.value === 'window') {
      let j = i + 1, member;
      if (tokens[j]?.value === '?.') j++;
      if (tokens[j]?.value === '.') j++;
      if (tokens[j]?.type === 'name' && ['.', '?.'].includes(tokens[j - 1]?.value)) member = tokens[j];
      else if (tokens[j]?.value === '[' && tokens[j + 1]?.type === 'string' && tokens[j + 2]?.value === ']') member = tokens[j + 1];
      else if (tokens[j]?.value === '[') unsupported.push({ rule: 'unsupported-global', line: token.line, message: 'Computed window access requires a literal property name in a reviewed profile' });
      if (member && /^SM[A-Za-z0-9_]*$/.test(member.value)) globals.push({ name: member.value, line: token.line });
    }
  }
  return { imports, globals, unsupported };
}

function extractImports(source) {
  return analyzeSource(source).imports.map(({ specifier, line }) => ({ specifier, line }));
}

function countNonBlankLines(source) {
  const lines = source.split(/\r?\n/);
  if (lines.length && lines[lines.length - 1] === '') lines.pop();
  return lines.filter((l) => l.trim().length > 0).length;
}

function toPosix(p) {
  return p.split(path.sep).join('/');
}

/** Build absPath -> { module, relFile, isPublic } for every file every module declares. */
function buildFileIndex(profile, root) {
  const index = new Map();
  for (const m of profile.modules) {
    for (const f of m.files) {
      const abs = fs.realpathSync(path.resolve(root, m.dir, f));
      if (index.has(abs)) throw new Error('invalid profile: multiple files refer to the same physical source');
      index.set(abs, { module: m, relFile: toPosix(path.join(m.dir, f)), isPublic: (m.publicApi || []).includes(f) });
    }
  }
  return index;
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
  validateProfile(profile);
  const root = fs.realpathSync(opts.root || process.cwd());
  const now = opts.now || new Date();
  if (!(now instanceof Date) || !Number.isFinite(now.getTime())) throw new Error('invalid check clock');
  const fileIndex = buildFileIndex(profile, root);
  const violations = [];
  const warnings = [];
  const exceptionsApplied = [];
  const edges = new Map(profile.modules.map((m) => [m.id, new Set()]));
  const edgeFiles = new Map();
  const activeExceptions = new Map();
  const applied = new Set();
  const applyException = (exception, extra = {}) => {
    if (!applied.has(exception)) exceptionsApplied.push({ ...exception, ...extra });
    applied.add(exception);
  };
  for (const exception of profile.exceptions || []) {
    if (new Date(exception.expires) <= now) {
      violations.push({ rule: 'expired-exception', module: null, file: exception.path, line: null,
        message: `Exception for "${exception.rule}" on ${exception.path} expired ${exception.expires}`,
        detail: { exception } });
    } else activeExceptions.set(`${exception.path}:${exception.rule}`, exception);
  }
  const reportViolation = (violation) => {
    const exception = activeExceptions.get(`${violation.file}:${violation.rule}`);
    if (exception && violation.rule !== 'size') applyException(exception);
    else violations.push(violation);
  };

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
      const exception = activeExceptions.get(`${relPath}:size`);
      const shielded = Boolean(exception);
      if (exception) {
        applyException(exception, { actualLines: lines });
        if (lines > exception.ceiling) {
          violations.push({ rule: 'size', module: m.id, file: relPath, line: null,
            message: `${lines} nonblank lines exceeds excepted ceiling ${exception.ceiling}`,
            detail: { lines, ceiling: exception.ceiling, exception: true } });
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

      const analysis = analyzeSource(source);
      for (const issue of analysis.unsupported) {
        violations.push({ rule: 'unsupported-import', module: m.id, file: relPath, ...issue });
      }
      // Scan the same tokens used for dependencies, including literal bracket/optional access.
      if (m.layer !== 'adapters' && m.layer !== 'bootstrap') {
        const seen = new Set();
        for (const global of analysis.globals) {
          if (seen.has(global.name)) continue;
          seen.add(global.name);
          reportViolation({ rule: 'global-state', module: m.id, file: relPath, line: global.line,
            message: `Implicit global "window.${global.name}" accessed from layer "${m.layer}" (only adapters/bootstrap may)`,
            detail: { global: `window.${global.name}` } });
        }
      }

      // --- imports: private-import, layer-violation, cycle edges ---
      for (const { specifier, line, kind } of analysis.imports) {
        const resolution = resolveImport(specifier, abs, kind);
        if (resolution.external) continue;
        const target = fileIndex.get(resolution.path);
        if (resolution.rule || !target) {
          violations.push({ rule: resolution.rule || 'unprofiled-local-import', module: m.id, file: relPath, line,
            message: resolution.message || `Local ${kind} target ${JSON.stringify(specifier)} exists but is not declared in this profile`,
            detail: { specifier, kind, ...(resolution.path ? { targetFile: toPosix(path.relative(root, resolution.path)) } : {}) },
          });
          continue;
        }
        if (target.module.id === m.id) continue; // Declared intra-module dependencies need no boundary checks.
        edges.get(m.id).add(target.module.id);
        const edgeKey = JSON.stringify([m.id, target.module.id]);
        if (!edgeFiles.has(edgeKey)) edgeFiles.set(edgeKey, new Set());
        edgeFiles.get(edgeKey).add(relPath);

        if (!target.isPublic) {
          reportViolation({
            rule: 'private-import', module: m.id, file: relPath, line,
            message: `${relPath} imports "${specifier}" (${target.relFile}), which module "${target.module.id}" does not list in publicApi`,
            detail: { specifier, targetModule: target.module.id, targetFile: target.relFile },
          });
        }

        const rule = profile.layerRules && profile.layerRules[m.layer];
        if (rule && Array.isArray(rule.allowedLayers) && !rule.allowedLayers.includes('*')) {
          if (!rule.allowedLayers.includes(target.module.layer)) {
            reportViolation({
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
  // A cycle exception removes only dependency contributions originating in its
  // exact file. Keep a module edge when any unexcepted file still contributes it.
  const cycleEdges = new Map(profile.modules.map((m) => [m.id, new Set()]));
  for (const [edgeKey, contributors] of edgeFiles) {
    const [from, to] = JSON.parse(edgeKey);
    if ([...contributors].some((file) => !activeExceptions.has(`${file}:cycle`))) cycleEdges.get(from).add(to);
  }
  for (const cyclePath of findCycles(edges)) {
    cyclePath.slice(0, -1).forEach((id, i) => {
      for (const file of edgeFiles.get(JSON.stringify([id, cyclePath[i + 1]])) || []) {
        const exception = activeExceptions.get(`${file}:cycle`);
        if (exception) applyException(exception, { cycle: cyclePath });
      }
    });
  }
  for (const cyclePath of findCycles(cycleEdges)) {
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
  validateProfile,
  extractImports,
  countNonBlankLines,
  resolveSpecifier,
  findCycles,
};
