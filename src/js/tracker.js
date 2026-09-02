// ---- SUIVI DE MOUVEMENT (2026-09) ----
// Le côté JS du tracker Lucas-Kanade écrit en Rust (geometry-wasm/src/track.rs).
// Trois responsabilités, et rien d'autre : fournir au noyau les pixels de la
// bonne image, convertir ses coordonnées PIXEL en coordonnées MONDE, et cuire
// le résultat en clés.
//
// La conversion pixel → monde passe par le rectangle d'AFFICHAGE du média,
// réévalué À CHAQUE image et non une fois pour toutes : ce rectangle compose
// déjà le placement, l'animation Motion et toute la chaîne de parents, donc un
// calque vidéo qu'on déplace pendant l'analyse reste suivi correctement, sans
// dupliquer une seule ligne de ces maths (CLAUDE.md §3).
(function () {
  function wasmReady() {
    return !!(window.GeometryWasm && GeometryWasm.ready && GeometryWasm.track_points);
  }

  // ---- source de pixels ----
  var _lumaCache = new Map(); // clé image -> {w,h,luma}
  function _cacheKey(ld, frame) { return (ld.layerUid || ld.name || 'l') + ':' + frame; }

  function _lumaFromRGBA(px, w, h) {
    var out = new Uint8Array(w * h);
    for (var i = 0, j = 0; j < out.length; i += 4, j++) {
      // Pondération BT.601 : la même que partout ailleurs dans le dépôt pour
      // passer d'une couleur à une luminance.
      out[j] = (px[i] * 77 + px[i + 1] * 150 + px[i + 2] * 29) >> 8;
    }
    return out;
  }

  function _rasterOf(ld, frame) {
    var f = ld.frames && ld.frames[frame];
    var strokes = (f && f.strokes) || [];
    for (var i = 0; i < strokes.length; i++) if (strokes[i] && strokes[i].isRaster) return strokes[i];
    return null;
  }

  // Une image de séquence ou une image fixe : on relit ses pixels par un
  // canvas hors écran, à la taille NATIVE du média (les coordonnées du
  // tracker sont en pixels média, pas en pixels écran — sinon un zoom
  // changerait le résultat).
  function _lumaFromRaster(sd) {
    return new Promise(function (resolve, reject) {
      if (!sd || !sd.src) { reject(new Error('no raster source')); return; }
      var im = new Image();
      im.onload = function () {
        var c = document.createElement('canvas');
        c.width = im.naturalWidth; c.height = im.naturalHeight;
        var g = c.getContext('2d', { willReadFrequently: true });
        g.drawImage(im, 0, 0);
        var d = g.getImageData(0, 0, c.width, c.height).data;
        resolve({ w: c.width, h: c.height, luma: _lumaFromRGBA(d, c.width, c.height) });
      };
      im.onerror = function () { reject(new Error('raster decode failed')); };
      im.src = sd.src;
    });
  }

  async function lumaAt(ld, frame) {
    var key = _cacheKey(ld, frame);
    if (_lumaCache.has(key)) return _lumaCache.get(key);
    var out = null;
    if (ld.nativeVideo && ld._nvSessionId && window.SMNativeVideo && SMNativeVideo.frameBytes) {
      var nv = ld.nativeVideo;
      var target = Math.max(0, Math.min(Math.max(0, (nv.frameCount || 1) - 1), frame - (nv.offsetFrames || 0)));
      var px = await SMNativeVideo.frameBytes(ld._nvSessionId, target);
      out = { w: nv.width, h: nv.height, luma: _lumaFromRGBA(px, nv.width, nv.height) };
    } else {
      out = await _lumaFromRaster(_rasterOf(ld, frame));
    }
    // Deux images suffisent au suivi ; garder toute une séquence en mémoire
    // ferait grimper l'occupation sans rien apporter.
    if (_lumaCache.size > 3) _lumaCache.clear();
    _lumaCache.set(key, out);
    return out;
  }

  // ---- conversion pixel <-> monde ----
  // Le rectangle d'affichage du média à cette image, dans l'espace monde.
  function displayRect(ld, frame) {
    if (ld.nativeVideo && window.SMNativeVideo && SMNativeVideo.displayRect) {
      var r = SMNativeVideo.displayRect(ld, frame);
      if (r) return r;
    }
    var sd = _rasterOf(ld, frame);
    if (sd) {
      // Attention : le x/y d'un raster stocké est son CENTRE (desR fait
      // `r.position = Point(d.x, d.y)`), pas son coin — le lire comme un coin
      // décalerait tout le suivi d'une demi-image.
      var w = sd.width || 0, h = sd.height || 0;
      return { x: (sd.x || 0) - w / 2, y: (sd.y || 0) - h / 2, width: w, height: h };
    }
    return null;
  }
  function pixelToWorld(ld, frame, px, py, imgW, imgH) {
    var r = displayRect(ld, frame);
    if (!r || !imgW || !imgH) return null;
    return [r.x + (px / imgW) * r.width, r.y + (py / imgH) * r.height];
  }
  function worldToPixel(ld, frame, wx, wy, imgW, imgH) {
    var r = displayRect(ld, frame);
    if (!r || !r.width || !r.height) return null;
    return [((wx - r.x) / r.width) * imgW, ((wy - r.y) / r.height) * imgH];
  }

  // ---- suivi ----
  // Suit UN point d'une image à l'autre sur toute la plage. Rend une liste
  // {frame, world:[x,y], pixel:[x,y], ok, error} — un point perdu est signalé,
  // jamais remplacé par une position inventée.
  async function trackLayer(layerIdx, opts) {
    var ld = state.layers[layerIdx];
    var o = opts || {};
    if (!ld) throw new Error('no layer');
    if (!wasmReady()) throw new Error('wasm tracker unavailable');
    var start = Math.max(0, o.start != null ? o.start : state.currentFrame);
    var end = Math.min(state.totalFrames - 1, o.end != null ? o.end : state.totalFrames - 1);
    if (end <= start) throw new Error('empty range');

    var first = await lumaAt(ld, start);
    var p = worldToPixel(ld, start, o.world[0], o.world[1], first.w, first.h);
    if (!p) throw new Error('no display rect');

    var results = [{ frame: start, world: [o.world[0], o.world[1]], pixel: [p[0], p[1]], ok: true, error: 0 }];
    var prev = first, cur = [p[0], p[1]];
    for (var f = start + 1; f <= end; f++) {
      var next = await lumaAt(ld, f);
      if (!next || next.w !== prev.w || next.h !== prev.h) break;
      var res = JSON.parse(GeometryWasm.track_points(JSON.stringify({
        width: prev.w, height: prev.h,
        prev: Array.from(prev.luma), next: Array.from(next.luma),
        points: [[cur[0], cur[1]]],
        window: o.window || 21, levels: o.levels || 4, iterations: o.iterations || 30,
      })));
      var np = res.points[0], ok = res.ok[0];
      var world = pixelToWorld(ld, f, np[0], np[1], next.w, next.h);
      results.push({ frame: f, world: world, pixel: [np[0], np[1]], ok: ok, error: res.error[0] });
      // Un point perdu arrête l'analyse : continuer à partir d'une position
      // fausse produirait une courbe qui a l'air correcte et ne l'est pas.
      if (!ok) break;
      cur = np;
      prev = next;
      if (o.onProgress) o.onProgress(f, end);
    }
    return results;
  }

  // ---- cuisson ----
  // Écrit le suivi en clés de Position sur un calque cible. Le décalage est
  // conservé : le calque garde sa place relative au point suivi à la première
  // image, sinon il sauterait sur le point à l'instant où on applique.
  function applyToLayer(targetIdx, results, opts) {
    var ld = state.layers[targetIdx];
    if (!ld || !results || results.length < 2) return { written: 0 };
    var M = window.SMMotion;
    if (!M || !M.setKeyAtFrame) return { written: 0 };
    var o = opts || {};
    var base = results[0].world;
    var cur = M.valueAtFrame ? M.valueAtFrame(ld, 'position', results[0].frame) : [0, 0];
    var offX = (o.keepOffset === false) ? 0 : (cur[0] || 0);
    var offY = (o.keepOffset === false) ? 0 : (cur[1] || 0);
    if (typeof saveAllLayerFrames === 'function') saveAllLayerFrames();
    if (typeof pushUndoLayers === 'function') pushUndoLayers(true);
    if (!ld.motion || !ld.motion.position) M.toggleAnimated(ld, 'position');
    var n = 0;
    for (var i = 0; i < results.length; i++) {
      var r = results[i];
      if (!r.ok || !r.world) break;
      M.setKeyAtFrame(ld, 'position', r.frame, [offX + (r.world[0] - base[0]), offY + (r.world[1] - base[1])]);
      n++;
    }
    if (typeof loadFrame === 'function') loadFrame(state.currentFrame);
    if (typeof updateUI === 'function') updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    return { written: n };
  }

  window.SMTracker = {
    available: wasmReady,
    lumaAt: lumaAt,
    displayRect: displayRect,
    pixelToWorld: pixelToWorld,
    worldToPixel: worldToPixel,
    trackLayer: trackLayer,
    applyToLayer: applyToLayer,
  };
})();
