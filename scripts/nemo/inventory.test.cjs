'use strict';
// Regressions for the R03 surface-inventory generator (scripts/nemo/inventory.cjs).
// Included in `npm test` / `verify` through tests/nemo-inventory.test.cjs.
// Run directly:  node --test scripts/nemo/inventory.test.cjs
//
// The generator is static analysis over the shipped source, so most checks run
// it once against the real src/ and assert properties of the result; the
// staleness check is exercised against a scratch directory, never against the
// committed outputs.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const inventory = require('./inventory.cjs');
const ROOT = path.resolve(__dirname, '..', '..');

let built = null;
function inv() { if (!built) built = inventory.build(); return built; }
function rows() { return inv().rows; }
function byId(id) { const r = rows().find((x) => x.id === id); assert.ok(r, 'row ' + id + ' exists'); return r; }

const REQUIRED = ['id', 'kind', 'area', 'surface', 'capability', 'handler', 'events', 'exposure', 'mcp', 'sdk', 'consumers', 'platforms', 'status', 'nextGate', 'source'];

test('every row carries the full schema and ids are unique', () => {
  const d = inv();
  assert.equal(d.schema, 'nemo.inventory/1');
  assert.match(d.inputsDigest, /^[0-9a-f]{64}$/);
  assert.ok(d.rows.length > 500, 'expected hundreds of rows, got ' + d.rows.length);
  const seen = new Set();
  for (const r of d.rows) {
    for (const k of REQUIRED) assert.ok(k in r, r.id + ' lacks ' + k);
    assert.ok(Array.isArray(r.handler) && Array.isArray(r.events) && Array.isArray(r.exposure), r.id);
    assert.ok(d.states.includes(r.status), r.id + ' has unknown status ' + r.status);
    assert.ok(!seen.has(r.id), 'duplicate row id ' + r.id);
    seen.add(r.id);
  }
  for (const k of ['dom', 'shortcut', 'menu', 'labs', 'sdk']) assert.ok(d.counts.byKind[k] > 0, 'no rows of kind ' + k);
});

test('a row without a handler is explicit: unmapped or unavailable-with-reason, never inventoried', () => {
  for (const r of rows()) {
    if (r.status === 'inventoried') assert.ok(r.handler.length > 0, r.id + ' is inventoried without a handler');
    if (r.status === 'unmapped') assert.equal(r.handler.length, 0, r.id + ' is unmapped but has a handler');
    if (r.status === 'unavailable-with-reason') assert.ok(r.reason && r.reason.length > 10, r.id + ' lacks a reason');
  }
});

test('consumer reachability records the depth at which each consumer is reached', () => {
  let withConsumers = 0;
  for (const r of rows()) {
    for (const [name, depth] of Object.entries(r.consumers)) {
      assert.ok(inv().consumers.includes(name), r.id + ' names unknown consumer ' + name);
      assert.ok(Number.isInteger(depth) && depth >= 1 && depth <= 3, r.id + ' consumer depth ' + depth);
    }
    if (Object.keys(r.consumers).length) withConsumers++;
  }
  assert.ok(withConsumers > rows().length / 2, 'most surfaces reach at least one document consumer');
});

// --- binding patterns the static pass must see (each was an unmapped row before) ---

test('ids looked up through a table (getElementById(IDS[key])) are bound', () => {
  for (const id of ['btn-combine-unite', 'btn-combine-subtract', 'btn-combine-intersect', 'btn-combine-exclude', 'btn-rig-mode-draw', 'btn-rig-mode-assign', 'btn-rig-mode-move']) {
    const r = byId('dom:#' + id);
    assert.equal(r.status, 'inventoried', id);
    assert.ok(r.handler.some((h) => h.startsWith('src/js/timeline.js:')), id + ' handler ' + r.handler);
    assert.ok(r.events.includes('click'), id + ' events ' + r.events);
  }
});

test('ids iterated from an array literal (["a","b"].forEach(id => getElementById(id))) are bound', () => {
  const r = byId('dom:#btn-open-tutorial-topbar');
  assert.equal(r.status, 'inventoried');
  assert.ok(r.handler.some((h) => h.startsWith('src/js/tutorial.js:')), r.handler);
  assert.ok(r.events.includes('click'));
});

test('compound descendant selectors (#align-toolbar .align-btn[data-align]) bind their buttons', () => {
  const btns = rows().filter((r) => r.kind === 'dom' && r.meta.container === 'align-toolbar' && r.surface.endsWith('button'));
  assert.equal(btns.length, 8, 'six align + two distribute buttons');
  for (const r of btns) {
    assert.equal(r.status, 'inventoried', r.id);
    assert.ok(r.handler.some((h) => h.startsWith('src/js/timeline.js:')), r.id);
    assert.ok(r.events.includes('click'), r.id);
  }
  const distribute = btns.filter((r) => r.meta.data.distribute);
  assert.equal(distribute.length, 2);
});

test('markup a container rebuilds at runtime is a placeholder, not an unmapped control', () => {
  const tabs = rows().filter((r) => r.kind === 'dom' && r.meta.container === 'symbol-tabs');
  assert.ok(tabs.length >= 1);
  for (const r of tabs) {
    assert.equal(r.status, 'unavailable-with-reason', r.id);
    assert.match(r.reason, /rebuilt at runtime by src\/js\/timeline\.js:\d+ renderSymbolTabs\(\)/);
    assert.equal(r.handler.length, 1);
    assert.equal(r.meta.via, 'container rebuild #symbol-tabs');
  }
});

test('the shipped source has no unmapped surface left; any regression names the row', () => {
  const unmapped = rows().filter((r) => r.status === 'unmapped').map((r) => r.id + ' (' + r.source + ')');
  assert.deepEqual(unmapped, []);
});

// --- outputs and the staleness gate ---

test('rendered outputs list unmapped rows and placeholders explicitly', () => {
  const out = inventory.renderOutputs(inv());
  assert.deepEqual(Object.keys(out).sort(), ['SURFACES.md', 'surfaces.csv', 'surfaces.json']);
  const n = inv().counts.byStatus.unmapped || 0;
  const p = inv().counts.byStatus['unavailable-with-reason'] || 0;
  assert.ok(out['SURFACES.md'].includes('## Unmapped surfaces (' + n + ')'));
  assert.ok(out['SURFACES.md'].includes('## Static placeholders (' + p + ')'));
  const header = out['surfaces.csv'].split('\n')[0];
  assert.equal(header, 'id,kind,area,surface,capability,handler,events,exposure,sdk,mcp,consumers,fixtures,platforms,status,nextGate,source,reason');
  assert.equal(out['surfaces.csv'].trim().split('\n').length, rows().length + 1);
  assert.equal(JSON.parse(out['surfaces.json']).rows.length, rows().length);
});

test('staleOutputs ignores the source stamp but reports missing and modified files', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-inventory-test-'));
  try {
    const out = inventory.renderOutputs(inv());
    for (const [name, content] of Object.entries(out)) fs.writeFileSync(path.join(dir, name), content);
    assert.deepEqual(inventory.staleOutputs(out, dir), []);
    // a different commit stamp alone is not staleness
    const restamped = {
      'surfaces.json': out['surfaces.json'].replace(/"head": "[0-9a-f]*"/, '"head": "0000000000000000000000000000000000000000"').replace(/"describe": "[^"]*"/, '"describe": "v0.0.0-0-g0000000"'),
      'surfaces.csv': out['surfaces.csv'],
      'SURFACES.md': out['SURFACES.md'].replace(/source `[0-9a-f]*` \([^)]*\)/, 'source `000000000000` (v0.0.0-0-g0000000)'),
    };
    assert.deepEqual(inventory.staleOutputs(restamped, dir), []);
    fs.appendFileSync(path.join(dir, 'surfaces.csv'), '"extra"\n');
    fs.rmSync(path.join(dir, 'SURFACES.md'));
    assert.deepEqual(inventory.staleOutputs(out, dir), ['surfaces.csv', 'SURFACES.md (missing)']);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('the committed inventory is up to date (`inventory.cjs --check`)', () => {
  const r = spawnSync(process.execPath, [path.join(__dirname, 'inventory.cjs'), '--check'], { cwd: ROOT, encoding: 'utf8' });
  assert.equal(r.status, 0, 'stale inventory: ' + r.stderr + ' — run `npm run inventory` and commit engineering/inventory');
  assert.match(r.stdout, /^inventory up to date \(\d+ rows, digest [0-9a-f]{12}\)/);
});
