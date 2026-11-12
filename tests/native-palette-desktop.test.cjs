'use strict';
// Real application/contract modules with an injected transport fixture; not a native provider receipt.
const test = require('node:test');
const assert = require('node:assert/strict');
const Model = require('../src/js/adapters/native-palette-desktop.js');
const Contract = require('../src/js/adapters/native-palette-contract.js');
const clone = value => JSON.parse(JSON.stringify(value));
function fixture(rawActive = 0) {
  let info = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1,
    contentRevision: 0, documentSnapshotId: 'snapshot-0' };
  let queueBusy = false, revision = 0, ticket = 1, active = true, available = true, raw = rawActive, serial = 0, queued = Promise.resolve(), block = null;
  const palettes = [{ id: 'same', name: 'First', colors: ['#AbC', '#abc', ...Array.from({ length: 16 }, (_, i) => '#color-' + i)] },
    { id: 'same', name: 'Second', colors: ['#def'] }];
  const initial = clone(palettes), calls = [], published = [], reconciles = [];
  const shown = () => ({ ...info, presentation: { ...info, owner: 'native', status: 'presented' } });
  function body(selector) {
    if (selector.kind === 'settings') return { palettesPresent: true, paletteCount: palettes.length,
      activePaletteIdx: raw, effectiveActivePaletteIdx: raw < palettes.length ? raw : 0 };
    const cursor = selector.cursor;
    if (selector.kind === 'palettePage') return { kind: 'palettePage', palettesPresent: true, paletteCount: palettes.length,
      cursor, items: palettes.slice(cursor, cursor + 4).map((p, i) => ({ ordinal: cursor + i + 1, id: p.id, name: p.name, colorCount: p.colors.length })),
      nextCursor: cursor + 4 < palettes.length ? cursor + 4 : null };
    const colors = palettes[selector.ordinal - 1].colors;
    return { kind: 'paletteColorPage', paletteOrdinal: selector.ordinal, colorCount: colors.length, cursor,
      items: colors.slice(cursor, cursor + 4).map((color, i) => ({ index: cursor + i + 1, color })),
      nextCursor: cursor + 4 < colors.length ? cursor + 4 : null };
  }
  async function dispatch(request) {
    calls.push(clone(request)); Contract.payload(request.operation, request.payload);
    if (block) { const held = block; block = null; held.entered(); await held.wait; }
    const reply = { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId,
      documentId: info.documentId, contentRevision: revision, ok: true };
    if (request.operation === Contract.EDIT) {
      assert.equal(request.expectedRevision, revision);
      assert.equal(request.payload.action.kind, 'select');
      const next = request.payload.action.ordinal - 1, applied = next !== raw;
      raw = next; if (applied) revision++;
      reply.contentRevision = revision;
      reply.result = { applied, historyEntriesAdded: Number(applied) };
    } else {
      assert.equal(request.payload.atRevision, revision);
      assert.equal(request.payload.documentSnapshotId, 'snapshot-' + revision);
      reply.result = { instanceId: info.instanceId, documentId: info.documentId, atRevision: revision,
        documentSnapshotId: 'snapshot-' + revision, contextId: 'scene-root', result: body(request.payload.selector) };
    }
    return reply;
  }
  const session = { info: () => info, isActive: () => active, requestId: () => 'palette-' + (++serial),
    readSubscriptionGeneration: () => ticket, readPresentedObjectSession: why => { if (queueBusy) { if (why) why('queue-pending'); return null; } return shown(); },
    runResourceCommand(action) {
      const work = queued.then(() => action({ dispatch, readPresentedObjectSession: shown,
        async reconcile(next) { reconciles.push(next); assert.equal(next, revision);
          info = { ...info, contentRevision: revision, documentSnapshotId: 'snapshot-' + revision }; return info; } }));
      queued = work.catch(() => {}); return work;
    } };
  const model = Model.create({ currentSession: () => session, available: () => available,
    publish(value) { published.push(value); return true; } });
  return { model, calls, published, busy(value) { queueBusy = value; }, reconciles, palettes, initial,
    replace() { info = { ...info, documentId: 'replacement', lifecycleGeneration: 2 }; },
    resubscribe() { ticket++; }, hide() { available = false; }, retire() { active = false; model.dispose(); },
    hold() { let entered, release; const started = new Promise(resolve => entered = resolve), wait = new Promise(resolve => release = resolve);
      block = { entered, wait }; return { started, release }; } };
}

test('palette presentation stages complete pinned summaries and active colors without deduplicating or writing metadata', async () => {
  const h = fixture(), pending = h.model.refresh();
  assert.equal(h.model.read(), null);
  const projection = await pending;
  assert.strictEqual(h.model.read(), projection);
  assert.equal(h.published.length, 1);
  assert.deepEqual(projection.palette.palettes.map(p => p.id), ['same', 'same']);
  assert.deepEqual(projection.palette.active.colors.map(p => p.color), h.palettes[0].colors);
  assert.equal(h.calls.filter(q => q.payload.selector && q.payload.selector.kind === 'colorPage').length, 5);
  assert.equal(h.calls.filter(q => q.operation === Contract.EDIT).length, 0);
  assert.deepEqual(h.palettes, h.initial);
  assert.equal(Object.isFrozen(projection.palette.active.colors), true);
});

test('raw fractional palette selection remains visible without silently selecting or loading another palette', async () => {
  const h = fixture(0.5), projection = await h.model.refresh();
  assert.equal(projection.palette.settings.activePaletteIdx, 0.5);
  assert.equal(projection.palette.settings.effectiveActivePaletteIdx, 0.5);
  assert.equal(projection.palette.active, null);
  assert.equal(h.calls.some(q => q.payload.selector && q.payload.selector.kind === 'colorPage'), false);
  assert.deepEqual(h.palettes, h.initial);
});

test('palette edits reconcile the real session pin before staged readback and reject a retained projection', async () => {
  const h = fixture(), before = await h.model.refresh();
  const result = await h.model.edit({ kind: 'select', ordinal: 2 }, before);
  assert.equal(result.status, 'applied-presented');
  assert.deepEqual(h.reconciles, [1]);
  assert.equal(result.projection.info.contentRevision, 1);
  assert.equal(result.projection.palette.active.summary.ordinal, 2);
  assert.deepEqual(result.projection.palette.active.colors.map(p => p.color), ['#def']);
  const count = h.calls.length;
  await assert.rejects(h.model.edit({ kind: 'select', ordinal: 1 }, before), e => e.code === 'stale_revision');
  assert.equal(h.calls.length, count);
  h.replace(); assert.equal(h.model.read(), null);
  await assert.rejects(h.model.edit({ kind: 'select', ordinal: 1 }, result.projection), e => e.code === 'stale_revision');
});

test('same-pin subscription replacement and retirement cannot publish or revive a previous palette projection', async () => {
  const h = fixture(), before = await h.model.refresh();
  h.resubscribe(); assert.equal(h.model.read(), null);
  const next = await h.model.refresh();
  assert.deepEqual(next.info, before.info); assert.notStrictEqual(next, before);
  await assert.rejects(h.model.edit({ kind: 'select', ordinal: 2 }, before), e => e.code === 'stale_revision');
  const held = h.hold(), reading = h.model.refresh(); await held.started;
  const count = h.published.length; h.retire(); held.release();
  await assert.rejects(reading, e => e.code === 'stale_revision');
  assert.equal(h.published.length, count); assert.equal(h.model.read(), null);
});

test('a palette no-op keeps the native pin and still retires the old gesture projection', async () => {
  const h = fixture(), before = await h.model.refresh();
  const result = await h.model.edit({ kind: 'select', ordinal: 1 }, before);
  assert.equal(result.status, 'unchanged'); assert.equal(result.applied, false);
  assert.deepEqual(result.projection.info, before.info);
  assert.notStrictEqual(result.projection, before);
  assert.deepEqual(h.reconciles, [0]);
  await assert.rejects(h.model.edit({ kind: 'select', ordinal: 2 }, before), e => e.code === 'stale_revision');
  assert.deepEqual(h.palettes, h.initial);
});

// #2372 presentation-busy-audit: the native queue hides the presentation while ANY command is pending (queue-pending).
test('a palette edit made while the queue hides the presentation keeps the sealed cache, waits for it and applies once', async () => {
  globalThis.NemoNativePresentationWait = require('../src/js/adapters/native-presentation-wait.js');
  try {
    const h = fixture(), before = await h.model.refresh();
    h.busy(true);
    assert.equal(h.model.read(), null, 'read() authorizes input, so it stays null while the presentation is hidden');
    const edit = h.model.edit({ kind: 'select', ordinal: 2 }, before);
    await new Promise(resolve => setTimeout(resolve, 150));
    assert.equal(h.calls.filter(q => q.operation === Contract.EDIT).length, 0, 'nothing is sent while the presentation is hidden');
    h.busy(false);
    const result = await edit;
    assert.equal(result.status, 'applied-presented');
    assert.equal(h.calls.filter(q => q.operation === Contract.EDIT).length, 1, 'the edit went out exactly once');
    assert.equal(result.projection.palette.active.summary.ordinal, 2);
  } finally { delete globalThis.NemoNativePresentationWait; }
});

test('a palette edit whose document was replaced during the wait still refuses as stale and sends nothing', async () => {
  globalThis.NemoNativePresentationWait = require('../src/js/adapters/native-presentation-wait.js');
  try {
    const h = fixture(), before = await h.model.refresh();
    h.busy(true);
    const edit = h.model.edit({ kind: 'select', ordinal: 2 }, before);
    const refused = assert.rejects(edit, e => e.code === 'stale_revision');
    await new Promise(resolve => setTimeout(resolve, 80));
    h.replace(); h.busy(false);
    await refused;
    assert.equal(h.calls.filter(q => q.operation === Contract.EDIT).length, 0);
  } finally { delete globalThis.NemoNativePresentationWait; }
});
