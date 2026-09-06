'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');
const { build } = require('../scripts/nemo/inventory.cjs');
const { lexRegions, matchClose } = require('../scripts/nemo/lib/inventory-lexer.cjs');

const root = path.resolve(__dirname, '..');
const read = relative => fs.readFileSync(path.join(root, relative), 'utf8');
const helper = read('src/js/color-picker.js');
const timeline = read('src/js/timeline.js');
const html = read('src/index.html');

// Extract a complete production call by balanced parentheses, not line numbers.
// Keep its literal records and callbacks unchanged. Runtime execution below also
// verifies that this extracted call actually registers the target listener.
function productionCall() {
  const indexed = lexRegions(timeline);
  const calls = [];
  const pattern = /window\.ColorPicker\.wireColorSwatches\s*\(/g;
  for (const match of indexed.code.matchAll(pattern)) {
    if (indexed.kind[match.index] !== 0) continue;
    const open = indexed.code.indexOf('(', match.index);
    const call = timeline.slice(match.index, matchClose(indexed, open) + 1);
    if (/\bwrap\s*:\s*['"]pm-stroke['"]/.test(call)) calls.push(call + ';');
  }
  assert.equal(calls.length, 1, 'expected one production pm-stroke swatch call');
  return calls[0];
}

function runtime(moduleSource, callSource, ids = ['pm-stroke', 'pm-stroke-c']) {
  const listeners = [];
  const elements = new Map(ids.map(id => [id, {
    id, style: {}, dataset: {}, value: '#000000',
    addEventListener(event, listener) {
      assert.equal(typeof listener, 'function');
      listeners.push({ id, event });
    },
  }]));
  const context = vm.createContext({
    window: {},
    document: { getElementById: id => elements.get(id) || null },
  });
  vm.runInContext(moduleSource, context, { filename: 'src/js/color-picker.js' });
  vm.runInContext(callSource, context, { filename: 'swatch-call.js' });
  return { elements, events: id => listeners.filter(l => l.id === id).map(l => l.event) };
}

function rowFor(inventory, id) {
  const row = inventory.rows.find(row => row.id === 'dom:#' + id);
  assert.ok(row, 'inventory must contain ' + id);
  return { events: row.events, status: row.status, handler: row.handler };
}

function fixture(t, moduleSource, callSource, fixtureHtml = html) {
  const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-inventory-swatch-'));
  t.after(() => fs.rmSync(temporaryRoot, { recursive: true, force: true }));
  fs.mkdirSync(path.join(temporaryRoot, 'src/js'), { recursive: true });
  fs.writeFileSync(path.join(temporaryRoot, 'src/index.html'), fixtureHtml);
  fs.writeFileSync(path.join(temporaryRoot, 'src/js/color-picker.js'), moduleSource);
  fs.writeFileSync(path.join(temporaryRoot, 'src/js/swatch-call.js'), callSource);
  return build({ root: temporaryRoot });
}

test('swatch inventory: production helper registers pm-stroke click at runtime', () => {
  const observed = runtime(helper, productionCall());
  assert.deepEqual(observed.events('pm-stroke'), ['click']);
  assert.deepEqual(observed.events('pm-stroke-c'), [], 'paired input is not the click owner');
  assert.equal(observed.elements.get('pm-stroke-c').style.pointerEvents, 'none');
});

// Share the expensive full-source build, while keeping the paired-input check
// independent: it must still run when the known wrapper omission fails.
let productionInventory;
function fullInventory() {
  return productionInventory || (productionInventory = build({ root }));
}

test('swatch inventory: production pm-stroke registration is inventoried', () => {
  // Use all actual production inputs: no copied generator or special ID mapping.
  const { handler, ...registration } = rowFor(fullInventory(), 'pm-stroke');
  assert.deepEqual(registration, { events: ['click'], status: 'inventoried' });
  assert.ok(handler.length > 0, 'wrapper must have handler evidence');
  assert.ok(handler.some(site => /^src\/js\/color-picker\.js:\d+\b/.test(site)),
    'wrapper handler must cite the production color-picker.js source');
});

test('swatch inventory: generated paired input does not acquire wrapper click', () => {
  const input = rowFor(fullInventory(), 'pm-stroke-c');
  assert.ok(!input.events.includes('click'), 'paired input must not inherit wrapper click');
});

test('swatch inventory: independent direct registration is inventoried', t => {
  const call = "document.getElementById('direct-swatch').addEventListener('click', function () {});";
  assert.deepEqual(runtime('', call, ['direct-swatch']).events('direct-swatch'), ['click']);
  const { handler, ...registration } = rowFor(
    fixture(t, '', call, '<button id="direct-swatch">Color</button>'), 'direct-swatch');
  assert.deepEqual(registration, { events: ['click'], status: 'inventoried' });
  assert.ok(handler.some(site => /^src\/js\/swatch-call\.js:\d+\b/.test(site)));
});

test('swatch inventory: same-name no-op helper does not invent registration', t => {
  const noop = 'window.ColorPicker = { wireColorSwatches: function (pairs) {} };';
  const call = productionCall();
  assert.deepEqual(runtime(noop, call).events('pm-stroke'), []);
  assert.deepEqual(rowFor(fixture(t, noop, call), 'pm-stroke'), { events: [], status: 'unmapped', handler: [] });
});

test('swatch inventory: commented production call does not activate helper', t => {
  const call = productionCall().split('\n').map(line => '// ' + line).join('\n');
  assert.deepEqual(runtime(helper, call).events('pm-stroke'), []);
  assert.deepEqual(rowFor(fixture(t, helper, call), 'pm-stroke'), { events: [], status: 'unmapped', handler: [] });
});

const lookupOnly = `window.ColorPicker = { wireColorSwatches: function (pairs) {
  pairs.forEach(function (pair) {
    var wrap = document.getElementById(pair.wrap);
    if (wrap) wrap.style.background = '#000000';
  });
} };`;

test('swatch inventory: lookup-only helper does not invent registration', t => {
  const call = productionCall();
  assert.deepEqual(runtime(lookupOnly, call).events('pm-stroke'), []);
  assert.deepEqual(rowFor(fixture(t, lookupOnly, call), 'pm-stroke'), { events: [], status: 'unmapped', handler: [] });
});

test('swatch inventory: listener comment does not invent registration', t => {
  const commented = lookupOnly.replace("if (wrap)", "// wrap.addEventListener('click', function () {});\n    if (wrap)");
  const call = productionCall();
  assert.deepEqual(runtime(commented, call).events('pm-stroke'), []);
  assert.deepEqual(rowFor(fixture(t, commented, call), 'pm-stroke'), { events: [], status: 'unmapped', handler: [] });
});
