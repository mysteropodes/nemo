'use strict';
// P23 — pure minimum-cost assignment solver (Hungarian / Kuhn-Munkres).
//
// Three kinds of expectation, none of them derived from the module itself:
//   * optimality against an independent brute-force enumeration of every
//     permutation on small square matrices (including random ones);
//   * fixed tie/trap matrices whose assignments were pinned from the
//     PRE-EXTRACTION production `hungarian(cost)` in tweens.js at
//     f29e7926ddef0a1527e1b36385069865cfd36cb8 — the tie ORDER is part of the
//     behaviour tween correspondence depends on, so it is asserted exactly,
//     not just the optimal cost;
//   * a routing guard over the real tweens.js source: the four production
//     call sites reach the module and the old inline body is gone.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const { solve } = require('../src/js/domain/tween/assignment.js');
const tweensSource = fs.readFileSync(path.join(root, 'src/js/tweens.js'), 'utf8');

function cost(matrix, assign) { return assign.reduce((sum, col, row) => sum + matrix[row][col], 0); }

// Independent oracle: minimum over all n! permutations.
function bruteForceMin(matrix) {
  const n = matrix.length; let best = Infinity;
  (function walk(row, used, acc) {
    if (row === n) { if (acc < best) best = acc; return; }
    for (let col = 0; col < n; col++) if (!used[col]) { used[col] = true; walk(row + 1, used, acc + matrix[row][col]); used[col] = false; }
  })(0, [], 0);
  return best;
}

function isPermutation(assign, n) {
  return assign.length === n && new Set(assign).size === n && assign.every((c) => Number.isInteger(c) && c >= 0 && c < n);
}

// Deterministic LCG so the "random" matrices are the same on every run.
function lcg(seed) { let s = seed >>> 0; return () => { s = (s * 1664525 + 1013904223) >>> 0; return s / 4294967296; }; }

// ---- check 1: empty, 1×1, brute-force-checked small squares ---------------

test('empty and 1×1 matrices', () => {
  assert.deepEqual(solve([]), []);
  assert.deepEqual(solve([[5]]), [0]);
  assert.deepEqual(solve([[-3]]), [0]);
});

test('every 2×2..6×6 matrix tried returns a permutation with the brute-force minimum cost', () => {
  const rand = lcg(20260913);
  let tried = 0;
  for (let n = 2; n <= 6; n++) {
    for (let k = 0; k < 40; k++) {
      const m = Array.from({ length: n }, () => Array.from({ length: n }, () => Math.round(rand() * 20) - 5));
      const a = solve(m);
      assert.ok(isPermutation(a, n), `n=${n} k=${k}: not a permutation ${JSON.stringify(a)}`);
      assert.equal(cost(m, a), bruteForceMin(m), `n=${n} k=${k}: not minimal for ${JSON.stringify(m)}`);
      tried++;
    }
  }
  assert.equal(tried, 200);
});

test('hand-picked structures: greedy trap, mirrored eyes, padded rectangle, negatives', () => {
  // The greedy "cheapest pair first" rule this solver replaced takes (0,0)=1 then (1,2)=1 then (2,1)=1 — total 3 —
  // which happens to be optimal here; the trap is the eyes matrix, where greedy would lock A->A (10) after seeing 1s.
  for (const m of [[[1, 2, 100], [2, 100, 1], [100, 1, 2]], [[10, 1, 50, 50], [1, 10, 50, 50], [50, 50, 10, 1], [50, 50, 1, 10]],
    [[3, 7, 1e6], [5, 2, 1e6], [1e6, 1e6, 1e6]], [[-1, -2, -3], [-3, -1, -2], [-2, -3, -1]]]) {
    const a = solve(m);
    assert.ok(isPermutation(a, m.length));
    assert.equal(cost(m, a), bruteForceMin(m));
  }
});

// ---- check 2: fixed tie matrices keep the current tie order; input untouched -

test('tie matrices resolve in the exact order the pre-extraction production solver used', () => {
  // Pinned from `hungarian(cost)` in tweens.js at f29e7926ddef0a1527e1b36385069865cfd36cb8.
  const pinned = {
    tie2: [[[1, 1], [1, 1]], [0, 1]],
    tie3: [[[0, 0, 0], [0, 0, 0], [0, 0, 0]], [0, 1, 2]],
    tieDiag: [[[1, 2, 2], [2, 1, 2], [2, 2, 1]], [0, 1, 2]],
    antiDiag: [[[3, 2, 1], [2, 1, 3], [1, 3, 2]], [2, 1, 0]],
    eyes: [[[10, 1, 50, 50], [1, 10, 50, 50], [50, 50, 10, 1], [50, 50, 1, 10]], [1, 0, 3, 2]],
    greedyTrap: [[[1, 2, 100], [2, 100, 1], [100, 1, 2]], [0, 2, 1]],
    padded: [[[3, 7, 1e6], [5, 2, 1e6], [1e6, 1e6, 1e6]], [0, 1, 2]],
    negatives: [[[-1, -2, -3], [-3, -1, -2], [-2, -3, -1]], [2, 0, 1]],
    fours: [[[4, 1, 3, 2], [2, 0, 5, 3], [3, 2, 2, 1], [1, 3, 4, 0]], [2, 1, 3, 0]],
  };
  for (const name of Object.keys(pinned)) {
    const [m, expected] = pinned[name];
    assert.deepEqual(solve(m), expected, name);
  }
});

test('the input matrix is never mutated and the result is a fresh array', () => {
  const m = [[4, 1, 3, 2], [2, 0, 5, 3], [3, 2, 2, 1], [1, 3, 4, 0]];
  const snapshot = JSON.stringify(m);
  const a = solve(m);
  assert.equal(JSON.stringify(m), snapshot);
  assert.notEqual(solve(m), a);
  assert.deepEqual(solve(m), a, 'deterministic');
});

// ---- check 3: production matching invokes the module -----------------------

test('routing guard: the four tweens.js matchers call NemoTweenAssignment.solve and the inline body is gone', () => {
  const calls = tweensSource.match(/NemoTweenAssignment\.solve\(/g) || [];
  assert.equal(calls.length, 4, 'autoMatchJS cost pass, refinement pass, relational augmentation loop and the second matcher');
  assert.doesNotMatch(tweensSource, /function hungarian\(/, 'the facade is retired, not kept as a second copy');
  assert.doesNotMatch(tweensSource, /\bhungarian\(/, 'no call site still reaches a removed function');
  // The reference comment to geometry-wasm's port stays with the matcher; the algorithm itself does not.
  assert.doesNotMatch(tweensSource, /var minv=new Array\(n\+1\)\.fill\(INF\)/, 'the O(n^3) body must not come back inline');
});
