// ---- EXPRESSION EXAMPLES LIBRARY (2026-09-01) ----
// Cyril: "développer les expression d'exemple avec un sous-menu dans les
// exemples et aller glaner les différentes expressions sur le github,
// creative cow pour convertir des expressions after effects couramment
// utilisées dans notre langage et les mettre dans les exemples et les
// classifier."
//
// Every snippet below is written in Nemo's OWN documented expression
// vocabulary (EXPR_PUBLIC_NAMES, motion.js) — never the undocumented AE-
// compatibility aliases (loopOut/valueAtTime/thisComp/index/...). Those
// aliases exist so a snippet PASTED from elsewhere still runs; this menu is
// the opposite direction — the native, idiomatic way to write the same
// idea in Nemo — and motion.js's own wiggle() comment already says so
// explicitly: aliases are "absent from the help tooltip, the examples menu
// and every user-facing string". A few entries below are direct, credited
// ports of well-known After Effects expressions (Creative COW, motionscript
// .com, assorted public cheat sheets); most are Nemo-native equivalents of
// the same idea built from Nemo's own primitives (self.keys, self.velocity,
// layer(), loopAfter/loopBefore, wiggle, noise, angleTo...).
//
// `time` is SECONDS, `frame` is the frame number — both frame-rate aware.
// A 2D property (Position/Anchor/Scale) receives `value` as [x, y]; a 1D
// one (Rotation/Opacity) receives a plain number. Examples marked (2D) index
// value[0]/value[1] explicitly; examples marked (1D) return a bare number.
//
// Scope note: example TITLES/descriptions here are English-only, unlike the
// rest of the app's 4-locale i18n — translating 18 curated code snippets'
// prose into en/fr/ja/es is a separate undertaking from building the menu
// itself, and the code bodies (JS keywords, comments) stay in English
// regardless of UI language anyway. The menu CHROME (the "Examples" button,
// category headers) IS wired through SM.t, same as everywhere else.
(function () {
  var CATEGORIES = [
    {
      id: 'bounce',
      label: 'Bounce & Overshoot',
      examples: [
        {
          id: 'inertialBounce',
          label: 'Inertial Bounce (Position)',
          source: 'Dan Ebberts, ported from the classic Creative COW thread',
          code:
            '// Inertial Bounce — settles into each keyframe with a decaying\n' +
            '// overshoot instead of stopping dead. Ported from Dan Ebberts\' classic\n' +
            '// AE expression (Creative COW forums, motionscript.com), rewritten on\n' +
            "// Nemo's own self.keys / self.velocity instead of AE's key()/\n" +
            '// nearestKey()/velocityAtTime() aliases. Position (2D) — indexes\n' +
            '// value[0]/value[1] so it drops onto any 2D property unchanged.\n' +
            'var n = self.keys.count > 0 ? self.keys.nearest(frame) : null;\n' +
            'if (n && n.frame > frame) n = self.keys.at(n.index - 1);\n' +
            'var t = n ? time - n.time : 0;\n' +
            'if (n && n.index > 1 && t >= 0 && t < 1) {\n' +
            '  var v = self.velocity(n.frame);\n' +
            '  var amp = 0.05, freq = 4, decay = 8;\n' +
            '  var k = amp * Math.sin(freq * t * 2 * Math.PI) / Math.exp(decay * t);\n' +
            '  return [value[0] + v[0] * k, value[1] + v[1] * k];\n' +
            '}\n' +
            'return value;',
        },
        {
          id: 'swingingPendulum',
          label: 'Swinging Pendulum (Rotation)',
          source: 'Common AE cheat-sheet pattern, ported',
          code:
            '// Decaying pendulum swing — starts wide, settles to rest.\n' +
            '// Rotation (1D). Trigger it by giving the layer a single keyframe\n' +
            "// (or none) — it just needs `time` to run from wherever it's placed.\n" +
            'var amplitude = 25, freq = 1.2, decay = 0.6;\n' +
            'return value + amplitude * Math.sin(time * freq * Math.PI * 2) * Math.exp(-decay * time);',
        },
        {
          id: 'elasticScaleIn',
          label: 'Elastic Scale-In (Scale)',
          source: 'Common AE cheat-sheet pattern, ported',
          code:
            "// Overshoots past the keyed scale then settles — the 2D sibling of\n" +
            '// Inertial Bounce, driven the same way (self.keys / self.velocity).\n' +
            '// Scale (2D, in percent — value is [sx, sy]).\n' +
            'var n = self.keys.count > 0 ? self.keys.nearest(frame) : null;\n' +
            'if (n && n.frame > frame) n = self.keys.at(n.index - 1);\n' +
            'var t = n ? time - n.time : 0;\n' +
            'if (n && n.index > 1 && t >= 0 && t < 0.8) {\n' +
            '  var v = self.velocity(n.frame);\n' +
            '  var amp = 0.04, freq = 5, decay = 9;\n' +
            '  var k = amp * Math.sin(freq * t * 2 * Math.PI) / Math.exp(decay * t);\n' +
            '  return [value[0] + v[0] * k, value[1] + v[1] * k];\n' +
            '}\n' +
            'return value;',
        },
      ],
    },
    {
      id: 'looping',
      label: 'Looping',
      examples: [
        {
          id: 'loopCycle',
          label: 'Cycle after the last key',
          source: "Nemo-native — AE's loopOut(\"cycle\")",
          code:
            '// Replays the keyed range forever once playback runs past the last\n' +
            "// keyframe. Works on whichever property it's attached to.\n" +
            "return loopAfter('cycle');",
        },
        {
          id: 'loopPingpong',
          label: 'Ping-pong loop',
          source: "Nemo-native — AE's loopOut(\"pingpong\")",
          code:
            '// Plays the keyed range forward, then backward, alternating —\n' +
            "// good for a breathing/idle cycle that shouldn't visibly snap.\n" +
            "return loopAfter('pingpong');",
        },
        {
          id: 'loopOffsetSpin',
          label: 'Accumulating spin (Rotation)',
          source: "Nemo-native — AE's loopOut(\"offset\")",
          code:
            "// Each repeat of the keyed range picks up where the last one left\n" +
            '// off, instead of jumping back — a wheel that keeps turning rather\n' +
            "// than resetting to 0°. Rotation (1D): key it 0° -> 360° once,\n" +
            "// this makes it spin forever.\n" +
            "return loopAfter('offset');",
        },
        {
          id: 'loopContinueThrow',
          label: 'Continue in a straight line',
          source: "Nemo-native — AE's loopOut(\"continue\")",
          code:
            '// No replay — keeps moving in a straight line at the exact speed\n' +
            '// the property had at its last keyframe. Good for "thrown off\n' +
            '// screen and gone" motion that shouldn\'t loop back.\n' +
            "return loopAfter('continue');",
        },
      ],
    },
    {
      id: 'wiggle',
      label: 'Wiggle & Noise',
      examples: [
        {
          id: 'basicWiggle',
          label: 'Basic wiggle (any property)',
          source: "Nemo-native — AE's wiggle()",
          code:
            '// wiggle(frequency-per-second, amplitude) — auto-shapes to the\n' +
            "// property's own dimension: a plain number on Rotation/Opacity,\n" +
            '// [wx, wy] on Position/Anchor/Scale. `value + wiggle(...)` only\n' +
            "// works on the 1D case — on a 2D one it silently STRING-CONCATENATES\n" +
            '// two arrays instead of adding them (found live testing this exact\n' +
            '// line: threw "must return a number or an [x, y] array"). Index\n' +
            "// per-axis on 2D properties instead — this version works on either.\n" +
            'var w = wiggle(2, 20);\n' +
            'return Array.isArray(value) ? [value[0] + w[0], value[1] + w[1]] : value + w;',
        },
        {
          id: 'perLayerWiggle',
          label: 'Per-layer unique wiggle (Position)',
          source: 'Common AE pattern (seedRandom(index)), adapted',
          code:
            "// AE staggers a wiggle per-layer with seedRandom(index) — Nemo has\n" +
            "// no bare `index`, so seed off the layer's own NAME instead (unique\n" +
            "// per layer, stable across sessions, no extra setup). Position (2D).\n" +
            'seed(self.name.length * 97 + self.name.charCodeAt(0));\n' +
            'return [value[0] + wiggle(1.5, 12)[0], value[1] + wiggle(1.5, 12)[1]];',
        },
        {
          id: 'smoothNoiseDrift',
          label: 'Smooth organic drift (Position)',
          source: 'Nemo-native — noise() instead of wiggle()',
          code:
            "// noise() is smooth (nearby inputs give nearby results) where\n" +
            '// wiggle() jitters between discrete random targets — better for a\n' +
            '// slow organic drift than a nervous shake. Position (2D).\n' +
            'var speed = 0.3, amount = 15;\n' +
            'return [value[0] + noise(time * speed) * amount, value[1] + noise(time * speed + 100) * amount];',
        },
        {
          id: 'wiggleFadeIn',
          label: 'Wiggle that fades in (Position)',
          source: 'Common AE pattern (envelope on wiggle), adapted',
          code:
            "// Ramps the wiggle's own amplitude up over the first second instead\n" +
            "// of starting at full strength immediately. Position (2D).\n" +
            'var envelope = clamp(time, 0, 1);\n' +
            'var w = wiggle(3, 25);\n' +
            'return [value[0] + w[0] * envelope, value[1] + w[1] * envelope];',
        },
      ],
    },
    {
      id: 'time',
      label: 'Motion & Time',
      examples: [
        {
          id: 'constantSpin',
          label: 'Constant rotation (RPM)',
          source: 'Common AE pattern (time * speed), Nemo-native',
          code:
            '// Spins at a fixed rate, independent of any keyframes.\n' +
            '// Rotation (1D). degreesPerSecond = 360 -> one full turn per second.\n' +
            'var degreesPerSecond = 90;\n' +
            'return value + time * degreesPerSecond;',
        },
        {
          id: 'stopMotionStep',
          label: 'Stop-motion / stepped time (Position)',
          source: "Nemo-native — AE's posterizeTime()",
          code:
            '// Snaps evaluation to every N frames — turns any expression below\n' +
            '// it (wiggle, noise, self.at...) into a choppy stop-motion feel\n' +
            '// instead of smooth interpolation. Put this line FIRST. Position\n' +
            '// (2D) — see "Basic wiggle" above for why w[0]/w[1] rather than\n' +
            '// `value + wiggle(...)` on a 2D property.\n' +
            'stepTime(4);\n' +
            'var w = wiggle(2, 15);\n' +
            'return [value[0] + w[0], value[1] + w[1]];',
        },
        {
          id: 'orbitPoint',
          label: 'Orbit around a fixed point (Position)',
          source: 'Common AE pattern (circular orbit), Nemo-native',
          code:
            '// Circles around a fixed canvas point at a constant rate.\n' +
            '// Position (2D) — ignores the keyed value entirely on purpose\n' +
            '// (an orbit is usually the whole motion, not an offset on top).\n' +
            'var center = [960, 540], radius = 200, degreesPerSecond = 60;\n' +
            'var a = radians(time * degreesPerSecond);\n' +
            'return [center[0] + Math.cos(a) * radius, center[1] + Math.sin(a) * radius];',
        },
      ],
    },
    {
      id: 'follow',
      label: 'Follow, Look-at & Rig',
      examples: [
        {
          id: 'lookAtLayer',
          label: '2D look-at another layer (Rotation)',
          source: 'Common AE pattern (lookAt/atan2), Nemo-native angleTo()',
          code:
            '// Points this layer toward another named layer, e.g. an eye\n' +
            "// tracking a cursor-driven Null. Rotation (1D). Replace 'Target'\n" +
            "// with the exact name of the layer to look at, and myPosition with\n" +
            '// this layer\'s own position (a fixed number here on purpose — found\n' +
            "// live: layer(self.name) re-reads EVERY base property of this same\n" +
            '// layer to build its snapshot, including Rotation, the very property\n' +
            '// this expression sits on — a genuine self-reference cycle, correctly\n' +
            '// caught by the depth guard rather than crashing, but broken all the\n' +
            '// same. A fixed socket point also matches the common real case: an\n' +
            '// eye/turret that rotates in place without itself translating.\n' +
            "var target = layer('Target');\n" +
            'var myPosition = [960, 540];\n' +
            'return target ? angleTo(myPosition, target.position) : value;',
        },
        {
          id: 'autoOrientVelocity',
          label: 'Auto-orient to direction of travel (Rotation)',
          source: 'Common AE pattern (auto-rotate along path), Nemo-native',
          code:
            "// Rotates to face wherever this layer's OWN Position is currently\n" +
            "// heading — the classic \"arrow follows the path it's animated\n" +
            '// along\" trick, without a separate path layer. Rotation (1D).\n' +
            '// Needs Position keyframed on this same layer.\n' +
            'var v = self.velocity(frame);\n' +
            'return length(v) > 0.01 ? angleTo([0, 0], v) : value;',
        },
        {
          id: 'midpointBetweenLayers',
          label: 'Place between two layers (Position)',
          source: 'Common AE cheat-sheet pattern, ported',
          code:
            "// Sits exactly halfway between two named layers — a rope/beam\n" +
            "// endpoint, a connecting line's midpoint. Position (2D).\n" +
            "var a = layer('LayerA'), b = layer('LayerB');\n" +
            'return (a && b) ? [(a.position[0] + b.position[0]) / 2, (a.position[1] + b.position[1]) / 2] : value;',
        },
        {
          id: 'cancelParentRotation',
          label: 'Cancel inherited parent rotation (Rotation)',
          source: 'Common AE pattern (ignore parent rotation), Nemo-native',
          code:
            '// Keeps this layer visually level even while its parent spins —\n' +
            "// e.g. a sign that stays upright on a rotating wheel. Rotation (1D).\n" +
            'return self.hasParent ? value - self.parent.rotation : value;',
        },
      ],
    },
  ];

  window.SM_EXPR_EXAMPLES = CATEGORIES;
})();
