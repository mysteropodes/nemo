'use strict';
// Event registrations reached from element lookups, with per-build helper caches.
const {
  CODE, bindingOwner, enclosingFunction, createFunctionIndex, lineOf, uniq, esc,
  isWs, skipWs, stringLiteral, firstParam, argsOf, statementStart, statementEnd,
  parameterIntact, paramLookupRe, paramLookups, returnsOf, subjectStartAt,
  matchClose, functionBodyAt,
} = require('./inventory-lexer.cjs');
const { recordExportedRecordArrayBindings } = require('./inventory-records.cjs');

function createBindings(idx, domRows, rel) {
  const { allFns, fnIndex, resolveFn } = createFunctionIndex(idx);

  // Functions that take an element id as a parameter (`function el(id){return document.getElementById(id)}`,
  // `wireBoolBtn(id, op)`): a call `name('literal')` is a lookup of that id.
  const idTakers = new Map(); // lexical function entry -> parameter index
  for (const fn of allFns) {
    fn.params.forEach((prm, i) => {
      if (idTakers.has(fn) || !/^[A-Za-z_$][\w$]*$/.test(prm)) return;
      if (paramLookups(fn.f, fn, prm).length) idTakers.set(fn, i);
    });
  }
  // Calls of known id-takers inside [span] of `f` that pass `prm` at the taker's id position.
  function takerCallsWithParam(f, span, prm, selfName) {
    const out = [];
    const re = /(?:^|[^\w$.])([A-Za-z_$][\w$]*)\s*\(/g;
    const body = f.code.slice(span.start, span.end + 1);
    let m;
    while ((m = re.exec(body))) {
      const at = span.start + m.index + m[0].indexOf(m[1]);
      const fn = resolveFn(f, m[1], at);
      if (!idTakers.has(fn) || fn === selfName) continue;
      const open = span.start + m.index + m[0].length - 1;
      if (f.kind[open] !== CODE) continue;
      const { args, close } = argsOf(f, open);
      const a = args[idTakers.get(fn)];
      if (a && a.text === prm && parameterIntact(f, span, prm, a.start)) out.push({ fn, at, close });
    }
    return out;
  }
  // A helper that forwards its parameter to an id-taker (`bindWidgetField(id, apply)` → `el(id)`) is one too.
  for (let round = 0; round < 4; round++) {
    let added = 0;
    for (const fn of allFns) {
      if (idTakers.has(fn)) continue;
      fn.params.forEach((prm, i) => {
        if (idTakers.has(fn) || !/^[A-Za-z_$][\w$]*$/.test(prm)) return;
        if (takerCallsWithParam(fn.f, fn, prm, fn).length) { idTakers.set(fn, i); added++; }
      });
    }
    if (!added) break;
  }
  // Zero-argument helpers that return a fixed element (`function macBtn() { return document.getElementById('mac-update-btn'); }`):
  // a call `macBtn()` is a lookup of that id.
  const returners = new Map(); // lexical function entry -> id
  for (const fn of allFns) {
    if (fn.params.length) continue;
    const returns = returnsOf(fn);
    const ids = returns.map((r) => {
      const m = /^return\s+(?:window\.)?document\.getElementById\(\s*(['"])([^'"]+)\1\s*\)\s*$/.exec(r.text);
      return m ? m[2] : null;
    });
    if (ids.length && ids.every((id) => id && id === ids[0])) returners.set(fn, ids[0]);
  }

  // -------------------------------------------------------------------------
  // Binding analysis. A "subject" is an expression that evaluates to an element
  // (or a NodeList): a lookup call, the variable it was assigned to, a helper
  // parameter that receives it, a forEach callback parameter. The analysis
  // returns the event registrations reachable from that subject and the
  // selectors scoped to it; nothing else counts as a binding.
  // -------------------------------------------------------------------------
  // (function declarations: hoisted, so the id-taker pass above can use them)
  function EMPTY() { return { bindings: [], scoped: [] }; }
  function merge(a, b) { a.bindings.push(...b.bindings); a.scoped.push(...b.scoped); return a; }

  // The function an event is bound to: inline (body span) or by name.
  function callbackOf(f, arg) {
    const t = arg.text || '';
    const none = { f, body: '', param0: null, span: null };
    let m;
    if ((m = /^(?:async\s+)?function\b[^(]*\(([^)]*)\)/.exec(t))) {
      const span = functionBodyAt(f, arg.start + m[0].length);
      return { f, body: span ? f.code.slice(span.start, span.end + 1) : '', param0: firstParam(m[1]), span };
    }
    if ((m = /^(?:async\s*)?(?:\(([^)]*)\)|([A-Za-z_$][\w$]*))\s*=>/.exec(t))) {
      const param0 = firstParam(m[1] !== undefined ? m[1] : m[2]);
      const k = skipWs(f, arg.start + m[0].length);
      if (f.code[k] === '{') { const span = { start: k, end: matchClose(f, k) }; return { f, body: f.code.slice(span.start, span.end + 1), param0, span }; }
      return { f, body: f.code.slice(k, arg.end), param0, span: { start: k, end: arg.end - 1 } };
    }
    if ((m = /^([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*(?:\.bind\([\s\S]*\))?$/.exec(t))) {
      const fn = resolveFn(f, m[1].split('.').pop(), arg.start);
      if (fn) return { f: fn.f, body: fn.body, param0: fn.params[0] || null, span: { start: fn.start, end: fn.end } };
    }
    return none;
  }

  // `subject.member…` at the `.` (or `?.`) at `dot`.
  function analyzeMember(f, dot, ctx) {
    const code = f.code, out = EMPTY();
    const i = dot + (code[dot] === '?' ? 2 : 1);
    const m = /^[A-Za-z_$][\w$]*/.exec(code.slice(i, i + 64));
    if (!m) return out;
    const name = m[0], j = skipWs(f, i + name.length);
    if (name === 'addEventListener' && code[j] === '(') {
      const { args } = argsOf(f, j);
      const ev = args[0] ? stringLiteral(args[0].text) : null;
      const cb = callbackOf(f, args[1] || { text: '' });
      out.bindings.push({ event: ev || '(dynamic)', body: cb.body, f, at: dot, via: ctx.via });
    } else if (/^on[a-z]+$/.test(name) && code[j] === '=' && code[j + 1] !== '=') {
      const k = skipWs(f, j + 1), end = statementEnd(f, k);
      const rhs = { start: k, end, text: code.slice(k, end).trim() };
      if (rhs.text && !/^(?:null|undefined)$/.test(rhs.text)) out.bindings.push({ event: name.slice(2), body: callbackOf(f, rhs).body, f, at: dot, via: ctx.via });
    } else if ((name === 'querySelector' || name === 'querySelectorAll') && code[j] === '(') {
      const { args, close } = argsOf(f, j);
      const sel = args[0] ? stringLiteral(args[0].text) : null;
      if (sel) out.scoped.push({ selector: sel, multi: name === 'querySelectorAll', start: ctx.subjectStart, end: close + 1 });
    } else if (name === 'forEach' && ctx.multi && code[j] === '(') {
      const { args } = argsOf(f, j);
      const cb = args[0] ? callbackOf(f, args[0]) : null;
      if (cb && cb.param0 && cb.span) merge(out, analyzeVariable(cb.f, cb.param0, cb.span, Object.assign({}, ctx, { multi: false, depth: ctx.depth + 1, assignAt: undefined, declAt: cb.span.start, via: 'callback ' + cb.param0 })));
    }
    return out;
  }

  // Uses of the element variable `name` inside `scope`: member registrations,
  // scoped selectors and calls that pass it to a helper. A use counts only when
  // the most recent assignment of `name` before it is the one that produced the
  // element (`assignAt`) and no nested function shadows the name.
  function analyzeVariable(f, name, scope, ctx) {
    const out = EMPTY();
    if (ctx.depth > 4) return out;
    const key = f.file + '#' + name + '@' + scope.start + ':' + (ctx.assignAt === undefined ? 'p' : ctx.assignAt);
    if (ctx.seen.has(key)) return out;
    ctx.seen.add(key);
    const code = f.code;
    const re = new RegExp('(?:^|[^\\w$.])(' + esc(name) + ')(?![\\w$])', 'g');
    const slice = code.slice(scope.start, scope.end + 1);
    const assigns = [], uses = [];
    const home = bindingOwner(f, name, ctx.declAt !== undefined ? ctx.declAt : scope.start);
    let m;
    while ((m = re.exec(slice))) {
      const u = scope.start + m.index + (m[0].length - m[1].length);
      if (f.kind[u] !== CODE || bindingOwner(f, name, u) !== home) continue;
      const after = skipWs(f, u + name.length);
      if (code[after] === '=' && code[after + 1] !== '=' && code[after + 1] !== '>') { assigns.push(u); continue; }
      uses.push({ u, after });
    }
    const own = ctx.assignAt === undefined ? undefined : assigns.filter((a) => a < ctx.assignAt).pop();
    for (const { u, after } of uses) {
      if (ctx.assignAt !== undefined && u < ctx.assignAt) continue;
      const last = assigns.filter((a) => a < u).pop();
      if (ctx.assignAt === undefined ? last !== undefined : last !== own) continue;
      if (code[after] === '.' || (code[after] === '?' && code[after + 1] === '.')) merge(out, analyzeMember(f, after, Object.assign({}, ctx, { subjectStart: u })));
      else merge(out, analyzeArgument(f, u, u + name.length, ctx));
    }
    return out;
  }

  // The subject spanning [start, end) is an argument of a call: follow it into
  // the callee's parameter (`bindSlider(el)`), through `Array.prototype.forEach.call(list, cb)`
  // or through an array conversion (`Array.from(list).forEach(...)`).
  function analyzeArgument(f, start, end, ctx) {
    const out = EMPTY();
    const code = f.code, kind = f.kind;
    let k = start - 1;
    while (k >= 0 && (isWs(code[k]) || kind[k] !== CODE)) k--;
    if (k < 0 || (code[k] !== '(' && code[k] !== ',')) return out;
    let depth = 0, open = -1;
    for (let i = k; i >= 0; i--) {
      if (kind[i] !== CODE) continue;
      const ch = code[i];
      if (ch === ')' || ch === ']' || ch === '}') depth++;
      else if (ch === '(') { if (depth === 0) { open = i; break; } depth--; }
      else if (ch === '[' || ch === '{') { if (depth === 0) return out; depth--; }
    }
    if (open < 0) return out;
    let e = open - 1;
    while (e >= 0 && isWs(code[e])) e--;
    let s = e;
    while (s >= 0 && /[\w$.]/.test(code[s])) s--;
    const callee = code.slice(s + 1, e + 1);
    if (!callee) return out;
    const { args, close } = argsOf(f, open);
    const argIndex = args.findIndex((a) => a.start <= start && start < a.end);
    if (argIndex < 0) return out;
    if (/\.forEach\.call$/.test(callee) && argIndex === 0) {
      const cb = args[1] ? callbackOf(f, args[1]) : null;
      if (cb && cb.param0 && cb.span) merge(out, analyzeVariable(cb.f, cb.param0, cb.span, Object.assign({}, ctx, { multi: false, depth: ctx.depth + 1, assignAt: undefined, declAt: cb.span.start, via: 'callback ' + cb.param0 })));
      return out;
    }
    if (/(?:^|\.)(?:slice\.call|Array\.from|from)$/.test(callee) && argIndex === 0) return merge(out, analyzeSubject(f, s + 1, close + 1, Object.assign({}, ctx, { multi: true, depth: ctx.depth + 1 })));
    const fn = resolveFn(f, callee.split('.').pop(), s + 1);
    if (fn && fn.params[argIndex] && ctx.depth < 3) merge(out, analyzeVariable(fn.f, fn.params[argIndex], { start: fn.start, end: fn.end }, Object.assign({}, ctx, { depth: ctx.depth + 1, assignAt: undefined, declAt: fn.start, via: 'helper ' + fn.name + '()' })));
    return out;
  }

  // What follows a subject expression spanning [start, end): a member chain, an
  // assignment to a variable, or a position in an argument list.
  function analyzeSubject(f, start, end, ctx) {
    const out = EMPTY();
    const i = skipWs(f, end);
    if (f.code[i] === '.' || (f.code[i] === '?' && f.code[i + 1] === '.')) return merge(out, analyzeMember(f, i, Object.assign({}, ctx, { subjectStart: start, via: ctx.via || 'chain' })));
    const prefix = f.code.slice(statementStart(f, start), start);
    const asg = /(?:^|[\s;,({[])(?:var|let|const)?\s*([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*=\s*(?:[^=;]*?(?:\|\||\?\?)\s*)?$/.exec(prefix);
    if (asg) return merge(out, analyzeVariable(f, asg[1], bindingOwner(f, asg[1], start), Object.assign({}, ctx, { assignAt: start, declAt: start, via: 'variable ' + asg[1] })));
    return merge(out, analyzeArgument(f, start, end, ctx));
  }

  // A lookup: bindings on the element itself plus, for each selector scoped to
  // it, the bindings on the elements that selector yields.
  function resolveLookup(f, start, end, ctx) {
    const r = analyzeSubject(f, start, end, ctx);
    const out = { bindings: r.bindings, scoped: [] };
    for (const sc of r.scoped) {
      const sub = resolveLookup(f, sc.start, sc.end, Object.assign({}, ctx, { multi: sc.multi, depth: ctx.depth + 1, assignAt: undefined, via: undefined }));
      if (sub.bindings.length) out.scoped.push({ selector: sc.selector, bindings: sub.bindings });
      for (const s2 of sub.scoped) out.scoped.push({ selector: sc.selector + ' ' + s2.selector, bindings: s2.bindings });
    }
    return out;
  }
  const newCtx = (multi, via) => ({ multi: !!multi, depth: 0, seen: new Set(), via });

  // One lookup site -> record. `sites` are the registration statements (file,
  // line, function) the row's handler column names; `ref` is the lookup itself,
  // kept for rows that are only referenced.
  function recFrom(f, at, res, via) {
    const fn = enclosingFunction(f, at);
    const where = (ff, off) => { const g = enclosingFunction(ff, off); return rel(ff.file) + ':' + lineOf(ff.code, off) + (g ? ' ' + g.name + '()' : ''); };
    const sites = uniq(res.bindings.map((b) => where(b.f, b.at)));
    return {
      file: res.bindings.length ? rel(res.bindings[0].f.file) : rel(f.file), line: lineOf(f.code, at), fn: fn ? fn.name : null,
      sites, ref: where(f, at), events: uniq(res.bindings.map((b) => b.event)), body: res.bindings.map((b) => b.body).join('\n'),
      binds: res.bindings.length > 0, via: res.bindings.length ? (via || res.bindings[0].via || 'chain') : null,
      fnBody: () => (fn ? f.code.slice(fn.start, fn.end + 1) : ''),
    };
  }

  const bindings = new Map();      // id -> [rec] (binding and reference records)
  const classBindings = new Map(); // selector -> [rec]
  const prefixBindings = [];       // {prefix, rec}
  const addRec = (map, key, rec) => { if (!map.has(key)) map.set(key, []); map.get(key).push(rec); };
  function record(f, at, start, end, target, multi, via) {
    const res = resolveLookup(f, start, end, newCtx(multi, via));
    const rec = recFrom(f, at, res, via);
    if (target.id) addRec(bindings, target.id, rec);
    else if (target.prefix) prefixBindings.push({ prefix: target.prefix, rec });
    else if (target.selector) addRec(classBindings, target.selector, rec);
    const scopePrefix = target.id ? '#' + target.id : target.selector || null;
    if (scopePrefix) for (const sc of res.scoped) addRec(classBindings, scopePrefix + ' ' + sc.selector, recFrom(f, at, { bindings: sc.bindings, scoped: [] }, via));
    return rec;
  }

  // A helper that looks its parameter up and binds it (`wireBoolBtn(id, op)`)
  // binds every id passed to it; a helper that merely returns the element
  // (`el(id)`) is transparent and the call site is analysed instead.
  // Bindings reached from the element that parameter `prm` names inside [span]:
  // its direct lookup (`getElementById(prm)`) and every id-taker it is passed to.
  function bindingsForParam(f, span, prm, via, selfName) {
    const res = EMPTY();
    for (const at of paramLookups(f, span, prm)) {
      if (!parameterIntact(f, span, prm, at)) continue;
      const open = f.code.indexOf('(', at);
      merge(res, resolveLookup(f, subjectStartAt(f, at), matchClose(f, open) + 1, Object.assign(newCtx(false, via), { depth: 1 })));
    }
    for (const call of takerCallsWithParam(f, span, prm, selfName)) {
      const tb = takerBinding(call.fn);
      if (tb && tb.bindings.length) merge(res, tb);
      if (tb && tb.transparent) merge(res, resolveLookup(f, call.at, call.close + 1, Object.assign(newCtx(false, via), { depth: 1 })));
    }
    return res;
  }
  const takerCache = new Map();
  function takerBinding(fn) {
    if (takerCache.has(fn)) return takerCache.get(fn);
    takerCache.set(fn, null);
    const prm = fn.params[idTakers.get(fn)];
    const res = bindingsForParam(fn.f, fn, prm, 'helper ' + fn.name + '()', fn);
    const returns = returnsOf(fn);
    res.transparent = returns.length > 0 && returns.every((r) => {
      if (!parameterIntact(fn.f, fn, prm, r.at)) return false;
      return new RegExp('^return\\s+(?:window\\.)?document\\.(?:' + paramLookupRe(prm).source + ')\\s*$').test(r.text);
    });
    takerCache.set(fn, res);
    return res;
  }

  // DOM lookups: getElementById / querySelector(All) with a literal argument,
  // and calls of id-taking helpers with a literal argument.
  const takerNames = uniq(Array.from(idTakers.keys(), (fn) => fn.name)).filter((n) => n.length > 1 || n === '$');
  const LOOKUP_RE = new RegExp("((?:window\\.)?document\\.)?\\b(getElementById|querySelectorAll|querySelector)\\s*\\(\\s*(['\"])([^'\"]+)\\3\\s*(\\)|\\+)" + (takerNames.length ? "|(?:^|[^\\w$.])(" + takerNames.map(esc).join('|') + ")\\s*\\(" : ''), 'g');
  for (const f of idx) {
    LOOKUP_RE.lastIndex = 0;
    let m;
    while ((m = LOOKUP_RE.exec(f.code))) {
      if (m[2]) {
        const nameAt = m.index + (m[1] ? m[1].length : 0);
        if (f.kind[nameAt] !== CODE) continue;
        if (!m[1] && f.code[nameAt - 1] === '.') continue; // `something.querySelector(…)`: scoped to an element, analysed where that element is bound
        const open = f.code.indexOf('(', nameAt);
        const start = m.index, end = matchClose(f, open) + 1;
        const multi = m[2] === 'querySelectorAll';
        if (m[2] === 'getElementById') { if (m[5] === '+') record(f, nameAt, start, end, { prefix: m[4] }, false); else record(f, nameAt, start, end, { id: m[4] }, false); }
        else if (m[5] === ')') { if (/^#[\w-]+$/.test(m[4])) record(f, nameAt, start, end, { id: m[4].slice(1) }, multi); else record(f, nameAt, start, end, { selector: m[4] }, multi); }
      } else if (m[6]) {
        const nameAt = m.index + m[0].indexOf(m[6]);
        if (f.kind[nameAt] !== CODE) continue;
        const open = m.index + m[0].length - 1;
        const { args, close } = argsOf(f, open);
        const fn = resolveFn(f, m[6], nameAt);
        if (!idTakers.has(fn)) continue;
        const a = args[idTakers.get(fn)];
        if (!a) continue;
        const lit = stringLiteral(a.text);
        const pre = lit === null ? /^(['"])([^'"]+)\1\s*\+/.exec(a.text) : null;
        if (lit === null && !pre) continue;
        const via = 'helper ' + m[6] + '()';
        const tb = takerBinding(fn);
        if (tb && tb.bindings.length) { const rec = recFrom(f, nameAt, tb, via); if (lit !== null) addRec(bindings, lit, rec); else prefixBindings.push({ prefix: pre[2], rec }); }
        else if (tb && tb.transparent) {
          if (lit !== null) record(f, nameAt, nameAt, close + 1, { id: lit }, false, via);
          else record(f, nameAt, nameAt, close + 1, { prefix: pre[2] }, false, via);
        } else {
          const rec = recFrom(f, nameAt, EMPTY(), via);
          rec.limit = 'helper parameter has no proved registration or unchanged returned lookup (reassignment, shadowing or unsupported return flow)';
          if (lit !== null) addRec(bindings, lit, rec);
          else prefixBindings.push({ prefix: pre[2], rec });
        }
      }
    }
  }
  if (returners.size) {
    const RETURNER_CALL_RE = new RegExp('(?:^|[^\\w$.])(' + uniq(Array.from(returners.keys(), (fn) => fn.name)).map(esc).join('|') + ')\\s*\\(\\s*\\)', 'g');
    for (const f of idx) {
      RETURNER_CALL_RE.lastIndex = 0;
      let m;
      while ((m = RETURNER_CALL_RE.exec(f.code))) {
        const nameAt = m.index + m[0].indexOf(m[1]);
        if (f.kind[nameAt] !== CODE || /function\s*$/.test(f.code.slice(Math.max(0, nameAt - 12), nameAt))) continue; // the definition itself
        const fn = resolveFn(f, m[1], nameAt);
        if (returners.has(fn)) record(f, nameAt, nameAt, m.index + m[0].length, { id: returners.get(fn) }, false, 'helper ' + m[1] + '()');
      }
    }
  }
  recordExportedRecordArrayBindings({
    idx, CODE, esc, argsOf, stringLiteral, bindingOwner, resolveFn, callbackOf, parameterIntact,
    subjectStartAt, matchClose, EMPTY, merge, resolveLookup, newCtx,
    addRec, bindings, recFrom,
  });
  // Lookups that go through a table of id literals — an object or an array,
  // flat or of objects — either directly (`getElementById(IDS[mode])`,
  // `getElementById(IDS.unite)`) or through the parameter of a callback that
  // iterates it (`tabs.forEach(function (t) { getElementById(t.btn)… })`), and
  // through an array of id literals (`['a', 'b'].forEach(id => getElementById(id)…)`).
  // Every id literal of the table is bound at the lookup site.
  const domIds = new Set(domRows.map((r) => r.id).filter(Boolean));
  const tableRoots = new Map(); // file -> [{name, ids, start, end}]: identifier that denotes the table (or one of its entries) inside [start, end]
  const addRoot = (f, root) => { if (!tableRoots.has(f.file)) tableRoots.set(f.file, []); tableRoots.get(f.file).push(root); };
  const TABLE_RE = /(?:^|[^\w$.])(?:var|let|const)\s+([A-Za-z_$][\w$]*)\s*=\s*([[{])/g;
  for (const f of idx) {
    TABLE_RE.lastIndex = 0;
    let m;
    while ((m = TABLE_RE.exec(f.code))) {
      const open = m.index + m[0].length - 1;
      if (f.kind[open] !== CODE) continue;
      const close = matchClose(f, open);
      if (close - open > 20000) continue;
      const lit = f.code.slice(open, close + 1);
      const ids = uniq((lit.match(/'([\w-]+)'|"([\w-]+)"/g) || []).map((s) => s.slice(1, -1)).filter((s) => domIds.has(s)));
      if (!ids.length) continue;
      const name = m[1];
      if (f.scopes.some((s) => s.function && s.params.includes(name) && s.start <= m.index && m.index <= s.end && bindingOwner(f, name, m.index) !== s)) continue;
      addRoot(f, { name, ids, start: 0, end: f.code.length, owner: bindingOwner(f, name, m.index) });
      const iter = new RegExp('(?:^|[^\\w$.])' + esc(name) + '\\s*\\.\\s*(?:forEach|map|some|every|filter)\\s*\\(', 'g');
      let c;
      while ((c = iter.exec(f.code))) {
        const cb = callbackOf(f, argsOf(f, c.index + c[0].length - 1).args[0] || { text: '' });
        if (cb.param0 && cb.span && cb.f === f) addRoot(f, { name: cb.param0, ids, start: cb.span.start, end: cb.span.end, owner: bindingOwner(f, cb.param0, cb.span.start) });
      }
      const forOf = new RegExp('for\\s*\\(\\s*(?:var|let|const)\\s+([A-Za-z_$][\\w$]*)\\s+of\\s+' + esc(name) + '\\s*\\)', 'g');
      while ((c = forOf.exec(f.code))) { const span = functionBodyAt(f, c.index + c[0].length); if (span) addRoot(f, { name: c[1], ids, start: span.start, end: span.end, owner: bindingOwner(f, c[1], span.start) }); }
    }
  }
  const TABLE_LOOKUP_RE = /getElementById\(\s*([A-Za-z_$][\w$]*)\s*[[.]/g;
  const ARRAY_FOREACH_RE = /\[((?:\s*'[\w-]+'\s*,?)+)\]\s*\.forEach\(\s*(?:function\s*\(\s*([\w$]+)\s*\)|\(?\s*([\w$]+)\s*\)?\s*=>)\s*\{/g;
  const recordAt = (f, at) => { const open = f.code.indexOf('(', at); return recFrom(f, at, resolveLookup(f, subjectStartAt(f, at), matchClose(f, open) + 1, newCtx(false))); };
  for (const f of idx) {
    let m;
    TABLE_LOOKUP_RE.lastIndex = 0;
    while ((m = TABLE_LOOKUP_RE.exec(f.code))) {
      if (f.kind[m.index] !== CODE) continue;
      const roots = (tableRoots.get(f.file) || []).filter((r) => r.name === m[1] && r.owner === bindingOwner(f, m[1], m.index) && r.start <= m.index && m.index <= r.end);
      if (!roots.length) continue;
      const rec = recordAt(f, m.index);
      for (const id of uniq(roots.flatMap((r) => r.ids))) addRec(bindings, id, rec);
    }
    ARRAY_FOREACH_RE.lastIndex = 0;
    while ((m = ARRAY_FOREACH_RE.exec(f.code))) {
      if (f.kind[m.index] !== CODE) continue;
      const prm = m[2] || m[3];
      const span = functionBodyAt(f, m.index + m[0].length - 1);
      if (!span) continue;
      const ids = (m[1].match(/'([\w-]+)'/g) || []).map((s) => s.slice(1, -1)).filter((s) => domIds.has(s));
      if (!ids.length) continue;
      const rec = recFrom(f, span.start, bindingsForParam(f, span, prm, 'callback ' + prm, null), 'callback ' + prm);
      for (const id of ids) addRec(bindings, id, rec);
    }
  }

  return { fnIndex, bindings, classBindings, prefixBindings };
}

module.exports = { createBindings };
