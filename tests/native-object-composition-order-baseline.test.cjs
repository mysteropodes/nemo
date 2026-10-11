'use strict';
// Actual frozen producer excerpts + complete Motion facade; no native painter.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const fixturePath = path.join(root, 'engineering/application/examples/native-object-order-v2/baseline.json');
const fixtureBytes = fs.readFileSync(fixturePath);
const fixture = JSON.parse(fixtureBytes);
const clone = value => JSON.parse(JSON.stringify(value));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
assert.equal(sha(fixtureBytes), '37297da17321c132a488cb170231b68be157a86502a36bcf8dd4ea175260411a');
const frozen = {};
for (const [file, pin] of Object.entries(fixture.pins)) {
  const git = args => execFileSync('git', args, { cwd: root, maxBuffer: 4 * 1024 * 1024 });
  assert.equal(git(['rev-parse', `${fixture.baseline}:${file}`]).toString().trim(), pin.blob);
  const bytes = git(['show', `${fixture.baseline}:${file}`]);
  assert.equal(sha(bytes), pin.sha256, `unexpected frozen ${file}`);
  frozen[file] = bytes.toString('utf8');
}
function excerpt(name) {
  const pin = fixture.excerpts[name], source = frozen['src/js/engine-bridge.js'];
  assert.equal(source.split(pin.start).length, 2, `unique ${name} opener`);
  const start = source.indexOf(pin.start), end = source.indexOf(pin.end, start);
  assert.ok(end > start, `missing ${name} closer`);
  const result = source.slice(start, end);
  assert.equal(sha(result), pin.sha256, `unexpected ${name} producer`);
  return result;
}
const layerProducer = excerpt('layers'), elementProducer = excerpt('elements');
function context(holders) {
  const result = {
    document: { readyState: 'loading', addEventListener() {} },
    localStorage: { getItem() { return null; } },
    state: { layers: clone(holders), fps: 20, currentFrame: 0 },
    SM: { t: key => key }, renderFrame: 0,
    setTimeout() { throw new Error('unexpected asynchronous oracle effect'); },
  };
  result.window = result;
  vm.createContext(result);
  for (const file of ['src/js/animation/curve.js', 'src/js/domain/animation/opacity.js', 'src/js/motion.js']) {
    vm.runInContext(frozen[file], result, { filename: `${fixture.baseline}:${file}`, timeout: 1000 });
  }
  assert.equal(typeof result.SMMotion.layerOrderAt, 'function');
  assert.equal(typeof result.SMMotion.elementOrderAt, 'function');
  return result;
}
function layerOrder(holders, frame = 0, producer = layerProducer) {
  const c = context(holders);
  c.renderFrame = frame;
  c.layers = holders.map((_, index) => ({ id: `L${index}` }));
  c._anyLayerOrder = holders.some((_, index) => c.SMMotion.layerHasExplicitOrder(index));
  const ranks = holders.map((_, index) => c.SMMotion.layerOrderAt(index, frame));
  vm.runInContext(producer, c, { timeout: 1000 });
  return { ranks, ids: Array.from(c.layers, item => item.id), context: c };
}
function elementOrder(holders, frame = 0, producer = elementProducer) {
  const c = context([{ elementMotion: holders }]);
  c.items = ['a', 'b', 'c'].map(id => ({ id, __ord: c.SMMotion.elementOrderAt(0, id, frame) }));
  c._anyElemOrder = true; // The excerpt's admitted sort branch, no geometry loop.
  vm.runInContext(producer, c, { timeout: 1000 });
  assert.ok(c.items.every(item => !('__ord' in item) && !('__ordIx' in item)));
  return Array.from(c.items, item => item.id);
}
const mismatch = (actual, expected) => assert.throws(() => assert.deepEqual(actual, expected), assert.AssertionError);

test('frozen provenance, exact producer excerpts and export routing remain identified', () => {
  assert.equal(fixture.baseline, 'ded641bd763379f36d629bf1686fcce8a6137a66');
  assert.equal(fixture.direction, 'back-to-front');
  assert.match(frozen['src/js/export.js'], /function exportHasEngineOnlyMotion\(\)/);
  assert.match(frozen['src/js/export.js'], /anyOrderUsedAnywhere/);
  assert.match(layerProducer, /Math\.round\(e.rank\) - 1/);
  assert.match(elementProducer, /a.__ordIx - b.__ordIx/);
});

test('natural, displacement, collision, rounding, clamping and static zero use real layer insertion', () => {
  for (const sample of fixture.layerCases) {
    assert.deepEqual(layerOrder(sample.holders).ids, sample.expected, sample.id);
  }
  const untouched = context([{}, {}, {}]);
  assert.deepEqual([0, 1, 2].map(i => untouched.SMMotion.layerOrderAt(i, 0)), [3, 2, 1]);
  assert.equal(untouched.SMMotion.layerHasExplicitOrder(0), false);
  const explicit = context([{ motionStatic: { order: [0] } }, {}, {}]);
  assert.equal(explicit.SMMotion.layerHasExplicitOrder(0), true);
  assert.equal(explicit.SMMotion.layerOrderAt(0, 0), 0);
});

test('actual element evaluator and stable descending producer stay within one layer', () => {
  for (const sample of fixture.elementCases) {
    assert.deepEqual(elementOrder(sample.holders), sample.expected, sample.id);
  }
});

test('held and linearly eased frame ranks drive the actual layer insertion producer', () => {
  const sample = fixture.animated.linearAndHold;
  for (const point of sample.samples) {
    const actual = layerOrder(sample.holders, point.frame);
    assert.deepEqual(actual.ranks, point.ranks, `frame ${point.frame} ranks`);
    assert.deepEqual(actual.ids, point.expected, `frame ${point.frame} paint`);
  }
});

test('whole frozen facade evaluates nonlinear easing, outgoing hold and real expression code', () => {
  for (const name of ['nonlinear', 'held', 'expression']) {
    const sample = fixture.animated[name], c = context([sample.holder]);
    for (const point of sample.samples) {
      assert.equal(c.SMMotion.layerOrderAt(0, point.frame), point.value, `${name} ${point.frame}`);
      assert.equal(c.SMMotion.elementOrderAt(0, 'a', point.frame), 0);
    }
    if (name === 'expression') {
      // Runtime compilation/error bookkeeping is an observed baseline effect.
      assert.ok(c.state.layers[0]._exprCompiled.order.fn);
      assert.equal(c.state.layers[0].expressions.order.lastError, null);
      assert.equal(c.state.layers[0]._exprSeed, 12345);
    }
  }
  const sample = fixture.animated.expressionWithoutBase;
  const c = context([{ ...clone(sample.holder), elementMotion: { a: clone(sample.holder) } }]);
  assert.equal(c.SMMotion.layerHasExplicitOrder(0), sample.layerExplicit);
  assert.equal(c.SMMotion.layerOrderAt(0, sample.frame), sample.layerValue);
  assert.equal(c.SMMotion.elementOrderAt(0, 'a', sample.frame), sample.elementValue);
});

test('same stroke ID in other complete scopes cannot alias; Reference identity remains separate', () => {
  const identities = fixture.scopedCases.map(({ contextId, frameScope, layerUid, strokeId }) =>
    JSON.stringify([contextId, frameScope.kind, frameScope.frame, layerUid, strokeId]));
  assert.equal(new Set(identities).size, fixture.scopedCases.length);
  const [first, otherLayer] = fixture.scopedCases;
  const shared = context([
    { elementMotion: { a: clone(first.holder) } },
    { elementMotion: { a: clone(otherLayer.holder) } },
  ]);
  assert.equal(shared.SMMotion.elementOrderAt(0, 'a', 7), first.expectedValue);
  assert.equal(shared.SMMotion.elementOrderAt(1, 'a', 7), otherLayer.expectedValue);
  mismatch(shared.SMMotion.elementOrderAt(0, 'a', 7), otherLayer.expectedValue);
  const aliased = new Set(fixture.scopedCases.map(sample => sample.strokeId));
  mismatch(aliased.size, fixture.scopedCases.length);
  for (const sample of fixture.scopedCases) {
    if (sample.frameScope.kind === 'reference') {
      assert.equal(sample.disposition, 'unavailable-reference-policy');
      continue; // No Reference render/evaluation policy is invented here.
    }
    const c = context([{ elementMotion: { [sample.strokeId]: clone(sample.holder) } }]);
    assert.equal(c.SMMotion.elementOrderAt(0, sample.strokeId, sample.frameScope.frame), sample.expectedValue);
  }
});

test('actual wrong layer-sort, direction and unstable-tie mutations fail independent expectations', () => {
  const displaced = fixture.layerCases.find(c => c.id === 'displacement');
  const flat = 'layers.sort(function(a,b){return SMMotion.layerOrderAt(Number(b.id.slice(1)),renderFrame)-SMMotion.layerOrderAt(Number(a.id.slice(1)),renderFrame);});';
  mismatch(layerOrder(displaced.holders, 0, flat).ids, displaced.expected);
  const tie = fixture.elementCases.find(c => c.id === 'stable-tie');
  const unstable = elementProducer.replace('a.__ordIx - b.__ordIx', 'b.__ordIx - a.__ordIx');
  assert.notEqual(unstable, elementProducer);
  mismatch(elementOrder(tie.holders, 0, unstable), tie.expected);
  const descending = fixture.elementCases.find(c => c.id === 'descending');
  const wrongDirection = elementProducer.replace('(b.__ord || 0) - (a.__ord || 0)', '(a.__ord || 0) - (b.__ord || 0)');
  assert.notEqual(wrongDirection, elementProducer);
  mismatch(elementOrder(descending.holders, 0, wrongDirection), descending.expected);
});

test('frame, hold, nonlinear ease and expression bypass mutations fail value or sequence oracles', () => {
  const frame = fixture.animated.linearAndHold.samples.find(p => p.frame === 10);
  mismatch(layerOrder(fixture.animated.linearAndHold.holders, 0).ids, frame.expected);
  const held = clone(fixture.animated.held.holder);
  held.motion.order.keys[0].hold = false;
  mismatch(context([held]).SMMotion.layerOrderAt(0, 9), 1);
  const eased = clone(fixture.animated.nonlinear.holder);
  eased.motion.order.keys[0].curvePoints = [];
  mismatch(context([eased]).SMMotion.layerOrderAt(0, 5), 1.3125);
  const expression = clone(fixture.animated.expression.holder);
  expression.expressions.order.enabled = false;
  mismatch(context([expression]).SMMotion.layerOrderAt(0, 5), 3);
});

test('storage witness is an explicit versioned authored plan, not inferred v1 encounter order', () => {
  const plan = fixture.storageExample;
  assert.equal(plan.format, 'nemo.native-object-order-plan');
  assert.equal(plan.formatVersion, 1);
  assert.equal(plan.documentFormatVersion, 2);
  assert.equal(plan.migration, 'explicit-plan-required');
  assert.equal(plan.referencePolicy, 'unresolved');
  for (const scope of plan.scopes) {
    assert.equal(scope.frameScope.kind, 'authored');
    assert.deepEqual(scope.layers.map(layer => layer.naturalPosition), [0, 1]);
    assert.equal(new Set(scope.layers.map(layer => layer.layerUid)).size, scope.layers.length);
    assert.equal(new Set(scope.elements.map(element => JSON.stringify([element.layerUid, element.strokeId]))).size, scope.elements.length);
    assert.ok(scope.elements.every(element => scope.layers.some(layer => layer.layerUid === element.layerUid)));
  }
  const contract = fs.readFileSync(path.join(root, 'engineering/application/NATIVE_OBJECT_COMPOSITION_ORDER.md'), 'utf8');
  for (const marker of ['compositionOrder', 'naturalPosition', 'frameScope', 'expression', 'v1', 'Reference', 'unavailable']) assert.ok(contract.includes(marker));
});

test('committed input fixture bytes remain unchanged after baseline reads and isolated cache effects', () => {
  assert.deepEqual(fs.readFileSync(fixturePath), fixtureBytes);
  assert.deepEqual(JSON.parse(fixtureBytes), fixture);
});
