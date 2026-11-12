'use strict';
// Shadow Brush (#2361): baseline Shadow Brush stroke dictionaries <-> native records. Expected values are the REAL
// serP(desP(dict)) of src/js/app.js at fbe4b8a3 over the shipped Paper.js (native-object-vector-fixtures/
// shadow-channel-oracle.json, generated outside the repository by gen-shadow-channel-oracle.cjs, hashes inside).
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Stroke = require('../src/js/adapters/native-object-vector-stroke.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-vector-fixtures', 'shadow-channel-oracle.json'), 'utf8'));
const WHERE = { layerUid: 'L1', frame: 0 };
const pick = (object, keys) => Object.fromEntries(keys.filter((key) => key in object).map((key) => [key, object[key]]));

test('the oracle is the pinned baseline run over the Shadow Brush members serP writes', () => {
  assert.equal(oracle.commit, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  assert.deepEqual(Object.keys(oracle.slices), ['colorHex8', 'applyBrushKeyline', '_r3', 'serP', 'desP']);
  for (const hash of [oracle.generatorSha256, ...Object.values(oracle.slices)]) assert.match(hash, /^[0-9a-f]{64}$/);
  assert.deepEqual(oracle.cases.map((c) => c.id), ['shadow-stroke', 'shadow-prefer-fill', 'shadow-no-swatch', 'shadow-legacy-no-stroke']);
  for (const c of oracle.cases) assert.deepEqual(c.out2, c.out, c.id + ' serP is stable on its own output');
  // isShadowNoStroke: a stroke-less shadow path stores null, not the white fallback serP writes for any other path.
  assert.equal(oracle.cases[1].out.strokeColor, null);
});

test('each Shadow Brush dictionary maps to vectorExt.shadow and back to exactly the members serP wrote', () => {
  for (const c of oracle.cases) {
    const mapped = Stroke.toRecord(c.in, WHERE);
    assert.equal(mapped.ok, true, c.id + ' ' + JSON.stringify(mapped));
    assert.deepEqual(mapped.unmapped, [], c.id);
    assert.doesNotThrow(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.create', record: mapped.record }), c.id);
    assert.deepEqual(mapped.record.vectorExt.shadow, pick(c.out, ['channelTag', 'shadowSwatchId']), c.id);
    assert.deepEqual(Stroke.toDict(mapped.record), pick(c.out, Stroke.CARRIED), c.id + ' inverse');
    assert.deepEqual(Stroke.toRecord(Stroke.toDict(mapped.record), WHERE).record, mapped.record, c.id + ' fixed point');
  }
});

test('channel members serP can never write refuse the mapping and the contract', () => {
  const base = oracle.cases[0].in;
  for (const change of [{ channelTag: 'fill' }, { channelTag: 7 }, { channelTag: undefined, shadowSwatchId: 'sh1' }, { shadowSwatchId: 'x'.repeat(256) }]) {
    const dict = Object.assign({}, base, change);
    if (change.channelTag === undefined && 'channelTag' in change) delete dict.channelTag;
    const result = Stroke.toRecord(dict, WHERE);
    assert.equal(result.ok, false, JSON.stringify(change)); assert.equal(result.code, 'invalid_dict', JSON.stringify(change));
  }
  const record = Stroke.toRecord(base, WHERE).record;
  for (const shadow of [{ channelTag: 'fill' }, { shadowSwatchId: 'sh1' }, { channelTag: 'shadow', shadowSwatchId: '' }, { channelTag: 'shadow', extra: 1 }]) {
    const bad = JSON.parse(JSON.stringify(record)); bad.vectorExt.shadow = shadow;
    assert.throws(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.create', record: bad }), JSON.stringify(shadow));
  }
});
