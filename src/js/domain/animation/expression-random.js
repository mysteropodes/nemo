// @ts-check
// Pure math for Motion expression randomness: the deterministic hash, the
// uniform-draw formula, the uniform-pair-to-Gaussian transform, and the
// scalar/array range application shared by all four random draw functions.
// Callers own the mutable evaluation context (_ectx in motion.js), the
// per-call watchdog tick and the counter increment discipline (one _rand01
// call = one counter step; a Gaussian draw is two _rand01 calls, so it
// already advances the counter twice through the ordinary path).
var NemoExpressionRandomDomain = (function () {
  'use strict';

  // Tiny deterministic hash (not cryptographic, doesn't need to be) - the
  // same value for the same (seed, n) every time.
  function hashUnit(seed, n) {
    var v = Math.sin(n * 12.9898 + seed * 78.233) * 43758.5453;
    return v - Math.floor(v);
  }

  // Pure equivalent of the hash half of motion.js's _rand01: given the ctx
  // fields it would have read (seed, the counter value BEFORE increment,
  // frame, the timeless flag) and the fixed/varying draw kind, returns the
  // drawn value in 0..1.
  function rand01FromInputs(seed, counter, frame, timeless, fixed) {
    var tPart = (fixed || timeless) ? 0 : Math.round(frame * 1000) / 1000;
    return hashUnit(seed, tPart * 1013.13 + counter * 7919 + 0.5);
  }

  // Box-Muller transform from two ALREADY-DRAWN uniforms to one Gaussian,
  // centred on 0.5 with a spread that keeps roughly nine draws in ten inside
  // 0..1. Does not draw anything itself - the caller supplies u1/u2 (each
  // its own _rand01 call, preserving the exact existing tick/counter
  // timing of two separate draws rather than folding them into one).
  function gaussFromUniforms(u1, u2) {
    return 0.5 + 0.304 * Math.sqrt(-2 * Math.log(Math.max(1e-9, u1))) * Math.cos(2 * Math.PI * u2);
  }

  // Shared by all four draw functions: no args = the raw 0..1 draw, one arg
  // = 0..max, two = min..max, and either bound may be an array for a
  // per-axis range. `gen` is a zero-arg draw function the caller supplies.
  function randomWith(gen, a, b) {
    function num(v) { var n = Number(v); return isFinite(n) ? n : 0; }
    function vec(v) { return Array.isArray(v) ? v : [num(v)]; }
    if (a === undefined) return gen();
    if (b === undefined) {
      if (Array.isArray(a)) { var o = []; for (var i = 0; i < a.length; i++) o.push(gen() * num(a[i])); return o; }
      return gen() * num(a);
    }
    if (Array.isArray(a) || Array.isArray(b)) {
      var A = vec(a), B = vec(b), n = Math.max(A.length, B.length), out = [];
      for (var j = 0; j < n; j++) { var lo = num(A[j]), hi = num(B[j]); out.push(lo + gen() * (hi - lo)); }
      return out;
    }
    var l = num(a), h = num(b);
    return l + gen() * (h - l);
  }

  return { hashUnit: hashUnit, rand01FromInputs: rand01FromInputs, gaussFromUniforms: gaussFromUniforms, randomWith: randomWith };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoExpressionRandomDomain;
