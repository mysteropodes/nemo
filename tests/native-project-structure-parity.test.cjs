'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const SOURCE_PATH = path.join(ROOT, 'src/js/project-document.js');
const source = fs.readFileSync(SOURCE_PATH, 'utf8');
const corpus = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures/native-project-structure/cases.json'), 'utf8'));
assert.equal(corpus.schema, 'nemo.project-structure-parity.v1');
assert.equal(corpus.cases.length, 46);
assert.equal(new Set(corpus.cases.map(entry => entry.id)).size, corpus.cases.length);

function productionParser(text = source) {
  // Run the whole production UMD module in its supported browser environment.
  // No import fallback or hand-copied implementation can become the oracle.
  const context = vm.createContext({ window: {} });
  vm.runInContext(text, context, { filename: SOURCE_PATH });
  assert.equal(typeof context.window.SMProjectDocument.parse, 'function');
  return context.window.SMProjectDocument.parse;
}

function jsOutcome(parse, input) {
  try {
    return { ok: true, value: JSON.parse(JSON.stringify(parse(input))) };
  } catch (error) {
    if (error.name === 'SyntaxError') {
      return { ok: false, error: { kind: 'invalid-json', message: 'Invalid JSON', layerIndex: null, frameIndex: null } };
    }
    const message = error.message;
    const frame = /^Fichier invalide \(calque (\d+), frame (\d+)\)$/.exec(message);
    const layer = /^Fichier invalide \(calque (\d+)\)$/.exec(message);
    let kind;
    if (frame) kind = 'frame-strokes';
    else if (layer) kind = 'layer-frames';
    else if (message === 'Fichier invalide (layers)') kind = 'layers';
    else if (message === 'Invalid') kind = 'invalid-root';
    else throw error;
    return { ok: false, error: { kind, message,
      layerIndex: frame ? Number(frame[1]) - 1 : layer ? Number(layer[1]) - 1 : null,
      frameIndex: frame ? Number(frame[2]) - 1 : null } };
  }
}

test('N23A fixed independent corpus matches the untouched production JS parser', () => {
  const parse = productionParser();
  for (const entry of corpus.cases) assert.deepEqual(jsOutcome(parse, entry.json), entry.expected, entry.id);
});

test('N23A production Rust API matches the same fixed corpus and production JS', { timeout: 180000 }, () => {
  const result = spawnSync('cargo', [
    'test', '--locked', '--manifest-path', 'native-engine/Cargo.toml',
    '--no-default-features', '--features', 'test-codec', '--test', 'codec',
    'project_structure_tests::emit_parity_corpus', '--', '--exact', '--nocapture',
  ], { cwd: ROOT, encoding: 'utf8', timeout: 170000, maxBuffer: 4 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  const marker = 'N23A_RESULTS=';
  const lines = result.stdout.split(/\r?\n/).filter(line => line.startsWith(marker));
  assert.equal(lines.length, 1, `Rust parity harness must execute exactly once:\n${result.stdout}`);
  const actual = JSON.parse(lines[0].slice(marker.length));
  assert.deepEqual(actual.map(entry => entry.id), corpus.cases.map(entry => entry.id));
  const parse = productionParser();
  for (let index = 0; index < corpus.cases.length; index++) {
    const entry = corpus.cases[index];
    assert.deepEqual(actual[index].actual, entry.expected, `Rust independent expected: ${entry.id}`);
    assert.deepEqual(actual[index].actual, jsOutcome(parse, entry.json), `JS/Rust parity: ${entry.id}`);
  }
});

function mutantParser(before, after) {
  assert.equal(source.split(before).length - 1, 1, 'mutation must target one exact production expression');
  return productionParser(source.replace(before, after));
}

function requireDetectedMutation(parse, caseId) {
  const entry = corpus.cases.find(entry => entry.id === caseId);
  assert.ok(entry, 'mutation control names a fixed independent fixture');
  // The same equality used for Rust/JS parity must fail on the intentional fault.
  assert.throws(() => assert.deepEqual(jsOutcome(parse, entry.json), entry.expected), assert.AssertionError);
}

test('N23A corpus detects a dropped unknown nested property', () => {
  const parse = mutantParser('    return d;\n', '    delete d.layers[0].frames[0].strokes[0].future;\n    return d;\n');
  requireDetectedMutation(parse, 'current-opaque-nested');
});

test('N23A corpus detects an omitted frame-strokes check', () => {
  const parse = mutantParser('if (!f || !Array.isArray(f.strokes))', 'if (false)');
  requireDetectedMutation(parse, 'frame-strokes-missing');
});

test('N23A corpus detects replacing JS truthiness with layers presence', () => {
  const parse = mutantParser('if (!d.layers) d.layers =', "if (!Object.prototype.hasOwnProperty.call(d, 'layers')) d.layers =");
  requireDetectedMutation(parse, 'legacy-layers-false');
});
