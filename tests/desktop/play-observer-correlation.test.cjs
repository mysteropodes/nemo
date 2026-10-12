'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ROOT = path.resolve(__dirname, '../..');
const RUN = 'da5f47f9-5681-4e71-a0b6-aa7578b6b5fa';
function realm(mode = 'on') {
  const window = {}, deliveries = [], cache = new Map();
  const context = vm.createContext({ window, console, TextEncoder, URL, setTimeout, clearTimeout });
  vm.runInContext('globalThis.structuredClone = value => JSON.parse(JSON.stringify(value));', context);
  function load(file) {
    file = path.resolve(file);
    if (cache.has(file)) return cache.get(file).exports;
    const module = { exports: {} }; cache.set(file, module);
    const localRequire = name => name.startsWith('.') ? load(path.resolve(path.dirname(file), name)) : require(name);
    const fn = vm.runInContext('(function(require,module,exports,__dirname,__filename){' + fs.readFileSync(file, 'utf8') + '\n})', context, { filename: file });
    fn(localRequire, module, module.exports, path.dirname(file), file); return module.exports;
  }
  const observer = load(path.join(ROOT, 'tests/desktop/play-publication-observer-bootstrap.js'));
  const api = observer.createObserver({ runId: RUN }, {
    invoke(action, payload) { deliveries.push({ action, payload });
      return mode === 'reject' ? Promise.reject(Error('transport failure')) : Promise.resolve({ sequence: payload.sequence }); },
    async listen() { return () => {}; },
  });
  if (mode === 'on' || mode === 'reject') window.NemoTestPlayObserver = api;
  if (mode === 'full') window.NemoTestPlayObserver = new Proxy(api, { get() { return () => { throw Error('full sink'); }; } });
  if (mode === 'missing') window.NemoTestPlayObserver = Object.assign({}, api, { bindPreview: undefined });
  if (mode === 'throwing') Object.defineProperty(window, 'NemoTestPlayObserver', { get() { throw Error('observer inaccessible'); } });
  const file = path.join(ROOT, 'tests/native-opacity-cutover.test.cjs');
  const fixture = fs.readFileSync(file, 'utf8');
  const end = fixture.indexOf("test('N20 characterization"); assert.ok(end > 0);
  // Only adapt fixture host identity encoding to the real collector UUID contract.
  const prefix = fixture.slice(0, end).replaceAll("'instance-a'", JSON.stringify(RUN));
  const fn = vm.runInContext('(function(require,__dirname){' + prefix + '\nreturn {nativeHarness,staticSource};})', context);
  const helpers = fn(name => name.startsWith('.') ? load(path.resolve(path.dirname(file), name)) : require(name), path.dirname(file));
  return { context, window, api, deliveries, helpers, load,
    events() { return deliveries.filter(x => x.action === 'record').map(x => JSON.parse(JSON.stringify(x.payload.event))); } };
}
test('actual lifecycle/raw preview/viewport joins the captured session and native host generation', async () => {
  const r = realm(); await r.api.ready;
  const options = vm.runInContext('({ previewReceipt(receipt) { receipt.workId = "native-work-" + receipt.viewGeneration; return receipt; } })', r.context);
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), options);
  await h.controller.activate(h.prepared);
  const viewport = r.load(path.join(ROOT, 'src/js/application/native-opacity-viewport.js')).create(h.state.lifecycle, {}, () => false, () => true);
  await viewport.presentPreview(10);
  await new Promise(resolve => setImmediate(resolve));
  const events = r.events().filter(e => e.event !== 'ready');
  assert.deepEqual(events.map(e => e.event), ['host-request', 'host-receipt', 'presented-return']);
  for (const e of events) {
    assert.equal(e.sessionOrdinal, 1); assert.equal(e.hostGeneration, h.state.hostGeneration);
    assert.equal(e.jsLifecycleGeneration, 1); assert.equal(e.hostCallOrdinal, 1);
    assert.equal(e.documentId, 'native-document-1'); assert.equal(e.frame, 10);
    assert.equal(typeof e.documentSnapshotId, 'string');
  }
  assert.equal(events[1].workId, events[2].workId);
  assert.equal(events[1].status, 'presented'); assert.equal(events[2].status, 'presented');
  assert.equal(r.api.inspect().dropped, 0);
});

function viewport(r, h) {
  return r.load(path.join(ROOT, 'src/js/application/native-opacity-viewport.js')).create(h.state.lifecycle, {}, () => false, () => true);
}
function options(r, status = 'presented') {
  return vm.runInContext(`({ previewReceipt(receipt) { receipt.workId = 'native-work-' + receipt.viewGeneration;
    receipt.status = ${JSON.stringify(status)}; return receipt; } })`, r.context);
}
async function run(mode, status = 'presented') {
  const r = realm(mode); await r.api.ready;
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), options(r, status));
  await h.controller.activate(h.prepared);
  let value, error;
  try { value = await viewport(r, h).presentPreview(10); }
  catch (e) { error = e.message; }
  await new Promise(resolve => setImmediate(resolve));
  return { r, h, value, error, summary: JSON.parse(JSON.stringify({ value, error,
    phase: h.state.lifecycle.inspect().phase, calls: h.state.previews.length,
    revision: h.state.identity.contentRevision, hostGeneration: h.state.hostGeneration })) };
}
for (const status of ['presented', 'deferred-occluded', 'deferred-timeout', 'failed-device-lost', 'invalid']) {
  test(`absent/throwing/full/rejecting observer preserves actual ${status} product result and calls`, async () => {
    const baseline = await run('off', status);
    for (const mode of ['on', 'throwing', 'full', 'reject']) {
      const actual = await run(mode, status);
      assert.deepEqual(actual.summary, baseline.summary, mode);
      if (mode === 'on' && status !== 'presented') {
        assert.equal(actual.r.events().some(e => e.event === 'presented-return'), false);
        if (status === 'invalid') assert.ok(actual.r.api.inspect().dropped > 0);
        else assert.equal(actual.r.events().some(e => e.event === 'host-receipt'), true);
      }
    }
  });
}
test('missing raw-adapter session binding stays partial without inventing a native generation', async () => {
  const actual = await run('missing');
  assert.equal(actual.value.status, 'presented');
  assert.ok(actual.r.api.inspect().dropped > 0);
  for (const e of actual.r.events().filter(e => e.event !== 'ready')) {
    assert.equal(Object.hasOwn(e, 'sessionOrdinal'), false);
    assert.equal(Object.hasOwn(e, 'hostGeneration'), false);
  }
});
test('retained final receipt keeps old immutable context after actual release/re-entry', async () => {
  const r = realm(); await r.api.ready;
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), options(r));
  await h.controller.activate(h.prepared);
  const old = await viewport(r, h).presentPreview(10), context = r.api.receiptContext(old);
  assert.equal(Object.isFrozen(context), true);
  await h.controller.releaseCurrent('test-close');
  await h.controller.activate(h.prepared);
  const next = await viewport(r, h).presentPreview(20);
  assert.equal(r.api.receiptContext(next).sessionOrdinal, 2);
  assert.equal(r.api.receiptContext(next).hostGeneration, 2);
  assert.equal(r.api.receiptContext(next).documentId, 'native-document-2');
  assert.strictEqual(r.api.receiptContext(old), context);
  assert.equal(context.hostGeneration, 1); assert.equal(context.sessionOrdinal, 1);
});

// Independent literal join, not the collector's internal maps or implementation.
function joined(events) {
  const keys = ['instanceId', 'documentId', 'contentRevision', 'documentSnapshotId', 'contextId',
    'frame', 'jsLifecycleGeneration', 'hostGeneration', 'sessionOrdinal', 'hostCallOrdinal'];
  return events.filter(e => e.event === 'presented-return').every((final, _, all) => {
    if (!all.length || final.status !== 'presented' || keys.some(k => final[k] === undefined)) return false;
    const index = events.indexOf(final), earlier = events.slice(0, index);
    const request = earlier.find(e => e.event === 'host-request' && keys.every(k => e[k] === final[k]));
    const raw = earlier.find(e => e.event === 'host-receipt' && e.status === 'presented' &&
      e.workId === final.workId && e.viewGeneration === final.viewGeneration && keys.every(k => e[k] === final[k]));
    return !!request && !!raw && events.indexOf(request) < events.indexOf(raw);
  }) && events.some(e => e.event === 'presented-return');
}
test('real collected chain rejects independent omission/order/snapshot/session/call/generation/work mutants', async () => {
  const { r } = await run('on'); const original = r.events(); assert.equal(joined(original), true);
  assert.equal(original.find(e => e.event === 'host-request').documentSnapshotId, 'native-opacity:native-document-1:0');
  for (const key of ['documentSnapshotId', 'sessionOrdinal', 'hostCallOrdinal', 'hostGeneration', 'workId']) {
    const changed = structuredClone(original), raw = changed.find(e => e.event === 'host-receipt');
    raw[key] = typeof raw[key] === 'number' ? raw[key] + 1 : 'wrong'; assert.equal(joined(changed), false, key);
  }
  assert.equal(joined(original.filter(e => e.event !== 'host-receipt')), false);
  const swapped = structuredClone(original); [swapped[1], swapped[2]] = [swapped[2], swapped[1]];
  assert.equal(joined(swapped), false);
});
test('raw host output is observed before the existing strict receipt rejects extra payload fields', async () => {
  const r = realm(); await r.api.ready;
  const input = options(r); input.previewReceipt = vm.runInContext('(receipt => ({...receipt,workId:"native-work-1",secret:"must-not-be-collected"}))', r.context);
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), input); await h.controller.activate(h.prepared);
  await assert.rejects(viewport(r, h).presentPreview(10), /receipt/);
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(r.events().slice(1).map(e => e.event), ['host-request', 'host-receipt', 'receipt-rejected']);
  assert.equal(JSON.stringify(r.events()).includes('must-not-be-collected'), false);
});
test('overlapping actual calls retain distinct host ordinals and immutable requested frames', async () => {
  const r = realm(); await r.api.ready;
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), options(r)); await h.controller.activate(h.prepared);
  const v = viewport(r, h), first = v.presentPreview(10), second = v.presentPreview(20);
  const [a,b] = await Promise.all([first, second]);
  const ca = r.api.receiptContext(a), cb = r.api.receiptContext(b);
  assert.equal(ca.frame, 10); assert.equal(cb.frame, 20);
  assert.notEqual(ca.hostCallOrdinal, cb.hostCallOrdinal);
  assert.equal(ca.sessionOrdinal, cb.sessionOrdinal);
});
test('actual revision drift rejects the old raw receipt and captures the new snapshot without a new session', async () => {
  const r = realm(); await r.api.ready;
  const gate = vm.runInContext('new Promise(resolve => { globalThis.finishHost = resolve; })', r.context);
  const input = options(r); input.previewGate = gate;
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), input); await h.controller.activate(h.prepared);
  const v = viewport(r, h), old = v.presentPreview(10);
  const rejected = assert.rejects(old, /synchronization is pending/);
  await new Promise(resolve => setImmediate(resolve));
  const revised = h.externalOpacity(40); r.context.finishHost();
  await Promise.all([rejected, revised]);
  const next = await v.presentPreview(20); const context = r.api.receiptContext(next);
  assert.equal(context.contentRevision, 1); assert.equal(context.sessionOrdinal, 1);
  assert.equal(context.hostGeneration, 1);
  assert.equal(context.documentSnapshotId, 'native-opacity:native-document-1:1');
  await new Promise(resolve => setImmediate(resolve));
  const events = r.events();
  assert.ok(events.some(e => e.event === 'host-receipt' && e.contentRevision === 0));
  assert.equal(events.some(e => e.event === 'presented-return' && e.contentRevision === 0), false);
});
test('pending raw host completion during actual release cannot acquire a final presented association', async () => {
  const r = realm(); await r.api.ready;
  const gate = vm.runInContext('new Promise(resolve => { globalThis.finishHost = resolve; })', r.context);
  const input = options(r); input.previewGate = gate;
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), input); await h.controller.activate(h.prepared);
  const old = viewport(r, h).presentPreview(10), rejected = assert.rejects(old, /active|superseded/);
  await new Promise(resolve => setImmediate(resolve));
  const released = h.controller.releaseCurrent('pending-close'); r.context.finishHost();
  await Promise.all([rejected, released]);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.controller.status(), 'closed');
  assert.equal(r.events().some(e => e.event === 'presented-return'), false);
});
test('actual resize overlap preserves the original retry and associates only its final host call', async () => {
  const r = realm(); await r.api.ready;
  const gate = vm.runInContext('new Promise(resolve => { globalThis.finishHost = resolve; })', r.context);
  const input = options(r); input.previewGate = gate;
  const h = r.helpers.nativeHarness(r.helpers.staticSource(), input); await h.controller.activate(h.prepared);
  let resizes = 0;
  const surface = { defer() { return 1; }, cancel() {}, async resize() { resizes++; } };
  const v = r.load(path.join(ROOT, 'src/js/application/native-opacity-viewport.js')).create(h.state.lifecycle, { surface }, () => false, () => true);
  const pending = v.presentPreview(10); await new Promise(resolve => setImmediate(resolve));
  v.resizeViewport(); r.context.finishHost(); const final = await pending;
  assert.equal(resizes, 1); assert.equal(h.state.previews.length, 2);
  assert.equal(r.api.receiptContext(final).hostCallOrdinal, 2);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(r.events().filter(e => e.event === 'host-receipt').length, 2);
  assert.equal(r.events().filter(e => e.event === 'presented-return').length, 1);
});
