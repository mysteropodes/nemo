// ---- LABS PROTOTYPE — Storyboard mode + multi-scene grouping (Toon Boom Storyboard Pro, scoped) ----
// feature-scouting.md #3 flagged this as "not a feature, a second app" —
// true for a FULL storyboard tool (its own project model, shot/sequence
// data, PDF export pipeline). This is the honestly-scoped slice that's
// actually buildable inside Nemo's existing document without a new data
// model: every KEYFRAME of the active layer becomes a "panel" — Nemo
// already treats a keyframe as a discrete drawn beat, which is exactly
// what a board panel is.
//
// Matched to Storyboard Pro's real structure (checked against its docs,
// not guessed): panels carry separate Action / Dialogue / Notes caption
// COLUMNS (not one blob field), plus a computed Duration — SBP shows
// duration near the end of a scene; here it's simply "frames until the
// next panel" converted to seconds at the project's fps, since Nemo has
// no scene/shot grouping to hang it off of. PDF export offers the same
// two layout families SBP does: "3 Panels" (grid, this session's default)
// and "Full Page" (one large panel + full caption block per page — the
// layout an animatic/handoff PDF actually wants). Drag-to-reorder panels
// is NOT included — reordering panels means reordering keyframes across
// the whole timeline, a real feature of its own, out of scope here.
//
//   SMLabs.openStoryboard()                    — floating panel grid
//   SMLabs.closeStoryboard()
//   SMLabs.printStoryboard({layout:'grid'|'full'})  — print-to-PDF window
//   SMLabs.setBoardCaption(frame, field, text)  — field: action|dialogue|notes
// Captions are Labs-local (localStorage per project key), same precedent
// as timeline-markers/pose-library — a real adoption would promote them
// into the project file.
//
// Multi-scene grouping (SBP groups panels under numbered Scenes, each
// with its own "Scene N — Panel M" numbering and a duration shown near
// the end of the scene — checked against the docs, not guessed):
//   SMLabs.setSceneBoundary(frame)     — this panel starts a new scene
//   SMLabs.removeSceneBoundary(frame)
//   SMLabs.listSceneBoundaries()
// A project with zero boundaries is treated as one single Scene 1 — no
// behavior change from the original single-scene prototype until you
// actually mark a boundary.
(function () {
  function projectKey() { try { if (window.SMProject && SMProject.getProjectKey) return SMProject.getProjectKey(); } catch (e) {} return 'default'; }
  function storeKey() { return 'nemo-labs-board-captions-' + projectKey(); }
  function loadCaptions() { try { return JSON.parse(localStorage.getItem(storeKey()) || '{}'); } catch (e) { return {}; } }
  function saveCaptions(m) { localStorage.setItem(storeKey(), JSON.stringify(m)); }
  var FIELDS = ['action', 'dialogue', 'notes'];
  function entryFor(m, f) {
    var e = m[f];
    // Migrate the first prototype's plain-string captions transparently.
    if (typeof e === 'string') e = { action: e, dialogue: '', notes: '' };
    return e || { action: '', dialogue: '', notes: '' };
  }

  window.SMLabs.setBoardCaption = function (frame, field, text) {
    if (arguments.length === 2) { text = field; field = 'action'; } // back-compat with the v1 signature
    if (FIELDS.indexOf(field) < 0) { console.warn('[labs] field doit être action|dialogue|notes'); return; }
    var m = loadCaptions();
    m[frame] = entryFor(m, frame);
    m[frame][field] = text;
    saveCaptions(m);
  };

  // ---- scene boundaries (multi-scene grouping) ----
  function sceneStoreKey() { return 'nemo-labs-board-scenes-' + projectKey(); }
  function loadSceneBoundaries() {
    try { return (JSON.parse(localStorage.getItem(sceneStoreKey()) || '[]')).sort(function (a, b) { return a - b; }); }
    catch (e) { return []; }
  }
  function saveSceneBoundaries(arr) { localStorage.setItem(sceneStoreKey(), JSON.stringify(arr)); }
  window.SMLabs.setSceneBoundary = function (frame) {
    var b = loadSceneBoundaries();
    if (b.indexOf(frame) < 0) { b.push(frame); saveSceneBoundaries(b); }
    return b;
  };
  window.SMLabs.removeSceneBoundary = function (frame) {
    var b = loadSceneBoundaries().filter(function (f) { return f !== frame; });
    saveSceneBoundaries(b);
    return b;
  };
  window.SMLabs.listSceneBoundaries = function () { return loadSceneBoundaries(); };

  // Splits `keys` (sorted panel frames) into scenes at every boundary
  // that lands ON a panel frame — a boundary elsewhere is a no-op (there's
  // no panel there to start a scene on), same "snap to the nearest real
  // beat" spirit as everything else keyframe-based in this file.
  function groupIntoScenes(keys) {
    var boundaries = loadSceneBoundaries();
    var scenes = [];
    var cur = { number: 1, panels: [] };
    keys.forEach(function (f) {
      if (boundaries.indexOf(f) >= 0 && cur.panels.length) {
        scenes.push(cur);
        cur = { number: scenes.length + 1, panels: [] };
      }
      cur.panels.push(f);
    });
    if (cur.panels.length) scenes.push(cur);
    return scenes;
  }

  function keyframesOf(layerIdx) {
    var ld = state.layers[layerIdx];
    var out = [];
    for (var f = 0; f < ld.frames.length; f++) if (ld.frames[f].isKeyframe) out.push(f);
    return out;
  }
  // Duration = span until the next panel (or the work-area/timeline end
  // for the last one), same "frames until the next discrete beat" idea
  // SBP's per-scene duration column expresses, just without scene
  // grouping to attach it to.
  function durationsFor(keys, totalFrames) {
    return keys.map(function (f, i) {
      var next = i + 1 < keys.length ? keys[i + 1] : totalFrames;
      var span = Math.max(1, next - f);
      return { frames: span, seconds: span / Math.max(1, state.fps) };
    });
  }

  // Rasterize a given frame's EFFECTIVE content (not just the active
  // layer — the whole visible stack, so a panel actually looks like the
  // shot) onto an offscreen canvas via Paper's own rasterize, without
  // disturbing the live document: build a scratch Paper Layer KEPT
  // ATTACHED to the project while populating it, then tear it down.
  //
  // Two live-found bugs here:
  // 1. desP/desR (app.js) do `layer.activate()` then insert via
  //    `new Path({insert:true})`, which targets `project.activeLayer` — a
  //    Layer removed from the project BEFORE population (the first
  //    version of this function) can still be `.activate()`-d (Paper just
  //    sets the pointer) but items constructed against a detached active
  //    layer never actually attach to it, so every rasterize came back
  //    blank (measured: 0 non-white pixels in the output PNG). Fixed by
  //    keeping `scratch` attached while populating it, removing it only
  //    after rasterizing. Also, desP never restores the PREVIOUS active
  //    layer afterward (unlike desR, which does) — restored explicitly
  //    here, or the next tool interaction after opening the board would
  //    silently draw into this dead scratch layer forever.
  // 2. Item#rasterize({resolution}) takes resolution in PIXELS PER INCH
  //    (72 = neutral 1:1), NOT a raw scale multiplier — passing our
  //    desired scale factor directly (e.g. 0.104) produced a 3×2px
  //    output (measured live) since Paper divided the item's bounds by
  //    72 first. Fixed: resolution = scale * 72.
  function thumbFor(frame, w, h) {
    var prevActiveLayer = project.activeLayer;
    var scratch = new Layer(); // attached, becomes topmost + active
    for (var li = 0; li < state.layers.length; li++) {
      if (typeof layerIsEffectivelyVisible === 'function' && !layerIsEffectivelyVisible(li)) continue;
      if (state.layers[li].symbolId) continue;
      var strokes = getEffectiveStrokes(li, frame);
      strokes.forEach(function (sd) {
        if (sd.isRaster) desR(sd, scratch); else desP(sd, scratch);
      });
    }
    var cv = document.createElement('canvas');
    cv.width = w; cv.height = h;
    var ctx = cv.getContext('2d');
    // Match the real document background (feedback 2026-07: "la vignette
    // de preview faut prendre le fond du canvas derrière les éléments
    // affichés") — was hardcoded white regardless of state.canvasBg.
    ctx.fillStyle = state.canvasBg || '#fff'; ctx.fillRect(0, 0, w, h);
    var cw = state.canvasW || 1920, ch = state.canvasH || 1080;
    var scale = Math.min(w / cw, h / ch) * (window.devicePixelRatio || 1);
    try {
      if (scratch.children.length) {
        var raster = scratch.rasterize({ resolution: scale * 72, insert: false });
        if (raster && raster.canvas) {
          var rw = raster.canvas.width, rh = raster.canvas.height;
          ctx.drawImage(raster.canvas, (w - rw) / 2, (h - rh) / 2);
          raster.remove();
        }
      }
    } catch (e) { console.warn('[labs] storyboard thumb rasterize a échoué', e); }
    scratch.remove();
    prevActiveLayer.activate();
    return cv.toDataURL('image/png');
  }

  var panel = null;
  function close() { if (panel) { panel.remove(); panel = null; } }

  function captionField(label, value, onCommit) {
    var wrap = document.createElement('label');
    wrap.style.cssText = 'display:block;margin-bottom:4px;';
    var lab = document.createElement('div');
    lab.textContent = label;
    lab.style.cssText = 'font:10px system-ui;color:#888;margin-bottom:1px;';
    var ta = document.createElement('textarea');
    ta.value = value || '';
    ta.rows = label === 'Action' ? 2 : 1;
    ta.style.cssText = 'width:100%;resize:vertical;background:#0f0e12;color:#eceae7;border:1px solid rgba(255,255,255,.08);border-radius:5px;font:11px system-ui;padding:3px;box-sizing:border-box;';
    ta.addEventListener('blur', function () { onCommit(ta.value); });
    wrap.appendChild(lab); wrap.appendChild(ta);
    return wrap;
  }

  function build() {
    if (!window.SMLabs.isOn('storyboard-mode')) return;
    close();
    var li = state.activeLayerIdx;
    var keys = keyframesOf(li);
    var durs = durationsFor(keys, state.totalFrames);
    var durByFrame = {}; keys.forEach(function (f, i) { durByFrame[f] = durs[i]; });
    var scenes = groupIntoScenes(keys);
    var captions = loadCaptions();
    panel = document.createElement('div');
    panel.id = 'labs-storyboard';
    panel.style.cssText = 'position:fixed;inset:24px;z-index:99998;background:#201f25;border:1px solid rgba(255,255,255,.14);border-radius:12px;box-shadow:0 20px 60px rgba(0,0,0,.6);display:flex;flex-direction:column;overflow:hidden;';
    var head = document.createElement('div');
    head.style.cssText = 'display:flex;align-items:center;justify-content:space-between;padding:10px 16px;border-bottom:1px solid rgba(255,255,255,.08);font:12px system-ui;color:#eceae7;';
    head.innerHTML = '<b>Storyboard — ' + scenes.length + ' scène(s), ' + keys.length + ' panneau(x) (calque « ' + (state.layers[li].name || '') + ' »)</b>';
    var btns = document.createElement('div');
    var sceneBtn = document.createElement('button'); sceneBtn.textContent = 'Nouvelle scène ici';
    sceneBtn.title = 'Marque la frame courante comme début de scène';
    sceneBtn.style.cssText = 'margin-right:6px;padding:5px 10px;background:#333;color:#eceae7;border:none;border-radius:6px;cursor:pointer;font:12px system-ui;';
    sceneBtn.addEventListener('click', function () { window.SMLabs.setSceneBoundary(state.currentFrame); build(); });
    var printGridBtn = document.createElement('button'); printGridBtn.textContent = 'PDF — 3 panneaux';
    printGridBtn.style.cssText = 'margin-right:6px;padding:5px 10px;background:#4E6FF2;color:#fff;border:none;border-radius:6px;cursor:pointer;font:12px system-ui;';
    printGridBtn.addEventListener('click', function () { window.SMLabs.printStoryboard({ layout: 'grid' }); });
    var printFullBtn = document.createElement('button'); printFullBtn.textContent = 'PDF — page complète';
    printFullBtn.style.cssText = 'margin-right:8px;padding:5px 10px;background:#333;color:#eceae7;border:none;border-radius:6px;cursor:pointer;font:12px system-ui;';
    printFullBtn.addEventListener('click', function () { window.SMLabs.printStoryboard({ layout: 'full' }); });
    var closeBtn = document.createElement('button'); closeBtn.textContent = 'Fermer';
    closeBtn.style.cssText = 'padding:5px 10px;background:#333;color:#eceae7;border:none;border-radius:6px;cursor:pointer;font:12px system-ui;';
    closeBtn.addEventListener('click', close);
    btns.appendChild(sceneBtn); btns.appendChild(printGridBtn); btns.appendChild(printFullBtn); btns.appendChild(closeBtn);
    head.appendChild(btns);
    var body = document.createElement('div');
    body.style.cssText = 'flex:1;overflow:auto;padding:16px;';
    var cw = state.canvasW || 1920, ch = state.canvasH || 1080;
    var thumbH = Math.round(220 * ch / cw);
    scenes.forEach(function (scene) {
      var sceneTotalFrames = scene.panels.reduce(function (s, f) { return s + durByFrame[f].frames; }, 0);
      var sceneHead = document.createElement('div');
      sceneHead.style.cssText = 'display:flex;align-items:baseline;gap:10px;margin:10px 0 8px;color:#eceae7;';
      sceneHead.innerHTML = '<b style="font:13px system-ui;">Scène ' + scene.number + '</b>' +
        '<span style="font:10.5px system-ui;color:#888;">' + scene.panels.length + ' panneau(x) — ' + (sceneTotalFrames / Math.max(1, state.fps)).toFixed(2) + 's</span>';
      if (scene.number > 1) {
        var unmark = document.createElement('button');
        unmark.textContent = 'Retirer cette limite de scène';
        unmark.style.cssText = 'margin-left:auto;padding:2px 6px;background:#333;color:#aaa;border:none;border-radius:4px;cursor:pointer;font:10px system-ui;';
        unmark.addEventListener('click', function () { window.SMLabs.removeSceneBoundary(scene.panels[0]); build(); });
        sceneHead.appendChild(unmark);
      }
      body.appendChild(sceneHead);
      var grid = document.createElement('div');
      grid.style.cssText = 'display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:14px;margin-bottom:6px;';
      scene.panels.forEach(function (f, idxInScene) {
        var entry = entryFor(captions, f);
        var d = durByFrame[f];
        var card = document.createElement('div');
        card.style.cssText = 'background:#141318;border:1px solid rgba(255,255,255,.08);border-radius:8px;overflow:hidden;';
        var img = document.createElement('img');
        img.style.cssText = 'width:100%;height:' + thumbH + 'px;object-fit:contain;background:#fff;display:block;cursor:pointer;';
        img.title = 'Aller à la frame ' + (f + 1);
        img.src = thumbFor(f, 220, thumbH);
        img.addEventListener('click', function () { goToFrame(f); });
        var foot = document.createElement('div');
        foot.style.cssText = 'padding:6px 8px;';
        var num = document.createElement('div');
        num.textContent = 'Scène ' + scene.number + ' — Panneau ' + (idxInScene + 1) + ' — frame ' + (f + 1) + ' — ' + d.frames + ' img (' + d.seconds.toFixed(2) + 's)';
        num.style.cssText = 'font:10.5px system-ui;color:#9FB4FA;margin-bottom:5px;';
        foot.appendChild(num);
        foot.appendChild(captionField('Action', entry.action, function (v) { window.SMLabs.setBoardCaption(f, 'action', v); }));
        foot.appendChild(captionField('Dialogue', entry.dialogue, function (v) { window.SMLabs.setBoardCaption(f, 'dialogue', v); }));
        foot.appendChild(captionField('Notes', entry.notes, function (v) { window.SMLabs.setBoardCaption(f, 'notes', v); }));
        card.appendChild(img); card.appendChild(foot);
        grid.appendChild(card);
      });
      body.appendChild(grid);
    });
    if (!keys.length) {
      var empty = document.createElement('div');
      empty.textContent = 'Aucune keyframe sur ce calque — sélectionne un calque avec des poses dessinées.';
      empty.style.cssText = 'color:#888;font:12px system-ui;';
      body.appendChild(empty);
    }
    panel.appendChild(head); panel.appendChild(body);
    document.body.appendChild(panel);
  }

  window.SMLabs.openStoryboard = build;
  window.SMLabs.closeStoryboard = close;
  window.SMLabs.printStoryboard = function (opts) {
    opts = opts || {};
    var layout = opts.layout === 'full' ? 'full' : 'grid';
    var li = state.activeLayerIdx, keys = keyframesOf(li), captions = loadCaptions();
    var durs = durationsFor(keys, state.totalFrames);
    var durByFrame = {}; keys.forEach(function (f, i) { durByFrame[f] = durs[i]; });
    var scenes = groupIntoScenes(keys);
    var cw = state.canvasW || 1920, ch = state.canvasH || 1080;
    var win = window.open('', '_blank');
    if (!win) { if (typeof showToast === 'function') showToast('Popup bloquée — autorise les popups pour imprimer'); return; }
    var html;
    if (layout === 'full') {
      // SBP "Full Page": one large panel + full caption block per printed
      // page, grouped under a Scene divider page (SBP shows scene+panel
      // numbers and a duration near the end of each scene).
      var thumbH = Math.round(520 * ch / cw);
      html = '<html><head><title>Storyboard</title><style>' +
        'body{font-family:system-ui;margin:0;} .page{page-break-after:always;padding:28px;box-sizing:border-box;}' +
        '.page:last-child{page-break-after:auto;}' +
        'img{width:100%;height:' + thumbH + 'px;object-fit:contain;background:#fff;border:1px solid #999;display:block;}' +
        '.hdr{display:flex;justify-content:space-between;font-size:13px;margin:8px 0;color:#333;}' +
        '.cap{margin-top:6px;font-size:12px;color:#111;} .cap b{display:inline-block;width:70px;}' +
        '.scenehdr{font-size:15px;font-weight:600;margin-bottom:4px;}' +
        '.sceneEnd{font-size:11px;color:#666;margin-top:10px;border-top:1px solid #ccc;padding-top:6px;}' +
        '@media print{.page{padding:14px;}}</style></head><body>';
      scenes.forEach(function (scene) {
        var sceneTotalFrames = scene.panels.reduce(function (s, f) { return s + durByFrame[f].frames; }, 0);
        scene.panels.forEach(function (f, idxInScene) {
          var e = entryFor(captions, f);
          var d = durByFrame[f];
          var isLastOfScene = idxInScene === scene.panels.length - 1;
          html += '<div class="page"><div class="scenehdr">Scène ' + scene.number + '</div>' +
            '<div class="hdr"><b>Panneau ' + (idxInScene + 1) + '</b><span>frame ' + (f + 1) + ' — ' + d.frames + ' img (' + d.seconds.toFixed(2) + 's)</span></div>' +
            '<img src="' + thumbFor(f, 900, thumbH * 2) + '">' +
            '<div class="cap"><b>Action</b>' + (e.action || '').replace(/</g, '&lt;') + '</div>' +
            '<div class="cap"><b>Dialogue</b>' + (e.dialogue || '').replace(/</g, '&lt;') + '</div>' +
            '<div class="cap"><b>Notes</b>' + (e.notes || '').replace(/</g, '&lt;') + '</div>' +
            (isLastOfScene ? '<div class="sceneEnd">Fin scène ' + scene.number + ' — durée totale : ' + (sceneTotalFrames / Math.max(1, state.fps)).toFixed(2) + 's (' + sceneTotalFrames + ' images)</div>' : '') +
            '</div>';
        });
      });
    } else {
      // SBP "3 Panels": grid of thumbnails with compact captions, grouped
      // under a scene heading row + a total-duration line at scene end.
      var thumbH2 = Math.round(280 * ch / cw);
      html = '<html><head><title>Storyboard</title><style>' +
        'body{font-family:system-ui;margin:24px;} .grid{display:grid;grid-template-columns:repeat(3,1fr);gap:20px;margin-bottom:6px;}' +
        '.card{break-inside:avoid;border:1px solid #ccc;border-radius:6px;overflow:hidden;}' +
        'img{width:100%;height:' + thumbH2 + 'px;object-fit:contain;background:#fff;display:block;}' +
        '.foot{padding:6px 8px;font-size:10.5px;} .foot b{color:#4E6FF2;}' +
        '.scenehdr{font-size:14px;font-weight:600;margin:14px 0 8px;}' +
        '.sceneEnd{font-size:11px;color:#666;margin:0 0 14px;}' +
        '@media print{body{margin:0;}}</style></head><body>';
      scenes.forEach(function (scene) {
        var sceneTotalFrames = scene.panels.reduce(function (s, f) { return s + durByFrame[f].frames; }, 0);
        html += '<div class="scenehdr">Scène ' + scene.number + '</div><div class="grid">';
        scene.panels.forEach(function (f, idxInScene) {
          var e2 = entryFor(captions, f);
          var d2 = durByFrame[f];
          var lines = [];
          if (e2.action) lines.push('<b>Action</b> ' + e2.action.replace(/</g, '&lt;'));
          if (e2.dialogue) lines.push('<b>Dial.</b> ' + e2.dialogue.replace(/</g, '&lt;'));
          if (e2.notes) lines.push('<b>Note</b> ' + e2.notes.replace(/</g, '&lt;'));
          html += '<div class="card"><img src="' + thumbFor(f, 400, thumbH2 * 2) + '"><div class="foot"><b>' + (idxInScene + 1) + '</b> — frame ' + (f + 1) + ' — ' + d2.seconds.toFixed(2) + 's<br>' + lines.join('<br>') + '</div></div>';
        });
        html += '</div><div class="sceneEnd">Durée totale scène ' + scene.number + ' : ' + (sceneTotalFrames / Math.max(1, state.fps)).toFixed(2) + 's (' + sceneTotalFrames + ' images)</div>';
      });
    }
    html += '</body></html>';
    win.document.write(html); win.document.close();
    setTimeout(function () { win.print(); }, 300);
  };

  window.SMLabs.register('storyboard-mode', {
    flag: 'nemo-labs-storyboard',
    describe: 'Storyboard scope Toon Boom Storyboard Pro (chaque keyframe du calque actif = un panneau) — multi-scènes (setSceneBoundary), colonnes Action/Dialogue/Notes, durée calculée + durée totale de scène, 2 mises en page PDF (grille 3-panneaux / page complète)',
    onDisable: close,
  });
})();
