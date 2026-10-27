'use strict';
// Fill Brush shapes: baseline dictionaries <-> native records. Expected values are the REAL serP(desP(dict)) of
// src/js/app.js at fbe4b8a3 over the shipped Paper.js (native-object-vector-fixtures/fill-shape-oracle.json,
// generated outside the repository by gen-fill-shape-oracle.cjs, hashes inside).
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Stroke = require('../src/js/adapters/native-object-vector-stroke.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-vector-fixtures', 'fill-shape-oracle.json'), 'utf8'));
const WHERE = { layerUid: 'L1', frame: 0 };
const pick = (object, keys) => Object.fromEntries(keys.filter((key) => key in object).map((key) => [key, object[key]]));

// Observed baseline defect (CLAUDE.md section 1 family): serP writes isFillShape, but desP never restores
// data.isFillShape, so serP(desP(dict)) drops it and a reopened baseline project forgets which fills the Fill Brush
// made. Native maps the stored document, which carries the flag, and keeps it.
test('the oracle is the pinned baseline run, and the baseline reload drops the Fill Brush flag', () => {
  assert.equal(oracle.commit, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  assert.deepEqual(oracle.cases.map((c) => c.id), ['fill-shape', 'fill-shape-shadow']);
  for (const c of oracle.cases) {
    assert.equal(c.in.isFillShape, true, c.id); assert.equal(c.out.isFillShape, undefined, c.id + ' desP does not restore it');
    assert.deepEqual(c.out2, c.out, c.id + ' serP is stable');
  }
  // No exemption for a Fill Brush shape: its missing stroke is stored as the white fallback beside hasRealStroke:false.
  assert.deepEqual([oracle.cases[0].out.strokeColor, oracle.cases[0].out.hasRealStroke], ['#ffffff', false]);
});

test('each stored Fill Brush dictionary keeps its flag in vectorExt.isFillShape and maps back with every other member serP wrote', () => {
  for (const c of oracle.cases) {
    const mapped = Stroke.toRecord(c.in, WHERE);
    assert.equal(mapped.ok, true, c.id + ' ' + JSON.stringify(mapped));
    assert.deepEqual(mapped.unmapped, [], c.id);
    assert.doesNotThrow(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.create', record: mapped.record }), c.id);
    assert.equal(mapped.record.vectorExt.isFillShape, true, c.id);
    assert.deepEqual(Stroke.toDict(mapped.record), Object.assign(pick(c.out, Stroke.CARRIED), { isFillShape: true }), c.id + ' inverse');
    assert.deepEqual(Stroke.toRecord(Stroke.toDict(mapped.record), WHERE).record, mapped.record, c.id + ' fixed point');
  }
  const record = Stroke.toRecord(oracle.cases[0].in, WHERE).record;
  for (const flag of [false, 'true', 1]) {
    const bad = JSON.parse(JSON.stringify(record)); bad.vectorExt.isFillShape = flag;
    assert.throws(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.create', record: bad }), String(flag));
  }
});
