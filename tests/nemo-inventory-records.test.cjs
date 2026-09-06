'use strict';
// Compare the exported-record inventory with actual registrations in a VM.
// Unsupported receiver flow must remain unmapped instead of inventing events.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const { build } = require('../scripts/nemo/inventory.cjs');

const exported = `function good(pairs) {
  pairs.forEach(function(pair) {
    var wrap = document.getElementById(pair.wrap);
    wrap.addEventListener('click', function() {});
  });
}
window.ColorPicker = { wireColorSwatches: good };`;
const call = "window.ColorPicker.wireColorSwatches([{wrap:'a',input:'b'}]);";
const namedReplacement = 'function noop() {} window.ColorPicker = {wireColorSwatches:noop};';
const inlineReplacement = 'window.ColorPicker = {wireColorSwatches:function(){}};';

const cases = [
  ['unique export and literal wrapper', [exported, call], ['click']],
  ['locally shadowed window', [exported,
    `(function(window){ ${call} })({ColorPicker:{wireColorSwatches:function(){}}});`], []],
  ['same-file named replacement', [exported + namedReplacement + call], []],
  ['cross-file named replacement', [exported, namedReplacement, call], []],
  ['same-file inline replacement', [exported + inlineReplacement + call], []],
  ['cross-file inline replacement', [exported, inlineReplacement, call], []],
  ['direct member replacement', [exported,
    'window.ColorPicker.wireColorSwatches = function() {};', call], []],
  ['list parameter reassignment', [exported.replace('pairs.forEach', 'pairs = []; pairs.forEach'), call], []],
];

for (const [name, sources, expected] of cases) {
  test('exported record identity: ' + name, (t) => {
    const actual = { a: [], b: [] };
    const context = vm.createContext({ window: {}, document: {
      getElementById: id => ({ addEventListener: event => actual[id].push(event) }),
    } });
    for (const source of sources) vm.runInContext(source, context, { timeout: 1000 });
    assert.deepEqual(actual, { a: expected, b: [] }, 'runtime registration precondition');

    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-inventory-records-'));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    fs.mkdirSync(path.join(root, 'src/js'), { recursive: true });
    fs.writeFileSync(path.join(root, 'src/index.html'), '<button id="a">A</button><button id="b">B</button>');
    sources.forEach((source, i) => fs.writeFileSync(path.join(root, 'src/js',
      ['app.js', 'timeline.js', 'zzz-call.js'][i]), source));
    const rows = build({ root }).rows;
    for (const id of ['a', 'b']) {
      const row = rows.find(item => item.id === 'dom:#' + id);
      assert.ok(row, 'control must be inventoried: ' + id);
      assert.deepEqual(row.events, actual[id], id + ': event identity');
      assert.equal(row.status, actual[id].length ? 'inventoried' : 'unmapped', id + ': status');
    }
  });
}
