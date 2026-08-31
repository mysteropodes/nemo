// Unit tests for the text range selector (src/js/text-selector.js).
//
// These are behaviour tests, not source-shape tests: the module is
// deliberately free of state/Paper/DOM so it can be required directly and
// exercised on real numbers. The reference values come from lottie-web's
// TextSelectorProperty, which this is a port of — if a formula drifts, a
// Nemo text animation and its Lottie counterpart would silently disagree.
const assert = require('node:assert/strict');
const test = require('node:test');
const path = require('node:path');

const sel = require(path.resolve(__dirname, '../src/js/text-selector.js'));
const { SHAPE } = sel;

function base(over) {
  return Object.assign(sel.defaultSelector(), over || {});
}
const round2 = (a) => a.map((v) => Number(v.toFixed(2)));

test('a full range selects every unit, an empty one selects none', () => {
  const N = 9;
  assert.deepEqual(sel.weights(base({ start: 0, end: 100 }), N), new Array(N).fill(1));
  assert.deepEqual(sel.weights(base({ start: 0, end: 0 }), N), new Array(N).fill(0));
});

test('every weight stays inside 0..1 across all shapes and ranges', () => {
  for (const shape of Object.values(SHAPE)) {
    for (const end of [0, 17, 45, 80, 100]) {
      for (const offset of [-60, 0, 35]) {
        const w = sel.weights(base({ shape, end, offset }), 9);
        for (const v of w) {
          assert.ok(v >= 0 && v <= 1, `shape ${shape} end ${end} offset ${offset} -> ${v}`);
          assert.ok(Number.isFinite(v), `shape ${shape} produced ${v}`);
        }
      }
    }
  }
});

test('square shape gives a hard range with a partial edge unit', () => {
  // 45% of 9 chars = 4.05 units: four full, then a 0.05 sliver, then nothing.
  assert.deepEqual(
    round2(sel.weights(base({ shape: SHAPE.SQUARE, end: 45 }), 9)),
    [1, 1, 1, 1, 0.05, 0, 0, 0, 0],
  );
});

test('triangle peaks at the centre and is symmetric', () => {
  const w = sel.weights(base({ shape: SHAPE.TRIANGLE, start: 0, end: 100 }), 9);
  assert.equal(w.indexOf(Math.max(...w)), 4, 'peak sits on the middle unit');
  assert.equal(w[0].toFixed(4), w[8].toFixed(4));
  assert.equal(w[1].toFixed(4), w[7].toFixed(4));
  assert.equal(Number(w[4].toFixed(4)), 1);
});

test('smooth shape is a cosine bell, also symmetric', () => {
  const w = sel.weights(base({ shape: SHAPE.SMOOTH, start: 0, end: 100 }), 9);
  assert.equal(w[0].toFixed(4), w[8].toFixed(4));
  assert.equal(Number(w[4].toFixed(4)), 1);
  assert.ok(w[2] > w[0] && w[4] > w[2], 'rises towards the centre');
});

test('ramp up and ramp down are mirror images', () => {
  const up = sel.weights(base({ shape: SHAPE.RAMP_UP, start: 0, end: 100 }), 9);
  const down = sel.weights(base({ shape: SHAPE.RAMP_DOWN, start: 0, end: 100 }), 9);
  for (let i = 0; i < up.length; i++) {
    assert.equal(Number((up[i] + down[i]).toFixed(6)), 1, `unit ${i} should sum to 1`);
  }
});

test('offset sweeps the window along the text', () => {
  const at0 = sel.weights(base({ end: 45, offset: 0 }), 9);
  const at50 = sel.weights(base({ end: 45, offset: 50 }), 9);
  assert.notDeepEqual(at0, at50);
  // The window moved right: what was selected at the head no longer is, and
  // units further along now are. This is the whole point of the model — one
  // keyframed value replaces N staggered per-glyph series.
  assert.ok(at0[0] > at50[0]);
  assert.ok(at50[7] > at0[7]);
});

test('a reversed selector (start past end) is swapped, not empty', () => {
  const fwd = sel.weights(base({ start: 20, end: 70 }), 9);
  const rev = sel.weights(base({ start: 70, end: 20 }), 9);
  assert.deepEqual(rev, fwd);
});

test('index units address whole characters instead of percentages', () => {
  const w = sel.weights(base({ units: 'index', start: 0, end: 3 }), 9);
  assert.deepEqual(round2(w), [1, 1, 1, 0, 0, 0, 0, 0, 0]);
});

test('amount scales the whole animator down', () => {
  const full = sel.weights(base({ start: 0, end: 100 }), 6);
  const half = sel.weights(base({ start: 0, end: 100, amount: 50 }), 6);
  for (let i = 0; i < full.length; i++) assert.equal(half[i], full[i] * 0.5);
});

test('smoothness narrows the transition band', () => {
  const soft = sel.weights(base({ shape: SHAPE.RAMP_UP, start: 0, end: 100, smooth: 100 }), 9);
  const hard = sel.weights(base({ shape: SHAPE.RAMP_UP, start: 0, end: 100, smooth: 10 }), 9);
  // A hard band pushes values to the extremes: count how many sit strictly
  // between 0 and 1 — fewer is harder.
  const mid = (a) => a.filter((v) => v > 0.001 && v < 0.999).length;
  assert.ok(mid(hard) < mid(soft), `hard=${mid(hard)} should be fewer than soft=${mid(soft)}`);
});

test('ease high/low bends the weight ramp without leaving 0..1', () => {
  const flat = sel.weights(base({ shape: SHAPE.RAMP_UP, start: 0, end: 100 }), 9);
  const eased = sel.weights(base({ shape: SHAPE.RAMP_UP, start: 0, end: 100, easeHigh: 80 }), 9);
  assert.notDeepEqual(round2(flat), round2(eased));
  for (const v of eased) assert.ok(v >= 0 && v <= 1);
  // The fixed points are the WEIGHTS 0 and 1, not the first/last glyph — a
  // ramp's end units are 0.06/0.94, not 0/1, so they do move. Check the
  // actual invariant instead: fully-out and fully-in units stay put.
  const clamped = sel.weights(base({ shape: SHAPE.SQUARE, start: 0, end: 100, easeHigh: 80 }), 9);
  assert.deepEqual(clamped, new Array(9).fill(1), 'weight 1 must stay 1 under easing');
  const none = sel.weights(base({ shape: SHAPE.SQUARE, start: 0, end: 0, easeHigh: 80 }), 9);
  assert.deepEqual(none, new Array(9).fill(0), 'weight 0 must stay 0 under easing');
});

test('unitIndexOf reads the index the selector is based on', () => {
  const sd = { charIndex: 7, wordIndex: 2, lineIndex: 1 };
  assert.equal(sel.unitIndexOf(sd, 'chars'), 7);
  assert.equal(sel.unitIndexOf(sd, 'words'), 2);
  assert.equal(sel.unitIndexOf(sd, 'lines'), 1);
  // Falls back to the character index rather than dropping the glyph, so a
  // stroke stamped before word/line indices existed still animates.
  assert.equal(sel.unitIndexOf({ charIndex: 4 }, 'words'), 4);
  assert.equal(sel.unitIndexOf(null, 'chars'), null);
});

test('a single-unit block does not divide by zero', () => {
  for (const shape of Object.values(SHAPE)) {
    const w = sel.weights(base({ shape }), 1);
    assert.equal(w.length, 1);
    assert.ok(Number.isFinite(w[0]), `shape ${shape} -> ${w[0]}`);
  }
  assert.deepEqual(sel.weights(base(), 0), []);
});
