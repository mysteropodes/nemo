'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const adapter = require('../src/js/adapters/native-application.js');
const nativeOperations = require('../src/js/application/native-opacity-operations.js');
const legacySurface = require('../src/js/adapters/native-opacity-legacy-surface.js');
const nativeFixture = { id: 'native-opacity-static', version: 1, sha256: '895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050' };
function nativeBundle() { return { format: 'nemo.native-opacity-reproduction', formatVersion: 1, apiVersion: 2, fixture: nativeFixture,
  command: 'layer.opacity.set', stableTarget: { layerUid: 'r08_curve_layer' }, clock: null, seed: null, versions: { nativeEngine: '0.1.0' },
  commands: [{ id: 1, expectedRevision: 0, value: 40, revision: 1, applied: true }] }; }
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
    appendChild() {}, remove() { this.removed = true; }, addEventListener(type, listener) { (listeners[type] ||= []).push(listener); },
    click() { for (const listener of listeners.click || []) listener(); },
    querySelector(selector) { return this.found[selector] ||= element(); }, querySelectorAll() { return this.rows; },
  };
}
function harness(options = {}) {
  const flags = {}, nodes = [], calls = [], selections = [], downloads = [], revoked = [], timers = new Map();
  const model = { instanceId: options.instanceId || 'instance', documentId: 'document', generation: 1, contentRevision: 2,
    records: [record()], truncated: false, active: !options.catalogIdle, history: [100, 80], serialized: '{opacity:80}' };
  const ctx = { console, TextEncoder, Blob, setTimeout(fn) { timers.set(timers.size + 1, fn); return timers.size; }, clearTimeout(id) { timers.delete(id); },
    URL: { createObjectURL(blob) { downloads.push(blob); return 'blob:owned'; }, revokeObjectURL(url) { revoked.push(url); } },
    state: { layers: [{ layerUid: 'layer' }], activeLayerIdx: 0 }, _layerSel: [],
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
    if (command === 'nemo_native_status') return { apiVersion: 2, available: model.active, ...model, documentId: model.active ? model.documentId : null };
    if (command === 'nemo_mcp_identity') return { instanceId: model.instanceId };
    if (command === 'nemo_native_revision_sync') return { ...model, lifecycleGeneration: model.generation, subscriptionId: 'binding' };
    if (command === 'nemo_native_reproduction_session') {
      model.active = true; model.documentId = 'catalog'; model.generation++; model.contentRevision = 0; model.records = [];
      model.opacity = 25; model.reportBundle = nativeBundle(); model.reportBundle.commands = [];
      return { apiVersion: 2, instanceId: model.instanceId, documentId: model.documentId, contentRevision: 0,
        lifecycleGeneration: model.generation, origin: 'embedded_catalog', fixture: nativeFixture,
        reproduction: { state: 'recording', reason: null, commandCount: 0, exportable: false }, viewportAvailable: false, resourceCount: 0 };
    }
    if (command === 'nemo_native_release') {
      model.active = false; model.generation++;
      return { apiVersion: 2, requestId: args.request.requestId, ...model, lifecycleGeneration: model.generation,
        status: 'succeeded', authorityRemovalCompleted: true, reentryAvailable: true };
    }
    assert.equal(command, 'nemo_native_dispatch');
    if (args.request.operation === 'command.document.apply') {
      const expectedRevision = model.contentRevision, value = args.request.payload.value, applied = value !== model.opacity;
      model.contentRevision += applied ? 1 : 0; model.opacity = value;
      model.records.push({ ...record(model.records.length + 1), contentRevision: model.contentRevision, targetId: 'r08_curve_layer' });
      model.reportBundle.commands.push({ id: model.reportBundle.commands.length + 1, expectedRevision, value, revision: model.contentRevision, applied });
      return { apiVersion: 2, requestId: args.request.requestId, instanceId: model.instanceId, documentId: model.documentId,
        contentRevision: model.contentRevision, ok: true, result: { applied, historyEntriesAdded: applied ? 1 : 0 } };
    }
    if (args.request.operation === 'query.reproduction.report') {
      const payload = args.request.payload;
      return { apiVersion: 2, requestId: args.request.requestId, instanceId: model.instanceId, documentId: model.documentId,
        contentRevision: model.contentRevision, ok: true, result: { bundle: model.reportBundle || nativeBundle(), verifiedContentRevision: payload.expectedContentRevision, verifiedSequence: payload.expectedSequence } };
    }
    assert.equal(args.request.operation, 'query.diagnostics.recent');
    const value = response(args.request, model.records); value.contentRevision = model.contentRevision; value.result.truncated = model.truncated;
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
    load('adapters/native-application.js'); load('adapters/application-mcp.js'); load('adapters/native-reproduction.js'); load('adapters/native-diagnostics-query.js');
    vm.runInContext('window.__TAURI__ = {core: {invoke: async (...args) => JSON.parse(await hostInvoke(...args))}}', ctx);
  }
  if (!options.early) host();
  load('labs/labs-core.js'); load('labs/diagnostics-panel.js');
  ctx.SMLabs.enable('diagnostics-panel');
  return { ctx, model, calls, nodes, selections, downloads, revoked, host, flush() { for (const fn of [...timers.values()]) fn(); }, get panel() { return nodes.find(n => n.id === 'labs-diagnostics' && !n.removed) || nodes.at(-1); },
    setInvoke(fn) { const previous = invoke; invoke = fn; return previous; },
    refresh() { this.panel.querySelector('[data-diag-refresh]').click(); } };
}

test('explicit Start, catalog edit, atomic displayed-token Report download and End use production adapters', async () => {
  const h = harness({ catalogIdle: true }); await settle();
  assert.equal(h.calls.length, 0, 'panel enable must not admit a session');
  assert.match(h.panel.innerHTML, /data-diag-start/);
  h.panel.querySelector('[data-diag-start]').click(); await settle();
  assert.match(h.panel.innerHTML, /Synthetic catalog session/);
  h.panel.querySelector('[data-diag-opacity]').value = '40'; h.panel.querySelector('[data-diag-apply]').click(); await settle();
  const before = JSON.stringify(h.model);
  h.panel.querySelector('[data-diag-report]').click(); await settle();
  assert.equal(h.downloads.length, 1); assert.deepEqual(JSON.parse(await h.downloads[0].text()), nativeBundle());
  assert.deepEqual(h.revoked, []); h.flush(); assert.deepEqual(h.revoked, ['blob:owned']); assert.equal(JSON.stringify(h.model), before);
  const r = h.calls.find(c => c.args?.request?.operation === 'query.reproduction.report').args.request;
  assert.deepEqual(JSON.parse(JSON.stringify(r.payload)), { expectedContentRevision: 1, expectedSequence: 1 }); assert.equal(r.expectedRevision, undefined);
  h.panel.querySelector('[data-diag-end]').click(); await settle(); assert.equal(h.model.active, false);
});

test('atomic report rejection, hostile bundle and delayed refresh/reopen/replacement never download', async () => {
  for (const transition of ['stale_revision', 'busy_conflict', 'unavailable', 'private', 'refresh', 'disable', 'reopen', 'replacement']) {
    const h = harness({ catalogIdle: true });
    h.panel.querySelector('[data-diag-start]').click(); await settle();
    h.panel.querySelector('[data-diag-opacity]').value = '40'; h.panel.querySelector('[data-diag-apply]').click(); await settle();
    let complete, pendingRequest;
    const prior = h.setInvoke(async (command, args) => {
      if (args?.request?.operation !== 'query.reproduction.report') return prior(command, args);
      pendingRequest = args.request;
      return new Promise(resolve => { complete = resolve; });
    });
    h.panel.querySelector('[data-diag-report]').click(); await settle(); assert.ok(pendingRequest);
    const response = { apiVersion: 2, requestId: pendingRequest.requestId, instanceId: h.model.instanceId, documentId: h.model.documentId,
      contentRevision: 1, ok: true, result: { bundle: nativeBundle(), verifiedContentRevision: 1, verifiedSequence: 1 } };
    if (['stale_revision', 'busy_conflict', 'unavailable'].includes(transition)) { response.ok = false; delete response.result; response.error = { code: transition, message: '/private/sentinel' }; }
    if (transition === 'private') response.result.bundle.private = '/private/sentinel';
    if (transition === 'refresh') h.refresh();
    if (transition === 'disable') h.ctx.SMLabs.disable('diagnostics-panel');
    if (transition === 'reopen') { h.ctx.SMLabs.disable('diagnostics-panel'); h.ctx.SMLabs.enable('diagnostics-panel'); }
    if (transition === 'replacement') h.model.generation += 2;
    complete(response); await settle(); assert.equal(h.downloads.length, 0); assert.doesNotMatch(h.panel.innerHTML, /sentinel/);
  }
});
test('panel disposal fences queued Start and delayed admission without implicit reentry', async () => {
  const queued = harness({ catalogIdle: true }); queued.panel.querySelector('[data-diag-start]').click();
  queued.ctx.SMLabs.disable('diagnostics-panel'); await settle(); assert.equal(queued.calls.length, 0);
  const h = harness({ catalogIdle: true }); let complete;
  const prior = h.setInvoke(async (command, args) => {
    if (command !== 'nemo_native_reproduction_session') return prior(command, args);
    const admitted = await prior(command, args); return new Promise(resolve => { complete = () => resolve(admitted); });
  });
  const oldPanel = h.panel; oldPanel.querySelector('[data-diag-start]').click(); await settle();
  assert.equal(h.calls.filter(c => c.command === 'nemo_native_reproduction_session').length, 1);
  h.ctx.SMLabs.disable('diagnostics-panel'); const oldMarkup = oldPanel.innerHTML;
  complete(); await settle(); assert.equal(oldPanel.innerHTML, oldMarkup); assert.equal(h.downloads.length, 0);
  h.ctx.SMLabs.enable('diagnostics-panel'); await settle(); assert.match(h.panel.innerHTML, /Synthetic catalog session/);
  assert.equal(h.calls.filter(c => c.command === 'nemo_native_reproduction_session').length, 1);
  h.panel.querySelector('[data-diag-report]').click(); await settle(); assert.equal(h.downloads.length, 0, 'empty native journal has no portable report');
});
test('same-revision no-op and retry preserve displayed sequence, portable capture and URL retirement', async () => {
  const h = harness({ catalogIdle: true }); h.panel.querySelector('[data-diag-start]').click(); await settle();
  for (const value of ['40', '60', '60']) {
    h.panel.querySelector('[data-diag-opacity]').value = value; h.panel.querySelector('[data-diag-apply]').click(); await settle();
  }
  assert.equal(h.model.contentRevision, 2); assert.equal(h.model.records.at(-1).sequence, 3);
  h.panel.querySelector('[data-diag-report]').click(); await settle();
  const downloaded = JSON.parse(await h.downloads[0].text());
  assert.deepEqual(downloaded.commands, [
    { id: 1, expectedRevision: 0, value: 40, revision: 1, applied: true },
    { id: 2, expectedRevision: 1, value: 60, revision: 2, applied: true },
    { id: 3, expectedRevision: 2, value: 60, revision: 2, applied: false },
  ]);
  assert.deepEqual(h.revoked, []); h.refresh(); assert.deepEqual(h.revoked, ['blob:owned']); await settle();
  h.model.records.push({ ...record(4), contentRevision: 2 }); h.refresh(); await settle();
  h.panel.querySelector('[data-diag-report]').click(); await settle();
  const r = h.calls.filter(c => c.args?.request?.operation === 'query.reproduction.report').at(-1).args.request;
  assert.equal(r.payload.expectedContentRevision, 2); assert.equal(r.payload.expectedSequence, 4);
  assert.deepEqual(JSON.parse(await h.downloads[1].text()), downloaded, 'native retry is trace detail, not a new journal command');
  h.ctx.SMLabs.disable('diagnostics-panel'); assert.equal(h.revoked.length, 2); h.flush(); assert.equal(h.revoked.length, 2);
});

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
