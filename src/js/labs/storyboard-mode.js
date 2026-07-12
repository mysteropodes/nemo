// ---- LABS PROTOTYPE — Storyboard mode (Toon Boom Storyboard Pro, scoped) ----
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
    ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, w, h);
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
    var captions = loadCaptions();
    panel = document.createElement('div');
    panel.id = 'labs-storyboard';
    panel.style.cssText = 'position:fixed;inset:24px;z-index:99998;background:#201f25;border:1px solid rgba(255,255,255,.14);border-radius:12px;box-shadow:0 20px 60px rgba(0,0,0,.6);display:flex;flex-direction:column;overflow:hidden;';
    var head = document.createElement('div');
    head.style.cssText = 'display:flex;align-items:center;justify-content:space-between;padding:10px 16px;border-bottom:1px solid rgba(255,255,255,.08);font:12px system-ui;color:#eceae7;';
    head.innerHTML = '<b>Storyboard — ' + keys.length + ' panneau(x) (calque « ' + (state.layers[li].name || '') + ' »)</b>';
    var btns = document.createElement('div');
    var printGridBtn = document.createElement('button'); printGridBtn.textContent = 'PDF — 3 panneaux';
    printGridBtn.style.cssText = 'margin-right:6px;padding:5px 10px;background:#4E6FF2;color:#fff;border:none;border-radius:6px;cursor:pointer;font:12px system-ui;';
    printGridBtn.addEventListener('click', function () { window.SMLabs.printStoryboard({ layout: 'grid' }); });
    var printFullBtn = document.createElement('button'); printFullBtn.textContent = 'PDF — page complète';
    printFullBtn.style.cssText = 'margin-right:8px;padding:5px 10px;background:#333;color:#eceae7;border:none;border-radius:6px;cursor:pointer;font:12px system-ui;';
    printFullBtn.addEventListener('click', function () { window.SMLabs.printStoryboard({ layout: 'full' }); });
    var closeBtn = document.createElement('button'); closeBtn.textContent = 'Fermer';
    closeBtn.style.cssText = 'padding:5px 10px;background:#333;color:#eceae7;border:none;border-radius:6px;cursor:pointer;font:12px system-ui;';
    closeBtn.addEventListener('click', close);
    btns.appendChild(printGridBtn); btns.appendChild(printFullBtn); btns.appendChild(closeBtn);
    head.appendChild(btns);
    var body = document.createElement('div');
    body.style.cssText = 'flex:1;overflow:auto;padding:16px;display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:14px;';
    var cw = state.canvasW || 1920, ch = state.canvasH || 1080;
    var thumbH = Math.round(220 * ch / cw);
    keys.forEach(function (f, idx) {
      var entry = entryFor(captions, f);
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
      num.textContent = 'Panneau ' + (idx + 1) + ' — frame ' + (f + 1) + ' — ' + durs[idx].frames + ' img (' + durs[idx].seconds.toFixed(2) + 's)';
      num.style.cssText = 'font:10.5px system-ui;color:#9FB4FA;margin-bottom:5px;';
      foot.appendChild(num);
      foot.appendChild(captionField('Action', entry.action, function (v) { window.SMLabs.setBoardCaption(f, 'action', v); }));
      foot.appendChild(captionField('Dialogue', entry.dialogue, function (v) { window.SMLabs.setBoardCaption(f, 'dialogue', v); }));
      foot.appendChild(captionField('Notes', entry.notes, function (v) { window.SMLabs.setBoardCaption(f, 'notes', v); }));
      card.appendChild(img); card.appendChild(foot);
      body.appendChild(card);
    });
    if (!keys.length) {
      var empty = document.createElement('div');
      empty.textContent = 'Aucune keyframe sur ce calque — sélectionne un calque avec des poses dessinées.';
      empty.style.cssText = 'color:#888;font:12px system-ui;grid-column:1/-1;';
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
    var cw = state.canvasW || 1920, ch = state.canvasH || 1080;
    var win = window.open('', '_blank');
    if (!win) { if (typeof showToast === 'function') showToast('Popup bloquée — autorise les popups pour imprimer'); return; }
    var html;
    if (layout === 'full') {
      // SBP "Full Page": one large panel + full caption block per printed page.
      var thumbH = Math.round(520 * ch / cw);
      html = '<html><head><title>Storyboard</title><style>' +
        'body{font-family:system-ui;margin:0;} .page{page-break-after:always;padding:28px;box-sizing:border-box;}' +
        '.page:last-child{page-break-after:auto;}' +
        'img{width:100%;height:' + thumbH + 'px;object-fit:contain;background:#fff;border:1px solid #999;display:block;}' +
        '.hdr{display:flex;justify-content:space-between;font-size:13px;margin:8px 0;color:#333;}' +
        '.cap{margin-top:6px;font-size:12px;color:#111;} .cap b{display:inline-block;width:70px;}' +
        '@media print{.page{padding:14px;}}</style></head><body>';
      keys.forEach(function (f, idx) {
        var e = entryFor(captions, f);
        html += '<div class="page"><div class="hdr"><b>Panneau ' + (idx + 1) + '</b><span>frame ' + (f + 1) + ' — ' + durs[idx].frames + ' img (' + durs[idx].seconds.toFixed(2) + 's)</span></div>' +
          '<img src="' + thumbFor(f, 900, thumbH * 2) + '">' +
          '<div class="cap"><b>Action</b>' + (e.action || '').replace(/</g, '&lt;') + '</div>' +
          '<div class="cap"><b>Dialogue</b>' + (e.dialogue || '').replace(/</g, '&lt;') + '</div>' +
          '<div class="cap"><b>Notes</b>' + (e.notes || '').replace(/</g, '&lt;') + '</div></div>';
      });
    } else {
      // SBP "3 Panels": grid of thumbnails with compact captions.
      var thumbH2 = Math.round(280 * ch / cw);
      html = '<html><head><title>Storyboard</title><style>' +
        'body{font-family:system-ui;margin:24px;} .grid{display:grid;grid-template-columns:repeat(3,1fr);gap:20px;}' +
        '.card{break-inside:avoid;border:1px solid #ccc;border-radius:6px;overflow:hidden;}' +
        'img{width:100%;height:' + thumbH2 + 'px;object-fit:contain;background:#fff;display:block;}' +
        '.foot{padding:6px 8px;font-size:10.5px;} .foot b{color:#4E6FF2;}' +
        '@media print{body{margin:0;}}</style></head><body><div class="grid">';
      keys.forEach(function (f, idx) {
        var e2 = entryFor(captions, f);
        var lines = [];
        if (e2.action) lines.push('<b>Action</b> ' + e2.action.replace(/</g, '&lt;'));
        if (e2.dialogue) lines.push('<b>Dial.</b> ' + e2.dialogue.replace(/</g, '&lt;'));
        if (e2.notes) lines.push('<b>Note</b> ' + e2.notes.replace(/</g, '&lt;'));
        html += '<div class="card"><img src="' + thumbFor(f, 400, thumbH2 * 2) + '"><div class="foot"><b>' + (idx + 1) + '</b> — frame ' + (f + 1) + ' — ' + durs[idx].seconds.toFixed(2) + 's<br>' + lines.join('<br>') + '</div></div>';
      });
      html += '</div>';
    }
    html += '</body></html>';
    win.document.write(html); win.document.close();
    setTimeout(function () { win.print(); }, 300);
  };

  window.SMLabs.register('storyboard-mode', {
    flag: 'nemo-labs-storyboard',
    describe: 'Storyboard scope Toon Boom Storyboard Pro (chaque keyframe du calque actif = un panneau) — colonnes Action/Dialogue/Notes, durée calculée, 2 mises en page PDF (grille 3-panneaux / page complète)',
    onDisable: close,
  });
})();
