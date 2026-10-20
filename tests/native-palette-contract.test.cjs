'use strict';
// Source-staged contract controls. They are not SDK/UI or installed acceptance.
const test = require('node:test');
const assert = require('node:assert/strict');
const Palette = require('../src/js/adapters/native-palette-contract.js');
const clone = value => JSON.parse(JSON.stringify(value));
function query(selector) {
  return { apiVersion: 2, requestId: 'palette-q', instanceId: 'instance-A', documentId: 'document-A',
    operation: Palette.READ, payload: { atRevision: 7, documentSnapshotId: 'snapshot-7', contextId: 'scene-root', selector } };
}
function response(request, body) {
  return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId,
    contentRevision: 7, ok: true, result: { instanceId: request.instanceId, documentId: request.documentId,
      atRevision: 7, documentSnapshotId: 'snapshot-7', contextId: 'scene-root', result: body } };
}
const settings = () => ({ palettesPresent: false, paletteCount: 0, effectiveActivePaletteIdx: 0 });

test('palette actions are closed field-specific intents with one-based safe ordinals and UTF-8 limits', () => {
  for (const value of [
    { kind: 'create', id: '', name: '', color: '#AbC' },
    { kind: 'rename', ordinal: 1, name: 'Name' },
    { kind: 'duplicate', ordinal: 1, id: 'same-id', name: 'Copy' },
    { kind: 'remove', ordinal: 2 }, { kind: 'select', ordinal: 2 },
    { kind: 'appendColor', ordinal: 1, color: '#abc' },
    { kind: 'removeColor', ordinal: 1, index: 2 },
    { kind: 'moveColor', ordinal: 1, from: 3, to: 1 }
  ]) {
    assert.deepEqual(Palette.action(value), value);
    Palette.payload(Palette.EDIT, { command: 'object.project.palette.edit', action: value });
  }
  for (const value of [
    { kind: 'replacePalettes', palettes: [] }, { kind: 'select', ordinal: 0 },
    { kind: 'select', ordinal: 1.5 }, { kind: 'select', ordinal: Number.MAX_SAFE_INTEGER + 1 },
    { kind: 'create', id: 'p', name: 'P', color: null },
    { kind: 'appendColor', ordinal: 1, color: '\0' },
    { kind: 'appendColor', ordinal: 1, color: '😀'.repeat(257) },
    { kind: 'appendColor', ordinal: 1, color: '\uD800' }
  ]) assert.throws(() => Palette.action(value), TypeError);
  assert.equal(Palette.action({ kind: 'appendColor', ordinal: 1, color: '😀'.repeat(256) }).color.length, 512);
  assert.throws(() => Palette.payload(Palette.EDIT, { command: 'object.project.settings.edit', action: { kind: 'select', ordinal: 1 } }), TypeError);
});

test('palette settings retain raw missing, negative and fractional selection without inventing stored palettes', () => {
  const request = query({ kind: 'settings' });
  for (const body of [settings(),
    { palettesPresent: false, paletteCount: 0, activePaletteIdx: 0.5, effectiveActivePaletteIdx: 0.5 },
    { palettesPresent: true, paletteCount: 0, activePaletteIdx: -1.5, effectiveActivePaletteIdx: -1.5 },
    { palettesPresent: true, paletteCount: 2, activePaletteIdx: 8, effectiveActivePaletteIdx: 0 }
  ]) Palette.correlate(request, response(request, body));
  for (const body of [
    { palettesPresent: false, paletteCount: 1, effectiveActivePaletteIdx: 0 },
    { palettesPresent: false, paletteCount: 0, activePaletteIdx: 0.5, effectiveActivePaletteIdx: 0 },
    { palettesPresent: true, paletteCount: 2, activePaletteIdx: null, effectiveActivePaletteIdx: 0 },
    { ...settings(), palettes: [] }
  ]) assert.throws(() => Palette.correlate(request, response(request, body)), TypeError);
});

test('palette and color pages preserve duplicate IDs and spellings while correlating complete forward progress', () => {
  const request = query({ kind: 'palettePage', cursor: 0, limit: 16 });
  const body = { kind: 'palettePage', palettesPresent: true, paletteCount: 3, cursor: 0,
    items: [{ ordinal: 1, id: 'p', name: 'Same', colorCount: 3 }, { ordinal: 2, id: 'p', name: 'Same', colorCount: 0 }], nextCursor: 2 };
  Palette.correlate(request, response(request, body));
  for (const change of [{ nextCursor: 1 }, { paletteCount: 1 }, { cursor: 1 }]) {
    assert.throws(() => Palette.correlate(request, response(request, { ...body, ...change })), TypeError);
  }
  const colors = query({ kind: 'colorPage', ordinal: 2, cursor: 1, limit: 16 });
  const page = { kind: 'paletteColorPage', paletteOrdinal: 2, colorCount: 3, cursor: 1,
    items: [{ index: 2, color: '#AbC' }, { index: 3, color: '#abc' }], nextCursor: null };
  Palette.correlate(colors, response(colors, page));
  assert.throws(() => Palette.correlate(colors, response(colors, { ...page, paletteOrdinal: 1 })), TypeError);
  assert.throws(() => Palette.correlate(colors, response(colors, { ...page, items: [], nextCursor: 1 })), TypeError);
  const sparse = []; sparse.length = 2;
  assert.throws(() => Palette.correlate(colors, response(colors, { ...page, items: sparse })), TypeError);
});

test('palette replies cannot cross request, owner, revision, snapshot or selector boundaries', () => {
  const request = query({ kind: 'settings' });
  const good = response(request, settings());
  for (const key of ['requestId', 'instanceId', 'documentId']) {
    const bad = clone(good); bad[key] = 'foreign';
    assert.throws(() => Palette.correlate(request, bad), TypeError);
  }
  for (const key of ['instanceId', 'documentId', 'documentSnapshotId']) {
    const bad = clone(good); bad.result[key] = 'foreign';
    assert.throws(() => Palette.correlate(request, bad), TypeError);
  }
  const future = clone(good); future.result.atRevision = 8;
  assert.throws(() => Palette.correlate(request, future), TypeError);
  const wrongKind = response(request, { kind: 'palettePage', palettesPresent: false, paletteCount: 0, cursor: 0, items: [], nextCursor: null });
  assert.throws(() => Palette.correlate(request, wrongKind), TypeError);
});

test('palette mutation receipts advance exactly once or retain the current revision for a no-op', () => {
  const request = { apiVersion: 2, requestId: 'edit', instanceId: 'instance-A', documentId: 'document-A',
    operation: Palette.EDIT, expectedRevision: 7, payload: { command: 'object.project.palette.edit', action: { kind: 'select', ordinal: 1 } } };
  const applied = { apiVersion: 2, requestId: 'edit', instanceId: 'instance-A', documentId: 'document-A',
    contentRevision: 8, ok: true, result: { applied: true, historyEntriesAdded: 1 } };
  Palette.correlate(request, applied);
  assert.throws(() => Palette.correlate(request, { ...applied, contentRevision: 9 }), TypeError);
  Palette.correlate(request, { ...applied, contentRevision: 7, result: { applied: false, historyEntriesAdded: 0 } });
  assert.throws(() => Palette.correlate(request, { ...applied, result: { applied: true, historyEntriesAdded: 0 } }), TypeError);
});

test('the palette seam delegates other operations and never reads accessors or relaxes resource bounds', () => {
  const seen = [], base = { admitted: op => op === 'existing', requiresRevision: () => true,
    requestByteLimit: () => 65536, responseByteLimit: () => 8192,
    payload: (...args) => seen.push(args), result() {}, correlate() {}, paths: () => null };
  const joined = Palette.compose(base);
  assert.equal(joined.admitted('existing'), true); assert.equal(joined.admitted(Palette.READ), true);
  assert.equal(joined.requestByteLimit('existing'), 65536); assert.equal(joined.responseByteLimit('existing'), 8192);
  assert.equal(joined.requestByteLimit(Palette.EDIT), 4096); assert.equal(joined.responseByteLimit(Palette.READ), 4096);
  joined.payload('existing', { retained: true }); assert.equal(seen.length, 1);
  const value = { kind: 'create', name: 'P' };
  Object.defineProperty(value, 'id', { enumerable: true, get() { throw new Error('must not execute'); } });
  assert.throws(() => Palette.action(value), TypeError);
  assert.equal(Palette.paths(Palette.EDIT).has('payload.action.name'), true);
  assert.equal(Palette.paths(Palette.EDIT).has('payload.rawDocument'), false);
});
