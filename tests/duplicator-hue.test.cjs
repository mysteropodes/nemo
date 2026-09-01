// Unit tests for hexHueRotate (src/js/app.js) — the pure color-math piece
// of the Duplicator's Hue effector/stagger channel (2026-09-01). Extracted
// via vm the same way performance-regressions.test.cjs pulls pushUndoLayers
// out of tweens.js: the function has zero external dependencies (no
// window/state reads), so it's cheap to isolate and worth testing directly
// rather than only through the stateful duplicator pipeline, which is
// verified live instead (see the PR description for exact live numbers).
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.join(__dirname, '..');
const source = fs.readFileSync(path.join(root, 'src/js/app.js'), 'utf8');
const start = source.indexOf('function hexHueRotate(');
const end = source.indexOf('\n}', start) + 2;
if (start === -1) throw new Error('hexHueRotate not found in src/js/app.js');

const sandbox = {};
vm.runInNewContext(`${source.slice(start, end)}\nthis.hexHueRotate = hexHueRotate;`, sandbox);
const hexHueRotate = sandbox.hexHueRotate;

test('a full 360 degree rotation returns to the original color', () => {
  assert.equal(hexHueRotate('#ff0000', 360), '#ff0000');
});

test('rotating red by 120 degrees gives green, by 240 gives blue', () => {
  assert.equal(hexHueRotate('#ff0000', 120), '#00ff00');
  assert.equal(hexHueRotate('#ff0000', 240), '#0000ff');
});

test('an 8-digit hex keeps its alpha byte untouched', () => {
  assert.equal(hexHueRotate('#ff0000aa', 120), '#00ff00aa');
});

test('zero degrees and falsy input pass through unchanged', () => {
  assert.equal(hexHueRotate('#ff0000', 0), '#ff0000');
  assert.equal(hexHueRotate(null, 90), null);
  assert.equal(hexHueRotate(undefined, 90), undefined);
  assert.equal(hexHueRotate('', 90), '');
});

test('a non-hex string passes through unchanged instead of throwing', () => {
  assert.equal(hexHueRotate('red', 90), 'red');
  assert.equal(hexHueRotate('rgba(0,0,0,1)', 90), 'rgba(0,0,0,1)');
});

test('a fully desaturated gray is unaffected by any hue rotation', () => {
  assert.equal(hexHueRotate('#808080', 90), '#808080');
  assert.equal(hexHueRotate('#000000', 180), '#000000');
  assert.equal(hexHueRotate('#ffffff', 45), '#ffffff');
});

test('negative and over-360 degree shifts wrap the same as their modulo', () => {
  assert.equal(hexHueRotate('#ff0000', -240), hexHueRotate('#ff0000', 120));
  assert.equal(hexHueRotate('#ff0000', 480), hexHueRotate('#ff0000', 120));
});
