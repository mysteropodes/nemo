'use strict';
// P29 — native project-write adapter.
//
// Two halves, the same shape as adapter-export-svg-frame.test.cjs. The pure
// half drives the adapter through injected fs failures and pins the write
// sequence, including the orderings inside the fallback. The characterization
// half runs the REAL production `writeProjectTo` sliced out of project.js
// (project.js cannot be loaded whole headlessly: it reaches for Tauri, the DOM
// and a dozen module-local closures), with the real adapter loaded next to it,
// and compares its fs call log against an INDEPENDENT oracle — the pre-P29
// inline block, reproduced here verbatim from project.js at c0ece41 — so a
// drift in either the adapter or the production binding shows up as a
// different sequence of filesystem calls.
//
// What is deliberately NOT asserted: that the save is crash-safe. The catch
// branch's direct write can tear. That is pre-existing behaviour preserved by
// this extraction, and the tests below pin it as-is so a future change to it
// has to be deliberate.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const adapterPath = path.join(root, 'src/js/adapters/project-native-save.js');
const adapter = require(adapterPath);
const adapterSource = fs.readFileSync(adapterPath, 'utf8');
const projectSource = fs.readFileSync(path.join(root, 'src/js/project.js'), 'utf8');

// A recording filesystem. `fail` maps an operation name to the call indices
// that should reject, so a test can fail the rename but not the writes.
function fakeFs(fail) {
  const log = [];
  const counts = {};
  const shouldFail = (op) => {
    counts[op] = (counts[op] || 0) + 1;
    const spec = (fail || {})[op];
    if (spec === true) return true;
    if (Array.isArray(spec)) return spec.includes(counts[op]);
    return false;
  };
  return {
    log,
    ports: {
      async writeTextFile(p, text) {
        log.push(['writeTextFile', p, text]);
        if (shouldFail('writeTextFile')) throw new Error('write refused: ' + p);
      },
      async rename(from, to) {
        log.push(['rename', from, to]);
        if (shouldFail('rename')) throw new Error('rename refused');
      },
      async remove(p) {
        log.push(['remove', p]);
        if (shouldFail('remove')) throw new Error('remove refused');
      },
    },
  };
}

// The exact pre-P29 inline block (project.js writeProjectTo at c0ece41), kept
// as the independent oracle for the moved code. Ports stand in for the
// window.__TAURI__.fs calls the original made directly.
async function inlineOracle(p, json, ports) {
  var tmp = p + '.saving';
  try {
    await ports.writeTextFile(tmp, json);
    await ports.rename(tmp, p);
  } catch (e) {
    try { await ports.remove(tmp); } catch (_e) {}
    await ports.writeTextFile(p, json);
  }
}

const TARGET = '/projects/film.nemo';
const TMP = '/projects/film.nemo.saving';
const JSON_BYTES = '{"layers":[],"fps":24}';

// Every failure shape worth distinguishing, each run through BOTH the adapter
// and the inline oracle so the comparison is behavioural, not textual.
const SCENARIOS = [
  { name: 'success: temp write then rename, nothing else', fail: null, rejects: false },
  { name: 'rename refused: temp removed, then direct write', fail: { rename: true }, rejects: false },
  { name: 'rename refused and remove also fails: remove error swallowed', fail: { rename: true, remove: true }, rejects: false },
  { name: 'temp write refused: rename never attempted', fail: { writeTextFile: [1] }, rejects: false },
  { name: 'rename refused and the fallback write also fails: propagates', fail: { rename: true, writeTextFile: [2] }, rejects: true },
  { name: 'both writes refused: propagates', fail: { writeTextFile: true }, rejects: true },
];

// ---- pure adapter semantics ------------------------------------------------

test('the temp sibling is the target path plus .saving, in the same directory', () => {
  assert.equal(adapter.TEMP_SUFFIX, '.saving');
  assert.equal(adapter.tempPathFor(TARGET), TMP);
  assert.equal(path.dirname(adapter.tempPathFor(TARGET)), path.dirname(TARGET),
    'rename is only atomic within one filesystem; the temp must be a sibling');
});

test('success writes the temp file, renames it over the target, and touches nothing else', async () => {
  const f = fakeFs(null);
  await adapter.writeProjectFile(TARGET, JSON_BYTES, f.ports);
  assert.deepEqual(f.log, [
    ['writeTextFile', TMP, JSON_BYTES],
    ['rename', TMP, TARGET],
  ]);
});

test('the bytes written are the caller\'s string, passed through unchanged', async () => {
  const f = fakeFs(null);
  // Multi-byte UTF-8 and an embedded NUL, because "passed through unchanged"
  // has to mean bytes, not "looks the same". The NUL is written as an escape
  // and must stay one: a literal 0x00 makes git classify this file as binary,
  // which drops its diff out of the pull request entirely (patch: ABSENT).
  const exact = '{"a":1}\né\u0000tail';
  await adapter.writeProjectFile(TARGET, exact, f.ports);
  assert.equal(f.log[0][2], exact, 'no re-serialization, no trimming, no encoding step');
});

test('a failed rename removes the temp BEFORE the direct write, and that order is load-bearing', async () => {
  const f = fakeFs({ rename: true });
  await adapter.writeProjectFile(TARGET, JSON_BYTES, f.ports);
  assert.deepEqual(f.log, [
    ['writeTextFile', TMP, JSON_BYTES],
    ['rename', TMP, TARGET],
    ['remove', TMP],
    ['writeTextFile', TARGET, JSON_BYTES],
  ]);
  assert.ok(f.log.findIndex((c) => c[0] === 'remove') < f.log.findIndex((c, i) => c[0] === 'writeTextFile' && i > 0),
    'the stale temp must be gone before the target is rewritten');
});

test('a failing remove inside the fallback is swallowed and the direct write still happens', async () => {
  const f = fakeFs({ rename: true, remove: true });
  await adapter.writeProjectFile(TARGET, JSON_BYTES, f.ports);
  assert.deepEqual(f.log.at(-1), ['writeTextFile', TARGET, JSON_BYTES],
    'the temp may never have been created; its removal failing must not abort the save');
});

test('a failing temp write skips the rename entirely and falls back', async () => {
  const f = fakeFs({ writeTextFile: [1] });
  await adapter.writeProjectFile(TARGET, JSON_BYTES, f.ports);
  assert.deepEqual(f.log.map((c) => c[0]), ['writeTextFile', 'remove', 'writeTextFile']);
  assert.equal(f.log.some((c) => c[0] === 'rename'), false);
});

test('a failing fallback write propagates, so a save is never silently reported as done', async () => {
  const f = fakeFs({ rename: true, writeTextFile: [2] });
  await assert.rejects(() => adapter.writeProjectFile(TARGET, JSON_BYTES, f.ports), /write refused/);
});

test('the promise resolves only after both filesystem calls have completed', async () => {
  let releaseWrite, releaseRename;
  const settled = [];
  const ports = {
    writeTextFile: () => new Promise((r) => { releaseWrite = r; }),
    rename: () => new Promise((r) => { releaseRename = r; }),
    remove: async () => {},
  };
  const done = adapter.writeProjectFile(TARGET, JSON_BYTES, ports).then(() => settled.push('done'));
  await new Promise((r) => setImmediate(r));
  assert.deepEqual(settled, [], 'still pending while the temp write is in flight');
  releaseWrite();
  await new Promise((r) => setImmediate(r));
  assert.deepEqual(settled, [], 'still pending while the rename is in flight');
  releaseRename();
  await done;
  assert.deepEqual(settled, ['done']);
});

test('the adapter reads no globals: its only entry points are the ports', () => {
  const code = adapterSource.split('\n').filter((line) => !/^\s*\/\//.test(line)).join('\n');
  assert.doesNotMatch(code, /window\.|__TAURI__|localStorage|document\./);
});

// ---- equivalence with the pre-P29 inline block -----------------------------

test('adapter and inline oracle produce identical filesystem call sequences', async () => {
  for (const scenario of SCENARIOS) {
    const viaAdapter = fakeFs(scenario.fail);
    const viaInline = fakeFs(scenario.fail);
    const run = (fn, f) => fn(TARGET, JSON_BYTES, f.ports);
    if (scenario.rejects) {
      await assert.rejects(() => run(adapter.writeProjectFile, viaAdapter), undefined, scenario.name);
      await assert.rejects(() => run(inlineOracle, viaInline), undefined, scenario.name);
    } else {
      await run(adapter.writeProjectFile, viaAdapter);
      await run(inlineOracle, viaInline);
    }
    assert.deepEqual(viaAdapter.log, viaInline.log, scenario.name);
  }
});

// ---- characterization of the real production binding -----------------------

function slice(source, startMarker, endMarker, label) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `${label}: start marker not found — the source moved`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `${label}: end marker not found — the source moved`);
  return source.slice(start, end);
}

const WRITE_PROJECT_TO = ['async function writeProjectTo(path){', '\n  // ---- Browser-mode save'];

// Evaluate the real adapter and the real production function together in one
// context whose only legacy globals are the ones the binding reaches for.
// Anything else would throw, which is the point: it pins what the caller still
// owns after the extraction.
function productionContext(f) {
  const calls = { touchRecent: [], renderRecents: 0, markSaved: [], updateCurrentLabel: 0, autosave: [] };
  const context = {
    console, JSON, Object, Array, String, Number, Error, Promise, setImmediate,
    state: { canvasW: 1920, canvasH: 1080, fps: 24 },
    currentPath: null,
    currentName: null,
    updateCurrentLabel() { calls.updateCurrentLabel++; },
    touchRecent(p, name, meta) { calls.touchRecent.push([p, name, meta]); },
    renderRecents() { calls.renderRecents++; },
    markSaved(json) { calls.markSaved.push(json); },
    localStorage: { setItem(key, value) { calls.autosave.push([key, value]); } },
  };
  context.window = context;
  context.SM = { exportJSON: () => JSON_BYTES };
  context.SMProjectDocument = { baseName: (p) => p.split('/').pop() };
  context.__TAURI__ = { fs: f.ports };
  vm.createContext(context);
  vm.runInContext(adapterSource, context, { filename: 'project-native-save.js' });
  const fn = slice(projectSource, WRITE_PROJECT_TO[0], WRITE_PROJECT_TO[1], 'writeProjectTo');
  const writeProjectTo = vm.runInContext(`(${fn})`, context, { filename: 'project-writeProjectTo.js' });
  return { writeProjectTo, calls, context };
}

test('production writeProjectTo drives the same filesystem sequence as the inline oracle', async () => {
  for (const scenario of SCENARIOS) {
    const viaProduction = fakeFs(scenario.fail);
    const viaInline = fakeFs(scenario.fail);
    const production = productionContext(viaProduction);
    if (scenario.rejects) {
      await assert.rejects(() => production.writeProjectTo(TARGET), undefined, scenario.name);
      await assert.rejects(() => inlineOracle(TARGET, JSON_BYTES, viaInline.ports), undefined, scenario.name);
    } else {
      await production.writeProjectTo(TARGET);
      await inlineOracle(TARGET, JSON_BYTES, viaInline.ports);
    }
    assert.deepEqual(viaProduction.log, viaInline.log, scenario.name);
  }
});

test('on success the caller still owns path, recents, dirty tracking and autosave', async () => {
  const f = fakeFs(null);
  const production = productionContext(f);
  await production.writeProjectTo(TARGET);
  assert.equal(production.context.currentPath, TARGET);
  assert.equal(production.context.currentName, 'film.nemo');
  assert.equal(production.calls.updateCurrentLabel, 1);
  // JSON-compared, not deepEqual: the meta object is constructed inside the vm
  // realm, so it is structurally equal but not reference-equal to a host object.
  assert.equal(JSON.stringify(production.calls.touchRecent),
    JSON.stringify([[TARGET, 'film.nemo', { canvasW: 1920, canvasH: 1080, fps: 24 }]]));
  assert.equal(production.calls.renderRecents, 1);
  assert.deepEqual(production.calls.autosave, [['nemo-auto', JSON_BYTES]]);
});

test('the bytes marked saved are byte-identical to the bytes handed to the adapter', async () => {
  const f = fakeFs(null);
  const production = productionContext(f);
  await production.writeProjectTo(TARGET);
  assert.deepEqual(production.calls.markSaved, [JSON_BYTES]);
  assert.equal(f.log[0][2], production.calls.markSaved[0],
    'marking clean against different bytes than were written would hide a dirty document');
});

test('a failed save marks nothing: no recents entry, no clean flag, no autosave', async () => {
  const f = fakeFs({ writeTextFile: true });
  const production = productionContext(f);
  await assert.rejects(() => production.writeProjectTo(TARGET));
  assert.deepEqual(production.calls.markSaved, [], 'a document that failed to save must stay dirty');
  assert.deepEqual(production.calls.touchRecent, []);
  assert.deepEqual(production.calls.autosave, []);
  assert.equal(production.context.currentPath, null);
});

// ---- routing guard ---------------------------------------------------------

test('the inline filesystem block did not come back into project.js', () => {
  const fn = slice(projectSource, WRITE_PROJECT_TO[0], WRITE_PROJECT_TO[1], 'writeProjectTo');
  // window-qualified on purpose: project.js reaches every other cross-module
  // dependency that way (76 window.* references, the closest precedent being
  // window.NemoOpacityApplication in this same file), so an unqualified global
  // here would be the odd one out. Pinned so it does not drift back.
  assert.match(fn, /window\.NemoProjectNativeSave\.writeProjectFile\(path,json,\{/);
  assert.doesNotMatch(fn, /\.saving/, 'the temp suffix is owned by the adapter now');
  // The fs names still appear — as the three one-line port delegations, which
  // is the binding and is meant to be here. What must NOT come back is the
  // SEQUENCING: the inline block called writeTextFile twice (temp, then the
  // fallback direct write). Exactly one occurrence means the fallback is gone.
  const occurrences = (re) => (fn.match(re) || []).length;
  assert.equal(occurrences(/fs\.writeTextFile\(/g), 1,
    'two writeTextFile calls here would mean the fallback was re-inlined');
  assert.equal(occurrences(/fs\.rename\(/g), 1);
  assert.equal(occurrences(/fs\.remove\(/g), 1);
  assert.match(fn, /writeTextFile:function\(p,text\)\{return window\.__TAURI__\.fs\.writeTextFile\(p,text\);\}/,
    'the fs references that remain are port delegations, nothing more');
  // One '.saving' anywhere in project.js would mean a second inline copy of
  // the temp-file convention; there must be none.
  assert.equal(projectSource.split("'.saving'").length, 1);
});

test('the browser save path is untouched by this extraction', () => {
  const browser = slice(projectSource, 'function downloadJson(filename,json){', '\n  function ', 'downloadJson');
  assert.doesNotMatch(browser, /NemoProjectNativeSave|__TAURI__/,
    'browser-mode save never went through the native write and still does not');
  assert.match(browser, /new Blob\(\[json\]/);
});

test('the adapter is loaded before project.js in index.html', () => {
  const html = fs.readFileSync(path.join(root, 'src/index.html'), 'utf8');
  const adapterTag = html.indexOf('js/adapters/project-native-save.js');
  const projectTag = html.indexOf('js/project.js');
  assert.notEqual(adapterTag, -1, 'the adapter must be served to the app, not only to tests');
  assert.ok(adapterTag < projectTag, 'project.js references NemoProjectNativeSave at call time, but load order should still be explicit');
});
