'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const domain = require('../src/js/domain/animation/expression-random.js');

function load(fps) {
  const context = {
    document: { readyState: 'loading', addEventListener() {} },
    localStorage: { getItem() { return null; } },
    state: { currentFrame: 0, layers: [], fps: fps || 24 }
  };
  context.window = context;
  vm.createContext(context);
  for (const file of [
    'src/js/animation/curve.js',
    'src/js/domain/animation/opacity.js',
    'src/js/domain/animation/expression-time.js',
    'src/js/domain/animation/expression-random.js',
    'src/js/motion.js'
  ]) {
    vm.runInContext(fs.readFileSync(path.join(root, file), 'utf8'), context, { filename: file });
  }
  return context;
}

// valueAtFrame returns an array shaped to the property's dimension (see
// tests/domain-expression-time.test.cjs); index into it rather than
// comparing the array, which also sidesteps vm's separate-realm identity.
function exprHolder(code) {
  return { expressions: { position: { code: code, enabled: true } } };
}

test('domain hashUnit is deterministic for the same (seed, n)', () => {
  assert.equal(domain.hashUnit(3, 5), domain.hashUnit(3, 5));
  assert.notEqual(domain.hashUnit(3, 5), domain.hashUnit(4, 5));
});

test('domain rand01FromInputs: fixed ignores frame, varying does not, different seeds diverge', () => {
  const a = domain.rand01FromInputs(7, 0, 10, false, true);
  const b = domain.rand01FromInputs(7, 0, 99, false, true);
  assert.equal(a, b); // fixed: frame is irrelevant
  const c = domain.rand01FromInputs(7, 0, 10, false, false);
  const d = domain.rand01FromInputs(7, 0, 99, false, false);
  assert.notEqual(c, d); // varying: frame matters
  const e = domain.rand01FromInputs(8, 0, 10, false, false);
  assert.notEqual(c, e); // different seed diverges
  const f = domain.rand01FromInputs(7, 1, 10, false, false);
  assert.notEqual(c, f); // counter advancing changes the draw
  for (const v of [a, b, c, d, e, f]) assert.ok(v >= 0 && v < 1);
});

test('domain rand01FromInputs: rngTimeless behaves like fixed regardless of the fixed flag', () => {
  const timelessA = domain.rand01FromInputs(7, 0, 10, true, false);
  const timelessB = domain.rand01FromInputs(7, 0, 99, true, false);
  assert.equal(timelessA, timelessB);
});

test('domain gaussFromUniforms stays in range and is a pure function of its two inputs', () => {
  const g1 = domain.gaussFromUniforms(0.3, 0.6);
  const g2 = domain.gaussFromUniforms(0.3, 0.6);
  assert.equal(g1, g2);
  const g3 = domain.gaussFromUniforms(0.3, 0.7);
  assert.notEqual(g1, g3);
  // u1=0 would log(0)=-Infinity without the clamp; must not throw or go NaN.
  assert.equal(Number.isFinite(domain.gaussFromUniforms(0, 0.5)), true);
});

test('domain randomWith: no-args, scalar, and array forms', () => {
  const gen = () => 0.5;
  assert.equal(domain.randomWith(gen), 0.5);
  assert.equal(domain.randomWith(gen, 10), 5);
  assert.deepEqual(domain.randomWith(gen, [10, 20]), [5, 10]);
  assert.equal(domain.randomWith(gen, 10, 20), 15);
  assert.deepEqual(domain.randomWith(gen, [0, 0], [10, 100]), [5, 50]);
});

test('real expression evaluation: seed() makes random() fully repeatable across separate evaluations', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  const code = 'seed(42); return random(100);';
  const v1 = motion.valueAtFrame(exprHolder(code), 'position', 5)[0];
  const v2 = motion.valueAtFrame(exprHolder(code), 'position', 5)[0];
  assert.equal(v1, v2);
});

test('real expression evaluation: random() varies by frame, randomFixed() does not', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  const varyCode = 'seed(1); return random(1000);';
  const v5 = motion.valueAtFrame(exprHolder(varyCode), 'position', 5)[0];
  const v6 = motion.valueAtFrame(exprHolder(varyCode), 'position', 6)[0];
  assert.notEqual(v5, v6);

  const fixedCode = 'seed(1); return randomFixed(1000);';
  const f5 = motion.valueAtFrame(exprHolder(fixedCode), 'position', 5)[0];
  const f6 = motion.valueAtFrame(exprHolder(fixedCode), 'position', 6)[0];
  assert.equal(f5, f6);
});

test('real expression evaluation: randomGauss/randomGaussFixed run through the real pipeline', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  const v = motion.valueAtFrame(exprHolder('seed(3); return randomGauss(100);'), 'position', 5)[0];
  assert.ok(Number.isFinite(v));
  const fixedCode = 'seed(3); return randomGaussFixed(100);';
  const f5 = motion.valueAtFrame(exprHolder(fixedCode), 'position', 5)[0];
  const f9 = motion.valueAtFrame(exprHolder(fixedCode), 'position', 9)[0];
  assert.equal(f5, f9);
});

test('real expression evaluation: two properties on the same holder draw distinct streams', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  const holder = {
    expressions: {
      position: { code: 'return random(1000);', enabled: true },
      rotation: { code: 'return random(1000);', enabled: true }
    }
  };
  const p = motion.valueAtFrame(holder, 'position', 5)[0];
  const r = motion.valueAtFrame(holder, 'rotation', 5)[0];
  assert.notEqual(p, r);
});
