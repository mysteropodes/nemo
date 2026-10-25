'use strict';
// Immutable stored bitmap anchors cross the real common SDK validators. The
// transport double supplies only the process boundary, never a validation bypass.
const test = require('node:test');
const assert = require('node:assert/strict');
const SDK = require('../src/js/adapters/native-application.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const clone = value => structuredClone(value);
const segment = x => ({ point: { x, y: 2 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } });
const anchor = () => ({
  schemaVersion: 1, family: 'editable-vector-path',
  target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'L', strokeId: 'brush' },
  geometry: { kind: 'cubic-path', closed: false, coordinateSpace: 'document-world', handleSpace: 'relative', segments: [segment(1), segment(9)] },
  fill: null, vector: { stroke: { kind: 'solid', r: 1, g: 0, b: 0, a: 1 }, hasRealStroke: true, strokeWidth: 2, strokeCap: 'round', strokeJoin: 'round', opacity: 0 },
  vectorExt: { bitmapBrush: { bitmapBrushSpec: { tip: 'custom', size: -2, spacing: 0, scatter: -1, opacity: 2, color: '#abc', pressure: false, seed: -4 }, brushGroupId: 'group', preTextureOpacity: 1 } },
});
const request = object => ({ apiVersion: 2, requestId: 'bitmap-read', instanceId: 'instance-A', documentId: 'document-A', operation: 'query.document.object', payload: { atRevision: 5, stableTarget: clone(object.target) } });
const reply = (query, object) => ({ apiVersion: 2, requestId: query.requestId, instanceId: query.instanceId, documentId: query.documentId, contentRevision: 5, ok: true, result: { atRevision: 5, documentSnapshotId: 'native-object:document-A:5', object } });
const read = async (object, mutate) => SDK.createNativeApplicationAdapter('bitmap-read', { dispatch: async query => {
  const response = reply(query, object); if (mutate) mutate(response); return response;
} }).dispatch(request(object));

test('stored bitmap anchors preserve optional presence and native values through the real SDK', async () => {
  for (const kind of ['authored', 'reference']) {
    const object = anchor(); object.target.frameScope.kind = kind;
    object.vectorExt.bitmapBrush.bitmapBrushSpec.tip = 'data:authored-tip';
    object.vectorExt.bitmapBrush.brushGroupId = 'blob:authored-group';
    const before = clone(object);
    assert.deepEqual((await read(object)).result.object, before);
    assert.deepEqual(object, before, 'validation does not mutate the response');
  }
  const filled = anchor(); filled.fill = filled.vector.stroke; filled.vector.stroke = null; filled.vector.hasRealStroke = false; filled.vector.opacity = 1;
  filled.vectorExt.bitmapBrush.preTextureStroke = null; delete filled.vectorExt.bitmapBrush.preTextureOpacity;
  filled.vectorExt.bitmapBrush.bitmapPressureProfile = [[-1, 0, 2]];
  filled.vectorExt.isLinkedFillCompanion = false;
  assert.deepEqual((await read(filled)).result.object, filled);
  const whitespace = anchor(); whitespace.vectorExt.bitmapBrush.bitmapBrushSpec.tip = ' '; whitespace.vectorExt.bitmapBrush.brushGroupId = ' ';
  await read(whitespace);
  const edge = anchor(); edge.vectorExt.bitmapBrush.brushGroupId = 'é'.repeat(127) + 'a'; await read(edge);
});

test('malformed stored fields, camouflage, general geometry and resource payloads are refused', async () => {
  const mutations = [
    r => { r.vectorExt.bitmapBrush = null; },
    r => { delete r.vectorExt.bitmapBrush.bitmapBrushSpec; },
    r => { delete r.vectorExt.bitmapBrush.brushGroupId; },
    r => { r.vectorExt.bitmapBrush.brushGroupId = 'é'.repeat(128); },
    r => { r.vectorExt.bitmapBrush.brushGroupId = 'nul\0id'; },
    r => { r.vectorExt.bitmapBrush.bitmapBrushSpec.tip = ''; },
    r => { r.vectorExt.bitmapBrush.bitmapBrushSpec.color = 'red'; },
    r => { r.vectorExt.bitmapBrush.bitmapBrushSpec.pressure = 1; },
    r => { r.vectorExt.bitmapBrush.bitmapBrushSpec.size = Infinity; },
    r => { r.vectorExt.bitmapBrush.preTextureOpacity = null; },
    r => { r.vectorExt.bitmapBrush.preTextureStroke = '#12'; },
    r => { r.vectorExt.bitmapBrush.bitmapPressureProfile = [[0, 1]]; },
    r => { r.vectorExt.bitmapBrush.bitmapPressureProfile = [[0, 1, NaN]]; },
    r => { r.vectorExt.bitmapBrush.bitmapPressureProfile = new Array(4097).fill([0, 1, 1]); },
    r => { delete r.vectorExt.bitmapBrush.preTextureOpacity; },
    r => { r.vector.opacity = 1; },
    r => { r.fill = r.vector.stroke; r.vectorExt.bitmapBrush.preTextureStroke = null; },
    r => { r.vectorExt.linkedFillId = 'pair'; },
    r => { r.vectorExt.isLinkedFillCompanion = true; },
    r => { r.geometry.segments.pop(); },
    r => { r.vectorExt.bitmapBrush.pixels = 'data:image/png;base64,AA=='; },
    r => { r.vectorExt.bitmapBrush.bitmapBrushSpec.pixels = 'blob:resource'; },
    r => { r.vectorExt.maskMode = 'data:resource'; },
  ];
  for (const mutate of mutations) { const object = anchor(); mutate(object); await assert.rejects(read(object), TypeError); }
});

test('reply identity, revision and byte bound stay strict; generic vector writers still refuse bitmap fields', async () => {
  for (const mutate of [r => { r.instanceId = 'other'; }, r => { r.documentId = 'other'; }, r => { r.contentRevision = 6; },
    r => { r.result.documentSnapshotId = 'wrong'; }, r => { r.result.object.target.strokeId = 'wrong'; }]) await assert.rejects(read(anchor(), mutate), TypeError);
  const large = anchor(); large.vectorExt.bitmapBrush.bitmapPressureProfile = new Array(4096).fill([0, 1, 1]);
  assert.doesNotThrow(() => Contract.readRecord(large));
  await assert.rejects(read(large), /4096/);
  for (const command of ['object.vector.create', 'object.vector.replace']) {
    const record = anchor(); if (command.endsWith('create')) record.target.strokeId = '';
    assert.throws(() => SDK.validateRequest({ apiVersion: 2, requestId: 'writer', instanceId: 'instance-A', documentId: 'document-A', operation: Contract.VECTOR, expectedRevision: 5, payload: { command, record } }), TypeError);
  }
});
