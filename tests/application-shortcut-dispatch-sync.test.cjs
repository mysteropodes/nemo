'use strict';
// Follow-up from the P30/#1114 review (2026-09-15): COMMAND_SHORTCUTS
// (src/js/application/shortcut-registry.js) and its dispatch switch
// (runCommandShortcutAction in src/js/timeline.js) are two files with
// nothing keeping them in sync. Before the extraction, a `run` closure
// missing from a table entry threw a TypeError -- loud. After, the switch
// has no `default:` and runCommandShortcut returns true as soon as a key
// matches, so an action declared without its case now swallows its key
// silently. This test pins the two sides together at the source-text
// level so that gap fails a test, not a bug report.
//
// Does not retest dispatch behavior itself (application-shortcut-registry
// .test.cjs covers the data/persistence layer; the P30 PR review verified
// each case body verbatim against its former closure) -- only that the
// two declared sets of action names are exactly equal.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const registry = require('../src/js/application/shortcut-registry.js');

// Bracket-balanced extraction, not a fixed-line assumption: robust to the
// function being reformatted across multiple lines later.
function extractFunctionSource(source, functionName) {
  const marker = 'function ' + functionName;
  const start = source.indexOf(marker);
  if (start === -1) throw new Error(`${functionName} not found in source`);
  const braceStart = source.indexOf('{', start);
  if (braceStart === -1) throw new Error(`${functionName}: no opening brace found`);
  let depth = 0;
  for (let i = braceStart; i < source.length; i++) {
    if (source[i] === '{') depth++;
    else if (source[i] === '}') {
      depth--;
      if (depth === 0) return source.slice(start, i + 1);
    }
  }
  throw new Error(`${functionName}: unbalanced braces`);
}

function dispatchedActions() {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/timeline.js'), 'utf8');
  const fnSource = extractFunctionSource(source, 'runCommandShortcutAction');
  const actions = [];
  const caseRe = /case'([^']+)':/g;
  let match;
  while ((match = caseRe.exec(fnSource))) actions.push(match[1]);
  return actions;
}

test('every COMMAND_SHORTCUTS action has exactly one dispatch case, and every case is a declared action', () => {
  const declared = registry.COMMAND_SHORTCUTS.map((s) => s.action).sort();
  const dispatched = dispatchedActions().sort();
  assert.deepEqual(dispatched, declared,
    'runCommandShortcutAction (timeline.js) and COMMAND_SHORTCUTS (shortcut-registry.js) drifted -- ' +
    'an action declared without a case silently swallows its key at runtime instead of failing here.');
});

test('no action name appears as a dispatch case more than once', () => {
  const dispatched = dispatchedActions();
  const seen = new Set();
  const duplicates = dispatched.filter((a) => (seen.has(a) ? true : (seen.add(a), false)));
  assert.deepEqual(duplicates, [], 'a duplicate case is dead code: the first match always wins in a switch');
});

test('mutation guard: a case removed from the switch is caught, proving this test can fail', () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/timeline.js'), 'utf8');
  const fnSource = extractFunctionSource(source, 'runCommandShortcutAction');
  const mutated = fnSource.replace("case'cmdTween':window.SM.generateTweens();return;", '');
  const caseRe = /case'([^']+)':/g;
  const dispatched = [];
  let match;
  while ((match = caseRe.exec(mutated))) dispatched.push(match[1]);
  const declared = registry.COMMAND_SHORTCUTS.map((s) => s.action).sort();
  assert.notDeepEqual(dispatched.sort(), declared);
});
