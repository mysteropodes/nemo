'use strict';
// L02-J layer lifecycle + selection edit + clipboard intents: literal wire requests, selection rebuild, typed unavailability.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const { createHarness, request } = require('./native-object-edit-harness.cjs');
const { createLayerLifecycleIntent, OPERATIONS } = require('../src/js/adapters/native-object-layer-lifecycle-intent.js');

const P = 'command.document.object.';
const cell = { contextId: 'scene-root', layerUid: 'layer-a', frame: 2 };
const strokes = { 'layer-a': ['stroke-1', 'stroke-2', 'stroke-3'] };
const clip = { contextId: 'scene-root', layerUid: 'layer-a', frame: 2, records: [] };

test('layer lifecycle intents send literal closed requests; the selection travels as stable layer identities', async () => {
  const h = createHarness({ strokes, step: (r) => (r.operation === P + 'layer.delete' ? { layers: ['layer-a', 'layer-c'] }
    : r.operation === P + 'layer.duplicate' ? { result: { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:9:0' } } : { applied: true }) });
  const intent = createLayerLifecycleIntent(h.ports);
  assert.equal(intent.selectLayers({ active: 'layer-b', selected: ['layer-b'], anchor: 'layer-b' }).ok, true);
  assert.equal((await intent.deleteLayers()).ok, true);
  assert.deepEqual(h.state.sent[0], request(P + 'layer.delete', { command: 'object.layer.delete',
    selection: { active: 'layer-b', selected: ['layer-b'], anchor: 'layer-b' } }, 'layer-delete-1'));
  const dup = await intent.duplicateLayer({ layerUid: 'layer-a' });
  assert.deepEqual(h.state.sent[1], request(P + 'layer.duplicate', { command: 'object.layer.duplicate', layerUid: 'layer-a' }, 'layer-duplicate-2', 8));
  assert.deepEqual({ ok: dup.ok, applied: dup.applied, revision: dup.revision }, { ok: true, applied: true, revision: 9 });
  const rename = await intent.renameLayer({ layerUid: 'layer-c', name: 'Ink' });
  assert.deepEqual(h.state.sent[2], request(P + 'layer.rename', { command: 'object.layer.rename', layerUid: 'layer-c', name: 'Ink' }, 'layer-rename-3', 9));
  assert.equal(rename.revision, 10);
  const reorder = await intent.reorderLayers({ layerUids: ['layer-c'], gap: 0, selection: { active: 'layer-c', selected: ['layer-c'] } });
  assert.deepEqual(h.state.sent[3], request(P + 'layer.reorder', { command: 'object.layer.reorder', layerUids: ['layer-c'], gap: 0,
    selection: { active: 'layer-c', selected: ['layer-c'] } }, 'layer-reorder-4', 10));
  assert.equal(reorder.revision, 11);
});

test('a duplicate reports the identity the owner minted', async () => {
  const h = createHarness({ step: () => ({ result: { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' } }) });
  const outcome = await createLayerLifecycleIntent(h.ports).duplicateLayer({ layerUid: 'layer-b' });
  assert.deepEqual({ ok: outcome.ok, layerUid: outcome.layerUid, revision: outcome.revision }, { ok: true, layerUid: 'native-layer:8:0', revision: 8 });
  const noId = createHarness({ step: () => ({ result: { applied: true, historyEntriesAdded: 1 } }) });
  assert.equal((await createLayerLifecycleIntent(noId.ports).duplicateLayer({ layerUid: 'layer-b' })).code, 'transport_failed');
});

test('layer selection is rebuilt from the post-revision projection and stale ids are rejected before sending', async () => {
  const h = createHarness({ step: () => ({ layers: ['layer-a', 'layer-c'] }) });
  const intent = createLayerLifecycleIntent(h.ports);
  const gone = intent.selectLayers({ active: 'layer-z', selected: ['layer-b'] });
  assert.deepEqual({ code: gone.code, ids: gone.ids }, { code: 'stale_selection', ids: ['layer-z'] });
  assert.deepEqual(intent.layerSelection(), { active: null, selected: [], anchor: null });
  intent.selectLayers({ active: 'layer-b', selected: ['layer-b', 'layer-c'], anchor: 'layer-b' });
  const stale = await intent.deleteLayers({ selection: { active: 'layer-b', selected: ['layer-b', 'layer-q'] } });
  assert.deepEqual({ code: stale.code, ids: stale.ids }, { code: 'stale_selection', ids: ['layer-q'] });
  assert.equal(h.state.sent.length, 0);
  const outcome = await intent.deleteLayers();
  assert.deepEqual(h.state.sent[0].payload.selection, { active: 'layer-b', selected: ['layer-b', 'layer-c'], anchor: 'layer-b' });
  assert.deepEqual(outcome.selection, { active: 'layer-c', selected: ['layer-c'], anchor: null });
  assert.equal(outcome.selectionRebuilt, true);
  assert.deepEqual(intent.layerSelection(), { active: 'layer-c', selected: ['layer-c'], anchor: null });
  const after = intent.selectLayers({ active: 'layer-b', selected: [] });
  assert.equal(after.code, 'stale_selection', 'the deleted layer cannot be selected again');
});

test('a missing selection, a repeated layer and a non-finite gap are invalid and send nothing', async () => {
  const h = createHarness();
  const intent = createLayerLifecycleIntent(h.ports);
  assert.equal((await intent.deleteLayers()).code, 'invalid_request');
  assert.equal((await intent.reorderLayers({ layerUids: ['layer-a'], gap: 1 })).code, 'invalid_request');
  const sel = { active: 'layer-a', selected: ['layer-a'] };
  for (const input of [{ layerUids: ['layer-a', 'layer-a'], gap: 1, selection: sel }, { layerUids: [], gap: 1, selection: sel },
    { layerUids: ['layer-a'], gap: NaN, selection: sel }, { layerUids: ['layer-a'], gap: '1', selection: sel },
    { layerUids: ['layer-a'], gap: 1, selection: sel, extra: true }, { layerUids: ['layer-a'], gap: 1, selection: { active: '', selected: [] } },
    { layerUids: ['layer-a'], gap: 1, selection: { active: 'layer-a', selected: [''] } }, { layerUids: ['layer-a'], gap: 1, selection: { ...sel, anchor: '' } }]) {
    assert.equal((await intent.reorderLayers(input)).code, 'invalid_request', JSON.stringify(input));
  }
  for (const input of [undefined, null, {}, { layerUid: '' }, { layerUid: 'layer-a', extra: 1 }, ['layer-a']]) {
    assert.equal((await intent.duplicateLayer(input)).code, 'invalid_request', JSON.stringify(input));
  }
  assert.equal((await intent.renameLayer({ layerUid: 'layer-a', name: 7 })).code, 'invalid_request');
  assert.equal((await intent.duplicateLayer({ layerUid: 'layer-zz' })).code, 'stale_selection');
  assert.equal(h.state.sent.length, 0);
});

test('shape selection edits send literal closed requests; an empty selection is a legal request', async () => {
  const h = createHarness({ strokes });
  const intent = createLayerLifecycleIntent(h.ports);
  await intent.deleteShapes({ ...cell, mode: 'animation2d', strokeIds: ['stroke-1'] });
  await intent.duplicateShapes({ ...cell, mode: 'motion', strokeIds: [] });
  await intent.splitLayer({ ...cell, labels: { shape: 'Shape', stroke: 'Stroke' } });
  await intent.pasteShapes({ ...cell, mode: 'motion', clip });
  assert.deepEqual(h.state.sent, [
    request(P + 'selection.delete', { command: 'object.selection.delete', contextId: 'scene-root', layerUid: 'layer-a', frame: 2, mode: 'animation2d', strokeIds: ['stroke-1'] }, 'selection-delete-1', 7),
    request(P + 'selection.duplicate', { command: 'object.selection.duplicate', contextId: 'scene-root', layerUid: 'layer-a', frame: 2, mode: 'motion', strokeIds: [] }, 'selection-duplicate-2', 8),
    request(P + 'layer.split', { command: 'object.layer.split', contextId: 'scene-root', layerUid: 'layer-a', frame: 2, labels: { shape: 'Shape', stroke: 'Stroke' } }, 'layer-split-3', 9),
    request(P + 'clipboard.paste', { command: 'object.clipboard.paste', contextId: 'scene-root', layerUid: 'layer-a', frame: 2, mode: 'motion', clip }, 'clipboard-paste-4', 10),
  ]);
});

test('shape edit inputs are closed: unknown modes, labels, clips and members never reach the transport', async () => {
  const h = createHarness({ strokes });
  const intent = createLayerLifecycleIntent(h.ports);
  const bad = [
    ['deleteShapes', { ...cell, mode: 'drawing', strokeIds: [] }], ['deleteShapes', { ...cell, strokeIds: [] }],
    ['deleteShapes', { ...cell, mode: 'motion', strokeIds: [''] }], ['deleteShapes', { ...cell, mode: 'motion', strokeIds: [], extra: 1 }],
    ['deleteShapes', { ...cell, frame: 4294967296, mode: 'motion', strokeIds: [] }], ['duplicateShapes', { ...cell, mode: 'motion', strokeIds: null }],
    ['splitLayer', { ...cell, labels: { shape: 'Shape' } }], ['splitLayer', { ...cell, labels: { shape: '', stroke: 'Stroke' } }],
    ['splitLayer', { ...cell, labels: { shape: 'x'.repeat(65), stroke: 'Stroke' } }],
    ['pasteShapes', { ...cell, mode: 'motion', clip: { ...clip, extra: 1 } }], ['pasteShapes', { ...cell, mode: 'motion', clip: { contextId: 'scene-root' } }],
    ['pasteShapes', { ...cell, mode: 'motion', clip: { ...clip, records: [{ schemaVersion: 1 }] } }], ['pasteShapes', { ...cell, mode: 'motion', clip: 'clip' }],
    ...[{ kind: 'authored' }, { kind: 'authored', frame: 2, extra: 1 }, 'authored', null].map((frameScope) => ['pasteShapes', { ...cell, mode: 'motion',
      clip: { ...clip, records: [{ schemaVersion: 1, family: 'closed-solid-fill-cubic-path', geometry: {}, target: { contextId: 'scene-root', frameScope, layerUid: 'layer-a', strokeId: 'stroke-1' } }] } }]),
  ];
  for (const [method, input] of bad) assert.equal((await intent[method](input)).code, 'invalid_request', JSON.stringify([method, input]));
  assert.equal(h.state.sent.length, 0);
});

test('a clip over the 4096-byte envelope is refused locally, never split', async () => {
  const record = (n) => ({ schemaVersion: 1, family: 'closed-solid-fill-cubic-path', geometry: { kind: 'cubic-path', segments: new Array(40).fill({ point: { x: 1.5, y: 2.5 } }) },
    target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 2 }, layerUid: 'layer-a', strokeId: 'stroke-' + n } });
  const h = createHarness({ strokes });
  const outcome = await createLayerLifecycleIntent(h.ports).pasteShapes({ ...cell, mode: 'motion', clip: { ...clip, records: [record(1), record(2), record(3), record(4)] } });
  assert.deepEqual({ code: outcome.code, limit: outcome.limit }, { code: 'request_too_large', limit: 4096 });
  const small = await createLayerLifecycleIntent(createHarness({ strokes }).ports).pasteShapes({ ...cell, mode: 'motion', clip: { ...clip, records: [record(1)] } });
  assert.equal(small.ok, true, 'a small clip with a structurally valid record is sent');
});

test('absent declarations and sessions are typed unavailability for every operation; a declared sibling still works', async () => {
  const undeclared = createHarness({ strokes, declared: () => false });
  const intent = createLayerLifecycleIntent(undeclared.ports);
  const calls = [intent.duplicateLayer({ layerUid: 'layer-a' }), intent.renameLayer({ layerUid: 'layer-a', name: 'n' }),
    intent.deleteShapes({ ...cell, mode: 'motion', strokeIds: [] }), intent.splitLayer({ ...cell, labels: { shape: 's', stroke: 't' } }),
    intent.pasteShapes({ ...cell, mode: 'motion', clip }), intent.copyShapes({ ...cell, mode: 'motion', strokeIds: [] }), intent.cutShapes({ ...cell, mode: 'motion', strokeIds: [] })];
  for (const outcome of await Promise.all(calls)) assert.deepEqual({ code: outcome.code, reason: outcome.reason }, { code: 'unavailable', reason: 'native_declaration_absent' });
  const partial = createHarness({ strokes, declared: (op) => op === P + 'layer.rename' });
  const part = createLayerLifecycleIntent(partial.ports);
  assert.equal((await part.duplicateLayer({ layerUid: 'layer-a' })).code, 'unavailable');
  assert.equal((await part.renameLayer({ layerUid: 'layer-a', name: 'n' })).ok, true);
  const noSession = createHarness({ strokes, noSession: true });
  assert.equal((await createLayerLifecycleIntent(noSession.ports).renameLayer({ layerUid: 'layer-a', name: 'n' })).reason, 'no_presented_native_session');
  assert.equal(undeclared.state.sent.length + noSession.state.sent.length, 0);
});

test('owner refusals are typed and keep the layer selection; stale never retries', async () => {
  for (const sessionStyle of [false, true]) {
    for (const code of ['stale_revision', 'not_found', 'unavailable', 'invalid_request']) {
      const h = createHarness({ sessionStyle, step: () => ({ error: { code, message: 'refused' } }) });
      const intent = createLayerLifecycleIntent(h.ports);
      intent.selectLayers({ active: 'layer-b', selected: ['layer-b'] });
      const outcome = await intent.deleteLayers();
      assert.deepEqual({ ok: outcome.ok, code: outcome.code, stale: outcome.stale }, { ok: false, code, stale: code === 'stale_revision' });
      assert.equal(h.state.sent.length, 1);
      assert.deepEqual(intent.layerSelection(), { active: 'layer-b', selected: ['layer-b'], anchor: null });
      assert.equal(h.state.revision, 7);
    }
  }
});

test('an unsynchronized pin after a revision clears the selection instead of trusting it', async () => {
  let held = true;
  const h = createHarness({ step: () => ({ layers: ['layer-a', 'layer-c'], stayStale: held }) });
  const intent = createLayerLifecycleIntent(h.ports);
  intent.selectLayers({ active: 'layer-b', selected: ['layer-b'] });
  const outcome = await intent.deleteLayers();
  assert.equal(outcome.ok, true);
  assert.equal(outcome.synchronized, false);
  assert.equal(outcome.selectionRebuilt, false);
  assert.deepEqual(intent.layerSelection(), { active: null, selected: [], anchor: null });
  held = false;
});

test('no legacy writer is reachable from the lifecycle, selection or clipboard intents', async () => {
  const legacy = ['reorderLayersAtGap', 'duplicateLayer', 'deleteLayer', 'splitLayerIntoElements', 'splitLayerIntoElementsCore', 'copySelection', 'pasteSelection', 'cutSelection', 'deleteSelStrokes', 'renameLayer'];
  const hits = [];
  legacy.forEach((name) => { globalThis[name] = () => { hits.push(name); }; });
  try {
    const intent = createLayerLifecycleIntent(createHarness({ strokes }).ports);
    await intent.renameLayer({ layerUid: 'layer-a', name: 'x' });
    await intent.duplicateLayer({ layerUid: 'layer-a' });
    await intent.splitLayer({ ...cell, labels: { shape: 's', stroke: 't' } });
  } finally { legacy.forEach((name) => { delete globalThis[name]; }); }
  assert.deepEqual(hits, []);
  const source = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'adapters', 'native-object-layer-lifecycle-intent.js'), 'utf8');
  assert.equal(/\b(?:reorderLayersAtGap|splitLayerIntoElements|deleteSelStrokes|pasteSelection|copySelection)\s*\(/.test(source), false);
  assert.equal(/(?<![\w.])(?:Paper|paper|SM|window|document|localStorage)\./.test(source), false);
  assert.deepEqual(OPERATIONS, ['layer.delete', 'layer.duplicate', 'layer.rename', 'layer.reorder', 'selection.delete', 'selection.duplicate', 'layer.split', 'clipboard.paste', 'clipboard.copy', 'clipboard.cut'].map((s) => P + s));
});

test('an un-awaited second layer intent is judged against the projection its predecessor left, not the one at call time', async () => {
  const h = createHarness({ step: (r) => (r.operation === P + 'layer.delete' ? { layers: ['layer-b', 'layer-c'] } : { applied: true }) });
  const intent = createLayerLifecycleIntent(h.ports);
  intent.selectLayers({ active: 'layer-a', selected: ['layer-a'] });
  const [first, second] = await Promise.all([intent.deleteLayers(), intent.duplicateLayer({ layerUid: 'layer-a' })]);
  assert.equal(first.ok, true);
  assert.deepEqual({ code: second.code, ids: second.ids }, { code: 'stale_selection', ids: ['layer-a'] });
  assert.equal(h.state.sent.length, 1, 'the second request is never sent with input computed against the old projection');
  // The default selection is also read in turn: the delete left layer-b active, so a following un-awaited delete targets it.
  const [a, b] = await Promise.all([intent.renameLayer({ layerUid: 'layer-b', name: 'Ink' }), intent.deleteLayers()]);
  assert.equal(a.ok && b.ok, true);
  assert.deepEqual(h.state.sent[2].payload.selection, { active: 'layer-b', selected: [] });
  assert.equal(h.state.sent[2].expectedRevision, h.state.sent[1].expectedRevision + 1);
});


test('copy returns a detached clipboard without changing revision or rebuilding selection; cut rebuilds only its changed selection', async () => {
  const clipboard = { ...clip, records: [JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/examples/native-object-v1/cases.json'), 'utf8')).records[0]] };
  const h = createHarness({ strokes, step: (r) => r.operation === P + 'clipboard.copy'
    ? { applied: false, clipboard }
    : { applied: true, clipboard, strokes: { 'layer-a': ['stroke-2', 'stroke-3'] } } });
  const intent = createLayerLifecycleIntent(h.ports);
  intent.selectShapes(cell, ['stroke-1', 'stroke-2']);
  intent.selectLayers({ active: 'layer-a', selected: ['layer-a'] });
  const selected = intent.shapeSelection(), layerSelection = intent.layerSelection(), reads = h.state.strokeReads;
  const copied = await intent.copyShapes({ ...cell, mode: 'animation2d', strokeIds: ['stroke-1'] });
  assert.deepEqual(h.state.sent[0], request(P + 'clipboard.copy', { command: 'object.clipboard.copy', ...cell, mode: 'animation2d', strokeIds: ['stroke-1'] }, 'clipboard-copy-1', 7));
  assert.deepEqual({ ok: copied.ok, applied: copied.applied, revision: copied.revision }, { ok: true, applied: false, revision: 7 });
  assert.deepEqual(copied.clipboard, clipboard);
  assert.notEqual(copied.clipboard, clipboard); assert.notEqual(copied.clipboard.records[0], clipboard.records[0]);
  assert.equal(h.state.strokeReads, reads + 1, 'copy checks input against current ids but never rebuilds selection');
  assert.equal(Object.hasOwn(copied, 'selectionRebuilt'), false);
  assert.deepEqual(intent.shapeSelection(), selected); assert.deepEqual(intent.layerSelection(), layerSelection);
  assert.equal(h.state.revision, 7);
  copied.clipboard.records[0].target.strokeId = 'changed-only-by-consumer';
  assert.notEqual(clipboard.records[0].target.strokeId, 'changed-only-by-consumer');
  const cut = await intent.cutShapes({ ...cell, mode: 'motion', strokeIds: ['stroke-1'] });
  assert.deepEqual(h.state.sent[1], request(P + 'clipboard.cut', { command: 'object.clipboard.cut', ...cell, mode: 'motion', strokeIds: ['stroke-1'] }, 'clipboard-cut-2', 7));
  assert.deepEqual({ ok: cut.ok, applied: cut.applied, revision: cut.revision, selection: cut.selection, rejected: cut.rejected },
    { ok: true, applied: true, revision: 8, selection: ['stroke-2'], rejected: ['stroke-1'] });
  assert.equal(cut.selectionRebuilt, true); assert.deepEqual(cut.clipboard, clipboard);
  assert.deepEqual(intent.layerSelection(), layerSelection);
});

test('empty clipboard results are explicit null and copy/cut malformed or stale selections send nothing', async () => {
  for (const method of ['copyShapes', 'cutShapes']) {
    const h = createHarness({ strokes, step: () => ({ applied: false, clipboard: null }) });
    const intent = createLayerLifecycleIntent(h.ports);
    intent.selectShapes(cell, ['stroke-2']);
    const outcome = await intent[method]({ ...cell, mode: 'motion', strokeIds: [] });
    assert.deepEqual({ ok: outcome.ok, applied: outcome.applied, revision: outcome.revision, clipboard: outcome.clipboard },
      { ok: true, applied: false, revision: 7, clipboard: null });
    assert.deepEqual(intent.shapeSelection(), ['stroke-2']); assert.equal(h.state.sent.length, 1);
    const fresh = createHarness({ strokes }), other = createLayerLifecycleIntent(fresh.ports);
    for (const input of [null, {}, { ...cell, mode: 'other', strokeIds: [] }, { ...cell, mode: 'motion', strokeIds: [''] },
      { ...cell, mode: 'motion', strokeIds: [], extra: true }, { ...cell, mode: 'motion', strokeIds: null }]) {
      assert.equal((await other[method](input)).code, 'invalid_request');
    }
    assert.equal((await other[method]({ ...cell, mode: 'motion', strokeIds: ['gone'] })).code, 'stale_selection');
    assert.equal(fresh.state.sent.length, 0);
  }
});

test('clipboard refuses owner errors without retries or shape state changes, and a large selection is never split', async () => {
  for (const method of ['copyShapes', 'cutShapes']) {
    for (const code of ['stale_revision', 'wrong_document', 'unavailable', 'cancelled_before_dispatch']) {
      const h = createHarness({ strokes, step: () => ({ error: { code, message: 'refused' } }) });
      const intent = createLayerLifecycleIntent(h.ports); intent.selectShapes(cell, ['stroke-1']);
      const outcome = await intent[method]({ ...cell, mode: 'motion', strokeIds: ['stroke-1'] });
      assert.equal(outcome.ok, false); assert.equal(outcome.code, code); assert.equal(h.state.sent.length, 1);
      assert.deepEqual(intent.shapeSelection(), ['stroke-1']); assert.equal(h.state.revision, 7);
      assert.equal(Object.hasOwn(outcome, 'clipboard'), false);
    }
    const ids = Array.from({ length: 100 }, (_, i) => 'stroke-' + i + 'x'.repeat(80));
    const h = createHarness({ strokes: { 'layer-a': ids } });
    const outcome = await createLayerLifecycleIntent(h.ports)[method]({ ...cell, mode: 'motion', strokeIds: ids });
    assert.equal(outcome.code, 'request_too_large'); assert.equal(h.state.sent.length, 0);
  }
});

test('clipboard raw-port replies require closed bounded owner-correlated receipts before exposing a detached result', async () => {
  const record = JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/examples/native-object-v1/cases.json'), 'utf8')).records[0];
  const clipboard = { ...clip, records: [record] };
  for (const method of ['copyShapes', 'cutShapes']) {
    const applied = method === 'cutShapes';
    const reply = (r) => ({ apiVersion: 2, requestId: r.requestId, instanceId: r.instanceId, documentId: r.documentId,
      contentRevision: r.expectedRevision + (applied ? 1 : 0), ok: true,
      result: { applied, historyEntriesAdded: applied ? 1 : 0, clipboard: JSON.parse(JSON.stringify(clipboard)) } });
    for (const mutate of [(r) => { r.requestId = 'foreign'; }, (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; },
      (r) => { r.contentRevision += 1; }, (r) => { r.apiVersion = 1; }, (r) => { r.extra = true; },
      (r) => { delete r.result.clipboard; }, (r) => { r.result.clipboard = []; },
      (r) => { r.result.clipboard.contextId = 'foreign'; }, (r) => { r.result.clipboard.layerUid = 'foreign'; }, (r) => { r.result.clipboard.frame += 1; },
      (r) => { r.result.clipboard.records[0].target.frameScope = null; },
      (r) => { r.result.clipboard.records[0].target.strokeId = 'x'.repeat(4096); },
      (r) => { r.result.historyEntriesAdded = applied ? 0 : 1; }]) {
      const h = createHarness({ strokes, rawResponse: (q) => { const r = reply(q); mutate(r); return r; } });
      const intent = createLayerLifecycleIntent(h.ports); intent.selectShapes(cell, ['stroke-1']);
      const outcome = await intent[method]({ ...cell, mode: 'motion', strokeIds: ['stroke-1'] });
      assert.deepEqual({ ok: outcome.ok, code: outcome.code, indeterminate: outcome.indeterminate }, { ok: false, code: 'transport_failed', indeterminate: true });
      assert.deepEqual(intent.shapeSelection(), ['stroke-1']); assert.equal(h.state.submits, 1);
      assert.equal(Object.hasOwn(outcome, 'clipboard'), false);
    }
  }
});
