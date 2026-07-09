// ---- CAMERA LAYER (v18) ----
// Classic 2D-animation camera (TVPaint/Callipeg-style, per the user's
// reference mockup): ONE camera per project, keyframed on its own timeline
// row. Each key stores the camera rect (center x,y + width; height is
// derived from the project aspect so resizing IS zooming — the camera
// always frames exactly what the output will show). Between keys, center
// and width interpolate along a per-segment cubic-bezier easing curve
// (editable in the right panel, same mental model as CSS cubic-bezier /
// Animate's custom ease). The canvas shows every key's rect dashed with a
// center cross, corner-to-corner dashed links between consecutive keys
// (matching the reference image), and the interpolated current-frame rect
// solid. "Vue caméra" locks the viewport to the interpolated rect while
// scrubbing/playing; export (export.js) bakes the same transform into
// every rendered frame, so what the camera frames is what the MP4 shows.
//
// Deliberately NOT part of state.layers: a camera key is not stroke data,
// none of the layer.children consumers (CLAUDE.md's "family of bug #1"
// list) should ever see it. It lives as its own top-of-timeline row,
// rendered by hooks called from renderLayerList()/renderTimeline().
(function () {
  var DEFAULT_EASE = [0.42, 0, 0.58, 1]; // easeInOut — the least surprising default for camera moves

  function ensureState() {
    if (!state.cameraKeys) state.cameraKeys = [];       // persisted: [{frame,x,y,w,ease:[x1,y1,x2,y2]}] sorted by frame
    if (state.cameraLayerOn === undefined) state.cameraLayerOn = false; // persisted: the camera row exists
    if (state.cameraView === undefined) state.cameraView = false;       // session-only: viewport locked to camera
  }
  ensureState();

  function aspect() { return state.canvasH / state.canvasW; }
  function keyHeight(k) { return k.w * aspect(); }

  // Solve a CSS-style cubic-bezier((x1,y1),(x2,y2)) for y at x=t.
  // Newton first (fast for well-behaved curves), bisection fallback so a
  // degenerate user-dragged curve can never hang or return NaN.
  function bezierEase(t, x1, y1, x2, y2) {
    if (t <= 0) return 0;
    if (t >= 1) return 1;
    function bx(u) { var v = 1 - u; return 3 * v * v * u * x1 + 3 * v * u * u * x2 + u * u * u; }
    function by(u) { var v = 1 - u; return 3 * v * v * u * y1 + 3 * v * u * u * y2 + u * u * u; }
    function dbx(u) { var v = 1 - u; return 3 * v * v * x1 + 6 * v * u * (x2 - x1) + 3 * u * u * (1 - x2); }
    var u = t;
    for (var i = 0; i < 8; i++) {
      var d = dbx(u);
      if (Math.abs(d) < 1e-6) break;
      var err = bx(u) - t;
      if (Math.abs(err) < 1e-5) return by(u);
      u = Math.max(0, Math.min(1, u - err / d));
    }
    var lo = 0, hi = 1;
    for (var j = 0; j < 24; j++) {
      u = (lo + hi) / 2;
      if (bx(u) < t) lo = u; else hi = u;
    }
    return by(u);
  }

  function sortKeys() { state.cameraKeys.sort(function (a, b) { return a.frame - b.frame; }); }
  function keyAt(frame) {
    return state.cameraKeys.find(function (k) { return k.frame === frame; }) || null;
  }
  function defaultRect() {
    return { x: state.canvasW / 2, y: state.canvasH / 2, w: state.canvasW };
  }
  // The camera rect at ANY frame: exact key, interpolated between keys, or
  // clamped to the first/last key outside the keyed range. Null when the
  // camera layer has no keys at all.
  function cameraAtFrame(frame) {
    ensureState();
    var ks = state.cameraKeys;
    if (!ks.length) return null;
    if (frame <= ks[0].frame) return { x: ks[0].x, y: ks[0].y, w: ks[0].w };
    var last = ks[ks.length - 1];
    if (frame >= last.frame) return { x: last.x, y: last.y, w: last.w };
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1];
      if (frame >= a.frame && frame < b.frame) {
        var t = (frame - a.frame) / (b.frame - a.frame);
        var e = a.ease || DEFAULT_EASE;
        var y = bezierEase(t, e[0], e[1], e[2], e[3]);
        return { x: a.x + (b.x - a.x) * y, y: a.y + (b.y - a.y) * y, w: a.w + (b.w - a.w) * y };
      }
    }
    return { x: last.x, y: last.y, w: last.w };
  }
  function setKey(frame, rect) {
    ensureState();
    var k = keyAt(frame);
    if (k) { k.x = rect.x; k.y = rect.y; k.w = rect.w; }
    else {
      state.cameraKeys.push({ frame: frame, x: rect.x, y: rect.y, w: rect.w, ease: DEFAULT_EASE.slice() });
      sortKeys();
    }
    return keyAt(frame);
  }
  function removeKey(frame) {
    ensureState();
    var i = state.cameraKeys.findIndex(function (k) { return k.frame === frame; });
    if (i >= 0) state.cameraKeys.splice(i, 1);
  }
  // The segment whose ease governs the CURRENT frame (its LEFT key). On a
  // key exactly: the segment leaving that key. Null when fewer than 2 keys.
  function segmentLeftKey(frame) {
    var ks = state.cameraKeys;
    if (ks.length < 2) return null;
    if (frame >= ks[ks.length - 1].frame) return ks[ks.length - 2];
    for (var i = ks.length - 2; i >= 0; i--) {
      if (frame >= ks[i].frame) return ks[i];
    }
    return ks[0];
  }

  // ---- canvas overlay (engine scene items, non-destructive — same
  // pattern as the revision outlines / comment pins) ----
  function dashedRectItems(cx, cy, w, h, col, sw, dash, items) {
    var l = cx - w / 2, t = cy - h / 2, r = cx + w / 2, b = cy + h / 2;
    items.push({
      segments: [{ point: [l, t] }, { point: [r, t] }, { point: [r, b] }, { point: [l, b] }],
      closed: true, fillColor: null, strokeColor: col, strokeWidth: sw,
      dashPattern: dash || undefined
    });
  }
  function crossItems(cx, cy, s, col, sw, items) {
    items.push({ segments: [{ point: [cx - s, cy] }, { point: [cx + s, cy] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: sw });
    items.push({ segments: [{ point: [cx, cy - s] }, { point: [cx, cy + s] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: sw });
  }
  function corners(k) {
    var h = keyHeight(k);
    return [
      [k.x - k.w / 2, k.y - h / 2], [k.x + k.w / 2, k.y - h / 2],
      [k.x + k.w / 2, k.y + h / 2], [k.x - k.w / 2, k.y + h / 2],
    ];
  }
  function buildOverlayItems() {
    ensureState();
    if (!state.cameraLayerOn) return [];
    var items = [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    var keyCol = [160, 160, 170, 220];
    var curCol = [255, 170, 40, 255]; // accent : rect interpolé de la frame courante
    var dash = [6 * zs, 5 * zs];
    state.cameraKeys.forEach(function (k) {
      dashedRectItems(k.x, k.y, k.w, keyHeight(k), keyCol, 1.5 * zs, dash, items);
      crossItems(k.x, k.y, 7 * zs, keyCol, 1.5 * zs, items);
    });
    // liens pointillés coin-à-coin entre clés consécutives (cf. mockup de réf)
    for (var i = 0; i < state.cameraKeys.length - 1; i++) {
      var ca = corners(state.cameraKeys[i]), cb = corners(state.cameraKeys[i + 1]);
      for (var c = 0; c < 4; c++) {
        items.push({ segments: [{ point: ca[c] }, { point: cb[c] }], closed: false, fillColor: null, strokeColor: [130, 130, 140, 130], strokeWidth: 1 * zs, dashPattern: [3 * zs, 4 * zs] });
      }
    }
    var cam = cameraAtFrame(state.currentFrame);
    if (cam) {
      var isKey = !!keyAt(state.currentFrame);
      dashedRectItems(cam.x, cam.y, cam.w, cam.w * aspect(), curCol, 2 * zs, isKey ? undefined : dash, items);
      crossItems(cam.x, cam.y, 9 * zs, curCol, 2 * zs, items);
      if (state.tool === 'camera') {
        // poignées de resize aux 4 coins (visibles seulement avec l'outil actif)
        corners({ x: cam.x, y: cam.y, w: cam.w }).forEach(function (pt) {
          var s = 5 * zs;
          items.push({ segments: [{ point: [pt[0] - s, pt[1] - s] }, { point: [pt[0] + s, pt[1] - s] }, { point: [pt[0] + s, pt[1] + s] }, { point: [pt[0] - s, pt[1] + s] }], closed: true, fillColor: [255, 170, 40, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
        });
      }
    }
    return items;
  }

  // ---- gizmo interactions (Paper-native tool events, delegated from
  // tools.js when state.tool === 'camera') ----
  var _drag = null; // {mode:'move'|'resize', corner, startPt, startRect, key}
  function hitCorner(pt, cam) {
    var tol = 12 / view.zoom;
    var cs = corners({ x: cam.x, y: cam.y, w: cam.w });
    for (var i = 0; i < 4; i++) {
      if (Math.abs(pt.x - cs[i][0]) < tol && Math.abs(pt.y - cs[i][1]) < tol) return i;
    }
    return -1;
  }
  function onDown(event) {
    ensureState();
    if (!state.cameraLayerOn) return;
    var cam = cameraAtFrame(state.currentFrame) || defaultRect();
    var corner = hitCorner(event.point, cam);
    var h = cam.w * aspect();
    var inside = Math.abs(event.point.x - cam.x) < cam.w / 2 && Math.abs(event.point.y - cam.y) < h / 2;
    if (corner < 0 && !inside) return;
    // Éditer une frame sans clé crée la clé (à partir du rect interpolé) —
    // le geste naturel des apps d'anim : on se place, on cadre, c'est clé.
    var key = keyAt(state.currentFrame) || setKey(state.currentFrame, cam);
    _drag = { mode: corner >= 0 ? 'resize' : 'move', corner: corner, startPt: event.point.clone(), startRect: { x: key.x, y: key.y, w: key.w }, key: key };
  }
  function onDrag(event) {
    if (!_drag) return;
    var k = _drag.key, s = _drag.startRect;
    if (_drag.mode === 'move') {
      k.x = s.x + (event.point.x - _drag.startPt.x);
      k.y = s.y + (event.point.y - _drag.startPt.y);
    } else {
      // resize aspect-lock : la distance au centre pilote la largeur (zoom)
      var d0 = Math.max(1, Math.abs(_drag.startPt.x - s.x) + Math.abs(_drag.startPt.y - s.y));
      var d1 = Math.abs(event.point.x - s.x) + Math.abs(event.point.y - s.y);
      k.w = Math.max(20, s.w * (d1 / d0));
    }
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    renderCameraRow();
  }
  function onUp() {
    if (!_drag) return;
    _drag = null;
    if (window.updateCameraPanel) updateCameraPanel();
    renderCameraRow();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }

  // ---- timeline row (layer panel + frame grid, prepended by hooks in
  // renderLayerList()/renderTimeline() so both stay aligned) ----
  function renderPanelRow(list) {
    ensureState();
    if (!state.cameraLayerOn) return;
    var row = document.createElement('div');
    row.className = 'lrow camrow' + (state.tool === 'camera' ? ' act' : '');
    var ico = document.createElement('div'); ico.className = 'lico'; ico.textContent = '🎥'; ico.style.fontSize = '11px';
    var eye = document.createElement('div'); eye.className = 'lico'; eye.title = 'Vue caméra — verrouille le viewport sur la caméra';
    eye.textContent = state.cameraView ? '⊙' : '○';
    eye.style.color = state.cameraView ? 'var(--accent, #ffaa28)' : '';
    eye.addEventListener('click', function (e) {
      e.stopPropagation();
      state.cameraView = !state.cameraView;
      applyCameraView();
      renderLayerList();
    });
    var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = 'Caméra'; nm.style.fontWeight = '600';
    row.appendChild(ico); row.appendChild(eye); row.appendChild(nm);
    row.addEventListener('click', function () { window.SM.setTool('camera'); renderLayerList(); });
    row.addEventListener('contextmenu', function (e) {
      e.preventDefault();
      window.showContextMenu(e.clientX, e.clientY, [
        { label: 'Supprimer le calque caméra', action: function () { state.cameraLayerOn = false; state.cameraKeys = []; state.cameraView = false; if (state.tool === 'camera') window.SM.setTool('select'); renderLayerList(); renderTimeline(); updateCameraPanel(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } },
      ]);
    });
    list.appendChild(row);
  }
  var _gridRow = null;
  function renderGridRow(grid) {
    ensureState();
    _gridRow = null;
    if (!state.cameraLayerOn) return;
    var row = document.createElement('div'); row.className = 'frow camrow';
    for (var i = 0; i < state.totalFrames; i++) {
      var c = document.createElement('div');
      c.className = 'fc' + (i === state.currentFrame ? ' cur' : '');
      c.dataset.frame = i;
      if (keyAt(i)) c.classList.add('camkey');
      (function (fi) {
        c.addEventListener('mousedown', function (e) { e.stopPropagation(); goToFrame(fi); window.SM.setTool('camera'); });
      })(i);
      row.appendChild(c);
    }
    grid.appendChild(row);
    _gridRow = row;
  }
  // Refresh only the key markers on the existing row (drag feedback) —
  // renderTimeline() is a full rebuild, too heavy per mousemove.
  function renderCameraRow() {
    if (!_gridRow) return;
    for (var i = 0; i < _gridRow.children.length; i++) {
      _gridRow.children[i].classList.toggle('camkey', !!keyAt(i));
    }
  }

  // ---- viewport lock ("Vue caméra") + loadFrame hook ----
  function applyCameraView() {
    if (!state.cameraView) return;
    var cam = cameraAtFrame(state.currentFrame);
    if (!cam) return;
    view.center = new Point(cam.x, cam.y);
    view.zoom = view.viewSize.width / cam.w;
    if (window.updZoom) updZoom();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow(true);
  }

  // ---- export : bake le zoom/pan caméra dans chaque frame rendue ----
  // Appelé par exportBuildFrame (export.js) APRÈS construction du contenu.
  // Transforme tout le layer pour que le rect caméra remplisse le canvas de
  // sortie, puis clippe au rect canvas (un zoom-out au-delà du canvas
  // montre du vide — même comportement que les apps de réf).
  function applyToExportLayer(L, frameIdx) {
    ensureState();
    if (!state.cameraLayerOn || !state.cameraKeys.length) return;
    var cam = cameraAtFrame(frameIdx);
    if (!cam) return;
    var s = state.canvasW / cam.w;
    L.scale(s, new Point(cam.x, cam.y));
    L.translate(new Point(state.canvasW / 2 - cam.x, state.canvasH / 2 - cam.y));
    var clip = new Path.Rectangle({ point: [0, 0], size: [state.canvasW, state.canvasH], insert: false });
    L.insertChild(0, clip);
    L.clipped = true;
  }

  // ---- right panel : clés + éditeur de courbe de Bézier du segment ----
  function updateCameraPanel() {
    ensureState();
    var sec = document.getElementById('camera-sec');
    if (!sec) return;
    var show = state.cameraLayerOn && state.tool === 'camera';
    sec.style.display = show ? '' : 'none';
    if (!show) return;
    var isKey = !!keyAt(state.currentFrame);
    var addBtn = document.getElementById('btn-cam-addkey');
    if (addBtn) addBtn.textContent = isKey ? 'Supprimer la clé (frame ' + (state.currentFrame + 1) + ')' : 'Ajouter une clé (frame ' + (state.currentFrame + 1) + ')';
    var info = document.getElementById('cam-key-info');
    if (info) {
      var cam = cameraAtFrame(state.currentFrame);
      info.textContent = state.cameraKeys.length + ' clé(s)' + (cam ? ' — ' + Math.round(cam.w) + '×' + Math.round(cam.w * aspect()) + ' @ ' + Math.round(cam.x) + ',' + Math.round(cam.y) : '');
    }
    drawEaseEditor();
  }
  // Mini éditeur cubic-bezier : 2 points de contrôle draggables, applique
  // à l'ease du segment couvrant la frame courante.
  var _easeDrag = null;
  function easeCanvas() { return document.getElementById('cam-ease-canvas'); }
  function easeRect() { var c = easeCanvas(); return { w: c.width, h: c.height, pad: 12 }; }
  function easeToPx(x, y) { var r = easeRect(); return [r.pad + x * (r.w - 2 * r.pad), r.h - r.pad - y * (r.h - 2 * r.pad)]; }
  function pxToEase(px, py) {
    var r = easeRect();
    return [Math.max(0, Math.min(1, (px - r.pad) / (r.w - 2 * r.pad))), (r.h - r.pad - py) / (r.h - 2 * r.pad)];
  }
  function drawEaseEditor() {
    var c = easeCanvas();
    if (!c) return;
    var ctx = c.getContext('2d');
    ctx.clearRect(0, 0, c.width, c.height);
    var seg = segmentLeftKey(state.currentFrame);
    var lbl = document.getElementById('cam-ease-label');
    if (!seg) {
      if (lbl) lbl.textContent = 'Courbe : ajoute au moins 2 clés';
      return;
    }
    var ks = state.cameraKeys;
    var next = ks[ks.indexOf(seg) + 1];
    if (lbl) lbl.textContent = 'Courbe : clé ' + (seg.frame + 1) + ' → ' + (next.frame + 1);
    var e = seg.ease || DEFAULT_EASE;
    var dim = getComputedStyle(document.documentElement).getPropertyValue('--text-dim').trim() || '#888';
    var txt = getComputedStyle(document.documentElement).getPropertyValue('--text').trim() || '#eee';
    // cadre + diagonale de référence
    ctx.strokeStyle = dim; ctx.lineWidth = 1; ctx.setLineDash([3, 3]);
    var p0 = easeToPx(0, 0), p1 = easeToPx(1, 1);
    ctx.strokeRect(p0[0], p1[1], p1[0] - p0[0], p0[1] - p1[1]);
    ctx.beginPath(); ctx.moveTo(p0[0], p0[1]); ctx.lineTo(p1[0], p1[1]); ctx.stroke();
    ctx.setLineDash([]);
    // courbe
    ctx.strokeStyle = '#ffaa28'; ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(p0[0], p0[1]);
    var c1 = easeToPx(e[0], e[1]), c2 = easeToPx(e[2], e[3]);
    ctx.bezierCurveTo(c1[0], c1[1], c2[0], c2[1], p1[0], p1[1]);
    ctx.stroke();
    // bras + poignées
    ctx.strokeStyle = dim; ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(p0[0], p0[1]); ctx.lineTo(c1[0], c1[1]); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(p1[0], p1[1]); ctx.lineTo(c2[0], c2[1]); ctx.stroke();
    [c1, c2].forEach(function (p) {
      ctx.fillStyle = '#ffaa28'; ctx.strokeStyle = txt;
      ctx.beginPath(); ctx.arc(p[0], p[1], 5, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    });
  }
  function initEaseEditor() {
    var c = easeCanvas();
    if (!c) return;
    c.addEventListener('mousedown', function (ev) {
      var seg = segmentLeftKey(state.currentFrame);
      if (!seg) return;
      var r = c.getBoundingClientRect();
      var px = (ev.clientX - r.left) * (c.width / r.width), py = (ev.clientY - r.top) * (c.height / r.height);
      var e = seg.ease || (seg.ease = DEFAULT_EASE.slice());
      var c1 = easeToPx(e[0], e[1]), c2 = easeToPx(e[2], e[3]);
      var d1 = Math.hypot(px - c1[0], py - c1[1]), d2 = Math.hypot(px - c2[0], py - c2[1]);
      _easeDrag = { seg: seg, which: d1 <= d2 ? 0 : 1 };
      ev.preventDefault();
    });
    window.addEventListener('mousemove', function (ev) {
      if (!_easeDrag) return;
      var r = c.getBoundingClientRect();
      var px = (ev.clientX - r.left) * (c.width / r.width), py = (ev.clientY - r.top) * (c.height / r.height);
      var xy = pxToEase(px, py);
      var e = _easeDrag.seg.ease;
      if (_easeDrag.which === 0) { e[0] = xy[0]; e[1] = xy[1]; }
      else { e[2] = xy[0]; e[3] = xy[1]; }
      drawEaseEditor();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    window.addEventListener('mouseup', function () { _easeDrag = null; });
  }

  function initUI() {
    var btn = document.getElementById('btn-camera');
    if (btn) btn.addEventListener('click', function () {
      ensureState();
      if (!state.cameraLayerOn) {
        state.cameraLayerOn = true;
        if (!state.cameraKeys.length) setKey(0, defaultRect());
        renderLayerList(); renderTimeline();
      }
      window.SM.setTool(state.tool === 'camera' ? 'select' : 'camera');
      renderLayerList();
      updateCameraPanel();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    var addKey = document.getElementById('btn-cam-addkey');
    if (addKey) addKey.addEventListener('click', function () {
      var f = state.currentFrame;
      if (keyAt(f)) removeKey(f);
      else setKey(f, cameraAtFrame(f) || defaultRect());
      renderCameraRow(); updateCameraPanel();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    initEaseEditor();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initUI); else initUI();

  window.SMCamera = {
    cameraAtFrame: cameraAtFrame,
    keyAt: keyAt,
    setKey: setKey,
    removeKey: removeKey,
    buildOverlayItems: buildOverlayItems,
    onDown: onDown, onDrag: onDrag, onUp: onUp,
    renderPanelRow: renderPanelRow,
    renderGridRow: renderGridRow,
    applyCameraView: applyCameraView,
    applyToExportLayer: applyToExportLayer,
    updatePanel: updateCameraPanel,
  };
  window.updateCameraPanel = updateCameraPanel;
})();
