'use strict';
// T10/#1399 -- the report link: the recorded pre-state that makes a trace
// window into a REPRODUCIBLE bundle.
//
// The property under test is not "a bundle is produced" but "the bundle says
// what it actually replays from". T07's buildBundle identifies a bundle by
// {id, hash of its STARTING state}; before T10 the trace kept only
// {request, revision, ok}, so no honest hash existed and the only way to
// produce one would have been to hash the current state and mislabel it.

const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fixture = require('./fixtures/lib/opacity-application.cjs');
const diagnostics = require('../src/js/domain/diagnostics/opacity-diagnostics.js');
const bundleCodec = require('../src/js/domain/diagnostics/opacity-reproduction-bundle.js');

const sha = (value) => crypto.createHash('sha256').update(JSON.stringify(value)).digest('hex');

function drive(f, id, value) {
  const identity = f.app.meta();
  return f.app.handle({
    apiVersion: 1, requestId: id, ...identity, expectedRevision: identity.revision,
    operation: 'property.set', payload: { layerId: f.state.layers[0].layerUid, property: 'opacity', value },
  });
}
function traceOf(f) {
  const identity = f.app.meta();
  return f.app.handle({ apiVersion: 1, requestId: 'read-trace', ...identity,
    expectedRevision: identity.revision, operation: 'diagnostics.trace', payload: {} }).result.entries;
}

// ---- the fixture equivalence that makes a report verifiable --------------

test('the oldest retained pre-state hashes to exactly the fixture hash it started from', () => {
  const f = fixture.build('t10-equiv', 55);
  drive(f, 'c1', 11);
  drive(f, 'c2', 22);
  const entries = traceOf(f);
  assert.equal(entries.length, 2);
  assert.equal(sha(entries[0].stateBefore), f.hash,
    'a report built from this window must carry the same fixture hash a synthetic fixture would');
  assert.notEqual(sha(entries[1].stateBefore), f.hash, 'the second command starts from a different state');
});

// ---- the reason pre-state is per entry rather than per buffer ------------

test('after eviction the window reports the state its OLDEST SURVIVING command assumed', () => {
  const f = fixture.build('t10-evict', 7);
  const layerId = f.state.layers[0].layerUid;
  for (let n = 0; n < 40; n++) drive(f, `w${n}`, n % 101);
  const entries = traceOf(f);
  assert.equal(entries.length, diagnostics.LIMIT, 'the ring still bounds the trace');
  assert.equal(entries[0].request.requestId, 'w8', 'w0..w7 were evicted');
  assert.notEqual(sha(entries[0].stateBefore), f.hash,
    'a buffer-wide starting state would still claim the original fixture here -- that is the bug this design avoids');
  const opacityIn = (s) => s.layers.find((l) => l.layerUid === layerId).motionStatic.opacity[0];
  assert.equal(opacityIn(entries[0].stateBefore), 7,
    'the surviving window starts from what the evicted predecessor w7 left behind');
});

// ---- startingState() is the window's start, and never a guess ------------

test('startingState() tracks the surviving window and is undefined when nothing was recorded', () => {
  const recorded = diagnostics.create(['property.set']);
  assert.equal(recorded.startingState(), undefined, 'an empty trace has no starting state');

  recorded.remember({ requestId: 'a', operation: 'property.set', payload: {} }, { revision: 1, ok: true }, { at: 'before-a' });
  recorded.remember({ requestId: 'b', operation: 'property.set', payload: {} }, { revision: 2, ok: true }, { at: 'before-b' });
  assert.deepEqual(recorded.startingState(), { at: 'before-a' });

  const legacy = diagnostics.create(['property.set']);
  legacy.remember({ requestId: 'a', operation: 'property.set', payload: {} }, { revision: 1, ok: true });
  assert.equal(legacy.startingState(), undefined,
    'a caller that records no pre-state must get undefined, never a plausible-looking wrong answer');
  assert.equal(legacy.entries()[0].stateBefore, undefined, 'and the entry carries no invented field');
});

test('remember() copies the pre-state it is handed, so a live caller object cannot rewrite history', () => {
  // The application happens to pass a fresh clone today, which is why storing
  // the argument by reference still looks correct end-to-end. It is not: any
  // other caller handing over live state would see its recorded history
  // change underneath it. Guard the copy where the copy is made.
  const recorded = diagnostics.create(['property.set']);
  const live = { layers: [{ opacity: 1 }] };
  recorded.remember({ requestId: 'a', operation: 'property.set', payload: {} }, { revision: 1, ok: true }, live);
  live.layers[0].opacity = 999;
  assert.equal(recorded.startingState().layers[0].opacity, 1, 'the trace kept what was recorded, not a live alias');
  assert.equal(recorded.entries()[0].stateBefore.layers[0].opacity, 1);
});

test('the recorded pre-state is a copy: mutating the trace never reaches the document', () => {
  const f = fixture.build('t10-iso', 3);
  drive(f, 'c1', 11);
  const entries = traceOf(f);
  entries[0].stateBefore.layers[0].motionStatic.opacity[0] = 999;
  assert.notEqual(traceOf(f)[0].stateBefore.layers[0].motionStatic.opacity[0], 999,
    'entries() hands out clones; a caller cannot write back through a report');
});

// ---- the bundle the panel would produce ----------------------------------

test('a bundle built from the recorded window carries a fixture the codec accepts', () => {
  const f = fixture.build('t10-bundle', 9);
  drive(f, 'c1', 11);
  drive(f, 'c2', 22);
  const entries = traceOf(f);
  const bundle = bundleCodec.buildBundle({ id: f.app.meta().documentId, hash: sha(entries[0].stateBefore) }, entries, null);
  const parsed = bundleCodec.parseBundle(bundle);
  assert.equal(parsed.error, undefined, 'the codec must accept what the panel produces');
  assert.deepEqual(parsed.commands.map((c) => c.payload.value), [11, 22]);
  assert.equal(parsed.fixture.hash, f.hash, 'and the bundle names the state it truly replays from');
});

// ---- the panel path itself, driven through a real click -----------------
// The layers below are covered above, but the panel is what the leaf is named
// after and it had no behavioural coverage: hashing the LAST entry instead of
// the first, or dropping the "no starting state" guard, both survived the full
// suite. Those two mutations are exactly what these tests kill. The provenance
// pins DO fail when the panel source changes, but they are edit detectors --
// an added space fails them identically -- so they prove nothing about
// behaviour.

const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');

// The panel's click handler is not itself async: it kicks off buildReport()
// and returns, so awaiting the handler proves nothing -- the digest is still in
// flight. A fixed setTimeout(0) is NOT enough either: crypto.subtle.digest
// settles on its own schedule and a single macrotask hop loses the race most
// runs (measured: 5 of 6 failed). Wait for the observable signal instead of
// guessing a delay, so the test is deterministic rather than timing-dependent.
async function waitUntil(predicate, what) {
  for (let attempt = 0; attempt < 500; attempt++) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 1));
  }
  throw new Error(`timed out waiting for ${what}`);
}

function recordingElement(tag) {
  const listeners = {};
  return {
    tagName: tag, style: {}, children: [], _html: '', listeners,
    get innerHTML() { return this._html; },
    // Assigning innerHTML replaces the children in a real DOM, which is what
    // drops the previous render's listeners. Without this the stub caches the
    // same button forever, handlers pile up, and listeners.click[0] is a stale
    // closure over an older trace -- the harness would then test nothing.
    set innerHTML(v) { this._html = v; this._found = {}; },
    setAttribute() {}, getAttribute() { return null; },
    appendChild(child) { this.children.push(child); return child; },
    remove() {}, click() {},
    addEventListener(type, fn) { (listeners[type] = listeners[type] || []).push(fn); },
    // The panel looks up its buttons and its status line by data-attribute.
    querySelector(selector) {
      this._found = this._found || {};
      if (!this._found[selector]) this._found[selector] = recordingElement('stub');
      return this._found[selector];
    },
    querySelectorAll() { return []; },
  };
}

// Drives the real labs panel over the real application, and returns the
// panel's own Report click handler plus the elements it writes into.
function panelUnderTest() {
  const flags = { 'nemo-labs-diagnostics-panel': true };
  let sequence = 0;
  const f = fixture.build('t10-panel', 21);
  const created = [];
  const ctx = {
    console,
    document: {
      createElement(tag) { const el = recordingElement(tag); created.push(el); return el; },
      body: { appendChild: () => {} }, readyState: 'complete', addEventListener: () => {},
      querySelector: () => null, querySelectorAll: () => [], getElementById: () => null,
    },
    localStorage: { getItem: (k) => (flags[k] ? '1' : null), setItem: (k, v) => { flags[k] = v === '1'; } },
    crypto: globalThis.crypto,
    TextEncoder,
    Blob: function Blob(parts) { this.parts = parts; },
    URL: { createObjectURL: () => 'blob:stub', revokeObjectURL: () => {} },
    SM: { t: (k) => k, setActiveLayer: () => {} },
    state: f.state, userLayers: [], _layerSel: [],
    NemoApplication: { handle: (request) => f.app.handle(request) },
    NemoOpacityApplication: f.app,
    NemoOpacityReproductionBundle: bundleCodec,
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  for (const file of ['labs/labs-core.js', 'labs/diagnostics-panel.js']) {
    const filename = path.resolve(ROOT, 'src/js', file);
    vm.runInContext(fs.readFileSync(filename, 'utf8'), ctx, { filename });
  }
  return { ctx, f, panel: created[0] };
}

test('panel: clicking Report hashes the OLDEST retained entry, not the newest', async () => {
  const { ctx, f, panel } = panelUnderTest();
  const layerId = f.state.layers[0].layerUid;
  for (const [id, value] of [['p1', 41], ['p2', 52], ['p3', 63]]) {
    const identity = f.app.meta();
    f.app.handle({ apiVersion: 1, requestId: id, ...identity, expectedRevision: identity.revision,
      operation: 'property.set', payload: { layerId, property: 'opacity', value } });
  }
  // The panel is built at load, before these writes; use its own Refresh
  // button to re-read the trace rather than rebuilding (disable/enable would
  // create a NEW element and leave this handle stale).
  panel.querySelector('[data-diag-refresh]').listeners.click[0]();

  const live = panel.querySelector('[data-diag-report]');
  const status = panel.querySelector('[data-diag-report-status]');
  assert.ok(live.listeners.click && live.listeners.click.length, 'the Report button must be wired');

  const captured = [];
  ctx.NemoOpacityReproductionBundle = {
    buildBundle: (fixtureRef, entries, meta) => {
      captured.push({ fixtureRef, entries });
      return bundleCodec.buildBundle(fixtureRef, entries, meta);
    },
  };
  live.listeners.click[0]();
  await waitUntil(() => captured.length === 1, 'the report click to reach the codec');
  const entries = traceOf(f);
  assert.equal(captured[0].fixtureRef.hash, sha(entries[0].stateBefore),
    'the reported fixture must be the state the OLDEST retained command assumed');
  assert.notEqual(captured[0].fixtureRef.hash, sha(entries[entries.length - 1].stateBefore),
    'hashing the newest entry would name a state these commands never replay from');
  assert.equal(captured[0].fixtureRef.id, f.app.meta().documentId,
    'the bundle must name the document it came from, so a report can be traced back');
  assert.match(status.innerHTML + status.textContent, /./);
});

test('panel: refuses to report when the trace carries no starting state', async () => {
  const { ctx, f, panel } = panelUnderTest();
  const layerId = f.state.layers[0].layerUid;
  const identity = f.app.meta();
  f.app.handle({ apiVersion: 1, requestId: 'p1', ...identity, expectedRevision: identity.revision,
    operation: 'property.set', payload: { layerId, property: 'opacity', value: 41 } });

  // A trace recorded before T10 (or by any caller that records no pre-state).
  ctx.NemoApplication = { handle: (request) => {
    const response = f.app.handle(request);
    if (request.operation === 'diagnostics.trace' && response.ok) {
      response.result.entries.forEach((entry) => { delete entry.stateBefore; });
    }
    return response;
  } };
  panel.querySelector('[data-diag-refresh]').listeners.click[0]();

  let built = false;
  ctx.NemoOpacityReproductionBundle = { buildBundle: () => { built = true; return {}; } };
  const live = panel.querySelector('[data-diag-report]');
  const status = panel.querySelector('[data-diag-report-status]');
  live.listeners.click[0]();
  await waitUntil(() => String(status.textContent).length > 0, 'the panel to report its refusal');

  assert.equal(built, false, 'no bundle may be built without a starting state -- inventing a fixture is the failure this leaf exists to prevent');
  assert.match(String(status.textContent), /no starting state/i, 'and the panel must say why');
});

test('panel: the refusal is decided by the OLDEST entry, not by whichever one happens to carry state', async () => {
  // A window whose newest entry has a pre-state but whose oldest does not: the
  // bundle would replay from the oldest, so it is the oldest that decides. A
  // guard reading the last entry passes this window and emits a bundle whose
  // fixture describes nothing it replays from.
  const { ctx, f, panel } = panelUnderTest();
  const layerId = f.state.layers[0].layerUid;
  for (const [id, value] of [['m1', 41], ['m2', 52]]) {
    const identity = f.app.meta();
    f.app.handle({ apiVersion: 1, requestId: id, ...identity, expectedRevision: identity.revision,
      operation: 'property.set', payload: { layerId, property: 'opacity', value } });
  }
  ctx.NemoApplication = { handle: (request) => {
    const response = f.app.handle(request);
    if (request.operation === 'diagnostics.trace' && response.ok) delete response.result.entries[0].stateBefore;
    return response;
  } };
  panel.querySelector('[data-diag-refresh]').listeners.click[0]();

  let built = false;
  ctx.NemoOpacityReproductionBundle = { buildBundle: () => { built = true; return {}; } };
  const status = panel.querySelector('[data-diag-report-status]');
  panel.querySelector('[data-diag-report]').listeners.click[0]();
  await waitUntil(() => String(status.textContent).length > 0, 'the panel to decide');

  assert.equal(built, false, 'the oldest entry lacks a pre-state, so this window is not reportable');
  assert.match(String(status.textContent), /no starting state/i);
});
