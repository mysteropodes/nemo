// @ts-check
// Minimum-cost assignment solver (P23): the Hungarian / Kuhn-Munkres
// algorithm that tween matching (tweens.js autoMatchJS and the relational
// matcher) uses to pair strokes across two keyframes. Moved verbatim from
// tweens.js `hungarian(cost)` — same O(n^3) potentials formulation, same
// INF sentinel, same tie order (rows in index order, the first strictly
// smaller reduced cost wins), so every existing correspondence is preserved.
//
// Contract: `cost` is a square n×n array of finite numbers (n may be 0).
// Returns an array of length n where result[i] is the column assigned to
// row i. The input is only read, never mutated. Rectangular matrices are the
// caller's responsibility (tweens.js pads to a square with a "no match"
// cost before calling); a non-square input is a programming error here, not
// something this module silently repairs.
//
// This is the whole extractable pure kernel. The surrounding matcher
// (feature extraction with Paper geometry, similarity-transform seeding,
// refinement passes) stays in tweens.js; geometry-wasm's tweenmatch.rs is a
// separate port of the same algorithm and no parity is claimed here.
var NemoTweenAssignment = (function () {
  'use strict';

  function solve(cost) {
    var n = cost.length; var INF = 1e9;
    var u = new Array(n + 1).fill(0), v = new Array(n + 1).fill(0);
    var p = new Array(n + 1).fill(0), way = new Array(n + 1).fill(0);
    for (var i = 1; i <= n; i++) {
      p[0] = i; var j0 = 0;
      var minv = new Array(n + 1).fill(INF);
      var used = new Array(n + 1).fill(false);
      do {
        used[j0] = true;
        var i0 = p[j0], delta = INF, j1 = -1;
        for (var j = 1; j <= n; j++) {
          if (!used[j]) {
            var cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
            if (cur < minv[j]) { minv[j] = cur; way[j] = j0; }
            if (minv[j] < delta) { delta = minv[j]; j1 = j; }
          }
        }
        for (var j2 = 0; j2 <= n; j2++) {
          if (used[j2]) { u[p[j2]] += delta; v[j2] -= delta; }
          else minv[j2] -= delta;
        }
        j0 = j1;
      } while (p[j0] !== 0);
      do { var j1b = way[j0]; p[j0] = p[j1b]; j0 = j1b; } while (j0);
    }
    var assign = new Array(n).fill(-1);
    for (var j = 1; j <= n; j++) { if (p[j] > 0) assign[p[j] - 1] = j - 1; }
    return assign;
  }

  return { solve: solve };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoTweenAssignment;
