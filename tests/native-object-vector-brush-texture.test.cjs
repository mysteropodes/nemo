'use strict';
// G16 (#2361): baseline brush-texture stroke dictionaries <-> native records. Expected values are the REAL
// serP(desP(dict)) of src/js/app.js at fbe4b8a3 over the shipped Paper.js (native-object-vector-fixtures/
// brush-texture-oracle.json, generated outside the repository by gen-brush-texture-oracle.cjs, hashes inside).
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Stroke = require('../src/js/adapters/native-object-vector-stroke.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-vector-fixtures', 'brush-texture-oracle.json'), 'utf8'));
const WHERE = { layerUid: 'L1', frame: 0 };
const pick = (object, keys) => Object.fromEntries(keys.filter((key) => key in object).map((key) => [key, object[key]]));
const TAGS = ['brushGroupId', 'isBrushTextureCopy', 'brushTexturePreset', 'preTextureOpacity', 'preTextureStroke'];

test('the oracle is the pinned baseline run over every brush-texture shape serP writes', () => {
  assert.equal(oracle.commit, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  assert.deepEqual(Object.keys(oracle.slices), ['colorHex8', 'applyBrushKeyline', '_r3', 'serP', 'desP']);
  for (const hash of [oracle.generatorSha256, ...Object.values(oracle.slices)]) assert.match(hash, /^[0-9a-f]{64}$/);
  assert.deepEqual(oracle.cases.map((c) => c.id), ['anchor-no-fill', 'anchor-fill', 'anchor-fill-null-stroke', 'anchor-opacity-zero', 'dab', 'dab-falsy-copy', 'group-only']);
  for (const c of oracle.cases) assert.deepEqual(c.out2, c.out, c.id + ' serP is stable on its own output');
});

test('each serialized dictionary maps to an admitted record and back to exactly the members serP wrote', () => {
  for (const c of oracle.cases) {
    const mapped = Stroke.toRecord(c.in, WHERE);
    assert.equal(mapped.ok, true, c.id + ' ' + JSON.stringify(mapped));
    assert.deepEqual(mapped.unmapped, [], c.id);
    assert.doesNotThrow(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.create', record: mapped.record }), c.id);
    const tags = pick(c.out, TAGS);
    if (Object.keys(tags).length) assert.deepEqual(mapped.record.vectorExt.brushTexture, tags, c.id);
    else assert.equal(mapped.record.vectorExt, undefined, c.id + ' falsy members are the baseline absent');
    assert.deepEqual(Stroke.toDict(mapped.record), pick(c.out, Stroke.CARRIED), c.id + ' inverse');
    assert.deepEqual(Stroke.toRecord(Stroke.toDict(mapped.record), WHERE).record, mapped.record, c.id + ' fixed point');
  }
});

test('members serP can never write refuse the mapping and the contract', () => {
  const base = oracle.cases.find((c) => c.id === 'dab').in;
  for (const [key, value] of [['brushGroupId', 7], ['brushTexturePreset', ['p']], ['preTextureOpacity', '1'], ['preTextureOpacity', Infinity],
    ['preTextureStroke', 'red'], ['brushGroupId', 'x'.repeat(256)]]) {
    const result = Stroke.toRecord(Object.assign({}, base, { [key]: value }), WHERE);
    assert.equal(result.ok, false, key + ' ' + String(value)); assert.equal(result.code, 'invalid_dict', key);
  }
  assert.equal(Stroke.toRecord(Object.assign({}, base, { brushTexturePreset: 'p' }), WHERE).code, 'invalid_dict', 'a dab is not an anchor');
  // G16 option C: an anchor's stamping seed maps both ways; a seed on a dab, or out of uint32, refuses.
  const anchor = oracle.cases.find((c) => c.in.brushTexturePreset).in;
  const seeded = Stroke.toRecord(Object.assign({}, anchor, { strokeSeed: 4294967295 }), WHERE);
  assert.equal(seeded.ok, true, JSON.stringify(seeded));
  assert.equal(seeded.record.vectorExt.brushTexture.strokeSeed, 4294967295);
  assert.equal(Stroke.toDict(seeded.record).strokeSeed, 4294967295);
  Contract.payload(Contract.VECTOR, { command: 'object.vector.create', record: seeded.record });
  for (const [dict, why] of [[Object.assign({}, base, { strokeSeed: 1 }), 'dab'], [Object.assign({}, anchor, { strokeSeed: -1 }), 'negative'],
    [Object.assign({}, anchor, { strokeSeed: 2 ** 32 }), 'too large'], [Object.assign({}, anchor, { strokeSeed: 1.5 }), 'fraction']]) {
    assert.equal(Stroke.toRecord(dict, WHERE).code, 'invalid_dict', why);
  }
  const unseeded = JSON.parse(JSON.stringify(seeded.record)); delete unseeded.vectorExt.brushTexture.brushTexturePreset;
  assert.throws(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.create', record: unseeded }), TypeError, 'a seed without a preset');
  const record = Stroke.toRecord(base, WHERE).record;
  for (const tags of [{}, { isBrushTextureCopy: false }, { brushGroupId: '' }, { brushGroupId: 'g', extra: 1 }]) {
    const bad = JSON.parse(JSON.stringify(record)); bad.vectorExt.brushTexture = tags;
    assert.throws(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.create', record: bad }), TypeError, JSON.stringify(tags));
  }
});
