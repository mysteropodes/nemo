// Regression test for #1099 — CLAUDE.md §1 bug family n°1: a synthetic,
// non-paintable layer kind must be excluded by EVERY reader that treats a
// layer's live Paper children as real strokes, not just the one where it was
// introduced. getEffectiveStrokes is the reference list; the two save loops
// (persisted data) and the multi-layer selection box must match it.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..');
const app = fs.readFileSync(path.join(root, 'src/js/app.js'), 'utf8');
const select = fs.readFileSync(path.join(root, 'src/js/select-bridge.js'), 'utf8');

const SYNTHETIC_KINDS = ['isNullLayer', 'isEffectLayer', 'isGuideLayer', 'isWidgetLayer', 'isEffectorLayer'];

// The guard is the first line after the function header that tests isWidgetLayer.
function guardLine(source, header) {
  const start = source.indexOf(header);
  assert.notEqual(start, -1, `${header} not found`);
  const line = source.slice(start).split('\n').find((l) => l.includes('isWidgetLayer'));
  assert.ok(line, `no synthetic-layer guard found after ${header}`);
  return line;
}

const sites = [
  ['getEffectiveStrokes', app, 'function getEffectiveStrokes('],
  ['saveActiveLayerFrame', app, 'function saveActiveLayerFrame('],
  ['saveAllLayerFrames', app, 'function saveAllLayerFrames('],
  ['multiLayerSelectionBox', select, 'function multiLayerSelectionBox('],
];

// buildSceneJson (render) skips each synthetic kind with its own early
// `continue`, one line per kind, rather than one combined condition.
test('buildSceneJson gives every synthetic layer kind an empty stack slot', () => {
  const bridge = fs.readFileSync(path.join(root, 'src/js/engine-bridge.js'), 'utf8');
  const start = bridge.indexOf('function buildSceneJson(');
  assert.notEqual(start, -1, 'buildSceneJson not found');
  const body = bridge.slice(start, bridge.indexOf('\n  function ', start + 1));
  for (const kind of SYNTHETIC_KINDS) {
    assert.match(body, new RegExp(`if \\(state\\.layers\\[i\\]\\.${kind}\\) \\{[^}]*\\}\\);? *(\\n[^\\n]*)?continue;`),
      `buildSceneJson has no skip for ${kind}`);
  }
});

for (const [name, source, header] of sites) {
  test(`${name} excludes every synthetic layer kind`, () => {
    const line = guardLine(source, header);
    for (const kind of SYNTHETIC_KINDS) {
      assert.ok(line.includes(kind), `${name} guard is missing ${kind}`);
    }
  });
}
