'use strict';
// CS3/#2342 — the pin refresher produces exactly the pins the application
// policy gate verifies, and reports drift instead of hiding it.
const assert = require('node:assert/strict');
const path = require('node:path');
const test = require('node:test');
const { refreshedPolicy, driftedPins } = require('./refresh-app-pins.cjs');

const ROOT = path.resolve(__dirname, '..', '..');

test('refreshing at HEAD leaves the committed pins unchanged (they are current)', () => {
  const { raw, text } = refreshedPolicy(ROOT, 'HEAD');
  assert.deepEqual(driftedPins(raw, text), []);
});

test('a stale pin is reported by name and repaired by the refresh', () => {
  const { raw, text } = refreshedPolicy(ROOT, 'HEAD');
  const stale = JSON.parse(raw);
  stale.provenance.inventoryDigest.value = '0'.repeat(64);
  stale.provenance.bootstrap.gitBlob = 'f'.repeat(40);
  const drift = driftedPins(JSON.stringify(stale), text);
  assert.deepEqual(drift.sort(), ['bootstrap.gitBlob', 'inventoryDigest.value']);
});

test('only derived pins move; policy, membership and exclusions are untouched', () => {
  const { raw, text } = refreshedPolicy(ROOT, 'HEAD');
  const before = JSON.parse(raw), after = JSON.parse(text);
  for (const key of Object.keys(before)) if (key !== 'provenance') assert.deepEqual(after[key], before[key], key);
});
