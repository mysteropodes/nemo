'use strict';
// CS3/#2342 — `inventory.cjs --check` staleness semantics: line positions and the
// inputs digest are advisory; any change to a row's content is stale.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const inventory = require('./inventory.cjs');

let built;
const inv = () => (built = built || inventory.build());

// CS3/#2342: shifting lines (and so every `file:line` position and the inputs
// digest) is not staleness; changing what a row says is.
test('staleOutputs treats shifted line positions and the digest as advisory', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-inventory-lines-'));
  try {
    const out = inventory.renderOutputs(inv());
    for (const [name, content] of Object.entries(out)) fs.writeFileSync(path.join(dir, name), content);
    const shift = (t) => t.replace(/((?:src|tests)\/[\w./-]+\.(?:html|js|cjs|mjs|json)):(\d+)/g, (_, f, n) => `${f}:${Number(n) + 7}`)
      .replace(/"inputsDigest": "[0-9a-f]*"/, `"inputsDigest": "${'f'.repeat(64)}"`).replace(/Inputs digest `[0-9a-f]*`/, 'Inputs digest `ffffffffffff`');
    const shifted = Object.fromEntries(Object.entries(out).map(([k, v]) => [k, shift(v)]));
    assert.notEqual(shifted['surfaces.json'], out['surfaces.json'], 'the fixture really moves positions');
    assert.deepEqual(inventory.staleOutputs(shifted, dir), []);
    const renamed = { ...shifted, 'surfaces.json': shifted['surfaces.json'].replace('"capability": "', '"capability": "Renamed ') };
    assert.deepEqual(inventory.staleOutputs(renamed, dir), ['surfaces.json'], 'a content change is still stale');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
