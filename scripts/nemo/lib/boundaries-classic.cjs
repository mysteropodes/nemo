'use strict';
// Bounded, source-pinned classic-script dependency analysis; no product execution.
// checkClassicScripts(contract, {root, profile, requiredScope, inventoryHTML}).
// requiredScope is independently adopted {symbols, loaders, applicability}, with
// applicability: [{loader, use, realm}]. Retain these obligations independently
// of the contract's mutable occurrence lists; never infer them from those lists.
// inventoryHTML is the synchronous script-order helper
// with parse5 already injected. Missing evidence fails closed. VM order/readiness
// are reviewed facts pinned to complete loader sources, not inferred execution.
// classicOk isolates this contract from retained legacy global-state violations;
// ok remains false for either. CI must require ok AND existing checkProfile gates.
// Supported binding form: unconditional var IIFE, literal named-function API,
// bare direct member capture/call. Other forms fail explicitly, including writes.
// Static ordering is conditional on successful execution without DOM mutation.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { Script } = require('node:vm');
const { validateProfile, extractImports, findCycles } = require('./boundaries.cjs');
const { resolveImport } = require('./boundaries-resolver.cjs');
const blob = (bytes) => crypto.createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex');

// Same conservative lexical conventions as boundaries.cjs, with source offsets.
function tokenize(source) {
  const tokens = [];
  let i = 0, line = 1, lineOffset = 0;
  // Token starts advance monotonically, including template substitutions. Avoid
  // repeatedly splitting whole prefixes of the large application source files.
  const lineAt = (index) => {
    while (lineOffset < index) if (source[lineOffset++] === '\n') line++;
    return line;
  };
  const add = (type, value, start) => tokens.push({ type, value, offset: start, line: lineAt(start) });
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
    const token = { type: 'string', value: '', offset: start, line: lineAt(start) };
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
      if (ch.charCodeAt(0) > 127) throw new Error('unsupported non-ASCII identifier syntax');
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

function validate(contract, options) {
  const need = (ok, message) => { if (!ok) throw new Error(message); };
  const object = (v, keys) => {
    need(v && typeof v === 'object' && !Array.isArray(v), 'expected object');
    need(Object.keys(v).every((k) => keys.includes(k)), 'unknown contract field');
  };
  const text = (v) => need(typeof v === 'string' && v.trim().length > 0, 'expected nonempty string');
  const list = (v) => { need(Array.isArray(v), 'expected array'); v.forEach(text); need(new Set(v).size === v.length, 'duplicate value'); };
  const relative = (v) => { text(v); need(!/[\\\x00-\x1f:]/.test(v) && !path.posix.isAbsolute(v) && v.split('/').every((s) => s && s !== '.' && s !== '..'), 'invalid relative path'); };
  const scope = (v, adopted = false) => { object(v, ['symbols', 'loaders'].concat(adopted ? ['applicability'] : [])); list(v.symbols); list(v.loaders); need(v.symbols.length && v.loaders.length, 'empty scope'); };
  const anchor = (v) => { relative(v.file); need(Number.isSafeInteger(v.line) && v.line > 0, 'invalid line'); text(v.anchor); need(Object.hasOwn(contract.sources, v.file), 'missing source pin'); };
  object(contract, ['version', 'scope', 'sources', 'providers', 'uses', 'loaders']);
  need(contract.version === 1, 'unsupported version'); scope(contract.scope); scope(options.requiredScope, true);
  for (const key of ['symbols', 'loaders']) need(JSON.stringify([...contract.scope[key]].sort()) === JSON.stringify([...options.requiredScope[key]].sort()), 'adopted scope changed');
  need(contract.scope.symbols.every((s) => /^[A-Za-z_$][\w$]*$/.test(s)), 'invalid global symbol');
  need(contract.sources && typeof contract.sources === 'object' && !Array.isArray(contract.sources), 'missing sources');
  for (const [file, oid] of Object.entries(contract.sources)) { relative(file); need(/^[a-f0-9]{40}$/.test(oid), 'invalid Git blob'); }
  const ids = new Set();
  for (const key of ['providers', 'uses', 'loaders']) {
    need(Array.isArray(contract[key]), `missing ${key}`);
    for (const r of contract[key]) {
      const common = ['id', 'file', 'line', 'anchor'];
      object(r, common.concat(key === 'providers' ? ['symbol', 'publication', 'publicMembers', 'api', 'compatibility'] : key === 'uses' ? ['symbol', 'member', 'access', 'phase'] : ['kind', 'sourceRoot', 'occurrences', 'order', 'readiness']));
      text(r.id); need(!ids.has(r.id), 'duplicate record id'); ids.add(r.id); anchor(r);
      if (key !== 'loaders') need(contract.scope.symbols.includes(r.symbol), 'symbol outside adopted scope');
      if (key === 'providers') {
        need(r.publication === 'classic-var', 'unsupported publication'); list(r.publicMembers); need(r.publicMembers.length, 'empty public API');
        object(r.api, ['file', 'line', 'anchor']); anchor(r.api); need(r.api.file === r.file, 'API outside provider');
        if (r.compatibility) { object(r.compatibility, ['file', 'line', 'anchor']); anchor(r.compatibility); need(r.compatibility.file === r.file, 'export outside provider'); }
      } else if (key === 'uses') {
        text(r.member); need(['read', 'write'].includes(r.access) && ['load', 'call'].includes(r.phase), 'invalid use');
      } else {
        need(['html', 'vm'].includes(r.kind), 'unsupported loader');
        if (r.kind === 'html') { relative(r.sourceRoot); need(r.occurrences === undefined && r.order === undefined, 'HTML order must be derived'); }
        else {
          need(r.sourceRoot === undefined && Array.isArray(r.occurrences) && r.occurrences.length, 'missing VM occurrences'); list(r.order);
          for (const o of r.occurrences) { object(o, ['id', 'file', 'realm']); text(o.id); relative(o.file); text(o.realm); need(Object.hasOwn(contract.sources, o.file), 'unpinned occurrence'); }
          need(new Set(r.occurrences.map((o) => o.id)).size === r.occurrences.length && r.order.length === r.occurrences.length && r.occurrences.every((o) => r.order.includes(o.id)), 'invalid VM order');
        }
        need(Array.isArray(r.readiness), 'missing readiness facts');
        for (const f of r.readiness) { object(f, ['use', 'after', 'file', 'line', 'anchor']); text(f.use); list(f.after); need(f.after.length, 'empty readiness'); anchor(f); }
      }
    }
  }
  need(contract.loaders.length === contract.scope.loaders.length && contract.loaders.every((l) => contract.scope.loaders.includes(l.id)), 'required loader missing');
  const required = options.requiredScope.applicability, applications = new Set();
  need(Array.isArray(required) && required.length > 0, 'independently adopted applicability is required');
  for (const r of required) {
    object(r, ['loader', 'use', 'realm']); text(r.loader); text(r.use); text(r.realm);
    need(contract.scope.loaders.includes(r.loader) && contract.uses.some((u) => u.id === r.use), 'unknown applicability loader/use');
    const key = JSON.stringify([r.loader, r.use, r.realm]);
    need(!applications.has(key), 'duplicate applicability'); applications.add(key);
  }
  need(contract.uses.every((u) => required.some((r) => r.use === u.id)), 'use lacks adopted applicability');
  validateProfile(options.profile);
}

function checkClassicScripts(contract, options = {}) {
  const violations = [], dependencies = [], requirements = [], coverage = [], edgeOrigins = [];
  const report = () => ({ ok: !violations.length, classicOk: !violations.some((v) => v.rule !== 'global-state'), violations, dependencies, requirements, coverage, edgeOrigins });
  const issue = (rule, record, message, detail = {}) => violations.push({ rule, file: record?.file || null, line: record?.line || null, message, detail });
  let root;
  try { validate(contract, options); root = fs.realpathSync(options.root); }
  catch (error) { issue('invalid-classic-contract', null, error.message); return report(); }
  const sources = new Map(), owners = new Map(), physical = new Map(), streams = new Map();
  const delimiterPairs = (tokens) => {
    const result = new Map(), stack = [];
    tokens.forEach((t, i) => {
      if (t.type !== 'punct') return;
      if (['(', '{', '['].includes(t.value)) stack.push(i);
      if ([')', '}', ']'].includes(t.value)) { const start = stack.pop(); result.set(start, i); }
    });
    return result;
  };
  try {
    for (const [file, oid] of Object.entries(contract.sources)) {
      const absolute = fs.realpathSync(path.join(root, file));
      if (path.relative(root, absolute).startsWith('..') || absolute !== path.join(root, file)) throw new Error(`noncanonical source ${file}`);
      const bytes = fs.readFileSync(absolute);
      if (blob(bytes) !== oid) { issue('stale-source', { file }, 'Reviewed Git blob differs'); continue; }
      sources.set(file, bytes.toString('utf8'));
    }
    for (const m of options.profile.modules) for (const f of m.files) {
      const file = path.posix.join(m.dir, f), absolute = fs.realpathSync(path.join(root, file));
      if (physical.has(absolute)) throw new Error('multiple physical file owners');
      physical.set(absolute, file); owners.set(file, { module: m, public: m.publicApi.includes(f) });
      if (!sources.has(file)) { issue('missing-source', { file }, 'Every analyzed profile file needs a valid full-file pin'); continue; }
      const source = sources.get(file);
      new Script(source, { filename: file }); // Parse only; never execute product code.
      streams.set(file, tokenize(source));
    }
  } catch (error) { issue('source-unavailable', null, error.message); }
  if (violations.length) return report();
  const anchored = (r) => {
    const source = sources.get(r.file), start = source?.indexOf(r.anchor);
    if (start === undefined || start < 0 || source.indexOf(r.anchor, start + 1) >= 0 || source.slice(0, start).split('\n').length !== r.line) {
      issue('invalid-source-anchor', r, 'Anchor must identify one exact source occurrence'); return -1;
    }
    return start;
  };
  const claimed = new Map();
  const claim = (r, symbol) => {
    const start = anchored(r), tokens = streams.get(r.file);
    if (!tokens) { issue('unprofiled-classic-source', r, 'Provider/use must have one profile owner'); return null; }
    const found = tokens.filter((t) => t.type === 'name' && t.value === symbol && t.offset >= start && t.offset < start + r.anchor.length);
    if (start < 0 || found.length !== 1) { issue('invalid-source-anchor', r, 'Anchor must cover exactly one scoped binding'); return null; }
    const token = found[0], key = `${r.file}:${token.offset}`;
    if (claimed.has(key)) issue('conflicting-binding', r, 'Binding claimed twice');
    claimed.set(key, r.id); return { tokens, index: tokens.indexOf(token), token };
  };
  const providers = new Map();
  for (const p of contract.providers) {
    if (providers.has(p.symbol)) issue('conflicting-provider', p, 'Multiple provider declarations');
    providers.set(p.symbol, p);
    const binding = claim(p, p.symbol);
    if (binding) {
      const { tokens: t, index: i } = binding;
      const close = delimiterPairs(t), body = i + 6, end = close.get(body);
      const depth = t.slice(0, i).reduce((n, v) => n + (v.type === 'punct' && v.value === '{' ? 1 : v.type === 'punct' && v.value === '}' ? -1 : 0), 0);
      const opening = t.slice(i + 1, body + 1).map((v) => v.value).join(' ');
      const closing = t.slice(end + 1, end + 5).map((v) => v.value).join(' ');
      if (depth || (i > 1 && t[i - 2]?.value !== ';') || t[i - 1]?.value !== 'var' ||
          opening !== '= ( function ( ) {' || closing !== ') ( ) ;') {
        issue('unsupported-binding', p, 'Provider must be an unconditional top-level classic var IIFE');
      }
      const apiStart = anchored(p.api), at = t.findIndex((v) => v.offset === apiStart);
      const apiEnd = close.get(at + 1);
      if (at < body || t[at]?.value !== 'return' || t[at + 1]?.value !== '{' || t[apiEnd + 1]?.value !== ';' || apiEnd + 2 !== end) issue('invalid-public-members', p, 'API must be the final executable IIFE return');
      // Only declarations/directives may precede the return; conditional or
      // effectful initialization cannot certify installation of this pure API.
      for (let j = body + 1; j < at;) {
        if (t[j].type === 'string' && t[j + 1]?.value === ';') { j += 2; continue; }
        if (t[j]?.value !== 'function' || t[j + 1]?.type !== 'name' || t[j + 2]?.value !== '(') {
          issue('unsupported-binding', p, 'Provider initialization must contain only named functions/directives'); break;
        }
        const bodyStart = close.get(j + 2) + 1, bodyEnd = close.get(bodyStart);
        if (t[bodyStart]?.value !== '{' || bodyEnd === undefined) { issue('unsupported-binding', p, 'Unsupported provider function'); break; }
        j = bodyEnd + 1;
      }
    }
    anchored(p.api);
    const api = /^return\s*\{\s*([\s\S]*?)\s*\};?$/.exec(p.api.anchor.trim());
    const pairs = api?.[1].split(',').map((s) => /^\s*([\w$]+)\s*:\s*([\w$]+)\s*$/.exec(s));
    if (!pairs?.length || pairs.some((v) => !v) || JSON.stringify(pairs.map((v) => v[1]).sort()) !== JSON.stringify([...p.publicMembers].sort())) issue('invalid-public-members', p, 'Reviewed literal return must match publicMembers');
    if (pairs?.every(Boolean) && binding) {
      const t = binding.tokens, nesting = [];
      const locals = new Set();
      t.forEach((v, i) => {
        if (v.type === 'punct' && v.value === '{') nesting.push(i);
        if (v.type === 'punct' && v.value === '}') nesting.pop();
        if (nesting.length === 1 && v.value === 'function' && t[i + 1]?.type === 'name') locals.add(t[i + 1].value);
      });
      if (pairs.some((v) => !locals.has(v[2]))) issue('invalid-public-members', p, 'Public members must name provider-local functions');
    }
    if (p.compatibility) {
      const c = p.compatibility;
      if (c.anchor.trim() !== `if (typeof module !== 'undefined' && module.exports) module.exports = ${p.symbol};`) issue('unsupported-binding', c, 'Unsupported compatibility export');
      claim({ ...c, id: `${p.id}.compatibility` }, p.symbol);
    }
  }
  for (const symbol of contract.scope.symbols) if (!providers.has(symbol)) issue('missing-provider', null, `No provider for ${symbol}`);
  const edges = new Map(options.profile.modules.map((m) => [m.id, new Set()]));
  const edge = (from, to, record, origin) => {
    const a = owners.get(from), b = owners.get(to);
    if (!a || !b) { issue('unprofiled-classic-source', record, 'Dependency endpoint has no owner'); return; }
    if (a.module.id === b.module.id) return;
    edges.get(a.module.id).add(b.module.id); edgeOrigins.push({ from: a.module.id, to: b.module.id, fromFile: from, toFile: to, origin });
    if (!b.public) issue('private-import', record, 'Provider file is not public', { targetFile: to });
    const allowed = options.profile.layerRules?.[a.module.layer]?.allowedLayers;
    if (allowed && !allowed.includes('*') && !allowed.includes(b.module.layer)) issue('layer-violation', record, 'Forbidden dependency layer', { fromLayer: a.module.layer, toLayer: b.module.layer });
  };
  for (const use of contract.uses) {
    const p = providers.get(use.symbol), binding = claim(use, use.symbol);
    if (binding) {
      const { tokens: t, index: i } = binding, tail = t[i + 3]?.value;
      if (t[i - 1]?.value === '.' || t[i + 1]?.value !== '.' || t[i + 2]?.type !== 'name' || t[i + 2].value !== use.member || ![';', '('].includes(tail)) issue('unsupported-binding', use, 'Only direct bare-global member access is supported');
      const writes = ['?', '<', '>', '=', '++', '--', '+', '-', '*', '/', '/=', '%', '&', '|', '^'].includes(tail) || ['delete', '++', '--'].includes(t[i - 1]?.value);
      if (writes || use.access === 'write') issue('unauthorized-global-write', use, 'Production consumers cannot modify scoped globals');
    }
    if (!p) { issue('missing-provider', use, 'Use has no provider'); continue; }
    if (!p.publicMembers.includes(use.member)) issue('private-global-member', use, 'Member is not public');
    dependencies.push({ fromFile: use.file, toFile: p.file, symbol: use.symbol, member: use.member, phase: use.phase, use: use.id });
    edge(use.file, p.file, use, 'classic');
  }
  for (const [file, tokens] of streams) {
    const globalNames = new Set();
    for (const [i, token] of tokens.entries()) {
      if (token.type === 'name' && (token.value === 'eval' || (token.value === 'with' && tokens[i + 1]?.value === '('))) issue('unsupported-binding', { file, line: token.line }, 'Dynamic scope requires binding analysis');
      if (token.type === 'name' && contract.scope.symbols.includes(token.value) && !claimed.has(`${file}:${token.offset}`)) issue('unmodeled-global-use', { file, line: token.line }, `Unmodeled ${token.value}`);
      if (token.value === 'window' && ['.', '?.'].includes(tokens[i + 1]?.value) && /^SM\w*$/.test(tokens[i + 2]?.value) && !['adapters', 'bootstrap'].includes(owners.get(file).module.layer)) {
        const name = `window.${tokens[i + 2].value}`;
        if (!globalNames.has(name)) issue('global-state', { file, line: token.line }, `Retained ${name} restriction`, { global: name, module: owners.get(file).module.id });
        globalNames.add(name);
      }
      if (['window', 'globalThis'].includes(token.value) && (tokens[i + 1]?.value === '[' || (tokens[i + 1]?.value === '?.' && tokens[i + 2]?.value === '['))) issue('unsupported-global', { file, line: token.line }, 'Computed global access requires binding analysis');
    }
    coverage.push({ file, scopedBindings: [...claimed.keys()].filter((k) => k.startsWith(`${file}:`)).length });
    try {
      for (const imp of extractImports(sources.get(file))) {
        const target = resolveImport(imp.specifier, path.join(root, file));
        if (target.external) continue;
        const to = physical.get(target.path);
        if (target.rule || !to) issue(target.rule || 'unprofiled-local-import', { file, line: imp.line }, target.message || 'Import target outside profile');
        else edge(file, to, { file, line: imp.line }, 'import');
      }
    } catch (error) { issue('unsupported-import', { file }, error.message); }
  }
  for (const cycle of findCycles(edges)) issue('cycle', null, 'Dependency cycle', { path: cycle });
  for (const loader of contract.loaders) {
    anchored(loader);
    let occurrences, order;
    try {
      if (loader.kind === 'html') {
        if (typeof options.inventoryHTML !== 'function') throw new Error('synchronous inventoryHTML prerequisite is required');
        const inventory = options.inventoryHTML(sources.get(loader.file), { root, htmlFile: loader.file, sourceRoot: loader.sourceRoot });
        if (inventory?.then) { Promise.resolve(inventory).catch(() => {}); throw new Error('HTML inventory must be synchronous'); }
        if (!inventory || !Array.isArray(inventory.scripts) || !Array.isArray(inventory.eagerOrder) || !Array.isArray(inventory.diagnostics)) throw new Error('invalid HTML inventory');
        if (inventory.scripts.some((s) => !s || typeof s.id !== 'string' || !s.id) || inventory.eagerOrder.some((id) => typeof id !== 'string')) throw new Error('invalid HTML occurrence IDs');
        for (const diagnostic of inventory.diagnostics) issue('html-inventory', loader, 'HTML inventory diagnostic', diagnostic);
        occurrences = inventory.scripts.map((s) => ({ id: s.id, file: s.src?.path, realm: loader.id, ready: s.kind === 'classic' && s.context === 'document' && s.mode === 'external' && s.execution === 'eager-classic' && s.src?.status === 'local' }));
        order = inventory.eagerOrder;
        if (JSON.stringify(order) !== JSON.stringify(inventory.scripts.filter((s) => s.execution === 'eager-classic').map((s) => s.id))) throw new Error('HTML eager order differs from source occurrences');
      } else { occurrences = loader.occurrences.map((o) => ({ ...o, ready: true })); order = loader.order; }
      if (new Set(occurrences.map((o) => o.id)).size !== occurrences.length || new Set(order).size !== order.length || order.some((id) => !occurrences.some((o) => o.id === id))) throw new Error('invalid occurrence identity/order');
    } catch (error) { issue('loader-unavailable', loader, error.message); continue; }
    const required = options.requiredScope.applicability.filter((r) => r.loader === loader.id);
    for (const r of required) {
      const use = contract.uses.find((u) => u.id === r.use);
      if (!occurrences.some((o) => o.file === use.file && o.realm === r.realm)) {
        issue('missing-loader-consumer', loader, 'Adopted use must execute in this loader and realm', r);
      }
    }
    for (const p of providers.values()) for (const realm of new Set(occurrences.map((o) => o.realm))) {
      if (occurrences.filter((o) => o.realm === realm && o.file === p.file).length > 1) issue('duplicate-provider-installation', loader, 'Provider loaded twice in one realm', { symbol: p.symbol, realm });
    }
    for (const fact of loader.readiness) {
      anchored(fact);
      if (!contract.uses.some((u) => u.id === fact.use && u.phase === 'call') || fact.after.some((id) => !occurrences.some((o) => o.id === id))) issue('invalid-readiness', loader, 'Readiness must identify a call use and loader occurrences');
    }
    for (const use of contract.uses) for (const consumer of occurrences.filter((o) => o.file === use.file)) {
      if (!required.some((r) => r.use === use.id && r.realm === consumer.realm)) {
        issue('unadopted-loader-consumer', loader, 'Consumer realm lacks adopted applicability', { loader: loader.id, use: use.id, realm: consumer.realm, consumer: consumer.id });
      }
      const provider = providers.get(use.symbol);
      const candidates = occurrences.filter((o) => o.file === provider?.file && o.realm === consumer.realm);
      if (candidates.length !== 1) { issue('missing-loader-provider', loader, 'Consumer needs exactly one provider in its realm', { use: use.id, consumer: consumer.id }); continue; }
      const before = candidates[0], providerIndex = order.indexOf(before.id), consumerIndex = order.indexOf(consumer.id);
      const requirement = { loader: loader.id, use: use.id, before: before.id, after: consumer.id, realm: consumer.realm, phase: use.phase };
      requirements.push(requirement);
      if (!before.ready || !consumer.ready || providerIndex < 0 || consumerIndex < 0) issue('unknown-readiness', loader, 'Non-eager scheduling is not evidence of availability', requirement);
      else if (use.phase === 'load' && providerIndex >= consumerIndex) issue('load-order', loader, 'Provider must precede eager consumer', requirement);
      else if (use.phase === 'call' && !loader.readiness.some((f) => f.use === use.id && f.after.includes(before.id) && f.after.includes(consumer.id))) issue('unknown-readiness', loader, 'Call use needs reviewed invocation-boundary evidence', requirement);
    }
  }
  return report();
}

module.exports = { checkClassicScripts };
