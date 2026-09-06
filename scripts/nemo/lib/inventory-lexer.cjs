'use strict';
// Lexical regions, spans and binding ownership for inventory source analysis.
// Source text is supplied by the caller; offsets remain those of that source.

function lineOf(text, offset) { let n = 1; for (let i = 0; i < offset; i++) if (text.charCodeAt(i) === 10) n++; return n; }
function uniq(a) { return Array.from(new Set(a)); }
function esc(s) { return String(s).replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }

// ---------------------------------------------------------------------------
// Lexical regions. Every offset of a file is code, comment or literal (string,
// template, regex). `code` is the source with comments blanked to spaces
// (newlines kept, so offsets and line numbers are those of the file); `kind`
// says what each offset is. Passes run on `code` and reject any match that
// starts inside a literal, so a lookup or listener quoted in a comment or a
// string is never a binding. Template `${…}` expressions are code.
// ---------------------------------------------------------------------------
const CODE = 0, COMMENT = 1, LITERAL = 2;
const REGEX_PREV_CHAR = /[(,=:[!&|?{};+\-*%<>~^]/;
const REGEX_PREV_WORD = /^(?:return|typeof|case|do|else|in|of|new|delete|void|throw|instanceof|yield|await)$/;
function lexRegions(text) {
  const n = text.length;
  const kind = new Uint8Array(n);
  const out = text.split('');
  const mark = (a, b, k, blank) => { for (let j = a; j < b && j < n; j++) { kind[j] = k; if (blank && text[j] !== '\n') out[j] = ' '; } };
  let i = 0, depth = 0, lastSig = -1, inTemplate = false;
  const tmpl = []; // brace depth at each `${` still open
  const regexAllowed = () => {
    if (lastSig < 0) return true;
    const c = text[lastSig];
    if (REGEX_PREV_CHAR.test(c)) return true;
    if (/[\w$]/.test(c)) { let s = lastSig; while (s > 0 && /[\w$]/.test(text[s - 1])) s--; return REGEX_PREV_WORD.test(text.slice(s, lastSig + 1)); }
    return false;
  };
  while (i < n) {
    const c = text[i];
    if (inTemplate) {
      if (c === '\\') { mark(i, i + 2, LITERAL); i += 2; continue; }
      if (c === '`') { kind[i] = LITERAL; inTemplate = false; lastSig = i; i++; continue; }
      if (c === '$' && text[i + 1] === '{') { mark(i, i + 2, LITERAL); tmpl.push(depth); depth++; inTemplate = false; i += 2; continue; }
      kind[i] = LITERAL; i++; continue;
    }
    const d = text[i + 1];
    if (c === '/' && d === '/') { let e = text.indexOf('\n', i); if (e < 0) e = n; mark(i, e, COMMENT, true); i = e; continue; }
    if (c === '/' && d === '*') { let e = text.indexOf('*/', i + 2); e = e < 0 ? n : e + 2; mark(i, e, COMMENT, true); i = e; continue; }
    if (c === '\'' || c === '"') {
      let j = i + 1;
      while (j < n && text[j] !== c && text[j] !== '\n') { if (text[j] === '\\') j++; j++; }
      mark(i, j + 1, LITERAL); lastSig = Math.min(j, n - 1); i = j + 1; continue;
    }
    if (c === '`') { kind[i] = LITERAL; inTemplate = true; i++; continue; }
    if (c === '/' && regexAllowed()) {
      let j = i + 1, cls = false, ok = false;
      while (j < n) {
        const ch = text[j];
        if (ch === '\\') { j += 2; continue; }
        if (ch === '\n') break;
        if (cls) { if (ch === ']') cls = false; } else if (ch === '[') cls = true; else if (ch === '/') { ok = true; break; }
        j++;
      }
      if (ok) { j++; while (j < n && /[a-z]/i.test(text[j])) j++; mark(i, j, LITERAL); lastSig = j - 1; i = j; continue; }
    }
    if (c === '{') depth++;
    else if (c === '}') {
      if (tmpl.length && depth - 1 === tmpl[tmpl.length - 1]) { tmpl.pop(); depth--; kind[i] = LITERAL; inTemplate = true; i++; continue; }
      depth--;
    }
    if (!/\s/.test(c)) lastSig = i;
    i++;
  }
  return { code: out.join(''), kind };
}

// Offset of the bracket closing the one at `open`, skipping literals.
function matchClose(f, open) {
  const { code, kind } = f;
  const o = code[open], c = o === '(' ? ')' : o === '[' ? ']' : '}';
  let depth = 0;
  for (let i = open; i < code.length; i++) {
    if (kind[i] !== CODE) continue;
    const ch = code[i];
    if (ch === o) depth++;
    else if (ch === c) { depth--; if (depth === 0) return i; }
  }
  return code.length - 1;
}
// Body span of the function whose parameter list or arrow starts at/after `from`.
function functionBodyAt(f, from) {
  for (let i = from; i < f.code.length; i++) if (f.kind[i] === CODE && f.code[i] === '{') return { start: i, end: matchClose(f, i) };
  return null;
}
// Same on a detached comment-free slice (menu-item literals): strings skipped.
function bodySpanInText(text, from) {
  const open = text.indexOf('{', from);
  if (open < 0) return null;
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    const c = text[i];
    if (c === '\'' || c === '"' || c === '`') { const q = c; for (i++; i < text.length; i++) { if (text[i] === '\\') { i++; continue; } if (text[i] === q) break; if (q !== '`' && text[i] === '\n') break; } continue; }
    if (c === '{') depth++;
    else if (c === '}') { depth--; if (depth === 0) return { start: open, end: i }; }
  }
  return { start: open, end: text.length - 1 };
}

// ---------------------------------------------------------------------------
// JS index: named functions and their bodies
// ---------------------------------------------------------------------------
const FN_DECL = /(?:^|[^\w$.])function\s+([A-Za-z_$][\w$]*)\s*\(|([A-Za-z_$][\w$]*)\s*:\s*function\s*\(|(?:^|[^\w$.])(?:var|let|const)?\s*([A-Za-z_$][\w$]*)\s*=\s*function\s*\(|(?:^|[^\w$])([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\([^()]*\)\s*=>\s*\{|(?:^|[^\w$])([A-Za-z_$][\w$]*)\s*:\s*(?:async\s*)?\([^()]*\)\s*=>\s*\{/g;

function indexFile(file, text) {
  const { code, kind } = lexRegions(text);
  const f = { file, text, code, kind, fns: [] };
  FN_DECL.lastIndex = 0;
  let m;
  while ((m = FN_DECL.exec(code))) {
    const name = m[1] || m[2] || m[3] || m[4] || m[5];
    const at = m.index + m[0].indexOf(name);
    if (kind[at] !== CODE) continue;
    const body = functionBodyAt(f, m.index + m[0].length - 1);
    if (!body) continue;
    const paren = code.indexOf('(', at);
    const params = paren >= 0 && paren < body.start ? code.slice(paren + 1, matchClose(f, paren)).split(',').map((x) => x.trim().replace(/=[\s\S]*$/, '').trim()).filter(Boolean) : [];
    f.fns.push({ name, params, start: body.start, end: body.end, line: lineOf(code, at), declStart: m.index });
  }
  indexBindingScopes(f);
  return f;
}

// Binding ownership only, not a general JavaScript parser. Braces delimit
// lexical declarations; function/arrow parameters and `var` belong to their
// function. Keep anonymous scopes separate from the named reporting index.
function declarationContinuesAt(f, start, newline, initialized) {
  const before = f.code.slice(start, newline).trimEnd();
  const after = f.code.slice(newline + 1).trimStart();
  // An uninitialized declarator can continue only into its initializer or
  // another declarator: `let n\n +value` starts a separate expression.
  if (/^[=,]/.test(after)) return true;
  if (!initialized) return false;
  const last = start + before.length - 1;
  if (f.kind[last] === CODE) {
    if (/[=+*/%&|^!~?:.<>-]$/.test(before) && !/(?:\+\+|--)$/.test(before)) return true;
    const keyword = /\b(?:new|delete|void|typeof|in|instanceof)$/.exec(before);
    // Exclude property names and $-prefixed identifiers, not a binary
    // operator's identifier operand (`value instanceof\n Constructor`).
    if (keyword && before[keyword.index - 1] !== '$' && !/\.$/.test(before.slice(0, keyword.index).trimEnd())) return true;
  }
  // A complete initializer may continue with an access, call or operator.
  // Prefix ++/-- have a restricted line break and start a new statement.
  return !/^(?:\+\+|--)/.test(after) && /^(?:[([.?+*/%&|^<>:-]|`|(?:in|instanceof)\b)/.test(after);
}

function indexBindingScopes(f) {
  f.rootScope = { start: 0, end: f.code.length, function: true, params: [] };
  f.scopes = [f.rootScope];
  const stack = [];
  for (let i = 0; i < f.code.length; i++) {
    if (f.kind[i] !== CODE) continue;
    if (f.code[i] === '{') stack.push(i);
    else if (f.code[i] === '}' && stack.length) f.scopes.push({ start: stack.pop(), end: i });
  }
  const functions = /\bfunction\s*(?:[A-Za-z_$][\w$]*\s*)?\(([^()]*)\)|(?:\(([^()]*)\)|\b([A-Za-z_$][\w$]*))\s*=>/g;
  let m;
  while ((m = functions.exec(f.code))) {
    if (f.kind[m.index] !== CODE) continue;
    let start = m.index + m[0].length;
    while (/\s/.test(f.code[start] || '') && start < f.code.length) start++;
    let scope = f.scopes.find((s) => s.start === start && s !== f.rootScope);
    if (!scope && m[1] === undefined) {
      // Concise arrow body: stop at its containing delimiter, not the next `{`.
      let end = start;
      for (; end < f.code.length; end++) {
        if (f.kind[end] !== CODE) continue;
        if (/[([{]/.test(f.code[end])) end = matchClose(f, end);
        else if (/[),;\]}\n]/.test(f.code[end])) break;
      }
      scope = { start, end: end - 1 };
      f.scopes.push(scope);
    }
    if (scope) {
      scope.function = true;
      scope.params = (m[1] ?? m[2] ?? m[3] ?? '').split(',').map((p) => p.trim().replace(/=[\s\S]*$/, '').trim());
    }
  }
  f.scopes.sort((a, b) => (a.end - a.start) - (b.end - b.start));
  f.declarations = [];
  for (const scope of f.scopes) for (const name of scope.params || []) f.declarations.push({ name, scope });
  const decl = /\b(var|let|const|function)\s+([A-Za-z_$][\w$]*)/g;
  while ((m = decl.exec(f.code))) {
    if (f.kind[m.index] !== CODE) continue;
    const scope = f.scopes.find((s) => s.start <= m.index && m.index <= s.end && (m[1] !== 'var' || s.function));
    f.declarations.push({ name: m[2], scope });
    if (m[1] === 'function') continue;
    // Subsequent declarators share this owner. Ignore commas within an
    // initializer's calls, arrays or objects; they do not start declarations.
    let initialized = false;
    for (let i = m.index + m[0].length; i < f.code.length; i++) {
      const c = f.code[i];
      if (f.kind[i] !== CODE && !(f.kind[i] === COMMENT && c === '\n')) continue;
      if (/[([{]/.test(c)) { i = matchClose(f, i); continue; }
      if (/[;)}\]]/.test(c)) break;
      if (c === '\n' && !declarationContinuesAt(f, m.index, i, initialized)) break;
      if (c === '=') initialized = true;
      if (c !== ',') continue;
      const next = /^\s*([A-Za-z_$][\w$]*)\s*(?==|,|;|$)/.exec(f.code.slice(i + 1));
      if (!next) break; // unsupported binding pattern: do not guess a name
      const at = i + 1 + next[0].indexOf(next[1]);
      if (f.kind[at] !== CODE) break;
      f.declarations.push({ name: next[1], scope });
      initialized = false;
      i = at + next[1].length - 1;
    }
  }
}

function bindingOwner(f, name, at) {
  const owners = f.declarations.filter((d) => d.name === name && d.scope.start <= at && at <= d.scope.end).map((d) => d.scope);
  return owners.sort((a, b) => (a.end - a.start) - (b.end - b.start))[0] || f.rootScope;
}

function enclosingFunction(f, offset) {
  let best = null;
  for (const fn of f.fns) {
    if (fn.start <= offset && offset <= fn.end) {
      if (!best || (fn.end - fn.start) < (best.end - best.start)) best = fn;
    }
  }
  return best;
}

function createFunctionIndex(idx) {
  // global function index (first definition wins; classic scripts share one scope)
  const fnEntry = (f, fn) => ({ file: f.file, f, name: fn.name, body: f.code.slice(fn.start, fn.end + 1), line: fn.line, params: fn.params, start: fn.start, end: fn.end, owner: bindingOwner(f, fn.name, fn.declStart + 1) });
  const allFns = idx.flatMap((f) => f.fns.map((fn) => fnEntry(f, fn)));
  const fnIndex = new Map();
  for (const fn of allFns) if (!fnIndex.has(fn.name)) fnIndex.set(fn.name, fn);
  // Resolve at the call site. A same-name helper in a sibling function is not
  // visible; ambiguous global definitions are deliberately not guessed.
  const resolveFn = (f, name, at) => {
    const owner = bindingOwner(f, name, at);
    const local = allFns.filter((fn) => fn.f === f && fn.name === name && fn.owner === owner);
    if (local.length) return local.length === 1 ? local[0] : null;
    if (owner !== f.rootScope || f.declarations.some((d) => d.name === name && d.scope === owner)) return null;
    const global = allFns.filter((fn) => fn.name === name && fn.owner === fn.f.rootScope);
    return global.length === 1 ? global[0] : null;
  };

  return { allFns, fnIndex, resolveFn };
}

const paramLookupRe = (prm) => new RegExp("getElementById\\(\\s*" + esc(prm) + "\\s*\\)|querySelector\\(\\s*'#'\\s*\\+\\s*" + esc(prm) + "(?![\\w$])");
function isWs(c) { return c === ' ' || c === '\n' || c === '\t' || c === '\r'; }
function skipWs(f, i) { while (i < f.code.length && (isWs(f.code[i]) || f.kind[i] === COMMENT)) i++; return i; }
function stringLiteral(t) { const m = /^'((?:[^'\\]|\\.)*)'$|^"((?:[^"\\]|\\.)*)"$/.exec(t); return m ? (m[1] !== undefined ? m[1] : m[2]) : null; }
function firstParam(s) { const p = (s || '').split(',')[0].trim().replace(/=[\s\S]*$/, '').trim(); return /^[A-Za-z_$][\w$]*$/.test(p) ? p : null; }
// Top-level argument spans of the call whose '(' is at `open`.
function argsOf(f, open) {
  const close = matchClose(f, open);
  const spans = [];
  let depth = 0, s = open + 1;
  for (let i = open + 1; i < close; i++) {
    if (f.kind[i] !== CODE) continue;
    const ch = f.code[i];
    if (ch === '(' || ch === '[' || ch === '{') depth++;
    else if (ch === ')' || ch === ']' || ch === '}') depth--;
    else if (ch === ',' && depth === 0) { spans.push([s, i]); s = i + 1; }
  }
  if (spans.length || f.code.slice(s, close).trim()) spans.push([s, close]);
  const args = spans.map(([a, b]) => { const start = skipWs(f, a); return { start, end: b, text: f.code.slice(start, b).trim() }; });
  return { args, close };
}
function statementStart(f, at) {
  for (let i = at - 1; i >= 0; i--) { if (f.kind[i] !== CODE) continue; const c = f.code[i]; if (c === ';' || c === '{' || c === '}') return i + 1; }
  return 0;
}
function statementEnd(f, k) {
  let depth = 0;
  for (let i = k; i < f.code.length; i++) {
    if (f.kind[i] !== CODE) continue;
    const c = f.code[i];
    if (c === '(' || c === '[' || c === '{') depth++;
    else if (c === ')' || c === ']' || c === '}') { if (depth === 0) return i; depth--; }
    else if (depth === 0 && (c === ';' || c === '\n' || c === ',')) return i;
  }
  return f.code.length;
}
function parameterIntact(f, span, name, at) {
  const owner = bindingOwner(f, name, span.start);
  if (bindingOwner(f, name, at) !== owner) return false;
  const re = new RegExp('(?:^|[^\\w$.])(' + esc(name) + ')\\s*(?:=(?!=|>)|(?:&&|\\|\\||\\?\\?|\\*\\*|<<|>>>?|[+*/%&|^-])=|\\+\\+|--)|(?:\\+\\+|--)\\s*(' + esc(name) + ')(?![\\w$])', 'g');
  let m;
  const body = f.code.slice(span.start, at);
  while ((m = re.exec(body))) {
    const u = span.start + m.index + m[0].indexOf(name);
    if (f.kind[u] === CODE && bindingOwner(f, name, u) === owner) return false;
  }
  // Destructuring does not put '=' after the identifier. Conservatively
  // reject a same-owner name in an assignment pattern; replacement values
  // and complex target expressions are not propagated by this analyzer.
  const patterns = /[\[{]/g;
  while ((m = patterns.exec(body))) {
    const open = span.start + m.index;
    if (f.kind[open] !== CODE) continue;
    const close = matchClose(f, open), after = skipWs(f, close + 1);
    if (close >= at || f.code[after] !== '=' || /[=>]/.test(f.code[after + 1])) continue;
    const names = new RegExp('(?:^|[^\\w$])(' + esc(name) + ')(?![\\w$])', 'g');
    const pattern = f.code.slice(open + 1, close);
    let target;
    while ((target = names.exec(pattern))) {
      const u = open + 1 + target.index + target[0].length - name.length;
      if (f.kind[u] !== CODE || f.code[skipWs(f, u + name.length)] === ':') continue;
      if (bindingOwner(f, name, u) === owner) return false;
    }
  }
  return true;
}
function paramLookups(f, span, prm) {
  const re = new RegExp(paramLookupRe(prm).source, 'g');
  const out = [];
  let m;
  while ((m = re.exec(f.code.slice(span.start, span.end + 1)))) {
    const at = span.start + m.index;
    if (f.kind[at] === CODE && bindingOwner(f, prm, at) === bindingOwner(f, prm, span.start)) out.push(at);
  }
  return out;
}
function returnsOf(fn) {
  const out = [], re = /\breturn\b/g;
  let m;
  while ((m = re.exec(fn.body))) {
    const at = fn.start + m.index;
    if (fn.f.kind[at] !== CODE) continue;
    const scope = fn.f.scopes.find((s) => s.function && s.start <= at && at <= s.end);
    if (!scope || scope.start !== fn.start) continue;
    out.push({ at, text: fn.f.code.slice(at, statementEnd(fn.f, at)).trim() });
  }
  return out;
}
const subjectStartAt = (f, at) => { let s = at; if (f.code.slice(s - 9, s) === 'document.') { s -= 9; if (f.code.slice(s - 7, s) === 'window.') s -= 7; } return s; };


module.exports = {
  CODE, lexRegions, indexFile, matchClose, functionBodyAt, bodySpanInText,
  bindingOwner, enclosingFunction, createFunctionIndex, lineOf, uniq, esc,
  isWs, skipWs, stringLiteral, firstParam, argsOf, statementStart, statementEnd,
  parameterIntact, paramLookupRe, paramLookups, returnsOf, subjectStartAt,
};
