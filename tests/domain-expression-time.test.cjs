'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const domain = require('../src/js/domain/animation/expression-time.js');

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
    'src/js/motion.js'
  ]) {
    vm.runInContext(fs.readFileSync(path.join(root, file), 'utf8'), context, { filename: file });
  }
  return context;
}

// valueAtFrame always returns an array shaped to the property's dimension
// (confirmed against tests/domain-opacity.test.cjs's own `[0]` indexing) -
// 'position' is 2D, so a scalar expression result comes back filled to both
// components; index into it rather than comparing the array itself, which
// also sidesteps vm.createContext's separate-realm Array identity.
function exprHolder(code) {
  return { expressions: { position: { code: code, enabled: true } } };
}

test('domain module is pure: no window/state/_ectx access outside comments', () => {
  const src = fs.readFileSync(path.join(root, 'src/js/domain/animation/expression-time.js'), 'utf8');
  const code = src.split('\n').map(line => line.replace(/\/\/.*$/, '')).join('\n');
  assert.equal(/\bwindow\b|\b_ectx\b|\bstate\./.test(code), false);
});

test('domain stepTime snaps to the grid and passes an invalid/non-positive step through unchanged', () => {
  assert.equal(domain.stepTime(10, 4), 8);
  assert.equal(domain.stepTime(10, 3), 9);
  assert.equal(domain.stepTime(10, 0), 10);
  assert.equal(domain.stepTime(10, -1), 10);
  assert.equal(domain.stepTime(10, NaN), 10);
});

test('domain toFrames/toSeconds convert both ways and guard zero fps', () => {
  assert.equal(domain.toFrames(2, 30), 60);
  assert.equal(domain.toSeconds(60, 30), 2);
  assert.equal(domain.toSeconds(60, 0), 0);
});

test('real expression evaluation: toFrames/toSeconds with an explicit argument', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  assert.equal(motion.valueAtFrame(exprHolder('return toFrames(2);'), 'position', 0)[0], 48);
  assert.equal(motion.valueAtFrame(exprHolder('return toSeconds(48);'), 'position', 0)[0], 2);
});

test('real expression evaluation: omitted argument falls back to the current ctx value', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  // toFrames() with no argument falls back to ctx.time (frame/fps); at frame
  // 48 @ 24fps that is 2s, so toFrames() round-trips back to 48.
  assert.equal(motion.valueAtFrame(exprHolder('return toFrames();'), 'position', 48)[0], 48);
  // toSeconds() with no argument falls back to ctx.frame directly (not
  // ctx.time) - documented existing behavior, unchanged by this extraction.
  assert.equal(motion.valueAtFrame(exprHolder('return toSeconds();'), 'position', 48)[0], 2);
});

test('real expression evaluation: stepTime mutates the evaluation clock for the rest of the SAME expression', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  // frame 10, step every 4 frames -> snaps to 8. The subsequent toSeconds()
  // call (no argument) must see the SNAPPED frame (8/24), not the raw one.
  const holder = exprHolder('stepTime(4); return toSeconds();');
  assert.equal(motion.valueAtFrame(holder, 'position', 10)[0], 8 / 24);
});

test('real expression evaluation: bare time/frame arguments keep their documented unsnapped values after stepTime', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  // Per motion.js's own doc comment on exprStepTime: the bare `frame`/`time`
  // arguments are bound by value before user code runs, so stepTime()
  // mutating the ctx does not retroactively change them.
  const before = motion.valueAtFrame(exprHolder('return frame;'), 'position', 10)[0];
  const after = motion.valueAtFrame(exprHolder('stepTime(4); return frame;'), 'position', 10)[0];
  assert.equal(before, 10);
  assert.equal(after, 10);
});

test('real expression evaluation: invalid/non-positive stepTime argument leaves the clock unchanged', () => {
  const ctx = load(24), motion = ctx.SMMotion;
  assert.equal(motion.valueAtFrame(exprHolder('stepTime(0); return frame;'), 'position', 10)[0], 10);
  assert.equal(motion.valueAtFrame(exprHolder('stepTime(-3); return frame;'), 'position', 10)[0], 10);
});
