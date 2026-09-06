'use strict';
// Event bindings reached through window-exported helpers that consume literal records.

function recordExportedRecordArrayBindings(context) {
  const {
    idx, CODE, esc, argsOf, stringLiteral, bindingOwner, resolveFn, callbackOf, parameterIntact,
    subjectStartAt, matchClose, EMPTY, merge, resolveLookup, newCtx,
    addRec, bindings, recFrom,
  } = context;

  // A window-exported helper can receive an array of literal records. Follow
  // only an exact `window.Object.member(...)` call whose member is proven by
  // the matching object-literal export, then use the helper's own lookup of
  // `record.property`. This keeps a same-named helper or a record-shaped
  // string from creating a binding.
  const rootedWindow = (f, at) => bindingOwner(f, 'window', at) === f.rootScope;
  function callbackParameterIntact(f, span, name, at) {
    if (!parameterIntact(f, span, name, at)) return false;
    // The lightweight scope index deliberately favours legacy syntax. Reject a
    // nested redeclaration even when that index cannot distinguish its block.
    const redeclared = new RegExp('\\b(?:var|let|const|function|class)\\s+' + esc(name) + '(?![\\w$])', 'g');
    let match;
    while ((match = redeclared.exec(f.code.slice(span.start, at)))) {
      const declaration = span.start + match.index + match[0].lastIndexOf(name);
      if (f.kind[declaration] === CODE) return false;
    }
    return true;
  }
  function exportedMember(objectName, memberName) {
    const re = new RegExp('\\bwindow\\.' + esc(objectName) + '\\s*=\\s*\\{', 'g');
    let found = null;
    for (const exportFile of idx) {
      re.lastIndex = 0;
      let m;
      while ((m = re.exec(exportFile.code))) {
        const open = exportFile.code.indexOf('{', m.index);
        if (!rootedWindow(exportFile, m.index) || exportFile.kind[m.index] !== CODE || open < 0 || exportFile.kind[open] !== CODE) continue;
        for (const prop of argsOf(exportFile, open).args) {
          const value = new RegExp('^' + esc(memberName) + '\\s*:\\s*([A-Za-z_$][\\w$]*)\\s*$').exec(prop.text);
          if (!value) continue;
          if (found) return null;
          const fn = resolveFn(exportFile, value[1], prop.start);
          if (fn) found = fn;
        }
      }
    }
    if (!found) return null;
    const reassigned = new RegExp('\\bwindow\\.' + esc(objectName) + '\\s*\\.\\s*' + esc(memberName) + '\\s*=(?!=)', 'g');
    for (const file of idx) {
      reassigned.lastIndex = 0;
      let match;
      while ((match = reassigned.exec(file.code))) {
        if (rootedWindow(file, match.index) && file.kind[match.index] === CODE) return null;
      }
    }
    return found;
  }

  function literalRecordProperty(f, objectOpen, property) {
    for (const prop of argsOf(f, objectOpen).args) {
      const value = new RegExp('^' + esc(property) + '\\s*:\\s*(.*)$').exec(prop.text);
      if (value) return stringLiteral(value[1]);
    }
    return null;
  }

  function recordArrayBindings(fn, property) {
    const out = EMPTY();
    const list = fn.params[0];
    if (!list) return out;
    const re = new RegExp('(?:^|[^\\w$.])' + esc(list) + '\\s*\\.\\s*forEach\\s*\\(', 'g');
    let m;
    while ((m = re.exec(fn.f.code.slice(fn.start, fn.end + 1)))) {
      const callAt = fn.start + m.index;
      if (fn.f.kind[callAt] !== CODE || !parameterIntact(fn.f, fn, list, callAt)) continue;
      const open = fn.f.code.indexOf('(', callAt);
      const cb = callbackOf(fn.f, argsOf(fn.f, open).args[0] || { text: '' });
      if (!cb || !cb.param0 || !cb.span) continue;
      const lookup = new RegExp('(?:window\\.)?document\\.getElementById\\(\\s*' + esc(cb.param0) + '\\s*\\.\\s*' + esc(property) + '\\s*\\)', 'g');
      let hit;
      while ((hit = lookup.exec(fn.f.code.slice(cb.span.start, cb.span.end + 1)))) {
        const at = cb.span.start + hit.index;
        if (fn.f.kind[at] !== CODE || !callbackParameterIntact(fn.f, cb.span, cb.param0, at)) continue;
        const openAt = fn.f.code.indexOf('(', at);
        merge(out, resolveLookup(fn.f, subjectStartAt(fn.f, at), matchClose(fn.f, openAt) + 1, newCtx(false, 'exported record helper')));
      }
    }
    return out;
  }

  const EXPORTED_RECORD_CALL_RE = /window\.([A-Za-z_$][\w$]*)\.([A-Za-z_$][\w$]*)\s*\(/g;
  for (const f of idx) {
    EXPORTED_RECORD_CALL_RE.lastIndex = 0;
    let m;
    while ((m = EXPORTED_RECORD_CALL_RE.exec(f.code))) {
      if (!rootedWindow(f, m.index) || f.kind[m.index] !== CODE) continue;
      const fn = exportedMember(m[1], m[2]);
      if (!fn) continue;
      const open = f.code.indexOf('(', m.index);
      const first = argsOf(f, open).args[0];
      if (!first || f.code[first.start] !== '[' || f.kind[first.start] !== CODE) continue;
      const arrayClose = matchClose(f, first.start);
      if (arrayClose + 1 !== first.end) continue;
      const res = recordArrayBindings(fn, 'wrap');
      if (!res.bindings.length) continue;
      for (const record of argsOf(f, first.start).args) {
        if (f.code[record.start] !== '{' || f.kind[record.start] !== CODE || matchClose(f, record.start) + 1 !== record.end) continue;
        const id = literalRecordProperty(f, record.start, 'wrap');
        if (id !== null) addRec(bindings, id, recFrom(f, m.index, res, 'exported ' + m[1] + '.' + m[2] + '()'));
      }
    }
  }
}

module.exports = { recordExportedRecordArrayBindings };
