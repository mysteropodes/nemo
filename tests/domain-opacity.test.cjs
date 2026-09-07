'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const requiredDomain = require('../src/js/domain/animation/opacity.js');
function load(frame) {
  const context = {
    document: { readyState: 'loading', addEventListener() {} },
    localStorage: { getItem() { return null; } },
    state: { currentFrame: frame || 0, layers: [] }
  };
  context.window = context;
  vm.createContext(context);
  for (const file of ['src/js/animation/curve.js', 'src/js/domain/animation/opacity.js', 'src/js/motion.js']) {
    vm.runInContext(fs.readFileSync(path.join(root, file), 'utf8'), context, { filename: file });
  }
  return context;
}
const values = value => Array.from(value);
const linear = () => [{ x: 0, y: 0 }, { x: 1, y: 1 }];

test('domain keeps stable layer lookup and snapshots read-only', () => {
  const ctx = load(4), domain = ctx.NemoOpacityDomain;
  assert.equal(requiredDomain.defaultCurve()[1].y, 0.156);
  const named = { layerUid: 'layer-a', name: 'A', motionStatic: { opacity: [45] } };
  const unnamed = { name: 'B', motionStatic: { opacity: [80] } };
  const state = { currentFrame: 4, layers: [named, unnamed] };
  assert.equal(domain.findLayer(state, 'layer-a'), named);
  assert.equal(domain.findLayer(state, 'layer-2'), null);
  assert.equal(unnamed.layerUid, undefined);
  assert.deepEqual(JSON.parse(JSON.stringify(domain.snapshot(state, ctx.SMMotion))), {
    frame: 4,
    layers: [{ id: 'layer-a', name: 'A', opacity: 45 }, { name: 'B', opacity: 80 }]
  });
  assert.equal(unnamed.layerUid, undefined);
  assert.equal(domain.number(12), true);
  assert.equal(domain.number(Infinity), false);
});

test('opacity writes preserve key shape, curves, cloning and holder separation', () => {
  const ctx = load(8), motion = ctx.SMMotion, domain = ctx.NemoOpacityDomain;
  const layer = {}, element = {};
  const custom = [{ x: 0, y: 0, tx: 0.3, ty: 0.2 }, { x: 1, y: 1 }];
  const first = motion.setKeyAtFrame(layer, 'opacity', 12, [30], custom);
  const second = motion.setKeyAtFrame(layer, 'opacity', 0, [90]);
  assert.equal(first.curvePoints, custom);
  assert.deepEqual(JSON.parse(JSON.stringify(second.curvePoints)), JSON.parse(JSON.stringify(motion.DEFAULT_CURVE())));
  assert.notEqual(second.curvePoints, domain.defaultCurve());
  const replacement = motion.setKeyAtFrame(layer, 'opacity', 12, [35]);
  assert.equal(replacement, first);
  assert.equal(replacement.curvePoints, custom);
  const supplied = linear();
  motion.setKeyAtFrame(layer, 'opacity', 12, [40], supplied);
  assert.equal(first.curvePoints, supplied);
  const source = [55];
  motion.setValue(element, 'opacity', source);
  source[0] = 0;
  assert.deepEqual(values(element.motionStatic.opacity), [55]);
  assert.deepEqual(values(layer.motion.opacity.keys[0].v), [90]);
  assert.equal(motion.removeKeyAtCurrentFrame(layer, 'opacity'), undefined);
  ctx.state.currentFrame = 12;
  motion.removeKeyAtCurrentFrame(layer, 'opacity');
  assert.equal(layer.motion.opacity.keys.length, 1);
});

test('real Motion facade writes animated opacity at the playhead and freezes its effective value', () => {
  const ctx = load(3), motion = ctx.SMMotion;
  const holder = { motionStatic: { opacity: [70] } };
  motion.toggleAnimated(holder, 'opacity');
  assert.deepEqual(values(holder.motion.opacity.keys[0].v), [70]);
  assert.equal(holder.motion.opacity.keys[0].frame, 3);
  ctx.state.currentFrame = 9;
  motion.setValue(holder, 'opacity', [25]);
  assert.deepEqual(values(holder.motion.opacity.keys[1].v), [25]);
  motion.toggleAnimated(holder, 'opacity');
  assert.deepEqual(values(holder.motionStatic.opacity), [25]);
  assert.deepEqual(values(holder.motion.opacity.keys), []);
});

test('legacy application interception and non-opacity Motion behavior remain distinct', () => {
  const ctx = load(6), motion = ctx.SMMotion;
  const calls = [];
  ctx.NemoOpacityApplication = { legacy(...args) { calls.push(args); return { legacy: true }; } };
  const opacity = motion.setKeyAtFrame({}, 'opacity', 9, [20], linear());
  assert.deepEqual(JSON.parse(JSON.stringify(opacity)), { legacy: true });
  assert.equal(calls[0][0], 'key-frame');
  assert.equal(calls[0][3], 9);
  assert.deepEqual(JSON.parse(JSON.stringify(calls[0][4])), linear());
  const holder = {};
  const key = motion.setKeyAtFrame(holder, 'rotation', 9, [20], linear());
  assert.equal(key.frame, 9);
  assert.deepEqual(values(motion.rawValueAtFrame(holder, 'rotation', 9)), [20]);
});
