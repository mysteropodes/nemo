// ---- TEXT RANGE SELECTOR (2026-08-31) ----
// The weighting half of a text animator, kept deliberately free of state/
// Paper/DOM so it can be unit-tested on its own (tests/text-selector.test.cjs)
// and called from the render path without dragging anything else in.
//
// WHY THIS EXISTS AT ALL — the problem it replaces:
// text-animator.js bakes N staggered keyframe series, one per glyph, keyed by
// strokeId. Measured 2026-08-31: typing a single extra character re-runs
// applyTextPropsEdit -> buildVectorTextGroup, every glyph is rebuilt and gets
// a NEW strokeId, and every baked series is instantly orphaned — the data is
// still in the file, no glyph reads it any more. Re-timing meant re-baking.
//
// After Effects, Lottie and Rive all answer this the same way, and it is the
// only answer that survives editing the text: the animator lives on the LAYER,
// and a "range selector" computes a 0..1 weight per character INDEX on the fly.
// Each animated property is multiplied by that weight. There is no per-glyph
// identity anywhere, so there is nothing to re-associate when the text changes
// — adding a letter just shifts the indices and the selector recomputes.
//
// The formulas below are a faithful port of lottie-web's
// player/js/utils/text/TextSelectorProperty.js (getValue + getMult), which is
// the reference implementation every Lottie player agrees with. Ported rather
// than invented so that a Nemo text animation and its Lottie export can agree
// on what "Ramp Up at 40% with ease high 30" actually looks like.
(function () {
  // Shape ids match Lottie's `sh` field exactly — same reason as above, and it
  // keeps a future Lottie import/export a straight field copy.
  var SHAPE = { SQUARE: 1, RAMP_UP: 2, RAMP_DOWN: 3, TRIANGLE: 4, ROUND: 5, SMOOTH: 6 };

  var DEFAULT_SELECTOR = {
    selectorType: 'range', // 'range' | 'wiggly' — see weightAt's own comment
    start: 0,          // %  (or unit index when units === 'index')
    end: 100,          // %
    offset: 0,         // %  — animate THIS alone and the effect sweeps
    units: 'percent',  // 'percent' | 'index'
    basedOn: 'chars',  // 'chars' | 'words' | 'lines'
    shape: SHAPE.SQUARE,
    amount: 100,       // % — global multiplier on the whole animator
    easeHigh: 0,       // -100..100
    easeLow: 0,        // -100..100
    smooth: 100,       // %
    // Wiggly Selector fields (AE: Add > Selector > Wiggly) — only read when
    // selectorType === 'wiggly'. wiggleMin/wiggleMax bracket the weight as a
    // percentage the same way `amount` does for the range selector; 2 and 50
    // are AE's own defaults for wigglesPerSec/correlation.
    wiggleMin: -50,
    wiggleMax: 50,
    wigglesPerSec: 2,
    correlation: 50,   // 0 = every unit wiggles independently, 100 = in unison
    seed: 1,
  };

  // Cubic-bezier solver, same role as lottie-web's BezierFactory. Bisection
  // rather than Newton: 24 halvings is exact to ~6e-8 on [0,1], the input is
  // always in range here (mult is clamped before it arrives), and it cannot
  // diverge the way Newton does on the near-vertical segments an aggressive
  // ease high/low produces.
  function bezierEaser(x1, y1, x2, y2) {
    if (x1 === 0 && y1 === 0 && x2 === 1 && y2 === 1) return function (t) { return t; };
    function calcX(t) { var u = 1 - t; return 3 * u * u * t * x1 + 3 * u * t * t * x2 + t * t * t; }
    function calcY(t) { var u = 1 - t; return 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t; }
    return function (x) {
      var lo = 0, hi = 1, t = x;
      for (var i = 0; i < 24; i++) {
        var cx = calcX(t);
        // 1e-9 for the same reason camera.js's Newton exit was tightened:
        // the test is on x, the returned y is off by that times the slope.
        // Pure bisection halves the interval every step regardless, so this
        // only removes an early exit — never adds iterations.
        if (Math.abs(cx - x) < 1e-9) break;
        if (cx < x) lo = t; else hi = t;
        t = (lo + hi) / 2;
      }
      return calcY(t);
    };
  }

  // lottie-web's getValue(): normalise start/end/offset into absolute unit
  // positions. The divisor is what lets the same selector be addressed either
  // as "the first 30 percent" or as "the first 3 characters".
  function resolveRange(sel, totalUnits) {
    var divisor = (sel.units === 'index') ? 1 : (100 / Math.max(1, totalUnits));
    var o = (sel.offset || 0) / divisor;
    var s = (sel.start || 0) / divisor + o;
    var e = ((sel.end == null ? 100 : sel.end)) / divisor + o;
    // A selector whose end precedes its start is not an error — dragging Start
    // past End is a normal gesture, and AE/Lottie both just swap them.
    if (s > e) { var t = s; s = e; e = t; }
    return { s: s, e: e };
  }

  // Deterministic pseudo-random in [0,1) from two integers — same hash shape
  // as the sin-based hashes buildBrushDabs/seededRng (tools.js) already use
  // elsewhere in this codebase, so a wiggle and a brush texture read equally
  // reproducibly from the same seed philosophy.
  function hashRand(seed, i) {
    var x = Math.sin(seed * 12.9898 + i * 78.233) * 43758.5453;
    return x - Math.floor(x);
  }
  // 1D value noise (smoothstep-interpolated random keys at integer `t`) —
  // organic, continuous wiggle rather than a fresh random value every frame,
  // the same character AE's own Perlin-based wiggle() has. Range [-1, 1].
  function smoothNoise1D(seed, t) {
    var i0 = Math.floor(t), i1 = i0 + 1, f = t - i0;
    var v0 = hashRand(seed, i0) * 2 - 1, v1 = hashRand(seed, i1) * 2 - 1;
    var u = f * f * (3 - 2 * f); // smoothstep
    return v0 + (v1 - v0) * u;
  }
  // Wiggly Selector (AE: Add > Selector > Wiggly) — a per-unit weight that
  // oscillates over TIME instead of reading a fixed start/end range. Each
  // unit gets its own noise phase (so characters don't all wiggle in lock-
  // step) blended against one shared, unit-independent noise track by
  // `correlation` — 0% is every character wiggling on its own, 100% is the
  // whole block moving together, exactly AE's own definition of the field.
  function wigglyWeightAt(sel, ind, timeSec) {
    var seed = sel.seed || 1;
    var hz = sel.wigglesPerSec == null ? 2 : sel.wigglesPerSec;
    var t = (timeSec || 0) * hz;
    // A large, index-derived phase offset decorrelates neighbouring units'
    // noise tracks — without it two adjacent characters would sample nearly
    // the same point on the curve and wiggle almost identically regardless
    // of `correlation`.
    var localT = t + ind * 37.13;
    var local = smoothNoise1D(seed, localT);
    var global = smoothNoise1D(seed + 9973, t); // distinct seed offset so global != local's own ind=0 case
    var corr = Math.max(0, Math.min(100, sel.correlation == null ? 50 : sel.correlation)) / 100;
    var n = local + (global - local) * corr; // -1..1
    var lo = sel.wiggleMin == null ? -50 : sel.wiggleMin, hi = sel.wiggleMax == null ? 50 : sel.wiggleMax;
    var pct = lo + (n * 0.5 + 0.5) * (hi - lo); // -1..1 -> lo..hi, same % convention as `amount`
    // NOT clamped to 0..1 — same contract the range selector's own
    // `mult * amt` return already has (a negative `amount` there gives a
    // negative weight too): wiggleMin defaults negative by design, exactly
    // like AE's Min Amount, so the property should invert on the low side
    // of the wiggle rather than floor at zero.
    return pct / 100;
  }

  // lottie-web's getMult(ind): the 0..1 weight for one unit index.
  function weightAt(sel, ind, totalUnits, timeSec) {
    if (sel.selectorType === 'wiggly') return wigglyWeightAt(sel, ind, timeSec);
    var r = resolveRange(sel, totalUnits);
    var s = r.s, e = r.e, tot = e - s;
    var shape = sel.shape || SHAPE.SQUARE;

    // Ease high/low build the bezier that is applied to the WEIGHT — this is
    // easing along the word, not along time, and it is what separates a
    // mechanical stagger from one that feels authored.
    var x1 = 0, y1 = 0, x2 = 1, y2 = 1;
    var ne = sel.easeLow || 0, xe = sel.easeHigh || 0;
    if (ne > 0) x1 = ne / 100; else y1 = -ne / 100;
    if (xe > 0) x2 = 1 - xe / 100; else y2 = 1 + xe / 100;
    var easer = bezierEaser(x1, y1, x2, y2);

    var mult = 0, i2 = ind;
    if (shape === SHAPE.RAMP_UP) {
      mult = (e === s) ? (ind >= e ? 1 : 0)
        : Math.max(0, Math.min(0.5 / (e - s) + (ind - s) / (e - s), 1));
    } else if (shape === SHAPE.RAMP_DOWN) {
      mult = (e === s) ? (ind >= e ? 0 : 1)
        : 1 - Math.max(0, Math.min(0.5 / (e - s) + (ind - s) / (e - s), 1));
    } else if (shape === SHAPE.TRIANGLE) {
      if (e !== s) {
        mult = Math.max(0, Math.min(0.5 / (e - s) + (ind - s) / (e - s), 1));
        mult = mult < 0.5 ? mult * 2 : 1 - 2 * (mult - 0.5);
      }
    } else if (shape === SHAPE.ROUND) {
      if (e !== s) {
        i2 = Math.min(Math.max(0, ind + 0.5 - s), tot);
        var x = -tot / 2 + i2, a = tot / 2;
        mult = a === 0 ? 0 : Math.sqrt(Math.max(0, 1 - (x * x) / (a * a)));
      }
    } else if (shape === SHAPE.SMOOTH) {
      if (e !== s) {
        i2 = Math.min(Math.max(0, ind + 0.5 - s), tot);
        mult = (1 + Math.cos(Math.PI + Math.PI * 2 * i2 / (e - s))) / 2;
      }
    } else { // SQUARE — the base case: a hard range with partial edge units
      if (ind >= Math.floor(s)) {
        mult = (ind - s < 0)
          ? Math.max(0, Math.min(Math.min(e, 1) - (s - ind), 1))
          : Math.max(0, Math.min(e - ind, 1));
      }
    }

    mult = easer(Math.max(0, Math.min(1, mult)));

    // Smoothness narrows the useful band: at 25% the weight only varies over
    // [0.375, 0.625] and saturates outside it, which hardens the boundary
    // between selected and unselected units.
    var sm = (sel.smooth == null ? 100 : sel.smooth);
    if (sm !== 100) {
      var smooth = sm * 0.01 || 1e-8;
      var threshold = 0.5 - smooth * 0.5;
      mult = mult < threshold ? 0 : Math.min(1, (mult - threshold) / smooth);
    }

    var amt = (sel.amount == null ? 100 : sel.amount) / 100;
    return mult * amt;
  }

  function weights(sel, totalUnits, timeSec) {
    var out = [];
    for (var i = 0; i < totalUnits; i++) out.push(weightAt(sel, i, totalUnits, timeSec));
    return out;
  }

  // Which index a glyph counts as, for a selector working on words or lines.
  // vector-text-bridge.js already stamps all three at build time (and serP/
  // desP already round-trip them — verified 2026-08-31), so this is a lookup,
  // not a computation.
  function unitIndexOf(sd, basedOn) {
    if (!sd) return null;
    if (basedOn === 'words') return sd.wordIndex != null ? sd.wordIndex : sd.charIndex;
    if (basedOn === 'lines') return sd.lineIndex != null ? sd.lineIndex : sd.charIndex;
    return sd.charIndex;
  }

  var API = {
    SHAPE: SHAPE,
    DEFAULT_SELECTOR: DEFAULT_SELECTOR,
    defaultSelector: function () { var o = {}; for (var k in DEFAULT_SELECTOR) o[k] = DEFAULT_SELECTOR[k]; return o; },
    resolveRange: resolveRange,
    weightAt: weightAt,
    weights: weights,
    unitIndexOf: unitIndexOf,
  };

  if (typeof window !== 'undefined') window.SMTextSelector = API;
  if (typeof module !== 'undefined' && module.exports) module.exports = API;
})();
