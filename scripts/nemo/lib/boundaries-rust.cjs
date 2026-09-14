'use strict';
// Rust-aware crate boundary checker (P12/#1014).
//
// A deliberately small analyzer for the module graph INSIDE one Rust crate:
// it reads `use crate::… / super::… / self::…` statements (single-line,
// multi-line and brace groups), `mod name;` declarations, inline
// `crate::name::` paths and `#[cfg(...)]` gates, after stripping comments
// and string literals. It never runs the JavaScript lexer on Rust, and it
// never guesses: a path it cannot resolve to a declared module file is
// reported as `unsupported`, not silently accepted. External crates
// (`use vello::…`) are outside the graph by design.
//
// Module identity comes from the existing Rust profile (rust.profile.json):
// each profile module lists its files, and a crate policy names the crate
// root, the permitted layer edges, the exported port and the declared Cargo
// features. Layer rules, cycles, undeclared feature gates and exported-port
// drift are the four rule families; exceptions are keyed by `file:rule`,
// carry owner/issue/reason/expiry like the JS profile's, and expire loudly.

const fs = require('node:fs');
const path = require('node:path');
const { findCycles } = require('./boundaries.cjs');

const RULES = ['layer-violation', 'cycle', 'undeclared-feature', 'exported-port', 'unsupported',
  'external-crate-violation', 'private-port-access'];

function invalid(message) { throw new Error(`invalid rust crate policy: ${message}`); }

function validateCratePolicy(policy) {
  if (!policy || policy.schemaVersion !== 1 || policy.kind !== 'rust-crate') invalid('unsupported schema or kind');
  if (policy.status !== 'adopted') invalid('status must be adopted before standard enforcement');
  for (const key of ['crate', 'root', 'sourceDir']) if (typeof policy[key] !== 'string' || !policy[key]) invalid(`${key} is required`);
  if (!policy.layerRules || typeof policy.layerRules !== 'object') invalid('layerRules is required');
  for (const [layer, rule] of Object.entries(policy.layerRules)) {
    if (!rule || !Array.isArray(rule.allowedLayers)) invalid(`layerRules.${layer}.allowedLayers must be an array`);
  }
  if (!policy.exportedPort || !Array.isArray(policy.exportedPort.items)) invalid('exportedPort.items must be an array');
  if (!policy.features || !Array.isArray(policy.features.declared) || !Array.isArray(policy.features.allowedCfgs)) invalid('features.declared and features.allowedCfgs must be arrays');
  if (policy.externalCratePorts !== undefined) {
    if (!policy.externalCratePorts || typeof policy.externalCratePorts !== 'object' || Array.isArray(policy.externalCratePorts)) invalid('externalCratePorts must be an object');
    for (const [name, port] of Object.entries(policy.externalCratePorts)) {
      if (!port || !Array.isArray(port.allowedModules) || !Array.isArray(port.items)) invalid(`externalCratePorts.${name} must have allowedModules and items arrays`);
    }
  }
  if (policy.unanalyzedModules !== undefined && !Array.isArray(policy.unanalyzedModules)) invalid('unanalyzedModules must be an array');
  for (const exception of policy.exceptions || []) {
    for (const key of ['path', 'rule', 'owner', 'issue', 'reason', 'expires']) if (typeof exception[key] !== 'string' || !exception[key]) invalid(`exception ${key} is required`);
    if (!RULES.includes(exception.rule)) invalid(`exception rule ${exception.rule} is not a crate rule`);
    if (Number.isNaN(new Date(exception.expires).getTime())) invalid(`exception expiry ${exception.expires} is not a date`);
  }
}

// --- source scanning ---------------------------------------------------------

// Remove comments and string literals so `use`/`crate::` inside them are not
// read as code. Line count is preserved (newlines are kept) for line numbers.
function stripRust(source, opts = {}) {
  const keepStrings = !!opts.keepStrings;
  let out = '', i = 0;
  const n = source.length;
  while (i < n) {
    const ch = source[i], next = source[i + 1];
    if (ch === '/' && next === '/') { while (i < n && source[i] !== '\n') i++; continue; }
    if (ch === '/' && next === '*') {
      let depth = 1; i += 2;
      while (i < n && depth) {
        if (source[i] === '/' && source[i + 1] === '*') { depth++; i += 2; continue; }
        if (source[i] === '*' && source[i + 1] === '/') { depth--; i += 2; continue; }
        if (source[i] === '\n') out += '\n';
        i++;
      }
      continue;
    }
    if (ch === 'r' && (next === '"' || (next === '#' && /^r#+"/.test(source.slice(i))))) {
      const m = /^r(#*)"/.exec(source.slice(i));
      const close = '"' + m[1];
      let j = i + m[0].length;
      while (j < n && source.slice(j, j + close.length) !== close) { if (source[j] === '\n') out += '\n'; j++; }
      out += keepStrings ? source.slice(i, j + close.length) : '""';
      i = j + close.length; continue;
    }
    if (ch === '"') {
      let j = i + 1;
      while (j < n && source[j] !== '"') { if (source[j] === '\\') j++; else if (source[j] === '\n') out += '\n'; j++; }
      out += keepStrings ? source.slice(i, j + 1) : '""';
      i = j + 1; continue;
    }
    if (ch === '\'' && /^'(\\.|[^\\'])'/.test(source.slice(i, i + 4))) { const m = /^'(\\.|[^\\'])'/.exec(source.slice(i)); i += m[0].length; out += "' '"; continue; }
    out += ch; i++;
  }
  return out;
}

function lineOf(text, index) { let line = 1; for (let i = 0; i < index; i++) if (text[i] === '\n') line++; return line; }

// Expand `a::{b, c::{d, e}, self}` into flat paths.
function expandUseTree(spec) {
  spec = spec.trim();
  const brace = spec.indexOf('{');
  if (brace === -1) {
    const clean = spec.replace(/\s+as\s+\w+$/, '').trim();
    return clean ? [clean.split('::').map((s) => s.trim()).filter(Boolean)] : [];
  }
  const prefix = spec.slice(0, brace).replace(/::\s*$/, '').trim();
  const inner = spec.slice(brace + 1, spec.lastIndexOf('}'));
  const parts = []; let depth = 0, cur = '';
  for (const ch of inner) {
    if (ch === '{') depth++; if (ch === '}') depth--;
    if (ch === ',' && depth === 0) { parts.push(cur); cur = ''; } else cur += ch;
  }
  if (cur.trim()) parts.push(cur);
  const out = [];
  for (const part of parts) {
    for (const sub of expandUseTree(part)) {
      const segs = (prefix ? prefix.split('::') : []).concat(sub);
      out.push(segs.map((s) => s.trim()).filter((s) => s && s !== 'self'));
    }
  }
  return out;
}

// Spans of top-level `mod name { ... }` bodies (inline modules such as
// `#[cfg(test)] mod tests { use super::*; }`): a `super::` inside one names the
// enclosing FILE module, not the file's parent. Deeper nesting is unsupported.
function inlineModSpans(text) {
  const spans = [];
  const re = /(?:^|[\s;{}])(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*\{/g;
  let m;
  while ((m = re.exec(text))) {
    const open = m.index + m[0].length - 1;
    let depth = 0, j = open;
    for (; j < text.length; j++) { if (text[j] === '{') depth++; else if (text[j] === '}' && --depth === 0) break; }
    const nested = /(?:^|[\s;{}])mod\s+[A-Za-z_]\w*\s*\{/.test(text.slice(open + 1, j));
    spans.push({ name: m[1], start: open, end: j, nested });
    re.lastIndex = j;
  }
  return spans;
}

function analyzeRustSource(source, opts = {}) {
  const text = stripRust(source);
  const withStrings = stripRust(source, { keepStrings: true });
  const spans = inlineModSpans(text);
  const depthAt = (index) => { const s = spans.find((sp) => index > sp.start && index < sp.end); return s ? (s.nested ? 2 : 1) : 0; };
  const uses = [], mods = [], inline = [], cfgs = [];
  const useRe = /(?:^|[\s;{}])(?:pub(?:\([^)]*\))?\s+)?use\s+([^;]+);/g;
  let m;
  while ((m = useRe.exec(text))) {
    const at = m.index + m[0].indexOf('use');
    const line = lineOf(text, at);
    for (const segs of expandUseTree(m[1].replace(/\s+/g, ' '))) uses.push({ segments: segs, line, pub: /\bpub\b/.test(m[0]), depth: depthAt(at) });
  }
  const modRe = /(?:^|[\s;{}])(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*(;|\{)/g;
  while ((m = modRe.exec(text))) mods.push({ name: m[1], line: lineOf(text, m.index + 1), inlineBody: m[2] === '{' });
  // opts.externalCrates lets a caller widen this to named external crates (e.g. `nemo_mcp::…`)
  // so an external-crate port policy can be enforced without walking the source twice.
  const externalCrates = (opts.externalCrates || []).filter((c) => /^[A-Za-z_]\w*$/.test(c));
  const inlineHeads = ['crate', 'super', 'self', ...externalCrates].join('|');
  const inlineRe = new RegExp(`\\b(${inlineHeads})::([A-Za-z_]\\w*)`, 'g');
  while ((m = inlineRe.exec(text))) {
    // Skip the `use` statements already captured (their spans contain the same tokens).
    const before = text.lastIndexOf('use ', m.index);
    const semi = text.indexOf(';', before === -1 ? 0 : before);
    if (before !== -1 && semi !== -1 && m.index > before && m.index < semi && /use\s/.test(text.slice(before, before + 5))) continue;
    inline.push({ segments: [m[1], m[2]], line: lineOf(text, m.index), depth: depthAt(m.index) });
  }
  const cfgRe = /#\s*\[\s*cfg\s*\(([^\]]*)\)\s*\]/g;
  while ((m = cfgRe.exec(withStrings))) cfgs.push({ expr: m[1].replace(/\s+/g, ' ').trim(), line: lineOf(withStrings, m.index) });
  return { uses, mods, inline, cfgs, inlineMods: spans.map((sp) => ({ name: sp.name, nested: sp.nested })) };
}

// --- crate graph -------------------------------------------------------------

function toPosix(p) { return p.split(path.sep).join('/'); }

function crateModules(profile, policy) {
  const dir = policy.sourceDir.replace(/\/$/, '');
  const modules = profile.modules.filter((m) => m.dir === dir || m.dir.startsWith(dir + '/'));
  const fileToModule = new Map();
  for (const m of modules) for (const f of m.files) fileToModule.set(toPosix(path.posix.join(m.dir, f)), m);
  return { modules, fileToModule };
}

// `crate::name` → `<sourceDir>/name.rs` or `<sourceDir>/name/mod.rs`; `crate` root → policy.root.
function resolveSegments(segments, fromFile, policy, fileToModule, depth = 0) {
  const dir = policy.sourceDir.replace(/\/$/, '');
  const [head, name] = segments;
  let file = null, reason = null;
  if (head === 'super' && depth === 1) return { file: fromFile, reason: null };   // inside `mod tests { }`: the file itself
  if ((head === 'super' || head === 'self') && depth > 1) return { file: null, reason: `${head}:: inside a nested inline module is not supported` };
  if (head === 'crate') {
    if (!name) file = policy.root;
    else {
      const flat = `${dir}/${name}.rs`, nested = `${dir}/${name}/mod.rs`;
      if (fileToModule.has(flat)) file = flat;
      else if (fileToModule.has(nested)) file = nested;
      else file = policy.root; // an item defined at the crate root (fn/struct in lib.rs)
    }
  } else if (head === 'super') {
    const rel = fromFile.slice(dir.length + 1);
    const parts = rel.split('/');
    if (parts.length === 1) file = policy.root; // src/x.rs → parent is the crate root
    else if (parts.length === 2 && parts[1] !== 'mod.rs') {
      // src/x/y.rs → the Rust-2018 shape prefers a sibling src/x.rs over src/x/mod.rs when both could apply.
      const flatParent = `${dir}/${parts[0]}.rs`;
      file = fileToModule.has(flatParent) ? flatParent : `${dir}/${parts[0]}/mod.rs`;
    }
    else reason = `super:: from ${rel} is not a supported module shape (flat crate or one nesting level only)`;
    if (file && !fileToModule.has(file)) { reason = `super:: target ${file} is not a declared profile file`; file = null; }
  } else if (head === 'self') {
    file = fromFile;
  }
  return { file, reason };
}

function checkRustCrate(profile, policy, opts = {}) {
  validateCratePolicy(policy);
  const root = path.resolve(opts.root || '.');
  const now = opts.now || new Date();
  const { modules, fileToModule } = crateModules(profile, policy);
  if (!fileToModule.has(policy.root)) invalid(`crate root ${policy.root} is not declared in the profile`);
  if (Array.isArray(policy.profileModules)) {
    const found = modules.map((m) => m.id).sort(), declared = policy.profileModules.slice().sort();
    if (JSON.stringify(found) !== JSON.stringify(declared)) invalid(`profile modules under ${policy.sourceDir} are ${found.join(', ')}; the policy declares ${declared.join(', ')}`);
  }
  const allModuleIds = new Set(profile.modules.map((m) => m.id));
  for (const id of policy.unanalyzedModules || []) if (!allModuleIds.has(id)) invalid(`unanalyzedModules references unknown module "${id}"`);
  const externalCrateNames = Object.keys(policy.externalCratePorts || {});
  const violations = [], unsupported = [], exceptionsApplied = [];
  const edges = new Map(modules.map((m) => [m.id, new Set()]));
  const edgeFiles = new Map();
  const active = new Map();
  for (const exception of policy.exceptions || []) {
    if (new Date(exception.expires) <= now) {
      violations.push({ rule: 'expired-exception', module: null, file: exception.path, line: null,
        message: `Exception for "${exception.rule}" on ${exception.path} expired ${exception.expires}`, detail: { exception } });
    } else active.set(`${exception.path}:${exception.rule}`, exception);
  }
  const report = (violation) => {
    const exception = active.get(`${violation.file}:${violation.rule}`);
    if (exception) { if (!exceptionsApplied.includes(exception)) exceptionsApplied.push(exception); return; }
    violations.push(violation);
  };
  const featureSet = new Set(policy.features.declared);
  const allowedCfgs = policy.features.allowedCfgs;
  let rootAnalysis = null;
  for (const [file, m] of fileToModule) {
    const abs = path.join(root, file);
    if (!fs.existsSync(abs)) { unsupported.push({ file, line: null, message: 'declared profile file is missing' }); continue; }
    const analysis = analyzeRustSource(fs.readFileSync(abs, 'utf8'), { externalCrates: externalCrateNames });
    if (file === policy.root) rootAnalysis = analysis;
    // Feature gates: every cfg must be an allowed predicate or a declared feature.
    for (const cfg of analysis.cfgs) {
      const features = [...cfg.expr.matchAll(/feature\s*=\s*"([^"]+)"/g)].map((x) => x[1]);
      const stripped = cfg.expr.replace(/feature\s*=\s*"[^"]+"/g, 'FEATURE');
      const tokens = stripped.split(/[(),]|\bnot\b|\ball\b|\bany\b/).map((s) => s.trim()).filter(Boolean);
      for (const token of tokens) {
        if (token === 'FEATURE') continue;
        if (!allowedCfgs.includes(token)) report({ rule: 'undeclared-feature', module: m.id, file, line: cfg.line,
          message: `cfg predicate ${JSON.stringify(token)} is not in the policy's allowed cfgs`, detail: { cfg: cfg.expr } });
      }
      for (const feature of features) {
        if (!featureSet.has(feature)) report({ rule: 'undeclared-feature', module: m.id, file, line: cfg.line,
          message: `cfg(feature = ${JSON.stringify(feature)}) references a feature Cargo.toml does not declare`, detail: { feature, cfg: cfg.expr } });
      }
    }
    // External-crate ports: a named crate (e.g. nemo_mcp) may only be referenced from its allowed
    // modules, and only through its declared items (an item name covers itself and any deeper path).
    if (externalCrateNames.length) {
      const externalRefs = analysis.uses.filter((u) => externalCrateNames.includes(u.segments[0]))
        .concat(analysis.inline.filter((i) => externalCrateNames.includes(i.segments[0])));
      for (const ref of externalRefs) {
        const crateName = ref.segments[0];
        const port = policy.externalCratePorts[crateName];
        const restPath = ref.segments.slice(1).join('::');
        if (!port.allowedModules.includes(m.id)) {
          report({ rule: 'external-crate-violation', module: m.id, file, line: ref.line,
            message: `Module "${m.id}" may not depend on external crate "${crateName}" via ${ref.segments.join('::')}`,
            detail: { crate: crateName, path: restPath, module: m.id } });
          continue;
        }
        if (!port.items.some((item) => restPath === item || restPath.startsWith(`${item}::`))) {
          report({ rule: 'private-port-access', module: m.id, file, line: ref.line,
            message: `"${crateName}::${restPath}" is not part of the declared port for "${crateName}"`,
            detail: { crate: crateName, path: restPath, module: m.id } });
        }
      }
    }
    // Edges: use statements + inline paths + mod declarations (root only, `mod x;` loads a file).
    const refs = analysis.uses.map((u) => ({ segments: u.segments, line: u.line, depth: u.depth }))
      .concat(analysis.inline.map((i) => ({ segments: i.segments, line: i.line, depth: i.depth })));
    if (file === policy.root) for (const mod of analysis.mods) if (!mod.inlineBody) refs.push({ segments: ['crate', mod.name], line: mod.line, depth: 0 });
    for (const ref of refs) {
      const head = ref.segments[0];
      if (!['crate', 'super', 'self'].includes(head)) continue; // external crate or std: outside the graph
      const { file: target, reason } = resolveSegments(ref.segments, file, policy, fileToModule, ref.depth);
      if (!target) { unsupported.push({ file, line: ref.line, message: reason, path: ref.segments.join('::') }); continue; }
      const targetModule = fileToModule.get(target);
      if (!targetModule || targetModule.id === m.id) continue;
      edges.get(m.id).add(targetModule.id);
      const key = JSON.stringify([m.id, targetModule.id]);
      if (!edgeFiles.has(key)) edgeFiles.set(key, new Set());
      edgeFiles.get(key).add(file);
      const rule = policy.layerRules[m.layer];
      if (!rule) { unsupported.push({ file, line: ref.line, message: `layer "${m.layer}" has no rule in the crate policy` }); continue; }
      if (!rule.allowedLayers.includes('*') && !rule.allowedLayers.includes(targetModule.layer)) {
        report({ rule: 'layer-violation', module: m.id, file, line: ref.line,
          message: `Layer "${m.layer}" (module "${m.id}") may not depend on layer "${targetModule.layer}" (module "${targetModule.id}") via ${ref.segments.join('::')}`,
          detail: { fromLayer: m.layer, toLayer: targetModule.layer, targetModule: targetModule.id, path: ref.segments.join('::') } });
      }
    }
  }
  // Cycles among module edges; an exception on any contributing file waives the cycle.
  for (const cyclePath of findCycles(edges)) {
    const files = new Set();
    cyclePath.slice(0, -1).forEach((id, i) => { for (const f of edgeFiles.get(JSON.stringify([id, cyclePath[i + 1]])) || []) files.add(f); });
    const waived = [...files].find((f) => active.get(`${f}:cycle`));
    if (waived) { const ex = active.get(`${waived}:cycle`); if (!exceptionsApplied.includes(ex)) exceptionsApplied.push(ex); continue; }
    violations.push({ rule: 'cycle', module: cyclePath[0], file: null, line: null, message: `Module cycle: ${cyclePath.join(' -> ')}`, detail: { path: cyclePath, files: [...files] } });
  }
  // Exported port: the crate root's `pub use` items must equal the policy's list exactly.
  const exported = rootAnalysis ? rootAnalysis.uses.filter((u) => u.pub).map((u) => u.segments.join('::')).sort() : [];
  const declared = policy.exportedPort.items.slice().sort();
  if (JSON.stringify(exported) !== JSON.stringify(declared)) {
    report({ rule: 'exported-port', module: fileToModule.get(policy.root).id, file: policy.root, line: null,
      message: `crate root re-exports differ from the declared exported port`,
      detail: { missing: declared.filter((x) => !exported.includes(x)), extra: exported.filter((x) => !declared.includes(x)) } });
  }
  for (const u of unsupported) report({ rule: 'unsupported', module: fileToModule.get(u.file)?.id || null, file: u.file, line: u.line, message: u.message, detail: { path: u.path || null } });
  const edgeList = [];
  for (const [from, tos] of edges) for (const to of tos) edgeList.push({ from, to, files: [...edgeFiles.get(JSON.stringify([from, to]))] });
  return { ok: violations.length === 0, crate: policy.crate, moduleCount: modules.length, edges: edgeList,
    exportedPort: exported, violations, exceptionsApplied, unsupported, unanalyzedModules: policy.unanalyzedModules || [] };
}

module.exports = { RULES, validateCratePolicy, stripRust, expandUseTree, analyzeRustSource, checkRustCrate };
