'use strict';

// #2348 — normal-discovery entry (tests/*.test.cjs) for the scripts/nemo suites that no other
// tests/ entry reached: the family-acceptance, inventory --check, legacy surface id, parity
// matrix and app-pin refresher regressions. Every other scripts/nemo/*.test.cjs already has
// its own tests/nemo-*.test.cjs entry; the boundaries-*.test.cjs set runs through
// tests/nemo-boundaries-suites.test.cjs. A new scripts/nemo/<x>.test.cjs must get an entry.
require('../scripts/nemo/family-acceptance.test.cjs');
require('../scripts/nemo/inventory-check.test.cjs');
require('../scripts/nemo/legacy-surface-ids.test.cjs');
require('../scripts/nemo/parity-matrix.test.cjs');
require('../scripts/nemo/refresh-app-pins.test.cjs');

// Guard: no scripts/nemo/*.test.cjs may sit outside every npm test entry again.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

test('every scripts/nemo/*.test.cjs suite is reachable from a tests/ entry', () => {
  const root = path.resolve(__dirname, '..');
  const entries = fs.readdirSync(__dirname).filter((f) => f.endsWith('.test.cjs'))
    .map((f) => fs.readFileSync(path.join(__dirname, f), 'utf8')).join('\n');
  const missing = fs.readdirSync(path.join(root, 'scripts/nemo'))
    .filter((f) => /\.test\.cjs$/.test(f) && !/^boundaries-.+\.test\.cjs$/.test(f))
    .filter((f) => !entries.includes(f));
  assert.deepEqual(missing, [], `scripts/nemo suites outside every tests/ entry: ${missing.join(', ')}`);
});
