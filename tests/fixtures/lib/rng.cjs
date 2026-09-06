'use strict';
// Deterministic PRNG for fixture and workload generation (R03). mulberry32:
// 32-bit state, no dependencies, identical sequence on every platform for a
// given seed. Every generated document records the seed that produced it.
function mulberry32(seed) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6D2B79F5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return {
    seed,
    next,
    // Uniform float in [lo, hi), rounded to 3 decimals so the generated JSON
    // is stable and readable.
    float(lo, hi, decimals = 3) { const v = lo + (hi - lo) * next(); const m = 10 ** decimals; return Math.round(v * m) / m; },
    int(lo, hi) { return lo + Math.floor(next() * (hi - lo + 1)); },
    pick(arr) { return arr[Math.floor(next() * arr.length)]; },
  };
}

// FNV-1a style string hash → 32-bit seed, so sub-generators can derive a
// stable seed from a fixture id without sharing one global RNG stream.
function seedFrom(text, base = 0x811C9DC5) {
  let h = base >>> 0;
  for (let i = 0; i < text.length; i++) { h ^= text.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h >>> 0;
}

module.exports = { mulberry32, seedFrom };
