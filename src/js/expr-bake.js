// ---- BAKE EXPRESSION TO KEYFRAMES (2026-09-01) ----
// Cyril: "mettre en place une fonctionnalité pour bake les expressions...
// j'aimerais un baking optimisé, que ça créer des keyframes optimale si
// c'est possible (exemple position si même valeur sur plusieurs keyframes
// alors on a pas besoin de ces keyframes, ou si on peut le reproduire en
// lissage/courbes/motion path avec moins de keyframes on le fait)."
//
// The obvious implementation — one keyframe per frame — is exactly what
// this was asked NOT to build. Two passes instead, both built on the same
// primitive (Ramer-Douglas-Peucker: keep only the points a straight chord
// between two neighbours can't already reproduce within a tolerance):
//
//   PASS A — WHERE keyframes land. Run RDP per axis on the raw sampled
//   (frame, value) curve, tolerance in the property's OWN unit. A flat run
//   collapses to its two endpoints (zero interior points survive — a
//   perfectly constant expression collapses the WHOLE range to nothing,
//   see the motionStatic special case below); a straight ramp collapses to
//   start+end; only genuine direction changes (bounce peaks, wiggle
//   extrema) force an interior keyframe. A 2D property (Position/Scale)
//   unions both axes' own keyframe times — X and Y each get to demand a
//   keyframe only where THEY actually need one.
//
//   PASS B — HOW each resulting segment eases. Nemo's key.curvePoints is a
//   normalized [0,1]-in-time -> eased-progress polyline (motion.js's
//   evalCurvePoints), and critically its own math never clamps the OUTPUT
//   — only the INPUT (evalCurvePoints: `x = Math.max(0, Math.min(1, x))`,
//   the y it returns is whatever the fitted cubic gives, which CAN exceed
//   1 or go below 0). That means a single 2-keyframe segment can already
//   express an overshoot/settle shape entirely through the curve, with no
//   extra keyframe at the peak — genuinely "reproduce with fewer
//   keyframes via the curve" rather than a cosmetic smoothing pass. Same
//   RDP primitive, run again on the segment's own (progress, value-
//   fraction) samples. One real constraint found reading the evaluator
//   (motion.js's evalTrack/rawValueAtFrame): curvePoints is ONE shared
//   curve per segment, applied identically to every axis of a multi-
//   dimensional property — there's no per-axis easing. The axis with the
//   larger |delta| is fit as the "driver"; a co-occurring near-flat axis
//   is insensitive to whatever curve wins (its own delta is ~0, so
//   `a+delta*y ~= a` for any y), which is what makes sharing safe here.
//
//   PASS C — MERGE adjacent Pass-A segments a curve can already cover.
//   Pass A alone only ever compares a sample against a STRAIGHT chord, so
//   on a smoothly curving signal (a sine, an ease, a slow pendulum — not
//   noise) it over-selects: confirmed live, one full sine cycle over 24
//   frames baked to 22 keyframes out of 24 sampled, because straight-line
//   RDP needs a point almost every frame to hug a curve tolerance never
//   asked it to hug in the first place. Pass B alone can't fix this either
//   — it only shapes a segment Pass A already fixed the ends of, it can't
//   widen it. So after Pass A produces candidate breakpoints, greedily
//   extend each segment across as many of them as a SINGLE fitted (or
//   plain linear) curve can still reconstruct — verified against every
//   original sample, on every axis, using SM.evalCurvePoints (the exact
//   function motion.js's own rawValueAtFrame renders with, not a second
//   hand-ported copy). This is what actually delivers "reproduce with
//   fewer keyframes via the curve" for anything wider than one Pass-A
//   segment, rather than only within spans Pass A already happened to fix.
//
// Deliberately NOT attempted here: fitting Position's own spatial
// hOut/hIn bezier HANDLES (a true curved motion PATH, not just eased
// timing along a straight line — motion.js's rawValueAtFrame has a
// separate branch for it). That's a genuinely harder 2D curve-fit problem
// (fitting a cubic to a point CLOUD, not a monotonic 1D progress curve)
// and Cyril named it explicitly ("motion path") — worth a real follow-up,
// not a rushed add to this same pass. Positions still reduce to fewer
// keyframes via Pass A/B alone; they just travel in straight segments
// between them rather than curved ones, same as today's Bounce/Overshoot
// examples already do (the examples library ports the SAME limitation).
(function () {
  // ---- RDP on a single axis: keep only points a straight chord between
  // its neighbours can't already reproduce within `epsilon`. ------------
  // `pts`: [{x, y}], sorted by x. Distance measured on y ONLY (vertical
  // distance from the chord at that x) — NOT full 2D Euclidean distance,
  // deliberately: x here is either a FRAME NUMBER or a normalized time
  // fraction, a different kind of axis than y (a spatial/rotation/opacity
  // value or a value-fraction), so a geometric-distance metric would let
  // whichever axis happens to have the larger absolute scale dominate the
  // point selection for no principled reason. This is the standard
  // "keyframe/curve reduction" variant (Maya/Blender's own filters work
  // the same way), not the GPS-track variant most RDP writeups describe.
  // A fresh [{x:0,y:0},{x:1,y:1}] each call — motion.js's own CURVE_LINEAR
  // is a single shared constant that every one of its call sites clones
  // before attaching to a key for exactly this reason (setKeyAtFrame keeps
  // whatever reference it's handed, uncloned).
  function linearCurve() { return [{ x: 0, y: 0 }, { x: 1, y: 1 }]; }

  function rdp(pts, epsilon) {
    if (pts.length < 3) return pts.slice();
    var first = pts[0], last = pts[pts.length - 1];
    var span = last.x - first.x;
    var dmax = -1, idx = 0;
    for (var i = 1; i < pts.length - 1; i++) {
      var t = span > 1e-9 ? (pts[i].x - first.x) / span : 0;
      var interp = first.y + (last.y - first.y) * t;
      var d = Math.abs(pts[i].y - interp);
      if (d > dmax) { dmax = d; idx = i; }
    }
    if (dmax > epsilon) {
      var left = rdp(pts.slice(0, idx + 1), epsilon);
      var right = rdp(pts.slice(idx), epsilon);
      return left.slice(0, -1).concat(right);
    }
    return [first, last];
  }

  // ---- Pass A: per-axis keyframe placement, unioned across axes -------
  // `axisSamples`: one array per axis, each [{frame, value}], same frames.
  // `epsilons`: one tolerance per axis, in that axis's own unit.
  // Returns the sorted, deduplicated union of frame numbers every axis's
  // own RDP pass actually needed.
  function unionKeyframeTimes(axisSamples, epsilons) {
    var frameSet = {};
    axisSamples.forEach(function (samples, d) {
      var pts = samples.map(function (s) { return { x: s.frame, y: s.value }; });
      var reduced = rdp(pts, epsilons[d]);
      reduced.forEach(function (p) { frameSet[p.x] = true; });
    });
    return Object.keys(frameSet).map(Number).sort(function (a, b) { return a - b; });
  }

  // ---- Pass B: fit ONE shared curvePoints for a segment ----------------
  // `segSamplesPerAxis`: per-axis [{frame, value}] restricted to
  // [frameA..frameB] inclusive. `vA`/`vB`: per-axis endpoint values (the
  // values actually being keyed — the fit must pass through them exactly,
  // which is why driverSamples always includes both segment endpoints).
  // Returns null for "plain linear is fine" (the common case — most
  // segments straddle only genuine direction changes by the time Pass A
  // is done, so there is rarely anything left for the curve to earn its
  // keep on) or a curvePoints array otherwise.
  function fitSegmentCurve(segSamplesPerAxis, vA, vB, fracEpsilon) {
    // Pick the driver axis: the one with the largest |delta| — see the
    // header comment for why a co-occurring near-flat axis doesn't need
    // its own curve (any shared y leaves it at ~vA regardless).
    var driver = -1, driverDelta = 0;
    for (var d = 0; d < vA.length; d++) {
      var delta = Math.abs(vB[d] - vA[d]);
      if (delta > driverDelta) { driverDelta = delta; driver = d; }
    }
    if (driver < 0 || segSamplesPerAxis[driver].length < 3) return null; // flat or nothing between the endpoints
    var samples = segSamplesPerAxis[driver];
    var frameA = samples[0].frame, frameB = samples[samples.length - 1].frame;
    var span = frameB - frameA;
    if (span <= 0) return null;
    var pts = samples.map(function (s) {
      return { x: (s.frame - frameA) / span, y: (s.value - vA[driver]) / (vB[driver] - vA[driver]) };
    });
    var reduced = rdp(pts, fracEpsilon);
    if (reduced.length <= 2) return null; // a straight chord already fits — plain linear, no curve needed
    // Guard against a pathological blow-up (a very noisy/high-frequency
    // driver axis could in principle need many points) — cap it and fall
    // back to linear rather than hand back a curve nobody could edit by
    // hand afterward. 24 points is generous for anything a real ease
    // shape needs; hitting the cap means this segment should have been
    // split further by Pass A instead, not smoothed over.
    if (reduced.length > 24) return null;
    return reduced.map(function (p) { return { x: p.x, y: p.y }; });
  }

  // ---- orchestration -----------------------------------------------------
  // `holder`: a layer or element-motion holder (same object valueAtFrame
  // etc. already take). `prop`: the property name. `frameStart`/`frameEnd`:
  // inclusive sample range. `toleranceFrac`: how much the baked result may
  // deviate from the live expression, as a FRACTION of that property's own
  // sampled value range on this holder (0.005 = 0.5%, self-calibrating
  // across pixels/degrees/percent without per-property-type tuning).
  function bakeExpressionToKeyframes(holder, prop, frameStart, frameEnd, toleranceFrac) {
    if (!window.SMMotion || !holder || !prop) return null;
    var SM = window.SMMotion;
    var ex = holder.expressions && holder.expressions[prop];
    if (!ex || !ex.enabled || !ex.code) return null;
    frameStart = Math.floor(frameStart); frameEnd = Math.ceil(frameEnd);
    if (frameEnd <= frameStart) frameEnd = frameStart + 1;
    var dim = SM.propDim(prop);
    toleranceFrac = (toleranceFrac == null) ? 0.005 : toleranceFrac;

    // Sample every frame in range — this IS the "naive" bake, kept only as
    // the raw material the two RDP passes above then reduce.
    var frames = [];
    var axisRaw = []; for (var d0 = 0; d0 < dim; d0++) axisRaw.push([]);
    var mins = new Array(dim).fill(Infinity), maxs = new Array(dim).fill(-Infinity);
    for (var f = frameStart; f <= frameEnd; f++) {
      // SM.valueAtFrame ALWAYS returns an array, even for a 1D property
      // (rawValueAtFrame stores every track as v:[n], and
      // normalizeExprResult's `fill(n)` wraps a bare-number expression
      // result the same way) — confirmed live: wrapping it again here
      // produced keys shaped [[172.5]] instead of [172.5], and
      // rawValueAtFrame's `a.v[d] + (b.v[d]-a.v[d])*y` then hit a nested
      // array on the `+`, which JS silently string-concatenates instead of
      // adding (comparisons like `<` happened to still work by accident,
      // via a single-element array's ToNumber coercion, which is why the
      // corruption didn't show up until interpolating BETWEEN keyframes).
      var arr = SM.valueAtFrame(holder, prop, f);
      frames.push(f);
      for (var d1 = 0; d1 < dim; d1++) {
        var val = arr[d1];
        axisRaw[d1].push({ frame: f, value: val });
        if (val < mins[d1]) mins[d1] = val;
        if (val > maxs[d1]) maxs[d1] = val;
      }
    }
    var sampledFrameCount = frames.length;
    var epsilons = [];
    for (var d2 = 0; d2 < dim; d2++) {
      var range = maxs[d2] - mins[d2];
      epsilons.push(Math.max(range * toleranceFrac, 1e-4));
    }

    var keyTimes = unionKeyframeTimes(axisRaw, epsilons);

    // Degenerate case: the expression is (near-)constant over the whole
    // range on every axis — Pass A's own RDP already reduces this to just
    // [frameStart, frameEnd], but a constant needs NO keyframes at all,
    // just the static value AE/Nemo already have a slot for. This is the
    // literal form of "même valeur sur plusieurs keyframes -> pas besoin
    // de ces keyframes", carried to its logical end.
    var isConstant = keyTimes.length <= 2 && (function () {
      for (var d = 0; d < dim; d++) if (maxs[d] - mins[d] > epsilons[d] * 2) return false;
      return true;
    })();

    function sampleAt(frame, axisIdx) {
      // keyTimes are always a subset of the sampled frames (Pass A only
      // ever selects from its own input points), so an exact lookup is
      // always available — no re-evaluation of the expression needed.
      var arr = axisRaw[axisIdx];
      // frames are contiguous [frameStart..frameEnd], so this is O(1).
      return arr[frame - frameStart].value;
    }

    if (window.pushUndo) pushUndo();

    if (isConstant) {
      var flatVal = [];
      for (var d3 = 0; d3 < dim; d3++) flatVal.push(sampleAt(frameStart, d3));
      if (SM.isAnimated && SM.isAnimated(holder, prop) && SM.toggleAnimated) SM.toggleAnimated(holder, prop);
      if (!holder.motionStatic) holder.motionStatic = {};
      holder.motionStatic[prop] = flatVal;
      ex.enabled = false;
      return { keyframeCount: 0, sampledFrameCount: sampledFrameCount, curvedSegments: 0, constant: true };
    }

    // Pass C: greedily extend each Pass-A segment across as many further
    // candidate breakpoints as a single fitted-or-linear curve can still
    // reconstruct within tolerance, verified sample-by-sample with the
    // SAME evaluator that will actually render it.
    function reconstructAt(aFrame, bFrame, curvePoints, vA, vB, frame) {
      var span = bFrame - aFrame;
      var t = span > 1e-9 ? (frame - aFrame) / span : 0;
      var y = curvePoints ? SM.evalCurvePoints(curvePoints, t) : t;
      var out = [];
      for (var d = 0; d < vA.length; d++) out.push(vA[d] + (vB[d] - vA[d]) * y);
      return out;
    }
    // Returns { curvePoints } on success (curvePoints may legitimately be
    // null, meaning "linear already fits") or null on failure — always an
    // object on success so the two outcomes can never be confused.
    function tryFitSpan(aFrame, bFrame) {
      var vA = [], vB = [], segSamplesPerAxis = [];
      for (var d = 0; d < dim; d++) {
        vA.push(sampleAt(aFrame, d)); vB.push(sampleAt(bFrame, d));
        segSamplesPerAxis.push(axisRaw[d].slice(aFrame - frameStart, bFrame - frameStart + 1));
      }
      // Normalized-space tolerance for curve SHAPE quality only — the real
      // correctness gate is the absolute per-sample check just below, so
      // this doesn't need per-property tuning, just to not be so loose the
      // fitted curve is visibly sloppy.
      var curvePoints = fitSegmentCurve(segSamplesPerAxis, vA, vB, toleranceFrac * 2);
      for (var f = aFrame; f <= bFrame; f++) {
        var recon = reconstructAt(aFrame, bFrame, curvePoints, vA, vB, f);
        for (var d2 = 0; d2 < dim; d2++) {
          if (Math.abs(recon[d2] - sampleAt(f, d2)) > epsilons[d2]) return null;
        }
      }
      return { curvePoints: curvePoints };
    }

    var segments = []; // {startFrame, endFrame, curvePoints}
    var i = 0;
    while (i < keyTimes.length - 1) {
      var startFrame = keyTimes[i];
      // The immediate next candidate is Pass A's own minimal segment — by
      // construction (straight-line RDP already passed it) this always
      // succeeds, but don't assume it: fitSegmentCurve's driver-axis
      // reduction is a different (curve) fit than the line RDP used to
      // place these breakpoints, so verify like every other candidate.
      var bestJ = i + 1;
      var bestFit = tryFitSpan(startFrame, keyTimes[bestJ]);
      if (!bestFit) bestFit = { curvePoints: null };
      var j = i + 2;
      while (j < keyTimes.length) {
        var fit = tryFitSpan(startFrame, keyTimes[j]);
        if (!fit) break;
        bestJ = j; bestFit = fit;
        j++;
      }
      segments.push({ startFrame: startFrame, endFrame: keyTimes[bestJ], curvePoints: bestFit.curvePoints });
      i = bestJ;
    }

    // Reset the track, then write the merged keyframe set. Any keys
    // outside [frameStart, frameEnd] are intentionally dropped — the bake
    // replaces the expression as the SOLE source of truth for this
    // property over the range it was asked to cover.
    if (!holder.motion) holder.motion = {};
    holder.motion[prop] = { keys: [] };
    var curvedSegments = 0;
    for (var s = 0; s < segments.length; s++) {
      var seg = segments[s];
      var kv = []; for (var d3 = 0; d3 < dim; d3++) kv.push(sampleAt(seg.startFrame, d3));
      if (seg.curvePoints) curvedSegments++;
      // setKeyAtFrame's OWN fallback for an omitted curvePoints is Nemo's
      // opinionated ease-in/ease-out DEFAULT_CURVE, not a straight line —
      // confirmed live: leaving this undefined on a linear segment put an
      // S-curve between two keyframes meant to reproduce a straight ramp,
      // so frame 12 of a 24-frame linear rotation baked to ~91.5° instead
      // of the correct 90°. "null" here means "no shaping needed", i.e.
      // genuinely linear, so it must be spelled out, and freshly each call
      // (setKeyAtFrame stores the reference as-is, no clone of its own —
      // sharing one literal across segments would let the curve editor's
      // later in-place edit on ONE key silently reshape every other key
      // pointing at the same array).
      SM.setKeyAtFrame(holder, prop, seg.startFrame, kv, seg.curvePoints || linearCurve());
    }
    var lastSeg = segments[segments.length - 1];
    var kvEnd = []; for (var d4 = 0; d4 < dim; d4++) kvEnd.push(sampleAt(lastSeg.endFrame, d4));
    SM.setKeyAtFrame(holder, prop, lastSeg.endFrame, kvEnd, linearCurve());
    ex.enabled = false;

    return { keyframeCount: segments.length + 1, sampledFrameCount: sampledFrameCount, curvedSegments: curvedSegments, constant: false };
  }

  var API = {
    bakeExpressionToKeyframes: bakeExpressionToKeyframes,
    _rdp: rdp, // exposed for unit tests only
    _fitSegmentCurve: fitSegmentCurve,
    _unionKeyframeTimes: unionKeyframeTimes,
  };
  if (typeof window !== 'undefined') window.SMExprBake = API;
  if (typeof module !== 'undefined' && module.exports) module.exports = API;
})();
