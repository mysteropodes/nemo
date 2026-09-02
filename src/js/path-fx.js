// ---- EFFETS DE TRACÉ (2026-09) ----
// Une pile de modificateurs NON DESTRUCTIFS appliqués à la GÉOMÉTRIE d'un
// calque, à la façon des « path effects » d'enve ou d'Inkscape. Cyril :
// « ça vaudrait le coup dans effets même si pas wgsl » — d'où ce module en
// JS pur plutôt qu'un shader : un shader travaille sur des pixels déjà
// rasterisés, il ne peut pas déformer un contour vectoriel qui doit rester
// exportable en SVG, en Lottie ou en Rive.
//
// Où ça s'applique : getEffectiveStrokesRendered (app.js), le seul entonnoir
// que traversent le rendu, l'export et les vignettes Motion — le même point
// d'accroche que le duplicateur mograph, et pour la même raison (CLAUDE.md
// §1 : un nouveau champ doit être vu par TOUS les consommateurs, pas par un
// seul).
//
// Contrat : la fonction reçoit les dictionnaires de traits STOCKÉS et rend
// une NOUVELLE liste ; elle ne modifie jamais l'original (le document ne
// change pas, l'effet reste réversible en le désactivant). Elle n'est
// appelée que si la pile existe et contient au moins un effet actif, donc
// le cas courant — aucun effet de tracé — ne coûte rien.
(function () {
  var TYPES = [
    {
      id: 'zigzag',
      label: 'pathFxZigzag',
      params: [
        { key: 'p1', label: 'pathFxSize', min: 0, max: 200, step: 1, def: 12 },
        { key: 'p2', label: 'pathFxRidges', min: 1, max: 20, step: 1, def: 4 },
        { key: 'p3', label: 'pathFxSmooth', min: 0, max: 1, step: 1, def: 0 },
      ],
    },
    {
      id: 'roughen',
      label: 'pathFxRoughen',
      params: [
        { key: 'p1', label: 'pathFxAmount', min: 0, max: 200, step: 1, def: 8 },
        { key: 'p2', label: 'pathFxDetail', min: 1, max: 40, step: 1, def: 6 },
        { key: 'p3', label: 'pathFxEvolution', min: 0, max: 100, step: 1, def: 0 },
      ],
    },
  ];

  function typeDef(id) { for (var i = 0; i < TYPES.length; i++) if (TYPES[i].id === id) return TYPES[i]; return null; }
  function paramValue(fx, key) {
    var def = typeDef(fx.type);
    var p = def && def.params.filter(function (q) { return q.key === key; })[0];
    var v = fx[key];
    return (v === undefined || v === null) ? (p ? p.def : 0) : v;
  }
  function defaultsFor(id) {
    var def = typeDef(id), out = { type: id, enabled: true };
    if (def) def.params.forEach(function (p) { out[p.key] = p.def; });
    return out;
  }

  // Bruit lisse et REPRODUCTIBLE : deux rendus de la même image doivent
  // donner exactement le même tracé, sinon l'export scintille et la
  // vignette ne correspond plus au canvas. D'où une fonction de hachage
  // déterministe plutôt que Math.random().
  function hash2(x, y) {
    var n = Math.sin(x * 127.1 + y * 311.7) * 43758.5453;
    return n - Math.floor(n);
  }
  function smoothNoise(x, y) {
    var xi = Math.floor(x), yi = Math.floor(y), xf = x - xi, yf = y - yi;
    var u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf);
    var a = hash2(xi, yi), b = hash2(xi + 1, yi), c = hash2(xi, yi + 1), d = hash2(xi + 1, yi + 1);
    return ((a * (1 - u) + b * u) * (1 - v) + (c * (1 - u) + d * u) * v) * 2 - 1;
  }

  function pt(p) { return [p[0], p[1]]; }
  function segOf(point, hIn, hOut) {
    return { point: pt(point), handleIn: hIn ? pt(hIn) : [0, 0], handleOut: hOut ? pt(hOut) : [0, 0] };
  }
  // Position sur une courbe de Bézier cubique et sa tangente — la même
  // formulation que partout ailleurs dans le dépôt (curveCubicAt, motion.js).
  function bez(t, a, c1, c2, b) {
    var u = 1 - t;
    return [
      u * u * u * a[0] + 3 * u * u * t * c1[0] + 3 * u * t * t * c2[0] + t * t * t * b[0],
      u * u * u * a[1] + 3 * u * u * t * c1[1] + 3 * u * t * t * c2[1] + t * t * t * b[1],
    ];
  }
  function bezDeriv(t, a, c1, c2, b) {
    var u = 1 - t;
    return [
      3 * u * u * (c1[0] - a[0]) + 6 * u * t * (c2[0] - c1[0]) + 3 * t * t * (b[0] - c2[0]),
      3 * u * u * (c1[1] - a[1]) + 6 * u * t * (c2[1] - c1[1]) + 3 * t * t * (b[1] - c2[1]),
    ];
  }
  function normalOf(d) {
    var len = Math.sqrt(d[0] * d[0] + d[1] * d[1]) || 1;
    return [-d[1] / len, d[0] / len];
  }
  // Échantillonne un contour en points ÉQUIRÉPARTIS par sous-division de
  // chaque courbe : un modificateur qui ne verrait que les ancres d'origine
  // ne pourrait rien faire sur un rectangle de quatre points.
  function walk(segments, closed, perCurve, fn) {
    var n = segments.length;
    var last = closed ? n : n - 1;
    for (var i = 0; i < last; i++) {
      var s0 = segments[i], s1 = segments[(i + 1) % n];
      var a = s0.point, b = s1.point;
      var c1 = [a[0] + (s0.handleOut ? s0.handleOut[0] : 0), a[1] + (s0.handleOut ? s0.handleOut[1] : 0)];
      var c2 = [b[0] + (s1.handleIn ? s1.handleIn[0] : 0), b[1] + (s1.handleIn ? s1.handleIn[1] : 0)];
      for (var k = 0; k < perCurve; k++) {
        var t = k / perCurve;
        fn(bez(t, a, c1, c2, b), normalOf(bezDeriv(t, a, c1, c2, b)), i * perCurve + k);
      }
    }
    if (!closed) {
      var sl = segments[n - 1], sp = segments[n - 2] || sl;
      fn(pt(sl.point), normalOf([sl.point[0] - sp.point[0], sl.point[1] - sp.point[1]]), last * perCurve);
    }
  }

  function applyZigzag(sd, fx) {
    var size = paramValue(fx, 'p1'), ridges = Math.max(1, Math.round(paramValue(fx, 'p2')));
    var smooth = paramValue(fx, 'p3') > 0.5;
    if (!size) return sd;
    var out = [];
    walk(sd.segments, !!sd.closed, ridges * 2, function (p, nrm, i) {
      var side = (i % 2 === 0) ? 1 : -1;
      out.push(segOf([p[0] + nrm[0] * size * side, p[1] + nrm[1] * size * side]));
    });
    if (out.length < 2) return sd;
    var copy = shallowCopy(sd);
    copy.segments = out;
    // Les pointes restent DURES par défaut (c'est ce qui fait un zigzag) ;
    // l'option lisse arrondit en donnant des poignées tangentes.
    if (smooth) roundCorners(copy.segments, !!sd.closed);
    return copy;
  }

  function applyRoughen(sd, fx, frameIdx) {
    var amount = paramValue(fx, 'p1'), detail = Math.max(1, Math.round(paramValue(fx, 'p2')));
    var evolution = paramValue(fx, 'p3');
    if (!amount) return sd;
    // L'évolution avance avec l'image quand elle est non nulle : le contour
    // frémit sans qu'on ait à poser une seule clé (le « boiling line » du
    // dessin traditionnel), et reste figé à zéro.
    var z = evolution ? (frameIdx || 0) * (evolution / 100) : 0;
    var out = [];
    walk(sd.segments, !!sd.closed, detail, function (p, nrm, i) {
      var d = smoothNoise(i * 0.35 + z, z * 0.7 + i * 0.017);
      out.push(segOf([p[0] + nrm[0] * d * amount, p[1] + nrm[1] * d * amount]));
    });
    if (out.length < 2) return sd;
    var copy = shallowCopy(sd);
    copy.segments = out;
    roundCorners(copy.segments, !!sd.closed);
    return copy;
  }

  // Poignées tangentes façon Catmull-Rom : assez pour adoucir un contour
  // rééchantillonné sans dépendre de Paper.js (ce module doit rester
  // utilisable depuis l'export, qui ne construit pas d'objets vivants).
  function roundCorners(segs, closed) {
    var n = segs.length;
    for (var i = 0; i < n; i++) {
      var prev = segs[(i - 1 + n) % n], next = segs[(i + 1) % n];
      if (!closed && (i === 0 || i === n - 1)) continue;
      var tx = (next.point[0] - prev.point[0]) / 6, ty = (next.point[1] - prev.point[1]) / 6;
      segs[i].handleIn = [-tx, -ty];
      segs[i].handleOut = [tx, ty];
    }
  }
  function shallowCopy(sd) {
    var out = {};
    for (var k in sd) if (Object.prototype.hasOwnProperty.call(sd, k)) out[k] = sd[k];
    return out;
  }

  function applyOne(strokes, fx, frameIdx) {
    return strokes.map(function (sd) {
      // Un raster, un trait sans géométrie ou une copie de texture de brosse
      // n'ont rien à déformer — ils traversent la pile intacts.
      if (!sd || sd.isRaster || !sd.segments || sd.segments.length < 2) return sd;
      if (fx.type === 'zigzag') return applyZigzag(sd, fx);
      if (fx.type === 'roughen') return applyRoughen(sd, fx, frameIdx);
      return sd;
    });
  }

  function activeStack(ld) {
    if (!ld || !ld.pathFx || !ld.pathFx.length) return null;
    var on = ld.pathFx.filter(function (f) { return f && f.enabled !== false && typeDef(f.type); });
    return on.length ? on : null;
  }

  function apply(strokes, ld, frameIdx) {
    var stack = activeStack(ld);
    if (!stack || !strokes || !strokes.length) return strokes;
    var out = strokes;
    for (var i = 0; i < stack.length; i++) out = applyOne(out, stack[i], frameIdx);
    return out;
  }

  window.SMPathFx = {
    TYPES: TYPES,
    typeDef: typeDef,
    defaultsFor: defaultsFor,
    paramValue: paramValue,
    hasAny: function (ld) { return !!activeStack(ld); },
    apply: apply,
  };
})();
