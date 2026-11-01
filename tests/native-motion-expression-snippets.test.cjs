'use strict';
// D18-C (#2376): every Examples entry inserts exactly the baseline text (buildExprEditorRow @1065e9d8), per dimensionality.
const test = require('node:test');
const assert = require('node:assert/strict');
const S = require('../src/js/adapters/native-motion-expression-snippets.js');

const BOUNCE = 'const n = self.keys.count;\nif (!n) return value;\nconst t = (frame - self.keys.at(n).frame) / comp.fps;\nif (t <= 0) return value;\n' +
  'const b = 40 * Math.exp(-4 * t) * Math.sin(3 * 2 * Math.PI * t);\n';
const ONE = {
  value: 'value', wiggle: 'value + wiggle(2, 10)', oscillate: 'value + Math.sin(time * 3) * 10', stepped: 'stepTime(4);\nreturn value + wiggle(3, 15);',
  loopCycle: "loopAfter('cycle')", loopPingpong: "loopAfter('pingpong')", loopOffset: "loopAfter('offset')", loopBefore: "loopBefore('cycle')",
  bounce: BOUNCE + 'return value + b;', remapOther: "remap(layer('Back').position[0], 0, comp.width, 0, 100)",
  stagger: 'seed(self.index);\nreturn value + randomFixed(-20, 20);', contentBox: 'contentBox().width + 20',
  angleTo: "angleTo(layer('Back').position, layer('Front').position)", followLayer: "layer('Back').rotation",
  firstKey: 'self.keys.count ? self.keys.at(1).value : value', control: 'value + self.control("Turn")',
};
const TWO = {
  value: 'value', wiggle: '[value[0] + wiggle(2, 10)[0], value[1] + wiggle(2, 10)[1]]',
  oscillate: 'const w = Math.sin(time * 3) * 10;\nreturn [value[0] + w, value[1] + w];',
  stepped: 'stepTime(4);\nconst w = wiggle(3, 15);\nreturn [value[0] + w[0], value[1] + w[1]];',
  loopCycle: "loopAfter('cycle')", loopPingpong: "loopAfter('pingpong')", loopOffset: "loopAfter('offset')", loopBefore: "loopBefore('cycle')",
  bounce: BOUNCE + 'return [value[0], value[1] + b];',
  remapOther: "const k = remap(layer('Back').position[0], 0, comp.width, 0, 1);\nreturn [value[0], value[1] + k * 100];",
  stagger: 'seed(self.index);\nconst o = randomFixed(-20, 20);\nreturn [value[0] + o, value[1] + o];',
  contentBox: 'const b = contentBox();\nreturn [b.width + 20, b.height + 20];', followLayer: "layer('Back').position",
  firstKey: 'self.keys.count ? self.keys.at(1).value : value', control: 'const c = self.control("Turn");\nreturn [value[0] + c, value[1] + c];',
};
const codes = (items) => Object.fromEntries(items.map((i) => [i.id, i.code]));

test('one-dimensional property: every snippet is the exact baseline text, in baseline order', () => {
  const items = S.list({ dimension: 1, property: 'rotation', others: ['Back', 'Front'], control: { name: 'Turn', type: 'number' } });
  assert.deepEqual(codes(items), ONE);
  assert.deepEqual(items.map((i) => i.id), Object.keys(ONE));
  assert.equal(items.find((i) => i.id === 'followLayer').suffix, ' · rotation');
  assert.equal(items.find((i) => i.id === 'control').suffix, ' · Turn');
  assert.equal(items.every((i) => /^exprEx[A-Z]/.test(i.labelKey)), true);
});

test('two-dimensional property: array-safe branches, angleTo dropped, follow names position', () => {
  const items = S.list({ dimension: 2, property: 'position', others: ['Back', 'Front'], control: { name: 'Turn', type: 'number' } });
  assert.deepEqual(codes(items), TWO);
  assert.equal(items.some((i) => i.id === 'angleTo'), false);
  assert.equal(items.find((i) => i.id === 'followLayer').suffix, ' · position');
});

test('a multi-dimension control is inserted on its own line; a control-less layer has no control entry; placeholders are real names or Layer 1/2', () => {
  const point = S.list({ dimension: 1, property: 'rotation', others: [], control: { name: 'Pad', type: 'point' } });
  assert.equal(point.find((i) => i.id === 'control').code, 'self.control("Pad")');
  const none = S.list({ dimension: 1, property: 'opacity', others: [], control: null });
  assert.equal(none.some((i) => i.id === 'control'), false);
  assert.equal(none.find((i) => i.id === 'angleTo').code, "angleTo(layer('Layer 1').position, layer('Layer 2').position)");
  assert.equal(none.find((i) => i.id === 'followLayer').code, "layer('Layer 1').opacity");
  assert.equal(S.list({ dimension: 1, property: 'xc_9', others: ['A'], control: null }).find((i) => i.id === 'followLayer').code, "layer('A').position");
});

test('every label key exists in the four locale blocks of i18n.js', () => {
  const text = require('node:fs').readFileSync(require('node:path').join(__dirname, '../src/js/i18n.js'), 'utf8');
  const keys = new Set(S.list({ dimension: 1, property: 'rotation', others: [], control: { name: 'x', type: 'number' } }).map((i) => i.labelKey));
  keys.forEach((k) => assert.equal(text.split(k + ':').length - 1, 4, k));
});
