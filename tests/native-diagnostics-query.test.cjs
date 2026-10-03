'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const adapter = require('../src/js/adapters/native-application.js');
const nativeOperations = require('../src/js/application/native-opacity-operations.js');
const legacySurface = require('../src/js/adapters/native-opacity-legacy-surface.js');
const settle = () => new Promise((resolve) => setImmediate(resolve));
const query = () => ({ apiVersion: 2, requestId: 'read', instanceId: 'instance', documentId: 'document', operation: 'query.diagnostics.recent', payload: {} });
const record = (sequence = 1) => ({ sequence, operation: 'command.document.apply', requestId: 'edit-' + sequence, targetId: 'layer', contentRevision: sequence, ok: true });
const response = (request = query(), records = [record()]) => ({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 2, ok: true, result: { records, truncated: false } });
function validate(value) { return adapter.validateResponse(value, 'read', 'query.diagnostics.recent'); }

test('closed native diagnostics request, sequence ordering and redaction contract', () => {
  assert.doesNotThrow(() => adapter.validateRequest(query()));
  for (const bad of [{ ...query(), expectedRevision: 0 }, { ...query(), payload: { limit: 32 } }]) assert.throws(() => adapter.validateRequest(bad));
  const valid = response(query(), [record(1), { sequence: 2, operation: 'history.undo', contentRevision: 0, ok: false, requestIdRedacted: true, errorCode: 'busy_conflict' }]);
  assert.doesNotThrow(() => validate(valid), 'revision can go backwards across a retried attempt');
  const mutations = [
    (v) => { v.extra = true; }, (v) => { v.result.extra = true; },
    (v) => { delete v.result.truncated; }, (v) => { v.result.truncated = 1; },
    (v) => { v.result.records[0].payload = {}; }, (v) => { v.result.records[0].sequence = 0; },
    (v) => { v.result.records[0].stateBefore = { privateAsset: '/private/sentinel' }; },
    (v) => { v.result.fixture = { id: 'user-document', hash: 'a'.repeat(64) }; },
    (v) => { v.result.records[1].sequence = 1; }, (v) => { v.result.records.reverse(); },
    (v) => { v.result.records[0].contentRevision = -1; }, (v) => { v.result.records[0].ok = 'true'; },
    (v) => { v.result.records[0].operation = 'query.document.revision'; },
    (v) => { v.result.records[0].requestIdRedacted = true; }, (v) => { delete v.result.records[0].requestId; },
    (v) => { v.result.records[1].requestIdRedacted = false; },
    (v) => { v.result.records[0].targetId = 'private/path'; }, (v) => { v.result.records[0].requestId = 'private:path'; },
    (v) => { v.result.records[0].requestId = '<img src=x>'; },
    (v) => { v.result.records[0].errorCode = 'internal'; }, (v) => { delete v.result.records[1].errorCode; },
    (v) => { v.result.records[1].errorCode = 'secret'; },
  ];
  for (const mutate of mutations) { const v = structuredClone(valid); mutate(v); assert.throws(() => validate(v), mutate.toString()); }
});

test('32 records and 4096 UTF-8 bytes are whole-envelope bounds', () => {
  const records = Array.from({ length: 32 }, (_, i) => ({ sequence: i + 1, operation: 'history.undo', contentRevision: 0, ok: true, requestId: 'x' }));
  assert.doesNotThrow(() => validate(response(query(), records)));
  assert.throws(() => validate(response(query(), [...records, { ...records[0], sequence: 33 }])));
  const success = response(query(), structuredClone(records));
  for (const entry of success.result.records) {
    const extra = Math.min(127, 4096 - Buffer.byteLength(JSON.stringify(success)));
    entry.requestId += 'x'.repeat(extra);
    if (Buffer.byteLength(JSON.stringify(success)) === 4096) break;
  }
  assert.equal(Buffer.byteLength(JSON.stringify(success)), 4096);
  assert.doesNotThrow(() => validate(success));
  success.result.records.at(-1).requestId += 'x';
  assert.throws(() => validate(success), /4096/);
  const failure = { ...response(), ok: false, error: { code: 'internal', message: '' } };
  delete failure.result;
  failure.error.message = 'é'.repeat(Math.floor((4096 - Buffer.byteLength(JSON.stringify(failure))) / 2));
  while (Buffer.byteLength(JSON.stringify(failure)) < 4096) failure.error.message += 'x';
  assert.equal(Buffer.byteLength(JSON.stringify(failure)), 4096);
  assert.doesNotThrow(() => validate(failure));
  failure.error.message += 'x';
  assert.throws(() => validate(failure), /4096/);
});

test('diagnostics rejects mismatched instance/document on both success and failure', async () => {
  for (const key of ['instanceId', 'documentId']) for (const ok of [true, false]) {
    const port = adapter.createNativeApplicationAdapter('test', { dispatch(request) {
      const value = response(request); value[key] = 'other';
      if (!ok) { value.ok = false; delete value.result; value.error = { code: 'wrong_document', message: 'changed' }; }
      return value;
    } });
    await assert.rejects(port.dispatch(query()), /identity/);
  }
});

function element() {
  const listeners = {};
  return { style: {}, _html: '', textContent: '', found: {}, rows: [],
    get innerHTML() { return this._html; },
    set innerHTML(value) {
      this._html = value; this.found = {}; this.rows = [];
      for (const match of value.matchAll(/<tr data-layer-idx="(-?\d+)"/g)) { const row = element(); row.getAttribute = () => match[1]; this.rows.push(row); }
    },
    appendChild() {}, remove() {}, addEventListener(type, listener) { (listeners[type] ||= []).push(listener); },
    click() { for (const listener of listeners.click || []) listener(); },
    querySelector(selector) { return this.found[selector] ||= element(); }, querySelectorAll() { return this.rows; },
  };
}
function harness(options = {}) {
  const flags = {}, nodes = [], calls = [], selections = [];
  const model = { instanceId: options.instanceId || 'instance', documentId: 'document', generation: 1, contentRevision: 2,
    records: [record()], truncated: false, active: true, history: [100, 80], serialized: '{opacity:80}' };
  const ctx = { console, TextEncoder, state: { layers: [{ layerUid: 'layer' }], activeLayerIdx: 0 }, _layerSel: [],
    SM: { t: (key) => key, setActiveLayer: (...args) => selections.push(args), importJSON() {} },
    localStorage: { getItem: (key) => flags[key] ? '1' : null, setItem: (key, value) => { flags[key] = value === '1'; } },
    document: { createElement() { const el = element(); nodes.push(el); return el; }, body: element() },
    NemoApplication: { handle() { throw new Error('v1 fallback forbidden'); } },
    NemoOpacityApplication: { meta() {}, legacy() {} },
    addEventListener() {}, removeEventListener() {},
  };
  // Publish through the actual operations.install -> desktop surface.publish.
  // The installed cutover object is narrower than both core and lifecycle APIs.
  ctx.__TAURI__ = { core: { invoke() { throw new Error('unexpected publication dispatch'); } } };
  const surface = legacySurface.desktopPorts(ctx, {}).surface;
  delete ctx.__TAURI__;
  const lifecycle = { isActive: () => model.active, blocksLegacy: () => model.active,
    getNativeIdentity: () => ({ documentId: model.documentId, generation: model.generation }),
    identity: () => ({ instanceId: model.instanceId, documentId: model.documentId, contentRevision: model.contentRevision }) };
  nativeOperations.create(lifecycle, { surface: { installGuard() {}, wrap: () => () => {}, publish: surface.publish } }, {},
    { create: () => ({}) }, { create: () => ({ handle: () => null }) }).install();
  let invoke = async (command, args) => {
    calls.push({ command, args });
    if (command === 'nemo_native_status') return { apiVersion: 2, available: true, ...model };
    assert.equal(command, 'nemo_native_dispatch');
    assert.equal(args.request.operation, 'query.diagnostics.recent');
    const value = response(args.request, model.records); value.result.truncated = model.truncated;
    return value;
  };
  ctx.window = ctx; vm.createContext(ctx);
  // JSON crossing mimics Tauri's deserialization into the webview's own realm.
  ctx.hostInvoke = async (...args) => JSON.stringify(await invoke(...args));
  function load(file) {
    let code = fs.readFileSync(path.resolve(__dirname, '../src/js', file), 'utf8');
    if (options.counterStart !== undefined && file === 'labs/diagnostics-panel.js') code = code.replace('var minted = 0;', `var minted = ${options.counterStart};`);
    vm.runInContext(code, ctx, { filename: file });
  }
  function host() {
    load('adapters/native-application.js'); load('adapters/application-mcp.js'); load('adapters/native-diagnostics-query.js');
    vm.runInContext('window.__TAURI__ = {core: {invoke: async (...args) => JSON.parse(await hostInvoke(...args))}}', ctx);
  }
  if (!options.early) host();
  load('labs/labs-core.js'); load('labs/diagnostics-panel.js');
  ctx.SMLabs.enable('diagnostics-panel');
  return { ctx, model, calls, nodes, selections, host, get panel() { return nodes.at(-1); },
    setInvoke(fn) { const previous = invoke; invoke = fn; return previous; },
    refresh() { this.panel.querySelector('[data-diag-refresh]').click(); } };
}

test('real panel/service/adapter/transport query only, render sequence, redact IDs and preserve selection', async () => {
  const h = harness();
  h.model.records = [record(), { ...record(2), contentRevision: 0, requestId: 'later' },
    { sequence: 3, operation: 'history.undo', contentRevision: 3, ok: true, requestIdRedacted: true }];
  h.model.truncated = true;
  await settle();
  assert.ok(h.panel.innerHTML.indexOf('later') < h.panel.innerHTML.indexOf('edit-1'));
  assert.match(h.panel.innerHTML, /\[redacted\]/); assert.match(h.panel.innerHTML, /data-diag-truncated/);
  assert.match(h.panel.innerHTML, /background:rgba/);
  h.panel.rows[1].click(); assert.deepEqual(h.selections, [[0, true]]);
  const before = JSON.stringify(h.model);
  for (let i = 0; i < 3; i++) { h.refresh(); await settle(); }
  assert.equal(JSON.stringify(h.model), before);
  const requests = h.calls.filter((c) => c.args).map((c) => c.args.request);
  assert.equal(requests.length, 4); assert.equal(new Set(requests.map((r) => r.requestId)).size, 4);
  assert.ok(requests.every((r) => r.apiVersion === 2 && r.expectedRevision === undefined && Object.keys(r.payload).length === 0));
  const staleRow = h.panel.rows[1]; h.model.generation++; staleRow.click(); assert.equal(h.selections.length, 1);
});

test('early persisted panel and browser/WASM are unavailable; later manual Refresh recovers without v1', async () => {
  const h = harness({ early: true });
  assert.match(h.panel.innerHTML, /unavailable.*browser\/WASM/);
  h.host(); h.refresh(); await settle(); assert.match(h.panel.innerHTML, /edit-1/);
  h.model.active = false; h.refresh(); assert.match(h.panel.innerHTML, /unavailable/);
  assert.equal(h.calls.filter((c) => c.args).length, 1);
});

test('native diagnostics captures the actual published cutover without status or inspect', async () => {
  const h = harness();
  assert.equal(h.ctx.NemoNativeOpacityCutover.inspect, undefined);
  assert.equal(h.ctx.NemoNativeOpacityCutover.status, undefined);
  assert.ok(Object.isFrozen(h.ctx.NemoNativeOpacityCutover));
  assert.equal(typeof h.ctx.NemoNativeOpacityCutover.getNativeIdentity, 'function');
  const captured = h.ctx.NemoNativeDiagnosticsQuery.capture(h.ctx);
  assert.equal(captured.instanceId, h.model.instanceId);
  assert.equal(captured.documentId, h.model.documentId);
  assert.equal(captured.generation, h.model.generation);
  await settle(); assert.match(h.panel.innerHTML, /edit-1/);
});

for (const reject of [false, true]) for (const transition of ['instance', 'document', 'controller', 'A-B-A', 'A-B-B', 'reopen', 'refresh']) {
  test(`delayed ${reject ? 'error' : 'success'} fenced on ${transition}`, async () => {
    const h = harness(); await settle();
    let resolve, fail, dispatched;
    const pending = new Promise((yes, no) => { resolve = yes; fail = no; });
    const original = h.setInvoke((command, args) => {
      if (command === 'nemo_native_dispatch') { dispatched = args.request; return pending; }
      return original(command, args);
    });
    h.refresh(); await settle(); assert.ok(dispatched);
    if (transition === 'instance') h.model.instanceId = 'replacement-instance';
    if (transition === 'document') h.model.documentId = 'replacement-document';
    if (transition === 'controller') h.ctx.NemoNativeOpacityCutover = { ...h.ctx.NemoNativeOpacityCutover };
    if (transition === 'A-B-B') h.model.generation += 2;
    if (transition === 'A-B-A') {
      const originalDocument = h.model.documentId;
      h.model.documentId = 'replacement-B'; h.model.generation++;
      h.model.documentId = originalDocument; h.model.generation++;
    }
    if (transition === 'reopen') { h.ctx.SMLabs.disable('diagnostics-panel'); h.ctx.SMLabs.enable('diagnostics-panel'); }
    h.setInvoke(original);
    h.model.records = [{ ...record(2), requestId: 'current-trace' }];
    h.refresh(); await settle(); assert.match(h.panel.innerHTML, /current-trace/);
    const current = h.panel.innerHTML;
    if (reject) fail(new Error('/private/error')); else resolve(response(dispatched, [{ ...record(), requestId: 'old-trace' }]));
    await settle(); assert.equal(h.panel.innerHTML, current);
  });
}

test('latest errors are generic and refreshable; hostile trace text never reaches HTML', async () => {
  const h = harness(); await settle();
  const original = h.setInvoke(() => Promise.reject(new Error('<img src=x onerror=secret()>')));
  h.refresh(); await settle(); assert.match(h.panel.innerHTML, /query failed/); assert.doesNotMatch(h.panel.innerHTML, /<img|secret/);
  h.setInvoke(original); h.model.records[0].requestId = '<img src=x onerror=secret()>';
  h.refresh(); await settle(); assert.match(h.panel.innerHTML, /query failed/); assert.doesNotMatch(h.panel.innerHTML, /<img|secret/);
  h.model.records[0].requestId = 'recovered'; h.refresh(); await settle(); assert.match(h.panel.innerHTML, /recovered/);
});

test('Report remains unavailable, never derives a bundle, and is fenced across refresh/replacement/reopen', async () => {
  for (const transition of ['none', 'refresh', 'replace', 'reopen']) {
    const h = harness(); await settle(); const before = JSON.stringify(h.model), count = h.calls.length;
    const status = h.panel.querySelector('[data-diag-report-status]'); h.panel.querySelector('[data-diag-report]').click();
    if (transition === 'refresh') h.refresh();
    if (transition === 'replace') h.model.generation++;
    if (transition === 'reopen') { h.ctx.SMLabs.disable('diagnostics-panel'); h.ctx.SMLabs.enable('diagnostics-panel'); }
    await settle();
    if (transition === 'none') {
      assert.match(status.textContent, /Reproduction unavailable.*native synthetic/);
      assert.equal(JSON.stringify(h.model), before); assert.equal(h.calls.length, count);
    } else assert.equal(status.textContent, '');
  }
});

test('T12 panel counter survives reopen, clipped identity collisions, and fails closed at exhaustion', async () => {
  const h = harness({ counterStart: Number.MAX_SAFE_INTEGER - 1, instanceId: 'x'.repeat(128) });
  await settle(); assert.match(h.panel.innerHTML, /edit-1/);
  const last = h.calls.find((c) => c.args).args.request;
  assert.ok(last.requestId.endsWith(':' + Number.MAX_SAFE_INTEGER));
  assert.equal(last.instanceId.length, 128); assert.ok(last.requestId.length <= 128);
  h.refresh(); await settle();
  assert.match(h.panel.innerHTML, /counter exhausted/);
  const count = h.calls.length;
  h.ctx.SMLabs.disable('diagnostics-panel'); h.ctx.SMLabs.enable('diagnostics-panel');
  assert.match(h.panel.innerHTML, /counter exhausted/); assert.equal(h.calls.length, count);
  const normal = harness(); await settle();
  const hint = 'h'.repeat(64);
  for (const suffix of ['first', 'second']) {
    normal.model.instanceId = hint + suffix;
    normal.ctx.SMLabs.disable('diagnostics-panel'); normal.ctx.SMLabs.enable('diagnostics-panel'); await settle();
  }
  const requests = normal.calls.filter((c) => c.args).map((c) => c.args.request);
  assert.equal(new Set(requests.map((r) => r.requestId)).size, 3);
  assert.equal(requests[1].instanceId, hint + 'first'); assert.equal(requests[2].instanceId, hint + 'second');
  assert.ok(requests.every((r) => r.requestId.length <= 128));
});

test('T12 each of 200 reopen queries has a consecutive module counter', async () => {
  const h = harness(); await settle();
  for (let i = 1; i < 200; i++) {
    h.ctx.SMLabs.disable('diagnostics-panel'); h.ctx.SMLabs.enable('diagnostics-panel'); await settle();
  }
  const ids = h.calls.filter((c) => c.args).map((c) => c.args.request.requestId);
  assert.equal(ids.length, 200); assert.equal(new Set(ids).size, 200);
  assert.deepEqual(ids.map((id) => Number(id.split(':').at(-1))), Array.from({ length: 200 }, (_, i) => i + 1));
});

test('service refuses missing, transitional, malformed or replaced native authority without dispatch', async () => {
  const h = harness(); await settle(); const service = h.ctx.NemoNativeDiagnosticsQuery;
  const identity = service.capture(h.ctx), controller = h.ctx.NemoNativeOpacityCutover;
  h.ctx.NemoNativeOpacityCutover = null; assert.equal(service.capture(h.ctx), null);
  h.ctx.NemoNativeOpacityCutover = { ...controller, identity() { throw new Error('disconnected'); } };
  assert.equal(service.capture(h.ctx), null);
  h.ctx.NemoNativeOpacityCutover = { ...controller, isActive: () => false };
  assert.equal(service.capture(h.ctx), null);
  for (const view of [null, { documentId: 'other', generation: 1 }, { documentId: h.model.documentId, generation: -1 },
    { documentId: h.model.documentId, generation: Number.MAX_SAFE_INTEGER + 1 }]) {
    h.ctx.NemoNativeOpacityCutover = { ...controller, getNativeIdentity: () => view }; assert.equal(service.capture(h.ctx), null);
  }
  await assert.rejects(service.recent(h.ctx, identity, 'read'), /unavailable/);
  h.ctx.NemoNativeOpacityCutover = controller;
  const original = h.setInvoke(async (command, args) => {
    const value = await original(command, args);
    if (command === 'nemo_native_status') value.documentId = 'other';
    return value;
  });
  h.refresh(); await settle(); assert.match(h.panel.innerHTML, /query failed/);
  assert.equal(h.calls.filter((c) => c.args).length, 1);
});

test('stale Report requests Refresh; matching native error envelopes stay generic', async () => {
  const h = harness(); await settle(); h.model.generation++;
  h.panel.querySelector('[data-diag-report]').click();
  assert.match(h.panel.querySelector('[data-diag-report-status]').textContent, /refresh/);
  const original = h.setInvoke(async (command, args) => {
    const value = await original(command, args);
    if (command === 'nemo_native_dispatch') { value.ok = false; delete value.result; value.error = { code: 'internal', message: '/private/secret' }; }
    return value;
  });
  h.refresh(); await settle(); assert.match(h.panel.innerHTML, /query failed/); assert.doesNotMatch(h.panel.innerHTML, /secret/);
});
