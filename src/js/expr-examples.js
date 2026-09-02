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
        {
          id: 'fadeInOut',
          label: 'Fade in, hold, fade out (Opacity)',
          source: 'Extremely common AE pattern, Nemo-native',
          code:
            '// Fades in over the first inDur frames, holds, fades out over the\n' +
            '// last outDur. No keyframes needed — comp.frames is the scene\'s\n' +
            '// own total length. Opacity (1D). remap() already clamps its\n' +
            "// input to the given range, so this never overshoots 0..100.\n" +
            'var inDur = 12, outDur = 12;\n' +
            'var fadeIn = remap(frame, 0, inDur, 0, 100);\n' +
            'var fadeOut = remap(frame, comp.frames - outDur, comp.frames, 100, 0);\n' +
            'return Math.min(fadeIn, fadeOut);',
        },
        {
          id: 'blinkFlicker',
          label: 'Blink / flicker (Opacity)',
          source: 'Common AE pattern (step on a sine), Nemo-native',
          code:
            "// Hard on/off blink at a fixed rate — a caution light, a text\n" +
            '// cursor. Opacity (1D). For a softer flicker, replace the > 0\n' +
            "// step with the sine value itself, remapped to 0..100.\n" +
            'var blinksPerSecond = 2;\n' +
            'return Math.sin(time * blinksPerSecond * Math.PI * 2) > 0 ? 100 : 0;',
        },
        {
          id: 'rhythmicPulse',
          label: 'Rhythmic pulse / heartbeat (Scale)',
          source: 'Common AE pattern, Nemo-native',
          code:
            '// A steady scale pulse — a heartbeat, a breathing UI element.\n' +
            '// Scale (2D, percent) — see "Basic wiggle" above for why\n' +
            "// value[0]/[1] rather than `value + ...` on a 2D property.\n" +
            'var speed = 2, amount = 15;\n' +
            'var pulse = Math.sin(time * speed * Math.PI * 2) * amount;\n' +
            'return [value[0] + pulse, value[1] + pulse];',
        },
        {
          id: 'dvdBounce',
          label: 'DVD-screensaver edge bounce (Position)',
          source: 'Classic AE cheat-sheet pattern, Nemo-native',
          code:
            '// Bounces linearly back and forth across the WHOLE canvas —\n' +
            '// the classic DVD-logo screensaver, no keyframes needed. A\n' +
            '// triangle wave via modulo: comp.width/height read the actual\n' +
            '// canvas size, so this adapts if the project is resized.\n' +
            '// Position (2D).\n' +
            'var speedX = 300, speedY = 220;\n' +
            'var w = comp.width, h = comp.height;\n' +
            'var rawX = (time * speedX) % (2 * w);\n' +
            'var rawY = (time * speedY) % (2 * h);\n' +
            'return [rawX > w ? 2 * w - rawX : rawX, rawY > h ? 2 * h - rawY : rawY];',
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
        {
          id: 'maintainScaleWhenParented',
          label: 'Maintain scale when parented (Scale)',
          source: 'Common AE pattern, Nemo-native — sibling of "Cancel inherited parent rotation"',
          code:
            "// Counteracts the parent's own scale so this layer's ON-SCREEN\n" +
            '// size stays constant even while the parent scales — e.g. an\n' +
            '// icon that should not shrink when its container does. Scale\n' +
            '// (2D, percent). Same self.parent read as the rotation version above.\n' +
            'if (!self.hasParent) return value;\n' +
            'var ps = self.parent.scale;\n' +
            'return [value[0] * 100 / ps[0], value[1] * 100 / ps[1]];',
        },
      ],
    },
    {
      id: 'staging',
      label: 'Staging & Layout',
      examples: [
        {
          id: 'staggerByIndex',
          label: 'Stagger by layer index (any property)',
          source: 'AE « index-based delay » cheat-sheet pattern, ported',
          code:
            '// Décale le MÊME mouvement d\'un calque à l\'autre : la seule\n' +
            '// expression qui transforme dix copies immobiles en vague. En AE on\n' +
            '// écrit valueAtTime(time - index*delay) ; ici self.at() lit la valeur\n' +
            '// BRUTE (pré-expression) de cette propriété à une autre image, donc\n' +
            '// aucune récursion possible.\n' +
            'var delayFrames = 3;\n' +
            'return self.at(frame - self.index * delayFrames);',
        },
        {
          id: 'delayedFollow',
          label: 'Delayed follow / motion trail (Position)',
          source: 'AE « echo with delay » pattern, ported',
          code:
            '// Suit un autre calque avec du retard — la traînée classique.\n' +
            '// Position (2D). Empile plusieurs copies avec des retards croissants\n' +
            '// pour obtenir une traîne complète.\n' +
            'var lead = layer(\'Leader\');\n' +
            'var delayFrames = 4;\n' +
            'if (!lead) return value;\n' +
            'return lead.position.at(frame - delayFrames);',
        },
        {
          id: 'proximityFade',
          label: 'Fade with distance to another layer (Opacity)',
          source: 'AE proximity/opacity pattern, ported',
          code:
            '// Plus le calque cible est loin, plus celui-ci s\'efface.\n' +
            '// Opacity (1D). Utile pour un halo, une ombre de contact, un repère\n' +
            '// qui n\'apparaît qu\'à l\'approche.\n' +
            'var target = layer(\'Target\');\n' +
            'if (!target) return value;\n' +
            'var d = length(self.at(frame), target.position);\n' +
            'return remapEase(d, 0, 400, 100, 0);',
        },
        {
          id: 'clampToCanvas',
          label: 'Keep inside the canvas (Position)',
          source: 'Nemo-native (comp.width/height)',
          code:
            '// Empêche un calque de sortir du cadre, marge comprise.\n' +
            '// Position (2D).\n' +
            'var m = 40;\n' +
            'return [clamp(value[0], m, comp.width - m), clamp(value[1], m, comp.height - m)];',
        },
      ],
    },
    {
      id: 'physics',
      label: 'Physics & Reaction',
      examples: [
        {
          id: 'squashStretch',
          label: 'Squash & stretch from speed (Scale)',
          source: 'AE velocity-driven squash, ported to self.velocity()',
          code:
            '// Étire dans le sens du déplacement et écrase dans l\'autre, en\n' +
            '// conservant à peu près le volume. Scale (2D, en pourcentage).\n' +
            '// À poser sur Scale d\'un calque dont la POSITION bouge : self.velocity()\n' +
            '// donnerait la vitesse de Scale elle-même (donc zéro), on lit donc la\n' +
            '// position du calque une image avant et une après.\n' +
            'var amount = 0.06, maxStretch = 40;\n' +
            'var me = layer(self.name);\n' +
            'if (!me) return value;\n' +
            'var p0 = me.at(\'position\', frame - 1), p1 = me.at(\'position\', frame + 1);\n' +
            'var speed = length(p0, p1) * comp.fps / 2;\n' +
            'var s = clamp(speed * amount, 0, maxStretch);\n' +
            'return [value[0] + s, value[1] - s];',
        },
        {
          id: 'springAfterKey',
          label: 'Spring settle after the last key (any property)',
          source: 'Dan Ebberts overshoot pattern, ported',
          code:
            '// Ressort amorti APRÈS la dernière clé : la propriété dépasse puis\n' +
            '// se pose. Marche en 1D comme en 2D.\n' +
            'var freq = 3, decay = 5, amp = 0.35;\n' +
            'if (self.keys.count < 2) return value;\n' +
            'var last = self.keys.at(self.keys.count);\n' +
            'if (frame <= last.frame) return value;\n' +
            'var t = toSeconds(frame - last.frame);\n' +
            'var v = self.velocity(last.frame - 1);\n' +
            'var osc = amp * Math.sin(freq * t * Math.PI * 2) * Math.exp(-decay * t);\n' +
            'return Array.isArray(value) ? add(value, mul(Array.isArray(v) ? v : [v, 0], osc))\n' +
            '                            : value + (Array.isArray(v) ? v[0] : v) * osc;',
        },
        {
          id: 'windowedWiggle',
          label: 'Wiggle only between two frames (any property)',
          source: 'AE « wiggle window » pattern, ported',
          code:
            '// Un tremblement qui existe seulement dans une fenêtre de temps, avec\n' +
            '// une entrée et une sortie en fondu — plus lisible qu\'un wiggle qu\'on\n' +
            '// coupe brutalement.\n' +
            'var startF = 24, endF = 72, fadeF = 8;\n' +
            'var w = remapEase(frame, startF, startF + fadeF, 0, 1) * remapEase(frame, endF, endF - fadeF, 0, 1);\n' +
            'if (w <= 0) return value;\n' +
            'var full = wiggle(4, 30);\n' +
            'return Array.isArray(value) ? add(value, mul(sub(full, value), w)) : value + (full - value) * w;',
        },
        {
          id: 'cameraShake',
          label: 'Camera shake with decay (Position)',
          source: 'Common AE shake preset, ported',
          code:
            '// Secousse d\'impact : forte au départ, éteinte en une seconde.\n' +
            '// Position (2D). Régler hitFrame sur l\'image du choc.\n' +
            'var hitFrame = 0, amp = 60, decay = 4;\n' +
            'var t = toSeconds(frame - hitFrame);\n' +
            'if (t < 0) return value;\n' +
            'var k = Math.exp(-decay * t);\n' +
            'var shake = wiggle(14, amp * k, 2);\n' +
            'return shake;',
        },
        {
          id: 'randomEveryN',
          label: 'A new random value every N frames (any property)',
          source: 'AE posterizeTime + random, ported to stepTime()',
          code:
            '// Tire une valeur au hasard qui ne change que toutes les N images —\n' +
            '// glitch, néon défaillant, texte qui se réécrit.\n' +
            'stepTime(6);\n' +
            'seed(self.index);\n' +
            'return Array.isArray(value) ? add(value, [random(-40, 40), random(-40, 40)])\n' +
            '                            : value + random(-40, 40);',
        },
      ],
    },
  ];

  window.SM_EXPR_EXAMPLES = CATEGORIES;
})();
