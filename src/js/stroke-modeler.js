// ---- STROKE MODELER — port of Google's ink-stroke-modeler core ----
// (github.com/google/ink-stroke-modeler, Apache-2.0 — the physics model
// Chrome/Android inking and rnote's PenPathModeledBuilder use). Replaces
// the trailing moving average (draw-bridge.js stabilizePoint) with a
// spring-mass-damper: the drawn point is a mass pulled toward the pen by a
// spring and slowed by drag. Compared to the moving average this is LESS
// destructive: it never throws away geometry after the fact — it shapes
// the input stream itself, upsampled to a fixed output rate, and its
// end-of-stroke catch-up iterates the same physics until the line reaches
// the lift-off point (instead of splicing one raw point onto an averaged
// tail, the visible "hook" the old approach could leave).
//
// Three pieces, same decomposition as the original:
//   1. Wobble smoother — a time-windowed moving average blended in ONLY at
//      low speeds (slow, deliberate strokes are where hand tremor shows;
//      fast strokes pass through raw, so there's no lag when it matters).
//   2. Position modeler — the spring-mass-damper integration, upsampled to
//      `rate` outputs/second along each input segment.
//   3. End-of-stroke — up to `eosIterations` extra physics steps toward
//      the final input until within `stopDistPx` (screen px) of it.
//
// Pressure rides along: each upsampled output carries the linear
// interpolation of the two surrounding raw pressures (simplification of
// the original's StylusStateModeler nearest-projection — equivalent here
// because our anchors move strictly along the input segment).
//
// This file is the REFERENCE implementation; geometry-wasm's
// strokemodeler.rs is the byte-for-byte Rust port used when the wasm
// module is loaded (window.SMStrokeModeler.create prefers it). Per
// CLAUDE.md §3, any change here must be mirrored there in the same commit.
//
// Levels (the user-facing "différents niveaux de lissage"): scale the
// spring/drag/wobble constants. Level 1 = Google's suggested defaults
// (subtle, what Chrome ships); 2 and 3 loosen the spring and widen the
// wobble window for progressively calmer lines.
(function () {
  // springMass: s² — bigger = weaker spring = more lag/smoothing.
  // drag: 1/s — bigger = velocity bleeds off faster (less overshoot).
  // rate: modeled outputs per second along the input.
  // wobbleTimeout: seconds of history in the low-speed averaging window.
  // wobbleFloor/Ceil: screen px/s — full averaging below floor, raw above
  //   ceiling (Google's 1.31/1.44 cm/s ≈ 50/54 px/s at 96dpi).
  // stopDistPx: end-of-stroke convergence threshold, screen px.
  // drag = 72.0 is Google's REAL default (params.h `drag_constant = 72.f`)
  // — an earlier draft misread it as 72/3240, which left the spring
  // essentially undamped (ζ≈2e-4): the modeled point ORBITED the pen
  // instead of trailing it, verified live as deviation INCREASING with
  // level. With 72, level 1 sits at ζ≈0.66 (Google's tuning). Levels 2/3
  // weaken the spring for more lag and lower the drag just enough to stay
  // at-or-above critical damping (ζ≈0.92 / 1.25) — smoother without any
  // ringing, never oscillating.
  var LEVELS = {
    1: { springMass: 11 / 32400, drag: 72, rate: 120, wobbleTimeout: 0.04, wobbleFloor: 50, wobbleCeil: 54, stopDistPx: 0.1, eosIterations: 20 },
    // eosIterations scales with the level: a weaker spring lags farther
    // behind the pen, so the end-of-stroke catch-up needs a bigger step
    // budget to settle onto the lift-off point (verified: 20 left level 3
    // ~27px short; 60 converges).
    2: { springMass: (11 / 32400) * 3, drag: 57.6, rate: 120, wobbleTimeout: 0.07, wobbleFloor: 90, wobbleCeil: 140, stopDistPx: 0.1, eosIterations: 30 },
    3: { springMass: (11 / 32400) * 8, drag: 48, rate: 120, wobbleTimeout: 0.12, wobbleFloor: 140, wobbleCeil: 260, stopDistPx: 0.1, eosIterations: 60 },
  };

  // unitScale: world→screen factor (view.zoom at stroke start) — the
  // spring/drag math is position-unit-independent, but the wobble speed
  // thresholds and stop distance are calibrated in SCREEN px.
  function JsModeler(level, unitScale) {
    this.P = LEVELS[level] || LEVELS[1];
    this.scale = unitScale || 1;
    this.pos = null; this.vel = [0, 0];
    this.lastInput = null; this.lastT = 0; this.lastP = 1;
    this.wobbleWin = []; // [t, x, y, speed]
  }
  JsModeler.prototype._wobble = function (x, y, t) {
    var win = this.wobbleWin, P = this.P;
    var speed = 0;
    if (win.length) {
      var last = win[win.length - 1];
      var dt = Math.max(1e-4, t - last[0]);
      speed = Math.hypot(x - last[1], y - last[2]) * this.scale / dt;
    }
    win.push([t, x, y, speed]);
    while (win.length && win[0][0] < t - P.wobbleTimeout) win.shift();
    var ax = 0, ay = 0, as = 0;
    for (var i = 0; i < win.length; i++) { ax += win[i][1]; ay += win[i][2]; as += win[i][3]; }
    ax /= win.length; ay /= win.length; as /= win.length;
    var ratio = Math.max(0, Math.min(1, (as - P.wobbleFloor) / Math.max(1e-6, P.wobbleCeil - P.wobbleFloor)));
    return [ax + (x - ax) * ratio, ay + (y - ay) * ratio];
  };
  JsModeler.prototype._step = function (target, ddt) {
    var P = this.P;
    for (var a = 0; a < 2; a++) {
      var accel = (target[a] - this.pos[a]) / P.springMass - P.drag * this.vel[a];
      this.vel[a] += ddt * accel;
      this.pos[a] += ddt * this.vel[a];
    }
  };
  JsModeler.prototype.down = function (x, y, t, p) {
    this.pos = [x, y]; this.vel = [0, 0];
    this.lastInput = [x, y]; this.lastT = t; this.lastP = p;
    this.wobbleWin = [[t, x, y, 0]];
    return [{ x: x, y: y, p: p }];
  };
  JsModeler.prototype.move = function (x, y, t, p) {
    if (!this.pos) return this.down(x, y, t, p);
    var sm = this._wobble(x, y, t);
    var dt = Math.max(1e-4, t - this.lastT);
    // 2026-09 fix — the substep count used to be capped at a flat 50
    // regardless of dt, so `ddt = dt/n` grew UNBOUNDED for any real gap
    // wider than 50/rate (≈0.42s at rate=120): explicit Euler on this
    // spring-damper is only stable for small ddt, so a big-enough gap made
    // `_step` diverge — position and velocity doubling in magnitude and
    // flipping sign every iteration, out to values in the sextillions
    // within one gesture. Reproduced live (perspective-guide drag, level-1
    // stabilizer, a background-tab timer stall inflating the real dt
    // between synthetic pointermoves) and confirmed the SAME divergence
    // happens with the guide off — this was always latent, any lag spike
    // or backgrounded tab mid-stroke could trigger it, not something
    // specific to the guide. Each substep costs two multiply-adds per
    // axis, so there is no real reason to cap the WORK — only ddt needs a
    // ceiling. Uncapping n (up to a generous safety net, still far cheaper
    // than a single frame budget even fully used) keeps ddt pinned near
    // 1/rate for any dt, however large.
    var n = Math.max(1, Math.min(20000, Math.ceil(dt * this.P.rate)));
    var ddt = dt / n, out = [];
    for (var i = 1; i <= n; i++) {
      var f = i / n;
      var target = [this.lastInput[0] + (sm[0] - this.lastInput[0]) * f, this.lastInput[1] + (sm[1] - this.lastInput[1]) * f];
      this._step(target, ddt);
      out.push({ x: this.pos[0], y: this.pos[1], p: this.lastP + (p - this.lastP) * f });
    }
    this.lastInput = sm; this.lastT = t; this.lastP = p;
    return out;
  };
  JsModeler.prototype.up = function (x, y, t, p) {
    var out = this.move(x, y, t, p);
    // End-of-stroke catch-up toward the FINAL raw input (the pen lifted
    // THERE, not at the wobble-smoothed point) — Google's ModelEndOfStroke
    // (position_modeler.h) verbatim: each candidate step is checked for
    // OVERSHOOT by projecting the anchor onto the [previous→candidate]
    // segment; an overshooting step is discarded and retried with half the
    // time step, so the tail settles ONTO the lift-off point instead of
    // orbiting it. Halts on: max iterations, within stop distance of the
    // anchor, or no more meaningful progress per step.
    var P = this.P, ddt = 1 / P.rate;
    var target = [x, y];
    var stopWorld = P.stopDistPx / Math.max(1e-9, this.scale);
    for (var i = 0; i < P.eosIterations; i++) {
      var prevPos = [this.pos[0], this.pos[1]], prevVel = [this.vel[0], this.vel[1]];
      this._step(target, ddt);
      var stepDist = Math.hypot(this.pos[0] - prevPos[0], this.pos[1] - prevPos[1]);
      if (stepDist < stopWorld) { this.pos = prevPos; this.vel = prevVel; break; }
      // Overshoot test: parameter of the anchor's projection onto the step
      // segment — t<1 means the closest approach is BEFORE the step's end.
      var dx = this.pos[0] - prevPos[0], dy = this.pos[1] - prevPos[1];
      var tt = ((target[0] - prevPos[0]) * dx + (target[1] - prevPos[1]) * dy) / (stepDist * stepDist);
      if (tt < 1) { this.pos = prevPos; this.vel = prevVel; ddt *= 0.5; continue; }
      out.push({ x: this.pos[0], y: this.pos[1], p: p });
      if (Math.hypot(target[0] - this.pos[0], target[1] - this.pos[1]) < stopWorld) break;
    }
    return out;
  };

  // Thin adapter over the Rust port (geometry-wasm strokemodeler.rs) so
  // draw-bridge.js talks to ONE shape of object regardless of backend.
  function unpackPoints(packed) {
    var out = new Array(Math.floor(packed.length / 3));
    for (var i = 0, j = 0; i < packed.length; i += 3, j++) {
      out[j] = { x: packed[i], y: packed[i + 1], p: packed[i + 2] };
    }
    return out;
  }
  function WasmModeler(inner) {
    this.inner = inner;
    // A stale wasm binary can coexist briefly with fresh glue during local
    // development. Select the packed API once per gesture, not once per
    // point, and retain the legacy JSON route as a compatibility fallback.
    this.packed = typeof inner.down_packed === 'function' &&
      typeof inner.move_packed === 'function' && typeof inner.up_packed === 'function';
  }
  WasmModeler.prototype.down = function (x, y, t, p) {
    return this.packed ? unpackPoints(this.inner.down_packed(x, y, t, p)) : JSON.parse(this.inner.down(x, y, t, p));
  };
  WasmModeler.prototype.move = function (x, y, t, p) {
    return this.packed ? unpackPoints(this.inner.move_packed(x, y, t, p)) : JSON.parse(this.inner.move(x, y, t, p));
  };
  WasmModeler.prototype.up = function (x, y, t, p) {
    return this.packed ? unpackPoints(this.inner.up_packed(x, y, t, p)) : JSON.parse(this.inner.up(x, y, t, p));
  };
  WasmModeler.prototype.downPacked = function (x, y, t, p) {
    return this.packed ? this.inner.down_packed(x, y, t, p) : null;
  };
  WasmModeler.prototype.movePacked = function (x, y, t, p) {
    return this.packed ? this.inner.move_packed(x, y, t, p) : null;
  };
  WasmModeler.prototype.upPacked = function (x, y, t, p) {
    return this.packed ? this.inner.up_packed(x, y, t, p) : null;
  };

  window.SMStrokeModeler = {
    LEVELS: LEVELS,
    // Prefer the Rust port when loaded; the JS class IS the reference
    // fallback (same silent-fallback convention as every other wasm path).
    create: function (level, unitScale) {
      if (window.GeometryWasm && window.GeometryWasm.ready && window.GeometryWasm.StrokeModeler) {
        try { return new WasmModeler(new window.GeometryWasm.StrokeModeler(level, unitScale || 1)); }
        catch (e) { console.warn('[stroke-modeler] wasm indisponible, repli JS', e); }
      }
      return new JsModeler(level, unitScale);
    },
    createJs: function (level, unitScale) { return new JsModeler(level, unitScale); }, // test hook: force the JS reference
    unpackPoints: unpackPoints, // test hook: validates the wasm boundary without loading a browser
  };
})();
