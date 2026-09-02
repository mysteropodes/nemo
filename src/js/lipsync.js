// ---- SYNCHRO LABIALE (2026-09) ----
// Cyril : « une fenêtre de préviz des composants avec choix des keyframes à
// remapper et lié avec l'audio comme dans les logiciels d'anim 2D ».
//
// C'est exactement la « mouth chart » du dessin animé traditionnel : on
// dessine une fois les bouches (une par son), on les numérote, puis on écrit
// image par image quelle bouche joue. Nemo a déjà TOUT le mécanisme de
// sortie : un calque instance de composant en mode « image fixe » affiche
// l'image interne écrite dans `frames[i].componentFrame`, et
// resolveSymbolFrameIdx (app.js) est le point de passage unique de tous les
// lecteurs. Ce module ne fait donc que deux choses : lire la piste audio pour
// proposer une bouche par image, et écrire ces images internes en clés.
//
// Ce que l'analyse peut et ne peut pas faire, dit franchement : elle mesure
// l'énergie, le centre de gravité spectral et le taux de passages par zéro.
// Cela sépare bien le silence, les bouches fermées, les voyelles ouvertes et
// les fricatives — pas les phonèmes L et W/Q, qui demanderaient une vraie
// reconnaissance de parole. Ces deux bouches restent donc mappables à la main
// et ne sont jamais posées automatiquement. Le résultat est un point de
// départ à corriger, comme la détection automatique de n'importe quel logiciel
// d'animation : la bande d'images du panneau reste le moyen de retoucher.
(function () {
  // Les bouches de la charte de Preston Blair, l'ordre dans lequel un
  // animateur 2D les dessine.
  var MOUTHS = [
    { id: 'rest', label: 'lipRest', auto: true },
    { id: 'MBP', label: 'lipMBP', auto: true },
    { id: 'AI', label: 'lipAI', auto: true },
    { id: 'E', label: 'lipE', auto: true },
    { id: 'O', label: 'lipO', auto: true },
    { id: 'U', label: 'lipU', auto: true },
    { id: 'FV', label: 'lipFV', auto: true },
    { id: 'L', label: 'lipL', auto: false },
    { id: 'WQ', label: 'lipWQ', auto: false },
  ];

  function defaults() {
    // Par défaut chaque bouche pointe vers une image interne différente, dans
    // l'ordre : c'est le cas le plus courant (un composant d'une image par
    // bouche, dessinées dans l'ordre de la charte).
    var map = {};
    MOUTHS.forEach(function (m, i) { map[m.id] = i; });
    return { trackIdx: 0, map: map, sensitivity: 1, hold: 2, start: 0, end: 0 };
  }
  function settingsOf(ld) {
    if (!ld.lipSync) ld.lipSync = defaults();
    if (!ld.lipSync.map) ld.lipSync.map = defaults().map;
    return ld.lipSync;
  }

  // ---- analyse ----
  // FFT radix-2 sur place, la plus petite qui fasse le travail : on n'a
  // besoin que d'un spectre de magnitude par image pour le centre de gravité.
  function fft(re, im) {
    var n = re.length;
    for (var i = 1, j = 0; i < n; i++) {
      var bit = n >> 1;
      for (; j & bit; bit >>= 1) j ^= bit;
      j ^= bit;
      if (i < j) { var tr = re[i]; re[i] = re[j]; re[j] = tr; var ti = im[i]; im[i] = im[j]; im[j] = ti; }
    }
    for (var len = 2; len <= n; len <<= 1) {
      var ang = -2 * Math.PI / len;
      var wr = Math.cos(ang), wi = Math.sin(ang);
      for (var k = 0; k < n; k += len) {
        var cr = 1, ci = 0;
        for (var m = 0; m < len / 2; m++) {
          var ar = re[k + m], ai = im[k + m];
          var br = re[k + m + len / 2] * cr - im[k + m + len / 2] * ci;
          var bi = re[k + m + len / 2] * ci + im[k + m + len / 2] * cr;
          re[k + m] = ar + br; im[k + m] = ai + bi;
          re[k + m + len / 2] = ar - br; im[k + m + len / 2] = ai - bi;
          var ncr = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = ncr;
        }
      }
    }
  }

  var N = 1024;
  function featuresAt(buf, timeSec) {
    var sr = buf.sampleRate;
    var data = buf.getChannelData(0);
    var start = Math.round(timeSec * sr);
    if (start < 0 || start + N > data.length) return null;
    var re = new Float64Array(N), im = new Float64Array(N);
    var rms = 0, zc = 0, prev = 0;
    for (var i = 0; i < N; i++) {
      var s = data[start + i];
      rms += s * s;
      if (i && ((s >= 0) !== (prev >= 0))) zc++;
      prev = s;
      // Fenêtre de Hann : sans elle, les bords de la tranche créent un
      // étalement spectral qui déplace le centre de gravité vers l'aigu.
      re[i] = s * (0.5 - 0.5 * Math.cos(2 * Math.PI * i / (N - 1)));
    }
    rms = Math.sqrt(rms / N);
    fft(re, im);
    var num = 0, den = 0;
    for (var k = 1; k < N / 2; k++) {
      var mag = Math.sqrt(re[k] * re[k] + im[k] * im[k]);
      num += mag * (k * sr / N);
      den += mag;
    }
    return { rms: rms, centroid: den > 0 ? num / den : 0, zcr: zc / N };
  }

  // Classement heuristique, seuils exprimés en fractions de l'énergie MAXIMALE
  // de l'extrait : une prise enregistrée bas et la même prise normalisée
  // doivent donner la même animation.
  function classify(f, peak, sens) {
    if (!f) return 'rest';
    var e = peak > 0 ? f.rms / peak : 0;
    var s = Math.max(0.1, sens || 1);
    if (e < 0.04 / s) return 'rest';
    if (e < 0.12 / s) return 'MBP';
    if (f.zcr > 0.22 && e < 0.45) return 'FV';
    if (f.centroid > 1800) return 'AI';
    if (f.centroid > 1100) return 'E';
    if (f.centroid > 650) return 'O';
    return 'U';
  }

  // Un plan d'une seule image est illisible à l'écran et donne cette bouillie
  // qu'on reconnaît tout de suite dans une synchro automatique ratée. La règle
  // du métier est un minimum de deux images par bouche ; on l'applique ici,
  // sauf pour le silence qui a le droit de tomber net.
  function smooth(classes, hold) {
    var h = Math.max(1, Math.round(hold || 2));
    if (h < 2) return classes;
    var out = classes.slice();
    var i = 0;
    while (i < out.length) {
      var j = i;
      while (j + 1 < out.length && out[j + 1] === out[i]) j++;
      var runLen = j - i + 1;
      if (runLen < h && i > 0 && out[i] !== 'rest') {
        for (var k = i; k <= j; k++) out[k] = out[i - 1];
      }
      i = j + 1;
    }
    return out;
  }

  function trackBuffer(idx) {
    var t = (state.audioTracks || [])[idx];
    return t && t._buffer ? t : null;
  }

  function analyze(ld, opts) {
    var st = settingsOf(ld);
    var o = opts || {};
    var track = trackBuffer(o.trackIdx != null ? o.trackIdx : st.trackIdx);
    if (!track) return { error: 'noAudio' };
    var fps = state.fps || 24;
    var startF = Math.max(0, o.start != null ? o.start : (st.start || 0));
    var endF = Math.min(state.totalFrames - 1, (o.end != null ? o.end : st.end) || (state.totalFrames - 1));
    if (endF < startF) return { error: 'range' };
    var raw = [], peak = 0;
    for (var f = startF; f <= endF; f++) {
      // L'audio est posé sur la timeline avec un décalage : l'image f lit
      // l'instant (f - offset)/fps DANS le fichier, sinon toute la synchro
      // est décalée d'exactement ce décalage.
      var t = (f - (track.offsetFrames || 0)) / fps;
      var ft = t >= 0 ? featuresAt(track._buffer, t) : null;
      if (ft && ft.rms > peak) peak = ft.rms;
      raw.push(ft);
    }
    var classes = raw.map(function (ft) { return classify(ft, peak, o.sensitivity != null ? o.sensitivity : st.sensitivity); });
    classes = smooth(classes, o.hold != null ? o.hold : st.hold);
    return { start: startF, end: endF, classes: classes, peak: peak };
  }

  // ---- écriture ----
  // Une clé seulement là où la bouche CHANGE : c'est ce qu'un animateur
  // écrirait à la main, et ça laisse une timeline lisible plutôt qu'une clé
  // par image.
  function apply(layerIdx, result, opts) {
    var ld = state.layers[layerIdx];
    if (!ld || !ld.symbolId || !result || result.error) return { written: 0 };
    var st = settingsOf(ld);
    var map = (opts && opts.map) || st.map;
    var sym = state.symbols[ld.symbolId];
    var maxFrame = Math.max(0, (sym ? Math.max(1, sym.totalFrames) : 1) - 1);
    ld.symPlayMode = 'single';
    var written = 0, prev = null;
    for (var i = 0; i < result.classes.length; i++) {
      var f = result.start + i;
      var cls = result.classes[i];
      if (cls === prev) continue;
      prev = cls;
      var target = Math.min(maxFrame, Math.max(0, Math.floor(map[cls] != null ? map[cls] : 0)));
      var fr = ld.frames && ld.frames[f];
      if (!fr) continue;
      fr.isKeyframe = true;
      fr.isInterpolated = false;
      fr.componentFrame = target;
      delete fr.blankOverride;
      written++;
    }
    return { written: written };
  }

  window.SMLipSync = {
    MOUTHS: MOUTHS,
    defaults: defaults,
    settingsOf: settingsOf,
    analyze: analyze,
    apply: apply,
    // exposés pour les tests et le débogage
    _featuresAt: featuresAt,
    _classify: classify,
    _smooth: smooth,
  };
})();
