'use strict';
// #2371 U11: the layer-row writes are ONE typed request each, pinned to the revision they read, and refuse stale, unknown or
// malformed targets before anything is sent. The fake session applies the engine's small semantics so the read-back is a fact.
const test = require('node:test'), assert = require('node:assert/strict');
const Writes = require('../src/js/adapters/native-layer-row-writes.js'), F = require('./helpers/native-layer-row-fixture.cjs');

const LAYERS = [
  { layerUid: 'a', name: 'A', color: '#111111', kind: 'draw' }, { layerUid: 'b', name: 'B', color: '#222222', kind: 'draw' },
  { layerUid: 'c', name: 'C', color: '#333333', kind: 'draw' }, { layerUid: 'f', name: 'Folder', kind: 'folder', folderCollapsed: false },
];
const make = (opts) => { const s = F.session(LAYERS, opts), w = Writes.create(s, { compositing: () => ({ setFolderCollapsed: async () => ({ applied: true }) }), labels: () => ({ shape: 'Shape', stroke: 'Stroke' }) }); return { s, w }; };
const names = (s) => s.state.layers.map((l) => l.layerUid).join('');

test('rename is one layer.rename request pinned to the presented revision', async () => {
  const { s, w } = make();
  const r = await w.rename('b', 'Ink');
  assert.deepEqual([r.ok, r.applied], [true, true]);
  assert.equal(s.requests.length, 1);
  assert.deepEqual(s.requests[0].payload, { command: 'object.layer.rename', layerUid: 'b', name: 'Ink' });
  assert.equal(s.requests[0].operation, 'command.document.object.layer.rename');
  assert.equal(s.requests[0].expectedRevision, 7);
  assert.equal(s.state.layers[1].name, 'Ink');
  assert.equal(s.state.contentRevision, 8, 'one revision');
});

test('a stale or unknown uid is refused before any request is sent', async () => {
  const { s, w } = make();
  for (const call of [() => w.duplicate('ghost'), () => w.remove({ active: 'a', selected: ['a', 'ghost'] }), () => w.reorder(['ghost'], 0, { active: 'a', selected: ['a'] }),
    () => w.recolor('ghost', [], '#abcdef'), () => w.setFolderCollapsed('ghost', true), () => w.split('ghost', 0)]) {
    const r = await call();
    assert.equal(r.ok, false); assert.equal(r.code, 'stale_selection');
  }
  assert.equal(s.requests.length, 0);
});

test('the colour label is a layer.color.set naming the selection only when the clicked layer is one of several selected', async () => {
  const { s, w } = make();
  await w.recolor('a', ['a', 'c'], '#ABCDEF');
  assert.deepEqual(s.requests[0].payload, { command: 'object.layer.color.set', layerUid: 'a', color: '#abcdef', selected: ['a', 'c'] });
  assert.deepEqual(s.state.layers.map((l) => l.color).slice(0, 3), ['#abcdef', '#222222', '#abcdef']);
  await w.recolor('b', ['b'], '#010203');
  assert.equal('selected' in s.requests[1].payload, false, 'one layer is just the clicked layer');
  // alpha from the colour picker is cut to the six digits the engine stores
  await w.recolor('b', [], '#11223344');
  assert.equal(s.requests[2].payload.color, '#112233');
  for (const bad of ['#12', 'red', '', null, 7]) assert.equal((await w.recolor('b', [], bad)).code, 'invalid_request');
  assert.equal(s.requests.length, 3);
});

test('delete refuses the last layer locally and otherwise sends the selection by uid', async () => {
  const { s, w } = make();
  const r = await w.remove({ active: 'b', selected: ['b', 'c'] });
  assert.equal(r.ok, true);
  assert.deepEqual(s.requests[0].payload, { command: 'object.layer.delete', selection: { active: 'b', selected: ['b', 'c'] } });
  assert.equal(names(s), 'af');
  const one = F.session([{ layerUid: 'x', name: 'X' }]), alone = Writes.create(one, {});
  assert.equal((await alone.remove({ active: 'x', selected: ['x'] })).code, 'last_layer');
  assert.equal(one.requests.length, 0);
});

test('duplicate reports the layer it minted; reorder sends uids, the gap and the selection', async () => {
  const { s, w } = make();
  const d = await w.duplicate('a');
  assert.equal(d.layerUid, 'copy-1');
  assert.equal(s.state.layers[1].layerUid, 'copy-1');
  const r = await w.reorder(['c'], 0, { active: 'c', selected: ['c'] });
  assert.equal(r.ok, true);
  assert.deepEqual(s.requests[1].payload, { command: 'object.layer.reorder', layerUids: ['c'], gap: 0, selection: { active: 'c', selected: ['c'] } });
  assert.equal(s.state.layers[0].layerUid, 'c');
  assert.equal((await w.reorder(['a'], NaN, { active: 'a', selected: ['a'] })).code, 'invalid_request');
});

test('split names the pieces with the two locale words; folder collapse goes through the compositing edit', async () => {
  const { s, w } = make();
  assert.equal((await w.split('a', 3)).ok, true);
  assert.deepEqual(s.requests[0].payload, { command: 'object.layer.split', contextId: 'scene-root', layerUid: 'a', frame: 3, labels: { shape: 'Shape', stroke: 'Stroke' } });
  assert.equal((await w.split('a', -1)).code, 'invalid_request');
  assert.equal((await w.setFolderCollapsed('f', true)).ok, true);
  const none = Writes.create(F.session(LAYERS), {});
  assert.equal((await none.setFolderCollapsed('f', true)).code, 'unavailable', 'no compositing adapter: typed refusal, no fallback');
});

test('a refusal by the owner and an inactive project come back typed; nothing changes', async () => {
  const { s, w } = make({ refuse: 'unavailable' });
  const r = await w.rename('a', 'Z');
  assert.equal(r.ok, false); assert.equal(r.code, 'unavailable');
  assert.equal(s.state.layers[0].name, 'A'); assert.equal(s.state.contentRevision, 7);
  const idle = make(); idle.s.active = false;
  assert.equal((await idle.w.rename('a', 'Z')).ok, false);
  assert.equal(idle.s.requests.length, 0);
});

test('two writes issued together run one after the other, each against the revision the predecessor left', async () => {
  const { s, w } = make();
  const [x, y] = await Promise.all([w.rename('a', 'One'), w.rename('b', 'Two')]);
  assert.deepEqual([x.ok, y.ok], [true, true]);
  assert.deepEqual(s.requests.map((q) => q.expectedRevision), [7, 8]);
});
