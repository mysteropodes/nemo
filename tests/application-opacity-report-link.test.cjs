'use strict';
// T10 source-stage safety. Native opt-in reproduction is T10A/T08B; a legacy
// trace or user-state hash is not a recoverable synthetic fixture.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const fixture = require('./fixtures/lib/opacity-application.cjs');
const diagnostics = require('../src/js/domain/diagnostics/opacity-diagnostics.js');
const bundleCodec = require('../src/js/domain/diagnostics/opacity-reproduction-bundle.js');

function drive(f, id, value) {
  const identity = f.app.meta();
  return f.app.handle({ apiVersion: 1, requestId: id, ...identity, expectedRevision: identity.revision,
    operation: 'property.set', payload: { layerId: f.state.layers[0].layerUid, property: 'opacity', value } });
}
function traceOf(f) {
  return f.app.handle({ apiVersion: 1, requestId: 'read-trace', ...f.app.meta(),
    operation: 'diagnostics.trace', payload: {} }).result.entries;
}

test('ordinary writes never retain the 50 KB private document sentinel', () => {
  const f = fixture.build('t10-private', 55);
  const sentinel = '/private/artist/assets/DO-NOT-EXPORT-' + 'x'.repeat(50000);
  f.state.privateAsset = sentinel;
  assert.equal(drive(f, 'c1', 11).ok, true);
  const entries = traceOf(f);
  assert.equal(Object.hasOwn(entries[0], 'stateBefore'), false);
  assert.equal(JSON.stringify(entries).includes('DO-NOT-EXPORT'), false);
  assert.ok(JSON.stringify(entries).length < 2048, 'trace size must not scale with document data');
  assert.equal(f.state.privateAsset, sentinel);
});

test('cyclic document state does not wedge the edit guard or a following valid write', () => {
  const f = fixture.build('t10-cycle', 7);
  f.state.privateCycle = f.state;
  let first, thrown;
  try { first = drive(f, 'c1', 11); } catch (error) { thrown = error; }
  delete f.state.privateCycle;
  const next = drive(f, 'c2', 22);
  assert.equal(next.ok, true, 'a failed diagnostic serialization must never retain edit ownership');
  assert.equal(thrown, undefined, 'a write must not serialize arbitrary root state');
  assert.equal(first.ok, true);
  assert.equal(f.state.layers[0].motionStatic.opacity[0], 22);
});

test('diagnostics never traverses an unrelated document getter', () => {
  const f = fixture.build('t10-getter', 3);
  Object.defineProperty(f.state, 'privateAsset', { enumerable: true, get() { throw new Error('private getter read'); } });
  assert.equal(drive(f, 'c1', 11).ok, true);
  assert.equal(drive(f, 'c2', 22).ok, true);
});

test('the diagnostics boundary ignores arbitrary state arguments and preserves detached entries', () => {
  const recorded = diagnostics.create(['property.set']);
  const privateState = { privateAsset: '/private/sentinel' };
  privateState.self = privateState;
  const request = { requestId: 'a', operation: 'property.set', payload: { value: 25 } };
  assert.doesNotThrow(() => recorded.remember(request, { revision: 1, ok: true }, privateState));
  const entries = recorded.entries();
  assert.deepEqual(Object.keys(entries[0]).sort(), ['ok', 'request', 'revision']);
  entries[0].request.payload.value = 99;
  assert.equal(recorded.entries()[0].request.payload.value, 25);
});

const settle = () => new Promise((resolve) => setImmediate(resolve));
function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}
function element(tag) {
  const listeners = {};
  return {
    tagName: tag, style: {}, children: [], listeners, isConnected: true, textContent: '', _html: '',
    get innerHTML() { return this._html; },
    set innerHTML(value) {
      for (const child of Object.values(this._found || {})) child.isConnected = false;
      this._html = value; this._found = {};
    },
    appendChild(child) { this.children.push(child); child.isConnected = true; return child; },
    remove() { this.isConnected = false; },
    click() { for (const listener of listeners.click || []) listener(); },
    addEventListener(type, listener) { (listeners[type] ||= []).push(listener); },
    querySelector(selector) { return (this._found ||= {})[selector] ||= element('stub'); },
    querySelectorAll() { return []; },
  };
}

async function panelUnderTest() {
  const f = fixture.build('t10-panel', 21);
  drive(f, 'p1', 41);
  const flags = { 'nemo-labs-diagnostics-panel': true };
  const created = [], requests = [], bundles = [], downloads = [];
  const ctx = {
    console, TextEncoder, Blob: function Blob(parts) { this.parts = parts; },
    crypto: { subtle: { digest: async () => new Uint8Array(32).buffer } },
    URL: { createObjectURL(blob) { downloads.push(blob); return 'blob:test'; }, revokeObjectURL() {} },
    document: { createElement(tag) { const node = element(tag); created.push(node); return node; }, body: element('body') },
    localStorage: { getItem: (key) => flags[key] ? '1' : null, setItem: (key, value) => { flags[key] = value === '1'; } },
    SM: { t: (key) => key, setActiveLayer() {} }, state: f.state, userLayers: [], _layerSel: [],
    NemoApplication: { handle(request) { requests.push(request); return f.app.handle(request); } },
    NemoOpacityApplication: f.app,
    NemoOpacityReproductionBundle: { buildBundle(...args) { bundles.push(args); return bundleCodec.buildBundle(...args); } },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  for (const file of ['labs/labs-core.js', 'labs/diagnostics-panel.js']) {
    const filename = path.resolve(__dirname, '../src/js', file);
    vm.runInContext(fs.readFileSync(filename, 'utf8'), ctx, { filename });
  }
  await settle();
  return { ctx, f, created, requests, bundles, downloads, panel: created[0] };
}
const report = (panel) => panel.querySelector('[data-diag-report]');
const refresh = (panel) => panel.querySelector('[data-diag-refresh]');
const status = (panel) => panel.querySelector('[data-diag-report-status]');

test('panel Report is explicitly unavailable and never invents a fixture or dispatches a write', async () => {
  const h = await panelUnderTest();
  const before = JSON.stringify({ state: h.f.state, undo: h.f.undo, redo: h.f.redo, trace: traceOf(h.f) });
  const queries = h.requests.length;
  report(h.panel).click();
  await settle();
  assert.match(status(h.panel).textContent, /reproduction unavailable/i);
  assert.match(status(h.panel).textContent, /native synthetic/i);
  assert.equal(h.bundles.length, 0);
  assert.equal(h.downloads.length, 0);
  assert.equal(h.requests.length, queries, 'report uses the displayed trace without dispatching');
  assert.equal(JSON.stringify({ state: h.f.state, undo: h.f.undo, redo: h.f.redo, trace: traceOf(h.f) }), before);
});

test('a stateBefore object or valid-looking hash does not make arbitrary user state replayable', async () => {
  const h = await panelUnderTest();
  h.ctx.NemoApplication.handle = (request) => {
    const response = h.f.app.handle(request);
    response.result.entries[0].stateBefore = { privateAsset: '/private/DO-NOT-EXPORT' };
    response.result.fixture = { id: 'user-document', hash: 'a'.repeat(64) };
    return response;
  };
  refresh(h.panel).click();
  await settle();
  report(h.panel).click();
  await settle();
  assert.match(status(h.panel).textContent, /reproduction unavailable/i);
  assert.equal(h.bundles.length, 0);
  assert.equal(h.downloads.length, 0);
});

test('report completion is discarded after document replacement', async () => {
  const h = await panelUnderTest();
  const oldStatus = status(h.panel);
  report(h.panel).click();
  h.f.app.documentChanged();
  await settle();
  assert.equal(oldStatus.textContent, '');
  assert.equal(h.downloads.length, 0);
});

test('a report started from an already stale document asks for Refresh', async () => {
  const h = await panelUnderTest();
  h.f.app.documentChanged();
  report(h.panel).click();
  await settle();
  assert.match(status(h.panel).textContent, /refresh/i);
  assert.equal(h.downloads.length, 0);
});

test('Refresh fences a pending report without updating its detached status', async () => {
  const h = await panelUnderTest();
  const oldStatus = status(h.panel);
  report(h.panel).click();
  refresh(h.panel).click();
  await settle();
  assert.equal(oldStatus.textContent, '');
  assert.equal(status(h.panel).textContent, '');
  assert.equal(h.downloads.length, 0);
});

test('disable and re-enable fences the prior panel report', async () => {
  const h = await panelUnderTest();
  const oldStatus = status(h.panel);
  report(h.panel).click();
  h.ctx.SMLabs.disable('diagnostics-panel');
  h.ctx.SMLabs.enable('diagnostics-panel');
  await settle();
  assert.equal(oldStatus.textContent, '');
  assert.equal(h.downloads.length, 0);
  assert.equal(status(h.created.find((node) => node.id === 'labs-diagnostics' && node !== h.panel)).textContent, '');
});

test('a delayed older refresh cannot overwrite a newer trace window', async () => {
  const h = await panelUnderTest();
  const pending = deferred();
  const oldResponse = h.f.app.handle({ apiVersion: 1, requestId: 'old-read', ...h.f.app.meta(), operation: 'diagnostics.trace', payload: {} });
  const before = h.panel.innerHTML;
  h.ctx.NemoApplication.handle = () => pending.promise;
  refresh(h.panel).click();
  await settle();
  assert.equal(h.panel.innerHTML, before, 'the pending request must not fabricate an error');
  h.ctx.NemoApplication.handle = (request) => h.f.app.handle(request);
  drive(h.f, 'newer-command', 52);
  refresh(h.panel).click();
  await settle();
  assert.match(h.panel.innerHTML, /newer-command/);
  pending.resolve(oldResponse);
  await settle();
  assert.match(h.panel.innerHTML, /newer-command/);
});

test('a delayed trace response is discarded after document replacement or panel disable', async () => {
  for (const action of ['replace', 'disable']) {
    const h = await panelUnderTest();
    const pending = deferred();
    const oldResponse = h.f.app.handle({ apiVersion: 1, requestId: 'old-read', ...h.f.app.meta(), operation: 'diagnostics.trace', payload: {} });
    const before = h.panel.innerHTML;
    h.ctx.NemoApplication.handle = () => pending.promise;
    refresh(h.panel).click();
    if (action === 'replace') h.f.app.documentChanged();
    else h.ctx.SMLabs.disable('diagnostics-panel');
    pending.resolve(oldResponse);
    await settle();
    assert.equal(h.panel.innerHTML, before, action);
    assert.equal(h.downloads.length, 0);
  }
});

test('the latest delayed query renders normally and rejected queries can be refreshed', async () => {
  const h = await panelUnderTest();
  const pending = deferred();
  drive(h.f, 'async-command', 72);
  let request;
  h.ctx.NemoApplication.handle = (value) => { request = value; return pending.promise; };
  refresh(h.panel).click();
  pending.resolve(h.f.app.handle(request));
  await settle();
  assert.match(h.panel.innerHTML, /async-command/);
  h.ctx.NemoApplication.handle = () => Promise.reject(new Error('/private/error-details'));
  refresh(h.panel).click();
  await settle();
  assert.match(h.panel.innerHTML, /diagnostics.trace failed/);
  assert.doesNotMatch(h.panel.innerHTML, /private/);
  h.ctx.NemoApplication.handle = (value) => h.f.app.handle(value);
  refresh(h.panel).click();
  await settle();
  assert.match(h.panel.innerHTML, /async-command/);
});

test('mismatched trace response identity is refused and trace labels remain injection-safe', async () => {
  const h = await panelUnderTest();
  const malicious = '<img src=x onerror="secret()">';
  drive(h.f, malicious, 55);
  refresh(h.panel).click();
  await settle();
  assert.ok(!h.panel.innerHTML.includes(malicious));
  assert.match(h.panel.innerHTML, /&lt;img/);
  h.ctx.NemoApplication.handle = (request) => ({ ...h.f.app.handle(request), documentId: 'different-document' });
  refresh(h.panel).click();
  await settle();
  assert.match(h.panel.innerHTML, /identity|document changed/i);
  assert.doesNotMatch(h.panel.innerHTML, /<table/);
});
